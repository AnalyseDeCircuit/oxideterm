use super::*;

fn sftp_transfer_queue_row_signature(transfer: &SftpTransferItem) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Transfer id is the row key; progress, speed, state, and error are visible
    // row fields and can change height when an error line appears.
    transfer.id.hash(&mut hasher);
    transfer.transfer_id.hash(&mut hasher);
    transfer.batch_id.hash(&mut hasher);
    transfer.remote_id.hash(&mut hasher);
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

#[derive(Clone)]
struct SftpTransferRowLabels {
    waiting: String,
    paused: String,
    completed: String,
    cancelled: String,
    error: String,
    pause_tooltip: String,
    resume_tooltip: String,
    cancel_tooltip: String,
    remove_tooltip: String,
    discard_tooltip: String,
    reveal_tooltip: String,
    loading: String,
}

#[derive(Clone)]
struct SftpTransferRowRenderer {
    sftp: Entity<SftpWorkspaceEntity>,
    theme: AppUiColors,
    radius: f32,
    mono_font: SharedString,
    labels: SftpTransferRowLabels,
}

#[derive(Clone)]
enum SftpTransferRowAction {
    SetState { id: u64, state: SftpTransferState },
    CancelOrRemove { id: u64 },
    ResumeIncomplete { transfer_id: String },
    DiscardIncomplete { transfer_id: String },
    RevealLocalPath { path: String },
}

impl SftpTransferRowRenderer {
    fn status_text(&self, transfer: &SftpTransferItem) -> String {
        match transfer.state {
            SftpTransferState::Pending => self.labels.waiting.clone(),
            SftpTransferState::Active => format_transfer_speed(transfer.speed),
            SftpTransferState::Paused => self.labels.paused.clone(),
            SftpTransferState::Completed => self.labels.completed.clone(),
            SftpTransferState::Cancelled => self.labels.cancelled.clone(),
            SftpTransferState::Error => transfer
                .error
                .clone()
                .unwrap_or_else(|| self.labels.error.clone()),
        }
    }

    fn action_button(
        &self,
        element_id: String,
        icon: LucideIcon,
        label: String,
        action: SftpTransferRowAction,
    ) -> AnyElement {
        let tooltip_id = element_id.clone();
        let tooltip_label = label.clone();
        let tooltip_sftp = self.sftp.clone();
        let clear_tooltip_id = element_id.clone();
        let clear_sftp = self.sftp.clone();
        let action_sftp = self.sftp.clone();

        div()
            .id(element_id)
            .size(px(SFTP_TOOL_BUTTON))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.radius))
            .text_color(rgb(self.theme.text))
            .hover({
                let hover = self.theme.bg_hover;
                move |button| button.bg(rgb(hover))
            })
            .cursor_pointer()
            .child(WorkspaceApp::render_lucide_icon(
                icon,
                SFTP_ICON_SM,
                rgb(self.theme.text),
            ))
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                tooltip_sftp.update(cx, |_sftp, cx| {
                    cx.emit(SftpWorkspaceEvent::TooltipRequested {
                        id: tooltip_id.clone(),
                        label: tooltip_label.clone(),
                        x: f32::from(event.position.x) + 12.0,
                        y: f32::from(event.position.y) + 16.0,
                    });
                });
            })
            .on_hover(move |hovered: &bool, _window, cx| {
                if !*hovered {
                    clear_sftp.update(cx, |_sftp, cx| {
                        cx.emit(SftpWorkspaceEvent::TooltipCleared {
                            id: clear_tooltip_id.clone(),
                        });
                    });
                }
            })
            .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                action_sftp.update(cx, |_sftp, cx| {
                    match &action {
                        SftpTransferRowAction::SetState { id, state } => {
                            cx.emit(SftpWorkspaceEvent::TransferStateRequested {
                                id: *id,
                                state: *state,
                            });
                        }
                        SftpTransferRowAction::CancelOrRemove { id } => {
                            cx.emit(SftpWorkspaceEvent::CancelOrRemoveTransferRequested {
                                id: *id,
                            });
                        }
                        SftpTransferRowAction::ResumeIncomplete { transfer_id } => {
                            cx.emit(SftpWorkspaceEvent::ResumeIncompleteTransferRequested {
                                transfer_id: transfer_id.clone(),
                            });
                        }
                        SftpTransferRowAction::DiscardIncomplete { transfer_id } => {
                            cx.emit(SftpWorkspaceEvent::DiscardIncompleteTransferRequested {
                                transfer_id: transfer_id.clone(),
                            });
                        }
                        SftpTransferRowAction::RevealLocalPath { path } => {
                            // Revealing a completed download is a local UI action and does
                            // not acquire or retain another SFTP/node consumer.
                            cx.reveal_path(Path::new(path));
                        }
                    }
                    cx.stop_propagation();
                });
            })
            .into_any_element()
    }

    fn render_transfer_actions(&self, transfer: &SftpTransferItem) -> AnyElement {
        let transfer_id = transfer.id;
        let status_color = match transfer.state {
            SftpTransferState::Error => SFTP_RED,
            SftpTransferState::Cancelled => SFTP_YELLOW,
            _ => self.theme.text_muted,
        };

        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(match transfer.state {
                SftpTransferState::Completed => {
                    WorkspaceApp::render_lucide_icon(LucideIcon::Check, 16.0, rgb(SFTP_GREEN))
                }
                SftpTransferState::Cancelled | SftpTransferState::Error => {
                    WorkspaceApp::render_lucide_icon(
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
                    actions.child(self.action_button(
                        format!("sftp-transfer-pause-{transfer_id}"),
                        LucideIcon::Pause,
                        self.labels.pause_tooltip.clone(),
                        SftpTransferRowAction::SetState {
                            id: transfer_id,
                            state: SftpTransferState::Paused,
                        },
                    ))
                },
            )
            .when(transfer.state == SftpTransferState::Paused, |actions| {
                actions.child(self.action_button(
                    format!("sftp-transfer-resume-{transfer_id}"),
                    LucideIcon::Play,
                    self.labels.resume_tooltip.clone(),
                    SftpTransferRowAction::SetState {
                        id: transfer_id,
                        state: SftpTransferState::Pending,
                    },
                ))
            })
            .when(
                transfer.state == SftpTransferState::Completed
                    && transfer.direction == SftpTransferDirection::Download,
                |actions| {
                    actions.child(self.action_button(
                        format!("sftp-transfer-reveal-{transfer_id}"),
                        LucideIcon::FolderOpen,
                        self.labels.reveal_tooltip.clone(),
                        SftpTransferRowAction::RevealLocalPath {
                            path: transfer.local_path.clone(),
                        },
                    ))
                },
            )
            .child(self.action_button(
                format!("sftp-transfer-dismiss-{transfer_id}"),
                LucideIcon::X,
                if matches!(
                    transfer.state,
                    SftpTransferState::Active
                        | SftpTransferState::Pending
                        | SftpTransferState::Paused
                ) {
                    self.labels.cancel_tooltip.clone()
                } else {
                    self.labels.remove_tooltip.clone()
                },
                SftpTransferRowAction::CancelOrRemove { id: transfer_id },
            ))
            .into_any_element()
    }

    fn render_compact_queue_item(&self, transfer: SftpTransferItem) -> AnyElement {
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
            _ => self.theme.text_muted,
        };
        let status_text = self.status_text(&transfer);

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .px_2()
            .py_1()
            .border_t_1()
            .border_color(rgb(self.theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_none()
                            .text_center()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(self.theme.text_muted))
                            .child(match transfer.direction {
                                SftpTransferDirection::Upload => "↑",
                                SftpTransferDirection::Download => "↓",
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(self.theme.text))
                            .child(transfer.name.clone()),
                    )
                    .child(
                        div()
                            .max_w(px(96.0))
                            .truncate()
                            .text_size(px(SFTP_TEXT_10))
                            .font_family(self.mono_font.clone())
                            .text_color(rgb(status_color))
                            .child(status_text),
                    )
                    .child(self.render_transfer_actions(&transfer)),
            )
            .child(
                div()
                    .ml(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .h(px(4.0))
                            .w_full()
                            .overflow_hidden()
                            .rounded_full()
                            .bg(rgb(self.theme.bg_panel))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(if indeterminate { 0.35 } else { progress }))
                                    .bg(rgba(
                                        (self.theme.accent << 8)
                                            | if indeterminate { 0x80 } else { 0xff },
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(SFTP_TEXT_10))
                            .text_color(rgb(self.theme.text_muted))
                            .child(if indeterminate {
                                format_file_size(transfer.transferred)
                            } else {
                                format!(
                                    "{} / {}",
                                    format_file_size(transfer.transferred),
                                    format_file_size(transfer.size)
                                )
                            })
                            .when(!indeterminate, |row| {
                                row.child(format!("{}%", (progress * 100.0).round() as u32))
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_queue_item(
        &self,
        transfer: SftpTransferItem,
        index: usize,
        total: usize,
        has_background: bool,
    ) -> AnyElement {
        let theme = self.theme;
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
        let status_text = self.status_text(&transfer);
        let destination_path = match transfer.direction {
            SftpTransferDirection::Upload => transfer.remote_path.clone(),
            SftpTransferDirection::Download => transfer.local_path.clone(),
        };
        let protocol_label = match transfer.protocol {
            RemoteTransferProtocol::Sftp => "SFTP",
            RemoteTransferProtocol::Scp => "SCP",
        };

        div()
            .px(px(8.0))
            .when(index == 0, |item| item.pt(px(8.0)))
            .pb(px(if index + 1 == total { 8.0 } else { 8.0 }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .p(px(8.0))
                    .rounded(px(self.radius))
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
                                    .truncate()
                                    .text_size(px(SFTP_TEXT_10))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{protocol_label} · {destination_path}")),
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
                                            .w(relative(if indeterminate {
                                                0.35
                                            } else {
                                                progress
                                            }))
                                            .bg(rgba(
                                                (theme.accent << 8)
                                                    | if indeterminate { 0x80 } else { 0xff },
                                            )),
                                    ),
                            )
                            // Only visible transfer rows format their progress.
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .text_size(px(SFTP_TEXT_10))
                                    .text_color(rgb(theme.text_muted))
                                    .child(if indeterminate {
                                        format_file_size(transfer.transferred)
                                    } else {
                                        format!(
                                            "{} / {}",
                                            format_file_size(transfer.transferred),
                                            format_file_size(transfer.size)
                                        )
                                    })
                                    .when(!indeterminate, |row| {
                                        row.child(format!("{}%", (progress * 100.0).round() as u32))
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .w(px(96.0))
                            .text_align(gpui::TextAlign::Right)
                            .text_size(px(SFTP_TEXT_XS))
                            .font_family(self.mono_font.clone())
                            .text_color(rgb(status_color))
                            .child(status_text),
                    )
                    .child(self.render_transfer_actions(&transfer)),
            )
            .into_any_element()
    }

    fn render_incomplete_item(
        &self,
        transfer: StoredTransferProgress,
        index: usize,
        has_background: bool,
    ) -> AnyElement {
        let theme = self.theme;
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
            oxideterm_sftp::TransferStatus::Paused => self.labels.paused.clone(),
            oxideterm_sftp::TransferStatus::Failed => self.labels.error.clone(),
            oxideterm_sftp::TransferStatus::Active => format_transfer_speed(0),
            oxideterm_sftp::TransferStatus::Completed => self.labels.completed.clone(),
            oxideterm_sftp::TransferStatus::Cancelled => self.labels.cancelled.clone(),
        };
        let transfer_id = transfer.transfer_id.clone();

        div()
            .px(px(8.0))
            .when(index == 0, |item| item.pt(px(8.0)))
            .pb(px(4.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .p(px(8.0))
                    .rounded(px(self.radius))
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
                            .child(div().truncate().text_color(rgb(theme.text)).child(name))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .text_size(px(SFTP_TEXT_10))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{transfer_type} · {protocol}"))
                                    .child("•")
                                    .child(format!("{:.0}%", transfer.progress_percent()))
                                    .child("•")
                                    .child(format!(
                                        "{} / {}",
                                        format_file_size(transfer.transferred_bytes),
                                        format_file_size(transfer.total_bytes)
                                    )),
                            )
                            .when_some(transfer.error.clone(), |row, error| {
                                row.child(
                                    div()
                                        .text_size(px(SFTP_TEXT_10))
                                        .text_color(rgb(SFTP_RED))
                                        .truncate()
                                        .child(error),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(SFTP_TEXT_10))
                            .text_color(rgb(theme.text_muted))
                            .child(status),
                    )
                    .when(transfer.is_incomplete(), |row| {
                        row.child(self.action_button(
                            format!("sftp-incomplete-resume-{transfer_id}"),
                            LucideIcon::RotateCcw,
                            self.labels.resume_tooltip.clone(),
                            SftpTransferRowAction::ResumeIncomplete {
                                transfer_id: transfer_id.clone(),
                            },
                        ))
                        .when(transfer.remote_relay.is_some(), |row| {
                            row.child(self.action_button(
                                format!("sftp-incomplete-discard-{transfer_id}"),
                                LucideIcon::Trash2,
                                self.labels.discard_tooltip.clone(),
                                SftpTransferRowAction::DiscardIncomplete {
                                    transfer_id: transfer_id.clone(),
                                },
                            ))
                        })
                    }),
            )
            .into_any_element()
    }

    fn render_loading_item(&self) -> AnyElement {
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
                    .text_color(rgb(self.theme.text_muted))
                    .child(WorkspaceApp::render_lucide_icon(
                        LucideIcon::RefreshCw,
                        SFTP_ICON_SM,
                        rgb(self.theme.text_muted),
                    ))
                    .child(self.labels.loading.clone()),
            )
            .into_any_element()
    }
}

impl WorkspaceApp {
    fn sftp_transfer_row_renderer(&self, _cx: &App) -> SftpTransferRowRenderer {
        SftpTransferRowRenderer {
            sftp: self.sftp_view.clone(),
            theme: self.tokens.ui,
            radius: self.tokens.radii.sm,
            mono_font: settings_mono_font_family(self.settings_store.settings()),
            labels: SftpTransferRowLabels {
                waiting: self.i18n.t("sftp.queue.status_waiting"),
                paused: self.i18n.t("sftp.queue.status_paused"),
                completed: self.i18n.t("sftp.queue.status_completed"),
                cancelled: self.i18n.t("sftp.queue.status_cancelled"),
                error: self.i18n.t("sftp.queue.status_error"),
                pause_tooltip: self.i18n.t("sftp.queue.pause_tooltip"),
                resume_tooltip: self.i18n.t("sftp.queue.resume_tooltip"),
                cancel_tooltip: self.i18n.t("sftp.queue.cancel_tooltip"),
                remove_tooltip: self.i18n.t("sftp.queue.remove_tooltip"),
                discard_tooltip: self.i18n.t("sftp.queue.discard_tooltip"),
                reveal_tooltip: self.i18n.t("fileManager.revealInFileManager"),
                loading: self.i18n.t("sftp.queue.loading"),
            },
        }
    }

    pub(in crate::workspace::sftp) fn render_sftp_sidebar_transfer_queue(
        &self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (transfer_count, active_count, mut transfers) = {
            let sftp = self.sftp_view.read(cx);
            let mut transfers = sftp
                .transfers
                .iter()
                .rev()
                .filter(|transfer| transfer.remote_id.node_id() == Some(node_id))
                .cloned()
                .collect::<Vec<_>>();
            let transfer_count = transfers.len();
            let active_count = transfers
                .iter()
                .filter(|transfer| {
                    matches!(
                        transfer.state,
                        SftpTransferState::Active | SftpTransferState::Pending
                    )
                })
                .count();
            // Stable sorting keeps the newest item first within each state group.
            transfers.sort_by_key(|transfer| match transfer.state {
                SftpTransferState::Active => 0,
                SftpTransferState::Pending => 1,
                SftpTransferState::Paused => 2,
                SftpTransferState::Error => 3,
                SftpTransferState::Completed => 4,
                SftpTransferState::Cancelled => 5,
            });
            (transfer_count, active_count, transfers)
        };
        if transfer_count == 0 {
            return None;
        }
        transfers.truncate(SFTP_SIDEBAR_TRANSFER_MAX_ROWS);

        let theme = self.tokens.ui;
        let renderer = self.sftp_transfer_row_renderer(cx);
        Some(
            div()
                .flex_none()
                .w_full()
                .flex()
                .flex_col()
                .border_t_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg))
                .child(
                    div()
                        .h(px(29.0))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .text_size(px(SFTP_TEXT_XS))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_muted))
                        .child(self.queue_title(active_count))
                        .child(
                            div()
                                .rounded_full()
                                .bg(rgb(theme.bg_panel))
                                .px_2()
                                .text_size(px(SFTP_TEXT_10))
                                .font_weight(gpui::FontWeight::NORMAL)
                                .child(transfer_count.to_string()),
                        ),
                )
                // This is a projection of the shared transfer entity. Hiding the
                // sidebar never cancels or takes ownership of a transfer task.
                .children(
                    transfers
                        .into_iter()
                        .map(|transfer| renderer.render_compact_queue_item(transfer)),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace::sftp) fn render_sftp_transfer_queue(
        &self,
        queue_height: f32,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (active_count, has_completed, incomplete_count, show_incomplete, transfers_empty) = {
            let sftp = self.sftp_view.read(cx);
            (
                sftp.transfers
                    .iter()
                    .filter(|item| {
                        matches!(
                            item.state,
                            SftpTransferState::Active | SftpTransferState::Pending
                        )
                    })
                    .count(),
                sftp.transfers
                    .iter()
                    .any(|item| item.state == SftpTransferState::Completed),
                sftp.incomplete_transfers.len(),
                sftp.show_incomplete,
                sftp.transfers.is_empty(),
            )
        };
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
                                            this.sftp_view.update(cx, |sftp, cx| {
                                                sftp.show_incomplete = !sftp.show_incomplete;
                                                cx.notify();
                                            });
                                            cx.stop_propagation();
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
                                this.sftp_view.update(cx, |sftp, cx| {
                                    sftp.transfers
                                        .retain(|item| item.state != SftpTransferState::Completed);
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                        ))
                    }),
            )
            .when(show_incomplete && has_incomplete, |queue| {
                queue.child(self.render_sftp_incomplete_section(has_background, cx))
            })
            .child(
                div()
                    .id("sftp-transfer-queue-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .when(transfers_empty, |body| {
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
                    .when(!transfers_empty, |body| {
                        self.sync_sftp_transfer_queue_list_state(cx);
                        let state = self.sftp_view.read(cx).transfer_queue_list_state.clone();
                        let spec = self.sftp_transfer_queue_list_spec();
                        let renderer = self.sftp_transfer_row_renderer(cx);
                        body.child(tauri_virtual_list(
                            state,
                            spec,
                            move |index, _window, cx| {
                                let (total, transfer) = {
                                    let sftp = renderer.sftp.read(cx);
                                    (sftp.transfers.len(), sftp.transfers.get(index).cloned())
                                };
                                transfer
                                    .map(|transfer| {
                                        renderer.render_queue_item(
                                            transfer,
                                            index,
                                            total,
                                            has_background,
                                        )
                                    })
                                    .unwrap_or_else(|| div().into_any_element())
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    fn sync_sftp_transfer_queue_list_state(&self, cx: &App) {
        let sftp = self.sftp_view.read(cx);
        let signatures = sftp
            .transfers
            .iter()
            .map(sftp_transfer_queue_row_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &sftp.transfer_queue_list_state,
            &mut sftp.transfer_queue_list_cache.borrow_mut(),
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
                .h(px(self.sftp_incomplete_transfer_list_height(cx)))
                .when(
                    self.sftp_incomplete_transfer_list_item_count(cx) > 0,
                    |list| {
                        self.sync_sftp_incomplete_transfer_list_state(cx);
                        let state = self
                            .sftp_view
                            .read(cx)
                            .incomplete_transfer_list_state
                            .clone();
                        let spec = self.sftp_incomplete_transfer_list_spec();
                        let renderer = self.sftp_transfer_row_renderer(cx);
                        list.child(tauri_virtual_list(
                            state,
                            spec,
                            move |index, _window, cx| {
                                renderer
                                    .sftp
                                    .read(cx)
                                    .incomplete_transfers
                                    .get(index)
                                    .cloned()
                                    .map(|transfer| {
                                        renderer.render_incomplete_item(
                                            transfer,
                                            index,
                                            has_background,
                                        )
                                    })
                                    .unwrap_or_else(|| renderer.render_loading_item())
                            },
                        ))
                    },
                ),
        )
        .into_any_element()
    }

    fn sftp_incomplete_transfer_list_item_count(&self, cx: &App) -> usize {
        let sftp = self.sftp_view.read(cx);
        sftp.incomplete_transfers.len() + usize::from(sftp.incomplete_load_inflight)
    }

    fn sftp_incomplete_transfer_list_height(&self, cx: &App) -> f32 {
        (self.sftp_incomplete_transfer_list_item_count(cx) as f32
            * SFTP_INCOMPLETE_TRANSFER_LIST_ESTIMATED_HEIGHT)
            .min(128.0)
    }

    fn sync_sftp_incomplete_transfer_list_state(&self, cx: &App) {
        let sftp = self.sftp_view.read(cx);
        let mut signatures = sftp
            .incomplete_transfers
            .iter()
            .map(sftp_incomplete_transfer_row_signature)
            .collect::<Vec<_>>();
        if sftp.incomplete_load_inflight {
            signatures.push(sftp_incomplete_loading_signature());
        }
        sync_tauri_variable_list_state_by_signatures(
            &sftp.incomplete_transfer_list_state,
            &mut sftp.incomplete_transfer_list_cache.borrow_mut(),
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
}
