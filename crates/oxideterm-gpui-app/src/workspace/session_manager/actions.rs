use super::*;
use crate::workspace::new_connection::{
    NewConnectionTransport, form_from_mosh_profile, form_from_remote_desktop_profile,
    form_from_serial_profile, form_from_telnet_profile, terminal_serial_flow_from_profile,
    terminal_serial_parity_from_profile,
};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopSecret,
};

impl WorkspaceApp {
    pub(super) fn connection_count_for_group(&self, group: &str) -> usize {
        let connection_count = self
            .connection_store
            .connections()
            .iter()
            .filter(|conn| {
                conn.group.as_deref().is_some_and(|candidate| {
                    candidate == group || candidate.starts_with(&format!("{group}/"))
                })
            })
            .count();
        let serial_count = self
            .connection_store
            .serial_profiles()
            .iter()
            .filter(|profile| {
                profile.group.as_deref().is_some_and(|candidate| {
                    candidate == group || candidate.starts_with(&format!("{group}/"))
                })
            })
            .count();
        let telnet_count = self
            .connection_store
            .telnet_profiles()
            .iter()
            .filter(|profile| {
                profile.group.as_deref().is_some_and(|candidate| {
                    candidate == group || candidate.starts_with(&format!("{group}/"))
                })
            })
            .count();
        let mosh_count = self
            .connection_store
            .mosh_profiles()
            .iter()
            .filter(|profile| {
                profile.group.as_deref().is_some_and(|candidate| {
                    candidate == group || candidate.starts_with(&format!("{group}/"))
                })
            })
            .count();
        let standalone_sftp_count = self
            .connection_store
            .standalone_sftp_profiles()
            .iter()
            .filter(|profile| {
                profile.group.as_deref().is_some_and(|candidate| {
                    candidate == group || candidate.starts_with(&format!("{group}/"))
                })
            })
            .count();
        let remote_desktop_count = self
            .connection_store
            .remote_desktop_profiles()
            .iter()
            .filter(|profile| {
                profile.group.as_deref().is_some_and(|candidate| {
                    candidate == group || candidate.starts_with(&format!("{group}/"))
                })
            })
            .count();
        connection_count
            + serial_count
            + telnet_count
            + mosh_count
            + standalone_sftp_count
            + remote_desktop_count
    }

    pub(super) fn session_group_tree(&self) -> (Vec<String>, HashMap<String, Vec<String>>) {
        let mut paths = HashSet::new();
        for group in self.connection_store.groups() {
            add_group_path_segments(group, &mut paths);
        }
        for conn in self.connection_store.connections() {
            if let Some(group) = conn.group.as_deref() {
                add_group_path_segments(group, &mut paths);
            }
        }
        for profile in self.connection_store.serial_profiles() {
            if let Some(group) = profile.group.as_deref() {
                add_group_path_segments(group, &mut paths);
            }
        }
        for profile in self.connection_store.telnet_profiles() {
            if let Some(group) = profile.group.as_deref() {
                add_group_path_segments(group, &mut paths);
            }
        }
        for profile in self.connection_store.mosh_profiles() {
            if let Some(group) = profile.group.as_deref() {
                add_group_path_segments(group, &mut paths);
            }
        }
        for profile in self.connection_store.standalone_sftp_profiles() {
            if let Some(group) = profile.group.as_deref() {
                add_group_path_segments(group, &mut paths);
            }
        }
        for profile in self.connection_store.remote_desktop_profiles() {
            if let Some(group) = profile.group.as_deref() {
                add_group_path_segments(group, &mut paths);
            }
        }

        let mut sorted = paths.into_iter().collect::<Vec<_>>();
        sorted.sort();
        let mut roots = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        for path in sorted {
            if let Some((parent, _name)) = path.rsplit_once('/') {
                children.entry(parent.to_string()).or_default().push(path);
            } else {
                roots.push(path);
            }
        }
        (roots, children)
    }

    pub(super) fn toggle_session_group_expanded(&mut self, group: &str, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            if session_manager.expanded_groups.contains(group) {
                session_manager.expanded_groups.remove(group);
            } else {
                session_manager.expanded_groups.insert(group.to_string());
            }
            cx.notify();
        });
    }

    pub(super) fn connection_info_by_id(&self, id: &str) -> Option<ConnectionInfo> {
        self.connection_store
            .connection_infos()
            .into_iter()
            .find(|conn| conn.id == id)
    }

    pub(in crate::workspace) fn close_session_row_menus(&mut self, cx: &mut Context<Self>) -> bool {
        self.session_manager.update(cx, |session_manager, cx| {
            let changed = close_session_menu_state(session_manager);
            if changed {
                cx.notify();
            }
            changed
        })
    }

    pub(super) fn open_session_manager_row_action_menu(
        &mut self,
        target: SessionManagerRowActionTarget,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.open_session_manager_menu_at(
            target,
            SessionManagerRowActionMenuOrigin::ActionButton,
            x,
            y,
            cx,
        );
    }

    pub(super) fn open_session_manager_context_menu(
        &mut self,
        target: SessionManagerRowActionTarget,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.open_session_manager_menu_at(
            target,
            SessionManagerRowActionMenuOrigin::Pointer,
            x,
            y,
            cx,
        );
    }

    fn open_session_manager_menu_at(
        &mut self,
        target: SessionManagerRowActionTarget,
        origin: SessionManagerRowActionMenuOrigin,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        // One shared floating-menu owner prevents row actions from overlapping
        // the sort, view-mode, or batch-move popovers.
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.row_action_menu = Some(SessionManagerRowActionMenu {
                target,
                origin,
                x,
                y,
            });
            cx.notify();
        });
    }

    pub(super) fn toggle_session_view_mode_menu(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            let was_open = session_manager.view_mode_menu_open;
            close_session_menu_state(session_manager);
            if !was_open {
                // The view-mode selector is root-mounted and positioned from its
                // cached trigger bounds, so opening only needs to claim menu owner.
                session_manager.view_mode_menu_open = true;
            }
            cx.notify();
        });
    }

    pub(super) fn toggle_session_sort_menu(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            let was_open = session_manager.sort_menu_open;
            close_session_menu_state(session_manager);
            if !was_open {
                // Sort uses the same root-mounted anchored menu as view mode; keep
                // positioning separate from pointer coordinates to avoid drift.
                session_manager.sort_menu_open = true;
            }
            cx.notify();
        });
    }

    pub(super) fn set_session_sort_field(
        &mut self,
        field: SessionSortField,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.update(cx, |session_manager, cx| {
            if session_manager.sort_field == field {
                session_manager.sort_direction = session_manager.sort_direction.toggled();
            } else {
                session_manager.sort_field = field;
                session_manager.sort_direction = field.default_direction();
            }
            cx.notify();
        });
    }

    pub(super) fn toggle_session_selection(
        &mut self,
        target: SessionManagerSelectionTarget,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.update(cx, |session_manager, cx| {
            if session_manager.selected_items.contains(&target) {
                session_manager.selected_items.remove(&target);
            } else {
                session_manager.selected_items.insert(target);
            }
            cx.notify();
        });
    }

    pub(in crate::workspace) fn clear_session_selection_for_invisible_rows(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let visible_items = self
            .session_manager_display_items(cx)
            .into_iter()
            .filter_map(|item| item.selection_target())
            .collect::<HashSet<_>>();
        self.session_manager.update(cx, |session_manager, cx| {
            let previous_count = session_manager.selected_items.len();
            session_manager
                .selected_items
                .retain(|target| visible_items.contains(target));
            if session_manager.selected_items.len() != previous_count {
                cx.notify();
            }
        });
    }

    pub(super) fn submit_session_group_editor(&mut self, cx: &mut Context<Self>) {
        let (leaf_name, editor) = {
            let session_manager = self.session_manager.read(cx);
            (
                session_manager.group_name_draft.trim().to_string(),
                session_manager.group_editor.clone(),
            )
        };
        let Some(editor) = editor else {
            return;
        };
        let parent_path = match &editor {
            SessionManagerGroupEditor::Create { parent_path }
            | SessionManagerGroupEditor::Rename { parent_path, .. } => parent_path.as_deref(),
        };
        let Some(name) = session_group_path_from_leaf(parent_path, &leaf_name) else {
            let status = self.i18n.t("sessionManager.folder_tree.group_name_invalid");
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.group_editor_error = Some(status);
                cx.notify();
            });
            return;
        };
        if let SessionManagerGroupEditor::Rename { old_path, .. } = editor {
            if old_path == name {
                self.cancel_session_group_editor(cx);
                return;
            }
            self.rename_session_group(&old_path, name, cx);
            return;
        }

        match self.connection_store.create_group(name.clone()) {
            Ok(()) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.selected_group = Some(name);
                    expand_group_path(
                        session_manager
                            .selected_group
                            .as_deref()
                            .unwrap_or_default(),
                        &mut session_manager.expanded_groups,
                    );
                    session_manager.group_editor = None;
                    session_manager.group_name_draft.clear();
                    session_manager.group_editor_error = None;
                    session_manager.focused_input = None;
                    session_manager.focused_basic_dialog_footer_action = None;
                    cx.notify();
                });
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.create_group_failed")
                );
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.group_editor_error = Some(status);
                    cx.notify();
                });
            }
        }
    }

    pub(super) fn open_session_group_creation(&mut self, cx: &mut Context<Self>) {
        self.open_session_group_creation_at(None, cx);
    }

    pub(super) fn open_session_subgroup_creation(
        &mut self,
        parent_path: &str,
        cx: &mut Context<Self>,
    ) {
        self.open_session_group_creation_at(Some(parent_path.to_string()), cx);
    }

    fn open_session_group_creation_at(
        &mut self,
        parent_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.show_group_manager = true;
            // The editor keeps the parent immutable so users only enter one segment.
            session_manager.group_editor = Some(SessionManagerGroupEditor::Create { parent_path });
            session_manager.group_name_draft.clear();
            session_manager.group_editor_error = None;
            session_manager.group_manager_error = None;
            session_manager.focused_input = Some(SessionManagerInput::GroupName);
            session_manager.focused_basic_dialog_footer_action = None;
            cx.notify();
        });
        self.ime_marked_text = None;
    }

    pub(super) fn open_session_group_rename(&mut self, group: &str, cx: &mut Context<Self>) {
        let (parent_path, leaf_name) = split_session_group_path(group);
        let old_path = group.to_string();
        let parent_path = parent_path.map(ToOwned::to_owned);
        let leaf_name = leaf_name.to_string();
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.show_group_manager = true;
            session_manager.group_name_draft = leaf_name;
            session_manager.group_editor = Some(SessionManagerGroupEditor::Rename {
                old_path,
                parent_path,
            });
            session_manager.group_editor_error = None;
            session_manager.group_manager_error = None;
            session_manager.focused_input = Some(SessionManagerInput::GroupName);
            session_manager.focused_basic_dialog_footer_action = None;
            cx.notify();
        });
        self.ime_marked_text = None;
    }

    fn rename_session_group(&mut self, old_name: &str, new_name: String, cx: &mut Context<Self>) {
        match self
            .connection_store
            .rename_group(old_name, new_name.clone())
        {
            Ok(_) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.selected_group = session_manager
                        .selected_group
                        .as_deref()
                        .and_then(|group| renamed_session_group_path(group, old_name, &new_name))
                        .or_else(|| session_manager.selected_group.clone());
                    session_manager.expanded_groups = session_manager
                        .expanded_groups
                        .iter()
                        .map(|group| {
                            renamed_session_group_path(group, old_name, &new_name)
                                .unwrap_or_else(|| group.clone())
                        })
                        .collect();
                    expand_group_path(&new_name, &mut session_manager.expanded_groups);
                    session_manager.group_editor = None;
                    session_manager.group_name_draft.clear();
                    session_manager.group_editor_error = None;
                    session_manager.focused_input = None;
                    session_manager.focused_basic_dialog_footer_action = None;
                    cx.notify();
                });
                // Group metadata is persisted independently from live node ownership.
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.rename_group_failed")
                );
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.group_editor_error = Some(status);
                    cx.notify();
                });
            }
        }
    }

    pub(super) fn request_delete_session_group(&mut self, group: &str, cx: &mut Context<Self>) {
        let name = group.to_string();
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.reopen_group_manager_after_delete = session_manager.show_group_manager;
            session_manager.show_group_manager = false;
            session_manager.group_editor = None;
            session_manager.group_editor_error = None;
            session_manager.focused_input = None;
            session_manager.focused_basic_dialog_footer_action = None;
            session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::Group { name });
            cx.notify();
        });
    }

    pub(super) fn open_session_group_manager(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.show_group_manager = true;
            session_manager.group_editor = None;
            session_manager.group_name_draft.clear();
            session_manager.group_editor_error = None;
            session_manager.group_manager_error = None;
            session_manager.focused_input = None;
            session_manager.focused_basic_dialog_footer_action = None;
            cx.notify();
        });
    }

    pub(in crate::workspace) fn close_session_group_manager(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            if session_manager.show_group_manager {
                session_manager.show_group_manager = false;
                session_manager.group_editor = None;
                session_manager.group_name_draft.clear();
                session_manager.group_editor_error = None;
                session_manager.group_manager_error = None;
                session_manager.focused_input = None;
                session_manager.focused_basic_dialog_footer_action = None;
                cx.notify();
            }
        });
        self.ime_marked_text = None;
    }

    fn delete_session_group(&mut self, group: &str, cx: &mut Context<Self>) {
        match self.connection_store.delete_group(group) {
            Ok(()) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    if session_manager
                        .selected_group
                        .as_deref()
                        .is_some_and(|selected| session_group_path_is_within(selected, group))
                    {
                        session_manager.selected_group = None;
                    }
                    session_manager
                        .expanded_groups
                        .retain(|expanded| !session_group_path_is_within(expanded, group));
                    session_manager.show_group_manager =
                        session_manager.reopen_group_manager_after_delete;
                    session_manager.reopen_group_manager_after_delete = false;
                    session_manager.group_manager_error = None;
                    cx.notify();
                });
                // Deleting a group only reassigns saved metadata; active nodes keep running.
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.delete_group_failed")
                );
                self.session_manager.update(cx, |session_manager, cx| {
                    let reopen_manager = session_manager.reopen_group_manager_after_delete;
                    session_manager.show_group_manager = reopen_manager;
                    session_manager.reopen_group_manager_after_delete = false;
                    if reopen_manager {
                        session_manager.group_manager_error = Some(status);
                        cx.notify();
                    } else {
                        session_manager.set_status(Some(status), cx);
                    }
                });
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.connection_store.delete(id) {
            let status = error.to_string();
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(status), cx)
            });
        } else {
            // Tauri deletes owner-bound saved forwards with the saved connection
            // so sync/import cannot later resurrect forwards for a missing owner.
            if let Err(error) = self.forwarding_service.registry().delete_owned_forwards(id) {
                let status = error.to_string();
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx);
                });
                return;
            }
            self.release_ide_runtime_for_saved_connection(id, cx);
            let status = self.i18n.t("sessionManager.toast.connection_deleted");
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager
                    .selected_items
                    .remove(&SessionManagerSelectionTarget::Connection(id.to_string()));
                session_manager.status = Some(status);
                cx.notify();
            });
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn request_delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(conn) = self.connection_info_by_id(id) else {
            return;
        };
        // Tauri snapshots the row payload before opening useConfirm; native
        // keeps the same target stable while the dialog is open.
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::Single {
                id: conn.id,
                name: conn.name,
            });
            cx.notify();
        });
    }

    pub(super) fn request_delete_serial_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(profile) = self
            .connection_store
            .serial_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::SerialProfile {
                id: profile.id,
                name: profile.name,
            });
            cx.notify();
        });
    }

    pub(super) fn request_delete_telnet_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(profile) = self
            .connection_store
            .telnet_profiles()
            .iter()
            .find(|profile| profile.id == id)
        else {
            return;
        };
        let profile_name = profile.name.clone();
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::TelnetProfile {
                id: id.to_string(),
                name: profile_name,
            });
            cx.notify();
        });
    }

    pub(super) fn request_delete_mosh_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(profile) = self.connection_store.get_mosh_profile(id) else {
            return;
        };
        let confirm = SessionManagerDeleteConfirm::MoshProfile {
            id: profile.id.clone(),
            name: profile.name.clone(),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.delete_confirm = Some(confirm);
            cx.notify();
        });
    }

    pub(super) fn request_delete_standalone_sftp_profile(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.connection_store.get_standalone_sftp_profile(id) else {
            return;
        };
        let confirm = SessionManagerDeleteConfirm::StandaloneSftpProfile {
            id: profile.id.clone(),
            name: profile.name.clone(),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.delete_confirm = Some(confirm);
            cx.notify();
        });
    }

    pub(super) fn request_delete_remote_desktop_profile(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.connection_store.get_remote_desktop_profile(id) else {
            return;
        };
        let confirm = SessionManagerDeleteConfirm::RemoteDesktopProfile {
            id: profile.id.clone(),
            name: profile.name.clone(),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.delete_confirm = Some(confirm);
            cx.notify();
        });
    }

    pub(super) fn request_delete_selected_connections(&mut self, cx: &mut Context<Self>) {
        let targets = self
            .session_manager
            .read(cx)
            .selected_items
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return;
        }
        // Batch delete follows Tauri's confirm closure and freezes the selected
        // ids at the time the destructive action is requested.
        self.session_manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::Batch { targets });
            cx.notify();
        });
    }

    pub(super) fn cancel_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            let canceled_group_delete = matches!(
                session_manager.delete_confirm,
                Some(SessionManagerDeleteConfirm::Group { .. })
            );
            session_manager.delete_confirm = None;
            if canceled_group_delete {
                session_manager.show_group_manager =
                    session_manager.reopen_group_manager_after_delete;
                session_manager.reopen_group_manager_after_delete = false;
            }
            cx.notify();
        });
    }

    pub(super) fn confirm_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        let confirm = self.session_manager.update(cx, |session_manager, cx| {
            let confirm = session_manager.delete_confirm.take();
            if confirm.is_some() {
                cx.notify();
            }
            confirm
        });
        let Some(confirm) = confirm else {
            return;
        };
        match confirm {
            SessionManagerDeleteConfirm::Single { id, .. } => self.delete_connection(&id, cx),
            SessionManagerDeleteConfirm::SerialProfile { id, .. } => {
                self.delete_serial_profile(&id, cx)
            }
            SessionManagerDeleteConfirm::TelnetProfile { id, .. } => {
                self.delete_telnet_profile(&id, cx)
            }
            SessionManagerDeleteConfirm::MoshProfile { id, .. } => {
                self.delete_mosh_profile(&id, cx)
            }
            SessionManagerDeleteConfirm::StandaloneSftpProfile { id, .. } => {
                self.delete_standalone_sftp_profile(&id, cx)
            }
            SessionManagerDeleteConfirm::RemoteDesktopProfile { id, .. } => {
                self.delete_remote_desktop_profile(&id, cx)
            }
            SessionManagerDeleteConfirm::Batch { targets } => {
                self.delete_selected_session_items(targets, cx)
            }
            SessionManagerDeleteConfirm::Group { name } => self.delete_session_group(&name, cx),
        }
    }

    pub(super) fn delete_serial_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let (status, changed) = match self.connection_store.delete_serial_profile(id) {
            Ok(true) => (self.i18n.t("sessionManager.serial_profiles.delete"), true),
            Ok(false) => (
                self.i18n.t("sessionManager.serial_profiles.delete_failed"),
                false,
            ),
            Err(error) => (
                format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.serial_profiles.delete_failed")
                ),
                false,
            ),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            if changed {
                session_manager
                    .selected_items
                    .remove(&SessionManagerSelectionTarget::Serial(id.to_string()));
            }
            session_manager.set_status(Some(status), cx)
        });
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn delete_telnet_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let (status, deleted) = match self.connection_store.delete_telnet_profile(id) {
            Ok(true) => (self.i18n.t("sessionManager.telnet_profiles.delete"), true),
            Ok(false) => (
                self.i18n.t("sessionManager.telnet_profiles.delete_failed"),
                false,
            ),
            Err(error) => (
                format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.telnet_profiles.delete_failed")
                ),
                false,
            ),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            if deleted {
                session_manager
                    .selected_items
                    .remove(&SessionManagerSelectionTarget::Telnet(id.to_string()));
            }
            session_manager.set_status(Some(status), cx)
        });
        if deleted {
            // Telnet profiles participate in the same Cloud Sync snapshot as other saved sessions.
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn delete_mosh_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let (status, changed) = match self.connection_store.delete_mosh_profile(id) {
            Ok(true) => (self.i18n.t("sessionManager.mosh_profiles.delete"), true),
            Ok(false) => (
                self.i18n.t("sessionManager.mosh_profiles.delete_failed"),
                false,
            ),
            Err(error) => (
                format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.mosh_profiles.delete_failed")
                ),
                false,
            ),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            if changed {
                session_manager
                    .selected_items
                    .remove(&SessionManagerSelectionTarget::Mosh(id.to_string()));
            }
            session_manager.set_status(Some(status), cx)
        });
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn delete_standalone_sftp_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let (status, changed) = match self.connection_store.delete_standalone_sftp_profile(id) {
            Ok(true) => (
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.delete"),
                true,
            ),
            Ok(false) => (
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.delete_failed"),
                false,
            ),
            Err(error) => (
                format!(
                    "{}: {error}",
                    self.i18n
                        .t("sessionManager.standalone_sftp_profiles.delete_failed")
                ),
                false,
            ),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            if changed {
                session_manager.selected_items.remove(
                    &SessionManagerSelectionTarget::StandaloneSftp(id.to_string()),
                );
            }
            session_manager.set_status(Some(status), cx)
        });
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn delete_remote_desktop_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let (status, deleted) = match self.connection_store.delete_remote_desktop_profile(id) {
            Ok(true) => (
                self.i18n.t("sessionManager.remote_desktop_profiles.delete"),
                true,
            ),
            Ok(false) => (
                self.i18n
                    .t("sessionManager.remote_desktop_profiles.delete_failed"),
                false,
            ),
            Err(error) => (
                format!(
                    "{}: {error}",
                    self.i18n
                        .t("sessionManager.remote_desktop_profiles.delete_failed"),
                ),
                false,
            ),
        };
        self.session_manager.update(cx, |session_manager, cx| {
            if deleted {
                session_manager.selected_items.remove(
                    &SessionManagerSelectionTarget::RemoteDesktop(id.to_string()),
                );
            }
            session_manager.set_status(Some(status), cx);
        });
        if deleted {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(in crate::workspace) fn open_saved_serial_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .serial_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        let config = oxideterm_terminal::SerialSessionConfig {
            port_path: profile.port_path.clone(),
            baud_rate: profile.baud_rate,
            data_bits: profile.data_bits,
            stop_bits: profile.stop_bits,
            parity: terminal_serial_parity_from_profile(&profile.parity),
            flow_control: terminal_serial_flow_from_profile(&profile.flow_control),
        };
        match self.create_serial_terminal_tab(config, profile.terminal.clone(), window, cx) {
            Ok(session_id) => {
                self.register_terminal_saved_connection(
                    session_id,
                    oxideterm_terminal_triggers::SavedConnectionKind::Serial,
                    profile.id.clone(),
                    cx,
                );
                let _ = self.connection_store.mark_serial_profile_used(id);
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.serial_profiles.open_failed")
                );
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx)
                });
            }
        }
    }

    pub(super) fn open_saved_serial_profile_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self
            .connection_store
            .serial_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        self.open_new_connection_form(window, cx);
        let form = form_from_serial_profile(&saved, self.i18n.t("ssh.form.ungrouped"));
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        // Refresh discovery without replacing the persisted port selected by the editor.
        self.refresh_serial_ports(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_telnet_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .telnet_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        let config = oxideterm_terminal::TelnetSessionConfig {
            host: profile.host.clone(),
            port: profile.port,
        };
        match self.create_telnet_terminal_tab(config, profile.terminal, window, cx) {
            Ok(session_id) => {
                self.telnet_terminal_profile_ids
                    .insert(session_id, profile.id.clone());
                self.register_terminal_saved_connection(
                    session_id,
                    oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                    profile.id.clone(),
                    cx,
                );
                let _ = self.connection_store.mark_telnet_profile_used(id);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.telnet_profiles.open_failed")
                );
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx)
                });
            }
        }
    }

    pub(super) fn open_saved_telnet_profile_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self
            .connection_store
            .telnet_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        self.open_new_connection_form(window, cx);
        let form = form_from_telnet_profile(&saved, self.i18n.t("ssh.form.ungrouped"));
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_remote_desktop_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self
            .connection_store
            .get_remote_desktop_profile(id)
            .cloned()
        else {
            return;
        };
        let password = match self.connection_store.get_remote_desktop_credential(id) {
            Ok(secret) => secret
                .map(SecretString::into_zeroizing)
                .map(RemoteDesktopSecret::from),
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n
                        .t("sessionManager.remote_desktop_profiles.open_failed")
                );
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx)
                });
                return;
            }
        };
        if saved.protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp
            && password.is_none()
        {
            // Synced and imported assets intentionally omit device-local credentials.
            // Reopen the regular form so the user can authenticate on this device.
            self.open_new_connection_form(window, cx);
            let password_required = self
                .i18n
                .t("modals.new_connection.remote_desktop_password_required");
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.transport = NewConnectionTransport::Rdp;
                    form.name = saved.name;
                    form.host = saved.host;
                    form.port = saved.port.to_string();
                    form.username = saved.username.unwrap_or_default();
                    form.group = saved.group.unwrap_or_default();
                    form.remote_desktop_session_options = saved.session_options;
                    form.remote_desktop_ssh_gateway_connection_id = saved.ssh_gateway_connection_id;
                    form.error = Some(password_required);
                    form.focused_field = NewConnectionField::Password;
                }
            });
            return;
        }
        let profile = RemoteDesktopConnectionProfile {
            id: saved.id.clone(),
            label: saved.name,
            protocol: saved.protocol,
            endpoint: RemoteDesktopEndpoint::new(saved.host, saved.port),
            transport_endpoint: None,
            username: saved.username,
            domain: saved.domain,
            credential_ref: saved.credential_ref,
            read_only: saved.read_only,
            session_options: saved.session_options,
        };
        self.open_remote_desktop_connection_with_gateway(
            profile,
            password,
            saved.ssh_gateway_connection_id,
            window,
            cx,
        );
        let _ = self.connection_store.mark_remote_desktop_profile_used(id);
        self.queue_cloud_sync_dirty_refresh(cx);
    }

    pub(super) fn open_saved_remote_desktop_profile_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self
            .connection_store
            .get_remote_desktop_profile(id)
            .cloned()
        else {
            return;
        };
        self.open_new_connection_form(window, cx);
        let form = form_from_remote_desktop_profile(&saved, self.i18n.t("ssh.form.ungrouped"));
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_mosh_profile_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self.connection_store.get_mosh_profile(id).cloned() else {
            return;
        };
        self.open_new_connection_form(window, cx);
        let form = form_from_mosh_profile(&saved, self.i18n.t("ssh.form.ungrouped"));
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        cx.notify();
    }

    pub(super) fn open_saved_standalone_sftp_profile_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self
            .connection_store
            .get_standalone_sftp_profile(id)
            .cloned()
        else {
            return;
        };
        self.open_new_connection_form(window, cx);
        let form = form_from_standalone_sftp_profile(&saved);
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        cx.notify();
    }

    pub(super) fn delete_selected_session_items(
        &mut self,
        targets: Vec<SessionManagerSelectionTarget>,
        cx: &mut Context<Self>,
    ) {
        let mut deleted = 0;
        for target in targets {
            match target {
                SessionManagerSelectionTarget::Connection(id) => {
                    if self.connection_store.delete(&id).unwrap_or(false) {
                        // Keep batch delete aligned with the single-delete command path.
                        if let Err(error) = self
                            .forwarding_service
                            .registry()
                            .delete_owned_forwards(&id)
                        {
                            let status = error.to_string();
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(status), cx);
                            });
                            return;
                        }
                        self.release_ide_runtime_for_saved_connection(&id, cx);
                        deleted += 1;
                    }
                }
                SessionManagerSelectionTarget::Serial(id) => {
                    if self
                        .connection_store
                        .delete_serial_profile(&id)
                        .unwrap_or(false)
                    {
                        deleted += 1;
                    }
                }
                SessionManagerSelectionTarget::Telnet(id) => {
                    if self
                        .connection_store
                        .delete_telnet_profile(&id)
                        .unwrap_or(false)
                    {
                        deleted += 1;
                    }
                }
                SessionManagerSelectionTarget::Mosh(id) => {
                    if self
                        .connection_store
                        .delete_mosh_profile(&id)
                        .unwrap_or(false)
                    {
                        deleted += 1;
                    }
                }
                SessionManagerSelectionTarget::StandaloneSftp(id) => {
                    if self
                        .connection_store
                        .delete_standalone_sftp_profile(&id)
                        .unwrap_or(false)
                    {
                        deleted += 1;
                    }
                }
                SessionManagerSelectionTarget::RemoteDesktop(id) => {
                    if self
                        .connection_store
                        .delete_remote_desktop_profile(&id)
                        .unwrap_or(false)
                    {
                        deleted += 1;
                    }
                }
            }
        }
        let status = connections_deleted_label(&self.i18n, deleted);
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.selected_items.clear();
            session_manager.show_batch_move = false;
            session_manager.status = Some(status);
            cx.notify();
        });
        if deleted > 0 {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn duplicate_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
            return;
        };
        let mut form = form_from_saved_connection(&conn, None);
        restore_legacy_jump_host_in_form(&mut form, &conn, &self.connection_store);
        form.name = duplicate_connection_template_name(
            &conn.name,
            self.connection_store
                .connections()
                .iter()
                .map(|connection| connection.name.as_str()),
        );
        form.focused_field = NewConnectionField::Name;
        form.field_focused = true;

        self.prepare_modal_interaction_boundary(cx);
        self.update_connection_form_state(cx, |state| {
            state.replace_with_new_form(form);
            state.duplicating_saved_connection_id = Some(id.to_string());
        });
        self.close_session_row_menus(cx);
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn test_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.connection_store.get(id).cloned() else {
            let status = self.i18n.t("sessionManager.toast.test_failed");
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(status), cx)
            });
            return;
        };
        let Some(config) = oxideterm_session_adapter::ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &conn,
        ) else {
            self.open_saved_connection_prompt(
                id,
                SavedConnectionPromptAction::Test,
                Some(
                    self.i18n
                        .t("sessionManager.edit_properties.password_placeholder"),
                ),
                window,
                cx,
            );
            return;
        };
        self.start_ssh_test_flow(config, conn.name, cx);
    }

    pub(super) fn move_selected_connections(
        &mut self,
        group: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let targets = self
            .session_manager
            .read(cx)
            .selected_items
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut connection_ids = Vec::new();
        let mut serial_profile_ids = Vec::new();
        let mut telnet_profile_ids = Vec::new();
        let mut mosh_profile_ids = Vec::new();
        let mut standalone_sftp_profile_ids = Vec::new();
        let mut remote_desktop_ids = Vec::new();
        for target in targets {
            match target {
                SessionManagerSelectionTarget::Connection(id) => connection_ids.push(id),
                SessionManagerSelectionTarget::Serial(id) => serial_profile_ids.push(id),
                SessionManagerSelectionTarget::Telnet(id) => telnet_profile_ids.push(id),
                SessionManagerSelectionTarget::Mosh(id) => mosh_profile_ids.push(id),
                SessionManagerSelectionTarget::StandaloneSftp(id) => {
                    standalone_sftp_profile_ids.push(id)
                }
                SessionManagerSelectionTarget::RemoteDesktop(id) => remote_desktop_ids.push(id),
            }
        }
        match self.connection_store.move_session_assets_to_group(
            &connection_ids,
            &serial_profile_ids,
            &telnet_profile_ids,
            &mosh_profile_ids,
            &standalone_sftp_profile_ids,
            &remote_desktop_ids,
            group,
        ) {
            Ok(count) => {
                let status =
                    connections_moved_label(&self.i18n, count, group_label(&self.i18n, group));
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.status = Some(status);
                    session_manager.selected_items.clear();
                    session_manager.show_batch_move = false;
                    cx.notify();
                });
                if count > 0 {
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
            }
            Err(error) => {
                let status = error.to_string();
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx)
                });
            }
        }
    }
}

pub(super) fn close_session_menu_state(session_manager: &mut SessionManagerState) -> bool {
    // SessionManager floating menus share one ContextMenu dismissal owner for
    // outside click, Esc, and guarded item activation.
    let changed = session_manager.view_mode_menu_open
        || session_manager.sort_menu_open
        || session_manager.show_batch_move
        || session_manager.row_action_menu.is_some();
    session_manager.view_mode_menu_open = false;
    session_manager.sort_menu_open = false;
    session_manager.show_batch_move = false;
    session_manager.row_action_menu = None;
    changed
}
