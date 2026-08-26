use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, IntoElement, Subscription, div,
};
use oxideterm_gpui_ide::{
    IdeAiContextSnapshot, IdeLabels, IdePluginSnapshot, IdeRuntimeSettings, IdeSurface,
    IdeSurfaceEvent, IdeSurfaceMount, NodeAgentMode,
};
use oxideterm_ide_fs::NodeAgentIdeFileSystem;
use oxideterm_settings::{IdeAgentMode, PersistedSettings};
use oxideterm_ssh::{NodeId, PhaseResult, ReconnectIdeSnapshot};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::SystemTime,
};

use super::{TabId, WorkspaceApp, tabs::TabRemovalTransition};

/// Cross-workspace IDE effects that require settings, tabs, or reconnect coordination.
pub(super) enum IdeWorkspaceEvent {
    RememberAgentMode(NodeAgentMode),
    SurfaceOpened {
        tab_id: TabId,
        node_id: NodeId,
    },
    SurfaceClosed {
        tab_id: TabId,
    },
    TransientSurfaceClosed {
        tab_id: TabId,
    },
    ReconnectRestoreCompleted {
        reconnect_node_id: NodeId,
        result: PhaseResult,
        message: String,
    },
}

impl EventEmitter<IdeWorkspaceEvent> for IdeWorkspaceEntity {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IdeSurfaceCloseReason {
    UserProjectClose,
    TransientFolderPickerCancel,
}

struct IdeSurfaceEntry {
    node_id: NodeId,
    surface: Entity<IdeSurface>,
    _subscription: Subscription,
}

pub(super) struct IdeWorkspaceTargetSnapshot {
    pub(super) tab_id: TabId,
    pub(super) node_id: NodeId,
    pub(super) project_root_path: Option<String>,
    pub(super) project_name: Option<String>,
    pub(super) active_editor_tab_id: Option<String>,
}

enum ExistingReconnectRestore {
    Suppressed,
    Missing(ReconnectIdeSnapshot),
    Restored {
        tab_id: TabId,
        same_project_was_open: bool,
    },
    Failed,
}

/// Owns the IDE surface registry, node index, subscriptions, and reconnect-close history.
///
/// The registry deliberately does not own tabs, settings persistence, windows, or SSH transport.
/// Each `IdeSurface` retains its own node consumer and mount lifecycle, so removing one entry only
/// releases that surface owner and cannot disconnect a shared node or another IDE consumer.
pub(super) struct IdeWorkspaceEntity {
    fs: NodeAgentIdeFileSystem,
    backend_runtime: Arc<tokio::runtime::Runtime>,
    surfaces_by_tab: BTreeMap<u64, IdeSurfaceEntry>,
    tabs_by_node: HashMap<NodeId, BTreeSet<u64>>,
    last_closed_at_by_node: HashMap<NodeId, SystemTime>,
}

impl IdeWorkspaceEntity {
    pub(super) fn new(
        fs: NodeAgentIdeFileSystem,
        backend_runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Self {
            fs,
            backend_runtime,
            surfaces_by_tab: BTreeMap::new(),
            tabs_by_node: HashMap::new(),
            last_closed_at_by_node: HashMap::new(),
        }
    }

    pub(super) fn create_folder_picker_surface(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        initial_path: String,
        tokens: oxideterm_theme::ThemeTokens,
        labels: IdeLabels,
        runtime_settings: IdeRuntimeSettings,
        cx: &mut Context<Self>,
    ) {
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let surface =
            cx.new(|cx| IdeSurface::new(fs, tokens, labels, runtime_settings, backend_runtime, cx));
        surface.update(cx, |surface, cx| {
            surface.open_remote_folder_picker_for_node(node_id.0.clone(), initial_path, cx);
        });
        self.register_surface(tab_id, node_id, surface, cx);
    }

    pub(super) fn create_reconnect_surface(
        &mut self,
        tab_id: TabId,
        reconnect_node_id: &NodeId,
        target_node_id: NodeId,
        ide_snapshot: ReconnectIdeSnapshot,
        tokens: oxideterm_theme::ThemeTokens,
        labels: IdeLabels,
        runtime_settings: IdeRuntimeSettings,
        cx: &mut Context<Self>,
    ) -> bool {
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let surface =
            cx.new(|cx| IdeSurface::new(fs, tokens, labels, runtime_settings, backend_runtime, cx));
        let restored = surface.update(cx, |surface, cx| {
            surface.restore_reconnect_snapshot(ide_snapshot, reconnect_node_id.0.clone(), cx)
        });
        if !restored {
            return false;
        }
        self.register_surface(tab_id, target_node_id, surface, cx);
        true
    }

    pub(super) fn register_surface(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        surface: Entity<IdeSurface>,
        cx: &mut Context<Self>,
    ) {
        if self.surfaces_by_tab.contains_key(&tab_id.0) {
            self.close_surface_at(
                tab_id,
                IdeSurfaceCloseReason::TransientFolderPickerCancel,
                SystemTime::now(),
                cx,
            );
        }

        let subscription = cx.subscribe(
            &surface,
            move |workspace, _surface, event: &IdeSurfaceEvent, cx| match event {
                IdeSurfaceEvent::RememberAgentMode(mode) => {
                    // The registry propagates the mode before notifying the root persistence
                    // adapter, avoiding a re-entrant update back into this Entity.
                    workspace.apply_agent_mode_to_surfaces(*mode, cx);
                    cx.emit(IdeWorkspaceEvent::RememberAgentMode(*mode));
                }
                IdeSurfaceEvent::ProjectOpened => {
                    if let Some(node_id) = workspace.node_for_tab(tab_id).cloned() {
                        // A successful project open supersedes an earlier explicit-close marker.
                        workspace.last_closed_at_by_node.remove(&node_id);
                    }
                }
                IdeSurfaceEvent::TransientFolderPickerCancelled => {
                    // Removing the subscription during its own callback is unsafe. A one-turn
                    // registry-owned task performs the close without capturing WorkspaceApp.
                    cx.spawn(async move |weak_registry, cx| {
                        let _ = weak_registry.update(cx, |workspace, cx| {
                            if workspace.close_surface(
                                tab_id,
                                IdeSurfaceCloseReason::TransientFolderPickerCancel,
                                cx,
                            ) {
                                cx.emit(IdeWorkspaceEvent::TransientSurfaceClosed { tab_id });
                            }
                        });
                    })
                    .detach();
                }
                IdeSurfaceEvent::ReconnectRestoreProjectOpened { reconnect_node_id } => {
                    cx.emit(IdeWorkspaceEvent::ReconnectRestoreCompleted {
                        reconnect_node_id: NodeId::new(reconnect_node_id.clone()),
                        result: PhaseResult::Ok,
                        message: "restored IDE project and open files".to_string(),
                    });
                }
                IdeSurfaceEvent::ReconnectRestoreProjectFailed {
                    reconnect_node_id,
                    message,
                } => {
                    cx.emit(IdeWorkspaceEvent::ReconnectRestoreCompleted {
                        reconnect_node_id: NodeId::new(reconnect_node_id.clone()),
                        result: PhaseResult::Failed,
                        message: message.clone(),
                    });
                }
            },
        );

        let event_node_id = node_id.clone();
        self.tabs_by_node
            .entry(node_id.clone())
            .or_default()
            .insert(tab_id.0);
        self.surfaces_by_tab.insert(
            tab_id.0,
            IdeSurfaceEntry {
                node_id,
                surface,
                _subscription: subscription,
            },
        );
        // The app-level subscriber registers capability authority only after
        // this surface has become the IDE workspace's real owner.
        cx.emit(IdeWorkspaceEvent::SurfaceOpened {
            tab_id,
            node_id: event_node_id,
        });
    }

    pub(super) fn close_surface(
        &mut self,
        tab_id: TabId,
        reason: IdeSurfaceCloseReason,
        cx: &mut Context<Self>,
    ) -> bool {
        self.close_surface_at(tab_id, reason, SystemTime::now(), cx)
    }

    fn close_surface_at(
        &mut self,
        tab_id: TabId,
        reason: IdeSurfaceCloseReason,
        closed_at: SystemTime,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(entry) = self.surfaces_by_tab.remove(&tab_id.0) else {
            return false;
        };
        if let Some(tab_ids) = self.tabs_by_node.get_mut(&entry.node_id) {
            tab_ids.remove(&tab_id.0);
            if tab_ids.is_empty() {
                self.tabs_by_node.remove(&entry.node_id);
            }
        }
        if reason == IdeSurfaceCloseReason::UserProjectClose
            && !self.tabs_by_node.contains_key(&entry.node_id)
        {
            // A close marker represents the node's final IDE project surface.
            // Closing one same-node surface must not suppress the surviving owner.
            self.last_closed_at_by_node
                .insert(entry.node_id.clone(), closed_at);
        }
        entry.surface.update(cx, |surface, cx| {
            // This releases only the surface-scoped IDE consumer and watch owner.
            surface.release_remote_session(cx);
        });
        cx.emit(IdeWorkspaceEvent::SurfaceClosed { tab_id });
        true
    }

    pub(super) fn surface(&self, tab_id: TabId) -> Option<Entity<IdeSurface>> {
        self.surfaces_by_tab
            .get(&tab_id.0)
            .map(|entry| entry.surface.clone())
    }

    /// Returns the existing IDE surface's filesystem owner for a capability
    /// already validated by the workspace runtime broker.
    pub(super) fn ai_owner_file_system(
        &self,
        tab_id: TabId,
        cx: &App,
    ) -> Option<NodeAgentIdeFileSystem> {
        self.surfaces_by_tab
            .get(&tab_id.0)
            .map(|entry| entry.surface.read(cx).ai_owner_file_system())
    }

    pub(super) fn node_for_tab(&self, tab_id: TabId) -> Option<&NodeId> {
        self.surfaces_by_tab
            .get(&tab_id.0)
            .map(|entry| &entry.node_id)
    }

    pub(super) fn tab_for_node(&self, node_id: &NodeId) -> Option<TabId> {
        self.tabs_by_node
            .get(node_id)
            .and_then(|tab_ids| tab_ids.first().copied())
            .map(TabId)
    }

    pub(super) fn surface_for_node(&self, node_id: &NodeId) -> Option<Entity<IdeSurface>> {
        self.tab_for_node(node_id)
            .and_then(|tab_id| self.surface(tab_id))
    }

    pub(super) fn set_mount(
        &mut self,
        tab_id: TabId,
        mount: IdeSurfaceMount,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.surfaces_by_tab.get(&tab_id.0) {
            entry
                .surface
                .update(cx, |surface, cx| surface.set_mount(mount, cx));
        }
    }

    pub(super) fn apply_runtime_settings(
        &mut self,
        tokens: oxideterm_theme::ThemeTokens,
        runtime_settings: IdeRuntimeSettings,
        cx: &mut Context<Self>,
    ) {
        for entry in self.surfaces_by_tab.values() {
            entry.surface.update(cx, |surface, cx| {
                surface.set_visual_and_runtime_settings(tokens, runtime_settings.clone(), cx);
            });
        }
    }

    fn apply_agent_mode_to_surfaces(&mut self, mode: NodeAgentMode, cx: &mut Context<Self>) {
        for entry in self.surfaces_by_tab.values() {
            entry
                .surface
                .update(cx, |surface, cx| surface.set_agent_mode(mode, cx));
        }
    }

    pub(super) fn reconnect_snapshot_for_nodes(
        &mut self,
        node_ids: &[NodeId],
        cx: &mut Context<Self>,
    ) -> Option<ReconnectIdeSnapshot> {
        self.surfaces_by_tab
            .values()
            .filter(|entry| node_ids.contains(&entry.node_id))
            .find_map(|entry| {
                entry
                    .surface
                    .update(cx, |surface, cx| surface.reconnect_snapshot(cx))
            })
    }

    fn restore_existing_for_reconnect(
        &mut self,
        reconnect_node_id: &NodeId,
        target_node_id: &NodeId,
        ide_snapshot: ReconnectIdeSnapshot,
        snapshot_at: Option<SystemTime>,
        cx: &mut Context<Self>,
    ) -> ExistingReconnectRestore {
        if ide_restore_was_closed_after_snapshot(
            self.last_closed_at_by_node.get(target_node_id).copied(),
            snapshot_at,
        ) {
            return ExistingReconnectRestore::Suppressed;
        }
        let Some(surface) = self.surface_for_node(target_node_id) else {
            return ExistingReconnectRestore::Missing(ide_snapshot);
        };
        let tab_id = self
            .tab_for_node(target_node_id)
            .expect("surface lookup and node index must stay in sync");
        let same_project_was_open = surface.update(cx, |surface, _cx| {
            surface.project_root_path().as_deref() == Some(ide_snapshot.project_path.as_str())
        });
        let restored = surface.update(cx, |surface, cx| {
            surface.restore_reconnect_snapshot(ide_snapshot, reconnect_node_id.0.clone(), cx)
        });
        if restored {
            ExistingReconnectRestore::Restored {
                tab_id,
                same_project_was_open,
            }
        } else {
            ExistingReconnectRestore::Failed
        }
    }

    pub(super) fn mark_connection_interrupted(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        let tab_ids = self
            .tabs_by_node
            .get(node_id)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        for tab_id in tab_ids {
            if let Some(entry) = self.surfaces_by_tab.get(&tab_id) {
                entry.surface.update(cx, |surface, cx| {
                    surface.mark_connection_interrupted(cx);
                });
            }
        }
    }

    pub(super) fn ai_context_snapshot(
        &self,
        active_tab_id: Option<TabId>,
        cx: &App,
    ) -> Option<IdeAiContextSnapshot> {
        if let Some(active_tab_id) = active_tab_id
            && let Some(snapshot) = self
                .surfaces_by_tab
                .get(&active_tab_id.0)
                .and_then(|entry| entry.surface.read(cx).ai_context_snapshot())
        {
            return Some(snapshot);
        }
        self.surfaces_by_tab
            .iter()
            .filter(|(tab_id, _)| Some(TabId(**tab_id)) != active_tab_id)
            .find_map(|(_, entry)| entry.surface.read(cx).ai_context_snapshot())
    }

    pub(super) fn plugin_snapshot(
        &self,
        active_tab_id: Option<TabId>,
        cx: &App,
    ) -> Option<IdePluginSnapshot> {
        if let Some(active_tab_id) = active_tab_id
            && let Some(snapshot) = self
                .surfaces_by_tab
                .get(&active_tab_id.0)
                .and_then(|entry| entry.surface.read(cx).plugin_snapshot())
        {
            return Some(snapshot);
        }
        self.surfaces_by_tab
            .iter()
            .filter(|(tab_id, _)| Some(TabId(**tab_id)) != active_tab_id)
            .find_map(|(_, entry)| entry.surface.read(cx).plugin_snapshot())
    }

    pub(super) fn surface_for_effect(
        &self,
        active_tab_id: Option<TabId>,
        requested_node_id: Option<&str>,
    ) -> Option<Entity<IdeSurface>> {
        if let Some(requested_node_id) = requested_node_id {
            let requested_node_id = NodeId::new(requested_node_id);
            if let Some(surface) = self.surface_for_node(&requested_node_id) {
                return Some(surface);
            }
        }
        active_tab_id
            .and_then(|tab_id| self.surface(tab_id))
            .or_else(|| {
                self.surfaces_by_tab
                    .first_key_value()
                    .map(|(_, entry)| entry.surface.clone())
            })
    }

    pub(super) fn target_snapshots(&self, cx: &App) -> Vec<IdeWorkspaceTargetSnapshot> {
        self.surfaces_by_tab
            .iter()
            .map(|(tab_id, entry)| {
                let surface = entry.surface.read(cx);
                let context = surface.ai_context_snapshot();
                IdeWorkspaceTargetSnapshot {
                    tab_id: TabId(*tab_id),
                    node_id: entry.node_id.clone(),
                    project_root_path: surface.project_root_path(),
                    project_name: context.map(|snapshot| snapshot.project_name),
                    active_editor_tab_id: surface.active_editor_tab_id(),
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IdeReconnectRestoreStatus {
    Skipped,
    Restored,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IdeOpenIntent {
    ActivateOrCreate,
    ChooseFolder,
}

impl IdeOpenIntent {
    fn reopens_folder_picker(self) -> bool {
        self == Self::ChooseFolder
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn sync_ide_surface_mount(&mut self, tab_id: TabId, cx: &mut App) {
        let (outside_main_window, detached_window_open) = {
            let tab_host = self.tab_host.read(cx);
            (
                tab_host.is_outside_main_window(tab_id),
                tab_host.is_detached(tab_id),
            )
        };
        let mount = ide_surface_mount_for_location(
            self.active_tab_id(cx) == Some(tab_id),
            outside_main_window,
            detached_window_open,
        );
        // WorkspaceApp reports only window placement; the IDE owner performs
        // the sampling, watcher, and node-session mount transition.
        self.ide_workspace
            .update(cx, |workspace, cx| workspace.set_mount(tab_id, mount, cx));
    }

    pub(super) fn open_ide_folder_picker_tab(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        let active_terminal_cwd = self.active_ssh_terminal_cwd_path_for_node(&node_id, cx);
        self.open_ide_folder_picker_tab_with_initial_path(
            node_id,
            active_terminal_cwd,
            IdeOpenIntent::ActivateOrCreate,
            cx,
        );
    }

    pub(in crate::workspace) fn open_ide_folder_picker_tab_at_path(
        &mut self,
        node_id: NodeId,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let initial_path = if path.trim().is_empty() {
            None
        } else {
            Some(path)
        };
        self.open_ide_folder_picker_tab_with_initial_path(
            node_id,
            initial_path,
            IdeOpenIntent::ChooseFolder,
            cx,
        );
    }

    fn open_ide_folder_picker_tab_with_initial_path(
        &mut self,
        node_id: NodeId,
        initial_path_override: Option<String>,
        intent: IdeOpenIntent,
        cx: &mut Context<Self>,
    ) {
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("IDE · {node_title}");
        let existing_tab_id = self.ide_workspace.read(cx).tab_for_node(&node_id);
        let tab_id = if let Some(tab_id) = existing_tab_id {
            if intent.reopens_folder_picker()
                && let Some(surface) = self.ide_workspace.read(cx).surface(tab_id)
            {
                // Explicit folder-selection actions may replace the workspace,
                // while the node sidebar entry only activates its existing tab.
                surface.update(cx, |surface: &mut IdeSurface, cx| {
                    let initial_path = initial_path_override.clone().unwrap_or_else(|| {
                        surface
                            .project_root_path()
                            .unwrap_or_else(|| "/".to_string())
                    });
                    surface.open_remote_folder_picker_for_node(node_id.0.clone(), initial_path, cx);
                });
            }
            tab_id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            let tokens = self.tokens;
            let labels = self.ide_labels();
            let runtime_settings = self.ide_runtime_settings();
            let initial_path = initial_path_override.unwrap_or_else(|| "/".to_string());

            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Ide,
                    title,
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            self.ide_workspace.update(cx, |workspace, cx| {
                workspace.create_folder_picker_surface(
                    tab_id,
                    node_id.clone(),
                    initial_path,
                    tokens,
                    labels,
                    runtime_settings,
                    cx,
                );
            });
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        if !self.tab_host.read(cx).is_outside_main_window(tab_id) {
            self.set_main_window_active_tab(Some(tab_id), cx);
            self.active_surface = oxideterm_gpui_settings_view::ActiveSurface::Terminal;
        }
        self.active_ssh_node_id = Some(node_id.clone());
        self.expanded_ssh_nodes.insert(node_id.clone());
        // The folder chooser is a node/SFTP consumer like Tauri's IDE tree.
        // Opening it must not create a terminal or implicitly start SSH.
        cx.notify();
    }

    pub(super) fn ide_snapshot_for_nodes(
        &mut self,
        node_ids: &[NodeId],
        cx: &mut Context<Self>,
    ) -> Option<ReconnectIdeSnapshot> {
        self.ide_workspace.update(cx, |workspace, cx| {
            workspace.reconnect_snapshot_for_nodes(node_ids, cx)
        })
    }

    pub(super) fn restore_ide_for_reconnect(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> IdeReconnectRestoreStatus {
        let Some((ide_snapshot, snapshot_at)) = self
            .workspace_runtime
            .read(cx)
            .reconnect_ide_snapshot(node_id)
        else {
            return IdeReconnectRestoreStatus::Skipped;
        };
        let target_node_id = NodeId::new(ide_snapshot.connection_id.clone());
        if !self.ssh_nodes.contains_key(&target_node_id) {
            return IdeReconnectRestoreStatus::Skipped;
        }
        // Tauri's reconnect phase restores the IDE after SFTP has been brought
        // back. Re-open through the same node-first IDE owner so the restored
        // surface consumes NodeRouter/SFTP directly rather than a terminal pane.
        self.open_ide_tab_with_reconnect_snapshot(
            node_id.clone(),
            target_node_id,
            ide_snapshot,
            snapshot_at,
            cx,
        )
    }

    fn open_ide_tab_with_reconnect_snapshot(
        &mut self,
        reconnect_node_id: NodeId,
        target_node_id: NodeId,
        ide_snapshot: ReconnectIdeSnapshot,
        snapshot_at: Option<SystemTime>,
        cx: &mut Context<Self>,
    ) -> IdeReconnectRestoreStatus {
        let node_title = self
            .ssh_nodes
            .get(&target_node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| target_node_id.0.clone());
        let title = format!("IDE · {node_title}");
        let restore = self.ide_workspace.update(cx, |workspace, cx| {
            workspace.restore_existing_for_reconnect(
                &reconnect_node_id,
                &target_node_id,
                ide_snapshot,
                snapshot_at,
                cx,
            )
        });
        let (tab_id, same_project_open) = match restore {
            ExistingReconnectRestore::Suppressed | ExistingReconnectRestore::Failed => {
                return IdeReconnectRestoreStatus::Skipped;
            }
            ExistingReconnectRestore::Restored {
                tab_id,
                same_project_was_open,
            } => (tab_id, same_project_was_open),
            ExistingReconnectRestore::Missing(ide_snapshot) => {
                let tab_id = self.alloc_tab_id(cx);
                let tokens = self.tokens;
                let labels = self.ide_labels();
                let runtime_settings = self.ide_runtime_settings();
                let restored = self.ide_workspace.update(cx, |workspace, cx| {
                    workspace.create_reconnect_surface(
                        tab_id,
                        &reconnect_node_id,
                        target_node_id.clone(),
                        ide_snapshot,
                        tokens,
                        labels,
                        runtime_settings,
                        cx,
                    )
                });
                if !restored {
                    return IdeReconnectRestoreStatus::Skipped;
                }

                self.insert_tab(
                    Tab {
                        id: tab_id,
                        kind: TabKind::Ide,
                        title,
                        title_source: TabTitleSource::Static,
                        root_pane: None,
                        active_pane_id: None,
                    },
                    cx,
                );
                (tab_id, false)
            }
        };

        if !self.tab_host.read(cx).is_outside_main_window(tab_id) {
            self.set_main_window_active_tab(Some(tab_id), cx);
            self.active_surface = oxideterm_gpui_settings_view::ActiveSurface::Terminal;
        }
        self.active_ssh_node_id = Some(target_node_id.clone());
        self.expanded_ssh_nodes.insert(target_node_id.clone());
        // Existing IDE surfaces do not emit SurfaceOpened during reconnect.
        // Re-register only after the real surface has restored its new node owner.
        self.register_ai_runtime_ide_surface_owner(tab_id, &target_node_id, cx);
        cx.notify();
        if same_project_open {
            IdeReconnectRestoreStatus::Restored
        } else {
            IdeReconnectRestoreStatus::Pending
        }
    }

    pub(super) fn mark_ide_interrupted_for_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        self.ide_workspace.update(cx, |workspace, cx| {
            workspace.mark_connection_interrupted(node_id, cx);
        });
    }

    pub(super) fn release_ide_runtime_for_saved_connection(
        &mut self,
        saved_connection_id: &str,
        cx: &mut Context<Self>,
    ) {
        let affected_nodes = self
            .ssh_nodes
            .iter()
            .filter_map(|(node_id, node)| {
                (node.saved_connection_id.as_deref() == Some(saved_connection_id))
                    .then_some(node_id.clone())
            })
            .collect::<Vec<_>>();

        // Tauri removeNode closes node-scoped IDE tabs, while delete_connection
        // removes persisted owner data. Native can still have open GPUI IDE
        // surfaces for the saved node, so at minimum invalidate their remote
        // runtime and release NodeRouter consumers before the owner disappears.
        for node_id in &affected_nodes {
            self.mark_ide_interrupted_for_node(node_id, cx);
        }
        self.saved_ssh_nodes.remove(saved_connection_id);
    }

    pub(super) fn render_ide_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(tab_id) = self.active_tab_id(cx) else {
            return div().into_any_element();
        };
        self.render_ide_surface_for_tab(tab_id, cx)
    }

    pub(super) fn render_ide_surface_for_tab(
        &self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ide_workspace
            .read(cx)
            .surface(tab_id)
            .map(IntoElement::into_any_element)
            .unwrap_or_else(|| div().into_any_element())
    }

    pub(super) fn copy_active_ide_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(surface) = self.active_ide_surface(cx) else {
            return false;
        };
        surface.update(cx, |surface, cx| surface.copy_active_editor_selection(cx))
    }

    pub(super) fn cut_active_ide_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(surface) = self.active_ide_surface(cx) else {
            return false;
        };
        surface.update(cx, |surface, cx| surface.cut_active_editor_selection(cx))
    }

    pub(super) fn paste_into_active_ide_editor(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(surface) = self.active_ide_surface(cx) else {
            return false;
        };
        surface.update(cx, |surface, cx| surface.paste_into_active_editor(cx))
    }

    pub(super) fn open_active_ide_search(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(surface) = self.active_ide_surface(cx) else {
            return false;
        };
        surface.update(cx, |surface, cx| surface.open_active_editor_search(cx))
    }

    pub(super) fn select_next_active_ide_search_match(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(surface) = self.active_ide_surface(cx) else {
            return false;
        };
        surface.update(cx, |surface, cx| {
            surface.select_next_active_editor_search_match(cx)
        })
    }

    pub(super) fn select_previous_active_ide_search_match(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(surface) = self.active_ide_surface(cx) else {
            return false;
        };
        surface.update(cx, |surface, cx| {
            surface.select_previous_active_editor_search_match(cx)
        })
    }

    pub(in crate::workspace) fn active_ide_surface(
        &self,
        cx: &App,
    ) -> Option<gpui::Entity<IdeSurface>> {
        let tab_id = self.active_tab_id(cx)?;
        let tab = self.tabs(cx).iter().find(|tab| tab.id == tab_id)?;
        (tab.kind == TabKind::Ide)
            .then(|| self.ide_workspace.read(cx).surface(tab_id))
            .flatten()
    }

    fn ide_labels(&self) -> IdeLabels {
        IdeLabels {
            open_folder: self.i18n.t("ide.open_folder"),
            search: self.i18n.t("ide.search"),
            refresh: self.i18n.t("ide.refresh"),
            search_placeholder: self.i18n.t("ide.search_placeholder"),
            search_hint: self.i18n.t("ide.search_hint"),
            searching: self.i18n.t("ide.searching"),
            no_results: self.i18n.t("ide.no_results"),
            search_truncated: self.i18n.t("ide.search_truncated"),
            search_results_count: self.i18n.t("ide.search_results_count"),
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
            conflict_title: self.i18n.t("ide.conflict_title"),
            conflict_desc: self.i18n.t("ide.conflict_desc"),
            your_version: self.i18n.t("ide.your_version"),
            remote_version: self.i18n.t("ide.remote_version"),
            reload_remote: self.i18n.t("ide.reload_remote"),
            overwrite: self.i18n.t("ide.overwrite"),
            unsaved_changes: self.i18n.t("ide.unsaved_changes"),
            unsaved_changes_folder: self.i18n.t("ide.unsaved_changes_folder"),
            unsaved_changes_desc: self.i18n.t("ide.unsaved_changes_desc"),
            save: self.i18n.t("ide.save"),
            discard: self.i18n.t("ide.discard"),
            cancel: self.i18n.t("ide.cancel"),
            find_placeholder: self.i18n.t("ide.find_placeholder"),
            replace_placeholder: self.i18n.t("ide.replace_placeholder"),
            find_next: self.i18n.t("ide.find_next"),
            find_previous: self.i18n.t("ide.find_previous"),
            match_case: self.i18n.t("ide.match_case"),
            toggle_replace: self.i18n.t("ide.toggle_replace"),
            close_search: self.i18n.t("ide.close_search"),
            replace_btn: self.i18n.t("ide.replace_btn"),
            replace_all_btn: self.i18n.t("ide.replace_all_btn"),
            replace: self.i18n.t("ide.replace"),
            replace_all: self.i18n.t("ide.replace_all"),
            editor_copy: self.i18n.t("menu.copy"),
            editor_cut: self.i18n.t("fileManager.cut"),
            editor_paste: self.i18n.t("menu.paste"),
            editor_select_all: self.i18n.t("fileManager.selectAll"),
            pin_tab: self.i18n.t("ide.pin_tab"),
            unpin_tab: self.i18n.t("ide.unpin_tab"),
            close_tab: self.i18n.t("tabbar.close_tab"),
            context_new_file: self.i18n.t("ide.contextMenu.newFile"),
            context_new_folder: self.i18n.t("ide.contextMenu.newFolder"),
            context_rename: self.i18n.t("ide.contextMenu.rename"),
            context_delete: self.i18n.t("ide.contextMenu.delete"),
            context_copy: self.i18n.t("menu.copy"),
            context_cut: self.i18n.t("fileManager.cut"),
            context_paste: self.i18n.t("menu.paste"),
            context_copy_path: self.i18n.t("ide.contextMenu.copyPath"),
            context_open_in_terminal: self.i18n.t("ide.contextMenu.openInTerminal"),
            new_file_placeholder: self.i18n.t("ide.inline.newFilePlaceholder"),
            new_folder_placeholder: self.i18n.t("ide.inline.newFolderPlaceholder"),
            validation_name_empty: self.i18n.t("ide.validation.nameEmpty"),
            validation_name_contains_slash: self.i18n.t("ide.validation.nameContainsSlash"),
            validation_name_invalid: self.i18n.t("ide.validation.nameInvalid"),
            validation_name_invalid_chars: self.i18n.t("ide.validation.nameInvalidChars"),
            validation_name_too_long: self.i18n.t("ide.validation.nameTooLong"),
            delete_confirm_title: self.i18n.t("ide.delete.confirmTitle"),
            delete_folder_warning: self.i18n.t("ide.delete.folderWarning"),
            delete_will_close_tabs: self.i18n.t("ide.delete.willCloseTabs"),
            delete_has_unsaved: self.i18n.t("ide.delete.hasUnsaved"),
            delete_confirm: self.i18n.t("ide.delete.confirm"),
            delete_deleting: self.i18n.t("ide.delete.deleting"),
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
            editor_font_fallback: ide_editor_font_fallback(settings),
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
            background_active: self.background_surface_active("ide"),
            agent_mode: node_agent_mode_from_settings(settings),
        }
    }

    pub(super) fn apply_ide_runtime_settings_to_surfaces(&mut self, cx: &mut Context<Self>) {
        let tokens = self.tokens;
        let runtime_settings = self.ide_runtime_settings();
        self.ide_workspace.update(cx, |workspace, cx| {
            workspace.apply_runtime_settings(tokens, runtime_settings, cx);
        });
    }

    pub(super) fn handle_ide_workspace_event(
        &mut self,
        event: &IdeWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            IdeWorkspaceEvent::RememberAgentMode(mode) => {
                self.remember_ide_agent_mode(*mode, cx);
            }
            IdeWorkspaceEvent::SurfaceOpened { tab_id, node_id } => {
                self.register_ai_runtime_ide_surface_owner(*tab_id, node_id, cx);
            }
            IdeWorkspaceEvent::SurfaceClosed { tab_id } => {
                self.ai_runtime_context
                    .update(cx, |runtime, _cx| runtime.revoke_ide_surface(*tab_id));
            }
            IdeWorkspaceEvent::TransientSurfaceClosed { tab_id } => {
                self.close_transient_ide_tab_after_folder_cancel(*tab_id, cx);
            }
            IdeWorkspaceEvent::ReconnectRestoreCompleted {
                reconnect_node_id,
                result,
                message,
            } => {
                self.complete_pending_ide_reconnect_restore(
                    reconnect_node_id,
                    *result,
                    message.clone(),
                    cx,
                );
            }
        }
    }

    fn register_ai_runtime_ide_surface_owner(
        &mut self,
        tab_id: TabId,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        let label = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| "IDE workspace".to_string());
        let resource_ref = self.ssh_nodes.get(node_id).and_then(|node| {
            node.saved_connection_id.as_ref().and_then(|connection_id| {
                oxideterm_ai::StableResourceRef::new(
                    oxideterm_ai::StableResourceKind::SavedConnection,
                    connection_id.clone(),
                    Some(label.clone()),
                )
                .ok()
            })
        });
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.register_ide_surface(tab_id, node_id.clone(), label, resource_ref);
        });
    }

    fn close_transient_ide_tab_after_folder_cancel(
        &mut self,
        tab_id: oxideterm_workspace::TabId,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tabs(cx).iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        let Some(TabRemovalTransition {
            mount_cleanup,
            previous_active_tab_id,
            next_active_tab_id,
            ..
        }) = self
            .tab_host
            .update(cx, |tab_host, _cx| tab_host.remove_tab_at(index))
        else {
            return;
        };
        self.apply_tab_mount_cleanup(mount_cleanup, None, cx);

        if previous_active_tab_id != next_active_tab_id {
            // Picker cancellation is not a user project-close action. Pick the
            // nearest visible tab without recording an IDE last-closed marker,
            // so reconnect restore remains governed only by real project tabs.
            self.apply_main_window_active_tab_change(
                previous_active_tab_id,
                next_active_tab_id,
                cx,
            );
        }

        self.sync_active_tab_surface(cx);
        cx.notify();
    }

    fn remember_ide_agent_mode(&mut self, mode: NodeAgentMode, cx: &mut Context<Self>) {
        self.settings_store.settings_mut().ide.agent_mode = match mode {
            NodeAgentMode::Ask => IdeAgentMode::Ask,
            NodeAgentMode::Enabled => IdeAgentMode::Enabled,
            NodeAgentMode::Disabled => IdeAgentMode::Disabled,
        };
        let _ = self.settings_store.save();
        self.settings_workspace.update(cx, |settings, _cx| {
            settings.acknowledge_external_store_state()
        });
        self.ai_entity
            .update(cx, |ai, _cx| ai.set_agent_fs_mode(mode));
        cx.notify();
    }
}

pub(super) fn node_agent_mode_from_settings(settings: &PersistedSettings) -> NodeAgentMode {
    match settings.ide.agent_mode {
        IdeAgentMode::Ask => NodeAgentMode::Ask,
        IdeAgentMode::Enabled => NodeAgentMode::Enabled,
        IdeAgentMode::Disabled => NodeAgentMode::Disabled,
    }
}

fn ide_editor_font_fallback(settings: &PersistedSettings) -> Option<String> {
    let terminal_family = settings
        .terminal
        .font_family
        .terminal_family_name(&settings.terminal.custom_font_family);
    let configured_cjk_family = settings.terminal.cjk_font_family.trim();
    // Code glyphs keep the editor's monospace primary while CJK glyphs follow
    // the same explicit family preference as terminal content.
    let preferred_family = if configured_cjk_family.is_empty() {
        terminal_family.as_str()
    } else {
        configured_cjk_family
    };
    oxideterm_gpui_ui::css_font_family_head(preferred_family).map(|family| family.to_string())
}

fn ide_restore_was_closed_after_snapshot(
    closed_at: Option<SystemTime>,
    snapshot_at: Option<SystemTime>,
) -> bool {
    matches!((closed_at, snapshot_at), (Some(closed_at), Some(snapshot_at)) if closed_at > snapshot_at)
}

fn ide_surface_mount_for_location(
    is_main_window_active: bool,
    is_detached: bool,
    has_detached_window: bool,
) -> IdeSurfaceMount {
    if has_detached_window {
        IdeSurfaceMount::DetachedWindow
    } else if is_main_window_active && !is_detached {
        IdeSurfaceMount::MainWindow
    } else {
        IdeSurfaceMount::Hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_ssh::{NodeRouter, SshConnectionRegistry};
    use std::time::Duration;

    fn test_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build IDE registry test runtime"),
        )
    }

    fn test_file_system() -> NodeAgentIdeFileSystem {
        NodeAgentIdeFileSystem::new(
            NodeRouter::new(SshConnectionRegistry::default()),
            NodeAgentMode::Ask,
        )
    }

    fn test_registry(cx: &mut TestAppContext) -> Entity<IdeWorkspaceEntity> {
        let fs = test_file_system();
        let backend_runtime = test_runtime();
        cx.new(move |_| IdeWorkspaceEntity::new(fs, backend_runtime))
    }

    fn test_surface(cx: &mut TestAppContext) -> Entity<IdeSurface> {
        let fs = test_file_system();
        let backend_runtime = test_runtime();
        cx.new(move |cx| {
            IdeSurface::new(
                fs,
                oxideterm_theme::default_tokens(),
                IdeLabels::default(),
                IdeRuntimeSettings::default(),
                backend_runtime,
                cx,
            )
        })
    }

    #[test]
    fn ide_editor_font_fallback_follows_terminal_font_preferences() {
        let mut settings = PersistedSettings::default();
        settings.terminal.font_family = oxideterm_settings::FontFamily::Custom;
        settings.terminal.custom_font_family = "'等线', monospace".to_string();

        assert_eq!(
            ide_editor_font_fallback(&settings).as_deref(),
            Some("DengXian")
        );

        settings.terminal.custom_font_family = "Consolas".to_string();
        settings.terminal.cjk_font_family = "等线".to_string();
        assert_eq!(
            ide_editor_font_fallback(&settings).as_deref(),
            Some("DengXian")
        );
    }

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

    #[test]
    fn ide_mount_tracks_main_hidden_detaching_and_detached_locations() {
        assert_eq!(
            ide_surface_mount_for_location(true, false, false),
            IdeSurfaceMount::MainWindow
        );
        assert_eq!(
            ide_surface_mount_for_location(false, false, false),
            IdeSurfaceMount::Hidden
        );
        assert_eq!(
            ide_surface_mount_for_location(false, true, false),
            IdeSurfaceMount::Hidden
        );
        assert_eq!(
            ide_surface_mount_for_location(false, true, true),
            IdeSurfaceMount::DetachedWindow
        );
    }

    #[gpui::test]
    fn ide_registry_owns_surfaces_node_index_and_deterministic_target(cx: &mut TestAppContext) {
        let registry = test_registry(cx);
        let later_surface = test_surface(cx);
        let earlier_surface = test_surface(cx);
        let later_tab_id = TabId(9);
        let earlier_tab_id = TabId(3);
        let later_node_id = NodeId::new("node-b");
        let earlier_node_id = NodeId::new("node-a");

        registry.update(cx, |registry, cx| {
            registry.register_surface(
                later_tab_id,
                later_node_id.clone(),
                later_surface.clone(),
                cx,
            );
            registry.register_surface(
                earlier_tab_id,
                earlier_node_id.clone(),
                earlier_surface.clone(),
                cx,
            );
            assert_eq!(registry.tab_for_node(&later_node_id), Some(later_tab_id));
            assert_eq!(
                registry.tab_for_node(&earlier_node_id),
                Some(earlier_tab_id)
            );
            assert_eq!(
                registry
                    .surface_for_effect(None, None)
                    .map(|surface| surface.entity_id()),
                Some(earlier_surface.entity_id())
            );
            assert_eq!(
                registry
                    .surface_for_effect(None, Some("node-b"))
                    .map(|surface| surface.entity_id()),
                Some(later_surface.entity_id())
            );
        });
    }

    #[gpui::test]
    fn same_node_close_only_suppresses_restore_after_final_surface(cx: &mut TestAppContext) {
        let registry = test_registry(cx);
        let first_surface = test_surface(cx);
        let second_surface = test_surface(cx);
        let node_id = NodeId::new("shared-node");
        let snapshot_at = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let closed_at = snapshot_at + Duration::from_secs(1);

        registry.update(cx, |registry, cx| {
            registry.register_surface(TabId(2), node_id.clone(), first_surface, cx);
            registry.register_surface(TabId(1), node_id.clone(), second_surface, cx);

            assert!(registry.close_surface_at(
                TabId(1),
                IdeSurfaceCloseReason::UserProjectClose,
                closed_at,
                cx,
            ));
            assert_eq!(registry.tab_for_node(&node_id), Some(TabId(2)));
            assert!(!ide_restore_was_closed_after_snapshot(
                registry.last_closed_at_by_node.get(&node_id).copied(),
                Some(snapshot_at),
            ));

            assert!(registry.close_surface_at(
                TabId(2),
                IdeSurfaceCloseReason::UserProjectClose,
                closed_at,
                cx,
            ));
            assert!(ide_restore_was_closed_after_snapshot(
                registry.last_closed_at_by_node.get(&node_id).copied(),
                Some(snapshot_at),
            ));
        });
    }

    #[gpui::test]
    fn transient_surface_close_does_not_record_reconnect_suppression(cx: &mut TestAppContext) {
        let registry = test_registry(cx);
        let surface = test_surface(cx);
        let node_id = NodeId::new("picker-node");

        registry.update(cx, |registry, cx| {
            registry.register_surface(TabId(7), node_id.clone(), surface, cx);
            assert!(registry.close_surface_at(
                TabId(7),
                IdeSurfaceCloseReason::TransientFolderPickerCancel,
                SystemTime::now(),
                cx,
            ));
            assert!(!registry.last_closed_at_by_node.contains_key(&node_id));
            assert!(registry.surface(TabId(7)).is_none());
        });
    }
}
