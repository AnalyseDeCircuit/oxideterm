use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_sftp_dialog(
        &self,
        dialog: SftpDialog,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dialog_visible =
            self.sftp_view.dialog_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        if let Some(generation) = self.sftp_view.dialog_exit_generation {
            let delay = oxideterm_gpui_ui::motion::duration(
                &self.tokens,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            );
            // The generation check prevents a stale close timer from clearing
            // a replacement dialog that reopened during the exit interval.
            cx.spawn(async move |weak, cx| {
                Timer::after(delay).await;
                let _ = weak.update(cx, |this, cx| {
                    if this.finish_sftp_dialog_exit(generation) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if let SftpDialog::EditorCloseConfirm { name } = dialog.clone() {
            return self.render_sftp_editor_close_confirm_dialog(name, cx);
        }

        let theme = self.tokens.ui;
        let (title, description, body, primary) = match dialog.clone() {
            SftpDialog::Drives => (
                self.i18n.t("sftp.dialogs.select_drive"),
                self.i18n.t("sftp.dialogs.select_drive_desc"),
                self.render_sftp_drives_dialog_body(has_background, cx),
                None,
            ),
            SftpDialog::Rename { .. } => (
                self.i18n.t("sftp.dialogs.rename"),
                self.i18n.t("sftp.dialogs.rename_desc"),
                self.render_sftp_dialog_input("sftp.dialogs.rename_desc", cx),
                Some(self.i18n.t("sftp.dialogs.rename")),
            ),
            SftpDialog::NewFolder { .. } => (
                self.i18n.t("sftp.dialogs.new_folder"),
                self.i18n.t("sftp.dialogs.new_folder_desc"),
                self.render_sftp_dialog_input("sftp.dialogs.new_folder_placeholder", cx),
                Some(self.i18n.t("sftp.dialogs.create")),
            ),
            SftpDialog::Delete { files, .. } => (
                self.i18n.t("sftp.dialogs.delete"),
                self.i18n
                    .t("sftp.dialogs.delete_confirm")
                    .replace("{{count}}", &files.len().to_string()),
                self.render_sftp_delete_dialog_body(files, has_background),
                Some(self.i18n.t("sftp.dialogs.delete")),
            ),
            SftpDialog::Conflict => (
                self.i18n.t("sftp.conflict.title"),
                self.sftp_conflict_description(),
                self.render_sftp_conflict_body(has_background, cx),
                Some(self.i18n.t("sftp.conflict.overwrite")),
            ),
            SftpDialog::Diff {
                local_path,
                local_content,
                remote_path,
                remote_content,
            } => (
                self.i18n.t("sftp.diff.title"),
                self.i18n.t("sftp.diff.description"),
                self.render_sftp_diff_body(
                    &local_path,
                    &local_content,
                    &remote_path,
                    &remote_content,
                    has_background,
                    cx,
                ),
                Some(self.i18n.t("sftp.diff.close")),
            ),
            SftpDialog::Preview { name } => (
                name,
                self.i18n.t("sftp.preview.description"),
                self.render_sftp_preview_body(has_background, cx),
                Some(self.i18n.t("sftp.preview.close")),
            ),
            SftpDialog::Editor { name } => (
                name,
                self.i18n.t("sftp.preview.editor_description"),
                self.render_sftp_editor_body(has_background, cx),
                None,
            ),
            SftpDialog::EditorCloseConfirm { .. } => unreachable!(),
        };
        let width = match &dialog {
            SftpDialog::Drives => SFTP_DIALOG_WIDTH_XS,
            SftpDialog::Rename { .. }
            | SftpDialog::NewFolder { .. }
            | SftpDialog::Delete { .. } => SFTP_DIALOG_WIDTH_SM,
            SftpDialog::Conflict => SFTP_DIALOG_WIDTH_LG,
            SftpDialog::Diff { .. } => SFTP_DIALOG_WIDTH_5XL,
            SftpDialog::Preview { .. } => SFTP_DIALOG_WIDTH_4XL,
            SftpDialog::Editor { .. } => SFTP_EDITOR_DIALOG_WIDTH_6XL,
            SftpDialog::EditorCloseConfirm { .. } => unreachable!(),
        };
        let height_ratio = match &dialog {
            SftpDialog::Diff { .. } => Some(SFTP_DIFF_DIALOG_HEIGHT_RATIO),
            SftpDialog::Preview { .. } | SftpDialog::Editor { .. } => {
                Some(SFTP_PREVIEW_DIALOG_HEIGHT_RATIO)
            }
            _ => None,
        };
        let header_py = match &dialog {
            SftpDialog::Preview { .. } => 8.0,
            _ => 12.0,
        };
        let show_description =
            !description.is_empty() && !matches!(&dialog, SftpDialog::Preview { .. });
        let edge_owned_dialog = matches!(
            &dialog,
            SftpDialog::Preview { .. } | SftpDialog::Editor { .. } | SftpDialog::Diff { .. }
        );

        let outside_dialog = dialog.clone();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    // Tauri SFTP dialogs are Radix Dialogs. Backdrop clicks map
                    // to their onOpenChange(false) close/cancel path; editor
                    // shells run the same dirty-check path as the close button.
                    match outside_dialog {
                        SftpDialog::Editor { .. } => this.request_close_sftp_editor(),
                        SftpDialog::Conflict => this.cancel_sftp_transfer_conflicts(),
                        _ => this.close_sftp_dialog(),
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "sftp-dialog-presence",
                div()
                    .w(px(width))
                    .max_w(relative(0.9))
                    .max_h(relative(0.9))
                    .when_some(height_ratio, |dialog, ratio| dialog.h(relative(ratio)))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .when(!edge_owned_dialog, |dialog| {
                        // Compact dialogs rely on DialogContent's background behind
                        // padded body content. Full-height preview/editor/diff shells
                        // let header, body, and footer own every edge so GPUI's
                        // rectangular overflow mask cannot expose a second corner color.
                        dialog.bg(rgb(theme.bg_elevated))
                    })
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .shadow(vec![gpui::BoxShadow {
                        inset: false,
                        color: gpui::Hsla::from(rgba(SFTP_DIALOG_SHADOW_ALPHA)),
                        offset: gpui::point(px(0.0), px(16.0)),
                        blur_radius: px(32.0),
                        spread_radius: px(0.0),
                    }])
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(header_py))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            // Browser DialogContent clips this painted header
                            // into the rounded shell. GPUI needs the edge child
                            // to own the top corners to avoid rectangular leaks.
                            .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            // Mirrors DialogHeader bg-theme-bg-panel, not the tab background alpha path.
                            .bg(rgb(theme.bg_panel))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .when(matches!(&dialog, SftpDialog::Conflict), |row| {
                                        row.child(Self::render_lucide_icon(
                                            LucideIcon::AlertTriangle,
                                            20.0,
                                            rgb(SFTP_YELLOW),
                                        ))
                                    })
                                    .when(matches!(&dialog, SftpDialog::Diff { .. }), |row| {
                                        row.child(Self::render_lucide_icon(
                                            LucideIcon::ArrowLeftRight,
                                            16.0,
                                            rgb(theme.accent),
                                        ))
                                    })
                                    .when(matches!(&dialog, SftpDialog::Preview { .. }), |row| {
                                        row.font_family(settings_mono_font_family(
                                            self.settings_store.settings(),
                                        ))
                                    })
                                    .child(self.render_selectable_text_scoped(
                                        "sftp-dialog-title",
                                        &title,
                                        title.clone(),
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .when(show_description, |header| {
                                header.child(
                                    div()
                                        .mt(px(6.0))
                                        .text_size(px(
                                            if matches!(&dialog, SftpDialog::Diff { .. }) {
                                                SFTP_TEXT_XS
                                            } else {
                                                SFTP_TEXT_SM
                                            },
                                        ))
                                        .text_color(rgb(theme.text_muted))
                                        .when(matches!(&dialog, SftpDialog::Conflict), |desc| {
                                            let remaining = self.sftp_conflict_remaining_count();
                                            desc.flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .child(self.render_selectable_text_scoped(
                                                    "sftp-dialog-description",
                                                    &title,
                                                    description.clone(),
                                                    theme.text_muted,
                                                    cx,
                                                ))
                                                .when(remaining > 0, |desc| {
                                                    desc.child(
                                                        div().text_color(rgb(SFTP_ORANGE)).child(
                                                            self.render_selectable_text_scoped(
                                                                "sftp-conflict-remaining",
                                                                remaining,
                                                                self.i18n
                                                                    .t("sftp.conflict.remaining")
                                                                    .replace(
                                                                        "{{count}}",
                                                                        &remaining.to_string(),
                                                                    ),
                                                                SFTP_ORANGE,
                                                                cx,
                                                            ),
                                                        ),
                                                    )
                                                })
                                        })
                                        .when(!matches!(&dialog, SftpDialog::Conflict), |desc| {
                                            desc.child(self.render_selectable_text_scoped(
                                                "sftp-dialog-description",
                                                &title,
                                                description,
                                                theme.text_muted,
                                                cx,
                                            ))
                                        }),
                                )
                            }),
                    )
                    .child(body)
                    .child(self.render_sftp_dialog_footer(
                        dialog.clone(),
                        primary,
                        has_background,
                        cx,
                    )),
                dialog_visible,
            ))
            .when(!dialog_visible, |backdrop| {
                // Retained exit payloads are visual-only and cannot receive
                // clicks, wheel input, or button activation while fading out.
                backdrop.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
                )
            })
            .into_any_element()
    }

    fn render_sftp_dialog_footer(
        &self,
        dialog: SftpDialog,
        primary: Option<String>,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (footer_px, footer_py) = match dialog {
            SftpDialog::Preview { .. } | SftpDialog::Editor { .. } | SftpDialog::Diff { .. } => {
                (8.0, 8.0)
            }
            _ => (16.0, 12.0),
        };
        let footer = div()
            .px(px(footer_px))
            .py(px(footer_py))
            .border_t_1()
            .border_color(rgb(theme.border))
            // Keep the footer background inside the dialog shell radius just
            // like Tauri's overflow-hidden DialogContent clipping.
            .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
            // Mirrors DialogFooter bg-theme-bg-panel, not the tab background alpha path.
            .bg(rgb(theme.bg_panel))
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_end()
            .gap(px(8.0));

        if let SftpDialog::Preview { name } = dialog.clone() {
            let path = self.sftp_view.preview_path.clone().unwrap_or_default();
            let can_compare = self.can_compare_sftp_preview(&name);
            let can_edit = self.can_edit_sftp_preview();
            let is_markdown = self.sftp_preview_is_markdown_content();
            let can_download = self.sftp_view.preview_pane == Some(SftpPane::Remote)
                && self.sftp_view.preview_path.is_some();
            return footer
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .px(px(8.0))
                        .truncate()
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(path),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .when(is_markdown, |actions| {
                            let label = if self.sftp_view.preview_markdown_source_mode {
                                self.i18n.t("sftp.preview.rendered")
                            } else {
                                self.i18n.t("sftp.preview.source")
                            };
                            actions.child(self.render_sftp_text_button(
                                label,
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    this.sftp_view.preview_markdown_source_mode =
                                        !this.sftp_view.preview_markdown_source_mode;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(can_edit, |actions| {
                            let name = name.clone();
                            actions.child(self.render_sftp_text_button(
                                self.i18n.t("sftp.preview.edit"),
                                true,
                                cx.listener(move |this, _event, window, cx| {
                                    this.open_sftp_preview_editor(&name, window, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(can_compare, |actions| {
                            let name = name.clone();
                            actions.child(self.render_sftp_text_button(
                                self.i18n.t("sftp.preview.compare"),
                                false,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.open_sftp_preview_compare(&name);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .when(can_download, |actions| {
                            let name = name.clone();
                            actions.child(self.render_sftp_text_button(
                                self.i18n.t("sftp.preview.download"),
                                false,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.download_sftp_preview(&name);
                                    this.close_sftp_dialog();
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        })
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.preview.close"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }

        if let SftpDialog::Editor { .. } = dialog.clone() {
            let path = self.sftp_view.preview_path.clone().unwrap_or_default();
            let saving = self.sftp_view.preview_editor_saving;
            let dirty = self.sftp_view.preview_editor_dirty;
            let save_label = if saving {
                self.i18n.t("sftp.preview.saving")
            } else {
                self.i18n.t("sftp.preview.save")
            };
            return footer
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .px(px(8.0))
                        .truncate()
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(path),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_text_button(
                            save_label,
                            true,
                            cx.listener(move |this, _event, _window, cx| {
                                if !saving && dirty {
                                    this.save_sftp_preview_editor(cx);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.preview.close"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.request_close_sftp_editor();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }

        if let SftpDialog::EditorCloseConfirm { name } = dialog.clone() {
            return footer
                .child(self.render_sftp_text_button(
                    self.i18n.t("sftp.dialogs.cancel"),
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.cancel_sftp_editor_close_confirm(name.clone());
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .child(self.render_sftp_text_button(
                    self.i18n.t("sftp.preview.discard"),
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.discard_sftp_editor_changes();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .into_any_element();
        }

        if let SftpDialog::Diff {
            local_content,
            remote_content,
            ..
        } = dialog.clone()
        {
            let stats = sftp_diff_stats(&compute_sftp_diff(&local_content, &remote_content));
            return footer
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(
                            self.i18n
                                .t("sftp.diff.unchanged")
                                .replace("{{count}}", &stats.unchanged.to_string()),
                        )
                        .child(", ")
                        .child(
                            div().text_color(rgb(SFTP_GREEN)).child(
                                self.i18n
                                    .t("sftp.diff.added")
                                    .replace("{{count}}", &stats.added.to_string()),
                            ),
                        )
                        .child(", ")
                        .child(
                            div().text_color(rgb(SFTP_RED)).child(
                                self.i18n
                                    .t("sftp.diff.removed")
                                    .replace("{{count}}", &stats.removed.to_string()),
                            ),
                        ),
                )
                .child(self.render_sftp_text_button(
                    self.i18n.t("sftp.diff.close"),
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_sftp_dialog();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .into_any_element();
        }

        if matches!(dialog, SftpDialog::Conflict) {
            let source_newer = self
                .sftp_view
                .conflict_state
                .as_ref()
                .and_then(|state| state.conflicts.get(state.current_index))
                .and_then(|conflict| Some(conflict.source_modified? > conflict.target_modified?));
            return footer
                .justify_between()
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_button_variant(
                            self.i18n.t("sftp.conflict.skip"),
                            SftpButtonVariant::Ghost,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_sftp_transfer_conflict(SftpConflictResolution::Skip);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .when(source_newer.is_some(), |actions| {
                            actions.child(self.render_sftp_button_variant(
                                self.i18n.t("sftp.conflict.skip_older"),
                                SftpButtonVariant::Ghost,
                                cx.listener(|this, _event, _window, cx| {
                                    this.resolve_sftp_transfer_conflict(
                                        SftpConflictResolution::SkipOlder,
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_button_variant(
                            self.i18n.t("sftp.conflict.keep_both"),
                            SftpButtonVariant::Secondary,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_sftp_transfer_conflict(SftpConflictResolution::Rename);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_button_variant(
                            self.i18n.t("sftp.conflict.overwrite"),
                            SftpButtonVariant::Default,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_sftp_transfer_conflict(
                                    SftpConflictResolution::Overwrite,
                                );
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }

        footer
            .child(self.render_sftp_text_button(
                self.i18n.t("sftp.dialogs.cancel"),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.close_sftp_dialog();
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .when_some(primary, |footer, label| {
                footer.child(self.render_sftp_text_button(
                    label,
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.accept_sftp_dialog();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .into_any_element()
    }
}
