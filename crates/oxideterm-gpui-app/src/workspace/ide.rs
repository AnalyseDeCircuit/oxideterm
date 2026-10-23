mod shell_ide;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, IntoElement, Subscription, div,
};
use oxideterm_gpui_ide::{
    IdeAiContextSnapshot, IdeLabels, IdeModalLayer, IdePluginSnapshot, IdeRuntimeSettings,
    IdeSurface, IdeSurfaceEvent, IdeSurfaceMount, NodeAgentMode,
};
use oxideterm_gpui_ui::choice_dialog::{ChoiceDialog, choice_dialog};
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
    ShellReturnReady {
        tab_id: TabId,
        failed: bool,
    },
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
    modal: Entity<IdeModalLayer>,
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
    shell_sources: HashMap<TabId, oxideterm_workspace::PaneId>,
    shell_open_tasks: HashMap<oxideterm_workspace::PaneId, gpui::Task<()>>,
    fs: NodeAgentIdeFileSystem,
    backend_runtime: Arc<tokio::runtime::Runtime>,
    surfaces_by_tab: BTreeMap<u64, IdeSurfaceEntry>,
    tabs_by_node: HashMap<NodeId, BTreeSet<u64>>,
    last_closed_at_by_node: HashMap<NodeId, SystemTime>,
    reconnect_access_by_node: HashMap<NodeId, oxideterm_ide_fs::RemoteAccess>,
}

impl IdeWorkspaceEntity {
    pub(super) fn modal_for_surface(
        &self,
        surface: &Entity<IdeSurface>,
    ) -> Option<Entity<IdeModalLayer>> {
        self.surfaces_by_tab
            .values()
            .find(|entry| entry.surface == *surface)
            .map(|entry| entry.modal.clone())
    }

    pub(super) fn open_project_count(&self, cx: &App) -> usize {
        // Count actual project owners, including inline and detached surfaces, rather than AI consumers.
        self.surfaces_by_tab
            .values()
            .filter(|entry| entry.surface.read(cx).has_open_project())
            .count()
    }

    pub(super) fn shell_surface_for_pane(
        &self,
        pane: oxideterm_workspace::PaneId,
    ) -> Option<(TabId, Entity<IdeSurface>)> {
        self.shell_sources.iter().find_map(|(id, source)| {
            (*source == pane)
                .then(|| self.surface(*id).map(|surface| (*id, surface)))
                .flatten()
        })
    }

    pub(super) fn new(
        fs: NodeAgentIdeFileSystem,
        backend_runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Self {
            shell_sources: HashMap::new(),
            shell_open_tasks: HashMap::new(),
            fs,
            backend_runtime,
            surfaces_by_tab: BTreeMap::new(),
            tabs_by_node: HashMap::new(),
            last_closed_at_by_node: HashMap::new(),
            reconnect_access_by_node: HashMap::new(),
        }
    }

    pub(super) fn create_folder_picker_surface(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        initial_path: String,
        access: Option<oxideterm_ide_fs::RemoteAccess>,
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
            surface.request_backend_selection(node_id.0.clone(), initial_path, cx);
            if let Some(access) = access {
                surface.select_access(access, cx);
            }
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
        let access = self
            .reconnect_access_by_node
            .get(&target_node_id)
            .copied()
            .unwrap_or_default();
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        let surface =
            cx.new(|cx| IdeSurface::new(fs, tokens, labels, runtime_settings, backend_runtime, cx));
        let restored = surface.update(cx, |surface, cx| {
            surface.restore_reconnect_with_access(
                ide_snapshot,
                reconnect_node_id.0.clone(),
                access,
                cx,
            )
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
                IdeSurfaceEvent::ShellReturnReady | IdeSurfaceEvent::ShellOpenFailed => {
                    let failed = matches!(event, IdeSurfaceEvent::ShellOpenFailed);
                    // Defer teardown until this surface subscription has returned.
                    cx.spawn(async move |weak_registry, cx| {
                        let _ = weak_registry.update(cx, |_workspace, cx| {
                            cx.emit(IdeWorkspaceEvent::ShellReturnReady { tab_id, failed });
                        });
                    })
                    .detach();
                }
                IdeSurfaceEvent::ProjectOpened => {
                    if let Some(node_id) = workspace.node_for_tab(tab_id).cloned() {
                        // A successful project open supersedes an earlier explicit-close marker.
                        workspace.last_closed_at_by_node.remove(&node_id);
                        workspace
                            .reconnect_access_by_node
                            .insert(node_id, _surface.read(cx).remote_access());
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
        let modal = cx.new(|cx| IdeModalLayer::new(surface.clone(), cx));
        self.surfaces_by_tab.insert(
            tab_id.0,
            IdeSurfaceEntry {
                modal,
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
        self.shell_sources.remove(&tab_id);
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
    pub(in crate::workspace) fn editor_search_labels(
        &self,
    ) -> oxideterm_gpui_editor::EditorSearchLabels {
        oxideterm_gpui_editor::EditorSearchLabels {
            find_placeholder: self.i18n.t("ide.find_placeholder"),
            replace_placeholder: self.i18n.t("ide.replace_placeholder"),
            find_next: self.i18n.t("ide.find_next"),
            find_previous: self.i18n.t("ide.find_previous"),
            match_case: self.i18n.t("ide.match_case"),
            toggle_replace: self.i18n.t("ide.toggle_replace"),
            close_search: self.i18n.t("ide.close_search"),
            replace_btn: self.i18n.t("ide.replace_btn"),
            replace_all_btn: self.i18n.t("ide.replace_all_btn"),
            use_regex: self.i18n.t("ide.use_regex"),
            invalid_regex: self.i18n.t("search.invalid_regex"),
            no_results: self.i18n.t("search.no_results"),
        }
    }
    pub(in crate::workspace) fn sync_ide_surface_mount(&mut self, tab_id: TabId, cx: &mut App) {
        let mut pages = Vec::new();
        let mut panes = Vec::new();
        if let Some(root) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref())
        {
            root.collect_page_ids(&mut pages);
            root.collect_pane_ids(&mut panes);
        }
        for page in pages {
            self.sync_ide_surface_mount(page, cx);
        }

        let (outside_main_window, detached_window_open) = {
            let tab_host = self.tab_host.read(cx);
            (
                tab_host.is_outside_main_window(tab_host.container_tab_id(tab_id)),
                tab_host.is_detached(tab_id),
            )
        };
        let mount = ide_surface_mount_for_location(
            self.tab_host.read(cx).surface_is_visible(tab_id),
            outside_main_window,
            detached_window_open,
        );
        // WorkspaceApp reports only window placement; the IDE owner performs
        // the sampling, watcher, and node-session mount transition.
        self.ide_workspace
            .update(cx, |workspace, cx| workspace.set_mount(tab_id, mount, cx));
        self.ide_workspace.update(cx, |workspace, cx| {
            for pane in panes {
                if let Some((id, _)) = workspace.shell_surface_for_pane(pane) {
                    workspace.set_mount(id, mount, cx);
                }
            }
        });
    }

    pub(super) fn open_ide_folder_picker_tab(
        &mut self,
        node_id: NodeId,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let initial_path = self.active_ssh_terminal_cwd_path_for_node(&node_id, cx);
        if self.ide_workspace.read(cx).tab_for_node(&node_id).is_some() {
            self.open_ide_folder_picker_tab_with_initial_path(
                node_id,
                initial_path,
                IdeOpenIntent::ActivateOrCreate,
                None,
                cx,
            );
            return;
        }
        let labels = self.ide_labels();
        let prompt = choice_dialog(
            self.tokens,
            ChoiceDialog {
                title: labels.access_title.clone(),
                description: None,
                choices: labels.access_modes.clone(),
                cancel: labels.cancel.clone(),
                confirm: labels.confirm.clone(),
            },
            window,
            cx,
        );
        let window_handle = window.window_handle();
        cx.spawn(async move |weak, cx| {
            let access = match prompt.await {
                Some(0) => oxideterm_ide_fs::RemoteAccess::Sftp,
                Some(1) => {
                    let Ok(prompt) = window_handle.update(cx, |_, window, cx| {
                        weak.update(cx, |this, cx| {
                            choice_dialog(
                                this.tokens,
                                ChoiceDialog {
                                    title: labels.access_modes[1].clone(),
                                    description: Some(labels.agent_choice_hint.clone()),
                                    choices: labels.agent_choices.clone(),
                                    cancel: labels.cancel.clone(),
                                    confirm: labels.confirm.clone(),
                                },
                                window,
                                cx,
                            )
                        })
                    }) else {
                        return;
                    };
                    let Ok(prompt) = prompt else {
                        return;
                    };
                    match prompt.await {
                        Some(0) => oxideterm_ide_fs::RemoteAccess::Commands,
                        Some(1) => oxideterm_ide_fs::RemoteAccess::Agent,
                        _ => return,
                    }
                }
                _ => return,
            };
            // The dialog owns no surface or node consumer; allocate only after confirmation.
            let _ = window_handle.update(cx, |_, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.ssh_nodes.contains_key(&node_id) {
                        this.open_ide_folder_picker_tab_with_initial_path(
                            node_id,
                            initial_path,
                            IdeOpenIntent::ActivateOrCreate,
                            Some(access),
                            cx,
                        );
                    }
                });
            });
        })
        .detach();
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
            None,
            cx,
        );
    }

    fn open_ide_folder_picker_tab_with_initial_path(
        &mut self,
        node_id: NodeId,
        initial_path_override: Option<String>,
        intent: IdeOpenIntent,
        access: Option<oxideterm_ide_fs::RemoteAccess>,
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
                    access,
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
        if !self
            .tab_host
            .read(cx)
            .is_outside_main_window(self.tab_host.read(cx).container_tab_id(tab_id))
        {
            self.set_main_window_active_tab(Some(tab_id), cx);
            self.active_surface = oxideterm_gpui_settings_view::ActiveSurface::Terminal;
        }
        self.active_ssh_node_id = Some(node_id.clone());
        self.expanded_ssh_nodes.insert(node_id.clone());
        // The chosen backend consumes the existing node without creating a terminal.
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

        if !self
            .tab_host
            .read(cx)
            .is_outside_main_window(self.tab_host.read(cx).container_tab_id(tab_id))
        {
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
        let Some(tab_id) = self.active_content_tab_id(cx) else {
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

    pub(in crate::workspace) fn handle_ide_modal_edit_action(
        &mut self,
        key: &str,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ide_modal_owns_input(cx) {
            return false;
        }
        if let Some(surface) = self.active_ide_surface(cx) {
            let event = gpui::KeyDownEvent {
                keystroke: gpui::Keystroke {
                    modifiers: gpui::Modifiers {
                        control: true,
                        ..Default::default()
                    },
                    key: key.into(),
                    key_char: None,
                },
                is_held: false,
                prefer_character_input: false,
            };
            surface.update(cx, |surface, cx| {
                surface.handle_modal_key(&event, window, cx);
            });
        }
        true
    }

    pub(in crate::workspace) fn active_ide_surface(
        &self,
        cx: &App,
    ) -> Option<gpui::Entity<IdeSurface>> {
        if let Some(pane) = self.active_pane_id(cx)
            && let Some((_, surface)) = self.ide_workspace.read(cx).shell_surface_for_pane(pane)
        {
            return Some(surface);
        }
        let tab_id = self.active_content_tab_id(cx)?;
        let tab = self.tabs(cx).iter().find(|tab| tab.id == tab_id)?;
        (tab.kind == TabKind::Ide)
            .then(|| self.ide_workspace.read(cx).surface(tab_id))
            .flatten()
    }

    fn ide_labels(&self) -> IdeLabels {
        IdeLabels {
            file_binary: self.i18n.t("ide.file_binary"),
            file_too_large: self.i18n.t("ide.file_too_large"),
            file_encoding: self.i18n.t("ide.file_encoding"),
            file_line_ending: self.i18n.t("ide.file_line_ending"),
            reopen_encoding: self.i18n.t("ide.reopen_encoding"),
            save_encoding: self.i18n.t("ide.save_encoding"),
            reopen_dirty: self.i18n.t("ide.reopen_dirty"),
            invalid_encoding: self.i18n.t("ide.invalid_encoding"),
            unrepresentable_text: self.i18n.t("ide.unrepresentable_text"),
            large_file: self.i18n.t("ide.large_file"),

            open_folder: self.i18n.t("ide.open_folder"),
            backend_requirements: self.i18n.t("terminal.shell_ide.requirements"),
            shell_commands: self.i18n.t("terminal.shell_ide.commands"),
            access_title: self.i18n.t("terminal.shell_ide.access_title"),
            agent_choices: ["commands", "agent"]
                .map(|key| self.i18n.t(&format!("terminal.shell_ide.{key}"))),
            agent_choice_hint: self.i18n.t("terminal.shell_ide.agent_choice_hint"),
            agent_installing: self.i18n.t("terminal.shell_ide.agent_installing"),
            agent_starting: self.i18n.t("terminal.shell_ide.agent_starting"),
            sftp_unavailable: self.i18n.t("terminal.shell_ide.sftp_unavailable"),
            ssh_unavailable: self.i18n.t("terminal.shell_ide.ssh_unavailable"),
            ssh_shell: self.i18n.t("terminal.shell_ide.ssh_shell"),
            access_modes: ["sftp", "ssh"]
                .map(|key| self.i18n.t(&format!("terminal.shell_ide.{key}"))),
            return_to_terminal: self.i18n.t("terminal.shell_ide.return"),
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
            confirm: self.i18n.t("common.actions.confirm"),
            editor_search: self.editor_search_labels(),
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
            editor_font_family: ide_editor_font_family(settings),
            editor_font_weight: settings
                .ide
                .font_weight
                .unwrap_or(settings.terminal.font_weight)
                .clamp(100, 900) as f32,
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
            IdeWorkspaceEvent::ShellReturnReady { tab_id, failed } => {
                self.finish_shell_ide_return(*tab_id, cx);
                if *failed {
                    self.push_command_palette_toast(
                        self.i18n.t("terminal.shell_ide.convert"),
                        Some(self.i18n.t("terminal.shell_ide.failed")),
                        super::TerminalNoticeVariant::Error,
                        cx,
                    );
                }
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
}

pub(super) fn node_agent_mode_from_settings(settings: &PersistedSettings) -> NodeAgentMode {
    match settings.ide.agent_mode {
        IdeAgentMode::Ask => NodeAgentMode::Ask,
        IdeAgentMode::Enabled => NodeAgentMode::Enabled,
        IdeAgentMode::Disabled => NodeAgentMode::Disabled,
    }
}

fn ide_editor_font_family(settings: &PersistedSettings) -> String {
    let family = settings
        .ide
        .font_family
        .unwrap_or(settings.terminal.font_family);
    let custom = if settings.ide.font_family.is_some() {
        &settings.ide.custom_font_family
    } else {
        &settings.terminal.custom_font_family
    };
    family.terminal_family_name(custom)
}

fn ide_editor_font_fallback(settings: &PersistedSettings) -> Option<String> {
    let primary_family = ide_editor_font_family(settings);
    let configured_cjk_family = settings
        .ide
        .cjk_font_family
        .as_deref()
        .unwrap_or(&settings.terminal.cjk_font_family)
        .trim();
    // An empty CJK override uses the primary font and the editor's platform fallback chain.
    let preferred_family = if configured_cjk_family.is_empty() {
        primary_family.as_str()
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

    #[gpui::test]
    fn ide_open_count_tracks_hidden_and_detached_projects_and_excludes_pending_pickers(
        cx: &mut TestAppContext,
    ) {
        let registry = test_registry(cx);
        let picker = test_surface(cx);
        let loading = test_surface(cx);
        loading.update(cx, |surface, cx| {
            surface.open_remote_project("node", "/loading", cx)
        });
        registry.update(cx, |registry, cx| {
            registry.register_surface(TabId(1), NodeId::new("node"), picker, cx);
            registry.register_surface(TabId(2), NodeId::new("node"), loading, cx);
            assert_eq!(registry.open_project_count(cx), 0);
        });
        for (tab, mount) in [
            (3, IdeSurfaceMount::Hidden),
            (4, IdeSurfaceMount::DetachedWindow),
        ] {
            let mut project = oxideterm_ide_core::IdeWorkspace::new();
            project.open_project(
                oxideterm_ide_core::IdeLocation::remote("node", format!("/project-{tab}")),
                "Project",
            );
            let surface = test_surface(cx);
            surface.update(cx, |surface, cx| {
                surface.restore_snapshot(project.snapshot().unwrap(), cx);
                surface.set_mount(mount, cx);
            });
            registry.update(cx, |registry, cx| {
                registry.register_surface(TabId(tab), NodeId::new("node"), surface, cx)
            });
        }
        registry.update(cx, |registry, cx| {
            assert_eq!(registry.open_project_count(cx), 2);
            registry
                .surface(TabId(3))
                .unwrap()
                .update(cx, |surface, cx| surface.mark_connection_interrupted(cx));
            assert_eq!(registry.open_project_count(cx), 2);
            registry.close_surface(TabId(3), IdeSurfaceCloseReason::UserProjectClose, cx);
            assert_eq!(registry.open_project_count(cx), 1);
            registry.close_surface(TabId(4), IdeSurfaceCloseReason::UserProjectClose, cx);
            assert_eq!(registry.open_project_count(cx), 0);
        });
    }

    #[cfg(unix)]
    #[gpui::test]
    fn inline_ide_text_input_does_not_fall_through_to_terminal(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_INLINE_IDE_INPUT_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; keep this regression away from user data.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let settings_path = super::super::default_settings_path();
        assert!(settings_path.starts_with(std::path::PathBuf::from(fixture_dir)));
        let mut settings = super::super::SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();
        cx.update(|cx| {
            cx.bind_keys(crate::platform::app_key_bindings(settings.settings()));
            crate::keybindings::install_context_keybindings(
                &settings.settings().keybindings.overrides,
                cx,
            );
        });
        cx.executor().allow_parking();
        let surface = test_surface(cx);
        surface.update(cx, |surface, cx| {
            let mut project = oxideterm_ide_core::IdeWorkspace::new();
            project.open_project(
                oxideterm_ide_core::IdeLocation::remote("node", "/repo"),
                "repo",
            );
            project
                .open_file(
                    oxideterm_ide_core::IdeLocation::remote("node", "/repo/text.txt"),
                    "original",
                    oxideterm_ide_core::SavedFileVersion::unknown(),
                )
                .unwrap();
            surface.restore_snapshot(project.snapshot().unwrap(), cx);
        });
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, cx| {
                let pane = cx.new(|cx| {
                    oxideterm_gpui_terminal::TerminalPane::new_local_with_config_and_preferences(
                        oxideterm_terminal::LocalPtyConfig {
                            shell: Some(oxideterm_terminal::ShellInfo::new(
                                "test-cat", "cat", "/bin/cat",
                            )),
                            load_profile: false,
                            ..Default::default()
                        },
                        Default::default(),
                        window,
                        cx,
                    )
                    .unwrap()
                });
                let pane_id = oxideterm_workspace::PaneId(100);
                let session = oxideterm_workspace::TerminalSessionId(100);
                workspace.tab_host.update(cx, |host, cx| {
                    host.register_terminal_pane(
                        pane_id,
                        session,
                        pane.clone(),
                        window.window_handle(),
                        cx,
                    );
                    host.insert_and_select_main_tab(Tab {
                        id: TabId(100),
                        kind: TabKind::SshTerminal,
                        title: "Terminal".into(),
                        title_source: TabTitleSource::Static,
                        root_pane: Some(oxideterm_workspace::PaneNode::leaf(pane_id, session)),
                        active_pane_id: Some(pane_id),
                    });
                });
            });
            super::super::WorkspaceWindowShell::new(workspace, window, cx)
        });
        cx.simulate_resize(gpui::size(gpui::px(1000.0), gpui::px(650.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
            window.focus(&workspace.read(cx).focus_handle.clone(), cx);
        });
        workspace.update(cx, |workspace, cx| {
            workspace.ide_workspace.update(cx, |registry, cx| {
                registry.register_surface(TabId(101), NodeId::new("node"), surface.clone(), cx);
                registry
                    .shell_sources
                    .insert(TabId(101), oxideterm_workspace::PaneId(100));
            });
            workspace.needs_active_pane_focus = true;
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
        let bounds = cx.debug_bounds("ide-editor-content").unwrap();
        cx.simulate_click(
            gpui::point(
                bounds.right() - gpui::px(30.0),
                bounds.top() + gpui::px(10.0),
            ),
            gpui::Modifiers::default(),
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_input("abc");
        surface.update(cx, |surface, cx| {
            assert_eq!(
                surface.snapshot(cx).unwrap().buffers[0].text.as_ref(),
                "originalabc"
            );
        });
        cx.simulate_keystrokes("backspace");
        surface.update(cx, |surface, cx| {
            assert_eq!(
                surface.snapshot(cx).unwrap().buffers[0].text.as_ref(),
                "originalab"
            );
        });

        surface.update(cx, |surface, cx| {
            surface.open_remote_folder_picker_for_node("node", "/repo", cx)
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        workspace.read_with(cx, |workspace, cx| {
            assert!(
                workspace.ide_modal_owns_input(cx),
                "unexpected modal owner: {:?}",
                workspace.active_window_modal_owner(cx)
            )
        });
        cx.update(|window, cx| {
            assert!(
                gpui::Focusable::focus_handle(surface.read(cx), cx).is_focused(window),
                "modal did not take focus"
            )
        });
        let modal_bounds = cx.debug_bounds("ide-window-modal").unwrap();
        assert_eq!(
            modal_bounds.origin,
            gpui::point(gpui::px(0.0), gpui::px(0.0))
        );
        assert_eq!(
            modal_bounds.size,
            gpui::size(gpui::px(1000.0), gpui::px(650.0))
        );
        cx.simulate_click(
            gpui::point(gpui::px(20.0), gpui::px(20.0)),
            gpui::Modifiers::default(),
        );
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.simulate_keystrokes(&format!("{modifier}-a"));
        cx.update(|window, cx| {
            surface.update(cx, |surface, cx| {
                assert_eq!(
                    gpui::EntityInputHandler::selected_text_range(surface, false, window, cx)
                        .unwrap()
                        .range,
                    0..5
                )
            })
        });

        cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("/新目录".into())));
        cx.simulate_keystrokes(&format!("{modifier}-v"));
        cx.update(|window, cx| {
            surface.update(cx, |surface, cx| {
                assert_eq!(
                    gpui::EntityInputHandler::text_for_range(
                        surface,
                        0..100,
                        &mut None,
                        window,
                        cx
                    )
                    .as_deref(),
                    Some("/新目录")
                );
            })
        });
        cx.simulate_keystrokes(&format!("{modifier}-a"));
        cx.update(|window, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("/菜单目录".into()));
            window.dispatch_action(Box::new(crate::Paste), cx);
        });
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes(&format!("{modifier}-k"));
        workspace.read_with(cx, |workspace, cx| {
            assert!(!workspace.command_palette.read(cx).is_open());
            assert_eq!(workspace.active_tab_id(cx), Some(TabId(100)));
        });
        cx.update(|window, cx| {
            surface.update(cx, |surface, cx| {
                assert_eq!(
                    gpui::EntityInputHandler::text_for_range(
                        surface,
                        0..100,
                        &mut None,
                        window,
                        cx
                    )
                    .as_deref(),
                    Some("/菜单目录")
                );
                assert_eq!(
                    surface.snapshot(cx).unwrap().buffers[0].text.as_ref(),
                    "originalab"
                );
            })
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_input("z");
        surface.update(cx, |surface, cx| {
            assert!(!surface.has_modal());
            assert_eq!(
                surface.snapshot(cx).unwrap().buffers[0].text.as_ref(),
                "originalabz"
            );
        });
        cx.simulate_keystrokes(&format!("{modifier}-k"));
        workspace.read_with(cx, |workspace, cx| {
            assert!(
                workspace.command_palette.read(cx).is_open(),
                "shortcut fixture must work after the modal closes"
            )
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let sidebar_collapsed = workspace.read_with(cx, |workspace, _| workspace.sidebar_collapsed);
        cx.simulate_keystrokes(&format!("{modifier}-\\"));
        workspace.read_with(cx, |workspace, _| {
            assert_eq!(workspace.sidebar_collapsed, sidebar_collapsed)
        });
        let exit_delay = workspace.read_with(cx, |workspace, _| {
            oxideterm_gpui_ui::motion::duration(
                &workspace.tokens,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            )
        });
        cx.simulate_keystrokes("escape");
        cx.executor().advance_clock(exit_delay);
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        workspace.read_with(cx, |workspace, cx| {
            assert!(!workspace.command_palette.read(cx).is_open())
        });
        cx.simulate_keystrokes(&format!("{modifier}-\\"));
        workspace.read_with(cx, |workspace, _| {
            assert_eq!(workspace.sidebar_collapsed, !sidebar_collapsed)
        });
    }

    #[gpui::test]
    fn closing_shell_surface_removes_only_the_exact_pane_binding(cx: &mut TestAppContext) {
        use oxideterm_workspace::PaneId;
        let registry = test_registry(cx);
        let first = test_surface(cx);
        let second = test_surface(cx);
        registry.update(cx, |registry, cx| {
            registry.register_surface(TabId(10), NodeId::new("shell:first"), first.clone(), cx);
            registry.register_surface(TabId(20), NodeId::new("shell:second"), second.clone(), cx);
            registry.shell_sources.insert(TabId(10), PaneId(1));
            registry.shell_sources.insert(TabId(20), PaneId(2));
            assert_eq!(
                registry
                    .shell_surface_for_pane(PaneId(1))
                    .map(|(_, s)| s.entity_id()),
                Some(first.entity_id()),
            );
            assert!(registry.shell_surface_for_pane(PaneId(3)).is_none());
            registry.close_surface(TabId(10), IdeSurfaceCloseReason::UserProjectClose, cx);
            assert!(registry.shell_surface_for_pane(PaneId(1)).is_none());
            assert_eq!(
                registry
                    .shell_surface_for_pane(PaneId(2))
                    .map(|(_, s)| s.entity_id()),
                Some(second.entity_id()),
            );
            assert_eq!(
                registry.node_for_tab(TabId(20)),
                Some(&NodeId::new("shell:second"))
            );
        });
    }

    #[test]
    fn ide_editor_font_preferences_follow_terminal_or_use_independent_overrides() {
        let mut settings = PersistedSettings::default();
        assert_eq!(
            oxideterm_theme::default_tokens()
                .metrics
                .markdown_code_font_family,
            oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY
        );
        assert_eq!(
            ide_editor_font_fallback(&settings).as_deref(),
            Some(oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY)
        );

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
        settings.ide.font_family = Some(oxideterm_settings::FontFamily::Maple);
        settings.ide.cjk_font_family = Some("PingFang SC".into());
        assert_eq!(
            ide_editor_font_family(&settings),
            oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY
        );
        assert_eq!(
            ide_editor_font_fallback(&settings).as_deref(),
            Some("PingFang SC")
        );
        settings.terminal.font_family = oxideterm_settings::FontFamily::Menlo;
        settings.terminal.cjk_font_family = "other".into();
        assert_eq!(
            ide_editor_font_family(&settings),
            oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY
        );
        assert_eq!(
            ide_editor_font_fallback(&settings).as_deref(),
            Some("PingFang SC")
        );
        settings.ide.cjk_font_family = Some(String::new());
        assert_eq!(
            ide_editor_font_fallback(&settings).as_deref(),
            Some(oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY)
        );
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
            for snapshot in [Some(closed_at + Duration::from_secs(1)), None] {
                assert!(!ide_restore_was_closed_after_snapshot(
                    registry.last_closed_at_by_node.get(&node_id).copied(),
                    snapshot,
                ));
            }
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
