use super::*;

#[derive(Clone)]
struct FileManagerBookmarkDrag {
    id: String,
    name: String,
    after: Rc<Cell<bool>>,
    tokens: ThemeTokens,
}

impl Render for FileManagerBookmarkDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .text_color(rgb(self.tokens.ui.text))
            .text_size(px(FILE_MANAGER_TEXT_SM))
            .child(self.name.clone())
    }
}

// Hallmark · pre-emit critique: P4 H5 E4 S5 R5 V4
fn file_manager_sidebar_location_presentation(
    kind: LocalSidebarLocationKind,
) -> (&'static str, LucideIcon) {
    // Keep path discovery in the domain crate while the GPUI layer owns copy and icons.
    match kind {
        LocalSidebarLocationKind::Home => ("fileManager.home", LucideIcon::Home),
        LocalSidebarLocationKind::Applications => {
            ("fileManager.applications", LucideIcon::AppWindow)
        }
        LocalSidebarLocationKind::Desktop => ("fileManager.desktop", LucideIcon::Monitor),
        LocalSidebarLocationKind::Documents => ("fileManager.documents", LucideIcon::FileText),
        LocalSidebarLocationKind::Downloads => ("fileManager.downloads", LucideIcon::Download),
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_file_manager_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("file_manager");
        let (filtered, filtered_rows) = self.file_manager.update(cx, |file_manager, _cx| {
            (file_manager.sorted_files(), file_manager.sorted_file_rows())
        });

        let mut root = div()
            .id("file-manager-view")
            .debug_selector(|| "file-manager-view".into())
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(file_manager_bg(theme.bg, has_background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.dismiss_file_manager_context_menu(cx);
                    if this.file_manager.read(cx).dialog.is_none() {
                        this.blur_file_manager_inline_inputs(cx);
                    }
                    cx.notify();
                }),
            )
            // Match the original hierarchy: favorites are an attached rail,
            // while the toolbar and file list belong to the main content.
            .child(self.render_file_manager_bookmarks(has_background, window, cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(self.render_file_manager_toolbar(has_background, window, cx))
                    .child(self.render_file_manager_list_panel(
                        filtered,
                        filtered_rows,
                        has_background,
                        window,
                        cx,
                    )),
            );

        if self
            .file_manager
            .read(cx)
            .context_menu_exit_generation
            .is_some()
        {
            let delay = oxideterm_gpui_ui::motion::duration(
                &self.tokens,
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            );
            self.file_manager.update(cx, |file_manager, cx| {
                // Dialog-opening actions retain the menu until the entity-owned exit task ends.
                file_manager.schedule_context_menu_exit(delay, cx);
            });
        }
        if self.file_manager.read(cx).dialog.is_none()
            && let Some(menu) = self.file_manager.read(cx).context_menu.clone()
        {
            root =
                root.child(self.render_file_manager_context_menu(menu, window, has_background, cx));
        }
        if self.file_manager.read(cx).dialog.is_none()
            && self.file_manager.read(cx).focused_input == Some(FileManagerInput::Path)
            && let Some(completion) =
                self.render_path_completion_overlay(PathCompletionOwner::FileManager, cx)
        {
            root = root.child(completion);
        }
        let operation_progress = self.file_manager.read(cx).operation_progress.clone();
        if let Some(progress) = operation_progress.as_ref()
            && progress.active
        {
            root = root.child(self.render_file_manager_operation_progress(progress, cx));
        }
        let surface = self.file_manager.clone();
        let window_id = window.window_handle().window_id();
        div()
            .size_full()
            .child(root)
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(bounds) = bounds.first() else {
                    return;
                };
                surface.update(cx, |file_manager, cx| {
                    if file_manager.bookmarks_surface_size != Some((window_id, bounds.size)) {
                        if file_manager
                            .bookmarks_resize
                            .is_some_and(|drag| drag.window_id != window_id)
                        {
                            file_manager.cancel_bookmarks_resize();
                        }
                        file_manager.bookmarks_surface_size = Some((window_id, bounds.size));
                        cx.notify();
                    }
                });
            })
            .into_any_element()
    }

    fn render_file_manager_toolbar(
        &self,
        has_background: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (current_path, bookmarks_visible) = {
            let file_manager = self.file_manager.read(cx);
            (file_manager.path.clone(), file_manager.bookmarks_visible)
        };
        let bookmarked = self.is_file_manager_path_bookmarked(&current_path, cx);
        div()
            .h(px(FILE_MANAGER_TOOLBAR_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .p(px(8.0))
            .border_b_1()
            .border_color(file_manager_border(theme.border, has_background))
            .bg(file_manager_panel_bg(
                theme.bg_panel,
                has_background,
                FILE_MANAGER_PANEL_80_ALPHA,
            ))
            .when(!bookmarks_visible, |toolbar| {
                // Once the favorites rail is fully hidden, its restore action belongs
                // to the persistent file-manager toolbar instead of an empty rail.
                toolbar.child(self.render_file_manager_icon_button(
                    LucideIcon::PanelLeft,
                    self.i18n.t("fileManager.expandSidebar"),
                    cx.listener(|this, _event, _window, cx| {
                        this.file_manager.update(cx, |file_manager, cx| {
                            file_manager.bookmarks_visible = true;
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                    cx.entity(),
                ))
            })
            .child(
                div()
                    .text_size(px(FILE_MANAGER_TEXT_SM))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.render_selectable_display_text(
                        "file-manager-title",
                        (),
                        self.i18n.t("fileManager.title"),
                        theme.text,
                        cx,
                    )),
            )
            .child(div().flex_1())
            .child(self.render_file_manager_icon_button(
                LucideIcon::FolderPlus,
                self.i18n.t("fileManager.newFolder"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.open_file_manager_new_folder_dialog(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::FilePlus,
                self.i18n.t("fileManager.newFile"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.open_file_manager_new_file_dialog(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(20.0))
                    .bg(file_manager_border(theme.border, has_background)),
            )
            .child(self.render_file_manager_icon_button(
                LucideIcon::Copy,
                self.i18n.t("fileManager.copy"),
                cx.listener(|this, _event, _window, cx| {
                    this.copy_file_manager_selection(false, cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::Pencil,
                self.i18n.t("fileManager.cut"),
                cx.listener(|this, _event, _window, cx| {
                    this.copy_file_manager_selection(true, cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::Download,
                self.i18n.t("fileManager.paste"),
                cx.listener(|this, _event, _window, cx| {
                    this.paste_file_manager_clipboard(cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(20.0))
                    .bg(file_manager_border(theme.border, has_background)),
            )
            .child(self.render_file_manager_icon_button(
                LucideIcon::Star,
                self.i18n.t(if bookmarked {
                    "fileManager.removeBookmark"
                } else {
                    "fileManager.addBookmark"
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.toggle_file_manager_current_bookmark(cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(20.0))
                    .bg(file_manager_border(theme.border, has_background)),
            )
            .child(self.render_file_manager_icon_button(
                LucideIcon::HardDrive,
                self.i18n.t("fileManager.showDrives"),
                cx.listener(|this, _event, _window, cx| {
                    // Drive switching must remain a primary action for removable
                    // media and Windows volumes, not a context-menu-only command.
                    this.blur_file_manager_inline_inputs(cx);
                    this.open_file_manager_drives_dialog(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::FolderOpen,
                self.i18n.t("fileManager.browse"),
                cx.listener(|this, _event, _window, cx| {
                    this.browse_file_manager_folder(cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::RefreshCw,
                self.i18n.t("fileManager.refresh"),
                cx.listener(|this, _event, _window, cx| {
                    this.refresh_file_manager_with_drives(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    fn render_file_manager_bookmarks(
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

    fn render_file_manager_sidebar_section_header(
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

    fn render_file_manager_sidebar_path_row(
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

    fn render_file_manager_sidebar_bookmark_row(
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

    fn render_file_manager_sidebar_selection(
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

    fn begin_file_manager_sidebar_transition(
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

    fn file_manager_sidebar_geometry_for_path(
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

    fn animate_file_manager_bookmarks_width(
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

    fn render_file_manager_list_panel(
        &self,
        files: Arc<Vec<LocalFileEntry>>,
        rows: Arc<Vec<FileManagerListRow>>,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            // The page toolbar already establishes the file-manager surface.
            // Keep the list body unframed so its internal dividers define hierarchy.
            .bg(file_manager_bg(theme.bg, has_background))
            .child(self.render_file_manager_header(has_background, window, cx))
            .child(self.render_file_manager_columns(has_background, cx))
            .child(self.render_file_manager_filter(has_background, cx))
            .child(self.render_file_manager_file_list(files, rows, has_background, cx))
            .into_any_element()
    }

    fn render_file_manager_header(
        &self,
        has_background: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(FILE_MANAGER_HEADER_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(FILE_MANAGER_HEADER_GAP))
            .p(px(8.0))
            .border_b_1()
            .border_color(file_manager_border(theme.border, has_background))
            .bg(file_manager_panel_bg(theme.bg_panel, has_background, 0xff))
            .child(
                div()
                    .min_w(px(FILE_MANAGER_HEADER_TITLE_MIN_WIDTH))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_display_text(
                        "file-manager-local-title",
                        (),
                        self.i18n.t("fileManager.local").to_uppercase(),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(self.render_file_manager_path_bar(has_background, cx))
            .child(self.render_file_manager_icon_button(
                LucideIcon::ArrowUp,
                self.i18n.t("fileManager.goUp"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.navigate_file_manager_parent(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::Home,
                self.i18n.t("fileManager.home"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.set_file_manager_path(home_path(), cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::RefreshCw,
                self.i18n.t("fileManager.refresh"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs(cx);
                    this.refresh_file_manager_with_drives(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                }),
            )
            .into_any_element()
    }

    fn render_file_manager_path_bar(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let input = FileManagerInput::Path;
        let (editing, focused, value) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.editing_path,
                file_manager.focused_input == Some(input),
                if file_manager.editing_path {
                    file_manager.path_input.clone()
                } else {
                    file_manager.path.clone()
                },
            )
        };
        div()
            .flex_1()
            .min_w(px(0.0))
            .h(px(24.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .px(px(FILE_MANAGER_PATH_BAR_HORIZONTAL_PADDING))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if focused {
                rgb(theme.accent)
            } else {
                file_manager_border(theme.border, has_background)
            })
            .bg(file_manager_bg(theme.bg_sunken, has_background))
            .overflow_hidden()
            .cursor_pointer()
            .when(editing, |bar| {
                bar.child(self.render_file_manager_inline_text(input, &value, focused, cx))
                    .child(
                        div()
                            .size(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::CornerDownLeft,
                                FILE_MANAGER_ICON_SM,
                                rgb(theme.text),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.commit_file_manager_path_input(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
            })
            .when(!editing, |bar| {
                bar.child(self.render_file_manager_breadcrumb(&value, cx))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    if editing || event.click_count >= 2 {
                        this.start_file_manager_path_edit(cx);
                    } else {
                        this.file_manager.update(cx, |file_manager, cx| {
                            file_manager.focused_input = None;
                            cx.notify();
                        });
                        this.ime_marked_text = None;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_file_manager_breadcrumb(&self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let segments = file_manager_path_segments(path);
        let mut inner = div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(FILE_MANAGER_BREADCRUMB_ROW_GAP));
        for (index, segment) in segments.iter().cloned().enumerate() {
            if index > 0 {
                inner = inner.child(Self::render_lucide_icon(
                    LucideIcon::ChevronRight,
                    FILE_MANAGER_ICON_MD,
                    rgb(theme.text_muted),
                ));
            }
            let is_last = index + 1 == segments.len();
            let full_path = segment.full_path.clone();
            let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
                "file-manager-breadcrumb-segment",
                &segment.full_path,
            );
            let segment_text_color = if is_last {
                theme.text_heading
            } else {
                theme.text
            };
            inner = inner.child(
                div()
                    .max_w(px(120.0))
                    .h(px(20.0))
                    .px(px(FILE_MANAGER_BREADCRUMB_SEGMENT_PADDING))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(FILE_MANAGER_BREADCRUMB_CONTENT_GAP))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(if is_last {
                        rgba((theme.bg_hover << 8) | FILE_MANAGER_BREADCRUMB_ACTIVE_ALPHA)
                    } else {
                        rgba(theme.bg_hover << 8)
                    })
                    .hover(move |crumb| {
                        crumb.bg(rgba(
                            (theme.bg_hover << 8) | FILE_MANAGER_BREADCRUMB_HOVER_ALPHA,
                        ))
                    })
                    .text_color(if is_last {
                        rgb(theme.text_heading)
                    } else {
                        rgb(theme.text)
                    })
                    .when(is_last, |item| item.font_weight(gpui::FontWeight::MEDIUM))
                    .when(index == 0, |item| {
                        item.child(Self::render_lucide_icon(
                            if segment.root_is_drive {
                                LucideIcon::HardDrive
                            } else {
                                LucideIcon::Home
                            },
                            FILE_MANAGER_ICON_MD,
                            rgb(theme.text_muted),
                        ))
                    })
                    .child(div().truncate().child(
                        self.render_row_safe_selectable_display_text_in_group(
                            selection_group_id,
                            "file-manager-breadcrumb-cell",
                            ("name", segment.full_path.as_str()),
                            0,
                            segment.name,
                            segment_text_color,
                            None,
                            cx,
                        ),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_file_manager_path(full_path.clone(), cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }

        div()
            .id("file-manager-breadcrumb-scroll")
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .overflow_hidden()
            .track_scroll(&self.file_manager.read(cx).path_scroll)
            .text_size(px(FILE_MANAGER_TEXT_SM))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                this.handle_file_manager_breadcrumb_scroll(event, cx);
            }))
            .child(inner)
            .into_any_element()
    }

    fn render_file_manager_inline_text(
        &self,
        input: FileManagerInput,
        value: &str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::FileManager(input);
        let control = self
            .workspace_text_input(
                target,
                TextInputView {
                    value,
                    placeholder: self.i18n.t("fileManager.pathPlaceholder"),
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .px(px(0.0))
            .border_0()
            .bg(rgba(0x00000000))
            .text_size(px(FILE_MANAGER_TEXT_XS));

        // The shared primitive keeps address completion geometry out of the render update path.
        self.text_input_with_workspace_ime(
            target,
            control,
            move |this, cx| {
                this.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_input = Some(input);
                    cx.notify();
                });
                this.dismiss_file_manager_context_menu(cx);
            },
            cx,
        )
        .into_any_element()
    }

    fn render_file_manager_columns(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(28.0))
            .flex()
            .items_center()
            .px(px(8.0))
            .border_b_1()
            .border_color(file_manager_border(self.tokens.ui.border, has_background))
            .bg(file_manager_panel_bg(
                self.tokens.ui.bg_panel,
                has_background,
                0xff,
            ))
            .child(self.render_file_manager_column(
                self.i18n.t("fileManager.colName"),
                LocalSortField::Name,
                true,
                cx,
            ))
            .child(self.render_file_manager_column(
                self.i18n.t("fileManager.colSize"),
                LocalSortField::Size,
                false,
                cx,
            ))
            .child(self.render_file_manager_column(
                self.i18n.t("fileManager.colModified"),
                LocalSortField::Modified,
                false,
                cx,
            ))
            .into_any_element()
    }

    fn render_file_manager_column(
        &self,
        label: String,
        field: LocalSortField,
        flexible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (active, sort_direction) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.sort_field == field,
                file_manager.sort_direction,
            )
        };
        let field_key = match field {
            LocalSortField::Name => "name",
            LocalSortField::Size => "size",
            LocalSortField::Modified => "modified",
        };
        let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
            "file-manager-sort-header",
            field_key,
        );
        let text_color = if active {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.text_muted
        };
        div()
            .when(flexible, |col| col.flex_1().min_w(px(0.0)))
            .when(!flexible && field == LocalSortField::Size, |col| {
                col.w(px(FILE_MANAGER_SIZE_COL)).flex_none()
            })
            .when(!flexible && field == LocalSortField::Modified, |col| {
                col.w(px(FILE_MANAGER_MODIFIED_COL)).flex_none()
            })
            .h_full()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .text_color(if active {
                rgb(self.tokens.ui.accent)
            } else {
                rgb(self.tokens.ui.text_muted)
            })
            .cursor_pointer()
            .child(
                div()
                    .when(flexible, |label| label.flex_1().min_w(px(0.0)))
                    .when(!flexible, |label| label.flex_none())
                    .truncate()
                    .whitespace_nowrap()
                    .child(self.render_row_safe_selectable_display_text_in_group(
                        selection_group_id,
                        "file-manager-sort-header-cell",
                        field_key,
                        0,
                        label,
                        text_color,
                        None,
                        cx,
                    )),
            )
            .when(active, |col| {
                col.child(Self::render_lucide_icon(
                    match sort_direction {
                        LocalSortDirection::Asc => LucideIcon::ArrowUpAZ,
                        LocalSortDirection::Desc => LucideIcon::ArrowDownAZ,
                    },
                    FILE_MANAGER_ICON_SM,
                    rgb(self.tokens.ui.accent),
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.toggle_file_manager_sort(field, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_file_manager_filter(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let input = FileManagerInput::Filter;
        let (focused, filter) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.focused_input == Some(input),
                file_manager.filter.clone(),
            )
        };
        let target = WorkspaceImeTarget::FileManager(input);
        self.sidebar_search_row(file_manager_panel_bg(theme.bg_panel, has_background, 0xff))
            .child(
                self.text_input_with_workspace_ime(
                    target,
                    self.workspace_text_input(
                        target,
                        TextInputView {
                            value: &filter,
                            placeholder: self.i18n.t("fileManager.filterPlaceholder"),
                            focused,
                            caret_visible: self.input_caret.visible(),
                            secret: false,
                            selected_all: false,
                            selected_range: self.ime_selected_range_for_target(target, cx),
                            marked_text: self.marked_text_for_target(target, cx),
                        },
                    )
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_0()
                    .border_0()
                    .rounded_none()
                    .bg(rgba(0x00000000))
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .line_height(px(20.0)),
                    |this, cx| {
                        this.file_manager.update(cx, |file_manager, cx| {
                            file_manager.focused_input = Some(FileManagerInput::Filter);
                            cx.notify();
                        });
                        this.dismiss_file_manager_context_menu(cx);
                    },
                    cx,
                ),
            )
            .into_any_element()
    }

    fn render_file_manager_file_list(
        &self,
        files: Arc<Vec<LocalFileEntry>>,
        rows: Arc<Vec<FileManagerListRow>>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = self.tokens;
        let theme = tokens.ui;
        let list = div()
            .id("file-manager-list-scroll")
            .flex_1()
            .min_h(px(0.0))
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                // Context-menu coordinates and row payloads describe the
                // pre-scroll list, so scrolling invalidates both immediately.
                if this.clear_file_manager_context_menu_immediately(cx) {
                    cx.notify();
                }
            }))
            .bg(file_manager_bg(theme.bg, has_background));
        let (loading, error, list_scroll) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.loading,
                file_manager.error.clone(),
                file_manager.list_scroll.clone(),
            )
        };
        if loading {
            return list
                .child(
                    div()
                        .w_full()
                        .py(px(48.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .text_size(px(FILE_MANAGER_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_loading_icon(
                            "file-manager-list-loading",
                            20.0,
                            rgb(theme.text_muted),
                        ))
                        .child(self.render_selectable_display_text(
                            "file-manager-list-loading",
                            (),
                            self.i18n.t("sftp.file_list.loading"),
                            theme.text_muted,
                            cx,
                        )),
                )
                .into_any_element();
        }
        if let Some(error) = error.as_ref() {
            return list
                .child(
                    div()
                        .m(px(12.0))
                        .p(px(12.0))
                        .rounded(px(self.tokens.radii.sm))
                        .border_1()
                        .border_color(rgba((FILE_MANAGER_RED << 8) | 0x80))
                        .bg(rgba((FILE_MANAGER_RED << 8) | 0x14))
                        .text_size(px(FILE_MANAGER_TEXT_XS))
                        .text_color(rgb(FILE_MANAGER_RED))
                        .child(self.render_selectable_text_scoped(
                            "file-manager-list-error",
                            (),
                            error.clone(),
                            FILE_MANAGER_RED,
                            cx,
                        )),
                )
                .into_any_element();
        }
        if files.is_empty() {
            return list
                .child(
                    div()
                        .w_full()
                        .py(px(48.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .text_size(px(FILE_MANAGER_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(
                            div()
                                .mb(px(8.0))
                                .opacity(0.4)
                                .child(Self::render_lucide_icon(
                                    LucideIcon::FolderOpen,
                                    32.0,
                                    rgb(theme.text_muted),
                                )),
                        )
                        .child(self.render_selectable_display_text(
                            "file-manager-list-empty",
                            (),
                            self.i18n.t("fileManager.empty"),
                            theme.text_muted,
                            cx,
                        )),
                )
                .into_any_element();
        }

        let file_manager = self.file_manager.clone();
        let row_count = files.len();
        let list_items = files;
        let row_items = rows;
        list.child(
            tauri_virtual_uniform_list(
                "file-manager-list-virtual",
                row_count,
                list_scroll,
                file_manager_list_virtual_spec(),
                move |range, _window, _cx| {
                    range
                        .map(|index| {
                            let file = &list_items[index];
                            let row = &row_items[index];
                            // Selection remains page-owned and is sampled only
                            // for rows requested by the virtual-list viewport.
                            let selected = file_manager.read(_cx).selected.contains(&file.name);
                            div()
                                .w_full()
                                .h(px(FILE_MANAGER_ROW_HEIGHT))
                                .flex()
                                .flex_row()
                                .items_center()
                                .px(px(8.0))
                                .py(px(4.0))
                                .border_b_1()
                                .border_color(rgba(theme.border << 8))
                                .text_size(px(FILE_MANAGER_TEXT_XS))
                                .text_color(if selected {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text)
                                })
                                .bg(if selected {
                                    rgba((theme.accent << 8) | FILE_MANAGER_SELECTED_BG_ALPHA)
                                } else {
                                    rgba(theme.bg << 8)
                                })
                                .hover(move |row| {
                                    row.bg(file_manager_hover_bg(theme.bg_hover, has_background))
                                })
                                .cursor_pointer()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(row.icon.render(FILE_MANAGER_ICON_MD, &tokens))
                                        // Tauri marks file rows as `select-none`. Plain text keeps
                                        // virtual-list scrolling free of per-cell anchor probes.
                                        .child(div().truncate().child(row.display_name.clone())),
                                )
                                .child(
                                    div()
                                        .w(px(FILE_MANAGER_SIZE_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(row.size_text.clone()),
                                )
                                .child(
                                    div()
                                        .w(px(FILE_MANAGER_MODIFIED_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(row.modified_text.clone()),
                                )
                                .on_mouse_down(MouseButton::Left, {
                                    let file_manager = file_manager.clone();
                                    let visible = list_items.clone();
                                    let entry = file.clone();
                                    move |event: &MouseDownEvent, _window, cx| {
                                        file_manager.update(cx, |file_manager, cx| {
                                            if event.click_count >= 2 {
                                                file_manager.activate_entry(entry.clone(), cx);
                                            } else {
                                                file_manager.clear_context_menu_immediately();
                                                file_manager.select_entry(
                                                    entry.name.clone(),
                                                    event.modifiers,
                                                    visible.as_ref(),
                                                );
                                                cx.notify();
                                            }
                                            cx.stop_propagation();
                                        });
                                    }
                                })
                                .on_mouse_down(MouseButton::Right, {
                                    let file_manager = file_manager.clone();
                                    let entry = file.clone();
                                    move |event: &MouseDownEvent, _window, cx| {
                                        file_manager.update(cx, |file_manager, cx| {
                                            file_manager.open_context_menu(
                                                Some(entry.clone()),
                                                f32::from(event.position.x),
                                                f32::from(event.position.y),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        });
                                    }
                                })
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                },
            )
            .drag_over::<gpui::ExternalPaths>({
                let theme = self.tokens.ui;
                move |style, _paths, _window, _cx| {
                    style
                        .bg(rgba((theme.accent << 8) | 0x1a))
                        .border_color(rgba((theme.accent << 8) | 0x4d))
                }
            })
            .can_drop(|drag, _window, _cx| drag.is::<gpui::ExternalPaths>())
            .on_drop(
                cx.listener(|this, paths: &gpui::ExternalPaths, _window, cx| {
                    this.queue_file_manager_external_drop_paths(paths.paths(), cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.dismiss_file_manager_context_menu(cx);
                    this.blur_file_manager_inline_inputs(cx);
                    this.clear_file_manager_selection(cx);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.open_file_manager_context_menu(
                        None,
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        cx,
                    );
                    cx.stop_propagation();
                    cx.notify();
                }),
            ),
        )
        .into_any_element()
    }

    fn render_file_manager_operation_progress(
        &self,
        progress: &FileManagerOperationProgress,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let percent = if progress.total > 0 {
            ((progress.current as f32 / progress.total as f32) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let label = if progress.file_name.is_empty() {
            self.i18n.t("fileManager.progressPreparing")
        } else {
            self.i18n
                .t("fileManager.progressFile")
                .replace("{{name}}", &progress.file_name)
        };
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_elevated << 8) | 0xf2))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(div().max_w(relative(0.7)).truncate().child(
                        self.render_selectable_display_text(
                            "file-manager-operation-label",
                            (&progress.file_name, progress.total),
                            label,
                            theme.text_muted,
                            cx,
                        ),
                    ))
                    .child(self.render_selectable_display_text(
                        "file-manager-operation-count",
                        (&progress.file_name, progress.current, progress.total),
                        format!(
                            "{}/{} ({}%)",
                            progress.current,
                            progress.total,
                            percent.round() as u32
                        ),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .h(px(6.0))
                    .rounded(px(self.tokens.radii.sm))
                    .overflow_hidden()
                    .bg(rgb(theme.bg_sunken))
                    .child(
                        div()
                            .h_full()
                            .w(relative(percent / 100.0))
                            .rounded(px(self.tokens.radii.sm))
                            .bg(rgb(theme.accent)),
                    ),
            )
            .into_any_element()
    }

    fn render_file_manager_icon_button(
        &self,
        icon: LucideIcon,
        tooltip: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        workspace: gpui::Entity<Self>,
    ) -> AnyElement {
        self.workspace_tooltip_icon_button(
            icon,
            FILE_MANAGER_ICON_MD,
            rgb(self.tokens.ui.text),
            IconButtonOptions::opaque_toolbar(FILE_MANAGER_TOOL_BUTTON, ButtonRadius::Sm),
            tooltip,
            "file-manager-icon-button",
            false,
            listener,
            workspace,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn favorites_drag_and_resize_preserve_navigation_and_save_on_release(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_FAVORITES_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; keep real settings and credentials isolated.
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
        let settings_path = default_settings_path();
        assert!(settings_path.starts_with(PathBuf::from(&fixture_dir)));
        let mut settings = SettingsStore::load_from_path(settings_path.clone()).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().appearance.animation_speed =
            oxideterm_settings::AnimationSpeed::Off;
        settings.save().unwrap();
        for id in ["a", "b", "c"] {
            std::fs::create_dir(PathBuf::from(&fixture_dir).join(id)).unwrap();
        }
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, cx| {
                workspace.open_file_manager_tab(window, cx);
                workspace.file_manager.update(cx, |state, cx| {
                    state.bookmarks = ["a", "b", "c"]
                        .into_iter()
                        .map(|id| LocalBookmark {
                            id: id.into(),
                            name: id.into(),
                            path: PathBuf::from(&fixture_dir)
                                .join(id)
                                .to_string_lossy()
                                .into_owned(),
                            created_at: 1,
                        })
                        .collect();
                    cx.notify();
                });
            });
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let initial_path = workspace.read_with(cx, |workspace, cx| {
            workspace.file_manager.read(cx).path.clone()
        });
        let row_a = cx.debug_bounds("file-manager-bookmark-a").unwrap();
        let row_b = cx.debug_bounds("file-manager-bookmark-b").unwrap();
        let start = gpui::point(row_a.left() + px(40.0), row_a.center().y);
        let end = gpui::point(row_b.left() + px(40.0), row_b.bottom() - px(3.0));
        let modifiers = gpui::Modifiers::default();
        cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(end, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(end, MouseButton::Left, modifiers);
        cx.simulate_mouse_up(end, MouseButton::Left, modifiers);
        workspace.read_with(cx, |workspace, cx| {
            let state = workspace.file_manager.read(cx);
            assert_eq!(
                state.path, initial_path,
                "dragging must not navigate to a favorite"
            );
            assert_eq!(
                state
                    .bookmarks
                    .iter()
                    .map(|bookmark| bookmark.id.as_str())
                    .collect::<Vec<_>>(),
                ["b", "a", "c"]
            );
        });

        let row_a = cx.debug_bounds("file-manager-bookmark-a").unwrap();
        let start = gpui::point(row_a.left() + px(40.0), row_a.center().y);
        let outside = gpui::point(px(-10.0), px(-10.0));
        cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(outside, MouseButton::Left, modifiers);
        cx.simulate_mouse_up(outside, MouseButton::Left, modifiers);
        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(workspace.file_manager.read(cx).bookmarks[0].id, "b");
        });
        cx.simulate_click(start, modifiers);
        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(
                workspace.file_manager.read(cx).path,
                PathBuf::from(&fixture_dir).join("a").to_string_lossy()
            );
        });

        let edge = cx
            .debug_bounds("file-manager-bookmarks-resize")
            .unwrap()
            .center();
        let wider = gpui::point(edge.x + px(80.0), edge.y);
        cx.simulate_mouse_down(edge, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(wider, MouseButton::Left, modifiers);
        assert_eq!(
            SettingsStore::load_from_path(settings_path.clone())
                .unwrap()
                .settings()
                .sidebar_ui
                .file_manager_sidebar_width,
            184
        );
        cx.simulate_mouse_up(wider, MouseButton::Left, modifiers);
        assert_eq!(
            SettingsStore::load_from_path(settings_path.clone())
                .unwrap()
                .settings()
                .sidebar_ui
                .file_manager_sidebar_width,
            264
        );
        let resized_edge = cx
            .debug_bounds("file-manager-bookmarks-resize")
            .unwrap()
            .center();
        assert_eq!(resized_edge.x - edge.x, px(80.0));

        cx.simulate_mouse_down(resized_edge, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(
            gpui::point(resized_edge.x + px(20.0), resized_edge.y),
            MouseButton::Left,
            modifiers,
        );
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(resized_edge, MouseButton::Left, modifiers);
        assert_eq!(
            cx.debug_bounds("file-manager-bookmarks-resize")
                .unwrap()
                .center()
                .x,
            resized_edge.x
        );
        assert_eq!(
            SettingsStore::load_from_path(settings_path.clone())
                .unwrap()
                .settings()
                .sidebar_ui
                .file_manager_sidebar_width,
            264
        );
        cx.simulate_event(MouseDownEvent {
            position: resized_edge,
            button: MouseButton::Left,
            modifiers,
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_mouse_up(resized_edge, MouseButton::Left, modifiers);
        assert_eq!(
            SettingsStore::load_from_path(settings_path.clone())
                .unwrap()
                .settings()
                .sidebar_ui
                .file_manager_sidebar_width,
            184
        );

        let detached = cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                let tab = workspace.active_content_tab_id(cx).unwrap();
                workspace.detach_tab_to_window(tab, None, window, cx);
                workspace
                    .tab_host
                    .read(cx)
                    .detached_window_handle(tab)
                    .unwrap()
            })
        });
        cx.run_until_parked();
        let cx = &mut gpui::VisualTestContext::from_window(detached, cx);
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
        let edge = cx
            .debug_bounds("file-manager-bookmarks-resize")
            .unwrap()
            .center();
        cx.simulate_mouse_down(edge, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(
            gpui::point(edge.x + px(40.0), edge.y),
            MouseButton::Left,
            modifiers,
        );
        assert_eq!(
            cx.debug_bounds("file-manager-bookmarks-resize")
                .unwrap()
                .center()
                .x,
            edge.x + px(40.0)
        );
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(edge, MouseButton::Left, modifiers);
        assert_eq!(
            cx.debug_bounds("file-manager-bookmarks-resize")
                .unwrap()
                .center()
                .x,
            edge.x
        );
        assert_eq!(
            SettingsStore::load_from_path(settings_path)
                .unwrap()
                .settings()
                .sidebar_ui
                .file_manager_sidebar_width,
            184
        );
    }
}
