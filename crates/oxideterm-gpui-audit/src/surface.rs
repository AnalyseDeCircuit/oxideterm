use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, SystemTime},
};

use gpui::{
    AnyElement, App, Context, KeyDownEvent, MouseButton, MouseDownEvent, UniformListScrollHandle,
    Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_audit::{
    AuditCategory, AuditClient, AuditError, AuditHealth, AuditOutcome, AuditQuery, AuditRecord,
    AuditService, AuditSeverity, AuditSource,
};
use oxideterm_gpui_ui::{
    SurfaceKind, SurfaceOptions, SurfacePadding,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    icons::LucideIcon,
    select::{OverlayAnchor, SelectAnchorId},
    semantic_surface,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use zeroize::{Zeroize, Zeroizing};

use super::AuditBindings;
use super::AuditWindowRequest;

#[path = "events.rs"]
mod events;
#[path = "filters.rs"]
mod filters;
#[path = "input.rs"]
mod input;
#[path = "policy.rs"]
mod policy;
#[path = "recordings.rs"]
mod recordings;
#[path = "sessions.rs"]
mod sessions;

use input::AuditInputs;
use recordings::AuditRecordings;
use sessions::AuditSessions;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuditPage {
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
enum AuditFilter {
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

pub struct AuditState {
    client: Option<AuditClient>,
    records: Arc<Vec<AuditRecord>>,
    recordings: AuditRecordings,
    view: AuditPage,
    sessions: AuditSessions,
    selected: Option<AuditRecord>,
    query: AuditQuery,
    cursors: Vec<Option<i64>>,
    next_cursor: Option<i64>,
    error: Option<AuditError>,
    loading: bool,
    open_filter: Option<AuditFilter>,
    highlighted_filter: usize,
    search: Zeroizing<String>,
    search_field: usize,
    time_filter: usize,
    policy: oxideterm_audit::AuditPolicy,
    policy_draft: Option<policy::AuditPolicyDraft>,
    settings_open: bool,
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
    pub fn new(service: Result<AuditService, AuditError>) -> Self {
        let (service, client, error) = match service {
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
            view: AuditPage::Events,
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
            search: Zeroizing::default(),
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

#[derive(Clone)]
pub struct AuditFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub has_background: bool,
}

pub struct AuditView {
    audit: AuditState,
    inputs: AuditInputs,
    pub(crate) bindings: AuditBindings,
    frame: AuditFrame,
    select_anchors: Rc<RefCell<HashMap<SelectAnchorId, OverlayAnchor>>>,
}

impl std::ops::Deref for AuditView {
    type Target = AuditFrame;

    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl gpui::Render for AuditView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(frame) = (self.bindings.frame)(cx) {
            self.frame = frame;
        }
        let content = self.render_audit_content(cx);
        let filter_menu = self.render_audit_filter_menu(cx);
        div()
            .relative()
            .w_full()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .track_focus(&self.inputs.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.handle_audit_filter_key(&event.keystroke.key, cx)
                    || this.handle_audit_input_key(event, window, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .child(
                semantic_surface(
                    &self.tokens,
                    SurfaceOptions::new(SurfaceKind::Inspector)
                        .padding(SurfacePadding::None)
                        .has_background_image(self.has_background),
                )
                .w_full()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(content),
            )
            .children(filter_menu)
    }
}

impl AuditView {
    pub fn new(
        audit: AuditState,
        bindings: AuditBindings,
        frame: AuditFrame,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            audit,
            inputs: AuditInputs::new(cx.focus_handle()),
            bindings,
            frame,
            select_anchors: Default::default(),
        };
        view.start_audit_delivery(cx);
        view
    }

    pub(crate) fn dispatch(&self, request: AuditWindowRequest, cx: &mut App) {
        (self.bindings.dispatch)(request, cx);
    }

    pub fn owns_keyboard(&self, window: &Window) -> bool {
        self.audit_input_focused(window)
            || (self.audit.open_filter.is_some() && self.inputs.focus.is_focused(window))
    }

    pub fn dismiss_transients(&mut self, cx: &mut Context<Self>) {
        self.dismiss_filter(cx);
        self.blur_input(cx);
    }

    pub fn show_page(&mut self, cx: &mut Context<Self>) {
        self.record_audit_view();
        self.refresh_audit(true, cx);
    }

    fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.audit.settings_open = !self.audit.settings_open;
        if !self.audit.settings_open {
            self.blur_input(cx);
        }
        cx.notify();
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        oxideterm_gpui_ui::button::toolbar_action_button(
            &self.tokens,
            label,
            icon,
            options,
            listener,
        )
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        gpui::svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn start_audit_delivery(&mut self, cx: &mut Context<Self>) {
        self.audit.observer = Some(cx.spawn(async move |weak, cx| {
            loop {
                // Keep foreground wakeups on GPUI's scheduler, including its test clock.
                cx.background_executor().timer(Duration::from_secs(1)).await;
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
                            this.dispatch(AuditWindowRequest::StorageUnavailable, cx);
                        }
                        if (this.bindings.is_showing)(cx)
                            && this.audit.cursors.len() == 1
                            && !this.audit.loading
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

    fn refresh_audit(&mut self, reset: bool, cx: &mut Context<Self>) {
        if self.audit.view == AuditPage::Recordings {
            self.refresh_audit_recordings(reset, cx);
            return;
        }
        if self.audit.view == AuditPage::Sessions {
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

    fn apply_audit_search(&mut self, cx: &mut Context<Self>) {
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

    pub fn show_notification_events(
        &mut self,
        connection_id: Option<String>,
        category: Option<AuditCategory>,
        created_at: SystemTime,
        cx: &mut Context<Self>,
    ) {
        self.stop_audit_recording_playback();
        self.dismiss_filter(cx);
        self.audit.view = AuditPage::Events;
        self.audit.policy_draft = None;
        self.blur_input(cx);
        self.audit.query = AuditQuery {
            latest_only: true,
            limit: 100,
            connection_id: connection_id.map(Zeroizing::new),
            category,
            after_ms: created_at
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|time| (time.as_millis().min(i64::MAX as u128) as i64).saturating_sub(60_000)),
            ..Default::default()
        };
        self.audit.search.zeroize();
    }

    fn record_audit_view(&self) {
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
            let Ok(save) = weak.update(cx, |view, cx| (view.bindings.save_export)(name, cx)) else {
                return;
            };
            let Some(path) = save.await else {
                return;
            };
            let result = client.export(query, path, format, include_details).await;
            let _ = weak.update(cx, |this, cx| {
                this.dispatch(
                    AuditWindowRequest::Notice {
                        title: this.i18n.t(if result.is_ok() {
                            "event_log.audit.export_success"
                        } else {
                            "event_log.audit.export_failed"
                        }),
                        description: matches!(result, Err(AuditError::ExportLimit))
                            .then(|| this.i18n.t("event_log.audit.export_limit")),
                        success: result.is_ok(),
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
        let tokens = self.tokens;
        let label = self.i18n.t(label_key);
        oxideterm_gpui_ui::button::icon_button(
            &tokens,
            Self::render_lucide_icon(icon, 15.0, rgb(tokens.ui.text_muted)),
            IconButtonOptions {
                disabled,
                hover_background: Some(rgb(tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
            },
        )
        .id((gpui::ElementId::from("audit-action"), label_key.to_owned()))
        .aria_label(label.clone())
        .flex_none()
        .tooltip(move |_, cx| {
            oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event, window, cx| {
                if !disabled {
                    action(view, event, window, cx);
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_audit_toggle(&self, cx: &Context<Self>) -> AnyElement {
        oxideterm_gpui_ui::checkbox::checkbox(
            &self.tokens,
            "audit-enabled",
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
