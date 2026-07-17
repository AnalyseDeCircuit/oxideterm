use super::*;

impl WorkspaceApp {
    pub(super) fn filtered_session_connections(&self) -> Vec<ConnectionInfo> {
        let query = self.session_manager.search_query.trim().to_lowercase();
        let mut rows = self.connection_store.connection_infos();
        rows.retain(|conn| self.connection_matches_filter(conn));
        if !query.is_empty() {
            rows.retain(|conn| {
                conn.name.to_lowercase().contains(&query)
                    || conn.host.to_lowercase().contains(&query)
                    || conn.username.to_lowercase().contains(&query)
                    || conn
                        .group
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || conn
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query))
            });
        }
        // The new grid/list/tree display model owns presentation sorting. This
        // helper is only used to retain valid checkbox selections after filters.
        rows.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        if self.session_manager.selected_group.as_deref() == Some(RECENT_FILTER) {
            rows.truncate(20);
        }
        rows
    }

    pub(super) fn connection_matches_filter(&self, conn: &ConnectionInfo) -> bool {
        match self.session_manager.selected_group.as_deref() {
            None => true,
            Some(UNGROUPED_FILTER) => conn.group.is_none(),
            Some(RECENT_FILTER) => conn.last_used_at.is_some(),
            Some(group) => conn.group.as_deref().is_some_and(|conn_group| {
                conn_group == group || conn_group.starts_with(&format!("{group}/"))
            }),
        }
    }

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
        connection_count + serial_count + telnet_count
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

    pub(super) fn toggle_session_group_expanded(&mut self, group: &str) {
        if self.session_manager.expanded_groups.contains(group) {
            self.session_manager.expanded_groups.remove(group);
        } else {
            self.session_manager
                .expanded_groups
                .insert(group.to_string());
        }
    }

    pub(super) fn connection_info_by_id(&self, id: &str) -> Option<ConnectionInfo> {
        self.connection_store
            .connection_infos()
            .into_iter()
            .find(|conn| conn.id == id)
    }

    pub(in crate::workspace) fn close_session_row_menus(&mut self) -> bool {
        close_session_menu_state(&mut self.session_manager)
    }

    pub(super) fn open_session_manager_row_action_menu(
        &mut self,
        target: SessionManagerRowActionTarget,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        // One shared floating-menu owner prevents row actions from overlapping
        // the sort, view-mode, or batch-move popovers.
        self.close_session_row_menus();
        self.session_manager.row_action_menu = Some(SessionManagerRowActionMenu { target, x, y });
        cx.notify();
    }

    pub(super) fn toggle_session_view_mode_menu(&mut self) {
        let was_open = self.session_manager.view_mode_menu_open;
        self.close_session_row_menus();
        if !was_open {
            // The view-mode selector is root-mounted and positioned from its
            // cached trigger bounds, so opening only needs to claim menu owner.
            self.session_manager.view_mode_menu_open = true;
        }
    }

    pub(super) fn toggle_session_sort_menu(&mut self) {
        let was_open = self.session_manager.sort_menu_open;
        self.close_session_row_menus();
        if !was_open {
            // Sort uses the same root-mounted anchored menu as view mode; keep
            // positioning separate from pointer coordinates to avoid drift.
            self.session_manager.sort_menu_open = true;
        }
    }

    pub(super) fn set_session_sort_field(&mut self, field: SessionSortField) {
        if self.session_manager.sort_field == field {
            self.session_manager.sort_direction = self.session_manager.sort_direction.toggled();
        } else {
            self.session_manager.sort_field = field;
            self.session_manager.sort_direction = field.default_direction();
        }
    }

    pub(super) fn toggle_connection_selection(&mut self, id: &str) {
        if self.session_manager.selected_ids.contains(id) {
            self.session_manager.selected_ids.remove(id);
        } else {
            self.session_manager.selected_ids.insert(id.to_string());
        }
    }

    pub(in crate::workspace) fn clear_session_selection_for_invisible_rows(&mut self) {
        let visible_ids = self
            .filtered_session_connections()
            .into_iter()
            .map(|conn| conn.id)
            .collect::<HashSet<_>>();
        self.session_manager
            .selected_ids
            .retain(|id| visible_ids.contains(id));
    }

    pub(super) fn create_session_group(&mut self, cx: &mut Context<Self>) {
        let name = self.session_manager.new_group_name.trim().to_string();
        match self.connection_store.create_group(name.clone()) {
            Ok(()) => {
                self.session_manager.selected_group = Some(name);
                expand_group_path(
                    self.session_manager
                        .selected_group
                        .as_deref()
                        .unwrap_or_default(),
                    &mut self.session_manager.expanded_groups,
                );
                self.session_manager.show_new_group = false;
                self.session_manager.focused_input = None;
                self.session_manager.focused_basic_dialog_footer_action = None;
                self.session_manager.status =
                    Some(self.i18n.t("sessionManager.toast.group_created"));
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.session_manager.status = Some(format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.create_group_failed")
                ));
            }
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub(super) fn delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.connection_store.delete(id) {
            self.session_manager.status = Some(error.to_string());
        } else {
            // Tauri deletes owner-bound saved forwards with the saved connection
            // so sync/import cannot later resurrect forwards for a missing owner.
            if let Err(error) = self.forwarding_registry.delete_owned_forwards(id) {
                self.session_manager.status = Some(error.to_string());
                cx.notify();
                return;
            }
            self.release_ide_runtime_for_saved_connection(id, cx);
            self.session_manager.selected_ids.remove(id);
            self.session_manager.status =
                Some(self.i18n.t("sessionManager.toast.connection_deleted"));
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
    }

    pub(super) fn request_delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(conn) = self.connection_info_by_id(id) else {
            return;
        };
        // Tauri snapshots the row payload before opening useConfirm; native
        // keeps the same target stable while the dialog is open.
        self.session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::Single {
            id: conn.id,
            name: conn.name,
        });
        self.close_session_row_menus();
        cx.notify();
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
        self.session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::SerialProfile {
            id: profile.id,
            name: profile.name,
        });
        self.close_session_row_menus();
        cx.notify();
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
        self.session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::TelnetProfile {
            id: id.to_string(),
            name: profile.name.clone(),
        });
        cx.notify();
    }

    pub(super) fn request_delete_selected_connections(&mut self, cx: &mut Context<Self>) {
        let ids = self
            .session_manager
            .selected_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }
        // Batch delete follows Tauri's confirm closure and freezes the selected
        // ids at the time the destructive action is requested.
        self.session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::Batch { ids });
        self.session_manager.show_batch_move = false;
        self.close_session_row_menus();
        cx.notify();
    }

    pub(super) fn cancel_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        self.session_manager.delete_confirm = None;
        cx.notify();
    }

    pub(super) fn confirm_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.session_manager.delete_confirm.take() else {
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
            SessionManagerDeleteConfirm::Batch { ids } => self.delete_connections_by_id(ids, cx),
        }
    }

    pub(super) fn delete_serial_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.connection_store.delete_serial_profile(id) {
            Ok(true) => {
                self.session_manager.status =
                    Some(self.i18n.t("sessionManager.serial_profiles.delete"));
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Ok(false) => {
                self.session_manager.status =
                    Some(self.i18n.t("sessionManager.serial_profiles.delete_failed"));
            }
            Err(error) => {
                self.session_manager.status = Some(format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.serial_profiles.delete_failed")
                ));
            }
        }
        cx.notify();
    }

    pub(super) fn delete_telnet_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.connection_store.delete_telnet_profile(id) {
            Ok(true) => {
                self.session_manager.status =
                    Some(self.i18n.t("sessionManager.telnet_profiles.delete"));
            }
            Ok(false) => {
                self.session_manager.status =
                    Some(self.i18n.t("sessionManager.telnet_profiles.delete_failed"));
            }
            Err(error) => {
                self.session_manager.status = Some(format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.telnet_profiles.delete_failed")
                ));
            }
        }
        cx.notify();
    }

    pub(super) fn open_saved_serial_profile(
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
        match self.create_serial_terminal_tab(config, window, cx) {
            Ok(_) => {
                let _ = self.connection_store.mark_serial_profile_used(id);
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => {
                self.session_manager.status = Some(format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.serial_profiles.open_failed")
                ));
            }
        }
        cx.notify();
    }

    pub(super) fn open_saved_telnet_profile(
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
        match self.create_telnet_terminal_tab(config, window, cx) {
            Ok(_) => {
                let _ = self.connection_store.mark_telnet_profile_used(id);
            }
            Err(error) => {
                self.session_manager.status = Some(format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.telnet_profiles.open_failed")
                ));
            }
        }
        cx.notify();
    }

    pub(super) fn delete_connections_by_id(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        let mut deleted = 0;
        for id in ids {
            if self.connection_store.delete(&id).unwrap_or(false) {
                // Keep batch delete aligned with the single-delete command path.
                if let Err(error) = self.forwarding_registry.delete_owned_forwards(&id) {
                    self.session_manager.status = Some(error.to_string());
                    cx.notify();
                    return;
                }
                self.release_ide_runtime_for_saved_connection(&id, cx);
                deleted += 1;
            }
        }
        self.session_manager.selected_ids.clear();
        self.session_manager.show_batch_move = false;
        self.session_manager.status = Some(connections_deleted_label(&self.i18n, deleted));
        if deleted > 0 {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
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
        form.name = duplicate_connection_template_name(
            &conn.name,
            self.connection_store
                .connections()
                .iter()
                .map(|connection| connection.name.as_str()),
        );
        form.focused_field = NewConnectionField::Name;
        form.field_focused = true;

        self.prepare_modal_interaction_boundary();
        self.new_connection_form = Some(form);
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.editing_saved_connection_connect_after_save_node_id = None;
        self.duplicating_saved_connection_id = Some(id.to_string());
        self.saved_connection_prompt_action = None;
        self.close_session_row_menus();
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
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
            self.session_manager.status = Some(self.i18n.t("sessionManager.toast.test_failed"));
            cx.notify();
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
        cx.notify();
    }

    pub(super) fn move_selected_connections(
        &mut self,
        group: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let ids = self
            .session_manager
            .selected_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        match self.connection_store.move_to_group(&ids, group) {
            Ok(count) => {
                self.session_manager.status = Some(connections_moved_label(
                    &self.i18n,
                    count,
                    group_label(&self.i18n, group),
                ));
                self.session_manager.selected_ids.clear();
                self.session_manager.show_batch_move = false;
                if count > 0 {
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
            }
            Err(error) => self.session_manager.status = Some(error.to_string()),
        }
        cx.notify();
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
