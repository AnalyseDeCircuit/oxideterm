use super::*;
use gpui::StatefulInteractiveElement;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_file_manager_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self
            .terminal_background_preferences("file_manager")
            .is_some();
        let filtered = sorted_local_files(
            &self.file_manager.files,
            &self.file_manager.filter,
            self.file_manager.sort_field,
            self.file_manager.sort_direction,
        );

        let mut root = div()
            .id("file-manager-view")
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .p(px(FILE_MANAGER_ROOT_PADDING))
            .gap(px(FILE_MANAGER_GAP))
            .bg(file_manager_bg(theme.bg, has_background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle);
                    this.file_manager.context_menu = None;
                    if this.file_manager.dialog.is_none() {
                        this.blur_file_manager_inline_inputs();
                    }
                    cx.notify();
                }),
            )
            .when(self.file_manager.bookmarks_visible, |root| {
                root.child(self.render_file_manager_bookmarks(has_background, cx))
            })
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
                        has_background,
                        window,
                        cx,
                    )),
            );

        if self.file_manager.dialog.is_none()
            && let Some(menu) = self.file_manager.context_menu.clone()
        {
            root =
                root.child(self.render_file_manager_context_menu(menu, window, has_background, cx));
        }
        if let Some(progress) = self.file_manager.operation_progress.as_ref()
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
        let bookmarked = self.is_file_manager_path_bookmarked(&self.file_manager.path);
        div()
            .h(px(FILE_MANAGER_HEADER_HEIGHT))
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
            .child(self.render_file_manager_icon_button(
                if self.file_manager.bookmarks_visible {
                    LucideIcon::PanelLeftClose
                } else {
                    LucideIcon::PanelLeft
                },
                self.i18n.t(if self.file_manager.bookmarks_visible {
                    "fileManager.collapseSidebar"
                } else {
                    "fileManager.expandSidebar"
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.file_manager.bookmarks_visible = !this.file_manager.bookmarks_visible;
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
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
                LucideIcon::Star,
                self.i18n.t(if bookmarked {
                    "fileManager.removeBookmark"
                } else {
                    "fileManager.addBookmark"
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs();
                    this.toggle_file_manager_current_bookmark(cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::FolderPlus,
                self.i18n.t("fileManager.newFolder"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs();
                    this.open_file_manager_new_folder_dialog();
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::FilePlus,
                self.i18n.t("fileManager.newFile"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs();
                    this.open_file_manager_new_file_dialog();
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
            .child(self.render_file_manager_icon_button(
                LucideIcon::FileArchive,
                self.i18n.t("fileManager.compress"),
                cx.listener(|this, _event, _window, cx| {
                    this.compress_file_manager_selection(cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::FolderArchive,
                self.i18n.t("fileManager.extract"),
                cx.listener(|this, _event, _window, cx| {
                    this.extract_selected_file_manager_archive(cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::HardDrive,
                self.i18n.t("fileManager.showDrives"),
                cx.listener(|this, _event, _window, cx| {
                    this.file_manager.dialog = Some(FileManagerDialog::Drives);
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
                    this.refresh_file_manager();
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
        let mut panel = div()
            .w(px(FILE_MANAGER_SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_1()
            .rounded(px(self.tokens.radii.sm))
            .border_color(file_manager_border(theme.border, has_background))
            .bg(file_manager_panel_bg(
                theme.bg_panel,
                has_background,
                FILE_MANAGER_PANEL_80_ALPHA,
            ))
            .child(
                div()
                    .h(px(FILE_MANAGER_HEADER_HEIGHT))
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_display_text(
                        "file-manager-bookmarks-title",
                        (),
                        self.i18n.t("fileManager.favorites").to_uppercase(),
                        theme.text_muted,
                        cx,
                    )),
            );
        if self.file_manager.bookmarks.is_empty() {
            panel = panel.child(
                div()
                    .px(px(12.0))
                    .py(px(16.0))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_display_text(
                        "file-manager-no-bookmarks",
                        (),
                        self.i18n.t("fileManager.noBookmarks"),
                        theme.text_muted,
                        cx,
                    )),
            );
        }
        for bookmark in self.file_manager.bookmarks.clone() {
            let active = bookmark.path == self.file_manager.path;
            let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
                "file-manager-bookmark-row",
                &bookmark.id,
            );
            panel = panel.child(
                div()
                    .h(px(32.0))
                    .mx(px(8.0))
                    .mb(px(4.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(if active {
                        rgba((theme.accent << 8) | FILE_MANAGER_SELECTED_BG_ALPHA)
                    } else {
                        rgba(theme.bg << 8)
                    })
                    .hover(move |row| row.bg(file_manager_hover_bg(theme.bg_hover, has_background)))
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Folder,
                        FILE_MANAGER_ICON_MD,
                        rgb(FILE_MANAGER_BLUE),
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
                            .child(self.render_row_safe_selectable_display_text_in_group(
                                selection_group_id,
                                "file-manager-bookmark-cell",
                                ("name", bookmark.id.as_str()),
                                0,
                                bookmark.name.clone(),
                                if active { theme.accent } else { theme.text },
                                None,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .hover({
                                let theme = self.tokens.ui;
                                move |button| button.bg(rgb(theme.bg_hover))
                            })
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
                                        this.blur_file_manager_inline_inputs();
                                        this.open_file_manager_edit_bookmark_dialog(
                                            bookmark.clone(),
                                        );
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }),
                            ),
                    )
                    .child(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .hover({
                                let theme = self.tokens.ui;
                                move |button| button.bg(rgb(theme.bg_hover))
                            })
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
                                        this.blur_file_manager_inline_inputs();
                                        this.remove_file_manager_bookmark(&id, cx);
                                        cx.stop_propagation();
                                    }
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let path = bookmark.path.clone();
                            move |this, _event, _window, cx| {
                                this.blur_file_manager_inline_inputs();
                                this.set_file_manager_path(path.clone());
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ),
            );
        }
        panel = panel.child(div().flex_1());
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
                                this.blur_file_manager_inline_inputs();
                                this.open_terminal_at_file_manager_path(window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                ),
        );
        panel.into_any_element()
    }

    fn render_file_manager_list_panel(
        &self,
        files: Vec<LocalFileEntry>,
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
            .border_1()
            .rounded(px(self.tokens.radii.sm))
            .border_color(rgba((theme.accent << 8) | FILE_MANAGER_ACTIVE_BORDER_ALPHA))
            .bg(file_manager_bg(theme.bg, has_background))
            .child(self.render_file_manager_header(has_background, window, cx))
            .child(self.render_file_manager_columns(has_background, cx))
            .child(self.render_file_manager_filter(has_background, cx))
            .child(self.render_file_manager_file_list(files, has_background, cx))
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
            .gap(px(8.0))
            .p(px(8.0))
            .border_b_1()
            .border_color(file_manager_border(theme.border, has_background))
            .bg(file_manager_panel_bg(theme.bg_panel, has_background, 0xff))
            .child(
                div()
                    .min_w(px(64.0))
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
                    this.blur_file_manager_inline_inputs();
                    this.navigate_file_manager_parent();
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::Home,
                self.i18n.t("fileManager.home"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs();
                    this.set_file_manager_path(home_path());
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .child(self.render_file_manager_icon_button(
                LucideIcon::RefreshCw,
                self.i18n.t("fileManager.refresh"),
                cx.listener(|this, _event, _window, cx| {
                    this.blur_file_manager_inline_inputs();
                    this.refresh_file_manager();
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx.entity(),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, _cx| {
                    window.focus(&this.focus_handle);
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
        let editing = self.file_manager.editing_path;
        let focused = self.file_manager.focused_input == Some(input);
        let value = if self.file_manager.editing_path {
            self.file_manager.path_input.as_str()
        } else {
            self.file_manager.path.as_str()
        };
        div()
            .flex_1()
            .min_w(px(0.0))
            .h(px(24.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
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
                bar.child(self.render_file_manager_inline_text(input, value, focused, cx))
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
                                    this.commit_file_manager_path_input();
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
            })
            .when(!editing, |bar| {
                bar.child(self.render_file_manager_breadcrumb(value, cx))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    if editing || event.click_count >= 2 {
                        this.start_file_manager_path_edit();
                    } else {
                        this.file_manager.focused_input = None;
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
            .gap(px(2.0));
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
                    .px(px(6.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
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
                            this.set_file_manager_path(full_path.clone());
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }

        div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .overflow_hidden()
            .text_size(px(FILE_MANAGER_TEXT_SM))
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
        let theme = self.tokens.ui;
        let text = if value.is_empty() {
            self.i18n.t("fileManager.pathPlaceholder")
        } else {
            value.to_string()
        };
        let target = WorkspaceImeTarget::FileManager(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .text_size(px(FILE_MANAGER_TEXT_XS))
                .text_color(if value.is_empty() {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                })
                .when(focused && value.is_empty(), |input| {
                    input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                })
                .child(div().truncate().child(text))
                .when_some(self.marked_text_for_target(target), |input, marked| {
                    input.child(div().underline().child(marked.to_string()))
                })
                .when(focused && !value.is_empty(), |input| {
                    input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                }),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
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
        let active = self.file_manager.sort_field == field;
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
                    match self.file_manager.sort_direction {
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
                    this.toggle_file_manager_sort(field);
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
        let focused = self.file_manager.focused_input == Some(input);
        let target = WorkspaceImeTarget::FileManager(input);
        let workspace = cx.entity();
        div()
            .h(px(32.0))
            .px(px(8.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(file_manager_border(theme.border, has_background))
            .bg(file_manager_panel_bg(theme.bg_panel, has_background, 0xff))
            .child(text_input_anchor_probe(
                target.anchor_id(),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value: &self.file_manager.filter,
                        placeholder: self.i18n.t("fileManager.filterPlaceholder"),
                        focused,
                        caret_visible: self.new_connection_caret_visible,
                        secret: false,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(target),
                        marked_text: self.marked_text_for_target(target),
                    },
                )
                .h(px(24.0))
                .bg(file_manager_bg(theme.bg_sunken, has_background))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        window.focus(&this.focus_handle);
                        this.file_manager.focused_input = Some(FileManagerInput::Filter);
                        this.file_manager.context_menu = None;
                        this.ime_marked_text = None;
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn render_file_manager_file_list(
        &self,
        files: Vec<LocalFileEntry>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let list = div()
            .id("file-manager-list-scroll")
            .flex_1()
            .min_h(px(0.0))
            .bg(file_manager_bg(theme.bg, has_background));
        if self.file_manager.loading {
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
                        .child(Self::render_lucide_icon(
                            LucideIcon::LoaderCircle,
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
        if let Some(error) = self.file_manager.error.as_ref() {
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

        let workspace = cx.entity();
        let selected = Arc::new(self.file_manager.selected.clone());
        let files = Arc::new(files);
        let row_count = files.len();
        let list_items = files.clone();
        let row_selected = selected.clone();
        let row_workspace = workspace.clone();
        let row_selectable_state = self.selectable_text_render_state(cx);
        list.child(
            tauri_virtual_uniform_list(
                "file-manager-list-virtual",
                row_count,
                self.file_manager.list_scroll.clone(),
                TauriVirtualListSpec::new(
                    px(FILE_MANAGER_ROW_HEIGHT),
                    FILE_MANAGER_VIRTUAL_OVERSCAN,
                ),
                move |range, _window, _cx| {
                    let selectable_state = row_selectable_state.clone();
                    range
                        .map(|index| {
                            let file = list_items[index].clone();
                            let file_for_open = file.clone();
                            let file_for_menu = file.clone();
                            let visible = list_items.as_ref().clone();
                            let selected = row_selected.contains(&file.name);
                            let (icon, icon_color) = file_icon_for_entry(&file);
                            let icon_color = if icon_color == 0 {
                                theme.text_muted
                            } else {
                                icon_color
                            };
                            let selection_group_id =
                                crate::workspace::selectable_text::selectable_text_id(
                                    "file-manager-list-row",
                                    &file.name,
                                );
                            let display_name = if let Some(target) = file.symlink_target.as_ref() {
                                format!("{} -> {target}", file.name)
                            } else {
                                file.name.clone()
                            };
                            let row_text_color = if selected { theme.accent } else { theme.text };
                            let size_text = if file.file_type == LocalFileType::Directory {
                                "-".to_string()
                            } else {
                                format_file_size(file.size)
                            };
                            let modified_text = format_modified(file.modified);
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
                                            icon,
                                            FILE_MANAGER_ICON_MD,
                                            rgb(icon_color),
                                        ))
                                        .child(div().truncate().child(
                                            selectable_state.render_row_safe_display_text_in_group(
                                                selection_group_id,
                                                "file-manager-list-cell",
                                                ("name", file.name.as_str()),
                                                0,
                                                display_name,
                                                row_text_color,
                                                _cx,
                                            ),
                                        )),
                                )
                                .child(
                                    div()
                                        .w(px(FILE_MANAGER_SIZE_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(
                                            selectable_state.render_row_safe_display_text_in_group(
                                                selection_group_id,
                                                "file-manager-list-cell",
                                                ("size", file.name.as_str()),
                                                1,
                                                size_text,
                                                theme.text_muted,
                                                _cx,
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(FILE_MANAGER_MODIFIED_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(
                                            selectable_state.render_row_safe_display_text_in_group(
                                                selection_group_id,
                                                "file-manager-list-cell",
                                                ("modified", file.name.as_str()),
                                                2,
                                                modified_text,
                                                theme.text_muted,
                                                _cx,
                                            ),
                                        ),
                                )
                                .on_mouse_down(MouseButton::Left, {
                                    let workspace = row_workspace.clone();
                                    let name = file.name.clone();
                                    move |event: &MouseDownEvent, window, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            window.focus(&this.focus_handle);
                                            this.file_manager.context_menu = None;
                                            this.blur_file_manager_inline_inputs();
                                            if event.click_count >= 2 {
                                                this.open_file_manager_entry(
                                                    file_for_open.clone(),
                                                    cx,
                                                );
                                            } else {
                                                this.select_file_manager_entry(
                                                    name.clone(),
                                                    event.modifiers,
                                                    &visible,
                                                );
                                            }
                                            cx.stop_propagation();
                                            cx.notify();
                                        });
                                    }
                                })
                                .on_mouse_down(MouseButton::Right, {
                                    let workspace = row_workspace.clone();
                                    move |event: &MouseDownEvent, window, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            window.focus(&this.focus_handle);
                                            this.open_file_manager_context_menu(
                                                Some(file_for_menu.clone()),
                                                f32::from(event.position.x),
                                                f32::from(event.position.y),
                                            );
                                            cx.stop_propagation();
                                            cx.notify();
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
                    window.focus(&this.focus_handle);
                    this.file_manager.context_menu = None;
                    this.blur_file_manager_inline_inputs();
                    this.clear_file_manager_selection();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    this.open_file_manager_context_menu(
                        None,
                        f32::from(event.position.x),
                        f32::from(event.position.y),
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
        let tooltip_for_move = tooltip.clone();
        let tooltip_element_id = tooltip.clone();
        let tooltip_request_id = tooltip.clone();
        let tooltip_workspace = workspace.clone();
        let clear_workspace = workspace;
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, FILE_MANAGER_ICON_MD, rgb(self.tokens.ui.text)),
            // Tauri file-manager toolbar icons are fully opaque until disabled.
            IconButtonOptions::opaque_toolbar(FILE_MANAGER_TOOL_BUTTON, ButtonRadius::Sm),
        )
        .id((
            gpui::ElementId::from("file-manager-icon-button"),
            tooltip_element_id,
        ))
        .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
            let _ = tooltip_workspace.update(cx, |this, cx| {
                this.queue_workspace_tooltip(
                    tooltip_request_id.clone(),
                    tooltip_for_move.clone(),
                    f32::from(event.position.x) + 12.0,
                    f32::from(event.position.y) + 16.0,
                    cx,
                );
            });
        })
        .on_hover(move |hovered: &bool, _window, cx| {
            if !*hovered {
                let _ = clear_workspace.update(cx, |this, cx| {
                    this.clear_workspace_tooltip(&tooltip, cx);
                });
            }
        })
        .on_mouse_down(MouseButton::Left, listener)
        .into_any_element()
    }
}
