use gpui::{AnchoredPositionMode, Corner, anchored, deferred};
use oxideterm_gpui_ui::{
    button::ToolbarButtonIconPosition,
    modal::popover_backdrop,
    select::{
        select_anchor_probe, select_option_action, select_option_highlighted, select_overlay_popup,
    },
};

use super::*;

impl AuditView {
    pub fn dismiss_filter(&mut self, cx: &mut Context<Self>) -> bool {
        if self.audit.open_filter.take().is_some() {
            cx.notify();
            true
        } else {
            false
        }
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
                self.audit.search = Zeroizing::new(match index {
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
                });
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
            "down" | "arrowdown" => {
                self.audit.highlighted_filter = (self.audit.highlighted_filter + 1) % count
            }
            "up" | "arrowup" => {
                self.audit.highlighted_filter = (self.audit.highlighted_filter + count - 1) % count
            }
            "enter" | "space" => self.apply_audit_filter(filter, self.audit.highlighted_filter, cx),
            _ => {}
        }
        cx.notify();
        true
    }

    pub(super) fn render_audit_filter(
        &self,
        filter: AuditFilter,
        cx: &Context<Self>,
    ) -> AnyElement {
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
                this.blur_input(cx);
                this.dispatch(AuditWindowRequest::PrepareFilter, cx);
                this.audit.open_filter = open.then_some(filter);
                this.audit.highlighted_filter = selected;
                window.focus(&this.inputs.focus, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let owner = cx.weak_entity();
        let anchors = self.select_anchors.clone();
        select_anchor_probe(filter.anchor(), trigger, move |anchor, window, cx| {
            if anchors.borrow_mut().insert(anchor.id, anchor) != Some(anchor) {
                window.defer(cx, move |_, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        if view.audit.open_filter == Some(filter) {
                            cx.notify();
                        }
                    });
                });
            }
        })
        .into_any_element()
    }

    // Mount outside the scrolling inspector so the popup is not clipped by its list.
    pub(super) fn render_audit_filter_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let filter = self.audit.open_filter?;
        let anchor = self
            .select_anchors
            .borrow()
            .get(&filter.anchor())
            .copied()?;
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
                            .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                                &self.tokens,
                                "audit-filter-popup",
                                div().child(popup),
                                -4.0,
                                gpui::point(0.5, 0.0),
                            )),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }
}
