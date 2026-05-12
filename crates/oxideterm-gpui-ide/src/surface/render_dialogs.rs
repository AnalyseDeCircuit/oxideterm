impl IdeSurface {
    fn render_disconnected_overlay(&self) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(rgba(IDE_OVERLAY_ALPHA))
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .text_color(rgb(self.tokens.ui.text))
            .child(self.icon("lucide/wifi-off.svg", 32.0, self.tokens.ui.error))
            .child(self.labels.disconnected_overlay.clone())
            .into_any_element()
    }

    fn render_dirty_close_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(request) = self.workspace.pending_close() else {
            return div().into_any_element();
        };
        let tokens = &self.tokens;
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, self.labels.unsaved_changes.clone()))
                    .child(dialog_description(
                        tokens,
                        self.labels
                            .unsaved_changes_desc
                            .replace("{{fileName}}", &request.title),
                    )),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
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
                                this.resolve_dirty_close(DirtyCloseDecision::Cancel, cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.discard.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_dirty_close(DirtyCloseDecision::Discard, cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.save.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_dirty_close(DirtyCloseDecision::Save, cx);
                            }),
                        ),
                    ),
            );
        dialog_backdrop()
            .child(dialog)
            .into_any_element()
    }

    fn render_conflict_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(conflict) = self.conflict_state.as_ref() else {
            return div().into_any_element();
        };
        let tokens = &self.tokens;
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.icon("lucide/alert-triangle.svg", 20.0, 0xeab308))
                            .child(dialog_title(tokens, self.labels.conflict_title.clone())),
                    )
                    .child(dialog_description(
                        tokens,
                        self.labels
                            .conflict_desc
                            .replace("{{fileName}}", &conflict.title),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(self.render_conflict_time_row(
                        self.labels.your_version.clone(),
                        format_conflict_mtime(conflict.local_mtime),
                        false,
                    ))
                    .child(self.render_conflict_time_row(
                        self.labels.remote_version.clone(),
                        format_conflict_mtime(conflict.remote_mtime),
                        true,
                    )),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
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
                                this.clear_conflict(cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.reload_remote.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .text_color(rgb(self.tokens.ui.info))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.reload_conflict(cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.overwrite.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.overwrite_conflict(cx);
                            }),
                        ),
                    ),
            );
        self.render_modal_overlay(dialog)
    }

    fn render_conflict_time_row(&self, label: String, value: String, accent: bool) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .py_2()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x80))
            .child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(SharedString::from("monospace"))
                    .text_color(rgb(if accent {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.text
                    }))
                    .child(value),
            )
            .into_any_element()
    }

    fn render_folder_switch_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, self.labels.unsaved_changes.clone()))
                    .child(dialog_description(
                        tokens,
                        self.labels.unsaved_changes_folder.clone(),
                    )),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
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
                                this.folder_switch_confirm_open = false;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.discard.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.folder_switch_confirm_open = false;
                                let Some(node_id) = this.node_id.clone() else {
                                    cx.notify();
                                    return;
                                };
                                let initial_path =
                                    this.root_path.clone().unwrap_or_else(|| "/".to_string());
                                this.open_remote_folder_picker_for_node(node_id, initial_path, cx);
                            }),
                        ),
                    ),
            );

        self.render_modal_overlay(dialog)
    }

    fn render_tree_name_input_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(input) = self.tree_name_input.as_ref() else {
            return div().into_any_element();
        };
        let tokens = &self.tokens;
        let (title, description, confirm_label) = match input.kind {
            TreeNameInputKind::NewFile => (
                self.labels.context_new_file.clone(),
                format!("Create in {}", input.parent_path),
                self.labels.context_new_file.clone(),
            ),
            TreeNameInputKind::NewFolder => (
                self.labels.context_new_folder.clone(),
                format!("Create in {}", input.parent_path),
                self.labels.context_new_folder.clone(),
            ),
            TreeNameInputKind::Rename => (
                self.labels.context_rename.clone(),
                input
                    .original_name
                    .as_ref()
                    .map(|name| format!("Rename {name}"))
                    .unwrap_or_else(|| self.labels.context_rename.clone()),
                self.labels.context_rename.clone(),
            ),
        };
        let can_submit = !input.submitting
            && !input.value.trim().is_empty()
            && validate_file_name(input.value.trim()).is_none();
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, title))
                    .child(dialog_description(tokens, description)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .h(px(36.0))
                            .w_full()
                            .flex()
                            .items_center()
                            .rounded(px(tokens.radii.sm))
                            .border_1()
                            .border_color(rgb(if input.error.is_some() {
                                tokens.ui.error
                            } else {
                                tokens.ui.border
                            }))
                            .bg(rgb(tokens.ui.bg_sunken))
                            .px_3()
                            .text_color(rgb(tokens.ui.text))
                            .child(if input.value.is_empty() {
                                div()
                                    .text_color(rgb(tokens.ui.text_muted))
                                    .child("filename.ext")
                                    .into_any_element()
                            } else {
                                div().child(input.value.clone()).into_any_element()
                            }),
                    )
                    .when_some(input.error.clone(), |this, error| {
                        this.child(
                            div()
                                .text_size(px(tokens.metrics.ui_text_xs))
                                .text_color(rgb(tokens.ui.error))
                                .child(error),
                        )
                    }),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: input.submitting,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.cancel_tree_name_input(cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            if input.submitting {
                                "Working...".to_string()
                            } else {
                                confirm_label
                            },
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: !can_submit,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.submit_tree_name_input(cx);
                            }),
                        ),
                    ),
            );

        self.render_modal_overlay(dialog)
    }

    fn render_project_search_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let result_count = self
            .search
            .results
            .iter()
            .map(|group| group.matches.len())
            .sum::<usize>();
        let panel = div()
            .absolute()
            .top(px(48.0))
            .left(px(IDE_TREE_DEFAULT_WIDTH + 12.0))
            .w(px(420.0))
            .max_h(px(560.0))
            .flex()
            .flex_col()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(tokens.ui.border))
            .bg(rgb(tokens.ui.bg))
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .h(px(42.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(tokens.ui.border))
                    .child(self.icon("lucide/search.svg", 15.0, tokens.ui.text_muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .rounded(px(tokens.radii.sm))
                            .bg(rgb(tokens.ui.bg_sunken))
                            .px_2()
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .text_color(if self.search.query.is_empty() {
                                rgb(tokens.ui.text_muted)
                            } else {
                                rgb(tokens.ui.text)
                            })
                            .child(if self.search.query.is_empty() {
                                "Search".to_string()
                            } else {
                                self.search.query.clone()
                            }),
                    )
                    .child(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(tokens.radii.sm))
                            .hover(|style| style.bg(rgb(tokens.ui.bg_hover)))
                            .child(self.icon("lucide/x.svg", 14.0, tokens.ui.text_muted))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.close_project_search(cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(if self.search.searching {
                        "Searching...".to_string()
                    } else if self.search.query.trim().is_empty() {
                        "Type to search project files".to_string()
                    } else {
                        format!("{result_count} result(s)")
                    }),
            )
            .when_some(self.search.error.clone(), |panel, error| {
                panel.child(
                    div()
                        .mx_3()
                        .mb_2()
                        .rounded(px(tokens.radii.sm))
                        .bg(rgba((TAILWIND_RED_500 << 8) | 0x1a))
                        .px_2()
                        .py_1()
                        .text_color(rgb(TAILWIND_RED_400))
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .child(error),
                )
            })
            .when(self.search.truncated, |panel| {
                panel.child(
                    div()
                        .mx_3()
                        .mb_2()
                        .text_color(rgb(TAILWIND_AMBER_400))
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .child("Results truncated. Refine your search."),
                )
            })
            .child(self.render_project_search_results(cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

        popover_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_project_search(cx);
                    cx.stop_propagation();
                }),
            )
            .child(panel)
            .into_any_element()
    }

    fn render_project_search_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let mut list = div()
            .id("ide-project-search-results")
            .h(px(450.0))
            .overflow_y_scroll()
            .px_2()
            .pb_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());

        if self.search.results.is_empty() {
            return list
                .child(
                    div()
                        .px_2()
                        .py_3()
                        .text_color(rgb(tokens.ui.text_muted))
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .child(if self.search.query.trim().is_empty() {
                            "No query".to_string()
                        } else if self.search.searching {
                            "Searching".to_string()
                        } else {
                            "No results".to_string()
                        }),
                )
                .into_any_element();
        }

        for group in &self.search.results {
            list = list.child(
                div()
                    .px_2()
                    .pt_2()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(tokens.ui.text))
                    .child(group.path.clone()),
            );
            for hit in &group.matches {
                let hit_for_click = hit.clone();
                list = list.child(
                    div()
                        .rounded(px(tokens.radii.sm))
                        .px_2()
                        .py_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(tokens.ui.bg_hover)))
                        .child(
                            div()
                                .text_color(rgb(tokens.ui.text_muted))
                                .text_size(px(tokens.metrics.ui_text_xs))
                                .child(format!("{}:{}", hit.line, hit.column)),
                        )
                        .child(
                            div()
                                .text_color(rgb(tokens.ui.text))
                                .text_size(px(tokens.metrics.ui_text_xs))
                                .child(hit.preview.clone()),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_search_match(hit_for_click.clone(), cx);
                            }),
                        ),
                );
            }
        }

        list.into_any_element()
    }

    fn render_delete_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(confirm) = self.delete_confirm.as_ref() else {
            return div().into_any_element();
        };
        let tokens = &self.tokens;
        let can_delete = confirm.unsaved_tab_count == 0 && !confirm.deleting;
        let item_kind = if confirm.is_directory {
            "folder"
        } else {
            "file"
        };
        let affected = confirm.affected_tab_count;
        let unsaved = confirm.unsaved_tab_count;
        let mut details = div().flex().flex_col().gap_2();
        if confirm.is_directory {
            details = details.child(
                div()
                    .text_color(rgb(TAILWIND_AMBER_400))
                    .child("This will permanently delete all contents inside the folder."),
            );
        }
        if affected > 0 && unsaved == 0 {
            details = details.child(format!("{affected} open tab(s) will be closed."));
        }
        if unsaved > 0 {
            details = details.child(
                div()
                    .text_color(rgb(TAILWIND_RED_400))
                    .child(format!(
                        "Cannot delete: {unsaved} file(s) have unsaved changes."
                    )),
            );
        }

        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.icon("lucide/alert-triangle.svg", 20.0, TAILWIND_RED_500))
                            .child(dialog_title(tokens, "Confirm Delete".to_string())),
                    )
                    .child(dialog_description(
                        tokens,
                        format!("Delete {} \"{}\"?", item_kind, confirm.name),
                    )),
            )
            .child(details)
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: confirm.deleting,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.cancel_delete_tree_item(cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            if confirm.deleting {
                                "Deleting...".to_string()
                            } else {
                                self.labels.context_delete.clone()
                            },
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: !can_delete,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.confirm_delete_tree_item(cx);
                            }),
                        ),
                    ),
            );

        self.render_modal_overlay(dialog)
    }

    fn render_folder_picker_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let current_path = self.folder_picker.current_path.clone();
        let selected_path = self.selected_folder_picker_path();
        let home_disabled = current_path == "/" || self.folder_picker.loading;
        let up_disabled = current_path == "/" || self.folder_picker.loading;
        let dialog = dialog_content(tokens)
            .w(px(IDE_FOLDER_DIALOG_WIDTH))
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, self.labels.select_folder.clone()))
                    .child(dialog_description(
                        tokens,
                        self.labels.select_folder_desc.clone(),
                    )),
            )
            .child(
                div()
                    .px(px(IDE_FOLDER_DIALOG_BODY_PADDING_X))
                    .py(px(IDE_FOLDER_DIALOG_BODY_GAP))
                    .flex()
                    .flex_col()
                    .gap(px(IDE_FOLDER_DIALOG_BODY_GAP))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.render_folder_path_input(cx))
                            .child(
                                button_with(
                                    tokens,
                                    self.labels.go.clone(),
                                    ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: self.folder_picker.loading,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.submit_folder_picker_path(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .h(px(tokens.metrics.ui_button_sm_height))
                                    .w(px(tokens.metrics.ui_button_sm_height))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(tokens.radii.md))
                                    .border_1()
                                    .border_color(rgb(tokens.ui.border))
                                    .opacity(if home_disabled { 0.5 } else { 1.0 })
                                    .cursor_pointer()
                                    .hover(|style| {
                                        if home_disabled {
                                            style
                                        } else {
                                            style.bg(rgba(
                                                (tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA,
                                            ))
                                        }
                                    })
                                    .child(self.icon(
                                        "lucide/home.svg",
                                        IDE_FOLDER_DIALOG_ICON_SIZE,
                                        tokens.ui.text,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            if !home_disabled {
                                                this.go_folder_picker_home(cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                button_with(
                                    tokens,
                                    self.labels.go_to_parent.clone(),
                                    ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: up_disabled,
                                    },
                                )
                                .child(self.icon(
                                    "lucide/arrow-up.svg",
                                    IDE_FOLDER_DIALOG_ICON_SIZE,
                                    tokens.ui.text,
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.go_folder_picker_parent(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    )
                    .child(self.render_folder_picker_list(cx))
                    .child(
                        div()
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .text_color(rgb(tokens.ui.text_muted))
                            .flex()
                            .items_center()
                            .gap_1()
                            .min_w_0()
                            .child(format!("{}: ", self.labels.selected_path))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .px_1()
                                    .rounded(px(tokens.radii.xs))
                                    .font_family(SharedString::from(
                                        tokens.metrics.markdown_code_font_family,
                                    ))
                                    .bg(rgb(tokens.ui.bg_panel))
                                    .text_color(rgb(tokens.ui.text))
                                    .child(selected_path),
                            ),
                    ),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
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
                                this.close_folder_picker(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.open_folder.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: self.folder_picker.loading,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.confirm_folder_picker(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            );

        self.render_modal_overlay(dialog)
    }

    fn render_folder_path_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let border = if self.folder_picker.path_input_focused {
            tokens.ui.accent
        } else {
            tokens.ui.border
        };
        div()
            .flex_1()
            .min_w_0()
            .h(px(tokens.metrics.form_input_height))
            .px(px(tokens.metrics.form_input_padding_x))
            .flex()
            .items_center()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(border))
            .bg(rgb(tokens.ui.bg_sunken))
            .font_family(SharedString::from(tokens.metrics.markdown_code_font_family))
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle);
                    this.folder_picker.path_input_focused = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(if self.folder_picker.path_input.is_empty() {
                        "/".to_string()
                    } else {
                        self.folder_picker.path_input.clone()
                    }),
            )
            .into_any_element()
    }

    fn render_folder_picker_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let mut list = div()
            .id("ide-folder-picker-list")
            .h(px(IDE_FOLDER_DIALOG_LIST_HEIGHT))
            .overflow_y_scroll()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(tokens.ui.border))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());

        if self.folder_picker.loading {
            return list
                .flex()
                .items_center()
                .justify_center()
                .child(self.icon("lucide/loader-circle.svg", 24.0, tokens.ui.text_muted))
                .into_any_element();
        }

        if let Some(error) = self.folder_picker.error.clone() {
            return list
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .p_4()
                .child(self.icon("lucide/alert-circle.svg", 24.0, tokens.ui.error))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .text_color(rgb(tokens.ui.error))
                        .text_align(gpui::TextAlign::Center)
                        .child(error),
                )
                .child(
                    button_with(
                        tokens,
                        self.labels.retry.clone(),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.load_folder_picker_current(cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element();
        }

        if self.folder_picker.folders.is_empty() {
            return list
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(tokens.ui.text_muted))
                .child(self.labels.no_subfolders.clone())
                .into_any_element();
        }

        let mut rows = div()
            .p(px(IDE_FOLDER_DIALOG_LIST_PADDING))
            .flex()
            .flex_col();
        for folder in self.folder_picker.folders.iter().cloned() {
            let selected = self.folder_picker.selected_folder.as_ref() == Some(&folder.name);
            let folder_name = folder.name.clone();
            // Tauri `IdeRemoteFolderDialog.tsx` renders `folders.map(...)`
            // directly inside the fixed-height scroller. The picker list is
            // small and variable-height, so native keeps the same direct rows;
            // uniform_list needs stricter row sizing and made loaded folders
            // look like an empty panel.
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(IDE_FOLDER_DIALOG_ROW_PADDING_X))
                    .py(px(IDE_FOLDER_DIALOG_ROW_PADDING_Y))
                    .rounded(px(tokens.radii.sm))
                    .cursor_pointer()
                    .bg(if selected {
                        rgba((tokens.ui.accent << 8) | IDE_FOLDER_DIALOG_SELECTED_ALPHA)
                    } else {
                        rgba(0x00000000)
                    })
                    .hover(|style| style.bg(rgba((tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA)))
                    .text_color(if selected {
                        rgb(tokens.ui.accent)
                    } else {
                        rgb(tokens.ui.text)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let folder_name = folder_name.clone();
                            move |this, event: &MouseDownEvent, _window, cx| {
                                if event.click_count >= 2 {
                                    this.enter_folder_picker_folder(&folder_name, cx);
                                } else if this.folder_picker.selected_folder.as_ref()
                                    == Some(&folder_name)
                                {
                                    this.folder_picker.selected_folder = None;
                                    cx.notify();
                                } else {
                                    this.folder_picker.selected_folder = Some(folder_name.clone());
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .child(if selected {
                        self.icon(
                            "lucide/folder-open.svg",
                            IDE_FOLDER_DIALOG_ICON_SIZE,
                            tokens.ui.accent,
                        )
                    } else {
                        self.icon(
                            "lucide/folder.svg",
                            IDE_FOLDER_DIALOG_ICON_SIZE,
                            tokens.ui.text_secondary,
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(folder.name.clone()),
                    )
                    .child(self.icon(
                        "lucide/chevron-right.svg",
                        IDE_FOLDER_DIALOG_ICON_SIZE,
                        tokens.ui.text_muted,
                    )),
            );
        }
        list = list.child(rows);
        list.into_any_element()
    }

}
