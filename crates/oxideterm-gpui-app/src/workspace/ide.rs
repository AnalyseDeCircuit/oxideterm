use gpui::{AnyElement, AppContext, Context, IntoElement, div};
use oxideterm_gpui_ide::{IdeLabels, IdeSurface};
use oxideterm_ssh::NodeId;
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};

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
            let backend_runtime = self.forwarding_runtime.clone();
            let surface = cx.new(|cx| IdeSurface::new(router, tokens, labels, backend_runtime, cx));
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

    pub(super) fn open_ide_tab_with_files(
        &mut self,
        node_id: NodeId,
        root_path: impl Into<String>,
        file_paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let root_path = root_path.into();
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
                    surface.open_remote_project_with_files(
                        node_id.0.clone(),
                        root_path.clone(),
                        file_paths.clone(),
                        cx,
                    );
                });
            }
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id();
            let router = self.node_router.clone();
            let tokens = self.tokens;
            let labels = self.ide_labels();
            let backend_runtime = self.forwarding_runtime.clone();
            let surface = cx.new(|cx| IdeSurface::new(router, tokens, labels, backend_runtime, cx));
            surface.update(cx, |surface: &mut IdeSurface, cx| {
                surface.open_remote_project_with_files(
                    node_id.0.clone(),
                    root_path.clone(),
                    file_paths.clone(),
                    cx,
                );
            });

            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Ide,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.ide_tab_surfaces.insert(tab_id, surface);
            self.ide_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        self.active_tab_id = Some(tab_id);
        self.active_surface = oxideterm_gpui_settings_view::ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.expanded_ssh_nodes.insert(node_id.clone());
        // IDE, SFTP, and forwarding are node consumers. Starting the node here
        // matches Tauri's connect_tree_node path without creating a terminal.
        self.ensure_node_connection_started(&node_id);
        cx.notify();
    }

    pub(super) fn ide_snapshot_for_node(
        &self,
        node_id: &NodeId,
        cx: &gpui::App,
    ) -> (Option<String>, Vec<String>) {
        let Some((tab_id, _)) = self
            .ide_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| existing_node_id.0 == node_id.0)
        else {
            return (None, Vec::new());
        };
        let Some(surface) = self.ide_tab_surfaces.get(tab_id) else {
            return (None, Vec::new());
        };
        let surface = surface.read(cx);
        (surface.project_root_path(), surface.open_file_paths())
    }

    pub(super) fn restore_ide_for_reconnect(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(job) = self.reconnect_orchestrator.job(&node_id.0) else {
            return false;
        };
        let Some(project_path) = job.snapshot.ide_project_path else {
            return false;
        };
        // Tauri's reconnect phase restores the IDE after SFTP has been brought
        // back. Re-open through the same node-first IDE owner so the restored
        // surface consumes NodeRouter/SFTP directly rather than a terminal pane.
        self.open_ide_tab_with_files(
            node_id.clone(),
            project_path,
            job.snapshot.open_ide_file_paths,
            cx,
        );
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
            sftp_mode: self.i18n.t("ide.agent_status_sftp"),
        }
    }
}
