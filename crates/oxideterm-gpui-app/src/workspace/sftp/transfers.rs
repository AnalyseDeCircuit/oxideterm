use super::*;

fn sftp_transfer_queue_row_signature(transfer: &SftpTransferItem) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Transfer id is the row key; progress, speed, state, and error are visible
    // row fields and can change height when an error line appears.
    transfer.id.hash(&mut hasher);
    transfer.transfer_id.hash(&mut hasher);
    transfer.batch_id.hash(&mut hasher);
    transfer.node_id.hash(&mut hasher);
    transfer.name.hash(&mut hasher);
    transfer.local_path.hash(&mut hasher);
    transfer.remote_path.hash(&mut hasher);
    format!("{:?}", transfer.direction).hash(&mut hasher);
    transfer.size.hash(&mut hasher);
    transfer.transferred.hash(&mut hasher);
    transfer.speed.hash(&mut hasher);
    format!("{:?}", transfer.state).hash(&mut hasher);
    transfer.error.hash(&mut hasher);
    hasher.finish()
}

fn sftp_incomplete_transfer_row_signature(transfer: &StoredTransferProgress) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Recovery rows are keyed by persisted transfer id. Progress, status, and
    // error affect visible row content and can add an error line.
    transfer.transfer_id.hash(&mut hasher);
    transfer.source_path.hash(&mut hasher);
    transfer.total_bytes.hash(&mut hasher);
    transfer.transferred_bytes.hash(&mut hasher);
    format!("{:?}", transfer.status).hash(&mut hasher);
    format!("{:?}", transfer.transfer_type).hash(&mut hasher);
    transfer.error.hash(&mut hasher);
    hasher.finish()
}

fn sftp_incomplete_loading_signature() -> u64 {
    let mut hasher = DefaultHasher::new();
    "sftp-incomplete-loading".hash(&mut hasher);
    hasher.finish()
}

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn render_sftp_transfer_queue(
        &self,
        queue_height: f32,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active_count = self
            .sftp_view
            .transfers
            .iter()
            .filter(|item| {
                matches!(
                    item.state,
                    SftpTransferState::Active | SftpTransferState::Pending
                )
            })
            .count();
        let has_completed = self
            .sftp_view
            .transfers
            .iter()
            .any(|item| item.state == SftpTransferState::Completed);
        let incomplete_count = self.sftp_view.incomplete_transfers.len();
        let has_incomplete = incomplete_count > 0;

        div()
            .h(px(queue_height))
            .flex_none()
            .flex()
            .flex_col()
            .bg(sftp_bg(theme.bg, has_background))
            .border_t_1()
            .border_color(sftp_border(theme.border, has_background))
            .child(
                div()
                    .h(px(29.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(8.0))
                    .py(px(4.0))
                    .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                    .border_b_1()
                    .border_color(sftp_border(theme.border, has_background))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(SFTP_TEXT_XS))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "sftp-queue-title",
                                &active_count,
                                self.queue_title(active_count),
                                theme.text_muted,
                                cx,
                            ))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("sftp.queue.shortcut_hint")),
                            )
                            .when(has_incomplete, |row| {
                                row.child(
                                    self.workspace_clickable_row_action(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .text_color(rgb(theme.accent))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::History,
                                                SFTP_ICON_SM,
                                                rgb(theme.accent),
                                            ))
                                            .child(
                                                self.i18n.t("sftp.queue.incomplete_count").replace(
                                                    "{{count}}",
                                                    &incomplete_count.to_string(),
                                                ),
                                            ),
                                        false,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.sftp_view.show_incomplete =
                                                !this.sftp_view.show_incomplete;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                                )
                            }),
                    )
                    .when(has_completed, |header| {
                        header.child(self.workspace_toolbar_action_button(
                            self.i18n.t("sftp.queue.clear_done"),
                            None,
                            ToolbarButtonOptions {
                                text_color: Some(rgb(theme.text)),
                                hover_background: Some(rgb(theme.bg_hover)),
                                // Queue toolbar labels are controls, so they stay out of
                                // read-only selection ownership and share button action guards.
                                ..ToolbarButtonOptions::compact_text(
                                    ButtonVariant::Ghost,
                                    ButtonRadius::Sm,
                                    24.0,
                                    8.0,
                                    SFTP_TEXT_XS,
                                )
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.sftp_view
                                    .transfers
                                    .retain(|item| item.state != SftpTransferState::Completed);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                    }),
            )
            .when(self.sftp_view.show_incomplete && has_incomplete, |queue| {
                queue.child(self.render_sftp_incomplete_section(has_background, cx))
            })
            .child(
                div()
                    .id("sftp-transfer-queue-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .when(self.sftp_view.transfers.is_empty(), |body| {
                        body.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(SFTP_TEXT_SM))
                                .text_color(rgb(theme.text_muted))
                                .child(self.render_selectable_display_text(
                                    "sftp-queue-empty",
                                    "empty",
                                    self.i18n.t("sftp.queue.empty"),
                                    theme.text_muted,
                                    cx,
                                )),
                        )
                    })
                    .when(!self.sftp_view.transfers.is_empty(), |body| {
                        self.sync_sftp_transfer_queue_list_state();
                        let state = self.sftp_view.transfer_queue_list_state.clone();
                        let spec = self.sftp_transfer_queue_list_spec();
                        let workspace = cx.entity();
                        body.child(tauri_virtual_list(
                            state,
                            spec,
                            move |index, _window, cx| {
                                workspace.update(cx, |this, cx| {
                                    this.render_sftp_transfer_queue_list_item(
                                        index,
                                        has_background,
                                        cx,
                                    )
                                })
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    fn sync_sftp_transfer_queue_list_state(&self) {
        let signatures = self
            .sftp_view
            .transfers
            .iter()
            .map(sftp_transfer_queue_row_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.sftp_view.transfer_queue_list_state,
            &mut self.sftp_view.transfer_queue_list_cache.borrow_mut(),
            "sftp-transfer-queue",
            &signatures,
            self.sftp_transfer_queue_list_spec(),
        );
    }

    fn sftp_transfer_queue_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(SFTP_TRANSFER_QUEUE_LIST_ESTIMATED_HEIGHT),
            SFTP_TRANSFER_QUEUE_LIST_OVERSCAN,
        )
    }

    fn render_sftp_transfer_queue_list_item(
        &self,
        index: usize,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let total = self.sftp_view.transfers.len();
        let Some(transfer) = self.sftp_view.transfers.get(index).cloned() else {
            return div().into_any_element();
        };
        div()
            .px(px(8.0))
            .when(index == 0, |item| item.pt(px(8.0)))
            .pb(px(if index + 1 == total { 8.0 } else { 8.0 }))
            .child(self.render_sftp_transfer_row(transfer, has_background, cx))
            .into_any_element()
    }

    fn render_sftp_incomplete_section(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        sftp_card_surface(
            div()
                .border_b_1()
                .border_color(sftp_border(theme.border, has_background))
                .bg(sftp_panel_bg(theme.bg_card, has_background, 0xff)),
            theme.bg_card,
        )
        .child(
            div()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(SFTP_TEXT_10))
                .text_color(rgb(theme.text_muted))
                .child(self.render_selectable_display_text(
                    "sftp-incomplete-title",
                    "title",
                    self.i18n.t("sftp.queue.incomplete_title").to_uppercase(),
                    theme.text_muted,
                    cx,
                )),
        )
        .child(
            div()
                .id("sftp-incomplete-transfer-scroll")
                .h(px(self.sftp_incomplete_transfer_list_height()))
                .when(
                    self.sftp_incomplete_transfer_list_item_count() > 0,
                    |list| {
                        self.sync_sftp_incomplete_transfer_list_state();
                        let state = self.sftp_view.incomplete_transfer_list_state.clone();
                        let spec = self.sftp_incomplete_transfer_list_spec();
                        let workspace = cx.entity();
                        list.child(tauri_virtual_list(
                            state,
                            spec,
                            move |index, _window, cx| {
                                workspace.update(cx, |this, cx| {
                                    this.render_sftp_incomplete_transfer_list_item(
                                        index,
                                        has_background,
                                        cx,
                                    )
                                })
                            },
                        ))
                    },
                ),
        )
        .into_any_element()
    }

    fn sftp_incomplete_transfer_list_item_count(&self) -> usize {
        self.sftp_view.incomplete_transfers.len()
            + usize::from(self.sftp_view.incomplete_load_inflight)
    }

    fn sftp_incomplete_transfer_list_height(&self) -> f32 {
        (self.sftp_incomplete_transfer_list_item_count() as f32
            * SFTP_INCOMPLETE_TRANSFER_LIST_ESTIMATED_HEIGHT)
            .min(128.0)
    }

    fn sync_sftp_incomplete_transfer_list_state(&self) {
        let mut signatures = self
            .sftp_view
            .incomplete_transfers
            .iter()
            .map(sftp_incomplete_transfer_row_signature)
            .collect::<Vec<_>>();
        if self.sftp_view.incomplete_load_inflight {
            signatures.push(sftp_incomplete_loading_signature());
        }
        sync_tauri_variable_list_state_by_signatures(
            &self.sftp_view.incomplete_transfer_list_state,
            &mut self.sftp_view.incomplete_transfer_list_cache.borrow_mut(),
            "sftp-incomplete-transfers",
            &signatures,
            self.sftp_incomplete_transfer_list_spec(),
        );
    }

    fn sftp_incomplete_transfer_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(SFTP_INCOMPLETE_TRANSFER_LIST_ESTIMATED_HEIGHT),
            SFTP_INCOMPLETE_TRANSFER_LIST_OVERSCAN,
        )
    }

    fn render_sftp_incomplete_transfer_list_item(
        &self,
        index: usize,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(transfer) = self.sftp_view.incomplete_transfers.get(index).cloned() {
            return div()
                .px(px(8.0))
                .when(index == 0, |item| item.pt(px(8.0)))
                .pb(px(4.0))
                .child(self.render_sftp_incomplete_row(transfer, has_background, cx))
                .into_any_element();
        }
        self.render_sftp_incomplete_loading_row(cx)
    }

    fn render_sftp_incomplete_loading_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .px(px(8.0))
            .py(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::RefreshCw,
                        SFTP_ICON_SM,
                        rgb(theme.text_muted),
                    ))
                    .child(self.render_selectable_display_text(
                        "sftp-incomplete-loading",
                        "loading",
                        self.i18n.t("sftp.queue.loading"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_sftp_incomplete_row(
        &self,
        transfer: StoredTransferProgress,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let name = transfer
            .source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| transfer.source_path.to_str().unwrap_or(""))
            .to_string();
        let transfer_type = match transfer.transfer_type {
            RemoteTransferType::Upload => "Upload",
            RemoteTransferType::Download => "Download",
        };
        let protocol = match transfer.protocol {
            RemoteTransferProtocol::Sftp => "SFTP",
            RemoteTransferProtocol::Scp => "SCP",
        };
        let status = match transfer.status {
            oxideterm_sftp::TransferStatus::Paused => self.i18n.t("sftp.queue.status_paused"),
            oxideterm_sftp::TransferStatus::Failed => self.i18n.t("sftp.queue.status_error"),
            oxideterm_sftp::TransferStatus::Active => {
                self.transfer_status_text(&SftpTransferItem {
                    id: 0,
                    transfer_id: transfer.transfer_id.clone(),
                    batch_id: None,
                    node_id: NodeId::new(String::new()),
                    name: name.clone(),
                    local_path: String::new(),
                    remote_path: String::new(),
                    direction: SftpTransferDirection::Download,
                    protocol: transfer.protocol,
                    size: transfer.total_bytes,
                    transferred: transfer.transferred_bytes,
                    state: SftpTransferState::Active,
                    speed: 0,
                    error: None,
                })
            }
            oxideterm_sftp::TransferStatus::Completed => self.i18n.t("sftp.queue.status_completed"),
            oxideterm_sftp::TransferStatus::Cancelled => self.i18n.t("sftp.queue.status_cancelled"),
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .p(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba(
                (SFTP_YELLOW << 8) | SFTP_TRANSFER_INCOMPLETE_BORDER_ALPHA,
            ))
            .bg(sftp_panel_bg(
                theme.bg_panel,
                has_background,
                SFTP_PANEL_80_ALPHA,
            ))
            .hover(|row| {
                row.border_color(rgba(
                    (SFTP_YELLOW << 8) | SFTP_TRANSFER_INCOMPLETE_HOVER_BORDER_ALPHA,
                ))
            })
            .text_size(px(SFTP_TEXT_XS))
            .child(
                div()
                    .w(px(16.0))
                    .text_center()
                    .text_color(rgb(SFTP_YELLOW))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(match transfer.transfer_type {
                        RemoteTransferType::Upload => "↑",
                        RemoteTransferType::Download => "↓",
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(div().truncate().text_color(rgb(theme.text)).child(
                        self.render_selectable_display_text(
                            "sftp-transfer-name",
                            &transfer.transfer_id,
                            name,
                            theme.text,
                            cx,
                        ),
                    ))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .text_size(px(SFTP_TEXT_10))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "sftp-transfer-type",
                                &transfer.transfer_id,
                                format!("{transfer_type} · {protocol}"),
                                theme.text_muted,
                                cx,
                            ))
                            .child("•")
                            .child(self.render_selectable_display_text(
                                "sftp-transfer-progress",
                                &transfer.transfer_id,
                                format!("{:.0}%", transfer.progress_percent()),
                                theme.text_muted,
                                cx,
                            ))
                            .child("•")
                            .child(self.render_selectable_display_text(
                                "sftp-transfer-size",
                                &transfer.transfer_id,
                                format!(
                                    "{} / {}",
                                    format_file_size(transfer.transferred_bytes),
                                    format_file_size(transfer.total_bytes)
                                ),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .when_some(transfer.error.clone(), |row, error| {
                        row.child(
                            div()
                                .text_size(px(SFTP_TEXT_10))
                                .text_color(rgb(SFTP_RED))
                                .truncate()
                                .child(self.render_selectable_display_text(
                                    "sftp-transfer-error",
                                    &transfer.transfer_id,
                                    error,
                                    SFTP_RED,
                                    cx,
                                )),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(SFTP_TEXT_10))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_display_text(
                        "sftp-transfer-status",
                        &transfer.transfer_id,
                        status,
                        theme.text_muted,
                        cx,
                    )),
            )
            .when(transfer.is_incomplete(), |row| {
                row.child(
                    div()
                        .size(px(SFTP_TOOL_BUTTON))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.sm))
                        .text_color(rgb(SFTP_YELLOW))
                        .hover(|button| {
                            button.bg(rgba((SFTP_YELLOW << 8) | SFTP_TRANSFER_CONTROL_HOVER_ALPHA))
                        })
                        .cursor_pointer()
                        .child(Self::render_lucide_icon(
                            LucideIcon::RotateCcw,
                            SFTP_ICON_SM,
                            rgb(SFTP_YELLOW),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener({
                                let transfer_id = transfer.transfer_id.clone();
                                move |this, _event, _window, cx| {
                                    this.resume_sftp_incomplete_transfer(transfer_id.clone());
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_sftp_transfer_row(
        &self,
        transfer: SftpTransferItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let progress = if transfer.size == 0 {
            0.0
        } else {
            (transfer.transferred as f32 / transfer.size as f32).clamp(0.0, 1.0)
        };
        let indeterminate =
            transfer.size == 0 && matches!(transfer.state, SftpTransferState::Active);
        let status_color = match transfer.state {
            SftpTransferState::Error => SFTP_RED,
            SftpTransferState::Cancelled => SFTP_YELLOW,
            _ => theme.text_muted,
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .p(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(match transfer.state {
                SftpTransferState::Error => {
                    rgba((SFTP_RED << 8) | SFTP_TRANSFER_ERROR_BORDER_ALPHA)
                }
                SftpTransferState::Cancelled => {
                    rgba((SFTP_YELLOW << 8) | SFTP_TRANSFER_CANCELLED_BORDER_ALPHA)
                }
                _ => rgba((theme.border << 8) | SFTP_TRANSFER_DEFAULT_BORDER_ALPHA),
            })
            .bg(sftp_panel_bg(
                theme.bg_panel,
                has_background,
                SFTP_PANEL_80_ALPHA,
            ))
            .hover(move |row| row.border_color(rgb(theme.border)))
            .text_size(px(SFTP_TEXT_SM))
            .child(
                div()
                    .w(px(16.0))
                    .text_center()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(match transfer.direction {
                        SftpTransferDirection::Upload => "↑",
                        SftpTransferDirection::Download => "↓",
                    }),
            )
            .child(
                div()
                    .w(px(192.0))
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(theme.text))
                            .child(transfer.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(SFTP_TEXT_10))
                            .text_color(rgb(theme.text_muted))
                            .child(match transfer.protocol {
                                RemoteTransferProtocol::Sftp => "SFTP",
                                RemoteTransferProtocol::Scp => "SCP",
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .h(px(6.0))
                            .w_full()
                            .overflow_hidden()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(if indeterminate { 0.35 } else { progress }))
                                    .bg(rgba(
                                        (theme.accent << 8)
                                            | if indeterminate { 0x80 } else { 0xff },
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(SFTP_TEXT_10))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "sftp-transfer-progress",
                                (&transfer.id, "bytes"),
                                if indeterminate {
                                    format_file_size(transfer.transferred)
                                } else {
                                    format!(
                                        "{} / {}",
                                        format_file_size(transfer.transferred),
                                        format_file_size(transfer.size)
                                    )
                                },
                                theme.text_muted,
                                cx,
                            ))
                            .when(!indeterminate, |row| {
                                row.child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "sftp-transfer-progress",
                                    (&transfer.id, "percent"),
                                    format!("{}%", (progress * 100.0).round() as u32),
                                    theme.text_muted,
                                    cx,
                                ))
                            }),
                    ),
            )
            .child(
                div()
                    .w(px(96.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(SFTP_TEXT_XS))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(status_color))
                    .child(self.transfer_status_text(&transfer)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(match transfer.state {
                        SftpTransferState::Completed => {
                            Self::render_lucide_icon(LucideIcon::Check, 16.0, rgb(SFTP_GREEN))
                        }
                        SftpTransferState::Cancelled | SftpTransferState::Error => {
                            Self::render_lucide_icon(
                                LucideIcon::AlertCircle,
                                16.0,
                                rgb(status_color),
                            )
                        }
                        _ => div().w(px(0.0)).into_any_element(),
                    })
                    .when(
                        matches!(
                            transfer.state,
                            SftpTransferState::Active | SftpTransferState::Pending
                        ),
                        |actions| {
                            actions.child(self.render_sftp_icon_button(
                                LucideIcon::Pause,
                                self.i18n.t("sftp.queue.pause_tooltip"),
                                cx.listener({
                                    let id = transfer.id;
                                    move |this, _event, _window, cx| {
                                        this.set_sftp_transfer_state(id, SftpTransferState::Paused);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }),
                                cx.entity(),
                            ))
                        },
                    )
                    .when(transfer.state == SftpTransferState::Paused, |actions| {
                        actions.child(self.render_sftp_icon_button(
                            LucideIcon::Play,
                            self.i18n.t("sftp.queue.resume_tooltip"),
                            cx.listener({
                                let id = transfer.id;
                                move |this, _event, _window, cx| {
                                    this.set_sftp_transfer_state(id, SftpTransferState::Pending);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }),
                            cx.entity(),
                        ))
                    })
                    .child(self.render_sftp_icon_button(
                        LucideIcon::X,
                        self.i18n.t(
                            if matches!(
                                transfer.state,
                                SftpTransferState::Active
                                    | SftpTransferState::Pending
                                    | SftpTransferState::Paused
                            ) {
                                "sftp.queue.cancel_tooltip"
                            } else {
                                "sftp.queue.remove_tooltip"
                            },
                        ),
                        cx.listener({
                            let id = transfer.id;
                            move |this, _event, _window, cx| {
                                this.cancel_or_remove_sftp_transfer(id);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                        cx.entity(),
                    )),
            )
            .into_any_element()
    }
}
