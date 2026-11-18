use super::*;

impl QuickCommandsView {
    pub(super) fn render_quick_commands_manager_modal(
        &self,
        target: Option<&QuickCommandTargetContext>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let snapshot = self.quick_commands_render_snapshot(target, cx);
        let theme = self.tokens.ui;
        let manager = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(QUICK_COMMANDS_MANAGER_WIDTH))
            .max_w_full()
            .h(px(QUICK_COMMANDS_MANAGER_HEIGHT))
            .max_h_full()
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .font_family(self.ui_font.clone())
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .h(px(54.0))
                    .flex_none()
                    .px(px(16.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Zap,
                                16.0,
                                rgb(theme.accent),
                            ))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("terminal.title")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(self.quick_command_tooltip_icon_button(
                                LucideIcon::Upload,
                                "quick-commands-import",
                                self.i18n.t("terminal.import"),
                                |this, _event, _window, cx| {
                                    this.import_quick_commands(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.quick_command_tooltip_icon_button(
                                LucideIcon::Download,
                                "quick-commands-export",
                                self.i18n.t("terminal.export"),
                                |this, _event, _window, cx| {
                                    this.export_quick_commands(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            ))
                            .child(self.quick_command_icon_button(
                                LucideIcon::X,
                                |this, _event, _window, cx| {
                                    this.close_quick_commands_manager(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .overflow_hidden()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .child(self.render_quick_command_category_sidebar(&snapshot, cx))
                    .child(self.render_quick_command_manager_list(&snapshot, cx))
                    .child(self.render_quick_command_manager_inspector(&snapshot, cx)),
            );

        let manager_dialog = staged_dialog_overlay(
            &self.tokens,
            "quick-commands-manager-dialog",
            TauriBackdropRole::Dialog,
            staged_overlay_root().on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_quick_commands_manager(cx);
                    cx.stop_propagation();
                }),
            ),
            overlay_content_boundary(manager),
            8.0,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
            true,
        );

        div()
            .size_full()
            .child(manager_dialog)
            .when_some(snapshot.pending_category_delete.as_ref(), |root, prompt| {
                let defaults = default_quick_command_categories();
                let destination = snapshot
                    .categories
                    .iter()
                    .chain(defaults.iter())
                    .find(|category| category.id == FALLBACK_QUICK_COMMAND_CATEGORY_ID)
                    .expect("default categories contain the fallback group");
                let description = self
                    .i18n
                    .t("terminal.delete_group_description")
                    .replace("{{name}}", &prompt.name)
                    .replace("{{destination}}", &destination.name);
                root.child(confirm_dialog(
                    &self.tokens,
                    ConfirmDialogView {
                        variant: ConfirmDialogVariant::Danger,
                        title: div()
                            .child(self.i18n.t("terminal.delete_group"))
                            .into_any_element(),
                        description: Some(div().child(description).into_any_element()),
                        cancel_label: div()
                            .child(self.i18n.t("common.actions.cancel"))
                            .into_any_element(),
                        confirm_label: div()
                            .child(self.i18n.t("terminal.delete_group"))
                            .into_any_element(),
                    },
                    cx.listener(|this, _event, _window, cx| {
                        this.cancel_quick_command_category_delete(cx);
                        cx.stop_propagation();
                    }),
                    cx.listener(|this, _event, _window, cx| {
                        this.confirm_quick_command_category_delete(cx);
                        cx.stop_propagation();
                    }),
                ))
            })
            .into_any_element()
    }

    pub(super) fn export_quick_commands(&mut self, cx: &mut Context<Self>) {
        let export = self.state.read(cx).store.export_snapshot_json();
        let Ok(snapshot_json) = export else {
            self.push_workspace_notice(
                QuickCommandsNotice {
                    title: self.i18n.t("terminal.export_failed"),
                    description: None,
                    variant: QuickCommandsNoticeKind::Error,
                },
                cx,
            );
            return;
        };
        // Command bodies may contain user-entered sensitive literals during explicit export.
        let snapshot_json = Zeroizing::new(snapshot_json);
        let receiver = cx.prompt_for_new_path(
            &quick_commands_export_directory(),
            Some("oxideterm-quick-commands.json"),
        );
        cx.spawn(async move |weak, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(path))) => {
                    oxideterm_atomic_file::durable_write(&path, snapshot_json.as_bytes())
                        .map(|()| Some(path))
                        .map_err(|_| ())
                }
                Ok(Ok(None)) => Ok(None),
                Ok(Err(_)) | Err(_) => Err(()),
            };
            let _ = weak.update(cx, |this, cx| match result {
                Ok(Some(path)) => this.push_workspace_notice(
                    QuickCommandsNotice {
                        title: this.i18n.t("terminal.export_success"),
                        description: Some(path.to_string_lossy().to_string()),
                        variant: QuickCommandsNoticeKind::Success,
                    },
                    cx,
                ),
                Ok(None) => {}
                Err(()) => this.push_workspace_notice(
                    QuickCommandsNotice {
                        title: this.i18n.t("terminal.export_failed"),
                        description: None,
                        variant: QuickCommandsNoticeKind::Error,
                    },
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(super) fn import_quick_commands(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(self.i18n.t("terminal.import"))),
        });
        cx.spawn(async move |weak, cx| {
            let selected_path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(_)) | Err(_) => {
                    let _ = weak.update(cx, |this, cx| {
                        this.push_workspace_notice(
                            QuickCommandsNotice {
                                title: this.i18n.t("terminal.import_failed"),
                                description: None,
                                variant: QuickCommandsNoticeKind::Error,
                            },
                            cx,
                        );
                    });
                    return;
                }
            };
            let Some(path) = selected_path else {
                return;
            };
            let snapshot_json = fs::metadata(&path)
                .map_err(|_| ())
                .and_then(|metadata| {
                    (metadata.len() <= oxideterm_quick_commands::MAX_QUICK_COMMANDS_FILE_BYTES)
                        .then_some(())
                        .ok_or(())
                })
                .and_then(|()| {
                    fs::read_to_string(&path)
                        .map(Zeroizing::new)
                        .map_err(|_| ())
                });
            let _ = weak.update(cx, |this, cx| {
                let Ok(snapshot_json) = snapshot_json else {
                    this.push_workspace_notice(
                        QuickCommandsNotice {
                            title: this.i18n.t("terminal.import_failed"),
                            description: None,
                            variant: QuickCommandsNoticeKind::Error,
                        },
                        cx,
                    );
                    return;
                };
                let result = this.state.update(cx, |terminal, _cx| {
                    // Desktop import keeps every existing record and renames conflicts.
                    terminal
                        .store
                        .apply_snapshot_json(&snapshot_json, QuickCommandImportStrategy::Rename)
                });
                if result.errors.is_empty() {
                    let title = this
                        .i18n
                        .t("terminal.import_success")
                        .replace("{{count}}", &result.imported.to_string());
                    this.push_workspace_notice(
                        QuickCommandsNotice {
                            title,
                            description: None,
                            variant: QuickCommandsNoticeKind::Success,
                        },
                        cx,
                    );
                    cx.notify();
                } else {
                    this.push_workspace_notice(
                        QuickCommandsNotice {
                            title: this.i18n.t("terminal.import_failed"),
                            description: None,
                            variant: QuickCommandsNoticeKind::Error,
                        },
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(super) fn render_quick_command_manager_list(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w(px(QUICK_COMMANDS_MANAGER_COMMAND_LIST_WIDTH))
            .h_full()
            .flex_none()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .child(self.render_quick_command_toolbar(snapshot, cx))
            .child(self.render_quick_command_rows(snapshot, cx))
            .into_any_element()
    }

    pub(super) fn render_quick_command_manager_inspector(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let content = if snapshot.category_editor.is_some() {
            self.render_quick_command_category_editor(snapshot, cx)
        } else if snapshot.command_editor.is_some() {
            self.render_quick_command_editor(snapshot, cx)
        } else {
            div()
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .text_color(rgb(theme.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::Pencil,
                    22.0,
                    rgb(theme.text_muted),
                ))
                .child(
                    div()
                        .max_w(px(280.0))
                        .text_center()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .child(self.i18n.t("terminal.manager_empty")),
                )
                .into_any_element()
        };
        div()
            .flex_1()
            .h_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .bg(rgba((theme.bg << 8) | 0x40))
            .child(content)
            .into_any_element()
    }

    pub(crate) fn render_quick_command_category_sidebar(
        &self,
        snapshot: &QuickCommandsRenderSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let sidebar = div()
            .w(px(if snapshot.managing { 220.0 } else { 160.0 }))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(if !snapshot.managing && self.has_background {
                // The dock owns the background; categories share its tint budget with controls.
                rgba(0x00000000)
            } else {
                rgba((theme.bg << 8) | 0x73)
            })
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .mb(px(2.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "quick-commands",
                                    "title",
                                    self.i18n
                                        .t(if snapshot.managing {
                                            "terminal.groups"
                                        } else {
                                            "terminal.title"
                                        })
                                        .to_uppercase(),
                                    theme.text_muted,
                                    cx,
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .when(!snapshot.managing, |actions| {
                                actions.child(self.quick_command_pin_button(
                                    snapshot.pinned,
                                    |this, _event, _window, cx| {
                                        this.state
                                            .update(cx, |terminal, _cx| terminal.toggle_pinned());
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                    cx,
                                ))
                            })
                            .when(snapshot.managing, |actions| {
                                actions.child(self.quick_command_icon_button(
                                    LucideIcon::Plus,
                                    |this, _event, window, cx| {
                                        this.start_quick_command_category_create(cx);
                                        window.focus(&this.focus, cx);
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
                            })
                            .when(!snapshot.managing, |actions| {
                                actions.child(self.quick_command_icon_button(
                                    LucideIcon::X,
                                    |this, _event, window, cx| {
                                        this.close_terminal_quick_commands_panel(cx);
                                        this.dispatch(
                                            QuickCommandsRequest::FocusPane,
                                            Some(window),
                                            cx,
                                        );
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                    cx,
                                ))
                            }),
                    ),
            );

        let mut category_list = div().flex().flex_col().gap(px(6.0));
        for category in &snapshot.categories {
            let category_id = category.id.clone();
            let active = category.id == snapshot.active_category;
            let count = snapshot
                .category_counts
                .get(category.id.as_str())
                .copied()
                .unwrap_or_default();
            let can_delete = category.id != FALLBACK_QUICK_COMMAND_CATEGORY_ID;
            category_list = category_list.child(
                div()
                    .id((
                        gpui::ElementId::from("quick-command-category"),
                        category_id.clone(),
                    ))
                    .group("quick-command-category")
                    .cursor_pointer()
                    .rounded(px(self.tokens.radii.md))
                    .px(px(8.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1f)
                    } else {
                        rgba(0x00000000)
                    })
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let category_id = category_id.clone();
                            move |this, _event, _window, cx| {
                                this.state.update(cx, |terminal, _cx| {
                                    terminal.select_category(&category_id)
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    )
                    .when(snapshot.managing, |row| {
                        let target = category_id.clone();
                        let drag = QuickCommandCategoryDrag {
                            id: category_id.clone(),
                            name: category.name.clone(),
                            position: gpui::Point::default(),
                            tokens: self.tokens,
                        };
                        row.drag_over::<QuickCommandCategoryDrag>(move |row, drag, _, _| {
                            if drag.id != target {
                                row.bg(rgba((theme.accent << 8) | 0x33))
                            } else {
                                row
                            }
                        })
                        .on_drop(cx.listener({
                            let target = category_id.clone();
                            move |this, drag: &QuickCommandCategoryDrag, _, cx| {
                                this.state.update(cx, |terminal, _| {
                                    if terminal.manager_open() {
                                        terminal.store.move_category(&drag.id, &target);
                                    }
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }))
                        .child(
                            div()
                                .id((
                                    gpui::ElementId::from("quick-command-category-drag"),
                                    category_id.clone(),
                                ))
                                .size(px(20.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(self.tokens.radii.sm))
                                .cursor(gpui::CursorStyle::OpenHand)
                                .hover(move |handle| handle.bg(rgb(theme.bg_hover)))
                                .child(div().w(px(6.0)).flex().flex_wrap().gap(px(2.0)).children(
                                    (0..6).map(move |_| {
                                        div()
                                            .size(px(2.0))
                                            .rounded(px(1.0))
                                            .bg(rgb(theme.text_muted))
                                    }),
                                ))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_drag(drag, |drag, position, _, cx| {
                                    let mut preview = drag.clone();
                                    preview.position = position;
                                    cx.new(|_| preview)
                                }),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(Self::render_lucide_icon(
                                quick_command_lucide_icon(category.icon),
                                14.0,
                                category
                                    .icon_color
                                    .as_deref()
                                    .and_then(oxideterm_quick_commands::parse_icon_color)
                                    .map(rgb)
                                    .unwrap_or_else(|| {
                                        if active {
                                            rgb(theme.accent)
                                        } else {
                                            rgb(theme.text_muted)
                                        }
                                    }),
                            ))
                            .child(div().flex_1().truncate().child(
                                // Tauri renders category labels as plain spans inside
                                // a button. Do not attach selectable-text mouse
                                // handlers here; category clicks must stay inside
                                // the category control instead of reaching workspace input blur.
                                self.render_display_text_with_role(
                                    SelectableTextRole::NonSelectable,
                                    "quick-command-category-cell",
                                    ("name", category.id.as_str()),
                                    category.name.clone(),
                                    if active {
                                        theme.accent
                                    } else {
                                        theme.text_muted
                                    },
                                    cx,
                                ),
                            ))
                            .child(status_pill(
                                &self.tokens,
                                count.to_string(),
                                StatusPillOptions::new(StatusTone::Neutral).compact(),
                            )),
                    )
                    .when(snapshot.managing, |row| {
                        row.child(self.quick_command_mini_button(
                            LucideIcon::Pencil,
                            {
                                let category = category.clone();
                                move |this, _event, window, cx| {
                                    this.start_quick_command_category_edit(category.clone(), cx);
                                    window.focus(&this.focus, cx);
                                    cx.stop_propagation();
                                }
                            },
                            cx,
                        ))
                    })
                    .when(snapshot.managing && can_delete, |row| {
                        row.child(self.quick_command_mini_button(
                            LucideIcon::Trash2,
                            {
                                let category = category.clone();
                                move |this, _event, _window, cx| {
                                    this.state.update(cx, |terminal, _cx| {
                                        terminal.request_category_delete(category.clone())
                                    });
                                    this.dispatch(QuickCommandsRequest::ClearIme, None, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            },
                            cx,
                        ))
                    }),
            );
        }

        sidebar
            .when(snapshot.managing, |sidebar| {
                sidebar.child(
                    div()
                        .px_2()
                        .pb_2()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("terminal.reorder_groups")),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .child(category_list),
            )
            .when(snapshot.last_persist_error.is_some(), |sidebar| {
                sidebar.child(
                    div()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.error << 8) | 0x80))
                        .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                        .p(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.error))
                        .child(self.i18n.t("terminal.persist_failed")),
                )
            })
            .into_any_element()
    }
}
