use super::*;

// This state sequences in-band protocols and display parsing. It holds only
// the terminal channel writer; the session retains the SSH consumer lease.
pub(super) struct SshParser {
    default_title: String,
    activity: crate::activity::TerminalActivitySender,
    pub(super) pending_writes: VecDeque<PendingWrite>,
    endpoint: String,
    // The session updates this on EOF/close; parsing cannot revive a retired transport.
    pub(super) transport_running: bool,
    pub(super) command_tx: Option<tokio::sync::mpsc::Sender<SshTransportCommand>>,
    pub(super) term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    pub(super) event_rx: LocalEventReceiver,
    pub(super) pending_events: Vec<TerminalEvent>,
    pub(super) resize: TerminalResize,
    pub(super) title: Option<String>,
    graphics_ingress: GraphicsIngress,
    pub(super) graphics: TerminalGraphicsState,
    graphics_alt_screen_active: bool,
    magic_scan: MagicScanWindow,
    encoding: TerminalEncoding,
    utf8_guard: crate::backpressure::Utf8ResidualGuard,
    output_decoder: TerminalOutputDecoder,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    trigger_stream: Option<oxideterm_terminal_triggers::TerminalTriggerStream>,
    privilege_prompt: TerminalPrivilegePromptStream,
    input_encoder: TerminalInputEncoder,
    encoding_detector: EncodingMismatchDetector,
    trzsz_consumer: Option<TrzszConsumer>,
    pub(super) modem_consumer: ModemConsumer,
    shell_integration: TerminalShellIntegration,
    pub(super) tmux_display: Arc<crate::tmux::TmuxDisplay>,
    tmux_controller: Option<crate::tmux::TmuxController>,
    tmux_command_queue: VecDeque<Vec<u8>>,
}

pub(super) struct PendingWrite(SshTransportCommand);

impl PendingWrite {
    fn take(mut self) -> SshTransportCommand {
        std::mem::replace(&mut self.0, SshTransportCommand::Close)
    }
}

impl Drop for PendingWrite {
    fn drop(&mut self) {
        if let SshTransportCommand::Data(bytes) = &mut self.0 {
            zeroize::Zeroize::zeroize(bytes);
        }
    }
}

impl SshParser {
    pub(super) fn new(
        config: &SshSessionConfig,
        resize: TerminalResize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> (Self, crate::activity::TerminalActivitySender) {
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        let (listener, event_rx) = local_event_channel();
        let activity = listener.activity_sender();
        let tmux_display = Arc::new(crate::tmux::TmuxDisplay::default());

        let term_config = interactive_terminal_config(scrollback_lines);
        let term = Arc::new(FairMutex::new(Term::new(
            term_config,
            &size,
            listener.clone(),
        )));

        let wake_activity = activity.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || wake_activity.notify());
        let trzsz_consumer = config.trzsz_policy().map(|policy| {
            let mut consumer = TrzszConsumer::new(policy);
            consumer.set_wake_callback(wake.clone());
            consumer
        });
        let tmux_graphics_options = graphics_options.clone();
        (
            Self {
                activity: activity.clone(),
                pending_writes: VecDeque::new(),
                default_title: format!("{}@{}", config.username(), config.host()),
                endpoint: format!("{}@{}:{}", config.username(), config.host(), config.port()),
                command_tx: None,
                transport_running: true,
                term,
                parser: Processor::new(),
                event_rx,
                pending_events: Vec::new(),
                resize,
                title: None,
                graphics_ingress: GraphicsIngress::new(graphics_options),
                graphics: TerminalGraphicsState::default(),
                graphics_alt_screen_active: false,
                magic_scan: MagicScanWindow::default(),
                encoding,
                utf8_guard: Default::default(),
                output_decoder: TerminalOutputDecoder::new(encoding),
                output_processor: None,
                output_events_enabled: false,
                trigger_stream: None,
                privilege_prompt: TerminalPrivilegePromptStream::default(),
                input_encoder: TerminalInputEncoder::new(encoding),
                encoding_detector: EncodingMismatchDetector::new(encoding),
                trzsz_consumer,
                modem_consumer: ModemConsumer::with_wake(wake),
                shell_integration: TerminalShellIntegration::default(),
                tmux_display: tmux_display.clone(),
                tmux_controller: Some(crate::tmux::TmuxController::new(
                    tmux_display,
                    listener,
                    size,
                    encoding,
                    scrollback_lines,
                    tmux_graphics_options,
                )),
                tmux_command_queue: VecDeque::new(),
            },
            activity,
        )
    }

    pub(super) fn retire_transport(&mut self) {
        self.command_tx = None;
        self.transport_running = false;
        self.pending_writes.clear();
        for mut command in self.tmux_command_queue.drain(..) {
            zeroize::Zeroize::zeroize(&mut command);
        }
        if let Some(consumer) = &mut self.trzsz_consumer {
            consumer.close();
        }
    }

    pub(super) fn send_command(&mut self, command: SshTransportCommand) -> Result<()> {
        if self.command_tx.is_none() {
            bail!("SSH PTY backend for {} is still connecting", self.endpoint);
        }
        self.pending_writes.push_back(PendingWrite(command));
        self.flush_pending_writes()
    }

    pub(super) fn flush_pending_writes(&mut self) -> Result<()> {
        let Some(sender) = &self.command_tx else {
            return Ok(());
        };
        while let Some(command) = self.pending_writes.pop_front() {
            match sender.try_send(command.take()) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(command)) => {
                    self.pending_writes.push_front(PendingWrite(command));
                    break;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(command)) => {
                    drop(PendingWrite(command));
                    self.pending_writes.clear();
                    bail!("SSH terminal channel closed");
                }
            }
        }
        Ok(())
    }

    pub(super) fn feed_transport_output(&mut self, bytes: &[u8]) {
        if self.trzsz_consumer.is_some() {
            self.feed_trzsz_transport_output(bytes);
            return;
        }
        self.feed_transport_output_to_terminal(bytes);
    }

    fn process_terminal_output<'a>(&self, bytes: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        apply_terminal_output_processor(&self.output_processor, bytes)
    }

    fn push_output_event(&mut self, bytes: &[u8]) {
        if self.output_events_enabled && !bytes.is_empty() {
            // File consumers are opt-in, so keep this allocation off the normal rendering path.
            self.pending_events
                .push(TerminalEvent::Output(bytes.to_vec()));
        }
    }

    fn feed_transport_output_to_terminal(&mut self, bytes: &[u8]) {
        let events = self.modem_consumer.process_server_output(bytes);
        self.handle_modem_consumer_events(events);
    }

    pub(super) fn feed_plain_transport_output_to_terminal(&mut self, bytes: &[u8]) {
        if self.encoding.is_utf8() {
            if let Some(bytes) = self.utf8_guard.push(bytes) {
                self.advance_plain_transport_output(&bytes);
            }
        } else {
            self.advance_plain_transport_output(bytes);
        }
    }

    fn flush_text_tail(&mut self) -> bool {
        if let Some(bytes) = self.utf8_guard.flush() {
            self.advance_plain_transport_output(&bytes);
            true
        } else {
            false
        }
    }

    fn advance_plain_transport_output(&mut self, bytes: &[u8]) {
        let mut controller = self
            .tmux_controller
            .take()
            .expect("SSH tmux controller must remain owned by its terminal session");
        let record_output = self.output_events_enabled;
        let mut tmux_events = Vec::new();
        let result = controller.advance(
            bytes,
            |terminal_bytes| self.feed_normal_transport_output_to_terminal(terminal_bytes),
            record_output,
            |event| tmux_events.push(event),
        );
        self.tmux_controller = Some(controller);
        self.pending_events.extend(tmux_events);
        match result {
            Ok(outcome) => {
                if outcome.entered {
                    self.graphics.clear();
                    self.graphics_alt_screen_active = false;
                }
                self.queue_tmux_commands(outcome.commands);
                if outcome.changed {
                    self.pending_events.push(TerminalEvent::Wakeup);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "tmux control stream was rejected");
                self.queue_tmux_commands(vec![b"\n".to_vec()]);
            }
        }
    }

    fn feed_normal_transport_output_to_terminal(&mut self, bytes: &[u8]) {
        // In-band protocols own raw transport bytes. Plugin output transforms
        // are applied only after modem/trzsz consumers release display data.
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
                    for event in self.privilege_prompt.observe(decoded.as_ref()) {
                        self.pending_events
                            .push(TerminalEvent::PrivilegePrompt(event));
                    }
                    if self.output_events_enabled {
                        // The scanner removes private OSC before persistence;
                        // decoded clipboard payloads must never reach a recording.
                        let (_, recordable) = self.shell_integration.advance_with_recording(
                            &mut self.parser,
                            &mut *term,
                            decoded.as_ref(),
                            |event| self.pending_events.push(event),
                        );
                        if !recordable.is_empty() {
                            self.pending_events.push(TerminalEvent::Output(recordable));
                        }
                    } else {
                        self.shell_integration.advance(
                            &mut self.parser,
                            &mut *term,
                            decoded.as_ref(),
                            |event| self.pending_events.push(event),
                        );
                    }
                    self.graphics.clear_for_alt_screen_transition(
                        &term,
                        &mut self.graphics_alt_screen_active,
                    );
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

    fn feed_trzsz_transport_output(&mut self, bytes: &[u8]) {
        let mut events = Vec::new();
        if let Some(consumer) = self.trzsz_consumer.as_mut() {
            events.extend(consumer.process_server_output(bytes));
            events.extend(consumer.drain_detected_handshakes());
        }
        self.handle_trzsz_consumer_events(events);
    }

    fn handle_trzsz_consumer_events(&mut self, events: Vec<TrzszConsumerEvent>) {
        for event in events {
            match event {
                TrzszConsumerEvent::WriteTerminal(bytes) => {
                    self.feed_transport_output_to_terminal(&bytes);
                }
                TrzszConsumerEvent::SendServer(bytes) => {
                    let _ = self.send_command(SshTransportCommand::Data(bytes));
                }
                TrzszConsumerEvent::TransferStarted(handshake) => {
                    // Tauri creates the transfer owner at magic-key detection time
                    // before showing file dialogs. Keep the same lock boundary:
                    // all later PTY output is routed into the pending transfer
                    // buffer until GPUI confirms/cancels the prompt.
                    self.pending_events
                        .push(TerminalEvent::TrzszTransferPrompt {
                            direction: handshake.direction,
                            selection: handshake.selection,
                            remote_is_windows: handshake.remote_is_windows,
                        });
                }
                TrzszConsumerEvent::TransferDataQueued => {}
                TrzszConsumerEvent::TransferCancelRequested => {}
                TrzszConsumerEvent::UploadTimedOut { .. } => {}
            }
        }
    }

    fn route_trzsz_text_input(&mut self, text: &str) -> bool {
        let Some(consumer) = self.trzsz_consumer.as_mut() else {
            return false;
        };
        let events = consumer.process_terminal_input(text);
        self.handle_trzsz_consumer_events(events);
        true
    }

    pub(super) fn transfer_input_full(&self) -> bool {
        self.trzsz_consumer.as_ref().is_some_and(|consumer| {
            consumer.buffered_input_bytes() >= oxideterm_trzsz::MAX_TRANSFER_CHUNK_SIZE * 2
        })
    }

    pub(super) fn flush_trzsz_server_writes(&mut self) -> bool {
        let Some(consumer) = self.trzsz_consumer.as_mut() else {
            return false;
        };
        let mut changed = false;
        for bytes in consumer.take_server_writes() {
            let _ = self.send_command(SshTransportCommand::Data(bytes));
            changed = true;
        }
        changed
    }

    pub(super) fn flush_modem_server_writes(&mut self) -> bool {
        let Some(transfer) = self.modem_consumer.active_transfer_input() else {
            return false;
        };
        let mut changed = false;
        while self.pending_writes.is_empty() {
            let Some(bytes) = transfer.take_server_write() else {
                break;
            };
            let byte_len = bytes.len();
            if self
                .send_command(SshTransportCommand::Data(bytes.clone()))
                .is_ok()
            {
                transfer.complete_server_write(byte_len);
                changed = true;
            } else {
                // A full bounded SSH command channel is transient; retain the
                // frame and retry on the next terminal drain instead of dropping it.
                transfer.restore_server_write(bytes);
                break;
            }
        }
        changed
    }

    fn handle_modem_consumer_events(&mut self, events: Vec<ModemConsumerEvent>) {
        for event in events {
            match event {
                ModemConsumerEvent::WriteTerminal(bytes) => {
                    self.feed_plain_transport_output_to_terminal(&bytes);
                }
                ModemConsumerEvent::SendServer(bytes) => {
                    let _ = self.send_command(SshTransportCommand::Data(bytes));
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

    pub(super) fn flush_buffered_modem_output(&mut self, running: bool) -> bool {
        // The session releases an incomplete prefix on its maintenance tick,
        // or immediately when the transport has ended and no continuation can arrive.
        let events = if running {
            self.modem_consumer.flush_expired_plain_output()
        } else {
            self.modem_consumer.flush_pending_plain_output()
        };
        let changed = !events.is_empty();
        self.handle_modem_consumer_events(events);
        if running {
            changed
        } else {
            self.flush_text_tail() || changed
        }
    }

    pub(super) fn feed_utf8_terminal_output(&mut self, bytes: &[u8]) {
        self.push_output_event(bytes);
        let mut term = self.term.lock();
        self.shell_integration
            .advance(&mut self.parser, &mut *term, bytes, |event| {
                self.pending_events.push(event);
            });
    }

    pub(super) fn handle_alacritty_event(&mut self, event: AlacEvent) -> bool {
        match event {
            AlacEvent::Title(title) => {
                self.title = Some(title.clone());
                self.pending_events.push(TerminalEvent::TitleChanged(title));
                false
            }
            AlacEvent::ResetTitle => {
                self.title = Some(self.default_title.clone());
                self.pending_events
                    .push(TerminalEvent::TitleChanged(self.default_title.clone()));
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
                let blinking = self.display_term().lock().cursor_style().blinking;
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

    pub(super) fn queue_tmux_commands(&mut self, commands: impl IntoIterator<Item = Vec<u8>>) {
        self.tmux_command_queue.extend(commands);
        self.flush_tmux_commands();
    }

    pub(super) fn flush_tmux_commands(&mut self) -> bool {
        if self.command_tx.is_none() {
            return false;
        }
        let mut changed = false;
        while self.pending_writes.is_empty() {
            let Some(command) = self.tmux_command_queue.pop_front() else {
                break;
            };
            let _ = self.send_command(SshTransportCommand::Data(command));
            changed = true;
        }
        changed
    }

    pub(super) fn display_term(&self) -> Arc<FairMutex<Term<LocalEventListener>>> {
        self.tmux_display
            .term()
            .unwrap_or_else(|| self.term.clone())
    }

    fn write_transport_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.transport_running && !bytes.is_empty() {
            self.send_command(SshTransportCommand::Data(bytes.to_vec()))?;
        }
        Ok(())
    }

    pub(super) fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(commands) = self.tmux_display.input_commands(bytes) {
            self.queue_tmux_commands(commands);
            return Ok(());
        }
        self.write_transport_bytes(bytes)
    }

    pub(super) fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(commands) = self.tmux_display.protocol_commands(bytes) {
            self.queue_tmux_commands(commands);
            return Ok(());
        }
        self.write_transport_bytes(bytes)
    }

    pub(super) fn write_text(&mut self, text: &str) -> Result<()> {
        if !self.tmux_display.is_active() && self.route_trzsz_text_input(text) {
            return Ok(());
        }
        let encoded = self.input_encoder.encode_text(text);
        self.write_input(encoded.as_ref())
    }

    pub(super) fn paste_text(&mut self, text: &str) -> Result<()> {
        let bytes = self
            .input_encoder
            .encode_paste(text, self.mode().contains(TermMode::BRACKETED_PASTE));
        if let Some(commands) = self.tmux_display.paste_commands(&bytes) {
            self.queue_tmux_commands(commands);
            return Ok(());
        }
        self.write_input(&bytes)
    }

    pub(super) fn set_encoding(&mut self, encoding: TerminalEncoding) {
        if self.encoding == encoding {
            return;
        }
        self.flush_text_tail();
        self.encoding = encoding;
        self.output_decoder.set_encoding(encoding);
        self.output_decoder.reset();
        self.privilege_prompt = TerminalPrivilegePromptStream::default();
        self.input_encoder.set_encoding(encoding);
        self.encoding_detector.set_encoding(encoding);
        if let Some(controller) = self.tmux_controller.as_mut() {
            controller.set_encoding(encoding);
        }
    }

    pub(super) fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.output_processor = processor;
        self.output_decoder.reset();
        self.privilege_prompt = TerminalPrivilegePromptStream::default();
        self.encoding_detector.set_encoding(self.encoding);
    }

    pub(super) fn set_output_events_enabled(&mut self, enabled: bool) {
        self.output_events_enabled = enabled;
    }

    pub(super) fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.trigger_stream = rules.map(oxideterm_terminal_triggers::TerminalTriggerStream::new);
    }

    pub(super) fn set_trzsz_policy(&mut self, policy: Option<TrzszTransferPolicy>) {
        // Tauri's terminal controller applies in-band transfer settings to an
        // existing terminal controller, not only to future panes. Native keeps
        // the same user-visible contract by replacing the idle consumer when
        // settings change; active transfers are left owned by the current
        // consumer so a settings toggle cannot orphan an in-flight protocol.
        match (&mut self.trzsz_consumer, policy) {
            (Some(consumer), Some(policy)) => consumer.update_transfer_policy(policy),
            (Some(consumer), None) if consumer.is_transferring() => {}
            (_, policy) => {
                let activity = self.activity.clone();
                self.trzsz_consumer = policy.map(|policy| {
                    let mut consumer = TrzszConsumer::new(policy);
                    consumer.set_wake_callback(Arc::new(move || activity.notify()));
                    consumer
                });
            }
        }
    }

    pub(super) fn take_trzsz_transfer(&mut self) -> Option<TrzszTransfer> {
        self.trzsz_consumer
            .as_mut()
            .and_then(TrzszConsumer::take_active_transfer)
    }

    pub(super) fn feed_trzsz_terminal_output(&mut self, bytes: &[u8]) {
        self.feed_transport_output_to_terminal(bytes);
    }

    pub(super) fn interrupt_trzsz_transfer(&mut self) {
        if let Some(consumer) = self.trzsz_consumer.as_mut() {
            consumer.interrupt_transfer();
        }
    }

    pub(super) fn finish_trzsz_transfer(&mut self) {
        if let Some(consumer) = self.trzsz_consumer.as_mut() {
            consumer.finish_transfer();
        }
    }

    pub(super) fn start_modem_transfer(
        &mut self,
        request: TerminalModemTransferRequest,
    ) -> Option<ModemTransfer> {
        self.modem_consumer.start_manual_transfer(request)
    }

    pub(super) fn interrupt_modem_transfer(&mut self) {
        self.modem_consumer.interrupt_transfer();
    }

    pub(super) fn finish_modem_transfer(&mut self) {
        let trailing_output = self.modem_consumer.finish_transfer();
        self.feed_plain_transport_output_to_terminal(&trailing_output);
    }

    pub(super) fn mode(&self) -> TermMode {
        let term = self.display_term();
        *term.lock().mode()
    }

    pub(super) fn select_tmux_pane_at(&mut self, col: usize, row: usize) -> Result<bool> {
        let Some(command) = self.tmux_display.select_pane_command(col, row) else {
            return Ok(false);
        };
        self.queue_tmux_commands([command]);
        Ok(true)
    }

    pub(super) fn tmux_local_point(&self, col: usize, row: usize) -> (usize, usize) {
        self.tmux_display.local_point(col, row)
    }

    pub(super) fn tmux_state(&self) -> Option<crate::TmuxUiState> {
        self.tmux_display.ui_state()
    }

    pub(super) fn tmux_action(&mut self, action: crate::TmuxAction) -> Result<bool> {
        self.tmux_action_ref(&zeroize::Zeroizing::new(action))
    }

    pub(super) fn tmux_action_ref(&mut self, action: &crate::TmuxAction) -> Result<bool> {
        let Some(command) = self.tmux_display.action_command(action) else {
            return Ok(false);
        };
        self.queue_tmux_commands([command]);
        Ok(true)
    }

    pub(super) fn tmux_separator_at(&self, col: usize, row: usize) -> Option<crate::TmuxSeparator> {
        self.tmux_display.separator_at(col, row)
    }

    pub(super) fn resize_tmux_separator(
        &mut self,
        separator: crate::TmuxSeparator,
        delta: i32,
    ) -> Result<bool> {
        let Some(command) = self.tmux_display.resize_separator_command(separator, delta) else {
            return Ok(false);
        };
        self.queue_tmux_commands([command]);
        Ok(true)
    }

    pub(super) fn set_focused(&mut self, focused: bool) -> Result<()> {
        let should_report = {
            let term = self.display_term();
            let mut term = term.lock();
            term.is_focused = focused;
            term.mode().contains(TermMode::FOCUS_IN_OUT)
        };

        if let Some(report) = focus_report_sequence(should_report, focused) {
            self.write_protocol_bytes(report)?;
        }

        Ok(())
    }

    pub(super) fn clear_buffer(&mut self) {
        let term = self.display_term();
        let mut term = term.lock();
        clear_terminal_buffer(&mut term);
        self.graphics.clear();
    }

    pub(super) fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
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
        if let Some(controller) = self.tmux_controller.as_mut() {
            controller.resize(size);
        }
        let _ = self.send_command(SshTransportCommand::Resize {
            cols: resize.cols as u16,
            rows: resize.rows as u16,
        });
        if let Some(command) = self.tmux_display.resize_command(resize.cols, resize.rows) {
            self.queue_tmux_commands([command]);
        }
        Ok(())
    }
}
