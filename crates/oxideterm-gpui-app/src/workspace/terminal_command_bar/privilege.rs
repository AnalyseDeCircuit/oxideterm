// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn active_privilege_scope_credentials(
        &self,
        cx: &App,
    ) -> Option<(String, Vec<SavedPrivilegeCredential>)> {
        match self.active_terminal_kind(cx)? {
            oxideterm_terminal::TerminalSessionKind::LocalPty => {
                // Local shell sudo/su prompts have no SavedConnection owner. Use a
                // dedicated store scope so secrets are never confused with SSH
                // connection credentials.
                let connection_id = LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string();
                let credentials = self
                    .connection_store
                    .list_privilege_credentials(&connection_id)
                    .unwrap_or_default();
                log_privilege_prompt_helper(format_args!(
                    "scope resolved: scope=local credentials_count={}",
                    credentials.len()
                ));
                Some((connection_id, credentials))
            }
            oxideterm_terminal::TerminalSessionKind::SshPty => {
                let Some(session_id) = self.active_terminal_session_id(cx) else {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh tab has no active terminal session"
                    ));
                    return None;
                };
                let Some(node_id) = self
                    .workspace_runtime
                    .read(cx)
                    .ssh_terminal_node_id(session_id)
                else {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh terminal session has no node mapping"
                    ));
                    return None;
                };
                let connection_id = self
                    .ssh_privilege_scope_id_for_node(&node_id)
                    .filter(|id| self.connection_store.get(id).is_some())
                    .unwrap_or_else(|| SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string());
                let mut credentials = self
                    .connection_store
                    .list_privilege_credentials(&connection_id)
                    .unwrap_or_default();
                if connection_id != SSH_COMMON_PRIVILEGE_SCOPE_ID {
                    credentials.extend(
                        self.connection_store
                            .list_privilege_credentials(SSH_COMMON_PRIVILEGE_SCOPE_ID)
                            .unwrap_or_default(),
                    );
                }
                log_privilege_prompt_helper(format_args!(
                    "scope resolved: scope=ssh credentials_count={}",
                    credentials.len()
                ));
                Some((connection_id, credentials))
            }
            _ => None,
        }
    }

    pub(super) fn ssh_privilege_scope_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        let node_saved_connection_id = self
            .ssh_nodes
            .get(node_id)
            .and_then(|node| node.saved_connection_id.as_deref());
        let node_origin = self
            .node_router
            .node_metadata(node_id)
            .map(|snapshot| snapshot.origin);
        let has_origin_saved_owner = node_origin
            .as_ref()
            .and_then(NodeOrigin::saved_connection_id)
            .is_some_and(|connection_id| !connection_id.trim().is_empty());
        // SSH privilege credentials are scoped to the node owner. Do not recover
        // a saved connection by matching host/port/user/title or by using the
        // runtime connection id; reused node terminals must share this same owner.
        let scope_id = saved_ssh_privilege_scope_id(node_saved_connection_id, node_origin.as_ref());
        log_privilege_prompt_helper(format_args!(
            "ssh scope lookup: has_node_saved_owner={} has_runtime_snapshot={} has_origin_saved_owner={} resolved={}",
            node_saved_connection_id.is_some_and(|connection_id| !connection_id.trim().is_empty()),
            node_origin.is_some(),
            has_origin_saved_owner,
            scope_id.is_some()
        ));
        scope_id
    }

    pub(super) fn active_privilege_prompt_state(
        &self,
        cx: &App,
    ) -> Option<PrivilegePromptHelperState> {
        let Some(active_tab) = self.active_tab(cx) else {
            log_privilege_prompt_helper(format_args!("state unavailable: no active tab"));
            return None;
        };
        if !tab_kind_allows_privilege_prompt_helper(&active_tab.kind) {
            log_privilege_prompt_helper(format_args!(
                "state unavailable: unsupported tab_kind={}",
                tab_kind_privilege_scope_name(&active_tab.kind)
            ));
            return None;
        }
        let Some(active_pane) = self.active_pane(cx) else {
            log_privilege_prompt_helper(format_args!("state unavailable: no active pane"));
            return None;
        };
        let pane = active_pane.read(cx);
        if pane.ai_screen_is_alternate_buffer() {
            // Full-screen applications own Enter and may display arbitrary
            // password-like text. Privilege assistance is shell-screen only.
            log_privilege_prompt_helper(format_args!("state unavailable: alternate screen active"));
            return None;
        }
        let visible_text = pane.privilege_prompt_text_snapshot();
        let visible_shape = privilege_prompt_text_shape(&visible_text);
        let tracked_prompt = pane
            .privilege_prompt_snapshot()
            .map(|snapshot| snapshot.prompt);
        let tracked_prompt_kind = tracked_prompt
            .as_ref()
            .map(privilege_prompt_kind_name)
            .unwrap_or("none");
        if tracked_prompt.is_none() && pane.privilege_prompt_fallback_suppressed() {
            log_privilege_prompt_helper(format_args!(
                "state unavailable: fallback suppressed visible_chars={}",
                visible_shape.chars
            ));
            return None;
        }
        // Tauri keeps the prompt state alive even when credential metadata
        // cannot be loaded. Do not let a missing credential row or transient
        // keychain/config error suppress the detected sudo/su prompt.
        let Some((connection_id, credentials)) = self.active_privilege_scope_credentials(cx) else {
            log_privilege_prompt_helper(format_args!(
                "state unavailable: no credential scope tracked_prompt={} visible_chars={}",
                tracked_prompt_kind, visible_shape.chars
            ));
            return None;
        };
        let state = build_privilege_prompt_helper_state_with_tracked_prompt(
            connection_id,
            &credentials,
            &visible_text,
            tracked_prompt,
        );
        match &state {
            Some(state) => log_privilege_prompt_helper(format_args!(
                "state ready: prompt_kind={} matches_count={} credentials_count={} tracked_prompt={} visible_chars={}",
                privilege_prompt_kind_name(&state.prompt),
                state.matches.len(),
                credentials.len(),
                tracked_prompt_kind,
                visible_shape.chars
            )),
            None => log_privilege_prompt_helper(format_args!(
                "state unavailable: no prompt match credentials_count={} tracked_prompt={} visible_chars={} visible_lines={} has_ascii_colon={} has_fullwidth_colon={} ends_colon={} contains_sudo_marker={} starts_sudo_marker={} contains_password_word={} contains_cjk_password={} contains_escape={}",
                credentials.len(),
                tracked_prompt_kind,
                visible_shape.chars,
                visible_shape.lines,
                visible_shape.has_ascii_colon,
                visible_shape.has_fullwidth_colon,
                visible_shape.ends_with_prompt_colon,
                visible_shape.contains_sudo_marker,
                visible_shape.starts_with_sudo_marker,
                visible_shape.contains_password_word,
                visible_shape.contains_cjk_password,
                visible_shape.contains_escape
            )),
        }
        state
    }

    pub(in crate::workspace) fn sync_active_privilege_prompt_inline_hint(
        &mut self,
        cx: &mut App,
    ) -> bool {
        let Some(active_pane) = self.active_pane(cx) else {
            return false;
        };
        if self.privilege_credential_picker.is_some()
            && !self.privilege_credential_picker_is_current(cx)
        {
            self.dismiss_privilege_credential_picker();
        }
        let hint = self.active_privilege_prompt_inline_hint(cx);
        let custom_prompt = self
            .active_privilege_prompt_state(cx)
            .map(|state| state.prompt)
            .filter(|prompt| matches!(prompt, PrivilegePromptMatch::Custom { .. }));
        active_pane.update(cx, |pane, cx| {
            pane.set_privilege_prompt_inline_hint(hint, custom_prompt, cx)
        })
    }

    pub(super) fn active_privilege_prompt_inline_hint(&self, cx: &App) -> Option<String> {
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("hint unavailable: no prompt state"));
            return None;
        };
        let prompt_kind = privilege_prompt_kind_name(&state.prompt);
        if state.matches.is_empty() {
            log_privilege_prompt_helper(format_args!(
                "hint suppressed: prompt_kind={} matches_count={}",
                prompt_kind,
                state.matches.len()
            ));
            return None;
        }
        log_privilege_prompt_helper(format_args!(
            "hint ready: prompt_kind={} matches_count={}",
            prompt_kind,
            state.matches.len()
        ));
        Some(self.i18n.t(if state.matches.len() == 1 {
            "terminal.privilege_helper.inline_fill_hint"
        } else {
            "terminal.privilege_helper.choose_hint"
        }))
    }

    pub(in crate::workspace) fn handle_privilege_prompt_helper_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = event.keystroke.modifiers;
        if event.keystroke.key.as_str() != "enter"
            || modifiers.platform
            || modifiers.control
            || modifiers.alt
            || modifiers.shift
        {
            return false;
        }

        let Some(active_pane) = self.active_pane(cx) else {
            return false;
        };
        if !active_pane.read(cx).has_privilege_prompt_inline_hint() {
            // Enter is confirmation only after the same pane exposes the
            // current fill affordance. A fallback screen match alone must not
            // consume input in a full-screen application.
            return false;
        }

        log_privilege_prompt_helper(format_args!("root enter: evaluating privilege helper"));
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("root enter: no prompt state"));
            return false;
        };
        self.confirm_or_choose_privilege_credential(state, window, cx)
    }

    pub(in crate::workspace) fn handle_active_privilege_prompt_submit_request(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active_pane) = self.active_pane(cx) else {
            return false;
        };
        let requested =
            active_pane.update(cx, |pane, _cx| pane.take_privilege_prompt_submit_request());
        if !requested {
            return false;
        }

        log_privilege_prompt_helper(format_args!(
            "terminal submit request: evaluating privilege helper"
        ));
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("terminal submit request: no prompt state"));
            return false;
        };
        self.confirm_or_choose_privilege_credential(state, window, cx)
    }

    fn confirm_or_choose_privilege_credential(
        &mut self,
        state: PrivilegePromptHelperState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if privilege_prompt_state_allows_confirmed_fill(&state) {
            self.fill_privilege_prompt_match(state.matches[0].clone(), state.prompt, window, cx);
        } else if !state.matches.is_empty() {
            let (Some(pane_id), Some(session_id), Some(pane)) = (
                self.active_pane_id(cx),
                self.active_terminal_session_id(cx),
                self.active_pane(cx),
            ) else {
                return false;
            };
            self.dismiss_workspace_context_menus(cx);
            self.selectable_text_scroll_handle("privilege-credential-picker-scroll")
                .scroll_to_top_of_item(0);
            self.privilege_credential_picker = Some(PrivilegeCredentialPicker {
                pane_id,
                session_id,
                prompt_generation: pane.read(cx).privilege_prompt_generation(),
                state,
                selected: 0,
            });
            window.focus(&self.focus_handle, cx);
            cx.notify();
        } else {
            return false;
        }
        true
    }

    pub(in crate::workspace) fn dismiss_privilege_credential_picker(&mut self) -> bool {
        self.privilege_credential_picker.take().is_some()
    }

    pub(in crate::workspace) fn privilege_credential_picker_is_current(&self, cx: &App) -> bool {
        let Some(picker) = &self.privilege_credential_picker else {
            return false;
        };
        let (Some(pane_id), Some(session_id), Some(pane), Some(state)) = (
            self.active_pane_id(cx),
            self.active_terminal_session_id(cx),
            self.active_pane(cx),
            self.active_privilege_prompt_state(cx),
        ) else {
            return false;
        };
        picker.is_current(
            pane_id,
            session_id,
            pane.read(cx).privilege_prompt_generation(),
            &state,
        )
    }

    fn choose_privilege_credential(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.privilege_credential_picker_is_current(cx) {
            self.dismiss_privilege_credential_picker();
            cx.notify();
            return;
        }
        let Some(picker) = self.privilege_credential_picker.take() else {
            return;
        };
        if let Some(matched) = picker.state.matches.get(index) {
            self.fill_privilege_prompt_match(matched.clone(), picker.state.prompt, window, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_privilege_credential_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.privilege_credential_picker.is_none() {
            return false;
        }
        if !self.privilege_credential_picker_is_current(cx) {
            self.dismiss_privilege_credential_picker();
            cx.notify();
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt || modifiers.shift {
            self.dismiss_privilege_credential_picker();
            self.focus_active_pane(window, cx);
            cx.notify();
            return false;
        }
        match key {
            "up" | "arrowup" | "down" | "arrowdown" => {
                let picker = self.privilege_credential_picker.as_mut().unwrap();
                let count = picker.state.matches.len();
                picker.selected = if matches!(key, "up" | "arrowup") {
                    (picker.selected + count - 1) % count
                } else {
                    (picker.selected + 1) % count
                };
                let selected = picker.selected;
                self.selectable_text_scroll_handle("privilege-credential-picker-scroll")
                    .scroll_to_item(selected);
                cx.notify();
                true
            }
            "enter" => {
                let index = self.privilege_credential_picker.as_ref().unwrap().selected;
                self.choose_privilege_credential(index, window, cx);
                true
            }
            "escape" => {
                self.dismiss_privilege_credential_picker();
                self.focus_active_pane(window, cx);
                cx.notify();
                true
            }
            _ => {
                self.dismiss_privilege_credential_picker();
                self.focus_active_pane(window, cx);
                cx.notify();
                false
            }
        }
    }

    pub(super) fn render_privilege_credential_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(picker) = &self.privilege_credential_picker else {
            return div().into_any_element();
        };
        let scroll = self.selectable_text_scroll_handle("privilege-credential-picker-scroll");
        let mut rows = div()
            .id("privilege-credential-picker-scroll")
            .max_h(px(288.0))
            .selectable_overflow_y_scroll(&scroll)
            .flex()
            .flex_col();
        for (index, matched) in picker.state.matches.iter().enumerate() {
            let source = if matched.connection_id == SSH_COMMON_PRIVILEGE_SCOPE_ID {
                self.i18n
                    .t("settings_view.privilege_credentials.ssh_common_scope")
            } else if matched.connection_id == LOCAL_SHELL_PRIVILEGE_CONNECTION_ID {
                self.i18n
                    .t("settings_view.privilege_credentials.local_scope")
            } else {
                self.connection_store
                    .get(&matched.connection_id)
                    .map(|connection| connection.name.clone())
                    .unwrap_or_default()
            };
            let credential = self
                .connection_store
                .list_privilege_credentials(&matched.connection_id)
                .unwrap_or_default()
                .into_iter()
                .find(|credential| credential.id == matched.credential_id);
            let kind = credential
                .map(|credential| self.settings_privilege_kind_label(credential.kind))
                .unwrap_or_default();
            rows = rows.child(
                oxideterm_gpui_ui::select::select_option(
                    &self.tokens,
                    format!("{} · {} · {}", matched.label, kind, source),
                    index == picker.selected,
                )
                .flex_none()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.choose_privilege_credential(index, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        context_menu_pointer_event_boundary(
            command_panel(
                &self.tokens,
                CommandPanelOptions::new()
                    .width(460.0)
                    .max_width_ratio(0.96)
                    .terminal_owned(),
            )
            .absolute()
            .bottom(px(TERMINAL_COMMAND_TOOLBAR_HEIGHT))
            .left(px(12.0))
            .occlude(),
        )
        .child(
            div()
                .px_3()
                .py_2()
                .child(self.i18n.t("terminal.privilege_helper.choose_title")),
        )
        .child(
            div()
                .id("privilege-credential-picker")
                .relative()
                .max_h(px(288.0))
                .child(rows)
                .vertical_scrollbar(&scroll),
        )
        .into_any_element()
    }

    pub(super) fn fill_privilege_prompt_match(
        &mut self,
        matched: MatchedPrivilegeCredential,
        confirmed_prompt: PrivilegePromptMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane_id) = self.active_pane_id(cx) else {
            return;
        };
        let Some(session_id) = self.active_terminal_session_id(cx) else {
            return;
        };
        let Some(pane) = self.active_pane(cx) else {
            return;
        };
        let generation = pane.read(cx).privilege_prompt_generation();
        log_privilege_prompt_helper(format_args!("fill: loading scoped credential secret"));
        let secret = match self
            .connection_store
            .get_privilege_credential_secret(&matched.connection_id, &matched.credential_id)
        {
            Ok(secret) => secret,
            Err(error) => {
                log_privilege_prompt_helper(format_args!("fill: secret load failed"));
                self.push_command_palette_toast(
                    self.i18n.t("terminal.privilege_helper.load_failed"),
                    Some(error.to_string()),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                cx.notify();
                return;
            }
        };
        log_privilege_prompt_helper(format_args!("fill: secret loaded"));
        // The newline-bearing buffer is the only owned cleartext copy in the
        // GPUI layer. It is zeroized after the PTY write attempt, matching the
        // Tauri click-only secret handoff without involving command history.
        let secret_line = zeroize::Zeroizing::new(format!("{}\n", secret.expose_secret()));
        let sent = self.active_pane_id(cx) == Some(pane_id)
            && self.active_terminal_session_id(cx) == Some(session_id)
            && pane.update(cx, |pane, cx| {
                pane.send_privilege_secret_input_bytes(
                    secret_line.as_bytes(),
                    confirmed_prompt,
                    generation,
                    cx,
                )
            });
        log_privilege_prompt_helper(format_args!("fill: write attempted sent={sent}"));
        if !sent {
            self.push_command_palette_toast(
                self.i18n.t("terminal.privilege_helper.send_failed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_terminal_surface(
        &mut self,
        root_pane: &PaneNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let terminal = self.render_pane_tree(root_pane, window, cx);
        let recording_status = self.active_terminal_recording_status(cx);
        let recording_active = recording_status.state != TerminalRecordingState::Idle;
        if self.active_pane(cx).is_none()
            || !self.settings_store.settings().terminal.command_bar.enabled
        {
            return div()
                .size_full()
                .relative()
                .child(terminal)
                .when(recording_active, |surface| {
                    surface.child(self.render_terminal_recording_controls(recording_status, cx))
                })
                .into_any_element();
        }

        let panel_available_height = self
            .terminal
            .read(cx)
            .quick_commands
            .panel
            .available_height
            .clone();
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
            .when(
                self.settings_store
                    .settings()
                    .terminal
                    .command_bar
                    .quick_bar_enabled,
                |surface| surface.child(self.render_terminal_quick_bar(window, cx)),
            )
            .child(self.render_terminal_command_bar(cx))
            .child(self.render_terminal_quick_commands_panel(cx))
            // The toolbar is the sender header. Hidden, compact, and expanded
            // layouts all retain the same document and running jobs below it.
            .child(self.render_terminal_command_sender_panel(window, cx))
            .on_children_prepainted(move |bounds, window, _| {
                // Only the terminal and dock share this budget; all other rows have already
                // consumed their actual layout heights, including an optional quick bar.
                if bounds.len() >= 4 {
                    let available =
                        f32::from(bounds[0].size.height + bounds[bounds.len() - 2].size.height);
                    if (panel_available_height.get() - available).abs() > 0.5 {
                        panel_available_height.set(available);
                        window.refresh();
                    }
                }
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn render_detached_terminal_surface(
        &mut self,
        tab_id: TabId,
        root_pane: &PaneNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Detached windows share terminal pane entities with the workspace, but
        // the command bar still uses the main active-tab pipeline. Keep the
        // first detachable surface pane-only so commands cannot target the
        // wrong tab while the UI ownership model is being made window-aware.
        div()
            .size_full()
            .relative()
            .child(self.render_pane_tree_for_tab(Some(tab_id), root_pane, window, cx))
            .into_any_element()
    }
}
