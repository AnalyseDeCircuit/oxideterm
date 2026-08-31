use super::*;

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
            .child(self.render_file_manager_bookmarks(has_background, cx))
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
        root.into_any_element()
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
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(file_manager_panel_bg(
                theme.bg_card,
                has_background,
                FILE_MANAGER_PANEL_80_ALPHA,
            ))
            .when(expanded, |panel| {
                panel
                    .border_r_1()
                    .border_color(file_manager_border(theme.border, has_background))
            })
            .overflow_hidden();

        if !expanded {
            return self.animate_file_manager_bookmarks_width(panel, expanded);
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
        self.animate_file_manager_bookmarks_width(panel, expanded)
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let path = bookmark.path;
                    move |this, _event, _window, cx| {
                        this.blur_file_manager_inline_inputs(cx);
                        this.begin_file_manager_sidebar_transition(&path, item_geometry, cx);
                        this.set_file_manager_path(path.clone(), cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
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
            let Some((generation, vertical_offset_y)) = self.segmented_control_user_transition(
                selection_motion::FILE_MANAGER_NAVIGATION_ID,
                item_geometry.transition_index,
            ) else {
                return surface.into_any_element();
            };
            let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
                return surface.into_any_element();
            };
            let animation_id = (
                gpui::ElementId::from(selection_motion::FILE_MANAGER_NAVIGATION_ID),
                format!("selection-{generation}"),
            );
            if motion.spatial
                && let Some(vertical_offset_y) = vertical_offset_y
            {
                // Animate only the indicator; icons and labels stay at their
                // final row just like the Settings navigation treatment.
                return surface
                    .with_animation(
                        animation_id,
                        Animation::new(motion.duration)
                            .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                        move |surface, progress| {
                            let offset =
                                oxideterm_gpui_ui::motion::lerp(vertical_offset_y, 0.0, progress);
                            surface.top(px(offset)).bottom(px(-offset))
                        },
                    )
                    .into_any_element();
            }

            surface
                .with_animation(
                    animation_id,
                    Animation::new(motion.duration)
                        .with_easing(oxideterm_gpui_ui::motion::ease_out_cubic),
                    |surface, progress| surface.opacity(progress),
                )
                .into_any_element()
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

    fn animate_file_manager_bookmarks_width(&self, panel: gpui::Div, expanded: bool) -> AnyElement {
        let target_width = if expanded {
            FILE_MANAGER_SIDEBAR_WIDTH
        } else {
            FILE_MANAGER_SIDEBAR_HIDDEN_WIDTH
        };
        if !self.tokens.motion.enabled || !self.tokens.motion.spatial_enabled {
            // Reduced and Off change layout synchronously without an animation duration.
            return panel.w(px(target_width)).into_any_element();
        }

        panel
            .with_animation(
                ("file-manager-bookmarks-width", expanded as usize),
                Animation::new(oxideterm_gpui_ui::motion::duration(
                    &self.tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Control,
                ))
                .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                move |panel, progress| {
                    let width = if expanded {
                        oxideterm_gpui_ui::motion::lerp(
                            FILE_MANAGER_SIDEBAR_HIDDEN_WIDTH,
                            FILE_MANAGER_SIDEBAR_WIDTH,
                            progress,
                        )
                    } else {
                        oxideterm_gpui_ui::motion::lerp(
                            FILE_MANAGER_SIDEBAR_WIDTH,
                            FILE_MANAGER_SIDEBAR_HIDDEN_WIDTH,
                            progress,
                        )
                    };
                    panel.w(px(width))
                },
            )
            .into_any_element()
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
        let control = text_input(
            &self.tokens,
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
        div()
            .h(px(32.0))
            .px(px(8.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(file_manager_border(theme.border, has_background))
            .bg(file_manager_panel_bg(theme.bg_panel, has_background, 0xff))
            .child(
                self.text_input_with_workspace_ime(
                    target,
                    text_input(
                        &self.tokens,
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
                    .h(px(24.0))
                    .bg(file_manager_bg(theme.bg_sunken, has_background)),
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
        let theme = self.tokens.ui;
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
                            let icon_color = if row.icon_color == 0 {
                                theme.text_muted
                            } else {
                                row.icon_color
                            };
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
                                        .child(Self::render_lucide_icon(
                                            row.icon,
                                            FILE_MANAGER_ICON_MD,
                                            rgb(icon_color),
                                        ))
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
