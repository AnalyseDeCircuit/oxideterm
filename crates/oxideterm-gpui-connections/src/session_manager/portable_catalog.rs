//! Mount the Session Manager with a display snapshot. The host applies emitted actions
//! and returns updated rows; the view retains its selection, search and scroll position.
use super::{
    catalog_view::{
        CatalogDispatch, CatalogIme, CatalogMount, CatalogRequest, CatalogSource, CatalogView,
        DisplayFrame,
    },
    *,
};
use crate::views::{ConnectionsEnvironment, ConnectionsEnvironmentSource, line_edit::LineEdit};
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent,
    MouseButton, Render, Window, div, prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

mod input;

#[derive(Clone, Default)]
pub struct PortableCatalogData {
    pub items: Vec<CatalogRow>,
    /// Include explicit group paths even when a folder has no sessions.
    pub groups: Vec<String>,
    pub tunnel_status: HashMap<String, TunnelDisplayStatus>,
}

impl From<PortableCatalogData> for DisplayFrame {
    fn from(data: PortableCatalogData) -> Self {
        let mut paths = HashSet::new();
        let mut counts = HashMap::new();
        for group in &data.groups {
            add_group_path_segments(group, &mut paths);
        }
        for group in data.items.iter().filter_map(|item| item.group.as_deref()) {
            add_group_path_segments(group, &mut paths);
            let mut ancestor = Some(group);
            while let Some(path) = ancestor {
                *counts.entry(path.to_owned()).or_default() += 1;
                ancestor = path.rsplit_once('/').map(|(parent, _)| parent);
            }
        }
        let mut stored_groups: Vec<_> = paths.iter().cloned().collect();
        stored_groups.sort();
        Self {
            items: data.items,
            tunnel_status: data.tunnel_status,
            groups: catalog::group_tree_from_paths(paths),
            group_counts: counts,
            stored_groups,
            ..Default::default()
        }
    }
}

impl PortableCatalogView {
    fn frame(&self, include_items: bool, cx: &App) -> DisplayFrame {
        let manager = self.manager.read(cx);
        let groups = if let Some(filter) = manager.selected_group.as_deref() {
            catalog::group_tree_from_paths(
                self.data
                    .stored_groups
                    .iter()
                    .filter(|p| {
                        session_group_path_is_within(p, filter)
                            || session_group_path_is_within(filter, p)
                    })
                    .cloned()
                    .collect(),
            )
        } else {
            self.data.groups.clone()
        };
        let items = if include_items {
            let items = self
                .data
                .items
                .iter()
                .filter(|row| row.in_group(manager.selected_group.as_deref()))
                .cloned();
            catalog::filtered_catalog(items, manager)
        } else {
            Vec::new()
        };
        DisplayFrame {
            items,
            groups,
            group_counts: self.data.group_counts.clone(),
            stored_groups: self.data.stored_groups.clone(),
            tunnel_status: self.data.tunnel_status.clone(),
            ui_font: self.environment.tokens.metrics.font_family.into(),
            chrome_divider: gpui::rgb(self.environment.tokens.ui.border),
            ..Default::default()
        }
    }
}

impl CatalogRow {
    fn in_group(&self, group: Option<&str>) -> bool {
        group.is_none_or(|g| {
            self.group
                .as_ref()
                .is_some_and(|p| session_group_path_is_within(p, g))
        })
    }
}

#[derive(Clone, Debug)]
pub enum PortableCatalogEvent {
    Connect {
        target: SessionManagerOpenTarget,
        open_sftp: bool,
    },
    Edit(SessionManagerOpenTarget),
    Delete(Vec<SessionManagerSelectionTarget>),
    Duplicate(String),
    Test(String),
    StartTunnel(String),
    StopTunnel(String),
    CreateConnection {
        group: Option<String>,
    },
    CreateGroup(String),
    RenameGroup {
        old: String,
        new: String,
    },
    DeleteGroup(String),
    MoveToGroup {
        targets: Vec<SessionManagerSelectionTarget>,
        group: Option<String>,
    },
    BatchAdd,
    ImportSshConfig(String),
    OpenSshConfigImporter,
    OpenImporters,
}

/// Mount with `cx.new` and subscribe to [`PortableCatalogEvent`] on the returned entity.
/// Replacing a snapshot keeps this mount's search, sort, and surviving selections.
pub struct PortableCatalogView {
    manager: Entity<SessionManagerState>,
    body: Entity<CatalogView>,
    dialogs: Entity<CatalogView>,
    data: DisplayFrame,
    environment: ConnectionsEnvironment,
    focus: FocusHandle,
    search: LineEdit,
    group_name: LineEdit,
    tooltip: Option<(String, String, f32, f32)>,
}

impl EventEmitter<PortableCatalogEvent> for PortableCatalogView {}
impl Focusable for PortableCatalogView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PortableCatalogView {
    pub fn new(
        data: PortableCatalogData,
        env: ConnectionsEnvironment,
        cx: &mut Context<Self>,
    ) -> Self {
        // Default only creates interaction state; the desktop constructor also starts delivery.
        let manager = cx.new(|_| SessionManagerState::default());
        let owner = cx.weak_entity();
        let theme: ConnectionsEnvironmentSource =
            Rc::new(move |cx| Some(owner.upgrade()?.read(cx).environment.clone()));
        let owner = cx.weak_entity();
        let query: CatalogSource = Rc::new(move |include_items, cx| {
            Some(owner.upgrade()?.read(cx).frame(include_items, cx))
        });
        let owner = cx.weak_entity();
        let dispatch: CatalogDispatch = Rc::new(move |request, window, cx| {
            let _ = owner.update(cx, |view, cx| view.dispatch(request, window, cx));
        });
        let owner = cx.weak_entity();
        let ime: CatalogIme =
            Rc::new(move |field, cx| owner.upgrade()?.read(cx).ime_frame(field, cx));
        let mut mount = |kind| {
            cx.new(|cx| {
                CatalogView::mount(
                    kind,
                    manager.clone(),
                    theme.clone(),
                    query.clone(),
                    ime.clone(),
                    dispatch.clone(),
                    cx,
                )
            })
        };
        let body = mount(CatalogMount::Main);
        let dialogs = mount(CatalogMount::Dialogs);
        cx.observe(&manager, |view, manager, cx| {
            view.sync_fields(manager.read(cx));
            cx.notify();
        })
        .detach();
        Self {
            manager,
            body,
            dialogs,
            data: data.into(),
            environment: env,
            focus: cx.focus_handle(),
            search: Default::default(),
            group_name: Default::default(),
            tooltip: None,
        }
    }
    pub fn set_catalog_data(&mut self, data: PortableCatalogData, cx: &mut Context<Self>) {
        self.data = data.into();
        self.manager.update(cx, |s, cx| {
            let has_group = |path: &String| self.data.stored_groups.binary_search(path).is_ok();
            s.expanded_groups.retain(has_group);
            if s.selected_group
                .as_ref()
                .is_some_and(|group| !has_group(group))
            {
                s.selected_group = None;
            }
            if s.row_action_menu
                .as_ref()
                .is_some_and(|menu| match &menu.target {
                    SessionManagerRowActionTarget::GroupRoot => false,
                    SessionManagerRowActionTarget::Group(group) => !has_group(group),
                    row => row
                        .open_target()
                        .is_some_and(|target| !self.data.items.iter().any(|r| r.target == target)),
                })
            {
                s.row_action_menu = None;
            }
            cx.notify();
        });
        self.prune_selection(cx);
        self.refresh(cx);
    }
    pub fn set_environment(&mut self, env: ConnectionsEnvironment, cx: &mut Context<Self>) {
        self.environment = env;
        self.refresh(cx);
    }
    /// Restrict the catalog to a group and its descendants, or show all groups with None.
    pub fn set_group_filter(&mut self, group: Option<String>, cx: &mut Context<Self>) {
        self.manager.update(cx, |state, cx| {
            if let Some(path) = &group {
                add_group_path_segments(path, &mut state.expanded_groups);
            }
            state.selected_group = group;
            cx.notify();
        });
        self.prune_selection(cx);
    }
    fn prune_selection(&self, cx: &mut App) {
        let state = self.manager.read(cx);
        let query = state.search_query.trim().to_lowercase();
        let visible = self
            .data
            .items
            .iter()
            .filter(|r| {
                r.in_group(state.selected_group.as_deref())
                    && (query.is_empty() || r.matches_query(&query))
            })
            .filter_map(CatalogEntry::selection_target)
            .collect();
        self.manager
            .update(cx, |s, cx| s.retain_selection_targets(visible, cx));
    }
    fn refresh(&self, cx: &mut Context<Self>) {
        App::notify(cx, self.body.entity_id());
        App::notify(cx, self.dialogs.entity_id());
        cx.notify();
    }

    fn dispatch(
        &mut self,
        request: CatalogRequest,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        use CatalogRequest::*;
        match request {
            Catalog(action) => self.action(action, cx),
            FocusSessionInput(field, event) => {
                if let Some(window) = window {
                    window.focus(&self.focus, cx);
                }
                self.manager.update(cx, |s, cx| {
                    s.focused_input = Some(field);
                    s.focused_basic_dialog_footer_action = None;
                    cx.notify();
                });
                if let Some(edit) = self.edit_mut(field) {
                    edit.marked = None;
                    edit.select_at(&event, true);
                }
            }
            DragInputSelection(event) => {
                if event.pressed_button == Some(MouseButton::Left)
                    && let Some(edit) = self.active_edit_mut(cx)
                    && let Some(caret) = edit.offset(event.position, true)
                {
                    edit.caret = caret;
                } else {
                    return;
                }
            }
            ClearComposition => {
                self.search.marked = None;
                self.group_name.marked = None;
            }
            #[cfg(not(target_family = "wasm"))]
            ToggleCatalogSidebarSearch => {
                self.manager
                    .update(cx, SessionManagerState::open_sidebar_search);
            }
            QueueTooltip { id, label, x, y } => {
                self.tooltip = Some((id, label, x, y));
                cx.notify();
                return;
            }
            ClearTooltip(id) => {
                if !self.tooltip.as_ref().is_some_and(|t| t.0 == id) {
                    return;
                }
                self.tooltip = None;
                cx.notify();
                return;
            }
        }
        self.refresh(cx);
    }

    fn action(&mut self, action: CatalogAction, cx: &mut Context<Self>) {
        use CatalogAction as A;
        use PortableCatalogEvent as E;
        let event = match action {
            A::Open {
                target: SessionManagerOpenTarget::Tunnel(id),
                ..
            } => {
                if self
                    .data
                    .tunnel_status
                    .get(&id)
                    .is_some_and(|s| s.can_stop())
                {
                    return;
                }
                E::StartTunnel(id)
            }
            A::Open { target, open_sftp } => E::Connect { target, open_sftp },
            A::Edit(target) => E::Edit(target),
            A::Duplicate(id) => E::Duplicate(id),
            A::Test(id) => E::Test(id),
            A::NewConnectionInGroup(group) => E::CreateConnection {
                group: (!group.is_empty()).then_some(group),
            },
            A::OpenSshConfigImporter => E::OpenSshConfigImporter,
            A::ImportSshConfig(alias) => E::ImportSshConfig(alias),
            A::OpenImporters => E::OpenImporters,
            A::ShowBatchAdd => E::BatchAdd,
            A::StopTunnel(id) => E::StopTunnel(id),
            A::StartTunnels(ids) => {
                for id in ids {
                    if !self
                        .data
                        .tunnel_status
                        .get(&id)
                        .is_some_and(|s| s.can_stop())
                    {
                        cx.emit(E::StartTunnel(id));
                    }
                }
                return;
            }
            A::Move { targets, group } => E::MoveToGroup { targets, group },
            A::MoveSelected(group) => {
                let targets = self
                    .manager
                    .read(cx)
                    .selected_items
                    .iter()
                    .cloned()
                    .collect();
                self.manager.update(cx, |s, cx| {
                    s.show_batch_move = false;
                    cx.notify();
                });
                E::MoveToGroup { targets, group }
            }
            A::SaveGroup => {
                let state = self.manager.read(cx);
                if !state.can_save_group() {
                    return;
                }
                let Some(editor) = &state.group_editor else {
                    return;
                };
                let (parent, old) = match editor {
                    SessionManagerGroupEditor::Create { parent_path } => (parent_path, None),
                    SessionManagerGroupEditor::Rename {
                        old_path,
                        parent_path,
                    } => (parent_path, Some(old_path.clone())),
                };
                let Some(new) =
                    session_group_path_from_leaf(parent.as_deref(), &state.group_name_draft)
                else {
                    return;
                };
                self.manager
                    .update(cx, SessionManagerState::cancel_group_editor);
                match old {
                    Some(old) => E::RenameGroup { old, new },
                    None => E::CreateGroup(new),
                }
            }
            A::Delete(row) => {
                let name = row
                    .open_target()
                    .and_then(|target| self.data.items.iter().find(|item| item.target == target))
                    .map(|item| item.name.clone());
                self.manager
                    .update(cx, |s, cx| s.request_display_delete(row, name, cx));
                return;
            }
            A::ConfirmDelete => {
                let deletion = self
                    .manager
                    .update(cx, SessionManagerState::take_catalog_deletion);
                match deletion {
                    Some(CatalogDeletion::Group(name)) => {
                        self.manager.update(cx, |state, cx| {
                            state.show_group_manager =
                                std::mem::take(&mut state.reopen_group_manager_after_delete);
                            cx.notify();
                        });
                        E::DeleteGroup(name)
                    }
                    Some(CatalogDeletion::Single(target)) => E::Delete(vec![target]),
                    Some(CatalogDeletion::Batch(targets)) => E::Delete(targets),
                    None => return,
                }
            }
            // These transitions are completed by the shared view before dispatch.
            A::CloseSidebarSort
            | A::NewGroup
            | A::NewSubgroup(_)
            | A::RenameGroup(_)
            | A::CloseGroupManager
            | A::CancelGroupEditor
            | A::DeleteSelected
            | A::CancelDelete => return,
        };
        cx.emit(event);
    }
}

impl Render for PortableCatalogView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.clone();
        let owner = cx.entity();
        div()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .capture_any_mouse_down(cx.listener(|view, _, window, cx| {
                window.focus(&view.focus, cx);
                view.manager
                    .update(cx, SessionManagerState::clear_input_focus);
            }))
            .on_key_down(cx.listener(Self::key))
            .child(self.body.clone())
            .child(self.dialogs.clone())
            .children(self.tooltip.as_ref().map(|(_, label, x, y)| {
                gpui::deferred(
                    gpui::anchored()
                        .position(gpui::point(gpui::px(*x), gpui::px(*y)))
                        .position_mode(gpui::AnchoredPositionMode::Window)
                        .child(oxideterm_gpui_ui::tooltip::tooltip_view(
                            self.environment.tokens,
                            label.clone(),
                            None,
                            cx,
                        )),
                )
            }))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            gpui::ElementInputHandler::new(bounds, owner),
                            cx,
                        )
                    },
                )
                .absolute()
                .inset_0(),
            )
    }
}
