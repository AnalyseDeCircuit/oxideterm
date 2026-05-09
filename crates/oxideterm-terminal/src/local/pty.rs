pub struct LocalPtySession {
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    notifier: LocalGraphicsNotifier,
    event_rx: Receiver<AlacEvent>,
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
    graphics: TerminalGraphicsState,
    encoding: TerminalEncoding,
    input_encoder: TerminalInputEncoder,
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
        #[cfg(target_os = "windows")]
        let shell_args = powershell_init_args(&local_config, &shell)
            .unwrap_or_else(|| shell_args_for_profile(&shell, local_config.load_profile));
        #[cfg(not(target_os = "windows"))]
        let shell_args = shell_args_for_profile(&shell, local_config.load_profile);

        let (event_tx, event_rx) = unbounded();
        let (graphics_tx, graphics_rx) = unbounded();
        let (magic_tx, magic_rx) = unbounded();
        let (terminal_event_tx, terminal_event_rx) = unbounded();
        let (stats_tx, stats_rx) = unbounded();

        let mut terminal_config = Config::default();
        terminal_config.scrolling_history = scrollback_lines;
        terminal_config.kitty_keyboard = true;

        let listener = LocalEventListener {
            tx: event_tx.clone(),
        };
        let term = Arc::new(FairMutex::new(Term::new(
            terminal_config,
            &size,
            listener.clone(),
        )));
        let cwd = local_config
            .cwd
            .clone()
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
                env: oxideterm_terminal_env(&local_config, &shell),
                #[cfg(target_os = "windows")]
                escape_args: false,
            },
            window_size(size),
            0,
        )
        .context("failed to spawn local shell PTY")?;
        let shell_pid = Some(pty.child().id());
        let pty_master = pty.file().try_clone().ok();
        let process = ProcessState::new(shell_pid, pty_master, cwd);
        let event_loop = LocalGraphicsEventLoop::new(
            term.clone(),
            listener,
            pty,
            true,
            graphics_tx,
            magic_tx,
            terminal_event_tx,
            stats_tx,
            size,
            graphics_options,
            encoding,
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
            graphics: TerminalGraphicsState::default(),
            encoding,
            input_encoder: TerminalInputEncoder::new(encoding),
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
        while report.events_drained < budget.max_events {
            let Ok(stats) = self.stats_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            report.drained_bytes = report.drained_bytes.saturating_add(stats.raw_bytes);
            report.budget_exhausted |= stats.budget_exhausted;
        }

        while report.events_drained < budget.max_events {
            let Ok(event) = self.event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if self.handle_alacritty_event(event) {
                changed = true;
            }
        }

        while report.events_drained < budget.max_events {
            let Ok(event) = self.graphics_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if let Some(response) = self.graphics.handle_event(event) {
                let _ = self.write_protocol_bytes(&response);
            }
            changed = true;
        }

        while report.events_drained < budget.max_events {
            let Ok(kind) = self.magic_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            self.pending_events.push(TerminalEvent::MagicDetected(kind));
        }

        while report.events_drained < budget.max_events {
            let Ok(event) = self.terminal_event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            self.pending_events.push(event);
        }

        report.changed = changed;
        report.budget_exhausted |= report.events_drained >= budget.max_events
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
        self.write_protocol_bytes(bytes)
    }

    pub fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.notifier.notify(Cow::Owned(bytes.to_vec()));
        }
        Ok(())
    }

    pub fn write_text(&mut self, text: &str) -> Result<()> {
        let encoded = self.input_encoder.encode_text(text);
        self.write_protocol_bytes(encoded.as_ref())
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

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    pub fn process_info(&self) -> TerminalProcessInfo {
        self.process.info.clone()
    }

    pub fn refresh_process_info(&mut self) {
        if self.lifecycle.is_running() {
            self.process.refresh();
        }
    }

    pub fn terminate_active_task(&mut self) -> Result<()> {
        self.signal_active_task(TerminalSignal::Terminate)
    }

    pub fn kill_active_task(&mut self) -> Result<()> {
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
        self.write_protocol_bytes(&bytes)
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn set_focused(&mut self, focused: bool) -> Result<()> {
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
            AlacEvent::ColorRequest(index, formatter) => {
                let override_color = (index <= 268)
                    .then(|| self.term.lock().colors()[index])
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
                let code = status.code();
                self.lifecycle = TerminalLifecycle::Exited(code);
                self.process.mark_exited();
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
            let _ = self.notifier.0.send(LocalGraphicsMsg::Shutdown);
            self.detach_io_thread();
        }

        self.lifecycle = TerminalLifecycle::Closed;
        self.process.mark_exited();
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
        self.size = next;
        Ok(())
    }

    pub fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.term.lock().scroll_display(Scroll::Delta(delta));
        }
    }

    pub fn page_up(&mut self) {
        self.term.lock().scroll_display(Scroll::PageUp);
    }

    pub fn page_down(&mut self) {
        self.term.lock().scroll_display(Scroll::PageDown);
    }

    pub fn scroll_to_top(&mut self) {
        self.term.lock().scroll_display(Scroll::Top);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    pub fn scroll_to_display_offset(&mut self, offset: usize) {
        let mut term = self.term.lock();
        let max_offset = term.total_lines().saturating_sub(term.screen_lines());
        let target = offset.min(max_offset);
        let current = term.grid().display_offset();
        let delta = target as i32 - current as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    pub fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
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
                self.size.cols,
                &mut logical_text,
                &mut logical_map,
            );

            let wrapped = row
                .last()
                .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE));
            if wrapped && line + 1 < bottom_line {
                continue;
            }

            matches.extend(search_logical_line_matches(
                &logical_text,
                &logical_map,
                query,
                self.size.cols,
            ));
            logical_text.clear();
            logical_map.clear();
        }

        matches
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let term = self.term.lock();
        snapshot_from_term(&term, self.size, &self.graphics)
    }
}

pub(crate) fn snapshot_from_term<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
) -> TerminalSnapshot {
    let content = term.renderable_content();
    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let mut rows = vec![
        TerminalRow {
            wrapped: false,
            active_input: false,
            cells: vec![
                TerminalCell {
                    ch: ' ',
                    zerowidth: String::new(),
                    wide: false,
                    fg: OXIDETERM_DARK_THEME.foreground,
                    bg: OXIDETERM_DARK_THEME.ansi_background,
                    attrs: TerminalAttrs::default(),
                    hyperlink: None,
                    cursor: false,
                };
                size.cols
            ],
        };
        size.rows
    ];

    for indexed in content.display_iter {
        let Some(row) = viewport_row_for_grid_line(indexed.point.line.0, content.display_offset)
        else {
            continue;
        };

        let col = indexed.point.column.0;
        if row >= size.rows || col >= size.cols {
            continue;
        }

        let cell: &Cell = &indexed.cell;
        if cell.flags.contains(Flags::WRAPLINE) {
            rows[row].wrapped = true;
        }

        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }

        let mut ch = cell.c;
        if ch == '\0' {
            ch = ' ';
        }

        let attrs = attrs_from_flags(cell.flags);
        let (fg, bg) = style_colors_for_cell(cell.fg, cell.bg, ch, attrs);

        rows[row].cells[col] = TerminalCell {
            ch,
            zerowidth: cell.zerowidth().into_iter().flatten().copied().collect(),
            wide: cell.flags.contains(Flags::WIDE_CHAR),
            fg,
            bg,
            attrs,
            hyperlink: cell
                .hyperlink()
                .map(|hyperlink| hyperlink.uri().to_string()),
            cursor: false,
        };
    }

    let cursor_row = (content.cursor.point.line.0 + content.display_offset as i32).max(0) as usize;
    let cursor_col = content.cursor.point.column.0;

    if cursor_row < rows.len() && cursor_col < size.cols {
        rows[cursor_row].cells[cursor_col].cursor = true;
        mark_active_input_rows(&mut rows, cursor_row);
    }

    TerminalSnapshot {
        cols: size.cols,
        rows: size.rows,
        cursor_col,
        cursor_row,
        cursor_shape: content.cursor.shape.into(),
        display_offset: content.display_offset,
        scrollback_lines,
        lines: rows,
        images: graphics.visible_images(content.display_offset, size.rows),
    }
}

fn mark_active_input_rows(rows: &mut [TerminalRow], cursor_row: usize) {
    let mut start = cursor_row;
    while start > 0 && rows.get(start - 1).is_some_and(|row| row.wrapped) {
        start -= 1;
    }

    let mut end = cursor_row + 1;
    while end < rows.len() && rows.get(end - 1).is_some_and(|row| row.wrapped) {
        end += 1;
    }

    for row in &mut rows[start..end] {
        row.active_input = true;
    }
}
