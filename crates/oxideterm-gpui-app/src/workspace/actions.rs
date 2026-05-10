use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_gpui_ui::text_input::{text_caret, text_input_anchor_probe};

#[derive(Clone, Copy)]
pub(super) enum TerminalBroadcastMenuPlacement {
    Bottom(f32),
    Top(f32),
}

#[derive(Default)]
pub(super) struct SearchBarState {
    pub(super) visible: bool,
    pub(super) query: String,
    pub(super) active_match: Option<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct TerminalCastPlayerState {
    file_name: String,
    width: usize,
    height: usize,
    duration: f64,
    position: f64,
    speed: f64,
    playing: bool,
    last_tick: Option<Instant>,
    events: Vec<TerminalCastEvent>,
    replayed_event_index: usize,
    pane: Option<gpui::Entity<TerminalPane>>,
    search_visible: bool,
    pub(super) search_focused: bool,
    pub(super) search_query: String,
}

#[derive(Clone, Debug)]
struct TerminalCastEvent {
    at: f64,
    kind: char,
    data: String,
}

#[derive(Clone, Debug)]
struct TerminalCastSearchResult {
    at: f64,
    snippet: String,
}

impl TerminalCastPlayerState {
    fn parse(file_name: String, content: &str) -> Result<Self, String> {
        let mut lines = content.lines();
        let header_line = lines.next().ok_or_else(|| "empty cast file".to_string())?;
        let header: serde_json::Value =
            serde_json::from_str(header_line).map_err(|error| error.to_string())?;
        let version = header
            .get("version")
            .and_then(|value| value.as_u64())
            .unwrap_or(1);
        let width = header
            .get("width")
            .and_then(|value| value.as_u64())
            .unwrap_or(80) as usize;
        let height = header
            .get("height")
            .and_then(|value| value.as_u64())
            .unwrap_or(24) as usize;
        let mut events = Vec::new();
        if version == 2 {
            for line in lines {
                let value: serde_json::Value =
                    serde_json::from_str(line).map_err(|error| error.to_string())?;
                let Some(array) = value.as_array() else {
                    continue;
                };
                let Some(at) = array.first().and_then(|value| value.as_f64()) else {
                    continue;
                };
                let kind = array
                    .get(1)
                    .and_then(|value| value.as_str())
                    .and_then(|value| value.chars().next())
                    .unwrap_or('o');
                let data = array
                    .get(2)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                events.push(TerminalCastEvent { at, kind, data });
            }
        } else if let Some(stdout) = header.get("stdout").and_then(|value| value.as_array()) {
            let mut at = 0.0;
            for item in stdout {
                let Some(array) = item.as_array() else {
                    continue;
                };
                at += array
                    .first()
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0);
                let data = array
                    .get(1)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                events.push(TerminalCastEvent {
                    at,
                    kind: 'o',
                    data,
                });
            }
        }
        let duration = header
            .get("duration")
            .and_then(|value| value.as_f64())
            .unwrap_or_else(|| events.last().map(|event| event.at).unwrap_or(0.0));
        let player = Self {
            file_name,
            width,
            height,
            duration,
            position: 0.0,
            speed: 1.0,
            playing: false,
            last_tick: None,
            events,
            replayed_event_index: 0,
            pane: None,
            search_visible: false,
            search_focused: false,
            search_query: String::new(),
        };
        Ok(player)
    }

    fn with_pane(mut self, pane: gpui::Entity<TerminalPane>) -> Self {
        self.pane = Some(pane);
        self
    }

    fn toggle_playing(&mut self) {
        self.playing = !self.playing;
        self.last_tick = self.playing.then(Instant::now);
    }

    fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }

    fn advance_to_now(&mut self) {
        if !self.playing {
            return;
        }
        let now = Instant::now();
        let Some(last_tick) = self.last_tick.replace(now) else {
            return;
        };
        self.position = (self.position + last_tick.elapsed().as_secs_f64() * self.speed)
            .min(self.duration.max(0.0));
        if self.position >= self.duration {
            self.playing = false;
            self.last_tick = None;
        }
    }

    fn seek(&mut self, ratio: f64) {
        self.position = (self.duration * ratio.clamp(0.0, 1.0)).max(0.0);
        self.replayed_event_index = 0;
    }

    fn reset_replay(&mut self) {
        self.replayed_event_index = 0;
    }

    fn take_due_events(&mut self) -> Vec<TerminalCastEvent> {
        let start = self.replayed_event_index;
        let mut end = start;
        while end < self.events.len() && self.events[end].at <= self.position {
            end += 1;
        }
        self.replayed_event_index = end;
        self.events[start..end].to_vec()
    }
}

fn format_recording_elapsed(duration: Duration) -> String {
    let total = duration.as_secs();
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn format_cast_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn terminal_cast_search_results(
    events: &[TerminalCastEvent],
    query: &str,
) -> Vec<TerminalCastSearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    events
        .iter()
        .filter(|event| event.kind == 'o')
        .filter_map(|event| {
            let snippet = terminal_cast_search_snippet(&event.data, &needle)?;
            Some(TerminalCastSearchResult {
                at: event.at,
                snippet,
            })
        })
        .take(50)
        .collect()
}

fn terminal_cast_search_snippet(data: &str, needle: &str) -> Option<String> {
    let plain = strip_cast_control_sequences(data)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = plain.to_lowercase();
    let found = lower.find(needle)?;
    let start = plain[..found]
        .char_indices()
        .rev()
        .nth(24)
        .map_or(0, |(i, _)| i);
    let end = plain[found..]
        .char_indices()
        .nth(96)
        .map_or(plain.len(), |(i, _)| found + i);
    let mut snippet = plain[start..end].trim().to_string();
    if start > 0 {
        snippet.insert_str(0, "...");
    }
    if end < plain.len() {
        snippet.push_str("...");
    }
    Some(snippet)
}

fn strip_cast_control_sequences(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if matches!(chars.peek(), Some('[' | ']' | 'P' | '_' | '^')) {
                while let Some(next) = chars.next() {
                    if next.is_ascii_alphabetic() || matches!(next, '\u{7}' | '\\') {
                        break;
                    }
                }
            }
            continue;
        }
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        output.push(ch);
    }
    output
}

fn apply_terminal_cast_events(
    pane: &mut TerminalPane,
    events: &[TerminalCastEvent],
    cx: &mut gpui::Context<TerminalPane>,
) {
    for event in events {
        match event.kind {
            'o' => pane.feed_recording_output(event.data.as_bytes(), cx),
            'r' => {
                if let Some((cols, rows)) = parse_cast_resize(&event.data) {
                    pane.resize_recording_playback(cols, rows, cx);
                }
            }
            _ => {}
        }
    }
}

fn parse_cast_resize(value: &str) -> Option<(usize, usize)> {
    let (cols, rows) = value.split_once('x')?;
    Some((cols.parse().ok()?, rows.parse().ok()?))
}

fn terminal_cast_player_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    div()
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_panel << 8) | 0xcc))
        .hover({
            let hover = tokens.ui.bg_hover;
            move |style| style.bg(rgb(hover))
        })
        .child(WorkspaceApp::render_lucide_icon(
            icon,
            15.0,
            rgb(tokens.ui.text),
        ))
}

fn terminal_cast_speed_button(
    tokens: &ThemeTokens,
    label: &'static str,
    active: bool,
) -> gpui::Div {
    div()
        .h(px(30.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .border_1()
        .border_color(if active {
            rgb(tokens.ui.accent)
        } else {
            rgb(tokens.ui.border)
        })
        .bg(if active {
            rgba((tokens.ui.accent << 8) | 0x1f)
        } else {
            rgba((tokens.ui.bg_panel << 8) | 0xcc)
        })
        .text_size(px(12.0))
        .text_color(if active {
            rgb(tokens.ui.accent)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .child(label)
}

fn terminal_cast_text_button(tokens: &ThemeTokens, label: &'static str) -> gpui::Div {
    div()
        .h(px(30.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_panel << 8) | 0xcc))
        .text_size(px(12.0))
        .text_color(rgb(tokens.ui.text_muted))
        .hover({
            let hover = tokens.ui.bg_hover;
            move |style| style.bg(rgb(hover))
        })
        .child(label)
}

impl WorkspaceApp {
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = true;
        window.focus(&self.focus_handle);
        if let Some(pane) = self.active_pane() {
            let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
            let _ = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, self.search.active_match, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = false;
        self.search.active_match = None;
        self.ime_marked_text = None;
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.set_search_query(None, None, cx));
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn update_search_query(&mut self, cx: &mut Context<Self>) {
        let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
        self.search.active_match = query.as_ref().map(|_| 0);
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, self.search.active_match, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn search_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.select_next_search_result(forward, cx);
            });
        }
    }

    pub(super) fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.copy_to_clipboard(cx));
        }
    }

    pub(super) fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.paste_from_clipboard(cx));
        }
    }

    pub(super) fn handle_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_connection_form.is_some() {
            let _ = self.handle_new_connection_key(event, window, cx);
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.active_surface == ActiveSurface::Settings && self.open_settings_select.is_some() {
            if key == "escape" && !modifiers.platform {
                self.open_settings_select = None;
                cx.notify();
            }
            return;
        }

        if self.active_surface == ActiveSurface::Settings && self.focused_settings_input.is_some() {
            let _ = self.handle_settings_input_key(event, cx);
            return;
        }

        if self.terminal_quick_commands_open && self.quick_commands.focused_input.is_some() {
            self.handle_quick_commands_key(event, cx);
            return;
        }

        if self
            .terminal_cast_player
            .as_ref()
            .is_some_and(|player| player.search_focused)
        {
            self.handle_terminal_cast_search_key(event, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::SessionManager)
            && self.session_manager.focused_input.is_some()
        {
            let _ = self.handle_session_manager_key(event, window, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Sftp)
        {
            let _ = self.handle_sftp_key(event, cx);
            return;
        }

        if self.terminal_command_bar_focused {
            self.handle_terminal_command_bar_key(event, window, cx);
            return;
        }

        if self.active_surface == ActiveSurface::Settings && key == "escape" && !modifiers.platform
        {
            self.close_settings(window, cx);
            return;
        }

        if self.search.visible && !modifiers.platform {
            match key {
                "escape" => self.close_search(window, cx),
                "enter" => self.search_next(!modifiers.shift, cx),
                "backspace" => {
                    self.search.query.pop();
                    self.update_search_query(cx);
                }
                _ => {}
            }
            return;
        }
    }

    pub(super) fn handle_terminal_command_bar_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform {
            return;
        }

        match key {
            "escape" => {
                self.terminal_command_bar_focused = false;
                self.terminal_quick_commands_open = false;
                self.terminal_quick_command_pending = None;
                self.ime_marked_text = None;
                self.focus_active_pane(window, cx);
                cx.notify();
            }
            "enter" => self.submit_terminal_command_bar(window, cx),
            "backspace" => {
                self.terminal_command_bar_draft.pop();
                self.ime_marked_text = None;
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn handle_terminal_cast_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform {
            return;
        }
        match key {
            "escape" => {
                if let Some(player) = self.terminal_cast_player.as_mut() {
                    player.search_focused = false;
                }
                self.ime_marked_text = None;
                cx.notify();
            }
            "backspace" => {
                if let Some(player) = self.terminal_cast_player.as_mut() {
                    player.search_query.pop();
                }
                self.update_terminal_cast_search(cx);
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn submit_terminal_command_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self.terminal_command_bar_draft.trim().to_string();
        if command.is_empty() {
            return;
        }

        self.submit_terminal_command_line(&command, window, cx);
        self.terminal_command_bar_draft.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    fn submit_terminal_command_line(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(source_pane_id) = self.active_pane_id() {
            self.send_terminal_command_to_pane(
                source_pane_id,
                command,
                TerminalCommandMarkDetectionSource::CommandBar,
                cx,
            );
            self.broadcast_terminal_command(source_pane_id, command, cx);
        } else {
            return false;
        }

        if self.terminal_command_should_handoff_focus(command) {
            self.terminal_command_bar_focused = false;
            self.focus_active_pane(window, cx);
        }
        true
    }

    pub(super) fn run_quick_command(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = &self.settings_store.settings().terminal.command_bar;
        let risk = classify_command_risk(command);
        if settings.quick_commands_confirm_before_run || risk.is_some() {
            self.terminal_quick_command_pending = Some(command.to_string());
            self.terminal_quick_commands_open = true;
            cx.notify();
            return;
        }
        self.execute_quick_command(command, window, cx);
    }

    fn execute_quick_command(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.submit_terminal_command_line(command, window, cx)
            && self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .quick_commands_show_toast
        {
            let _ = self.terminal_notice_tx.send(TerminalNotice {
                title: self.i18n.t("terminal.quick_commands.toast_executed"),
                description: Some(command.to_string()),
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            });
        }
        self.terminal_quick_command_pending = None;
        self.terminal_quick_commands_open = false;
        self.terminal_command_bar_draft.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn active_terminal_recording_status(
        &self,
        cx: &mut Context<Self>,
    ) -> TerminalRecordingStatus {
        self.active_pane()
            .map(|pane| pane.read(cx).recording_status())
            .unwrap_or_default()
    }

    pub(super) fn any_terminal_recording_active(&self, cx: &mut Context<Self>) -> bool {
        self.panes
            .values()
            .any(|pane| pane.read(cx).recording_status().state != TerminalRecordingState::Idle)
    }

    pub(super) fn start_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        let title = self.active_tab().map(|tab| tab.title.clone());
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.start_recording(title, cx));
            let _ = self.terminal_notice_tx.send(TerminalNotice {
                title: self.i18n.t("terminal.recording.started"),
                description: None,
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            });
        }
        cx.notify();
    }

    pub(super) fn pause_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.pause_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn resume_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.resume_recording(cx));
        }
        cx.notify();
    }

    pub(super) fn discard_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.discard_recording(cx));
            let _ = self.terminal_notice_tx.send(TerminalNotice {
                title: self.i18n.t("terminal.recording.discarded"),
                description: None,
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Warning,
            });
        }
        cx.notify();
    }

    pub(super) fn stop_active_terminal_recording(&mut self, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_pane_id() else {
            return;
        };
        let Some(pane) = self.panes.get(&pane_id).cloned() else {
            return;
        };
        let session_label = self
            .active_terminal_session_id()
            .map(|id| id.0.to_string())
            .unwrap_or_else(|| pane_id.0.to_string());
        let content = pane.update(cx, |pane, cx| pane.stop_recording(cx));
        let Some(content) = content else {
            return;
        };
        self.prompt_save_terminal_recording(session_label, content, cx);
        cx.notify();
    }

    fn prompt_save_terminal_recording(
        &mut self,
        session_label: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Downloads"))
            .unwrap_or_else(|| PathBuf::from("."));
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let suggested = format!("oxideterm-{session_label}-{timestamp}.cast");
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |weak, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => fs::write(&path, content)
                    .map(|_| Some(path))
                    .map_err(|error| error.to_string()),
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(Some(path)) => {
                        let _ = this.terminal_notice_tx.send(TerminalNotice {
                            title: this.i18n.t("terminal.recording.saved"),
                            description: Some(path.to_string_lossy().to_string()),
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Success,
                        });
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = this.terminal_notice_tx.send(TerminalNotice {
                            title: this.i18n.t("terminal.recording.save_failed"),
                            description: Some(error),
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Error,
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

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
        let cols = player.width;
        let rows = player.height;
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
            if player.playing {
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
        if let Some(player) = self.terminal_cast_player.as_mut() {
            player.seek(ratio);
        }
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
                    should_schedule = player.playing;
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
        let width = player.width;
        let height = player.height;
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

    fn send_terminal_command_to_pane(
        &self,
        pane_id: PaneId,
        command: &str,
        mark_source: TerminalCommandMarkDetectionSource,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.panes.get(&pane_id).cloned() {
            let _ = pane.update(cx, |pane, cx| {
                pane.begin_command_mark(command, mark_source, cx);
                pane.send_command_line(command, cx);
            });
        }
    }

    fn broadcast_terminal_command(
        &mut self,
        source_pane_id: PaneId,
        command: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_broadcast_enabled {
            return;
        }

        self.retain_live_terminal_broadcast_targets();
        let targets = self.terminal_broadcast_target_panes(source_pane_id);
        for pane_id in targets {
            self.send_terminal_command_to_pane(
                pane_id,
                command,
                TerminalCommandMarkDetectionSource::Broadcast,
                cx,
            );
        }
    }

    pub(super) fn terminal_broadcast_target_panes(&self, source_pane_id: PaneId) -> Vec<PaneId> {
        let mut candidates = Vec::new();
        for tab in &self.tabs {
            if let Some(root) = tab.root_pane.as_ref() {
                root.collect_pane_ids(&mut candidates);
            }
        }
        candidates.retain(|pane_id| *pane_id != source_pane_id && self.panes.contains_key(pane_id));

        if self.terminal_broadcast_targets.is_empty() {
            candidates
        } else {
            candidates
                .into_iter()
                .filter(|pane_id| self.terminal_broadcast_targets.contains(pane_id))
                .collect()
        }
    }

    fn retain_live_terminal_broadcast_targets(&mut self) {
        let panes = &self.panes;
        self.terminal_broadcast_targets
            .retain(|pane_id| panes.contains_key(pane_id));
    }

    fn terminal_broadcast_entries(&self) -> Vec<(PaneId, String, TabKind)> {
        let mut entries = Vec::new();
        for tab in &self.tabs {
            let Some(root) = tab.root_pane.as_ref() else {
                continue;
            };
            let mut pane_ids = Vec::new();
            root.collect_pane_ids(&mut pane_ids);
            for pane_id in pane_ids {
                if !self.panes.contains_key(&pane_id) {
                    continue;
                }
                let label = if root.pane_count() > 1 {
                    format!("{} · {}", tab.title, pane_id)
                } else {
                    tab.title.clone()
                };
                entries.push((pane_id, label, tab.kind.clone()));
            }
        }
        entries
    }

    fn terminal_command_should_handoff_focus(&self, command: &str) -> bool {
        let Some(command_name) = terminal_command_executable(command) else {
            return false;
        };
        self.settings_store
            .settings()
            .terminal
            .command_bar
            .focus_handoff_commands
            .iter()
            .any(|candidate| candidate == &command_name)
    }

    pub(super) fn switch_locale(
        &mut self,
        locale: Locale,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.i18n.set_locale(locale);
        self.settings_store.settings_mut().general.language = settings_language_from_locale(locale);
        let _ = self.settings_store.save();
        self.sync_tab_titles(cx);
        let panes = self
            .panes
            .iter()
            .map(|(pane_id, pane)| (*pane_id, pane.clone()))
            .collect::<Vec<_>>();
        for (pane_id, pane) in panes {
            let preferences = self.terminal_preferences_for_pane(pane_id);
            let _ = pane.update(cx, |pane, cx| {
                pane.set_preferences(preferences, cx);
            });
        }

        let menus = crate::platform::app_menus(&self.i18n);
        let _ = cx.update_window(window.window_handle(), move |_root, _window, app| {
            app.set_menus(menus);
        });
        cx.notify();
    }

    pub(super) fn sync_tab_titles(&mut self, _cx: &App) {
        for tab in &mut self.tabs {
            if let TabTitleSource::I18nKey(key) = tab.title_source {
                tab.title = self.i18n.t(key);
            }
        }
    }

    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::Search;
        let workspace = cx.entity();
        let query = if self.search.query.is_empty() {
            self.i18n.t("search.placeholder")
        } else {
            self.search.query.clone()
        };
        div()
            .h(px(self.tokens.metrics.searchbar_height))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .bg(rgb(theme.bg_panel))
            .border_b_1()
            .border_color(rgb(theme.border))
            .text_size(px(self.tokens.metrics.searchbar_font_size))
            .text_color(rgb(theme.text))
            .child(text_input_anchor_probe(
                target.anchor_id(),
                div()
                    .flex_1()
                    .h(px(self.tokens.metrics.search_input_height))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(theme.bg))
                    .text_color(if self.search.query.is_empty() {
                        rgb(theme.text_muted)
                    } else {
                        rgb(theme.text)
                    })
                    .child(query)
                    .when_some(self.marked_text_for_target(target), |input, marked| {
                        input.child(
                            div()
                                .underline()
                                .text_color(rgb(theme.text))
                                .child(marked.to_string()),
                        )
                    }),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            ))
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.previous"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.search_next(false, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.next"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.search_next(true, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.close"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.close_search(window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_terminal_surface(
        &self,
        root_pane: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let terminal = self.render_pane_tree(root_pane, cx);
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        if !self.settings_store.settings().terminal.command_bar.enabled {
            return div()
                .size_full()
                .relative()
                .child(terminal)
                .when(recording_active, |surface| {
                    surface.child(self.render_terminal_recording_controls(recording_status, cx))
                })
                .into_any_element();
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(terminal)
                    .when(recording_active, |surface| {
                        surface.child(self.render_terminal_recording_controls(recording_status, cx))
                    }),
            )
            .child(self.render_terminal_command_bar(cx))
            .into_any_element()
    }

    fn render_terminal_recording_controls(
        &self,
        status: TerminalRecordingStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let is_paused = status.state == TerminalRecordingState::Paused;
        div()
            .absolute()
            .top(px(12.0))
            .right(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded_lg()
            .border_1()
            .border_color(rgba(0xef444459))
            .bg(rgba((theme.bg_elevated << 8) | 0xe6))
            .px(px(10.0))
            .py(px(6.0))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgba(0xfca5a5ff))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Circle,
                        9.0,
                        if is_paused {
                            rgb(theme.text_muted)
                        } else {
                            rgba(0xf87171ff)
                        },
                    ))
                    .child(if is_paused {
                        self.i18n.t("terminal.recording.paused")
                    } else {
                        self.i18n.t("terminal.recording.recording")
                    })
                    .child(format_recording_elapsed(status.elapsed)),
            )
            .child(
                terminal_cast_player_button(
                    &self.tokens,
                    if is_paused {
                        LucideIcon::Play
                    } else {
                        LucideIcon::Pause
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if is_paused {
                            this.resume_active_terminal_recording(cx);
                        } else {
                            this.pause_active_terminal_recording(cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                terminal_cast_player_button(&self.tokens, LucideIcon::Square).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.stop_active_terminal_recording(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                terminal_cast_player_button(&self.tokens, LucideIcon::Trash2).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.discard_active_terminal_recording(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const COMMAND_BAR_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg/95
        const COMMAND_BAR_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70
        const COMMAND_BAR_INPUT_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-border/45
        const COMMAND_BAR_FOCUSED_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-accent/45

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandBar;
        let workspace = cx.entity();
        let focused = self.terminal_command_bar_focused;
        let marked_text = self.marked_text_for_target(target);
        let command_is_empty = self.terminal_command_bar_draft.is_empty();
        let showing_placeholder = command_is_empty && marked_text.is_none();
        let command_text = if showing_placeholder {
            self.i18n.t("terminal.command_bar.command_placeholder")
        } else {
            self.terminal_command_bar_draft.clone()
        };
        let target_label = self
            .active_tab()
            .map(|tab| match tab.kind {
                TabKind::LocalTerminal => self.i18n.t("terminal.command_bar.local_shell"),
                TabKind::SshTerminal => tab.title.clone(),
                _ => tab.title.clone(),
            })
            .unwrap_or_else(|| self.i18n.t("terminal.command_bar.remote_shell"));
        let active_pane_id = self.active_pane_id();
        let is_local_terminal = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::LocalTerminal);
        let can_split = self.active_tab().is_some_and(|tab| {
            tab.kind == TabKind::LocalTerminal
                && tab
                    .root_pane
                    .as_ref()
                    .is_some_and(|root| root.pane_count() < MAX_PANES_PER_TAB)
        });
        let broadcast_targets =
            self.terminal_broadcast_target_panes(active_pane_id.unwrap_or(PaneId(0)));
        let broadcast_label = if self.terminal_broadcast_enabled {
            if self.terminal_broadcast_targets.is_empty() {
                self.i18n.t("terminal.command_bar.all_targets")
            } else {
                format!("{}", broadcast_targets.len())
            }
        } else {
            String::new()
        };
        let quick_commands_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_enabled;
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;

        div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | COMMAND_BAR_BORDER_ALPHA))
            .bg(rgba((theme.bg << 8) | COMMAND_BAR_BG_ALPHA))
            .px(px(12.0))
            .py(px(4.0))
            .shadow_lg()
            .when(self.terminal_broadcast_menu_open, |bar| {
                bar.child(self.render_terminal_broadcast_menu(
                    TerminalBroadcastMenuPlacement::Bottom(62.0),
                    cx,
                ))
            })
            .when(
                quick_commands_enabled && self.terminal_quick_commands_open,
                |bar| bar.child(self.render_terminal_quick_commands_popover(cx)),
            )
            .child(
                div()
                    .min_h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(target_label),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .when(
                                self.terminal_broadcast_enabled && !broadcast_label.is_empty(),
                                |actions| {
                                    actions.child(
                                        div()
                                            .h(px(20.0))
                                            .px(px(6.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgba(0xf973164d))
                                            .bg(rgba(0xf973161a))
                                            .text_size(px(11.0))
                                            .text_color(rgba(0xfdba74ff))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Radio,
                                                12.0,
                                                rgba(0xfdba74ff),
                                            ))
                                            .child(broadcast_label),
                                    )
                                },
                            )
                            .when(is_local_terminal, |actions| {
                                actions
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .text_color(if can_split {
                                                rgb(theme.text_muted)
                                            } else {
                                                rgba((theme.text_muted << 8) | 0x59)
                                            })
                                            .when(can_split, |button| {
                                                button
                                                    .cursor_pointer()
                                                    .hover(move |style| {
                                                        style.bg(rgb(theme.bg_hover))
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, window, cx| {
                                                            this.split_active_pane(
                                                                SplitDirection::Horizontal,
                                                                window,
                                                                cx,
                                                            );
                                                            cx.stop_propagation();
                                                        }),
                                                    )
                                            })
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::SplitSquareHorizontal,
                                                14.0,
                                                if can_split {
                                                    rgb(theme.text_muted)
                                                } else {
                                                    rgba((theme.text_muted << 8) | 0x59)
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .when(can_split, |button| {
                                                button
                                                    .cursor_pointer()
                                                    .hover(move |style| {
                                                        style.bg(rgb(theme.bg_hover))
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, window, cx| {
                                                            this.split_active_pane(
                                                                SplitDirection::Vertical,
                                                                window,
                                                                cx,
                                                            );
                                                            cx.stop_propagation();
                                                        }),
                                                    )
                                            })
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::SplitSquareVertical,
                                                14.0,
                                                if can_split {
                                                    rgb(theme.text_muted)
                                                } else {
                                                    rgba((theme.text_muted << 8) | 0x59)
                                                },
                                            )),
                                    )
                            })
                            .child(
                                div()
                                    .relative()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(if self.terminal_broadcast_enabled {
                                        rgba(0xf9731626)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x00)
                                    })
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.terminal_broadcast_menu_open =
                                                !this.terminal_broadcast_menu_open;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Radio,
                                        14.0,
                                        if self.terminal_broadcast_enabled {
                                            rgba(0xfb923cff)
                                        } else {
                                            rgb(theme.text_muted)
                                        },
                                    )),
                            )
                            .when(recording_active, |actions| {
                                actions.child(
                                    div()
                                        .h(px(20.0))
                                        .px(px(6.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgba(0xef44444d))
                                        .bg(rgba(0xef44441a))
                                        .text_size(px(11.0))
                                        .text_color(rgba(0xfca5a5ff))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Circle,
                                            10.0,
                                            rgba(0xfca5a5ff),
                                        ))
                                        .child(format_recording_elapsed(recording_status.elapsed)),
                                )
                            })
                            .child(
                                div()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(if recording_active {
                                        rgba(0xef444426)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            match recording_status.state {
                                                TerminalRecordingState::Idle => {
                                                    this.start_active_terminal_recording(cx)
                                                }
                                                TerminalRecordingState::Recording => {
                                                    this.pause_active_terminal_recording(cx)
                                                }
                                                TerminalRecordingState::Paused => {
                                                    this.resume_active_terminal_recording(cx)
                                                }
                                            }
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        match recording_status.state {
                                            TerminalRecordingState::Paused => LucideIcon::Play,
                                            _ => LucideIcon::Circle,
                                        },
                                        14.0,
                                        if recording_active {
                                            rgba(0xf87171ff)
                                        } else {
                                            rgb(theme.text_muted)
                                        },
                                    )),
                            )
                            .when(recording_active, |actions| {
                                actions
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.stop_active_terminal_recording(cx);
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Square,
                                                14.0,
                                                rgba(0xf87171ff),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .size(px(24.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_md()
                                            .cursor_pointer()
                                            .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.discard_active_terminal_recording(cx);
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Trash2,
                                                14.0,
                                                rgba(0xf87171ff),
                                            )),
                                    )
                            })
                            .child(
                                div()
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, window, cx| {
                                            this.open_terminal_cast_file(window, cx);
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::FilePlay,
                                        14.0,
                                        rgb(theme.text_muted),
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(if focused {
                        rgba((theme.accent << 8) | COMMAND_BAR_FOCUSED_BORDER_ALPHA)
                    } else {
                        rgba((theme.border << 8) | COMMAND_BAR_INPUT_BORDER_ALPHA)
                    })
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.terminal_command_bar_focused = true;
                            this.ime_marked_text = None;
                            window.focus(&this.focus_handle);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        16.0,
                        rgb(theme.text_muted),
                    ))
                    .child(text_input_anchor_probe(
                        target.anchor_id(),
                        div()
                            .h(px(24.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .text_size(px(13.0))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_color(if showing_placeholder {
                                rgb(theme.text_muted)
                            } else {
                                rgb(theme.text)
                            })
                            .when(focused && showing_placeholder, |input| {
                                input.child(text_caret(
                                    &self.tokens,
                                    self.new_connection_caret_visible,
                                ))
                            })
                            .child(command_text)
                            .when_some(marked_text, |input, marked| {
                                input.child(
                                    div()
                                        .underline()
                                        .text_color(rgb(theme.text))
                                        .child(marked.to_string()),
                                )
                            })
                            .when(focused && !showing_placeholder, |input| {
                                input.child(text_caret(
                                    &self.tokens,
                                    self.new_connection_caret_visible,
                                ))
                            }),
                        move |anchor, _window, cx| {
                            let _ = workspace.update(cx, |this, cx| {
                                this.update_text_input_anchor(anchor, cx);
                            });
                        },
                    ))
                    .when(quick_commands_enabled, |input_row| {
                        input_row.child(
                            div()
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(if self.terminal_quick_commands_open {
                                    rgba((theme.accent << 8) | 0x1a)
                                } else {
                                    rgba(0x00000000)
                                })
                                .text_color(if self.terminal_quick_commands_open {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                })
                                .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.terminal_quick_commands_open =
                                            !this.terminal_quick_commands_open;
                                        this.terminal_broadcast_menu_open = false;
                                        if !this.terminal_quick_commands_open {
                                            this.terminal_quick_command_pending = None;
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Zap,
                                    14.0,
                                    if self.terminal_quick_commands_open {
                                        rgb(theme.accent)
                                    } else {
                                        rgb(theme.text_muted)
                                    },
                                )),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_terminal_quick_commands_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_quick_commands_popover(cx)
    }

    pub(super) fn render_terminal_cast_player(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let player = self.terminal_cast_player.as_ref()?;
        const PLAYER_PANEL_ALPHA: u32 = 0xf2;
        const PLAYER_BORDER_ALPHA: u32 = 0x99;
        let theme = self.tokens.ui;
        let progress = if player.duration <= 0.0 {
            0.0
        } else {
            (player.position / player.duration).clamp(0.0, 1.0) as f32
        };
        let search_target = WorkspaceImeTarget::TerminalCastSearch;
        let search_marked = self.marked_text_for_target(search_target);
        let search_empty = player.search_query.is_empty() && search_marked.is_none();
        let search_text = if search_empty {
            self.i18n.t("terminal.recording.search_placeholder")
        } else {
            player.search_query.clone()
        };
        let search_results = terminal_cast_search_results(&player.events, &player.search_query);
        let pane = player.pane.clone();
        let workspace = cx.entity();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .flex_col()
                .bg(rgba((theme.bg_sunken << 8) | PLAYER_PANEL_ALPHA))
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .bg(rgba((theme.bg_sunken << 8) | PLAYER_PANEL_ALPHA))
                        .child(
                            div()
                                .h(px(48.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .px(px(16.0))
                                .border_b_1()
                                .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .flex_1()
                                        .flex()
                                        .items_center()
                                        .gap(px(12.0))
                                        .child(
                                            div()
                                                .max_w(px(400.0))
                                                .truncate()
                                                .text_size(px(14.0))
                                                .text_color(rgb(theme.text))
                                                .child(player.file_name.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_size(px(11.0))
                                                .font_family(settings_mono_font_family(
                                                    self.settings_store.settings(),
                                                ))
                                                .text_color(rgb(theme.text_muted))
                                                .child(format!("{}x{}", player.width, player.height)),
                                        ),
                                )
                                .child(
                                    div()
                                        .size(px(28.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(if player.search_visible {
                                            rgb(theme.bg_hover)
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, window, cx| {
                                                if let Some(player) =
                                                    this.terminal_cast_player.as_mut()
                                                {
                                                    player.search_visible = !player.search_visible;
                                                    player.search_focused = player.search_visible;
                                                    if !player.search_visible {
                                                        player.search_query.clear();
                                                        this.update_terminal_cast_search(cx);
                                                    }
                                                }
                                                this.terminal_command_bar_focused = false;
                                                this.ime_marked_text = None;
                                                window.focus(&this.focus_handle);
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        )
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Search,
                                            16.0,
                                            if player.search_visible {
                                                rgb(theme.text)
                                            } else {
                                                rgb(theme.text_muted)
                                            },
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text_muted))
                                        .when(!player.search_query.is_empty(), |label| {
                                            label.child(format!(
                                                "{} {}",
                                                search_results.len(),
                                                self.i18n.t("terminal.recording.matches")
                                            ))
                                        }),
                                )
                                .child(
                                    div()
                                        .size(px(28.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.close_terminal_cast_player(cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::X,
                                            16.0,
                                            rgb(theme.text_muted),
                                        )),
                                ),
                        )
                        .when(player.search_visible, |player_view| {
                            player_view.child(
                                div()
                                    .flex_none()
                                    .border_b_1()
                                    .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                    .bg(rgba((theme.bg_panel << 8) | 0x99))
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .child(
                                        div()
                                            .max_w(px(512.0))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Search,
                                                14.0,
                                                rgb(theme.text_muted),
                                            ))
                                            .child(text_input_anchor_probe(
                                                search_target.anchor_id(),
                                                div()
                                                    .h(px(30.0))
                                                    .flex_1()
                                                    .min_w(px(0.0))
                                                    .flex()
                                                    .items_center()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(if player.search_focused {
                                                        rgba((theme.accent << 8) | 0x80)
                                                    } else {
                                                        rgba((theme.border << 8) | 0x80)
                                                    })
                                                    .bg(rgba((theme.bg_hover << 8) | 0x99))
                                                    .px(px(8.0))
                                                    .text_size(px(13.0))
                                                    .text_color(if search_empty {
                                                        rgb(theme.text_muted)
                                                    } else {
                                                        rgb(theme.text)
                                                    })
                                                    .cursor_text()
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(
                                                            |this, _event, window, cx| {
                                                                if let Some(player) = this
                                                                    .terminal_cast_player
                                                                    .as_mut()
                                                                {
                                                                    player.search_focused = true;
                                                                    player.search_visible = true;
                                                                }
                                                                this.terminal_command_bar_focused =
                                                                    false;
                                                                this.ime_marked_text = None;
                                                                window.focus(&this.focus_handle);
                                                                cx.stop_propagation();
                                                                cx.notify();
                                                            },
                                                        ),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .overflow_hidden()
                                                            .child(search_text)
                                                            .when_some(
                                                                search_marked,
                                                                |input, marked| {
                                                                    input.child(
                                                                        div()
                                                                            .underline()
                                                                            .text_color(rgb(
                                                                                theme.text,
                                                                            ))
                                                                            .child(
                                                                                marked.to_string(),
                                                                            ),
                                                                    )
                                                                },
                                                            )
                                                            .when(
                                                                player.search_focused,
                                                                |input| {
                                                                    input.child(text_caret(
                                                                        &self.tokens,
                                                                        self.new_connection_caret_visible,
                                                                    ))
                                                                },
                                                            ),
                                                    ),
                                                move |anchor, _window, cx| {
                                                    let _ = workspace.update(cx, |this, cx| {
                                                        this.update_text_input_anchor(anchor, cx);
                                                    });
                                                },
                                            ))
                                            .when(!player.search_query.is_empty(), |row| {
                                                row.child(
                                                    div()
                                                        .flex_none()
                                                        .text_size(px(12.0))
                                                        .text_color(rgb(theme.text_muted))
                                                        .child(format!(
                                                            "{} {}",
                                                            search_results.len(),
                                                            self.i18n
                                                                .t("terminal.recording.matches")
                                                        )),
                                                )
                                            }),
                                    ),
                            )
                        })
                        .when(!player.search_query.is_empty(), |player_view| {
                            player_view.child(
                                div()
                                    .flex_none()
                                    .max_h(px(118.0))
                                    .overflow_hidden()
                                    .border_b_1()
                                    .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                    .bg(rgba((theme.bg_panel << 8) | 0x99))
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .when(search_results.is_empty(), |panel| {
                                        panel.child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(
                                                    self.i18n
                                                        .t("terminal.recording.search_no_results"),
                                                ),
                                        )
                                    })
                                    .when(!search_results.is_empty(), |panel| {
                                        panel.child(div().flex().flex_col().gap(px(2.0)).children(
                                            search_results.iter().map(|result| {
                                                let at = result.at;
                                                let snippet = result.snippet.clone();
                                                div()
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .rounded_md()
                                                    .px(px(6.0))
                                                    .cursor_pointer()
                                                    .hover(move |style| {
                                                        style.bg(rgb(theme.bg_hover))
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(
                                                            move |this, _event, _window, cx| {
                                                                if let Some(player) =
                                                                    &this.terminal_cast_player
                                                                {
                                                                    this.seek_terminal_cast(
                                                                        at / player
                                                                            .duration
                                                                            .max(1.0),
                                                                        cx,
                                                                    );
                                                                }
                                                                cx.stop_propagation();
                                                            },
                                                        ),
                                                    )
                                                    .child(
                                                        div()
                                                            .w(px(48.0))
                                                            .text_size(px(11.0))
                                                            .font_family(settings_mono_font_family(
                                                                self.settings_store.settings(),
                                                            ))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(format_cast_time(at)),
                                                    )
                                                    .child(Self::render_lucide_icon(
                                                        LucideIcon::ChevronRight,
                                                        12.0,
                                                        rgb(theme.text_muted),
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w(px(0.0))
                                                            .truncate()
                                                            .text_size(px(11.0))
                                                            .font_family(settings_mono_font_family(
                                                                self.settings_store.settings(),
                                                            ))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(snippet),
                                                    )
                                            }),
                                        ))
                                    }),
                            )
                        })
                        .child(div().flex_1().min_h(px(0.0)).child(
                            pane.map(|pane| pane.into_any_element()).unwrap_or_else(|| {
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("terminal.recording.player_empty"))
                                    .into_any_element()
                            }),
                        ))
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .p(px(14.0))
                                .border_t_1()
                                .border_color(rgba((theme.border << 8) | PLAYER_BORDER_ALPHA))
                                .child(select_anchor_probe(
                                    SelectAnchorId::TerminalCastSeekbar,
                                    div()
                                        .h(px(10.0))
                                        .flex()
                                        .items_center()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this, event: &MouseDownEvent, _window, cx| {
                                                    this.terminal_cast_seek_dragging = true;
                                                    this.apply_terminal_cast_seek_from_x(
                                                        f32::from(event.position.x),
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                },
                                            ),
                                        )
                                        .child(
                                            div()
                                                .h(px(6.0))
                                                .w_full()
                                                .rounded_full()
                                                .overflow_hidden()
                                                .bg(rgba((theme.bg_panel << 8) | 0xcc))
                                                .child(
                                                    div()
                                                        .h_full()
                                                        .w(relative(progress))
                                                        .rounded_full()
                                                        .bg(rgb(theme.accent)),
                                                ),
                                        ),
                                    {
                                        let workspace = cx.entity();
                                        move |anchor, _window, cx| {
                                            let _ = workspace.update(cx, |this, _cx| {
                                                this.select_anchors.insert(anchor.id, anchor);
                                            });
                                        }
                                    },
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(10.0))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    terminal_cast_player_button(
                                                        &self.tokens,
                                                        if player.playing {
                                                            LucideIcon::Pause
                                                        } else {
                                                            LucideIcon::Play
                                                        },
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.toggle_terminal_cast_playback(cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                )
                                                .child(
                                                    terminal_cast_speed_button(
                                                        &self.tokens,
                                                        "0.5x",
                                                        player.speed == 0.5,
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.set_terminal_cast_speed(0.5, cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                )
                                                .child(
                                                    terminal_cast_speed_button(
                                                        &self.tokens,
                                                        "1x",
                                                        player.speed == 1.0,
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.set_terminal_cast_speed(1.0, cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                )
                                                .child(
                                                    terminal_cast_speed_button(
                                                        &self.tokens,
                                                        "2x",
                                                        player.speed == 2.0,
                                                    )
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _event, _window, cx| {
                                                            this.set_terminal_cast_speed(2.0, cx);
                                                            cx.stop_propagation();
                                                        }),
                                                    ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    terminal_cast_text_button(&self.tokens, "-10s")
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                |this, _event, _window, cx| {
                                                                    if let Some(player) =
                                                                        &this.terminal_cast_player
                                                                    {
                                                                        let target = (player
                                                                            .position
                                                                            - 10.0)
                                                                            / player
                                                                                .duration
                                                                                .max(1.0);
                                                                        this.seek_terminal_cast(
                                                                            target, cx,
                                                                        );
                                                                    }
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        ),
                                                )
                                                .child(
                                                    terminal_cast_text_button(&self.tokens, "+10s")
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(
                                                                |this, _event, _window, cx| {
                                                                    if let Some(player) =
                                                                        &this.terminal_cast_player
                                                                    {
                                                                        let target = (player
                                                                            .position
                                                                            + 10.0)
                                                                            / player
                                                                                .duration
                                                                                .max(1.0);
                                                                        this.seek_terminal_cast(
                                                                            target, cx,
                                                                        );
                                                                    }
                                                                    cx.stop_propagation();
                                                                },
                                                            ),
                                                        ),
                                                ),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_terminal_broadcast_menu(
        &self,
        placement: TerminalBroadcastMenuPlacement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let entries = self.terminal_broadcast_entries();
        let active_pane_id = self.active_pane_id();
        let selectable = entries
            .iter()
            .filter(|(pane_id, _, _)| Some(*pane_id) != active_pane_id)
            .map(|(pane_id, _, _)| *pane_id)
            .collect::<Vec<_>>();
        let all_selected = !selectable.is_empty()
            && selectable
                .iter()
                .all(|pane_id| self.terminal_broadcast_targets.contains(pane_id));

        let mut menu = div()
            .absolute()
            .right(px(12.0))
            .w(px(260.0))
            .max_h(px(320.0))
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_elevated << 8) | 0xf2))
            .shadow_lg()
            .p(px(6.0))
            .text_size(px(12.0))
            .child(
                div()
                    .px(px(6.0))
                    .py(px(4.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.broadcast.select_targets")),
            );
        menu = match placement {
            TerminalBroadcastMenuPlacement::Bottom(offset) => menu.bottom(px(offset)),
            TerminalBroadcastMenuPlacement::Top(offset) => menu.top(px(offset)),
        };

        if entries.len() <= 1 {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .py(px(12.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("terminal.broadcast.no_targets")),
            );
        } else {
            for (pane_id, label, kind) in entries {
                let is_current = Some(pane_id) == active_pane_id;
                let checked = self.terminal_broadcast_targets.contains(&pane_id);
                let badge = match kind {
                    TabKind::LocalTerminal => self.i18n.t("terminal.typeLocal"),
                    TabKind::SshTerminal => self.i18n.t("terminal.typeSsh"),
                    _ => String::new(),
                };
                let row_color = if is_current {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                };
                menu = menu.child(
                    div()
                        .h(px(30.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .rounded_md()
                        .text_color(row_color)
                        .when(!is_current, |row| {
                            row.cursor_pointer()
                                .hover(move |style| style.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        if this.terminal_broadcast_targets.remove(&pane_id) {
                                            if this.terminal_broadcast_targets.is_empty() {
                                                this.terminal_broadcast_enabled = false;
                                            }
                                        } else {
                                            this.terminal_broadcast_targets.insert(pane_id);
                                            this.terminal_broadcast_enabled = true;
                                        }
                                        this.terminal_broadcast_menu_open = true;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                        })
                        .child(if checked {
                            Self::render_lucide_icon(LucideIcon::Check, 12.0, rgba(0xfb923cff))
                        } else if is_current {
                            div()
                                .size(px(12.0))
                                .rounded_full()
                                .bg(rgba(0xfb923cff))
                                .into_any_element()
                        } else {
                            div().size(px(12.0)).into_any_element()
                        })
                        .child(div().flex_1().truncate().child(label))
                        .when(!badge.is_empty(), |row| {
                            row.child(
                                div()
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded_md()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .bg(rgba((theme.bg_panel << 8) | 0x99))
                                    .child(badge),
                            )
                        })
                        .when(is_current, |row| {
                            row.child(
                                div()
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded_md()
                                    .text_size(px(10.0))
                                    .text_color(rgba(0xfb923cff))
                                    .bg(rgba(0xf9731626))
                                    .child(self.i18n.t("terminal.broadcast.current")),
                            )
                        }),
                );
            }

            menu = menu.child(
                div()
                    .mt(px(4.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .child(
                        div()
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.text_color(rgb(theme.accent)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if all_selected {
                                        this.terminal_broadcast_enabled = false;
                                        this.terminal_broadcast_targets.clear();
                                    } else {
                                        this.terminal_broadcast_targets =
                                            selectable.iter().copied().collect();
                                        this.terminal_broadcast_enabled =
                                            !this.terminal_broadcast_targets.is_empty();
                                    }
                                    this.terminal_broadcast_menu_open = true;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(if all_selected {
                                self.i18n.t("terminal.broadcast.deselect_all")
                            } else {
                                self.i18n.t("terminal.broadcast.select_all")
                            }),
                    )
                    .when(self.terminal_broadcast_enabled, |footer| {
                        footer.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(0xfb923cff))
                                .child(self.i18n.t("terminal.broadcast.target_count")),
                        )
                    }),
            );
        }

        menu.into_any_element()
    }
}

fn terminal_command_executable(command: &str) -> Option<String> {
    let segment = command
        .trim()
        .split("&&")
        .flat_map(|part| part.split("||"))
        .flat_map(|part| part.split(';'))
        .find(|part| !part.trim().is_empty())?;
    let tokens = shell_words(segment);
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].trim();
        if token.is_empty()
            || token.starts_with('-')
            || token
                .split_once('=')
                .is_some_and(|(name, _)| is_shell_assignment_name(name))
        {
            index += 1;
            continue;
        }
        if matches!(token, "sudo" | "command" | "exec" | "env") {
            index += 1;
            continue;
        }
        return token.rsplit('/').next().map(|name| name.to_lowercase());
    }
    None
}

fn shell_words(segment: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in segment.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_shell_assignment_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(super) fn classify_command_risk(command: &str) -> Option<&'static str> {
    let lower = command.to_lowercase();
    let high_risk = [
        "kubectl delete",
        "systemctl stop",
        "systemctl restart",
        "systemctl disable",
        "systemctl kill",
        "docker rm",
        "docker rmi",
        "docker system prune",
        "docker container prune",
        "docker volume prune",
        "docker network prune",
        "shutdown",
        "reboot",
        "halt",
        "poweroff",
        "mkfs",
        "chmod -r",
        "chown -r",
    ];
    if (lower.contains("rm -rf") || lower.contains("rm -fr"))
        || lower.contains("kill -9")
        || lower.contains("killall -9")
        || lower.contains("dd ") && lower.contains("of=")
        || high_risk.iter().any(|pattern| lower.contains(pattern))
    {
        return Some("high");
    }
    if lower.split_whitespace().any(|token| token == "sudo") || lower.contains("chmod 777") {
        return Some("medium");
    }
    None
}
