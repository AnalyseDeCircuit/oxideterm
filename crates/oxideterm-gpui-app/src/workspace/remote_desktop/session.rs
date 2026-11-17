// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    fn prompt_remote_desktop_credentials(
        &mut self,
        session: &Entity<RemoteDesktopSessionEntity>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = session.read(cx).credential_prompt(generation) else {
            return;
        };
        let window_handle = prompt.window;
        let needs_username = prompt.needs_username;
        let mut prompts = Vec::new();
        if needs_username {
            prompts.push(oxideterm_ssh::KeyboardInteractivePrompt {
                prompt: self.i18n.t("ssh.form.username"),
                echo: true,
            });
        }
        prompts.push(oxideterm_ssh::KeyboardInteractivePrompt {
            prompt: self.i18n.t("ssh.form.password"),
            echo: false,
        });
        let request = oxideterm_ssh::KeyboardInteractivePromptRequest {
            flow_id: uuid::Uuid::new_v4().to_string(),
            name: prompt.name,
            instructions: String::new(),
            prompts,
            chained: false,
        };
        let weak_session = session.downgrade();
        let task = cx.spawn(async move |workspace, cx| {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let _ = cx.update_window(window_handle, |_, window, cx| {
                let _ = workspace.update(cx, |workspace, cx| {
                    workspace.open_keyboard_interactive_challenge(request, sender, window, cx)
                });
            });
            let Ok(Ok(mut responses)) = receiver.await else {
                return;
            };
            if responses.len() != if needs_username { 2 } else { 1 } {
                return;
            }
            let _ = cx.update_window(window_handle, |_, window, cx| {
                let _ = weak_session.update(cx, |session, cx| {
                    let username = needs_username.then(|| std::mem::take(&mut responses[0]));
                    let password = RemoteDesktopSecret::from(Zeroizing::new(std::mem::take(
                        &mut responses[usize::from(needs_username)],
                    )));
                    session.submit_credentials(generation, username, password, window, cx);
                });
            });
        });
        session.update(cx, |session, _| session.retain_credential_prompt(task));
    }

    pub(in crate::workspace) fn handle_remote_desktop_session_event(
        &mut self,
        session_entity: &Entity<RemoteDesktopSessionEntity>,
        event: &RemoteDesktopSessionEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            RemoteDesktopSessionEvent::PluginRequired { protocol } => {
                self.show_missing_remote_desktop_plugin(*protocol, cx);
                return;
            }
            RemoteDesktopSessionEvent::CredentialsRequired { generation } => {
                self.prompt_remote_desktop_credentials(session_entity, *generation, cx);
                return;
            }
            RemoteDesktopSessionEvent::ClipboardTransferFailed => {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.clipboard_file_failed"),
                    None,
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
            RemoteDesktopSessionEvent::VncFileTransferCompleted => {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.file_download_completed"),
                    None,
                    TerminalNoticeVariant::Success,
                    cx,
                );
                return;
            }
            RemoteDesktopSessionEvent::VncFileTransferFailed(kind) => {
                let key = match kind {
                    RemoteDesktopFileTransferFailureKind::Remote => {
                        "remote_desktop.file_download_failed_remote"
                    }
                    RemoteDesktopFileTransferFailureKind::Local => {
                        "remote_desktop.file_download_failed_local"
                    }
                    RemoteDesktopFileTransferFailureKind::Protocol => {
                        "remote_desktop.file_download_failed_protocol"
                    }
                    RemoteDesktopFileTransferFailureKind::Canceled => {
                        "remote_desktop.file_download_canceled"
                    }
                };
                self.push_command_palette_toast(
                    self.i18n.t(key),
                    None,
                    if *kind == RemoteDesktopFileTransferFailureKind::Canceled {
                        TerminalNoticeVariant::Default
                    } else {
                        TerminalNoticeVariant::Error
                    },
                    cx,
                );
                return;
            }
        }
    }

    pub(in crate::workspace) fn remote_desktop_session_entity(
        &self,
        tab_id: TabId,
        cx: &App,
    ) -> Option<Entity<RemoteDesktopSessionEntity>> {
        self.remote_desktop.read(cx).session(tab_id)
    }

    pub(in crate::workspace) fn bind_remote_desktop_window(
        &mut self,
        tab_id: TabId,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            // Window affinity is a lifecycle property: delivery and resource
            // cleanup must follow the tab across detach and dock transitions.
            session.update(cx, |session, _cx| {
                session.bind_window(window_handle);
            });
        }
    }

    pub(in crate::workspace) fn remote_desktop_tab_visible(&self, tab_id: TabId, cx: &App) -> bool {
        let tab_host = self.tab_host.read(cx);
        let main_tab_visible = tab_host.surface_is_visible(tab_id) && !tab_host.is_detached(tab_id);
        let detached_tab_visible = tab_host.is_detached(tab_id);
        remote_desktop_tab_visible(main_tab_visible, detached_tab_visible)
    }

    pub(in crate::workspace) fn resume_remote_desktop_frame_delivery(
        &self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return;
        };
        // Hidden tabs retain one coalesced frame. Visibility resumes that frame
        // without restarting or disconnecting either protocol worker.
        session_entity.update(cx, |session, cx| session.set_frame_visibility(true, cx));
    }

    pub(in crate::workspace) fn sync_remote_desktop_frame_visibility(
        &self,
        tab_id: TabId,
        cx: &mut App,
    ) {
        let visible = self.remote_desktop_tab_visible(tab_id, cx);
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, cx| {
                session.set_frame_visibility(visible, cx);
            });
        }
    }

    pub(in crate::workspace) fn close_remote_desktop_tab(
        &mut self,
        tab_id: TabId,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.release_public_mcp_desktop_for_closed_tab(tab_id);
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| session.shutdown(window, cx));
        }
        self.remote_desktop
            .update(cx, |remote_desktop, _cx| remote_desktop.remove(tab_id));
    }

    pub(in crate::workspace) fn release_remote_desktop_inputs_for_tab(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        let mut pages = vec![tab_id];
        if let Some(root) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref())
        {
            root.collect_page_ids(&mut pages);
        }
        for page in pages {
            if let Some(session) = self.remote_desktop_session_entity(page, cx) {
                session.update(cx, |session, _cx| session.release_inputs());
            }
        }
    }

    pub(in crate::workspace) fn release_active_remote_desktop_inputs(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self.active_remote_desktop_tab_id(cx) {
            self.release_remote_desktop_inputs_for_tab(tab_id, cx);
        }
    }

    pub(in crate::workspace) fn focus_remote_desktop_keyboard(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The remote surface stops root mouse propagation, so it must run the
        // same blur path that an outside workspace click would normally run.
        self.blur_text_inputs(cx);

        let mut changed = self.clear_ime_selection();
        changed |= self.ime_marked_text.take().is_some();
        changed |= self.pending_platform_text_commit.take().is_some();

        let ai_focus_changed = self.ai_entity.read(cx).chat_ui().input_focused
            || self.ai_entity.read(cx).chat_ui().footer_focus.is_some()
            || self.ai_entity.read(cx).model_selector_open()
            || self.ai_entity.read(cx).model_selector_search_focused();
        self.clear_ai_sidebar_keyboard_focus(cx);
        changed |= ai_focus_changed;

        self.sync_remote_desktop_lock_keys(tab_id, window.capslock(), cx);
        window.focus(&self.focus_handle, cx);
        if changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn disconnect_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let surface = standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab_id);
        if let Some(connection_id) = self
            .standalone_connections
            .connection_id_for_surface(surface)
        {
            self.disconnect_standalone_connection(&connection_id, window, cx);
        }
    }

    pub(in crate::workspace) fn reconnect_remote_desktop(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let surface = standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab_id);
        if let Some(connection_id) = self
            .standalone_connections
            .connection_id_for_surface(surface)
        {
            self.reconnect_standalone_connection(&connection_id, window, cx);
        }
    }
}
