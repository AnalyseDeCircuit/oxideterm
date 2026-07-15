impl IdeSurface {
    fn render_empty_project(&self, _cx: &mut Context<Self>) -> AnyElement {
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
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .items_center()
            .justify_center()
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.spinner_icon("ide-project-loading", 24.0, self.tokens.ui.accent))
            .child(self.labels.loading_project.clone())
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
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba((self.tokens.ui.border << 8) | IDE_BORDER_HALF_ALPHA))
            .bg(self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA));

        let Some(snapshot) = self.workspace.snapshot().ok() else {
            return tree.into_any_element();
        };
        let root_location = snapshot.project.root.clone();
        let root_title = snapshot.project.title.clone();
        tree = tree
            .child(
                div()
                    .h(px(36.0))
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
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            if !search_disabled {
                                                this.open_project_search(cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
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
                    .flex_1()
                    .min_h_0()
                    .py_1()
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
                    .child(self.render_tree_rows(snapshot.project.root, cx)),
            );
        tree.into_any_element()
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
        let selected = self.workspace.file_tree().selected().cloned();
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
                            selected.as_ref(),
                            loading_paths.as_ref(),
                            &tokens,
                            entity.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            },
        )
        .track_scroll(self.tree_scroll_handle.clone())
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
