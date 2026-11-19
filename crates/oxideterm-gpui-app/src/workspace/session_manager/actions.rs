use super::*;
use crate::workspace::new_connection::{
    form_from_remote_desktop_profile, terminal_serial_flow_from_profile,
    terminal_serial_parity_from_profile, terminal_serial_runtime_options_from_profile,
};
use crate::workspace::terminal_command_bar::{
    SavedConsoleProfileKind, delete_saved_console_profile,
};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopSecret,
};

impl WorkspaceApp {
    fn update_catalog_groups(
        &mut self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut CatalogGroups<'_>, &mut App),
    ) {
        let tunnels = self.saved_ssh_tunnels();
        let mut groups = CatalogGroups {
            manager: &self.connections.session_manager,
            store: &mut self.connection_store,
            i18n: &self.i18n,
            tunnels,
            forward_store: self.forwarding_service.registry().saved_store(),
            wrote: false,
        };
        update(&mut groups, cx);
        if groups.wrote {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }
    pub(super) fn connection_count_for_group(&self, group: &str) -> usize {
        connection_count_for_group(group, &self.connection_store, &self.saved_ssh_tunnels())
    }

    pub(super) fn session_group_tree(&self) -> (Vec<String>, HashMap<String, Vec<String>>) {
        session_group_tree(&self.connection_store, &self.saved_ssh_tunnels())
    }

    pub(in crate::workspace) fn close_session_row_menus(&mut self, cx: &mut Context<Self>) -> bool {
        self.connections
            .session_manager
            .update(cx, |session_manager, cx| {
                let changed = close_session_menu_state(session_manager);
                if changed {
                    cx.notify();
                }
                changed
            })
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
        self.connections
            .session_manager
            .update(cx, |session_manager, cx| {
                let previous_count = session_manager.selected_items.len();
                session_manager
                    .selected_items
                    .retain(|target| visible_items.contains(target));
                // A hidden anchor row would make the next Shift+click degrade or
                // span across rows the user cannot see; keep anchors that are
                // still visible.
                if session_manager
                    .selection_anchor
                    .as_ref()
                    .is_some_and(|anchor| !visible_items.contains(anchor))
                {
                    session_manager.selection_anchor = None;
                }
                if session_manager.selected_items.len() != previous_count {
                    cx.notify();
                }
            });
    }

    pub(super) fn submit_session_group_editor(&mut self, cx: &mut Context<Self>) {
        self.update_catalog_groups(cx, |catalog, cx| catalog.submit_session_group_editor(cx));
    }

    pub(super) fn open_session_group_creation(&mut self, cx: &mut Context<Self>) {
        self.update_catalog_groups(cx, |catalog, cx| catalog.open_session_group_creation(cx));
        self.ime_marked_text = None;
    }

    pub(super) fn open_session_subgroup_creation(
        &mut self,
        parent_path: &str,
        cx: &mut Context<Self>,
    ) {
        self.update_catalog_groups(cx, |catalog, cx| {
            catalog.open_session_subgroup_creation(parent_path, cx)
        });
        self.ime_marked_text = None;
    }

    pub(super) fn open_session_group_rename(&mut self, group: &str, cx: &mut Context<Self>) {
        self.update_catalog_groups(cx, |catalog, cx| {
            catalog.open_session_group_rename(group, cx)
        });
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn close_session_group_manager(&mut self, cx: &mut Context<Self>) {
        self.update_catalog_groups(cx, |catalog, cx| catalog.close_session_group_manager(cx));
        self.ime_marked_text = None;
    }

    fn delete_session_group(&mut self, group: &str, cx: &mut Context<Self>) {
        self.update_catalog_groups(cx, |catalog, cx| catalog.delete_session_group(group, cx));
    }

    pub(super) fn confirm_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        use SessionManagerDeleteConfirm as Confirm;
        use SessionManagerSelectionTarget as Target;
        let Some(confirm) = self
            .connections
            .session_manager
            .update(cx, SessionManagerState::take_delete)
        else {
            return;
        };
        let target = match confirm {
            Confirm::Single { id, .. } => Target::Connection(id),
            Confirm::SerialProfile { id, .. } => Target::Serial(id),
            Confirm::BleProfile { id, .. } => Target::Ble(id),
            Confirm::LocalTerminalProfile { id, .. } => Target::LocalTerminal(id),
            Confirm::TelnetProfile { id, .. } => Target::Telnet(id),
            Confirm::MoshProfile { id, .. } => Target::Mosh(id),
            Confirm::StandaloneSftpProfile { id, .. } => Target::StandaloneSftp(id),
            Confirm::RemoteDesktopProfile { id, .. } => Target::RemoteDesktop(id),
            Confirm::Batch { targets } => return self.delete_selected_session_items(targets, cx),
            Confirm::Group { name } => return self.delete_session_group(&name, cx),
        };
        let result = self.remove_catalog_target(&target, true, cx);
        if self.connections.session_manager.update(cx, |state, cx| {
            state.finish_single_delete(target, result, &self.i18n, cx)
        }) {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    fn remove_catalog_target(
        &mut self,
        target: &SessionManagerSelectionTarget,
        single: bool,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<bool> {
        use SessionManagerSelectionTarget as Target;
        let console = match target {
            Target::Serial(id) => Some((SavedConsoleProfileKind::Serial, id)),
            Target::Telnet(id) => Some((SavedConsoleProfileKind::Telnet, id)),
            Target::Mosh(id) => Some((SavedConsoleProfileKind::Mosh, id)),
            _ => None,
        };
        if let Some((kind, id)) = console {
            return delete_saved_console_profile(
                &mut self.connection_store,
                &self.terminal_history_store,
                kind,
                id,
            );
        }
        match target {
            Target::Tunnel(id) => {
                self.stop_ssh_tunnel(id, true, cx);
                Ok(false)
            }
            Target::Connection(id) => {
                let removed = match self.connection_store.delete(id) {
                    Err(_) if !single => return Ok(false),
                    result => result?,
                };
                if removed || single {
                    // Owner-bound forwards must not be restored by a later sync.
                    self.forwarding_service
                        .registry()
                        .delete_owned_forwards(id)
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                    self.release_ide_runtime_for_saved_connection(id, cx);
                }
                Ok(removed || single)
            }
            _ => delete_catalog_profile(&mut self.connection_store, target),
        }
    }

    pub(super) fn cancel_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        self.connections
            .session_manager
            .update(cx, SessionManagerState::cancel_delete);
    }

    pub(super) fn request_session_manager_row_delete(
        &mut self,
        target: SessionManagerRowActionTarget,
        cx: &mut Context<Self>,
    ) {
        if let SessionManagerRowActionTarget::Group(group) = target {
            self.update_catalog_groups(cx, |catalog, cx| {
                catalog.request_delete_session_group(&group, cx)
            });
        } else {
            self.connections.session_manager.update(cx, |state, cx| {
                state.request_row_delete(target, &self.connection_store, cx)
            });
        }
    }

    pub(super) fn request_delete_selected_connections(&mut self, cx: &mut Context<Self>) {
        self.connections
            .session_manager
            .update(cx, SessionManagerState::request_selected_delete);
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
            pre_connect_command: profile.pre_connect_command.clone(),
            port_path: profile.port_path.clone(),
            baud_rate: profile.baud_rate,
            data_bits: profile.data_bits,
            stop_bits: profile.stop_bits,
            parity: terminal_serial_parity_from_profile(&profile.parity),
            flow_control: terminal_serial_flow_from_profile(&profile.flow_control),
            runtime_options: terminal_serial_runtime_options_from_profile(&profile),
        };
        match self.create_serial_terminal_tab(
            config,
            &profile.name,
            profile.terminal.clone(),
            window,
            cx,
        ) {
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
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(status), cx)
                    });
            }
        }
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
            pre_connect_command: profile.pre_connect_command.clone(),
            line_endings: new_connection::terminal_telnet_line_endings_from_profile(&profile),
            host: profile.host.clone(),
            port: profile.port,
        };
        match self.create_telnet_terminal_tab(
            config,
            profile.upstream_proxy,
            profile.terminal,
            window,
            cx,
        ) {
            Ok(session_id) => {
                self.telnet_terminal_profile_ids
                    .insert(session_id, profile.id.clone());
                self.register_terminal_saved_connection(
                    session_id,
                    oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                    profile.id,
                    cx,
                );
                let _ = self.connection_store.mark_telnet_profile_used(id);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.telnet_profiles.open_failed")
                );
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(status), cx)
                    });
            }
        }
    }

    pub(in crate::workspace) fn open_saved_remote_desktop_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_remote_desktop_profile_for_connection(id, None, window, cx);
    }

    pub(in crate::workspace) fn open_saved_remote_desktop_profile_for_connection(
        &mut self,
        id: &str,
        runtime_connection_attempt_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self
            .connection_store
            .get_remote_desktop_profile(id)
            .cloned()
        else {
            if let Some(connection_attempt_id) = runtime_connection_attempt_id.as_deref() {
                self.standalone_connections
                    .mark_attempt_error(connection_attempt_id);
            }
            return;
        };
        let password = match self.connection_store.get_remote_desktop_credential(id) {
            Ok(secret) => secret
                .map(SecretString::into_zeroizing)
                .map(RemoteDesktopSecret::from),
            Err(error) => {
                if let Some(connection_attempt_id) = runtime_connection_attempt_id.as_deref() {
                    self.standalone_connections
                        .mark_attempt_error(connection_attempt_id);
                }
                let status = format!(
                    "{}: {error}",
                    self.i18n
                        .t("sessionManager.remote_desktop_profiles.open_failed")
                );
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(status), cx)
                    });
                return;
            }
        };
        if saved.protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp
            && password.is_none()
        {
            if let Some(connection_attempt_id) = runtime_connection_attempt_id.as_deref() {
                self.standalone_connections
                    .mark_attempt_error(connection_attempt_id);
            }
            // Synced and imported assets intentionally omit device-local credentials.
            // Reopen the regular form so the user can authenticate on this device.
            self.open_new_connection_form(window, cx);
            let password_required = self
                .i18n
                .t("modals.new_connection.remote_desktop_password_required");
            let reconnect_id = runtime_connection_attempt_id
                .as_deref()
                .and_then(|attempt| {
                    self.standalone_connections
                        .connection_id_for_attempt(attempt)
                });
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    *form = form_from_remote_desktop_profile(&saved, String::new());
                    form.standalone_connection_id = reconnect_id;
                    form.error = Some(password_required);
                    form.focused_field = NewConnectionField::Password;
                }
            });
            return;
        }
        let socks_proxy = if saved.protocol == oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp
        {
            match oxideterm_session_adapter::rdp_socks_proxy_from_saved_policy(
                &self.connection_store,
                self.settings_store.settings(),
                &saved.upstream_proxy,
                saved.ssh_gateway_connection_id.is_some(),
            ) {
                Ok(proxy) => proxy,
                Err(error) => {
                    if let Some(attempt) = runtime_connection_attempt_id.as_deref() {
                        self.standalone_connections.mark_attempt_error(attempt);
                    }
                    let status = format!(
                        "{}: {error}",
                        self.i18n
                            .t("modals.new_connection.remote_desktop_proxy_failed")
                    );
                    self.connections
                        .session_manager
                        .update(cx, |manager, cx| manager.set_status(Some(status), cx));
                    return;
                }
            }
        } else {
            None
        };
        let profile = RemoteDesktopConnectionProfile {
            pre_connect_command: saved.pre_connect_command.clone(),
            id: saved.id.clone(),
            label: saved.name,
            protocol: saved.protocol,
            endpoint: RemoteDesktopEndpoint::new(saved.host, saved.port),
            transport_endpoint: None,
            socks_proxy,
            username: saved.username,
            domain: saved.domain,
            credential_ref: saved.credential_ref,
            read_only: saved.read_only,
            session_options: saved.session_options,
        };
        self.open_remote_desktop_connection_for_connection(
            profile,
            password,
            saved.ssh_gateway_connection_id,
            runtime_connection_attempt_id,
            window,
            cx,
        );
        let _ = self.connection_store.mark_remote_desktop_profile_used(id);
        self.queue_cloud_sync_dirty_refresh(cx);
    }

    pub(super) fn delete_selected_session_items(
        &mut self,
        targets: Vec<SessionManagerSelectionTarget>,
        cx: &mut Context<Self>,
    ) {
        use SessionManagerSelectionTarget as Target;
        let mut deleted = 0;
        for target in targets {
            match self.remove_catalog_target(&target, false, cx) {
                Ok(true) => deleted += 1,
                Ok(false) => {}
                Err(error) => {
                    let status = match &target {
                        Target::Connection(_) | Target::Ftp(_) => error.to_string(),
                        Target::Plugin(_) => self.i18n.t("plugin.connections.failed"),
                        _ => continue,
                    };
                    self.connections
                        .session_manager
                        .update(cx, |state, cx| state.set_status(Some(status), cx));
                    if !matches!(target, Target::Ftp(_)) {
                        return;
                    }
                }
            }
        }
        self.connections.session_manager.update(cx, |state, cx| {
            state.finish_batch_delete(deleted, &self.i18n, cx)
        });
        if deleted > 0 {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    pub(super) fn open_catalog_profile_editor(
        &mut self,
        target: SessionManagerOpenTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = saved_catalog_profile_form(&self.connection_store, &target, &self.i18n)
        else {
            return;
        };
        self.open_new_connection_form(window, cx);
        self.update_connection_form_state(cx, |state| state.replace_with_new_form(form));
        // Refresh discovery without replacing the persisted device selected for editing.
        match target {
            SessionManagerOpenTarget::Serial(_) => self.refresh_serial_ports(cx),
            SessionManagerOpenTarget::Ble(_) => self.refresh_ble_adapters(cx),
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn duplicate_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = duplicate_catalog_connection_form(&self.connection_store, id) else {
            return;
        };

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
            self.connections
                .session_manager
                .update(cx, |session_manager, cx| {
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
        self.update_catalog_groups(cx, |catalog, cx| {
            catalog.move_selected_connections(group, cx)
        });
    }

    pub(super) fn move_dragged_sessions_to_group(
        &mut self,
        targets: &[SessionManagerSelectionTarget],
        group: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.update_catalog_groups(cx, |catalog, cx| {
            catalog.move_dragged_sessions_to_group(targets, group, cx)
        });
    }
}
