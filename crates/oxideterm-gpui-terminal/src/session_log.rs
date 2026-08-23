use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Local};
use oxideterm_settings::{
    ParsedTerminalSessionLogTemplate, TerminalSessionLogFileMode, TerminalSessionLogTemplatePart,
    TerminalSessionLogTemplateVariable, parse_terminal_session_log_content_template,
    parse_terminal_session_log_file_name_template,
};

const SESSION_LOG_QUEUE_CAPACITY: usize = 256;
const SESSION_LOG_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSessionLogState {
    Idle,
    Logging,
    Paused,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalSessionLogStatus {
    pub state: TerminalSessionLogState,
    pub path: Option<PathBuf>,
    pub bytes_written: u64,
    pub failed: bool,
}

impl Default for TerminalSessionLogStatus {
    fn default() -> Self {
        Self {
            state: TerminalSessionLogState::Idle,
            path: None,
            bytes_written: 0,
            failed: false,
        }
    }
}

#[derive(Clone)]
pub struct TerminalSessionLogOptions {
    pub directory: PathBuf,
    pub include_control_sequences: bool,
    pub retention_days: u64,
    pub max_file_bytes: u64,
    pub file_name_template: String,
    pub content_template: String,
    pub file_mode: TerminalSessionLogFileMode,
    pub context: TerminalSessionLogContext,
}

#[derive(Clone, Default)]
pub struct TerminalSessionLogContext {
    pub session: String,
    pub host: String,
    pub username: String,
    pub protocol: String,
}

enum SessionLogCommand {
    Output(Vec<u8>),
    Flush(SyncSender<bool>),
    Finish,
}

pub struct TerminalSessionLog {
    state: TerminalSessionLogState,
    path: PathBuf,
    sender: Option<SyncSender<SessionLogCommand>>,
    worker: Option<JoinHandle<io::Result<()>>>,
    cancelled: Arc<AtomicBool>,
    bytes_written: Arc<AtomicU64>,
    failure: Arc<Mutex<Option<String>>>,
}

pub fn prune_terminal_session_logs(directory: &Path, retention_days: u64) -> io::Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    remove_expired_logs(directory, retention_days)
}

impl TerminalSessionLog {
    pub fn start(options: TerminalSessionLogOptions) -> io::Result<Self> {
        fs::create_dir_all(&options.directory)?;
        remove_expired_logs(&options.directory, options.retention_days)?;
        let file_name_template =
            parse_terminal_session_log_file_name_template(&options.file_name_template)
                .map_err(|_| io::Error::other("invalid terminal session log file name template"))?;
        let content_template =
            parse_terminal_session_log_content_template(&options.content_template)
                .map_err(|_| io::Error::other("invalid terminal session log content template"))?;
        let (path, file, initial_bytes) = create_log_file(
            &options.directory,
            &file_name_template,
            &options.context,
            options.file_mode,
        )?;
        if initial_bytes >= options.max_file_bytes.max(1) {
            return Err(io::Error::other(
                "terminal session log already reached its size limit",
            ));
        }
        let (sender, receiver) = mpsc::sync_channel(SESSION_LOG_QUEUE_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let bytes_written = Arc::new(AtomicU64::new(initial_bytes));
        let failure = Arc::new(Mutex::new(None));
        let worker_cancelled = cancelled.clone();
        let worker_bytes_written = bytes_written.clone();
        let worker_failure = failure.clone();
        let worker = thread::Builder::new()
            .name("terminal-session-log".to_string())
            .spawn(move || {
                let result = run_session_log_writer(
                    file,
                    receiver,
                    worker_cancelled,
                    worker_bytes_written,
                    options.include_control_sequences,
                    options.max_file_bytes,
                    content_template,
                    options.context,
                );
                if let Err(error) = &result
                    && let Ok(mut failure) = worker_failure.lock()
                {
                    // The pane reports only a generic failure; terminal contents never enter errors.
                    *failure = Some(error.to_string());
                }
                result
            })?;

        Ok(Self {
            state: TerminalSessionLogState::Logging,
            path,
            sender: Some(sender),
            worker: Some(worker),
            cancelled,
            bytes_written,
            failure,
        })
    }

    pub fn status(&self) -> TerminalSessionLogStatus {
        let failed = self.failure.lock().is_ok_and(|failure| failure.is_some());
        TerminalSessionLogStatus {
            state: if failed {
                TerminalSessionLogState::Idle
            } else {
                self.state
            },
            path: Some(self.path.clone()),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            failed,
        }
    }

    pub fn pause(&mut self) -> io::Result<()> {
        if self.state != TerminalSessionLogState::Logging {
            return Ok(());
        }
        self.flush()?;
        self.state = TerminalSessionLogState::Paused;
        Ok(())
    }

    pub fn resume(&mut self) {
        if self.state == TerminalSessionLogState::Paused {
            self.state = TerminalSessionLogState::Logging;
        }
    }

    pub fn flush(&self) -> io::Result<()> {
        let (acknowledge, acknowledgement) = mpsc::sync_channel(0);
        self.send(SessionLogCommand::Flush(acknowledge))?;
        match acknowledgement.recv() {
            Ok(true) => Ok(()),
            _ => Err(io::Error::other(
                "terminal session log could not be flushed",
            )),
        }
    }

    pub fn write_output(&mut self, bytes: Vec<u8>) -> io::Result<()> {
        if self.state != TerminalSessionLogState::Logging || bytes.is_empty() {
            return Ok(());
        }
        if self.failure.lock().is_ok_and(|failure| failure.is_some()) {
            return Err(io::Error::other("terminal session log writer failed"));
        }

        if bytes.len() <= SESSION_LOG_CHUNK_BYTES {
            return self.try_send(SessionLogCommand::Output(bytes));
        }
        for chunk in bytes.chunks(SESSION_LOG_CHUNK_BYTES) {
            self.try_send(SessionLogCommand::Output(chunk.to_vec()))?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<PathBuf> {
        let sender = self.sender.take();
        if let Some(sender) = sender {
            sender
                .send(SessionLogCommand::Finish)
                .map_err(|_| io::Error::other("terminal session log writer stopped"))?;
        }
        let result = self.join_worker();
        if result.is_ok() {
            Ok(self.path.clone())
        } else {
            result.map(|()| self.path.clone())
        }
    }

    fn send(&self, command: SessionLogCommand) -> io::Result<()> {
        self.sender
            .as_ref()
            .ok_or_else(|| io::Error::other("terminal session log writer stopped"))?
            .send(command)
            .map_err(|_| io::Error::other("terminal session log writer stopped"))
    }

    fn try_send(&self, command: SessionLogCommand) -> io::Result<()> {
        match self
            .sender
            .as_ref()
            .ok_or_else(|| io::Error::other("terminal session log writer stopped"))?
            .try_send(command)
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(io::Error::other(
                "terminal session log writer is overloaded",
            )),
            Err(TrySendError::Disconnected(_)) => {
                Err(io::Error::other("terminal session log writer stopped"))
            }
        }
    }

    fn join_worker(&mut self) -> io::Result<()> {
        self.sender.take();
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker
            .join()
            .map_err(|_| io::Error::other("terminal session log writer panicked"))?
    }

    fn cancel_worker(&mut self) {
        // The pane owns this writer task; teardown cancels only file output, never the terminal node.
        self.cancelled.store(true, Ordering::Release);
        self.sender.take();
        let _ = self.join_worker();
    }
}

impl Drop for TerminalSessionLog {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.cancel_worker();
        }
    }
}

fn run_session_log_writer(
    file: File,
    receiver: mpsc::Receiver<SessionLogCommand>,
    cancelled: Arc<AtomicBool>,
    bytes_written: Arc<AtomicU64>,
    include_control_sequences: bool,
    max_file_bytes: u64,
    content_template: ParsedTerminalSessionLogTemplate,
    context: TerminalSessionLogContext,
) -> io::Result<()> {
    let mut writer = BoundedLogWriter::new(file, max_file_bytes, bytes_written);
    let mut printable_filter = PrintableTextFilter::default();
    let mut line_formatter = SessionLogLineFormatter::new(content_template, context)?;

    while let Ok(command) = receiver.recv() {
        if cancelled.load(Ordering::Acquire) {
            line_formatter.finish(&mut writer)?;
            return writer.flush();
        }
        match command {
            SessionLogCommand::Output(bytes) => {
                if include_control_sequences {
                    line_formatter.write(&mut writer, &bytes)?;
                } else {
                    let printable = printable_filter.filter(&bytes);
                    line_formatter.write(&mut writer, printable.as_bytes())?;
                }
            }
            SessionLogCommand::Flush(acknowledge) => match writer.flush() {
                Ok(()) => {
                    let _ = acknowledge.send(true);
                }
                Err(error) => {
                    let _ = acknowledge.send(false);
                    return Err(error);
                }
            },
            SessionLogCommand::Finish => {
                line_formatter.finish(&mut writer)?;
                return writer.flush();
            }
        }
    }
    line_formatter.finish(&mut writer)?;
    writer.flush()
}

struct SessionLogLineFormatter {
    prefix: Vec<TerminalSessionLogTemplatePart>,
    suffix: Vec<TerminalSessionLogTemplatePart>,
    context: TerminalSessionLogContext,
    current_line_time: Option<DateTime<Local>>,
    pending_carriage_return: bool,
}

impl SessionLogLineFormatter {
    fn new(
        template: ParsedTerminalSessionLogTemplate,
        context: TerminalSessionLogContext,
    ) -> io::Result<Self> {
        let text_index = template
            .parts()
            .iter()
            .position(|part| {
                *part
                    == TerminalSessionLogTemplatePart::Variable(
                        TerminalSessionLogTemplateVariable::Text,
                    )
            })
            .ok_or_else(|| io::Error::other("terminal session log template has no text field"))?;
        Ok(Self {
            prefix: template.parts()[..text_index].to_vec(),
            suffix: template.parts()[text_index + 1..].to_vec(),
            context,
            current_line_time: None,
            pending_carriage_return: false,
        })
    }

    fn write(&mut self, writer: &mut BoundedLogWriter, content: &[u8]) -> io::Result<()> {
        let mut index = 0;
        if self.pending_carriage_return {
            if content.first() == Some(&b'\n') {
                self.finish_line(writer, b"\r\n")?;
                index = 1;
            } else {
                self.finish_line(writer, b"\r")?;
            }
            self.pending_carriage_return = false;
        }

        while index < content.len() {
            let next_break = content[index..]
                .iter()
                .position(|byte| matches!(*byte, b'\r' | b'\n'))
                .map(|offset| index + offset);
            let Some(line_break) = next_break else {
                self.write_text(writer, &content[index..])?;
                return Ok(());
            };
            self.write_text(writer, &content[index..line_break])?;
            if content[line_break] == b'\r' {
                if line_break + 1 >= content.len() {
                    self.pending_carriage_return = true;
                    return Ok(());
                }
                if content[line_break + 1] == b'\n' {
                    self.finish_line(writer, b"\r\n")?;
                    index = line_break + 2;
                } else {
                    self.finish_line(writer, b"\r")?;
                    index = line_break + 1;
                }
            } else {
                self.finish_line(writer, b"\n")?;
                index = line_break + 1;
            }
        }
        Ok(())
    }

    fn write_text(&mut self, writer: &mut BoundedLogWriter, text: &[u8]) -> io::Result<()> {
        self.start_line(writer)?;
        writer.write_all(text)
    }

    fn start_line(&mut self, writer: &mut BoundedLogWriter) -> io::Result<()> {
        if self.current_line_time.is_some() {
            return Ok(());
        }
        let now = Local::now();
        write_template_parts(writer, &self.prefix, &self.context, &now)?;
        self.current_line_time = Some(now);
        Ok(())
    }

    fn finish_line(&mut self, writer: &mut BoundedLogWriter, ending: &[u8]) -> io::Result<()> {
        self.start_line(writer)?;
        let line_time = self.current_line_time.take().unwrap_or_else(Local::now);
        write_template_parts(writer, &self.suffix, &self.context, &line_time)?;
        writer.write_all(ending)
    }

    fn finish(&mut self, writer: &mut BoundedLogWriter) -> io::Result<()> {
        if self.pending_carriage_return {
            self.pending_carriage_return = false;
            self.finish_line(writer, b"\r")?;
        } else if let Some(line_time) = self.current_line_time.take() {
            write_template_parts(writer, &self.suffix, &self.context, &line_time)?;
        }
        Ok(())
    }
}

fn write_template_parts(
    writer: &mut BoundedLogWriter,
    parts: &[TerminalSessionLogTemplatePart],
    context: &TerminalSessionLogContext,
    now: &DateTime<Local>,
) -> io::Result<()> {
    for part in parts {
        match part {
            TerminalSessionLogTemplatePart::Literal(literal) => {
                writer.write_all(literal.as_bytes())?
            }
            TerminalSessionLogTemplatePart::Variable(variable) => {
                let value = template_variable_value(*variable, context, now);
                writer.write_all(value.as_bytes())?;
            }
        }
    }
    Ok(())
}

struct BoundedLogWriter {
    writer: BufWriter<File>,
    max_bytes: u64,
    bytes_written: Arc<AtomicU64>,
}

impl BoundedLogWriter {
    fn new(file: File, max_bytes: u64, bytes_written: Arc<AtomicU64>) -> Self {
        Self {
            writer: BufWriter::new(file),
            max_bytes: max_bytes.max(1),
            bytes_written,
        }
    }
}

impl Write for BoundedLogWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.bytes_written.load(Ordering::Relaxed);
        if written >= self.max_bytes {
            return Err(io::Error::other(
                "terminal session log reached its size limit",
            ));
        }
        let remaining = (self.max_bytes - written).min(buffer.len() as u64) as usize;
        if remaining < buffer.len() {
            return Err(io::Error::other(
                "terminal session log reached its size limit",
            ));
        }
        self.writer.write_all(buffer)?;
        self.bytes_written
            .fetch_add(buffer.len() as u64, Ordering::Relaxed);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[derive(Default)]
struct PrintableTextFilter {
    parser: vte::Parser,
    output: String,
}

impl PrintableTextFilter {
    fn filter(&mut self, bytes: &[u8]) -> String {
        let mut performer = PrintableTextCollector {
            output: &mut self.output,
        };
        self.parser.advance(&mut performer, bytes);
        std::mem::take(&mut self.output)
    }
}

struct PrintableTextCollector<'a> {
    output: &'a mut String,
}

impl vte::Perform for PrintableTextCollector<'_> {
    fn print(&mut self, character: char) {
        self.output.push(character);
    }

    fn print_text(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn execute(&mut self, byte: u8) {
        if matches!(byte, b'\n' | b'\r' | b'\t') {
            self.output.push(byte as char);
        }
    }
}

fn create_log_file(
    directory: &Path,
    template: &ParsedTerminalSessionLogTemplate,
    context: &TerminalSessionLogContext,
    mode: TerminalSessionLogFileMode,
) -> io::Result<(PathBuf, File, u64)> {
    let file_name = render_log_file_name(template, context)?;
    match mode {
        TerminalSessionLogFileMode::Unique => {
            for suffix in 0..1000 {
                let candidate = unique_file_name(&file_name, suffix);
                let path = directory.join(candidate);
                match open_log_file(&path, TerminalSessionLogFileMode::Unique) {
                    Ok(file) => return Ok((path, file, 0)),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique terminal session log name",
            ))
        }
        TerminalSessionLogFileMode::Append | TerminalSessionLogFileMode::Overwrite => {
            let path = directory.join(file_name);
            let file = open_log_file(&path, mode)?;
            let initial_bytes = if mode == TerminalSessionLogFileMode::Append {
                file.metadata()?.len()
            } else {
                0
            };
            Ok((path, file, initial_bytes))
        }
    }
}

fn open_log_file(path: &Path, mode: TerminalSessionLogFileMode) -> io::Result<File> {
    if mode != TerminalSessionLogFileMode::Unique
        && let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        // Append and overwrite must never follow a pre-existing link outside the log folder.
        return Err(io::Error::other(
            "terminal session log target is not a regular file",
        ));
    }
    let mut options = OpenOptions::new();
    match mode {
        TerminalSessionLogFileMode::Unique => {
            options.write(true).create_new(true);
        }
        TerminalSessionLogFileMode::Append => {
            options.append(true).create(true);
        }
        TerminalSessionLogFileMode::Overwrite => {
            options.write(true).create(true).truncate(true);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Existing append targets are tightened to the same private boundary as new logs.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn render_log_file_name(
    template: &ParsedTerminalSessionLogTemplate,
    context: &TerminalSessionLogContext,
) -> io::Result<String> {
    const MAX_LOG_FILE_NAME_CHARS: usize = 240;

    let now = Local::now();
    let mut file_name = String::new();
    for part in template.parts() {
        match part {
            TerminalSessionLogTemplatePart::Literal(literal) => file_name.push_str(literal),
            TerminalSessionLogTemplatePart::Variable(variable) => file_name.push_str(
                &sanitize_file_name_component(&template_variable_value(*variable, context, &now)),
            ),
        }
    }
    if file_name.is_empty()
        || matches!(file_name.as_str(), "." | "..")
        || file_name.ends_with(['.', ' '])
    {
        return Err(io::Error::other(
            "terminal session log template produced an invalid file name",
        ));
    }
    if !file_name.to_ascii_lowercase().ends_with(".log") {
        file_name.push_str(".log");
    }
    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if file_name.chars().count() > MAX_LOG_FILE_NAME_CHARS || is_windows_reserved_file_stem(stem) {
        return Err(io::Error::other(
            "terminal session log template produced an invalid file name",
        ));
    }
    Ok(file_name)
}

fn unique_file_name(file_name: &str, suffix: usize) -> String {
    if suffix == 0 {
        return file_name.to_string();
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("log");
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => format!("{stem}-{suffix}.{extension}"),
        None => format!("{stem}-{suffix}"),
    }
}

fn sanitize_file_name_component(value: &str) -> String {
    const MAX_COMPONENT_CHARS: usize = 64;

    let mut sanitized = String::new();
    let mut previous_was_separator = false;
    for character in value.chars().take(MAX_COMPONENT_CHARS) {
        let replace = character.is_control()
            || character.is_whitespace()
            || matches!(
                character,
                '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
            );
        if replace {
            if !previous_was_separator {
                sanitized.push('_');
            }
            previous_was_separator = true;
        } else {
            sanitized.push(character);
            previous_was_separator = false;
        }
    }
    let sanitized = sanitized.trim_matches(['.', '_']);
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized.to_string()
    }
}

fn is_windows_reserved_file_stem(stem: &str) -> bool {
    let stem = stem
        .split('.')
        .next()
        .unwrap_or(stem)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

fn template_variable_value(
    variable: TerminalSessionLogTemplateVariable,
    context: &TerminalSessionLogContext,
    now: &DateTime<Local>,
) -> String {
    match variable {
        TerminalSessionLogTemplateVariable::Date => now.format("%Y-%m-%d").to_string(),
        TerminalSessionLogTemplateVariable::Time => now.format("%H-%M-%S").to_string(),
        TerminalSessionLogTemplateVariable::DateTime => {
            now.format("%Y-%m-%dT%H-%M-%S%:z").to_string()
        }
        TerminalSessionLogTemplateVariable::Timestamp => {
            now.format("%Y-%m-%d %H:%M:%S%.3f%:z").to_string()
        }
        TerminalSessionLogTemplateVariable::Session => context.session.clone(),
        TerminalSessionLogTemplateVariable::Host => context.host.clone(),
        TerminalSessionLogTemplateVariable::Username => context.username.clone(),
        TerminalSessionLogTemplateVariable::Protocol => context.protocol.clone(),
        TerminalSessionLogTemplateVariable::Text => String::new(),
    }
}

fn remove_expired_logs(directory: &Path, retention_days: u64) -> io::Result<()> {
    if retention_days == 0 {
        return Ok(());
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(
            retention_days.saturating_mul(24 * 60 * 60),
        ))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("log") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            continue;
        }
        if metadata.modified().is_ok_and(|modified| modified < cutoff) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(directory: &Path) -> TerminalSessionLogOptions {
        TerminalSessionLogOptions {
            directory: directory.to_path_buf(),
            include_control_sequences: false,
            retention_days: 30,
            max_file_bytes: 1024,
            file_name_template: "{date}_{time}_{session}.log".to_string(),
            content_template: "{text}".to_string(),
            file_mode: TerminalSessionLogFileMode::Unique,
            context: TerminalSessionLogContext {
                session: "test".to_string(),
                host: "example.test".to_string(),
                username: "tester".to_string(),
                protocol: "ssh".to_string(),
            },
        }
    }

    #[test]
    fn printable_log_strips_split_ansi_sequences_without_losing_text() {
        let directory = tempfile::tempdir().unwrap();
        let mut log = TerminalSessionLog::start(options(directory.path())).unwrap();

        log.write_output(b"plain \x1b[3".to_vec()).unwrap();
        log.write_output("1m红色\x1b[0m\r\nnext".as_bytes().to_vec())
            .unwrap();
        let path = log.finish().unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "plain 红色\r\nnext");
    }

    #[test]
    fn paused_log_skips_output_and_resumes_in_order() {
        let directory = tempfile::tempdir().unwrap();
        let mut log = TerminalSessionLog::start(options(directory.path())).unwrap();

        log.write_output(b"before\n".to_vec()).unwrap();
        log.pause().unwrap();
        log.write_output(b"secret\n".to_vec()).unwrap();
        log.resume();
        log.write_output(b"after\n".to_vec()).unwrap();
        let path = log.finish().unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "before\nafter\n");
    }

    #[test]
    fn log_file_never_exceeds_configured_size_limit() {
        let directory = tempfile::tempdir().unwrap();
        let mut bounded = options(directory.path());
        bounded.include_control_sequences = true;
        bounded.max_file_bytes = 4;
        let mut log = TerminalSessionLog::start(bounded).unwrap();
        let path = log.status().path.unwrap();

        log.write_output(b"abcdef".to_vec()).unwrap();
        assert!(log.finish().is_err());

        assert!(fs::metadata(path).unwrap().len() <= 4);
    }

    #[test]
    fn starting_log_removes_only_expired_log_files() {
        let directory = tempfile::tempdir().unwrap();
        let expired = directory.path().join("expired.log");
        let unrelated = directory.path().join("notes.txt");
        fs::write(&expired, b"old").unwrap();
        fs::write(&unrelated, b"keep").unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
        File::options()
            .write(true)
            .open(&expired)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old_time))
            .unwrap();

        let mut retention_options = options(directory.path());
        retention_options.retention_days = 1;
        let log = TerminalSessionLog::start(retention_options).unwrap();
        log.finish().unwrap();

        assert!(!expired.exists());
        assert!(unrelated.exists());
    }

    #[cfg(unix)]
    #[test]
    fn session_log_file_is_private_to_the_current_user() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let log = TerminalSessionLog::start(options(directory.path())).unwrap();
        let path = log.status().path.unwrap();
        log.finish().unwrap();

        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn content_template_formats_complete_and_split_lines() {
        let directory = tempfile::tempdir().unwrap();
        let mut configured = options(directory.path());
        configured.content_template = "{protocol}:{text} [{session}]".to_string();
        let mut log = TerminalSessionLog::start(configured).unwrap();

        log.write_output(b"first\r".to_vec()).unwrap();
        log.write_output(b"\nsec".to_vec()).unwrap();
        log.write_output(b"ond".to_vec()).unwrap();
        let path = log.finish().unwrap();

        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "ssh:first [test]\r\nssh:second [test]"
        );
    }

    #[test]
    fn append_and_overwrite_modes_use_the_rendered_file_name() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ssh_test.log");
        fs::write(&path, b"existing\n").unwrap();
        let mut configured = options(directory.path());
        configured.file_name_template = "{protocol}_{session}.log".to_string();
        configured.file_mode = TerminalSessionLogFileMode::Append;
        let mut append = TerminalSessionLog::start(configured.clone()).unwrap();
        append.write_output(b"appended\n".to_vec()).unwrap();
        append.finish().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "existing\nappended\n");

        configured.file_mode = TerminalSessionLogFileMode::Overwrite;
        let mut overwrite = TerminalSessionLog::start(configured).unwrap();
        overwrite.write_output(b"replacement\n".to_vec()).unwrap();
        overwrite.finish().unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "replacement\n");
    }

    #[test]
    fn file_name_variables_cannot_escape_the_log_directory() {
        let directory = tempfile::tempdir().unwrap();
        let mut configured = options(directory.path());
        configured.file_name_template = "{session}.log".to_string();
        configured.context.session = "../../production host".to_string();

        let log = TerminalSessionLog::start(configured).unwrap();
        let path = log.status().path.unwrap();
        log.finish().unwrap();

        assert_eq!(path.parent(), Some(directory.path()));
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("production_host.log")
        );
    }

    #[test]
    fn file_name_rejects_windows_device_names_on_every_platform() {
        let directory = tempfile::tempdir().unwrap();
        let mut configured = options(directory.path());
        configured.file_name_template = "CON.extra".to_string();

        assert!(TerminalSessionLog::start(configured).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn overwrite_mode_rejects_symbolic_link_targets() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let protected = directory.path().join("protected.txt");
        let link = directory.path().join("ssh_test.log");
        fs::write(&protected, b"keep").unwrap();
        symlink(&protected, &link).unwrap();
        let mut configured = options(directory.path());
        configured.file_name_template = "{protocol}_{session}.log".to_string();
        configured.file_mode = TerminalSessionLogFileMode::Overwrite;

        assert!(TerminalSessionLog::start(configured).is_err());
        assert_eq!(fs::read(&protected).unwrap(), b"keep");
    }
}
