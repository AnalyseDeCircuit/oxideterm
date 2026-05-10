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
        cx.notify();
    }

    pub fn pause_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.pause();
            cx.notify();
        }
    }

    pub fn resume_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.resume();
            cx.notify();
        }
    }

    pub fn discard_recording(&mut self, cx: &mut Context<Self>) {
        if self.recorder.take().is_some() {
            cx.notify();
        }
    }

    pub fn stop_recording(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let recorder = self.recorder.take()?;
        cx.notify();
        Some(recorder.stop())
    }

    pub fn reset_recording_playback(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        self.terminal.lock().reset_recording_playback(cols, rows);
        self.selection = None;
        self.search_query = None;
        self.selected_search_match = None;
        self.snapshot = self.terminal.lock().snapshot();
        cx.notify();
    }

    pub fn feed_recording_output(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.terminal.lock().feed_recording_output(bytes);
        let _ = self.terminal.lock().take_events();
        self.snapshot = self.terminal.lock().snapshot();
        cx.notify();
    }

    pub fn resize_recording_playback(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let _ = self.terminal.lock().resize_with_cell_size(cols, rows, 0, 0);
        self.snapshot = self.terminal.lock().snapshot();
        cx.notify();
    }

}
