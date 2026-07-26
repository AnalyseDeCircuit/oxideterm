// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{io::BufReader, sync::mpsc, thread};

use crate::{
    RemoteDesktopConnectionProfile, RemoteDesktopFakeBackend, RemoteDesktopFrameDeliverySlot,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopMonitorLayout,
    RemoteDesktopProviderManifest, RemoteDesktopSecret, RemoteDesktopSessionId,
    RemoteDesktopSessionOptions, RemoteDesktopSize, is_remote_desktop_frame_event, read_event_line,
};
use crate::{helper_process, request_writer};

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

            if let Err(error) =
                request_writer::write_remote_desktop_requests(&mut helper.stdin, request_rx)
            {
                send_delivery(
                    &delivery_tx,
                    RemoteDesktopWorkerDelivery::TransportFailed {
                        worker_id: config.worker_id.clone(),
                        message: error.to_string(),
                    },
                );
            }
            drop(helper.stdin);
            let exit_code = helper.child.wait().ok().and_then(|status| status.code());
            if let Some(reader_thread) = reader_thread {
                // Preserve every event emitted before process termination.
                let _ = reader_thread.join();
            }
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
) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_event_line(&mut reader) {
            Ok(Some(event)) => {
                deliver_worker_event(&worker_id, event, &delivery_tx, &frame_slot);
            }
            Ok(None) => break,
            Err(error) => {
                send_delivery(
                    &delivery_tx,
                    RemoteDesktopWorkerDelivery::TransportFailed {
                        worker_id,
                        message: error.to_string(),
                    },
                );
                break;
            }
        }
    }
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
}
