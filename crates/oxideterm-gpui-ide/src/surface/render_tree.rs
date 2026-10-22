impl IdeSurface {
    fn render_empty_project(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.access_request.is_some() {
            let mut choices = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.labels.access_title.clone());
            for index in 0..self.labels.access_modes.len() {
                choices = choices.child(
                    button_with(
                        &self.tokens,
                        self.labels.access_modes[index].clone(),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Default,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if index == 1 {
                                this.select_ssh_access(window, cx);
                            } else {
                                this.select_access(oxideterm_ide_fs::RemoteAccess::Sftp, cx);
                            }
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
            if let Some(error) = &self.last_error {
                choices = choices.child(error.clone());
            }
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(choices)
                .into_any_element();
        }
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.labels.no_project.clone())
            .into_any_element()
    }

    fn render_loading_project(&self, _cx: &mut Context<Self>) -> AnyElement {
        let (label, percent) = self
            .agent_install_progress
            .map(|progress| self.labels.agent_install_status(progress))
            .unwrap_or_else(|| (self.labels.loading_project.clone(), None));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .items_center()
            .justify_center()
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.spinner_icon("ide-project-loading", 24.0, self.tokens.ui.accent))
            .child(label)
            .when(self.agent_install_progress.is_some(), |view| {
                view.child(
                    div()
                        .w(px(280.0))
                        .child(oxideterm_gpui_ui::progress::progress(
                            &self.tokens,
                            percent,
                            percent.is_none(),
                        )),
                )
            })
            .into_any_element()
    }

    fn render_project_error(&self, message: String, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .items_center()
            .justify_center()
            .text_color(rgb(tokens.ui.text_muted))
            .child(self.icon("lucide/alert-triangle.svg", 28.0, tokens.ui.warning))
            .child(
                div()
                    .text_color(rgb(tokens.ui.text_heading))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{}: {message}", self.labels.open_failed)),
            )
            .child(
                button_with(
                    tokens,
                    self.labels.retry.clone(),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.retry_open_project(cx);
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_workspace(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .when_some(self.last_error.clone(), |body, error| {
                body.child(
                    oxideterm_gpui_ui::state_notice(
                        &self.tokens,
                        oxideterm_gpui_ui::UiStateTone::Error,
                        self.icon(
                            "lucide/circle-alert.svg",
                            IDE_ICON_SIZE,
                            self.tokens.ui.error,
                        ),
                        error,
                        None,
                    )
                    .debug_selector(|| "ide-workspace-error".into())
                    .relative()
                    .flex_none()
                    .pr(px(self.tokens.metrics.ui_toast_padding * 2.0))
                    .child(
                        oxideterm_gpui_ui::toast::toast_close(&self.tokens)
                            .id("ide-dismiss-error")
                            .debug_selector(|| "ide-dismiss-error".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.last_error = None;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(self.render_tree_panel(cx))
                    .child(self.render_editor_area(cx)),
            )
            .child(self.render_status_bar(cx))
            .into_any_element()
    }

    fn render_tree_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let width = self
            .tree_width
            .clamp(IDE_TREE_MIN_WIDTH, IDE_TREE_MAX_WIDTH);
        let mut tree = div()
            .debug_selector(|| "ide-tree-panel".into())
            .relative()
            .flex_none()
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .bg(self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA));

        let Some(snapshot) = self.workspace.snapshot().ok() else {
            return tree.into_any_element();
        };
        let root_location = snapshot.project.root.clone();
        let root_title = snapshot.project.title.clone();
        let tree_scrollbar_handle = self.tree_scroll_handle.0.borrow().base_handle.clone();
        tree = tree
            .child(
                div()
                    .h(px(IDE_WORKSPACE_HEADER_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.icon(
                                "lucide/folder-open.svg",
                                IDE_ICON_SIZE,
                                self.tokens.ui.info,
                            ))
                            .child(div().truncate().child(snapshot.project.title.clone())),
                    )
                    .child({
                        let remote_disabled = !self.remote_actions_ready();
                        let folder_disabled = self.workspace.has_dirty_buffers()
                            || matches!(self.load_state, IdeLoadState::Loading)
                            || remote_disabled;
                        let refresh_disabled =
                            matches!(self.load_state, IdeLoadState::Loading) || remote_disabled;
                        let search_disabled =
                            matches!(self.load_state, IdeLoadState::Loading) || remote_disabled;
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("ide-tree-search")
                                    .size(px(IDE_TREE_TOOLBAR_BUTTON_SIZE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.sm))
                                    .opacity(if search_disabled { 0.5 } else { 1.0 })
                                    .hover(|style| {
                                        if search_disabled {
                                            style
                                        } else {
                                            style.bg(rgba(
                                                (self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA,
                                            ))
                                        }
                                    })
                                    .child(self.icon(
                                        "lucide/search.svg",
                                        IDE_TREE_TOOLBAR_ICON_SIZE,
                                        self.tokens.ui.text_muted,
                                    ))
                                    .tooltip({
                                        let tokens = self.tokens;
                                        let label = self.labels.search.clone();
                                        move |_window, cx| {
                                            oxideterm_gpui_ui::tooltip::tooltip_view(
                                                tokens,
                                                label.clone(),
                                                None,
                                                cx,
                                            )
                                        }
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, window, cx| {
                                            if !search_disabled {
                                                this.open_project_search(window, cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("ide-tree-open-folder")
                                    .size(px(IDE_TREE_TOOLBAR_BUTTON_SIZE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.sm))
                                    .opacity(if folder_disabled { 0.5 } else { 1.0 })
                                    .hover(|style| {
                                        if folder_disabled {
                                            style
                                        } else {
                                            style.bg(rgba(
                                                (self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA,
                                            ))
                                        }
                                    })
                                    .child(self.icon(
                                        "lucide/folder-input.svg",
                                        IDE_TREE_TOOLBAR_ICON_SIZE,
                                        self.tokens.ui.text_muted,
                                    ))
                                    .tooltip({
                                        let tokens = self.tokens;
                                        let label = self.labels.open_folder.clone();
                                        move |_window, cx| {
                                            oxideterm_gpui_ui::tooltip::tooltip_view(
                                                tokens,
                                                label.clone(),
                                                None,
                                                cx,
                                            )
                                        }
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            if !folder_disabled {
                                                this.request_open_folder_picker(cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("ide-tree-refresh")
                                    .size(px(IDE_TREE_TOOLBAR_BUTTON_SIZE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.sm))
                                    .opacity(if refresh_disabled { 0.5 } else { 1.0 })
                                    .hover(|style| {
                                        if refresh_disabled {
                                            style
                                        } else {
                                            style.bg(rgba(
                                                (self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA,
                                            ))
                                        }
                                    })
                                    .child(self.icon(
                                        "lucide/refresh-cw.svg",
                                        IDE_TREE_TOOLBAR_ICON_SIZE,
                                        self.tokens.ui.text_muted,
                                    ))
                                    .tooltip({
                                        let tokens = self.tokens;
                                        let label = self.labels.refresh.clone();
                                        move |_window, cx| {
                                            oxideterm_gpui_ui::tooltip::tooltip_view(
                                                tokens,
                                                label.clone(),
                                                None,
                                                cx,
                                            )
                                        }
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            if !refresh_disabled {
                                                this.refresh_project_tree_root(cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                    }),
            )
            .child(
                div()
                    .id("ide-tree-scroll")
                    .relative()
                    // Leave the existing scrollbar outside the divider's hit target.
                    .mr(px(SIDEBAR_RESIZE_HOTZONE_WIDTH))
                    .flex_1()
                    .min_h_0()
                    .py_1()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            window.focus(&this.focus_handle, cx);
                            let _ = this.workspace.select_tree_entry(None);
                            this.tree_context_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.open_tree_context_menu(
                                root_location.clone(),
                                true,
                                root_title.clone(),
                                event.position,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    // Tauri's tree is a native browser scroller over fixed-height
                    // rows. GPUI needs `uniform_list` here to keep the same
                    // trackpad feel without laying out every file on each frame.
                    .child(self.render_tree_rows(snapshot.project.root, cx))
                    // UniformList owns the actual scroll range through its base
                    // handle; share that handle so the visible thumb and drag
                    // interaction always follow the virtualized file tree.
                    .child(
                        Scrollbar::new(&tree_scrollbar_handle).id("ide-tree-vertical-scrollbar"),
                    ),
            );
        tree.child(
            sidebar_resize_hotzone_chrome(
                "ide-tree-resize",
                if self.tree_resize.is_some() {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgba((self.tokens.ui.border << 8) | IDE_BORDER_HALF_ALPHA)
                },
                true,
            )
            .right_0()
            .top_0()
            .bottom_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.tree_resize = Some((event.position.x, width));
                    this.tree_resize_activation =
                        Some(cx.observe_window_activation(window, |this, window, cx| {
                            if !window.is_window_active() {
                                this.finish_tree_resize(cx);
                            }
                        }));
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }),
            ),
        )
        .into_any_element()
    }

    fn update_tree_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((start_x, start_width)) = self.tree_resize else {
            return;
        };
        if !event.dragging() {
            self.finish_tree_resize(cx);
            return;
        }
        let width = (start_width + f32::from(event.position.x - start_x))
            .clamp(IDE_TREE_MIN_WIDTH, IDE_TREE_MAX_WIDTH);
        if self.tree_width != width {
            self.tree_width = width;
            cx.notify();
        }
    }

    fn finish_tree_resize(&mut self, cx: &mut Context<Self>) {
        if self.tree_resize.take().is_some() {
            self.tree_resize_activation = None;
            cx.notify();
        }
    }

    fn render_tree_rows(&mut self, root: IdeLocation, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.flatten_tree_rows(root);
        if rows.is_empty() {
            return div()
                .id("ide-tree-scroll-content")
                .size_full()
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.labels.no_subfolders.clone()),
                )
                .into_any_element();
        }

        let row_count = rows.len();
        let selected = self
            .workspace
            .file_tree()
            .selection()
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let loading_paths = Arc::new(self.loading_paths.clone());
        let tokens = self.tokens;
        let entity = cx.entity();

        uniform_list(
            "ide-tree-scroll-content",
            row_count,
            move |range, _window, _cx| {
                range
                    .filter_map(|index| rows.get(index).cloned())
                    .map(|row| {
                        render_tree_row_virtual(
                            row,
                            &selected,
                            loading_paths.as_ref(),
                            &tokens,
                            entity.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            },
        )
        .track_scroll(&self.tree_scroll_handle)
        .size_full()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .into_any_element()
    }

    fn flatten_tree_rows(&mut self, root: IdeLocation) -> Arc<Vec<TreeRenderRow>> {
        let root_key = root.stable_key();
        let tree_revision = self.workspace.file_tree().revision();
        if let Some(cache) = self.tree_rows_cache.as_ref()
            && cache.root_key == root_key
            && cache.tree_revision == tree_revision
        {
            return cache.rows.clone();
        }

        let mut rows = Vec::new();
        self.push_flattened_tree_rows(root, 0, &mut rows);
        // FileTreeState owns a revision counter, so the GPUI surface can keep
        // the expensive flattened tree stable across renders while selection
        // and loading state continue to resolve live per row.
        let rows = Arc::new(rows);
        self.tree_rows_cache = Some(TreeRowsCache {
            root_key,
            tree_revision,
            rows: rows.clone(),
        });
        rows
    }

    fn push_flattened_tree_rows(
        &self,
        directory: IdeLocation,
        depth: usize,
        rows: &mut Vec<TreeRenderRow>,
    ) {
        let children = self
            .workspace
            .file_tree()
            .children(&directory)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        for entry in children {
            let expanded = self.workspace.file_tree().is_expanded(&entry.location);
            rows.push(TreeRenderRow {
                entry: entry.clone(),
                depth,
                expanded,
            });
            if expanded && matches!(entry.kind, FileKind::Directory) {
                self.push_flattened_tree_rows(entry.location, depth + 1, rows);
            }
        }
    }
}

#[cfg(test)]
mod tree_resize_tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, point, size};

    struct OffsetIdeView(Entity<IdeSurface>);

    impl Render for OffsetIdeView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().pl(px(100.0)).child(self.0.clone())
        }
    }

    #[gpui::test]
    fn workspace_file_errors_are_visible_and_dismissible(cx: &mut TestAppContext) {
        let (surface, cx) = cx.add_window_view(|_, cx| {
            let router =
                oxideterm_ssh::NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::default());
            let backend = Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            );
            let mut surface = IdeSurface::new(
                NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled),
                oxideterm_theme::default_tokens(),
                IdeLabels::default(),
                IdeRuntimeSettings::default(),
                backend,
                cx,
            );
            surface
                .workspace
                .open_project(IdeLocation::local("/repo"), "repo");
            surface.load_state = IdeLoadState::Ready;
            let error = IdeFileError::new(IdeFileErrorKind::Binary, "File is binary");
            assert_eq!(
                surface.file_error_message(&error),
                "This is a binary file and cannot be opened in the text editor."
            );
            surface.last_error = Some(surface.file_error_message(&error));
            surface
        });
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ide-workspace-error").is_some());
        let close = cx.debug_bounds("ide-dismiss-error").unwrap().center();
        cx.simulate_mouse_down(close, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(close, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("ide-workspace-error").is_none());
        surface.read_with(cx, |surface, _| {
            assert_eq!(surface.load_state, IdeLoadState::Ready);
        });
    }

    #[gpui::test]
    fn tree_divider_drag_resizes_layout_and_stops_on_release(cx: &mut TestAppContext) {
        let surface = cx.new(|cx| {
            let router =
                oxideterm_ssh::NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::default());
            let backend = Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            );
            let mut surface = IdeSurface::new(
                NodeAgentIdeFileSystem::new(router, NodeAgentMode::Disabled),
                oxideterm_theme::default_tokens(),
                IdeLabels::default(),
                IdeRuntimeSettings::default(),
                backend,
                cx,
            );
            surface
                .workspace
                .open_project(IdeLocation::local("/repo"), "repo");
            surface
                .workspace
                .set_tree_children(
                    IdeLocation::local("/repo"),
                    (0..60)
                        .map(|index| FileTreeEntry {
                            location: IdeLocation::local(format!("/repo/{index:02}.rs")),
                            name: format!("{index:02}_long_filename_for_resize.rs"),
                            kind: FileKind::File,
                            version: SavedFileVersion::unknown(),
                        })
                        .collect(),
                )
                .unwrap();
            surface.load_state = IdeLoadState::Ready;
            surface
        });
        let (_, cx) = cx.add_window_view(|_, _| OffsetIdeView(surface.clone()));
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let modifiers = Modifiers::default();
        // Use an offset IDE surface and grab inside the hotzone, not on its edge.
        for (delta, expected) in [(100.0, 380.0), (400.0, 500.0), (-400.0, 200.0)] {
            let bounds = cx.debug_bounds("ide-tree-panel").unwrap();
            let start = point(bounds.right() - px(3.0), bounds.top() + px(80.0));
            let end = point(start.x + px(delta), start.y);
            cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
            cx.simulate_mouse_move(end, MouseButton::Left, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("ide-tree-panel").unwrap().size.width,
                px(expected)
            );
            cx.simulate_mouse_up(end, MouseButton::Left, modifiers);
            cx.simulate_mouse_move(start, None, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("ide-tree-panel").unwrap().size.width,
                px(expected)
            );
        }
        let bounds = cx.debug_bounds("ide-tree-panel").unwrap();
        let start = point(bounds.right() - px(3.0), bounds.top() + px(80.0));
        cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(
            start + point(px(110.0), px(0.0)),
            MouseButton::Left,
            modifiers,
        );
        cx.simulate_mouse_up(point(px(-10.0), px(-10.0)), MouseButton::Left, modifiers);
        cx.simulate_mouse_move(start, None, modifiers);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("ide-tree-panel").unwrap().size.width,
            px(310.0)
        );
        surface.read_with(cx, |surface, _| assert!(surface.tree_resize.is_none()));

        // Missing mouse-up must not leave a captured drag after re-entering.
        let start = point(px(407.0), px(80.0));
        cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(start + point(px(90.0), px(0.0)), None, modifiers);
        surface.read_with(cx, |surface, _| assert!(surface.tree_resize.is_none()));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        // The adjacent scrollbar must still drag without resizing the tree.
        let thumb = point(px(396.0), px(80.0));
        let end = point(thumb.x, px(350.0));
        cx.simulate_mouse_down(thumb, MouseButton::Left, modifiers);
        cx.simulate_mouse_move(
            thumb + point(px(0.0), px(10.0)),
            MouseButton::Left,
            modifiers,
        );
        cx.simulate_mouse_move(end, MouseButton::Left, modifiers);
        cx.simulate_mouse_up(end, MouseButton::Left, modifiers);
        surface.read_with(cx, |surface, _| {
            assert_eq!(surface.tree_width, 310.0);
            assert!(surface.tree_scroll_handle.0.borrow().base_handle.offset().y < px(0.0));
        });
        surface.update(cx, |surface, cx| {
            surface.search.open = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("ide-project-search-panel").unwrap().left(),
            px(422.0)
        );
    }
}
