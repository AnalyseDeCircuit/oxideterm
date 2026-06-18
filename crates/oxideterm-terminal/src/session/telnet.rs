const TELNET_DEFAULT_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TELNET_TERMINAL_TYPE: &[u8] = b"xterm-256color";
const TELNET_COMMAND_IAC: u8 = 255;
const TELNET_COMMAND_DONT: u8 = 254;
const TELNET_COMMAND_DO: u8 = 253;
const TELNET_COMMAND_WONT: u8 = 252;
const TELNET_COMMAND_WILL: u8 = 251;
const TELNET_COMMAND_SB: u8 = 250;
const TELNET_COMMAND_SE: u8 = 240;
const TELNET_OPTION_BINARY: u8 = 0;
const TELNET_OPTION_ECHO: u8 = 1;
const TELNET_OPTION_SUPPRESS_GO_AHEAD: u8 = 3;
const TELNET_OPTION_TERMINAL_TYPE: u8 = 24;
const TELNET_OPTION_NAWS: u8 = 31;
const TELNET_TERMINAL_TYPE_IS: u8 = 0;
const TELNET_TERMINAL_TYPE_SEND: u8 = 1;

pub struct TelnetSession {
    config: TelnetSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: Receiver<AlacEvent>,
    worker_rx: Receiver<TelnetWorkerEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    command_tx: tokio::sync::mpsc::Sender<TelnetCommand>,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
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
}

#[derive(Debug)]
enum TelnetCommand {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

#[derive(Debug)]
enum TelnetWorkerEvent {
    Connected,
    Output(Vec<u8>),
    Failed(String),
    Closed,
}

#[derive(Clone, Debug)]
struct TelnetCodec {
    cols: u16,
    rows: u16,
}

impl TelnetCodec {
    fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }

    fn set_window_size(&mut self, cols: u16, rows: u16) {
        self.cols = cols.max(2);
        self.rows = rows.max(2);
    }

    fn filter_server_bytes(&self, bytes: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut data = Vec::with_capacity(bytes.len());
        let mut responses = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != TELNET_COMMAND_IAC {
                data.push(bytes[index]);
                index += 1;
                continue;
            }

            index += 1;
            let Some(command) = bytes.get(index).copied() else {
                break;
            };
            index += 1;
            match command {
                TELNET_COMMAND_IAC => data.push(TELNET_COMMAND_IAC),
                TELNET_COMMAND_DO | TELNET_COMMAND_DONT | TELNET_COMMAND_WILL
                | TELNET_COMMAND_WONT => {
                    let Some(option) = bytes.get(index).copied() else {
                        break;
                    };
                    index += 1;
                    responses.extend(self.negotiation_responses(command, option));
                }
                TELNET_COMMAND_SB => {
                    let start = index;
                    while index + 1 < bytes.len()
                        && !(bytes[index] == TELNET_COMMAND_IAC
                            && bytes[index + 1] == TELNET_COMMAND_SE)
                    {
                        index += 1;
                    }
                    let subnegotiation = &bytes[start..index.min(bytes.len())];
                    responses.extend(self.subnegotiation_responses(subnegotiation));
                    if index + 1 < bytes.len() {
                        index += 2;
                    }
                }
                _ => {}
            }
        }
        (data, responses)
    }

    fn encode_client_data(&self, bytes: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(bytes.len());
        for byte in bytes {
            encoded.push(*byte);
            if *byte == TELNET_COMMAND_IAC {
                encoded.push(TELNET_COMMAND_IAC);
            }
        }
        encoded
    }

    fn naws_message(&self) -> Vec<u8> {
        let mut bytes = vec![
            TELNET_COMMAND_IAC,
            TELNET_COMMAND_SB,
            TELNET_OPTION_NAWS,
            (self.cols >> 8) as u8,
            self.cols as u8,
            (self.rows >> 8) as u8,
            self.rows as u8,
            TELNET_COMMAND_IAC,
            TELNET_COMMAND_SE,
        ];
        // Telnet command bytes inside NAWS payload must be escaped even when
        // the current terminal size happens to contain 255 in a high/low byte.
        let payload_range = 3..7;
        let payload = bytes[payload_range.clone()].to_vec();
        bytes.splice(payload_range, telnet_escape_iac_payload(&payload));
        bytes
    }

    fn negotiation_responses(&self, command: u8, option: u8) -> Vec<Vec<u8>> {
        match command {
            TELNET_COMMAND_DO => {
                if matches!(
                    option,
                    TELNET_OPTION_BINARY
                        | TELNET_OPTION_SUPPRESS_GO_AHEAD
                        | TELNET_OPTION_TERMINAL_TYPE
                        | TELNET_OPTION_NAWS
                ) {
                    let mut responses = vec![vec![
                        TELNET_COMMAND_IAC,
                        TELNET_COMMAND_WILL,
                        option,
                    ]];
                    if option == TELNET_OPTION_NAWS {
                        responses.push(self.naws_message());
                    }
                    responses
                } else {
                    vec![vec![TELNET_COMMAND_IAC, TELNET_COMMAND_WONT, option]]
                }
            }
            TELNET_COMMAND_WILL => {
                if matches!(
                    option,
                    TELNET_OPTION_BINARY | TELNET_OPTION_ECHO | TELNET_OPTION_SUPPRESS_GO_AHEAD
                ) {
                    vec![vec![TELNET_COMMAND_IAC, TELNET_COMMAND_DO, option]]
                } else {
                    vec![vec![TELNET_COMMAND_IAC, TELNET_COMMAND_DONT, option]]
                }
            }
            TELNET_COMMAND_DONT => vec![vec![TELNET_COMMAND_IAC, TELNET_COMMAND_WONT, option]],
            TELNET_COMMAND_WONT => vec![vec![TELNET_COMMAND_IAC, TELNET_COMMAND_DONT, option]],
            _ => Vec::new(),
        }
    }

    fn subnegotiation_responses(&self, bytes: &[u8]) -> Vec<Vec<u8>> {
        if bytes.first().copied() == Some(TELNET_OPTION_TERMINAL_TYPE)
            && bytes.get(1).copied() == Some(TELNET_TERMINAL_TYPE_SEND)
        {
            let mut response = vec![
                TELNET_COMMAND_IAC,
                TELNET_COMMAND_SB,
                TELNET_OPTION_TERMINAL_TYPE,
                TELNET_TERMINAL_TYPE_IS,
            ];
            response.extend_from_slice(TELNET_TERMINAL_TYPE);
            response.extend_from_slice(&[TELNET_COMMAND_IAC, TELNET_COMMAND_SE]);
            return vec![response];
        }
        Vec::new()
    }
}

impl TelnetSession {
    pub fn new(
        config: TelnetSessionConfig,
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
        let term = Arc::new(FairMutex::new(Term::new(
            term_config,
            &size,
            listener,
        )));

        let runtime = Runtime::new().ok();
        if let Some(runtime) = runtime.as_ref() {
            let worker_config = config.clone();
            runtime.spawn(run_telnet_worker(
                worker_config,
                resize,
                command_rx,
                worker_tx,
            ));
        } else {
            let _ = worker_tx.send(TelnetWorkerEvent::Failed(
                "failed to initialize Telnet runtime".to_string(),
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
        }
    }

    fn title_text(&self) -> String {
        format!("Telnet {}", self.config.endpoint_label())
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
                Ok(TelnetWorkerEvent::Connected) => {
                    self.title = Some(self.title_text());
                    self.pending_events
                        .push(TerminalEvent::TitleChanged(self.title_text()));
                    report.events_drained += 1;
                    report.mark_changed();
                }
                Ok(TelnetWorkerEvent::Output(bytes)) => {
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
                Ok(TelnetWorkerEvent::Failed(error)) => {
                    self.lifecycle = TerminalLifecycle::Exited(None);
                    self.feed_utf8_terminal_output(
                        format!("\r\nTelnet connection failed: {error}\r\n").as_bytes(),
                    );
                    self.pending_events.push(TerminalEvent::ChildExited(None));
                    report.events_drained += 1;
                    report.mark_changed();
                    break;
                }
                Ok(TelnetWorkerEvent::Closed) => {
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
        let processed_output = self.process_terminal_output(bytes);
        let bytes = processed_output.as_ref();
        let events = self.modem_consumer.process_server_output(bytes);
        self.handle_modem_consumer_events(events);
    }

    fn feed_plain_transport_output(&mut self, bytes: &[u8]) {
        for kind in self.magic_scan.scan(bytes) {
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
        let events = self.graphics_ingress.advance_with(
            bytes,
            |terminal_bytes| {
                if let Some(hint) = self.encoding_detector.observe(terminal_bytes) {
                    self.pending_events.push(TerminalEvent::EncodingHint(hint));
                }
                let decoded = self.output_decoder.decode_to_utf8_bytes(terminal_bytes);
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
                cursor.set(graphics_cursor_from_term(&term, size));
            },
            || cursor.get(),
        );
        drop(term);
        for event in events {
            if let Some(response) = self.graphics.handle_event(event) {
                let _ = self.write_protocol_bytes(&response);
            }
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

    fn process_terminal_output<'a>(&self, bytes: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        apply_terminal_output_processor(&self.output_processor, bytes)
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
                self.pending_events
                    .push(TerminalEvent::ClipboardStore(text));
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

    fn send_command(&mut self, command: TelnetCommand) -> Result<()> {
        self.command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

impl TerminalSessionBackend for TelnetSession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::Telnet
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
            self.send_command(TelnetCommand::Data(bytes.to_vec()))?;
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
        let _ = self.send_command(TelnetCommand::Resize {
            cols: resize.cols as u16,
            rows: resize.rows as u16,
        });
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
        let _ = self.send_command(TelnetCommand::Close);
        self.runtime = None;
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

async fn run_telnet_worker(
    config: TelnetSessionConfig,
    initial_resize: TerminalResize,
    mut command_rx: tokio::sync::mpsc::Receiver<TelnetCommand>,
    worker_tx: crossbeam_channel::Sender<TelnetWorkerEvent>,
) {
    let endpoint = (config.host.as_str(), config.port);
    let stream = match tokio::time::timeout(
        TELNET_DEFAULT_CONNECT_TIMEOUT,
        TcpStream::connect(endpoint),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            let _ = worker_tx.send(TelnetWorkerEvent::Failed(error.to_string()));
            return;
        }
        Err(_) => {
            let _ = worker_tx.send(TelnetWorkerEvent::Failed(format!(
                "timed out connecting to {}",
                config.endpoint_label()
            )));
            return;
        }
    };

    let _ = worker_tx.send(TelnetWorkerEvent::Connected);
    let (mut reader, mut writer) = stream.into_split();
    let mut codec = TelnetCodec::new(initial_resize.cols as u16, initial_resize.rows as u16);
    let mut buffer = vec![0_u8; 8192];
    loop {
        tokio::select! {
            read_result = reader.read(&mut buffer) => {
                match read_result {
                    Ok(0) => {
                        let _ = worker_tx.send(TelnetWorkerEvent::Closed);
                        break;
                    }
                    Ok(read_count) => {
                        let (data, responses) = codec.filter_server_bytes(&buffer[..read_count]);
                        for response in responses {
                            if writer.write_all(&response).await.is_err() {
                                let _ = worker_tx.send(TelnetWorkerEvent::Closed);
                                return;
                            }
                        }
                        if !data.is_empty()
                            && worker_tx.send(TelnetWorkerEvent::Output(data)).is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = worker_tx.send(TelnetWorkerEvent::Failed(error.to_string()));
                        break;
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(TelnetCommand::Data(bytes)) => {
                        let encoded = codec.encode_client_data(&bytes);
                        if writer.write_all(&encoded).await.is_err() {
                            let _ = worker_tx.send(TelnetWorkerEvent::Closed);
                            break;
                        }
                    }
                    Some(TelnetCommand::Resize { cols, rows }) => {
                        codec.set_window_size(cols, rows);
                        if writer.write_all(&codec.naws_message()).await.is_err() {
                            let _ = worker_tx.send(TelnetWorkerEvent::Closed);
                            break;
                        }
                    }
                    Some(TelnetCommand::Close) | None => {
                        let _ = writer.shutdown().await;
                        let _ = worker_tx.send(TelnetWorkerEvent::Closed);
                        break;
                    }
                }
            }
        }
    }
}

fn telnet_escape_iac_payload(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len());
    for byte in bytes {
        escaped.push(*byte);
        if *byte == TELNET_COMMAND_IAC {
            escaped.push(TELNET_COMMAND_IAC);
        }
    }
    escaped
}

#[cfg(test)]
mod telnet_tests {
    use super::*;

    #[test]
    fn telnet_codec_filters_negotiation_and_answers_supported_options() {
        let codec = TelnetCodec::new(80, 24);
        let (data, responses) = codec.filter_server_bytes(&[
            b'h',
            TELNET_COMMAND_IAC,
            TELNET_COMMAND_DO,
            TELNET_OPTION_NAWS,
            TELNET_COMMAND_IAC,
            TELNET_COMMAND_WILL,
            TELNET_OPTION_ECHO,
            b'i',
        ]);

        assert_eq!(data, b"hi");
        assert!(responses.contains(&vec![
            TELNET_COMMAND_IAC,
            TELNET_COMMAND_WILL,
            TELNET_OPTION_NAWS
        ]));
        assert!(responses.contains(&vec![
            TELNET_COMMAND_IAC,
            TELNET_COMMAND_DO,
            TELNET_OPTION_ECHO
        ]));
        assert!(responses.iter().any(|response| response.starts_with(&[
            TELNET_COMMAND_IAC,
            TELNET_COMMAND_SB,
            TELNET_OPTION_NAWS
        ])));
    }

    #[test]
    fn telnet_codec_escapes_client_iac_bytes() {
        let codec = TelnetCodec::new(80, 24);
        assert_eq!(
            codec.encode_client_data(&[b'a', TELNET_COMMAND_IAC, b'b']),
            vec![b'a', TELNET_COMMAND_IAC, TELNET_COMMAND_IAC, b'b']
        );
    }

    #[test]
    fn terminal_output_processor_transforms_and_suppresses_parser_input() {
        let transform: Option<TerminalOutputProcessor> =
            Some(Arc::new(|bytes| bytes.iter().map(u8::to_ascii_uppercase).collect()));
        assert_eq!(
            apply_terminal_output_processor(&transform, b"prompt").as_ref(),
            b"PROMPT"
        );

        let suppress: Option<TerminalOutputProcessor> = Some(Arc::new(|_| Vec::new()));
        assert!(apply_terminal_output_processor(&suppress, b"hidden").is_empty());
        let raw = apply_terminal_output_processor(&None, b"raw");
        assert!(matches!(raw, std::borrow::Cow::Borrowed(_)));
        assert_eq!(raw.as_ref(), b"raw");
    }
}
