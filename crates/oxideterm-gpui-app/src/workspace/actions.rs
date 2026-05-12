use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_gpui_ui::text_input::text_input_anchor_probe;

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

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Launcher)
            && self.launcher.focused_input.is_some()
        {
            let _ = self.handle_launcher_key(event, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Graphics)
            && self.graphics.focused_input.is_some()
        {
            let _ = self.handle_graphics_key(event, cx);
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

    pub(in crate::workspace) fn terminal_broadcast_entries(
        &self,
    ) -> Vec<(PaneId, String, TabKind)> {
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
