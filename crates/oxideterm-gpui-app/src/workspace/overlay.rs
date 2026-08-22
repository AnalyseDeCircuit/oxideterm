// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::Task;

const WORKSPACE_NOTICE_TTL: Duration = Duration::from_secs(4);
const TOOLTIP_DELAY: Duration = Duration::from_millis(300);
const CONNECTION_TRACE_UPDATE_COALESCE: Duration = Duration::from_millis(300);
const CONNECTION_TRACE_SUCCESS_TTL: Duration = Duration::from_millis(900);
const CONNECTION_TRACE_HISTORY_LIMIT: usize = 12;
const DISMISSED_CONNECTION_TRACE_LIMIT: usize = 64;
const CONNECTION_CARD_LAYER_PRIORITY: usize = 40;
const TERMINAL_FONT_SIZE_HUD_HORIZONTAL_PADDING: f32 = 20.0;
const TERMINAL_FONT_SIZE_HUD_VERTICAL_PADDING: f32 = 12.0;
const TERMINAL_FONT_SIZE_HUD_VALUE_TEXT_SIZE: f32 = 24.0;
const TERMINAL_FONT_SIZE_HUD_UNIT_TEXT_SIZE: f32 = 16.0;
const TERMINAL_FONT_SIZE_HUD_UNIT_GAP: f32 = 2.0;
const TERMINAL_FONT_SIZE_HUD_BACKGROUND_ALPHA: u32 = 0xe6;

/// Typed cross-system updates accepted by the window overlay owner.
pub(in crate::workspace) enum WorkspaceOverlayIntent {
    Notice {
        notice: TerminalNotice,
        ttl: Duration,
    },
    PluginProgress {
        key: String,
        notice: TerminalNotice,
        ttl: Duration,
    },
    DismissPluginProgress {
        key: String,
    },
    ConnectionTraceEvents(Vec<ConnectionTraceEvent>),
    QueueTooltip {
        id: String,
        label: String,
        x: f32,
        y: f32,
    },
    ClearTooltip {
        id: String,
    },
    ClearAllTooltips,
    ShowZenHint {
        ttl: Duration,
    },
    ClearZenHint,
    ShowTerminalFontSizeHud {
        font_size: i64,
        ttl: Duration,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceOverlayConfirmKind {
    SettingsReset,
    LegalNotice,
    NativeUpdateReleaseNotes,
    NodeDisconnect {
        node_id: NodeId,
        display_name: Arc<str>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct WorkspaceOverlayConfirmSnapshot {
    pub(in crate::workspace) kind: WorkspaceOverlayConfirmKind,
    pub(in crate::workspace) phase: oxideterm_gpui_ui::motion::ExitPhase,
    pub(in crate::workspace) focused_action: Option<ConfirmDialogAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceOverlayConfirmEffect {
    ResetSettings,
    DisconnectNode { node_id: NodeId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceOverlayConfirmKeyAction {
    Cancel,
    Confirm,
    Handled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceOverlayConfirmOwnerKind {
    SettingsReset,
    LegalNotice,
    NativeUpdateReleaseNotes,
    NodeDisconnect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct WorkspaceOverlayConfirmOwnerSnapshot {
    pub(in crate::workspace) kind: WorkspaceOverlayConfirmOwnerKind,
    pub(in crate::workspace) phase: oxideterm_gpui_ui::motion::ExitPhase,
}

#[derive(Clone, Debug)]
struct OverlayToast {
    id: u64,
    notice: TerminalNotice,
    expires_at: Instant,
    remove_at: Option<Instant>,
    presence: oxideterm_gpui_ui::motion::ExitPresence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalFontSizeHud {
    font_size: i64,
    generation: u64,
    expires_at: Instant,
}

#[derive(Clone, Debug)]
struct ActiveConnectionCard {
    visible: bool,
    latest: ConnectionTraceEvent,
    displayed: Option<ConnectionTraceEvent>,
    history: VecDeque<ConnectionTraceEvent>,
    started_at: Instant,
    flush_deadline: Option<(Instant, u64)>,
    flush_generation: u64,
    expires_at: Option<Instant>,
    remove_at: Option<Instant>,
    presence: oxideterm_gpui_ui::motion::ExitPresence,
}

#[derive(Clone, Debug)]
struct WorkspaceTooltip {
    id: String,
    label: String,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
struct WorkspaceTooltipPending {
    id: String,
    label: String,
    x: f32,
    y: f32,
    generation: u64,
    show_at: Instant,
}

/// Owns every window-global transient overlay and its delivery/timer lifetime.
pub(in crate::workspace) struct WorkspaceOverlayEntity {
    notice_tx: delivery::ActiveDeliverySender<TerminalNotice>,
    notice_rx: std::sync::mpsc::Receiver<TerminalNotice>,
    _notice_wake: delivery::ActiveDeliveryWake,
    _notice_delivery_task: Task<()>,
    deadline_task: Option<Task<()>>,
    deadline_generation: u64,
    control_exit_duration: Duration,
    next_toast_id: u64,
    standard_toasts: Vec<OverlayToast>,
    plugin_progress_toasts: HashMap<String, OverlayToast>,
    connection_trace_cards: HashMap<String, ActiveConnectionCard>,
    dismissed_connection_traces: VecDeque<String>,
    tooltip: Option<WorkspaceTooltip>,
    tooltip_pending: Option<WorkspaceTooltipPending>,
    tooltip_generation: u64,
    zen_hint_expires_at: Option<Instant>,
    terminal_font_size_hud: Option<TerminalFontSizeHud>,
    terminal_font_size_hud_generation: u64,
    confirm: Option<WorkspaceOverlayConfirmKind>,
    confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    confirm_focused_action: Option<ConfirmDialogAction>,
    confirm_exit_task: Option<Task<()>>,
}

impl WorkspaceOverlayEntity {
    pub(in crate::workspace) fn new(
        control_exit_duration: Duration,
        cx: &mut Context<Self>,
    ) -> Self {
        let notice_wake = delivery::ActiveDeliveryWake::default();
        let (notice_tx, notice_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(notice_wake.clone());
        let release_wake = notice_wake.clone();
        cx.on_release(move |_, _| {
            // Background producers may outlive the window overlay Entity.
            release_wake.stop();
        })
        .detach();
        let delivery_wake = notice_wake.clone();
        let notice_delivery_task = cx.spawn(async move |overlay, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = overlay
                        .update(cx, |overlay, cx| overlay.drain_notices(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        });
        Self {
            notice_tx,
            notice_rx,
            _notice_wake: notice_wake,
            _notice_delivery_task: notice_delivery_task,
            deadline_task: None,
            deadline_generation: 0,
            control_exit_duration,
            next_toast_id: 1,
            standard_toasts: Vec::new(),
            plugin_progress_toasts: HashMap::new(),
            connection_trace_cards: HashMap::new(),
            dismissed_connection_traces: VecDeque::new(),
            tooltip: None,
            tooltip_pending: None,
            tooltip_generation: 0,
            zen_hint_expires_at: None,
            terminal_font_size_hud: None,
            terminal_font_size_hud_generation: 0,
            confirm: None,
            confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            confirm_focused_action: None,
            confirm_exit_task: None,
        }
    }

    pub(in crate::workspace) fn notice_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<TerminalNotice> {
        self.notice_tx.clone()
    }

    pub(in crate::workspace) fn apply_intent(
        &mut self,
        intent: WorkspaceOverlayIntent,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = match intent {
            WorkspaceOverlayIntent::Notice { notice, ttl } => {
                self.push_notice(notice, ttl);
                true
            }
            WorkspaceOverlayIntent::PluginProgress { key, notice, ttl } => {
                self.upsert_plugin_progress(key, notice, ttl);
                true
            }
            WorkspaceOverlayIntent::DismissPluginProgress { key } => {
                self.dismiss_plugin_progress(&key, Instant::now())
            }
            WorkspaceOverlayIntent::ConnectionTraceEvents(events) => {
                self.apply_connection_trace_events(events, Instant::now())
            }
            WorkspaceOverlayIntent::QueueTooltip { id, label, x, y } => {
                self.queue_tooltip(id, label, x, y, Instant::now())
            }
            WorkspaceOverlayIntent::ClearTooltip { id } => self.clear_tooltip(&id),
            WorkspaceOverlayIntent::ClearAllTooltips => self.clear_all_tooltips(),
            WorkspaceOverlayIntent::ShowZenHint { ttl } => {
                self.zen_hint_expires_at = Some(Instant::now() + ttl);
                true
            }
            WorkspaceOverlayIntent::ClearZenHint => self.zen_hint_expires_at.take().is_some(),
            WorkspaceOverlayIntent::ShowTerminalFontSizeHud { font_size, ttl } => {
                self.terminal_font_size_hud_generation =
                    self.terminal_font_size_hud_generation.wrapping_add(1);
                self.terminal_font_size_hud = Some(TerminalFontSizeHud {
                    font_size,
                    generation: self.terminal_font_size_hud_generation,
                    expires_at: Instant::now() + ttl,
                });
                true
            }
        };
        if changed {
            self.schedule_next_deadline(cx);
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn set_control_exit_duration(
        &mut self,
        duration: Duration,
        cx: &mut Context<Self>,
    ) {
        if self.control_exit_duration != duration {
            self.control_exit_duration = duration;
            self.schedule_next_deadline(cx);
        }
    }

    pub(in crate::workspace) fn open_confirm(
        &mut self,
        confirm: WorkspaceOverlayConfirmKind,
        cx: &mut Context<Self>,
    ) {
        // Window-global confirms are mutually exclusive; replacement cancels
        // the stale retained exit before installing the new payload.
        self.confirm_exit_task = None;
        self.confirm = Some(confirm);
        self.confirm_presence.reopen();
        self.confirm_focused_action = None;
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_snapshot(&self) -> Option<WorkspaceOverlayConfirmSnapshot> {
        self.confirm
            .as_ref()
            .cloned()
            .map(|kind| WorkspaceOverlayConfirmSnapshot {
                kind,
                phase: self.confirm_presence.phase(),
                focused_action: self.confirm_focused_action,
            })
    }

    pub(in crate::workspace) fn confirm_owner_snapshot(
        &self,
    ) -> Option<WorkspaceOverlayConfirmOwnerSnapshot> {
        self.confirm
            .as_ref()
            .map(|confirm| WorkspaceOverlayConfirmOwnerSnapshot {
                kind: match confirm {
                    WorkspaceOverlayConfirmKind::SettingsReset => {
                        WorkspaceOverlayConfirmOwnerKind::SettingsReset
                    }
                    WorkspaceOverlayConfirmKind::LegalNotice => {
                        WorkspaceOverlayConfirmOwnerKind::LegalNotice
                    }
                    WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes => {
                        WorkspaceOverlayConfirmOwnerKind::NativeUpdateReleaseNotes
                    }
                    WorkspaceOverlayConfirmKind::NodeDisconnect { .. } => {
                        WorkspaceOverlayConfirmOwnerKind::NodeDisconnect
                    }
                },
                phase: self.confirm_presence.phase(),
            })
    }

    pub(in crate::workspace) fn handle_confirm_key(
        &mut self,
        key: &str,
        shift: bool,
        blocked_by_primary_modifier: bool,
        cx: &mut Context<Self>,
    ) -> Option<WorkspaceOverlayConfirmKeyAction> {
        let confirm = self.confirm.as_ref()?;
        if blocked_by_primary_modifier
            || self.confirm_presence.phase() != oxideterm_gpui_ui::motion::ExitPhase::Visible
        {
            return None;
        }
        const FULL_ACTIONS: [ConfirmDialogAction; 2] =
            [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];
        const CLOSE_ACTION: [ConfirmDialogAction; 1] = [ConfirmDialogAction::Cancel];
        let actions = if matches!(
            confirm,
            WorkspaceOverlayConfirmKind::SettingsReset
                | WorkspaceOverlayConfirmKind::NodeDisconnect { .. }
        ) {
            &FULL_ACTIONS[..]
        } else {
            &CLOSE_ACTION[..]
        };
        match browser_behavior::modal_footer_key_action(
            key,
            shift,
            actions,
            self.confirm_focused_action,
            ConfirmDialogAction::Cancel,
        ) {
            Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.confirm_focused_action = None;
                Some(WorkspaceOverlayConfirmKeyAction::Cancel)
            }
            Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.confirm_focused_action = Some(action);
                cx.notify();
                Some(WorkspaceOverlayConfirmKeyAction::Handled)
            }
            Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                self.confirm_focused_action = None;
                Some(match action {
                    ConfirmDialogAction::Cancel => WorkspaceOverlayConfirmKeyAction::Cancel,
                    ConfirmDialogAction::Confirm => WorkspaceOverlayConfirmKeyAction::Confirm,
                })
            }
            None => None,
        }
    }

    pub(in crate::workspace) fn begin_confirm_exit(
        &mut self,
        confirmed: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> (bool, Option<WorkspaceOverlayConfirmEffect>) {
        let Some(confirm) = self.confirm.as_ref() else {
            return (false, None);
        };
        let Some(generation) = self.confirm_presence.begin_exit() else {
            return (false, None);
        };
        self.confirm_focused_action = None;
        let effect = if confirmed {
            match confirm {
                WorkspaceOverlayConfirmKind::SettingsReset => {
                    Some(WorkspaceOverlayConfirmEffect::ResetSettings)
                }
                WorkspaceOverlayConfirmKind::NodeDisconnect { node_id, .. } => {
                    Some(WorkspaceOverlayConfirmEffect::DisconnectNode {
                        node_id: node_id.clone(),
                    })
                }
                WorkspaceOverlayConfirmKind::LegalNotice
                | WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes => None,
            }
        } else {
            None
        };
        self.confirm_exit_task = None;
        if delay.is_zero() {
            self.finish_confirm_exit(generation, cx);
            return (true, effect);
        }
        self.confirm_exit_task = Some(cx.spawn(async move |overlay, cx| {
            Timer::after(delay).await;
            let _ = overlay.update(cx, |overlay, cx| {
                overlay.finish_confirm_exit(generation, cx);
            });
        }));
        cx.notify();
        (true, effect)
    }

    fn finish_confirm_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.confirm_exit_task = None;
        if self.confirm.is_some() && self.confirm_presence.finish_exit(generation) {
            self.confirm = None;
            self.confirm_presence.reopen();
            self.confirm_focused_action = None;
            cx.notify();
        }
    }

    fn drain_notices(&mut self, cx: &mut Context<Self>) -> bool {
        let batch =
            delivery::drain_channel(&self.notice_rx, delivery::NOTIFICATION_DELIVERY_BUDGET);
        if !batch.items.is_empty() {
            for notice in batch.items {
                self.push_notice(notice, WORKSPACE_NOTICE_TTL);
            }
            self.schedule_next_deadline(cx);
            cx.notify();
        }
        batch.outcome.backlog_remaining
    }

    fn push_notice(&mut self, notice: TerminalNotice, ttl: Duration) {
        let id = self.next_toast_id;
        self.next_toast_id = self.next_toast_id.wrapping_add(1).max(1);
        self.standard_toasts.push(OverlayToast {
            id,
            notice,
            expires_at: Instant::now() + ttl,
            remove_at: None,
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
        });
    }

    fn upsert_plugin_progress(&mut self, key: String, notice: TerminalNotice, ttl: Duration) {
        let expires_at = Instant::now() + ttl;
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
                expires_at,
                remove_at: None,
                presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            },
        );
    }

    fn queue_tooltip(&mut self, id: String, label: String, x: f32, y: f32, now: Instant) -> bool {
        if let Some(tooltip) = self.tooltip.as_mut()
            && tooltip.id == id
        {
            let changed = tooltip.label != label || tooltip.x != x || tooltip.y != y;
            tooltip.label = label;
            tooltip.x = x;
            tooltip.y = y;
            return changed;
        }
        if let Some(pending) = self.tooltip_pending.as_mut()
            && pending.id == id
        {
            pending.x = x;
            pending.y = y;
            return false;
        }
        self.tooltip = None;
        self.tooltip_generation = self.tooltip_generation.wrapping_add(1);
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

    fn clear_tooltip(&mut self, id: &str) -> bool {
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
        if self
            .tooltip
            .as_ref()
            .is_some_and(|tooltip| tooltip.id == id)
        {
            self.tooltip = None;
            changed = true;
        }
        changed
    }

    fn clear_all_tooltips(&mut self) -> bool {
        let changed = self.tooltip.take().is_some() || self.tooltip_pending.take().is_some();
        if changed {
            self.tooltip_generation = self.tooltip_generation.wrapping_add(1);
        }
        changed
    }

    fn apply_connection_trace_events(
        &mut self,
        events: Vec<ConnectionTraceEvent>,
        now: Instant,
    ) -> bool {
        let mut changed = false;
        for event in coalesce_connection_trace_running_events(events) {
            let attempt_id = event.attempt_id.clone();
            if let Some(position) = self
                .dismissed_connection_traces
                .iter()
                .position(|dismissed| dismissed == &attempt_id)
            {
                if event.status != ConnectionTraceStatus::Running {
                    self.dismissed_connection_traces.remove(position);
                }
                continue;
            }
            let final_step_ready = event.status == ConnectionTraceStatus::Ready
                && event
                    .step_index
                    .zip(event.total_steps)
                    .is_none_or(|(step, total)| step >= total);
            if final_step_ready {
                // A ready terminal is already the success confirmation. Remove
                // its overlay in the same delivery instead of covering the pane.
                changed |= self.connection_trace_cards.remove(&attempt_id).is_some();
                continue;
            }
            let trace = self
                .connection_trace_cards
                .entry(attempt_id.clone())
                .or_insert_with(|| ActiveConnectionCard {
                    visible: false,
                    latest: event.clone(),
                    displayed: None,
                    history: VecDeque::from([event.clone()]),
                    started_at: now,
                    flush_deadline: None,
                    flush_generation: 0,
                    expires_at: None,
                    remove_at: None,
                    presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
                });
            if trace.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
                trace.presence.reopen();
            }
            if trace.history.back().is_none_or(|previous| {
                previous.stage != event.stage || previous.status != event.status
            }) {
                trace.history.push_back(event.clone());
                if trace.history.len() > CONNECTION_TRACE_HISTORY_LIMIT {
                    trace.history.pop_front();
                }
            }
            trace.latest = event.clone();
            trace.expires_at = None;
            trace.remove_at = None;

            match event.status {
                ConnectionTraceStatus::Running => {
                    if !trace.visible {
                        // Show the first stage immediately so a fast connection cannot finish
                        // before the card has ever entered the rendered overlay tree.
                        trace.visible = true;
                        trace.displayed = Some(event);
                        changed = true;
                    } else {
                        trace.flush_generation = trace.flush_generation.wrapping_add(1);
                        trace.flush_deadline = Some((
                            now + CONNECTION_TRACE_UPDATE_COALESCE,
                            trace.flush_generation,
                        ));
                    }
                }
                ConnectionTraceStatus::Ready => {
                    trace.flush_deadline = None;
                    let mut success = event;
                    success.elapsed_ms = trace
                        .started_at
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64;
                    trace.visible = true;
                    trace.latest = success.clone();
                    trace.displayed = Some(success);
                    trace.expires_at = Some(now + CONNECTION_TRACE_SUCCESS_TTL);
                    changed = true;
                }
                ConnectionTraceStatus::Failed => {
                    trace.visible = true;
                    trace.flush_deadline = None;
                    trace.latest = event.clone();
                    trace.displayed = Some(event);
                    trace.expires_at = None;
                    changed = true;
                }
                ConnectionTraceStatus::Cancelled => {
                    trace.flush_deadline = None;
                    if trace.displayed.is_some() {
                        self.begin_trace_exit(&attempt_id, now);
                    } else {
                        self.connection_trace_cards.remove(&attempt_id);
                    }
                    changed = true;
                }
            }
        }
        changed
    }

    fn begin_standard_exit(&mut self, toast_id: u64, now: Instant) -> bool {
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

    fn dismiss_plugin_progress(&mut self, key: &str, now: Instant) -> bool {
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

    fn begin_trace_exit(&mut self, attempt_id: &str, now: Instant) -> bool {
        let Some(trace) = self.connection_trace_cards.get_mut(attempt_id) else {
            return false;
        };
        if trace.presence.begin_exit().is_none() {
            return false;
        }
        trace.expires_at = None;
        if self.control_exit_duration.is_zero() {
            self.connection_trace_cards.remove(attempt_id);
        } else {
            trace.remove_at = Some(now + self.control_exit_duration);
        }
        true
    }

    fn dismiss_connection_trace(&mut self, attempt_id: &str, now: Instant) -> bool {
        let Some(trace) = self.connection_trace_cards.get(attempt_id) else {
            return false;
        };
        // Keep a bounded tombstone so late progress cannot reopen a card the user dismissed.
        if trace.latest.status == ConnectionTraceStatus::Running
            && !self
                .dismissed_connection_traces
                .iter()
                .any(|dismissed| dismissed == attempt_id)
        {
            self.dismissed_connection_traces
                .push_back(attempt_id.to_string());
            if self.dismissed_connection_traces.len() > DISMISSED_CONNECTION_TRACE_LIMIT {
                self.dismissed_connection_traces.pop_front();
            }
        }
        self.begin_trace_exit(attempt_id, now)
    }

    fn schedule_next_deadline(&mut self, cx: &mut Context<Self>) {
        self.deadline_generation = self.deadline_generation.wrapping_add(1);
        self.deadline_task.take();
        let Some(deadline) = self.next_deadline() else {
            return;
        };
        let generation = self.deadline_generation;
        let delay = deadline.saturating_duration_since(Instant::now());
        self.deadline_task = Some(cx.spawn(async move |overlay, cx| {
            Timer::after(delay).await;
            let _ = overlay.update(cx, |overlay, cx| {
                overlay.handle_deadline_generation(generation, Instant::now(), cx);
            });
        }));
    }

    fn handle_deadline_generation(
        &mut self,
        generation: u64,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        if self.deadline_generation != generation {
            return;
        }
        if self.process_due_deadlines(now) {
            cx.notify();
        }
        self.schedule_next_deadline(cx);
    }

    fn process_due_deadlines(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if self
            .tooltip_pending
            .as_ref()
            .is_some_and(|pending| pending.show_at <= now)
            && let Some(pending) = self.tooltip_pending.take()
            && pending.generation == self.tooltip_generation
        {
            self.tooltip = Some(WorkspaceTooltip {
                id: pending.id,
                label: pending.label,
                x: pending.x,
                y: pending.y,
            });
            changed = true;
        }
        if self
            .zen_hint_expires_at
            .is_some_and(|expires_at| expires_at <= now)
        {
            self.zen_hint_expires_at = None;
            changed = true;
        }
        if self
            .terminal_font_size_hud
            .is_some_and(|hud| hud.expires_at <= now)
        {
            self.terminal_font_size_hud = None;
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

        let trace_ids = self
            .connection_trace_cards
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for attempt_id in trace_ids {
            let Some(trace) = self.connection_trace_cards.get_mut(&attempt_id) else {
                continue;
            };
            if let Some((flush_at, generation)) = trace.flush_deadline
                && flush_at <= now
            {
                trace.flush_deadline = None;
                if trace.flush_generation == generation
                    && trace.visible
                    && trace.latest.status == ConnectionTraceStatus::Running
                {
                    trace.displayed = Some(trace.latest.clone());
                    changed = true;
                }
            }
            if trace.expires_at.is_some_and(|expires_at| expires_at <= now)
                && trace.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible
            {
                changed |= self.begin_trace_exit(&attempt_id, now);
            }
        }
        let trace_remove = self
            .connection_trace_cards
            .iter()
            .filter(|(_, trace)| trace.remove_at.is_some_and(|remove_at| remove_at <= now))
            .map(|(attempt_id, _)| attempt_id.clone())
            .collect::<Vec<_>>();
        for attempt_id in trace_remove {
            self.connection_trace_cards.remove(&attempt_id);
            changed = true;
        }
        changed
    }

    fn next_deadline(&self) -> Option<Instant> {
        let mut next = self.tooltip_pending.as_ref().map(|pending| pending.show_at);
        next = min_deadline(next, self.zen_hint_expires_at);
        next = min_deadline(next, self.terminal_font_size_hud.map(|hud| hud.expires_at));
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
        for trace in self.connection_trace_cards.values() {
            next = min_deadline(next, trace.flush_deadline.map(|(deadline, _)| deadline));
            next = min_deadline(next, trace.expires_at);
            next = min_deadline(next, trace.remove_at);
        }
        next
    }

    pub(in crate::workspace) fn render_layers(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        native_update: Option<ToastView>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut layers = Vec::new();
        if let Some(tooltip) = self.tooltip.clone() {
            layers.push(render_tooltip(tokens, tooltip));
        }
        if self
            .zen_hint_expires_at
            .is_some_and(|expires_at| expires_at > Instant::now())
        {
            layers.push(render_zen_hint(tokens, i18n));
        }
        if let Some(toasts) = self.render_toasts(tokens, native_update, cx) {
            layers.push(toasts);
        }
        if let Some(connection_cards) = self.render_connection_cards(tokens, i18n, cx) {
            layers.push(connection_cards);
        }
        if let Some(hud) = self.terminal_font_size_hud {
            layers.push(render_terminal_font_size_hud(
                tokens,
                mono_font_family,
                hud.font_size,
            ));
        }
        layers
    }

    fn render_toasts(
        &self,
        tokens: &ThemeTokens,
        native_update: Option<ToastView>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.standard_toasts.is_empty()
            && self.plugin_progress_toasts.is_empty()
            && native_update.is_none()
        {
            return None;
        }
        let overlay = cx.entity();
        let standard = self.standard_toasts.iter().map(|toast| {
            let toast_id = toast.id;
            let overlay = overlay.clone();
            ToastView {
                id: format!("workspace-{}", toast.id),
                phase: toast.presence.phase(),
                title: toast.notice.title.clone(),
                description: toast.notice.description.clone(),
                status_text: toast.notice.status_text.clone(),
                progress: toast.notice.progress,
                variant: toast_variant_from_terminal(toast.notice.variant),
                actions: None,
                close: Some(
                    toast_close(tokens)
                        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                            let _ = overlay.update(cx, |overlay, cx| {
                                if overlay.begin_standard_exit(toast_id, Instant::now()) {
                                    overlay.schedule_next_deadline(cx);
                                    cx.notify();
                                }
                            });
                            cx.stop_propagation();
                        })
                        .into_any_element(),
                ),
            }
        });
        let plugin = self.plugin_progress_toasts.iter().map(|(key, toast)| {
            let key = key.clone();
            let overlay = overlay.clone();
            ToastView {
                id: format!("plugin-{key}"),
                phase: toast.presence.phase(),
                title: toast.notice.title.clone(),
                description: toast.notice.description.clone(),
                status_text: toast.notice.status_text.clone(),
                progress: toast.notice.progress,
                variant: toast_variant_from_terminal(toast.notice.variant),
                actions: None,
                close: Some(
                    toast_close(tokens)
                        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                            let _ = overlay.update(cx, |overlay, cx| {
                                if overlay.dismiss_plugin_progress(&key, Instant::now()) {
                                    overlay.schedule_next_deadline(cx);
                                    cx.notify();
                                }
                            });
                            cx.stop_propagation();
                        })
                        .into_any_element(),
                ),
            }
        });
        Some(toaster(tokens, standard.chain(plugin).chain(native_update)).into_any_element())
    }

    fn render_connection_cards(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let overlay = cx.entity();
        let cards = self
            .connection_trace_cards
            .iter()
            .filter_map(|(attempt_id, trace)| {
                let event = trace.displayed.as_ref()?;
                Some(render_connection_card(
                    tokens,
                    i18n,
                    attempt_id,
                    event,
                    &trace.history,
                    trace.presence.phase(),
                    overlay.clone(),
                ))
            })
            .collect::<Vec<_>>();
        if cards.is_empty() {
            return None;
        }

        Some(
            deferred(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .left_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(24.0))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(720.0))
                            .flex()
                            .flex_col()
                            .gap(px(tokens.spacing.three))
                            .children(cards),
                    ),
            )
            .with_priority(CONNECTION_CARD_LAYER_PRIORITY)
            .into_any_element(),
        )
    }

    #[cfg(test)]
    fn notice_wake(&self) -> delivery::ActiveDeliveryWake {
        self._notice_wake.clone()
    }
}

fn render_connection_card(
    tokens: &ThemeTokens,
    i18n: &I18n,
    attempt_id: &str,
    event: &ConnectionTraceEvent,
    history: &VecDeque<ConnectionTraceEvent>,
    phase: oxideterm_gpui_ui::motion::ExitPhase,
    overlay: Entity<WorkspaceOverlayEntity>,
) -> AnyElement {
    let status_color = match event.status {
        ConnectionTraceStatus::Ready => tokens.ui.success,
        ConnectionTraceStatus::Failed => tokens.ui.error,
        ConnectionTraceStatus::Running | ConnectionTraceStatus::Cancelled => tokens.ui.accent,
    };
    let title = connection_trace_title(i18n, event);
    let status = (event.status != ConnectionTraceStatus::Failed)
        .then(|| connection_trace_status_text(i18n, event));
    let endpoint = event.endpoint.clone();
    let attempt_id_for_close = attempt_id.to_string();
    let close = toast_close(tokens)
        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
            let _ = overlay.update(cx, |overlay, cx| {
                if overlay.dismiss_connection_trace(&attempt_id_for_close, Instant::now()) {
                    overlay.schedule_next_deadline(cx);
                    cx.notify();
                }
            });
            cx.stop_propagation();
        })
        .into_any_element();

    let header = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(tokens.spacing.three))
        .child(
            div()
                .size(px(44.0))
                .flex_none()
                .rounded(px(tokens.radii.md))
                .bg(rgba((status_color << 8) | 0x24))
                .border_1()
                .border_color(rgba((status_color << 8) | 0x66))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(status_color))
                .child("SSH"),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(tokens.ui.text))
                        .child(title),
                )
                .when_some(endpoint, |header, endpoint| {
                    header.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(format!("SSH  {endpoint}")),
                    )
                })
                .when_some(status, |header, status| {
                    header.child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(status_color))
                            .child(status),
                    )
                }),
        )
        .child(close);

    let mut card = oxideterm_gpui_ui::semantic_surface(
        tokens,
        oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::TerminalOverlay)
            .padding(oxideterm_gpui_ui::SurfacePadding::Spacious),
    )
    .w_full()
    .flex()
    .flex_col()
    .gap(px(tokens.spacing.three * 2.0))
    .occlude()
    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
        cx.stop_propagation();
    })
    .child(header)
    .child(render_connection_stage_path(
        tokens, i18n, attempt_id, event,
    ));

    if let Some(error_log) = render_connection_error_log(tokens, event) {
        card = card.child(error_log);
    }
    card = card.child(render_connection_history(tokens, i18n, history));

    oxideterm_gpui_ui::motion::fade(
        tokens,
        format!("connection-card-{attempt_id}"),
        card,
        oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        phase == oxideterm_gpui_ui::motion::ExitPhase::Visible,
    )
}

fn render_connection_stage_path(
    tokens: &ThemeTokens,
    i18n: &I18n,
    attempt_id: &str,
    event: &ConnectionTraceEvent,
) -> AnyElement {
    const STAGES: [(ConnectionTraceStage, &str); 4] = [
        (
            ConnectionTraceStage::OpeningTransport,
            "connections.trace.stage.opening_transport",
        ),
        (
            ConnectionTraceStage::SshHandshake,
            "connections.trace.stage.ssh_handshake",
        ),
        (
            ConnectionTraceStage::Authentication,
            "connections.trace.stage.authentication",
        ),
        (ConnectionTraceStage::Ready, "connections.trace.stage.ready"),
    ];
    let current = connection_trace_path_index(event.stage);
    let failed = event.status == ConnectionTraceStatus::Failed;
    let ready = event.status == ConnectionTraceStatus::Ready;

    let nodes = STAGES
        .iter()
        .enumerate()
        .map(|(index, (_, label_key))| {
            let completed = ready || index < current;
            let active = !ready && index == current;
            let color = if failed && active {
                tokens.ui.error
            } else if completed {
                tokens.ui.success
            } else if active {
                tokens.ui.accent
            } else {
                tokens.ui.text_muted
            };
            let marker_text = if completed { "✓" } else { "" };
            let marker = div()
                .size(px(30.0))
                .flex_none()
                .rounded_full()
                .border_1()
                .border_color(rgba((color << 8) | 0xaa))
                .bg(rgb(tokens.ui.bg_elevated))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(color))
                .child(marker_text);
            let marker = if active && !failed && tokens.motion.enabled {
                marker
                    .with_animation(
                        format!("connection-stage-{attempt_id}-{index}"),
                        Animation::new(Duration::from_millis(1200)).repeat(),
                        |marker, progress| {
                            let wave = if progress < 0.5 {
                                progress * 2.0
                            } else {
                                (1.0 - progress) * 2.0
                            };
                            marker.opacity(0.7 + wave * 0.3)
                        },
                    )
                    .into_any_element()
            } else {
                marker.into_any_element()
            };
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(tokens.spacing.two))
                .child(marker)
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(color))
                        .whitespace_nowrap()
                        .child(i18n.t(label_key)),
                )
                .into_any_element()
        })
        .collect::<Vec<_>>();

    let current_connector = current.min(STAGES.len() - 1);
    let connectors = (0..STAGES.len() - 1)
        .map(|index| {
            let complete = ready || index < current_connector;
            div()
                .absolute()
                .top(px(14.0))
                .left(relative((index as f32 + 0.5) / STAGES.len() as f32))
                .w(relative(1.0 / STAGES.len() as f32))
                .h(px(2.0))
                .bg(rgb(if complete {
                    tokens.ui.success
                } else {
                    tokens.ui.border
                }))
                .into_any_element()
        })
        .collect::<Vec<_>>();

    div()
        .relative()
        .w_full()
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .children(connectors),
        )
        .child(div().relative().w_full().flex().children(nodes))
        .into_any_element()
}

fn render_connection_history(
    tokens: &ThemeTokens,
    i18n: &I18n,
    history: &VecDeque<ConnectionTraceEvent>,
) -> AnyElement {
    let rows = history.iter().map(|entry| {
        let color = match entry.status {
            ConnectionTraceStatus::Ready => tokens.ui.success,
            ConnectionTraceStatus::Failed => tokens.ui.error,
            ConnectionTraceStatus::Running | ConnectionTraceStatus::Cancelled => {
                tokens.ui.text_muted
            }
        };
        let detail = entry
            .detail
            .clone()
            .filter(|detail| !detail.is_empty())
            .filter(|_| entry.status != ConnectionTraceStatus::Failed);
        let text_color = if entry.status == ConnectionTraceStatus::Failed {
            tokens.ui.error
        } else {
            tokens.ui.text_muted
        };
        div()
            .w_full()
            .flex()
            .items_start()
            .gap(px(tokens.spacing.two))
            .child(
                div()
                    .mt(px(7.0))
                    .size(px(6.0))
                    .flex_none()
                    .rounded_full()
                    .bg(rgb(color)),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(rgb(text_color))
                    .child(i18n.t(connection_trace_stage_key(entry.stage)))
                    .when_some(detail, |row, detail| {
                        row.child(
                            div()
                                .text_color(rgb(if entry.status == ConnectionTraceStatus::Failed {
                                    tokens.ui.error
                                } else {
                                    tokens.ui.text_muted
                                }))
                                .child(detail),
                        )
                    }),
            )
    });

    oxideterm_gpui_ui::semantic_surface(
        tokens,
        oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::InsetGroup)
            .padding(oxideterm_gpui_ui::SurfacePadding::Normal),
    )
    .w_full()
    .max_h(px(210.0))
    .overflow_y_scrollbar()
    .flex()
    .flex_col()
    .gap(px(tokens.spacing.two))
    .children(rows)
    .into_any_element()
}

fn render_connection_error_log(
    tokens: &ThemeTokens,
    event: &ConnectionTraceEvent,
) -> Option<AnyElement> {
    if event.status != ConnectionTraceStatus::Failed {
        return None;
    }
    let detail = event.detail.clone().filter(|detail| !detail.is_empty())?;
    Some(
        oxideterm_gpui_ui::semantic_surface(
            tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::InsetGroup)
                .padding(oxideterm_gpui_ui::SurfacePadding::Normal),
        )
        .w_full()
        .max_h(px(210.0))
        .overflow_y_scrollbar()
        .border_color(rgba((tokens.ui.error << 8) | 0x88))
        .bg(rgba((tokens.ui.error << 8) | 0x12))
        .text_size(px(13.0))
        .line_height(px(19.0))
        .text_color(rgb(tokens.ui.error))
        .whitespace_normal()
        .child(detail)
        .into_any_element(),
    )
}

fn connection_trace_path_index(stage: ConnectionTraceStage) -> usize {
    match stage {
        ConnectionTraceStage::Queued
        | ConnectionTraceStage::Preparing
        | ConnectionTraceStage::OpeningTransport => 0,
        ConnectionTraceStage::SshHandshake | ConnectionTraceStage::HostKey => 1,
        ConnectionTraceStage::Authentication
        | ConnectionTraceStage::KerberosCredentials
        | ConnectionTraceStage::GssapiExchange
        | ConnectionTraceStage::FallbackAuthentication => 2,
        ConnectionTraceStage::Pty
        | ConnectionTraceStage::ShellReady
        | ConnectionTraceStage::Ready => 3,
    }
}

fn min_deadline(current: Option<Instant>, candidate: Option<Instant>) -> Option<Instant> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (Some(current), None) => Some(current),
        (None, candidate) => candidate,
    }
}

pub(super) fn coalesce_connection_trace_running_events(
    events: Vec<ConnectionTraceEvent>,
) -> Vec<ConnectionTraceEvent> {
    let mut coalesced = Vec::with_capacity(events.len());
    let mut pending_running_by_attempt = HashMap::<String, usize>::new();
    for event in events {
        if event.status == ConnectionTraceStatus::Running {
            if let Some(index) = pending_running_by_attempt.get(&event.attempt_id).copied() {
                coalesced[index] = event;
            } else {
                pending_running_by_attempt.insert(event.attempt_id.clone(), coalesced.len());
                coalesced.push(event);
            }
        } else {
            pending_running_by_attempt.remove(&event.attempt_id);
            coalesced.push(event);
        }
    }
    coalesced
}

fn render_tooltip(tokens: &ThemeTokens, tooltip: WorkspaceTooltip) -> AnyElement {
    deferred(
        anchored()
            .anchor(Corner::TopLeft)
            .position(gpui::point(px(tooltip.x), px(tooltip.y)))
            .position_mode(AnchoredPositionMode::Window)
            .child(tooltip_content(tokens, tooltip.label, None)),
    )
    .with_priority(oxideterm_gpui_ui::modal::TAURI_TOOLTIP_LAYER_PRIORITY)
    .into_any_element()
}

fn render_zen_hint(tokens: &ThemeTokens, i18n: &I18n) -> AnyElement {
    let key = if cfg!(target_os = "macos") {
        "zen_mode.hint"
    } else {
        "zen_mode.hint_other"
    };
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(24.0))
        .flex()
        .justify_center()
        .child(
            div()
                .rounded(px(tokens.radii.md))
                .border_1()
                .border_color(rgb(tokens.ui.border))
                .bg(rgba((tokens.ui.bg_elevated << 8) | 0xe6))
                .px(px(16.0))
                .py(px(8.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(rgb(tokens.ui.text_muted))
                .shadow_lg()
                .child(i18n.t(key)),
        )
        .into_any_element()
}

fn render_terminal_font_size_hud(
    tokens: &ThemeTokens,
    mono_font_family: SharedString,
    font_size: i64,
) -> AnyElement {
    let card = div()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba(
            (tokens.ui.bg_elevated << 8) | TERMINAL_FONT_SIZE_HUD_BACKGROUND_ALPHA,
        ))
        .px(px(TERMINAL_FONT_SIZE_HUD_HORIZONTAL_PADDING))
        .py(px(TERMINAL_FONT_SIZE_HUD_VERTICAL_PADDING))
        .flex()
        .items_baseline()
        .font_family(mono_font_family)
        .shadow_lg()
        .child(
            div()
                .text_size(px(TERMINAL_FONT_SIZE_HUD_VALUE_TEXT_SIZE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(tokens.ui.text))
                .child(font_size.to_string()),
        )
        .child(
            div()
                .ml(px(TERMINAL_FONT_SIZE_HUD_UNIT_GAP))
                .text_size(px(TERMINAL_FONT_SIZE_HUD_UNIT_TEXT_SIZE))
                .font_weight(gpui::FontWeight::NORMAL)
                .text_color(rgb(tokens.ui.text_muted))
                .child("px"),
        );
    let layer = div()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .flex()
        .items_center()
        .justify_center()
        .child(card);
    deferred(oxideterm_gpui_ui::motion::fade_in(
        tokens,
        "terminal-font-size-hud",
        layer,
        oxideterm_gpui_ui::motion::MotionDuration::Control,
    ))
    .with_priority(oxideterm_gpui_ui::modal::TAURI_TOOLTIP_LAYER_PRIORITY)
    .into_any_element()
}

fn connection_trace_title(i18n: &I18n, event: &ConnectionTraceEvent) -> String {
    let label = event
        .label
        .clone()
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| {
            if event.node_id.0.is_empty() {
                i18n.t("connections.trace.target_unknown")
            } else {
                event.node_id.0.clone()
            }
        });
    let chain_title = event
        .step_index
        .zip(event.total_steps)
        .filter(|(_, total)| *total > 1);
    if event.status == ConnectionTraceStatus::Failed {
        return i18n
            .t("connections.trace.failed")
            .replace("{{label}}", &label);
    }
    match (event.mode, chain_title) {
        (ConnectionTraceMode::Reconnect, Some((current, total))) => i18n
            .t("connections.trace.reconnecting_chain")
            .replace("{{current}}", &current.to_string())
            .replace("{{total}}", &total.to_string())
            .replace("{{label}}", &label),
        (ConnectionTraceMode::Connect, Some((current, total))) => i18n
            .t("connections.trace.connecting_chain")
            .replace("{{current}}", &current.to_string())
            .replace("{{total}}", &total.to_string())
            .replace("{{label}}", &label),
        (ConnectionTraceMode::Reconnect, None) => i18n
            .t("connections.trace.reconnecting")
            .replace("{{label}}", &label),
        (ConnectionTraceMode::Connect, None) => i18n
            .t("connections.trace.connecting")
            .replace("{{label}}", &label),
    }
}

fn connection_trace_status_text(i18n: &I18n, event: &ConnectionTraceEvent) -> String {
    if event.status == ConnectionTraceStatus::Ready {
        return i18n.t("connections.trace.connected").replace(
            "{{elapsed}}",
            &format_connection_trace_elapsed(event.elapsed_ms),
        );
    }
    event
        .detail
        .clone()
        .unwrap_or_else(|| i18n.t(connection_trace_stage_key(event.stage)))
}

pub(in crate::workspace) fn toast_variant_from_terminal(
    variant: TerminalNoticeVariant,
) -> ToastVariant {
    match variant {
        TerminalNoticeVariant::Default => ToastVariant::Default,
        TerminalNoticeVariant::Success => ToastVariant::Success,
        TerminalNoticeVariant::Error => ToastVariant::Error,
        TerminalNoticeVariant::Warning => ToastVariant::Warning,
    }
}

pub(in crate::workspace) fn connection_trace_stage_key(
    stage: ConnectionTraceStage,
) -> &'static str {
    match stage {
        ConnectionTraceStage::Queued => "connections.trace.stage.queued",
        ConnectionTraceStage::Preparing => "connections.trace.stage.preparing",
        ConnectionTraceStage::OpeningTransport => "connections.trace.stage.opening_transport",
        ConnectionTraceStage::SshHandshake => "connections.trace.stage.ssh_handshake",
        ConnectionTraceStage::HostKey => "connections.trace.stage.host_key",
        ConnectionTraceStage::Authentication => "connections.trace.stage.authentication",
        ConnectionTraceStage::KerberosCredentials => "connections.trace.stage.kerberos_credentials",
        ConnectionTraceStage::GssapiExchange => "connections.trace.stage.gssapi_exchange",
        ConnectionTraceStage::FallbackAuthentication => {
            "connections.trace.stage.fallback_authentication"
        }
        ConnectionTraceStage::Pty => "connections.trace.stage.pty",
        ConnectionTraceStage::ShellReady => "connections.trace.stage.shell_ready",
        ConnectionTraceStage::Ready => "connections.trace.stage.ready",
    }
}

pub(in crate::workspace) fn format_connection_trace_elapsed(ms: u64) -> String {
    if ms < 10_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}s", (ms + 500) / 1000)
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    fn notice(index: usize) -> TerminalNotice {
        TerminalNotice {
            title: format!("notice-{index}"),
            description: None,
            status_text: None,
            progress: None,
            variant: TerminalNoticeVariant::Default,
        }
    }

    #[gpui::test]
    fn fast_connection_removes_its_completed_card_immediately(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let event = |stage, status, progress| ConnectionTraceEvent {
            attempt_id: "fast-attempt".to_string(),
            node_id: NodeId::new("fast-node"),
            stage,
            status,
            progress,
            elapsed_ms: 0,
            detail: None,
            label: None,
            endpoint: None,
            step_index: Some(1),
            total_steps: Some(1),
            mode: ConnectionTraceMode::Connect,
        };

        overlay.update(cx, |overlay, _cx| {
            let now = Instant::now();
            overlay.apply_connection_trace_events(
                vec![
                    event(
                        ConnectionTraceStage::Queued,
                        ConnectionTraceStatus::Running,
                        5.0,
                    ),
                    event(
                        ConnectionTraceStage::Ready,
                        ConnectionTraceStatus::Ready,
                        100.0,
                    ),
                ],
                now,
            );

            assert!(!overlay.connection_trace_cards.contains_key("fast-attempt"));
        });
    }

    #[gpui::test]
    fn dismissed_connection_card_stays_hidden_until_the_attempt_finishes(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let event = |stage, status, progress| ConnectionTraceEvent {
            attempt_id: "dismissed-attempt".to_string(),
            node_id: NodeId::new("dismissed-node"),
            stage,
            status,
            progress,
            elapsed_ms: 0,
            detail: None,
            label: None,
            endpoint: None,
            step_index: Some(1),
            total_steps: Some(1),
            mode: ConnectionTraceMode::Connect,
        };

        overlay.update(cx, |overlay, _cx| {
            let now = Instant::now();
            overlay.apply_connection_trace_events(
                vec![event(
                    ConnectionTraceStage::Queued,
                    ConnectionTraceStatus::Running,
                    5.0,
                )],
                now,
            );
            assert!(overlay.dismiss_connection_trace("dismissed-attempt", now));

            overlay.apply_connection_trace_events(
                vec![event(
                    ConnectionTraceStage::Authentication,
                    ConnectionTraceStatus::Running,
                    72.0,
                )],
                now,
            );
            assert!(
                !overlay
                    .connection_trace_cards
                    .contains_key("dismissed-attempt")
            );

            overlay.apply_connection_trace_events(
                vec![event(
                    ConnectionTraceStage::Ready,
                    ConnectionTraceStatus::Ready,
                    100.0,
                )],
                now,
            );
            assert!(overlay.dismissed_connection_traces.is_empty());
        });
    }

    #[gpui::test]
    fn hidden_notice_delivery_continues_across_budgets(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let sender = cx.read(|cx| overlay.read(cx).notice_sender());
        for index in 0..130 {
            sender.send(notice(index)).expect("overlay notice");
        }

        cx.run_until_parked();

        cx.read(|cx| {
            let overlay = overlay.read(cx);
            assert_eq!(overlay.standard_toasts.len(), 130);
            assert_eq!(overlay.standard_toasts[0].notice.title, "notice-0");
            assert_eq!(overlay.standard_toasts[129].notice.title, "notice-129");
        });
    }

    #[gpui::test]
    fn stale_deadline_generation_does_not_hide_newer_hud(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let stale_generation = overlay.update(cx, |overlay, cx| {
            overlay.apply_intent(
                WorkspaceOverlayIntent::ShowTerminalFontSizeHud {
                    font_size: 14,
                    ttl: Duration::from_secs(60),
                },
                cx,
            );
            overlay.deadline_generation
        });
        overlay.update(cx, |overlay, cx| {
            overlay.apply_intent(
                WorkspaceOverlayIntent::ShowTerminalFontSizeHud {
                    font_size: 18,
                    ttl: Duration::from_secs(60),
                },
                cx,
            );
            overlay.handle_deadline_generation(
                stale_generation,
                Instant::now() + Duration::from_secs(120),
                cx,
            );
            assert_eq!(
                overlay.terminal_font_size_hud.map(|hud| hud.font_size),
                Some(18)
            );
        });
    }

    #[gpui::test]
    fn entity_release_stops_notice_delivery(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::from_millis(120), cx));
        let wake = cx.read(|cx| overlay.read(cx).notice_wake());

        drop(overlay);
        cx.update(|_cx| {});

        assert!(wake.is_stopped());
    }

    #[gpui::test]
    fn confirm_reopen_cancels_stale_exit_and_replaces_the_payload(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        overlay.update(cx, |overlay, cx| {
            overlay.open_confirm(WorkspaceOverlayConfirmKind::LegalNotice, cx);
            assert_eq!(
                overlay.begin_confirm_exit(false, Duration::from_secs(60), cx),
                (true, None)
            );
            assert!(overlay.confirm_exit_task.is_some());

            overlay.open_confirm(WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes, cx);
            assert!(overlay.confirm_exit_task.is_none());
            assert_eq!(
                overlay.confirm_snapshot(),
                Some(WorkspaceOverlayConfirmSnapshot {
                    kind: WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes,
                    phase: oxideterm_gpui_ui::motion::ExitPhase::Visible,
                    focused_action: None,
                })
            );
        });
    }

    #[gpui::test]
    fn confirm_keys_publish_each_typed_effect_at_most_once(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        overlay.update(cx, |overlay, cx| {
            overlay.open_confirm(
                WorkspaceOverlayConfirmKind::NodeDisconnect {
                    node_id: NodeId("node-a".to_string()),
                    display_name: Arc::from("Node A"),
                },
                cx,
            );
            assert_eq!(
                overlay.handle_confirm_key("escape", false, false, cx),
                Some(WorkspaceOverlayConfirmKeyAction::Cancel)
            );
            assert_eq!(
                overlay.begin_confirm_exit(false, Duration::ZERO, cx),
                (true, None)
            );

            overlay.open_confirm(WorkspaceOverlayConfirmKind::SettingsReset, cx);
            assert_eq!(
                overlay.handle_confirm_key("end", false, false, cx),
                Some(WorkspaceOverlayConfirmKeyAction::Handled)
            );
            assert_eq!(
                overlay.handle_confirm_key("enter", false, false, cx),
                Some(WorkspaceOverlayConfirmKeyAction::Confirm)
            );
            assert_eq!(
                overlay.begin_confirm_exit(true, Duration::ZERO, cx),
                (true, Some(WorkspaceOverlayConfirmEffect::ResetSettings))
            );
            assert_eq!(
                overlay.begin_confirm_exit(true, Duration::ZERO, cx),
                (false, None)
            );
        });
    }

    #[gpui::test]
    fn entity_release_cancels_retained_confirm_exit(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
        overlay.update(cx, |overlay, cx| {
            // The task is retained by the confirmation owner, so releasing the
            // Entity must drop the pending receiver.
            overlay.confirm_exit_task = Some(cx.spawn(async move |_, _| {
                let _ = release_receiver.await;
            }));
        });
        cx.run_until_parked();

        drop(overlay);
        cx.update(|_| {});
        cx.run_until_parked();

        assert!(release_sender.send(()).is_err());
    }
}
