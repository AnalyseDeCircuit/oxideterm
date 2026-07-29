// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_ssh::NodeId;

use super::{
    NewConnectionForm, NewConnectionSelect, SavedConnectionPromptAction,
    form_state::{NewConnectionFormMode, new_connection_form_mode},
};
use crate::workspace::browser_behavior::{self, BrowserFocusOrigin};

/// Owns the secret-bearing connection draft and its modal interaction metadata.
pub(in crate::workspace) struct ConnectionFormState {
    pub(in crate::workspace) form: Option<NewConnectionForm>,
    pub(in crate::workspace) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(in crate::workspace) jump_server_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(in crate::workspace) jump_server_exit_commits: bool,
    pub(in crate::workspace) drill_down_parent_node_id: Option<NodeId>,
    pub(in crate::workspace) editing_saved_connection_id: Option<String>,
    pub(in crate::workspace) editing_saved_connection_connect_after_save_node_id: Option<NodeId>,
    pub(in crate::workspace) duplicating_saved_connection_id: Option<String>,
    pub(in crate::workspace) saved_connection_prompt_action: Option<SavedConnectionPromptAction>,
    pub(in crate::workspace) open_select: Option<NewConnectionSelect>,
    pub(in crate::workspace) select_focus_origin: Option<BrowserFocusOrigin>,
}

impl ConnectionFormState {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            form: None,
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            jump_server_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            jump_server_exit_commits: false,
            drill_down_parent_node_id: None,
            editing_saved_connection_id: None,
            editing_saved_connection_connect_after_save_node_id: None,
            duplicating_saved_connection_id: None,
            saved_connection_prompt_action: None,
            open_select: None,
            select_focus_origin: None,
        }
    }

    pub(in crate::workspace) fn mode(&self) -> NewConnectionFormMode {
        new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.duplicating_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        )
    }

    pub(in crate::workspace) fn saved_connection_source_id(&self) -> Option<&str> {
        self.editing_saved_connection_id
            .as_deref()
            .or(self.duplicating_saved_connection_id.as_deref())
    }

    pub(in crate::workspace) fn replace_with_new_form(&mut self, form: NewConnectionForm) {
        // Replacing the draft drops and scrubs the previous secret-bearing form.
        self.form = Some(form);
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_select();
        self.presence.reopen();
    }

    pub(in crate::workspace) fn clear(&mut self) {
        // Dropping the form is the final UI boundary for all draft credentials.
        self.form = None;
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = None;
        self.saved_connection_prompt_action = None;
        self.close_select();
        self.jump_server_exit_commits = false;
        self.jump_server_presence.reopen();
    }

    pub(in crate::workspace) fn close_select(&mut self) {
        browser_behavior::close_browser_trigger_select(
            &mut self.open_select,
            &mut self.select_focus_origin,
        );
    }
}
