// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{Context, Window};
use oxideterm_connections::FtpProfile;
use oxideterm_gpui_connections::views::ConnectedFtp;
use std::sync::Arc;

use super::form_state::NewConnectionSubmitAction;
use crate::workspace::{WorkspaceApp, sftp::ftp::FtpRuntime};

impl WorkspaceApp {
    pub(in crate::workspace) fn submit_ftp_connection_form(
        &mut self, action: NewConnectionSubmitAction, cx: &mut Context<Self>,
    ) {
        if self.connections.submit_ftp(action, &mut self.connection_store, self.settings_store.settings(), &self.i18n, self.forwarding_runtime.handle(), cx) {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn adopt_ftp_connection(&mut self, result: ConnectedFtp, cx: &mut Context<Self>) {
        if result.saved {
            let _ = self.connection_store.mark_ftp_profile_used(&result.profile.id);
        }
        self.ftp_sessions.insert(result.profile.id.clone(), Arc::new(FtpRuntime::adopt(result.options, result.session)));
        self.open_ftp_tab(&result.profile, cx);
    }

    pub(in crate::workspace) fn open_ftp_tab(&mut self, profile: &FtpProfile, cx: &mut Context<Self>) {
        self.open_standalone_sftp_tab_surface(profile.id.clone(), profile.name.clone(), Some(profile.initial_path.clone()), cx);
    }

    pub(in crate::workspace) fn open_saved_ftp_profile(
        &mut self, id: &str, window: &mut Window, cx: &mut Context<Self>,
    ) {
        self.open_saved_ftp_profile_editor(id, window, cx);
        self.submit_ftp_connection_form(NewConnectionSubmitAction::Connect, cx);
    }

    pub(in crate::workspace) fn open_saved_ftp_profile_editor(
        &mut self, id: &str, window: &mut Window, cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.connection_store.get_ftp_profile(id).cloned() else { return; };
        self.open_new_connection_form(window, cx);
        self.connections.flow.update(cx, |flow, cx| flow.populate_ftp_form(profile, cx));
    }
}
