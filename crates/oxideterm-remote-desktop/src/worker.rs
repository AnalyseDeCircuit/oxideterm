// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{io::BufReader, sync::mpsc, thread, time::Duration};

use crate::{
    RemoteDesktopConnectionProfile, RemoteDesktopFakeBackend, RemoteDesktopFrameDeliverySlot,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopMonitorLayout,
    RemoteDesktopProviderManifest, RemoteDesktopSecret, RemoteDesktopSessionId,
    RemoteDesktopSessionOptions, RemoteDesktopSize, is_remote_desktop_frame_event, read_event_line,
};
use crate::{helper_process, request_writer};

const HELPER_CLOSE_GRACE_PERIOD: Duration = Duration::from_secs(2);
const HELPER_LIVENESS_CHECK_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RemoteDesktopReaderOutcome {
    terminal_delivery_sent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteDesktopWorkerId {
    pub session_id: RemoteDesktopSessionId,
    pub request_id: u64,
}

impl RemoteDesktopWorkerId {
    pub fn new(session_id: RemoteDesktopSessionId, request_id: u64) -> Self {
        Self {
            session_id,
            request_id,
        }
    }
}

#[derive(Debug)]
pub enum RemoteDesktopWorkerDelivery {
    FrameReady {
        worker_id: RemoteDesktopWorkerId,
    },
    FrameRecoveryRequired {
        worker_id: RemoteDesktopWorkerId,
    },
    Event {
        worker_id: RemoteDesktopWorkerId,
        event: RemoteDesktopHelperEvent,
    },
    TransportFailed {
        worker_id: RemoteDesktopWorkerId,
        message: String,
    },
}

pub struct RemoteDesktopWorkerConfig {
    pub worker_id: RemoteDesktopWorkerId,
    pub profile: RemoteDesktopConnectionProfile,
    pub provider: RemoteDesktopProviderManifest,
    pub password_available: bool,
    pub initial_size: RemoteDesktopSize,
    pub scale_factor: Option<u32>,
    pub monitor_layout: RemoteDesktopMonitorLayout,
}

pub fn run_remote_desktop_worker(
    config: RemoteDesktopWorkerConfig,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    run_remote_desktop_worker_with_close_grace(
        config,
        frame_slot,
        request_rx,
        delivery_tx,
        HELPER_CLOSE_GRACE_PERIOD,
    );
}

fn run_remote_desktop_worker_with_close_grace(
    config: RemoteDesktopWorkerConfig,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    close_grace_period: Duration,
) {
    match helper_process::spawn_remote_desktop_helper(&config.provider) {
        Ok(mut helper) => {
            let stdout = helper.child.stdout.take();
            let connect = initial_connect_request(
                &config.profile,
                &config.provider,
                config.password_available,
                config.initial_size,
                config.scale_factor,
                config.monitor_layout,
            );
            if let Err(error) = helper_process::write_initial_remote_desktop_connect(
                &mut helper.child,
                &mut helper.stdin,
                &connect,
            ) {
                send_delivery(
                    &delivery_tx,
                    RemoteDesktopWorkerDelivery::TransportFailed {
                        worker_id: config.worker_id,
                        message: error.to_string(),
                    },
                );
                return;
            }

            let reader_thread = stdout.and_then(|stdout| {
                let reader_worker_id = config.worker_id.clone();
                let reader_tx = delivery_tx.clone();
                let reader_frame_slot = frame_slot.clone();
                thread::Builder::new()
                    .name(format!(
                        "remote-desktop-reader-{}",
                        reader_worker_id.request_id
                    ))
                    .spawn(move || {
                        read_remote_desktop_events(
                            reader_worker_id,
                            stdout,
                            reader_tx,
                            reader_frame_slot,
                        )
                    })
                    .ok()
            });

            let mut exit_status = None;
            let mut pending_transport_failure = None;
            loop {
                match helper.child.try_wait() {
                    Ok(Some(status)) => {
                        exit_status = Some(status);
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        pending_transport_failure = Some(error.to_string());
                        break;
                    }
                }

                match request_writer::write_remote_desktop_request_batch(
                    &mut helper.stdin,
                    &request_rx,
                    HELPER_LIVENESS_CHECK_INTERVAL,
                ) {
                    Ok(request_writer::RemoteDesktopRequestBatchOutcome::Continue) => {}
                    Ok(request_writer::RemoteDesktopRequestBatchOutcome::ShutdownRequested) => {
                        break;
                    }
                    Err(error) => {
                        pending_transport_failure = Some(error.to_string());
                        break;
                    }
                }
            }
            drop(helper.stdin);
            // Close is cooperative first. A helper that ignores the protocol is
            // killed and reaped only after the session-scoped grace period.
            let exit_code = exit_status
                .or_else(|| {
                    helper_process::wait_for_remote_desktop_helper_exit(
                        &mut helper.child,
                        close_grace_period,
                        HELPER_LIVENESS_CHECK_INTERVAL,
                    )
                })
                .and_then(|status| status.code());
            // The reader owns helper-reported termination semantics. Join it before deciding
            // whether a writer-side pipe error is the only available failure reason.
            let reader_outcome = reader_thread
                .and_then(|reader_thread| reader_thread.join().ok())
                .unwrap_or_default();
            deliver_pending_transport_failure(
                &delivery_tx,
                &config.worker_id,
                pending_transport_failure,
                reader_outcome,
            );
            send_delivery(
                &delivery_tx,
                RemoteDesktopWorkerDelivery::Event {
                    worker_id: config.worker_id,
                    event: RemoteDesktopHelperEvent::Terminated { exit_code },
                },
            );
            return;
        }
        Err(error) if !remote_desktop_provider_uses_fake_backend(&config.provider) => {
            send_delivery(
                &delivery_tx,
                RemoteDesktopWorkerDelivery::TransportFailed {
                    worker_id: config.worker_id,
                    message: format!("Remote desktop helper failed to start: {error}"),
                },
            );
            return;
        }
        Err(_) => {}
    }

    // Preview providers alone may fall back to the in-process fake backend.
    run_fake_worker(config, frame_slot, request_rx, delivery_tx);
}

pub fn connect_request(
    profile: &RemoteDesktopConnectionProfile,
    password: Option<RemoteDesktopSecret>,
    initial_size: RemoteDesktopSize,
    scale_factor: Option<u32>,
) -> RemoteDesktopHelperRequest {
    RemoteDesktopHelperRequest::Connect {
        protocol: profile.protocol,
        endpoint: profile.endpoint.clone(),
        username: profile.username.clone(),
        // Credentials cross the process boundary without entering the profile model.
        password,
        domain: profile.domain.clone(),
        size: RemoteDesktopSize::clamped(initial_size.width, initial_size.height),
        scale_factor,
        read_only: profile.read_only,
    }
}

pub fn initial_connect_request(
    profile: &RemoteDesktopConnectionProfile,
    provider: &RemoteDesktopProviderManifest,
    password_available: bool,
    initial_size: RemoteDesktopSize,
    scale_factor: Option<u32>,
    monitor_layout: RemoteDesktopMonitorLayout,
) -> RemoteDesktopHelperRequest {
    let session_options = effective_session_options(profile.session_options, provider);
    let monitor_layout = if session_options.display.use_all_monitors {
        monitor_layout
    } else {
        RemoteDesktopMonitorLayout::default()
    };

    RemoteDesktopHelperRequest::StartConnect {
        protocol: profile.protocol,
        endpoint: profile.endpoint.clone(),
        password_available,
        size: RemoteDesktopSize::clamped(initial_size.width, initial_size.height),
        scale_factor,
        read_only: profile.read_only,
        session_options,
        monitor_layout,
    }
}

pub fn effective_session_options(
    requested: RemoteDesktopSessionOptions,
    provider: &RemoteDesktopProviderManifest,
) -> RemoteDesktopSessionOptions {
    let capabilities = &provider.capabilities;
    RemoteDesktopSessionOptions {
        clipboard: crate::RemoteDesktopClipboardOptions {
            text: requested.clipboard.text && capabilities.clipboard_text,
            images: requested.clipboard.images && capabilities.clipboard_data,
            files: requested.clipboard.files && capabilities.clipboard_files,
        },
        audio: crate::RemoteDesktopAudioOptions {
            playback: requested.audio.playback && capabilities.audio_playback,
            capture: requested.audio.capture && capabilities.audio_capture,
        },
        display: crate::RemoteDesktopDisplayOptions {
            use_all_monitors: requested.display.use_all_monitors && capabilities.multi_monitor,
        },
        // VNC connection preferences are policy inputs, not negotiated provider capabilities.
        vnc: requested.vnc,
    }
}

pub fn remote_desktop_provider_uses_fake_backend(provider: &RemoteDesktopProviderManifest) -> bool {
    provider.entry.args.iter().any(|arg| arg == "--fake")
}

fn read_remote_desktop_events(
    worker_id: RemoteDesktopWorkerId,
    stdout: impl std::io::Read,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    frame_slot: RemoteDesktopFrameDeliverySlot,
) -> RemoteDesktopReaderOutcome {
    let mut reader = BufReader::new(stdout);
    let mut outcome = RemoteDesktopReaderOutcome::default();
    loop {
        match read_event_line(&mut reader) {
            Ok(Some(event)) => {
                if is_terminal_helper_event(&event) {
                    outcome.terminal_delivery_sent = true;
                }
                deliver_worker_event(&worker_id, event, &delivery_tx, &frame_slot);
            }
            Ok(None) => break,
            Err(error) => {
                if !outcome.terminal_delivery_sent {
                    send_delivery(
                        &delivery_tx,
                        RemoteDesktopWorkerDelivery::TransportFailed {
                            worker_id,
                            message: error.to_string(),
                        },
                    );
                    outcome.terminal_delivery_sent = true;
                }
                break;
            }
        }
    }
    outcome
}

fn is_terminal_helper_event(event: &RemoteDesktopHelperEvent) -> bool {
    matches!(
        event,
        RemoteDesktopHelperEvent::ConnectionFailure { .. }
            | RemoteDesktopHelperEvent::Disconnected { .. }
            | RemoteDesktopHelperEvent::Terminated { .. }
    )
}

fn deliver_pending_transport_failure(
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_id: &RemoteDesktopWorkerId,
    pending_message: Option<String>,
    reader_outcome: RemoteDesktopReaderOutcome,
) {
    let Some(message) = pending_message else {
        return;
    };
    if reader_outcome.terminal_delivery_sent {
        return;
    }
    send_delivery(
        delivery_tx,
        RemoteDesktopWorkerDelivery::TransportFailed {
            worker_id: worker_id.clone(),
            message,
        },
    );
}

fn deliver_worker_event(
    worker_id: &RemoteDesktopWorkerId,
    event: RemoteDesktopHelperEvent,
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    frame_slot: &RemoteDesktopFrameDeliverySlot,
) {
    if !is_remote_desktop_frame_event(&event) {
        send_delivery(
            delivery_tx,
            RemoteDesktopWorkerDelivery::Event {
                worker_id: worker_id.clone(),
                event,
            },
        );
        return;
    }

    let decision = frame_slot.push(event);
    if decision.recovery_required {
        send_delivery(
            delivery_tx,
            RemoteDesktopWorkerDelivery::FrameRecoveryRequired {
                worker_id: worker_id.clone(),
            },
        );
    }
    if decision.frame_ready {
        send_delivery(
            delivery_tx,
            RemoteDesktopWorkerDelivery::FrameReady {
                worker_id: worker_id.clone(),
            },
        );
    }
}

fn run_fake_worker(
    config: RemoteDesktopWorkerConfig,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let mut backend = RemoteDesktopFakeBackend::new(config.profile.protocol);
    for event in backend.handle_request(connect_request(
        &config.profile,
        None,
        config.initial_size,
        config.scale_factor,
    )) {
        deliver_worker_event(&config.worker_id, event, &delivery_tx, &frame_slot);
    }

    for request in request_rx {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        for event in backend.handle_request(request) {
            deliver_worker_event(&config.worker_id, event, &delivery_tx, &frame_slot);
        }
        if should_close {
            break;
        }
    }
}

fn send_delivery(
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    delivery: RemoteDesktopWorkerDelivery,
) {
    let _ = delivery_tx.send(delivery);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RemoteDesktopEndpoint, RemoteDesktopProtocol, builtin_preview_provider_registry,
        builtin_provider_manifest,
    };

    fn profile() -> RemoteDesktopConnectionProfile {
        RemoteDesktopConnectionProfile {
            id: "preview-rdp".to_string(),
            label: "RDP Preview".to_string(),
            protocol: RemoteDesktopProtocol::Rdp,
            endpoint: RemoteDesktopEndpoint::for_protocol(
                "preview.local",
                RemoteDesktopProtocol::Rdp,
            ),
            username: None,
            domain: None,
            credential_ref: None,
            read_only: false,
            session_options: RemoteDesktopSessionOptions::default(),
        }
    }

    #[test]
    fn connect_request_preserves_measured_size_and_scale() {
        let request = connect_request(
            &profile(),
            None,
            RemoteDesktopSize {
                width: 1600,
                height: 900,
            },
            Some(200),
        );

        assert!(matches!(
            request,
            RemoteDesktopHelperRequest::Connect {
                size: RemoteDesktopSize {
                    width: 1600,
                    height: 900
                },
                scale_factor: Some(200),
                ..
            }
        ));
    }

    #[test]
    fn staged_vnc_preflight_sends_only_password_availability() {
        let mut vnc_profile = profile();
        vnc_profile.protocol = RemoteDesktopProtocol::Vnc;
        vnc_profile.endpoint =
            RemoteDesktopEndpoint::for_protocol("preview.local", RemoteDesktopProtocol::Vnc);
        let request = initial_connect_request(
            &vnc_profile,
            &builtin_provider_manifest(RemoteDesktopProtocol::Vnc),
            true,
            RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
            None,
            RemoteDesktopMonitorLayout::default(),
        );

        let encoded = serde_json::to_string(&request).unwrap();
        assert!(encoded.contains("\"passwordAvailable\":true"));
        assert!(!encoded.contains("wire-secret"));
        assert!(!encoded.contains("\"password\":"));
        assert!(!encoded.contains("\"username\":"));
        assert!(!encoded.contains("\"domain\":"));
    }

    #[test]
    fn preview_provider_is_the_only_fake_backend() {
        let registry = builtin_preview_provider_registry().unwrap();
        let provider = registry
            .get_for_protocol(RemoteDesktopProtocol::Rdp)
            .unwrap();

        assert!(remote_desktop_provider_uses_fake_backend(provider));
    }

    #[test]
    fn effective_options_never_exceed_provider_capabilities() {
        let mut provider = builtin_provider_manifest(RemoteDesktopProtocol::Vnc);
        // A custom provider can still cap user options below built-in support.
        provider.capabilities.clipboard_data = false;
        provider.capabilities.clipboard_files = false;
        let requested = RemoteDesktopSessionOptions {
            clipboard: crate::RemoteDesktopClipboardOptions {
                text: true,
                images: true,
                files: true,
            },
            audio: crate::RemoteDesktopAudioOptions {
                playback: true,
                capture: true,
            },
            display: crate::RemoteDesktopDisplayOptions {
                use_all_monitors: true,
            },
            vnc: crate::RemoteDesktopVncOptions::default(),
        };

        let effective = effective_session_options(requested, &provider);

        assert!(effective.clipboard.text);
        assert!(!effective.clipboard.images);
        assert!(!effective.clipboard.files);
        assert!(effective.audio.playback);
        assert!(!effective.audio.capture);
        assert!(effective.display.use_all_monitors);
    }

    #[test]
    fn helper_protocol_failure_suppresses_later_reader_transport_error() {
        let worker_id = RemoteDesktopWorkerId::new(RemoteDesktopSessionId::new(), 41);
        let event = RemoteDesktopHelperEvent::ConnectionFailure {
            message: "ClearCodec decode failed".to_string(),
            category: Some(crate::RemoteDesktopErrorCategory::Protocol),
        };
        let input = format!(
            "{}\nnot-valid-json\n",
            serde_json::to_string(&event).unwrap()
        );
        let (delivery_tx, delivery_rx) = mpsc::channel();

        let outcome = read_remote_desktop_events(
            worker_id,
            input.as_bytes(),
            delivery_tx,
            RemoteDesktopFrameDeliverySlot::new(),
        );

        assert!(outcome.terminal_delivery_sent);
        let deliveries = delivery_rx.try_iter().collect::<Vec<_>>();
        assert_eq!(deliveries.len(), 1);
        assert!(matches!(
            &deliveries[0],
            RemoteDesktopWorkerDelivery::Event {
                event: RemoteDesktopHelperEvent::ConnectionFailure {
                    category: Some(crate::RemoteDesktopErrorCategory::Protocol),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn pending_transport_failure_is_delivered_without_helper_terminal_reason() {
        let worker_id = RemoteDesktopWorkerId::new(RemoteDesktopSessionId::new(), 42);
        let (delivery_tx, delivery_rx) = mpsc::channel();

        deliver_pending_transport_failure(
            &delivery_tx,
            &worker_id,
            Some("helper pipe closed".to_string()),
            RemoteDesktopReaderOutcome::default(),
        );

        assert!(matches!(
            delivery_rx.recv().unwrap(),
            RemoteDesktopWorkerDelivery::TransportFailed { message, .. }
                if message == "helper pipe closed"
        ));
    }

    #[test]
    fn helper_terminal_reason_suppresses_pending_transport_failure() {
        let worker_id = RemoteDesktopWorkerId::new(RemoteDesktopSessionId::new(), 43);
        let (delivery_tx, delivery_rx) = mpsc::channel();

        deliver_pending_transport_failure(
            &delivery_tx,
            &worker_id,
            Some("os error 232".to_string()),
            RemoteDesktopReaderOutcome {
                terminal_delivery_sent: true,
            },
        );

        assert!(delivery_rx.try_recv().is_err());
    }

    #[test]
    #[cfg(unix)]
    fn close_kills_and_reaps_a_helper_that_ignores_the_protocol() {
        let mut provider = builtin_provider_manifest(RemoteDesktopProtocol::Rdp);
        provider.entry.command = "sh".to_string();
        provider.entry.args = vec![
            "-c".to_string(),
            "IFS= read -r _line; exec sleep 3".to_string(),
        ];
        provider.entry.working_dir = None;
        let config = RemoteDesktopWorkerConfig {
            worker_id: RemoteDesktopWorkerId::new(RemoteDesktopSessionId::new(), 7),
            profile: profile(),
            provider,
            password_available: false,
            initial_size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
            scale_factor: None,
            monitor_layout: RemoteDesktopMonitorLayout::default(),
        };
        let (request_tx, request_rx) = mpsc::channel();
        request_tx.send(RemoteDesktopHelperRequest::Close).unwrap();
        let (delivery_tx, delivery_rx) = mpsc::channel();
        let started_at = std::time::Instant::now();

        run_remote_desktop_worker_with_close_grace(
            config,
            RemoteDesktopFrameDeliverySlot::new(),
            request_rx,
            delivery_tx,
            Duration::from_millis(20),
        );

        assert!(started_at.elapsed() < Duration::from_secs(1));
        assert!(delivery_rx.try_iter().any(|delivery| {
            matches!(
                delivery,
                RemoteDesktopWorkerDelivery::Event {
                    event: RemoteDesktopHelperEvent::Terminated { .. },
                    ..
                }
            )
        }));
    }
}
