const TELNET_DEFAULT_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TELNET_TERMINAL_TYPE: &[u8] = b"xterm-256color";
const TELNET_COMMAND_IAC: u8 = 255;
const TELNET_COMMAND_DONT: u8 = 254;
const TELNET_COMMAND_DO: u8 = 253;
const TELNET_COMMAND_WONT: u8 = 252;
const TELNET_COMMAND_WILL: u8 = 251;
const TELNET_COMMAND_SB: u8 = 250;
const TELNET_COMMAND_GA: u8 = 249;
const TELNET_COMMAND_EL: u8 = 248;
const TELNET_COMMAND_EC: u8 = 247;
const TELNET_COMMAND_AYT: u8 = 246;
const TELNET_COMMAND_AO: u8 = 245;
const TELNET_COMMAND_IP: u8 = 244;
const TELNET_COMMAND_BRK: u8 = 243;
const TELNET_COMMAND_NOP: u8 = 241;
const TELNET_COMMAND_SE: u8 = 240;
const TELNET_OPTION_BINARY: u8 = 0;
const TELNET_OPTION_ECHO: u8 = 1;
const TELNET_OPTION_SUPPRESS_GO_AHEAD: u8 = 3;
const TELNET_OPTION_TERMINAL_TYPE: u8 = 24;
const TELNET_OPTION_NAWS: u8 = 31;
const TELNET_TERMINAL_TYPE_IS: u8 = 0;
const TELNET_TERMINAL_TYPE_SEND: u8 = 1;
const TELNET_URI_LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const TELNET_URI_PROMPT_TAIL_LIMIT: usize = 256;

pub struct TelnetSession {
    audit: Option<oxideterm_audit::AuditContext>,
    recording_sink: Option<oxideterm_audit::RecordingSink>,
    recording_pending: Option<crate::recording_output::RecordingOutput>,
    connect_audit: Option<oxideterm_audit::AuditOperation>,
    connected: bool,
    config: TelnetSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: LocalEventReceiver,
    worker_rx: crate::backpressure::ByteBoundedReceiver<TelnetWorkerEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    command_tx: tokio::sync::mpsc::Sender<TelnetCommand>,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    palette: TerminalPalette,
    graphics_alt_screen_active: bool,
    output_queue: VecDeque<crate::backpressure::ByteBoundedItem<TelnetWorkerEvent>>,
    magic_scan: MagicScanWindow,
    encoding: TerminalEncoding,
    output_decoder: TerminalOutputDecoder,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    trigger_stream: Option<oxideterm_terminal_triggers::TerminalTriggerStream>,
    input_encoder: TerminalInputEncoder,
    encoding_detector: EncodingMismatchDetector,
    modem_consumer: ModemConsumer,
    shell_integration: TerminalShellIntegration,
}

#[derive(Debug)]
enum TelnetCommand {
    Data(Vec<u8>),
    Control(TelnetControlCommand),
    Resize { cols: u16, rows: u16 },
    Close,
}

#[derive(Debug)]
enum TelnetWorkerEvent {
    Connected,
    Output(Vec<u8>),
    Failed(String),
    Eof,
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
    pub fn new_with_login(
        config: TelnetSessionConfig,
        login: Option<TelnetLoginCredentials>,
        upstream_proxy: Option<oxideterm_network_proxy::tcp::UpstreamProxyConfig>,
        audit: Option<oxideterm_audit::AuditContext>,
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
        let (listener, event_rx) = local_event_channel();
        let recording_activity = listener.activity_sender();
        let (worker_tx, worker_rx) = crate::backpressure::byte_bounded_channel_with_activity(
            crate::backpressure::TRANSPORT_OUTPUT_BACKLOG_BYTES,
            listener.activity_sender(),
        );
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(256);
        let term_config = interactive_terminal_config(scrollback_lines);
        let term = Arc::new(FairMutex::new(Term::new(term_config, &size, listener)));
        let connect_audit = Some(oxideterm_audit::AuditOperation::in_context(
            audit.as_ref(),
            oxideterm_audit::AuditCategory::Connection,
            "telnet_connect",
            None,
        ));
        let recording_sink = audit
            .as_ref()
            .map(oxideterm_audit::AuditContext::recording_sink);
        if let Some(sink) = &recording_sink {
            sink.set_wake_callback(Arc::new(move || recording_activity.notify()));
        }
        let recording_pending = recording_sink.as_ref().map(|sink| {
            let mut pending = crate::recording_output::RecordingOutput::new(sink.clone());
            pending.resize(resize.cols as u16, resize.rows as u16);
            pending
        });

        let runtime = Runtime::new().ok();
        if let Some(runtime) = runtime.as_ref() {
            let worker_config = config.clone();
            runtime.spawn(run_telnet_worker(
                worker_config,
                login,
                upstream_proxy,
                encoding,
                resize,
                command_rx,
                worker_tx,
            ));
        } else {
            let _ = worker_tx.send_control(TelnetWorkerEvent::Failed(
                "failed to initialize Telnet runtime".to_string(),
            ));
        }

        Self {
            audit,
            recording_sink,
            recording_pending,
            connect_audit,
            connected: false,
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
            palette: TerminalPalette::default(),
            graphics_alt_screen_active: false,
            output_queue: VecDeque::new(),
            magic_scan: MagicScanWindow::default(),
            encoding,
            output_decoder: TerminalOutputDecoder::new(encoding),
            output_processor: None,
            output_events_enabled: false,
            trigger_stream: None,
            input_encoder: TerminalInputEncoder::new(encoding),
            encoding_detector: EncodingMismatchDetector::new(encoding),
            modem_consumer: ModemConsumer::new(),
            shell_integration: TerminalShellIntegration::default(),
        }
    }

    fn title_text(&self) -> String {
        format!("Telnet {}", self.config.endpoint_label())
    }

    fn audit_exit(&mut self, outcome: oxideterm_audit::AuditOutcome) {
        if let Some(operation) = self.connect_audit.take() {
            operation.finish(
                outcome,
                oxideterm_audit::AuditEvidence::Lifecycle,
                None,
                None,
            );
        } else if self.connected {
            if let Some(context) = &self.audit {
                context.observe(
                    oxideterm_audit::AuditCategory::Connection,
                    "telnet_disconnect",
                    None,
                    outcome,
                    oxideterm_audit::AuditEvidence::Lifecycle,
                    oxideterm_audit::AuditAuthorization::NotRequired,
                );
            }
            self.connected = false;
        }
    }

    fn drain_worker_events_with_budget(
        &mut self,
        budget: TerminalDrainBudget,
    ) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = TerminalDrainReport::default();
        loop {
            if self
                .recording_pending
                .as_mut()
                .is_some_and(|pending| !pending.flush())
            {
                break;
            }

            if budget.time_exhausted(started)
                || report.drained_bytes >= budget.max_bytes
                || report.events_drained >= budget.max_events
            {
                report.budget_exhausted =
                    !self.output_queue.is_empty() || !self.worker_rx.is_empty();
                break;
            }

            let event = match self
                .output_queue
                .pop_front()
                .map(Ok)
                .unwrap_or_else(|| self.worker_rx.try_recv())
            {
                Ok(event) => event,
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    if self.lifecycle.is_running() {
                        if let Some(sink) = &self.recording_sink {
                            sink.interrupt();
                        }
                        self.audit_exit(oxideterm_audit::AuditOutcome::Interrupted);
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    break;
                }
            };
            if let TelnetWorkerEvent::Output(bytes) = event.value()
                && report.drained_bytes > 0
                && report.drained_bytes.saturating_add(bytes.len()) > budget.max_bytes
            {
                self.output_queue.push_back(event);
                report.budget_exhausted = true;
                break;
            }

            if matches!(event.value(), TelnetWorkerEvent::Eof) {
                let tail = self.modem_consumer.flush_pending_plain_output();
                if !tail.is_empty() {
                    self.handle_modem_consumer_events(tail);
                    report.mark_changed();
                }
                if self
                    .recording_pending
                    .as_mut()
                    .is_some_and(|pending| !pending.flush())
                {
                    self.output_queue.push_front(event);
                    break;
                }
            }
            match event.into_inner() {
                TelnetWorkerEvent::Connected => {
                    self.connected = true;
                    if let Some(operation) = self.connect_audit.take() {
                        operation.finish(
                            oxideterm_audit::AuditOutcome::Succeeded,
                            oxideterm_audit::AuditEvidence::Protocol,
                            None,
                            None,
                        );
                    }
                    self.title = Some(self.title_text());
                    self.pending_events
                        .push(TerminalEvent::TitleChanged(self.title_text()));
                    report.events_drained += 1;
                    report.mark_changed();
                }
                TelnetWorkerEvent::Output(bytes) => {
                    report.events_drained += 1;
                    let processing_started = budget.collect_performance_metrics.then(Instant::now);
                    self.feed_transport_output(&bytes);
                    report.record_data_chunk(
                        bytes.len(),
                        processing_started.map_or(Duration::ZERO, |started| started.elapsed()),
                    );
                    report.mark_changed();
                }
                TelnetWorkerEvent::Failed(error) => {
                    if let Some(sink) = &self.recording_sink {
                        sink.interrupt();
                    }
                    self.audit_exit(oxideterm_audit::AuditOutcome::Failed);
                    self.lifecycle = TerminalLifecycle::Exited(None);
                    self.feed_utf8_terminal_output(
                        format!("\r\nTelnet connection failed: {error}\r\n").as_bytes(),
                    );
                    self.pending_events.push(TerminalEvent::ChildExited(None));
                    report.events_drained += 1;
                    report.mark_changed();
                    break;
                }
                closed @ (TelnetWorkerEvent::Eof | TelnetWorkerEvent::Closed) => {
                    if self.lifecycle.is_running() {
                        let confirmed_eof = matches!(closed, TelnetWorkerEvent::Eof);
                        if let Some(sink) = &self.recording_sink {
                            if confirmed_eof {
                                sink.close();
                            } else {
                                sink.interrupt();
                            }
                        }
                        self.audit_exit(if confirmed_eof {
                            oxideterm_audit::AuditOutcome::Succeeded
                        } else {
                            oxideterm_audit::AuditOutcome::Interrupted
                        });
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    report.events_drained += 1;
                    break;
                }
            }
        }
        report.pending_bytes = self.worker_rx.pending_bytes();
        report.drain_duration = started.elapsed();
        report
    }

    fn feed_transport_output(&mut self, bytes: &[u8]) {
        let events = self.modem_consumer.process_server_output(bytes);
        self.handle_modem_consumer_events(events);
    }

    fn feed_plain_transport_output(&mut self, bytes: &[u8]) {
        // Preserve protocol bytes before optional plugin display transforms.
        let processed_output = self.process_terminal_output(bytes);
        let bytes = processed_output.as_ref();
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
        let mut protocol_responses = Vec::new();
        self.graphics_ingress.advance_ordered(
            bytes,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                    if let Some(hint) = self.encoding_detector.observe(&terminal_bytes) {
                        self.pending_events.push(TerminalEvent::EncodingHint(hint));
                    }
                    let decoded = self.output_decoder.decode_to_utf8_bytes(&terminal_bytes);
                    if let Some(stream) = self.trigger_stream.as_mut() {
                        stream.observe_bytes(decoded.as_ref(), |matched| {
                            self.pending_events
                                .push(TerminalEvent::TriggerMatched(matched));
                        });
                    }
                    let record_output = self.output_events_enabled
                        || self
                            .recording_sink
                            .as_ref()
                            .is_some_and(oxideterm_audit::RecordingSink::is_enabled);
                    if record_output {
                        // Apply the same private-OSC recording boundary as PTY sessions.
                        let (_, recordable) = self.shell_integration.advance_with_recording(
                            &mut self.parser,
                            &mut *term,
                            decoded.as_ref(),
                            |event| self.pending_events.push(event),
                        );
                        if !recordable.is_empty() {
                            if let Some(pending) = &mut self.recording_pending {
                                pending.output(&recordable);
                            }
                            if self.output_events_enabled {
                                self.pending_events.push(TerminalEvent::Output(recordable));
                            }
                        }
                    } else {
                        self.shell_integration.advance(
                            &mut self.parser,
                            &mut *term,
                            decoded.as_ref(),
                            |event| self.pending_events.push(event),
                        );
                    }
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
        let Some(transfer) = self.modem_consumer.active_transfer_input() else {
            return false;
        };
        let mut changed = false;
        while let Some(bytes) = transfer.take_server_write() {
            let byte_len = bytes.len();
            if self.write_protocol_bytes(&bytes).is_ok() {
                transfer.complete_server_write(byte_len);
                changed = true;
            } else {
                transfer.restore_server_write(bytes);
                break;
            }
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

    fn flush_buffered_modem_output(&mut self) -> bool {
        // The session owns the detection tail until it is released or the
        // transport closes, so no detached timer or worker can outlive it.
        let events = if self.lifecycle.is_running() {
            self.modem_consumer.flush_expired_plain_output()
        } else {
            self.modem_consumer.flush_pending_plain_output()
        };
        let changed = !events.is_empty();
        self.handle_modem_consumer_events(events);
        changed
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
            // File consumers are opt-in, so keep this allocation off the normal rendering path.
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
            AlacEvent::ColorSchemeRequest => {
                let _ = self.write_protocol_bytes(self.palette.color_scheme_report().as_bytes());
                false
            }
            AlacEvent::ColorRequest(index, formatter) => {
                let color = crate::color_for_alacritty_request(
                    index,
                    &self.palette,
                    self.term.lock().colors(),
                );
                let _ = self.write_protocol_bytes(formatter(color).as_bytes());
                false
            }
            AlacEvent::TextAreaSizeRequest(_) => false,
            AlacEvent::ChildExit(_) | AlacEvent::Exit => false,
        }
    }

    fn send_command(&mut self, command: TelnetCommand) -> Result<()> {
        self.command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    fn control_command_byte(command: TelnetControlCommand) -> u8 {
        match command {
            TelnetControlCommand::NoOperation => TELNET_COMMAND_NOP,
            TelnetControlCommand::Break => TELNET_COMMAND_BRK,
            TelnetControlCommand::InterruptProcess => TELNET_COMMAND_IP,
            TelnetControlCommand::AbortOutput => TELNET_COMMAND_AO,
            TelnetControlCommand::AreYouThere => TELNET_COMMAND_AYT,
            TelnetControlCommand::EraseCharacter => TELNET_COMMAND_EC,
            TelnetControlCommand::EraseLine => TELNET_COMMAND_EL,
            TelnetControlCommand::GoAhead => TELNET_COMMAND_GA,
        }
    }
}

impl Drop for TelnetSession {
    fn drop(&mut self) {
        // Dropping this backend cancels its private runtime and any pending recording.
        self.shutdown();
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
        if self.flush_buffered_modem_output() {
            report.mark_changed();
        }
        if self.flush_modem_server_writes() {
            report.mark_changed();
        }
        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(event) = self.event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if self.handle_alacritty_event(event) {
                report.mark_changed();
            }
        }
        if (report.events_drained >= budget.max_events || budget.time_exhausted(started))
            && !self.event_rx.is_empty()
        {
            report.budget_exhausted = true;
        }
        report.drain_duration = started.elapsed();
        report
    }

    fn pending_output_flush_delay(&self) -> Option<Duration> {
        self.modem_consumer.pending_plain_output_delay()
    }

    fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.event_rx.activity_receiver()
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

    fn send_telnet_control(&mut self, command: TelnetControlCommand) -> Result<()> {
        if !self.lifecycle.is_running() {
            bail!("Telnet session is not running")
        }
        self.send_command(TelnetCommand::Control(command))
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

    fn set_palette(&mut self, palette: TerminalPalette) {
        if self.palette == palette {
            return;
        }
        self.palette = palette;
        // Unchanged rows still hold colors resolved from the old palette.
        self.term.lock().mark_fully_damaged();
        self.term.lock().notify_palette_changed();
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

    fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.trigger_stream = rules.map(oxideterm_terminal_triggers::TerminalTriggerStream::new);
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
        let trailing_output = self.modem_consumer.finish_transfer();
        self.feed_plain_transport_output(&trailing_output);
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
        let grid_changed = self.resize.cols != resize.cols || self.resize.rows != resize.rows;
        if grid_changed {
            self.shell_integration
                .reset_command_marks_for_grid_reflow(|event| self.pending_events.push(event));
        }
        self.resize = resize;
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        self.term.lock().resize(size);
        if grid_changed {
            if let Some(pending) = &mut self.recording_pending {
                pending.resize(resize.cols as u16, resize.rows as u16);
            }
        }
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

    fn scroll_lines_snapshot_incremental(
        &mut self,
        delta: i32,
        previous: &TerminalSnapshot,
    ) -> TerminalSnapshot {
        let mut term = self.term.lock();
        scroll_snapshot_from_term(
            &mut term,
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
            &self.palette,
            delta,
            previous,
        )
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
        let term = self.term.lock();
        search_matches_from_term(&term, self.resize.cols, query)
    }

    fn search_source(&self) -> Option<crate::TerminalSearchSource> {
        Some(crate::TerminalSearchSource::new(
            self.term.clone(),
            self.resize.cols,
        ))
    }

    fn set_selection(&self, selection: Option<crate::TerminalSelectionRange>) {
        crate::selection::set_term_selection(&mut self.term.lock(), selection);
    }

    fn selection(&self) -> Option<crate::TerminalSelectionRange> {
        crate::selection::term_selection(&self.term.lock())
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
            &self.palette,
        )
    }

    fn snapshot_incremental(&self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        let mut term = self.term.lock();
        incremental_snapshot_from_term(
            &mut term,
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
            &self.palette,
            previous,
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
            &self.palette,
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
        if let Some(sink) = &self.recording_sink {
            sink.interrupt();
        }
        if let Some(operation) = self.connect_audit.take() {
            operation.finish(
                oxideterm_audit::AuditOutcome::Cancelled,
                oxideterm_audit::AuditEvidence::Lifecycle,
                None,
                None,
            );
        }
        if self.connected {
            if let Some(context) = &self.audit {
                context.observe(
                    oxideterm_audit::AuditCategory::Connection,
                    "telnet_disconnect",
                    None,
                    oxideterm_audit::AuditOutcome::CancelRequested,
                    oxideterm_audit::AuditEvidence::Dispatch,
                    oxideterm_audit::AuditAuthorization::NotRequired,
                );
            }
            self.connected = false;
        }
        let _ = self.send_command(TelnetCommand::Close);
        self.runtime = None;
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TelnetLoginStage {
    Username,
    Password,
    Done,
}

struct TelnetLoginAutomation {
    credentials: EncodedTelnetLoginCredentials,
    stage: TelnetLoginStage,
    prompt_tail: Vec<u8>,
}

impl TelnetLoginAutomation {
    fn new(credentials: EncodedTelnetLoginCredentials) -> Self {
        Self {
            credentials,
            stage: TelnetLoginStage::Username,
            prompt_tail: Vec::with_capacity(TELNET_URI_PROMPT_TAIL_LIMIT),
        }
    }

    fn observe(&mut self, bytes: &[u8]) -> Option<zeroize::Zeroizing<Vec<u8>>> {
        for byte in bytes {
            match byte {
                b'\r' | b'\n' => self.prompt_tail.clear(),
                0x08 | 0x7f => {
                    self.prompt_tail.pop();
                }
                byte if byte.is_ascii_graphic() || *byte == b' ' => {
                    if self.prompt_tail.len() == TELNET_URI_PROMPT_TAIL_LIMIT {
                        self.prompt_tail.remove(0);
                    }
                    self.prompt_tail.push(byte.to_ascii_lowercase());
                }
                _ => {}
            }
        }

        let prompt = self.prompt_tail.strip_suffix(b" ").unwrap_or(&self.prompt_tail);
        match self.stage {
            TelnetLoginStage::Username
                if [b"login:".as_slice(), b"username:", b"user:"]
                    .iter()
                    .any(|suffix| prompt.ends_with(suffix)) =>
            {
                self.stage = if self.credentials.password.is_some() {
                    TelnetLoginStage::Password
                } else {
                    TelnetLoginStage::Done
                };
                self.prompt_tail.clear();
                Some(login_line(&self.credentials.username))
            }
            TelnetLoginStage::Password
                if [b"password:".as_slice(), b"passcode:"]
                    .iter()
                    .any(|suffix| prompt.ends_with(suffix)) =>
            {
                self.stage = TelnetLoginStage::Done;
                self.prompt_tail.clear();
                self.credentials
                    .password
                    .as_ref()
                    .map(|password| login_line(password.as_slice()))
            }
            _ => None,
        }
    }

    fn complete(&self) -> bool {
        self.stage == TelnetLoginStage::Done
    }
}

struct EncodedTelnetLoginCredentials {
    username: zeroize::Zeroizing<Vec<u8>>,
    password: Option<zeroize::Zeroizing<Vec<u8>>>,
}

fn encode_telnet_login(
    credentials: TelnetLoginCredentials,
    encoding: TerminalEncoding,
) -> EncodedTelnetLoginCredentials {
    let encoder = TerminalInputEncoder::new(encoding);
    EncodedTelnetLoginCredentials {
        username: zeroize::Zeroizing::new(
            encoder
                .encode_text(credentials.username.as_str())
                .into_owned(),
        ),
        password: credentials.password.map(|password| {
            zeroize::Zeroizing::new(encoder.encode_text(password.as_str()).into_owned())
        }),
    }
}

fn login_line(value: &[u8]) -> zeroize::Zeroizing<Vec<u8>> {
    let mut line = zeroize::Zeroizing::new(Vec::with_capacity(value.len() + 2));
    line.extend_from_slice(value);
    line.extend_from_slice(b"\r\n");
    line
}

async fn run_telnet_worker(
    config: TelnetSessionConfig,
    login: Option<TelnetLoginCredentials>,
    upstream_proxy: Option<oxideterm_network_proxy::tcp::UpstreamProxyConfig>,
    encoding: TerminalEncoding,
    initial_resize: TerminalResize,
    mut command_rx: tokio::sync::mpsc::Receiver<TelnetCommand>,
    worker_tx: crate::backpressure::ByteBoundedSender<TelnetWorkerEvent>,
) {
    let dial = oxideterm_network_proxy::tcp::dial_initial_tcp(
        &config.host,
        config.port,
        TELNET_DEFAULT_CONNECT_TIMEOUT.as_secs(),
        upstream_proxy.as_ref(),
    )
    .await;
    // Only the handshake needs this credential copy; retries belong to the session registry.
    drop(upstream_proxy);
    let stream = match dial {
        Ok(stream) => stream,
        Err(error) => {
            let _ = worker_tx.send_control(TelnetWorkerEvent::Failed(error.to_string()));
            return;
        }
    };

    let _ = worker_tx.send_control(TelnetWorkerEvent::Connected);
    let (mut reader, mut writer) = stream.into_split();
    let mut codec = TelnetCodec::new(initial_resize.cols as u16, initial_resize.rows as u16);
    let mut login = login
        .map(|credentials| encode_telnet_login(credentials, encoding))
        .map(TelnetLoginAutomation::new);
    let login_timeout = tokio::time::sleep(TELNET_URI_LOGIN_TIMEOUT);
    tokio::pin!(login_timeout);
    let mut buffer = zeroize::Zeroizing::new(vec![0_u8; 8192]);
    let mut pending_output: Option<std::pin::Pin<Box<_>>> = None;
    loop {
        tokio::select! {
            sent = async {
                match &mut pending_output {
                    Some(send) => send.await,
                    None => std::future::pending::<std::result::Result<(), TelnetWorkerEvent>>().await,
                }
            } => {
                pending_output = None;
                if sent.is_err() { break; }
            }
            read_result = reader.read(&mut buffer), if pending_output.is_none() => {
                match read_result {
                    Ok(0) => {
                        let _ = worker_tx.send_control(TelnetWorkerEvent::Eof);
                        break;
                    }
                    Ok(read_count) => {
                        let (data, responses) = codec.filter_server_bytes(&buffer[..read_count]);
                        for response in responses {
                            if writer.write_all(&response).await.is_err() {
                                let _ = worker_tx.send_control(TelnetWorkerEvent::Closed);
                                return;
                            }
                        }
                        if let Some(automation) = login.as_mut()
                            && let Some(line) = automation.observe(&data)
                        {
                            let encoded = zeroize::Zeroizing::new(codec.encode_client_data(&line));
                            if writer.write_all(&encoded).await.is_err() {
                                let _ = worker_tx.send_control(TelnetWorkerEvent::Closed);
                                return;
                            }
                            if automation.complete() {
                                login = None;
                            }
                        }
                        if !data.is_empty() {
                            let data_len = data.len();
                            pending_output = Some(Box::pin(worker_tx.send_async(
                                TelnetWorkerEvent::Output(data), data_len,
                            )));
                        }
                    }
                    Err(error) => {
                        let _ = worker_tx.send_control(TelnetWorkerEvent::Failed(error.to_string()));
                        break;
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(TelnetCommand::Data(bytes)) => {
                        let encoded = codec.encode_client_data(&bytes);
                        if writer.write_all(&encoded).await.is_err() {
                            let _ = worker_tx.send_control(TelnetWorkerEvent::Closed);
                            break;
                        }
                    }
                    Some(TelnetCommand::Control(command)) => {
                        // Protocol controls must bypass normal data escaping so
                        // the peer receives one IAC command rather than literal bytes.
                        let bytes = [TELNET_COMMAND_IAC, TelnetSession::control_command_byte(command)];
                        if writer.write_all(&bytes).await.is_err() {
                            let _ = worker_tx.send_control(TelnetWorkerEvent::Closed);
                            break;
                        }
                    }
                    Some(TelnetCommand::Resize { cols, rows }) => {
                        codec.set_window_size(cols, rows);
                        if writer.write_all(&codec.naws_message()).await.is_err() {
                            let _ = worker_tx.send_control(TelnetWorkerEvent::Closed);
                            break;
                        }
                    }
                    Some(TelnetCommand::Close) | None => {
                        let _ = writer.shutdown().await;
                        let _ = worker_tx.send_control(TelnetWorkerEvent::Closed);
                        break;
                    }
                }
            }
            _ = &mut login_timeout, if login.is_some() => {
                // URI credentials are one-shot runtime state and must not survive a stalled login.
                login = None;
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

    struct AuditTestKeys;
    impl oxideterm_audit::AuditKeyProvider for AuditTestKeys {
        fn load(
            &self,
            _: &str,
        ) -> std::result::Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            Ok(zeroize::Zeroizing::new(vec![9; 32]))
        }
        fn create(
            &self,
            id: &str,
        ) -> std::result::Result<zeroize::Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            self.load(id)
        }
    }

    #[test]
    fn telnet_connect_and_close_record_protocol_lifecycle_without_login_secret() {
        use oxideterm_audit::{
            AuditContext, AuditOutcome, AuditPolicy, AuditQuery, AuditService, AuditSource,
            RecordingState, StoredRecordingFrameKind,
        };
        use zeroize::Zeroizing;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            use std::io::Write;
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream.write_all(b"login: ").unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            use std::io::BufRead;
            let mut reply = Vec::new();
            reader.read_until(b'\n', &mut reply).unwrap();
            assert_eq!(reply, b"audit-user\r\n");
            reply.clear();
            stream.write_all(b"Password: ").unwrap();
            reader.read_until(b'\n', &mut reply).unwrap();
            assert_eq!(reply, b"audit-secret\r\n");
        });
        let directory = tempfile::tempdir().unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), AuditTestKeys)
                .unwrap();
        let client = service.client();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(client.set_policy(AuditPolicy {
                enabled: true,
                record_output: true,
                ..Default::default()
            }))
            .unwrap();
        let context = AuditContext::new(client.clone(), AuditSource::User)
            .session("telnet", &format!("127.0.0.1:{port}"));
        let login = TelnetLoginCredentials {
            username: Zeroizing::new("audit-user".to_string()),
            password: Some(Zeroizing::new("audit-secret".to_string())),
        };
        let mut session = TelnetSession::new_with_login(
            TelnetSessionConfig {
                host: "127.0.0.1".to_string(),
                port,
            },
            Some(login),
            None,
            Some(context),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while session.lifecycle.is_running() && Instant::now() < deadline {
            session.read_pending();
            std::thread::sleep(Duration::from_millis(5));
        }
        peer.join().unwrap();
        assert!(matches!(session.lifecycle, TerminalLifecycle::Exited(None)));
        let page = runtime
            .block_on(client.query(AuditQuery {
                limit: 20,
                ..Default::default()
            }))
            .unwrap();
        let operations: Vec<_> = page
            .records
            .iter()
            .filter_map(|record| record.details.operation.as_ref())
            .collect();
        assert!(
            operations
                .iter()
                .any(|operation| operation.action == "telnet_connect"
                    && operation.outcome == AuditOutcome::Succeeded)
        );
        assert!(
            operations
                .iter()
                .any(|operation| operation.action == "telnet_disconnect"
                    && operation.outcome == AuditOutcome::Succeeded)
        );
        assert!(page.records.iter().all(|record| {
            !record.details.title.contains("audit-secret")
                && record
                    .details
                    .detail
                    .as_ref()
                    .is_none_or(|value| !value.contains("audit-secret"))
                && record
                    .details
                    .target
                    .as_ref()
                    .is_none_or(|value| !value.contains("audit-secret"))
        }));
        let deadline = Instant::now() + Duration::from_secs(5);
        let recording_id = loop {
            let list = runtime.block_on(client.list_recordings(None, 10)).unwrap();
            if let Some(recording) = list.recordings.first()
                && recording.state == RecordingState::Finished
            {
                break recording.id.clone();
            }
            assert!(
                Instant::now() < deadline,
                "Telnet EOF should finish recording"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let frames = runtime
            .block_on(client.read_recording_page(recording_id, None, None, 10))
            .unwrap();
        let mut output = Vec::new();
        let mut sizes = Vec::new();
        for frame in frames.chunks.iter().flat_map(|chunk| &chunk.frames) {
            match &frame.kind {
                StoredRecordingFrameKind::Output(bytes) => output.extend_from_slice(bytes),
                StoredRecordingFrameKind::Resize { columns, rows } => sizes.push((*columns, *rows)),
                StoredRecordingFrameKind::Gap { .. } => panic!("unexpected recording gap"),
            }
        }
        assert!(
            output
                .windows(b"Password: ".len())
                .any(|window| window == b"Password: ")
        );
        assert!(
            !output
                .windows(b"audit-secret".len())
                .any(|window| window == b"audit-secret")
        );
        assert!(sizes.contains(&(80, 24)));
    }

    #[test]
    fn telnet_recording_pressure_keeps_controls_live_and_preserves_output() {
        let _pressure_guard = crate::recording_test_support::RECORDING_PRESSURE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        use crate::recording_test_support::{PausedFiles, read_finished};
        use oxideterm_audit::{
            AuditContext, AuditPolicy, AuditService, AuditSource, RecordingState,
            StoredRecordingFrameKind,
        };
        use std::sync::{Mutex, atomic::AtomicBool, mpsc};
        let corpus = Arc::new(
            (0..600_000)
                .map(|i| format!("{i:08}:0123456789abcdef0123456789abcdef\r\n"))
                .collect::<String>(),
        );
        for cancel in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let (ack_tx, ack_rx) = mpsc::channel();
            let source = corpus.clone();
            let peer = std::thread::spawn(move || {
                use std::io::{Read, Write};
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(30)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(30)))
                    .unwrap();
                let mut writer = stream.try_clone().unwrap();
                let producer = std::thread::spawn(move || {
                    let result = writer.write_all(source.as_bytes());
                    let _ = writer.shutdown(std::net::Shutdown::Write);
                    result
                });
                let mut input = [0; 3];
                stream.read_exact(&mut input).unwrap();
                ack_tx.send(input).unwrap();
                let result = producer.join().unwrap();
                if !cancel {
                    result.unwrap();
                }
            });
            let directory = tempfile::tempdir().unwrap();
            let (entered, waiting) = mpsc::channel();
            let (resume, resumed) = mpsc::channel();
            let service = AuditService::with_recording_files(
                directory.path().join("audit.db"),
                AuditTestKeys,
                Arc::new(PausedFiles {
                    first: AtomicBool::new(false),
                    entered,
                    resume: Mutex::new(resumed),
                }),
            )
            .unwrap();
            let client = service.client();
            let runtime = Runtime::new().unwrap();
            runtime
                .block_on(client.set_policy(AuditPolicy {
                    enabled: true,
                    record_output: true,
                    ..Default::default()
                }))
                .unwrap();
            let context = AuditContext::new(client.clone(), AuditSource::User)
                .session("telnet", "pressure-fixture");
            let mut session = TelnetSession::new_with_login(
                TelnetSessionConfig {
                    host: "127.0.0.1".into(),
                    port,
                },
                None,
                None,
                Some(context),
                80,
                24,
                Default::default(),
                TerminalEncoding::Utf8,
                100,
            );
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut paused = false;
            loop {
                session.read_pending();
                paused |= waiting.try_recv().is_ok();
                if paused
                    && session
                        .recording_pending
                        .as_mut()
                        .is_some_and(|pending| !pending.flush())
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Telnet did not encounter recording pressure"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                session
                    .read_pending_with_budget(TerminalDrainBudget::normal())
                    .drained_bytes,
                0
            );
            session.write_input(b"\x03").unwrap();
            session
                .send_telnet_control(TelnetControlCommand::InterruptProcess)
                .unwrap();
            assert_eq!(
                ack_rx
                    .recv_timeout(Duration::from_secs(3))
                    .expect("recording pause blocked Telnet controls"),
                [3, 255, 244]
            );
            if cancel {
                session.shutdown();
            }
            resume.send(()).unwrap();
            if !cancel {
                let deadline = Instant::now() + Duration::from_secs(30);
                while session.lifecycle.is_running() {
                    session.read_pending();
                    assert!(
                        Instant::now() < deadline,
                        "Telnet EOF remained blocked after resume"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            peer.join().unwrap();
            let (state, frames) = read_finished(&runtime, &client);
            assert_eq!(
                state,
                if cancel {
                    RecordingState::Interrupted
                } else {
                    RecordingState::Finished
                }
            );
            let mut output = Vec::new();
            for frame in frames {
                match frame {
                    StoredRecordingFrameKind::Output(bytes) => output.extend_from_slice(&bytes),
                    StoredRecordingFrameKind::Gap { .. } => {
                        panic!("retryable Telnet recording lost output")
                    }
                    _ => {}
                }
            }
            if cancel {
                assert!(output.len() > 0 && output.len() < corpus.len());
                assert_eq!(output, corpus.as_bytes()[..output.len()]);
            } else {
                assert_eq!(output, corpus.as_bytes());
            }
        }
    }

    #[test]
    fn telnet_refused_connection_records_failure() {
        use oxideterm_audit::{AuditContext, AuditOutcome, AuditQuery, AuditService, AuditSource};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &AuditTestKeys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), AuditTestKeys)
                .unwrap();
        let context = AuditContext::new(service.client(), AuditSource::User)
            .session("telnet", &format!("127.0.0.1:{port}"));
        let mut session = TelnetSession::new_with_login(
            TelnetSessionConfig {
                host: "127.0.0.1".to_string(),
                port,
            },
            None,
            None,
            Some(context),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while session.lifecycle.is_running() && Instant::now() < deadline {
            session.read_pending();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(session.lifecycle, TerminalLifecycle::Exited(None)));
        let page = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(service.client().query(AuditQuery {
                limit: 20,
                ..Default::default()
            }))
            .unwrap();
        assert!(
            page.records
                .iter()
                .filter_map(|record| record.details.operation.as_ref())
                .any(|operation| operation.action == "telnet_connect"
                    && operation.outcome == AuditOutcome::Failed)
        );
    }

    #[test]
    fn telnet_drop_records_close_request_without_claiming_remote_exit() {
        use oxideterm_audit::{AuditContext, AuditOutcome, AuditQuery, AuditService, AuditSource};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let peer = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            release_rx.recv().unwrap();
            drop(stream);
        });
        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &AuditTestKeys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), AuditTestKeys)
                .unwrap();
        let context = AuditContext::new(service.client(), AuditSource::User)
            .session("telnet", &format!("127.0.0.1:{port}"));
        let mut session = TelnetSession::new_with_login(
            TelnetSessionConfig {
                host: "127.0.0.1".to_string(),
                port,
            },
            None,
            None,
            Some(context),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            100,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !session.connected && Instant::now() < deadline {
            session.read_pending();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(session.connected);
        drop(session);
        let page = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(service.client().query(AuditQuery {
                limit: 20,
                ..Default::default()
            }))
            .unwrap();
        assert!(
            page.records
                .iter()
                .filter_map(|record| record.details.operation.as_ref())
                .any(|operation| operation.action == "telnet_disconnect"
                    && operation.outcome == AuditOutcome::CancelRequested)
        );
        assert!(
            !page
                .records
                .iter()
                .filter_map(|record| record.details.operation.as_ref())
                .any(|operation| operation.action == "telnet_disconnect"
                    && operation.outcome == AuditOutcome::Succeeded)
        );
        release_tx.send(()).unwrap();
        peer.join().unwrap();
    }

    fn uri_login() -> TelnetLoginAutomation {
        TelnetLoginAutomation::new(encode_telnet_login(
            TelnetLoginCredentials {
                username: zeroize::Zeroizing::new("uri-user".to_string()),
                password: Some(zeroize::Zeroizing::new("uri-password".to_string())),
            },
            TerminalEncoding::Utf8,
        ))
    }

    #[test]
    fn uri_login_credentials_follow_split_prompts_once() {
        let mut login = uri_login();
        assert!(login.observe(b"device lo").is_none());
        assert_eq!(login.observe(b"gin:").unwrap().as_slice(), b"uri-user\r\n");
        assert!(login.observe(b"Pass").is_none());
        assert_eq!(
            login.observe(b"word:").unwrap().as_slice(),
            b"uri-password\r\n"
        );
        assert!(login.complete());
        assert!(login.observe(b"Password:").is_none());
    }

    #[test]
    fn telnet_control_commands_map_to_protocol_bytes() {
        assert_eq!(
            TelnetSession::control_command_byte(TelnetControlCommand::InterruptProcess),
            TELNET_COMMAND_IP
        );
        assert_eq!(
            TelnetSession::control_command_byte(TelnetControlCommand::AreYouThere),
            TELNET_COMMAND_AYT
        );
        assert_eq!(
            TelnetSession::control_command_byte(TelnetControlCommand::Break),
            TELNET_COMMAND_BRK
        );
    }

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
