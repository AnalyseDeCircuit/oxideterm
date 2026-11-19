// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn session_manager_catalog_environment(
        &self,
        include_items: bool,
        cx: &App,
    ) -> CatalogEnvironment {
        let catalog = CatalogEnvironment::from_store(
            self.connections.session_manager.read(cx),
            &self.connection_store,
            self.saved_ssh_tunnels(),
            include_items,
        );
        let tunnel_status = catalog.items.iter().filter_map(|item| match item {
            SessionManagerDisplayItem::Tunnel(_) => Some((item.id().to_owned(), self.ssh_tunnel_status(item.id()))),
            _ => None,
        }).collect();
        let settings = self.settings_store.settings();
        let mut horizontal_chrome_width = self.context_sidebar_occupied_width(cx);
        if !settings.sidebar_ui.zen_mode {
            horizontal_chrome_width += self.tokens.metrics.activity_bar_width;
            if !self.sidebar_collapsed {
                horizontal_chrome_width += self.sidebar_panel_width();
            }
        }
        CatalogEnvironment {
            tunnel_status,
            horizontal_chrome_width,
            chrome_divider: self.workspace_chrome_divider(),
            has_background: self.background_surface_active("session_manager"),
            ui_font: settings_ui_font_family(&settings.appearance.ui_font_family).into(),
            sidebar_background: self.workspace_sidebar_background(self.tokens.ui.shell_bg()),
            sidebar_sort_anchor: self
                .select_anchors
                .get(&SelectAnchorId::SessionManagerSidebarSort)
                .copied(),
            ..catalog
        }
    }

    pub(in crate::workspace) fn apply_session_manager_catalog_action(
        &mut self,
        action: CatalogAction,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        match action {
            CatalogAction::Open {
                target: SessionManagerOpenTarget::Plugin(id),
                ..
            } => {
                self.open_plugin_connection(&id, cx);
                return;
            }
            CatalogAction::Delete(target) => {
                return self.request_session_manager_row_delete(target, cx);
            }
            CatalogAction::ImportSshConfig(alias) => {
                return self.import_session_manager_ssh_config_host(alias, cx);
            }
            CatalogAction::OpenSshConfigImporter => {
                return self.open_settings_ssh_config_import_dialog(cx);
            }
            CatalogAction::Move { targets, group } => {
                return self.move_dragged_sessions_to_group(&targets, group.as_deref(), cx);
            }
            CatalogAction::StopTunnel(id) => {
                self.stop_ssh_tunnel(&id, false, cx);
                return;
            }
            CatalogAction::ShowBatchAdd => return self.open_batch_add_dialog(cx),
            CatalogAction::SaveGroup => return self.submit_session_group_editor(cx),
            CatalogAction::ConfirmDelete => return self.confirm_session_manager_delete(cx),
            CatalogAction::MoveSelected(group) => {
                return self.move_selected_connections(group.as_deref(), cx);
            }
            CatalogAction::CloseSidebarSort => {
                if self.connections.session_manager.update(cx, |manager, cx| {
                    manager.set_sidebar_sort_open(false, cx)
                }) {
                    cx.notify();
                }
                return;
            }
            _ => {}
        }
        let Some(window) = window else {
            return;
        };
        match action {
            CatalogAction::Open { target, open_sftp } => {
                self.open_session_manager_target_with_sftp(target, open_sftp, window, cx)
            }
            CatalogAction::Edit(target) => match target {
                SessionManagerOpenTarget::Tunnel(id) => {
                    self.open_ssh_tunnel_form(Some(&id), window, cx)
                }
                SessionManagerOpenTarget::Connection(id) => {
                    self.open_saved_connection_editor(&id, None, window, cx)
                }
                SessionManagerOpenTarget::Plugin(id) => {
                    self.edit_plugin_connection(&id, window, cx)
                }
                SessionManagerOpenTarget::Ftp(id) => {
                    self.open_saved_ftp_profile_editor(&id, window, cx)
                }
                SessionManagerOpenTarget::LocalTerminal(id) => {
                    self.open_saved_local_terminal_profile_editor(&id, window, cx)
                }
                SessionManagerOpenTarget::SshConfig(_) => {}
                target => self.open_catalog_profile_editor(target, window, cx),
            },
            CatalogAction::Duplicate(id) => self.duplicate_connection(&id, window, cx),
            CatalogAction::Test(id) => self.test_connection(&id, window, cx),
            CatalogAction::OpenImporters => self.open_connection_importers_settings(window, cx),
            CatalogAction::StartTunnels(ids) => {
                self.start_ssh_tunnels(&ids, false, window, cx);
            }
            CatalogAction::NewConnectionInGroup(group) => {
                self.open_new_connection_form(window, cx);
                self.connections
                    .flow
                    .update(cx, |flow, cx| flow.set_form_group(group, cx));
            }
            _ => {}
        }
    }
}
