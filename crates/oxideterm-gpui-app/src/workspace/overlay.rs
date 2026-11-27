// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::{Task, WeakEntity, point};
use oxideterm_gpui_settings_view::{SettingsSelectAnchorExt, settings_slider_anchor_id};
use oxideterm_gpui_shell::overlays::transients::*;
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId};

impl WorkspaceApp {
    pub(in crate::workspace) fn update_settings_select_anchor(
        &mut self,
        window_id: gpui::WindowId,
        anchor: OverlayAnchor,
        cx: &mut Context<Self>,
    ) {
        let is_open_select_in_other_window = self
            .open_settings_select
            .is_some_and(|select| select.anchor_id() == anchor.id)
            && self.open_settings_select_owner_window_id != Some(window_id);
        self.settings_select_anchors
            .insert((window_id, anchor.id), anchor);
        if is_open_select_in_other_window {
            // The popup is mounted in the owning native window. Ignore a
            // coincidental repaint from another window instead of replacing
            // its coordinate space with foreign bounds.
            return;
        }
        self.update_select_anchor(anchor, cx);
    }

    pub(in crate::workspace) fn update_select_anchor(
        &mut self,
        anchor: OverlayAnchor,
        cx: &mut Context<Self>,
    ) {
        // Resolve new-connection anchors through their canonical mapping so a
        // newly added select cannot silently lose its root-mounted overlay.
        let should_notify = self
            .open_settings_select
            .is_some_and(|select| select.anchor_id() == anchor.id)
            || self
                .connection_form_state(cx)
                .open_select
                .is_some_and(|select| Self::new_connection_select_anchor_id(select) == anchor.id)
            || matches!(
                (self.cloud_sync.read(cx).view.open_select, anchor.id),
                (
                    Some(crate::workspace::cloud_sync::CloudSyncSelect::Backend),
                    SelectAnchorId::CloudSyncBackend
                ) | (
                    Some(crate::workspace::cloud_sync::CloudSyncSelect::AuthMode),
                    SelectAnchorId::CloudSyncAuthMode
                ) | (
                    Some(crate::workspace::cloud_sync::CloudSyncSelect::ConflictStrategy),
                    SelectAnchorId::CloudSyncConflictStrategy
                )
            )
            || (matches!(
                anchor.id,
                SelectAnchorId::AiPanelRoot
                    | SelectAnchorId::AiConversationList
                    | SelectAnchorId::AiChatMenu
                    | SelectAnchorId::AiModelSelector
                    | SelectAnchorId::AiInlineModelSelector
                    | SelectAnchorId::AiReasoningMenu
                    | SelectAnchorId::AiSafetyMenu
                    | SelectAnchorId::AiContextPopover
                    | SelectAnchorId::AiAutocomplete
            ) && self.has_ai_sidebar_floating_overlay(cx))
            || (anchor.id == SelectAnchorId::TerminalBroadcastMenu
                && self.terminal.read(cx).broadcast_menu_open())
            || (anchor.id == SelectAnchorId::TerminalCommandBar
                && self.terminal.read(cx).quick_commands.is_open())
            || (anchor.id == SelectAnchorId::TerminalCwdMenu
                && self.terminal.read(cx).cwd_picker_open())
            || (anchor.id == SelectAnchorId::TerminalGitBranchMenu
                && self.terminal.read(cx).git_panel_open())
            || (anchor.id == SelectAnchorId::TerminalProjectMenu
                && self.terminal.read(cx).project_panel_open())
            || (anchor.id == SelectAnchorId::SessionManagerViewMode
                && self
                    .connections
                    .session_manager
                    .read(cx)
                    .view_mode_menu_open)
            || (anchor.id == SelectAnchorId::ActiveSessionSort && self.session_sort_menu_open)
            || (anchor.id == SelectAnchorId::SessionManagerSort
                && self.connections.session_manager.read(cx).sort_menu_open)
            || (anchor.id == SelectAnchorId::SessionManagerBatchMove
                && self.connections.session_manager.read(cx).show_batch_move)
            || (anchor.id == SelectAnchorId::SessionManagerBatchTemplate
                && self
                    .connections
                    .session_manager
                    .read(cx)
                    .batch_add
                    .as_ref()
                    .is_some_and(|dialog| dialog.template_menu_open))
            || (anchor.id == SelectAnchorId::SessionManagerBatchKeyAuth && {
                // The key menu only owns a trigger while it is open; a
                // closed menu must not claim the anchor.
                let session_manager = self.connections.session_manager.read(cx);
                session_manager.batch_add_uses_key_auth()
                    && session_manager
                        .batch_add
                        .as_ref()
                        .is_some_and(|dialog| dialog.key_menu_open)
            })
            || self
                .settings_workspace
                .read(cx)
                .slider_drag()
                .is_some_and(|slider| settings_slider_anchor_id(slider) == anchor.id);
        if !should_notify && !select_anchor_tracks_while_closed(anchor.id) {
            self.select_anchors.remove(&anchor.id);
            return;
        }
        if self.select_anchors.get(&anchor.id) != Some(&anchor) {
            self.select_anchors.insert(anchor.id, anchor);
            if should_notify {
                cx.notify();
            }
        }
    }
}

pub(in crate::workspace) fn select_anchor_tracks_while_closed(anchor_id: SelectAnchorId) -> bool {
    // Browser/Radix selects can synchronously read their trigger rect on the
    // opening click. GPUI portals cannot, so modal select triggers keep a
    // closed-state anchor cache without notifying; that makes first-click open
    // immediate while scroll handlers still clear stale coordinates.
    if anchor_id.is_settings_select_trigger()
        || anchor_id.is_new_connection_select_trigger()
        || anchor_id.is_cloud_sync_select_trigger()
    {
        return true;
    }

    // Sliders and non-settings overlays also need an anchor before pointer-down
    // can open or drag them.
    matches!(
        anchor_id,
        SelectAnchorId::ActiveSessionSort
            | SelectAnchorId::SettingsAppearanceUiFontSizeSlider
            | SelectAnchorId::SettingsAppearanceBorderRadiusSlider
            | SelectAnchorId::OnboardingBorderRadiusSlider
            | SelectAnchorId::SettingsAppearanceWindowOpacitySlider
            | SelectAnchorId::VersionMigrationBorderRadiusSlider
            | SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider
            | SelectAnchorId::SettingsAppearanceBackgroundBlurSlider
            | SelectAnchorId::SettingsBackgroundSurfaceOpacity
            | SelectAnchorId::SettingsBackgroundReadability
            | SelectAnchorId::SettingsBackgroundEffectStrength
            | SelectAnchorId::SettingsBackgroundEffectSheen
            | SelectAnchorId::SettingsBackgroundEffectSpeed
            | SelectAnchorId::SettingsBackgroundEffectSize
            | SelectAnchorId::SettingsBackgroundEffectBrightness
            | SelectAnchorId::SettingsBackgroundEffectRoughness
            | SelectAnchorId::SettingsBackgroundEffectDirection
            | SelectAnchorId::SettingsBackgroundParticleCount
            | SelectAnchorId::SettingsBackgroundCameraAmount
            | SelectAnchorId::SettingsBackgroundCameraSpeed
            | SelectAnchorId::SettingsTerminalFontSizeSlider
            | SelectAnchorId::AiPanelRoot
            | SelectAnchorId::AiConversationList
            | SelectAnchorId::AiChatMenu
            | SelectAnchorId::AiModelSelector
            | SelectAnchorId::AiInlineModelSelector
            | SelectAnchorId::AiReasoningMenu
            | SelectAnchorId::AiSafetyMenu
            | SelectAnchorId::AiContextPopover
            | SelectAnchorId::AiAutocomplete
            | SelectAnchorId::NewConnectionGroup
            | SelectAnchorId::NewConnectionKeyAuthSource
            | SelectAnchorId::NewConnectionManagedKey
            | SelectAnchorId::NewConnectionTotp
            | SelectAnchorId::NewConnectionJumpTotp
            | SelectAnchorId::NewConnectionJumpSavedConnection
            | SelectAnchorId::NewConnectionRemoteDesktopSshGateway
            | SelectAnchorId::NewConnectionJumpKeyAuthSource
            | SelectAnchorId::NewConnectionJumpManagedKey
            | SelectAnchorId::NewConnectionSerialPort
            | SelectAnchorId::NewConnectionSerialDataBits
            | SelectAnchorId::NewConnectionSerialStopBits
            | SelectAnchorId::NewConnectionSerialParity
            | SelectAnchorId::NewConnectionSerialFlowControl
            | SelectAnchorId::IdeAgentStatus
            // Broadcast targets are rendered through the root backdrop, but
            // Tauri/Radix positions them from the trigger button. Keep the
            // closed trigger rect warm so the first pointer-down opens at the
            // command-bar/tabbar button even when the AI sidebar changes root width.
            | SelectAnchorId::TerminalBroadcastMenu
            // Quick Commands uses Tauri's `min(860px, calc(100% - 1.5rem))`
            // width against the command bar. Keep the bar rect warm so the
            // first open and later resizes can compute the same adaptive width.
            | SelectAnchorId::TerminalCommandBar
            | SelectAnchorId::TerminalCwdMenu
            | SelectAnchorId::TerminalGitBranchMenu
            | SelectAnchorId::TerminalProjectMenu
            | SelectAnchorId::TerminalCastSeekbar
            | SelectAnchorId::RemoteDesktopResizeMenu(_)
            // Session Manager toolbar menus use window-anchored overlays, so
            // their trigger bounds must be cached before pointer-down.
            | SelectAnchorId::SessionManagerViewMode
            | SelectAnchorId::SessionManagerSort
            | SelectAnchorId::SessionManagerSidebarSort
            | SelectAnchorId::SessionManagerBatchMove
    )
}

const WORKSPACE_NOTICE_TTL: Duration = Duration::from_secs(4);
const PLUGIN_REQUIRED_NOTICE_TTL: Duration = Duration::from_secs(12);

const CONNECTION_TRACE_UPDATE_COALESCE: Duration = Duration::from_millis(45);
const CONNECTION_TRACE_SUCCESS_TTL: Duration = Duration::from_millis(720);
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
    PluginRequired {
        notice: TerminalNotice,
        plugin_id: String,
        label: String,
        workspace: WeakEntity<WorkspaceApp>,
    },
    ShellIntegrationReady {
        notice: TerminalNotice,
        node_id: NodeId,
        label: String,
        workspace: WeakEntity<WorkspaceApp>,
    },
    AppearanceSetup {
        notice: TerminalNotice,
        scheme: Option<bool>,
        label: String,
        workspace: WeakEntity<WorkspaceApp>,
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
        label: String,
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
    CommandHistoryClear {
        all: bool,
    },
    LegalNotice,
    ThirdPartyNotices,
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
    ClearCommandHistory { all: bool },
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
    CommandHistoryClear,
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
struct NoticeAction {
    target: NoticeActionTarget,
    label: String,
    workspace: WeakEntity<WorkspaceApp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NoticeActionTarget {
    Plugin(String),
    SshTerminal(NodeId),
    AppearanceSettings(Option<bool>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalFontSizeHud {
    font_size: i64,
    generation: u64,
    expires_at: Instant,
}

struct ZenHint {
    label: String,
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

/// Owns every window-global transient overlay and its delivery/timer lifetime.
pub(in crate::workspace) struct WorkspaceOverlayEntity {
    notice_tx: delivery::ActiveDeliverySender<TerminalNotice>,
    notice_rx: std::sync::mpsc::Receiver<TerminalNotice>,
    _notice_wake: delivery::ActiveDeliveryWake,
    _notice_delivery_task: Task<()>,
    deadline_task: Option<Task<()>>,
    deadline_generation: u64,
    transients: TransientOverlays<TerminalNotice, NoticeAction>,
    connection_trace_cards: HashMap<String, ActiveConnectionCard>,
    dismissed_connection_traces: VecDeque<String>,
    zen_hint: Option<ZenHint>,
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
            transients: TransientOverlays::new(control_exit_duration),
            connection_trace_cards: HashMap::new(),
            dismissed_connection_traces: VecDeque::new(),
            zen_hint: None,
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
                self.transients
                    .push_notice(notice, ttl, None, cx.background_executor().now());
                true
            }
            WorkspaceOverlayIntent::PluginRequired {
                notice,
                plugin_id,
                label,
                workspace,
            } => {
                self.transients.push_notice(
                    notice,
                    PLUGIN_REQUIRED_NOTICE_TTL,
                    Some(NoticeAction {
                        target: NoticeActionTarget::Plugin(plugin_id),
                        label,
                        workspace,
                    }),
                    cx.background_executor().now(),
                );
                true
            }
            WorkspaceOverlayIntent::ShellIntegrationReady {
                notice,
                node_id,
                label,
                workspace,
            } => {
                self.transients.push_notice(
                    notice,
                    Duration::from_secs(12),
                    Some(NoticeAction {
                        target: NoticeActionTarget::SshTerminal(node_id),
                        label,
                        workspace,
                    }),
                    cx.background_executor().now(),
                );
                true
            }
            WorkspaceOverlayIntent::AppearanceSetup {
                notice,
                scheme,
                label,
                workspace,
            } => {
                self.transients.push_notice(
                    notice,
                    Duration::from_secs(12),
                    Some(NoticeAction {
                        target: NoticeActionTarget::AppearanceSettings(scheme),
                        label,
                        workspace,
                    }),
                    cx.background_executor().now(),
                );
                true
            }
            WorkspaceOverlayIntent::PluginProgress { key, notice, ttl } => {
                self.transients.upsert_plugin_progress(
                    key,
                    notice,
                    ttl,
                    cx.background_executor().now(),
                );
                true
            }
            WorkspaceOverlayIntent::DismissPluginProgress { key } => self
                .transients
                .dismiss_plugin_progress(&key, cx.background_executor().now()),
            WorkspaceOverlayIntent::ConnectionTraceEvents(events) => {
                self.apply_connection_trace_events(events, cx.background_executor().now())
            }
            WorkspaceOverlayIntent::QueueTooltip { id, label, x, y } => self
                .transients
                .queue_tooltip(id, label, x, y, cx.background_executor().now()),
            WorkspaceOverlayIntent::ClearTooltip { id } => self
                .transients
                .clear_tooltip(&id, cx.background_executor().now()),
            WorkspaceOverlayIntent::ClearAllTooltips => self.transients.clear_all_tooltips(),
            WorkspaceOverlayIntent::ShowZenHint { ttl, label } => {
                self.zen_hint = Some(ZenHint {
                    label,
                    expires_at: cx.background_executor().now() + ttl,
                });
                true
            }
            WorkspaceOverlayIntent::ClearZenHint => self.zen_hint.take().is_some(),
            WorkspaceOverlayIntent::ShowTerminalFontSizeHud { font_size, ttl } => {
                self.terminal_font_size_hud_generation =
                    self.terminal_font_size_hud_generation.wrapping_add(1);
                self.terminal_font_size_hud = Some(TerminalFontSizeHud {
                    font_size,
                    generation: self.terminal_font_size_hud_generation,
                    expires_at: cx.background_executor().now() + ttl,
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
        if self.transients.control_exit_duration != duration {
            if self.transients.set_exit_duration(duration) {
                cx.notify();
            }
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
                    WorkspaceOverlayConfirmKind::CommandHistoryClear { .. } => {
                        WorkspaceOverlayConfirmOwnerKind::CommandHistoryClear
                    }
                    WorkspaceOverlayConfirmKind::SettingsReset => {
                        WorkspaceOverlayConfirmOwnerKind::SettingsReset
                    }
                    WorkspaceOverlayConfirmKind::LegalNotice
                    | WorkspaceOverlayConfirmKind::ThirdPartyNotices => {
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
                | WorkspaceOverlayConfirmKind::CommandHistoryClear { .. }
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
                WorkspaceOverlayConfirmKind::CommandHistoryClear { all } => {
                    Some(WorkspaceOverlayConfirmEffect::ClearCommandHistory { all: *all })
                }
                WorkspaceOverlayConfirmKind::SettingsReset => {
                    Some(WorkspaceOverlayConfirmEffect::ResetSettings)
                }
                WorkspaceOverlayConfirmKind::NodeDisconnect { node_id, .. } => {
                    Some(WorkspaceOverlayConfirmEffect::DisconnectNode {
                        node_id: node_id.clone(),
                    })
                }
                WorkspaceOverlayConfirmKind::LegalNotice
                | WorkspaceOverlayConfirmKind::ThirdPartyNotices
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
        let timer = cx.background_executor().timer(delay);
        self.confirm_exit_task = Some(cx.spawn(async move |overlay, cx| {
            timer.await;
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
                self.transients.push_notice(
                    notice,
                    WORKSPACE_NOTICE_TTL,
                    None,
                    cx.background_executor().now(),
                );
            }
            self.schedule_next_deadline(cx);
            cx.notify();
        }
        batch.outcome.backlog_remaining
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
                if matches!(
                    event.status,
                    ConnectionTraceStatus::Ready | ConnectionTraceStatus::Cancelled
                ) {
                    self.dismissed_connection_traces.remove(position);
                }
                continue;
            }
            if !self.connection_trace_cards.contains_key(&attempt_id) {
                // A node owns one visible connection trace. A retry replaces the
                // failed physical attempt instead of accumulating another card.
                let superseded_attempt_ids = self
                    .connection_trace_cards
                    .iter()
                    .filter(|(_, trace)| trace.latest.node_id == event.node_id)
                    .map(|(attempt_id, _)| attempt_id.clone())
                    .collect::<Vec<_>>();
                for superseded_attempt_id in superseded_attempt_ids {
                    self.connection_trace_cards.remove(&superseded_attempt_id);
                    self.dismissed_connection_traces
                        .retain(|dismissed| dismissed != &superseded_attempt_id);
                }
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
                        // Scheduling the deadline is the only way `displayed` catches up
                        // with `latest`; without marking the intent as changed the owner
                        // never arms the timer, leaving the stage path frozen on the
                        // first stage while the history list keeps updating.
                        changed = true;
                    }
                }
                ConnectionTraceStatus::Ready => {
                    trace.flush_deadline = None;
                    let mut success = event;
                    success.elapsed_ms = now
                        .saturating_duration_since(trace.started_at)
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

    fn begin_trace_exit(&mut self, attempt_id: &str, now: Instant) -> bool {
        let Some(trace) = self.connection_trace_cards.get_mut(attempt_id) else {
            return false;
        };
        if trace.presence.begin_exit().is_none() {
            return false;
        }
        trace.expires_at = None;
        if self.transients.control_exit_duration.is_zero() {
            self.connection_trace_cards.remove(attempt_id);
        } else {
            trace.remove_at = Some(now + self.transients.control_exit_duration);
        }
        true
    }

    fn dismiss_connection_trace(&mut self, attempt_id: &str, now: Instant) -> bool {
        let Some(trace) = self.connection_trace_cards.get(attempt_id) else {
            return false;
        };
        // Keep a bounded workflow tombstone so retries and late progress cannot
        // reopen a card without cancelling its runtime-owned connection job.
        if !matches!(
            trace.latest.status,
            ConnectionTraceStatus::Ready | ConnectionTraceStatus::Cancelled
        ) && !self
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
        self.deadline_task = Some(at_deadline(deadline, cx, move |overlay, now, cx| {
            overlay.handle_deadline_generation(generation, now, cx)
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
        let mut changed = self.transients.process_due_deadlines(now);
        if self
            .zen_hint
            .as_ref()
            .is_some_and(|hint| hint.expires_at <= now)
        {
            self.zen_hint = None;
            changed = true;
        }
        if self
            .terminal_font_size_hud
            .is_some_and(|hud| hud.expires_at <= now)
        {
            self.terminal_font_size_hud = None;
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
        let mut next = self.transients.next_deadline();
        next = min_deadline(next, self.zen_hint.as_ref().map(|hint| hint.expires_at));
        next = min_deadline(next, self.terminal_font_size_hud.map(|hud| hud.expires_at));
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
        native_update: AnyElement,
        show_connection_cards: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut layers = Vec::new();
        layers.extend(self.transients.render_tooltip(tokens));
        if let Some(hint) = &self.zen_hint
            && hint.expires_at > cx.background_executor().now()
        {
            layers.push(render_zen_hint(tokens, &hint.label));
        }
        layers.push(self.render_toasts(tokens, native_update, cx));
        // Connection cards use a deferred layer and must yield while the MFA dialog owns input.
        if show_connection_cards {
            if let Some(connection_cards) = self.render_connection_cards(tokens, i18n, cx) {
                layers.push(connection_cards);
            }
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
        native_update: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.weak_entity();
        self.transients
            .render_toasts(
                tokens,
                |notice| ToastContent {
                    title: notice.title.clone(),
                    description: notice.description.clone(),
                    status_text: notice.status_text.clone(),
                    progress: notice.progress,
                    variant: toast_variant_from_terminal(notice.variant),
                },
                |action| action.label.clone(),
                Rc::new(move |request, window, cx| {
                    let action = owner
                        .update(cx, |this, cx| {
                            let now = cx.background_executor().now();
                            let action = match &request {
                                ToastRequest::Invoke(id) => this.transients.action(*id).cloned(),
                                _ => None,
                            };
                            let changed = match request {
                                ToastRequest::Invoke(id) | ToastRequest::Dismiss(id) => {
                                    this.transients.begin_standard_exit(id, now)
                                }
                                ToastRequest::DismissProgress(key) => {
                                    this.transients.dismiss_plugin_progress(&key, now)
                                }
                            };
                            if changed {
                                this.schedule_next_deadline(cx);
                                cx.notify();
                            }
                            action
                        })
                        .ok()
                        .flatten();
                    if let Some(action) = action {
                        let _ = action
                            .workspace
                            .update(cx, |workspace, cx| match action.target {
                                NoticeActionTarget::Plugin(id) => {
                                    workspace.open_required_plugin(&id, window, cx)
                                }
                                NoticeActionTarget::SshTerminal(node) => {
                                    let _ = workspace
                                        .queue_ssh_terminal_tab_for_sidebar_node(node, window, cx);
                                }
                                NoticeActionTarget::AppearanceSettings(scheme) => {
                                    workspace.open_appearance_settings(scheme, window, cx)
                                }
                            });
                    }
                }),
            )
            .child(native_update)
            .into_any_element()
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
                    .top(px(
                        tokens.metrics.titlebar_height + tokens.metrics.tabbar_height
                    ))
                    .right_0()
                    .flex()
                    .flex_col()
                    .gap(px(tokens.spacing.three))
                    .p(px(tokens.metrics.ui_toast_padding))
                    .w(px(tokens.metrics.ui_toast_width))
                    .max_w(px(tokens.metrics.ui_toast_width))
                    .children(cards),
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
    let theme = tokens.ui;
    let ready = event.status == ConnectionTraceStatus::Ready;
    let failed = event.status == ConnectionTraceStatus::Failed;
    let status_color = if ready {
        theme.success
    } else if failed {
        theme.error
    } else {
        theme.accent
    };
    let transport = connection_trace_transport(event.stage);
    let transport_icon = match transport {
        TraceTransport::Ssh => "lucide/terminal.svg",
        TraceTransport::Ble => "lucide/bluetooth.svg",
    };
    let title = connection_trace_title(i18n, event);
    let status_text = (!failed).then(|| connection_trace_status_text(i18n, event));
    let endpoint = event.endpoint.clone();
    let elapsed_badge =
        (event.elapsed_ms > 0).then(|| format_connection_trace_elapsed(event.elapsed_ms));
    let attempt_id_for_close = attempt_id.to_string();

    let (stages, current) = connection_trace_stage_list(event.stage, history);
    let step_total = stages.len();
    let step_current = if ready {
        step_total
    } else {
        (current + 1).min(step_total)
    };
    let top_progress = if ready {
        1.0
    } else if failed {
        ((current as f32 + 0.5) / step_total.max(1) as f32).clamp(0.12, 0.92)
    } else {
        (event.progress / 100.0).clamp(0.08, 0.96)
    };

    // Reserve dedicated header space for close so long hostnames truncate cleanly.
    let close = oxideterm_gpui_ui::icon_button(
        tokens,
        svg()
            .path("lucide/x.svg")
            .size(px(13.0))
            .text_color(rgb(theme.text_muted))
            .into_any_element(),
        oxideterm_gpui_ui::IconButtonOptions::opaque_toolbar(
            24.0,
            oxideterm_gpui_ui::button::ButtonRadius::Sm,
        ),
    )
    .flex_none()
    .self_start()
    .debug_selector(|| "connection-card-close".to_string())
    .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
        let _ = overlay.update(cx, |overlay, cx| {
            if overlay
                .dismiss_connection_trace(&attempt_id_for_close, cx.background_executor().now())
            {
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
        .gap(px(10.0))
        .child(
            div()
                .size(px(28.0))
                .flex_none()
                .rounded(px(tokens.radii.md))
                .bg(rgba((status_color << 8) | 0x1a))
                .border_1()
                .border_color(rgba((status_color << 8) | 0x45))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(transport_icon)
                        .size(px(14.0))
                        .text_color(rgb(status_color)),
                ),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .debug_selector(|| "connection-card-heading".to_string())
                .overflow_hidden()
                .flex()
                .flex_col()
                .gap(px(1.5))
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.0))
                        .line_height(px(17.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_heading))
                        .child(title),
                )
                .when_some(endpoint, |header, endpoint| {
                    header.child(
                        div()
                            .truncate()
                            .font_family(tokens.metrics.markdown_code_font_family)
                            .text_size(px(11.0))
                            .line_height(px(14.0))
                            .text_color(rgb(theme.text_muted))
                            .child(endpoint),
                    )
                }),
        )
        .when_some(elapsed_badge, |row, elapsed| {
            row.child(
                div()
                    .flex_none()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(tokens.radii.xs))
                    .bg(rgba(
                        (if ready { theme.success } else { theme.border } << 8)
                            | if ready { 0x1e } else { 0x33 },
                    ))
                    .font_family(tokens.metrics.markdown_code_font_family)
                    .text_size(px(10.5))
                    .line_height(px(13.0))
                    .text_color(rgb(if ready {
                        theme.success
                    } else {
                        theme.text_muted
                    }))
                    .child(elapsed),
            )
        })
        .child(close);

    let top_rail = div()
        .w_full()
        .h(px(2.0))
        .bg(rgba((theme.border << 8) | 0x40))
        .child(
            div()
                .h_full()
                .w(relative(top_progress))
                .bg(rgb(status_color)),
        );

    let mut body = div()
        .w_full()
        .px(px(14.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(header)
        .child(render_connection_stage_path(
            tokens, i18n, attempt_id, event, history,
        ));

    if let Some(error_log) = render_connection_error_log(tokens, event) {
        body = body.child(error_log);
    } else if let Some(status_line) = status_text {
        body = body.child(
            div()
                .w_full()
                .pt(px(8.0))
                .border_t_1()
                .border_color(rgba((theme.border << 8) | 0x45))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .size(px(5.0))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(status_color)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_size(px(11.5))
                                .line_height(px(15.0))
                                .text_color(rgb(if ready {
                                    theme.success
                                } else {
                                    theme.text_secondary
                                }))
                                .child(status_line),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .font_family(tokens.metrics.markdown_code_font_family)
                        .text_size(px(10.5))
                        .text_color(rgb(theme.text_muted))
                        .child(format!("{step_current}/{step_total}")),
                ),
        );
    }

    let card = oxideterm_gpui_ui::semantic_surface(
        tokens,
        oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::TerminalOverlay)
            .padding(oxideterm_gpui_ui::SurfacePadding::None),
    )
    .w_full()
    .overflow_hidden()
    .debug_selector(|| "connection-card".to_string())
    .flex()
    .flex_col()
    .occlude()
    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
        cx.stop_propagation();
    })
    .child(top_rail)
    .child(body);

    oxideterm_gpui_ui::motion::fade(
        tokens,
        format!("connection-card-{attempt_id}"),
        card,
        oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        phase == oxideterm_gpui_ui::motion::ExitPhase::Visible,
    )
}

// Attempts that skipped the scan window render a shorter path so no dead
// node lingers for the scan stages they never entered.
const BLE_DIRECT_STAGES: [(ConnectionTraceStage, &str); 3] = [
    (
        ConnectionTraceStage::BleOpeningAdapter,
        "connections.trace.stage.ble_opening_adapter",
    ),
    (
        ConnectionTraceStage::BleDirectConnect,
        "connections.trace.stage.ble_direct_connect",
    ),
    (
        ConnectionTraceStage::BleVerifyingService,
        "connections.trace.stage.ble_verifying_service",
    ),
];

const BLE_SCAN_STAGES: [(ConnectionTraceStage, &str); 4] = [
    (
        ConnectionTraceStage::BleOpeningAdapter,
        "connections.trace.stage.ble_opening_adapter",
    ),
    (
        ConnectionTraceStage::BleScanning,
        "connections.trace.stage.ble_scanning",
    ),
    (
        ConnectionTraceStage::BleMatching,
        "connections.trace.stage.ble_matching",
    ),
    (
        ConnectionTraceStage::BleVerifyingService,
        "connections.trace.stage.ble_verifying_service",
    ),
];

const SSH_STAGES: [(ConnectionTraceStage, &str); 4] = [
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

fn connection_trace_stage_list(
    event_stage: ConnectionTraceStage,
    history: &VecDeque<ConnectionTraceEvent>,
) -> (&'static [(ConnectionTraceStage, &'static str)], usize) {
    match connection_trace_transport(event_stage) {
        TraceTransport::Ble => ble_stage_path(event_stage, history),
        TraceTransport::Ssh => (&SSH_STAGES, connection_trace_path_index(event_stage)),
    }
}

/// BLE stage vocabulary and current-node index for an attempt. Split from the
/// renderer so the direct/scan fallback rules are testable without gpui.
fn ble_stage_path(
    event_stage: ConnectionTraceStage,
    history: &VecDeque<ConnectionTraceEvent>,
) -> (&'static [(ConnectionTraceStage, &'static str)], usize) {
    // Once the attempt leaves the direct stage for a scan stage, the path
    // falls back to the scan vocabulary.
    let direct_path = event_stage == ConnectionTraceStage::BleDirectConnect
        || (history
            .iter()
            .any(|entry| entry.stage == ConnectionTraceStage::BleDirectConnect)
            && !matches!(
                event_stage,
                ConnectionTraceStage::BleScanning | ConnectionTraceStage::BleMatching
            ));
    let stages: &'static [(ConnectionTraceStage, &'static str)] = if direct_path {
        &BLE_DIRECT_STAGES
    } else {
        &BLE_SCAN_STAGES
    };
    // Ready is past the last node so every node renders completed.
    let current = stages
        .iter()
        .position(|(stage, _)| *stage == event_stage)
        .unwrap_or(stages.len());
    (stages, current)
}

fn trace_hsla(hex: u32, alpha: f32) -> gpui::Hsla {
    let mut color: gpui::Hsla = rgb(hex).into();
    color.a = alpha.clamp(0.0, 1.0);
    color
}

fn trace_circle_fill(
    center: gpui::Point<gpui::Pixels>,
    radius: f32,
) -> Option<gpui::Path<gpui::Pixels>> {
    if radius <= 0.1 {
        return None;
    }
    let mut builder = gpui::PathBuilder::fill();
    let cx = f32::from(center.x);
    let cy = f32::from(center.y);
    let r = px(radius);
    builder.move_to(point(px(cx + radius), px(cy)));
    builder.arc_to(
        point(r, r),
        px(0.0),
        false,
        true,
        point(px(cx - radius), px(cy)),
    );
    builder.arc_to(
        point(r, r),
        px(0.0),
        false,
        true,
        point(px(cx + radius), px(cy)),
    );
    builder.close();
    builder.build().ok()
}

fn trace_circle_ring(
    center: gpui::Point<gpui::Pixels>,
    radius: f32,
    stroke_w: f32,
) -> Option<gpui::Path<gpui::Pixels>> {
    if radius <= 0.1 {
        return None;
    }
    let mut builder = gpui::PathBuilder::stroke(px(stroke_w));
    let cx = f32::from(center.x);
    let cy = f32::from(center.y);
    let r = px(radius);
    builder.move_to(point(px(cx + radius), px(cy)));
    builder.arc_to(
        point(r, r),
        px(0.0),
        false,
        true,
        point(px(cx - radius), px(cy)),
    );
    builder.arc_to(
        point(r, r),
        px(0.0),
        false,
        true,
        point(px(cx + radius), px(cy)),
    );
    builder.build().ok()
}

fn paint_connection_signal_track(
    tokens: &ThemeTokens,
    stage_count: usize,
    current: usize,
    ready: bool,
    failed: bool,
    phase: f32,
    window: &mut Window,
    bounds: gpui::Bounds<gpui::Pixels>,
) {
    let count = stage_count.max(2);
    let theme = tokens.ui;
    let light = theme.is_light();
    let left = f32::from(bounds.origin.x);
    let width = f32::from(bounds.size.width).max(1.0);
    let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) * 0.5;

    let node_x = |index: usize| -> f32 { left + ((index as f32 + 0.5) / count as f32) * width };
    let first_x = node_x(0);
    let last_x = node_x(count - 1);
    let active_idx = current.min(count - 1);
    let active_color = if failed {
        theme.error
    } else if ready {
        theme.success
    } else {
        theme.accent
    };

    // Baseline recessed optical track connecting all nodes.
    window.paint_quad(gpui::fill(
        gpui::Bounds::new(
            point(px(first_x), px(cy - 1.0)),
            gpui::size(px((last_x - first_x).max(0.0)), px(2.0)),
        ),
        trace_hsla(theme.border, if light { 0.55 } else { 0.42 }),
    ));

    // Lit completed rail up to the active node (or full span when Ready).
    let lit_end_x = if ready { last_x } else { node_x(active_idx) };
    if lit_end_x > first_x + 0.5 {
        let rail_color = if ready {
            theme.success
        } else if failed {
            theme.error
        } else {
            theme.accent
        };
        // Soft ambient under-glow along the energized rail.
        window.paint_quad(gpui::fill(
            gpui::Bounds::new(
                point(px(first_x), px(cy - 3.0)),
                gpui::size(px(lit_end_x - first_x), px(6.0)),
            ),
            trace_hsla(rail_color, if light { 0.14 } else { 0.18 }),
        ));
        window.paint_quad(gpui::fill(
            gpui::Bounds::new(
                point(px(first_x), px(cy - 1.1)),
                gpui::size(px(lit_end_x - first_x), px(2.2)),
            ),
            trace_hsla(rail_color, 0.92),
        ));
    }

    // Live traveling signal comet along the frontier segment while connecting,
    // or a single emerald confirmation shimmer sweeping the whole track on Ready.
    let motion_active = tokens.motion.enabled && tokens.motion.spatial_enabled;
    if !failed && motion_active {
        let (seg_start, seg_end, comet_color) = if ready {
            (first_x, last_x, theme.success)
        } else if active_idx + 1 < count {
            (node_x(active_idx), node_x(active_idx + 1), theme.accent)
        } else {
            (
                node_x(active_idx.saturating_sub(1)),
                node_x(active_idx),
                theme.accent,
            )
        };
        let seg_len = (seg_end - seg_start).max(1.0);
        let travel = oxideterm_gpui_ui::motion::ease_in_out_cubic(phase);
        let head_x = seg_start + seg_len * travel;
        let tail_len = (seg_len * 0.42).min(48.0);
        // 8-step fading luminous comet tail.
        let steps = 8usize;
        for step in 0..steps {
            let f0 = step as f32 / steps as f32;
            let f1 = (step + 1) as f32 / steps as f32;
            let x0 = (head_x - tail_len * f1).max(seg_start);
            let x1 = (head_x - tail_len * f0).max(seg_start);
            if x1 > x0 + 0.25 {
                let fade = (1.0 - f0).powi(2) * (1.0 - (travel - 0.5).abs() * 0.7).max(0.2);
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(
                        point(px(x0), px(cy - 1.25)),
                        gpui::size(px(x1 - x0), px(2.5)),
                    ),
                    trace_hsla(comet_color, fade * 0.85),
                ));
            }
        }
        // Bright specular pulse head.
        let head_alpha = (1.0 - (travel - 0.5).abs() * 1.4).clamp(0.0, 1.0);
        if head_alpha > 0.05 {
            if let Some(halo) = trace_circle_fill(point(px(head_x), px(cy)), 4.5) {
                window.paint_path(halo, trace_hsla(comet_color, head_alpha * 0.34));
            }
            if let Some(core) = trace_circle_fill(point(px(head_x), px(cy)), 2.2) {
                window.paint_path(core, trace_hsla(0xffffff, head_alpha * 0.92));
            }
        }
    }

    // Paint each precision stage node.
    for index in 0..count {
        let cx = node_x(index);
        let center = point(px(cx), px(cy));
        let completed = ready || index < current;
        let active = !ready && index == active_idx;

        if active && !failed {
            // Dual concentric sonar radar rings on the active handshake node.
            if motion_active {
                let p1 = oxideterm_gpui_ui::motion::ease_out_cubic(phase);
                let r1 = oxideterm_gpui_ui::motion::lerp(6.5, 16.0, p1);
                if let Some(ring1) = trace_circle_ring(center, r1, 1.35) {
                    window.paint_path(ring1, trace_hsla(active_color, (1.0 - p1) * 0.48));
                }
                let p2 = oxideterm_gpui_ui::motion::ease_out_cubic((phase + 0.5) % 1.0);
                let r2 = oxideterm_gpui_ui::motion::lerp(6.5, 15.0, p2);
                if let Some(ring2) = trace_circle_ring(center, r2, 1.0) {
                    window.paint_path(ring2, trace_hsla(active_color, (1.0 - p2) * 0.28));
                }
            }
            // Soft ambient halo + crisp bezel + bright inner spark.
            if let Some(halo) = trace_circle_fill(center, 10.0) {
                window.paint_path(
                    halo,
                    trace_hsla(active_color, if light { 0.16 } else { 0.22 }),
                );
            }
            if let Some(base) = trace_circle_fill(center, 6.5) {
                window.paint_path(base, trace_hsla(theme.bg_elevated, 1.0));
            }
            if let Some(ring) = trace_circle_ring(center, 6.5, 1.6) {
                window.paint_path(ring, trace_hsla(active_color, 0.95));
            }
            if let Some(core) = trace_circle_fill(center, 3.2) {
                window.paint_path(core, trace_hsla(active_color, 1.0));
            }
            if let Some(glint) = trace_circle_fill(point(px(cx - 0.8), px(cy - 0.8)), 1.1) {
                window.paint_path(glint, trace_hsla(0xffffff, 0.85));
            }
        } else if active && failed {
            // Crisp crimson fault beacon with vector × mark.
            if let Some(halo) = trace_circle_fill(center, 11.0) {
                window.paint_path(halo, trace_hsla(theme.error, 0.20));
            }
            if let Some(disc) = trace_circle_fill(center, 6.8) {
                window.paint_path(disc, trace_hsla(theme.error, 1.0));
            }
            let mut cross = gpui::PathBuilder::stroke(px(1.6));
            cross.move_to(point(px(cx - 2.6), px(cy - 2.6)));
            cross.line_to(point(px(cx + 2.6), px(cy + 2.6)));
            cross.move_to(point(px(cx + 2.6), px(cy - 2.6)));
            cross.line_to(point(px(cx - 2.6), px(cy + 2.6)));
            if let Ok(path) = cross.build() {
                window.paint_path(path, trace_hsla(0xffffff, 0.96));
            }
        } else if completed {
            // Completed emerald jewel node with crisp vector checkmark.
            let done_color = theme.success;
            if let Some(halo) = trace_circle_fill(center, 8.5) {
                window.paint_path(
                    halo,
                    trace_hsla(done_color, if light { 0.14 } else { 0.18 }),
                );
            }
            if let Some(disc) = trace_circle_fill(center, 6.0) {
                window.paint_path(disc, trace_hsla(done_color, 1.0));
            }
            let mut check = gpui::PathBuilder::stroke(px(1.55));
            check.move_to(point(px(cx - 2.6), px(cy + 0.1)));
            check.line_to(point(px(cx - 0.7), px(cy + 2.1)));
            check.line_to(point(px(cx + 2.9), px(cy - 2.0)));
            if let Ok(path) = check.build() {
                window.paint_path(path, trace_hsla(0xffffff, 0.96));
            }
        } else {
            // Quiet pending node.
            if let Some(disc) = trace_circle_fill(center, 3.8) {
                window.paint_path(disc, trace_hsla(theme.bg_elevated, 1.0));
            }
            if let Some(ring) = trace_circle_ring(center, 3.8, 1.35) {
                window.paint_path(
                    ring,
                    trace_hsla(theme.border, if light { 0.85 } else { 0.72 }),
                );
            }
        }
    }
}

fn render_connection_stage_path(
    tokens: &ThemeTokens,
    i18n: &I18n,
    attempt_id: &str,
    event: &ConnectionTraceEvent,
    history: &VecDeque<ConnectionTraceEvent>,
) -> AnyElement {
    let (stages, current) = connection_trace_stage_list(event.stage, history);
    let failed = event.status == ConnectionTraceStatus::Failed;
    let ready = event.status == ConnectionTraceStatus::Ready;
    let stage_count = stages.len();
    let tokens_copy = *tokens;

    let build_track_canvas = move |phase: f32| {
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                paint_connection_signal_track(
                    &tokens_copy,
                    stage_count,
                    current,
                    ready,
                    failed,
                    phase,
                    window,
                    bounds,
                );
            },
        )
        .w_full()
        .h(px(34.0))
    };

    let track_visual: AnyElement =
        if !failed && tokens.motion.enabled && tokens.motion.spatial_enabled {
            if ready {
                div()
                    .w_full()
                    .h(px(34.0))
                    .with_animation(
                        format!("connection-signal-ready-{attempt_id}"),
                        Animation::new(oxideterm_gpui_ui::motion::scaled_duration(tokens, 560)),
                        move |slot, progress| slot.child(build_track_canvas(progress)),
                    )
                    .into_any_element()
            } else {
                div()
                    .w_full()
                    .h(px(34.0))
                    .with_animation(
                        format!("connection-signal-live-{attempt_id}-{current}"),
                        Animation::new(oxideterm_gpui_ui::motion::scaled_duration(tokens, 1350))
                            .repeat(),
                        move |slot, progress| slot.child(build_track_canvas(progress)),
                    )
                    .into_any_element()
            }
        } else {
            build_track_canvas(0.0).into_any_element()
        };

    let labels = stages
        .iter()
        .enumerate()
        .map(|(index, (_, label_key))| {
            let completed = ready || index < current;
            let active = !ready && index == current.min(stage_count.saturating_sub(1));
            let text_color = if failed && active {
                rgb(tokens.ui.error)
            } else if active {
                rgb(tokens.ui.text_heading)
            } else if completed {
                rgb(tokens.ui.text_secondary)
            } else {
                rgba((tokens.ui.text_muted << 8) | 0xa8)
            };
            div()
                .flex_1()
                .min_w_0()
                .px(px(2.0))
                .truncate()
                .text_align(gpui::TextAlign::Center)
                .text_size(px(11.0))
                .line_height(px(14.0))
                .font_weight(if active {
                    gpui::FontWeight::MEDIUM
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_color(text_color)
                .child(i18n.t(label_key))
                .into_any_element()
        })
        .collect::<Vec<_>>();

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(track_visual)
        .child(div().w_full().flex().items_center().children(labels))
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
        div()
            .w_full()
            .max_h(px(140.0))
            .overflow_y_scrollbar()
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgba((tokens.ui.error << 8) | 0x66))
            .bg(rgba((tokens.ui.error << 8) | 0x12))
            .font_family(tokens.metrics.markdown_code_font_family)
            .text_size(px(11.5))
            .line_height(px(17.0))
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
        // BLE stages are rendered through their own path; this SSH mapping is
        // only reached if a BLE stage ever arrives on a non-BLE card.
        ConnectionTraceStage::BleOpeningAdapter
        | ConnectionTraceStage::BleDirectConnect
        | ConnectionTraceStage::BleScanning
        | ConnectionTraceStage::BleMatching
        | ConnectionTraceStage::BleVerifyingService => 0,
        ConnectionTraceStage::BleReady => 3,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TraceTransport {
    Ssh,
    Ble,
}

/// Derives the card flavor from the stage vocabulary so BLE progress events do
/// not need a parallel event type just to change the badge and stage labels.
fn connection_trace_transport(stage: ConnectionTraceStage) -> TraceTransport {
    match stage {
        ConnectionTraceStage::BleOpeningAdapter
        | ConnectionTraceStage::BleDirectConnect
        | ConnectionTraceStage::BleScanning
        | ConnectionTraceStage::BleMatching
        | ConnectionTraceStage::BleVerifyingService
        | ConnectionTraceStage::BleReady => TraceTransport::Ble,
        _ => TraceTransport::Ssh,
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

fn render_zen_hint(tokens: &ThemeTokens, label: &str) -> AnyElement {
    let card = div()
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
        .child(label.to_owned());
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(24.0))
        .flex()
        .justify_center()
        .child(oxideterm_gpui_ui::motion::zoom_fade_in_y(
            tokens,
            "workspace-zen-hint",
            card,
            8.0,
            0.94,
            gpui::point(0.5, 1.0),
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        ))
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
    // Zoom the pill itself rather than fading the full-screen wrapper so font
    // step updates scale cleanly around the badge center.
    let layer = div()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .flex()
        .items_center()
        .justify_center()
        .child(oxideterm_gpui_ui::motion::zoom_fade_in_y(
            tokens,
            "terminal-font-size-hud",
            card,
            6.0,
            0.92,
            gpui::point(0.5, 0.5),
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        ));
    deferred(layer)
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
        ConnectionTraceStage::BleOpeningAdapter => "connections.trace.stage.ble_opening_adapter",
        ConnectionTraceStage::BleDirectConnect => "connections.trace.stage.ble_direct_connect",
        ConnectionTraceStage::BleScanning => "connections.trace.stage.ble_scanning",
        ConnectionTraceStage::BleMatching => "connections.trace.stage.ble_matching",
        ConnectionTraceStage::BleVerifyingService => {
            "connections.trace.stage.ble_verifying_service"
        }
        ConnectionTraceStage::BleReady => "connections.trace.stage.ble_ready",
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

    fn ble_trace_event(stage: ConnectionTraceStage) -> ConnectionTraceEvent {
        ConnectionTraceEvent {
            attempt_id: "attempt-1".to_string(),
            node_id: NodeId::new("node-1"),
            stage,
            status: ConnectionTraceStatus::Running,
            progress: 0.0,
            elapsed_ms: 0,
            detail: None,
            label: None,
            endpoint: None,
            step_index: None,
            total_steps: None,
            mode: ConnectionTraceMode::Connect,
        }
    }

    #[gpui::test]
    fn running_stage_updates_arm_the_coalesced_flush(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::from_millis(90), cx));
        overlay.update(cx, |overlay, _cx| {
            let started = Instant::now();
            assert!(overlay.apply_connection_trace_events(
                vec![ble_trace_event(ConnectionTraceStage::BleOpeningAdapter)],
                started,
            ));

            // A stage change only updates `latest`; the card must still report the
            // intent as changed so the owner arms the coalescing timer.
            let dispatched_at = started + Duration::from_millis(50);
            assert!(overlay.apply_connection_trace_events(
                vec![ble_trace_event(ConnectionTraceStage::BleScanning)],
                dispatched_at,
            ));
            let card = overlay.connection_trace_cards.get("attempt-1").unwrap();
            assert_eq!(card.latest.stage, ConnectionTraceStage::BleScanning);
            assert_eq!(
                card.displayed.as_ref().unwrap().stage,
                ConnectionTraceStage::BleOpeningAdapter
            );
            assert!(card.flush_deadline.is_some());

            assert!(overlay.process_due_deadlines(
                dispatched_at + CONNECTION_TRACE_UPDATE_COALESCE + Duration::from_millis(1)
            ));
            let card = overlay.connection_trace_cards.get("attempt-1").unwrap();
            assert_eq!(
                card.displayed.as_ref().unwrap().stage,
                ConnectionTraceStage::BleScanning
            );
            assert!(card.flush_deadline.is_none());

            // A BLE ready event must keep the card for the success TTL: the
            // scan→match→verify sequence can finish in milliseconds, so an
            // immediate removal would flash the card instead of confirming it.
            let mut ready = ble_trace_event(ConnectionTraceStage::BleReady);
            ready.status = ConnectionTraceStatus::Ready;
            let ready_at = dispatched_at + CONNECTION_TRACE_UPDATE_COALESCE;
            assert!(overlay.apply_connection_trace_events(vec![ready], ready_at));
            let card = overlay.connection_trace_cards.get("attempt-1").unwrap();
            assert_eq!(
                card.displayed.as_ref().unwrap().stage,
                ConnectionTraceStage::BleReady
            );
            assert!(card.expires_at.is_some());

            assert!(overlay.process_due_deadlines(ready_at + CONNECTION_TRACE_SUCCESS_TTL));
            assert_eq!(
                overlay
                    .connection_trace_cards
                    .get("attempt-1")
                    .unwrap()
                    .presence
                    .phase(),
                oxideterm_gpui_ui::motion::ExitPhase::Exiting
            );
        });
    }

    #[gpui::test]
    fn plugin_requirement_notices_route_install_and_enable_actions(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_CONNECTION_PLUGIN_NOTICE_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Portable storage keeps this real-workspace regression away from user plugins.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            fs::hard_link(&executable, &child).unwrap();
            fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let settings_path = default_settings_path();
        assert!(settings_path.starts_with(PathBuf::from(fixture_dir)));
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().general.language = oxideterm_settings::Language::En;
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();
        // The full workspace owns real background workers and filesystem watches.
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        for (protocol, name, id) in [
            (
                Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp),
                "RDP",
                "com.oxideterm.remote-desktop.rdp",
            ),
            (
                Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc),
                "VNC",
                "com.oxideterm.remote-desktop.vnc",
            ),
            (None, "Mosh", "com.oxideterm.terminal.mosh"),
            (None, "Codex", "com.oxideterm.acp.codex"),
        ] {
            cx.update(|window, cx| workspace.update(cx, |workspace, cx| {
                let acp_agent_id = if id == "com.oxideterm.acp.codex" {
                    workspace.edit_settings(|settings| oxideterm_settings_model::ai_add_acp_plugin_agent(settings, id, name), cx);
                    Some(workspace.settings_store.settings().ai.acp_agents.iter()
                        .find(|agent| agent.plugin_id.as_deref() == Some(id)).unwrap().id.clone())
                } else { None };
                if let Some(protocol) = protocol {
                    workspace.open_remote_desktop_connection_for_connection(
                        oxideterm_remote_desktop::RemoteDesktopConnectionProfile {
                            pre_connect_command: None,
                            id: "restored-profile".into(),
                            label: "Restored profile".into(),
                            protocol,
                            endpoint: oxideterm_remote_desktop::RemoteDesktopEndpoint::for_protocol("fixture.invalid", protocol),
                            transport_endpoint: None,
                            socks_proxy: None,
                            username: None,
                            domain: None,
                            credential_ref: None,
                            read_only: false,
                            session_options: Default::default(),
                        }, None, None, None, window, cx,
                    );
                } else if let Some(agent_id) = &acp_agent_id {
                    workspace.test_ai_acp_agent(agent_id.clone(), cx);
                } else {
                    workspace.show_required_plugin_notice(id, name, cx);
                }
                let overlay = workspace.overlay.read(cx);
                let toast = overlay.transients.standard_toasts.last().unwrap();
                assert_eq!(toast.notice.title, format!("The {name} plugin is not installed. Install it from the plugin marketplace, then retry."));
                let action = toast.action.as_ref().unwrap();
                assert_eq!(action.target, NoticeActionTarget::Plugin(id.to_string()));
                assert_eq!(action.label, "Plugin Marketplace");
                assert_eq!(action.workspace.upgrade().unwrap().entity_id(), cx.entity().entity_id());
                workspace.open_required_plugin(id, window, cx);
                assert_eq!(workspace.plugin_manager_state(cx).active_tab, plugin_manager::NativePluginManagerTab::Marketplace);
                assert_eq!(workspace.plugin_manager_state(cx).marketplace_search_draft, id);
                let directory = plugin_host::native_plugins_dir(workspace.settings_store.path()).join(id);
                fs::create_dir_all(directory.join("bin")).unwrap();
                fs::write(directory.join("bin/helper"), b"disabled provider fixture").unwrap();
                let (kind, contributes) = match protocol {
                    Some(protocol) => ("remote-desktop", serde_json::json!({
                        "remoteDesktop": {"protocol": protocol.provider_id(), "protocolVersion": 1}
                    })),
                    None if id == "com.oxideterm.terminal.mosh" => ("terminal-transport", serde_json::json!({
                        "terminalTransport": {"protocol": "mosh", "protocolVersion": 1}
                    })),
                    None => ("acp", serde_json::Value::Null),
                };
                fs::write(directory.join("plugin.json"), serde_json::to_vec(&serde_json::json!({
                    "id": id, "name": name, "version": "0.1.0",
                    "runtime": {"kind": kind, "entry": "bin/helper"},
                    "contributes": contributes,
                    "engines": {"oxideterm": ">=2.2.2"}
                })).unwrap()).unwrap();
                let registry = plugin_host::NativePluginRegistry::discover(workspace.settings_store.path());
                workspace.plugin_entity.update(cx, |plugins, cx| plugins.replace_registry(registry, cx));
                if let Some(agent_id) = &acp_agent_id {
                    workspace.test_ai_acp_agent(agent_id.clone(), cx);
                } else {
                    workspace.show_required_plugin_notice(id, name, cx);
                }
                let toast = workspace.overlay.read(cx).transients.standard_toasts.last().unwrap();
                assert_eq!(toast.notice.title, format!("The installed {name} plugin is unavailable. Enable or update it in Plugin Manager, then retry."));
                assert_eq!(toast.action.as_ref().unwrap().label, "Manage plugin");
                assert_eq!(toast.action.as_ref().unwrap().target, NoticeActionTarget::Plugin(id.to_string()));
                workspace.open_required_plugin(id, window, cx);
                assert_eq!(workspace.plugin_manager_state(cx).active_tab, plugin_manager::NativePluginManagerTab::Installed);
            }));
        }
    }

    #[gpui::test]
    fn palette_open_failures_are_visible_without_session_manager(cx: &mut TestAppContext) {
        use crate::workspace::session_manager::tests::isolated_workspace_fixture;
        use oxideterm_gpui_connections::launch::SshConnectionWorkerResult;

        let Some(fixture_dir) =
            isolated_workspace_fixture(cx, "OXIDETERM_PALETTE_FAILURE_TEST_DIR")
        else {
            return;
        };
        let settings_path = default_settings_path();
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().general.language = oxideterm_settings::Language::En;
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();

        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        let directory = fixture_dir.join("saved-working-directory");
        fs::create_dir(&directory).unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .tokens
                    .apply_motion(oxideterm_theme::UiMotionProfile::Off);
                assert!(
                    workspace
                        .tabs(cx)
                        .iter()
                        .all(|tab| tab.kind != TabKind::SessionManager)
                );
                workspace
                    .connection_store
                    .upsert_local_terminal_profile(
                        oxideterm_connections::SaveLocalTerminalProfileRequest {
                            pre_connect_command: None,
                            name: "Unavailable palette profile".into(),
                            cwd: Some(directory.to_string_lossy().into_owned()),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                fs::remove_dir(&directory).unwrap();
                // A cached search result can outlive its source in this window.
                let cached_target = oxideterm_discovery::CatalogEntry {
                    row_id: "discovery:4:west42".into(),
                    source_id: "west".into(),
                    target_id: "42".into(),
                    source_name: "West".into(),
                    label: "Withdrawn discovery target".into(),
                    detail: None,
                    group_paths: vec![],
                    scope: None,
                    tags: vec![],
                };
                workspace
                    .connections
                    .session_manager
                    .update(cx, |manager, cx| {
                        manager.update_discovery(vec![cached_target], vec![], cx);
                    });
                for query in ["Unavailable palette profile", "Withdrawn discovery target"] {
                    workspace.open_command_palette(window, cx);
                    workspace.command_palette.update(cx, |palette, cx| {
                        palette.push_query_text(query, cx);
                    });
                    workspace.handle_command_palette_key(
                        &KeyDownEvent {
                            keystroke: gpui::Keystroke::parse("enter").unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    );
                    assert!(!workspace.command_palette.read(cx).is_open(), "{query}");
                }
                assert!(
                    workspace
                        .tabs(cx)
                        .iter()
                        .all(|tab| tab.kind != TabKind::SessionManager)
                );

                // A worker may also return a failed handshake after the palette closed.
                let discovered = oxideterm_discovery::DiscoveryOrigin::new(
                    "east".into(),
                    "ssh".into(),
                    uuid::Uuid::new_v4(),
                );
                for (origin, message) in [
                    (Some(discovered), "Fixture SSH handshake failed"),
                    (None, "Saved SSH handshake failed"),
                ] {
                    let result = SshConnectionWorkerResult::Preflight {
                        config: SshConfig {
                            host: "fixture.invalid".into(),
                            discovery_origin: origin,
                            ..Default::default()
                        },
                        upstream_proxy: None,
                        title: "SSH".into(),
                        intent: SshConnectionIntent::Connect(Default::default()),
                        host: "fixture.invalid".into(),
                        port: 22,
                        checked_hop: None,
                        status: oxideterm_ssh::HostKeyStatus::Error {
                            message: message.into(),
                        },
                    };
                    let request = workspace
                        .connections
                        .flow
                        .update(cx, |flow, cx| {
                            flow.apply_ssh_worker_result(result, &workspace.i18n, cx)
                        })
                        .unwrap();
                    workspace.dispatch_connections_request(request, Some(window), cx);
                }
                assert_eq!(
                    workspace
                        .connections
                        .session_manager
                        .read(cx)
                        .status
                        .as_deref(),
                    Some("Saved SSH handshake failed")
                );
            });
        });
        cx.run_until_parked();
        let overlay = workspace.read_with(cx, |workspace, _| workspace.overlay.clone());
        overlay.read_with(cx, |overlay, _| {
            assert_eq!(
                overlay
                    .transients
                    .standard_toasts
                    .iter()
                    .map(|toast| (
                        toast.notice.title.as_str(),
                        toast.notice.description.as_deref(),
                        toast.notice.variant
                    ))
                    .collect::<Vec<_>>(),
                [
                    (
                        "The startup directory is unavailable. Edit the session to choose an existing directory.",
                        None,
                        TerminalNoticeVariant::Error,
                    ),
                    (
                        "Connection Failed",
                        Some("Target has changed. Refresh the source."),
                        TerminalNoticeVariant::Error,
                    ),
                    (
                        "Connection Failed",
                        Some("Fixture SSH handshake failed"),
                        TerminalNoticeVariant::Error,
                    ),
                ]
            );
        });
        let error_background: gpui::Hsla = workspace.read_with(cx, |workspace, _| {
            rgba((workspace.tokens.ui.error << 8) | 0x1a).into()
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(
                window
                    .painted_quads()
                    .iter()
                    .any(|quad| { quad.background.as_solid() == Some(error_background) }),
                "the error toast must be painted in the current window"
            );
        });
    }

    #[gpui::test]
    fn tooltip_delay_exit_reentry_and_replacement_cancel_stale_deadlines(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::from_millis(300), cx));
        overlay.update(cx, |overlay, _| {
            let start = Instant::now();
            overlay.transients.queue_tooltip(
                "cloud".into(),
                "Cloud sync".into(),
                10.0,
                20.0,
                start,
            );
            overlay.process_due_deadlines(start + Duration::from_millis(299));
            assert!(overlay.transients.tooltip.is_none());
            let shown = start + TOOLTIP_DELAY;
            overlay.process_due_deadlines(shown);
            assert_eq!(
                overlay.transients.tooltip.as_ref().unwrap().label,
                "Cloud sync"
            );
            assert!(!overlay.transients.queue_tooltip(
                "cloud".into(),
                "Cloud sync".into(),
                12.0,
                22.0,
                shown
            ));
            assert_eq!(overlay.transients.tooltip.as_ref().unwrap().x, 10.0);
            overlay.transients.clear_tooltip("cloud", shown);
            assert_eq!(overlay.next_deadline(), Some(shown + TOOLTIP_EXIT_DURATION));
            overlay.transients.queue_tooltip(
                "cloud".into(),
                "Cloud sync".into(),
                15.0,
                25.0,
                shown + Duration::from_millis(30),
            );
            overlay.process_due_deadlines(shown + TOOLTIP_EXIT_DURATION);
            let tooltip = overlay.transients.tooltip.as_ref().unwrap();
            assert_eq!(
                (tooltip.label.as_str(), tooltip.x, tooltip.y),
                ("Cloud sync", 15.0, 25.0)
            );
            let switch = shown + Duration::from_millis(100);
            overlay.transients.queue_tooltip(
                "tools".into(),
                "Host tools".into(),
                80.0,
                25.0,
                switch,
            );
            let tooltip = overlay.transients.tooltip.as_ref().unwrap();
            assert_eq!(tooltip.label, "Host tools");
            assert!(tooltip.switching);
            overlay.transients.clear_tooltip("cloud", switch);
            let exit = switch;
            overlay.transients.clear_tooltip("tools", exit);
            overlay
                .transients
                .clear_tooltip("tools", exit + Duration::from_millis(30));
            overlay.process_due_deadlines(exit + Duration::from_millis(89));
            assert_eq!(overlay.transients.tooltip.as_ref().unwrap().id, "tools");
            overlay.process_due_deadlines(exit + TOOLTIP_EXIT_DURATION);
            assert!(overlay.transients.tooltip.is_none());
            let warm = exit + Duration::from_millis(100);
            overlay
                .transients
                .queue_tooltip("theme".into(), "Theme".into(), 110.0, 25.0, warm);
            assert_eq!(overlay.transients.tooltip.as_ref().unwrap().id, "theme");
            overlay.transients.clear_tooltip("theme", warm);
            let cold = warm + TOOLTIP_REENTRY_WINDOW;
            overlay.process_due_deadlines(cold);
            overlay
                .transients
                .queue_tooltip("late".into(), "Delayed again".into(), 0.0, 0.0, cold);
            overlay.process_due_deadlines(cold + TOOLTIP_DELAY - Duration::from_millis(1));
            assert!(overlay.transients.tooltip.is_none());
            overlay.process_due_deadlines(cold + TOOLTIP_DELAY);
            assert_eq!(
                overlay.transients.tooltip.as_ref().unwrap().label,
                "Delayed again"
            );
            overlay.transients.clear_all_tooltips();
            let cancelled = cold + TOOLTIP_DELAY;
            overlay.transients.queue_tooltip(
                "cancelled".into(),
                "Never shown".into(),
                0.0,
                0.0,
                cancelled,
            );
            assert!(overlay.transients.tooltip.is_none());
            overlay.transients.clear_tooltip("cancelled", cancelled);
            overlay.process_due_deadlines(cancelled + TOOLTIP_DELAY);
            assert!(overlay.transients.tooltip.is_none());
        });
    }

    struct TooltipMotionTestRoot {
        tooltip: WorkspaceTooltip,
        tokens: ThemeTokens,
    }

    impl Render for TooltipMotionTestRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(render_tooltip(
                &self.tokens,
                self.tooltip.clone(),
                TOOLTIP_EXIT_DURATION,
            ))
        }
    }

    fn assert_tooltip_origin(cx: &mut gpui::VisualTestContext, x: f32, y: f32) {
        let actual = cx.debug_bounds("workspace-tooltip-surface").unwrap().origin;
        // GPUI's anchored layer rounds its positioning offset to logical pixels.
        let tolerance = px(0.5);
        let expected = gpui::point(px(x), px(y));
        assert!(
            (actual.x - expected.x).abs() <= tolerance
                && (actual.y - expected.y).abs() <= tolerance,
            "expected {expected:?} within {tolerance:?}, got {actual:?}"
        );
    }

    #[gpui::test]
    fn tooltip_motion_resumes_from_the_rendered_position_and_opacity(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::from_millis(200), cx));
        let start = Instant::now();
        let tooltip = overlay.update(cx, |overlay, _| {
            overlay.transients.queue_tooltip(
                "cloud".into(),
                "Cloud sync".into(),
                10.0,
                20.0,
                start,
            );
            overlay.process_due_deadlines(start + TOOLTIP_DELAY);
            overlay.transients.tooltip.clone().unwrap()
        });
        let opacity = tooltip.opacity.clone();
        let position = tooltip.position.clone();
        let (view, cx) = cx.add_window_view(|_, _| TooltipMotionTestRoot {
            tooltip,
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(gpui::size(px(640.0), px(480.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(45));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert!((opacity.get() - 0.875).abs() < 0.0001);
        assert_eq!(position.get(), gpui::point(10.0, 20.375));
        assert_tooltip_origin(cx, 10.0, 20.375);
        view.update(cx, |view, cx| {
            overlay.update(cx, |overlay, _| {
                overlay.transients.queue_tooltip(
                    "tools".into(),
                    "Host tools".into(),
                    70.0,
                    20.0,
                    start + TOOLTIP_DELAY,
                );
                view.tooltip = overlay.transients.tooltip.clone().unwrap();
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_tooltip_origin(cx, 10.0, 20.375);
        cx.executor().advance_clock(Duration::from_millis(75));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let moving_position = view.read_with(cx, |view, _| view.tooltip.position.get());
        assert_eq!(moving_position, gpui::point(40.0, 20.1875));
        assert_tooltip_origin(cx, 40.0, 20.1875);
        view.update(cx, |view, cx| {
            overlay.update(cx, |overlay, _| {
                overlay
                    .transients
                    .clear_tooltip("tools", start + TOOLTIP_DELAY);
                view.tooltip = overlay.transients.tooltip.clone().unwrap();
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let switched_opacity = view.read_with(cx, |view, _| view.tooltip.opacity.clone());
        assert!((switched_opacity.get() - 0.9375).abs() < 0.0001);
        cx.executor().advance_clock(Duration::from_millis(45));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!((switched_opacity.get() - 0.1171875).abs() < 0.0001);
        assert_tooltip_origin(cx, 40.0, 20.1875);
        view.update(cx, |view, cx| {
            overlay.update(cx, |overlay, _| {
                overlay.transients.queue_tooltip(
                    "tools".into(),
                    "Host tools".into(),
                    70.0,
                    20.0,
                    start + TOOLTIP_DELAY + Duration::from_millis(45),
                );
                view.tooltip = overlay.transients.tooltip.clone().unwrap();
            });
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!((switched_opacity.get() - 0.1171875).abs() < 0.0001);
        assert_tooltip_origin(cx, 40.0, 20.1875);
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert_eq!(switched_opacity.get(), 1.0);
        assert_tooltip_origin(cx, 70.0, 20.0);

        let edge = start + TOOLTIP_DELAY + TOOLTIP_REENTRY_WINDOW;
        view.update(cx, |view, cx| {
            overlay.update(cx, |overlay, _| {
                overlay.transients.queue_tooltip(
                    "caption".into(),
                    "Caption button".into(),
                    650.0,
                    20.0,
                    edge,
                );
                view.tooltip = overlay.transients.tooltip.clone().unwrap();
            });
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.executor().advance_clock(Duration::from_millis(120));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.update(cx, |view, cx| {
            overlay.update(cx, |overlay, _| {
                overlay.transients.queue_tooltip(
                    "neighbor".into(),
                    "A neighboring button with a longer hint".into(),
                    620.0,
                    20.0,
                    edge + Duration::from_millis(120),
                );
                view.tooltip = overlay.transients.tooltip.clone().unwrap();
            });
            cx.notify();
        });
        for elapsed in [0, 50, 50, 100] {
            cx.executor().advance_clock(Duration::from_millis(elapsed));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let bounds = cx.debug_bounds("workspace-tooltip-surface").unwrap();
            assert!(
                bounds.left() >= px(0.0) && bounds.right() <= px(640.0),
                "tooltip outside window: {bounds:?}"
            );
        }
    }

    #[gpui::test]
    fn tooltip_reduced_and_disabled_motion_do_not_shift_the_surface(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| TooltipMotionTestRoot {
            tooltip: WorkspaceTooltip::new(
                "hint".into(),
                "Hint".into(),
                gpui::point(30.0, 40.0),
                0,
            ),
            tokens: oxideterm_theme::default_tokens(),
        });
        cx.simulate_resize(gpui::size(px(640.0), px(480.0)));
        for (generation, profile, opacity) in [
            (1, oxideterm_theme::UiMotionProfile::Off, 1.0),
            (2, oxideterm_theme::UiMotionProfile::Reduced, 0.875),
        ] {
            view.update(cx, |view, cx| {
                view.tokens.apply_motion(profile);
                view.tooltip = WorkspaceTooltip::new(
                    "hint".into(),
                    "Hint".into(),
                    gpui::point(30.0, 40.0),
                    generation,
                );
                cx.notify();
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.executor().advance_clock(Duration::from_millis(40));
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.debug_bounds("workspace-tooltip-surface").unwrap().origin,
                gpui::point(px(30.0), px(40.0))
            );
            view.read_with(cx, |view, _| {
                assert!((view.tooltip.opacity.get() - opacity).abs() < 0.0001)
            });
        }
    }

    #[gpui::test]
    fn tooltip_animation_disabled_clears_visible_and_pending_prompts(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        overlay.update(cx, |overlay, _| {
            let now = Instant::now();
            overlay
                .transients
                .queue_tooltip("cloud".into(), "Cloud sync".into(), 0.0, 0.0, now);
            let shown = now + TOOLTIP_DELAY;
            overlay.process_due_deadlines(shown);
            overlay.transients.clear_tooltip("cloud", shown);
            assert!(overlay.transients.tooltip.is_none());
            overlay
                .transients
                .queue_tooltip("tools".into(), "Host tools".into(), 0.0, 0.0, shown);
            assert_eq!(
                overlay.transients.tooltip.as_ref().unwrap().label,
                "Host tools"
            );
            overlay.transients.clear_all_tooltips();
            overlay.process_due_deadlines(shown + TOOLTIP_DELAY);
            assert!(overlay.transients.tooltip.is_none());
        });
    }

    struct ConnectionCardTestRoot {
        overlay: Entity<WorkspaceOverlayEntity>,
    }

    impl Render for ConnectionCardTestRoot {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let mut tokens = oxideterm_theme::default_tokens();
            tokens.apply_motion(oxideterm_theme::UiMotionProfile::Off);
            let cards = self.overlay.update(cx, |overlay, cx| {
                overlay.render_connection_cards(&tokens, &I18n::default(), cx)
            });
            div().size_full().children(cards)
        }
    }

    #[gpui::test]
    fn failed_connection_card_close_has_its_own_click_target_and_stays_dismissed(
        cx: &mut TestAppContext,
    ) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let event = ConnectionTraceEvent {
            attempt_id: "failed-reconnect".to_string(),
            node_id: NodeId::new("node-1"),
            stage: ConnectionTraceStage::OpeningTransport,
            status: ConnectionTraceStatus::Failed,
            progress: 30.0,
            elapsed_ms: 1000,
            detail: Some("Connection timed out".to_string()),
            label: Some("Production server with a long connection name".to_string()),
            endpoint: Some("operator@long-hostname.example.test:2222".to_string()),
            step_index: Some(1),
            total_steps: Some(1),
            mode: ConnectionTraceMode::Reconnect,
        };
        overlay.update(cx, |overlay, _| {
            overlay.apply_connection_trace_events(vec![event.clone()], Instant::now());
        });
        let (_, cx) = cx.add_window_view(|_, _| ConnectionCardTestRoot {
            overlay: overlay.clone(),
        });
        cx.simulate_resize(gpui::size(px(720.0), px(600.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let card = cx.debug_bounds("connection-card").expect("visible card");
        let heading = cx.debug_bounds("connection-card-heading").expect("heading");
        let close = cx
            .debug_bounds("connection-card-close")
            .expect("close button");
        assert!(card.contains(&close.origin) && card.contains(&close.bottom_right()));
        assert!(close.size.width >= px(24.0) && close.size.height >= px(24.0));
        assert!(
            close.left() >= heading.right(),
            "close target overlaps the heading"
        );
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        cx.update(|_, cx| {
            overlay.update(cx, |overlay, _| {
                assert!(
                    !overlay
                        .connection_trace_cards
                        .contains_key("failed-reconnect")
                );
                overlay.apply_connection_trace_events(
                    vec![ConnectionTraceEvent {
                        status: ConnectionTraceStatus::Running,
                        ..event
                    }],
                    Instant::now(),
                );
                assert!(
                    !overlay
                        .connection_trace_cards
                        .contains_key("failed-reconnect")
                );
            });
        });
    }

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
    fn fast_connection_shows_completed_card_for_success_ttl(cx: &mut TestAppContext) {
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
            let ready_at = now + Duration::from_millis(80);
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
                ready_at,
            );

            let card = overlay
                .connection_trace_cards
                .get("fast-attempt")
                .expect("completed SSH card stays visible long enough to play ready sweep");
            assert_eq!(
                card.displayed.as_ref().unwrap().stage,
                ConnectionTraceStage::Ready
            );
            assert_eq!(
                card.expires_at,
                Some(ready_at + CONNECTION_TRACE_SUCCESS_TTL)
            );

            assert!(overlay.process_due_deadlines(ready_at + CONNECTION_TRACE_SUCCESS_TTL));
            assert!(!overlay.connection_trace_cards.contains_key("fast-attempt"));
        });
    }

    #[gpui::test]
    fn dismissed_connection_card_stays_hidden_across_retries_until_the_workflow_finishes(
        cx: &mut TestAppContext,
    ) {
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
            mode: ConnectionTraceMode::Reconnect,
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
                    ConnectionTraceStatus::Failed,
                    100.0,
                )],
                now,
            );
            assert!(
                overlay
                    .dismissed_connection_traces
                    .iter()
                    .any(|attempt_id| attempt_id == "dismissed-attempt")
            );

            overlay.apply_connection_trace_events(
                vec![event(
                    ConnectionTraceStage::OpeningTransport,
                    ConnectionTraceStatus::Running,
                    30.0,
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
    fn retry_replaces_the_previous_card_for_the_same_node(cx: &mut TestAppContext) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let event = |attempt_id: &str, status| ConnectionTraceEvent {
            attempt_id: attempt_id.to_string(),
            node_id: NodeId::new("retry-node"),
            stage: ConnectionTraceStage::OpeningTransport,
            status,
            progress: 100.0,
            elapsed_ms: 0,
            detail: None,
            label: None,
            endpoint: None,
            step_index: Some(1),
            total_steps: Some(1),
            mode: ConnectionTraceMode::Reconnect,
        };

        overlay.update(cx, |overlay, _cx| {
            let now = Instant::now();
            overlay.apply_connection_trace_events(
                vec![event("retry-attempt-1", ConnectionTraceStatus::Failed)],
                now,
            );
            overlay.apply_connection_trace_events(
                vec![event("retry-attempt-2", ConnectionTraceStatus::Running)],
                now,
            );

            assert_eq!(overlay.connection_trace_cards.len(), 1);
            assert!(
                !overlay
                    .connection_trace_cards
                    .contains_key("retry-attempt-1")
            );
            assert!(
                overlay
                    .connection_trace_cards
                    .contains_key("retry-attempt-2")
            );
        });
    }

    #[gpui::test]
    fn hidden_notice_delivery_continues_across_budgets_and_expires_on_deadline(
        cx: &mut TestAppContext,
    ) {
        let overlay = cx.new(|cx| WorkspaceOverlayEntity::new(Duration::ZERO, cx));
        let sender = cx.read(|cx| overlay.read(cx).notice_sender());
        for index in 0..130 {
            sender.send(notice(index)).expect("overlay notice");
        }

        cx.run_until_parked();

        cx.read(|cx| {
            let overlay = overlay.read(cx);
            assert_eq!(overlay.transients.standard_toasts.len(), 130);
            assert_eq!(
                overlay.transients.standard_toasts[0].notice.title,
                "notice-0"
            );
            assert_eq!(
                overlay.transients.standard_toasts[129].notice.title,
                "notice-129"
            );
        });
        cx.executor().advance_clock(WORKSPACE_NOTICE_TTL);
        cx.run_until_parked();
        cx.read(|cx| assert!(overlay.read(cx).transients.standard_toasts.is_empty()));
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
}
