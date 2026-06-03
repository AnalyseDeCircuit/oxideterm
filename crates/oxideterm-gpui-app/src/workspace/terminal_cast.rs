use super::*;
use oxideterm_terminal_recording::{
    AsciicastEvent, AsciicastEventKind, AsciicastRecording, TerminalRecordingPlayback,
    parse_cast_resize,
};

mod render;

#[derive(Clone, Debug)]
pub(super) struct TerminalCastPlayerState {
    playback: TerminalRecordingPlayback,
    pane: Option<gpui::Entity<TerminalPane>>,
    search_visible: bool,
    pub(super) search_focused: bool,
    pub(super) search_query: String,
}

impl TerminalCastPlayerState {
    fn parse(file_name: String, content: &str) -> Result<Self, String> {
        Ok(Self {
            playback: TerminalRecordingPlayback::new(AsciicastRecording::parse(
                file_name, content,
            )?),
            pane: None,
            search_visible: false,
            search_focused: false,
            search_query: String::new(),
        })
    }

    fn with_pane(mut self, pane: gpui::Entity<TerminalPane>) -> Self {
        self.pane = Some(pane);
        self
    }

    fn toggle_playing(&mut self) {
        self.playback.toggle_playing();
    }

    fn set_speed(&mut self, speed: f64) {
        self.playback.set_speed(speed);
    }

    fn advance_to_now(&mut self) {
        self.playback.advance_to_now();
    }

    fn seek(&mut self, ratio: f64) {
        self.playback.seek_ratio(ratio);
    }

    fn reset_replay(&mut self) {
        self.playback.reset_replay();
    }

    fn take_due_events(&mut self) -> Vec<AsciicastEvent> {
        self.playback.take_due_events()
    }
}

fn apply_terminal_cast_events(
    pane: &mut TerminalPane,
    events: &[AsciicastEvent],
    cx: &mut gpui::Context<TerminalPane>,
) {
    for event in events {
        match event.kind {
            AsciicastEventKind::Output => pane.feed_recording_output(event.data.as_bytes(), cx),
            AsciicastEventKind::Resize => {
                if let Some((cols, rows)) = parse_cast_resize(&event.data) {
                    pane.resize_recording_playback(cols, rows, cx);
                }
            }
            AsciicastEventKind::Input => {}
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_terminal_cast_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("terminal.recording.open_cast"),
            )),
        });
        let window_handle = window.window_handle();
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("recording.cast")
                .to_string();
            let result = fs::read_to_string(&path)
                .map_err(|error| error.to_string())
                .and_then(|content| TerminalCastPlayerState::parse(file_name, &content));
            let _ = cx.update_window(window_handle, |_, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    match result {
                        Ok(player) => {
                            this.open_terminal_cast_player(player, window, cx);
                        }
                        Err(error) => {
                            let _ = this.terminal_notice_tx.send(TerminalNotice {
                                title: this.i18n.t("terminal.recording.open_failed"),
                                description: Some(error),
                                status_text: None,
                                progress: None,
                                variant: TerminalNoticeVariant::Error,
                            });
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn open_terminal_cast_player(
        &mut self,
        player: TerminalCastPlayerState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::LocalTerminal);
        let cols = player.playback.recording().width;
        let rows = player.playback.recording().height;
        let pane = cx.new(|cx| {
            TerminalPane::new_recording_playback(cols, rows, preferences, window, cx)
                .expect("recording playback terminal should not spawn a PTY")
        });
        self.terminal_cast_player = Some(player.with_pane(pane));
        self.rebuild_terminal_cast_playback(cx);
    }

    pub(super) fn close_terminal_cast_player(&mut self, cx: &mut Context<Self>) {
        self.terminal_cast_player = None;
        cx.notify();
    }

    pub(super) fn toggle_terminal_cast_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = self.terminal_cast_player.as_mut() {
            player.toggle_playing();
            if player.playback.playing() {
                self.schedule_terminal_cast_player_tick(cx);
            }
        }
        cx.notify();
    }

    pub(super) fn set_terminal_cast_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        if let Some(player) = self.terminal_cast_player.as_mut() {
            player.set_speed(speed);
        }
        cx.notify();
    }

    pub(super) fn seek_terminal_cast(&mut self, ratio: f64, cx: &mut Context<Self>) {
        let Some(player) = self.terminal_cast_player.as_mut() else {
            return;
        };
        let target_position =
            (player.playback.recording().duration * ratio.clamp(0.0, 1.0)).max(0.0);
        if (player.playback.position() - target_position).abs() <= f64::EPSILON {
            // Seekbar drags can repeat inside one playback timestamp. Rebuilding
            // the terminal replay is expensive, so skip unchanged seeks.
            return;
        }
        player.seek(ratio);
        self.rebuild_terminal_cast_playback(cx);
        cx.notify();
    }

    fn schedule_terminal_cast_player_tick(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(33)).await;
            let _ = weak.update(cx, |this, cx| {
                let mut should_schedule = false;
                if let Some(player) = this.terminal_cast_player.as_mut() {
                    player.advance_to_now();
                    should_schedule = player.playback.playing();
                }
                this.feed_due_terminal_cast_events(cx);
                if should_schedule {
                    this.schedule_terminal_cast_player_tick(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn rebuild_terminal_cast_playback(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.terminal_cast_player.as_mut() else {
            return;
        };
        let Some(pane) = player.pane.clone() else {
            return;
        };
        player.reset_replay();
        let width = player.playback.recording().width;
        let height = player.playback.recording().height;
        let query = (!player.search_query.is_empty()).then(|| player.search_query.clone());
        let events = player.take_due_events();
        let _ = pane.update(cx, |pane, cx| {
            pane.reset_recording_playback(width, height, cx);
            apply_terminal_cast_events(pane, &events, cx);
            pane.set_search_query(query, Some(0), cx);
        });
    }

    fn feed_due_terminal_cast_events(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.terminal_cast_player.as_mut() else {
            return;
        };
        let Some(pane) = player.pane.clone() else {
            return;
        };
        let query = (!player.search_query.is_empty()).then(|| player.search_query.clone());
        let events = player.take_due_events();
        if events.is_empty() {
            return;
        }
        let _ = pane.update(cx, |pane, cx| {
            apply_terminal_cast_events(pane, &events, cx);
            pane.set_search_query(query, Some(0), cx);
        });
    }

    pub(super) fn update_terminal_cast_search(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.terminal_cast_player.as_ref() else {
            return;
        };
        let Some(pane) = player.pane.clone() else {
            return;
        };
        let query = (!player.search_query.is_empty()).then(|| player.search_query.clone());
        let _ = pane.update(cx, |pane, cx| {
            pane.set_search_query(query, Some(0), cx);
        });
    }

    pub(super) fn update_terminal_cast_seek_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_cast_seek_dragging {
            self.apply_terminal_cast_seek_from_x(f32::from(event.position.x), cx);
        }
    }

    pub(super) fn finish_terminal_cast_seek_drag(&mut self, cx: &mut Context<Self>) {
        if self.terminal_cast_seek_dragging {
            self.terminal_cast_seek_dragging = false;
            cx.notify();
        }
    }

    fn apply_terminal_cast_seek_from_x(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::TerminalCastSeekbar)
        else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        self.seek_terminal_cast(((x - left) / width) as f64, cx);
    }
}
