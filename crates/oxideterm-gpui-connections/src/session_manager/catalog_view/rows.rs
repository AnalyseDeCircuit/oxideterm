use super::*;
use gpui::StatefulInteractiveElement;

const MANAGER_ROW_META_TEXT_SIZE: f32 = 12.0;
const MANAGER_TABLE_HEADER_TEXT_SIZE: f32 = 12.0;
pub(super) const MANAGER_ROW_ACTION_BUTTON: f32 = 24.0; // Tauri h-6 w-6
pub(super) const MANAGER_ROW_ACTION_ICON_SIZE: f32 = 12.0;
const MANAGER_ROW_ACTION_GAP: f32 = 2.0;
const MANAGER_ROW_ICON_SIZE: f32 = 40.0;
const MANAGER_SIDEBAR_ROW_ICON_SIZE: f32 = 24.0;
const MANAGER_SIDEBAR_ROW_ICON_GLYPH_SIZE: f32 = 14.0;
const MANAGER_DRAG_ROOT_BG_ALPHA: u32 = 0x0d;
const MANAGER_DRAG_GROUP_BG_ALPHA: u32 = 0x16;
const MANAGER_LIST_LAST_USED_WIDTH: f32 = 136.0;
const MANAGER_ROW_ACTIONS_WIDTH: f32 = 84.0; // Three compact actions and their two gaps.
const MANAGER_RECENT_ITEM_MIN_WIDTH: f32 = 180.0;
const MANAGER_RECENT_ITEM_BASIS: f32 = 240.0;
const MANAGER_RECENT_ICON_SIZE: f32 = 28.0;
const MANAGER_RECENT_ICON_GLYPH_SIZE: f32 = 14.0;
const MANAGER_GRID_CARD_MIN_WIDTH: f32 = 260.0;
const MANAGER_GRID_CARD_BASIS: f32 = 320.0;
const MANAGER_SIDEBAR_TREE_ESTIMATED_ROW_HEIGHT: f32 = 38.0;
const MANAGER_ROW_ACTION_MENU_WIDTH: f32 = 176.0;
const MANAGER_ROW_ACTION_MENU_CONNECTION_HEIGHT: f32 = 192.0;
const MANAGER_ROW_ACTION_MENU_PROFILE_HEIGHT: f32 = 44.0;
const MANAGER_ROW_ACTION_MENU_EDITABLE_PROFILE_HEIGHT: f32 = 120.0;
const MANAGER_ROW_ACTION_MENU_SSH_CONFIG_HEIGHT: f32 = 76.0;
const MANAGER_ROW_ACTION_MENU_GROUP_HEIGHT: f32 = 156.0;

impl CatalogView {
    fn session_manager_card_surface(&self, radius: f32, has_background: bool) -> Div {
        let surface = oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::None)
                .has_background_image(has_background),
        );
        surface
            .rounded(px(radius))
            .shadow_none()
            .border_color(self.workspace_chrome_divider())
            .bg(theme_bg(self.tokens.ui.bg, has_background))
    }

    fn session_manager_grid_columns(&self, window: &Window, _cx: &App) -> (usize, usize) {
        let available_width =
            f32::from(window.viewport_size().width) - self.environment.horizontal_chrome_width;
        let grid_width =
            (available_width - self.tokens.spacing.three * 2.0).max(MANAGER_GRID_CARD_MIN_WIDTH);
        let gap = self.tokens.spacing.three;
        let card_columns = ((grid_width + gap) / (MANAGER_GRID_CARD_BASIS + gap))
            .floor()
            .max(1.0) as usize;
        let recent_columns = ((grid_width + self.tokens.spacing.two)
            / (MANAGER_RECENT_ITEM_BASIS + self.tokens.spacing.two))
            .floor()
            .max(1.0) as usize;
        (card_columns, recent_columns)
    }

    fn session_manager_grid_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(MANAGER_GRID_ESTIMATED_ROW_HEIGHT),
            MANAGER_MAIN_VIEW_OVERSCAN,
        )
    }

    fn session_manager_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(MANAGER_LIST_ESTIMATED_ROW_HEIGHT),
            MANAGER_MAIN_VIEW_OVERSCAN,
        )
    }

    pub(super) fn session_manager_tree_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(MANAGER_TREE_ESTIMATED_ROW_HEIGHT),
            MANAGER_MAIN_VIEW_OVERSCAN,
        )
    }

    pub(super) fn session_manager_sidebar_tree_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(MANAGER_SIDEBAR_TREE_ESTIMATED_ROW_HEIGHT),
            MANAGER_MAIN_VIEW_OVERSCAN,
        )
    }

    fn sync_session_manager_grid_list_state(
        &self,
        rows: &[SessionManagerGridRow],
        items: &[SessionManagerDisplayItem],
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| session_manager_grid_row_signature(row, items))
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.main_grid_list_state,
            &mut manager.main_grid_list_cache.borrow_mut(),
            "session-manager-grid",
            &signatures,
            Self::session_manager_grid_list_spec(),
        );
    }

    fn sync_session_manager_main_list_state(&self, items: &[SessionManagerDisplayItem], cx: &App) {
        let signatures = items
            .iter()
            .map(session_manager_display_item_signature)
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.main_list_state,
            &mut manager.main_list_cache.borrow_mut(),
            "session-manager-list",
            &signatures,
            Self::session_manager_list_spec(),
        );
    }

    fn sync_session_manager_tree_list_state(
        &self,
        rows: &[SessionManagerTreeRow],
        items: &[SessionManagerDisplayItem],
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| session_manager_tree_row_signature(row, items))
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.main_tree_list_state,
            &mut manager.main_tree_list_cache.borrow_mut(),
            "session-manager-tree",
            &signatures,
            Self::session_manager_tree_list_spec(),
        );
    }

    pub(super) fn sync_session_manager_sidebar_tree_list_state(
        &self,
        rows: &[SessionManagerTreeRow],
        items: &[SessionManagerDisplayItem],
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| session_manager_tree_row_signature(row, items))
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.sidebar_tree_list_state,
            &mut manager.sidebar_tree_list_cache.borrow_mut(),
            "session-manager-sidebar-tree",
            &signatures,
            Self::session_manager_sidebar_tree_list_spec(),
        );
    }

    pub(super) fn render_session_manager_view_content(
        &mut self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = std::mem::take(&mut self.environment.items);
        let view_mode = self.session_manager.read(cx).view_mode;
        if items.is_empty() && view_mode != SessionManagerViewMode::Tree {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_session_manager_view_actions(
                    view_mode == SessionManagerViewMode::Tree,
                    has_background,
                    cx,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(self.render_session_manager_empty_view(has_background, cx)),
                )
                .into_any_element();
        }
        match view_mode {
            SessionManagerViewMode::Grid => {
                self.render_session_manager_grid_view(items, window, has_background, cx)
            }
            SessionManagerViewMode::List => {
                self.render_session_manager_list_view(items, has_background, cx)
            }
            SessionManagerViewMode::Tree => {
                self.render_session_manager_tree_view(items, has_background, cx)
            }
        }
    }

    pub(super) fn render_session_manager_empty_view(&self, has_background: bool, cx: &App) -> Div {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(self.tokens.spacing.three))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                48.0,
                rgba((theme.text_muted << 8) | 0x66),
            ))
            .child(
                div()
                    .text_size(px(MANAGER_ROW_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(
                        if self.session_manager.read(cx).search_query.trim().is_empty() {
                            self.i18n.t("sessionManager.table.no_connections")
                        } else {
                            self.i18n.t("sessionManager.table.no_search_results")
                        },
                    ),
            )
    }

    pub(super) fn render_session_manager_grid_view(
        &self,
        items: Vec<SessionManagerDisplayItem>,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (card_columns, recent_columns) = self.session_manager_grid_columns(window, cx);
        let (roots, _) = self.session_group_tree();
        let rows = Arc::<[SessionManagerGridRow]>::from(session_manager_grid_rows(
            &items,
            &roots,
            self.i18n.t("sessionManager.views.recent"),
            self.i18n.t("sessionManager.views.hosts"),
            card_columns,
            recent_columns,
        ));
        self.set_session_manager_visible_selection_targets(
            session_manager_grid_visible_targets(&rows, &items),
            cx,
        );
        self.sync_session_manager_grid_list_state(&rows, &items, cx);
        let state = self.session_manager.read(cx).main_grid_list_state.clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_grid_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_session_manager_grid_row(
                        rows.get(index),
                        &items,
                        has_background,
                        cx,
                    )
                })
            },
        )
        // Virtual rows own their horizontal gutters so full-width rows stay symmetric.
        .pt(px(self.tokens.spacing.three));
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(false, has_background, cx));

        content
            .child(div().flex_1().min_h(px(0.0)).child(list))
            .into_any_element()
    }

    pub(super) fn render_session_manager_grid_row(
        &self,
        row: Option<&SessionManagerGridRow>,
        items: &[SessionManagerDisplayItem],
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            Some(SessionManagerGridRow::SectionHeader { title, item_count }) => self
                .render_session_manager_section_header(title.clone(), *item_count)
                .pb(px(self.tokens.spacing.three))
                .into_any_element(),
            Some(SessionManagerGridRow::RecentItems {
                item_indices,
                is_last_in_section,
            }) => div()
                .w_full()
                .min_w(px(0.0))
                .px(px(self.tokens.spacing.three))
                .flex()
                .flex_wrap()
                .gap(px(self.tokens.spacing.two))
                .pb(px(if *is_last_in_section {
                    self.tokens.spacing.three
                } else {
                    self.tokens.spacing.two
                }))
                .children(item_indices.iter().filter_map(|index| {
                    items.get(*index).map(|item| {
                        self.render_session_manager_recent_item(item, has_background, cx)
                    })
                }))
                .into_any_element(),
            Some(SessionManagerGridRow::Cards { item_indices }) => div()
                .w_full()
                .min_w(px(0.0))
                .px(px(self.tokens.spacing.three))
                .flex()
                .flex_wrap()
                .gap(px(self.tokens.spacing.three))
                .pb(px(self.tokens.spacing.three))
                .children(item_indices.iter().filter_map(|index| {
                    items
                        .get(*index)
                        .map(|item| self.render_session_manager_item_card(item, has_background, cx))
                }))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    /// Shared mousedown dispatch for the non-draggable grid surfaces (cards
    /// and the recent strip), which keep selecting on press.
    fn handle_session_manager_item_mousedown(
        &mut self,
        event: &MouseDownEvent,
        selection_target: Option<SessionManagerSelectionTarget>,
        open_target: SessionManagerOpenTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match session_manager_item_pointer_action(
            event.click_count,
            selection_target.is_some(),
            event.modifiers.control,
            event.modifiers.shift,
        ) {
            SessionManagerItemPointerAction::SelectReplace => {
                if let Some(target) = selection_target {
                    self.apply_session_manager_selection(target, RowSelectionMode::Replace, cx);
                }
            }
            SessionManagerItemPointerAction::SelectToggle => {
                if let Some(target) = selection_target {
                    self.apply_session_manager_selection(target, RowSelectionMode::Toggle, cx);
                }
            }
            SessionManagerItemPointerAction::SelectRange => {
                if let Some(target) = selection_target {
                    self.apply_session_manager_selection(target, RowSelectionMode::Range, cx);
                }
            }
            SessionManagerItemPointerAction::Open => {
                self.open_session_manager_target_with_sftp(
                    open_target,
                    event.modifiers.control,
                    window,
                    cx,
                );
            }
            SessionManagerItemPointerAction::None => {}
        }
    }

    pub(super) fn render_session_manager_recent_item(
        &self,
        item: &SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let open_target = item.open_target();
        let selection_target = item.selection_target();
        let open_button_target = item.open_target();
        let last_used = format_last_used(item.last_used().as_deref(), &self.i18n);
        let is_selected = selection_target.as_ref().is_some_and(|target| {
            self.session_manager
                .read(cx)
                .selected_items
                .contains(target)
        });
        self.session_manager_card_surface(self.tokens.radii.md, has_background)
            .min_w(px(MANAGER_RECENT_ITEM_MIN_WIDTH))
            .flex_basis(px(MANAGER_RECENT_ITEM_BASIS))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .hover(|shortcut| shortcut.bg(theme_row_hover_bg(theme.bg_hover, has_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_session_manager_item_mousedown(
                        event,
                        selection_target.clone(),
                        open_target.clone(),
                        window,
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .size(px(MANAGER_RECENT_ICON_SIZE))
                    .flex_none()
                    .rounded(px(self.tokens.radii.md))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba((theme.accent << 8) | 0x1a))
                    .child(item.icon().render(
                        MANAGER_RECENT_ICON_GLYPH_SIZE,
                        rgb(theme.accent),
                        true,
                    )),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(if is_selected {
                                theme.accent
                            } else {
                                theme.text
                            }))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(last_used),
                    ),
            )
            .child(self.render_row_icon_button(
                LucideIcon::Play,
                MANAGER_ROW_ACTION_BUTTON,
                MANAGER_ROW_ACTION_ICON_SIZE,
                rgb(theme.accent),
                has_background,
                move |this, event: &MouseDownEvent, window, cx| {
                    this.open_session_manager_target_with_sftp(
                        open_button_target.clone(),
                        event.modifiers.control,
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                },
                cx,
            ))
    }

    pub(super) fn render_session_manager_section_header(&self, title: String, count: usize) -> Div {
        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(self.tokens.spacing.three))
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(count.to_string()),
                    ),
            )
    }

    pub(super) fn render_session_manager_item_card(
        &self,
        item: &SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let open_target = item.open_target();
        let selection_target = item.selection_target();
        let checkbox_target = selection_target.clone();
        let subtitle = if matches!(item, SessionManagerDisplayItem::SshConfig(_)) {
            format!(
                "{} · {}",
                item.subtitle(),
                self.i18n.t("command_palette.ssh_config_source")
            )
        } else {
            item.subtitle()
        };
        // Keep the selected connection name aligned with the checkbox's accent treatment.
        let is_selected = selection_target.as_ref().is_some_and(|target| {
            self.session_manager
                .read(cx)
                .selected_items
                .contains(target)
        });
        self.session_manager_card_surface(self.tokens.radii.lg, has_background)
            .when(is_selected, |card| {
                card.bg(rgba((theme.accent << 8) | 0x12))
            })
            .hover(|card| card.bg(theme_row_hover_bg(theme.bg_hover, has_background)))
            .min_w(px(260.0))
            .flex_grow_1()
            .flex_basis(px(320.0))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.three))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.three))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_session_manager_item_mousedown(
                        event,
                        selection_target.clone(),
                        open_target.clone(),
                        window,
                        cx,
                    );
                }),
            )
            .when_some(checkbox_target, |card, target| {
                card.child(
                    checkbox(
                        &self.tokens,
                        format!("session-selection-{target:?}"),
                        String::new(),
                        is_selected,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.toggle_session_selection(target.clone(), cx);
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
                )
            })
            .child(self.render_session_manager_item_icon(item, theme.text))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(if is_selected {
                                theme.accent
                            } else {
                                theme.text
                            }))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(subtitle),
                    ),
            )
            .child(self.render_session_manager_display_item_actions(item, has_background, cx))
    }

    pub(super) fn render_session_manager_list_view(
        &self,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.set_session_manager_visible_selection_targets(
            items
                .iter()
                .filter_map(|item| item.selection_target())
                .collect(),
            cx,
        );
        self.sync_session_manager_main_list_state(&items, cx);
        let state = self.session_manager.read(cx).main_list_state.clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    let Some(item) = items.get(index) else {
                        return div().into_any_element();
                    };
                    this.render_session_manager_display_item_row(
                        item,
                        0,
                        has_background,
                        false,
                        false,
                        cx,
                    )
                    .into_any_element()
                })
            },
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(false, has_background, cx))
            .child(
                div()
                    .border_b_1()
                    .border_color(theme_border(theme.border, has_background))
                    .bg(theme_secondary_bg(theme.bg_secondary, has_background))
                    .px_3()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.three))
                    .text_size(px(MANAGER_TABLE_HEADER_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(div().w(px(MANAGER_ROW_ICON_SIZE)).flex_none())
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(self.i18n.t("sessionManager.table.name")),
                    )
                    .child(
                        div()
                            .w(px(MANAGER_LIST_LAST_USED_WIDTH))
                            .flex_none()
                            .child(self.i18n.t("sessionManager.table.last_used")),
                    )
                    .child(
                        div()
                            .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .child(self.i18n.t("sessionManager.table.actions")),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            // Clicking below the last row clears the selection;
                            // rows stop propagation so their own handlers win.
                            this.clear_session_manager_selection(cx);
                        }),
                    )
                    .child(list),
            )
            .into_any_element()
    }

    pub(super) fn render_session_manager_tree_view(
        &mut self,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (roots, children) = self.session_group_tree();
        let expanded_groups = self.session_manager.read(cx).expanded_groups.clone();
        let rows = Arc::<[SessionManagerTreeRow]>::from(session_manager_tree_rows(
            &items,
            &roots,
            &children,
            &expanded_groups,
        ));
        let has_rows = !rows.is_empty();
        self.set_session_manager_visible_selection_targets(
            session_manager_tree_visible_targets(&rows, &items),
            cx,
        );
        self.sync_session_manager_tree_list_state(&rows, &items, cx);
        let state = self.session_manager.read(cx).main_tree_list_state.clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_tree_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_session_manager_tree_row(
                        rows.get(index),
                        &items,
                        has_background,
                        false,
                        cx,
                    )
                })
            },
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(true, has_background, cx))
            .child(
                div()
                    .id("session-manager-tree-root-drop-target")
                    .flex_1()
                    .min_h(px(0.0))
                    .drag_over::<SessionManagerDrag>(move |target, _drag, _window, _cx| {
                        target
                            .border_1()
                            .border_dashed()
                            .border_color(rgb(theme.accent))
                            .bg(rgba((theme.accent << 8) | MANAGER_DRAG_ROOT_BG_ALPHA))
                    })
                    .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
                    .on_drop(cx.listener(|this, drag: &SessionManagerDrag, _window, cx| {
                        // Empty tree space and ungrouped rows both represent the root group.
                        this.move_dragged_sessions_to_group(&drag.targets, None, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            // Empty tree space represents the root group level.
                            this.open_session_manager_context_menu(
                                SessionManagerRowActionTarget::GroupRoot,
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            // Clicking empty tree space clears the selection;
                            // rows and group headers stop propagation so their
                            // own handlers keep ownership of the press.
                            this.clear_session_manager_selection(cx);
                        }),
                    )
                    // GPUI renamed the wheel listener from on_scroll_wheel.
                    .on_scroll(cx.listener(|this, _event, _window, cx| {
                        // Close pointer-positioned menus before the virtual rows move.
                        this.close_session_row_menus(cx);
                    }))
                    .child(if has_rows {
                        list.into_any_element()
                    } else {
                        self.render_session_manager_empty_view(has_background, cx)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_session_manager_view_actions(
        &self,
        include_tree_controls: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let mut row = div()
            // The SSH config importer is a discovery action for every
            // session-manager layout, not a tree-only folder operation.
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .border_b_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_bg(theme.bg, has_background))
            .px_3()
            .py_1();
        if include_tree_controls {
            row = row
                .child(self.render_tree_mode_action_button(
                    LucideIcon::ChevronDown,
                    self.i18n.t("sessionManager.views.expand_all"),
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        let (roots, children) = this.session_group_tree();
                        let mut groups = HashSet::new();
                        collect_session_group_paths(&roots, &children, &mut groups);
                        this.session_manager.update(cx, |manager, cx| {
                            manager.expanded_groups = groups;
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                    cx,
                ))
                .child(self.render_tree_mode_action_button(
                    LucideIcon::ChevronRight,
                    self.i18n.t("sessionManager.views.collapse_all"),
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            manager.expanded_groups.clear();
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                    cx,
                ));
        }
        // Root-level creation remains available in every layout while object-specific
        // maintenance stays beside each group through its menu.
        row = row.child(self.render_tree_mode_action_button(
            LucideIcon::FolderPlus,
            self.i18n.t("sessionManager.folder_tree.new_group"),
            has_background,
            cx.listener(|this, _event, _window, cx| {
                this.action(CatalogAction::NewGroup, None, cx);
                cx.stop_propagation();
            }),
            cx,
        ));
        row.child(self.render_tree_mode_action_button(
            LucideIcon::FolderInput,
            self.i18n.t("settings_view.connections.ssh_config.title"),
            has_background,
            cx.listener(|this, _event, _window, cx| {
                this.close_session_row_menus(cx);
                this.action(CatalogAction::OpenSshConfigImporter, None, cx);
                cx.stop_propagation();
            }),
            cx,
        ))
        .child(self.render_tree_mode_action_button(
            LucideIcon::Download,
            self.i18n.t("settings_view.connections.importers.title"),
            has_background,
            cx.listener(|this, _event, window, cx| {
                this.close_session_row_menus(cx);
                this.action(CatalogAction::OpenImporters, Some(window), cx);
                cx.stop_propagation();
            }),
            cx,
        ))
    }

    pub(super) fn render_tree_mode_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> Div {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                has_background,
                show_label: true,
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    pub(super) fn render_session_manager_sidebar_tree_row(
        &self,
        rows: &[SessionManagerTreeRow],
        index: usize,
        items: &[SessionManagerDisplayItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = rows.get(index) else {
            return div().into_any_element();
        };
        let row_element = self.render_session_manager_tree_row(Some(row), items, false, true, cx);
        let (ancestor_rails, is_last) = session_manager_tree_row_rails(rows, index);
        let depth = row.depth();
        let guided = if depth == 0 {
            row_element
        } else {
            self.render_session_tree_child_with_rails(depth, &ancestor_rails, is_last, row_element)
        };
        div().px_1().child(guided).into_any_element()
    }

    pub(super) fn render_session_manager_tree_row(
        &self,
        row: Option<&SessionManagerTreeRow>,
        items: &[SessionManagerDisplayItem],
        has_background: bool,
        sidebar: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            Some(SessionManagerTreeRow::Group {
                path,
                depth,
                expanded,
                has_children,
            }) => self
                .render_session_manager_tree_group_row(
                    path,
                    *depth,
                    *expanded,
                    *has_children,
                    has_background,
                    sidebar,
                    cx,
                )
                .into_any_element(),
            Some(SessionManagerTreeRow::Item { item_index, depth }) => items
                .get(*item_index)
                .map(|item| {
                    let context_target = item.row_action_target();
                    let drop_group = item.group().map(ToOwned::to_owned);
                    let theme = self.tokens.ui;
                    let radius = self.tokens.radii.sm;
                    let row = self
                        .render_session_manager_display_item_row(
                            item,
                            *depth,
                            has_background,
                            true,
                            sidebar,
                            cx,
                        )
                        .drag_over::<SessionManagerDrag>(move |row, _drag, _window, _cx| {
                            // Every visible child row participates in its nearest folder's drop area.
                            let row = if sidebar {
                                row.rounded(px(radius))
                            } else {
                                row
                            };
                            row.border_1()
                                .border_color(rgb(theme.accent))
                                .bg(rgba((theme.accent << 8) | MANAGER_DRAG_GROUP_BG_ALPHA))
                        })
                        .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
                        .on_drop(cx.listener(
                            move |this, drag: &SessionManagerDrag, _window, cx| {
                                this.move_dragged_sessions_to_group(
                                    &drag.targets,
                                    drop_group.as_deref(),
                                    cx,
                                );
                                cx.stop_propagation();
                            },
                        ))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                if let Some(target) = context_target.clone() {
                                    this.open_session_manager_context_menu(
                                        target,
                                        f32::from(event.position.x),
                                        f32::from(event.position.y),
                                        cx,
                                    );
                                }
                                cx.stop_propagation();
                            }),
                        );
                    if !sidebar {
                        return row.into_any_element();
                    }
                    // The sidebar drops the last-used column; surface the value
                    // as a hover hint on the already-stateful row. The tooltip
                    // queue key stays distinct from the element id so hover
                    // bookkeeping cannot collide with drag state.
                    let tooltip_id = format!(
                        "session-manager-row-last-used-{}",
                        session_manager_display_item_signature(item)
                    );
                    let hint = format!(
                        "{}: {}",
                        self.i18n.t("sessionManager.table.last_used"),
                        format_last_used(item.last_used().as_deref(), &self.i18n)
                    );
                    let clear_id = tooltip_id.clone();
                    row.on_mouse_move(cx.listener(
                        move |this, event: &MouseMoveEvent, _window, cx| {
                            this.queue_workspace_tooltip(
                                &tooltip_id,
                                hint.clone(),
                                f32::from(event.position.x) + 12.0,
                                f32::from(event.position.y) + 16.0,
                                cx,
                            );
                        },
                    ))
                    .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            this.clear_workspace_tooltip(&clear_id, cx);
                        }
                    }))
                    .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element()),
            None => div().into_any_element(),
        }
    }

    pub(super) fn render_session_manager_tree_group_row(
        &self,
        group: &str,
        depth: usize,
        expanded: bool,
        has_children: bool,
        has_background: bool,
        sidebar: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let group_name = group.rsplit('/').next().unwrap_or(group).to_string();
        let group_id = group.to_string();
        let context_group = group.to_string();
        let menu_group = group.to_string();
        let drop_group = group.to_string();
        let subgroup_parent = group.to_string();
        let subgroup_tooltip = format!(
            "{} — {group}",
            self.i18n.t("sessionManager.folder_tree.new_subgroup")
        );
        let group_count = self.connection_count_for_group(group).to_string();
        let workspace = cx.entity();
        let radius = self.tokens.radii.sm;
        let chevron = self.render_animated_chevron(
            (
                gpui::SharedString::from(format!("session-group-chevron-{group}")),
                expanded as usize,
            ),
            expanded,
            if sidebar { 12.0 } else { 16.0 },
            rgb(theme.text_muted),
        );
        let folder_icon = Self::render_lucide_icon(
            if expanded {
                LucideIcon::FolderOpen
            } else {
                LucideIcon::Folder
            },
            if sidebar { 14.0 } else { 16.0 },
            rgb(theme.warning),
        );
        div()
            .w_full()
            .min_w(px(0.0))
            .when(sidebar, |row| {
                row.h(px(self.session_tree_node_height()))
                    .px_2()
                    .rounded(px(radius))
                    .cursor_pointer()
                    .gap(px(6.0))
                    .hover(move |row| row.bg(rgb(theme.bg_hover)))
            })
            .when(!sidebar, |row| {
                row.border_b_1()
                    .border_color(self.workspace_chrome_divider())
                    .px_3()
                    .py_2()
                    .pl(px(depth as f32 * 24.0 + 12.0))
                    .gap(px(self.tokens.spacing.two))
                    .hover(|row| row.bg(theme_row_hover_bg(theme.bg_hover, has_background)))
            })
            .flex()
            .items_center()
            .drag_over::<SessionManagerDrag>(move |row, _drag, _window, _cx| {
                let row = if sidebar {
                    row.rounded(px(radius))
                } else {
                    row
                };
                row.border_1()
                    .border_color(rgb(theme.accent))
                    .bg(rgba((theme.accent << 8) | MANAGER_DRAG_GROUP_BG_ALPHA))
            })
            .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
            .on_drop(
                cx.listener(move |this, drag: &SessionManagerDrag, _window, cx| {
                    this.move_dragged_sessions_to_group(&drag.targets, Some(&drop_group), cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if has_children {
                        this.toggle_session_group_expanded(&group_id, cx);
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.open_session_manager_context_menu(
                        SessionManagerRowActionTarget::Group(context_group.clone()),
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .child(if sidebar {
                div()
                    .w(px(12.0))
                    .h(px(20.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(chevron)
                    .into_any_element()
            } else {
                chevron.into_any_element()
            })
            .child(if sidebar {
                div()
                    .w(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(folder_icon)
                    .into_any_element()
            } else {
                folder_icon.into_any_element()
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(if sidebar {
                        self.tokens.metrics.ui_text_xs
                    } else {
                        MANAGER_ROW_TEXT_SIZE
                    }))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(group_name),
            )
            .child(if sidebar {
                div()
                    .flex_none()
                    .text_size(px(self.tokens.metrics.ui_text_2xs))
                    .text_color(rgb(theme.text_muted))
                    .child(group_count)
            } else {
                div()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(theme_input_bg(theme.bg, has_background))
                    .px_2()
                    .py(px(1.0))
                    .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(group_count)
            })
            // The sidebar keeps group rows clean; the right-click menu already
            // exposes subgroup creation and the same action list.
            .when(!sidebar, |row| {
                row.child(self.workspace_tooltip_icon_button(
                    LucideIcon::FolderPlus,
                    MANAGER_ROW_ACTION_ICON_SIZE,
                    rgb(theme.text),
                    IconButtonOptions {
                        has_background,
                        ..IconButtonOptions::opaque_toolbar(
                            MANAGER_ROW_ACTION_BUTTON,
                            ButtonRadius::Sm,
                        )
                    },
                    subgroup_tooltip,
                    "session-manager-new-subgroup",
                    true,
                    cx.listener(move |this, _event, _window, cx| {
                        this.action(
                            CatalogAction::NewSubgroup(subgroup_parent.clone()),
                            None,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                    workspace,
                ))
                .child(self.render_row_icon_button(
                    LucideIcon::MoreVertical,
                    MANAGER_ROW_ACTION_BUTTON,
                    MANAGER_ROW_ACTION_ICON_SIZE,
                    rgb(theme.text),
                    has_background,
                    move |this, event, _window, cx| {
                        this.open_session_manager_row_action_menu(
                            SessionManagerRowActionTarget::Group(menu_group.clone()),
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
            })
    }

    pub(super) fn render_session_manager_display_item_row(
        &self,
        item: &SessionManagerDisplayItem,
        depth: usize,
        has_background: bool,
        allow_drag: bool,
        sidebar: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.tokens.ui;
        let open_target = item.open_target();
        let selection_target = item.selection_target();
        let last_used = item.last_used();
        let subtitle = if matches!(item, SessionManagerDisplayItem::SshConfig(_)) {
            format!(
                "{} · {}",
                item.subtitle(),
                self.i18n.t("command_palette.ssh_config_source")
            )
        } else {
            item.subtitle()
        };
        // List rows mirror the card view so selection feedback is consistent.
        let is_selected = selection_target.as_ref().is_some_and(|target| {
            self.session_manager
                .read(cx)
                .selected_items
                .contains(target)
        });
        // The whole row is the drag source. The payload is computed from the
        // selection at render time: a selected row drags the full selection,
        // an unselected row drags only itself. Single-click selection is
        // applied on release (on_click below), never on press, so a re-render
        // between press and drag-start cannot shrink the payload before the
        // drag listener captures it.
        let drag_payload =
            if allow_drag && self.session_manager.read(cx).search_query.trim().is_empty() {
                selection_target.clone().map(|target| {
                    let selected_targets = &self.session_manager.read(cx).selected_items;
                    let targets = if is_selected {
                        selected_targets.iter().cloned().collect::<Vec<_>>()
                    } else {
                        vec![target]
                    };
                    let label = if targets.len() > 1 {
                        selected_count_label(&self.i18n, targets.len())
                    } else {
                        item.name().to_string()
                    };
                    SessionManagerDrag {
                        targets,
                        label,
                        position: Point::default(),
                        background: rgb(theme.bg_panel),
                        border: rgb(theme.accent),
                        text: rgb(theme.text),
                    }
                })
            } else {
                None
            };
        let drag_workspace = cx.entity();
        let row = div()
            .id(SharedString::from(format!(
                "session-manager-row-{}",
                session_manager_display_item_signature(item)
            )))
            .debug_selector(|| format!("session-manager-row-{}", item.id()))
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .relative()
            .when(sidebar, |row| {
                row.px_2()
                    .py(px(5.0))
                    .gap(px(self.tokens.spacing.two))
                    .rounded(px(self.tokens.radii.sm))
                    .cursor_pointer()
                    .when(is_selected, |row| {
                        row.bg(rgba((theme.accent << 8) | 0x1a)).child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(5.0))
                                .bottom(px(5.0))
                                .w(px(2.0))
                                .rounded_full()
                                .bg(rgb(theme.accent)),
                        )
                    })
                    .when(!is_selected, |row| {
                        row.hover(move |row| row.bg(rgb(theme.bg_hover)))
                    })
            })
            .when(!sidebar, |row| {
                row.border_b_1()
                    .border_color(self.workspace_chrome_divider())
                    .px_3()
                    .py_2()
                    .pl(px(depth as f32 * 24.0 + 12.0))
                    .gap(px(self.tokens.spacing.three))
                    .when(is_selected, |row| {
                        row.bg(rgba((theme.accent << 8) | 0x12)).child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(2.0))
                                .bg(rgb(theme.accent)),
                        )
                    })
                    .hover(|row| row.bg(theme_row_hover_bg(theme.bg_hover, has_background)))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    // Double-click opens immediately on press; single-click
                    // selection waits for release so the drag payload above
                    // stays intact (see the payload comment).
                    if event.click_count == 2 {
                        this.open_session_manager_target_with_sftp(
                            open_target.clone(),
                            event.modifiers.control,
                            window,
                            cx,
                        );
                    }
                    cx.stop_propagation();
                }),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                    // Keyboard/touch clicks carry no click_count or modifiers;
                    // only the mouse press/release pair drives row selection.
                    let gpui::ClickEvent::Mouse(click) = event else {
                        return;
                    };
                    match session_manager_item_pointer_action(
                        click.down.click_count,
                        selection_target.is_some(),
                        click.down.modifiers.control,
                        click.down.modifiers.shift,
                    ) {
                        SessionManagerItemPointerAction::SelectReplace => {
                            if let Some(target) = selection_target.clone() {
                                this.apply_session_manager_selection(
                                    target,
                                    RowSelectionMode::Replace,
                                    cx,
                                );
                            }
                        }
                        SessionManagerItemPointerAction::SelectToggle => {
                            if let Some(target) = selection_target.clone() {
                                this.apply_session_manager_selection(
                                    target,
                                    RowSelectionMode::Toggle,
                                    cx,
                                );
                            }
                        }
                        SessionManagerItemPointerAction::SelectRange => {
                            if let Some(target) = selection_target.clone() {
                                this.apply_session_manager_selection(
                                    target,
                                    RowSelectionMode::Range,
                                    cx,
                                );
                            }
                        }
                        // Open is handled on mouse-down, and a drag consumes the
                        // pending mouse-down so this click never fires mid-drag.
                        SessionManagerItemPointerAction::Open
                        | SessionManagerItemPointerAction::None => {}
                    }
                }),
            )
            .child(if sidebar {
                self.render_session_manager_item_icon_sized(
                    item,
                    theme.text,
                    MANAGER_SIDEBAR_ROW_ICON_SIZE,
                    MANAGER_SIDEBAR_ROW_ICON_GLYPH_SIZE,
                    self.tokens.radii.sm,
                )
            } else {
                self.render_session_manager_item_icon(item, theme.text)
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(if sidebar {
                                self.tokens.metrics.ui_text_xs
                            } else {
                                MANAGER_ROW_TEXT_SIZE
                            }))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(if is_selected && !sidebar {
                                theme.accent
                            } else {
                                theme.text
                            }))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(if sidebar {
                                self.tokens.metrics.ui_text_2xs
                            } else {
                                MANAGER_ROW_META_TEXT_SIZE
                            }))
                            .text_color(rgb(theme.text_muted))
                            .child(subtitle),
                    ),
            )
            .when(!sidebar, |row| {
                row.child(
                    div()
                        .w(px(MANAGER_LIST_LAST_USED_WIDTH))
                        .flex_none()
                        .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                        .text_color(rgb(theme.text_muted))
                        .child(format_last_used(last_used.as_deref(), &self.i18n)),
                )
                .child(self.render_session_manager_display_item_actions(item, has_background, cx))
            });
        if let Some(drag) = drag_payload {
            row.on_drag(drag, move |drag, position, _window, cx| {
                let dragged_targets = drag.targets.iter().cloned().collect::<HashSet<_>>();
                let _ = drag_workspace.update(cx, |this, cx| {
                    this.session_manager.update(cx, |session_manager, cx| {
                        // The drag payload owns the selection it previews, so
                        // any selection change made while pressing is
                        // reconciled to the exact payload at drag start.
                        if session_manager.selected_items != dragged_targets {
                            session_manager.selected_items = dragged_targets;
                            cx.notify();
                        }
                    });
                });
                cx.new(|_| drag.with_position(position))
            })
        } else {
            row
        }
    }

    pub(super) fn render_session_manager_item_icon(
        &self,
        item: &SessionManagerDisplayItem,
        text: u32,
    ) -> Div {
        self.render_session_manager_item_icon_sized(
            item,
            text,
            MANAGER_ROW_ICON_SIZE,
            20.0,
            self.tokens.radii.lg,
        )
    }

    pub(super) fn render_session_manager_item_icon_sized(
        &self,
        item: &SessionManagerDisplayItem,
        text: u32,
        box_size: f32,
        glyph_size: f32,
        radius: f32,
    ) -> Div {
        let ui = self.tokens.ui;
        let semantic_color = match item {
            SessionManagerDisplayItem::Connection(_)
            | SessionManagerDisplayItem::Tunnel(_)
            | SessionManagerDisplayItem::RemoteDesktop(_) => ui.info,
            SessionManagerDisplayItem::SshConfig(_) | SessionManagerDisplayItem::Plugin(_) => {
                ui.accent_secondary
            }
            SessionManagerDisplayItem::Serial(_)
            | SessionManagerDisplayItem::Ble(_)
            | SessionManagerDisplayItem::LocalTerminal(_) => ui.warning,
            SessionManagerDisplayItem::Telnet(_)
            | SessionManagerDisplayItem::Ftp(_)
            | SessionManagerDisplayItem::StandaloneSftp(_) => ui.success,
            SessionManagerDisplayItem::Mosh(_) => ui.accent,
        };
        let (default_background, default_foreground) =
            ((semantic_color << 8) | 0x26, semantic_color);
        let configured_foreground = item.icon_color().and_then(parse_hex_color);
        // Older assets used one accent for both layers; keep that appearance
        // until an explicit background is selected.
        let bg = item
            .icon_background_color()
            .and_then(parse_hex_color)
            .map(rgb)
            .or_else(|| configured_foreground.map(|color| rgba((color << 8) | 0x33)))
            .unwrap_or_else(|| rgba(default_background));
        let fg = configured_foreground
            .map(rgb)
            .unwrap_or_else(|| rgb(default_foreground));
        div()
            .w(px(box_size))
            .h(px(box_size))
            .flex_none()
            .rounded(px(radius))
            .flex()
            .items_center()
            .justify_center()
            .bg(bg)
            .child(item.icon().render(glyph_size, fg, true))
            .when(
                matches!(item, SessionManagerDisplayItem::Connection(_)),
                |icon| icon.border_1().border_color(rgba((text << 8) | 0x1a)),
            )
    }

    pub(super) fn render_session_manager_display_item_actions(
        &self,
        item: &SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        match item {
            SessionManagerDisplayItem::Tunnel(item) => {
                let id = item.forward.id.clone();
                let edit = id.clone();
                let menu = id.clone();
                let running = matches!(
                    self.ssh_tunnel_status(&id),
                    oxideterm_forwarding::ForwardStatus::Active
                        | oxideterm_forwarding::ForwardStatus::Starting
                        | oxideterm_forwarding::ForwardStatus::Suspended
                );
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        if running {
                            LucideIcon::Square
                        } else {
                            LucideIcon::Play
                        },
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _, window, cx| {
                            if running {
                                this.action(
                                    CatalogAction::StopTunnel(id.to_owned().into()),
                                    None,
                                    cx,
                                );
                            } else {
                                this.action(
                                    CatalogAction::Open {
                                        target: SessionManagerOpenTarget::Tunnel(
                                            id.to_owned().into(),
                                        ),
                                        open_sftp: false,
                                    },
                                    Some(window),
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Tunnel(edit.clone())),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Tunnel(menu.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Plugin(profile) => {
                let open = profile.id.clone();
                let edit = profile.id.clone();
                let menu = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _, _, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::Plugin(open.clone()),
                                    open_sftp: false,
                                },
                                None,
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Plugin(
                                    edit.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Plugin(menu.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Connection(connection) => {
                let open_id = connection.id.clone();
                let edit_id = connection.id.clone();
                let menu_id = connection.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, event: &MouseDownEvent, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::Connection(open_id.clone()),
                                    open_sftp: event.modifiers.control,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Connection(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Connection(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::SshConfig(host) => {
                let open_alias = host.alias.clone();
                let import_alias = host.alias.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::SshConfig(open_alias.clone()),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Download,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, _window, cx| {
                            this.action(
                                CatalogAction::ImportSshConfig(import_alias.clone()),
                                None,
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Serial(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::Serial(open_id.to_owned()),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Serial(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Serial(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Ble(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::Ble(open_id.to_owned()),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Ble(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Ble(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::LocalTerminal(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::LocalTerminal(
                                        open_id.to_owned(),
                                    ),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::LocalTerminal(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::LocalTerminal(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Telnet(_) | SessionManagerDisplayItem::Ftp(_) => {
                let ftp = matches!(item, SessionManagerDisplayItem::Ftp(_));
                let open_id = item.id().to_owned();
                let edit_id = open_id.clone();
                let menu_id = open_id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            if ftp {
                                this.action(
                                    CatalogAction::Open {
                                        target: SessionManagerOpenTarget::Ftp(open_id.to_owned()),
                                        open_sftp: false,
                                    },
                                    Some(window),
                                    cx,
                                );
                            } else {
                                this.action(
                                    CatalogAction::Open {
                                        target: SessionManagerOpenTarget::Telnet(
                                            open_id.to_owned(),
                                        ),
                                        open_sftp: false,
                                    },
                                    Some(window),
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            if ftp {
                                this.action(
                                    CatalogAction::Edit(SessionManagerOpenTarget::Ftp(
                                        edit_id.to_owned(),
                                    )),
                                    Some(window),
                                    cx,
                                );
                            } else {
                                this.action(
                                    CatalogAction::Edit(SessionManagerOpenTarget::Telnet(
                                        edit_id.to_owned(),
                                    )),
                                    Some(window),
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                if ftp {
                                    SessionManagerRowActionTarget::Ftp(menu_id.clone())
                                } else {
                                    SessionManagerRowActionTarget::Telnet(menu_id.clone())
                                },
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Mosh(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::Mosh(open_id.to_owned()),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Mosh(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Mosh(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::StandaloneSftp(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::StandaloneSftp(
                                        open_id.to_owned(),
                                    ),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::StandaloneSftp(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::StandaloneSftp(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::RemoteDesktop(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::RemoteDesktop(
                                        open_id.to_owned(),
                                    ),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::RemoteDesktop(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::RemoteDesktop(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
        }
    }

    pub(super) fn render_session_manager_row_action_menu(
        &self,
        menu: SessionManagerRowActionMenu,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let menu_height = match &menu.target {
            SessionManagerRowActionTarget::Connection(_) => {
                MANAGER_ROW_ACTION_MENU_CONNECTION_HEIGHT
            }
            SessionManagerRowActionTarget::Serial(_)
            | SessionManagerRowActionTarget::Tunnel(_)
            | SessionManagerRowActionTarget::Ble(_)
            | SessionManagerRowActionTarget::Plugin(_)
            | SessionManagerRowActionTarget::LocalTerminal(_)
            | SessionManagerRowActionTarget::Telnet(_)
            | SessionManagerRowActionTarget::Ftp(_)
            | SessionManagerRowActionTarget::Mosh(_)
            | SessionManagerRowActionTarget::StandaloneSftp(_)
            | SessionManagerRowActionTarget::RemoteDesktop(_) => {
                MANAGER_ROW_ACTION_MENU_EDITABLE_PROFILE_HEIGHT
            }
            SessionManagerRowActionTarget::SshConfig(_) => {
                MANAGER_ROW_ACTION_MENU_SSH_CONFIG_HEIGHT
            }
            SessionManagerRowActionTarget::GroupRoot => MANAGER_ROW_ACTION_MENU_PROFILE_HEIGHT,
            SessionManagerRowActionTarget::Group(_) => MANAGER_ROW_ACTION_MENU_GROUP_HEIGHT,
        };
        let menu_width = {
            let mut labels: Vec<String> = Vec::new();
            match &menu.target {
                SessionManagerRowActionTarget::GroupRoot => {
                    labels.push(self.i18n.t("sessionManager.folder_tree.new_group"));
                }
                SessionManagerRowActionTarget::Group(_) => {
                    labels.push(self.i18n.t("sessionManager.toolbar.new_connection"));
                    labels.push(self.i18n.t("sessionManager.folder_tree.new_subgroup"));
                    labels.push(self.i18n.t("sessionManager.folder_tree.rename_group"));
                    labels.push(self.i18n.t("sessionManager.actions.delete"));
                }
                SessionManagerRowActionTarget::SshConfig(_) => {
                    labels.push(self.i18n.t("sessionManager.actions.connect"));
                    labels.push(self.i18n.t("settings_view.connections.ssh_config.import"));
                }
                SessionManagerRowActionTarget::Tunnel(_) => {
                    labels.push(self.i18n.t("sessionManager.actions.start_tunnel"));
                    labels.push(self.i18n.t("sessionManager.actions.stop_tunnel"));
                    labels.push(self.i18n.t("sessionManager.actions.edit"));
                    labels.push(self.i18n.t("sessionManager.actions.delete"));
                }
                SessionManagerRowActionTarget::Connection(_) => {
                    labels.push(self.i18n.t("sessionManager.actions.connect"));
                    labels.push(self.i18n.t("sessionManager.actions.edit"));
                    labels.push(self.i18n.t("sessionManager.actions.test_connection"));
                    labels.push(self.i18n.t("sessionManager.actions.duplicate"));
                    labels.push(self.i18n.t("sessionManager.actions.delete"));
                }
                _ => {
                    labels.push(self.i18n.t("sessionManager.actions.connect"));
                    labels.push(self.i18n.t("sessionManager.actions.edit"));
                    labels.push(self.i18n.t("sessionManager.actions.delete"));
                }
            }
            oxideterm_gpui_ui::estimate_menu_width(
                &self.tokens,
                MANAGER_ROW_ACTION_MENU_WIDTH,
                280.0,
                true,
                labels.iter().map(String::as_str),
            )
        };
        let (requested_x, requested_y) = match menu.origin {
            SessionManagerRowActionMenuOrigin::ActionButton => (
                menu.x - menu_width + MANAGER_ROW_ACTION_BUTTON / 2.0,
                menu.y + MANAGER_ROW_ACTION_BUTTON / 2.0,
            ),
            SessionManagerRowActionMenuOrigin::Pointer => (menu.x, menu.y),
        };
        let placement = catalog_menu_position(
            requested_x,
            requested_y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            menu_width,
            menu_height,
            self.tokens.spacing.two,
        );
        let mut popup =
            context_menu_event_boundary(dropdown_menu_content(&self.tokens).w(px(menu_width)));

        // Opening is the primary row action now that the sidebar hides inline buttons.
        if let Some(open_target) = menu.target.open_target() {
            let tunnel_id = match &menu.target {
                SessionManagerRowActionTarget::Tunnel(id) => Some(id.clone()),
                _ => None,
            };
            let tunnel_running = tunnel_id.as_deref().is_some_and(|id| {
                matches!(
                    self.ssh_tunnel_status(id),
                    oxideterm_forwarding::ForwardStatus::Active
                        | oxideterm_forwarding::ForwardStatus::Starting
                        | oxideterm_forwarding::ForwardStatus::Suspended
                )
            });
            let open_label = if tunnel_id.is_some() {
                self.i18n.t(if tunnel_running {
                    "sessionManager.actions.stop_tunnel"
                } else {
                    "sessionManager.actions.start_tunnel"
                })
            } else {
                self.i18n.t("sessionManager.actions.connect")
            };
            let open_icon = if tunnel_id.is_some() && tunnel_running {
                "lucide/circle-stop.svg"
            } else {
                "lucide/play.svg"
            };
            popup = popup.child(self.render_session_manager_menu_action(
                oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                    &self.tokens,
                    open_icon,
                    open_label,
                    false,
                    false,
                ),
                false,
                false,
                has_background,
                move |this, _event, window, cx| {
                    if let Some(id) = tunnel_id.as_deref() {
                        if tunnel_running {
                            this.action(CatalogAction::StopTunnel(id.into()), None, cx);
                        } else {
                            this.action(
                                CatalogAction::Open {
                                    target: SessionManagerOpenTarget::Tunnel(id.into()),
                                    open_sftp: false,
                                },
                                Some(window),
                                cx,
                            );
                        }
                    } else {
                        this.open_session_manager_target_with_sftp(
                            open_target.clone(),
                            false,
                            window,
                            cx,
                        );
                    }
                    cx.stop_propagation();
                },
                cx,
            ));
        }

        if matches!(&menu.target, SessionManagerRowActionTarget::GroupRoot) {
            popup = popup.child(self.render_session_manager_menu_action(
                oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                    &self.tokens,
                    "lucide/folder-plus.svg",
                    self.i18n.t("sessionManager.folder_tree.new_group"),
                    false,
                    false,
                ),
                false,
                false,
                has_background,
                move |this, _event, _window, cx| {
                    this.action(CatalogAction::NewGroup, None, cx);
                    cx.stop_propagation();
                },
                cx,
            ));
        }

        if let SessionManagerRowActionTarget::Group(group) = &menu.target {
            let new_connection_group = group.clone();
            let subgroup_parent = group.clone();
            let rename_group = group.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/plus.svg",
                        self.i18n.t("sessionManager.toolbar.new_connection"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::NewConnectionInGroup(new_connection_group.clone()),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/folder-plus.svg",
                        self.i18n.t("sessionManager.folder_tree.new_subgroup"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, _window, cx| {
                        this.action(
                            CatalogAction::NewSubgroup(subgroup_parent.clone()),
                            None,
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.folder_tree.rename_group"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, _window, cx| {
                        this.action(CatalogAction::RenameGroup(rename_group.clone()), None, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Connection(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup.child(self.render_session_manager_menu_action(
                oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                    &self.tokens,
                    "lucide/pencil.svg",
                    self.i18n.t("sessionManager.actions.edit"),
                    false,
                    false,
                ),
                false,
                false,
                has_background,
                move |this, _event, window, cx| {
                    this.action(
                        CatalogAction::Edit(SessionManagerOpenTarget::Connection(
                            edit_id.to_owned(),
                        )),
                        Some(window),
                        cx,
                    );
                    cx.stop_propagation();
                },
                cx,
            ));

            let test_id = id.clone();
            popup = popup.child(self.render_session_manager_menu_action(
                oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                    &self.tokens,
                    "lucide/wifi.svg",
                    self.i18n.t("sessionManager.actions.test_connection"),
                    false,
                    false,
                ),
                false,
                false,
                has_background,
                move |this, _event, window, cx| {
                    this.action(CatalogAction::Test(test_id.to_owned()), Some(window), cx);
                    cx.stop_propagation();
                },
                cx,
            ));

            let duplicate_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/copy.svg",
                        self.i18n.t("sessionManager.actions.duplicate"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::Duplicate(duplicate_id.to_owned()),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::SshConfig(alias) = &menu.target {
            let import_alias = alias.clone();
            popup = popup.child(self.render_session_manager_menu_action(
                oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                    &self.tokens,
                    "lucide/download.svg",
                    self.i18n.t("settings_view.connections.ssh_config.import"),
                    false,
                    false,
                ),
                false,
                false,
                has_background,
                move |this, _event, _window, cx| {
                    this.action(
                        CatalogAction::ImportSshConfig(import_alias.clone()),
                        None,
                        cx,
                    );
                    cx.stop_propagation();
                },
                cx,
            ));
        }

        if let SessionManagerRowActionTarget::RemoteDesktop(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::RemoteDesktop(
                                edit_id.to_owned(),
                            )),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Mosh(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::Mosh(edit_id.to_owned())),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::StandaloneSftp(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::StandaloneSftp(
                                edit_id.to_owned(),
                            )),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Serial(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::Serial(
                                edit_id.to_owned(),
                            )),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Ble(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::Ble(edit_id.to_owned())),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::LocalTerminal(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::LocalTerminal(
                                edit_id.to_owned(),
                            )),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Telnet(id) | SessionManagerRowActionTarget::Ftp(id) =
            &menu.target
        {
            let ftp = matches!(&menu.target, SessionManagerRowActionTarget::Ftp(_));
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        if ftp {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Ftp(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                        } else {
                            this.action(
                                CatalogAction::Edit(SessionManagerOpenTarget::Telnet(
                                    edit_id.to_owned(),
                                )),
                                Some(window),
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Plugin(id) = &menu.target {
            let id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::Plugin(id.to_owned())),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }
        if let SessionManagerRowActionTarget::Tunnel(id) = &menu.target {
            let id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                        &self.tokens,
                        "lucide/pencil.svg",
                        self.i18n.t("sessionManager.actions.edit"),
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _, window, cx| {
                        this.action(
                            CatalogAction::Edit(SessionManagerOpenTarget::Tunnel(id.clone())),
                            Some(window),
                            cx,
                        );
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }
        let mut delete_action = match &menu.target {
            SessionManagerRowActionTarget::Tunnel(id) => {
                Some((id.clone(), self.i18n.t("sessionManager.actions.delete")))
            }
            SessionManagerRowActionTarget::Plugin(id) => {
                Some((id.clone(), self.i18n.t("sessionManager.actions.delete")))
            }
            SessionManagerRowActionTarget::Ftp(id) => {
                Some((id.clone(), self.i18n.t("sessionManager.actions.delete")))
            }
            SessionManagerRowActionTarget::Connection(id) => {
                Some((id.clone(), self.i18n.t("sessionManager.actions.delete")))
            }
            SessionManagerRowActionTarget::Serial(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.serial_profiles.delete"),
            )),
            SessionManagerRowActionTarget::Ble(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.ble_profiles.delete"),
            )),
            SessionManagerRowActionTarget::LocalTerminal(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.local_terminal_profiles.delete"),
            )),
            SessionManagerRowActionTarget::Telnet(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.telnet_profiles.delete"),
            )),
            SessionManagerRowActionTarget::Mosh(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.mosh_profiles.delete"),
            )),
            SessionManagerRowActionTarget::StandaloneSftp(id) => Some((
                id.clone(),
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.delete"),
            )),
            SessionManagerRowActionTarget::RemoteDesktop(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.remote_desktop_profiles.delete"),
            )),
            SessionManagerRowActionTarget::Group(group) => Some((
                group.clone(),
                self.i18n.t("sessionManager.folder_tree.delete_group"),
            )),
            // SSH config discoveries are transient; there is nothing to delete.
            SessionManagerRowActionTarget::SshConfig(_)
            | SessionManagerRowActionTarget::GroupRoot => None,
        };
        // Right-clicking a row that is part of the active multi-selection
        // deletes every selected item; right-clicking an unselected row
        // keeps the single-row delete flow.
        let batch_delete =
            session_manager_row_menu_selection_target(&menu.target).is_some_and(|target| {
                let selected = self.session_manager.read(cx).selected_items.clone();
                selected.len() > 1 && selected.contains(&target)
            });
        if let Some((_, delete_label)) = delete_action.as_mut() {
            if batch_delete {
                *delete_label = self.i18n.t("sessionManager.batch.delete");
            }
        }
        if let Some((delete_id, delete_label)) = delete_action {
            let delete_target = menu.target;
            popup = popup.child(self.workspace_context_menu_styled_action(
                oxideterm_gpui_ui::dropdown_menu::dropdown_menu_item_with_icon(
                    &self.tokens,
                    "lucide/trash-2.svg",
                    delete_label,
                    true,
                    false,
                ),
                false,
                false,
                ContextMenuActionableStyle {
                    hover_background: Some(rgba((self.tokens.ui.error << 8) | 0x1f)),
                    hover_text_color: None,
                },
                |_| {},
                move |this, _event, _window, cx| {
                    this.close_session_row_menus(cx);
                    if batch_delete {
                        this.action(CatalogAction::DeleteSelected, None, cx);
                        cx.stop_propagation();
                        return;
                    }
                    match &delete_target {
                        SessionManagerRowActionTarget::Tunnel(_) => {
                            this.session_manager.update(cx, |state, cx| {
                                state.delete_confirm = Some(SessionManagerDeleteConfirm::Batch {
                                    targets: vec![SessionManagerSelectionTarget::Tunnel(
                                        delete_id.clone(),
                                    )],
                                });
                                cx.notify();
                            });
                        }
                        SessionManagerRowActionTarget::Plugin(_) => {
                            this.session_manager.update(cx, |state, cx| {
                                state.delete_confirm = Some(SessionManagerDeleteConfirm::Batch {
                                    targets: vec![SessionManagerSelectionTarget::Plugin(
                                        delete_id.clone(),
                                    )],
                                });
                                cx.notify();
                            });
                        }
                        SessionManagerRowActionTarget::Ftp(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::Ftp(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::Connection(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::Connection(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::Serial(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::Serial(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::Ble(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::Ble(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::LocalTerminal(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::LocalTerminal(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::Telnet(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::Telnet(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::Mosh(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::Mosh(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::StandaloneSftp(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::StandaloneSftp(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::RemoteDesktop(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::RemoteDesktop(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::Group(_) => this.action(
                            CatalogAction::Delete(SessionManagerRowActionTarget::Group(
                                delete_id.clone(),
                            )),
                            None,
                            cx,
                        ),
                        SessionManagerRowActionTarget::GroupRoot
                        | SessionManagerRowActionTarget::SshConfig(_) => {}
                    }
                    cx.stop_propagation();
                },
                cx,
            ));
        }

        // Pointer menus use the click position while ellipsis menus preserve
        // their established button alignment; both mount at the window root.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                    &self.tokens,
                    "session-manager-row-menu-motion",
                    div().child(overlay_content_boundary(popup)),
                    -4.0,
                    gpui::point(0.0, 0.0),
                )),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(super) fn render_session_manager_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        has_background: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Session Manager menus share Workspace's guarded context-menu action
        // styling so dropdown and batch popovers dismiss consistently.
        self.workspace_context_menu_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: Some(theme_hover_bg(self.tokens.ui.bg_hover, has_background)),
                hover_text_color: None,
            },
            |_| {},
            move |this, event, window, cx| {
                this.close_session_row_menus(cx);
                listener(this, event, window, cx);
            },
            cx,
        )
    }

    pub(super) fn render_row_icon_button(
        &self,
        icon: LucideIcon,
        size: f32,
        icon_size: f32,
        icon_color: Rgba,
        has_background: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_icon_action_button(
            icon,
            icon_size,
            icon_color,
            IconButtonOptions {
                has_background,
                ..IconButtonOptions::opaque_toolbar(size, ButtonRadius::Sm)
            },
            listener,
            cx,
        )
    }

    pub(super) fn render_session_manager_sidebar_tree(
        &mut self,
        items: Vec<SessionManagerDisplayItem>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (roots, children) = self.session_group_tree();
        let (expanded_groups, has_search_query) = {
            let manager = self.session_manager.read(cx);
            (
                manager.expanded_groups.clone(),
                !manager.search_query.trim().is_empty(),
            )
        };
        let rows = Arc::<[SessionManagerTreeRow]>::from(session_manager_tree_rows(
            &items,
            &roots,
            &children,
            &expanded_groups,
        ));
        let has_rows = !rows.is_empty();
        self.set_session_manager_visible_selection_targets(
            session_manager_tree_visible_targets(&rows, &items),
            cx,
        );
        self.sync_session_manager_sidebar_tree_list_state(&rows, &items, cx);
        let state = self
            .session_manager
            .read(cx)
            .sidebar_tree_list_state
            .clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_sidebar_tree_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_session_manager_sidebar_tree_row(&rows, index, &items, cx)
                })
            },
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("session-manager-sidebar-tree-drop-target")
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .drag_over::<SessionManagerDrag>(move |target, _drag, _window, _cx| {
                        target
                            .border_1()
                            .border_dashed()
                            .border_color(rgb(theme.accent))
                            .bg(rgba((theme.accent << 8) | MANAGER_DRAG_ROOT_BG_ALPHA))
                    })
                    .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
                    .on_drop(cx.listener(|this, drag: &SessionManagerDrag, _window, cx| {
                        this.move_dragged_sessions_to_group(&drag.targets, None, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.open_session_manager_context_menu(
                                SessionManagerRowActionTarget::GroupRoot,
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            // Clicking empty sidebar tree space clears the
                            // selection; rows and group headers stop
                            // propagation so their own handlers keep the press.
                            this.clear_session_manager_selection(cx);
                        }),
                    )
                    // GPUI renamed the wheel listener from on_scroll_wheel.
                    .on_scroll(cx.listener(|this, _event, _window, cx| {
                        this.close_session_row_menus(cx);
                    }))
                    .child(if has_rows {
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .w_full()
                            .pt(px(4.0))
                            .child(list)
                            .into_any_element()
                    } else if has_search_query {
                        div()
                            .p_4()
                            .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sessionManager.table.no_search_results"))
                            .into_any_element()
                    } else {
                        self.render_sidebar_empty_state(
                            LucideIcon::Server,
                            "session-manager-sidebar-empty-title",
                            self.i18n.t("sessionManager.table.no_connections"),
                            None,
                            cx,
                        )
                    }),
            )
            .into_any_element()
    }
}
