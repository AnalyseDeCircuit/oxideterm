// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use gpui::{
    AnchoredPositionMode, AnyElement, App, Context, Corner, Div, Entity, MouseButton,
    MouseDownEvent, MouseMoveEvent, Point, Render, Rgba, ScrollHandle, SharedString, Subscription,
    Window, anchored, deferred, div, prelude::*, px, rgb, rgba,
};
use oxideterm_forwarding::ForwardStatus;
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    checkbox,
    context_menu::{
        ContextMenuActionableStyle, context_menu_action, context_menu_actionable_row,
        context_menu_event_boundary,
    },
    dropdown_menu::{dropdown_menu_content, dropdown_menu_separator},
    icons::LucideIcon,
    modal::overlay_content_boundary,
    select::{OverlayAnchor, SelectAnchorId},
    text_input::{TextInputView, text_input_anchor_probe, text_input_with_viewport},
    virtual_list::{
        TauriVirtualListSpec, sync_tauri_variable_list_state_by_signatures, tauri_virtual_list,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use super::*;
use crate::views::{
    ConnectionsDispatch, ConnectionsEnvironmentSource, ConnectionsWindowRequest as Request,
};

mod group_dialogs;
mod rows;
mod surface;

#[derive(Clone, Default)]
pub struct CatalogEnvironment {
    pub items: Vec<SessionManagerDisplayItem>,
    pub groups: (Vec<String>, HashMap<String, Vec<String>>),
    pub group_counts: HashMap<String, usize>,
    pub tunnel_status: HashMap<String, ForwardStatus>,
    pub horizontal_chrome_width: f32,
    pub chrome_divider: Rgba,
    pub has_background: bool,
    pub ui_font: SharedString,
    pub sidebar_background: Rgba,
    pub sidebar_sort_anchor: Option<OverlayAnchor>,
    pub stored_groups: Vec<String>,
}

impl CatalogEnvironment {
    pub fn from_store(
        manager: &SessionManagerState,
        store: &oxideterm_connections::ConnectionStore,
        tunnels: Vec<oxideterm_forwarding::PersistedForward>,
        include_items: bool,
    ) -> Self {
        let group_counts = if include_items {
            super::groups::connection_counts_by_group(store, &tunnels)
        } else {
            HashMap::new()
        };
        let groups = session_group_tree(store, &tunnels);
        Self {
            items: if include_items {
                manager.display_catalog(store, tunnels)
            } else {
                Vec::new()
            },
            group_counts,
            groups,
            stored_groups: store.groups().to_vec(),
            ..Self::default()
        }
    }
}

#[derive(Clone)]
pub enum CatalogAction {
    Open {
        target: SessionManagerOpenTarget,
        open_sftp: bool,
    },
    Edit(SessionManagerOpenTarget),
    Delete(SessionManagerRowActionTarget),
    DeleteSelected,
    Duplicate(String),
    Test(String),
    ImportSshConfig(String),
    OpenSshConfigImporter,
    OpenImporters,
    NewConnectionInGroup(String),
    NewGroup,
    NewSubgroup(String),
    RenameGroup(String),
    Move {
        targets: Vec<SessionManagerSelectionTarget>,
        group: Option<String>,
    },
    StopTunnel(String),
    StartTunnels(Vec<String>),
    ShowBatchAdd,
    MoveSelected(Option<String>),
    CloseSidebarSort,
    CloseGroupManager,
    CancelGroupEditor,
    SaveGroup,
    CancelDelete,
    ConfirmDelete,
}

#[derive(Clone, Copy)]
pub(crate) enum CatalogMount {
    Main,
    Rows,
    Dialogs,
    SidebarSearch,
    SidebarSearchButton,
}

pub(crate) struct CatalogView {
    mount: CatalogMount,
    session_manager: Entity<SessionManagerState>,
    _observation: Subscription,
    tokens: ThemeTokens,
    i18n: I18n,
    theme: ConnectionsEnvironmentSource,
    query: Rc<dyn Fn(bool, &App) -> Option<CatalogEnvironment>>,
    dispatch: ConnectionsDispatch,
    environment: CatalogEnvironment,
    anchors: RefCell<HashMap<SelectAnchorId, OverlayAnchor>>,
    batch_move_scroll: ScrollHandle,
    group_scroll: ScrollHandle,
    ime: Rc<dyn Fn(SessionManagerInput, &App) -> Option<crate::views::ConnectionImeFrame>>,
}

impl CatalogView {
    pub(crate) fn new(
        mount: CatalogMount,
        manager: Entity<SessionManagerState>,
        theme: ConnectionsEnvironmentSource,
        bindings: &SessionManagerViewBindings,
        dispatch: ConnectionsDispatch,
        cx: &mut Context<Self>,
    ) -> Self {
        let observation = cx.observe(&manager, |_, _, cx| cx.notify());
        Self {
            mount,
            session_manager: manager,
            _observation: observation,
            tokens: oxideterm_theme::default_tokens(),
            i18n: I18n::new(oxideterm_i18n::Locale::En),
            theme,
            query: bindings.catalog.clone(),
            dispatch,
            environment: Default::default(),
            anchors: RefCell::new(HashMap::new()),
            batch_move_scroll: ScrollHandle::new(),
            group_scroll: ScrollHandle::new(),
            ime: bindings.ime.clone(),
        }
    }

    fn action(&self, action: CatalogAction, window: Option<&mut Window>, cx: &mut App) {
        use CatalogAction as Action;
        let reset_composition = match action {
            Action::NewGroup | Action::NewSubgroup(_) => {
                let parent_path = match action {
                    Action::NewSubgroup(group) => Some(group),
                    _ => None,
                };
                self.session_manager.update(cx, |manager, cx| {
                    manager.open_group_editor(
                        SessionManagerGroupEditor::Create { parent_path },
                        String::new(),
                        cx,
                    );
                });
                true
            }
            Action::RenameGroup(group) => {
                self.session_manager
                    .update(cx, |manager, cx| manager.rename_group_editor(group, cx));
                true
            }
            Action::CloseGroupManager => {
                self.session_manager
                    .update(cx, SessionManagerState::close_session_group_manager);
                true
            }
            Action::CancelGroupEditor => {
                self.session_manager
                    .update(cx, SessionManagerState::cancel_group_editor);
                true
            }
            Action::Delete(SessionManagerRowActionTarget::Group(group)) => {
                self.session_manager.update(cx, |manager, cx| {
                    manager.request_delete_session_group(group, cx)
                });
                false
            }
            Action::DeleteSelected => {
                self.session_manager
                    .update(cx, SessionManagerState::request_selected_delete);
                false
            }
            Action::CancelDelete => {
                self.session_manager
                    .update(cx, SessionManagerState::cancel_delete);
                false
            }
            action => return (self.dispatch)(Request::Catalog(action), window, cx),
        };
        if reset_composition {
            (self.dispatch)(Request::ClearComposition, None, cx);
        }
    }

    fn open_session_manager_target_with_sftp(
        &self,
        target: SessionManagerOpenTarget,
        open_sftp: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.action(CatalogAction::Open { target, open_sftp }, Some(window), cx);
    }

    fn move_dragged_sessions_to_group(
        &self,
        targets: &[SessionManagerSelectionTarget],
        group: Option<&str>,
        cx: &mut App,
    ) {
        self.action(
            CatalogAction::Move {
                targets: targets.to_vec(),
                group: group.map(str::to_owned),
            },
            None,
            cx,
        );
    }

    fn render_sidebar_empty_state(
        &self,
        icon: LucideIcon,
        title_id: &'static str,
        title: String,
        _subtitle: Option<(&'static str, String)>,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let metrics = self.tokens.metrics;
        let color = self.tokens.ui.text_muted;
        div()
            .flex_1()
            .w_full()
            .px(px(metrics.empty_sidebar_padding_x))
            .child(
                div()
                    .w_full()
                    .h(px(metrics.empty_sidebar_height))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(div().mb_3().child(Self::render_lucide_icon(
                        icon,
                        metrics.empty_sidebar_icon_size,
                        rgba((color << 8) | 0x4d),
                    )))
                    .child(
                        div()
                            .id(title_id)
                            .w_full()
                            .text_center()
                            .text_size(px(metrics.empty_sidebar_title_font_size))
                            .text_color(rgb(color))
                            .child(title),
                    ),
            )
            .into_any_element()
    }

    fn session_group_tree(&self) -> (Vec<String>, HashMap<String, Vec<String>>) {
        self.environment.groups.clone()
    }

    fn connection_count_for_group(&self, group: &str) -> usize {
        self.environment
            .group_counts
            .get(group)
            .copied()
            .unwrap_or_default()
    }

    fn ssh_tunnel_status(&self, id: &str) -> ForwardStatus {
        self.environment
            .tunnel_status
            .get(id)
            .cloned()
            .unwrap_or(ForwardStatus::Stopped)
    }

    fn workspace_chrome_divider(&self) -> Rgba {
        self.environment.chrome_divider
    }

    fn session_tree_node_height(&self) -> f32 {
        (28.0_f32).max((self.tokens.metrics.ui_text_xs + 16.0).round())
    }

    fn render_session_tree_child_with_rails(
        &self,
        depth: usize,
        ancestor_continues: &[bool],
        line_stops_here: bool,
        child: AnyElement,
    ) -> AnyElement {
        oxideterm_gpui_ui::tree::tree_guide_child(
            &self.tokens,
            oxideterm_gpui_ui::tree::TreeGuideMetrics::session_tree(),
            depth,
            ancestor_continues,
            line_stops_here,
            child,
        )
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        gpui::svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn render_animated_chevron(
        &self,
        id: impl Into<gpui::ElementId>,
        expanded: bool,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        oxideterm_gpui_ui::motion::animated_chevron(
            &self.tokens,
            id,
            gpui::svg()
                .path(LucideIcon::ChevronRight.path())
                .size(px(size))
                .text_color(color),
            expanded,
        )
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        oxideterm_gpui_ui::button::toolbar_action_button(
            &self.tokens,
            label,
            icon,
            options,
            listener,
        )
    }

    fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let enabled = !(options.disabled || options.loading);
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .when(enabled, |button| {
            button.on_mouse_down(MouseButton::Left, cx.listener(listener))
        })
        .when(!enabled, |button| {
            button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        })
    }

    fn workspace_tooltip_icon_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        tooltip: String,
        id: &'static str,
        flex_none: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _entity: Entity<Self>,
    ) -> AnyElement {
        let enabled = !(options.disabled || options.loading);
        let dispatch = self.dispatch.clone();
        let clear = self.dispatch.clone();
        let tooltip_id = tooltip.clone();
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .id((gpui::ElementId::from(id), tooltip_id))
        .on_mouse_move({
            let tooltip = tooltip.clone();
            move |event: &MouseMoveEvent, _, cx| {
                dispatch(
                    Request::QueueTooltip {
                        id: tooltip.clone(),
                        label: tooltip.clone(),
                        x: f32::from(event.position.x) + 12.,
                        y: f32::from(event.position.y) + 16.,
                    },
                    None,
                    cx,
                )
            }
        })
        .on_hover(move |hovered, _, cx| {
            if !*hovered {
                clear(Request::ClearTooltip(tooltip.clone()), None, cx);
            }
        })
        .when(enabled, |button| {
            button.on_mouse_down(MouseButton::Left, listener)
        })
        .when(!enabled, |button| {
            button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        })
        .when(flex_none, |button| button.flex_none())
        .into_any_element()
    }

    fn queue_workspace_tooltip(&self, id: &str, label: String, x: f32, y: f32, cx: &mut App) {
        (self.dispatch)(
            Request::QueueTooltip {
                id: id.to_owned(),
                label,
                x,
                y,
            },
            None,
            cx,
        );
    }

    fn clear_workspace_tooltip(&self, id: &str, cx: &mut App) {
        (self.dispatch)(Request::ClearTooltip(id.to_owned()), None, cx);
    }

    fn workspace_context_menu_styled_action(
        &self,
        item: Div,
        disabled: bool,
        loading: bool,
        style: ContextMenuActionableStyle,
        close: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let item = context_menu_actionable_row(item, disabled, loading, style);
        context_menu_action(
            item,
            disabled,
            loading,
            cx.listener(move |this, event, window, cx| {
                close(this);
                listener(this, event, window, cx);
            }),
        )
    }

    fn close_session_row_menus(&self, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            if close_session_menu_state(manager) {
                cx.notify();
            }
        });
    }

    fn context_menu_backdrop(&self, menu: AnyElement, cx: &Context<Self>) -> Div {
        oxideterm_gpui_ui::modal::popover_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.close_session_row_menus(cx);
                    this.action(CatalogAction::CloseSidebarSort, None, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.close_session_row_menus(cx);
                    this.action(CatalogAction::CloseSidebarSort, None, cx);
                    cx.stop_propagation();
                }),
            )
            .child(menu)
    }

    fn select_anchor(&self, id: SelectAnchorId) -> Option<OverlayAnchor> {
        if id == SelectAnchorId::SessionManagerSidebarSort {
            self.environment.sidebar_sort_anchor
        } else {
            self.anchors.borrow().get(&id).copied()
        }
    }

    fn update_select_anchor(&self, anchor: OverlayAnchor, cx: &mut Context<Self>) {
        let changed = self.anchors.borrow_mut().insert(anchor.id, anchor) != Some(anchor);
        if changed {
            let manager = self.session_manager.read(cx);
            if manager.view_mode_menu_open || manager.sort_menu_open || manager.show_batch_move {
                cx.notify();
            }
        }
    }

    fn render_session_text_input(
        &self,
        input: SessionManagerInput,
        value: &str,
        placeholder: String,
        cx: &App,
    ) -> AnyElement {
        let Some(frame) = (self.ime)(input, cx) else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let focused = self.session_manager.read(cx).focused_input == Some(input);
        let field = text_input_with_viewport(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused,
                caret_visible: frame.caret_visible,
                secret: false,
                selected_all: false,
                selected_range: frame.selected_range,
                marked_text: frame.marked_text.as_ref().map(|s| s.as_str()),
            },
            &frame.viewport,
            frame.active_offset,
        )
        .flex_1()
        .min_w_0()
        .h_full()
        .border_0()
        .px(px(0.))
        .bg(rgba(0));
        let focus = self.dispatch.clone();
        let drag = self.dispatch.clone();
        let search = input == SessionManagerInput::Search;
        text_input_anchor_probe(
            frame.anchor_id,
            div()
                .id(("session-manager-input", input.anchor_key()))
                .h(px(if search { 36. } else { 32. }))
                .w_full()
                .px_3()
                .flex()
                .items_center()
                .when(search, |input| {
                    input
                        .border_b_1()
                        .bg(if matches!(self.mount, CatalogMount::SidebarSearch) {
                            self.environment.sidebar_background
                        } else {
                            rgba(0).into()
                        })
                })
                .when(!search, |input| {
                    input
                        .border_1()
                        .rounded(px(self.tokens.radii.md))
                        .bg(theme_input_bg(theme.bg, self.environment.has_background))
                })
                .border_color(if focused {
                    rgb(theme.accent)
                } else if search {
                    self.workspace_chrome_divider().into()
                } else {
                    theme_border_half(theme.border, self.environment.has_background)
                })
                .child(field)
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    focus(
                        Request::FocusSessionInput(input, event.clone()),
                        Some(window),
                        cx,
                    );
                    cx.stop_propagation();
                })
                .on_mouse_move(move |event, window, cx| {
                    drag(Request::DragInputSelection(event.clone()), Some(window), cx)
                }),
            move |anchor, _, _| (frame.update_anchor)(anchor),
        )
        .into_any_element()
    }

    fn open_menu(
        &self,
        target: SessionManagerRowActionTarget,
        origin: SessionManagerRowActionMenuOrigin,
        x: f32,
        y: f32,
        cx: &mut App,
    ) {
        self.session_manager.update(cx, |manager, cx| {
            close_session_menu_state(manager);
            manager.row_action_menu = Some(SessionManagerRowActionMenu {
                target,
                origin,
                x,
                y,
            });
            cx.notify();
        });
    }

    fn open_session_manager_row_action_menu(
        &self,
        target: SessionManagerRowActionTarget,
        x: f32,
        y: f32,
        cx: &mut App,
    ) {
        self.open_menu(
            target,
            SessionManagerRowActionMenuOrigin::ActionButton,
            x,
            y,
            cx,
        );
    }

    fn open_session_manager_context_menu(
        &self,
        target: SessionManagerRowActionTarget,
        x: f32,
        y: f32,
        cx: &mut App,
    ) {
        self.open_menu(target, SessionManagerRowActionMenuOrigin::Pointer, x, y, cx);
    }

    fn apply_session_manager_selection(
        &self,
        target: SessionManagerSelectionTarget,
        mode: RowSelectionMode,
        cx: &mut App,
    ) {
        self.session_manager.update(cx, |manager, cx| {
            manager.apply_session_manager_selection(target, mode, cx)
        });
    }

    fn clear_session_manager_selection(&self, cx: &mut App) {
        self.session_manager
            .update(cx, SessionManagerState::clear_session_manager_selection);
    }

    fn toggle_session_selection(&self, target: SessionManagerSelectionTarget, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            manager.toggle_session_selection(target, cx)
        });
    }

    fn toggle_session_group_expanded(&self, group: &str, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            manager.toggle_session_group_expanded(group, cx)
        });
    }

    fn set_session_manager_visible_selection_targets(
        &self,
        targets: Vec<SessionManagerSelectionTarget>,
        cx: &mut App,
    ) {
        self.session_manager.update(cx, |manager, _| {
            manager.set_session_manager_visible_selection_targets(targets)
        });
    }
}

impl CatalogView {
    fn render_sidebar_search_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.session_manager.read(cx).sidebar_search_open;
        let theme = self.tokens.ui;
        self.workspace_tooltip_icon_button(
            LucideIcon::Search,
            self.tokens.metrics.sidebar_action_icon_size,
            rgb(if open { theme.text } else { theme.text_muted }),
            IconButtonOptions {
                has_background: open,
                background: open.then_some(rgb(theme.bg_active)),
                hover_background: Some(rgb(theme.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    self.tokens.metrics.sidebar_action_size,
                    ButtonRadius::Md,
                )
            },
            self.i18n.t("sidebar.search.title"),
            "sidebar-session-manager-search",
            false,
            cx.listener(|this, _event, window, cx| {
                (this.dispatch)(Request::ToggleCatalogSidebarSearch, Some(window), cx);
            }),
            cx.entity(),
        )
    }
}

impl Render for CatalogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(theme) = (self.theme)(cx) else {
            return div().into_any_element();
        };
        self.tokens = theme.tokens;
        self.i18n = theme.i18n;
        if matches!(self.mount, CatalogMount::SidebarSearchButton) {
            return self.render_sidebar_search_button(cx);
        }
        let (groups, delete) = {
            let state = self.session_manager.read(cx);
            (state.show_group_manager, state.delete_confirm.is_some())
        };
        if matches!(self.mount, CatalogMount::Dialogs) && !groups && !delete {
            return div().into_any_element();
        }
        let include_items = matches!(self.mount, CatalogMount::Main | CatalogMount::Rows);
        let Some(environment) = (self.query)(include_items, cx) else {
            return div().into_any_element();
        };
        self.environment = environment;
        match self.mount {
            CatalogMount::Main => self.render_session_manager_surface(window, cx),
            CatalogMount::Rows => self.render_session_manager_sidebar_surface(window, cx),
            CatalogMount::Dialogs => div()
                .absolute()
                .inset_0()
                .children(groups.then(|| self.render_group_manager_dialog(window, cx)))
                .children(delete.then(|| self.render_session_manager_delete_confirm(cx)))
                .into_any_element(),
            CatalogMount::SidebarSearchButton => unreachable!(),
            CatalogMount::SidebarSearch => self.render_session_text_input(
                SessionManagerInput::Search,
                &self.session_manager.read(cx).search_query,
                self.i18n.t("sessionManager.toolbar.search_placeholder"),
                cx,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::ConnectionsEnvironment;
    use gpui::{AppContext, TestAppContext};
    use oxideterm_connections::{ConnectionStore, ConnectionStoreData, SerialProfile};

    #[gpui::test]
    fn catalog_renders_filtered_rows_without_a_workspace(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        let mut lab = SerialProfile::new("Lab console", "/dev/tty.lab");
        lab.id = "lab".into();
        let mut workshop = SerialProfile::new("Workshop console", "/dev/tty.shop");
        workshop.id = "workshop".into();
        std::fs::write(
            &path,
            serde_json::to_vec(&ConnectionStoreData {
                serial_profiles: vec![lab, workshop],
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        let store = ConnectionStore::load_read_only(path).unwrap();

        let manager = cx.new(|_| {
            let mut manager = SessionManagerState::default();
            manager.view_mode = SessionManagerViewMode::List;
            manager
        });
        let state = manager.clone();
        let bindings = SessionManagerViewBindings {
            catalog: Rc::new(move |_, cx| {
                Some(CatalogEnvironment {
                    items: state.read(cx).display_catalog(&store, vec![]),
                    ui_font: "sans-serif".into(),
                    ..Default::default()
                })
            }),
            oxide_environment: Rc::new(|_| None),
            ime: Rc::new(|_, _| None),
            text: Rc::new(|_, _, _, value, _, _, _| div().child(value).into_any_element()),
        };
        let (_, cx) = cx.add_window_view(|_, cx| {
            CatalogView::new(
                CatalogMount::Main,
                manager.clone(),
                Rc::new(|_| {
                    Some(ConnectionsEnvironment {
                        tokens: oxideterm_theme::default_tokens(),
                        i18n: I18n::new(oxideterm_i18n::Locale::En),
                    })
                }),
                &bindings,
                Rc::new(|_, _, _| {}),
                cx,
            )
        });
        cx.simulate_resize(gpui::size(px(1000.), px(720.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("session-manager-row-lab").is_some());
        assert!(cx.debug_bounds("session-manager-row-workshop").is_some());

        manager.update(cx, |manager, cx| {
            manager.replace_input(SessionManagerInput::Search, None, "lab", cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("session-manager-row-lab").is_some());
        assert!(cx.debug_bounds("session-manager-row-workshop").is_none());
    }
}

struct MenuPosition {
    x: f32,
    y: f32,
}

fn catalog_menu_position(
    x: f32,
    y: f32,
    viewport_width: f32,
    viewport_height: f32,
    width: f32,
    height: f32,
    margin: f32,
) -> MenuPosition {
    MenuPosition {
        x: x.min(viewport_width - width - margin).max(margin),
        y: y.min(viewport_height - height - margin).max(margin),
    }
}
