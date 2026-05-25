pub struct SshPtySession {
    config: SshSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: Receiver<AlacEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    connect_rx: Receiver<Result<SshPtyHandle, String>>,
    handle: Option<SshPtyHandle>,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    output_queue: VecDeque<Vec<u8>>,
    output_queue_bytes: usize,
    magic_scan: MagicScanWindow,
    encoding: TerminalEncoding,
    output_decoder: TerminalOutputDecoder,
    output_processor: Option<TerminalOutputProcessor>,
    input_encoder: TerminalInputEncoder,
    encoding_detector: EncodingMismatchDetector,
    trzsz_consumer: Option<TrzszConsumer>,
    shell_integration: TerminalShellIntegration,
}

impl SshPtySession {
    pub fn new(
        config: SshSessionConfig,
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
        let listener = LocalEventListener { tx: event_tx };

        let mut term_config = Config::default();
        term_config.scrolling_history = scrollback_lines;
        term_config.kitty_keyboard = true;
        let term = Arc::new(FairMutex::new(Term::new(
            term_config,
            &size,
            listener.clone(),
        )));

        let runtime = Runtime::new().ok();
        let (connect_tx, connect_rx) = unbounded();
        if let Some(runtime) = runtime.as_ref() {
            let mut ssh_config = config.config.clone();
            if config.defer_pty_until_resize() {
                ssh_config.cols = 0;
                ssh_config.rows = 0;
            } else {
                ssh_config.cols = resize.cols as u32;
                ssh_config.rows = resize.rows as u32;
            }
            let registry = config.registry.clone();
            let consumer = config.consumer.clone();
            let prompt_handler = config.prompt_handler.clone();
            runtime.spawn(async move {
                let mut client = SshTransportClient::new(ssh_config);
                if let Some(prompt_handler) = prompt_handler {
                    client = client.with_prompt_handler(prompt_handler);
                }
                let result = match (registry, consumer) {
                    (Some(registry), Some(consumer)) => {
                        client.connect_shell_with_registry(registry, consumer).await
                    }
                    _ => client.connect_shell().await,
                }
                .map_err(|error| error.to_string());
                let _ = connect_tx.send(result);
            });
        } else {
            let _ = connect_tx.send(Err("failed to initialize SSH runtime".to_string()));
        }

        let trzsz_consumer = config.trzsz_policy().map(TrzszConsumer::new);

        Self {
            config,
            term,
            parser: Processor::new(),
            event_rx,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            runtime,
            connect_rx,
            handle: None,
            title: None,
            graphics_ingress: GraphicsIngress::new(graphics_options),
            graphics: TerminalGraphicsState::default(),
            output_queue: VecDeque::new(),
            output_queue_bytes: 0,
            magic_scan: MagicScanWindow::default(),
            encoding,
            output_decoder: TerminalOutputDecoder::new(encoding),
            output_processor: None,
            input_encoder: TerminalInputEncoder::new(encoding),
            encoding_detector: EncodingMismatchDetector::new(encoding),
            trzsz_consumer,
            shell_integration: TerminalShellIntegration::default(),
        }
    }

    fn title_text(&self) -> String {
        format!("{}@{}", self.config.username(), self.config.host())
    }

    fn process_connect_result(&mut self) -> bool {
        let Ok(result) = self.connect_rx.try_recv() else {
            return false;
        };

        match result {
            Ok(handle) => {
                self.handle = Some(handle);
                let _ = self.send_command(SshTransportCommand::Resize {
                    cols: self.resize.cols as u16,
                    rows: self.resize.rows as u16,
                });
                if let Some(command) = self.config.post_connect_command() {
                    let mut payload = command.as_bytes().to_vec();
                    payload.push(b'\r');
                    let _ = self.send_command(SshTransportCommand::Data(payload));
                }
                self.title = Some(self.title_text());
                self.pending_events
                    .push(TerminalEvent::TitleChanged(self.title_text()));
                true
            }
            Err(error) => {
                self.lifecycle = TerminalLifecycle::Exited(None);
                self.feed_utf8_terminal_output(
                    format!("\r\nSSH connection failed: {error}\r\n").as_bytes(),
                );
                self.pending_events.push(TerminalEvent::ChildExited(None));
                true
            }
        }
    }

    fn feed_transport_output(&mut self, bytes: &[u8]) {
        let processed_output = self.process_terminal_output(bytes);
        let bytes = processed_output.as_slice();
        if self.trzsz_consumer.is_some() {
            self.feed_trzsz_transport_output(bytes);
            return;
        }
        self.feed_transport_output_to_terminal(bytes);
    }

    fn process_terminal_output(&self, bytes: &[u8]) -> Vec<u8> {
        apply_terminal_output_processor(&self.output_processor, bytes)
    }

    fn feed_transport_output_to_terminal(&mut self, bytes: &[u8]) {
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
                if !decoded.is_empty() {
                    // Match the Tauri hook boundary: recording observes UTF-8 display
                    // output after terminal decoding, not the SSH transport bytes.
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
                    self.pending_events.push(TerminalEvent::TrzszTransferPrompt {
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

    fn flush_trzsz_server_writes(&mut self) -> bool {
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

    fn feed_utf8_terminal_output(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.pending_events
                .push(TerminalEvent::Output(bytes.to_vec()));
        }
        let mut term = self.term.lock();
        self.shell_integration
            .advance(&mut self.parser, &mut *term, bytes, |event| {
                self.pending_events.push(event);
            });
    }

    fn drain_transport_output(&mut self) -> TerminalDrainReport {
        self.drain_transport_output_with_budget(TerminalDrainBudget::unlimited())
    }

    fn drain_transport_output_with_budget(
        &mut self,
        budget: TerminalDrainBudget,
    ) -> TerminalDrainReport {
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

            let result = {
                let Some(handle) = self.handle.as_mut() else {
                    break;
                };
                handle.output_rx.try_recv()
            };

            match result {
                Ok(bytes) => {
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
                Err(TryRecvError::Disconnected) => {
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                    }
                    report.mark_changed();
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        report.pending_bytes = self.output_queue_bytes;
        report.drain_duration = started.elapsed();
        report
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

    fn send_command(&mut self, command: SshTransportCommand) -> Result<()> {
        let Some(handle) = self.handle.as_mut() else {
            anyhow::bail!(
                "SSH PTY backend for {}@{}:{} is still connecting",
                self.config.username(),
                self.config.host(),
                self.config.port()
            );
        };
        handle
            .command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

impl TerminalSessionBackend for SshPtySession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::SshPty
    }

    fn title(&self) -> Option<String> {
        Some(self.title.clone().unwrap_or_else(|| self.title_text()))
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    fn is_interactive(&self) -> bool {
        self.lifecycle.is_running() && self.handle.is_some()
    }

    fn process_info(&self) -> TerminalProcessInfo {
        TerminalProcessInfo::default()
    }

    fn refresh_process_info(&mut self) {}

    fn read_pending(&mut self) -> bool {
        let mut changed = self.process_connect_result();
        changed |= self.drain_transport_output().changed;
        changed |= self.flush_trzsz_server_writes();
        while let Ok(event) = self.event_rx.try_recv() {
            if self.handle_alacritty_event(event) {
                changed = true;
            }
        }
        changed
    }

    fn read_pending_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = TerminalDrainReport::default();
        if self.process_connect_result() {
            report.mark_changed();
        }
        report.combine(self.drain_transport_output_with_budget(budget));
        if self.flush_trzsz_server_writes() {
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
            self.send_command(SshTransportCommand::Data(bytes.to_vec()))?;
        }
        Ok(())
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        if self.route_trzsz_text_input(text) {
            return Ok(());
        }
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

    fn set_trzsz_policy(&mut self, policy: Option<TrzszTransferPolicy>) {
        // Tauri's terminal controller applies in-band transfer settings to an
        // existing terminal controller, not only to future panes. Native keeps
        // the same user-visible contract by replacing the idle consumer when
        // settings change; active transfers are left owned by the current
        // consumer so a settings toggle cannot orphan an in-flight protocol.
        match (&mut self.trzsz_consumer, policy) {
            (Some(consumer), Some(policy)) => consumer.update_transfer_policy(policy),
            (Some(consumer), None) if consumer.is_transferring() => {}
            (_, policy) => {
                self.trzsz_consumer = policy.map(TrzszConsumer::new);
            }
        }
    }

    fn take_trzsz_transfer(&mut self) -> Option<TrzszTransfer> {
        self.trzsz_consumer
            .as_mut()
            .and_then(TrzszConsumer::take_active_transfer)
    }

    fn feed_trzsz_terminal_output(&mut self, bytes: &[u8]) {
        self.feed_transport_output_to_terminal(bytes);
    }

    fn interrupt_trzsz_transfer(&mut self) {
        if let Some(consumer) = self.trzsz_consumer.as_mut() {
            consumer.interrupt_transfer();
        }
    }

    fn finish_trzsz_transfer(&mut self) {
        if let Some(consumer) = self.trzsz_consumer.as_mut() {
            consumer.finish_transfer();
        }
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
        let _ = self.send_command(SshTransportCommand::Resize {
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
            let row = &grid[alacritty_terminal::index::Line(line)];
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
        let _ = self.send_command(SshTransportCommand::Close);
        self.handle = None;
        self.runtime = None;
        self.lifecycle = TerminalLifecycle::Closed;
    }

    fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.handle
            .as_ref()
            .and_then(SshPtyHandle::ssh_connection_handle)
    }
}
