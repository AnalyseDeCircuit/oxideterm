pub struct LocalPtySession {
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    notifier: LocalGraphicsNotifier,
    event_rx: LocalEventReceiver,
    graphics_rx: Receiver<TerminalGraphicsEvent>,
    magic_rx: Receiver<TerminalMagicKind>,
    terminal_event_rx: Receiver<TerminalEvent>,
    stats_rx: Receiver<LocalPtyReadReport>,
    pending_events: Vec<TerminalEvent>,
    io_thread: Option<JoinHandle<()>>,
    size: TerminalSize,
    title: Option<String>,
    lifecycle: TerminalLifecycle,
    process: ProcessState,
    _shell_integration: Option<LocalShellIntegration>,
    shell_integration_launch_state: TerminalCwdIntegrationLaunchState,
    #[cfg(windows)]
    process_job: Option<WindowsTerminalJob>,
    graphics: TerminalGraphicsState,
    encoding: TerminalEncoding,
    input_encoder: TerminalInputEncoder,
    tmux_display: Arc<crate::tmux::TmuxDisplay>,
}

pub type LocalTerminal = LocalPtySession;

impl LocalPtySession {
    pub fn spawn_default(cols: usize, rows: usize) -> Result<Self> {
        Self::spawn_with_graphics_and_encoding(
            cols,
            rows,
            GraphicsOptions::default(),
            TerminalEncoding::Utf8,
            1000,
        )
    }

    pub fn spawn_with_graphics_options(
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
    ) -> Result<Self> {
        Self::spawn_with_graphics_and_encoding(
            cols,
            rows,
            graphics_options,
            TerminalEncoding::Utf8,
            1000,
        )
    }

    pub fn spawn_with_graphics_and_encoding(
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Result<Self> {
        Self::spawn_with_config_graphics_and_encoding(
            cols,
            rows,
            LocalPtyConfig::default(),
            graphics_options,
            encoding,
            scrollback_lines,
        )
    }

    pub fn spawn_with_config_graphics_and_encoding(
        cols: usize,
        rows: usize,
        local_config: LocalPtyConfig,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
    ) -> Result<Self> {
        let size = TerminalSize {
            cols: cols.max(2),
            rows: rows.max(2),
            cell_width: 0,
            cell_height: 0,
        };

        let shell = local_config.shell.clone().unwrap_or_else(default_shell);
        let shell_program = shell.path.display().to_string();
        let terminal_env = oxideterm_terminal_env(&local_config, &shell);
        #[cfg(target_os = "windows")]
        let base_shell_args = powershell_init_args(&local_config, &shell)
            .unwrap_or_else(|| shell_args_for_profile(&shell, local_config.load_profile));
        #[cfg(not(target_os = "windows"))]
        let base_shell_args = shell_args_for_profile(&shell, local_config.load_profile);
        let launch =
            prepare_local_shell_launch(&local_config, &shell, terminal_env, base_shell_args);
        let shell_args = launch.args;

        let (listener, event_rx) = local_event_channel();
        let tmux_display = Arc::new(crate::tmux::TmuxDisplay::default());
        let (graphics_tx, graphics_rx) = unbounded();
        let (magic_tx, magic_rx) = unbounded();
        let (terminal_event_tx, terminal_event_rx) = unbounded();
        let (stats_tx, stats_rx) = unbounded();

        let terminal_config = interactive_terminal_config(scrollback_lines);

        let term = Arc::new(FairMutex::new(Term::new(
            terminal_config,
            &size,
            listener.clone(),
        )));
        let cwd = local_config
            .cwd
            .filter(|path| !path.as_os_str().is_empty())
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
            .or_else(|| env::current_dir().ok());
        #[cfg(target_os = "windows")]
        let working_directory = if matches!(shell.id.as_str(), "powershell" | "pwsh") {
            None
        } else {
            cwd.clone()
        };
        #[cfg(not(target_os = "windows"))]
        let working_directory = cwd.clone();
        let pty = tty::new(
            &tty::Options {
                shell: Some(Shell::new(shell_program, shell_args)),
                working_directory,
                drain_on_exit: true,
                env: launch.env,
                #[cfg(target_os = "windows")]
                escape_args: false,
            },
            window_size(size),
            0,
        )
        .context("failed to spawn local shell PTY")?;
        #[cfg(target_os = "windows")]
        let shell_pid = windows_shell_pid(&pty);
        #[cfg(not(target_os = "windows"))]
        let shell_pid = Some(pty.child().id());
        #[cfg(windows)]
        let process_job = WindowsTerminalJob::for_shell(shell_pid);
        #[cfg(not(target_os = "windows"))]
        let pty_master = pty.file().try_clone().ok();
        #[cfg(target_os = "windows")]
        let pty_master = None;
        let process = ProcessState::new(shell_pid, pty_master, cwd);
        let tmux_graphics_options = graphics_options.clone();
        let event_loop = LocalGraphicsEventLoop::new(
            term.clone(),
            listener.clone(),
            pty,
            true,
            graphics_tx,
            magic_tx,
            terminal_event_tx,
            stats_tx,
            size,
            graphics_options,
            encoding,
            crate::tmux::TmuxController::new(
                tmux_display.clone(),
                listener.clone(),
                size,
                encoding,
                scrollback_lines,
                tmux_graphics_options,
            ),
        )
        .context("failed to create terminal event loop")?;
        let pty_tx = event_loop.channel();
        let notifier = LocalGraphicsNotifier(pty_tx);
        let io_thread = event_loop.spawn();

        Ok(Self {
            term,
            notifier,
            event_rx,
            graphics_rx,
            magic_rx,
            terminal_event_rx,
            stats_rx,
            pending_events: Vec::new(),
            io_thread: Some(io_thread),
            size,
            title: None,
            lifecycle: TerminalLifecycle::Running,
            process,
            _shell_integration: launch.integration,
            shell_integration_launch_state: launch.integration_state,
            #[cfg(windows)]
            process_job,
            graphics: TerminalGraphicsState::default(),
            encoding,
            input_encoder: TerminalInputEncoder::new(encoding),
            tmux_display,
        })
    }

    pub fn drain_output(&mut self) -> bool {
        self.drain_output_with_budget(TerminalDrainBudget::unlimited())
            .changed
    }

    pub fn drain_output_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        let started = std::time::Instant::now();
        let mut report = TerminalDrainReport::default();
        let mut changed = false;
        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(stats) = self.stats_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            report.drained_bytes = report.drained_bytes.saturating_add(stats.raw_bytes);
            report.max_data_chunk_bytes = report
                .max_data_chunk_bytes
                .max(stats.max_data_chunk_bytes);
            if budget.collect_performance_metrics {
                report.output_processing_duration += stats.output_processing_duration;
                report.terminal_lock_wait_duration += stats.terminal_lock_wait_duration;
            }
            report.budget_exhausted |= stats.budget_exhausted;
        }

        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(event) = self.event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if self.handle_alacritty_event(event) {
                changed = true;
            }
        }

        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(event) = self.graphics_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if let Some(response) = self.graphics.handle_event(event) {
                let _ = self.write_protocol_bytes(&response);
            }
            changed = true;
        }

        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(kind) = self.magic_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            self.pending_events.push(TerminalEvent::MagicDetected(kind));
        }

        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(event) = self.terminal_event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            self.pending_events.push(event);
        }

        report.changed = changed;
        report.budget_exhausted |= (report.events_drained >= budget.max_events
            || budget.time_exhausted(started))
            && (!self.event_rx.is_empty()
                || !self.graphics_rx.is_empty()
                || !self.magic_rx.is_empty()
                || !self.terminal_event_rx.is_empty()
                || !self.stats_rx.is_empty());
        report.drain_duration = started.elapsed();
        report
    }

    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(commands) = self.tmux_display.input_commands(bytes) {
            for command in commands {
                self.write_control_bytes(command)?;
            }
            return Ok(());
        }
        self.write_transport_bytes(bytes)
    }

    pub fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(commands) = self.tmux_display.protocol_commands(bytes) {
            for command in commands {
                self.write_control_bytes(command)?;
            }
            return Ok(());
        }
        self.write_transport_bytes(bytes)
    }

    fn write_transport_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.notifier.notify(Cow::Owned(bytes.to_vec()));
        }
        Ok(())
    }

    fn write_control_bytes(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.notifier
                .0
                .send(LocalGraphicsMsg::ControlInput(Cow::Owned(bytes)))?;
        }
        Ok(())
    }

    pub fn write_text(&mut self, text: &str) -> Result<()> {
        let encoded = self.input_encoder.encode_text(text);
        self.write_input(encoded.as_ref())
    }

    pub fn set_encoding(&mut self, encoding: TerminalEncoding) {
        if self.encoding == encoding {
            return;
        }
        self.encoding = encoding;
        self.input_encoder.set_encoding(encoding);
        let _ = self
            .notifier
            .0
            .send(LocalGraphicsMsg::SetEncoding(encoding));
    }

    pub fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        // Local PTY output is parsed on the graphics reader thread, so the
        // processor must be transferred to that owner instead of stored only on
        // the UI-side session facade.
        let _ = self
            .notifier
            .0
            .send(LocalGraphicsMsg::SetOutputProcessor(processor));
    }

    pub fn set_output_events_enabled(&mut self, enabled: bool) {
        // Output events are only needed while recording. Let the reader thread
        // skip allocating TerminalEvent::Output on the normal render path.
        let _ = self
            .notifier
            .0
            .send(LocalGraphicsMsg::SetOutputEventsEnabled(enabled));
    }

    pub fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        // Cross-chunk state is transferred to and owned by the PTY reader thread.
        let _ = self
            .notifier
            .0
            .send(LocalGraphicsMsg::SetTriggerRules(rules));
    }

    pub fn start_modem_transfer(
        &mut self,
        request: TerminalModemTransferRequest,
    ) -> Option<ModemTransfer> {
        let (response_tx, response_rx) = std::sync::mpsc::channel();
        let message = LocalGraphicsMsg::StartModemTransfer {
            request,
            response_tx,
        };
        self.notifier.0.send(message).ok()?;
        response_rx.recv_timeout(std::time::Duration::from_secs(1)).ok()?
    }

    pub fn interrupt_modem_transfer(&mut self) {
        let _ = self.notifier.0.send(LocalGraphicsMsg::InterruptModemTransfer);
    }

    pub fn finish_modem_transfer(&mut self) {
        let _ = self.notifier.0.send(LocalGraphicsMsg::FinishModemTransfer);
    }

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    pub fn process_info(&self) -> TerminalProcessInfo {
        self.process.info.clone()
    }

    pub fn shell_integration_launch_state(&self) -> TerminalCwdIntegrationLaunchState {
        self.shell_integration_launch_state
    }

    pub fn buffer_line_count(&self) -> usize {
        // Line-count consumers do not need an immutable cell snapshot. Reading the emulator's
        // geometry avoids copying the full scrollback for hidden or detached local sessions.
        self.display_term().lock().total_lines()
    }

    pub fn process_info_probe(&self) -> Option<TerminalProcessProbe> {
        self.lifecycle.is_running().then(|| self.process.probe()).flatten()
    }

    pub fn apply_process_info(&mut self, info: TerminalProcessInfo) -> bool {
        self.process.apply_probe_result(info)
    }

    pub fn refresh_process_info(&mut self) {
        if self.lifecycle.is_running() {
            self.process.refresh();
        }
    }

    pub fn terminate_active_task(&mut self) -> Result<()> {
        if self.tmux_display.is_active() {
            return self.write_input(b"\x03");
        }
        self.signal_active_task(TerminalSignal::Terminate)
    }

    pub fn kill_active_task(&mut self) -> Result<()> {
        if self.tmux_display.is_active() {
            return self.write_input(b"\x03");
        }
        self.signal_active_task(TerminalSignal::Kill)
    }

    fn signal_active_task(&mut self, signal: TerminalSignal) -> Result<()> {
        self.refresh_process_info();
        let foreground_group = self.process.info.foreground_process_group_id;
        let shell_pid = self.process.info.shell_pid;
        if foreground_group.is_none() || foreground_group == shell_pid {
            anyhow::bail!("no foreground terminal task is active");
        }

        signal_process_group(foreground_group, signal)
    }

    pub fn paste_text(&mut self, text: &str) -> Result<()> {
        let bytes = self
            .input_encoder
            .encode_paste(text, self.mode().contains(TermMode::BRACKETED_PASTE));
        if let Some(commands) = self.tmux_display.paste_commands(&bytes) {
            for command in commands {
                self.write_control_bytes(command)?;
            }
            return Ok(());
        }
        self.write_input(&bytes)
    }

    pub fn mode(&self) -> TermMode {
        let term = self.display_term();
        *term.lock().mode()
    }

    pub fn select_tmux_pane_at(&mut self, col: usize, row: usize) -> Result<bool> {
        let Some(command) = self.tmux_display.select_pane_command(col, row) else {
            return Ok(false);
        };
        self.write_control_bytes(command)?;
        Ok(true)
    }

    pub fn tmux_local_point(&self, col: usize, row: usize) -> (usize, usize) {
        self.tmux_display.local_point(col, row)
    }

    pub fn tmux_state(&self) -> Option<crate::TmuxUiState> {
        self.tmux_display.ui_state()
    }

    pub fn tmux_action(&mut self, action: crate::TmuxAction) -> Result<bool> {
        let Some(command) = self.tmux_display.action_command(action) else {
            return Ok(false);
        };
        self.write_control_bytes(command)?;
        Ok(true)
    }

    pub fn tmux_separator_at(&self, col: usize, row: usize) -> Option<crate::TmuxSeparator> {
        self.tmux_display.separator_at(col, row)
    }

    pub fn resize_tmux_separator(
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
        self.write_control_bytes(command)?;
        Ok(true)
    }

    fn display_term(&self) -> Arc<FairMutex<Term<LocalEventListener>>> {
        self.tmux_display.term().unwrap_or_else(|| self.term.clone())
    }

    pub fn set_focused(&mut self, focused: bool) -> Result<()> {
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

    fn handle_alacritty_event(&mut self, event: AlacEvent) -> bool {
        match event {
            AlacEvent::Title(title) => {
                self.title = Some(title.clone());
                self.pending_events.push(TerminalEvent::TitleChanged(title));
                false
            }
            AlacEvent::ResetTitle => {
                self.title = None;
                self.pending_events.push(TerminalEvent::TitleReset);
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
            AlacEvent::ColorRequest(index, formatter) => {
                let override_color = (index <= 268)
                    .then(|| self.display_term().lock().colors()[index])
                    .flatten();
                let color = color_for_alacritty_request_with_override(index, override_color);
                let _ = self.write_protocol_bytes(formatter(color).as_bytes());
                false
            }
            AlacEvent::TextAreaSizeRequest(formatter) => {
                let response = formatter(window_size(self.size));
                let _ = self.write_protocol_bytes(response.as_bytes());
                false
            }
            AlacEvent::ChildExit(status) => {
                self.tmux_display.reset();
                let code = status.code();
                self.lifecycle = TerminalLifecycle::Exited(code);
                self.process.mark_exited();
                // Startup files are session-owned and can be removed as soon
                // as the shell exits, even if its closed pane remains mounted.
                self._shell_integration.take();
                self.pending_events.push(TerminalEvent::ChildExited(code));
                self.join_io_thread();
                true
            }
            AlacEvent::Exit => false,
        }
    }

    pub fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }

        if self.lifecycle.is_running() {
            #[cfg(windows)]
            if let Some(job) = &self.process_job {
                job.terminate();
            }
            #[cfg(not(windows))]
            cleanup_local_pty_process_tree(self.process.info.shell_pid);

            let _ = self.notifier.0.send(LocalGraphicsMsg::Shutdown);
            self.detach_io_thread();
        }

        self.lifecycle = TerminalLifecycle::Closed;
        self.tmux_display.reset();
        self.process.mark_exited();
        self._shell_integration.take();
    }

    fn join_io_thread(&mut self) {
        if let Some(io_thread) = self.io_thread.take() {
            if let Err(error) = io_thread.join() {
                tracing::debug!(
                    ?error,
                    "terminal graphics event loop thread panicked during shutdown"
                );
            }
        }
    }

    fn detach_io_thread(&mut self) {
        let _ = self.io_thread.take();
    }

    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<()> {
        self.apply_resize(TerminalResize::new(
            cols,
            rows,
            self.size.cell_width,
            self.size.cell_height,
        ))
    }

    pub fn resize_with_cell_size(
        &mut self,
        cols: usize,
        rows: usize,
        cell_width: u16,
        cell_height: u16,
    ) -> Result<()> {
        self.apply_resize(TerminalResize::new(cols, rows, cell_width, cell_height))
    }

    fn apply_resize(&mut self, resize: TerminalResize) -> Result<()> {
        let next = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };

        if next.cols == self.size.cols
            && next.rows == self.size.rows
            && next.cell_width == self.size.cell_width
            && next.cell_height == self.size.cell_height
        {
            return Ok(());
        }

        if next.cols != self.size.cols || next.rows != self.size.rows {
            self.term.lock().resize(next);
        }
        self.notifier.on_resize(window_size(next));
        if let Some(command) = self.tmux_display.resize_command(next.cols, next.rows) {
            self.write_control_bytes(command)?;
        }
        self.size = next;
        Ok(())
    }

    pub fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.display_term().lock().scroll_display(Scroll::Delta(delta));
        }
    }

    pub fn scroll_lines_snapshot_incremental(
        &mut self,
        delta: i32,
        previous: &TerminalSnapshot,
    ) -> TerminalSnapshot {
        if self.tmux_display.is_active() {
            self.scroll_lines(delta);
            if let Some(snapshot) = self.tmux_display.snapshot(self.size, Some(previous)) {
                return snapshot;
            }
        }
        let term = self.display_term();
        let mut term = term.lock();
        scroll_snapshot_from_term(&mut term, self.size, &self.graphics, delta, previous)
    }

    pub fn page_up(&mut self) {
        self.display_term().lock().scroll_display(Scroll::PageUp);
    }

    pub fn page_down(&mut self) {
        self.display_term().lock().scroll_display(Scroll::PageDown);
    }

    pub fn scroll_to_top(&mut self) {
        self.display_term().lock().scroll_display(Scroll::Top);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.display_term().lock().scroll_display(Scroll::Bottom);
    }

    pub fn scroll_to_display_offset(&mut self, offset: usize) {
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

    pub fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        let term = self.display_term();
        let term = term.lock();
        search_matches_from_term(&term, self.size.cols, query)
    }

    pub fn search_source(&self) -> TerminalSearchSource {
        TerminalSearchSource::new(self.display_term(), self.size.cols)
    }

    pub fn clear_buffer(&mut self) {
        let term = self.display_term();
        let mut term = term.lock();
        crate::session::clear_terminal_buffer(&mut term);
        self.graphics.clear();
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        if let Some(snapshot) = self.tmux_display.snapshot(self.size, None) {
            return snapshot;
        }
        let term = self.display_term();
        let term = term.lock();
        snapshot_from_term(&term, self.size, &self.graphics)
    }

    pub fn snapshot_incremental(&self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        if let Some(snapshot) = self.tmux_display.snapshot(self.size, Some(previous)) {
            return snapshot;
        }
        let term = self.display_term();
        let mut term = term.lock();
        incremental_snapshot_from_term(&mut term, self.size, &self.graphics, previous)
    }

    pub fn snapshot_with_display_offset(
        &self,
        display_offset: usize,
        rows: usize,
    ) -> TerminalSnapshot {
        let requested_size = TerminalSize { rows, ..self.size };
        if let Some(snapshot) = self.tmux_display.snapshot(requested_size, None) {
            return snapshot;
        }
        let term = self.display_term();
        let term = term.lock();
        snapshot_from_term_with_display_offset(
            &term,
            self.size,
            &self.graphics,
            display_offset,
            rows,
        )
    }
}

#[cfg(target_os = "windows")]
fn windows_shell_pid(pty: &tty::Pty) -> Option<u32> {
    let handle = windows::Win32::Foundation::HANDLE(pty.child_watcher().raw_handle());
    let pid = unsafe { windows::Win32::System::Threading::GetProcessId(handle) };
    (pid != 0).then_some(pid)
}

pub(crate) fn snapshot_from_term<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
) -> TerminalSnapshot {
    snapshot_from_term_with_display_offset(
        term,
        size,
        graphics,
        term.grid().display_offset(),
        size.rows,
    )
}

pub(crate) fn incremental_snapshot_from_term<T: EventListener>(
    term: &mut Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    previous: &TerminalSnapshot,
) -> TerminalSnapshot {
    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let display_offset = term.grid().display_offset().min(scrollback_lines);
    let shape_compatible = previous.cols == size.cols
        && previous.rows == size.rows
        && previous.display_offset == display_offset
        && previous.lines.len() == size.rows;

    let (dirty_rows, scroll_up) = match term.damage() {
        TermDamage::Full => (None, None),
        TermDamage::ScrollUp { lines, damage } => (
            Some(
                damage
                    .filter(|line| line.is_damaged() && line.line < size.rows)
                    .map(|line| line.line)
                    .collect::<Vec<_>>(),
            ),
            Some(lines),
        ),
        TermDamage::Partial(damage) => (
            Some(
                damage
                    .filter(|line| line.is_damaged() && line.line < size.rows)
                    .map(|line| line.line)
                    .collect::<Vec<_>>(),
            ),
            None,
        ),
    };
    term.reset_damage();

    if let (Some(scroll_up), Some(dirty_rows)) = (scroll_up, dirty_rows.as_deref())
        && shape_compatible
        && display_offset == 0
        && scroll_up < size.rows
    {
        return snapshot_from_term_after_scroll_up(
            term,
            size,
            graphics,
            scrollback_lines,
            scroll_up,
            dirty_rows,
            previous,
        );
    }

    let compatible = shape_compatible && previous.scrollback_lines == scrollback_lines;
    if !compatible || dirty_rows.is_none() || scroll_up.is_some() {
        return snapshot_from_term_with_display_offset(
            term,
            size,
            graphics,
            display_offset,
            size.rows,
        );
    }

    let mut snapshot = previous.clone();
    for row in dirty_rows.expect("partial terminal damage must contain row indexes") {
        let line_id = snapshot.lines[row].line_id;
        let mut next_row = snapshot_row_from_term(term, size, display_offset, row);
        next_row.line_id = line_id;
        snapshot.lines[row] = next_row;
    }
    refresh_snapshot_metadata(&mut snapshot, term, size, graphics, display_offset);
    snapshot
}

fn snapshot_from_term_after_scroll_up<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    scrollback_lines: usize,
    scroll_up: usize,
    dirty_rows: &[usize],
    previous: &TerminalSnapshot,
) -> TerminalSnapshot {
    let mut rebuild_rows = vec![false; size.rows];
    rebuild_rows[size.rows - scroll_up..].fill(true);
    for &dirty_row in dirty_rows {
        // Damage can be recorded on either side of one or more scrolls. Rebuilding every possible
        // destination keeps the transform correct without adding per-cell generations to parsing.
        let first_possible_row = dirty_row.saturating_sub(scroll_up);
        let last_possible_row = dirty_row.min(size.rows - 1);
        rebuild_rows[first_possible_row..=last_possible_row].fill(true);
    }

    let mut snapshot = previous.clone();
    snapshot.display_offset = 0;
    snapshot.scrollback_lines = scrollback_lines;
    for (row, rebuild) in rebuild_rows.into_iter().enumerate() {
        let previous_row = row + scroll_up;
        if !rebuild {
            snapshot.lines[row] = previous.lines[previous_row].clone();
            snapshot.lines[row].absolute_line = row as i64;
            continue;
        }

        let mut next_row = snapshot_row_from_term(term, size, 0, row);
        if previous_row < previous.lines.len() {
            next_row.line_id = previous.lines[previous_row].line_id;
        }
        snapshot.lines[row] = next_row;
    }
    refresh_snapshot_metadata(&mut snapshot, term, size, graphics, 0);
    snapshot
}

pub(crate) fn scroll_snapshot_from_term<T: EventListener>(
    term: &mut Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    delta: i32,
    previous: &TerminalSnapshot,
) -> TerminalSnapshot {
    if delta != 0 {
        term.scroll_display(Scroll::Delta(delta));
    }

    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let display_offset = term.grid().display_offset().min(scrollback_lines);
    let compatible = previous.cols == size.cols
        && previous.rows == size.rows
        && previous.scrollback_lines == scrollback_lines
        && previous.lines.len() == size.rows;
    let offset_distance = previous.display_offset.abs_diff(display_offset);
    if !compatible || offset_distance >= size.rows {
        let snapshot = snapshot_from_term_with_display_offset(
            term,
            size,
            graphics,
            display_offset,
            size.rows,
        );
        term.reset_damage();
        return snapshot;
    }

    let previous_row_index = |row: usize| {
        if display_offset >= previous.display_offset {
            row.checked_sub(offset_distance)
        } else {
            row.checked_add(offset_distance)
                .filter(|row| *row < previous.lines.len())
        }
    };
    let lines = (0..size.rows)
        .map(|row| {
            previous_row_index(row)
                .and_then(|previous_row| previous.lines.get(previous_row))
                .cloned()
                .unwrap_or_else(|| snapshot_row_from_term(term, size, display_offset, row))
        })
        .collect::<Vec<_>>();

    // The caller guarantees that the previous snapshot represented the terminal immediately
    // before this viewport-only scroll, so overlapping rows can retain their shared cell buffers.
    let mapped_cursor_row = if display_offset >= previous.display_offset {
        previous.cursor_row.checked_add(offset_distance)
    } else {
        previous.cursor_row.checked_sub(offset_distance)
    }
    .unwrap_or(usize::MAX);
    let mut snapshot = TerminalSnapshot {
        generation: 0,
        cols: size.cols,
        rows: size.rows,
        cursor_col: previous.cursor_col,
        cursor_row: mapped_cursor_row,
        cursor_shape: previous.cursor_shape,
        display_offset,
        scrollback_lines,
        lines,
        images: Vec::new(),
    };
    refresh_snapshot_metadata(&mut snapshot, term, size, graphics, display_offset);
    term.reset_damage();
    snapshot
}

pub(crate) fn snapshot_from_term_with_display_offset<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    display_offset: usize,
    rows: usize,
) -> TerminalSnapshot {
    let content = term.renderable_content();
    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let display_offset = display_offset.min(scrollback_lines);
    let requested_rows = rows.max(1);
    let mut rows = (0..requested_rows)
        .map(|row| snapshot_row_from_term(term, size, display_offset, row))
        .collect::<Vec<_>>();

    let cursor_row = (content.cursor.point.line.0 + display_offset as i32).max(0) as usize;
    let cursor_col = content.cursor.point.column.0;

    if cursor_row < rows.len() && cursor_col < size.cols {
        rows[cursor_row].cells_mut()[cursor_col].cursor = true;
        let active_input_rows = mark_active_input_rows(&mut rows, cursor_row);
        // Snapshot rows already carry content signatures. Only cursor and
        // active-input metadata changed after row construction.
        for row in &mut rows[active_input_rows] {
            row.refresh_signature();
        }
    }

    TerminalSnapshot {
        generation: 0,
        cols: size.cols,
        rows: size.rows,
        cursor_col,
        cursor_row,
        cursor_shape: content.cursor.shape.into(),
        display_offset,
        scrollback_lines,
        lines: rows,
        images: graphics.visible_images(display_offset, requested_rows),
    }
}

fn snapshot_row_from_term<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    display_offset: usize,
    row: usize,
) -> TerminalRow {
    let Some(source) = snapshot_row_source(term, size, display_offset, row) else {
        return blank_snapshot_row(size, display_offset, row);
    };
    snapshot_row_from_source(term, size, display_offset, row, source)
}

#[derive(Clone, Copy)]
struct SnapshotRowSource {
    source_id: usize,
    populated_cols: usize,
    wrapped: bool,
}

fn snapshot_row_source<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    display_offset: usize,
    row: usize,
) -> Option<SnapshotRowSource> {
    let grid_line = row as i32 - display_offset as i32;
    if grid_line < -(term.grid().history_size() as i32) || grid_line >= term.screen_lines() as i32 {
        return None;
    }

    let terminal_row = &term.grid()[Line(grid_line)];
    let terminal_cells = &terminal_row[..];
    let default_cell = AlacrittyCell::default();
    let populated_cols = terminal_cells
        .iter()
        .take(size.cols)
        .rposition(|cell| cell != &default_cell)
        .map_or(0, |column| column + 1);
    let visible_cells = &terminal_cells[..populated_cols];
    let wrapped = visible_cells
        .iter()
        .any(|cell| cell.flags.contains(Flags::WRAPLINE));

    Some(SnapshotRowSource {
        // The cell allocation follows the row content when Alacritty rotates or swaps row values.
        source_id: terminal_cells.as_ptr() as usize,
        populated_cols,
        wrapped,
    })
}

fn snapshot_row_from_source<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    display_offset: usize,
    row: usize,
    source: SnapshotRowSource,
) -> TerminalRow {
    let grid_line = row as i32 - display_offset as i32;
    let terminal_row = &term.grid()[Line(grid_line)];
    let terminal_cells = &terminal_row[..];
    let mut cells = Vec::with_capacity(size.cols);
    // Default trailing cells have fixed paint data, so skip color and metadata conversion.
    for cell in &terminal_cells[..source.populated_cols] {
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            cells.push(blank_terminal_cell());
            continue;
        }
        let ch = if cell.c == '\0' { ' ' } else { cell.c };
        let attrs = attrs_from_flags(cell.flags);
        let (fg, bg) = style_colors_for_cell(cell.fg, cell.bg, ch, attrs);
        let style_origin = style_origin_for_cell(cell.fg, cell.bg, attrs);
        let zerowidth = cell.zerowidth().into_iter().flatten().copied().collect();
        let hyperlink = cell
            .hyperlink()
            .map(|hyperlink| hyperlink.uri().to_string());
        let mut snapshot_cell = TerminalCell {
            ch,
            wide: cell.flags.contains(Flags::WIDE_CHAR),
            fg,
            bg,
            style_origin,
            attrs,
            extra: None,
            cursor: false,
        };
        snapshot_cell.set_extra(zerowidth, hyperlink);
        cells.push(snapshot_cell);
    }
    cells.resize(size.cols, blank_terminal_cell());
    let mut snapshot_row = TerminalRow {
        line_id: 0,
        source_id: source.source_id,
        absolute_line: i64::from(grid_line),
        wrapped: source.wrapped,
        active_input: false,
        signature: 0,
        cells: Arc::new(cells),
    };
    snapshot_row.refresh_signature();
    snapshot_row
}

fn blank_snapshot_row(size: TerminalSize, display_offset: usize, row: usize) -> TerminalRow {
    let mut snapshot_row = TerminalRow {
        line_id: 0,
        source_id: 0,
        absolute_line: row as i64 - display_offset as i64,
        wrapped: false,
        active_input: false,
        signature: 0,
        cells: Arc::new(vec![blank_terminal_cell(); size.cols]),
    };
    snapshot_row.refresh_signature();
    snapshot_row
}

fn blank_terminal_cell() -> TerminalCell {
    TerminalCell {
        ch: ' ',
        wide: false,
        fg: OXIDETERM_DARK_THEME.foreground,
        bg: OXIDETERM_DARK_THEME.ansi_background,
        style_origin: TerminalStyleOrigin::default(),
        attrs: TerminalAttrs::default(),
        extra: None,
        cursor: false,
    }
}

fn refresh_snapshot_metadata<T: EventListener>(
    snapshot: &mut TerminalSnapshot,
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    display_offset: usize,
) {
    let content = term.renderable_content();
    let cursor_row = (content.cursor.point.line.0 + display_offset as i32).max(0) as usize;
    let cursor_col = content.cursor.point.column.0;
    let mut metadata_rows = Vec::new();
    let cursor_position_changed = snapshot.cursor_row != cursor_row || snapshot.cursor_col != cursor_col;
    if cursor_position_changed
        && snapshot.cursor_row < snapshot.lines.len()
        && snapshot.cursor_col < size.cols
    {
        let previous_cursor_row = snapshot.cursor_row;
        if snapshot.lines[previous_cursor_row].cells[snapshot.cursor_col].cursor {
            snapshot.lines[previous_cursor_row].cells_mut()[snapshot.cursor_col].cursor = false;
            metadata_rows.push(previous_cursor_row);
        }
    }
    let active_input_rows = if cursor_row < snapshot.lines.len() && cursor_col < size.cols {
        active_input_row_range(&snapshot.lines, cursor_row)
    } else {
        0..0
    };
    for (row_index, row) in snapshot.lines.iter_mut().enumerate() {
        let active_input = active_input_rows.contains(&row_index);
        if row.active_input != active_input {
            row.active_input = active_input;
            metadata_rows.push(row_index);
        }
    }
    if cursor_row < snapshot.lines.len()
        && cursor_col < size.cols
        && !snapshot.lines[cursor_row].cells[cursor_col].cursor
    {
        snapshot.lines[cursor_row].cells_mut()[cursor_col].cursor = true;
        metadata_rows.push(cursor_row);
    }
    snapshot.cursor_col = cursor_col;
    snapshot.cursor_row = cursor_row;
    snapshot.cursor_shape = content.cursor.shape.into();
    snapshot.images = graphics.visible_images(display_offset, snapshot.lines.len());
    // Damaged rows already carry fresh signatures. Rehash only rows whose cursor or active-input
    // metadata changed instead of walking every visible cell after a small terminal update.
    metadata_rows.sort_unstable();
    metadata_rows.dedup();
    for row_index in metadata_rows {
        snapshot.lines[row_index].refresh_signature();
    }
}

#[cfg(test)]
mod incremental_snapshot_tests {
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    use super::*;

    fn assert_snapshot_content_eq(actual: &TerminalSnapshot, expected: &TerminalSnapshot) {
        assert_eq!(actual.cols, expected.cols);
        assert_eq!(actual.rows, expected.rows);
        assert_eq!(actual.cursor_col, expected.cursor_col);
        assert_eq!(actual.cursor_row, expected.cursor_row);
        assert_eq!(actual.cursor_shape, expected.cursor_shape);
        assert_eq!(actual.display_offset, expected.display_offset);
        assert_eq!(actual.scrollback_lines, expected.scrollback_lines);
        assert_eq!(actual.lines.len(), expected.lines.len());
        for (actual, expected) in actual.lines.iter().zip(&expected.lines) {
            assert_eq!(actual.source_id, expected.source_id);
            assert_eq!(actual.absolute_line, expected.absolute_line);
            assert_eq!(actual.cells, expected.cells);
            assert_eq!(actual.wrapped, expected.wrapped);
            assert_eq!(actual.active_input, expected.active_input);
            assert_eq!(actual.signature, expected.signature);
        }
        assert_eq!(actual.images, expected.images);
    }

    #[test]
    fn incremental_snapshot_reuses_undamaged_rows() {
        let size = TerminalSize {
            cols: 8,
            rows: 3,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let mut term = Term::new(Config::default(), &size, listener);
        let graphics = TerminalGraphicsState::default();
        let previous = snapshot_from_term(&term, size, &graphics);
        term.reset_damage();

        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"changed");
        let next = incremental_snapshot_from_term(&mut term, size, &graphics, &previous);
        let full = snapshot_from_term(&term, size, &graphics);

        assert!(!Arc::ptr_eq(
            &previous.lines[0].cells,
            &next.lines[0].cells
        ));
        assert!(Arc::ptr_eq(
            &previous.lines[2].cells,
            &next.lines[2].cells
        ));
        assert_snapshot_content_eq(&next, &full);
    }

    #[test]
    fn snapshot_source_identity_follows_output_rows_during_scroll() {
        let size = TerminalSize {
            cols: 8,
            rows: 3,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let mut term = Term::new(Config::default(), &size, listener);
        let graphics = TerminalGraphicsState::default();
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"one\r\ntwo\r\nthree\r\nfour");
        let previous = snapshot_from_term(&term, size, &graphics);

        parser.advance(&mut term, b"\r\nfive");
        let next = snapshot_from_term(&term, size, &graphics);

        assert_eq!(next.lines[0].source_id, previous.lines[1].source_id);
        assert_eq!(next.lines[1].source_id, previous.lines[2].source_id);
    }

    #[test]
    fn incremental_snapshot_keeps_content_written_before_scroll() {
        let size = TerminalSize {
            cols: 16,
            rows: 3,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let mut term = Term::new(Config::default(), &size, listener);
        let graphics = TerminalGraphicsState::default();
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"one\r\ntwo\r\nprompt");
        let previous = snapshot_from_term(&term, size, &graphics);
        term.reset_damage();

        // Output commonly completes the active row before the following linefeed scrolls it.
        parser.advance(&mut term, b"-completed\r\nnext");
        let next = incremental_snapshot_from_term(&mut term, size, &graphics, &previous);
        let full = snapshot_from_term(&term, size, &graphics);

        assert!(next.lines[1].text().starts_with("prompt-completed"));
        assert_snapshot_content_eq(&next, &full);
    }

    #[test]
    fn incremental_snapshot_reuses_unchanged_rows_after_output_scroll() {
        let size = TerminalSize {
            cols: 16,
            rows: 3,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let mut term = Term::new(Config::default(), &size, listener);
        let graphics = TerminalGraphicsState::default();
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"one\r\ntwo\r\nprompt");
        let initial = snapshot_from_term(&term, size, &graphics);
        // Consume startup full damage so this assertion covers the steady output path.
        let previous = incremental_snapshot_from_term(&mut term, size, &graphics, &initial);

        parser.advance(&mut term, b"-completed\r\nnext");
        let next = incremental_snapshot_from_term(&mut term, size, &graphics, &previous);
        let full = snapshot_from_term(&term, size, &graphics);

        assert!(Arc::ptr_eq(
            &previous.lines[1].cells,
            &next.lines[0].cells
        ));
        assert!(!Arc::ptr_eq(
            &previous.lines[2].cells,
            &next.lines[1].cells
        ));
        assert_snapshot_content_eq(&next, &full);
    }

    #[test]
    fn incremental_snapshot_preserves_rows_across_multiple_output_scrolls() {
        let size = TerminalSize {
            cols: 16,
            rows: 5,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let mut term = Term::new(Config::default(), &size, listener);
        let graphics = TerminalGraphicsState::default();
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"zero\r\none\r\ntwo\r\nthree\r\nprompt");
        let initial = snapshot_from_term(&term, size, &graphics);
        let previous = incremental_snapshot_from_term(&mut term, size, &graphics, &initial);

        parser.advance(&mut term, b"-completed\r\nnext-a\r\nnext-b");
        let next = incremental_snapshot_from_term(&mut term, size, &graphics, &previous);
        let full = snapshot_from_term(&term, size, &graphics);

        assert!(Arc::ptr_eq(
            &previous.lines[2].cells,
            &next.lines[0].cells
        ));
        assert!(Arc::ptr_eq(
            &previous.lines[3].cells,
            &next.lines[1].cells
        ));
        assert!(next.lines[2].text().starts_with("prompt-completed"));
        assert_snapshot_content_eq(&next, &full);
    }

    #[test]
    fn scroll_snapshot_reuses_overlapping_viewport_rows() {
        let size = TerminalSize {
            cols: 8,
            rows: 3,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let mut term = Term::new(Config::default(), &size, listener);
        let graphics = TerminalGraphicsState::default();
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"one\r\ntwo\r\nthree\r\nfour");
        let previous = snapshot_from_term(&term, size, &graphics);

        let next = scroll_snapshot_from_term(&mut term, size, &graphics, 1, &previous);
        let full = snapshot_from_term(&term, size, &graphics);

        assert!(Arc::ptr_eq(
            &previous.lines[0].cells,
            &next.lines[1].cells
        ));
        assert_snapshot_content_eq(&next, &full);
    }

    #[test]
    fn incremental_snapshot_clears_the_previous_cursor_cell() {
        let size = TerminalSize {
            cols: 8,
            rows: 3,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let mut term = Term::new(Config::default(), &size, listener);
        let graphics = TerminalGraphicsState::default();
        let previous = snapshot_from_term(&term, size, &graphics);
        term.reset_damage();

        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"\r\n");
        let next = incremental_snapshot_from_term(&mut term, size, &graphics, &previous);

        assert!(!next.lines[0].cells[0].cursor);
        assert!(next.lines[1].cells[0].cursor);
        assert_snapshot_content_eq(&next, &snapshot_from_term(&term, size, &graphics));
    }

    #[test]
    fn search_source_honors_cancellation_and_finds_completed_work() {
        let size = TerminalSize {
            cols: 16,
            rows: 3,
            cell_width: 0,
            cell_height: 0,
        };
        let (listener, _events) = local_event_channel();
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &size,
            listener,
        )));
        let mut parser = Processor::<StdSyncHandler>::new();
        {
            let mut term = term.lock();
            parser.advance(&mut *term, b"prefix needle suffix");
        }
        let source = TerminalSearchSource::new(term, size.cols);

        assert!(source.search_matches("needle", &|| true).is_empty());
        assert_eq!(source.search_matches("needle", &|| false).len(), 1);
    }
}

fn mark_active_input_rows(
    rows: &mut [TerminalRow],
    cursor_row: usize,
) -> std::ops::Range<usize> {
    let active_rows = active_input_row_range(rows, cursor_row);
    for row in &mut rows[active_rows.clone()] {
        row.active_input = true;
    }
    active_rows
}

fn active_input_row_range(
    rows: &[TerminalRow],
    cursor_row: usize,
) -> std::ops::Range<usize> {
    let mut start = cursor_row;
    while start > 0 && rows.get(start - 1).is_some_and(|row| row.wrapped) {
        start -= 1;
    }

    let mut end = cursor_row + 1;
    while end < rows.len() && rows.get(end - 1).is_some_and(|row| row.wrapped) {
        end += 1;
    }
    start..end
}
