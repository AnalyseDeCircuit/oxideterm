impl TerminalPane {
    pub fn recording_status(&self) -> TerminalRecordingStatus {
        self.recorder
            .as_ref()
            .map(TerminalRecorder::status)
            .unwrap_or_default()
    }

    pub fn start_recording(&mut self, title: Option<String>, cx: &mut Context<Self>) {
        let options = TerminalRecordingOptions {
            title,
            capture_input: false,
            theme: Some(TerminalRecordingTheme {
                fg: hex_color(self.theme.foreground),
                bg: hex_color(self.theme.background),
            }),
        };
        self.recorder = Some(TerminalRecorder::start(
            self.snapshot.cols,
            self.snapshot.rows,
                options,
            ));
        self.set_recording_output_events_enabled(true);
        cx.notify();
    }

    pub fn pause_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.pause();
            self.set_recording_output_events_enabled(false);
            cx.notify();
        }
    }

    pub fn resume_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.resume();
            self.set_recording_output_events_enabled(true);
            cx.notify();
        }
    }

    pub fn discard_recording(&mut self, cx: &mut Context<Self>) {
        if self.recorder.take().is_some() {
            self.set_recording_output_events_enabled(false);
            cx.notify();
        }
    }

    pub fn stop_recording(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let recorder = self.recorder.take()?;
        self.set_recording_output_events_enabled(false);
        cx.notify();
        Some(recorder.stop())
    }

    pub fn reset_recording_playback(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.reset_recording_playback(cols, rows);
            terminal.snapshot()
        };
        self.snapshot = self.stamp_snapshot(snapshot);
        self.selection = None;
        self.search_query = None;
        self.selected_search_match = None;
        cx.notify();
    }

    pub fn feed_recording_output(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.feed_recording_output(bytes);
            let _ = terminal.take_events();
            terminal.snapshot()
        };
        self.snapshot = self.stamp_snapshot(snapshot);
        cx.notify();
    }

    pub fn resize_recording_playback(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let snapshot = {
            let mut terminal = self.terminal.lock();
            let _ = terminal.resize_with_cell_size(cols, rows, 0, 0);
            terminal.snapshot()
        };
        self.snapshot = self.stamp_snapshot(snapshot);
        cx.notify();
    }

    fn set_recording_output_events_enabled(&mut self, enabled: bool) {
        // Output events duplicate decoded terminal bytes and are only consumed by
        // TerminalRecorder, so keep them disabled outside active recording.
        self.terminal
            .lock()
            .set_output_events_enabled(enabled);
    }

}
