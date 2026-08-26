use super::*;

impl SftpWorkspaceEntity {
    fn visible_file_indices(&self, pane: SftpPane) -> Vec<usize> {
        let (files, filter, sort_field, sort_direction) = match pane {
            SftpPane::Local => (
                &self.local_files,
                &self.local_filter,
                self.local_sort_field,
                self.local_sort_direction,
            ),
            SftpPane::Remote => (
                &self.remote_files,
                &self.remote_filter,
                self.remote_sort_field,
                self.remote_sort_direction,
            ),
        };
        let filter = filter.trim().to_lowercase();
        let mut indices = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                (filter.is_empty() || file.name.to_lowercase().contains(&filter)).then_some(index)
            })
            .collect::<Vec<_>>();
        // Sort lightweight indices so the virtual list never owns a duplicate
        // of every file entry merely to preserve the requested presentation.
        indices.sort_by(|left_index, right_index| {
            let left = &files[*left_index];
            let right = &files[*right_index];
            if left.file_type == SftpFileType::Directory
                && right.file_type != SftpFileType::Directory
            {
                return std::cmp::Ordering::Less;
            }
            if left.file_type != SftpFileType::Directory
                && right.file_type == SftpFileType::Directory
            {
                return std::cmp::Ordering::Greater;
            }
            let ordering = match sort_field {
                SftpSortField::Name => left.name.cmp(&right.name),
                SftpSortField::Size => left.size.cmp(&right.size),
                SftpSortField::Modified => left.modified.cmp(&right.modified),
            };
            match sort_direction {
                SftpSortDirection::Asc => ordering,
                SftpSortDirection::Desc => ordering.reverse(),
            }
        });
        indices
    }
}

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn render_sftp_file_list(
        &self,
        pane: SftpPane,
        loading: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let compact = self.sftp_view.read(cx).current_surface_id == Some(SftpSurfaceId::Sidebar);
        let drag_over = self.sftp_view.read(cx).drag_over_pane == Some(pane);
        let list = div()
            .id(("sftp-file-list-scroll", pane as u64))
            .flex_1()
            .min_h(px(0.0))
            .bg(if drag_over {
                rgba((theme.accent << 8) | SFTP_DRAG_BG_ALPHA)
            } else {
                sftp_bg(theme.bg, has_background)
            })
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    if this.update_sftp_drag(
                        pane,
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        cx,
                    ) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.finish_sftp_drag(pane, cx) {
                        // Mouse-up also fires for ordinary list clicks. Only
                        // repaint when it actually clears drag chrome or starts
                        // a cross-pane transfer.
                        cx.notify();
                    }
                }),
            )
            .when(pane == SftpPane::Remote, |list| {
                list.can_drop(|drag, _window, _cx| drag.is::<gpui::ExternalPaths>())
                    .on_drop(
                        cx.listener(|this, paths: &gpui::ExternalPaths, _window, cx| {
                            this.queue_sftp_external_upload_paths(paths.paths(), cx);
                            this.sftp_view.update(cx, |sftp, cx| {
                                sftp.drag_over_pane = None;
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }),
                    )
            })
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                // The menu is positioned in window coordinates, so any pane
                // scroll invalidates the row that produced the coordinates.
                this.sftp_view.update(cx, |sftp, cx| {
                    if sftp.clear_context_menu_immediately() {
                        cx.notify();
                    }
                });
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    let menu_changed = this
                        .sftp_view
                        .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
                    let drag_changed = this.cancel_sftp_drag_capture(cx);
                    let selection_changed = this.clear_sftp_selection(pane, cx);
                    if menu_changed || drag_changed || selection_changed {
                        // Blank-list clicks can happen repeatedly while no
                        // row/menu/drag state exists; repaint only when the
                        // click actually cleared visible SFTP chrome.
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.sftp_view.update(cx, |sftp, cx| {
                        sftp.open_context_menu(
                            pane,
                            None,
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            cx,
                        );
                    });
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
                        .child(self.render_loading_icon(
                            ("sftp-file-list-loading", pane as usize),
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

        let visible_indices = self.sftp_view.read(cx).visible_file_indices(pane);
        if visible_indices.is_empty() {
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

        let workspace_focus = self.focus_handle.clone();
        let sftp_view = self.sftp_view.clone();
        let visible_indices = std::sync::Arc::new(visible_indices);
        let scroll_handle = match pane {
            SftpPane::Local => self.sftp_view.read(cx).local_file_scroll.clone(),
            SftpPane::Remote => self.sftp_view.read(cx).remote_file_scroll.clone(),
        };
        let row_count = visible_indices.len();

        list.child(tauri_virtual_uniform_list(
            ("sftp-file-list-virtual", pane as u64),
            row_count,
            scroll_handle,
            sftp_file_list_virtual_spec(),
            move |range, _window, _cx| {
                let workspace_focus = workspace_focus.clone();
                range
                    .map(|index| {
                        let source_index = visible_indices[index];
                        let (file, is_selected) = {
                            let sftp = sftp_view.read(_cx);
                            let (files, selected) = match pane {
                                SftpPane::Local => (&sftp.local_files, &sftp.local_selected),
                                SftpPane::Remote => (&sftp.remote_files, &sftp.remote_selected),
                            };
                            let Some(file) = files.get(source_index).cloned() else {
                                return div().into_any_element();
                            };
                            let is_selected = selected.contains(&file.name);
                            (file, is_selected)
                        };
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
                            .hover(move |row| row.bg(sftp_hover_bg(theme.bg_hover, has_background)))
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
                                    // Tauri file rows are select-none. Plain
                                    // display text also prevents retaining the
                                    // root selectable-text adapter here.
                                    .child(div().truncate().child(display_name)),
                            )
                            .when(!compact, |row| {
                                row.child(
                                    div()
                                        .w(px(SFTP_SIZE_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(size_text),
                                )
                            })
                            .when(!compact, |row| {
                                row.child(
                                    div()
                                        .w(px(SFTP_MODIFIED_COL))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(modified_text),
                                )
                            })
                            .on_mouse_down(MouseButton::Left, {
                                let workspace_focus = workspace_focus.clone();
                                let sftp_view = sftp_view.clone();
                                move |event: &MouseDownEvent, window, cx| {
                                    // Row handlers stop propagation, so they must restore the
                                    // workspace focus that owns SFTP keyboard shortcuts.
                                    window.focus(&workspace_focus, cx);
                                    sftp_view.update(cx, |sftp, cx| {
                                        if event.click_count >= 2 {
                                            sftp.activate_file(pane, row_file.clone(), cx);
                                        } else {
                                            sftp.select_file(pane, name.clone(), event.modifiers);
                                            sftp.start_drag_candidate(
                                                pane,
                                                f32::from(event.position.x),
                                                f32::from(event.position.y),
                                            );
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    });
                                }
                            })
                            .on_mouse_down(MouseButton::Right, {
                                let workspace_focus = workspace_focus.clone();
                                let sftp_view = sftp_view.clone();
                                move |event: &MouseDownEvent, window, cx| {
                                    window.focus(&workspace_focus, cx);
                                    sftp_view.update(cx, |sftp, cx| {
                                        let selected = match pane {
                                            SftpPane::Local => &sftp.local_selected,
                                            SftpPane::Remote => &sftp.remote_selected,
                                        };
                                        if !selected.contains(&context_file.name) {
                                            // Right-clicking an unselected row makes that row the
                                            // operation target before the shared menu is rendered.
                                            sftp.select_file(
                                                pane,
                                                context_file.name.clone(),
                                                event.modifiers,
                                            );
                                        }
                                        sftp.open_context_menu(
                                            pane,
                                            Some(context_file.clone()),
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
        ))
        .into_any_element()
    }
}
