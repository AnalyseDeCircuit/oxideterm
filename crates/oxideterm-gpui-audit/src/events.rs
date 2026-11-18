use oxideterm_gpui_ui::virtual_list::{TauriVirtualListSpec, tauri_virtual_uniform_list};

use super::*;

impl AuditView {
    pub(super) fn render_audit_content(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.audit.view == AuditPage::Recordings {
            return self.render_audit_recordings(cx);
        }
        if self.audit.view == AuditPage::Sessions {
            return self.render_audit_sessions(cx);
        }
        let theme = self.tokens.ui;
        let health = self
            .audit
            .client
            .as_ref()
            .map(AuditClient::health)
            .unwrap_or_default();
        let error = self.audit.error.or(health.error);
        let body = if self.audit.records.is_empty() {
            if self.audit.loading {
                oxideterm_gpui_ui::loading_state(
                    &self.tokens,
                    Self::render_lucide_icon(LucideIcon::LoaderCircle, 20.0, rgb(theme.accent)),
                    self.i18n.t("event_log.audit.loading"),
                    None,
                )
                .into_any_element()
            } else if error.is_some() {
                oxideterm_gpui_ui::error_state(
                    &self.tokens,
                    Self::render_lucide_icon(LucideIcon::AlertTriangle, 20.0, rgb(theme.error)),
                    self.i18n.t("event_log.audit.unavailable"),
                    None,
                    None,
                )
                .into_any_element()
            } else {
                oxideterm_gpui_ui::empty_state(
                    &self.tokens,
                    Self::render_lucide_icon(LucideIcon::History, 20.0, rgb(theme.accent)),
                    self.i18n.t("event_log.audit.empty"),
                    None,
                    None,
                )
                .into_any_element()
            }
        } else {
            self.render_audit_list(cx)
        };
        div()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .flex_none()
                    .min_h(px(40.0))
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.audit_view_button(AuditPage::Events, cx))
                    .child(self.audit_view_button(AuditPage::Sessions, cx))
                    .child(self.audit_view_button(AuditPage::Recordings, cx))
                    .child(self.render_audit_filter(AuditFilter::Category, cx))
                    .child(
                        div()
                            .border_l_1()
                            .border_color(rgb(theme.border))
                            .pl_2()
                            .child(self.render_audit_filter(AuditFilter::Severity, cx)),
                    )
                    .child(div().flex_1())
                    .child(self.render_audit_toggle(cx))
                    .child(self.audit_icon_button(
                        LucideIcon::Settings,
                        "event_log.audit.settings",
                        false,
                        |this, _, _, cx| {
                            this.toggle_settings(cx);
                        },
                        cx,
                    ))
                    .child(self.audit_icon_button(
                        LucideIcon::Download,
                        "event_log.audit.export_json",
                        self.audit.loading,
                        |this, _, window, cx| {
                            this.export_audit(
                                oxideterm_audit::AuditExportFormat::Json,
                                false,
                                window,
                                cx,
                            )
                        },
                        cx,
                    ))
                    .child(self.audit_icon_button(
                        LucideIcon::FileSpreadsheet,
                        "event_log.audit.export_csv",
                        self.audit.loading,
                        |this, _, window, cx| {
                            this.export_audit(
                                oxideterm_audit::AuditExportFormat::Csv,
                                false,
                                window,
                                cx,
                            )
                        },
                        cx,
                    ))
                    .child(self.audit_icon_button(
                        LucideIcon::Shield,
                        "event_log.audit.export_details",
                        self.audit.loading,
                        |this, _, window, cx| {
                            this.export_audit(
                                oxideterm_audit::AuditExportFormat::Json,
                                true,
                                window,
                                cx,
                            )
                        },
                        cx,
                    ))
                    .child(self.audit_icon_button(
                        LucideIcon::RefreshCw,
                        "event_log.audit.refresh",
                        self.audit.loading,
                        |this, _, _, cx| this.refresh_audit(true, cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.render_audit_filter(AuditFilter::Time, cx))
                    .child(self.render_audit_filter(AuditFilter::Source, cx))
                    .child(self.render_audit_filter(AuditFilter::Outcome, cx))
                    .child(self.render_audit_filter(AuditFilter::SearchField, cx))
                    .child(self.audit_icon_button(
                        LucideIcon::X,
                        "event_log.audit.reset_filters",
                        false,
                        |this, _, _, cx| {
                            this.audit.query = AuditQuery {
                                latest_only: true,
                                limit: 100,
                                ..Default::default()
                            };
                            this.audit.search.zeroize();
                            this.audit.time_filter = 0;
                            this.refresh_audit(true, cx);
                        },
                        cx,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(180.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Self::render_lucide_icon(
                                LucideIcon::Search,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            .child(self.render_audit_search(cx)),
                    ),
            )
            .children(self.render_audit_disabled_hint())
            .when(self.audit.settings_open, |view| {
                view.child(
                    div()
                        .flex_none()
                        .px_3()
                        .py_2()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_3()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(self.render_audit_policy_inputs(cx))
                        .child(self.audit_icon_button(
                            LucideIcon::Trash2,
                            "event_log.clear",
                            self.audit.settings_task.is_some(),
                            |this, _, window, cx| this.clear_audit(window, cx),
                            cx,
                        )),
                )
            })
            .when(
                (error.is_some() && !self.audit.records.is_empty()) || health.unrecorded > 0,
                |view| {
                    view.child(
                        div()
                            .flex_none()
                            .px_3()
                            .py_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.error))
                            .child(Self::render_lucide_icon(
                                LucideIcon::AlertTriangle,
                                14.0,
                                rgb(theme.error),
                            ))
                            .child(self.i18n.t("event_log.audit.unavailable")),
                    )
                },
            )
            .child(div().flex_1().min_h(px(0.0)).overflow_hidden().child(body))
            .when_some(self.audit.selected.as_ref(), |view, record| {
                view.child(self.render_audit_detail(record, cx))
            })
            .child(
                div()
                    .flex_none()
                    .min_h(px(36.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(if self.audit.loading {
                        self.i18n.t("event_log.audit.loading")
                    } else {
                        self.i18n
                            .t("event_log.audit.page_summary")
                            .replace("{{page}}", &self.audit.cursors.len().to_string())
                            .replace("{{count}}", &self.audit.records.len().to_string())
                    })
                    .child(div().flex_1())
                    .child(self.audit_icon_button(
                        LucideIcon::ChevronLeft,
                        "event_log.audit.newer",
                        self.audit.loading || self.audit.cursors.len() <= 1,
                        |this, _, _, cx| {
                            this.audit.cursors.pop();
                            this.audit.selected = None;
                            this.audit.scroll = UniformListScrollHandle::new();
                            this.refresh_audit(false, cx);
                        },
                        cx,
                    ))
                    .child(self.audit_icon_button(
                        LucideIcon::ChevronRight,
                        "event_log.audit.older",
                        self.audit.loading || self.audit.next_cursor.is_none(),
                        |this, _, _, cx| {
                            this.audit.cursors.push(this.audit.next_cursor);
                            this.audit.selected = None;
                            this.audit.scroll = UniformListScrollHandle::new();
                            this.refresh_audit(false, cx);
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_audit_list(&self, cx: &Context<Self>) -> AnyElement {
        let records = self.audit.records.clone();
        let owner = cx.entity();
        let selected_id = self.audit.selected.as_ref().map(|r| r.id.clone());
        tauri_virtual_uniform_list(
            "audit-list",
            records.len(),
            self.audit.scroll.clone(),
            TauriVirtualListSpec::new(px(AUDIT_ROW_HEIGHT), 4),
            move |range, _window, app| {
                owner.update(app, |this, cx| {
                    let theme = this.tokens.ui;
                    range
                        .filter_map(|index| {
                            let record = records.get(index)?.clone();
                            let (icon, color) = match record.severity {
                                AuditSeverity::Info => (LucideIcon::Info, theme.text_muted),
                                AuditSeverity::Warning => {
                                    (LucideIcon::AlertTriangle, theme.warning)
                                }
                                AuditSeverity::Error => (LucideIcon::AlertCircle, theme.error),
                            };
                            let selected = selected_id.as_deref() == Some(record.id.as_str());
                            Some(
                                div()
                                    .id((gpui::ElementId::from("audit-record"), record.id.clone()))
                                    .w_full()
                                    .h(px(AUDIT_ROW_HEIGHT))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .border_b_1()
                                    .border_color(rgb(theme.border))
                                    .cursor_pointer()
                                    .when(selected, |row| row.bg(rgba((theme.accent << 8) | 0x14)))
                                    .hover(|row| row.bg(rgb(theme.bg_hover)))
                                    .child(div().flex_none().child(Self::render_lucide_icon(
                                        icon,
                                        16.0,
                                        rgb(color),
                                    )))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_3()
                                                    .child(div().flex_1().truncate().child(
                                                        audit_text(
                                                            &this.i18n,
                                                            &record.details.title,
                                                        ),
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex_none()
                                                            .text_size(px(this
                                                                .tokens
                                                                .metrics
                                                                .ui_text_xs))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(audit_time(
                                                                record.occurred_at_ms,
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_size(px(this.tokens.metrics.ui_text_xs))
                                                    .text_color(rgb(theme.text_muted))
                                                    .child({
                                                        let operation = record.details.operation.as_ref();
                                                        let mut fields = vec![
                                                            operation.map(|op| audit_operation_status(&this.i18n, op)).unwrap_or_else(|| this.i18n.t("event_log.outcome.unknown")),
                                                            operation.map(|op| this.i18n.t(&format!("event_log.source.{}", op.source.key()))).unwrap_or_else(|| record.details.source.to_string()),
                                                            record.details.actor.to_string(),
                                                            record.details.target.as_ref().map(|v| v.to_string()).unwrap_or_else(|| this.i18n.t("event_log.audit.unknown_target")),
                                                        ];
                                                        if let Some(duration) = operation.and_then(|op| op.duration_ms) { fields.push(format!("{duration} ms")); }
                                                        fields.join(" · ")
                                                    }),
                                            ),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _, _, cx| {
                                            this.audit.selected = Some(record.clone());
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    )
                                    .into_any_element(),
                            )
                        })
                        .collect()
                })
            },
        )
        .into_any_element()
    }

    fn render_audit_detail(&self, record: &AuditRecord, cx: &Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let session_id = record
            .details
            .operation
            .as_ref()
            .and_then(|op| op.session_id.clone());
        let operation_id = record.details.operation.as_ref().map(|op| op.id.clone());
        div()
            .flex_none()
            .max_h(px(240.0))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex_none()
                    .px_3()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(audit_text(&self.i18n, &record.details.title)),
                    )
                    .when_some(session_id.clone(), |row, session| {
                        row.child(self.audit_icon_button(
                            LucideIcon::Terminal,
                            "event_log.sessions.related",
                            false,
                            move |this, _, _, cx| this.show_audit_session(session.clone(), cx),
                            cx,
                        ))
                    })
                    .when_some(session_id, |row, session| {
                        row.child(self.audit_icon_button(
                            LucideIcon::Play,
                            "event_log.recordings.title",
                            false,
                            move |this, _, _, cx| this.open_session_recordings(session.clone(), cx),
                            cx,
                        ))
                    })
                    .when_some(operation_id.clone(), |row, operation| {
                        row.child(self.audit_icon_button(
                            LucideIcon::History,
                            "event_log.audit.operation_history",
                            false,
                            move |this, _, _, cx| {
                                this.audit.query = AuditQuery {
                                    operation_id: Some(operation.clone()),
                                    limit: 100,
                                    ..Default::default()
                                };
                                this.audit.search.zeroize();
                                this.refresh_audit(true, cx);
                            },
                            cx,
                        ))
                    })
                    .when_some(operation_id, |row, operation| {
                        row.child(self.audit_icon_button(
                            LucideIcon::ListTree,
                            "event_log.audit.related_operations",
                            false,
                            move |this, _, _, cx| {
                                this.audit.query = AuditQuery {
                                    latest_only: true,
                                    parent_id: Some(operation.clone()),
                                    limit: 100,
                                    ..Default::default()
                                };
                                this.audit.search.zeroize();
                                this.refresh_audit(true, cx);
                            },
                            cx,
                        ))
                    })
                    .child(self.audit_icon_button(
                        LucideIcon::X,
                        "event_log.close",
                        false,
                        |this, _, _, cx| {
                            this.audit.selected = None;
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .id("audit-detail")
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .px_3()
                    .pb_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when_some(record.details.operation.as_ref(), |view, operation| {
                        view.child(
                            self.audit_detail_line(
                                "event_log.audit.outcome",
                                &self
                                    .i18n
                                    .t(&format!("event_log.outcome.{}", operation.outcome.key())),
                            ),
                        )
                        .child(
                            self.audit_detail_line(
                                "event_log.audit.evidence",
                                &self
                                    .i18n
                                    .t(&format!("event_log.evidence.{}", operation.evidence.key())),
                            ),
                        )
                        .child(self.audit_detail_line(
                            "event_log.audit.authorization",
                            &self.i18n.t(&format!(
                                "event_log.authorization.{}",
                                operation.authorization.key()
                            )),
                        ))
                        .when_some(operation.authorization_ref.as_ref(), |view, reference| {
                            view.child(
                                self.audit_detail_line(
                                    "event_log.audit.authorization_ref",
                                    reference,
                                ),
                            )
                        })
                        .when_some(operation.phase, |view, phase| {
                            let key = match phase {
                                oxideterm_audit::AuditPhase::Observation => "observation",
                                oxideterm_audit::AuditPhase::Start => "start",
                                oxideterm_audit::AuditPhase::Result => "result",
                                oxideterm_audit::AuditPhase::Authorization => "authorization",
                                oxideterm_audit::AuditPhase::Progress => "progress",
                            };
                            view.child(self.audit_detail_line(
                                "event_log.audit.phase",
                                &self.i18n.t(&format!("event_log.phase.{key}")),
                            ))
                        })
                        .when_some(operation.exit_code, |view, code| {
                            view.child(
                                self.audit_detail_line(
                                    "event_log.audit.exit_code",
                                    &code.to_string(),
                                ),
                            )
                        })
                        .when_some(operation.duration_ms, |view, duration| {
                            view.child(self.audit_detail_line(
                                "event_log.audit.duration",
                                &format!("{duration} ms"),
                            ))
                        })
                        .when_some(operation.bytes, |view, bytes| {
                            view.child(
                                self.audit_detail_line("event_log.audit.bytes", &bytes.to_string()),
                            )
                        })
                        .when_some(operation.session_id.as_ref(), |view, session| {
                            view.child(self.audit_detail_line("event_log.audit.session", session))
                        })
                        .when_some(operation.capture, |view, capture| {
                            view.child(self.audit_detail_line(
                                "event_log.audit.capture",
                                &self.i18n.t(&format!("event_log.capture.{}", capture.key())),
                            ))
                        })
                        .when_some(operation.recovered_by.as_ref(), |view, instance| {
                            view.child(
                                self.audit_detail_line("event_log.audit.recovered_by", instance),
                            )
                        })
                        .when_some(operation.transport_id.as_ref(), |view, transport| {
                            view.child(
                                self.audit_detail_line("event_log.audit.transport", transport),
                            )
                        })
                        .when_some(operation.consumer_id.as_ref(), |view, consumer| {
                            view.child(self.audit_detail_line("event_log.audit.consumer", consumer))
                        })
                        .when_some(operation.protocol.as_ref(), |view, protocol| {
                            view.child(self.audit_detail_line(
                                "event_log.audit.protocol",
                                &self.i18n.t(&format!("event_log.protocol.{protocol}")),
                            ))
                        })
                        .when_some(operation.agent_id.as_ref(), |view, agent| {
                            view.child(self.audit_detail_line("event_log.audit.agent", agent))
                        })
                        .when_some(operation.parent_id.as_ref(), |view, parent| {
                            view.child(
                                self.audit_detail_line("event_log.audit.parent_operation", parent),
                            )
                        })
                        .child(self.audit_detail_line("event_log.audit.operation", &operation.id))
                    })
                    .child(self.audit_detail_line("event_log.audit.actor", &record.details.actor))
                    .child(self.audit_detail_line("event_log.audit.device", &record.details.device))
                    .when_some(record.details.remote_account.as_ref(), |view, account| {
                        view.child(
                            self.audit_detail_line("event_log.audit.remote_account", account),
                        )
                    })
                    .child(
                        self.audit_detail_line(
                            "event_log.audit.source",
                            &record
                                .details
                                .operation
                                .as_ref()
                                .map(|operation| {
                                    self.i18n
                                        .t(&format!("event_log.source.{}", operation.source.key()))
                                })
                                .unwrap_or_else(|| record.details.source.to_string()),
                        ),
                    )
                    .when_some(record.details.target.as_ref(), |view, target| {
                        view.child(self.audit_detail_line("event_log.audit.target", target))
                    })
                    .when_some(record.details.detail.as_ref(), |view, detail| {
                        view.child(div().child(audit_detail(&self.i18n, record, detail)))
                    })
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("event_log.audit.identity_note")),
                    ),
            )
            .into_any_element()
    }

    fn audit_detail_line(&self, key: &str, value: &str) -> gpui::Div {
        div()
            .flex()
            .gap_3()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .child(
                div()
                    .w(px(96.0))
                    .flex_none()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(key)),
            )
            .child(div().flex_1().min_w(px(0.0)).child(value.to_string()))
    }
}

fn audit_operation_status(i18n: &I18n, operation: &oxideterm_audit::OperationDetails) -> String {
    if operation.phase == Some(oxideterm_audit::AuditPhase::Authorization) {
        i18n.t(&format!(
            "event_log.authorization.{}",
            operation.authorization.key()
        ))
    } else {
        i18n.t(&format!("event_log.outcome.{}", operation.outcome.key()))
    }
}

fn audit_detail(i18n: &I18n, record: &AuditRecord, detail: &str) -> String {
    if matches!(
        detail,
        "ssh_connection"
            | "connection_bundle"
            | "privilege_credential"
            | "remote_desktop_credential"
            | "global_proxy_credential"
    ) {
        return i18n.t(&format!("event_log.objects.{detail}"));
    }
    if record.details.source.as_str() == "reconnect_orchestrator"
        && !detail.starts_with("event_log.")
    {
        let key = format!("event_log.reconnect_phase.{detail}");
        let translated = i18n.t(&key);
        if translated != key {
            return translated;
        }
    }
    audit_text(i18n, detail)
}
