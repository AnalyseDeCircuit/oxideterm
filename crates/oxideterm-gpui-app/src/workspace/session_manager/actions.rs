impl WorkspaceApp {
    fn filtered_session_connections(&self) -> Vec<ConnectionInfo> {
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
        self.sort_session_rows(&mut rows);
        if self.session_manager.selected_group.as_deref() == Some(RECENT_FILTER) {
            rows.truncate(20);
        }
        rows
    }

    fn filtered_session_serial_profiles(&self) -> Vec<SerialProfile> {
        let query = self.session_manager.search_query.trim().to_lowercase();
        let mut rows = self.connection_store.serial_profiles().to_vec();
        rows.retain(|profile| self.serial_profile_matches_filter(profile));
        if !query.is_empty() {
            rows.retain(|profile| {
                profile.name.to_lowercase().contains(&query)
                    || profile.port_path.to_lowercase().contains(&query)
                    || profile
                        .group
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            });
        }
        rows.sort_by(|left, right| right.last_used_at.cmp(&left.last_used_at));
        if self.session_manager.selected_group.as_deref() == Some(RECENT_FILTER) {
            rows.truncate(20);
        }
        rows
    }

    fn connection_matches_filter(&self, conn: &ConnectionInfo) -> bool {
        match self.session_manager.selected_group.as_deref() {
            None => true,
            Some(UNGROUPED_FILTER) => conn.group.is_none(),
            Some(RECENT_FILTER) => conn.last_used_at.is_some(),
            Some(group) => conn.group.as_deref().is_some_and(|conn_group| {
                conn_group == group || conn_group.starts_with(&format!("{group}/"))
            }),
        }
    }

    fn serial_profile_matches_filter(&self, profile: &SerialProfile) -> bool {
        match self.session_manager.selected_group.as_deref() {
            None => true,
            Some(UNGROUPED_FILTER) => profile.group.is_none(),
            Some(RECENT_FILTER) => profile.last_used_at.is_some(),
            Some(group) => profile.group.as_deref().is_some_and(|profile_group| {
                profile_group == group || profile_group.starts_with(&format!("{group}/"))
            }),
        }
    }

    fn sort_session_rows(&self, rows: &mut [ConnectionInfo]) {
        let field = self.session_manager.sort_field;
        rows.sort_by(|left, right| {
            let ordering = match field {
                SessionSortField::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                SessionSortField::Host => left.host.to_lowercase().cmp(&right.host.to_lowercase()),
                SessionSortField::Port => left.port.cmp(&right.port),
                SessionSortField::Username => left
                    .username
                    .to_lowercase()
                    .cmp(&right.username.to_lowercase()),
                SessionSortField::AuthType => {
                    auth_label(left.auth_type).cmp(&auth_label(right.auth_type))
                }
                SessionSortField::Group => left.group.cmp(&right.group),
                SessionSortField::LastUsed => left.last_used_at.cmp(&right.last_used_at),
            };
            match self.session_manager.sort_direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            }
        });
    }

    fn connection_count_for_group(&self, group: &str) -> usize {
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
        connection_count + serial_count
    }

    fn session_group_tree(&self) -> (Vec<String>, HashMap<String, Vec<String>>) {
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

    fn toggle_session_group_expanded(&mut self, group: &str) {
        if self.session_manager.expanded_groups.contains(group) {
            self.session_manager.expanded_groups.remove(group);
        } else {
            self.session_manager
                .expanded_groups
                .insert(group.to_string());
        }
    }

    fn connection_info_by_id(&self, id: &str) -> Option<ConnectionInfo> {
        self.connection_store
            .connection_infos()
            .into_iter()
            .find(|conn| conn.id == id)
    }

    pub(in crate::workspace) fn close_session_row_menus(&mut self) -> bool {
        // SessionManager owns inline row menus and tree context menus. Radix
        // closes them through one ContextMenu root, so native exposes one
        // dismissal owner for outside click, Esc, and guarded item activation.
        let changed = self.session_manager.row_menu_connection_id.is_some()
            || self.session_manager.row_context_menu_connection_id.is_some()
            || self.session_manager.folder_tree_context_menu_x.is_some()
            || self.session_manager.folder_tree_context_menu_y.is_some();
        self.session_manager.row_menu_connection_id = None;
        self.session_manager.row_context_menu_connection_id = None;
        self.session_manager.folder_tree_context_menu_x = None;
        self.session_manager.folder_tree_context_menu_y = None;
        changed
    }

    fn open_session_row_context_menu(&mut self, id: &str, x: f32, y: f32) {
        // Opening a Radix ContextMenu replaces any sibling menu owner. Native
        // row context menus share the same close helper so right-click cannot
        // leave an inline "more" menu or folder menu alive behind it.
        self.close_session_row_menus();
        self.select_connection_for_context_menu(id);
        self.session_manager.row_context_menu_connection_id = Some(id.to_string());
        self.session_manager.row_context_menu_x = x;
        self.session_manager.row_context_menu_y = y;
    }

    fn toggle_session_row_more_menu(&mut self, id: &str, x: f32, y: f32) {
        let same_row_open = self.session_manager.row_menu_connection_id.as_deref() == Some(id);
        self.close_session_row_menus();
        if !same_row_open {
            // The inline "more" trigger is rendered inside the table row, but
            // the menu itself is portaled to the surface with a shared backdrop
            // so outside click and Esc follow the same Radix close owner.
            self.session_manager.row_menu_connection_id = Some(id.to_string());
            self.session_manager.row_menu_x = x;
            self.session_manager.row_menu_y = y;
        }
    }

    fn open_session_folder_tree_context_menu(&mut self, x: f32, y: f32) {
        // FolderTree's blank-area context menu is a sibling of row menus in
        // Tauri. Keep the replacement rule explicit before assigning the new
        // tree menu coordinates.
        self.close_session_row_menus();
        self.session_manager.folder_tree_context_menu_x = Some(x);
        self.session_manager.folder_tree_context_menu_y = Some(y);
        self.session_manager.show_batch_move = false;
    }

    fn toggle_connection_selection(&mut self, id: &str) {
        if self.session_manager.selected_ids.contains(id) {
            self.session_manager.selected_ids.remove(id);
        } else {
            self.session_manager.selected_ids.insert(id.to_string());
        }
    }

    fn select_connection_for_context_menu(&mut self, id: &str) {
        // Browser file/table UIs keep an existing multi-selection when the
        // context target is already selected, but right-clicking an unselected
        // row first moves selection to that row before opening the menu.
        crate::workspace::browser_behavior::preserve_or_move_context_selection(
            &mut self.session_manager.selected_ids,
            id.to_string(),
        );
    }

    fn toggle_all_visible_connections(&mut self, cx: &mut Context<Self>) {
        let rows = self.filtered_session_connections();
        let all_selected = !rows.is_empty()
            && rows
                .iter()
                .all(|row| self.session_manager.selected_ids.contains(&row.id));
        if all_selected {
            for row in rows {
                self.session_manager.selected_ids.remove(&row.id);
            }
        } else {
            for row in rows {
                self.session_manager.selected_ids.insert(row.id);
            }
        }
        cx.notify();
    }

    pub(super) fn clear_session_selection_for_invisible_rows(&mut self) {
        let visible_ids = self
            .filtered_session_connections()
            .into_iter()
            .map(|conn| conn.id)
            .collect::<HashSet<_>>();
        self.session_manager
            .selected_ids
            .retain(|id| visible_ids.contains(id));
    }

    fn create_session_group(&mut self, cx: &mut Context<Self>) {
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
    fn delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
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

    fn request_delete_connection(&mut self, id: &str, cx: &mut Context<Self>) {
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

    fn request_delete_serial_profile(&mut self, id: &str, cx: &mut Context<Self>) {
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

    fn request_delete_selected_connections(&mut self, cx: &mut Context<Self>) {
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

    fn cancel_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        self.session_manager.delete_confirm = None;
        cx.notify();
    }

    fn confirm_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.session_manager.delete_confirm.take() else {
            return;
        };
        match confirm {
            SessionManagerDeleteConfirm::Single { id, .. } => self.delete_connection(&id, cx),
            SessionManagerDeleteConfirm::SerialProfile { id, .. } => {
                self.delete_serial_profile(&id, cx)
            }
            SessionManagerDeleteConfirm::Batch { ids } => self.delete_connections_by_id(ids, cx),
        }
    }

    fn delete_serial_profile(&mut self, id: &str, cx: &mut Context<Self>) {
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

    fn open_saved_serial_profile(
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

    fn delete_connections_by_id(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
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

    fn duplicate_connection(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
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
        // Tauri duplicate mode does not copy privilege credentials. Keep the
        // native draft empty so saved secrets are never silently aliased.
        form.privilege_credentials.clear();
        form.privilege_draft = Default::default();
        form.focused_field = NewConnectionField::Name;
        form.field_focused = true;

        self.prepare_modal_interaction_boundary();
        self.new_connection_form = Some(form);
        self.drill_down_parent_node_id = None;
        self.editing_saved_connection_id = None;
        self.duplicating_saved_connection_id = Some(id.to_string());
        self.saved_connection_prompt_action = None;
        self.close_session_row_menus();
        self.close_new_connection_select();
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn test_connection(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
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
        self.start_ssh_test_flow(config, conn.name.clone(), cx);
        cx.notify();
    }

    fn move_selected_connections(&mut self, group: Option<&str>, cx: &mut Context<Self>) {
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

    #[allow(dead_code)]
    fn open_ssh_config_import(&mut self, cx: &mut Context<Self>) {
        let names = self
            .connection_store
            .connections()
            .iter()
            .map(|conn| conn.name.clone())
            .collect::<HashSet<_>>();
        match list_ssh_config_hosts(&names) {
            Ok(hosts) => {
                self.session_manager.selected_import_aliases = hosts
                    .iter()
                    .filter(|host| !host.already_imported)
                    .map(|host| host.alias.clone())
                    .collect();
                self.session_manager.ssh_config_hosts = hosts;
                self.session_manager.show_import = true;
                // SSH import opens without a text field; the footer focus ring
                // is still keyboard-owned and starts unset until Tab is pressed.
                self.session_manager.focused_basic_dialog_footer_action = None;
                self.session_manager.status = None;
            }
            Err(error) => self.session_manager.status = Some(error.to_string()),
        }
        cx.notify();
    }

    fn import_selected_ssh_hosts(&mut self, cx: &mut Context<Self>) {
        let aliases = self
            .session_manager
            .selected_import_aliases
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut imported = 0;
        let mut errors = Vec::new();
        for alias in aliases {
            match resolve_ssh_config_alias(&alias) {
                Ok(Some(host)) => match saved_connection_from_ssh_host(host) {
                    Ok(connection) => {
                        if self
                            .connection_store
                            .import_ssh_connection(connection)
                            .is_ok()
                        {
                            imported += 1;
                        }
                    }
                    Err(error) => errors.push(format!("{alias}: {error}")),
                },
                Ok(None) => errors.push(alias),
                Err(error) => errors.push(format!("{alias}: {error}")),
            }
        }
        self.session_manager.show_import = false;
        self.session_manager.selected_import_aliases.clear();
        self.session_manager.focused_basic_dialog_footer_action = None;
        self.session_manager.status = if errors.is_empty() {
            Some(format!("Imported {imported}"))
        } else {
            Some(format!("Imported {imported}; {}", errors.join(", ")))
        };
        if imported > 0 {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
    }
}
