use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use oxideterm_gpui_remote_desktop::{
    RemoteDesktopFrameApplyStats, RemoteDesktopMappedPoint, RemoteDesktopViewState,
    SharedRemoteDesktopGeometry, remote_desktop_surface_with_geometry,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
};
use oxideterm_remote_desktop::{
    NegotiatedCapabilities, NegotiatedCapabilityStatus, RemoteDesktopClipboardData,
    RemoteDesktopClipboardFormat, RemoteDesktopConnectionProfile, RemoteDesktopEndpoint,
    RemoteDesktopErrorCategory, RemoteDesktopFrameDeliverySlot, RemoteDesktopHelperEvent,
    RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState, RemoteDesktopLockKeys,
    RemoteDesktopMonitor, RemoteDesktopMonitorLayout, RemoteDesktopMonitorOrientation,
    RemoteDesktopMouseButton, RemoteDesktopMouseButtonState, RemoteDesktopProtocol,
    RemoteDesktopProviderManifest, RemoteDesktopSecret, RemoteDesktopSessionStatus,
    RemoteDesktopSize, RemoteDesktopWheelDelta, builtin_preview_provider_registry,
    builtin_provider_registry,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};
use tokio::sync::Notify;

use super::*;

mod certificate;
mod clipboard;
mod input;
mod interaction;
mod session;
mod view;
mod worker;

use certificate::*;
use clipboard::*;
use input::*;
use worker::*;

const REMOTE_DESKTOP_INITIAL_WIDTH: u32 = 1280;
const REMOTE_DESKTOP_INITIAL_HEIGHT: u32 = 720;
const REMOTE_DESKTOP_SCROLL_LINE: f32 = 38.0;
const REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_INTERVAL: Duration = Duration::from_millis(16);
const REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_TICKS: usize = 120;
const REMOTE_DESKTOP_RESIZE_DEBOUNCE: Duration = Duration::from_millis(120);
const REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD: u32 = 16;
const REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT: u32 = 500;
const REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER: f32 = 100.0;
const REMOTE_DESKTOP_SCROLL_PIXEL_STEP: f32 = 120.0;
const REMOTE_DESKTOP_FRAME_READY_DRAIN_LIMIT: usize = 32;
const REMOTE_DESKTOP_FRAME_READY_DRAIN_BUDGET: Duration = Duration::from_millis(6);
const REMOTE_DESKTOP_DIAGNOSTICS_ENV: &str = "OXIDETERM_REMOTE_DESKTOP_DIAGNOSTICS";

#[derive(Debug)]
pub(super) enum RemoteDesktopWorkerDelivery {
    FrameReady {
        tab_id: TabId,
        generation: u64,
    },
    FrameRecoveryRequired {
        tab_id: TabId,
        generation: u64,
    },
    Event {
        tab_id: TabId,
        generation: u64,
        event: RemoteDesktopHelperEvent,
    },
    TransportFailed {
        tab_id: TabId,
        generation: u64,
        message: String,
    },
}

#[derive(Clone)]
struct RemoteDesktopWorkerWake {
    pending: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    notification: Arc<Notify>,
}

impl Default for RemoteDesktopWorkerWake {
    fn default() -> Self {
        Self {
            pending: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            notification: Arc::new(Notify::new()),
        }
    }
}

impl RemoteDesktopWorkerWake {
    fn mark(&self) {
        // Worker threads cannot touch GPUI state directly. Notify stores one
        // permit when the foreground task has not started waiting yet.
        self.pending.store(true, Ordering::Release);
        self.notification.notify_one();
    }

    fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.notification.notify_one();
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    async fn wait(&self) {
        self.notification.notified().await;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RemoteDesktopRenderDiagnostics {
    batches: u64,
    events_drained: u64,
    drain_budget_hits: u64,
    full_frames: u64,
    frame_updates: u64,
    dirty_updates_applied: u64,
    dirty_updates_rejected: u64,
    full_update_recoveries: u64,
    corrupted_frames: u64,
    first_trace_id: Option<u64>,
    last_trace_id: Option<u64>,
    dirty_rect_pixels: u64,
    dirty_frame_pixels: u64,
    pending_texture_updates: u64,
    pending_texture_upload_bytes: u64,
    dirty_tiles_refreshed: u64,
    frame_tiles_created: u64,
    retired_images: u64,
    total_apply_micros: u64,
    max_apply_micros: u64,
}

impl RemoteDesktopRenderDiagnostics {
    fn record_batch(
        &mut self,
        drained_events: usize,
        budget_hit: bool,
        apply_elapsed: Duration,
        apply_stats: RemoteDesktopFrameApplyStats,
        retired_images: usize,
    ) {
        self.batches = self.batches.saturating_add(1);
        self.events_drained = self.events_drained.saturating_add(drained_events as u64);
        if budget_hit {
            self.drain_budget_hits = self.drain_budget_hits.saturating_add(1);
        }
        self.full_frames = self
            .full_frames
            .saturating_add(apply_stats.full_frames as u64);
        self.frame_updates = self
            .frame_updates
            .saturating_add(apply_stats.frame_updates as u64);
        self.dirty_updates_applied = self
            .dirty_updates_applied
            .saturating_add(apply_stats.dirty_updates_applied as u64);
        self.dirty_updates_rejected = self
            .dirty_updates_rejected
            .saturating_add(apply_stats.dirty_updates_rejected as u64);
        self.full_update_recoveries = self
            .full_update_recoveries
            .saturating_add(apply_stats.full_update_recoveries as u64);
        self.corrupted_frames = self
            .corrupted_frames
            .saturating_add(apply_stats.corrupted_frames as u64);
        if self.first_trace_id.is_none() {
            self.first_trace_id = apply_stats.first_trace_id;
        }
        if apply_stats.last_trace_id.is_some() {
            self.last_trace_id = apply_stats.last_trace_id;
        }
        self.dirty_rect_pixels = self
            .dirty_rect_pixels
            .saturating_add(apply_stats.dirty_rect_pixels);
        self.dirty_frame_pixels = self
            .dirty_frame_pixels
            .saturating_add(apply_stats.dirty_frame_pixels);
        self.pending_texture_updates = apply_stats.pending_texture_updates as u64;
        self.pending_texture_upload_bytes = apply_stats.pending_texture_upload_bytes as u64;
        self.dirty_tiles_refreshed = self
            .dirty_tiles_refreshed
            .saturating_add(apply_stats.dirty_tiles_refreshed as u64);
        self.frame_tiles_created = self
            .frame_tiles_created
            .saturating_add(apply_stats.frame_tiles_created as u64);
        self.retired_images = self.retired_images.saturating_add(retired_images as u64);
        let apply_micros = duration_micros_u64(apply_elapsed);
        self.total_apply_micros = self.total_apply_micros.saturating_add(apply_micros);
        self.max_apply_micros = self.max_apply_micros.max(apply_micros);
    }
}

fn send_remote_desktop_worker_delivery(
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_wake: &RemoteDesktopWorkerWake,
    delivery: RemoteDesktopWorkerDelivery,
) {
    worker_wake.mark();
    let _ = delivery_tx.send(delivery);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RemoteDesktopModifierState {
    // GPUI key events carry aggregate modifier state; mirror that state so the
    // helper can correct missed platform modifier key transitions.
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
}

impl RemoteDesktopModifierState {
    fn from_gpui(modifiers: gpui::Modifiers) -> Self {
        Self {
            shift: modifiers.shift,
            ctrl: modifiers.control,
            alt: modifiers.alt,
            meta: modifiers.platform,
        }
    }
}

pub(super) struct RemoteDesktopSession {
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    password: Option<RemoteDesktopSecret>,
    certificate_challenge: Option<RemoteDesktopCertificateChallengeState>,
    session_trusted_certificate_fingerprint: Option<String>,
    state: RemoteDesktopViewState,
    geometry: SharedRemoteDesktopGeometry,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_tx: Option<mpsc::Sender<RemoteDesktopHelperRequest>>,
    worker_generation: u64,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    last_viewport_scale_factor: Option<u32>,
    last_monitor_layout: RemoteDesktopMonitorLayout,
    resize_generation: Arc<AtomicU64>,
    last_input_modifiers: RemoteDesktopModifierState,
    last_lock_keys: Option<RemoteDesktopLockKeys>,
    pressed_mouse_buttons: HashSet<RemoteDesktopMouseButton>,
    wheel_pixel_remainder: RemoteDesktopWheelDelta,
    render_diagnostics: RemoteDesktopRenderDiagnostics,
}

impl RemoteDesktopSession {
    fn new(
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password: Option<RemoteDesktopSecret>,
        frame_slot: RemoteDesktopFrameDeliverySlot,
    ) -> Self {
        let mut state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
            .with_read_only(profile.read_only);
        state.apply_event(RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connecting,
            message: None,
        });
        Self {
            profile,
            provider,
            // Runtime credentials are kept only for this tab so a user-visible
            // reconnect can start a fresh helper after the previous one exits.
            password,
            certificate_challenge: None,
            session_trusted_certificate_fingerprint: None,
            state,
            geometry: SharedRemoteDesktopGeometry::default(),
            frame_slot,
            request_tx: None,
            worker_generation: 0,
            last_viewport_size: None,
            last_sent_resize: None,
            last_viewport_scale_factor: None,
            last_monitor_layout: RemoteDesktopMonitorLayout::default(),
            resize_generation: Arc::new(AtomicU64::new(0)),
            last_input_modifiers: RemoteDesktopModifierState::default(),
            last_lock_keys: None,
            pressed_mouse_buttons: HashSet::new(),
            wheel_pixel_remainder: remote_desktop_empty_wheel_delta(),
            render_diagnostics: RemoteDesktopRenderDiagnostics::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RemoteDesktopResizeRequestState {
    size: RemoteDesktopSize,
    scale_factor: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_wake_uses_event_notification_and_stops_explicitly() {
        let wake = RemoteDesktopWorkerWake::default();
        let runtime = tokio::runtime::Runtime::new().unwrap();

        wake.mark();
        runtime.block_on(wake.wait());
        assert!(wake.take());

        wake.stop();
        runtime.block_on(wake.wait());
        assert!(wake.is_stopped());
    }

    #[test]
    fn reconnect_mode_restarts_helper_after_terminal_states() {
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Disconnected),
            Some(RemoteDesktopReconnectMode::RestartHelper)
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Failed),
            Some(RemoteDesktopReconnectMode::RestartHelper)
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Idle),
            Some(RemoteDesktopReconnectMode::RestartHelper)
        );
    }

    #[test]
    fn reconnect_mode_uses_live_helper_only_when_connected() {
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Connected),
            Some(RemoteDesktopReconnectMode::ProtocolRequest)
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Connecting),
            None
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Reconnecting),
            None
        );
    }

    #[test]
    fn force_recover_stays_available_for_connected_and_inflight_sessions() {
        for status in [
            RemoteDesktopSessionStatus::Idle,
            RemoteDesktopSessionStatus::Connecting,
            RemoteDesktopSessionStatus::Connected,
            RemoteDesktopSessionStatus::Reconnecting,
            RemoteDesktopSessionStatus::Disconnected,
            RemoteDesktopSessionStatus::Failed,
        ] {
            assert!(remote_desktop_force_recover_enabled(status));
        }
    }

    #[test]
    fn worker_generation_never_wraps_to_stale_zero() {
        assert_eq!(next_remote_desktop_worker_generation(0), 1);
        assert_eq!(next_remote_desktop_worker_generation(7), 8);
        assert_eq!(next_remote_desktop_worker_generation(u64::MAX), u64::MAX);
    }

    #[test]
    fn worker_delivery_adapter_restores_tab_mapping_at_the_app_boundary() {
        let tab_id = TabId(42);
        let generation = 7;
        let delivery = oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::FrameReady {
            worker_id: oxideterm_remote_desktop::RemoteDesktopWorkerId::new(
                oxideterm_remote_desktop::RemoteDesktopSessionId::new(),
                generation,
            ),
        };

        assert!(matches!(
            map_remote_desktop_worker_delivery(tab_id, generation, delivery),
            RemoteDesktopWorkerDelivery::FrameReady {
                tab_id: mapped_tab_id,
                generation: mapped_generation,
            } if mapped_tab_id == tab_id && mapped_generation == generation
        ));
    }

    #[test]
    fn requested_size_uses_physical_pixels_for_high_dpi_viewports() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert_eq!(
            remote_desktop_requested_size_for_viewport(viewport, Some(200)),
            RemoteDesktopSize {
                width: 3200,
                height: 1800,
            }
        );
        assert_eq!(
            remote_desktop_requested_size_for_viewport(viewport, None),
            viewport,
        );
    }

    #[test]
    fn requested_size_clamps_scaled_viewport_to_protocol_bounds() {
        let viewport = RemoteDesktopSize {
            width: 5000,
            height: 5000,
        };

        assert_eq!(
            remote_desktop_requested_size_for_viewport(viewport, Some(200)),
            RemoteDesktopSize {
                width: RemoteDesktopSize::MAX_DIMENSION,
                height: RemoteDesktopSize::MAX_DIMENSION,
            }
        );
    }

    #[test]
    fn resize_delta_ignores_border_sized_differences() {
        let previous = Some(RemoteDesktopSize {
            width: 1600,
            height: 900,
        });

        assert!(!remote_desktop_size_delta_is_meaningful(
            previous,
            RemoteDesktopSize {
                width: 1598,
                height: 898
            },
        ));
        assert!(remote_desktop_size_delta_is_meaningful(
            previous,
            RemoteDesktopSize {
                width: 1500,
                height: 900
            },
        ));
    }

    #[test]
    fn resize_scale_factor_matches_window_percent() {
        assert_eq!(remote_desktop_scale_factor_percent(1.0), 100);
        assert_eq!(remote_desktop_scale_factor_percent(1.25), 125);
        assert_eq!(remote_desktop_scale_factor_percent(5.0), 500);
        assert_eq!(remote_desktop_scale_factor_percent(0.75), 100);
        assert_eq!(remote_desktop_scale_factor_percent(5.25), 100);
        assert_eq!(remote_desktop_scale_factor_percent(0.0), 100);
        assert_eq!(remote_desktop_scale_factor_percent(f32::NAN), 100);
    }

    #[test]
    fn clipboard_image_item_maps_to_remote_desktop_data() {
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]));

        let data = remote_desktop_clipboard_data_from_item(&item).unwrap();

        assert_eq!(data.format, RemoteDesktopClipboardFormat::ImagePng);
        assert_eq!(data.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn remote_desktop_clipboard_data_maps_to_image_item() {
        let data =
            RemoteDesktopClipboardData::new(RemoteDesktopClipboardFormat::ImageJpeg, vec![4, 5, 6]);

        let item = remote_desktop_clipboard_item_from_data(&data).unwrap();

        assert!(matches!(
            item.entries(),
            [ClipboardEntry::Image(image)]
                if image.format == ImageFormat::Jpeg && image.bytes == vec![4, 5, 6]
        ));
    }

    #[test]
    fn mouse_button_mapping_forwards_navigation_buttons() {
        assert_eq!(
            remote_desktop_mouse_button_from_gpui(gpui::MouseButton::Navigate(
                gpui::NavigationDirection::Back
            )),
            Some(RemoteDesktopMouseButton::Back)
        );
        assert_eq!(
            remote_desktop_mouse_button_from_gpui(gpui::MouseButton::Navigate(
                gpui::NavigationDirection::Forward
            )),
            Some(RemoteDesktopMouseButton::Forward)
        );
    }

    #[test]
    fn pixel_wheel_delta_accumulates_until_full_notch() {
        let mut remainder = remote_desktop_empty_wheel_delta();

        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(60.0), gpui::px(0.0))),
                &mut remainder,
            ),
            None
        );
        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(60.0), gpui::px(0.0))),
                &mut remainder,
            ),
            Some(RemoteDesktopWheelDelta { x: 120.0, y: 0.0 })
        );
        assert_eq!(remainder, remote_desktop_empty_wheel_delta());
    }

    #[test]
    fn pixel_wheel_delta_drops_opposite_direction_remainder() {
        let mut remainder = remote_desktop_empty_wheel_delta();

        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(80.0), gpui::px(0.0))),
                &mut remainder,
            ),
            None
        );
        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(-120.0), gpui::px(0.0))),
                &mut remainder,
            ),
            Some(RemoteDesktopWheelDelta { x: -120.0, y: 0.0 })
        );
        assert_eq!(remainder, remote_desktop_empty_wheel_delta());
    }

    #[test]
    fn line_wheel_delta_resets_pixel_remainder() {
        let mut remainder = RemoteDesktopWheelDelta { x: 80.0, y: 40.0 };

        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Lines(gpui::point(0.0, 1.0)),
                &mut remainder,
            ),
            Some(RemoteDesktopWheelDelta {
                x: 0.0,
                y: REMOTE_DESKTOP_SCROLL_LINE,
            })
        );
        assert_eq!(remainder, remote_desktop_empty_wheel_delta());
    }

    #[test]
    fn resize_request_retries_when_initial_frame_size_differs_from_viewport() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            None,
            Some(viewport),
            None,
            viewport,
            viewport,
            Some(100),
        ));
    }

    #[test]
    fn resize_request_does_not_repeat_pending_retry() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            Some(viewport),
            Some(viewport),
            None,
            viewport,
            viewport,
            Some(100),
        ));
    }

    #[test]
    fn remote_desktop_clipboard_shortcuts_accept_physical_key_codes() {
        let mut modifiers = gpui::Modifiers::default();
        modifiers.control = true;

        assert!(remote_desktop_paste_shortcut(&gpui::Keystroke {
            modifiers,
            key: "KeyV".to_string(),
            key_char: Some("v".to_string()),
        }));
        assert!(remote_desktop_paste_shortcut(&gpui::Keystroke {
            modifiers,
            key: "keyv".to_string(),
            key_char: Some("v".to_string()),
        }));
        assert!(remote_desktop_copy_shortcut(&gpui::Keystroke {
            modifiers,
            key: "KeyC".to_string(),
            key_char: Some("c".to_string()),
        }));
    }

    #[test]
    fn remote_desktop_clipboard_shortcuts_release_forwarded_modifiers() {
        let mut modifiers = gpui::Modifiers::default();
        modifiers.control = true;
        modifiers.platform = true;
        modifiers.shift = true;

        let codes = remote_desktop_shortcut_modifier_release_codes(&gpui::Keystroke {
            modifiers,
            key: "KeyV".to_string(),
            key_char: Some("v".to_string()),
        });

        assert_eq!(codes, vec!["control", "meta", "shift"]);
    }

    #[test]
    fn modifier_sync_presses_new_modifier_state() {
        let next = RemoteDesktopModifierState {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
        };

        let requests =
            remote_desktop_modifier_sync_requests(RemoteDesktopModifierState::default(), next);

        assert_eq!(
            requests,
            vec![
                modifier_request("ShiftLeft", RemoteDesktopKeyState::Pressed),
                modifier_request("ControlLeft", RemoteDesktopKeyState::Pressed),
            ]
        );
    }

    #[test]
    fn modifier_sync_releases_cleared_modifier_state() {
        let previous = RemoteDesktopModifierState {
            shift: false,
            ctrl: true,
            alt: false,
            meta: true,
        };

        let requests =
            remote_desktop_modifier_sync_requests(previous, RemoteDesktopModifierState::default());

        assert_eq!(
            requests,
            vec![
                modifier_request("ControlLeft", RemoteDesktopKeyState::Released),
                modifier_request("MetaLeft", RemoteDesktopKeyState::Released),
            ]
        );
    }

    #[test]
    fn capslock_state_maps_to_rdp_lock_key_sync() {
        let keys = remote_desktop_lock_keys_with_capslock(None, gpui::Capslock { on: true });

        assert_eq!(
            keys,
            RemoteDesktopLockKeys {
                scroll_lock: false,
                num_lock: false,
                caps_lock: true,
                kana_lock: false,
            }
        );
        assert_eq!(
            remote_desktop_lock_key_sync_request(None, keys),
            Some(RemoteDesktopHelperRequest::SynchronizeLockKeys { keys })
        );
        assert_eq!(remote_desktop_lock_key_sync_request(Some(keys), keys), None);
    }

    #[test]
    fn capslock_sync_preserves_estimated_lock_keys() {
        let previous = RemoteDesktopLockKeys {
            scroll_lock: true,
            num_lock: true,
            caps_lock: false,
            kana_lock: true,
        };

        let keys =
            remote_desktop_lock_keys_with_capslock(Some(previous), gpui::Capslock { on: true });

        assert_eq!(
            keys,
            RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: true,
                caps_lock: true,
                kana_lock: true,
            }
        );
    }

    #[test]
    fn lock_key_press_toggles_estimated_non_caps_states() {
        let after_num_lock = remote_desktop_lock_keys_after_pressed_code(None, "NumLock").unwrap();
        assert_eq!(
            after_num_lock,
            RemoteDesktopLockKeys {
                num_lock: true,
                ..RemoteDesktopLockKeys::default()
            }
        );

        let after_scroll_lock =
            remote_desktop_lock_keys_after_pressed_code(Some(after_num_lock), "Scroll_Lock")
                .unwrap();
        assert_eq!(
            after_scroll_lock,
            RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: true,
                ..RemoteDesktopLockKeys::default()
            }
        );

        let after_kana =
            remote_desktop_lock_keys_after_pressed_code(Some(after_scroll_lock), "KanaMode")
                .unwrap();
        assert_eq!(
            after_kana,
            RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: true,
                kana_lock: true,
                ..RemoteDesktopLockKeys::default()
            }
        );
        assert_eq!(
            remote_desktop_lock_keys_after_pressed_code(Some(after_kana), "CapsLock"),
            None
        );
    }

    fn modifier_request(code: &str, state: RemoteDesktopKeyState) -> RemoteDesktopHelperRequest {
        RemoteDesktopHelperRequest::Key {
            key: RemoteDesktopKey {
                code: code.to_string(),
                text: None,
                alt: false,
                ctrl: false,
                shift: false,
                meta: false,
            },
            state,
        }
    }

    #[test]
    fn resize_request_does_not_repeat_ignored_retry() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            None,
            Some(viewport),
            Some(resize_state(viewport, Some(100))),
            viewport,
            viewport,
            Some(100),
        ));
    }

    #[test]
    fn resize_request_skips_when_frame_already_matches_viewport() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(viewport),
            None,
            Some(viewport),
            None,
            viewport,
            viewport,
            None,
        ));
    }

    #[test]
    fn resize_request_does_not_duplicate_initial_scaled_connect() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };
        let request_size = RemoteDesktopSize {
            width: 3200,
            height: 1800,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(request_size),
            None,
            Some(viewport),
            None,
            viewport,
            request_size,
            Some(200),
        ));
    }

    #[test]
    fn resize_request_sends_scale_only_change_once() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(remote_desktop_resize_request_needed(
            Some(viewport),
            None,
            Some(viewport),
            Some(resize_state(viewport, Some(100))),
            viewport,
            viewport,
            Some(125),
        ));
        assert!(!remote_desktop_resize_request_needed(
            Some(viewport),
            None,
            Some(viewport),
            Some(resize_state(viewport, Some(125))),
            viewport,
            viewport,
            Some(125),
        ));
    }

    #[test]
    fn resize_request_can_replace_pending_scale_change() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            Some(viewport),
            Some(viewport),
            Some(resize_state(viewport, Some(100))),
            viewport,
            viewport,
            Some(125),
        ));
    }

    #[test]
    fn resize_request_is_blocked_when_provider_does_not_support_resize() {
        let current = RemoteDesktopSize {
            width: 1280,
            height: 720,
        };
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed_for_capability(
            false,
            Some(current),
            None,
            Some(current),
            None,
            viewport,
            viewport,
            Some(100),
        ));
        assert!(remote_desktop_resize_request_needed_for_capability(
            true,
            Some(current),
            None,
            Some(current),
            None,
            viewport,
            viewport,
            Some(100),
        ));
    }

    fn resize_state(
        size: RemoteDesktopSize,
        scale_factor: Option<u32>,
    ) -> RemoteDesktopResizeRequestState {
        RemoteDesktopResizeRequestState { size, scale_factor }
    }
}
