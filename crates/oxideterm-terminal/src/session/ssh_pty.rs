const SSH_OUTPUT_PARSE_SLICE_BYTES: usize = 4 * 1024;

struct PendingSshOutput {
    chunk: SshOutputChunk,
    consumed_bytes: usize,
}

impl PendingSshOutput {
    fn new(chunk: SshOutputChunk) -> Self {
        Self {
            chunk,
            consumed_bytes: 0,
        }
    }

    fn remaining_len(&self) -> usize {
        self.chunk.len().saturating_sub(self.consumed_bytes)
    }
}

pub struct SshPtySession {
    config: SshSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: LocalEventReceiver,
    activity: crate::activity::TerminalActivitySender,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    connect_rx: Receiver<Result<SshPtyHandle, String>>,
    handle: Option<SshPtyHandle>,
    layout_resize_seen: bool,
    post_connect_input_sent: bool,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    graphics_alt_screen_active: bool,
    output_queue: VecDeque<PendingSshOutput>,
    output_queue_bytes: usize,
    magic_scan: MagicScanWindow,
    encoding: TerminalEncoding,
    output_decoder: TerminalOutputDecoder,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    trigger_stream: Option<oxideterm_terminal_triggers::TerminalTriggerStream>,
    privilege_prompt: TerminalPrivilegePromptStream,
    input_encoder: TerminalInputEncoder,
    encoding_detector: EncodingMismatchDetector,
    trzsz_consumer: Option<TrzszConsumer>,
    modem_consumer: ModemConsumer,
    shell_integration: TerminalShellIntegration,
    tmux_display: Arc<crate::tmux::TmuxDisplay>,
    tmux_controller: Option<crate::tmux::TmuxController>,
    tmux_command_queue: VecDeque<Vec<u8>>,
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
        Self::new_inner(
            config,
            cols,
            rows,
            graphics_options,
            encoding,
            scrollback_lines,
            true,
        )
    }

    #[cfg(test)]
    pub(crate) fn new_disconnected_for_test(
        config: SshSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Self {
        // State-only tests must not create a runtime or attempt a network connection.
        Self::new_inner(
            config,
            cols,
            rows,
            graphics_options,
            encoding,
            scrollback_lines,
            false,
        )
    }

    fn new_inner(
        mut config: SshSessionConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
        start_connection: bool,
    ) -> Self {
        let resize = TerminalResize::new(cols, rows, 0, 0);
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

        // GPUI owns a backend runtime for SSH-adjacent work; standalone
        // terminal sessions keep this fallback runtime alive for compatibility.
        let runtime = if !start_connection || config.runtime_handle.is_some() {
            None
        } else {
            Runtime::new().ok()
        };
        let runtime_handle = if start_connection {
            config
                .runtime_handle
                .take()
                .or_else(|| runtime.as_ref().map(|runtime| runtime.handle().clone()))
        } else {
            None
        };
        let (connect_tx, connect_rx) = unbounded();
        if let Some(runtime_handle) = runtime_handle {
            let connection = config.connection.take();
            let registry = config.registry.take();
            let consumer = config.consumer.take();
            let prompt_handler = config.prompt_handler.take();
            let managed_key_resolver = config.managed_key_resolver.take();
            let (cols, rows) = if config.defer_pty_until_resize() {
                (0, 0)
            } else {
                (resize.cols as u32, resize.rows as u32)
            };
            let connect_activity = activity.clone();
            runtime_handle.spawn(async move {
                let result = match connection {
                    Some(SshSessionConnection::New(mut ssh_config)) => {
                        ssh_config.cols = cols;
                        ssh_config.rows = rows;
                        let mut client = SshTransportClient::new(ssh_config);
                        if let Some(prompt_handler) = prompt_handler {
                            client = client.with_prompt_handler(prompt_handler);
                        }
                        if let Some(resolver) = managed_key_resolver {
                            client = client.with_managed_key_resolver(resolver);
                        }
                        match (registry, consumer) {
                            (Some(registry), Some(consumer)) => {
                                client.connect_shell_with_registry(registry, consumer).await
                            }
                            _ => client.connect_shell().await,
                        }
                    }
                    Some(SshSessionConnection::Existing {
                        connection_id,
                        x11_forwarding_override,
                    }) => {
                        match (registry, consumer) {
                            (Some(registry), Some(consumer)) => match x11_forwarding_override {
                                Some(x11_forwarding) => {
                                    SshTransportClient::connect_shell_on_existing_connection_with_x11_forwarding(
                                        registry,
                                        connection_id,
                                        consumer,
                                        cols,
                                        rows,
                                        x11_forwarding,
                                    )
                                    .await
                                }
                                None => SshTransportClient::connect_shell_on_existing_connection(
                                    registry,
                                    connection_id,
                                    consumer,
                                    cols,
                                    rows,
                                )
                                .await,
                            },
                            _ => Err(oxideterm_ssh::SshTransportError::ConnectionFailed(
                                "existing SSH terminal requires a connection registry".to_string(),
                            )),
                        }
                    }
                    Some(SshSessionConnection::Dedicated {
                        mut config,
                        parent_connection_id,
                    }) => {
                        config.cols = cols;
                        config.rows = rows;
                        let mut client = SshTransportClient::new(config);
                        if let Some(prompt_handler) = prompt_handler {
                            client = client.with_prompt_handler(prompt_handler);
                        }
                        if let Some(resolver) = managed_key_resolver {
                            client = client.with_managed_key_resolver(resolver);
                        }
                        match (registry, consumer) {
                            (Some(registry), Some(consumer)) => {
                                // Dedicated terminals still use the registry so
                                // their physical connection has one clear owner.
                                client
                                    .connect_shell_with_dedicated_registry(
                                        registry,
                                        consumer,
                                        parent_connection_id,
                                    )
                                    .await
                            }
                            _ => Err(oxideterm_ssh::SshTransportError::ConnectionFailed(
                                "dedicated SSH terminal requires a connection registry".to_string(),
                            )),
                        }
                    }
                    None => Err(oxideterm_ssh::SshTransportError::ConnectionFailed(
                        "SSH terminal connection ownership was already transferred".to_string(),
                    )),
                }
                .map_err(|error| error.to_string());
                let _ = connect_tx.send(result);
                connect_activity.notify();
            });
        } else if start_connection {
            let _ = connect_tx.send(Err("failed to initialize SSH runtime".to_string()));
            activity.notify();
        }

        let trzsz_consumer = config.trzsz_policy().map(TrzszConsumer::new);
        let tmux_graphics_options = graphics_options.clone();
        Self {
            config,
            term,
            parser: Processor::new(),
            event_rx,
            activity,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            runtime,
            connect_rx,
            handle: None,
            layout_resize_seen: false,
            post_connect_input_sent: false,
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
            trigger_stream: None,
            privilege_prompt: TerminalPrivilegePromptStream::default(),
            input_encoder: TerminalInputEncoder::new(encoding),
            encoding_detector: EncodingMismatchDetector::new(encoding),
            trzsz_consumer,
            modem_consumer: ModemConsumer::new(),
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
            Ok(mut handle) => {
                let activity = self.activity.clone();
                handle
                    .output_rx
                    .set_activity_callback(Arc::new(move || activity.notify()));
                let auth_banner_prelude = handle.take_auth_banner_prelude();
                self.handle = Some(handle);
                if !self.waiting_for_deferred_pty_resize() {
                    let _ = self.send_command(SshTransportCommand::Resize {
                        cols: self.resize.cols as u16,
                        rows: self.resize.rows as u16,
                    });
                }
                if !auth_banner_prelude.is_empty() {
                    // Tauri prepends SSH authentication banners when the first
                    // visible terminal becomes ready; consume them before any
                    // post-connect input so the login notice keeps that order.
                    self.feed_transport_output(&auth_banner_prelude);
                }
                self.maybe_send_post_connect_input();
                self.title = Some(self.title_text());
                self.pending_events
                    .push(TerminalEvent::TitleChanged(self.title_text()));
                true
            }
            Err(error) => {
                self.lifecycle = TerminalLifecycle::Exited(None);
                self.tmux_display.reset();
                self.feed_utf8_terminal_output(
                    format!("\r\nSSH connection failed: {error}\r\n").as_bytes(),
                );
                self.pending_events.push(TerminalEvent::ChildExited(None));
                true
            }
        }
    }

    fn waiting_for_deferred_pty_resize(&self) -> bool {
        self.config.defer_pty_until_resize() && !self.layout_resize_seen
    }

    fn maybe_send_post_connect_input(&mut self) {
        if self.post_connect_input_sent
            || self.handle.is_none()
            || self.waiting_for_deferred_pty_resize()
        {
            return;
        }

        self.post_connect_input_sent = true;
        match self.config.post_connect_input() {
            Ok(Some(payload)) => {
                let _ = self.send_command(SshTransportCommand::Data(payload));
            }
            Ok(None) => {}
            Err(error) => {
                self.feed_utf8_terminal_output(format!("\r\n{error}\r\n").as_bytes());
            }
        }
    }

    fn feed_transport_output(&mut self, bytes: &[u8]) {
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
            self.pending_events.push(TerminalEvent::Output(bytes.to_vec()));
        }
    }

    fn feed_transport_output_to_terminal(&mut self, bytes: &[u8]) {
        let events = self.modem_consumer.process_server_output(bytes);
        self.handle_modem_consumer_events(events);
    }

    fn feed_plain_transport_output_to_terminal(&mut self, bytes: &[u8]) {
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

    fn flush_modem_server_writes(&mut self) -> bool {
        let Some(transfer) = self.modem_consumer.active_transfer_input() else {
            return false;
        };
        let mut changed = false;
        while let Some(bytes) = transfer.take_server_write() {
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

    fn feed_utf8_terminal_output(&mut self, bytes: &[u8]) {
        self.push_output_event(bytes);
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
            if budget.time_exhausted(started)
                || report.drained_bytes >= budget.max_bytes
                || report.events_drained >= budget.max_events
            {
                report.budget_exhausted = !self.output_queue.is_empty()
                    || self
                        .handle
                        .as_ref()
                        .is_some_and(|handle| !handle.output_rx.is_empty());
                break;
            }

            if let Some(mut output) = self.output_queue.pop_front() {
                let remaining_budget = budget.max_bytes.saturating_sub(report.drained_bytes);
                let slice_len = output
                    .remaining_len()
                    .min(SSH_OUTPUT_PARSE_SLICE_BYTES)
                    .min(remaining_budget);
                let slice_end = output.consumed_bytes.saturating_add(slice_len);
                report.events_drained += 1;
                let processing_started = budget.collect_performance_metrics.then(Instant::now);
                self.feed_transport_output(&output.chunk[output.consumed_bytes..slice_end]);
                report.record_data_chunk(
                    slice_len,
                    processing_started.map_or(Duration::ZERO, |started| started.elapsed()),
                );
                output.consumed_bytes = slice_end;
                self.output_queue_bytes = self.output_queue_bytes.saturating_sub(slice_len);
                if output.remaining_len() > 0 {
                    // Keep the transport permit and the unconsumed suffix together so
                    // backpressure and byte ordering remain unchanged across UI yields.
                    self.output_queue.push_front(output);
                }
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
                    self.output_queue_bytes = self.output_queue_bytes.saturating_add(bytes.len());
                    self.output_queue.push_back(PendingSshOutput::new(bytes));
                }
                Err(TryRecvError::Disconnected) => {
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.tmux_display.reset();
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

    fn write_transport_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.send_command(SshTransportCommand::Data(bytes.to_vec()))?;
        }
        Ok(())
    }

    fn queue_tmux_commands(&mut self, commands: impl IntoIterator<Item = Vec<u8>>) {
        self.tmux_command_queue.extend(commands);
        self.flush_tmux_commands();
    }

    fn flush_tmux_commands(&mut self) -> bool {
        let Some(handle) = self.handle.as_mut() else {
            return false;
        };
        let mut changed = false;
        while let Some(command) = self.tmux_command_queue.pop_front() {
            match handle
                .command_tx
                .try_send(SshTransportCommand::Data(command))
            {
                Ok(()) => changed = true,
                Err(tokio::sync::mpsc::error::TrySendError::Full(
                    SshTransportCommand::Data(command),
                )) => {
                    self.tmux_command_queue.push_front(command);
                    break;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    self.tmux_command_queue.clear();
                    break;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => unreachable!(),
            }
        }
        changed
    }

    fn display_term(&self) -> Arc<FairMutex<Term<LocalEventListener>>> {
        self.tmux_display.term().unwrap_or_else(|| self.term.clone())
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
        changed |= self.flush_tmux_commands();
        changed |= self.flush_trzsz_server_writes();
        changed |= self.flush_modem_server_writes();
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
        if self.flush_tmux_commands() {
            report.mark_changed();
        }
        if self.flush_trzsz_server_writes() {
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

    fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.event_rx.activity_receiver()
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(commands) = self.tmux_display.input_commands(bytes) {
            self.queue_tmux_commands(commands);
            return Ok(());
        }
        self.write_transport_bytes(bytes)
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(commands) = self.tmux_display.protocol_commands(bytes) {
            self.queue_tmux_commands(commands);
            return Ok(());
        }
        self.write_transport_bytes(bytes)
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        if !self.tmux_display.is_active() && self.route_trzsz_text_input(text) {
            return Ok(());
        }
        let encoded = self.input_encoder.encode_text(text);
        self.write_input(encoded.as_ref())
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        let bytes = self
            .input_encoder
            .encode_paste(text, self.mode().contains(TermMode::BRACKETED_PASTE));
        if let Some(commands) = self.tmux_display.paste_commands(&bytes) {
            self.queue_tmux_commands(commands);
            return Ok(());
        }
        self.write_input(&bytes)
    }

    fn set_encoding(&mut self, encoding: TerminalEncoding) {
        if self.encoding == encoding {
            return;
        }
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

    fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.output_processor = processor;
        self.output_decoder.reset();
        self.privilege_prompt = TerminalPrivilegePromptStream::default();
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
        self.feed_plain_transport_output_to_terminal(&trailing_output);
    }

    fn mode(&self) -> TermMode {
        let term = self.display_term();
        *term.lock().mode()
    }

    fn select_tmux_pane_at(&mut self, col: usize, row: usize) -> Result<bool> {
        let Some(command) = self.tmux_display.select_pane_command(col, row) else {
            return Ok(false);
        };
        self.queue_tmux_commands([command]);
        Ok(true)
    }

    fn tmux_local_point(&self, col: usize, row: usize) -> (usize, usize) {
        self.tmux_display.local_point(col, row)
    }

    fn tmux_state(&self) -> Option<crate::TmuxUiState> {
        self.tmux_display.ui_state()
    }

    fn tmux_action(&mut self, action: crate::TmuxAction) -> Result<bool> {
        let Some(command) = self.tmux_display.action_command(action) else {
            return Ok(false);
        };
        self.queue_tmux_commands([command]);
        Ok(true)
    }

    fn tmux_separator_at(&self, col: usize, row: usize) -> Option<crate::TmuxSeparator> {
        self.tmux_display.separator_at(col, row)
    }

    fn resize_tmux_separator(
        &mut self,
        separator: crate::TmuxSeparator,
        delta: i32,
    ) -> Result<bool> {
        let Some(command) = self
            .tmux_display
            .resize_separator_command(separator, delta)
        else {
            return Ok(false);
        };
        self.queue_tmux_commands([command]);
        Ok(true)
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
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

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        let grid_changed = self.resize.cols != resize.cols || self.resize.rows != resize.rows;
        if grid_changed {
            self.shell_integration
                .reset_command_marks_for_grid_reflow(|event| self.pending_events.push(event));
        }
        self.resize = resize;
        self.layout_resize_seen = true;
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
        // Deferred SSH sessions use this first real GPUI layout resize as the
        // remote PTY allocation boundary. Post-connect input must follow it so
        // the transport cannot fall back to a synthetic 120x40 shell.
        self.maybe_send_post_connect_input();
        Ok(())
    }

    fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.display_term().lock().scroll_display(Scroll::Delta(delta));
        }
    }

    fn scroll_lines_snapshot_incremental(
        &mut self,
        delta: i32,
        previous: &TerminalSnapshot,
    ) -> TerminalSnapshot {
        if self.tmux_display.is_active() {
            self.scroll_lines(delta);
            let size = TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            };
            if let Some(snapshot) = self.tmux_display.snapshot(size, Some(previous)) {
                return snapshot;
            }
        }
        let term = self.display_term();
        let mut term = term.lock();
        scroll_snapshot_from_term(
            &mut term,
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
            delta,
            previous,
        )
    }

    fn page_up(&mut self) {
        self.display_term().lock().scroll_display(Scroll::PageUp);
    }

    fn page_down(&mut self) {
        self.display_term().lock().scroll_display(Scroll::PageDown);
    }

    fn scroll_to_top(&mut self) {
        self.display_term().lock().scroll_display(Scroll::Top);
    }

    fn scroll_to_bottom(&mut self) {
        self.display_term().lock().scroll_display(Scroll::Bottom);
    }

    fn scroll_to_display_offset(&mut self, offset: usize) {
        let term = self.display_term();
        let mut term = term.lock();
        let max_offset = term.total_lines().saturating_sub(term.screen_lines());
        let target = offset.min(max_offset);
        let current = term.grid().display_offset();
        let delta = target as i32 - current as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        let term = self.display_term();
        let term = term.lock();
        search_matches_from_term(&term, self.resize.cols, query)
    }

    fn search_source(&self) -> Option<crate::TerminalSearchSource> {
        Some(crate::TerminalSearchSource::new(
            self.display_term(),
            self.resize.cols,
        ))
    }

    fn clear_buffer(&mut self) {
        let term = self.display_term();
        let mut term = term.lock();
        clear_terminal_buffer(&mut term);
        self.graphics.clear();
    }

    fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        let term = self.display_term();
        let term = term.lock();
        command_output_text_from_term(&term, mark)
    }

    fn buffer_text(&self) -> String {
        let term = self.display_term();
        let term = term.lock();
        terminal_buffer_text_from_term(&term, self.resize.cols)
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let size = TerminalSize {
            cols: self.resize.cols,
            rows: self.resize.rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        };
        if let Some(snapshot) = self.tmux_display.snapshot(size, None) {
            return snapshot;
        }
        let term = self.display_term();
        let term = term.lock();
        snapshot_from_term(&term, size, &self.graphics)
    }

    fn snapshot_incremental(&self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        let size = TerminalSize {
            cols: self.resize.cols,
            rows: self.resize.rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        };
        if let Some(snapshot) = self.tmux_display.snapshot(size, Some(previous)) {
            return snapshot;
        }
        let term = self.display_term();
        let mut term = term.lock();
        incremental_snapshot_from_term(&mut term, size, &self.graphics, previous)
    }

    fn snapshot_with_display_offset(
        &self,
        display_offset: usize,
        rows: usize,
    ) -> TerminalSnapshot {
        let size = TerminalSize {
            cols: self.resize.cols,
            rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        };
        if let Some(snapshot) = self.tmux_display.snapshot(size, None) {
            return snapshot;
        }
        let term = self.display_term();
        let term = term.lock();
        snapshot_from_term_with_display_offset(
            &term,
            TerminalSize {
                rows: self.resize.rows,
                ..size
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
        let _ = self.send_command(SshTransportCommand::Close);
        self.handle = None;
        self.runtime = None;
        self.lifecycle = TerminalLifecycle::Closed;
        self.tmux_display.reset();
    }

    fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.handle
            .as_ref()
            .and_then(SshPtyHandle::ssh_connection_handle)
    }
}
