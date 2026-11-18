use super::*;
use gpui::StatefulInteractiveElement;
use oxideterm_gpui_ui::{
    MaterialRole,
    button::ToolbarButtonIconPosition,
    dropdown_menu::{DropdownMenuItemKind, dropdown_menu_item},
    material_surface,
    select::select_anchor_probe,
};

const MANAGER_TOOLBAR_SEARCH_WIDTH: f32 = 384.0; // Tauri max-w-sm
const MANAGER_VIEW_MODE_MENU_WIDTH: f32 = 168.0; // Tauri DropdownMenuContent min-w-[160px] plus native menu padding.
const MANAGER_VIEW_MODE_MENU_HEIGHT: f32 = 104.0; // Three compact radio rows plus menu padding.
const MANAGER_SORT_MENU_WIDTH: f32 = 184.0; // Sort fields reuse the compact toolbar dropdown rhythm.
const MANAGER_SORT_MENU_HEIGHT: f32 = 220.0; // Seven compact radio rows plus menu padding.
const MANAGER_BATCH_MOVE_MENU_WIDTH: f32 = 220.0; // Tauri batch move DropdownMenuContent natural width.
const MANAGER_BATCH_MOVE_MENU_HEIGHT: f32 = 260.0; // Keeps long group lists scrollable without covering the viewport.
const MANAGER_RESPONSIVE_SM: f32 = 640.0;
const MANAGER_RESPONSIVE_MD: f32 = 768.0;

impl CatalogView {
    pub(super) fn render_session_manager_surface(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.environment.has_background;
        let (
            view_mode,
            status,
            row_action_menu,
            view_mode_menu_open,
            sort_menu_open,
            show_batch_move,
            has_selection,
        ) = {
            let session_manager = self.session_manager.read(cx);
            (
                session_manager.view_mode,
                session_manager.status.clone(),
                (!session_manager.show_group_manager)
                    .then(|| session_manager.row_action_menu.clone())
                    .flatten(),
                session_manager.view_mode_menu_open,
                session_manager.sort_menu_open,
                session_manager.show_batch_move,
                !session_manager.selected_items.is_empty(),
            )
        };
        let content = self.render_session_manager_view_content(window, has_background, cx);
        let content = oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            ("session-manager-view", view_mode as usize),
            div().size_full().child(content),
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            // Session Manager is ordinary UI chrome, so it must explicitly
            // follow the configured Tauri UI font instead of inheriting a
            // terminal/mono font from surrounding tab content.
            .font_family(self.environment.ui_font.clone())
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text))
            .child(self.render_session_manager_toolbar(window, has_background, cx))
            .child(div().flex_1().min_h(px(0.0)).min_w(px(0.0)).child(content))
            .when_some(status, |surface, status| {
                surface.child(
                    div()
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .px_4()
                        .border_t_1()
                        .border_color(theme_border(theme.border, has_background))
                        .bg(theme_bg(theme.bg, has_background))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.accent))
                        .child(status),
                )
            })
            .when_some(row_action_menu, |surface, menu| {
                let menu =
                    self.render_session_manager_row_action_menu(menu, window, has_background, cx);
                let backdrop = self.context_menu_backdrop(menu, cx).on_scroll(cx.listener(
                    |this, _event, _window, cx| {
                        // Pointer-positioned menus are stale as soon as their list scrolls.
                        this.close_session_row_menus(cx);
                        cx.stop_propagation();
                    },
                ));
                surface.child(backdrop)
            })
            .when(view_mode_menu_open, |surface| {
                surface.child(self.context_menu_backdrop(
                    self.render_session_manager_view_mode_menu(window, has_background, cx),
                    cx,
                ))
            })
            .when(sort_menu_open, |surface| {
                surface.child(self.context_menu_backdrop(
                    self.render_session_manager_sort_menu(
                        window,
                        has_background,
                        SelectAnchorId::SessionManagerSort,
                        false,
                        cx,
                    ),
                    cx,
                ))
            })
            .when(has_selection && show_batch_move, |surface| {
                surface.child(
                    self.context_menu_backdrop(self.render_batch_move_popover(window, cx), cx),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_session_manager_toolbar(
        &self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (selected_count, search_query) = {
            let manager = self.session_manager.read(cx);
            (manager.selected_items.len(), manager.search_query.clone())
        };
        let selected_tunnels = self
            .session_manager
            .read(cx)
            .selected_items
            .iter()
            .map(|target| match target {
                SessionManagerSelectionTarget::Tunnel(id) => Some(id.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        let viewport_width = f32::from(window.viewport_size().width);
        let show_primary_labels = viewport_width >= MANAGER_RESPONSIVE_SM;
        let show_secondary_labels = viewport_width >= MANAGER_RESPONSIVE_MD;
        let workspace = cx.entity();
        div()
            .min_h(px(48.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(self.workspace_chrome_divider())
            .bg(theme_bg(theme.bg, has_background))
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.0))
                    .max_w(px(MANAGER_TOOLBAR_SEARCH_WIDTH))
                    .child(self.render_session_text_input(
                        SessionManagerInput::Search,
                        &search_query,
                        self.i18n.t("sessionManager.toolbar.search_placeholder"),
                        cx,
                    )),
            )
            .child(
                self.render_toolbar_button(
                    LucideIcon::Plus,
                    self.i18n.t("sessionManager.toolbar.new_connection"),
                    ButtonVariant::Ghost,
                    has_background,
                    show_primary_labels,
                    cx.listener(|this, _event, window, cx| {
                        this.action(
                            CatalogAction::NewConnectionInGroup(String::new()),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .flex_none(),
            )
            .child(
                self.render_toolbar_button(
                    LucideIcon::Layers,
                    self.i18n.t("sessionManager.batchAdd.button"),
                    ButtonVariant::Ghost,
                    has_background,
                    show_primary_labels,
                    cx.listener(|this, _event, _window, cx| {
                        this.action(CatalogAction::ShowBatchAdd, None, cx);
                        cx.stop_propagation();
                    }),
                )
                .flex_none(),
            )
            .child(
                div()
                    .flex_none()
                    .border_l_1()
                    .border_color(self.workspace_chrome_divider())
                    .pl(px(8.0))
                    .child(self.render_session_manager_sort_trigger(
                        has_background,
                        show_secondary_labels,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .border_l_1()
                    .border_color(self.workspace_chrome_divider())
                    .pl(px(8.0))
                    .child(self.render_session_manager_view_mode_trigger(
                        has_background,
                        show_secondary_labels,
                        cx,
                    )),
            )
            .when(selected_count > 0, |toolbar| {
                toolbar.child(
                    // Match Tauri's selected-action island: it must not live
                    // inside the spacer, or GPUI can shrink the spacer to zero
                    // and let selected buttons paint over later toolbar items.
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_none()
                                .px_1()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(selected_count_label(&self.i18n, selected_count)),
                        )
                        .when_some(
                            selected_tunnels.filter(|ids| {
                                ids.iter().any(|id| {
                                    !matches!(
                                        self.ssh_tunnel_status(id),
                                        oxideterm_forwarding::ForwardStatus::Active
                                            | oxideterm_forwarding::ForwardStatus::Starting
                                            | oxideterm_forwarding::ForwardStatus::Suspended
                                    )
                                })
                            }),
                            |actions, ids| {
                                actions.child(self.render_session_manager_button(
                                    LucideIcon::Play,
                                    self.i18n.t("ssh_tunnel.start_selected"),
                                    ButtonVariant::Ghost,
                                    cx.listener(move |this, _event, window, cx| {
                                        this.action(
                                            CatalogAction::StartTunnels(ids.clone()),
                                            Some(window),
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                ))
                            },
                        )
                        .child(select_anchor_probe(
                            SelectAnchorId::SessionManagerBatchMove,
                            self.render_session_manager_button(
                                LucideIcon::FolderInput,
                                self.i18n.t("sessionManager.batch.move_to_group"),
                                ButtonVariant::Ghost,
                                cx.listener(|this, _event, _window, cx| {
                                    this.session_manager.update(cx, |manager, cx| {
                                        manager.show_batch_move = !manager.show_batch_move;
                                        cx.notify();
                                    });
                                    cx.stop_propagation();
                                }),
                            )
                            .flex_none(),
                            move |anchor, _window, cx| {
                                let _ = workspace.update(cx, |this, cx| {
                                    this.update_select_anchor(anchor, cx);
                                });
                            },
                        ))
                        .child(
                            self.render_session_manager_button(
                                LucideIcon::Trash2,
                                self.i18n.t("sessionManager.batch.delete"),
                                ButtonVariant::Ghost,
                                cx.listener(|this, _event, _window, cx| {
                                    this.action(CatalogAction::DeleteSelected, None, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .flex_none(),
                        ),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_session_manager_view_mode_trigger(
        &self,
        has_background: bool,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (current_mode, menu_open) = {
            let manager = self.session_manager.read(cx);
            (manager.view_mode, manager.view_mode_menu_open)
        };
        let workspace = cx.entity();
        let trigger = self.render_toolbar_button(
            current_mode.icon(),
            self.i18n.t(current_mode.label_key()),
            ButtonVariant::Ghost,
            has_background,
            show_label,
            cx.listener(move |this, _event, _window, cx| {
                this.toggle_session_view_mode_menu(cx);
                cx.notify();
                cx.stop_propagation();
            }),
        );

        // Keep the trigger rect warm while closed, matching Radix's immediate
        // trigger measurement and avoiding pointer-coordinate drift.
        select_anchor_probe(
            SelectAnchorId::SessionManagerViewMode,
            trigger.when(menu_open, |trigger| trigger.underline()),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_session_manager_sort_trigger(
        &self,
        has_background: bool,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (direction, sort_field, menu_open) = {
            let manager = self.session_manager.read(cx);
            (
                manager.sort_direction,
                manager.sort_field,
                manager.sort_menu_open,
            )
        };
        let label = self.i18n.t(sort_field.label_key());
        let workspace = cx.entity();
        let trigger = self.render_toolbar_button(
            direction.icon(),
            label,
            ButtonVariant::Ghost,
            has_background,
            show_label,
            cx.listener(move |this, _event, _window, cx| {
                this.toggle_session_sort_menu(cx);
                cx.notify();
                cx.stop_propagation();
            }),
        );

        select_anchor_probe(
            SelectAnchorId::SessionManagerSort,
            trigger.when(menu_open, |trigger| trigger.underline()),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_session_manager_view_mode_menu(
        &self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(anchor) = self.select_anchor(SelectAnchorId::SessionManagerViewMode) else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let placement = catalog_menu_position(
            f32::from(anchor.bounds.right()) - MANAGER_VIEW_MODE_MENU_WIDTH,
            f32::from(anchor.bounds.bottom()) + 4.0,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_VIEW_MODE_MENU_WIDTH,
            MANAGER_VIEW_MODE_MENU_HEIGHT,
            8.0,
        );
        let modes = [
            SessionManagerViewMode::Tree,
            SessionManagerViewMode::Grid,
            SessionManagerViewMode::List,
        ];
        let mut menu = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens).w(px(MANAGER_VIEW_MODE_MENU_WIDTH)),
        );
        let current_mode = self.session_manager.read(cx).view_mode;
        for mode in modes {
            let active = current_mode == mode;
            let item = dropdown_menu_item(
                &self.tokens,
                self.i18n.t(mode.label_key()),
                DropdownMenuItemKind::Radio(active),
                false,
                false,
            );
            menu = menu.child(self.render_session_manager_menu_action(
                item,
                false,
                false,
                has_background,
                move |this, _event, _window, cx| {
                    this.session_manager.update(cx, |manager, cx| {
                        manager.view_mode = mode;
                        cx.notify();
                    });
                    this.close_session_row_menus(cx);
                    cx.stop_propagation();
                },
                cx,
            ));
        }
        // The trigger probe reports window coordinates. Mount this like the
        // settings/AI popovers instead of as a surface-local absolute child.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                    &self.tokens,
                    "session-manager-view-menu-motion",
                    div().child(overlay_content_boundary(menu)),
                    -6.0,
                    gpui::point(0.5, 0.0),
                )),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(super) fn render_session_manager_sort_menu(
        &self,
        window: &Window,
        has_background: bool,
        anchor_id: SelectAnchorId,
        sidebar: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(anchor) = self.select_anchor(anchor_id) else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let placement = catalog_menu_position(
            f32::from(anchor.bounds.right()) - MANAGER_SORT_MENU_WIDTH,
            f32::from(anchor.bounds.bottom()) + 4.0,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_SORT_MENU_WIDTH,
            MANAGER_SORT_MENU_HEIGHT,
            8.0,
        );
        let fields = [
            SessionSortField::Name,
            SessionSortField::Host,
            SessionSortField::Port,
            SessionSortField::Username,
            SessionSortField::AuthType,
            SessionSortField::Group,
            SessionSortField::LastUsed,
        ];
        let mut menu = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens).w(px(MANAGER_SORT_MENU_WIDTH)),
        );
        let (current_field, current_direction) = {
            let manager = self.session_manager.read(cx);
            (manager.sort_field, manager.sort_direction)
        };
        for field in fields {
            let active = current_field == field;
            let mut label = self.i18n.t(field.label_key());
            if active {
                label.push_str(match current_direction {
                    SortDirection::Asc => " ↑",
                    SortDirection::Desc => " ↓",
                });
            }
            let item = dropdown_menu_item(
                &self.tokens,
                label,
                DropdownMenuItemKind::Radio(active),
                false,
                false,
            );
            menu = menu.child(self.render_session_manager_menu_action(
                item,
                false,
                false,
                has_background,
                move |this, _event, _window, cx| {
                    this.set_session_sort_field(field, cx);
                    // Sidebar uses its own open flag; the tab shares the
                    // entity's sort_menu_open with the row action menu.
                    if sidebar {
                        this.action(CatalogAction::CloseSidebarSort, None, cx);
                    } else {
                        this.close_session_row_menus(cx);
                    }
                    cx.notify();
                    cx.stop_propagation();
                },
                cx,
            ));
        }
        // The sort trigger is inside the tab content, while the popup floats
        // at window level just like Radix DropdownMenuContent in Tauri.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                    &self.tokens,
                    "session-manager-view-menu-motion",
                    div().child(overlay_content_boundary(menu)),
                    -6.0,
                    gpui::point(0.5, 0.0),
                )),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(super) fn render_batch_move_popover(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let groups = self.environment.stored_groups.clone();
        let Some(anchor) = self.select_anchor(SelectAnchorId::SessionManagerBatchMove) else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let placement = catalog_menu_position(
            f32::from(anchor.bounds.left()),
            f32::from(anchor.bounds.bottom()) + 4.0,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_BATCH_MOVE_MENU_WIDTH,
            MANAGER_BATCH_MOVE_MENU_HEIGHT,
            8.0,
        );
        let popup = material_surface(&self.tokens, div(), MaterialRole::Popover)
            .id("session-manager-batch-move-scroll")
            .w(px(MANAGER_BATCH_MOVE_MENU_WIDTH))
            .max_h(px(MANAGER_BATCH_MOVE_MENU_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.batch_move_scroll)
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .shadow_lg()
            .child(self.render_batch_move_item(
                None,
                self.i18n.t("sessionManager.folder_tree.ungrouped"),
                cx,
            ))
            .children(
                groups
                    .into_iter()
                    .map(|group| self.render_batch_move_item(Some(group.clone()), group, cx)),
            );

        // Batch move is a Radix dropdown in Tauri; keep it anchored to the
        // actual trigger instead of the old toolbar-relative hard-coded corner.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                    &self.tokens,
                    "session-manager-batch-move-motion",
                    div().child(overlay_content_boundary(popup)),
                    -6.0,
                    gpui::point(0.5, 0.0),
                )),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(super) fn render_batch_move_item(
        &self,
        group: Option<String>,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(34.0))
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .text_color(rgb(theme.text))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.action(CatalogAction::MoveSelected(group.clone()), None, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    /// Renders the session manager as a tree-only panel inside the sidebar body.
    /// Reuses the tab's tree row renderer, context menu, and sort menu, but keeps
    /// its own virtual list state so the tab and sidebar don't fight over scroll.
    pub(super) fn render_session_manager_sidebar_surface(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.environment.has_background;
        let items = std::mem::take(&mut self.environment.items);
        let body = self.render_session_manager_sidebar_tree(items, cx);
        let row_action_menu = self.session_manager.read(cx).row_action_menu.clone();
        let sort_menu_open = self.session_manager.read(cx).sidebar_sort_open;
        // Operation output shares the tab's status field; mirror the compact
        // sidebar footer chrome so status feedback matches Active Sessions.
        let status = self.session_manager.read(cx).status.clone();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().flex_1().min_h(px(0.0)).child(body))
            .when_some(status, |surface, status| {
                surface.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .h(px(32.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .px(px(self.tokens.spacing.three))
                        .border_t_1()
                        .border_color(self.workspace_chrome_divider())
                        .bg(self.environment.sidebar_background)
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.accent))
                        .child(status),
                )
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .when(sort_menu_open, |this| {
                        this.child(self.context_menu_backdrop(
                            self.render_session_manager_sort_menu(
                                window,
                                has_background,
                                SelectAnchorId::SessionManagerSidebarSort,
                                true,
                                cx,
                            ),
                            cx,
                        ))
                    })
                    .when_some(row_action_menu, |this, menu| {
                        this.child(self.context_menu_backdrop(
                            self.render_session_manager_row_action_menu(
                                menu,
                                window,
                                has_background,
                                cx,
                            ),
                            cx,
                        ))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_session_manager_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 14.0, rgb(theme.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                icon_position: ToolbarButtonIconPosition::Trailing,
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    pub(super) fn render_toolbar_button(
        &self,
        icon: LucideIcon,
        label: String,
        variant: ButtonVariant,
        has_background: bool,
        show_label: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        let icon_color = match variant {
            ButtonVariant::Default => rgb(theme.bg),
            _ => rgb(theme.text),
        };
        // Toolbar commands match Tauri Button chrome while sharing the native
        // disabled/loading action guard with other workspace toolbars.
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 16.0, icon_color)),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                has_background,
                show_label,
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    fn toggle_session_view_mode_menu(&self, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            let was_open = manager.view_mode_menu_open;
            close_session_menu_state(manager);
            manager.view_mode_menu_open = !was_open;
            cx.notify();
        });
    }

    fn toggle_session_sort_menu(&self, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            let was_open = manager.sort_menu_open;
            close_session_menu_state(manager);
            manager.sort_menu_open = !was_open;
            cx.notify();
        });
    }

    fn set_session_sort_field(&self, field: SessionSortField, cx: &mut App) {
        self.session_manager.update(cx, |manager, cx| {
            if manager.sort_field == field {
                manager.sort_direction = manager.sort_direction.toggled();
            } else {
                manager.sort_field = field;
                manager.sort_direction = field.default_direction();
            }
            cx.notify();
        });
    }
}
