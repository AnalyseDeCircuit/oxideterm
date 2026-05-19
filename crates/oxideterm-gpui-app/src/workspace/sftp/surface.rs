impl WorkspaceApp {
    pub(super) fn render_sftp_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(tab_id) = self.active_tab_id else {
            return self.render_empty_workspace(cx);
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return self.render_empty_workspace(cx);
        };
        let has_background = self.terminal_background_preferences("sftp").is_some();
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.as_str())
            .unwrap_or("mock-host");

        let mut root = div()
            .id("sftp-view")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .p(px(SFTP_ROOT_PADDING))
            .gap(px(SFTP_GAP))
            .bg(sftp_bg(theme.bg, has_background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle);
                    this.sftp_view.context_menu = None;
                    cx.notify();
                }),
            )
            .when_some(self.sftp_view.init_error.as_ref(), |root, error| {
                root.child(self.render_sftp_init_error(error, has_background, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    .gap(px(SFTP_GAP))
                    .child(self.render_sftp_pane(
                        SftpPane::Local,
                        self.i18n.t("sftp.file_list.local"),
                        &self.sftp_view.local_path,
                        &self.sftp_view.local_filter,
                        self.sftp_view.local_sort_field,
                        self.sftp_view.local_sort_direction,
                        &self.sftp_view.local_files,
                        &self.sftp_view.local_selected,
                        self.sftp_view.editing_local_path,
                        &self.sftp_view.local_path_input,
                        self.sftp_view.focused_input,
                        false,
                        has_background,
                        window,
                        cx,
                    ))
                    .child(
                        self.render_sftp_pane(
                            SftpPane::Remote,
                            self.i18n
                                .t("sftp.file_list.remote")
                                .replace("{{host}}", node_title),
                            &self.sftp_view.remote_path,
                            &self.sftp_view.remote_filter,
                            self.sftp_view.remote_sort_field,
                            self.sftp_view.remote_sort_direction,
                            &self.sftp_view.remote_files,
                            &self.sftp_view.remote_selected,
                            self.sftp_view.editing_remote_path,
                            &self.sftp_view.remote_path_input,
                            self.sftp_view.focused_input,
                            self.sftp_view.remote_loading,
                            has_background,
                            window,
                            cx,
                        ),
                    ),
            )
            .child(self.render_sftp_transfer_queue(has_background, cx));

        if self.sftp_view.dialog.is_none()
            && let Some(menu) = self.sftp_view.context_menu.clone()
        {
            root = root.child(self.render_sftp_context_menu(menu, window, has_background, cx));
        }

        root.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_sftp_pane(
        &self,
        pane: SftpPane,
        title: String,
        path: &str,
        filter: &str,
        sort_field: SftpSortField,
        sort_direction: SftpSortDirection,
        files: &[SftpFileEntry],
        selected: &HashSet<String>,
        path_editing: bool,
        path_input: &str,
        focused_input: Option<SftpInput>,
        loading: bool,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.sftp_view.active_pane == pane;
        let drag_over = self.sftp_view.drag_over_pane == Some(pane);
        let drag_bg = rgba((theme.accent << 8) | SFTP_DRAG_BG_ALPHA);
        let drag_border = rgba((theme.accent << 8) | SFTP_DRAG_RING_ALPHA);
        let filtered = sorted_sftp_files(files, filter, sort_field, sort_direction);
        let transfer_direction = if pane == SftpPane::Local {
            SftpTransferDirection::Upload
        } else {
            SftpTransferDirection::Download
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(if drag_over {
                drag_border
            } else if active {
                rgba((theme.accent << 8) | SFTP_ACTIVE_BORDER_ALPHA)
            } else {
                sftp_border(theme.border, has_background)
            })
            .bg(if drag_over {
                drag_bg
            } else {
                sftp_bg(theme.bg, has_background)
            })
            .drag_over::<gpui::ExternalPaths>(move |style, _paths, _window, _cx| {
                style.bg(drag_bg).border_color(drag_border)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    window.focus(&this.focus_handle);
                    this.sftp_view.active_pane = pane;
                    cx.notify();
                }),
            )
            .child(self.render_sftp_pane_header(
                pane,
                title,
                path,
                path_editing,
                path_input,
                focused_input,
                selected.len(),
                transfer_direction,
                active,
                has_background,
                window,
                cx,
            ))
            .child(self.render_sftp_column_header(
                pane,
                sort_field,
                sort_direction,
                has_background,
                cx,
            ))
            .child(self.render_sftp_filter(pane, filter, focused_input, has_background, cx))
            .child(self.render_sftp_file_list(
                pane,
                path,
                filtered,
                selected,
                loading,
                has_background,
                cx,
            ))
            .into_any_element()
    }

    fn render_sftp_pane_header(
        &self,
        pane: SftpPane,
        title: String,
        path: &str,
        path_editing: bool,
        path_input: &str,
        focused_input: Option<SftpInput>,
        selected_count: usize,
        transfer_direction: SftpTransferDirection,
        active: bool,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let input = if pane == SftpPane::Local {
            SftpInput::LocalPath
        } else {
            SftpInput::RemotePath
        };
        let mut header = div()
            .h(px(SFTP_PANE_HEADER_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .p(px(8.0))
            .border_b_1()
            .border_color(if active {
                rgba((theme.accent << 8) | SFTP_HEADER_ACTIVE_BORDER_ALPHA)
            } else {
                sftp_border(theme.border, has_background)
            })
            .bg(if active {
                rgba((theme.bg_hover << 8) | SFTP_HEADER_ACTIVE_BG_ALPHA)
            } else {
                sftp_panel_bg(theme.bg_panel, has_background, 0xff)
            })
            .child(
                div()
                    .min_w(px(48.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(title.to_uppercase()),
            )
            .child(self.render_sftp_path_bar(
                pane,
                input,
                path,
                path_input,
                path_editing,
                focused_input,
                has_background,
                window,
                cx,
            ));

        if pane == SftpPane::Local {
            header = header
                .child(self.render_sftp_icon_button(
                    LucideIcon::HardDrive,
                    self.i18n.t("sftp.toolbar.show_drives"),
                    cx.listener(|this, _event, _window, cx| {
                        this.sftp_view.dialog = Some(SftpDialog::Drives);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                    cx.entity(),
                ))
                .child(self.render_sftp_icon_button(
                    LucideIcon::FolderOpen,
                    self.i18n.t("sftp.toolbar.browse_folder"),
                    cx.listener(|this, _event, _window, cx| {
                        this.browse_sftp_local_folder(cx);
                        cx.stop_propagation();
                    }),
                    cx.entity(),
                ));
        }

        header = header
            .child(self.render_sftp_nav_button(
                pane,
                "..",
                LucideIcon::ArrowUp,
                "sftp.toolbar.go_up",
                cx,
            ))
            .child(self.render_sftp_nav_button(
                pane,
                "~",
                LucideIcon::Home,
                "sftp.toolbar.home",
                cx,
            ))
            .child(self.render_sftp_refresh_button(pane, cx));

        if selected_count > 0 {
            let label = match transfer_direction {
                SftpTransferDirection::Upload => self
                    .i18n
                    .t("sftp.toolbar.upload_count")
                    .replace("{{count}}", &selected_count.to_string()),
                SftpTransferDirection::Download => self
                    .i18n
                    .t("sftp.toolbar.download_count")
                    .replace("{{count}}", &selected_count.to_string()),
            };
            let icon = match transfer_direction {
                SftpTransferDirection::Upload => LucideIcon::Upload,
                SftpTransferDirection::Download => LucideIcon::Download,
            };
            header = header.child(self.render_sftp_transfer_button(
                pane,
                transfer_direction,
                icon,
                label,
                cx,
            ));
        }

        header.into_any_element()
    }

    fn render_sftp_path_bar(
        &self,
        pane: SftpPane,
        input: SftpInput,
        path: &str,
        path_input: &str,
        editing: bool,
        focused_input: Option<SftpInput>,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = focused_input == Some(input);
        let value = if editing { path_input } else { path };
        let path_bar = div()
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
                rgb(theme.border)
            })
            .bg(sftp_bg(theme.bg_sunken, has_background))
            .overflow_hidden()
            .cursor_pointer()
            .when(editing, |bar| {
                bar.child(self.render_sftp_inline_text(
                    input,
                    value,
                    "sftp.file_list.path_placeholder",
                    focused,
                    cx,
                ))
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
                            SFTP_ICON_SM,
                            rgb(theme.text),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.commit_sftp_path_input(pane);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .when(!editing, |bar| {
                bar.child(self.render_sftp_breadcrumb(pane, path, window, cx))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.sftp_view.active_pane = pane;
                    if editing || event.click_count >= 2 {
                        match pane {
                            SftpPane::Local => {
                                this.sftp_view.editing_local_path = true;
                                this.sftp_view.local_path_input = this.sftp_view.local_path.clone();
                            }
                            SftpPane::Remote => {
                                this.sftp_view.editing_remote_path = true;
                                this.sftp_view.remote_path_input =
                                    this.sftp_view.remote_path.clone();
                            }
                        }
                        this.sftp_view.focused_input = Some(input);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        path_bar.into_any_element()
    }

    fn render_sftp_breadcrumb(
        &self,
        pane: SftpPane,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let segments = sftp_path_segments(path, pane == SftpPane::Remote);
        let max_scroll = sftp_breadcrumb_max_scroll(
            &segments,
            sftp_path_bar_viewport_width(window),
            SFTP_ICON_MD,
        );
        let scroll_x = match pane {
            SftpPane::Local => self.sftp_view.local_path_scroll_x,
            SftpPane::Remote => self.sftp_view.remote_path_scroll_x,
        }
        .clamp(0.0, max_scroll);
        let mut inner = div()
            .flex_none()
            .relative()
            .left(px(-scroll_x))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.0));
        for (index, segment) in segments.iter().cloned().enumerate() {
            if index > 0 {
                inner = inner.child(Self::render_lucide_icon(
                    LucideIcon::ChevronRight,
                    SFTP_ICON_MD,
                    rgb(theme.text_muted),
                ));
            }
            let is_last = index + 1 == segments.len();
            let full_path = segment.full_path.clone();
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
                        rgba((theme.bg_hover << 8) | SFTP_BREADCRUMB_ACTIVE_ALPHA)
                    } else {
                        rgba(theme.bg_hover << 8)
                    })
                    .hover(move |crumb| {
                        crumb.bg(rgba((theme.bg_hover << 8) | SFTP_BREADCRUMB_HOVER_ALPHA))
                    })
                    .text_color(if is_last {
                        rgb(theme.text_heading)
                    } else {
                        rgb(theme.text)
                    })
                    .when(index == 0, |item| {
                        item.child(Self::render_lucide_icon(
                            if pane == SftpPane::Remote {
                                LucideIcon::Server
                            } else {
                                LucideIcon::Home
                            },
                            SFTP_ICON_MD,
                            rgb(theme.text_muted),
                        ))
                    })
                    .child(div().truncate().child(segment.name))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_sftp_path(pane, full_path.clone());
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
            .text_size(px(SFTP_TEXT_SM))
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, window, cx| {
                this.handle_sftp_breadcrumb_scroll(pane, event, window, cx);
            }))
            .child(
                // Tauri PathBreadcrumb is `overflow-x-auto`. GPUI's native
                // scroll container does not expose the same hidden scrollbar
                // shape here, so we preserve the user-visible horizontal scroll
                // by translating the full breadcrumb row inside the clipped bar.
                inner,
            )
            .into_any_element()
    }

    fn render_sftp_column_header(
        &self,
        pane: SftpPane,
        sort_field: SftpSortField,
        sort_direction: SftpSortDirection,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(25.0))
            .flex()
            .flex_row()
            .items_center()
            .px(px(8.0))
            .py(px(4.0))
            .bg(sftp_panel_bg(self.tokens.ui.bg_panel, has_background, 0xff))
            .border_b_1()
            .border_color(sftp_border(self.tokens.ui.border, has_background))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Name,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_name"),
                None,
                cx,
            ))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Size,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_size"),
                Some(SFTP_SIZE_COL),
                cx,
            ))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Modified,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_modified"),
                Some(SFTP_MODIFIED_COL),
                cx,
            ))
            .into_any_element()
    }

    fn render_sftp_sort_header(
        &self,
        pane: SftpPane,
        field: SftpSortField,
        active_field: SftpSortField,
        direction: SftpSortDirection,
        label: String,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .when_some(width, |header, width| {
                header.w(px(width)).flex_none().justify_end()
            })
            .when(width.is_none(), |header| header.flex_1())
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .text_color(if active_field == field {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |header| header.text_color(rgb(theme.text)))
            .cursor_pointer()
            .child(div().truncate().child(label))
            .when(active_field == field, |header| {
                let icon = match (field, direction) {
                    (SftpSortField::Name, SftpSortDirection::Asc) => LucideIcon::ArrowUpAZ,
                    (SftpSortField::Name, SftpSortDirection::Desc) => LucideIcon::ArrowDownAZ,
                    _ => LucideIcon::ArrowUpDown,
                };
                header.child(Self::render_lucide_icon(
                    icon,
                    SFTP_ICON_SM,
                    rgb(theme.accent),
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.toggle_sftp_sort(pane, field);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_filter(
        &self,
        pane: SftpPane,
        filter: &str,
        focused_input: Option<SftpInput>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = if pane == SftpPane::Local {
            SftpInput::LocalFilter
        } else {
            SftpInput::RemoteFilter
        };
        let focused = focused_input == Some(input);
        let theme = self.tokens.ui;
        div()
            .h(px(30.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .py(px(4.0))
            .bg(sftp_panel_bg(
                theme.bg_panel,
                has_background,
                SFTP_PANEL_80_ALPHA,
            ))
            .border_b_1()
            .border_color(sftp_border(theme.border, has_background))
            .child(Self::render_lucide_icon(
                LucideIcon::Search,
                SFTP_ICON_SM,
                rgb(theme.text_muted),
            ))
            .child(self.render_sftp_inline_text(
                input,
                filter,
                "sftp.file_list.filter_placeholder",
                focused,
                cx,
            ))
            .when(!filter.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .hover(move |x| x.text_color(rgb(theme.text)))
                        .cursor_pointer()
                        .child("×")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                *this.sftp_input_value_mut(input) = String::new();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.sftp_view.active_pane = pane;
                    this.sftp_view.focused_input = Some(input);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_inline_text(
        &self,
        input: SftpInput,
        value: &str,
        placeholder_key: &'static str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let text = if value.is_empty() {
            self.i18n.t(placeholder_key)
        } else {
            value.to_string()
        };
        let target = WorkspaceImeTarget::Sftp(input);
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
                .text_size(px(SFTP_TEXT_XS))
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
                let _ = workspace.update(cx, |this, _cx| {
                    this.text_input_anchors.insert(anchor.id, anchor);
                });
            },
        )
        .into_any_element()
    }


}
