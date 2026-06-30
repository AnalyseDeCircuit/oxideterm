use std::{fmt::Write as _, io::ErrorKind};

const RAW_TCP_DEFAULT_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const RAW_TCP_READ_BUFFER_SIZE: usize = 16 * 1024;
const RAW_TCP_HEXDUMP_WIDTH: usize = 16;
const RAW_TCP_ESC_BYTE: u8 = 0x1b;
const RAW_TCP_STRING_CONTROL_MARKER: &[u8] = b"?";
const RAW_TCP_REMOTE_CLOSE_MESSAGE: &[u8] = b"\r\nRaw TCP remote closed the connection.\r\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawTcpTlsVerification {
    System,
    AllowInvalidCertificates,
}

impl Default for RawTcpTlsVerification {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawTcpTlsConfig {
    pub enabled: bool,
    pub verification: RawTcpTlsVerification,
    pub server_name: Option<String>,
}

impl RawTcpTlsConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            verification: RawTcpTlsVerification::System,
            server_name: None,
        }
    }

    pub fn server_name_for_host(&self, host: &str) -> String {
        self.server_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(host)
            .to_string()
    }
}

impl Default for RawTcpTlsConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawTcpSessionConfig {
    pub host: String,
    pub port: u16,
    pub line_ending: RawTcpLineEnding,
    pub display_mode: RawTcpDisplayMode,
    pub send_mode: RawTcpSendMode,
    pub tls: RawTcpTlsConfig,
}

impl RawTcpSessionConfig {
    pub fn endpoint_label(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn validate(&self) -> Result<()> {
        if self.host.trim().is_empty() {
            bail!("Raw TCP host is required");
        }
        if self.port == 0 {
            bail!("Raw TCP port must be greater than zero");
        }
        Ok(())
    }
}

pub struct RawTcpSession {
    config: RawTcpSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: Receiver<AlacEvent>,
    worker_rx: Receiver<RawTcpWorkerEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    command_tx: tokio::sync::mpsc::Sender<RawTcpCommand>,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    graphics_alt_screen_active: bool,
    output_queue: VecDeque<Vec<u8>>,
    output_queue_bytes: usize,
    magic_scan: MagicScanWindow,
    encoding: TerminalEncoding,
    output_decoder: TerminalOutputDecoder,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    input_encoder: TerminalInputEncoder,
    encoding_detector: EncodingMismatchDetector,
    terminal_ingress: RawTcpTerminalIngress,
    hexdump_offset: u64,
}

#[derive(Debug)]
enum RawTcpCommand {
    Data(Vec<u8>),
    Close,
}

#[derive(Debug)]
enum RawTcpWorkerEvent {
    Connected,
    Output(Vec<u8>),
    Failed(String),
    Closed,
}

#[derive(Debug)]
enum RawTcpStream {
    Plain(TcpStream),
    Tls(tokio_native_tls::TlsStream<TcpStream>),
}

impl RawTcpStream {
    async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer).await,
            Self::Tls(stream) => stream.read(buffer).await,
        }
    }

    async fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.write_all(bytes).await,
            Self::Tls(stream) => stream.write_all(bytes).await,
        }
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.shutdown().await,
            Self::Tls(stream) => stream.shutdown().await,
        }
    }
}

#[derive(Debug, Default)]
struct RawTcpTerminalIngress {
    pending_escape: bool,
}

impl RawTcpTerminalIngress {
    fn filter(&mut self, bytes: &[u8]) -> Vec<u8> {
        if bytes.is_empty() {
            return Vec::new();
        }

        let mut filtered = Vec::with_capacity(bytes.len());
        let mut index = 0;
        if self.pending_escape {
            self.pending_escape = false;
            append_raw_tcp_escape_pair(&mut filtered, bytes[0]);
            index = 1;
        }

        while index < bytes.len() {
            let byte = bytes[index];
            if is_c1_string_control(byte) {
                filtered.extend_from_slice(RAW_TCP_STRING_CONTROL_MARKER);
                index += 1;
                continue;
            }
            if byte != RAW_TCP_ESC_BYTE {
                filtered.push(byte);
                index += 1;
                continue;
            }

            let Some(next) = bytes.get(index + 1).copied() else {
                // Socket reads can split ESC and its command byte. Keep the
                // boundary so unsafe string controls are still neutralized.
                self.pending_escape = true;
                break;
            };
            append_raw_tcp_escape_pair(&mut filtered, next);
            index += 2;
        }

        filtered
    }
}

fn append_raw_tcp_escape_pair(output: &mut Vec<u8>, next: u8) {
    match next {
        // OSC, DCS, PM, APC, and SOS can carry arbitrary payloads. Raw TCP is
        // not a trusted shell transport, so turn the control introducer into
        // visible text instead of letting it steer the terminal emulator.
        b']' | b'P' | b'_' | b'^' | b'X' => {
            output.extend_from_slice(RAW_TCP_STRING_CONTROL_MARKER);
        }
        _ => {
            output.push(RAW_TCP_ESC_BYTE);
            output.push(next);
        }
    }
}

fn is_c1_string_control(byte: u8) -> bool {
    matches!(byte, 0x90 | 0x98 | 0x9d | 0x9e | 0x9f)
}

impl RawTcpSession {
    pub fn new(
        config: RawTcpSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Self {
        let resize = TerminalResize::new(cols, rows, 0, 0);
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        let (event_tx, event_rx) = unbounded();
        let (worker_tx, worker_rx) = unbounded();
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(256);
        let listener = LocalEventListener { tx: event_tx };

        let mut term_config = Config::default();
        term_config.scrolling_history = scrollback_lines;
        term_config.kitty_keyboard = true;
        let term = Arc::new(FairMutex::new(Term::new(term_config, &size, listener)));

        let runtime = Runtime::new().ok();
        if let Err(error) = config.validate() {
            let _ = worker_tx.send(RawTcpWorkerEvent::Failed(error.to_string()));
        } else if let Some(runtime) = runtime.as_ref() {
            let worker_config = config.clone();
            runtime.spawn(run_raw_tcp_worker(worker_config, command_rx, worker_tx));
        } else {
            let _ = worker_tx.send(RawTcpWorkerEvent::Failed(
                "failed to initialize Raw TCP runtime".to_string(),
            ));
        }

        Self {
            config,
            term,
            parser: Processor::new(),
            event_rx,
            worker_rx,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            runtime,
            command_tx,
            title: None,
            graphics_ingress: GraphicsIngress::new(graphics_options),
            graphics: TerminalGraphicsState::default(),
            graphics_alt_screen_active: false,
            output_queue: VecDeque::new(),
            output_queue_bytes: 0,
            magic_scan: MagicScanWindow::default(),
            encoding,
            output_decoder: TerminalOutputDecoder::new(encoding),
            output_processor: None,
            output_events_enabled: false,
            input_encoder: TerminalInputEncoder::new(encoding),
            encoding_detector: EncodingMismatchDetector::new(encoding),
            terminal_ingress: RawTcpTerminalIngress::default(),
            hexdump_offset: 0,
        }
    }

    fn title_text(&self) -> String {
        format!("TCP {}", self.config.endpoint_label())
    }

    fn drain_worker_events_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = TerminalDrainReport::default();
        loop {
            if report.drained_bytes >= budget.max_bytes
                || report.events_drained >= budget.max_events
            {
                report.budget_exhausted = !self.output_queue.is_empty();
                break;
            }

            if let Some(bytes) = self.output_queue.pop_front() {
                self.output_queue_bytes = self.output_queue_bytes.saturating_sub(bytes.len());
                report.drained_bytes = report.drained_bytes.saturating_add(bytes.len());
                report.events_drained += 1;
                self.feed_transport_output(&bytes);
                report.mark_changed();
                continue;
            }

            match self.worker_rx.try_recv() {
                Ok(RawTcpWorkerEvent::Connected) => {
                    self.title = Some(self.title_text());
                    self.pending_events
                        .push(TerminalEvent::TitleChanged(self.title_text()));
                    report.events_drained += 1;
                    report.mark_changed();
                }
                Ok(RawTcpWorkerEvent::Output(bytes)) => {
                    if report.drained_bytes > 0
                        && report.drained_bytes.saturating_add(bytes.len()) > budget.max_bytes
                    {
                        self.output_queue_bytes =
                            self.output_queue_bytes.saturating_add(bytes.len());
                        self.output_queue.push_back(bytes);
                        report.budget_exhausted = true;
                        break;
                    }
                    report.drained_bytes = report.drained_bytes.saturating_add(bytes.len());
                    report.events_drained += 1;
                    self.feed_transport_output(&bytes);
                    report.mark_changed();
                }
                Ok(RawTcpWorkerEvent::Failed(error)) => {
                    self.lifecycle = TerminalLifecycle::Exited(None);
                    self.feed_utf8_terminal_output(
                        format!("\r\nRaw TCP connection failed: {error}\r\n").as_bytes(),
                    );
                    self.pending_events.push(TerminalEvent::ChildExited(None));
                    report.events_drained += 1;
                    report.mark_changed();
                    break;
                }
                Ok(RawTcpWorkerEvent::Closed) => {
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.feed_utf8_terminal_output(RAW_TCP_REMOTE_CLOSE_MESSAGE);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    report.events_drained += 1;
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    break;
                }
            }
        }
        report.pending_bytes = self.output_queue_bytes;
        report.drain_duration = started.elapsed();
        report
    }

    fn feed_transport_output(&mut self, bytes: &[u8]) {
        for kind in self.magic_scan.scan(bytes) {
            self.pending_events.push(TerminalEvent::MagicDetected(kind));
        }
        let processed_output = self.process_terminal_output(bytes);
        let display_output = self.prepare_display_output(processed_output.as_ref());
        self.feed_plain_transport_output(display_output.as_ref());
    }

    fn prepare_display_output<'a>(&mut self, bytes: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        match self.config.display_mode {
            RawTcpDisplayMode::Text => std::borrow::Cow::Owned(self.terminal_ingress.filter(bytes)),
            RawTcpDisplayMode::Hex => {
                std::borrow::Cow::Owned(format_raw_tcp_hexdump(bytes, &mut self.hexdump_offset, false))
            }
            RawTcpDisplayMode::Mixed => {
                std::borrow::Cow::Owned(format_raw_tcp_hexdump(bytes, &mut self.hexdump_offset, true))
            }
        }
    }

    fn feed_plain_transport_output(&mut self, bytes: &[u8]) {
        let mut term = self.term.lock();
        let size = TerminalSize {
            cols: self.resize.cols,
            rows: self.resize.rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        };
        let cursor = Cell::new(graphics_cursor_from_term(&term, size));
        self.graphics_ingress.advance_ordered(
            bytes,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                    if let Some(hint) = self.encoding_detector.observe(&terminal_bytes) {
                        self.pending_events.push(TerminalEvent::EncodingHint(hint));
                    }
                    let decoded = self.output_decoder.decode_to_utf8_bytes(&terminal_bytes);
                    if self.output_events_enabled && !decoded.is_empty() {
                        // Recording observes decoded display bytes; raw socket
                        // payload is intentionally not duplicated by default.
                        self.pending_events
                            .push(TerminalEvent::Output(decoded.as_ref().to_vec()));
                    }
                    self.parser.advance(&mut *term, decoded.as_ref());
                    self.graphics
                        .clear_for_alt_screen_transition(&term, &mut self.graphics_alt_screen_active);
                    cursor.set(graphics_cursor_from_term(&term, size));
                }
                TerminalGraphicsSegment::Event(event) => {
                    let _ = self.graphics.handle_event(event);
                }
            },
            || cursor.get(),
        );
    }

    fn process_terminal_output<'a>(&self, bytes: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        apply_terminal_output_processor(&self.output_processor, bytes)
    }

    fn feed_utf8_terminal_output(&mut self, bytes: &[u8]) {
        self.push_output_event(bytes);
        let mut term = self.term.lock();
        self.parser.advance(&mut *term, bytes);
    }

    fn push_output_event(&mut self, bytes: &[u8]) {
        if self.output_events_enabled && !bytes.is_empty() {
            self.pending_events.push(TerminalEvent::Output(bytes.to_vec()));
        }
    }

    fn handle_alacritty_event(&mut self, event: AlacEvent) -> bool {
        match event {
            AlacEvent::Title(title) => {
                self.title = Some(title.clone());
                self.pending_events.push(TerminalEvent::TitleChanged(title));
                false
            }
            AlacEvent::ResetTitle => {
                self.title = Some(self.title_text());
                self.pending_events
                    .push(TerminalEvent::TitleChanged(self.title_text()));
                false
            }
            AlacEvent::Bell => {
                self.pending_events.push(TerminalEvent::Bell);
                false
            }
            AlacEvent::Wakeup | AlacEvent::MouseCursorDirty => {
                self.pending_events.push(TerminalEvent::Wakeup);
                true
            }
            AlacEvent::CursorBlinkingChange => {
                let blinking = self.term.lock().cursor_style().blinking;
                self.pending_events
                    .push(TerminalEvent::BlinkChanged(blinking));
                true
            }
            AlacEvent::PtyWrite(_) => {
                // Raw TCP output is untrusted protocol data, not a shell that
                // can safely ask this client to emit terminal replies.
                false
            }
            AlacEvent::ClipboardStore(_, text) => {
                self.pending_events.push(TerminalEvent::ClipboardStore(text));
                false
            }
            AlacEvent::ClipboardLoad(_, formatter) => {
                self.pending_events
                    .push(TerminalEvent::ClipboardLoad(formatter));
                false
            }
            AlacEvent::ColorRequest(_, _) | AlacEvent::TextAreaSizeRequest(_) => false,
            AlacEvent::ChildExit(_) | AlacEvent::Exit => false,
        }
    }

    fn send_command(&mut self, command: RawTcpCommand) -> Result<()> {
        self.command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    fn encode_user_input(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        match self.config.send_mode {
            RawTcpSendMode::Text => Ok(encode_raw_tcp_text_input(bytes, self.config.line_ending)),
            RawTcpSendMode::Hex => parse_raw_tcp_hex_input(bytes),
        }
    }
}

impl TerminalSessionBackend for RawTcpSession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::RawTcp
    }

    fn title(&self) -> Option<String> {
        Some(self.title.clone().unwrap_or_else(|| self.title_text()))
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    fn process_info(&self) -> TerminalProcessInfo {
        TerminalProcessInfo::default()
    }

    fn refresh_process_info(&mut self) {}

    fn read_pending(&mut self) -> bool {
        self.read_pending_with_budget(TerminalDrainBudget::unlimited())
            .changed
    }

    fn read_pending_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = self.drain_worker_events_with_budget(budget);
        while report.events_drained < budget.max_events {
            let Ok(event) = self.event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if self.handle_alacritty_event(event) {
                report.mark_changed();
            }
        }
        if report.events_drained >= budget.max_events && !self.event_rx.is_empty() {
            report.budget_exhausted = true;
        }
        report.drain_duration = started.elapsed();
        report
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        let encoded = self.encode_user_input(bytes)?;
        self.write_protocol_bytes(&encoded)
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.send_command(RawTcpCommand::Data(bytes.to_vec()))?;
        }
        Ok(())
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        let encoded = self.input_encoder.encode_text(text);
        self.write_input(encoded.as_ref())
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        let bytes = self
            .input_encoder
            .encode_paste(text, self.mode().contains(TermMode::BRACKETED_PASTE));
        self.write_input(&bytes)
    }

    fn set_encoding(&mut self, encoding: TerminalEncoding) {
        if self.encoding == encoding {
            return;
        }
        self.encoding = encoding;
        self.output_decoder.set_encoding(encoding);
        self.output_decoder.reset();
        self.input_encoder.set_encoding(encoding);
        self.encoding_detector.set_encoding(encoding);
    }

    fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.output_processor = processor;
        self.output_decoder.reset();
        self.encoding_detector.set_encoding(self.encoding);
    }

    fn set_output_events_enabled(&mut self, enabled: bool) {
        self.output_events_enabled = enabled;
    }

    fn set_raw_tcp_runtime_options(&mut self, options: RawTcpRuntimeOptions) -> Result<()> {
        if self.config.display_mode != options.display_mode {
            // Display mode changes only affect future socket bytes; reset the
            // hexdump offset so the new view starts at a readable boundary.
            self.hexdump_offset = 0;
        }
        self.config.line_ending = options.line_ending;
        self.config.display_mode = options.display_mode;
        self.config.send_mode = options.send_mode;
        Ok(())
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
        self.term.lock().is_focused = focused;
        Ok(())
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        self.resize = resize;
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        self.term.lock().resize(size);
        Ok(())
    }

    fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.term.lock().scroll_display(Scroll::Delta(delta));
        }
    }

    fn page_up(&mut self) {
        self.term.lock().scroll_display(Scroll::PageUp);
    }

    fn page_down(&mut self) {
        self.term.lock().scroll_display(Scroll::PageDown);
    }

    fn scroll_to_top(&mut self) {
        self.term.lock().scroll_display(Scroll::Top);
    }

    fn scroll_to_bottom(&mut self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    fn scroll_to_display_offset(&mut self, offset: usize) {
        let mut term = self.term.lock();
        let max_offset = term.total_lines().saturating_sub(term.screen_lines());
        let target = offset.min(max_offset);
        let current = term.grid().display_offset();
        let delta = target as i32 - current as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let term = self.term.lock();
        let grid = term.grid();
        let top_line = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
        let bottom_line = term.screen_lines() as i32;
        let mut matches = Vec::new();
        let mut logical_text = String::new();
        let mut logical_map = Vec::new();

        for line in top_line..bottom_line {
            let row = &grid[Line(line)];
            append_grid_line_text(
                row[..].iter(),
                line,
                self.resize.cols,
                &mut logical_text,
                &mut logical_map,
            );

            let wrapped = row.last().is_some_and(|cell| {
                cell.flags
                    .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
            });
            if wrapped && line + 1 < bottom_line {
                continue;
            }

            matches.extend(search_logical_line_matches(
                &logical_text,
                &logical_map,
                query,
                self.resize.cols,
            ));
            logical_text.clear();
            logical_map.clear();
        }
        matches
    }

    fn clear_buffer(&mut self) {
        let mut term = self.term.lock();
        clear_terminal_buffer(&mut term);
        self.graphics.clear();
    }

    fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        let term = self.term.lock();
        command_output_text_from_term(&term, mark)
    }

    fn buffer_text(&self) -> String {
        let term = self.term.lock();
        terminal_buffer_text_from_term(&term, self.resize.cols)
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let term = self.term.lock();
        snapshot_from_term(
            &term,
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
        )
    }

    fn snapshot_with_display_offset(
        &self,
        display_offset: usize,
        rows: usize,
    ) -> TerminalSnapshot {
        let term = self.term.lock();
        snapshot_from_term_with_display_offset(
            &term,
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
            display_offset,
            rows,
        )
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        self.shutdown();
        Ok(())
    }

    fn kill_active_task(&mut self) -> Result<()> {
        self.shutdown();
        Ok(())
    }

    fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }
        let _ = self.send_command(RawTcpCommand::Close);
        self.runtime = None;
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

async fn run_raw_tcp_worker(
    config: RawTcpSessionConfig,
    mut command_rx: tokio::sync::mpsc::Receiver<RawTcpCommand>,
    worker_tx: crossbeam_channel::Sender<RawTcpWorkerEvent>,
) {
    let mut stream = match connect_raw_tcp_stream(&config).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = worker_tx.send(RawTcpWorkerEvent::Failed(error));
            return;
        }
    };

    let _ = worker_tx.send(RawTcpWorkerEvent::Connected);
    let mut buffer = vec![0_u8; RAW_TCP_READ_BUFFER_SIZE];
    loop {
        tokio::select! {
            read_result = stream.read(&mut buffer) => {
                match read_result {
                    Ok(0) => {
                        let _ = worker_tx.send(RawTcpWorkerEvent::Closed);
                        break;
                    }
                    Ok(read_count) => {
                        if worker_tx
                            .send(RawTcpWorkerEvent::Output(buffer[..read_count].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = worker_tx.send(RawTcpWorkerEvent::Failed(raw_tcp_io_error_message(
                            "read from",
                            &config.endpoint_label(),
                            &error,
                        )));
                        break;
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(RawTcpCommand::Data(bytes)) => {
                        if let Err(error) = stream.write_all(&bytes).await {
                            let _ = worker_tx.send(RawTcpWorkerEvent::Failed(raw_tcp_io_error_message(
                                "write to",
                                &config.endpoint_label(),
                                &error,
                            )));
                            break;
                        }
                    }
                    Some(RawTcpCommand::Close) | None => {
                        let _ = stream.shutdown().await;
                        let _ = worker_tx.send(RawTcpWorkerEvent::Closed);
                        break;
                    }
                }
            }
        }
    }
}

async fn connect_raw_tcp_stream(config: &RawTcpSessionConfig) -> std::result::Result<RawTcpStream, String> {
    let endpoint = (config.host.as_str(), config.port);
    let stream = match tokio::time::timeout(
        RAW_TCP_DEFAULT_CONNECT_TIMEOUT,
        TcpStream::connect(endpoint),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            return Err(raw_tcp_io_error_message(
                "connect to",
                &config.endpoint_label(),
                &error,
            ));
        }
        Err(_) => {
            return Err(format!(
                "timed out connecting to {}",
                config.endpoint_label()
            ));
        }
    };
    let _ = stream.set_nodelay(true);

    if !config.tls.enabled {
        return Ok(RawTcpStream::Plain(stream));
    }

    let server_name = config.tls.server_name_for_host(&config.host);
    let mut builder = native_tls::TlsConnector::builder();
    if matches!(
        config.tls.verification,
        RawTcpTlsVerification::AllowInvalidCertificates
    ) {
        builder.danger_accept_invalid_certs(true);
        builder.danger_accept_invalid_hostnames(true);
    }
    let connector = builder
        .build()
        .map_err(|error| format!("failed to initialize TLS connector: {error}"))?;
    let connector = tokio_native_tls::TlsConnector::from(connector);
    let tls_stream = connector
        .connect(&server_name, stream)
        .await
        .map_err(|error| raw_tcp_tls_error_message(&server_name, &error))?;

    Ok(RawTcpStream::Tls(tls_stream))
}

fn raw_tcp_io_error_message(action: &str, endpoint: &str, error: &std::io::Error) -> String {
    match error.kind() {
        ErrorKind::ConnectionRefused => {
            format!("connection refused while trying to {action} {endpoint}")
        }
        ErrorKind::TimedOut => format!("timed out while trying to {action} {endpoint}"),
        ErrorKind::NotFound => format!("could not resolve host while trying to {action} {endpoint}"),
        ErrorKind::InvalidInput => format!("invalid host or port while trying to {action} {endpoint}"),
        ErrorKind::ConnectionAborted
        | ErrorKind::ConnectionReset
        | ErrorKind::BrokenPipe
        | ErrorKind::UnexpectedEof => {
            format!("remote closed the connection while trying to {action} {endpoint}")
        }
        ErrorKind::PermissionDenied => {
            format!("permission denied while trying to {action} {endpoint}")
        }
        _ => format!("failed to {action} {endpoint}: {error}"),
    }
}

fn raw_tcp_tls_error_message(server_name: &str, error: &impl std::fmt::Display) -> String {
    format!("TLS certificate or handshake failed for {server_name}: {error}")
}

fn encode_raw_tcp_text_input(bytes: &[u8], line_ending: RawTcpLineEnding) -> Vec<u8> {
    if matches!(line_ending, RawTcpLineEnding::None) {
        return bytes.to_vec();
    }

    let replacement = match line_ending {
        RawTcpLineEnding::Lf => b"\n".as_slice(),
        RawTcpLineEnding::CrLf => b"\r\n".as_slice(),
        RawTcpLineEnding::Cr => b"\r".as_slice(),
        RawTcpLineEnding::None => unreachable!(),
    };

    let mut encoded = Vec::with_capacity(bytes.len() + 4);
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
                encoded.extend_from_slice(replacement);
            }
            b'\n' => encoded.extend_from_slice(replacement),
            byte => encoded.push(byte),
        }
        index += 1;
    }
    encoded
}

fn parse_raw_tcp_hex_input(bytes: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).context("Raw TCP hex input must be UTF-8 text")?;
    let mut nibbles = Vec::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        let Some(value) = ch.to_digit(16) else {
            bail!("Raw TCP hex input contains non-hex character '{ch}'");
        };
        nibbles.push(value as u8);
    }
    if nibbles.len() % 2 != 0 {
        bail!("Raw TCP hex input must contain an even number of hex digits");
    }

    let mut parsed = Vec::with_capacity(nibbles.len() / 2);
    for pair in nibbles.chunks_exact(2) {
        parsed.push((pair[0] << 4) | pair[1]);
    }
    Ok(parsed)
}

fn format_raw_tcp_hexdump(bytes: &[u8], offset: &mut u64, include_ascii: bool) -> Vec<u8> {
    if bytes.is_empty() {
        return Vec::new();
    }

    let mut output = String::new();
    for chunk in bytes.chunks(RAW_TCP_HEXDUMP_WIDTH) {
        let _ = write!(&mut output, "{:08x}  ", *offset);
        for index in 0..RAW_TCP_HEXDUMP_WIDTH {
            if let Some(byte) = chunk.get(index) {
                let _ = write!(&mut output, "{byte:02x} ");
            } else {
                output.push_str("   ");
            }
            if index == 7 {
                output.push(' ');
            }
        }
        if include_ascii {
            output.push_str(" |");
            for byte in chunk {
                output.push(printable_raw_tcp_ascii(*byte));
            }
            output.push('|');
        }
        output.push_str("\r\n");
        *offset = offset.saturating_add(chunk.len() as u64);
    }
    output.into_bytes()
}

fn printable_raw_tcp_ascii(byte: u8) -> char {
    if byte.is_ascii_graphic() || byte == b' ' {
        byte as char
    } else {
        '.'
    }
}

#[cfg(test)]
mod raw_tcp_tests {
    use super::*;

    fn wait_for_raw_tcp_session(
        session: &mut RawTcpSession,
        predicate: impl Fn(&RawTcpSession) -> bool,
    ) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            session.read_pending();
            if predicate(session) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn raw_tcp_text_input_maps_line_endings() {
        assert_eq!(
            encode_raw_tcp_text_input(b"GET /\n", RawTcpLineEnding::CrLf),
            b"GET /\r\n"
        );
        assert_eq!(
            encode_raw_tcp_text_input(b"a\r\nb\r", RawTcpLineEnding::Lf),
            b"a\nb\n"
        );
        assert_eq!(
            encode_raw_tcp_text_input(b"a\n", RawTcpLineEnding::None),
            b"a\n"
        );
    }

    #[test]
    fn raw_tcp_hex_parser_accepts_whitespace_separated_bytes() {
        assert_eq!(parse_raw_tcp_hex_input(b"48 65 6c 6c 6f").unwrap(), b"Hello");
        assert_eq!(parse_raw_tcp_hex_input(b"48656c6c6f").unwrap(), b"Hello");
    }

    #[test]
    fn raw_tcp_hex_parser_rejects_invalid_or_odd_input() {
        assert!(parse_raw_tcp_hex_input(b"4").is_err());
        assert!(parse_raw_tcp_hex_input(b"zz").is_err());
    }

    #[test]
    fn raw_tcp_terminal_ingress_neutralizes_string_controls() {
        let mut ingress = RawTcpTerminalIngress::default();
        assert_eq!(ingress.filter(b"\x1b]0;title\x07ok"), b"?0;title\x07ok");
        assert_eq!(ingress.filter(&[0x9d, b'a']), b"?a");
    }

    #[test]
    fn raw_tcp_hexdump_can_include_ascii_column() {
        let mut offset = 0;
        let dump = format_raw_tcp_hexdump(b"Hello", &mut offset, true);

        assert_eq!(offset, 5);
        assert!(String::from_utf8(dump).unwrap().contains("|Hello|"));
    }

    #[test]
    fn raw_tcp_error_message_identifies_connection_refused() {
        let error = std::io::Error::new(ErrorKind::ConnectionRefused, "refused");
        let message = raw_tcp_io_error_message("connect to", "127.0.0.1:7", &error);

        assert!(message.contains("connection refused"));
    }

    #[test]
    fn raw_tcp_error_messages_cover_common_failure_reasons() {
        let timeout = std::io::Error::new(ErrorKind::TimedOut, "slow");
        assert!(raw_tcp_io_error_message("connect to", "10.0.0.1:7", &timeout).contains("timed out"));

        let host = std::io::Error::new(ErrorKind::NotFound, "dns");
        assert!(
            raw_tcp_io_error_message("connect to", "missing.invalid:7", &host)
                .contains("could not resolve host")
        );

        let invalid = std::io::Error::new(ErrorKind::InvalidInput, "bad address");
        assert!(
            raw_tcp_io_error_message("connect to", "bad host:7", &invalid)
                .contains("invalid host or port")
        );

        let reset = std::io::Error::new(ErrorKind::ConnectionReset, "reset");
        assert!(
            raw_tcp_io_error_message("read from", "127.0.0.1:7", &reset)
                .contains("remote closed the connection")
        );
    }

    #[test]
    fn raw_tcp_validation_and_tls_error_copy_are_specific() {
        let empty_host = RawTcpSessionConfig {
            host: "  ".to_string(),
            port: 7,
            line_ending: RawTcpLineEnding::Lf,
            display_mode: RawTcpDisplayMode::Text,
            send_mode: RawTcpSendMode::Text,
            tls: RawTcpTlsConfig::disabled(),
        };
        assert_eq!(
            empty_host.validate().unwrap_err().to_string(),
            "Raw TCP host is required"
        );

        let invalid_port = RawTcpSessionConfig {
            port: 0,
            host: "127.0.0.1".to_string(),
            ..empty_host
        };
        assert_eq!(
            invalid_port.validate().unwrap_err().to_string(),
            "Raw TCP port must be greater than zero"
        );

        assert!(
            raw_tcp_tls_error_message("example.test", &"certificate verify failed")
                .contains("TLS certificate or handshake failed")
        );
    }

    #[test]
    fn raw_tcp_tls_config_uses_host_when_server_name_is_blank() {
        let tls = RawTcpTlsConfig {
            enabled: true,
            verification: RawTcpTlsVerification::System,
            server_name: Some("   ".to_string()),
        };

        assert_eq!(tls.server_name_for_host("example.test"), "example.test");
    }

    #[test]
    fn raw_tcp_session_reads_local_socket_and_marks_remote_close() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"hello from raw tcp").unwrap();
        });

        let mut session = RawTcpSession::new(
            RawTcpSessionConfig {
                host: "127.0.0.1".to_string(),
                port,
                line_ending: RawTcpLineEnding::Lf,
                display_mode: RawTcpDisplayMode::Text,
                send_mode: RawTcpSendMode::Text,
                tls: RawTcpTlsConfig::disabled(),
            },
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );

        assert!(wait_for_raw_tcp_session(&mut session, |session| {
            session.buffer_text().contains("hello from raw tcp")
        }));
        server.join().unwrap();
        assert!(wait_for_raw_tcp_session(&mut session, |session| {
            matches!(session.lifecycle(), TerminalLifecycle::Exited(None))
                && session
                    .buffer_text()
                    .contains("Raw TCP remote closed the connection.")
        }));
    }

    #[test]
    fn raw_tcp_runtime_option_changes_keep_existing_socket() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
        let (payload_tx, payload_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            accepted_tx.send(()).unwrap();

            let mut first = [0_u8; 5];
            stream.read_exact(&mut first).unwrap();
            payload_tx.send(first.to_vec()).unwrap();

            let mut second = [0_u8; 2];
            stream.read_exact(&mut second).unwrap();
            payload_tx.send(second.to_vec()).unwrap();

            let _ = release_rx.recv_timeout(std::time::Duration::from_secs(2));
        });

        let mut session = TerminalSession::raw_tcp_with_graphics_and_encoding(
            RawTcpSessionConfig {
                host: "127.0.0.1".to_string(),
                port,
                line_ending: RawTcpLineEnding::Lf,
                display_mode: RawTcpDisplayMode::Text,
                send_mode: RawTcpSendMode::Text,
                tls: RawTcpTlsConfig::disabled(),
            },
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );

        accepted_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        session.write_text("ping\n").unwrap();
        assert_eq!(
            payload_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            b"ping\n"
        );

        session
            .set_raw_tcp_runtime_options(RawTcpRuntimeOptions {
                line_ending: RawTcpLineEnding::None,
                display_mode: RawTcpDisplayMode::Mixed,
                send_mode: RawTcpSendMode::Hex,
            })
            .unwrap();
        session.write_text("41 42").unwrap();
        assert_eq!(
            payload_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            b"AB"
        );
        session.read_pending();
        assert!(session.status().lifecycle.is_running());

        release_tx.send(()).unwrap();
        session.shutdown();
        server.join().unwrap();
    }
}
