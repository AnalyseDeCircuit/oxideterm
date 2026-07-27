use super::*;
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_terminal_recording::{
    AsciicastEvent, AsciicastEventKind, AsciicastRecording, TerminalRecordingPlayback,
    TerminalRecordingSearchResult, parse_cast_resize,
};

mod render;

#[derive(Debug)]
pub(in crate::workspace) struct TerminalCastPlayerState {
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

pub(in crate::workspace) struct TerminalCastRenderSnapshot {
    file_name: String,
    width: usize,
    height: usize,
    duration: f64,
    position: f64,
    playing: bool,
    speed: f64,
    pane: Option<gpui::Entity<TerminalPane>>,
    search_visible: bool,
    search_focused: bool,
    search_query: String,
    search_results: Vec<TerminalRecordingSearchResult>,
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

impl WorkspaceTerminalEntity {
    pub(in crate::workspace) fn cast_render_snapshot(&self) -> Option<TerminalCastRenderSnapshot> {
        let player = self.cast_player.as_ref()?;
        let recording = player.playback.recording();
        Some(TerminalCastRenderSnapshot {
            file_name: recording.file_name.clone(),
            width: recording.width,
            height: recording.height,
            duration: recording.duration,
            position: player.playback.position(),
            playing: player.playback.playing(),
            speed: player.playback.speed(),
            pane: player.pane.clone(),
            search_visible: player.search_visible,
            search_focused: player.search_focused,
            search_query: player.search_query.clone(),
            search_results: player.playback.search(&player.search_query),
        })
    }

    pub(in crate::workspace) fn cast_search_focused(&self) -> bool {
        self.cast_player
            .as_ref()
            .is_some_and(|player| player.search_focused)
    }

    pub(in crate::workspace) fn cast_search_query(&self) -> Option<&str> {
        self.cast_player
            .as_ref()
            .filter(|player| player.search_focused)
            .map(|player| player.search_query.as_str())
    }

    pub(in crate::workspace) fn open_cast_player(
        &mut self,
        player: TerminalCastPlayerState,
        cx: &mut Context<Self>,
    ) {
        self.cast_tick_generation = self.cast_tick_generation.saturating_add(1);
        self.cast_tick_scheduled = false;
        self.cast_seek_dragging = false;
        self.cast_player = Some(player);
        self.rebuild_cast_playback(cx);
    }

    pub(in crate::workspace) fn close_cast_player(&mut self) -> bool {
        let was_open = self.cast_player.take().is_some();
        if was_open {
            self.cast_tick_generation = self.cast_tick_generation.saturating_add(1);
            self.cast_tick_scheduled = false;
            self.cast_seek_dragging = false;
        }
        was_open
    }

    pub(in crate::workspace) fn toggle_cast_playback(&mut self, cx: &mut Context<Self>) {
        let should_schedule = self.cast_player.as_mut().is_some_and(|player| {
            player.toggle_playing();
            player.playback.playing()
        });
        if should_schedule {
            self.schedule_cast_tick(cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn set_cast_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        if let Some(player) = self.cast_player.as_mut() {
            player.set_speed(speed);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn seek_cast(&mut self, ratio: f64, cx: &mut Context<Self>) {
        let Some(player) = self.cast_player.as_mut() else {
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
        self.rebuild_cast_playback(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn seek_cast_by_seconds(
        &mut self,
        seconds: f64,
        cx: &mut Context<Self>,
    ) {
        let Some(player) = self.cast_player.as_ref() else {
            return;
        };
        let duration = player.playback.recording().duration.max(1.0);
        let target = (player.playback.position() + seconds) / duration;
        self.seek_cast(target, cx);
    }

    pub(in crate::workspace) fn toggle_cast_search(&mut self, cx: &mut Context<Self>) -> bool {
        let (search_visible, cleared_query) = {
            let Some(player) = self.cast_player.as_mut() else {
                return false;
            };
            player.search_visible = !player.search_visible;
            player.search_focused = player.search_visible;
            let cleared_query = !player.search_visible;
            if cleared_query {
                player.search_query.clear();
            }
            (player.search_visible, cleared_query)
        };
        if cleared_query {
            self.update_cast_search(cx);
        }
        cx.notify();
        search_visible
    }

    pub(in crate::workspace) fn focus_cast_search(&mut self) -> bool {
        let Some(player) = self.cast_player.as_mut() else {
            return false;
        };
        player.search_focused = true;
        player.search_visible = true;
        true
    }

    pub(in crate::workspace) fn blur_cast_search(&mut self) -> bool {
        let Some(player) = self.cast_player.as_mut() else {
            return false;
        };
        let was_focused = player.search_focused;
        player.search_focused = false;
        was_focused
    }

    pub(in crate::workspace) fn replace_cast_search(
        &mut self,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(player) = self
            .cast_player
            .as_mut()
            .filter(|player| player.search_focused)
        else {
            return false;
        };
        replace_utf16(&mut player.search_query, replacement_range, text);
        self.update_cast_search(cx);
        true
    }

    pub(in crate::workspace) fn pop_cast_search(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(player) = self
            .cast_player
            .as_mut()
            .filter(|player| player.search_focused)
        else {
            return false;
        };
        if player.search_query.pop().is_none() {
            return false;
        }
        self.update_cast_search(cx);
        true
    }

    pub(in crate::workspace) fn cast_seek_dragging(&self) -> bool {
        self.cast_seek_dragging
    }

    pub(in crate::workspace) fn begin_cast_seek_drag(&mut self) {
        self.cast_seek_dragging = self.cast_player.is_some();
    }

    pub(in crate::workspace) fn finish_cast_seek_drag(&mut self) -> bool {
        let was_dragging = self.cast_seek_dragging;
        self.cast_seek_dragging = false;
        was_dragging
    }

    fn schedule_cast_tick(&mut self, cx: &mut Context<Self>) {
        if self.cast_tick_scheduled {
            return;
        }
        self.cast_tick_scheduled = true;
        let generation = self.cast_tick_generation;
        cx.spawn(async move |terminal, cx| {
            Timer::after(Duration::from_millis(33)).await;
            let _ = terminal.update(cx, |terminal, cx| {
                if terminal.cast_tick_generation != generation {
                    return;
                }
                terminal.cast_tick_scheduled = false;
                let should_schedule = terminal.cast_player.as_mut().is_some_and(|player| {
                    player.advance_to_now();
                    player.playback.playing()
                });
                terminal.feed_due_cast_events(cx);
                if should_schedule {
                    terminal.schedule_cast_tick(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn rebuild_cast_playback(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.cast_player.as_mut() else {
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

    fn feed_due_cast_events(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.cast_player.as_mut() else {
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

    fn update_cast_search(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.cast_player.as_ref() else {
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
        let preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let cols = player.playback.recording().width;
        let rows = player.playback.recording().height;
        let pane = cx.new(|cx| {
            TerminalPane::new_recording_playback(cols, rows, preferences, window, cx)
                .expect("recording playback terminal should not spawn a PTY")
        });
        self.terminal.update(cx, |terminal, cx| {
            terminal.open_cast_player(player.with_pane(pane), cx);
        });
    }

    pub(super) fn close_terminal_cast_player(&mut self, cx: &mut Context<Self>) {
        if self
            .terminal
            .update(cx, |terminal, _cx| terminal.close_cast_player())
        {
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
        }
    }

    pub(super) fn toggle_terminal_cast_playback(&mut self, cx: &mut Context<Self>) {
        self.terminal
            .update(cx, |terminal, cx| terminal.toggle_cast_playback(cx));
    }

    pub(super) fn set_terminal_cast_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        self.terminal
            .update(cx, |terminal, cx| terminal.set_cast_speed(speed, cx));
    }

    pub(super) fn seek_terminal_cast(&mut self, ratio: f64, cx: &mut Context<Self>) {
        self.terminal
            .update(cx, |terminal, cx| terminal.seek_cast(ratio, cx));
    }

    pub(super) fn seek_terminal_cast_by_seconds(&mut self, seconds: f64, cx: &mut Context<Self>) {
        self.terminal.update(cx, |terminal, cx| {
            terminal.seek_cast_by_seconds(seconds, cx)
        });
    }

    pub(super) fn update_terminal_cast_seek_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.read(cx).cast_seek_dragging() {
            self.apply_terminal_cast_seek_from_x(f32::from(event.position.x), cx);
        }
    }

    pub(super) fn finish_terminal_cast_seek_drag(&mut self, cx: &mut Context<Self>) {
        if self
            .terminal
            .update(cx, |terminal, _cx| terminal.finish_cast_seek_drag())
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn new_terminal_entity(cx: &mut TestAppContext) -> Entity<WorkspaceTerminalEntity> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("create test runtime"),
        );
        let registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let settings_path = std::env::temp_dir().join("oxideterm-cast-entity-tests-settings.json");
        cx.new(|cx| {
            WorkspaceTerminalEntity::new(runtime, NodeRouter::new(registry), &settings_path, cx)
        })
    }

    fn sample_cast_player() -> TerminalCastPlayerState {
        // Keep the fixture long enough for playback to remain active while its timer is pending.
        let cast = concat!(
            "{\"version\":2,\"width\":80,\"height\":24,\"duration\":2.0}\n",
            "[0.5,\"o\",\"hello world\"]\n"
        );
        TerminalCastPlayerState::parse("demo.cast".to_string(), cast)
            .expect("parse terminal recording")
    }

    #[gpui::test]
    fn cast_state_and_pending_tick_are_released_by_the_entity(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        terminal.update(cx, |terminal, cx| {
            terminal.open_cast_player(sample_cast_player(), cx);
            terminal.toggle_cast_search(cx);
            assert!(terminal.replace_cast_search(None, "world", cx));
            terminal.begin_cast_seek_drag();
            terminal.toggle_cast_playback(cx);
        });

        let active_generation = terminal.read_with(cx, |terminal, _cx| {
            let snapshot = terminal
                .cast_render_snapshot()
                .expect("cast player should be open");
            assert!(snapshot.playing);
            assert!(snapshot.search_visible);
            assert!(snapshot.search_focused);
            assert_eq!(snapshot.search_query, "world");
            assert_eq!(snapshot.search_results.len(), 1);
            assert!(terminal.cast_seek_dragging);
            assert!(terminal.cast_tick_scheduled);
            terminal.cast_tick_generation
        });

        terminal.update(cx, |terminal, _cx| {
            assert!(terminal.close_cast_player());
        });
        terminal.read_with(cx, |terminal, _cx| {
            assert!(terminal.cast_player.is_none());
            assert!(!terminal.cast_seek_dragging);
            assert!(!terminal.cast_tick_scheduled);
            assert!(terminal.cast_tick_generation > active_generation);
        });
    }
}
