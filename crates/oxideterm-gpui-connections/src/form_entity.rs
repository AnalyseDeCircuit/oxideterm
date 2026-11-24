// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::form_model::NodeId;

use super::{
    NewConnectionForm, NewConnectionSelect, SavedConnectionPromptAction,
    form_state::{NewConnectionFormMode, new_connection_form_mode},
};
use oxideterm_gpui_ui::select::{BrowserFocusOrigin, close_browser_trigger_select};

/// Owns the secret-bearing connection draft and its modal interaction metadata.
pub struct ConnectionFormState {
    pub form: Option<NewConnectionForm>,
    /// Stable focus target for the BLE scan-result list so Enter can inspect
    /// the selected device after a click moves focus onto the list.
    pub ble_device_list_focus: gpui::FocusHandle,
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub jump_server_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub jump_server_exit_commits: bool,
    pub drill_down_parent_node_id: Option<NodeId>,
    pub editing_saved_connection_id: Option<String>,
    pub editing_saved_connection_connect_after_save_node_id: Option<NodeId>,
    pub duplicating_saved_connection_id: Option<String>,
    pub saved_connection_prompt_action: Option<SavedConnectionPromptAction>,
    pub tunnel_start_groups: std::collections::VecDeque<(String, Vec<String>)>,
    pub open_select: Option<NewConnectionSelect>,
    pub select_focus_origin: Option<BrowserFocusOrigin>,
    /// Laid-out width of the BLE scan table captured by a bounds probe. The
    /// overflow-tooltip predicates for flexible cells need the previous
    /// frame's real geometry, which no render-time arithmetic reproduces.
    pub ble_scan_table_width: Option<f32>,
}

impl ConnectionFormState {
    pub fn new(ble_device_list_focus: gpui::FocusHandle) -> Self {
        Self {
            form: None,
            ble_device_list_focus,
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            jump_server_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            jump_server_exit_commits: false,
            drill_down_parent_node_id: None,
            editing_saved_connection_id: None,
            editing_saved_connection_connect_after_save_node_id: None,
            duplicating_saved_connection_id: None,
            saved_connection_prompt_action: None,
            tunnel_start_groups: Default::default(),
            open_select: None,
            select_focus_origin: None,
            ble_scan_table_width: None,
        }
    }

    pub fn mode(&self) -> NewConnectionFormMode {
        new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.duplicating_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        )
    }

    pub fn saved_connection_source_id(&self) -> Option<&str> {
        self.editing_saved_connection_id
            .as_deref()
            .or(self.duplicating_saved_connection_id.as_deref())
    }

    pub fn replace_with_new_form(&mut self, form: NewConnectionForm) {
        // Replacing the draft drops and scrubs the previous secret-bearing form.
        self.form = Some(form);
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.tunnel_start_groups.clear();
        self.close_select();
        self.presence.reopen();
    }

    pub fn clear(&mut self) {
        // Dropping the form is the final UI boundary for all draft credentials.
        self.form = None;
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.tunnel_start_groups.clear();
        self.close_select();
        self.jump_server_exit_commits = false;
        self.jump_server_presence.reopen();
        self.ble_scan_table_width = None;
    }

    pub fn close_select(&mut self) {
        close_browser_trigger_select(&mut self.open_select, &mut self.select_focus_origin);
    }
}

use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
/// Shares layout-only select anchors without routing every prepaint through WorkspaceApp.
#[derive(Clone, Default)]
pub struct ConnectionSelectAnchorStore {
    anchors: Rc<RefCell<HashMap<SelectAnchorId, OverlayAnchor>>>,
}

impl ConnectionSelectAnchorStore {
    pub fn get(&self, id: SelectAnchorId) -> Option<OverlayAnchor> {
        self.anchors.borrow().get(&id).copied()
    }

    pub fn update(&self, anchor: OverlayAnchor) -> bool {
        if self.get(anchor.id) == Some(anchor) {
            return false;
        }
        self.anchors.borrow_mut().insert(anchor.id, anchor);
        true
    }

    pub fn clear(&self) {
        self.anchors.borrow_mut().clear();
    }
}

impl ConnectionFormState {
    pub(crate) fn finish_jump_edit(&mut self, commit: bool) {
        if let Some(form) = self.form.as_mut() {
            if let Some(jump_server) = form.jump_server_form.take() {
                let edit_index = form.jump_server_edit_index.take();
                let proxy_hops = match form.jump_server_target {
                    super::ConnectionRouteTarget::Primary => {
                        form.proxy_chain_expanded = true;
                        &mut form.proxy_hops
                    }
                    super::ConnectionRouteTarget::StandaloneSftpSecondary => {
                        form.standalone_sftp_secondary.proxy_chain_expanded = true;
                        &mut form.standalone_sftp_secondary.proxy_hops
                    }
                };
                if let Some(index) = edit_index {
                    // The existing hop is moved into the modal, so both save and cancel
                    // restore it at its original position without duplicating secrets.
                    proxy_hops.insert(index.min(proxy_hops.len()), jump_server);
                } else if commit {
                    proxy_hops.push(jump_server);
                }
                if commit {
                    if form.auth_tab == super::SshAuthTab::TwoFactor {
                        form.auth_tab = super::SshAuthTab::Password;
                        form.focused_field = super::NewConnectionField::Password;
                    }
                    form.field_focused = false;
                    form.selected_field = None;
                    form.error = None;
                }
            }
        }
    }
}
