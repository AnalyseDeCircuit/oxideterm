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

pub(crate) struct SshPtyCore {
    config: SshSessionConfig,
    parser_state: SshParser,
    activity: crate::activity::TerminalActivitySender,
    lifecycle: TerminalLifecycle,
    runtime: Option<Arc<Runtime>>,
    connect_task: Option<tokio::task::JoinHandle<()>>,
    consumed_sequence: u64,
    last_parse_completed: Option<Instant>,
    connect_rx: Receiver<Result<SshPtyHandle, String>>,
    handle: Option<SshPtyHandle>,
    layout_resize_seen: bool,
    post_connect_input_sent: bool,
    output_queue: VecDeque<PendingSshOutput>,
    output_queue_bytes: usize,
}

impl SshPtyCore {
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
        let (parser_state, activity) = SshParser::new(
            &config,
            resize,
            graphics_options,
            encoding,
            scrollback_lines,
        );

        // GPUI owns a backend runtime for SSH-adjacent work; standalone
        // terminal sessions keep this fallback runtime alive for compatibility.
        let runtime = if start_connection {
            config
                .runtime
                .take()
                .or_else(ssh_worker::standalone_runtime)
        } else {
            None
        };
        let runtime_handle = runtime.as_ref().map(|runtime| runtime.handle().clone());
        let mut connect_task = None;
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
            connect_task = Some(runtime_handle.spawn(async move {
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
            }));
        } else if start_connection {
            let _ = connect_tx.send(Err("failed to initialize SSH runtime".to_string()));
            activity.notify();
        }

        Self {
            parser_state,
            config,
            activity,
            lifecycle: TerminalLifecycle::Running,
            runtime,
            connect_task,
            consumed_sequence: 0,
            last_parse_completed: None,
            connect_rx,
            handle: None,
            layout_resize_seen: false,
            post_connect_input_sent: false,
            output_queue: VecDeque::new(),
            output_queue_bytes: 0,
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
                self.parser_state.command_tx = Some(handle.command_tx.clone());
                self.handle = Some(handle);
                if !self.waiting_for_deferred_pty_resize() {
                    let _ = self.send_command(SshTransportCommand::Resize {
                        cols: self.parser_state.resize.cols as u16,
                        rows: self.parser_state.resize.rows as u16,
                    });
                }
                if !auth_banner_prelude.is_empty() {
                    // Tauri prepends SSH authentication banners when the first
                    // visible terminal becomes ready; consume them before any
                    // post-connect input so the login notice keeps that order.
                    self.parser_state
                        .feed_transport_output(&auth_banner_prelude);
                }
                self.maybe_send_post_connect_input();
                self.parser_state.title = Some(self.title_text());
                self.parser_state
                    .pending_events
                    .push(TerminalEvent::TitleChanged(self.title_text()));
                true
            }
            Err(error) => {
                self.lifecycle = TerminalLifecycle::Exited(None);
                self.parser_state.transport_running = false;
                self.parser_state.tmux_display.reset();
                self.parser_state.feed_utf8_terminal_output(
                    format!("\r\nSSH connection failed: {error}\r\n").as_bytes(),
                );
                self.parser_state
                    .pending_events
                    .push(TerminalEvent::StartupFailed);
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
                self.parser_state
                    .feed_utf8_terminal_output(format!("\r\n{error}\r\n").as_bytes());
            }
        }
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
                self.parser_state
                    .feed_transport_output(&output.chunk[output.consumed_bytes..slice_end]);
                let completed = Instant::now();
                self.last_parse_completed = Some(completed);
                report.record_data_chunk(
                    slice_len,
                    processing_started
                        .map_or(Duration::ZERO, |started| completed.duration_since(started)),
                );
                output.consumed_bytes = slice_end;
                self.output_queue_bytes = self.output_queue_bytes.saturating_sub(slice_len);
                if output.remaining_len() > 0 {
                    // Keep the transport permit and the unconsumed suffix together so
                    // backpressure and byte ordering remain unchanged across UI yields.
                    self.output_queue.push_front(output);
                } else {
                    self.consumed_sequence = output.chunk.sequence();
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
                        self.parser_state.transport_running = false;
                        self.parser_state.tmux_display.reset();
                        let event = if self
                            .handle
                            .as_ref()
                            .is_some_and(SshPtyHandle::shell_started)
                        {
                            TerminalEvent::ChildExited(None)
                        } else {
                            TerminalEvent::StartupFailed
                        };
                        self.parser_state.pending_events.push(event);
                        report.mark_changed();
                    }
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        report.pending_bytes = self.output_queue_bytes;
        report.drain_duration = started.elapsed();
        report
    }

    fn send_command(&mut self, command: SshTransportCommand) -> Result<()> {
        self.parser_state.send_command(command)
    }
}

impl TerminalSessionBackend for SshPtyCore {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::SshPty
    }

    fn title(&self) -> Option<String> {
        Some(
            self.parser_state
                .title
                .clone()
                .unwrap_or_else(|| self.title_text()),
        )
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
        changed |= self
            .parser_state
            .flush_buffered_modem_output(self.lifecycle.is_running());
        changed |= self.parser_state.flush_tmux_commands();
        changed |= self.parser_state.flush_trzsz_server_writes();
        changed |= self.parser_state.flush_modem_server_writes();
        while let Ok(event) = self.parser_state.event_rx.try_recv() {
            if self.parser_state.handle_alacritty_event(event) {
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
        if self
            .parser_state
            .flush_buffered_modem_output(self.lifecycle.is_running())
        {
            report.mark_changed();
        }
        if self.parser_state.flush_tmux_commands() {
            report.mark_changed();
        }
        if self.parser_state.flush_trzsz_server_writes() {
            report.mark_changed();
        }
        if self.parser_state.flush_modem_server_writes() {
            report.mark_changed();
        }

        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(event) = self.parser_state.event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if self.parser_state.handle_alacritty_event(event) {
                report.mark_changed();
            }
        }

        if (report.events_drained >= budget.max_events || budget.time_exhausted(started))
            && !self.parser_state.event_rx.is_empty()
        {
            report.budget_exhausted = true;
        }
        report.drain_duration = started.elapsed();
        report
    }

    fn pending_output_flush_delay(&self) -> Option<Duration> {
        self.parser_state
            .modem_consumer
            .pending_plain_output_delay()
    }

    fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.parser_state.event_rx.activity_receiver()
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.parser_state.pending_events)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        self.parser_state.write_input(bytes)
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.parser_state.write_protocol_bytes(bytes)
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        self.parser_state.write_text(text)
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        self.parser_state.paste_text(text)
    }

    fn set_encoding(&mut self, encoding: TerminalEncoding) {
        self.parser_state.set_encoding(encoding)
    }

    fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.parser_state.set_output_processor(processor)
    }

    fn set_output_events_enabled(&mut self, enabled: bool) {
        self.parser_state.set_output_events_enabled(enabled)
    }

    fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.parser_state.set_trigger_rules(rules)
    }

    fn set_trzsz_policy(&mut self, policy: Option<TrzszTransferPolicy>) {
        self.parser_state.set_trzsz_policy(policy)
    }

    fn take_trzsz_transfer(&mut self) -> Option<TrzszTransfer> {
        self.parser_state.take_trzsz_transfer()
    }

    fn feed_trzsz_terminal_output(&mut self, bytes: &[u8]) {
        self.parser_state.feed_trzsz_terminal_output(bytes)
    }

    fn interrupt_trzsz_transfer(&mut self) {
        self.parser_state.interrupt_trzsz_transfer()
    }

    fn finish_trzsz_transfer(&mut self) {
        self.parser_state.finish_trzsz_transfer()
    }

    fn start_modem_transfer(
        &mut self,
        request: TerminalModemTransferRequest,
    ) -> Option<ModemTransfer> {
        self.parser_state.start_modem_transfer(request)
    }

    fn interrupt_modem_transfer(&mut self) {
        self.parser_state.interrupt_modem_transfer()
    }

    fn finish_modem_transfer(&mut self) {
        self.parser_state.finish_modem_transfer()
    }

    fn mode(&self) -> TermMode {
        self.parser_state.mode()
    }

    fn select_tmux_pane_at(&mut self, col: usize, row: usize) -> Result<bool> {
        self.parser_state.select_tmux_pane_at(col, row)
    }

    fn tmux_local_point(&self, col: usize, row: usize) -> (usize, usize) {
        self.parser_state.tmux_local_point(col, row)
    }

    fn tmux_state(&self) -> Option<crate::TmuxUiState> {
        self.parser_state.tmux_state()
    }

    fn tmux_action(&mut self, action: crate::TmuxAction) -> Result<bool> {
        self.parser_state.tmux_action(action)
    }

    fn tmux_separator_at(&self, col: usize, row: usize) -> Option<crate::TmuxSeparator> {
        self.parser_state.tmux_separator_at(col, row)
    }

    fn resize_tmux_separator(
        &mut self,
        separator: crate::TmuxSeparator,
        delta: i32,
    ) -> Result<bool> {
        self.parser_state.resize_tmux_separator(separator, delta)
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
        self.parser_state.set_focused(focused)
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        self.layout_resize_seen = true;
        self.parser_state.resize_with_cell_size(resize)?;
        // Deferred SSH sessions use this first real GPUI layout resize as the
        // remote PTY allocation boundary. Post-connect input must follow it so
        // the transport cannot fall back to a synthetic 120x40 shell.
        self.maybe_send_post_connect_input();
        Ok(())
    }

    fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.parser_state
                .display_term()
                .lock()
                .scroll_display(Scroll::Delta(delta));
        }
    }

    fn scroll_lines_snapshot_incremental(
        &mut self,
        delta: i32,
        previous: &TerminalSnapshot,
    ) -> TerminalSnapshot {
        if self.parser_state.tmux_display.is_active() {
            self.scroll_lines(delta);
            let size = TerminalSize {
                cols: self.parser_state.resize.cols,
                rows: self.parser_state.resize.rows,
                cell_width: self.parser_state.resize.cell_width,
                cell_height: self.parser_state.resize.cell_height,
            };
            if let Some(snapshot) = self
                .parser_state
                .tmux_display
                .snapshot(size, Some(previous))
            {
                return snapshot;
            }
        }
        let term = self.parser_state.display_term();
        let mut term = term.lock();
        scroll_snapshot_from_term(
            &mut term,
            TerminalSize {
                cols: self.parser_state.resize.cols,
                rows: self.parser_state.resize.rows,
                cell_width: self.parser_state.resize.cell_width,
                cell_height: self.parser_state.resize.cell_height,
            },
            &self.parser_state.graphics,
            delta,
            previous,
        )
    }

    fn page_up(&mut self) {
        self.parser_state
            .display_term()
            .lock()
            .scroll_display(Scroll::PageUp);
    }

    fn page_down(&mut self) {
        self.parser_state
            .display_term()
            .lock()
            .scroll_display(Scroll::PageDown);
    }

    fn scroll_to_top(&mut self) {
        self.parser_state
            .display_term()
            .lock()
            .scroll_display(Scroll::Top);
    }

    fn scroll_to_bottom(&mut self) {
        self.parser_state
            .display_term()
            .lock()
            .scroll_display(Scroll::Bottom);
    }

    fn scroll_to_display_offset(&mut self, offset: usize) {
        let term = self.parser_state.display_term();
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
        let term = self.parser_state.display_term();
        let term = term.lock();
        search_matches_from_term(&term, self.parser_state.resize.cols, query)
    }

    fn search_source(&self) -> Option<crate::TerminalSearchSource> {
        Some(crate::TerminalSearchSource::new(
            self.parser_state.display_term(),
            self.parser_state.resize.cols,
        ))
    }

    fn set_selection(&self, selection: Option<crate::TerminalSelectionRange>) {
        crate::selection::set_term_selection(
            &mut self.parser_state.display_term().lock(),
            selection,
        );
    }

    fn selection(&self) -> Option<crate::TerminalSelectionRange> {
        crate::selection::term_selection(&self.parser_state.display_term().lock())
    }

    fn clear_buffer(&mut self) {
        self.parser_state.clear_buffer();
    }

    fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        let term = self.parser_state.display_term();
        let term = term.lock();
        command_output_text_from_term(&term, mark)
    }

    fn buffer_text(&self) -> String {
        let term = self.parser_state.display_term();
        let term = term.lock();
        terminal_buffer_text_from_term(&term, self.parser_state.resize.cols)
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let size = TerminalSize {
            cols: self.parser_state.resize.cols,
            rows: self.parser_state.resize.rows,
            cell_width: self.parser_state.resize.cell_width,
            cell_height: self.parser_state.resize.cell_height,
        };
        if let Some(snapshot) = self.parser_state.tmux_display.snapshot(size, None) {
            return snapshot;
        }
        let term = self.parser_state.display_term();
        let term = term.lock();
        snapshot_from_term(&term, size, &self.parser_state.graphics)
    }

    fn snapshot_incremental(&self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        let size = TerminalSize {
            cols: self.parser_state.resize.cols,
            rows: self.parser_state.resize.rows,
            cell_width: self.parser_state.resize.cell_width,
            cell_height: self.parser_state.resize.cell_height,
        };
        if let Some(snapshot) = self
            .parser_state
            .tmux_display
            .snapshot(size, Some(previous))
        {
            return snapshot;
        }
        let term = self.parser_state.display_term();
        let mut term = term.lock();
        incremental_snapshot_from_term(&mut term, size, &self.parser_state.graphics, previous)
    }

    fn snapshot_with_display_offset(&self, display_offset: usize, rows: usize) -> TerminalSnapshot {
        let size = TerminalSize {
            cols: self.parser_state.resize.cols,
            rows,
            cell_width: self.parser_state.resize.cell_width,
            cell_height: self.parser_state.resize.cell_height,
        };
        if let Some(snapshot) = self.parser_state.tmux_display.snapshot(size, None) {
            return snapshot;
        }
        let term = self.parser_state.display_term();
        let term = term.lock();
        snapshot_from_term_with_display_offset(
            &term,
            TerminalSize {
                rows: self.parser_state.resize.rows,
                ..size
            },
            &self.parser_state.graphics,
            display_offset,
            rows,
        )
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        self.parser_state.write_protocol_bytes(b"\x03")
    }

    fn kill_active_task(&mut self) -> Result<()> {
        self.parser_state.write_protocol_bytes(b"\x03")
    }

    fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }
        let _ = self.send_command(SshTransportCommand::Close);
        self.parser_state.command_tx = None;
        self.handle = None;

        self.lifecycle = TerminalLifecycle::Closed;
        self.parser_state.transport_running = false;
        self.parser_state.tmux_display.reset();
    }

    fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.handle
            .as_ref()
            .and_then(SshPtyHandle::ssh_connection_handle)
    }
}

#[cfg(test)]
mod ssh_startup_tests {
    use super::*;

    #[test]
    fn failed_ssh_startup_does_not_emit_normal_child_exit() {
        let mut session = SshPtyCore::new_disconnected_for_test(
            SshSessionConfig::new("localhost", 22, "test"),
            80,
            24,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            1000,
        );
        let (tx, rx) = crossbeam_channel::unbounded();
        session.connect_rx = rx;
        tx.send(Err("session limit reached".into())).unwrap();

        assert!(session.process_connect_result());
        assert!(matches!(
            session.lifecycle(),
            TerminalLifecycle::Exited(None)
        ));
        let events = session.take_events();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, TerminalEvent::StartupFailed))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, TerminalEvent::ChildExited(_)))
        );
    }
}

#[cfg(test)]
mod ssh_output_protocol_tests {
    use super::*;
    use oxideterm_modem_transfer::{
        DetectedModemProtocol, ModemIo,
        zmodem::{ZFrameType, encode_hex_header, position_header},
    };

    fn session() -> SshPtyCore {
        let config = SshSessionConfig::new("localhost", 22, "test")
            .with_trzsz_policy(Some(TrzszTransferPolicy::default()));
        let mut session = SshPtyCore::new_disconnected_for_test(
            config,
            80,
            24,
            Default::default(),
            Default::default(),
            100,
        );
        // Display transforms must never touch a transfer's byte stream.
        session.set_output_processor(Some(Arc::new(|bytes| bytes.to_ascii_uppercase())));
        session.set_output_events_enabled(true);
        session
    }

    #[test]
    fn trzsz_split_handshake_routes_config_to_taken_worker_before_display_processing() {
        let handshake = b"::TRZSZ:TRANSFER:R:1.1.6:9\n";
        // zlib/base64 encoding of {"binary":false,"directory":false}.
        let config = b"#CFG:eJyrVkrKzEssqlSySkvMKU7VUUrJLEpNLsmHi9QCANctDJE=\n";
        for split in 0..=handshake.len() {
            let mut session = session();
            session
                .parser_state
                .feed_transport_output(&handshake[..split]);
            session
                .parser_state
                .feed_transport_output(&handshake[split..]);
            let events = session.take_events();
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, TerminalEvent::TrzszTransferPrompt { .. }))
                    .count(),
                1
            );
            let mut transfer = session.take_trzsz_transfer().expect("transfer worker");
            for chunk in config.chunks(7) {
                session.parser_state.feed_transport_output(chunk);
            }
            assert!(
                !session
                    .take_events()
                    .iter()
                    .any(|event| matches!(event, TerminalEvent::Output(_))),
                "protocol data entered recording"
            );
            let config = transfer.recv_config().expect("unmodified protocol config");
            assert_eq!(config["binary"].as_bool(), Some(false));
            assert_eq!(config["directory"].as_bool(), Some(false));
            session.interrupt_trzsz_transfer();
            assert!(
                matches!(transfer.recv_config(), Err(oxideterm_trzsz::TrzszError::InvalidState(reason)) if reason == "Stopped"),
                "subsequent protocol reads must report cancellation"
            );
            session.parser_state.feed_transport_output(b"\r\nafter\r\n");
            session
                .parser_state
                .flush_buffered_modem_output(session.lifecycle.is_running());
            assert!(session.buffer_text().contains("AFTER"));
        }
    }

    #[test]
    fn zmodem_split_handshake_preserves_binary_data_and_returns_trailing_prompt() {
        let header = encode_hex_header(ZFrameType::ZrqInit, position_header(0), true);
        let binary: Vec<u8> = (0..=255).collect();
        for split in 0..=header.len() {
            let mut session = session();
            session.parser_state.feed_transport_output(b"before\r\n");
            session.parser_state.feed_transport_output(&header[..split]);
            session.parser_state.feed_transport_output(&header[split..]);
            assert_eq!(
                session
                    .take_events()
                    .iter()
                    .filter(|event| matches!(event, TerminalEvent::ModemTransferPrompt { .. }))
                    .count(),
                1
            );
            let mut transfer = session
                .parser_state
                .modem_consumer
                .active_transfer()
                .unwrap()
                .clone();
            assert_eq!(transfer.drain_remote_output(), header);
            for chunk in binary.chunks(13) {
                session.parser_state.feed_transport_output(chunk);
            }
            assert_eq!(transfer.drain_remote_output(), binary);
            assert!(
                !session
                    .take_events()
                    .iter()
                    .any(|event| matches!(event, TerminalEvent::Output(_))),
                "binary data entered recording"
            );
            session
                .parser_state
                .feed_transport_output(b"OO\r\n$ after ");
            assert_eq!(transfer.read_byte(Duration::from_millis(10)).unwrap(), b'O');
            assert_eq!(transfer.read_byte(Duration::from_millis(10)).unwrap(), b'O');
            session.finish_modem_transfer();
            assert!(session.buffer_text().contains("$ AFTER"));
        }
    }

    #[test]
    fn lrzsz_xy_negotiation_survives_bytewise_input_with_trzsz_enabled() {
        let cases: &[(&[u8], DetectedModemProtocol)] = &[
            (b"\r\n$ lrx upload.bin\r\nC", DetectedModemProtocol::Xmodem),
            (b"\r\n$ lrb\r\nC", DetectedModemProtocol::Ymodem),
        ];
        for (input, protocol) in cases {
            let mut session = session();
            for byte in *input {
                session.parser_state.feed_transport_output(&[*byte]);
            }
            assert!(session.take_events().iter().any(|event| matches!(event,
                TerminalEvent::ModemTransferPrompt { request, .. } if request.protocol == *protocol
            )));
            let transfer = session
                .parser_state
                .modem_consumer
                .active_transfer()
                .unwrap()
                .clone();
            assert_eq!(transfer.drain_remote_output(), b"C");
            session.interrupt_modem_transfer();
            assert_eq!(
                transfer.take_server_write().as_deref(),
                Some([oxideterm_modem_transfer::xymodem::CAN; 8].as_slice()),
                "the transport must receive the X/YMODEM cancellation sequence"
            );
            session.finish_modem_transfer();
            session.parser_state.feed_transport_output(b"\r\nafter\r\n");
            session
                .parser_state
                .flush_buffered_modem_output(session.lifecycle.is_running());
            assert!(session.buffer_text().contains("AFTER"));
        }
    }
}
