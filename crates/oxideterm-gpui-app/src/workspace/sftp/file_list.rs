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

#[cfg(test)]
mod drag_tests {
    use super::*;
    use crate::workspace::window_shell::WorkspaceWindowShell;
    use gpui::TestAppContext;

    fn file(path: &str) -> SftpFileEntry {
        SftpFileEntry {
            name: "drag.txt".into(),
            path: path.into(),
            file_type: SftpFileType::File,
            size_known: true,
            size: 12,
            modified: None,
            permissions: None,
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        }
    }

    #[gpui::test]
    fn cross_pane_drag_reaches_transfer_conflicts_before_workspace_cleanup(
        cx: &mut TestAppContext,
    ) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_SFTP_DRAG_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; isolate the real workspace in a portable child.
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
        assert!(settings_path.starts_with(PathBuf::from(fixture_dir)));
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().sftp.conflict_action = oxideterm_settings::ConflictAction::Ask;
        settings.save().unwrap();
        // Real workspace workers and drag timers wake the UI from I/O threads.
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, cx| {
                workspace.open_sftp_tab_surface(NodeId::new("drag-fixture"), None, cx);
            });
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.run_until_parked();
        let view = workspace.read_with(cx, |workspace, _| workspace.sftp_view().clone());
        for (source, target, direction, path, release_outside) in [
            (
                SftpPane::Local,
                SftpPane::Remote,
                Some(SftpTransferDirection::Upload),
                "/local/drag.txt",
                false,
            ),
            (
                SftpPane::Remote,
                SftpPane::Local,
                Some(SftpTransferDirection::Download),
                "/remote/drag.txt",
                false,
            ),
            (
                SftpPane::Local,
                SftpPane::Local,
                None,
                "/local/drag.txt",
                false,
            ),
            (
                SftpPane::Local,
                SftpPane::Remote,
                None,
                "/local/drag.txt",
                true,
            ),
        ] {
            view.update(cx, |sftp, cx| {
                sftp.local_files = vec![file("/local/drag.txt")];
                sftp.remote_files = vec![file("/remote/drag.txt")];
                sftp.local_selected.clear();
                sftp.remote_selected.clear();
                sftp.local_path = "/local".into();
                sftp.remote_path = "/remote".into();
                sftp.remote_loading = false;
                sftp.remote_load_pending = false;
                sftp.remote_load_inflight = false;
                sftp.init_error = None;
                sftp.dialog = None;
                sftp.conflict_state = None;
                cx.notify();
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
            });
            let (start, end) = view.read_with(cx, |sftp, _| {
                let bounds = |pane| match pane {
                    SftpPane::Local => sftp.local_file_scroll.0.borrow().base_handle.bounds(),
                    SftpPane::Remote => sftp.remote_file_scroll.0.borrow().base_handle.bounds(),
                };
                let start = bounds(source);
                assert!(start.size.width > px(0.0) && start.size.height > px(SFTP_ROW_HEIGHT));
                (
                    Point::new(start.center().x, start.top() + px(SFTP_ROW_HEIGHT / 2.0)),
                    bounds(target).center(),
                )
            });
            cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::default());
            cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::default());
            view.read_with(cx, |sftp, _| {
                let drag = sftp
                    .drag_state
                    .as_ref()
                    .expect("mouse-down must capture the source row");
                assert_eq!(drag.names, ["drag.txt"]);
                assert_eq!(drag.source_pane, source);
                assert!(drag.active);
                assert_eq!(sftp.drag_over_pane, Some(target));
            });
            let release = if release_outside {
                Point::new(px(-10.0), px(-10.0))
            } else {
                end
            };
            cx.simulate_mouse_up(release, MouseButton::Left, gpui::Modifiers::default());
            view.read_with(cx, |sftp, _| {
                if let Some(direction) = direction {
                    let conflicts = sftp
                        .conflict_state
                        .as_ref()
                        .expect("drop must reach the existing transfer flow");
                    let pending = &conflicts.pending_transfers;
                    assert_eq!(pending.len(), 1);
                    assert_eq!(pending[0].name, "drag.txt");
                    assert_eq!(pending[0].source.path, path);
                    assert_eq!(pending[0].direction, direction);
                } else {
                    assert!(
                        sftp.conflict_state.is_none(),
                        "cancelled drops must not transfer"
                    );
                    assert!(sftp.transfers.is_empty());
                }
                assert!(sftp.drag_state.is_none());
                assert!(sftp.drag_over_pane.is_none());
                assert!(sftp.drag_autoscroll_position.is_none());
            });
        }
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
        let tokens = self.tokens;
        let theme = tokens.ui;
        let compact = self.sftp_view().read(cx).current_surface_id == Some(SftpSurfaceId::Sidebar);
        let drag_over = self.sftp_view().read(cx).drag_over_pane == Some(pane);
        let list = div()
            .id(("sftp-file-list-scroll", pane as u64))
            .flex_1()
            .min_h(px(0.0))
            .bg(if drag_over {
                rgba((theme.accent << 8) | SFTP_DRAG_BG_ALPHA)
            } else {
                sftp_bg(theme.bg, has_background)
            })
            .on_mouse_move(self.sftp_listener(
                cx,
                move |this, event: &MouseMoveEvent, _window, cx| {
                    if this.update_sftp_drag(
                        pane,
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        cx,
                    ) {
                        cx.notify();
                    }
                },
            ))
            .on_mouse_up(
                MouseButton::Left,
                self.sftp_listener(cx, move |this, _event, _window, cx| {
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
                    .on_drop(self.sftp_listener(
                        cx,
                        |this, paths: &gpui::ExternalPaths, _window, cx| {
                            this.queue_sftp_external_upload_paths(paths.paths(), cx);
                            this.sftp_view().update(cx, |sftp, cx| {
                                sftp.drag_over_pane = None;
                                cx.notify();
                            });
                            cx.stop_propagation();
                        },
                    ))
            })
            .on_scroll_wheel(self.sftp_listener(cx, |this, _event, _window, cx| {
                // The menu is positioned in window coordinates, so any pane
                // scroll invalidates the row that produced the coordinates.
                this.sftp_view().update(cx, |sftp, cx| {
                    if sftp.clear_context_menu_immediately() {
                        cx.notify();
                    }
                });
            }))
            .on_mouse_down(
                MouseButton::Left,
                self.sftp_listener(cx, move |this, _event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    let menu_changed = this
                        .sftp_view()
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
                self.sftp_listener(cx, move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.sftp_view().update(cx, |sftp, cx| {
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

        let visible_indices = self.sftp_view().read(cx).visible_file_indices(pane);
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
        let sftp_view = self.sftp_view().clone();
        let visible_indices = std::sync::Arc::new(visible_indices);
        let scroll_handle = match pane {
            SftpPane::Local => self.sftp_view().read(cx).local_file_scroll.clone(),
            SftpPane::Remote => self.sftp_view().read(cx).remote_file_scroll.clone(),
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
                        let size_text =
                            if file.file_type == SftpFileType::Directory || !file.size_known {
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
                                    .child({
                                        let icon = if file.file_type == SftpFileType::Directory {
                                            oxideterm_gpui_ui::file_icons::folder_icon(
                                                &file.name, false,
                                            )
                                        } else {
                                            oxideterm_gpui_ui::file_icons::file_icon(&file.name)
                                        };
                                        icon.with_symlink(file.is_symlink)
                                            .render(SFTP_ICON_MD, &tokens)
                                    })
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
