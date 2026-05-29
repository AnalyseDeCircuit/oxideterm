struct PlaybackTerminalSession {
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    listener: LocalEventListener,
    parser: Processor,
    event_rx: Receiver<AlacEvent>,
    pending_events: Vec<TerminalEvent>,
    size: TerminalSize,
    graphics_options: GraphicsOptions,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    shell_integration: TerminalShellIntegration,
    scrollback_lines: usize,
}

impl PlaybackTerminalSession {
    fn new(
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        scrollback_lines: usize,
    ) -> Self {
        let (event_tx, event_rx) = unbounded();
        let listener = LocalEventListener { tx: event_tx };
        let size = TerminalSize {
            cols: cols.max(2),
            rows: rows.max(2),
            cell_width: 0,
            cell_height: 0,
        };
        let term = Arc::new(FairMutex::new(Term::new(
            playback_config(scrollback_lines),
            &size,
            listener.clone(),
        )));
        Self {
            term,
            listener,
            parser: Processor::new(),
            event_rx,
            pending_events: Vec::new(),
            size,
            graphics_options: graphics_options.clone(),
            graphics_ingress: GraphicsIngress::new(graphics_options),
            graphics: TerminalGraphicsState::default(),
            shell_integration: TerminalShellIntegration::default(),
            scrollback_lines,
        }
    }

    fn reset(&mut self, cols: usize, rows: usize) {
        self.size.cols = cols.max(2);
        self.size.rows = rows.max(2);
        self.term = Arc::new(FairMutex::new(Term::new(
            playback_config(self.scrollback_lines),
            &self.size,
            self.listener.clone(),
        )));
        self.parser = Processor::new();
        self.graphics_ingress = GraphicsIngress::new(self.graphics_options.clone());
        self.graphics = TerminalGraphicsState::default();
        self.shell_integration = TerminalShellIntegration::default();
        self.pending_events.clear();
        while self.event_rx.try_recv().is_ok() {}
    }

    fn feed_output(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut term = self.term.lock();
        let cursor = Cell::new(graphics_cursor_from_term(&term, self.size));
        let events = self.graphics_ingress.advance_with(
            bytes,
            |terminal_bytes| {
                self.shell_integration.advance(
                    &mut self.parser,
                    &mut *term,
                    terminal_bytes,
                    |event| self.pending_events.push(event),
                );
                cursor.set(graphics_cursor_from_term(&term, self.size));
            },
            || cursor.get(),
        );
        drop(term);
        for event in events {
            if let Some(response) = self.graphics.handle_event(event) {
                let _ = response;
            }
        }
    }

    fn drain_alacritty_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AlacEvent::Title(title) => {
                    self.pending_events.push(TerminalEvent::TitleChanged(title));
                }
                AlacEvent::ResetTitle => {
                    self.pending_events.push(TerminalEvent::TitleReset);
                }
                AlacEvent::Bell => {
                    self.pending_events.push(TerminalEvent::Bell);
                }
                AlacEvent::Wakeup | AlacEvent::MouseCursorDirty => {
                    self.pending_events.push(TerminalEvent::Wakeup);
                    changed = true;
                }
                AlacEvent::CursorBlinkingChange => {
                    let blinking = self.term.lock().cursor_style().blinking;
                    self.pending_events
                        .push(TerminalEvent::BlinkChanged(blinking));
                    changed = true;
                }
                AlacEvent::ClipboardStore(_, text) => {
                    self.pending_events.push(TerminalEvent::ClipboardStore(text));
                }
                AlacEvent::ClipboardLoad(_format, callback) => {
                    self.pending_events
                        .push(TerminalEvent::ClipboardLoad(Arc::new(move |text| {
                            callback(text.into())
                        })));
                }
                AlacEvent::PtyWrite(_) => {}
                _ => {}
            }
        }
        changed
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
                self.size.cols,
                &mut logical_text,
                &mut logical_map,
            );

            let wrapped = row
                .last()
                .is_some_and(|cell| cell.flags.contains(alacritty_terminal::term::cell::Flags::WRAPLINE));
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
}

impl TerminalSessionBackend for PlaybackTerminalSession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::LocalPty
    }

    fn title(&self) -> Option<String> {
        Some("Recording".to_string())
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        TerminalLifecycle::Exited(Some(0))
    }

    fn is_interactive(&self) -> bool {
        false
    }

    fn process_info(&self) -> TerminalProcessInfo {
        TerminalProcessInfo::default()
    }

    fn refresh_process_info(&mut self) {}

    fn read_pending(&mut self) -> bool {
        self.drain_alacritty_events()
    }

    fn read_pending_with_budget(&mut self, _budget: TerminalDrainBudget) -> TerminalDrainReport {
        let changed = self.drain_alacritty_events();
        let mut report = TerminalDrainReport::default();
        if changed {
            report.mark_changed();
        }
        report
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        self.drain_alacritty_events();
        std::mem::take(&mut self.pending_events)
    }

    fn write_input(&mut self, _bytes: &[u8]) -> Result<()> {
        Ok(())
    }

    fn write_protocol_bytes(&mut self, _bytes: &[u8]) -> Result<()> {
        Ok(())
    }

    fn write_text(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }

    fn paste_text(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }

    fn set_encoding(&mut self, _encoding: TerminalEncoding) {}

    fn feed_recording_output(&mut self, bytes: &[u8]) {
        self.feed_output(bytes);
    }

    fn reset_recording_playback(&mut self, cols: usize, rows: usize) {
        self.reset(cols, rows);
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn set_focused(&mut self, _focused: bool) -> Result<()> {
        Ok(())
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        let next = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        if next.cols != self.size.cols || next.rows != self.size.rows {
            self.term.lock().resize(next);
        }
        self.size = next;
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
        PlaybackTerminalSession::search_matches(self, query)
    }

    fn clear_buffer(&mut self) {
        let mut term = self.term.lock();
        clear_terminal_buffer(&mut term);
    }

    fn buffer_text(&self) -> String {
        let term = self.term.lock();
        terminal_buffer_text_from_term(&term, self.size.cols)
    }

    fn snapshot(&self) -> TerminalSnapshot {
        let term = self.term.lock();
        snapshot_from_term(&term, self.size, &self.graphics)
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        Ok(())
    }

    fn kill_active_task(&mut self) -> Result<()> {
        Ok(())
    }

    fn shutdown(&mut self) {}
}

fn playback_config(scrollback_lines: usize) -> Config {
    let mut config = Config::default();
    config.scrolling_history = scrollback_lines;
    config.kitty_keyboard = true;
    config
}
