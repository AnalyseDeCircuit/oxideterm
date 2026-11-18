use super::*;

impl FileManagerView {
    pub(super) fn render_file_manager_bookmarks(
        &self,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let expanded = self.file_manager.read(cx).bookmarks_visible;
        let (sidebar_locations, bookmarks, drives) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.sidebar_locations.clone(),
                file_manager.bookmarks.clone(),
                file_manager.drives.clone(),
            )
        };
        let mut panel = div()
            .relative()
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(file_manager_panel_bg(
                theme.bg_card,
                has_background,
                FILE_MANAGER_PANEL_80_ALPHA,
            ))
            .overflow_hidden();

        if !expanded {
            return self.animate_file_manager_bookmarks_width(panel, expanded, window, cx);
        }

        let mut navigation = div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scrollbar()
            .py(px(8.0))
            .child(self.render_file_manager_sidebar_section_header(
                self.i18n.t("fileManager.favorites"),
                true,
                true,
                cx,
            ));
        let mut item_geometry = FileManagerSidebarItemGeometry::FIRST;
        for location in sidebar_locations {
            let (label_key, icon) = file_manager_sidebar_location_presentation(location.kind);
            navigation = navigation.child(self.render_file_manager_sidebar_path_row(
                location.path,
                self.i18n.t(label_key),
                icon,
                theme.accent,
                has_background,
                item_geometry,
                cx,
            ));
            item_geometry = item_geometry.next();
        }
        for bookmark in bookmarks {
            navigation = navigation.child(self.render_file_manager_sidebar_bookmark_row(
                bookmark,
                has_background,
                item_geometry,
                cx,
            ));
            item_geometry = item_geometry.next();
        }
        navigation = navigation.child(div().mt(px(10.0)).child(
            self.render_file_manager_sidebar_section_header(
                self.i18n.t("fileManager.sidebarLocations"),
                false,
                false,
                cx,
            ),
        ));
        item_geometry = item_geometry.after_section_header();
        for drive in &drives {
            navigation = navigation.child(self.render_file_manager_sidebar_path_row(
                drive.path.clone(),
                drive.name.clone(),
                LucideIcon::HardDrive,
                theme.text_secondary,
                has_background,
                item_geometry,
                cx,
            ));
            item_geometry = item_geometry.next();
        }
        panel = panel.child(navigation);
        panel = panel.child(
            div()
                .border_t_1()
                .border_color(file_manager_border(theme.border, has_background))
                .p(px(8.0))
                .child(
                    div()
                        .h(px(28.0))
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .rounded(px(self.tokens.radii.sm))
                        .cursor_pointer()
                        .hover(move |button| {
                            button.bg(file_manager_hover_bg(theme.bg_hover, has_background))
                        })
                        .child(Self::render_lucide_icon(
                            LucideIcon::Terminal,
                            FILE_MANAGER_ICON_MD,
                            rgb(theme.text),
                        ))
                        .child(
                            div()
                                .text_size(px(FILE_MANAGER_TEXT_XS))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::NonSelectable,
                                    "file-manager-action",
                                    "open-terminal-here",
                                    self.i18n.t("fileManager.openTerminalHere"),
                                    theme.text,
                                    cx,
                                )),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, window, cx| {
                                this.blur_file_manager_inline_inputs(cx);
                                this.open_terminal_at_file_manager_path(window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                ),
        );
        let label = self.i18n.t("fileManager.resizeFavorites");
        panel = panel.child(
            sidebar::sidebar_resize_hotzone_chrome(
                "file-manager-bookmarks-resize",
                file_manager_border(theme.border, has_background),
                true,
            )
            .debug_selector(|| "file-manager-bookmarks-resize".into())
            .right_0()
            .top_0()
            .bottom_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    if event.click_count >= 2 {
                        this.reset_file_manager_bookmarks_width(window, cx);
                    } else {
                        this.start_file_manager_bookmarks_resize(event, window, cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        "file-manager-bookmarks-resize",
                        label.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }),
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if !hovered {
                    this.clear_workspace_tooltip("file-manager-bookmarks-resize", cx);
                }
            })),
        );
        self.animate_file_manager_bookmarks_width(panel, expanded, window, cx)
    }

    pub(super) fn render_file_manager_sidebar_section_header(
        &self,
        label: String,
        show_sidebar_collapse: bool,
        show_add_bookmark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let title = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .when(show_sidebar_collapse, |title| {
                title.child(self.render_file_manager_icon_button(
                    LucideIcon::PanelLeftClose,
                    self.i18n.t("fileManager.collapseSidebar"),
                    cx.listener(|this, _event, _window, cx| {
                        this.file_manager.update(cx, |file_manager, cx| {
                            file_manager.bookmarks_visible = false;
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                    cx.entity(),
                ))
            })
            .child(label);
        div()
            .h(px(FILE_MANAGER_SIDEBAR_SECTION_HEADER_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .px(px(12.0))
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(theme.text_muted))
            .child(title)
            .when(show_add_bookmark, |header| {
                header.child(self.render_file_manager_icon_button(
                    LucideIcon::Plus,
                    self.i18n.t("fileManager.addBookmark"),
                    cx.listener(|this, _event, _window, cx| {
                        let current_path = this.file_manager.read(cx).path.clone();
                        if !this.is_file_manager_path_bookmarked(&current_path, cx) {
                            this.toggle_file_manager_current_bookmark(cx);
                        }
                        cx.stop_propagation();
                    }),
                    cx.entity(),
                ))
            })
            .into_any_element()
    }

    pub(super) fn render_file_manager_sidebar_path_row(
        &self,
        path: String,
        label: String,
        icon: LucideIcon,
        icon_color: u32,
        has_background: bool,
        item_geometry: FileManagerSidebarItemGeometry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = path == self.file_manager.read(cx).path;
        let selection_surface = self.render_file_manager_sidebar_selection(active, item_geometry);
        div()
            .h(px(FILE_MANAGER_SIDEBAR_ROW_HEIGHT))
            .mx(px(8.0))
            .px(px(8.0))
            .relative()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba(theme.bg << 8))
            .hover(move |row| row.bg(file_manager_hover_bg(theme.bg_hover, has_background)))
            .cursor_pointer()
            .when_some(selection_surface, |row, surface| row.child(surface))
            .child(Self::render_lucide_icon(
                icon,
                FILE_MANAGER_ICON_MD,
                rgb(icon_color),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(FILE_MANAGER_TEXT_SM))
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text)
                    })
                    .child(label),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.begin_file_manager_sidebar_transition(&path, item_geometry, cx);
                    this.set_file_manager_path(path.clone(), cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_file_manager_sidebar_bookmark_row(
        &self,
        bookmark: LocalBookmark,
        has_background: bool,
        item_geometry: FileManagerSidebarItemGeometry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = bookmark.path == self.file_manager.read(cx).path;
        let selection_surface = self.render_file_manager_sidebar_selection(active, item_geometry);
        let drag = FileManagerBookmarkDrag {
            id: bookmark.id.clone(),
            name: bookmark.name.clone(),
            after: Rc::new(Cell::new(false)),
            tokens: self.tokens,
        };
        let source_id = bookmark.id.clone();
        let target_id = bookmark.id.clone();
        let hover_id = bookmark.id.clone();
        let tooltip = format!(
            "{}\n{}\n{}",
            bookmark.name,
            bookmark.path,
            self.i18n.t("fileManager.dragToReorder")
        );
        let bookmark_selector = format!("file-manager-bookmark-{}", bookmark.id);
        div()
            .id(SharedString::from(format!(
                "file-manager-bookmark-{}",
                bookmark.id
            )))
            .debug_selector(move || bookmark_selector)
            .h(px(FILE_MANAGER_SIDEBAR_ROW_HEIGHT))
            .mx(px(8.0))
            .px(px(8.0))
            .relative()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba(theme.bg << 8))
            .hover(move |row| row.bg(file_manager_hover_bg(theme.bg_hover, has_background)))
            .cursor_pointer()
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .can_drop(move |value, _, _| {
                value
                    .downcast_ref::<FileManagerBookmarkDrag>()
                    .is_some_and(|drag| drag.id != source_id)
            })
            .on_drag_move::<FileManagerBookmarkDrag>(|event, window, cx| {
                if event.bounds.contains(&event.event.position) {
                    let after = event.event.position.y >= event.bounds.center().y;
                    if event.drag(cx).after.replace(after) != after {
                        window.refresh();
                    }
                }
            })
            .drag_over::<FileManagerBookmarkDrag>(move |style, drag, _, _| {
                if drag.id == hover_id {
                    return style;
                }
                let style = style.border_color(rgb(theme.accent));
                if drag.after.get() {
                    style.border_b_2().border_t_0()
                } else {
                    style.border_t_2().border_b_0()
                }
            })
            .on_drop(
                cx.listener(move |this, drag: &FileManagerBookmarkDrag, _, cx| {
                    this.reorder_file_manager_bookmark(&drag.id, &target_id, drag.after.get(), cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                this.queue_workspace_tooltip(
                    "file-manager-bookmark",
                    tooltip.clone(),
                    f32::from(event.position.x) + 12.0,
                    f32::from(event.position.y) + 16.0,
                    cx,
                );
            }))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if !hovered {
                    this.clear_workspace_tooltip("file-manager-bookmark", cx);
                }
            }))
            .when_some(selection_surface, |row, surface| row.child(surface))
            .child(Self::render_lucide_icon(
                LucideIcon::Folder,
                FILE_MANAGER_ICON_MD,
                rgb(theme.accent),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(FILE_MANAGER_TEXT_SM))
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text)
                    })
                    .child(bookmark.name.clone()),
            )
            .child(
                div()
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Pencil,
                        FILE_MANAGER_ICON_SM,
                        rgb(theme.text_muted),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let bookmark = bookmark.clone();
                            move |this, _event, _window, cx| {
                                this.blur_file_manager_inline_inputs(cx);
                                this.open_file_manager_edit_bookmark_dialog(bookmark.clone(), cx);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Trash2,
                        FILE_MANAGER_ICON_SM,
                        rgb(theme.text_muted),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let id = bookmark.id.clone();
                            move |this, _event, _window, cx| {
                                this.blur_file_manager_inline_inputs(cx);
                                this.remove_file_manager_bookmark(&id, cx);
                                cx.stop_propagation();
                            }
                        }),
                    ),
            )
            .on_click(cx.listener({
                let path = bookmark.path;
                move |this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.begin_file_manager_sidebar_transition(&path, item_geometry, cx);
                    this.set_file_manager_path(path.clone(), cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .into_any_element()
    }

    pub(super) fn render_file_manager_sidebar_selection(
        &self,
        active: bool,
        item_geometry: FileManagerSidebarItemGeometry,
    ) -> Option<AnyElement> {
        active.then(|| {
            let theme = self.tokens.ui;
            let surface = div()
                .absolute()
                .inset_0()
                .rounded(px(self.tokens.radii.sm))
                .bg(rgba((theme.accent << 8) | FILE_MANAGER_SELECTED_BG_ALPHA));
            self.render_vertical_selection_surface(
                selection_motion::FILE_MANAGER_NAVIGATION_ID,
                item_geometry.transition_index,
                surface,
            )
        })
    }

    pub(super) fn begin_file_manager_sidebar_transition(
        &mut self,
        target_path: &str,
        target_geometry: FileManagerSidebarItemGeometry,
        cx: &mut Context<Self>,
    ) {
        let current_path = self.file_manager.read(cx).path.clone();
        if current_path == target_path {
            return;
        }
        let vertical_offset_y = self
            .file_manager_sidebar_geometry_for_path(&current_path, cx)
            .map(|source_geometry| source_geometry.top - target_geometry.top);
        self.begin_user_segmented_control_transition_with_vertical_offset(
            selection_motion::FILE_MANAGER_NAVIGATION_ID,
            target_geometry.transition_index,
            vertical_offset_y,
            cx,
        );
    }

    pub(super) fn file_manager_sidebar_geometry_for_path(
        &self,
        path: &str,
        cx: &App,
    ) -> Option<FileManagerSidebarItemGeometry> {
        let (sidebar_locations, bookmarks, drives) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.sidebar_locations.clone(),
                file_manager.bookmarks.clone(),
                file_manager.drives.clone(),
            )
        };
        let mut item_geometry = FileManagerSidebarItemGeometry::FIRST;
        for location in &sidebar_locations {
            if location.path == path {
                return Some(item_geometry);
            }
            item_geometry = item_geometry.next();
        }
        for bookmark in &bookmarks {
            if bookmark.path == path {
                return Some(item_geometry);
            }
            item_geometry = item_geometry.next();
        }
        item_geometry = item_geometry.after_section_header();
        for drive in &drives {
            if drive.path == path {
                return Some(item_geometry);
            }
            item_geometry = item_geometry.next();
        }
        None
    }

    pub(super) fn animate_file_manager_bookmarks_width(
        &self,
        panel: gpui::Div,
        expanded: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = if expanded {
            self.file_manager_bookmarks_width(window, cx)
        } else {
            0.0
        };
        let available = layout::bookmarks_width(
            FILE_MANAGER_SIDEBAR_MAX_WIDTH as f32,
            self.file_manager_layout_width(window, cx),
        );
        self.file_manager.update(cx, |file_manager, _cx| {
            file_manager.bookmarks_motion.retarget(target);
            file_manager.bookmarks_motion.animate(
                &self.tokens,
                "file-manager-bookmarks-width",
                panel,
                move |panel, width| panel.w(px(width.min(available))),
            )
        })
    }
}
