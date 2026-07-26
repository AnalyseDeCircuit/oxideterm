// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn preview_remote_desktop_profile(
    protocol: RemoteDesktopProtocol,
) -> RemoteDesktopConnectionProfile {
    let label = match protocol {
        RemoteDesktopProtocol::Rdp => "RDP Preview",
        RemoteDesktopProtocol::Vnc => "VNC Preview",
    };

    RemoteDesktopConnectionProfile {
        id: format!("preview-{}", protocol.provider_id()),
        label: label.to_string(),
        protocol,
        endpoint: RemoteDesktopEndpoint::for_protocol("preview.local", protocol),
        username: None,
        domain: None,
        credential_ref: None,
        read_only: false,
        session_options: Default::default(),
    }
}

pub(super) fn run_remote_desktop_worker(
    tab_id: TabId,
    generation: u64,
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    password_available: bool,
    initial_size: RemoteDesktopSize,
    scale_factor: Option<u32>,
    monitor_layout: RemoteDesktopMonitorLayout,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    worker_wake: RemoteDesktopWorkerWake,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let worker_id = oxideterm_remote_desktop::RemoteDesktopWorkerId::new(
        oxideterm_remote_desktop::RemoteDesktopSessionId::new(),
        generation,
    );
    let (domain_delivery_tx, domain_delivery_rx) = mpsc::channel();
    let bridge_wake = worker_wake.clone();
    let bridge_thread = thread::Builder::new()
        .name(format!("remote-desktop-delivery-{}", tab_id.0))
        .spawn(move || {
            while let Ok(delivery) = domain_delivery_rx.recv() {
                let delivery = map_remote_desktop_worker_delivery(tab_id, generation, delivery);
                send_remote_desktop_worker_delivery(&delivery_tx, &bridge_wake, delivery);
            }
        })
        .ok();

    oxideterm_remote_desktop::run_remote_desktop_worker(
        oxideterm_remote_desktop::RemoteDesktopWorkerConfig {
            worker_id,
            profile,
            provider,
            password_available,
            initial_size,
            scale_factor,
            monitor_layout,
        },
        frame_slot,
        request_rx,
        domain_delivery_tx,
    );
    if let Some(bridge_thread) = bridge_thread {
        // Stop only after all domain deliveries have crossed the TabId adapter.
        let _ = bridge_thread.join();
    }
    worker_wake.stop();
}

pub(super) fn map_remote_desktop_worker_delivery(
    tab_id: TabId,
    generation: u64,
    delivery: oxideterm_remote_desktop::RemoteDesktopWorkerDelivery,
) -> RemoteDesktopWorkerDelivery {
    match delivery {
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::FrameReady { worker_id } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation }
        }
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::FrameRecoveryRequired {
            worker_id,
        } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::FrameRecoveryRequired { tab_id, generation }
        }
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::Event { worker_id, event } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::Event {
                tab_id,
                generation,
                event,
            }
        }
        oxideterm_remote_desktop::RemoteDesktopWorkerDelivery::TransportFailed {
            worker_id,
            message,
        } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::TransportFailed {
                tab_id,
                generation,
                message,
            }
        }
    }
}

pub(super) fn default_remote_desktop_initial_size() -> RemoteDesktopSize {
    RemoteDesktopSize::clamped(REMOTE_DESKTOP_INITIAL_WIDTH, REMOTE_DESKTOP_INITIAL_HEIGHT)
}

pub(super) fn initial_remote_desktop_sizes_for_session(
    session: &RemoteDesktopSession,
) -> (RemoteDesktopSize, Option<RemoteDesktopSize>) {
    if let Some(viewport_size) = session.geometry.viewport_size() {
        let viewport_size = RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
        return (
            remote_desktop_requested_size_for_viewport(
                viewport_size,
                session.last_viewport_scale_factor,
            ),
            Some(viewport_size),
        );
    }

    (
        session
            .state
            .snapshot()
            .size
            .unwrap_or_else(default_remote_desktop_initial_size),
        None,
    )
}

pub(super) fn remote_desktop_scale_factor_percent(scale_factor: f32) -> u32 {
    let percent = (scale_factor * REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER).round();
    if percent.is_finite() {
        let percent = percent as u32;
        if (REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT..=REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT)
            .contains(&percent)
        {
            return percent;
        }
    }
    REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT
}

pub(super) fn remote_desktop_requested_size_for_viewport(
    viewport_size: RemoteDesktopSize,
    scale_factor: Option<u32>,
) -> RemoteDesktopSize {
    let viewport_size = RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
    let Some(scale_factor) = scale_factor else {
        return viewport_size;
    };
    if !(REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT..=REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT)
        .contains(&scale_factor)
    {
        return viewport_size;
    }

    // GPUI canvas bounds are logical pixels; RDP desktop_size is the remote
    // framebuffer pixel size, so high-DPI windows need an explicit conversion.
    let denominator = u64::from(REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT);
    let scale_factor = u64::from(scale_factor);
    let width = remote_desktop_scaled_dimension(viewport_size.width, scale_factor, denominator);
    let height = remote_desktop_scaled_dimension(viewport_size.height, scale_factor, denominator);
    RemoteDesktopSize::clamped(width, height)
}

pub(super) fn remote_desktop_scaled_dimension(
    value: u32,
    scale_factor: u64,
    denominator: u64,
) -> u32 {
    let scaled = (u64::from(value) * scale_factor + denominator / 2) / denominator;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

pub(super) fn remote_desktop_resize_request_needed(
    current_frame_size: Option<RemoteDesktopSize>,
    pending_resize: Option<RemoteDesktopSize>,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    viewport_size: RemoteDesktopSize,
    request_size: RemoteDesktopSize,
    viewport_scale_factor: Option<u32>,
) -> bool {
    let next_request = RemoteDesktopResizeRequestState {
        size: request_size,
        scale_factor: viewport_scale_factor,
    };
    if Some(next_request) == last_sent_resize {
        return false;
    }

    let frame_mismatch = remote_desktop_size_delta_is_meaningful(current_frame_size, request_size)
        && Some(request_size) != current_frame_size;
    let viewport_changed = Some(viewport_size) != last_viewport_size;
    let scale_changed = viewport_scale_factor.is_some()
        && last_sent_resize
            .is_some_and(|last_sent| last_sent.scale_factor != viewport_scale_factor);
    if !viewport_changed && !frame_mismatch && !scale_changed {
        return false;
    }
    if !frame_mismatch {
        return scale_changed;
    }
    if Some(request_size) == pending_resize {
        return scale_changed && last_sent_resize.is_some();
    }
    let last_sent_size = last_sent_resize.map(|last_sent| last_sent.size);
    if !remote_desktop_size_delta_is_meaningful(last_sent_size, request_size) && !scale_changed {
        return false;
    }
    true
}

pub(super) fn remote_desktop_resize_request_needed_for_capability(
    resize_supported: bool,
    current_frame_size: Option<RemoteDesktopSize>,
    pending_resize: Option<RemoteDesktopSize>,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    viewport_size: RemoteDesktopSize,
    request_size: RemoteDesktopSize,
    viewport_scale_factor: Option<u32>,
) -> bool {
    // VNC's built-in provider has a fixed server framebuffer; viewport changes
    // still update local geometry, but they must not create remote resize state.
    resize_supported
        && remote_desktop_resize_request_needed(
            current_frame_size,
            pending_resize,
            last_viewport_size,
            last_sent_resize,
            viewport_size,
            request_size,
            viewport_scale_factor,
        )
}

pub(super) fn remote_desktop_size_delta_is_meaningful(
    previous: Option<RemoteDesktopSize>,
    next: RemoteDesktopSize,
) -> bool {
    previous.is_none_or(|previous| {
        previous.width.abs_diff(next.width) >= REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD
            || previous.height.abs_diff(next.height) >= REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD
    })
}
