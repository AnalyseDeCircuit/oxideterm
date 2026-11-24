mod sidebar;
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

impl FileManagerView {
    pub fn render_file_manager_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background;
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
            // The enclosing `tab_canvas_surface` owns the uniform base tint when
            // a background is active so toolbars and rails retain panel stepping.
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.pointer(FileManagerPointer::Focus, window, cx);
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
            && let Some(completion) = self.render_path_completion_overlay(cx)
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
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
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
                    this.set_file_manager_path(this.file_manager.read(cx).fs.home(), cx);
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
                    this.pointer(FileManagerPointer::Focus, window, cx);
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
                    this.pointer(FileManagerPointer::Focus, window, cx);
                    if editing || event.click_count >= 2 {
                        this.start_file_manager_path_edit(cx);
                    } else {
                        this.file_manager.update(cx, |file_manager, cx| {
                            file_manager.focused_input = None;
                            cx.notify();
                        });
                        this.request(FileManagerRequest::ClearIme, cx);
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
            let selection_group_id =
                selectable_text_id("file-manager-breadcrumb-segment", &segment.full_path);
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
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _window, cx| {
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
        let target = input;
        let control = self
            .workspace_text_input(
                target,
                TextInputView {
                    value,
                    placeholder: self.i18n.t("fileManager.pathPlaceholder"),
                    focused,
                    caret_visible: self.caret_visible,
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
        let selection_group_id = selectable_text_id("file-manager-sort-header", field_key);
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
        let target = input;
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
                            caret_visible: self.caret_visible,
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
            .on_scroll(cx.listener(|this, _event, _window, cx| {
                // Context-menu coordinates and row payloads describe the
                // pre-scroll list, so scrolling invalidates both immediately.
                if this.clear_file_manager_context_menu_immediately(cx) {
                    cx.notify();
                }
            }))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            });
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
                                        .font_features(oxideterm_gpui_ui::tabular_nums_features())
                                        .text_color(rgb(theme.text_muted))
                                        .child(row.size_text.clone()),
                                )
                                .child(
                                    div()
                                        .w(px(FILE_MANAGER_MODIFIED_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .font_features(oxideterm_gpui_ui::tabular_nums_features())
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
                    this.pointer(FileManagerPointer::Focus, window, cx);
                    this.dismiss_file_manager_context_menu(cx);
                    this.blur_file_manager_inline_inputs(cx);
                    this.clear_file_manager_selection(cx);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.pointer(FileManagerPointer::Focus, window, cx);
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
                    .child(
                        div()
                            .font_features(oxideterm_gpui_ui::tabular_nums_features())
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
                    ),
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
    use std::fs;

    #[gpui::test]
    fn dragging_favorites_reorders_without_navigating_and_click_still_opens(
        cx: &mut TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        for id in ["a", "b", "c"] {
            fs::create_dir(dir.path().join(id)).unwrap();
        }
        let (root, cx) = cx.add_window_view(|_, cx| {
            let views = test_views(&dir.path().join("settings.json"), |_, _| {}, cx);
            views.body.update(cx, |view, cx| {
                view.set_file_manager_path(dir.path().to_string_lossy().into_owned(), cx)
            });
            views.update(cx, |state, cx| {
                state.bookmarks = ["a", "b", "c"]
                    .into_iter()
                    .map(|id| LocalBookmark {
                        id: id.into(),
                        name: id.into(),
                        path: dir.path().join(id).to_string_lossy().into_owned(),
                        created_at: 1,
                    })
                    .collect();
                cx.notify();
            });
            TestFileManagerRoot(views.body)
        });
        let body = root.read_with(cx, |root, _| root.0.clone());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let initial = body.read_with(cx, |view, cx| view.file_manager.read(cx).path.clone());
        let row_a = cx.debug_bounds("file-manager-bookmark-a").unwrap();
        let row_b = cx.debug_bounds("file-manager-bookmark-b").unwrap();
        let start = gpui::point(row_a.left() + px(40.0), row_a.center().y);
        let end = gpui::point(row_b.left() + px(40.0), row_b.bottom() - px(3.0));
        let modifiers = gpui::Modifiers::default();
        cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(end, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(end, MouseButton::Left, modifiers);
        cx.simulate_mouse_up(end, MouseButton::Left, modifiers);
        body.read_with(cx, |view, cx| {
            let state = view.file_manager.read(cx);
            assert_eq!(state.path, initial);
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
        cx.simulate_click(
            gpui::point(row_a.left() + px(40.0), row_a.center().y),
            modifiers,
        );
        body.read_with(cx, |view, cx| {
            assert_eq!(
                view.file_manager.read(cx).path,
                dir.path().join("a").to_string_lossy()
            );
        });
    }
}
