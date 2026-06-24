const SERIAL_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerialSessionConfig {
    pub port_path: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: SerialParity,
    pub flow_control: SerialFlowControl,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerialPortInfo {
    pub port_path: String,
    pub display_name: String,
    pub port_type: String,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialErrorCode {
    PortNotFound,
    PermissionDenied,
    PortBusy,
    InvalidParameters,
    OpenFailed,
    WriteFailed,
    ReadFailed,
    DeviceDisconnected,
    SessionNotFound,
    UnsupportedPlatform,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerialError {
    pub code: SerialErrorCode,
    pub message: String,
    pub port_path: Option<String>,
    pub recoverable: bool,
}

impl std::fmt::Display for SerialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SerialError {}

impl SerialSessionConfig {
    pub fn validate(&self) -> Result<(), SerialError> {
        if self.port_path.trim().is_empty() {
            return Err(SerialError::new(
                SerialErrorCode::InvalidParameters,
                "Serial port path is required",
                None,
                false,
            ));
        }
        if self.baud_rate == 0 {
            return Err(SerialError::new(
                SerialErrorCode::InvalidParameters,
                "Serial baud rate must be greater than zero",
                Some(self.port_path.clone()),
                false,
            ));
        }
        if !(5..=8).contains(&self.data_bits) {
            return Err(SerialError::new(
                SerialErrorCode::InvalidParameters,
                "Serial data bits must be between 5 and 8",
                Some(self.port_path.clone()),
                false,
            ));
        }
        if !matches!(self.stop_bits, 1 | 2) {
            return Err(SerialError::new(
                SerialErrorCode::InvalidParameters,
                "Serial stop bits must be 1 or 2",
                Some(self.port_path.clone()),
                false,
            ));
        }
        Ok(())
    }

    fn title_text(&self) -> String {
        format!("Serial {}", self.port_path)
    }
}

impl SerialError {
    fn new(
        code: SerialErrorCode,
        message: impl Into<String>,
        port_path: Option<String>,
        recoverable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            port_path,
            recoverable,
        }
    }
}

pub fn serial_list_ports() -> Result<Vec<SerialPortInfo>, SerialError> {
    let mut ports: Vec<SerialPortInfo> = serialport::available_ports()
        .map_err(|error| {
            SerialError::new(
                SerialErrorCode::OpenFailed,
                format!("Failed to list serial ports: {error}"),
                None,
                true,
            )
        })?
        .into_iter()
        .map(map_serial_port_info)
        .collect();
    ports.sort_by(|left, right| left.port_path.cmp(&right.port_path));
    Ok(ports)
}

pub struct SerialSession {
    config: SerialSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: Receiver<AlacEvent>,
    worker_rx: Receiver<SerialWorkerEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    command_tx: crossbeam_channel::Sender<SerialCommand>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    port_reservation: Option<SerialPortReservation>,
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
    modem_consumer: ModemConsumer,
    shell_integration: TerminalShellIntegration,
    serial_console_ingress: SerialConsoleIngress,
}

#[derive(Debug)]
enum SerialCommand {
    Data(Vec<u8>),
    Close,
}

#[derive(Debug)]
enum SerialWorkerEvent {
    Connected,
    Output(Vec<u8>),
    Failed(SerialError),
    Closed,
}

#[derive(Debug)]
struct SerialPortReservation {
    normalized_port_path: String,
}

impl Drop for SerialPortReservation {
    fn drop(&mut self) {
        if let Ok(mut owners) = serial_port_owners().lock() {
            owners.remove(&self.normalized_port_path);
        }
    }
}

const ESC_BYTE: u8 = 0x1b;
const SERIAL_STRING_CONTROL_MARKER: &[u8] = b"?";

#[derive(Debug, Default)]
struct SerialConsoleIngress {
    pending_escape: bool,
}

impl SerialConsoleIngress {
    fn filter(&mut self, bytes: &[u8]) -> Vec<u8> {
        if bytes.is_empty() {
            return Vec::new();
        }

        let mut filtered = Vec::with_capacity(bytes.len());
        let mut index = 0;
        if self.pending_escape {
            self.pending_escape = false;
            append_serial_escape_pair(&mut filtered, bytes[0]);
            index = 1;
        }

        while index < bytes.len() {
            let byte = bytes[index];
            if byte != ESC_BYTE {
                filtered.push(byte);
                index += 1;
                continue;
            }

            let Some(next) = bytes.get(index + 1).copied() else {
                // Serial reads can split an escape sequence across chunks.
                self.pending_escape = true;
                break;
            };
            append_serial_escape_pair(&mut filtered, next);
            index += 2;
        }

        filtered
    }
}

fn append_serial_escape_pair(output: &mut Vec<u8>, next: u8) {
    match next {
        // Raw serial boot noise can contain unterminated terminal string controls.
        // Passing them to the VTE parser can hide every later printable byte.
        b']' | b'P' | b'_' | b'^' | b'X' => {
            output.extend_from_slice(SERIAL_STRING_CONTROL_MARKER)
        }
        _ => {
            output.push(ESC_BYTE);
            output.push(next);
        }
    }
}

impl SerialSession {
    pub fn new(
        config: SerialSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Result<Self, SerialError> {
        config.validate()?;
        ensure_serial_port_exists(&config.port_path)?;
        let port_reservation = reserve_serial_port(&config.port_path)?;

        let resize = TerminalResize::new(cols, rows, 0, 0);
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        let (event_tx, event_rx) = unbounded();
        let (worker_tx, worker_rx) = unbounded();
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let listener = LocalEventListener { tx: event_tx };

        let mut term_config = Config::default();
        term_config.scrolling_history = scrollback_lines;
        term_config.kitty_keyboard = true;
        let term = Arc::new(FairMutex::new(Term::new(term_config, &size, listener)));

        // Tauri owns serial handles in a registry; native mirrors that by
        // reserving the normalized port for the lifetime of this backend.
        let worker_config = config.clone();
        let worker_handle = std::thread::spawn(move || {
            run_serial_worker(worker_config, command_rx, worker_tx);
        });

        let mut serial_graphics_options = graphics_options;
        // A serial console is a raw byte stream; image protocols are opt-in
        // terminal features and should not parse arbitrary device boot noise.
        serial_graphics_options.enabled = false;

        Ok(Self {
            config,
            term,
            parser: Processor::new(),
            event_rx,
            worker_rx,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            command_tx,
            worker_handle: Some(worker_handle),
            port_reservation: Some(port_reservation),
            title: None,
            graphics_ingress: GraphicsIngress::new(serial_graphics_options),
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
            modem_consumer: ModemConsumer::new(),
            shell_integration: TerminalShellIntegration::default(),
            serial_console_ingress: SerialConsoleIngress::default(),
        })
    }

    fn title_text(&self) -> String {
        self.config.title_text()
    }

    fn release_port_reservation(&mut self) {
        // Dropping the reservation removes the in-process owner entry while
        // the worker thread owns the OS-level serial handle lifecycle.
        self.port_reservation.take();
    }

    fn drain_worker_events_with_budget(
        &mut self,
        budget: TerminalDrainBudget,
    ) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = TerminalDrainReport::default();
        loop {
            if report.drained_bytes >= budget.max_bytes || report.events_drained >= budget.max_events
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
                Ok(SerialWorkerEvent::Connected) => {
                    self.title = Some(self.title_text());
                    self.pending_events
                        .push(TerminalEvent::TitleChanged(self.title_text()));
                    report.events_drained += 1;
                    report.mark_changed();
                }
                Ok(SerialWorkerEvent::Output(bytes)) => {
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
                Ok(SerialWorkerEvent::Failed(error)) => {
                    self.lifecycle = TerminalLifecycle::Exited(None);
                    self.release_port_reservation();
                    self.feed_utf8_terminal_output(
                        format!("\r\nSerial session failed: {}\r\n", error.message).as_bytes(),
                    );
                    self.pending_events.push(TerminalEvent::ChildExited(None));
                    report.events_drained += 1;
                    report.mark_changed();
                    break;
                }
                Ok(SerialWorkerEvent::Closed) => {
                    self.release_port_reservation();
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    report.events_drained += 1;
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.release_port_reservation();
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
        let processed_output = apply_terminal_output_processor(&self.output_processor, bytes);
        let bytes = processed_output.as_ref();
        let events = self.modem_consumer.process_server_output(bytes);
        self.handle_modem_consumer_events(events);
    }

    fn feed_plain_transport_output(&mut self, bytes: &[u8]) {
        let terminal_bytes = self.serial_console_ingress.filter(bytes);
        if terminal_bytes.is_empty() {
            return;
        }

        for kind in self.magic_scan.scan(&terminal_bytes) {
            self.pending_events.push(TerminalEvent::MagicDetected(kind));
        }
        let mut term = self.term.lock();
        let size = TerminalSize {
            cols: self.resize.cols,
            rows: self.resize.rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        };
        let cursor = Cell::new(graphics_cursor_from_term(&term, size));
        let mut protocol_responses = Vec::new();
        self.graphics_ingress.advance_ordered(
            &terminal_bytes,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                    if let Some(hint) = self.encoding_detector.observe(&terminal_bytes) {
                        self.pending_events.push(TerminalEvent::EncodingHint(hint));
                    }
                    let decoded = self.output_decoder.decode_to_utf8_bytes(&terminal_bytes);
                    if self.output_events_enabled && !decoded.is_empty() {
                        // Terminal recording observes decoded display bytes, not
                        // transport bytes, and is disabled on the normal path.
                        self.pending_events
                            .push(TerminalEvent::Output(decoded.as_ref().to_vec()));
                    }
                    self.shell_integration.advance(
                        &mut self.parser,
                        &mut *term,
                        decoded.as_ref(),
                        |event| self.pending_events.push(event),
                    );
                    self.graphics
                        .clear_for_alt_screen_transition(&term, &mut self.graphics_alt_screen_active);
                    cursor.set(graphics_cursor_from_term(&term, size));
                }
                TerminalGraphicsSegment::Event(event) => {
                    if let Some(response) = self.graphics.handle_event(event) {
                        protocol_responses.push(response);
                    }
                }
            },
            || cursor.get(),
        );
        drop(term);
        for response in protocol_responses {
            let _ = self.write_protocol_bytes(&response);
        }
    }

    fn flush_modem_server_writes(&mut self) -> bool {
        let mut changed = false;
        for bytes in self.modem_consumer.take_server_writes() {
            let _ = self.write_protocol_bytes(&bytes);
            changed = true;
        }
        changed
    }

    fn handle_modem_consumer_events(&mut self, events: Vec<ModemConsumerEvent>) {
        for event in events {
            match event {
                ModemConsumerEvent::WriteTerminal(bytes) => self.feed_plain_transport_output(&bytes),
                ModemConsumerEvent::SendServer(bytes) => {
                    let _ = self.write_protocol_bytes(&bytes);
                }
                ModemConsumerEvent::TransferStarted(request) => {
                    if let Some(transfer) = self.modem_consumer.active_transfer().cloned() {
                        self.pending_events
                            .push(TerminalEvent::ModemTransferPrompt { request, transfer });
                    }
                }
                ModemConsumerEvent::TransferDataQueued => {}
                ModemConsumerEvent::TransferCancelRequested => {}
            }
        }
    }

    fn feed_utf8_terminal_output(&mut self, bytes: &[u8]) {
        self.push_output_event(bytes);
        let mut term = self.term.lock();
        self.shell_integration
            .advance(&mut self.parser, &mut *term, bytes, |event| {
                self.pending_events.push(event);
            });
    }

    fn push_output_event(&mut self, bytes: &[u8]) {
        if self.output_events_enabled && !bytes.is_empty() {
            // Terminal recording is the only consumer of raw display-output events;
            // keep this allocation off the normal rendering path.
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
            AlacEvent::PtyWrite(text) => {
                let _ = self.write_protocol_bytes(text.as_bytes());
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
}

impl TerminalSessionBackend for SerialSession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::Serial
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
        if self.flush_modem_server_writes() {
            report.mark_changed();
        }
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
        self.write_protocol_bytes(bytes)
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.command_tx
                .try_send(SerialCommand::Data(bytes.to_vec()))
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        }
        Ok(())
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        let encoded = self.input_encoder.encode_text(text);
        self.write_protocol_bytes(encoded.as_ref())
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        let bytes = self
            .input_encoder
            .encode_paste(text, self.mode().contains(TermMode::BRACKETED_PASTE));
        self.write_protocol_bytes(&bytes)
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

    fn start_modem_transfer(
        &mut self,
        request: TerminalModemTransferRequest,
    ) -> Option<ModemTransfer> {
        self.modem_consumer.start_manual_transfer(request)
    }

    fn interrupt_modem_transfer(&mut self) {
        self.modem_consumer.interrupt_transfer();
    }

    fn finish_modem_transfer(&mut self) {
        self.modem_consumer.finish_transfer();
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
        let should_report = {
            let mut term = self.term.lock();
            term.is_focused = focused;
            term.mode().contains(TermMode::FOCUS_IN_OUT)
        };

        if let Some(report) = focus_report_sequence(should_report, focused) {
            self.write_protocol_bytes(report)?;
        }
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
        self.write_protocol_bytes(b"\x03")
    }

    fn kill_active_task(&mut self) -> Result<()> {
        self.write_protocol_bytes(b"\x03")
    }

    fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }
        self.release_port_reservation();
        let _ = self.command_tx.try_send(SerialCommand::Close);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

fn run_serial_worker(
    config: SerialSessionConfig,
    command_rx: crossbeam_channel::Receiver<SerialCommand>,
    worker_tx: crossbeam_channel::Sender<SerialWorkerEvent>,
) {
    let mut port = match open_serial_port(&config) {
        Ok(port) => port,
        Err(error) => {
            let _ = worker_tx.send(SerialWorkerEvent::Failed(error));
            return;
        }
    };

    let _ = worker_tx.send(SerialWorkerEvent::Connected);
    run_serial_worker_with_port(&mut *port, &config, command_rx, worker_tx);
}

// Keep the worker loop injectable so lifecycle tests can use a fake serial
// stream while production still owns a real serialport handle.
fn run_serial_worker_with_port<P>(
    port: &mut P,
    config: &SerialSessionConfig,
    command_rx: crossbeam_channel::Receiver<SerialCommand>,
    worker_tx: crossbeam_channel::Sender<SerialWorkerEvent>,
) where
    P: Read + Write + ?Sized,
{
    let mut buffer = [0_u8; 8192];
    loop {
        while let Ok(command) = command_rx.try_recv() {
            match command {
                SerialCommand::Data(bytes) => {
                    if let Err(error) = port.write_all(&bytes).and_then(|_| port.flush()) {
                        let _ = worker_tx.send(SerialWorkerEvent::Failed(map_serial_io_error(
                            error,
                            SerialErrorCode::WriteFailed,
                            &config.port_path,
                        )));
                        return;
                    }
                }
                SerialCommand::Close => {
                    let _ = worker_tx.send(SerialWorkerEvent::Closed);
                    return;
                }
            }
        }

        match port.read(&mut buffer) {
            Ok(0) => {}
            Ok(read_count) => {
                if worker_tx
                    .send(SerialWorkerEvent::Output(buffer[..read_count].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => {
                let _ = worker_tx.send(SerialWorkerEvent::Failed(map_serial_io_error(
                    error,
                    SerialErrorCode::ReadFailed,
                    &config.port_path,
                )));
                return;
            }
        }
    }
}

fn open_serial_port(
    config: &SerialSessionConfig,
) -> Result<Box<dyn serialport::SerialPort>, SerialError> {
    serialport::new(&config.port_path, config.baud_rate)
        .data_bits(map_serial_data_bits(config.data_bits)?)
        .stop_bits(map_serial_stop_bits(config.stop_bits)?)
        .parity(map_serial_parity(config.parity))
        .flow_control(map_serial_flow_control(config.flow_control))
        .timeout(SERIAL_READ_TIMEOUT)
        .open()
        .map_err(|error| map_serial_open_error(error, &config.port_path))
}

fn map_serial_port_info(port: serialport::SerialPortInfo) -> SerialPortInfo {
    match port.port_type {
        serialport::SerialPortType::UsbPort(info) => SerialPortInfo {
            display_name: info.product.clone().unwrap_or_else(|| port.port_name.clone()),
            port_path: port.port_name,
            port_type: "usb".to_string(),
            manufacturer: info.manufacturer,
            product: info.product,
            serial_number: info.serial_number,
            vid: Some(info.vid),
            pid: Some(info.pid),
        },
        serialport::SerialPortType::BluetoothPort => serial_port_info_without_usb(port, "bluetooth"),
        serialport::SerialPortType::PciPort => serial_port_info_without_usb(port, "pci"),
        serialport::SerialPortType::Unknown => serial_port_info_without_usb(port, "unknown"),
    }
}

fn serial_port_info_without_usb(
    port: serialport::SerialPortInfo,
    port_type: &str,
) -> SerialPortInfo {
    SerialPortInfo {
        display_name: port.port_name.clone(),
        port_path: port.port_name,
        port_type: port_type.to_string(),
        manufacturer: None,
        product: None,
        serial_number: None,
        vid: None,
        pid: None,
    }
}

fn ensure_serial_port_exists(port_path: &str) -> Result<(), SerialError> {
    let normalized = normalize_serial_port_path(port_path);
    let ports = serialport::available_ports().map_err(|error| {
        SerialError::new(
            SerialErrorCode::OpenFailed,
            format!("Failed to list serial ports before opening: {error}"),
            Some(port_path.to_string()),
            true,
        )
    })?;
    if ports
        .iter()
        .any(|port| normalize_serial_port_path(&port.port_name) == normalized)
    {
        return Ok(());
    }
    Err(SerialError::new(
        SerialErrorCode::PortNotFound,
        format!("Serial port not found: {port_path}"),
        Some(port_path.to_string()),
        true,
    ))
}

fn reserve_serial_port(port_path: &str) -> Result<SerialPortReservation, SerialError> {
    let normalized = normalize_serial_port_path(port_path);
    let mut owners = serial_port_owners().lock().map_err(|_| {
        SerialError::new(
            SerialErrorCode::OpenFailed,
            "Serial port registry lock is poisoned",
            Some(port_path.to_string()),
            true,
        )
    })?;
    if owners.contains_key(&normalized) {
        return Err(SerialError::new(
            SerialErrorCode::PortBusy,
            format!("Serial port is already open: {port_path}"),
            Some(port_path.to_string()),
            true,
        ));
    }
    owners.insert(normalized.clone(), port_path.to_string());
    Ok(SerialPortReservation {
        normalized_port_path: normalized,
    })
}

fn serial_port_owners() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static OWNERS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> =
        std::sync::OnceLock::new();
    OWNERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn normalize_serial_port_path(port_path: &str) -> String {
    let trimmed = port_path.trim();
    #[cfg(target_os = "windows")]
    {
        normalize_windows_serial_port_path(trimmed)
    }
    #[cfg(not(target_os = "windows"))]
    {
        trimmed.to_string()
    }
}

#[cfg(any(target_os = "windows", test))]
fn normalize_windows_serial_port_path(port_path: &str) -> String {
    let uppercase = port_path.trim().to_ascii_uppercase();
    // Windows accepts both COM10 and the Win32 device namespace form; use one
    // owner key so existence checks and duplicate reservations agree.
    uppercase
        .strip_prefix("\\\\.\\")
        .or_else(|| uppercase.strip_prefix("\\\\?\\"))
        .unwrap_or(&uppercase)
        .to_string()
}

fn map_serial_data_bits(data_bits: u8) -> Result<serialport::DataBits, SerialError> {
    match data_bits {
        5 => Ok(serialport::DataBits::Five),
        6 => Ok(serialport::DataBits::Six),
        7 => Ok(serialport::DataBits::Seven),
        8 => Ok(serialport::DataBits::Eight),
        _ => Err(SerialError::new(
            SerialErrorCode::InvalidParameters,
            "Serial data bits must be between 5 and 8",
            None,
            false,
        )),
    }
}

fn map_serial_stop_bits(stop_bits: u8) -> Result<serialport::StopBits, SerialError> {
    match stop_bits {
        1 => Ok(serialport::StopBits::One),
        2 => Ok(serialport::StopBits::Two),
        _ => Err(SerialError::new(
            SerialErrorCode::InvalidParameters,
            "Serial stop bits must be 1 or 2",
            None,
            false,
        )),
    }
}

fn map_serial_parity(parity: SerialParity) -> serialport::Parity {
    match parity {
        SerialParity::None => serialport::Parity::None,
        SerialParity::Odd => serialport::Parity::Odd,
        SerialParity::Even => serialport::Parity::Even,
    }
}

fn map_serial_flow_control(flow_control: SerialFlowControl) -> serialport::FlowControl {
    match flow_control {
        SerialFlowControl::None => serialport::FlowControl::None,
        SerialFlowControl::Software => serialport::FlowControl::Software,
        SerialFlowControl::Hardware => serialport::FlowControl::Hardware,
    }
}

fn map_serial_open_error(error: serialport::Error, port_path: &str) -> SerialError {
    let description = error.to_string();
    let lower = description.to_ascii_lowercase();
    let code = match error.kind() {
        serialport::ErrorKind::NoDevice => SerialErrorCode::PortNotFound,
        serialport::ErrorKind::InvalidInput => SerialErrorCode::InvalidParameters,
        serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) => {
            SerialErrorCode::PermissionDenied
        }
        _ if lower.contains("busy")
            || lower.contains("in use")
            || lower.contains("resource busy")
            || lower.contains("access denied") =>
        {
            SerialErrorCode::PortBusy
        }
        _ => SerialErrorCode::OpenFailed,
    };
    let recoverable = !matches!(code, SerialErrorCode::InvalidParameters);
    SerialError::new(
        code,
        format!("Failed to open serial port {port_path}: {description}"),
        Some(port_path.to_string()),
        recoverable,
    )
}

fn map_serial_io_error(
    error: std::io::Error,
    fallback: SerialErrorCode,
    port_path: &str,
) -> SerialError {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound
        | std::io::ErrorKind::BrokenPipe
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::UnexpectedEof => SerialErrorCode::DeviceDisconnected,
        std::io::ErrorKind::PermissionDenied => SerialErrorCode::PermissionDenied,
        _ => fallback,
    };
    SerialError::new(code, error.to_string(), Some(port_path.to_string()), true)
}

#[cfg(test)]
mod serial_tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{Arc, Mutex};

    fn valid_config() -> SerialSessionConfig {
        SerialSessionConfig {
            port_path: "/dev/cu.usbserial-1".to_string(),
            baud_rate: 115_200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::None,
        }
    }

    enum FakeRead {
        Bytes(Vec<u8>),
        Error(io::ErrorKind),
    }

    struct FakeSerialPort {
        reads: VecDeque<FakeRead>,
        writes: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl FakeSerialPort {
        fn new(reads: impl Into<VecDeque<FakeRead>>) -> Self {
            Self {
                reads: reads.into(),
                writes: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl Read for FakeSerialPort {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.reads.pop_front() {
                Some(FakeRead::Bytes(bytes)) => {
                    let len = bytes.len().min(buf.len());
                    buf[..len].copy_from_slice(&bytes[..len]);
                    Ok(len)
                }
                Some(FakeRead::Error(kind)) => Err(io::Error::new(kind, "fake serial error")),
                None => Err(io::Error::new(io::ErrorKind::TimedOut, "fake timeout")),
            }
        }
    }

    impl Write for FakeSerialPort {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes.lock().unwrap().push(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn test_serial_session() -> SerialSession {
        let resize = TerminalResize::new(80, 24, 0, 0);
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        let (event_tx, event_rx) = unbounded();
        let (_worker_tx, worker_rx) = unbounded();
        let (command_tx, _command_rx) = crossbeam_channel::unbounded();
        let listener = LocalEventListener { tx: event_tx };
        let mut term_config = Config::default();
        term_config.scrolling_history = 100;

        SerialSession {
            config: valid_config(),
            term: Arc::new(FairMutex::new(Term::new(term_config, &size, listener))),
            parser: Processor::new(),
            event_rx,
            worker_rx,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            command_tx,
            worker_handle: None,
            port_reservation: Some(SerialPortReservation {
                normalized_port_path: "test-serial-session".to_string(),
            }),
            title: None,
            graphics_ingress: GraphicsIngress::new(GraphicsOptions::default()),
            graphics: TerminalGraphicsState::default(),
            graphics_alt_screen_active: false,
            output_queue: VecDeque::new(),
            output_queue_bytes: 0,
            magic_scan: MagicScanWindow::default(),
            encoding: TerminalEncoding::Utf8,
            output_decoder: TerminalOutputDecoder::new(TerminalEncoding::Utf8),
            output_processor: None,
            output_events_enabled: false,
            input_encoder: TerminalInputEncoder::new(TerminalEncoding::Utf8),
            encoding_detector: EncodingMismatchDetector::new(TerminalEncoding::Utf8),
            modem_consumer: ModemConsumer::new(),
            shell_integration: TerminalShellIntegration::default(),
            serial_console_ingress: SerialConsoleIngress::default(),
        }
    }

    #[test]
    fn serial_config_validation_rejects_invalid_parameters() {
        let mut config = valid_config();
        assert!(config.validate().is_ok());

        config.baud_rate = 0;
        assert_eq!(
            config.validate().unwrap_err().code,
            SerialErrorCode::InvalidParameters
        );

        config.baud_rate = 115_200;
        config.stop_bits = 3;
        assert_eq!(
            config.validate().unwrap_err().code,
            SerialErrorCode::InvalidParameters
        );
    }

    #[test]
    fn serial_duplicate_reservation_returns_port_busy() {
        let first = reserve_serial_port("/tmp/oxideterm-test-serial").unwrap();
        let error = reserve_serial_port("/tmp/oxideterm-test-serial").unwrap_err();

        assert_eq!(error.code, SerialErrorCode::PortBusy);
        drop(first);
    }

    #[test]
    fn failed_worker_releases_port_reservation() {
        let port_path = "/tmp/oxideterm-test-serial-failed-worker";
        let reservation = reserve_serial_port(port_path).unwrap();
        let normalized_port_path = reservation.normalized_port_path.clone();
        let mut session = test_serial_session();
        session.port_reservation = Some(reservation);

        let error = SerialError::new(
            SerialErrorCode::PortBusy,
            "fake serial busy",
            Some(port_path.to_string()),
            true,
        );
        let (worker_tx, worker_rx) = unbounded();
        session.worker_rx = worker_rx;
        worker_tx.send(SerialWorkerEvent::Failed(error)).unwrap();
        session.read_pending();

        let second = reserve_serial_port(port_path).unwrap();
        assert_eq!(second.normalized_port_path, normalized_port_path);
    }

    #[test]
    fn windows_serial_normalization_collapses_device_namespace() {
        assert_eq!(normalize_windows_serial_port_path("COM10"), "COM10");
        assert_eq!(normalize_windows_serial_port_path("com10"), "COM10");
        assert_eq!(normalize_windows_serial_port_path("\\\\.\\COM10"), "COM10");
        assert_eq!(normalize_windows_serial_port_path("\\\\?\\com10"), "COM10");
        assert_eq!(normalize_windows_serial_port_path(" COM3 "), "COM3");
    }

    #[test]
    fn serial_io_error_maps_disconnect() {
        let error = map_serial_io_error(
            std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "gone"),
            SerialErrorCode::ReadFailed,
            "/dev/cu.usbserial-1",
        );

        assert_eq!(error.code, SerialErrorCode::DeviceDisconnected);
    }

    #[test]
    fn fake_serial_worker_lifecycle_writes_reads_and_reports_disconnect() {
        let config = valid_config();
        let mut port = FakeSerialPort::new(VecDeque::from([
            FakeRead::Bytes(vec![0x00, b'o', b'k']),
            FakeRead::Error(io::ErrorKind::UnexpectedEof),
        ]));
        let writes = port.writes.clone();
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let (worker_tx, worker_rx) = crossbeam_channel::unbounded();

        command_tx
            .send(SerialCommand::Data(b"at\r".to_vec()))
            .unwrap();
        run_serial_worker_with_port(&mut port, &config, command_rx, worker_tx);

        assert_eq!(writes.lock().unwrap().as_slice(), &[b"at\r".to_vec()]);
        assert!(matches!(
            worker_rx.recv().unwrap(),
            SerialWorkerEvent::Output(bytes) if bytes == [0x00, b'o', b'k']
        ));
        assert!(matches!(
            worker_rx.recv().unwrap(),
            SerialWorkerEvent::Failed(error)
                if error.code == SerialErrorCode::DeviceDisconnected
        ));
    }

    #[test]
    fn fake_serial_worker_lifecycle_closes_without_reading_after_close_command() {
        let config = valid_config();
        let mut port = FakeSerialPort::new(VecDeque::from([FakeRead::Bytes(
            b"unexpected".to_vec(),
        )]));
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let (worker_tx, worker_rx) = crossbeam_channel::unbounded();

        command_tx.send(SerialCommand::Close).unwrap();
        run_serial_worker_with_port(&mut port, &config, command_rx, worker_tx);

        assert!(matches!(
            worker_rx.recv().unwrap(),
            SerialWorkerEvent::Closed
        ));
        assert!(worker_rx.try_recv().is_err());
    }

    #[test]
    fn serial_boot_text_survives_unfinished_osc_noise() {
        let mut session = test_serial_session();

        session.feed_transport_output(b"\x1b]boot-noise-without-terminator");
        session.feed_transport_output(b"I (30) boot: ESP-IDF v3.0.7 2nd stage bootloader\r\n");

        assert!(
            session
                .buffer_text()
                .contains("I (30) boot: ESP-IDF v3.0.7")
        );
    }

    #[test]
    fn serial_preserves_split_csi_sequences() {
        let mut session = test_serial_session();

        session.feed_transport_output(b"\x1b");
        session.feed_transport_output(b"[31mred\x1b[0m\r\n");

        assert!(session.buffer_text().contains("red"));
    }

    #[test]
    #[ignore = "requires OXIDETERM_SERIAL_MANUAL_PORT to point at a real or pseudo serial device"]
    fn manual_serial_pseudo_device_round_trip_and_reopen() {
        let port_path = std::env::var("OXIDETERM_SERIAL_MANUAL_PORT")
            .expect("OXIDETERM_SERIAL_MANUAL_PORT must point at a serial device");
        let mut config = valid_config();
        config.port_path = port_path.clone();
        config.validate().unwrap();

        let first_ping = b"oxideterm-serial-ping-1\r";
        let first_pong = b"oxideterm-serial-pong-1\r";
        let second_ping = b"oxideterm-serial-ping-2\r";
        let second_pong = b"oxideterm-serial-pong-2\r";
        let first_expected = manual_serial_expected(first_ping, first_pong);
        let second_expected = manual_serial_expected(second_ping, second_pong);

        manual_serial_round_trip(&port_path, first_ping, &first_expected);
        manual_serial_round_trip(&port_path, second_ping, &second_expected);
    }

    fn manual_serial_expected(loopback_payload: &[u8], responder_payload: &[u8]) -> Vec<u8> {
        match std::env::var("OXIDETERM_SERIAL_MANUAL_MODE")
            .unwrap_or_else(|_| "loopback".to_string())
            .as_str()
        {
            "loopback" => loopback_payload.to_vec(),
            "responder" => responder_payload.to_vec(),
            mode => panic!(
                "unsupported OXIDETERM_SERIAL_MANUAL_MODE={mode}; use loopback or responder"
            ),
        }
    }

    fn manual_serial_round_trip(port_path: &str, ping: &[u8], expected: &[u8]) {
        let mut port = serialport::new(port_path, 115_200)
            .data_bits(serialport::DataBits::Eight)
            .stop_bits(serialport::StopBits::One)
            .parity(serialport::Parity::None)
            .flow_control(serialport::FlowControl::None)
            .timeout(std::time::Duration::from_secs(2))
            .open()
            .expect("manual serial port should open at 115200 8N1");

        port.write_all(ping).expect("manual serial write failed");
        port.flush().expect("manual serial flush failed");

        let mut read_buf = vec![0_u8; expected.len()];
        port.read_exact(&mut read_buf)
            .expect("manual serial read failed");
        assert_eq!(read_buf, expected);
        drop(port);
    }
}
