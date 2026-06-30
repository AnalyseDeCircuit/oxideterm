use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::net::UdpSocket;

const RAW_UDP_READ_BUFFER_SIZE: usize = 65_535;
const RAW_UDP_HEXDUMP_WIDTH: usize = 16;
const RAW_UDP_ESC_BYTE: u8 = 0x1b;
const RAW_UDP_STRING_CONTROL_MARKER: &[u8] = b"?";
const RAW_UDP_STATUS_BOUND: &str = "bound";
const RAW_UDP_STATUS_BIND_FAILED: &str = "bind_failed";
const RAW_UDP_STATUS_ERROR: &str = "error";
const RAW_UDP_STATUS_CLOSED: &str = "closed";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawUdpSessionConfig {
    pub remote_host: String,
    pub remote_port: u16,
    pub local_bind_host: Option<String>,
    pub local_bind_port: u16,
    pub line_ending: RawUdpLineEnding,
    pub display_mode: RawUdpDisplayMode,
    pub send_mode: RawUdpSendMode,
}

impl RawUdpSessionConfig {
    pub fn remote_endpoint_label(&self) -> String {
        format!("{}:{}", self.remote_host, self.remote_port)
    }

    pub fn local_bind_label(&self) -> String {
        let host = self.local_bind_host.as_deref().unwrap_or("*");
        format!("{}:{}", host, self.local_bind_port)
    }

    fn validate(&self) -> Result<()> {
        if self.remote_host.trim().is_empty() {
            bail!("Raw UDP remote host is required");
        }
        if self.remote_port == 0 {
            bail!("Raw UDP remote port must be greater than zero");
        }
        if self
            .local_bind_host
            .as_deref()
            .is_some_and(|host| host.trim().is_empty())
        {
            bail!("Raw UDP local bind host must not be empty");
        }
        Ok(())
    }
}

pub struct RawUdpSession {
    config: RawUdpSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: Receiver<AlacEvent>,
    worker_rx: Receiver<RawUdpWorkerEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    command_tx: tokio::sync::mpsc::Sender<RawUdpCommand>,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    graphics_alt_screen_active: bool,
    output_queue: VecDeque<RawUdpDatagram>,
    output_queue_bytes: usize,
    magic_scan: MagicScanWindow,
    encoding: TerminalEncoding,
    output_decoder: TerminalOutputDecoder,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    input_encoder: TerminalInputEncoder,
    encoding_detector: EncodingMismatchDetector,
    terminal_ingress: RawUdpTerminalIngress,
    hexdump_offset: u64,
}

#[derive(Debug)]
enum RawUdpCommand {
    Datagram(Vec<u8>),
    Close,
}

#[derive(Debug)]
enum RawUdpWorkerEvent {
    Bound { local_addr: SocketAddr },
    Datagram(RawUdpDatagram),
    Failed(RawUdpWorkerFailure),
    Closed,
}

#[derive(Clone, Debug)]
struct RawUdpDatagram {
    source: SocketAddr,
    received_at_unix_ms: u128,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct RawUdpWorkerFailure {
    status: RawUdpFailureStatus,
    message: String,
}

#[derive(Debug)]
enum RawUdpFailureStatus {
    BindFailed,
    Error,
}

impl RawUdpFailureStatus {
    fn label(&self) -> &'static str {
        match self {
            Self::BindFailed => RAW_UDP_STATUS_BIND_FAILED,
            Self::Error => RAW_UDP_STATUS_ERROR,
        }
    }
}

#[derive(Debug, Default)]
struct RawUdpTerminalIngress {
    pending_escape: bool,
}

impl RawUdpTerminalIngress {
    fn filter(&mut self, bytes: &[u8]) -> Vec<u8> {
        if bytes.is_empty() {
            return Vec::new();
        }

        let mut filtered = Vec::with_capacity(bytes.len());
        let mut index = 0;
        if self.pending_escape {
            self.pending_escape = false;
            append_raw_udp_escape_pair(&mut filtered, bytes[0]);
            index = 1;
        }

        while index < bytes.len() {
            let byte = bytes[index];
            if is_raw_udp_c1_string_control(byte) {
                filtered.extend_from_slice(RAW_UDP_STRING_CONTROL_MARKER);
                index += 1;
                continue;
            }
            if byte != RAW_UDP_ESC_BYTE {
                filtered.push(byte);
                index += 1;
                continue;
            }

            let Some(next) = bytes.get(index + 1).copied() else {
                // Datagram display can still process split render chunks after
                // output processors, so preserve the ESC boundary defensively.
                self.pending_escape = true;
                break;
            };
            append_raw_udp_escape_pair(&mut filtered, next);
            index += 2;
        }

        filtered
    }
}

fn append_raw_udp_escape_pair(output: &mut Vec<u8>, next: u8) {
    match next {
        // OSC, DCS, PM, APC, and SOS can carry arbitrary payloads. Raw UDP is
        // untrusted datagram data, so render these controls visibly.
        b']' | b'P' | b'_' | b'^' | b'X' => {
            output.extend_from_slice(RAW_UDP_STRING_CONTROL_MARKER);
            output.push(next);
        }
        _ => {
            output.push(RAW_UDP_ESC_BYTE);
            output.push(next);
        }
    }
}

fn is_raw_udp_c1_string_control(byte: u8) -> bool {
    matches!(byte, 0x90 | 0x98 | 0x9d | 0x9e | 0x9f)
}

impl RawUdpSession {
    pub fn new(
        config: RawUdpSessionConfig,
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
            let _ = worker_tx.send(RawUdpWorkerEvent::Failed(RawUdpWorkerFailure {
                status: RawUdpFailureStatus::Error,
                message: error.to_string(),
            }));
        } else if let Some(runtime) = runtime.as_ref() {
            let worker_config = config.clone();
            runtime.spawn(run_raw_udp_worker(worker_config, command_rx, worker_tx));
        } else {
            let _ = worker_tx.send(RawUdpWorkerEvent::Failed(RawUdpWorkerFailure {
                status: RawUdpFailureStatus::Error,
                message: "failed to initialize Raw UDP runtime".to_string(),
            }));
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
            terminal_ingress: RawUdpTerminalIngress::default(),
            hexdump_offset: 0,
        }
    }

    fn title_text(&self) -> String {
        format!("UDP {}", self.config.remote_endpoint_label())
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

            if let Some(datagram) = self.output_queue.pop_front() {
                self.output_queue_bytes = self.output_queue_bytes.saturating_sub(datagram.bytes.len());
                report.drained_bytes = report.drained_bytes.saturating_add(datagram.bytes.len());
                report.events_drained += 1;
                self.feed_transport_datagram(&datagram);
                report.mark_changed();
                continue;
            }

            match self.worker_rx.try_recv() {
                Ok(RawUdpWorkerEvent::Bound { local_addr }) => {
                    self.title = Some(self.title_text());
                    self.pending_events
                        .push(TerminalEvent::TitleChanged(self.title_text()));
                    self.feed_utf8_terminal_output(
                        format!(
                            "Raw UDP status: {RAW_UDP_STATUS_BOUND}; local {local_addr}; remote {}\r\n",
                            self.config.remote_endpoint_label()
                        )
                        .as_bytes(),
                    );
                    report.events_drained += 1;
                    report.mark_changed();
                }
                Ok(RawUdpWorkerEvent::Datagram(datagram)) => {
                    if report.drained_bytes > 0
                        && report.drained_bytes.saturating_add(datagram.bytes.len()) > budget.max_bytes
                    {
                        self.output_queue_bytes =
                            self.output_queue_bytes.saturating_add(datagram.bytes.len());
                        self.output_queue.push_back(datagram);
                        report.budget_exhausted = true;
                        break;
                    }
                    report.drained_bytes = report.drained_bytes.saturating_add(datagram.bytes.len());
                    report.events_drained += 1;
                    self.feed_transport_datagram(&datagram);
                    report.mark_changed();
                }
                Ok(RawUdpWorkerEvent::Failed(failure)) => {
                    self.lifecycle = TerminalLifecycle::Exited(None);
                    self.feed_utf8_terminal_output(
                        format!(
                            "\r\nRaw UDP status: {}; {}\r\n",
                            failure.status.label(),
                            failure.message
                        )
                        .as_bytes(),
                    );
                    self.pending_events.push(TerminalEvent::ChildExited(None));
                    report.events_drained += 1;
                    report.mark_changed();
                    break;
                }
                Ok(RawUdpWorkerEvent::Closed) => {
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.feed_utf8_terminal_output(
                            format!("\r\nRaw UDP status: {RAW_UDP_STATUS_CLOSED}\r\n").as_bytes(),
                        );
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

    fn feed_transport_datagram(&mut self, datagram: &RawUdpDatagram) {
        for kind in self.magic_scan.scan(&datagram.bytes) {
            self.pending_events.push(TerminalEvent::MagicDetected(kind));
        }
        let processed_output = self.process_terminal_output(&datagram.bytes);
        let display_output = self.prepare_display_datagram(datagram, processed_output.as_ref());
        self.feed_plain_transport_output(&display_output);
    }

    fn prepare_display_datagram(&mut self, datagram: &RawUdpDatagram, bytes: &[u8]) -> Vec<u8> {
        let mut output = format!(
            "\r\n[UDP datagram from {}, {} bytes, received {}]\r\n",
            datagram.source,
            bytes.len(),
            datagram.received_at_unix_ms,
        )
        .into_bytes();
        match self.config.display_mode {
            RawUdpDisplayMode::Text => output.extend(self.terminal_ingress.filter(bytes)),
            RawUdpDisplayMode::Hex => {
                output.extend(format_raw_udp_hexdump(bytes, &mut self.hexdump_offset, false))
            }
            RawUdpDisplayMode::Mixed => {
                output.extend(format_raw_udp_hexdump(bytes, &mut self.hexdump_offset, true))
            }
        }
        output.extend_from_slice(b"\r\n");
        output
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
            AlacEvent::PtyWrite(_) => false,
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

    fn send_command(&mut self, command: RawUdpCommand) -> Result<()> {
        self.command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    fn encode_user_input(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        match self.config.send_mode {
            RawUdpSendMode::Text => Ok(encode_raw_udp_text_input(bytes, self.config.line_ending)),
            RawUdpSendMode::Hex => parse_raw_udp_hex_input(bytes),
        }
    }
}

impl TerminalSessionBackend for RawUdpSession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::RawUdp
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
            self.send_command(RawUdpCommand::Datagram(bytes.to_vec()))?;
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

    fn set_raw_udp_runtime_options(&mut self, options: RawUdpRuntimeOptions) -> Result<()> {
        if self.config.display_mode != options.display_mode {
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
        let _ = self.send_command(RawUdpCommand::Close);
        self.runtime = None;
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

async fn run_raw_udp_worker(
    config: RawUdpSessionConfig,
    mut command_rx: tokio::sync::mpsc::Receiver<RawUdpCommand>,
    worker_tx: crossbeam_channel::Sender<RawUdpWorkerEvent>,
) {
    let socket = match connect_raw_udp_socket(&config).await {
        Ok(socket) => socket,
        Err(error) => {
            let _ = worker_tx.send(RawUdpWorkerEvent::Failed(error));
            return;
        }
    };

    if let Ok(local_addr) = socket.local_addr() {
        let _ = worker_tx.send(RawUdpWorkerEvent::Bound { local_addr });
    }
    let mut buffer = vec![0_u8; RAW_UDP_READ_BUFFER_SIZE];
    loop {
        tokio::select! {
            read_result = socket.recv_from(&mut buffer) => {
                match read_result {
                    Ok((read_count, source)) => {
                        if worker_tx
                            .send(RawUdpWorkerEvent::Datagram(RawUdpDatagram {
                                source,
                                received_at_unix_ms: raw_udp_timestamp_ms(),
                                bytes: buffer[..read_count].to_vec(),
                            }))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = worker_tx.send(RawUdpWorkerEvent::Failed(RawUdpWorkerFailure {
                            status: RawUdpFailureStatus::Error,
                            message: raw_udp_io_error_message(
                                "receive from",
                                &config.remote_endpoint_label(),
                                &error,
                            ),
                        }));
                        break;
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(RawUdpCommand::Datagram(bytes)) => {
                        if let Err(error) = socket.send(&bytes).await {
                            let _ = worker_tx.send(RawUdpWorkerEvent::Failed(RawUdpWorkerFailure {
                                status: RawUdpFailureStatus::Error,
                                message: raw_udp_io_error_message(
                                    "send to",
                                    &config.remote_endpoint_label(),
                                    &error,
                                ),
                            }));
                            break;
                        }
                    }
                    Some(RawUdpCommand::Close) | None => {
                        let _ = worker_tx.send(RawUdpWorkerEvent::Closed);
                        break;
                    }
                }
            }
        }
    }
}

async fn connect_raw_udp_socket(
    config: &RawUdpSessionConfig,
) -> std::result::Result<UdpSocket, RawUdpWorkerFailure> {
    let remote_addr = resolve_raw_udp_remote_addr(config).await?;
    let bind_addr = resolve_raw_udp_bind_addr(config, remote_addr).await?;
    let socket = UdpSocket::bind(bind_addr)
        .await
        .map_err(|error| RawUdpWorkerFailure {
            status: RawUdpFailureStatus::BindFailed,
            message: raw_udp_io_error_message("bind", &bind_addr.to_string(), &error),
        })?;
    socket
        .connect(remote_addr)
        .await
        .map_err(|error| RawUdpWorkerFailure {
            status: RawUdpFailureStatus::Error,
            message: raw_udp_io_error_message("connect to", &remote_addr.to_string(), &error),
        })?;
    Ok(socket)
}

async fn resolve_raw_udp_remote_addr(
    config: &RawUdpSessionConfig,
) -> std::result::Result<SocketAddr, RawUdpWorkerFailure> {
    let endpoint = (config.remote_host.as_str(), config.remote_port);
    let mut addrs = tokio::net::lookup_host(endpoint)
        .await
        .map_err(|error| RawUdpWorkerFailure {
            status: RawUdpFailureStatus::Error,
            message: raw_udp_io_error_message("resolve", &config.remote_endpoint_label(), &error),
        })?;
    addrs.next().ok_or_else(|| RawUdpWorkerFailure {
        status: RawUdpFailureStatus::Error,
        message: format!(
            "could not resolve Raw UDP remote {}",
            config.remote_endpoint_label()
        ),
    })
}

async fn resolve_raw_udp_bind_addr(
    config: &RawUdpSessionConfig,
    remote_addr: SocketAddr,
) -> std::result::Result<SocketAddr, RawUdpWorkerFailure> {
    if let Some(host) = config.local_bind_host.as_deref() {
        let endpoint = (host, config.local_bind_port);
        let mut addrs = tokio::net::lookup_host(endpoint)
            .await
            .map_err(|error| RawUdpWorkerFailure {
                status: RawUdpFailureStatus::BindFailed,
                message: raw_udp_io_error_message("resolve bind", &config.local_bind_label(), &error),
            })?;
        return addrs.next().ok_or_else(|| RawUdpWorkerFailure {
            status: RawUdpFailureStatus::BindFailed,
            message: format!("could not resolve Raw UDP bind {}", config.local_bind_label()),
        });
    }

    let ip = match remote_addr.ip() {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    Ok(SocketAddr::new(ip, config.local_bind_port))
}

fn raw_udp_io_error_message(action: &str, endpoint: &str, error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::AddrInUse => {
            format!("address already in use while trying to {action} {endpoint}")
        }
        std::io::ErrorKind::AddrNotAvailable => {
            format!("address not available while trying to {action} {endpoint}")
        }
        std::io::ErrorKind::ConnectionRefused => {
            format!("connection refused while trying to {action} {endpoint}")
        }
        std::io::ErrorKind::TimedOut => format!("timed out while trying to {action} {endpoint}"),
        std::io::ErrorKind::NotFound => {
            format!("could not resolve host while trying to {action} {endpoint}")
        }
        std::io::ErrorKind::InvalidInput => {
            format!("invalid address while trying to {action} {endpoint}")
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("permission denied while trying to {action} {endpoint}")
        }
        _ => format!("failed to {action} {endpoint}: {error}"),
    }
}

fn raw_udp_timestamp_ms() -> u128 {
    // Wall-clock milliseconds are enough for packet ordering hints without
    // adding a date-time dependency to the terminal crate.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn encode_raw_udp_text_input(bytes: &[u8], line_ending: RawUdpLineEnding) -> Vec<u8> {
    if matches!(line_ending, RawUdpLineEnding::None) {
        return bytes.to_vec();
    }

    let replacement = match line_ending {
        RawUdpLineEnding::Lf => b"\n".as_slice(),
        RawUdpLineEnding::CrLf => b"\r\n".as_slice(),
        RawUdpLineEnding::Cr => b"\r".as_slice(),
        RawUdpLineEnding::None => unreachable!(),
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

fn parse_raw_udp_hex_input(bytes: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).context("Raw UDP hex input must be UTF-8 text")?;
    let mut nibbles = Vec::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        let Some(value) = ch.to_digit(16) else {
            bail!("Raw UDP hex input contains non-hex character '{ch}'");
        };
        nibbles.push(value as u8);
    }
    if nibbles.len() % 2 != 0 {
        bail!("Raw UDP hex input must contain an even number of hex digits");
    }

    let mut parsed = Vec::with_capacity(nibbles.len() / 2);
    for pair in nibbles.chunks_exact(2) {
        parsed.push((pair[0] << 4) | pair[1]);
    }
    Ok(parsed)
}

fn format_raw_udp_hexdump(bytes: &[u8], offset: &mut u64, include_ascii: bool) -> Vec<u8> {
    use std::fmt::Write as _;

    if bytes.is_empty() {
        return Vec::new();
    }

    let mut output = String::new();
    for chunk in bytes.chunks(RAW_UDP_HEXDUMP_WIDTH) {
        let _ = write!(&mut output, "{:08x}  ", *offset);
        for index in 0..RAW_UDP_HEXDUMP_WIDTH {
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
                output.push(printable_raw_udp_ascii(*byte));
            }
            output.push('|');
        }
        output.push_str("\r\n");
        *offset = offset.saturating_add(chunk.len() as u64);
    }
    output.into_bytes()
}

fn printable_raw_udp_ascii(byte: u8) -> char {
    if byte.is_ascii_graphic() || byte == b' ' {
        byte as char
    } else {
        '.'
    }
}

#[cfg(test)]
mod raw_udp_tests {
    use super::*;

    fn wait_for_raw_udp_session(
        session: &mut TerminalSession,
        predicate: impl Fn(&TerminalSession) -> bool,
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

    fn raw_udp_config(remote_port: u16) -> RawUdpSessionConfig {
        RawUdpSessionConfig {
            remote_host: "127.0.0.1".to_string(),
            remote_port,
            local_bind_host: Some("127.0.0.1".to_string()),
            local_bind_port: 0,
            line_ending: RawUdpLineEnding::None,
            display_mode: RawUdpDisplayMode::Text,
            send_mode: RawUdpSendMode::Text,
        }
    }

    fn parse_raw_udp_bound_port(buffer_text: &str) -> Option<u16> {
        buffer_text
            .split("Raw UDP status: bound; local 127.0.0.1:")
            .nth(1)
            .and_then(|tail| tail.split(';').next())
            .and_then(|port| port.parse::<u16>().ok())
    }

    #[test]
    fn raw_udp_text_input_maps_line_endings() {
        assert_eq!(
            encode_raw_udp_text_input(b"ping\n", RawUdpLineEnding::CrLf),
            b"ping\r\n"
        );
        assert_eq!(
            encode_raw_udp_text_input(b"ping\n", RawUdpLineEnding::None),
            b"ping\n"
        );
    }

    #[test]
    fn raw_udp_hex_parser_accepts_whitespace_separated_bytes() {
        assert_eq!(parse_raw_udp_hex_input(b"48 65 6c 6c 6f").unwrap(), b"Hello");
        assert_eq!(parse_raw_udp_hex_input(b"48656c6c6f").unwrap(), b"Hello");
    }

    #[test]
    fn raw_udp_hex_parser_rejects_invalid_or_odd_input() {
        assert!(parse_raw_udp_hex_input(b"4").is_err());
        assert!(parse_raw_udp_hex_input(b"zz").is_err());
    }

    #[test]
    fn raw_udp_terminal_ingress_neutralizes_string_controls() {
        let mut ingress = RawUdpTerminalIngress::default();
        assert_eq!(ingress.filter(b"\x1b]0;title\x07ok"), b"?]0;title\x07ok");
        assert_eq!(ingress.filter(&[0x9d, b'a']), b"?a");
    }

    #[test]
    fn raw_udp_datagram_renderer_preserves_packet_boundary() {
        let datagram = RawUdpDatagram {
            source: "127.0.0.1:9001".parse().unwrap(),
            received_at_unix_ms: 1_782_821_696_789,
            bytes: b"hello".to_vec(),
        };
        let mut session = RawUdpSession::new(
            raw_udp_config(9000),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );

        let rendered =
            String::from_utf8(session.prepare_display_datagram(&datagram, b"hello")).unwrap();

        assert!(rendered.contains(
            "[UDP datagram from 127.0.0.1:9001, 5 bytes, received 1782821696789]"
        ));
        assert!(rendered.contains("hello"));
    }

    #[test]
    fn raw_udp_error_message_identifies_address_in_use() {
        let error = std::io::Error::new(ErrorKind::AddrInUse, "busy");
        let message = raw_udp_io_error_message("bind", "127.0.0.1:9000", &error);

        assert!(message.contains("address already in use"));
    }

    #[test]
    fn raw_udp_session_receives_local_datagram() {
        let remote = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let remote_addr = remote.local_addr().unwrap();
        let mut session = TerminalSession::raw_udp_with_graphics_and_encoding(
            raw_udp_config(remote_addr.port()),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );

        assert!(wait_for_raw_udp_session(&mut session, |session| {
            session.buffer_text().contains("Raw UDP status: bound")
        }));
        let local_port = parse_raw_udp_bound_port(&session.buffer_text()).unwrap();
        remote.send_to(b"hello udp", ("127.0.0.1", local_port)).unwrap();

        assert!(wait_for_raw_udp_session(&mut session, |session| {
            let text = session.buffer_text();
            text.contains("UDP datagram") && text.contains("hello udp")
        }));
    }

    #[test]
    fn raw_udp_session_sends_connected_datagram() {
        let remote = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        remote
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let remote_addr = remote.local_addr().unwrap();
        let mut session = TerminalSession::raw_udp_with_graphics_and_encoding(
            raw_udp_config(remote_addr.port()),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );

        assert!(wait_for_raw_udp_session(&mut session, |session| {
            session.status().lifecycle.is_running()
                && session.buffer_text().contains("Raw UDP status: bound")
        }));
        session.write_text("ping").unwrap();

        let mut bytes = [0_u8; 16];
        let (count, _) = remote.recv_from(&mut bytes).unwrap();
        assert_eq!(&bytes[..count], b"ping");
    }

    #[test]
    fn raw_udp_runtime_option_changes_keep_socket_bound() {
        let remote = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        remote
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let remote_addr = remote.local_addr().unwrap();
        let mut session = TerminalSession::raw_udp_with_graphics_and_encoding(
            raw_udp_config(remote_addr.port()),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );

        assert!(wait_for_raw_udp_session(&mut session, |session| {
            session.status().lifecycle.is_running()
                && session.buffer_text().contains("Raw UDP status: bound")
        }));
        session
            .set_raw_udp_runtime_options(RawUdpRuntimeOptions {
                line_ending: RawUdpLineEnding::Lf,
                display_mode: RawUdpDisplayMode::Mixed,
                send_mode: RawUdpSendMode::Hex,
            })
            .unwrap();
        session.write_text("41 42").unwrap();

        let mut bytes = [0_u8; 16];
        let (count, _) = remote.recv_from(&mut bytes).unwrap();
        assert_eq!(&bytes[..count], b"AB");
        assert!(session.status().lifecycle.is_running());
    }

    #[test]
    fn raw_udp_bind_failure_uses_bind_failed_status() {
        let remote = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let occupied = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let mut config = raw_udp_config(remote.local_addr().unwrap().port());
        config.local_bind_port = occupied_port;
        let mut session = TerminalSession::raw_udp_with_graphics_and_encoding(
            config,
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );

        assert!(wait_for_raw_udp_session(&mut session, |session| {
            let text = session.buffer_text();
            text.contains("Raw UDP status: bind_failed")
                && text.contains("address already in use")
        }));
    }

    #[test]
    fn raw_udp_rebind_after_shutdown_can_reuse_fixed_local_port() {
        let remote = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let local_reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let local_port = local_reservation.local_addr().unwrap().port();
        drop(local_reservation);

        let mut config = raw_udp_config(remote.local_addr().unwrap().port());
        config.local_bind_port = local_port;
        let mut first = TerminalSession::raw_udp_with_graphics_and_encoding(
            config.clone(),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );
        assert!(wait_for_raw_udp_session(&mut first, |session| {
            session.buffer_text().contains("Raw UDP status: bound")
        }));
        first.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(30));

        let mut second = TerminalSession::raw_udp_with_graphics_and_encoding(
            config,
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );
        assert!(wait_for_raw_udp_session(&mut second, |session| {
            session.buffer_text().contains("Raw UDP status: bound")
        }));
    }
}
