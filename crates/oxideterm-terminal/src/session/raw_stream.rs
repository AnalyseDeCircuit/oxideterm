// Method bodies shared verbatim by the raw byte-stream sessions.
//
// `SerialSession` and `BleSession` drive the same terminal pipeline over
// different transports, so every method here must stay transport-agnostic:
// it may only touch session fields that both structs own with identical
// types, and helpers that exist on both sessions. Transport-specific
// behavior (worker event pumping, send paths, lifecycle teardown) stays in
// each session file. A change made here applies to both sessions at once,
// which is the point: the copies used to drift independently.
//
// `session.rs` includes this file textually before `serial.rs` and
// `ble.rs`, so the macros are in scope for both sessions.

/// TerminalSessionBackend methods shared verbatim by SerialSession and BleSession.
macro_rules! raw_stream_backend_methods {
() => {
    fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.event_rx.activity_receiver()
    }

    fn buffer_text(&self) -> String {
        let term = self.term.lock();
        terminal_buffer_text_from_term(&term, self.resize.cols)
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

    fn finish_modem_transfer(&mut self) {
        let trailing_output = self.modem_consumer.finish_transfer();
        self.feed_plain_transport_output(&trailing_output);
    }

    fn interrupt_modem_transfer(&mut self) {
        self.modem_consumer.interrupt_transfer();
    }

    fn kill_active_task(&mut self) -> Result<()> {
        self.write_protocol_bytes(b"\x03")
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn page_down(&mut self) {
        self.term.lock().scroll_display(Scroll::PageDown);
    }

    fn page_up(&mut self) {
        self.term.lock().scroll_display(Scroll::PageUp);
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        let bytes = self
            .input_encoder
            .encode_paste(text, self.mode().contains(TermMode::BRACKETED_PASTE));
        self.write_input(&bytes)
    }

    fn pending_output_flush_delay(&self) -> Option<Duration> {
        self.modem_consumer.pending_plain_output_delay()
    }

    fn process_info(&self) -> TerminalProcessInfo {
        TerminalProcessInfo::default()
    }

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

    fn refresh_process_info(&mut self) {}

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

    fn scroll_to_top(&mut self) {
        self.term.lock().scroll_display(Scroll::Top);
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

    fn selection(&self) -> Option<crate::TerminalSelectionRange> {
        crate::selection::term_selection(&self.term.lock())
    }

    fn serial_runtime_options(&self) -> Option<SerialRuntimeOptions> {
        Some(self.runtime_options)
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

    fn set_output_events_enabled(&mut self, enabled: bool) {
        self.output_events_enabled = enabled;
    }

    fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.output_processor = processor;
        self.output_decoder.reset();
        self.encoding_detector.set_encoding(self.encoding);
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

    fn set_selection(&self, selection: Option<crate::TerminalSelectionRange>) {
        crate::selection::set_term_selection(&mut self.term.lock(), selection);
    }

    fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.trigger_stream = rules.map(oxideterm_terminal_triggers::TerminalTriggerStream::new);
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

    fn start_modem_transfer(
        &mut self,
        request: TerminalModemTransferRequest,
    ) -> Option<ModemTransfer> {
        self.modem_consumer.start_manual_transfer(request)
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        self.write_protocol_bytes(b"\x03")
    }

    fn title(&self) -> Option<String> {
        Some(self.title.clone().unwrap_or_else(|| self.title_text()))
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        let encoded = self.encode_user_input(bytes)?;
        self.write_protocol_bytes(&encoded)?;
        if self.runtime_options.local_echo {
            // Local echo is pane feedback, never device output: keep it out of
            // recording and file consumers like the serial backend does.
            self.feed_display_output(&encoded, false);
        }
        Ok(())
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        let encoded = self.input_encoder.encode_text(text);
        self.write_input(encoded.as_ref())
    }

    };
}

/// Inherent byte-stream methods shared verbatim by SerialSession and BleSession.
macro_rules! raw_stream_inherent_methods {
() => {
    fn feed_plain_transport_output(&mut self, bytes: &[u8]) {
        self.feed_display_output(bytes, true);
    }

    fn feed_transport_output(&mut self, bytes: &[u8]) {
        let events = self.modem_consumer.process_server_output(bytes);
        self.handle_modem_consumer_events(events);
    }

    fn feed_utf8_terminal_output(&mut self, bytes: &[u8]) {
        self.push_output_event(bytes);
        let mut term = self.term.lock();
        self.shell_integration
            .advance(&mut self.parser, &mut *term, bytes, |event| {
                self.pending_events.push(event);
            });
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
                self.pending_events.push(TerminalEvent::ClipboardStore(text));
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

    fn encode_user_input(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        match self.runtime_options.send_mode {
            SerialSendMode::Text => Ok(encode_text_line_endings(
                bytes,
                self.runtime_options.line_ending,
            )),
            SerialSendMode::Hex => parse_serial_hex_input(bytes),
        }
    }
    fn prepare_display_output(&mut self, bytes: &[u8]) -> Vec<u8> {
        match self.runtime_options.display_mode {
            SerialDisplayMode::Text => {
                // Serial ingress pairs: LineEndingIngress normalizes CR/LF,
                // SerialConsoleIngress preserves OSC selectors while escaping
                // invalid OSC prefixes and unsupported string controls in boot
                // noise. BLE-UART bridges produce the same console traffic.
                let normalized = self
                    .line_ending_ingress
                    .normalize(bytes, self.runtime_options.output_line_ending);
                self.serial_console_ingress.filter(&normalized)
            }
            SerialDisplayMode::Hex => {
                format_serial_hexdump(bytes, &mut self.hexdump_offset, false)
            }
            SerialDisplayMode::Mixed => {
                format_serial_hexdump(bytes, &mut self.hexdump_offset, true)
            }
        }
    }

    fn flush_buffered_modem_output(&mut self) -> bool {
        // The serial session owns the undecided bytes and releases them at the
        // maintenance deadline or when the device can no longer send a suffix.
        let events = if self.lifecycle.is_running() {
            self.modem_consumer.flush_expired_plain_output()
        } else {
            self.modem_consumer.flush_pending_plain_output()
        };
        let changed = !events.is_empty();
        self.handle_modem_consumer_events(events);
        changed
    }

    };
}
