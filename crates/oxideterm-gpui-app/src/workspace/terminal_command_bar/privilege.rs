// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn active_privilege_scope_credentials(
        &self,
        cx: &App,
    ) -> Option<(String, Vec<SavedPrivilegeCredential>)> {
        let Some(active_tab) = self.active_tab(cx) else {
            log_privilege_prompt_helper(format_args!("scope unavailable: no active tab"));
            return None;
        };
        match &active_tab.kind {
            TabKind::LocalTerminal => {
                if self.active_tab_has_serial_terminal(cx) {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: local tab is backed by a serial terminal"
                    ));
                    return None;
                }
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
            TabKind::SshTerminal => {
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
                let Some(connection_id) = self.ssh_privilege_scope_id_for_node(&node_id) else {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh node has no saved owner"
                    ));
                    return None;
                };
                if self.connection_store.get(&connection_id).is_none() {
                    log_privilege_prompt_helper(format_args!(
                        "scope unavailable: ssh saved owner is missing from connection store"
                    ));
                    return None;
                }
                let credentials = self
                    .connection_store
                    .list_privilege_credentials(&connection_id)
                    .unwrap_or_default();
                log_privilege_prompt_helper(format_args!(
                    "scope resolved: scope=ssh credentials_count={}",
                    credentials.len()
                ));
                Some((connection_id, credentials))
            }
            tab_kind => {
                log_privilege_prompt_helper(format_args!(
                    "scope unavailable: tab_kind={}",
                    tab_kind_privilege_scope_name(tab_kind)
                ));
                None
            }
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
        let hint = self.active_privilege_prompt_inline_hint(cx);
        active_pane.update(cx, |pane, cx| {
            pane.set_privilege_prompt_inline_hint(hint, cx)
        })
    }

    pub(super) fn active_privilege_prompt_inline_hint(&self, cx: &App) -> Option<String> {
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("hint unavailable: no prompt state"));
            return None;
        };
        let prompt_kind = privilege_prompt_kind_name(&state.prompt);
        if !privilege_prompt_state_allows_confirmed_fill(&state) {
            log_privilege_prompt_helper(format_args!(
                "hint suppressed: prompt_kind={} matches_count={}",
                prompt_kind,
                state.matches.len()
            ));
            return None;
        }
        log_privilege_prompt_helper(format_args!(
            "hint ready: prompt_kind={} matches_count=1",
            prompt_kind
        ));
        Some(self.i18n.t("terminal.privilege_helper.inline_fill_hint"))
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

        log_privilege_prompt_helper(format_args!("root enter: evaluating privilege helper"));
        let Some(state) = self.active_privilege_prompt_state(cx) else {
            log_privilege_prompt_helper(format_args!("root enter: no prompt state"));
            return false;
        };
        if !privilege_prompt_state_allows_confirmed_fill(&state) {
            log_privilege_prompt_helper(format_args!(
                "root enter: ignored match_count={}",
                state.matches.len()
            ));
            return false;
        };
        let [matched] = state.matches.as_slice() else {
            return false;
        };
        // The inline hint is the confirmation affordance: Enter is accepted only
        // when prompt detection produces exactly one scoped credential. Bare
        // `Password:` prompts therefore work for macOS sudo without guessing
        // between multiple saved sudo/su candidates.
        log_privilege_prompt_helper(format_args!(
            "root enter: filling prompt_kind={}",
            privilege_prompt_kind_name(&state.prompt)
        ));
        self.fill_privilege_prompt_match(matched.clone(), window, cx);
        true
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
        if !privilege_prompt_state_allows_confirmed_fill(&state) {
            log_privilege_prompt_helper(format_args!(
                "terminal submit request: ignored match_count={}",
                state.matches.len()
            ));
            return false;
        };
        let [matched] = state.matches.as_slice() else {
            return false;
        };
        // TerminalPane captures Enter before it is written as a plain newline;
        // Workspace still owns the secret lookup and one-shot PTY handoff.
        log_privilege_prompt_helper(format_args!(
            "terminal submit request: filling prompt_kind={}",
            privilege_prompt_kind_name(&state.prompt)
        ));
        self.fill_privilege_prompt_match(matched.clone(), window, cx);
        true
    }

    pub(super) fn fill_privilege_prompt_match(
        &mut self,
        matched: MatchedPrivilegeCredential,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let sent = self.active_pane(cx).is_some_and(|pane| {
            pane.update(cx, |pane, cx| {
                pane.send_privilege_secret_input_bytes(secret_line.as_bytes(), cx)
            })
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

    pub(in crate::workspace) fn render_detached_terminal_surface(
        &self,
        tab_id: TabId,
        root_pane: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Detached windows share terminal pane entities with the workspace, but
        // the command bar still uses the main active-tab pipeline. Keep the
        // first detachable surface pane-only so commands cannot target the
        // wrong tab while the UI ownership model is being made window-aware.
        div()
            .size_full()
            .relative()
            .child(self.render_pane_tree_for_tab(Some(tab_id), root_pane, cx))
            .into_any_element()
    }
}
