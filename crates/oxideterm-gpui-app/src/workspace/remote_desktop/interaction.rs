// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn sync_remote_desktop_modifiers(
        &mut self,
        tab_id: TabId,
        modifiers: gpui::Modifiers,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.sync_modifiers(modifiers));
        }
    }

    pub(in crate::workspace) fn sync_remote_desktop_lock_keys(
        &mut self,
        tab_id: TabId,
        capslock: gpui::Capslock,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.sync_lock_keys(capslock));
        }
    }

    pub(in crate::workspace) fn forward_remote_desktop_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        self.sync_remote_desktop_modifiers(tab_id, event.modifiers, cx);
        self.sync_remote_desktop_lock_keys(tab_id, event.capslock, cx);
        true
    }

    pub(in crate::workspace) fn forward_remote_desktop_key_from_capture(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self
            .active_remote_desktop_tab_id(cx)
            .and_then(|tab_id| self.remote_desktop_session_entity(tab_id, cx))
        else {
            return false;
        };
        let overrides = &self.settings_store.settings().keybindings.overrides;
        session.update(cx, |session, cx| {
            session.forward_key_down(
                event,
                overrides,
                crate::keybindings::keystroke_matches_action,
                cx,
            )
        });
        true
    }

    pub(in crate::workspace) fn forward_remote_desktop_key_up(
        &mut self,
        event: &KeyUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self
            .active_remote_desktop_tab_id(cx)
            .and_then(|tab_id| self.remote_desktop_session_entity(tab_id, cx))
        else {
            return false;
        };
        let overrides = &self.settings_store.settings().keybindings.overrides;
        session.update(cx, |session, _cx| {
            session.forward_key_up(
                event,
                overrides,
                crate::keybindings::keystroke_matches_action,
            )
        });
        true
    }

    pub(in crate::workspace) fn copy_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        self.send_remote_desktop_control_shortcut(tab_id, "c", cx);
        true
    }

    pub(in crate::workspace) fn paste_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        let Some(item) = cx.read_from_clipboard() else {
            return true;
        };
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.paste_clipboard(item));
        }
        true
    }

    pub(in crate::workspace) fn send_remote_desktop_control_shortcut(
        &mut self,
        tab_id: TabId,
        code: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.send_control_shortcut(code));
        }
    }

    pub(in crate::workspace) fn active_remote_desktop_tab_id(&self, cx: &App) -> Option<TabId> {
        self.active_content_tab(cx)
            .filter(|tab| tab.kind == TabKind::RemoteDesktop)
            .map(|tab| tab.id)
    }

    pub(in crate::workspace) fn remote_desktop_preview_tab_title(
        &self,
        protocol: RemoteDesktopProtocol,
    ) -> String {
        match protocol {
            RemoteDesktopProtocol::Rdp => self.i18n.t("remote_desktop.rdp_preview_title"),
            RemoteDesktopProtocol::Vnc => self.i18n.t("remote_desktop.vnc_preview_title"),
        }
    }
}
