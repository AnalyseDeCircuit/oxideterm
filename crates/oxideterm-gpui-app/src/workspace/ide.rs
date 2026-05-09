use gpui::{AnyElement, AppContext, Context, IntoElement, div};
use oxideterm_gpui_ide::{
    IdeLabels, IdeRuntimeSettings, IdeSurface, IdeSurfaceEvent, NodeAgentMode,
};
use oxideterm_settings::IdeAgentMode;
use oxideterm_ssh::{NodeId, ReconnectIdeSnapshot};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};
use std::time::SystemTime;

use super::WorkspaceApp;

impl WorkspaceApp {
    pub(super) fn open_ide_folder_picker_tab(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("IDE · {node_title}");
        let tab_id = if let Some((tab_id, _)) = self
            .ide_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| existing_node_id.0 == node_id.0)
        {
            if let Some(surface) = self.ide_tab_surfaces.get(tab_id) {
                surface.update(cx, |surface: &mut IdeSurface, cx| {
                    let initial_path = surface
                        .project_root_path()
                        .unwrap_or_else(|| "/".to_string());
                    surface.open_remote_folder_picker_for_node(node_id.0.clone(), initial_path, cx);
                });
            }
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id();
            let router = self.node_router.clone();
            let tokens = self.tokens;
            let labels = self.ide_labels();
            let runtime_settings = self.ide_runtime_settings();
            let backend_runtime = self.forwarding_runtime.clone();
            let surface = cx.new(|cx| {
                IdeSurface::new(
                    router,
                    tokens,
                    labels,
                    runtime_settings,
                    backend_runtime,
                    cx,
                )
            });
            surface.update(cx, |surface: &mut IdeSurface, cx| {
                surface.open_remote_folder_picker_for_node(node_id.0.clone(), "/", cx);
            });

            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Ide,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.subscribe_ide_surface(tab_id, &surface, cx);
            self.ide_tab_surfaces.insert(tab_id, surface);
            self.ide_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        self.active_tab_id = Some(tab_id);
        self.active_surface = oxideterm_gpui_settings_view::ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.expanded_ssh_nodes.insert(node_id.clone());
        // The folder chooser is a node/SFTP consumer like Tauri's IDE tree; it
        // connects through NodeRouter and must not create or require a terminal.
        self.ensure_node_connection_started(&node_id);
        cx.notify();
    }

    pub(super) fn ide_snapshot_for_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> Option<ReconnectIdeSnapshot> {
        let Some((tab_id, _)) = self
            .ide_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| existing_node_id.0 == node_id.0)
        else {
            return None;
        };
        let Some(surface) = self.ide_tab_surfaces.get(tab_id) else {
            return None;
        };
        surface.update(cx, |surface: &mut IdeSurface, cx| {
            surface.reconnect_snapshot(cx)
        })
    }

    pub(super) fn restore_ide_for_reconnect(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(job) = self.reconnect_orchestrator.job(&node_id.0) else {
            return false;
        };
        let Some(ide_snapshot) = job.snapshot.ide_snapshot else {
            return false;
        };
        if ide_restore_was_closed_after_snapshot(
            self.ide_last_closed_at_by_node.get(node_id).copied(),
            job.snapshot.snapshot_at,
        ) {
            return false;
        }
        // Tauri's reconnect phase restores the IDE after SFTP has been brought
        // back. Re-open through the same node-first IDE owner so the restored
        // surface consumes NodeRouter/SFTP directly rather than a terminal pane.
        self.open_ide_tab_with_reconnect_snapshot(node_id.clone(), ide_snapshot, cx)
    }

    fn open_ide_tab_with_reconnect_snapshot(
        &mut self,
        node_id: NodeId,
        ide_snapshot: ReconnectIdeSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("IDE · {node_title}");
        let tab_id = if let Some((tab_id, _)) = self
            .ide_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| existing_node_id.0 == node_id.0)
        {
            let Some(surface) = self.ide_tab_surfaces.get(tab_id) else {
                return false;
            };
            let restored = surface.update(cx, |surface: &mut IdeSurface, cx| {
                surface.restore_reconnect_snapshot(ide_snapshot, cx)
            });
            if !restored {
                return false;
            }
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id();
            let router = self.node_router.clone();
            let tokens = self.tokens;
            let labels = self.ide_labels();
            let runtime_settings = self.ide_runtime_settings();
            let backend_runtime = self.forwarding_runtime.clone();
            let surface = cx.new(|cx| {
                IdeSurface::new(
                    router,
                    tokens,
                    labels,
                    runtime_settings,
                    backend_runtime,
                    cx,
                )
            });
            let restored = surface.update(cx, |surface: &mut IdeSurface, cx| {
                surface.restore_reconnect_snapshot(ide_snapshot, cx)
            });
            if !restored {
                return false;
            }

            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Ide,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.subscribe_ide_surface(tab_id, &surface, cx);
            self.ide_tab_surfaces.insert(tab_id, surface);
            self.ide_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        self.active_tab_id = Some(tab_id);
        self.active_surface = oxideterm_gpui_settings_view::ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.expanded_ssh_nodes.insert(node_id);
        cx.notify();
        true
    }

    pub(super) fn render_ide_surface(&self, _cx: &mut Context<Self>) -> AnyElement {
        let Some(tab_id) = self.active_tab_id else {
            return div().into_any_element();
        };
        self.ide_tab_surfaces
            .get(&tab_id)
            .cloned()
            .map(IntoElement::into_any_element)
            .unwrap_or_else(|| div().into_any_element())
    }

    fn ide_labels(&self) -> IdeLabels {
        IdeLabels {
            open_folder: self.i18n.t("ide.open_folder"),
            select_folder: self.i18n.t("ide.select_folder"),
            select_folder_desc: self.i18n.t("ide.select_folder_desc"),
            go: self.i18n.t("ide.go"),
            go_to_parent: self.i18n.t("ide.go_to_parent"),
            no_subfolders: self.i18n.t("ide.no_subfolders"),
            selected_path: self.i18n.t("ide.selected_path"),
            loading_project: self.i18n.t("ide.loading_project"),
            open_failed: self.i18n.t("ide.open_failed"),
            retry: self.i18n.t("ide.retry"),
            disconnected_overlay: self.i18n.t("ide.disconnected_overlay"),
            no_project: self.i18n.t("ide.no_project"),
            no_open_files: self.i18n.t("ide.no_open_files"),
            click_to_open: self.i18n.t("ide.click_to_open"),
            loading_file: self.i18n.t("ide.loading_file"),
            save_failed: self.i18n.t("ide.save_failed"),
            unsaved_changes: self.i18n.t("ide.unsaved_changes"),
            unsaved_changes_folder: self.i18n.t("ide.unsaved_changes_folder"),
            unsaved_changes_desc: self.i18n.t("ide.unsaved_changes_desc"),
            save: self.i18n.t("ide.save"),
            discard: self.i18n.t("ide.discard"),
            cancel: self.i18n.t("ide.cancel"),
            pin_tab: self.i18n.t("ide.pin_tab"),
            unpin_tab: self.i18n.t("ide.unpin_tab"),
            close_tab: self.i18n.t("tabbar.close_tab"),
            context_new_file: self.i18n.t("ide.contextMenu.newFile"),
            context_new_folder: self.i18n.t("ide.contextMenu.newFolder"),
            context_rename: self.i18n.t("ide.contextMenu.rename"),
            context_delete: self.i18n.t("ide.contextMenu.delete"),
            context_copy_path: self.i18n.t("ide.contextMenu.copyPath"),
            context_open_in_terminal: self.i18n.t("ide.contextMenu.openInTerminal"),
            sftp_mode: self.i18n.t("ide.agent_status_sftp"),
            agent_ready: self.i18n.t("ide.agent_status_ready"),
            agent_deploying: self.i18n.t("ide.agent_status_deploying"),
            agent_checking: self.i18n.t("ide.agent_status_checking"),
            agent_manual_upload: self.i18n.t("ide.agent_status_manual_upload"),
            agent_manual_update: self.i18n.t("ide.agent_status_manual_update"),
            agent_optin_title: self.i18n.t("ide.agent_optin_title"),
            agent_optin_desc: self.i18n.t("ide.agent_optin_desc"),
            agent_optin_benefit_watch: self.i18n.t("ide.agent_optin_benefit_watch"),
            agent_optin_benefit_git: self.i18n.t("ide.agent_optin_benefit_git"),
            agent_optin_benefit_atomic: self.i18n.t("ide.agent_optin_benefit_atomic"),
            agent_optin_remember: self.i18n.t("ide.agent_optin_remember"),
            agent_optin_sftp_only: self.i18n.t("ide.agent_optin_sftp_only"),
            agent_optin_enable: self.i18n.t("ide.agent_optin_enable"),
            agent_remove_btn: self.i18n.t("ide.agent_remove_btn"),
            agent_deploy_btn: self.i18n.t("ide.agent_deploy_btn"),
            agent_remove_confirm_title: self.i18n.t("ide.agent_remove_confirm_title"),
            agent_remove_confirm_desc: self.i18n.t("ide.agent_remove_confirm_desc"),
            agent_remove_confirm_btn: self.i18n.t("ide.agent_remove_confirm_btn"),
            agent_manual_upload_hint: self.i18n.t("ide.agent_manual_upload_hint"),
            agent_manual_update_hint: self.i18n.t("ide.agent_manual_update_hint"),
            agent_download_link: self.i18n.t("ide.agent_download_link"),
            agent_upload_to: self.i18n.t("ide.agent_upload_to"),
            agent_manual_upload_arch: self.i18n.t("ide.agent_manual_upload_arch"),
            agent_manual_update_current_agent_version: self
                .i18n
                .t("ide.agent_manual_update_current_agent_version"),
            agent_manual_update_current_compatibility_version: self
                .i18n
                .t("ide.agent_manual_update_current_compatibility_version"),
            agent_manual_update_expected_compatibility_version: self
                .i18n
                .t("ide.agent_manual_update_expected_compatibility_version"),
            agent_retry_btn: self.i18n.t("ide.agent_retry_btn"),
        }
    }

    pub(super) fn ide_runtime_settings(&self) -> IdeRuntimeSettings {
        let settings = self.settings_store.settings();
        IdeRuntimeSettings {
            auto_save: settings.ide.auto_save,
            editor_font_size: settings
                .ide
                .font_size
                .unwrap_or(settings.terminal.font_size)
                .clamp(8, 32) as f32,
            editor_line_height: settings
                .ide
                .line_height
                .unwrap_or(settings.terminal.line_height)
                .clamp(0.8, 3.0) as f32,
            word_wrap: settings.ide.word_wrap,
            background_active: self.terminal_background_preferences("ide").is_some(),
            agent_mode: match settings.ide.agent_mode {
                IdeAgentMode::Ask => NodeAgentMode::Ask,
                IdeAgentMode::Enabled => NodeAgentMode::Enabled,
                IdeAgentMode::Disabled => NodeAgentMode::Disabled,
            },
        }
    }

    pub(super) fn apply_ide_runtime_settings_to_surfaces(&mut self, cx: &mut Context<Self>) {
        let tokens = self.tokens;
        let runtime_settings = self.ide_runtime_settings();
        for surface in self.ide_tab_surfaces.values() {
            surface.update(cx, |surface, cx| {
                surface.set_visual_and_runtime_settings(tokens, runtime_settings, cx);
            });
        }
    }

    fn subscribe_ide_surface(
        &mut self,
        tab_id: oxideterm_workspace::TabId,
        surface: &gpui::Entity<IdeSurface>,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe(
            surface,
            move |this, _surface, event: &IdeSurfaceEvent, cx| match event {
                IdeSurfaceEvent::RememberAgentMode(mode) => {
                    this.remember_ide_agent_mode(*mode, cx);
                }
                IdeSurfaceEvent::ProjectOpened => {
                    if let Some(node_id) = this.ide_tab_nodes.get(&tab_id).cloned() {
                        // Tauri ideStore.openProject clears lastClosedAt.
                        // Native records it per node because it can keep
                        // independent IDE surfaces for different nodes.
                        this.ide_last_closed_at_by_node.remove(&node_id);
                    }
                }
            },
        );
        self.ide_surface_subscriptions.insert(tab_id, subscription);
    }

    fn remember_ide_agent_mode(&mut self, mode: NodeAgentMode, cx: &mut Context<Self>) {
        self.settings_store.settings_mut().ide.agent_mode = match mode {
            NodeAgentMode::Ask => IdeAgentMode::Ask,
            NodeAgentMode::Enabled => IdeAgentMode::Enabled,
            NodeAgentMode::Disabled => IdeAgentMode::Disabled,
        };
        let _ = self.settings_store.save();
        self.apply_ide_runtime_settings_to_surfaces(cx);
        cx.notify();
    }
}

fn ide_restore_was_closed_after_snapshot(
    closed_at: Option<SystemTime>,
    snapshot_at: Option<SystemTime>,
) -> bool {
    matches!((closed_at, snapshot_at), (Some(closed_at), Some(snapshot_at)) if closed_at > snapshot_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ide_restore_skips_when_close_happened_after_snapshot() {
        let snapshot_at = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let closed_at = snapshot_at + Duration::from_secs(1);

        assert!(ide_restore_was_closed_after_snapshot(
            Some(closed_at),
            Some(snapshot_at)
        ));
    }

    #[test]
    fn ide_restore_allows_close_before_snapshot_or_missing_timestamp() {
        let snapshot_at = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let closed_at = snapshot_at - Duration::from_secs(1);

        assert!(!ide_restore_was_closed_after_snapshot(
            Some(closed_at),
            Some(snapshot_at)
        ));
        assert!(!ide_restore_was_closed_after_snapshot(
            None,
            Some(snapshot_at)
        ));
        assert!(!ide_restore_was_closed_after_snapshot(
            Some(closed_at),
            None
        ));
    }
}
