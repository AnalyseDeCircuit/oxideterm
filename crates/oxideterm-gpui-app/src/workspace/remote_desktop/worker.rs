// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn preview_remote_desktop_profile(
    protocol: RemoteDesktopProtocol,
) -> RemoteDesktopConnectionProfile {
    let label = match protocol {
        RemoteDesktopProtocol::Rdp => "RDP Preview",
        RemoteDesktopProtocol::Vnc => "VNC Preview",
        RemoteDesktopProtocol::Spice => "SPICE Preview",
    };

    RemoteDesktopConnectionProfile {
        id: format!("preview-{}", protocol.provider_id()),
        label: label.to_string(),
        protocol,
        endpoint: RemoteDesktopEndpoint::for_protocol("preview.local", protocol),
        transport_endpoint: None,
        username: None,
        domain: None,
        credential_ref: None,
        sasl_credential_ref: None,
        read_only: false,
        session_options: Default::default(),
    }
}

pub(super) fn run_remote_desktop_worker(
    tab_id: TabId,
    generation: u64,
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    spice_ticket: Option<SpiceSecret>,
    spice_sasl_password: Option<SpiceSecret>,
    password_available: bool,
    initial_size: RemoteDesktopSize,
    scale_factor: Option<u32>,
    monitor_layout: RemoteDesktopMonitorLayout,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    worker_wake: RemoteDesktopWorkerWake,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    spice_tool_request_rx: Option<mpsc::Receiver<SpiceWorkerRequest>>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    if profile.protocol == RemoteDesktopProtocol::Spice {
        run_spice_remote_desktop_worker(
            tab_id,
            generation,
            profile,
            spice_ticket.unwrap_or_else(|| SpiceSecret::new(String::new())),
            spice_sasl_password,
            initial_size,
            monitor_layout,
            frame_slot,
            worker_wake,
            request_rx,
            spice_tool_request_rx.expect("SPICE sessions own a tool request channel"),
            delivery_tx,
        );
        return;
    }
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

#[allow(clippy::too_many_arguments)]
fn run_spice_remote_desktop_worker(
    tab_id: TabId,
    generation: u64,
    profile: RemoteDesktopConnectionProfile,
    ticket: SpiceSecret,
    sasl_password: Option<SpiceSecret>,
    initial_size: RemoteDesktopSize,
    monitor_layout: RemoteDesktopMonitorLayout,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    worker_wake: RemoteDesktopWorkerWake,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    spice_tool_request_rx: mpsc::Receiver<SpiceWorkerRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let worker_id = oxideterm_remote_desktop::RemoteDesktopWorkerId::new(
        oxideterm_remote_desktop::RemoteDesktopSessionId::new(),
        generation,
    );
    let helper = match resolve_spice_helper_command() {
        Ok(helper) => helper,
        Err(error) => {
            send_remote_desktop_worker_delivery(
                &delivery_tx,
                &worker_wake,
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message: error.to_string(),
                },
            );
            worker_wake.stop();
            return;
        }
    };
    let audio_playback = profile.session_options.audio.playback;
    let audio_capture = profile.session_options.audio.capture;
    let connect = match spice_connect_options(profile, ticket, sasl_password) {
        Ok(connect) => connect,
        Err(message) => {
            send_remote_desktop_worker_delivery(
                &delivery_tx,
                &worker_wake,
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message,
                },
            );
            worker_wake.stop();
            return;
        }
    };
    let (spice_request_tx, spice_request_rx) = crossbeam_channel::bounded(256);
    let (spice_delivery_tx, spice_delivery_rx) = mpsc::channel();
    let spice_worker = thread::Builder::new()
        .name(format!("spice-worker-{}", tab_id.0))
        .spawn(move || {
            oxideterm_spice::run_spice_worker(
                SpiceWorkerConfig {
                    worker_id,
                    helper,
                    connect,
                    frame_slot,
                    audio_playback,
                    audio_capture,
                },
                spice_request_rx,
                spice_delivery_tx,
            );
        });
    let Ok(spice_worker) = spice_worker else {
        send_remote_desktop_worker_delivery(
            &delivery_tx,
            &worker_wake,
            RemoteDesktopWorkerDelivery::TransportFailed {
                tab_id,
                generation,
                message: "failed to start the SPICE worker".to_string(),
            },
        );
        worker_wake.stop();
        return;
    };

    let mut adapter = SpiceRemoteDesktopAdapter::new(initial_size, monitor_layout);
    let mut file_uploads = SpiceFileUploadRuntime::default();
    let mut closing = false;
    let mut terminated = false;
    while !terminated {
        terminated |= deliver_spice_file_upload_actions(
            tab_id,
            generation,
            file_uploads.poll(),
            &spice_request_tx,
            &delivery_tx,
            &worker_wake,
        );
        while let Ok(delivery) = spice_delivery_rx.try_recv() {
            if let SpiceWorkerDelivery::Event { event, .. } = &delivery {
                terminated |= deliver_spice_file_upload_actions(
                    tab_id,
                    generation,
                    file_uploads.handle_event(event),
                    &spice_request_tx,
                    &delivery_tx,
                    &worker_wake,
                );
            }
            terminated |= deliver_spice_worker_event(
                tab_id,
                generation,
                delivery,
                &mut adapter,
                &spice_request_tx,
                &delivery_tx,
                &worker_wake,
            );
        }
        if terminated {
            break;
        }
        while let Ok(request) = spice_tool_request_rx.try_recv() {
            // Handshake and process shutdown remain owned by this adapter, not UI tools.
            if matches!(
                request,
                SpiceWorkerRequest::Hello { .. }
                    | SpiceWorkerRequest::Connect { .. }
                    | SpiceWorkerRequest::Close
            ) {
                continue;
            }
            if spice_request_tx.send(request).is_err() {
                terminated = true;
                break;
            }
        }
        if terminated {
            break;
        }
        match request_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(request) => {
                let close = matches!(request, RemoteDesktopHelperRequest::Close);
                let upload_actions = match request {
                    RemoteDesktopHelperRequest::ClipboardFiles { transfer_id, paths } => {
                        file_uploads.start_group(transfer_id, paths)
                    }
                    RemoteDesktopHelperRequest::CancelClipboardTransfer { transfer_id } => {
                        file_uploads.cancel_group(&transfer_id)
                    }
                    request => {
                        for request in adapter.map_request(request) {
                            if spice_request_tx.send(request).is_err() {
                                terminated = true;
                                break;
                            }
                        }
                        Vec::new()
                    }
                };
                terminated |= deliver_spice_file_upload_actions(
                    tab_id,
                    generation,
                    upload_actions,
                    &spice_request_tx,
                    &delivery_tx,
                    &worker_wake,
                );
                if terminated {
                    break;
                }
                closing |= close;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) if !closing => {
                let _ = spice_request_tx.send(SpiceWorkerRequest::Close);
                closing = true;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }
    }
    drop(spice_request_tx);
    let _ = spice_worker.join();
    worker_wake.stop();
}

fn deliver_spice_file_upload_actions(
    tab_id: TabId,
    generation: u64,
    actions: Vec<SpiceFileUploadAction>,
    request_tx: &crossbeam_channel::Sender<SpiceWorkerRequest>,
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_wake: &RemoteDesktopWorkerWake,
) -> bool {
    for action in actions {
        match action {
            SpiceFileUploadAction::Request(request) => {
                if request_tx.send(request).is_err() {
                    return true;
                }
            }
            SpiceFileUploadAction::Failed { group_id, message } => {
                send_remote_desktop_worker_delivery(
                    delivery_tx,
                    worker_wake,
                    RemoteDesktopWorkerDelivery::Event {
                        tab_id,
                        generation,
                        event: RemoteDesktopHelperEvent::ClipboardTransferFailed {
                            transfer_id: group_id,
                            message,
                        },
                    },
                );
            }
        }
    }
    false
}

fn spice_connect_options(
    profile: RemoteDesktopConnectionProfile,
    ticket: SpiceSecret,
    sasl_password: Option<SpiceSecret>,
) -> Result<SpiceConnectOptions, String> {
    let public_host = profile.endpoint.host.clone();
    let endpoint = profile.transport_endpoint.unwrap_or(profile.endpoint);
    let spice = profile.session_options.spice;
    let transport_security = match spice.transport_security {
        oxideterm_remote_desktop::RemoteDesktopSpiceTransportSecurity::Plain => {
            SpiceTransportSecurity::Plain
        }
        oxideterm_remote_desktop::RemoteDesktopSpiceTransportSecurity::Tls => {
            if spice.tls_root_certificates_der.is_empty() {
                return Err("SPICE TLS requires at least one root certificate".to_string());
            }
            SpiceTransportSecurity::Tls {
                server_name: spice.tls_server_name.unwrap_or_else(|| public_host.clone()),
                root_certificates_der: spice.tls_root_certificates_der,
            }
        }
    };
    let sasl_hostname = spice.sasl_hostname.unwrap_or_else(|| public_host.clone());
    let sasl = match spice.sasl_mode {
        oxideterm_remote_desktop::RemoteDesktopSpiceSaslMode::Disabled => None,
        oxideterm_remote_desktop::RemoteDesktopSpiceSaslMode::Gssapi => Some(SpiceSasl::Gssapi {
            hostname: sasl_hostname,
            service: spice.sasl_service,
        }),
        oxideterm_remote_desktop::RemoteDesktopSpiceSaslMode::Password => {
            let Some(password) = sasl_password.filter(|password| !password.is_empty()) else {
                return Err("The SPICE SASL password is unavailable for this session".to_string());
            };
            let authentication_id = spice
                .sasl_authentication_id
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "SPICE SASL requires an authentication identity".to_string())?;
            Some(SpiceSasl::Password {
                hostname: sasl_hostname,
                service: spice.sasl_service,
                authentication_id,
                authorization_id: spice.sasl_authorization_id,
                password,
                allow_gssapi: spice.sasl_allow_gssapi,
            })
        }
    };
    Ok(SpiceConnectOptions {
        endpoint: SpiceEndpoint::Tcp {
            host: endpoint.host,
            port: endpoint.port,
        },
        ticket,
        transport_security,
        sasl,
    })
}

fn deliver_spice_worker_event(
    tab_id: TabId,
    generation: u64,
    delivery: SpiceWorkerDelivery,
    adapter: &mut SpiceRemoteDesktopAdapter,
    request_tx: &crossbeam_channel::Sender<SpiceWorkerRequest>,
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_wake: &RemoteDesktopWorkerWake,
) -> bool {
    let delivery = match delivery {
        SpiceWorkerDelivery::FrameReady { worker_id } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation }
        }
        SpiceWorkerDelivery::FrameRecoveryRequired { worker_id } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::FrameRecoveryRequired { tab_id, generation }
        }
        SpiceWorkerDelivery::RemoteDesktopEvent { worker_id, event } => {
            debug_assert_eq!(worker_id.request_id, generation);
            RemoteDesktopWorkerDelivery::Event {
                tab_id,
                generation,
                event,
            }
        }
        SpiceWorkerDelivery::Event { worker_id, event } => {
            debug_assert_eq!(worker_id.request_id, generation);
            let actions = adapter.map_event(event);
            if let Some(response) = actions.response {
                let _ = request_tx.send(response);
            }
            if let Some(event) = actions.shared_event {
                send_remote_desktop_worker_delivery(
                    delivery_tx,
                    worker_wake,
                    RemoteDesktopWorkerDelivery::Event {
                        tab_id,
                        generation,
                        event,
                    },
                );
            }
            let Some(event) = actions.event else {
                return false;
            };
            RemoteDesktopWorkerDelivery::SpiceEvent {
                tab_id,
                generation,
                event,
            }
        }
        SpiceWorkerDelivery::TransportFailed { worker_id, message } => {
            debug_assert_eq!(worker_id.request_id, generation);
            send_remote_desktop_worker_delivery(
                delivery_tx,
                worker_wake,
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message,
                },
            );
            return true;
        }
        SpiceWorkerDelivery::Terminated {
            worker_id,
            exit_code,
        } => {
            debug_assert_eq!(worker_id.request_id, generation);
            send_remote_desktop_worker_delivery(
                delivery_tx,
                worker_wake,
                RemoteDesktopWorkerDelivery::Event {
                    tab_id,
                    generation,
                    event: RemoteDesktopHelperEvent::Terminated { exit_code },
                },
            );
            return true;
        }
    };
    send_remote_desktop_worker_delivery(delivery_tx, worker_wake, delivery);
    false
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
    session: &RemoteDesktopSessionEntity,
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
