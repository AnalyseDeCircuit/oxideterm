#[cfg(feature = "bench")]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalPlaybackUpdateTimings {
    pub terminal_lock: Duration,
    pub parse_and_grid: Duration,
    pub event_extraction: Duration,
    pub gpui_state_update: Duration,
}

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
                palette: asciicast_palette(self.theme.tokens.terminal),
            }),
        };
        self.recorder = Some(TerminalRecorder::start(
            self.snapshot.cols,
            self.snapshot.rows,
            options,
        ));
        self.sync_terminal_output_events_enabled();
        cx.emit(TerminalPaneEvent::RecordingStatusChanged);
        cx.notify();
    }

    pub fn pause_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.pause();
            self.sync_terminal_output_events_enabled();
            cx.emit(TerminalPaneEvent::RecordingStatusChanged);
            cx.notify();
        }
    }

    pub fn resume_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(recorder) = self.recorder.as_mut() {
            recorder.resume();
            self.sync_terminal_output_events_enabled();
            cx.emit(TerminalPaneEvent::RecordingStatusChanged);
            cx.notify();
        }
    }

    pub fn discard_recording(&mut self, cx: &mut Context<Self>) {
        if self.recorder.take().is_some() {
            self.sync_terminal_output_events_enabled();
            cx.emit(TerminalPaneEvent::RecordingStatusChanged);
            cx.notify();
        }
    }

    pub fn stop_recording(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let recorder = self.recorder.take()?;
        self.sync_terminal_output_events_enabled();
        cx.emit(TerminalPaneEvent::RecordingStatusChanged);
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
        self.mark_terminal_content_changed(cx);
        self.selection = None;
        self.search_query = None;
        self.search_cache = None;
        self.selected_search_match = None;
        cx.notify();
    }

    pub fn feed_recording_output(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        {
            let mut terminal = self.terminal.lock();
            terminal.feed_recording_output(bytes);
            let _ = terminal.take_events();
        }
        // Match live PTY output: multiple updates may coalesce before the visible pane builds one
        // latest incremental snapshot during render.
        self.snapshot_dirty = true;
        self.mark_terminal_content_changed(cx);
        cx.notify();
    }

    #[cfg(feature = "bench")]
    #[doc(hidden)]
    pub fn enable_benchmark_performance_metrics(&mut self) {
        // Benchmark collection is separate from the visible overlay so diagnostic chrome does
        // not change the scene whose layout and paint cost is being measured.
        self.benchmark_performance_metrics_enabled = true;
    }

    #[cfg(feature = "bench")]
    #[doc(hidden)]
    pub fn benchmark_render_stage_micros(&self) -> (u64, u64, u64, u64, u64) {
        let performance = self.layout_cache.lock().performance();
        (
            self.render_stats.snapshot_micros,
            self.benchmark_backend_snapshot_micros,
            self.benchmark_snapshot_state_micros,
            performance.layout_micros,
            performance.paint_micros,
        )
    }

    #[cfg(feature = "bench")]
    #[doc(hidden)]
    pub fn feed_recording_output_profiled(
        &mut self,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> TerminalPlaybackUpdateTimings {
        // Stage clocks are opt-in benchmark work; the production feed path above remains free of
        // timing calls and uses the same operation order.
        let lock_started = Instant::now();
        let mut terminal = self.terminal.lock();
        let terminal_lock = lock_started.elapsed();

        let parse_started = Instant::now();
        terminal.feed_recording_output(bytes);
        let parse_and_grid = parse_started.elapsed();

        let events_started = Instant::now();
        let _ = terminal.take_events();
        let event_extraction = events_started.elapsed();

        drop(terminal);

        let state_started = Instant::now();
        self.snapshot_dirty = true;
        self.mark_terminal_content_changed(cx);
        cx.notify();
        let gpui_state_update = state_started.elapsed();

        TerminalPlaybackUpdateTimings {
            terminal_lock,
            parse_and_grid,
            event_extraction,
            gpui_state_update,
        }
    }

    pub fn resize_recording_playback(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let snapshot = {
            let mut terminal = self.terminal.lock();
            let _ = terminal.resize_with_cell_size(cols, rows, 0, 0);
            terminal.snapshot()
        };
        self.snapshot = self.stamp_snapshot(snapshot);
        self.mark_terminal_content_changed(cx);
        cx.notify();
    }

}

fn asciicast_palette(theme: oxideterm_theme::TerminalTheme) -> String {
    // Asciicast v2 themes require all 16 ANSI colors when a theme object is present.
    [
        theme.black,
        theme.red,
        theme.green,
        theme.yellow,
        theme.blue,
        theme.magenta,
        theme.cyan,
        theme.white,
        theme.bright_black,
        theme.bright_red,
        theme.bright_green,
        theme.bright_yellow,
        theme.bright_blue,
        theme.bright_magenta,
        theme.bright_cyan,
        theme.bright_white,
    ]
    .map(hex_color)
    .join(":")
}
