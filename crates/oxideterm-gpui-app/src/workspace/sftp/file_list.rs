impl WorkspaceApp {
    fn render_sftp_file_list(
        &self,
        pane: SftpPane,
        _path: &str,
        files: Vec<SftpFileEntry>,
        selected: &HashSet<String>,
        loading: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let drag_over = self.sftp_view.drag_over_pane == Some(pane);
        let list = div()
            .id(("sftp-file-list-scroll", pane as u64))
            .flex_1()
            .min_h(px(0.0))
            .bg(if drag_over {
                rgba((theme.accent << 8) | SFTP_DRAG_BG_ALPHA)
            } else {
                sftp_bg(theme.bg, has_background)
            })
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                this.update_sftp_drag(
                    pane,
                    f32::from(event.position.x),
                    f32::from(event.position.y),
                );
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.finish_sftp_drag(pane);
                    cx.notify();
                }),
            )
            .when(pane == SftpPane::Remote, |list| {
                list.can_drop(|drag, _window, _cx| drag.is::<gpui::ExternalPaths>())
                    .on_drop(cx.listener(
                        |this, paths: &gpui::ExternalPaths, _window, cx| {
                            this.queue_sftp_external_upload_paths(paths.paths());
                            this.sftp_view.drag_over_pane = None;
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ))
            })
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                // The menu is positioned in window coordinates, so any pane
                // scroll invalidates the row that produced the coordinates.
                if this.sftp_view.context_menu.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    window.focus(&this.focus_handle);
                    this.sftp_view.context_menu = None;
                    this.sftp_view.drag_state = None;
                    this.sftp_view.drag_over_pane = None;
                    this.clear_sftp_selection(pane);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    this.open_sftp_context_menu(
                        pane,
                        None,
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                    );
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

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
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::LoaderCircle,
                            20.0,
                            rgb(theme.text_muted),
                        ))
                        .child(self.render_selectable_display_text(
                            "sftp-file-list-loading",
                            pane as u64,
                            self.i18n.t("sftp.file_list.loading"),
                            theme.text_muted,
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
                        .text_size(px(SFTP_TEXT_XS))
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
                            "sftp-file-list-empty",
                            pane as u64,
                            self.i18n.t("sftp.file_list.empty"),
                            theme.text_muted,
                            cx,
                        )),
                )
                .into_any_element();
        }

        let workspace = cx.entity();
        let selected = std::sync::Arc::new(selected.clone());
        let files = std::sync::Arc::new(files);
        let scroll_handle = match pane {
            SftpPane::Local => self.sftp_view.local_file_scroll.clone(),
            SftpPane::Remote => self.sftp_view.remote_file_scroll.clone(),
        };
        let row_count = files.len();
        let list_items = files.clone();
        let row_selected = selected.clone();
        let row_workspace = workspace.clone();
        let row_selectable_state = self.selectable_text_render_state(cx);

        list.child(
            tracked_uniform_list(
                ("sftp-file-list-virtual", pane as u64),
                row_count,
                scroll_handle,
                move |range, _window, _cx| {
                    let selectable_state = row_selectable_state.clone();
                    range
                        .map(|index| {
                            let file = list_items[index].clone();
                            let name = file.name.clone();
                            let row_file = file.clone();
                            let context_file = file.clone();
                            let display_name = if let Some(target) = file.symlink_target.as_ref() {
                                format!("{} -> {target}", file.name)
                            } else {
                                file.name.clone()
                            };
                            let _metadata_fields_consumed =
                                (&file.permissions, &file.owner, &file.group);
                            let is_selected = row_selected.contains(&name);
                            let selection_group_id =
                                crate::workspace::selectable_text::selectable_text_id(
                                    "sftp-file-list-row",
                                    (pane as u64, file.name.as_str()),
                                );
                            let row_text_color = if is_selected {
                                theme.accent
                            } else {
                                theme.text
                            };
                            let size_text = if file.file_type == SftpFileType::Directory {
                                "-".to_string()
                            } else {
                                format_file_size(file.size)
                            };
                            let modified_text = format_modified(file.modified);
                            div()
                                .w_full()
                                .h(px(SFTP_ROW_HEIGHT))
                                .flex()
                                .flex_row()
                                .items_center()
                                .px(px(8.0))
                                .py(px(4.0))
                                .border_b_1()
                                .border_color(rgba(theme.border << 8))
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(if is_selected {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text)
                                })
                                .bg(if is_selected {
                                    rgba((theme.accent << 8) | SFTP_SELECTED_BG_ALPHA)
                                } else {
                                    rgba(theme.bg << 8)
                                })
                                .hover(move |row| {
                                    row.bg(sftp_hover_bg(theme.bg_hover, has_background))
                                })
                                .cursor_pointer()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(Self::render_lucide_icon(
                                            if file.is_symlink {
                                                LucideIcon::Link2
                                            } else if file.file_type == SftpFileType::Directory {
                                                LucideIcon::Folder
                                            } else {
                                                LucideIcon::File
                                            },
                                            SFTP_ICON_MD,
                                            if file.file_type == SftpFileType::Directory {
                                                rgb(SFTP_FOLDER_BLUE)
                                            } else if file.is_symlink {
                                                rgb(theme.accent)
                                            } else {
                                                rgb(theme.text_muted)
                                            },
                                        ))
                                        .child(div().truncate().child(
                                            selectable_state.render_row_safe_display_text_in_group(
                                                selection_group_id,
                                                "sftp-file-list-cell",
                                                ("name", pane as u64, file.name.as_str()),
                                                0,
                                                display_name,
                                                row_text_color,
                                                _cx,
                                            ),
                                        )),
                                )
                                .child(
                                    div()
                                        .w(px(SFTP_SIZE_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(selectable_state.render_row_safe_display_text_in_group(
                                            selection_group_id,
                                            "sftp-file-list-cell",
                                            ("size", pane as u64, file.name.as_str()),
                                            1,
                                            size_text,
                                            theme.text_muted,
                                            _cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .w(px(SFTP_MODIFIED_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(selectable_state.render_row_safe_display_text_in_group(
                                            selection_group_id,
                                            "sftp-file-list-cell",
                                            ("modified", pane as u64, file.name.as_str()),
                                            2,
                                            modified_text,
                                            theme.text_muted,
                                            _cx,
                                        )),
                                )
                                .on_mouse_down(MouseButton::Left, {
                                    let workspace = row_workspace.clone();
                                    move |event: &MouseDownEvent, window, cx| {
                                        let _ = workspace.update(cx, |this, cx| {
                                            window.focus(&this.focus_handle);
                                            this.sftp_view.context_menu = None;
                                            if event.click_count >= 2 {
                                                this.open_or_preview_sftp_file(pane, &row_file);
                                            } else {
                                                this.select_sftp_file(
                                                    pane,
                                                    name.clone(),
                                                    event.modifiers,
                                                );
                                                if !this.read_only_selection_drag_active() {
                                                    this.start_sftp_drag_candidate(
                                                        pane,
                                                        f32::from(event.position.x),
                                                        f32::from(event.position.y),
                                                    );
                                                }
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
                                            this.open_sftp_context_menu(
                                                pane,
                                                Some(context_file.clone()),
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
        )
        .into_any_element()
    }
}
