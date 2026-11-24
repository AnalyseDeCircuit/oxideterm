//! Window transients share one deadline, including the exit animation after dismissal.
use gpui::{
    AnchoredPositionMode, Animation, AnimationExt, AnyElement, App, Context, Corner, MouseButton,
    Task, Window, anchored, deferred, prelude::*, px,
};
use oxideterm_gpui_ui::{
    motion::ExitPhase,
    toast::{ToastVariant, ToastView, toast_close},
    toaster::toaster,
    tooltip::tooltip_surface,
};
use oxideterm_theme::ThemeTokens;
use std::{collections::HashMap, rc::Rc, time::Duration};
use web_time::Instant;
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(300);
pub const TOOLTIP_EXIT_DURATION: Duration = Duration::from_millis(90);
pub const TOOLTIP_REENTRY_WINDOW: Duration = Duration::from_millis(250);
pub const TOOLTIP_ENTER_OFFSET_Y: f32 = 3.0;
pub const TOOLTIP_SWITCH_ROW_TOLERANCE: f32 = 12.0;
pub const TOOLTIP_SWITCH_MAX_DISTANCE: f32 = 96.0;
#[derive(Clone, Debug)]
pub struct OverlayToast<N, A> {
    id: u64,
    pub notice: N,
    pub action: Option<A>,
    expires_at: Instant,
    remove_at: Option<Instant>,
    presence: oxideterm_gpui_ui::motion::ExitPresence,
}
#[derive(Clone, Debug)]
pub struct WorkspaceTooltip {
    generation: u64,
    remove_at: Option<Instant>,
    fade_from: f32,
    pub opacity: std::rc::Rc<std::cell::Cell<f32>>,
    position_from: gpui::Point<f32>,
    pub position: std::rc::Rc<std::cell::Cell<gpui::Point<f32>>>,
    pub switching: bool,
    pub id: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
}

impl WorkspaceTooltip {
    pub fn new(id: String, label: String, position: gpui::Point<f32>, generation: u64) -> Self {
        let position_from = gpui::point(position.x, position.y + TOOLTIP_ENTER_OFFSET_Y);
        Self {
            generation,
            remove_at: None,
            fade_from: 0.0,
            opacity: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            position_from,
            position: std::rc::Rc::new(std::cell::Cell::new(position_from)),
            switching: false,
            id,
            label,
            x: position.x,
            y: position.y,
        }
    }
}

#[derive(Clone, Debug)]
struct WorkspaceTooltipPending {
    pub id: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
    generation: u64,
    show_at: Instant,
}
pub struct TransientOverlays<N, A> {
    pub control_exit_duration: Duration,
    next_toast_id: u64,
    pub standard_toasts: Vec<OverlayToast<N, A>>,
    plugin_progress_toasts: HashMap<String, OverlayToast<N, A>>,
    pub tooltip: Option<WorkspaceTooltip>,
    tooltip_pending: Option<WorkspaceTooltipPending>,
    tooltip_generation: u64,
    tooltip_warm_until: Option<Instant>,
}
impl<N, A> TransientOverlays<N, A> {
    pub fn new(control_exit_duration: Duration) -> Self {
        Self {
            control_exit_duration,
            next_toast_id: 1,
            standard_toasts: Vec::new(),
            plugin_progress_toasts: HashMap::new(),
            tooltip: None,
            tooltip_pending: None,
            tooltip_generation: 0,
            tooltip_warm_until: None,
        }
    }
    pub fn set_exit_duration(&mut self, duration: Duration) -> bool {
        self.control_exit_duration = duration;
        if duration.is_zero() && self.tooltip.as_ref().is_some_and(|t| t.remove_at.is_some()) {
            self.tooltip = None;
            true
        } else {
            false
        }
    }
    pub fn push_notice(&mut self, notice: N, ttl: Duration, action: Option<A>, now: Instant) {
        let id = self.next_toast_id;
        self.next_toast_id = self.next_toast_id.wrapping_add(1).max(1);
        self.standard_toasts.push(OverlayToast {
            id,
            notice,
            action,
            expires_at: now + ttl,
            remove_at: None,
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
        });
    }

    pub fn upsert_plugin_progress(&mut self, key: String, notice: N, ttl: Duration, now: Instant) {
        let expires_at = now + ttl;
        if let Some(toast) = self.plugin_progress_toasts.get_mut(&key) {
            toast.notice = notice;
            toast.expires_at = expires_at;
            toast.remove_at = None;
            if toast.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
                toast.presence.reopen();
            }
            return;
        }
        let id = self.next_toast_id;
        self.next_toast_id = self.next_toast_id.wrapping_add(1).max(1);
        self.plugin_progress_toasts.insert(
            key,
            OverlayToast {
                id,
                notice,
                action: None,
                expires_at,
                remove_at: None,
                presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            },
        );
    }

    pub fn queue_tooltip(
        &mut self,
        id: String,
        label: String,
        x: f32,
        y: f32,
        now: Instant,
    ) -> bool {
        if let Some(tooltip) = self.tooltip.as_mut()
            && tooltip.id == id
            && tooltip.remove_at.is_none_or(|deadline| now < deadline)
        {
            let resumed = tooltip.remove_at.take().is_some();
            if resumed {
                self.tooltip_generation = self.tooltip_generation.wrapping_add(1);
                tooltip.generation = self.tooltip_generation;
                tooltip.fade_from = tooltip.opacity.get();
                tooltip.position_from = tooltip.position.get();
                tooltip.switching = true;
                tooltip.x = x;
                tooltip.y = y;
            }
            // A visible hint stays still while the pointer moves within its trigger.
            let changed = resumed || tooltip.label != label;
            tooltip.label = label;
            return changed;
        }
        if let Some(pending) = self.tooltip_pending.as_mut()
            && pending.id == id
        {
            pending.label = label;
            pending.x = x;
            pending.y = y;
            return false;
        }
        self.tooltip_generation = self.tooltip_generation.wrapping_add(1);
        let previous = self
            .tooltip
            .take()
            .filter(|tooltip| tooltip.remove_at.is_none_or(|deadline| now < deadline));
        let warm = previous.is_some() || self.tooltip_warm_until.is_some_and(|until| now < until);
        if warm {
            self.tooltip_pending = None;
            let mut tooltip =
                WorkspaceTooltip::new(id, label, gpui::point(x, y), self.tooltip_generation);
            if let Some(previous) = previous
                && (previous.y - y).abs() <= TOOLTIP_SWITCH_ROW_TOLERANCE
                && (previous.x - x).abs() <= TOOLTIP_SWITCH_MAX_DISTANCE
            {
                tooltip.fade_from = previous.opacity.get();
                tooltip.position_from = previous.position.get();
                tooltip.position.set(tooltip.position_from);
                tooltip.switching = true;
            }
            self.tooltip = Some(tooltip);
            return true;
        }
        self.tooltip_pending = Some(WorkspaceTooltipPending {
            id,
            label,
            x,
            y,
            generation: self.tooltip_generation,
            show_at: now + TOOLTIP_DELAY,
        });
        true
    }

    pub fn clear_tooltip(&mut self, id: &str, now: Instant) -> bool {
        let mut changed = false;
        if self
            .tooltip_pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            self.tooltip_pending = None;
            self.tooltip_generation = self.tooltip_generation.wrapping_add(1);
            changed = true;
        }
        if let Some(tooltip) = self.tooltip.as_mut()
            && tooltip.id == id
            && tooltip.remove_at.is_none()
        {
            self.tooltip_warm_until = Some(now + TOOLTIP_REENTRY_WINDOW);
            if self.control_exit_duration.is_zero() {
                self.tooltip = None;
            } else {
                self.tooltip_generation = self.tooltip_generation.wrapping_add(1);
                tooltip.generation = self.tooltip_generation;
                tooltip.fade_from = tooltip.opacity.get();
                tooltip.position_from = tooltip.position.get();
                tooltip.remove_at =
                    Some(now + self.control_exit_duration.min(TOOLTIP_EXIT_DURATION));
            }
            changed = true;
        }
        changed
    }

    pub fn clear_all_tooltips(&mut self) -> bool {
        self.tooltip_warm_until = None;
        let visible = self.tooltip.take().is_some();
        let pending = self.tooltip_pending.take().is_some();
        let changed = visible || pending;
        if changed {
            self.tooltip_generation = self.tooltip_generation.wrapping_add(1);
        }
        changed
    }

    pub fn begin_standard_exit(&mut self, toast_id: u64, now: Instant) -> bool {
        let Some(toast) = self
            .standard_toasts
            .iter_mut()
            .find(|toast| toast.id == toast_id)
        else {
            return false;
        };
        if toast.presence.begin_exit().is_none() {
            return false;
        }
        if self.control_exit_duration.is_zero() {
            self.standard_toasts.retain(|toast| toast.id != toast_id);
        } else {
            toast.remove_at = Some(now + self.control_exit_duration);
        }
        true
    }

    pub fn dismiss_plugin_progress(&mut self, key: &str, now: Instant) -> bool {
        let Some(toast) = self.plugin_progress_toasts.get_mut(key) else {
            return false;
        };
        if toast.presence.begin_exit().is_none() {
            return false;
        }
        if self.control_exit_duration.is_zero() {
            self.plugin_progress_toasts.remove(key);
        } else {
            toast.remove_at = Some(now + self.control_exit_duration);
        }
        true
    }

    pub fn process_due_deadlines(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if self
            .tooltip
            .as_ref()
            .is_some_and(|tooltip| tooltip.remove_at.is_some_and(|deadline| deadline <= now))
        {
            self.tooltip = None;
            changed = true;
        }
        if self
            .tooltip_pending
            .as_ref()
            .is_some_and(|pending| pending.show_at <= now)
            && let Some(pending) = self.tooltip_pending.take()
            && pending.generation == self.tooltip_generation
        {
            self.tooltip = Some(WorkspaceTooltip::new(
                pending.id,
                pending.label,
                gpui::point(pending.x, pending.y),
                pending.generation,
            ));
            changed = true;
        }
        let standard_expired = self
            .standard_toasts
            .iter()
            .filter(|toast| {
                toast.expires_at <= now
                    && toast.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible
            })
            .map(|toast| toast.id)
            .collect::<Vec<_>>();
        for toast_id in standard_expired {
            changed |= self.begin_standard_exit(toast_id, now);
        }
        let standard_count = self.standard_toasts.len();
        self.standard_toasts
            .retain(|toast| !toast.remove_at.is_some_and(|remove_at| remove_at <= now));
        changed |= self.standard_toasts.len() != standard_count;

        let plugin_expired = self
            .plugin_progress_toasts
            .iter()
            .filter(|(_, toast)| {
                toast.expires_at <= now
                    && toast.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in plugin_expired {
            changed |= self.dismiss_plugin_progress(&key, now);
        }
        let plugin_remove = self
            .plugin_progress_toasts
            .iter()
            .filter(|(_, toast)| toast.remove_at.is_some_and(|remove_at| remove_at <= now))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in plugin_remove {
            self.plugin_progress_toasts.remove(&key);
            changed = true;
        }

        changed
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        let mut next = self.tooltip_pending.as_ref().map(|pending| pending.show_at);
        next = min_deadline(
            next,
            self.tooltip.as_ref().and_then(|tooltip| tooltip.remove_at),
        );
        for toast in &self.standard_toasts {
            next = min_deadline(
                next,
                if toast.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible {
                    Some(toast.expires_at)
                } else {
                    toast.remove_at
                },
            );
        }
        for toast in self.plugin_progress_toasts.values() {
            next = min_deadline(
                next,
                if toast.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible {
                    Some(toast.expires_at)
                } else {
                    toast.remove_at
                },
            );
        }
        next
    }
    pub fn render_tooltip(&self, tokens: &ThemeTokens) -> Option<AnyElement> {
        self.tooltip.clone().map(|tip| {
            render_tooltip(
                tokens,
                tip,
                self.control_exit_duration.min(TOOLTIP_EXIT_DURATION),
            )
        })
    }
    pub fn render_toasts(
        &self,
        tokens: &ThemeTokens,
        present: impl Fn(&N) -> ToastContent,
        action_label: impl Fn(&A) -> String,
        request: Rc<dyn Fn(ToastRequest, &mut Window, &mut App)>,
    ) -> gpui::Div {
        let render = |toast: &OverlayToast<N, A>, key: Option<&String>| {
            let id = toast.id;
            let ToastContent {
                title,
                description,
                status_text,
                progress,
                variant,
            } = present(&toast.notice);
            let close_request = key.map_or(ToastRequest::Dismiss(id), |k| {
                ToastRequest::DismissProgress(k.clone())
            });
            let dismiss = request.clone();
            ToastView {
                id: key.map_or_else(|| format!("workspace-{id}"), |key| format!("plugin-{key}")),
                phase: toast.presence.phase(),
                title,
                description,
                status_text,
                progress,
                variant,
                actions: toast.action.as_ref().map(|action| {
                    let invoke = request.clone();
                    oxideterm_gpui_ui::button::button_with(
                        tokens,
                        action_label(action),
                        oxideterm_gpui_ui::button::ButtonOptions {
                            variant: oxideterm_gpui_ui::button::ButtonVariant::Outline,
                            size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                            ..Default::default()
                        },
                    )
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        invoke(ToastRequest::Invoke(id), window, cx);
                        cx.stop_propagation()
                    })
                    .into_any_element()
                }),
                close: Some(
                    toast_close(tokens)
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            dismiss(close_request.clone(), window, cx);
                            cx.stop_propagation()
                        })
                        .into_any_element(),
                ),
            }
        };
        toaster(
            tokens,
            self.standard_toasts.iter().map(|t| render(t, None)).chain(
                self.plugin_progress_toasts
                    .iter()
                    .map(|(k, t)| render(t, Some(k))),
            ),
        )
    }
    pub fn action(&self, id: u64) -> Option<&A> {
        self.standard_toasts
            .iter()
            .find(|t| t.id == id && t.presence.phase() == ExitPhase::Visible)?
            .action
            .as_ref()
    }
}
#[derive(Clone)]
pub enum ToastRequest {
    Invoke(u64),
    Dismiss(u64),
    DismissProgress(String),
}
pub struct ToastContent {
    pub title: String,
    pub description: Option<String>,
    pub status_text: Option<String>,
    pub progress: Option<f32>,
    pub variant: ToastVariant,
}

/// Drop or replace the returned task when the owning window changes its earliest deadline.
pub fn at_deadline<T: 'static>(
    deadline: Instant,
    cx: &mut Context<T>,
    due: impl FnOnce(&mut T, Instant, &mut Context<T>) + 'static,
) -> Task<()> {
    let timer = cx
        .background_executor()
        .timer(deadline.saturating_duration_since(cx.background_executor().now()));
    cx.spawn(async move |owner, cx| {
        timer.await;
        let _ = owner.update(cx, |owner, cx| {
            due(owner, cx.background_executor().now(), cx)
        });
    })
}
pub fn min_deadline(current: Option<Instant>, candidate: Option<Instant>) -> Option<Instant> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (Some(current), None) => Some(current),
        (None, candidate) => candidate,
    }
}
pub fn render_tooltip(
    tokens: &ThemeTokens,
    tooltip: WorkspaceTooltip,
    exit_duration: Duration,
) -> AnyElement {
    let exiting = tooltip.remove_at.is_some();
    let target = if exiting { 0.0 } else { 1.0 };
    let target_position = if exiting {
        tooltip.position_from
    } else {
        gpui::point(tooltip.x, tooltip.y)
    };
    let surface = tooltip_surface(tokens, tooltip.label, None)
        .debug_selector(|| "workspace-tooltip-surface".into());
    let anchored_surface = if !tokens.motion.enabled {
        tooltip.opacity.set(target);
        tooltip.position.set(target_position);
        anchored()
            .anchor(Corner::TopLeft)
            .position(gpui::point(px(target_position.x), px(target_position.y)))
            .position_mode(AnchoredPositionMode::Window)
            .child(surface.opacity(target))
            .into_any_element()
    } else {
        let duration = if exiting {
            exit_duration
        } else {
            oxideterm_gpui_ui::motion::duration(
                tokens,
                if tooltip.switching {
                    oxideterm_gpui_ui::motion::MotionDuration::Control
                } else {
                    oxideterm_gpui_ui::motion::MotionDuration::Micro
                },
            )
        };
        let opacity = tooltip.opacity;
        let from = tooltip.fade_from;
        let position = tooltip.position;
        let position_from = tooltip.position_from;
        let spatial = tokens.motion.spatial_enabled;
        let easing = if tooltip.switching && !exiting {
            oxideterm_gpui_ui::motion::ease_in_out_cubic
        } else {
            oxideterm_gpui_ui::motion::ease_out_cubic
        };
        let surface = surface.with_animation(
            ("workspace-tooltip-fade", tooltip.generation as usize),
            Animation::new(duration).with_easing(easing),
            move |surface, progress| {
                let value = from + (target - from) * progress;
                opacity.set(value);
                oxideterm_gpui_ui::motion::surface_opacity(surface, value)
            },
        );
        let anchor = anchored()
            .anchor(Corner::TopLeft)
            .position_mode(AnchoredPositionMode::Window)
            .child(surface);
        if spatial {
            // Move the anchor so viewport fitting applies to every animation frame.
            anchor
                .with_animation(
                    ("workspace-tooltip-position", tooltip.generation as usize),
                    Animation::new(duration).with_easing(easing),
                    move |anchor, progress| {
                        let point = gpui::point(
                            position_from.x + (target_position.x - position_from.x) * progress,
                            position_from.y + (target_position.y - position_from.y) * progress,
                        );
                        position.set(point);
                        anchor.position(gpui::point(px(point.x), px(point.y)))
                    },
                )
                .into_any_element()
        } else {
            position.set(target_position);
            anchor
                .position(gpui::point(px(target_position.x), px(target_position.y)))
                .into_any_element()
        }
    };
    deferred(anchored_surface)
        .with_priority(oxideterm_gpui_ui::modal::TAURI_TOOLTIP_LAYER_PRIORITY)
        .into_any_element()
}
