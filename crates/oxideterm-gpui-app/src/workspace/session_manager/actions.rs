use super::*;
use crate::workspace::terminal_command_bar::delete_saved_console_profile;
use oxideterm_remote_desktop::RemoteDesktopSecret;

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
        let visible = self.session_manager_display_items(cx);
        self.connections.session_manager.update(cx, |manager, cx| {
            manager.retain_visible_selection(visible, cx);
        });
    }

    pub(super) fn submit_session_group_editor(&mut self, cx: &mut Context<Self>) {
        self.update_catalog_groups(cx, |catalog, cx| catalog.submit_session_group_editor(cx));
    }

    pub(in crate::workspace) fn close_session_group_manager(&mut self, cx: &mut Context<Self>) {
        self.connections
            .session_manager
            .update(cx, SessionManagerState::close_session_group_manager);
        self.ime_marked_text = None;
    }

    fn delete_session_group(&mut self, group: &str, cx: &mut Context<Self>) {
        self.update_catalog_groups(cx, |catalog, cx| catalog.delete_session_group(group, cx));
    }

    pub(super) fn confirm_session_manager_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self
            .connections
            .session_manager
            .update(cx, SessionManagerState::take_catalog_deletion)
        else {
            return;
        };
        let target = match confirm {
            CatalogDeletion::Single(target) => target,
            CatalogDeletion::Batch(targets) => {
                return self.delete_selected_session_items(targets, cx);
            }
            CatalogDeletion::Group(group) => return self.delete_session_group(&group, cx),
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
        match remove_catalog_target(&mut self.connection_store, target, single)? {
            CatalogRemoval::Completed(changed) => Ok(changed),
            CatalogRemoval::DeleteConsole { id, kind } => delete_saved_console_profile(
                &mut self.connection_store,
                &self.terminal_history_store,
                kind,
                id,
            ),
            CatalogRemoval::StopTunnel(id) => {
                self.stop_ssh_tunnel(id, true, cx);
                Ok(false)
            }
            CatalogRemoval::ReleaseSavedSsh(id) => {
                // Owner-bound forwards must not be restored by a later sync.
                self.forwarding_service
                    .registry()
                    .delete_owned_forwards(id)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                self.release_ide_runtime_for_saved_connection(id, cx);
                Ok(true)
            }
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
        self.connections.session_manager.update(cx, |state, cx| {
            state.request_row_delete(target, &self.connection_store, cx)
        });
    }

    pub(in crate::workspace) fn open_saved_serial_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_console_profile(DirectConsoleProfileKind::Serial, id, window, cx);
    }

    pub(in crate::workspace) fn open_saved_telnet_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_console_profile(DirectConsoleProfileKind::Telnet, id, window, cx);
    }

    fn open_saved_console_profile(
        &mut self,
        kind: DirectConsoleProfileKind,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(launch) = prepare_saved_console_launch(&self.connection_store, kind, id) else {
            return;
        };
        let result = match launch {
            SavedConsoleLaunch::Serial {
                config,
                title,
                terminal,
            } => self.create_serial_terminal_tab(config, &title, terminal, window, cx),
            SavedConsoleLaunch::Telnet {
                config,
                proxy,
                terminal,
            } => self.create_telnet_terminal_tab(config, proxy, terminal, window, cx),
        };
        match result {
            Ok(session_id) => {
                if kind == DirectConsoleProfileKind::Telnet {
                    self.telnet_terminal_profile_ids
                        .insert(session_id, id.to_owned());
                }
                self.register_terminal_saved_connection(
                    session_id,
                    kind.connection_kind(),
                    id.to_owned(),
                    cx,
                );
                if mark_catalog_console_used(&mut self.connection_store, kind, id) {
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
            }
            Err(error) => {
                self.connections.session_manager.update(cx, |state, cx| {
                    state.report_console_open_failure(kind, &error, &self.i18n, cx);
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
        use oxideterm_gpui_connections::submission::{
            SavedDesktopOpen, prepare_saved_desktop_open,
        };
        let plan = prepare_saved_desktop_open(
            &self.connection_store,
            self.settings_store.settings(),
            id,
            &self.i18n,
        );
        match plan {
            Ok(Some(SavedDesktopOpen::Launch {
                profile,
                password,
                gateway,
            })) => {
                self.open_remote_desktop_connection_for_connection(
                    profile,
                    password.map(|secret| RemoteDesktopSecret::from(secret.into_zeroizing())),
                    gateway,
                    runtime_connection_attempt_id,
                    window,
                    cx,
                );
            }
            other => {
                if let Some(attempt) = runtime_connection_attempt_id.as_deref() {
                    self.standalone_connections.mark_attempt_error(attempt);
                }
                match other {
                    Ok(Some(SavedDesktopOpen::Prompt(form))) => {
                        let reconnect_id =
                            runtime_connection_attempt_id
                                .as_deref()
                                .and_then(|attempt| {
                                    self.standalone_connections
                                        .connection_id_for_attempt(attempt)
                                });
                        self.open_new_connection_form(window, cx);
                        self.connections.flow.update(cx, |flow, cx| {
                            flow.open_desktop_reconnect_form(*form, reconnect_id, cx)
                        });
                    }
                    Err(status) => {
                        self.connections.session_manager.update(cx, |manager, cx| {
                            manager.set_status(Some(status), cx);
                        });
                    }
                    _ => {}
                }
                return;
            }
        }
        let _ = self.connection_store.mark_remote_desktop_profile_used(id);
        self.queue_cloud_sync_dirty_refresh(cx);
    }

    pub(super) fn delete_selected_session_items(
        &mut self,
        targets: Vec<SessionManagerSelectionTarget>,
        cx: &mut Context<Self>,
    ) {
        let manager = self.connections.session_manager.clone();
        let i18n = self.i18n.clone();
        if delete_catalog_selection(&manager, targets, &i18n, cx, |target, cx| {
            self.remove_catalog_target(target, false, cx)
        }) {
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
        self.connections
            .flow
            .update(cx, |flow, cx| flow.replace_form(form, cx));
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
        self.connections
            .flow
            .update(cx, |flow, cx| flow.open_duplicate_form(form, id, cx));
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
        match prepare_saved_ssh_test(
            &self.connection_store,
            self.settings_store.settings(),
            id,
            &self.i18n,
        ) {
            SavedSshTest::Missing(status) => self.connections.show_catalog_status(status, cx),
            SavedSshTest::Prompt(message) => {
                self.open_saved_connection_prompt(
                    id,
                    SavedConnectionPromptAction::Test,
                    Some(message),
                    window,
                    cx,
                );
            }
            SavedSshTest::Run { config, title } => self.start_ssh_test_flow(config, title, cx),
        }
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
