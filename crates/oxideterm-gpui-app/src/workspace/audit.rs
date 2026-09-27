use super::*;
mod policy;
mod recordings;
mod sessions;
use oxideterm_audit::{
    AuditCategory, AuditClient, AuditError, AuditHealth, AuditOutcome, AuditQuery, AuditRecord,
    AuditService, AuditSeverity, AuditSource,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
    ToolbarButtonIconPosition, ToolbarButtonOptions,
};
use oxideterm_gpui_ui::select::{
    select_anchor_probe, select_option_action, select_option_highlighted, select_overlay_popup,
};
pub(super) use policy::AuditPolicyInput;
use recordings::AuditRecordings;
use sessions::AuditSessions;

use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuditView {
    Events,
    Sessions,
    Recordings,
}

const AUDIT_ROW_HEIGHT: f32 = 64.0;
const AUDIT_CATEGORIES: [Option<AuditCategory>; 13] = [
    None,
    Some(AuditCategory::Connection),
    Some(AuditCategory::Reconnect),
    Some(AuditCategory::Node),
    Some(AuditCategory::Command),
    Some(AuditCategory::File),
    Some(AuditCategory::Forward),
    Some(AuditCategory::Host),
    Some(AuditCategory::Configuration),
    Some(AuditCategory::Security),
    Some(AuditCategory::Automation),
    Some(AuditCategory::System),
    Some(AuditCategory::Audit),
];
const AUDIT_SEVERITIES: [Option<AuditSeverity>; 4] = [
    None,
    Some(AuditSeverity::Info),
    Some(AuditSeverity::Warning),
    Some(AuditSeverity::Error),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AuditFilter {
    Category,
    Severity,
    Source,
    Outcome,
    Time,
    SearchField,
}

impl AuditFilter {
    fn anchor(self) -> SelectAnchorId {
        match self {
            Self::Category => SelectAnchorId::AuditCategory,
            Self::Severity => SelectAnchorId::AuditSeverity,
            Self::Source => SelectAnchorId::AuditSource,
            Self::Outcome => SelectAnchorId::AuditOutcome,
            Self::Time => SelectAnchorId::AuditTime,
            Self::SearchField => SelectAnchorId::AuditSearchField,
        }
    }
}

pub(super) struct AuditState {
    client: Option<AuditClient>,
    records: Arc<Vec<AuditRecord>>,
    recordings: AuditRecordings,
    view: AuditView,
    sessions: AuditSessions,
    selected: Option<AuditRecord>,
    query: AuditQuery,
    cursors: Vec<Option<i64>>,
    next_cursor: Option<i64>,
    error: Option<AuditError>,
    loading: bool,
    pub(super) open_filter: Option<AuditFilter>,
    highlighted_filter: usize,
    pub(super) search: String,
    search_field: usize,
    time_filter: usize,
    policy: oxideterm_audit::AuditPolicy,
    policy_draft: Option<policy::AuditPolicyDraft>,
    pub(super) settings_open: bool,
    settings_task: Option<gpui::Task<()>>,
    task: Option<gpui::Task<()>>,
    generation: u64,
    observer: Option<gpui::Task<()>>,
    last_health: Option<AuditHealth>,
    scroll: UniformListScrollHandle,
    // Cancel page tasks and pending view operations before stopping collection.
    _registration: Option<oxideterm_audit::AuditRegistration>,
    _service: Option<AuditService>,
}

impl AuditState {
    pub(super) fn new(path: PathBuf) -> Self {
        let (service, client, error) = match AuditService::start(path) {
            Ok(service) => {
                let client = service.client().with_source(AuditSource::User);
                (Some(service), Some(client), None)
            }
            Err(error) => (None, None, Some(error)),
        };
        let registration = client.as_ref().map(|client| {
            oxideterm_audit::AuditContext::new(
                client.clone(),
                oxideterm_audit::AuditSource::Application,
            )
            .install()
        });
        Self {
            _registration: registration,
            _service: service,
            client,
            error,
            records: Arc::new(Vec::new()),
            recordings: AuditRecordings::default(),
            view: AuditView::Events,
            sessions: AuditSessions::default(),
            selected: None,
            query: AuditQuery {
                latest_only: true,
                limit: 100,
                ..Default::default()
            },
            cursors: vec![None],
            next_cursor: None,
            loading: false,
            open_filter: None,
            highlighted_filter: 0,
            search: String::new(),
            search_field: 0,
            time_filter: 0,
            policy: oxideterm_audit::AuditPolicy::default(),
            policy_draft: None,
            settings_open: false,
            settings_task: None,
            task: None,
            generation: 0,
            observer: None,
            last_health: None,
            scroll: UniformListScrollHandle::new(),
        }
    }
}

impl WorkspaceApp {
    pub(super) fn start_audit_delivery(&mut self, cx: &mut Context<Self>) {
        self.audit.observer = Some(cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_secs(1)).await;
                if weak
                    .update(cx, |this, cx| {
                        let Some(client) = &this.audit.client else {
                            return;
                        };
                        let health = client.health();
                        if this.audit.last_health == Some(health) {
                            return;
                        }
                        let previous = this.audit.last_health.replace(health);
                        if health.error.is_some()
                            && previous.is_none_or(|last| last.error != health.error)
                        {
                            this.push_notification_entry(
                                WorkspaceNotificationKind::Security,
                                WorkspaceNotificationSeverity::Error,
                                this.i18n.t("event_log.audit.center_title"),
                                Some(this.i18n.t("event_log.audit.unavailable")),
                                WorkspaceNotificationScope::Global,
                                Some("audit-storage-failed".into()),
                            );
                        }
                        if this.notification_center.active_view == WorkspaceActivityView::EventLog
                            && this.audit.cursors.len() == 1
                            && !this.audit.loading
                            && this
                                .active_content_tab(cx)
                                .is_some_and(|tab| tab.kind == TabKind::NotificationCenter)
                        {
                            this.refresh_audit(false, cx);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(super) fn refresh_audit(&mut self, reset: bool, cx: &mut Context<Self>) {
        if self.audit.view == AuditView::Recordings {
            self.refresh_audit_recordings(reset, cx);
            return;
        }
        if self.audit.view == AuditView::Sessions {
            self.refresh_audit_sessions(reset, cx);
            return;
        }
        if reset {
            self.audit.cursors = vec![None];
        }
        let Some(client) = self.audit.client.clone() else {
            return;
        };
        self.audit.generation += 1;
        let generation = self.audit.generation;
        let mut query = self.audit.query.clone();
        query.before_sequence = self.audit.cursors.last().copied().flatten();
        self.audit.loading = true;
        if reset {
            self.audit.selected = None;
        }
        self.audit.error = None;
        self.audit.task = Some(cx.spawn(async move |weak, cx| {
            let policy = client.policy().await;
            let result = client.query(query).await;
            let _ = weak.update(cx, |this, cx| {
                if this.audit.generation != generation {
                    return;
                }
                this.audit.loading = false;
                if let Ok(policy) = policy {
                    this.audit.policy = policy;
                }
                match result {
                    Ok(page) => {
                        this.audit.records = Arc::new(page.records);
                        this.audit.next_cursor = page.next_cursor;
                        if reset {
                            this.audit.scroll = UniformListScrollHandle::new();
                        }
                    }
                    Err(error) => {
                        this.audit.records = Arc::new(Vec::new());
                        this.audit.next_cursor = None;
                        this.audit.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn apply_audit_search(&mut self, cx: &mut Context<Self>) {
        let value = (!self.audit.search.trim().is_empty())
            .then(|| Zeroizing::new(self.audit.search.trim().to_string()));
        match self.audit.search_field {
            1 => self.audit.query.protocol = value.map(|s| s.to_string()),
            2 => self.audit.query.connection_id = value,
            3 => self.audit.query.remote_account = value,
            4 => self.audit.query.local_account = value,
            _ => self.audit.query.search = value,
        }
        self.refresh_audit(true, cx);
    }

    fn save_audit_policy(&mut self, policy: oxideterm_audit::AuditPolicy, cx: &mut Context<Self>) {
        if self.audit.settings_task.is_some() {
            return;
        }
        let Some(client) = self.audit.client.clone() else {
            return;
        };
        self.audit.open_filter = None;
        self.audit.settings_task = Some(cx.spawn(async move |weak, cx| {
            let result = client.set_policy(policy).await;
            let _ = weak.update(cx, |this, cx| {
                this.audit.settings_task = None;
                match result {
                    Ok(()) => {
                        this.audit.policy = policy;
                        if this
                            .audit
                            .policy_draft
                            .as_ref()
                            .is_some_and(|draft| draft.apply(policy) == Some(policy))
                        {
                            this.audit.policy_draft = None;
                        }
                        this.refresh_audit(true, cx);
                    }
                    Err(error) => {
                        this.audit.error = Some(error);
                        cx.notify();
                    }
                }
            });
        }));
        cx.notify();
    }

    fn clear_audit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.audit.client.clone() else {
            return;
        };
        let prompt = window.prompt(
            gpui::PromptLevel::Warning,
            &self.i18n.t("event_log.clear"),
            Some(&self.i18n.t("event_log.audit.clear_confirm")),
            &[
                self.i18n.t("common.actions.cancel").as_str(),
                self.i18n.t("event_log.clear").as_str(),
            ],
            cx,
        );
        self.audit.settings_task = Some(cx.spawn(async move |weak, cx| {
            if !matches!(prompt.await, Ok(1)) {
                let _ = weak.update(cx, |this, cx| {
                    this.audit.settings_task = None;
                    cx.notify();
                });
                return;
            }
            let before = chrono::Utc::now().timestamp_millis();
            let result = client.clear_before(before).await;
            let _ = weak.update(cx, |this, cx| {
                this.audit.settings_task = None;
                if let Err(error) = result {
                    this.audit.error = Some(error);
                } else {
                    this.refresh_audit(true, cx);
                }
                cx.notify();
            });
        }));
    }

    pub(in crate::workspace) fn open_notification_audit(
        &mut self,
        entry: &WorkspaceNotificationEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let connection_id = match &entry.scope {
            WorkspaceNotificationScope::Connection(id) => Some(id.clone()),
            WorkspaceNotificationScope::Node(id) => {
                self.node_router.connection_id_for_node(&NodeId(id.clone()))
            }
            WorkspaceNotificationScope::Global => None,
        };
        self.stop_audit_recording_playback();
        self.audit.view = AuditView::Events;
        self.audit.policy_draft = None;
        self.clear_ime_selection();
        self.audit.query = AuditQuery {
            latest_only: true,
            limit: 100,
            connection_id: connection_id.map(Zeroizing::new),
            category: if entry.dedupe_key.as_deref() == Some("audit-storage-failed") {
                Some(AuditCategory::Audit)
            } else if entry.kind == WorkspaceNotificationKind::Security {
                Some(AuditCategory::Security)
            } else {
                None
            },
            after_ms: entry
                .created_at
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|time| (time.as_millis().min(i64::MAX as u128) as i64).saturating_sub(60_000)),
            ..Default::default()
        };
        self.audit.search.clear();
        self.notification_center.active_view = WorkspaceActivityView::EventLog;
        self.open_notification_center_tab(window, cx);
    }

    pub(super) fn record_audit_view(&self) {
        if let Some(client) = &self.audit.client {
            oxideterm_audit::AuditContext::new(client.clone(), AuditSource::User).observe(
                AuditCategory::Audit,
                "audit_view",
                None,
                AuditOutcome::Succeeded,
                oxideterm_audit::AuditEvidence::Lifecycle,
                oxideterm_audit::AuditAuthorization::NotRequired,
            );
        }
    }

    fn export_audit(
        &mut self,
        format: oxideterm_audit::AuditExportFormat,
        include_details: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.audit.client.clone() else {
            return;
        };
        let mut query = self.audit.query.clone();
        query.before_sequence = None;
        let name = match format {
            oxideterm_audit::AuditExportFormat::Json => "oxideterm-audit.json",
            oxideterm_audit::AuditExportFormat::Csv => "oxideterm-audit.csv",
        };
        let prompt = if include_details {
            let sample = Zeroizing::new(
                self.audit
                    .selected
                    .as_ref()
                    .or_else(|| self.audit.records.first())
                    .map(|record| {
                        format!(
                            "{}\n{}\n{}",
                            audit_text(&self.i18n, &record.details.title),
                            record
                                .details
                                .target
                                .as_deref()
                                .map(|v| v.as_str())
                                .unwrap_or_default(),
                            record
                                .details
                                .detail
                                .as_deref()
                                .map(|v| v.as_str())
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_default(),
            );
            let sample = oxideterm_audit::redact(&sample);
            let description = self
                .i18n
                .t("event_log.audit.export_details_confirm")
                .replace(
                    "{{preview}}",
                    &sample.chars().take(1000).collect::<String>(),
                );
            Some(window.prompt(
                gpui::PromptLevel::Warning,
                &self.i18n.t("event_log.audit.export_details"),
                Some(&description),
                &[
                    self.i18n.t("common.actions.cancel").as_str(),
                    self.i18n.t("event_log.audit.export_details").as_str(),
                ],
                cx,
            ))
        } else {
            None
        };
        cx.spawn(async move |weak, cx| {
            if let Some(prompt) = prompt {
                if !matches!(prompt.await, Ok(1)) {
                    return;
                }
            }
            let Ok(receiver) = weak.update(cx, |_, cx| {
                cx.prompt_for_new_path(
                    &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                    Some(name),
                )
            }) else {
                return;
            };
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let result = client.export(query, path, format, include_details).await;
            let _ = weak.update(cx, |this, cx| {
                this.push_workspace_notice(
                    TerminalNotice {
                        title: this.i18n.t(if result.is_ok() {
                            "event_log.audit.export_success"
                        } else {
                            "event_log.audit.export_failed"
                        }),
                        description: matches!(result, Err(AuditError::ExportLimit))
                            .then(|| this.i18n.t("event_log.audit.export_limit")),
                        status_text: None,
                        progress: None,
                        variant: if result.is_ok() {
                            TerminalNoticeVariant::Success
                        } else {
                            TerminalNoticeVariant::Error
                        },
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    fn audit_icon_button(
        &self,
        icon: LucideIcon,
        label_key: &str,
        disabled: bool,
        action: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.workspace_tooltip_icon_button(
            icon,
            15.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                disabled,
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
            },
            self.i18n.t(label_key),
            "audit-action",
            true,
            cx.listener(move |this, event, window, cx| {
                action(this, event, window, cx);
                cx.stop_propagation();
            }),
            cx.entity(),
        )
    }

    fn audit_filter_options(&self, filter: AuditFilter) -> Vec<(String, bool)> {
        match filter {
            AuditFilter::SearchField => [
                "event_log.audit.content",
                "event_log.audit.protocol",
                "event_log.audit.connection",
                "event_log.audit.remote_account",
                "event_log.audit.actor",
            ]
            .into_iter()
            .enumerate()
            .map(|(index, key)| (self.i18n.t(key), self.audit.search_field == index))
            .collect(),
            AuditFilter::Source => std::iter::once(None)
                .chain(AuditSource::ALL.into_iter().map(Some))
                .map(|value| {
                    (
                        value
                            .map(|v| self.i18n.t(&format!("event_log.source.{}", v.key())))
                            .unwrap_or_else(|| self.i18n.t("event_log.all")),
                        self.audit.query.source == value,
                    )
                })
                .collect(),
            AuditFilter::Outcome => std::iter::once(None)
                .chain(AuditOutcome::ALL.into_iter().map(Some))
                .map(|value| {
                    (
                        value
                            .map(|v| self.i18n.t(&format!("event_log.outcome.{}", v.key())))
                            .unwrap_or_else(|| self.i18n.t("event_log.all")),
                        self.audit.query.outcome == value,
                    )
                })
                .collect(),
            AuditFilter::Time => [
                "event_log.all",
                "event_log.audit.today",
                "event_log.audit.week",
                "event_log.audit.month",
            ]
            .into_iter()
            .enumerate()
            .map(|(index, key)| (self.i18n.t(key), self.audit.time_filter == index))
            .collect(),
            AuditFilter::Category => AUDIT_CATEGORIES
                .into_iter()
                .map(|value| {
                    (
                        value
                            .map(|v| self.i18n.t(&format!("event_log.category.{}", v.key())))
                            .unwrap_or_else(|| self.i18n.t("event_log.all")),
                        self.audit.query.category == value,
                    )
                })
                .collect(),
            AuditFilter::Severity => AUDIT_SEVERITIES
                .into_iter()
                .map(|value| {
                    (
                        value
                            .map(|v| self.i18n.t(&format!("event_log.severity.{}", v.key())))
                            .unwrap_or_else(|| self.i18n.t("event_log.all")),
                        self.audit.query.severity == value,
                    )
                })
                .collect(),
        }
    }

    fn apply_audit_filter(&mut self, filter: AuditFilter, index: usize, cx: &mut Context<Self>) {
        match filter {
            AuditFilter::SearchField => {
                self.audit.search_field = index;
                self.audit.search = match index {
                    1 => self.audit.query.protocol.clone().unwrap_or_default(),
                    2 => self
                        .audit
                        .query
                        .connection_id
                        .as_deref()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    3 => self
                        .audit
                        .query
                        .remote_account
                        .as_deref()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    4 => self
                        .audit
                        .query
                        .local_account
                        .as_deref()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    _ => self
                        .audit
                        .query
                        .search
                        .as_deref()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                };
                self.audit.open_filter = None;
                cx.notify();
                return;
            }
            AuditFilter::Source => {
                self.audit.query.source = index.checked_sub(1).map(|i| AuditSource::ALL[i])
            }
            AuditFilter::Outcome => {
                self.audit.query.outcome = index.checked_sub(1).map(|i| AuditOutcome::ALL[i])
            }
            AuditFilter::Time => {
                self.audit.time_filter = index;
                self.audit.query.after_ms = match index {
                    1 => Some(chrono::Utc::now().timestamp_millis() - 86_400_000),
                    2 => Some(chrono::Utc::now().timestamp_millis() - 7 * 86_400_000),
                    3 => Some(chrono::Utc::now().timestamp_millis() - 30 * 86_400_000),
                    _ => None,
                };
            }
            AuditFilter::Category => self.audit.query.category = AUDIT_CATEGORIES[index],
            AuditFilter::Severity => self.audit.query.severity = AUDIT_SEVERITIES[index],
        }
        self.audit.open_filter = None;
        self.refresh_audit(true, cx);
    }

    pub(super) fn handle_audit_filter_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some(filter) = self.audit.open_filter else {
            return false;
        };
        let count = self.audit_filter_options(filter).len();
        match key {
            "escape" => self.audit.open_filter = None,
            "down" => self.audit.highlighted_filter = (self.audit.highlighted_filter + 1) % count,
            "up" => {
                self.audit.highlighted_filter = (self.audit.highlighted_filter + count - 1) % count
            }
            "enter" | "space" => self.apply_audit_filter(filter, self.audit.highlighted_filter, cx),
            _ => {}
        }
        cx.notify();
        true
    }

    fn render_audit_filter(&self, filter: AuditFilter, cx: &Context<Self>) -> AnyElement {
        let options = self.audit_filter_options(filter);
        let selected = options
            .iter()
            .position(|(_, selected)| *selected)
            .unwrap_or(0);
        let label_key = match filter {
            AuditFilter::Category => "event_log.audit.category",
            AuditFilter::Severity => "event_log.audit.severity",
            AuditFilter::Source => "event_log.audit.source",
            AuditFilter::Outcome => "event_log.audit.outcome",
            AuditFilter::Time => "event_log.audit.time",
            AuditFilter::SearchField => "event_log.audit.search_field",
        };
        let trigger = self.workspace_toolbar_action_button(
            format!("{} · {}", self.i18n.t(label_key), options[selected].0),
            Some(Self::render_lucide_icon(
                LucideIcon::ChevronDown,
                14.0,
                rgb(self.tokens.ui.text_muted),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                icon_position: ToolbarButtonIconPosition::Trailing,
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..Default::default()
            },
            cx.listener(move |this, _, window, cx| {
                let open = this.audit.open_filter != Some(filter);
                this.prepare_modal_interaction_boundary(cx);
                this.audit.open_filter = open.then_some(filter);
                this.audit.highlighted_filter = selected;
                window.focus(&this.focus_handle, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let workspace = cx.entity();
        select_anchor_probe(filter.anchor(), trigger, move |anchor, window, cx| {
            window.defer(cx, move |_, cx| {
                workspace.update(cx, |this, cx| this.update_select_anchor(anchor, cx));
            });
        })
        .into_any_element()
    }

    // Mounted outside the scrolling inspector, including in detached center tabs.
    pub(super) fn render_audit_filter_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.notification_center.active_view != WorkspaceActivityView::EventLog {
            return None;
        }
        let filter = self.audit.open_filter?;
        let anchor = self.select_anchors.get(&filter.anchor())?;
        let mut popup = select_overlay_popup(&self.tokens, 200.0);
        for (index, (label, selected)) in self.audit_filter_options(filter).into_iter().enumerate()
        {
            popup = popup.child(select_option_action(
                select_option_highlighted(
                    &self.tokens,
                    label,
                    selected,
                    index == self.audit.highlighted_filter,
                ),
                false,
                false,
                cx.listener(move |this, _, _, cx| {
                    this.apply_audit_filter(filter, index, cx);
                    cx.stop_propagation();
                }),
            ));
        }
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.audit.open_filter = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.audit.open_filter = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(Corner::TopLeft)
                            .position(anchor.bounds.bottom_left())
                            .offset(gpui::point(px(0.0), px(4.0)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(popup),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    fn render_audit_toggle(&self, cx: &Context<Self>) -> AnyElement {
        oxideterm_gpui_ui::checkbox::checkbox(
            &self.tokens,
            self.i18n.t(if self.audit.policy.enabled {
                "event_log.audit.enabled_status"
            } else {
                "event_log.audit.enabled"
            }),
            self.audit.policy.enabled,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                if this.audit.settings_task.is_none()
                    && !this.audit.loading
                    && this.audit.client.is_some()
                {
                    let mut policy = this.audit.policy;
                    policy.enabled = !policy.enabled;
                    this.save_audit_policy(policy, cx);
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_audit_disabled_hint(&self) -> Option<AnyElement> {
        (!self.audit.policy.enabled).then(|| {
            div()
                .flex_none()
                .px_3()
                .py_2()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("event_log.audit.disabled_hint"))
                .into_any_element()
        })
    }

    pub(super) fn render_audit_content(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.audit.view == AuditView::Recordings {
            return self.render_audit_recordings(cx);
        }
        if self.audit.view == AuditView::Sessions {
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
                    .child(self.audit_view_button(AuditView::Events, cx))
                    .child(self.audit_view_button(AuditView::Sessions, cx))
                    .child(self.audit_view_button(AuditView::Recordings, cx))
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
                            this.audit.settings_open = !this.audit.settings_open;
                            cx.notify();
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
                            this.audit.search.clear();
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
                            .child({
                                let target = WorkspaceImeTarget::AuditSearch;
                                let input = oxideterm_gpui_ui::text_input::text_input(
                                    &self.tokens,
                                    oxideterm_gpui_ui::text_input::TextInputView {
                                        value: &self.audit.search,
                                        placeholder: self.i18n.t("event_log.audit.search"),
                                        focused: self.active_ime_target(cx) == Some(target),
                                        caret_visible: self.input_caret.visible(),
                                        secret: false,
                                        selected_all: false,
                                        selected_range: self
                                            .ime_selected_range_for_target(target, cx),
                                        marked_text: self.marked_text_for_target(target, cx),
                                    },
                                )
                                .flex_1()
                                .min_w_0()
                                .h(px(28.0));
                                self.text_input_with_workspace_ime(
                                    target,
                                    input,
                                    |this, cx| {
                                        this.audit.open_filter = None;
                                        this.selected_ime_target =
                                            Some(WorkspaceImeTarget::AuditSearch);
                                        this.show_active_input_caret(cx);
                                    },
                                    cx,
                                )
                            }),
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
                                this.audit.search.clear();
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
                                this.audit.search.clear();
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

fn audit_time(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

fn audit_text(i18n: &I18n, raw: &str) -> String {
    if !raw.starts_with("event_log.") {
        return raw.to_string();
    }
    let (key, count) = raw
        .split_once(':')
        .map(|(key, value)| (key, value.parse::<usize>().ok()))
        .unwrap_or((raw, None));
    let translated = i18n.t(key);
    match count {
        Some(count) => translated.replace("{{count}}", &count.to_string()),
        None => translated,
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
