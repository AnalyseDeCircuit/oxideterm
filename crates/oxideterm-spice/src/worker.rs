// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use crossbeam_channel::Receiver;
use oxide_spice_helper_protocol::{
    FULL_HELPER_CAPABILITIES, HelperErrorCategory, HelperEvent, HelperHello, HelperRequest,
    HelperStatus, read_event, write_request,
};
use oxideterm_remote_desktop::{
    RemoteDesktopCursorShape, RemoteDesktopErrorCategory, RemoteDesktopFrameFormat,
    RemoteDesktopHelperEvent, RemoteDesktopSessionStatus, RemoteDesktopSize,
};
use zeroize::Zeroize;

use crate::{
    SpiceHelperCommand, SpiceWorkerConfig, SpiceWorkerDelivery,
    audio::SpiceAudioRuntime,
    frame::{SpiceFrameComposer, SpiceFrameMapping},
    helper_process,
};

const HELPER_CLOSE_GRACE_PERIOD: Duration = Duration::from_secs(2);
const HELPER_LIVENESS_CHECK_INTERVAL: Duration = Duration::from_millis(250);
const HELPER_REQUEST_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CAPTURE_REQUEST_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Default)]
struct SpiceReaderSignals {
    finished: Arc<AtomicBool>,
    helper_failure_reported: Arc<AtomicBool>,
}

#[derive(Clone)]
struct SpiceReaderConfig {
    audio_playback: bool,
    audio_capture: bool,
    signals: SpiceReaderSignals,
}

pub fn resolve_spice_helper_command() -> Result<SpiceHelperCommand, std::io::Error> {
    for executable in spice_helper_candidates() {
        if executable.is_file() {
            let working_directory = executable
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_else(|| executable.parent().unwrap_or(Path::new(".")).to_path_buf());
            return Ok(SpiceHelperCommand {
                executable,
                working_directory,
            });
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "the bundled OxideSpice helper is unavailable for this platform",
    ))
}

pub fn run_spice_worker(
    config: SpiceWorkerConfig,
    request_rx: Receiver<HelperRequest>,
    delivery_tx: mpsc::Sender<SpiceWorkerDelivery>,
) {
    let worker_id = config.worker_id.clone();
    match run_worker(config, request_rx, &delivery_tx) {
        Ok(()) | Err(SpiceWorkerError::ReportedFailure) => {}
        Err(error) => {
            send_delivery(
                &delivery_tx,
                SpiceWorkerDelivery::TransportFailed {
                    worker_id,
                    message: error.to_string(),
                },
            );
        }
    }
}

fn run_worker(
    config: SpiceWorkerConfig,
    request_rx: Receiver<HelperRequest>,
    delivery_tx: &mpsc::Sender<SpiceWorkerDelivery>,
) -> Result<(), SpiceWorkerError> {
    let mut child = helper_process::spawn_helper(&config.helper)?;
    let mut stdin = child.stdin.take().ok_or(SpiceWorkerError::MissingStdin)?;
    let stdout = child.stdout.take().ok_or(SpiceWorkerError::MissingStdout)?;
    let hello = HelperHello::current(FULL_HELPER_CAPABILITIES.to_vec());
    let (write_tx, write_rx) = crossbeam_channel::bounded::<HelperRequest>(256);
    let (write_result_tx, write_result_rx) = crossbeam_channel::bounded(1);
    let writer = thread::Builder::new()
        .name("oxide-spice-writer".into())
        .spawn(move || {
            let result = (|| -> Result<(), SpiceWorkerError> {
                while let Ok(mut request) = write_rx.recv() {
                    let close = matches!(request, HelperRequest::Close);
                    let result = write_request(&mut stdin, &request).map_err(SpiceWorkerError::Ipc);
                    zeroize_request_payload(&mut request);
                    result?;
                    if close {
                        break;
                    }
                }
                Ok(())
            })();
            let _ = write_result_tx.send(result);
        })?;
    let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
    let (capture_tx, capture_rx) = crossbeam_channel::bounded(CAPTURE_REQUEST_QUEUE_CAPACITY);
    let signals = SpiceReaderSignals::default();
    let reader_config = SpiceReaderConfig {
        audio_playback: config.audio_playback,
        audio_capture: config.audio_capture,
        signals: signals.clone(),
    };
    let worker_id = config.worker_id.clone();
    let reader_delivery = delivery_tx.clone();
    let frame_slot = config.frame_slot.clone();
    let reader_hello = hello.clone();
    let reader = match thread::Builder::new()
        .name("oxide-spice-reader".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let handshake = (|| -> Result<(), SpiceWorkerError> {
                match read_event(&mut reader)? {
                    Some(HelperEvent::HelloAck { acknowledgement }) => {
                        acknowledgement.validate_for(&reader_hello)?;
                        Ok(())
                    }
                    Some(_) => Err(SpiceWorkerError::UnexpectedHandshakeEvent),
                    None => Err(SpiceWorkerError::HandshakeEof),
                }
            })();
            let valid = handshake.is_ok();
            let _ = ready_tx.send(handshake);
            if valid {
                read_events(
                    worker_id,
                    reader,
                    reader_delivery,
                    frame_slot,
                    capture_tx,
                    reader_config.clone(),
                );
            }
            reader_config
                .signals
                .finished
                .store(true, Ordering::Release);
        }) {
        Ok(reader) => reader,
        Err(error) => {
            drop(write_tx);
            let _ = child.kill();
            let _ = child.wait();
            let _ = writer.join();
            return Err(error.into());
        }
    };
    let _ = write_tx.try_send(HelperRequest::Hello { hello });
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut connect = Some(config.connect);
    let no_handshake = crossbeam_channel::never();
    let no_capture = crossbeam_channel::never();
    let runtime_result = loop {
        // The process owner never performs pipe I/O: Close and deadlines remain actionable
        // even when the peer stops reading stdin or never completes HelloAck.
        if connect.is_some() && std::time::Instant::now() >= deadline {
            break Err(SpiceWorkerError::HandshakeTimeout);
        }
        if signals.finished.load(Ordering::Acquire) && connect.is_none() {
            break Ok(());
        }
        match child.try_wait() {
            Ok(Some(_)) => break Ok(()),
            Ok(None) => {}
            Err(error) => break Err(error.into()),
        }
        let handshake = if connect.is_some() {
            &ready_rx
        } else {
            &no_handshake
        };
        let capture = if connect.is_none() {
            &capture_rx
        } else {
            &no_capture
        };
        crossbeam_channel::select! {
            recv(handshake) -> ready => {
                match ready {
                    Ok(Ok(())) => {
                        let options = connect.take().unwrap().into_helper_options();
                        if write_tx.try_send(HelperRequest::Connect { options }).is_err() {
                            break Err(SpiceWorkerError::WriterUnavailable);
                        }
                    }
                    Ok(Err(error)) => break Err(error),
                    Err(_) => break Err(SpiceWorkerError::HandshakeEof),
                }
            }
            recv(request_rx) -> request => {
                let request = request.unwrap_or(HelperRequest::Close);
                if matches!(request, HelperRequest::Close) {
                    let _ = write_tx.try_send(request);
                    break Ok(());
                }
                if matches!(request, HelperRequest::Hello { .. } | HelperRequest::Connect { .. }) {
                    break Err(SpiceWorkerError::InvalidRuntimeRequest);
                }
                if connect.is_none() && write_tx.try_send(request).is_err() {
                    break Err(SpiceWorkerError::WriterUnavailable);
                }
            }
            recv(capture) -> request => {
                match request {
                    Ok(request) if connect.is_none() => {
                        if write_tx.try_send(request).is_err() { break Err(SpiceWorkerError::WriterUnavailable); }
                    }
                    Ok(_) => {}
                    Err(_) if signals.finished.load(Ordering::Acquire) => break Ok(()),
                    Err(_) => {}
                }
            }
            recv(write_result_rx) -> result => {
                break result.unwrap_or(Err(SpiceWorkerError::WriterUnavailable));
            }
            default(HELPER_REQUEST_POLL_INTERVAL) => {}
        }
    };
    drop(write_tx);
    let exit_status = helper_process::wait_or_terminate(
        &mut child,
        HELPER_CLOSE_GRACE_PERIOD,
        HELPER_LIVENESS_CHECK_INTERVAL,
    );
    let _ = reader.join();
    let _ = writer.join();
    if signals.helper_failure_reported.load(Ordering::Acquire) {
        return Err(SpiceWorkerError::ReportedFailure);
    }
    if runtime_result.is_ok() {
        send_delivery(
            delivery_tx,
            SpiceWorkerDelivery::Terminated {
                worker_id: config.worker_id,
                exit_code: exit_status.as_ref().and_then(|status| status.code()),
            },
        );
    }
    runtime_result
}

fn zeroize_request_payload(request: &mut HelperRequest) {
    match request {
        HelperRequest::ClipboardProvide { data, .. }
        | HelperRequest::FileTransferData { data, .. }
        | HelperRequest::PortWrite { data, .. } => data.zeroize(),
        HelperRequest::RecordData { pcm_s16le, .. } => pcm_s16le.zeroize(),
        HelperRequest::Scancodes { bytes } => bytes.zeroize(),
        _ => {}
    }
}

fn read_events(
    worker_id: oxideterm_remote_desktop::RemoteDesktopWorkerId,
    mut reader: impl std::io::BufRead,
    delivery_tx: mpsc::Sender<SpiceWorkerDelivery>,
    frame_slot: oxideterm_remote_desktop::RemoteDesktopFrameDeliverySlot,
    capture_request_tx: crossbeam_channel::Sender<HelperRequest>,
    config: SpiceReaderConfig,
) {
    let mut connected_size = None;
    let mut session_connected = false;
    let mut frame_composer = SpiceFrameComposer::default();
    let mut delivered_layout = crate::SpiceDisplayLayout::default();
    let mut audio = SpiceAudioRuntime::new(config.audio_playback, config.audio_capture);
    loop {
        let mut event = match read_event(&mut reader) {
            Ok(Some(event)) => event,
            Ok(None) => return,
            Err(error) => {
                send_delivery(
                    &delivery_tx,
                    SpiceWorkerDelivery::TransportFailed {
                        worker_id,
                        message: error.to_string(),
                    },
                );
                return;
            }
        };
        if audio.handle_event(&mut event, &capture_request_tx) {
            continue;
        }
        let mapping = if matches!(event, HelperEvent::Topology { .. }) {
            let mapping = frame_composer.observe_topology(&event);
            if matches!(mapping, SpiceFrameMapping::Frame(_)) {
                send_helper_event(&delivery_tx, &worker_id, event);
            }
            mapping
        } else {
            frame_composer.map_event(event)
        };
        let event = match mapping {
            SpiceFrameMapping::Frame(frame_event) => {
                if delivered_layout != *frame_composer.layout() {
                    send_remote_event(
                        &delivery_tx,
                        &worker_id,
                        RemoteDesktopHelperEvent::CursorHidden,
                    );
                    delivered_layout = frame_composer.layout().clone();
                    send_delivery(
                        &delivery_tx,
                        SpiceWorkerDelivery::DisplayLayout {
                            worker_id: worker_id.clone(),
                            layout: delivered_layout.clone(),
                        },
                    );
                }
                let size = match &frame_event {
                    RemoteDesktopHelperEvent::Frame { frame } => frame.size,
                    RemoteDesktopHelperEvent::FrameUpdate { update } => update.size,
                    _ => unreachable!("SPICE frame adapter only returns frame events"),
                };
                if connected_size != Some(size) {
                    connected_size = Some(size);
                    send_remote_event(
                        &delivery_tx,
                        &worker_id,
                        RemoteDesktopHelperEvent::Connected { size },
                    );
                }
                let decision = frame_slot.push(frame_event);
                if decision.recovery_required {
                    send_delivery(
                        &delivery_tx,
                        SpiceWorkerDelivery::FrameRecoveryRequired {
                            worker_id: worker_id.clone(),
                        },
                    );
                }
                if decision.frame_ready {
                    send_delivery(
                        &delivery_tx,
                        SpiceWorkerDelivery::FrameReady {
                            worker_id: worker_id.clone(),
                        },
                    );
                }
                continue;
            }
            SpiceFrameMapping::Other(event) => event,
            SpiceFrameMapping::Ignored => continue,
            SpiceFrameMapping::Invalid => {
                send_delivery(
                    &delivery_tx,
                    SpiceWorkerDelivery::TransportFailed {
                        worker_id,
                        message: "OxideSpice helper returned an invalid frame".to_string(),
                    },
                );
                return;
            }
        };

        match event {
            event @ HelperEvent::Connected { .. } => {
                session_connected = true;
                send_helper_event(&delivery_tx, &worker_id, event);
            }
            HelperEvent::Status { status, message } => {
                if status == HelperStatus::Failed {
                    config
                        .signals
                        .helper_failure_reported
                        .store(true, Ordering::Release);
                }
                send_remote_event(
                    &delivery_tx,
                    &worker_id,
                    RemoteDesktopHelperEvent::Status {
                        status: remote_status(status),
                        message: message.clone(),
                    },
                );
                send_helper_event(
                    &delivery_tx,
                    &worker_id,
                    HelperEvent::Status { status, message },
                );
            }
            HelperEvent::Cursor {
                channel_id,
                x,
                y,
                visible,
                width,
                height,
                hot_spot_x,
                hot_spot_y,
                rgba,
                ..
            } => {
                if !rgba.is_empty() {
                    send_remote_event(
                        &delivery_tx,
                        &worker_id,
                        RemoteDesktopHelperEvent::CursorShape {
                            shape: RemoteDesktopCursorShape::new(
                                RemoteDesktopSize {
                                    width: u32::from(width),
                                    height: u32::from(height),
                                },
                                u32::from(hot_spot_x),
                                u32::from(hot_spot_y),
                                RemoteDesktopFrameFormat::Rgba8,
                                rgba,
                            ),
                        },
                    );
                }
                // A hidden update can introduce the shape used by the next visible
                // move, whose payload the helper is then allowed to omit.
                if !visible {
                    send_remote_event(
                        &delivery_tx,
                        &worker_id,
                        RemoteDesktopHelperEvent::CursorHidden,
                    );
                    continue;
                }
                let position = frame_composer.layout().cursor_position(
                    channel_id,
                    u32::try_from(x).unwrap_or(0),
                    u32::try_from(y).unwrap_or(0),
                );
                let Some((x, y)) = position else {
                    send_remote_event(
                        &delivery_tx,
                        &worker_id,
                        RemoteDesktopHelperEvent::CursorHidden,
                    );
                    continue;
                };
                send_remote_event(
                    &delivery_tx,
                    &worker_id,
                    RemoteDesktopHelperEvent::Cursor {
                        x,
                        y,
                        width: u32::from(width),
                        height: u32::from(height),
                    },
                );
            }
            HelperEvent::Error { category, message } => {
                let connection_failure = !session_connected
                    || matches!(
                        category,
                        HelperErrorCategory::Network
                            | HelperErrorCategory::Tls
                            | HelperErrorCategory::Authentication
                            | HelperErrorCategory::RemoteDisconnect
                    );
                if connection_failure {
                    config
                        .signals
                        .helper_failure_reported
                        .store(true, Ordering::Release);
                    send_remote_event(
                        &delivery_tx,
                        &worker_id,
                        RemoteDesktopHelperEvent::ConnectionFailure {
                            message: message.clone(),
                            category: Some(remote_error_category(category)),
                        },
                    );
                }
                send_helper_event(
                    &delivery_tx,
                    &worker_id,
                    HelperEvent::Error { category, message },
                );
            }
            event => send_helper_event(&delivery_tx, &worker_id, event),
        }
    }
}

fn remote_status(status: HelperStatus) -> RemoteDesktopSessionStatus {
    match status {
        HelperStatus::Connecting => RemoteDesktopSessionStatus::Connecting,
        HelperStatus::Connected => RemoteDesktopSessionStatus::Connected,
        HelperStatus::Closing | HelperStatus::Disconnected => {
            RemoteDesktopSessionStatus::Disconnected
        }
        HelperStatus::Failed => RemoteDesktopSessionStatus::Failed,
    }
}

fn remote_error_category(category: HelperErrorCategory) -> RemoteDesktopErrorCategory {
    match category {
        HelperErrorCategory::Configuration => RemoteDesktopErrorCategory::Configuration,
        HelperErrorCategory::Network | HelperErrorCategory::RemoteDisconnect => {
            RemoteDesktopErrorCategory::Network
        }
        HelperErrorCategory::Tls => RemoteDesktopErrorCategory::Protocol,
        HelperErrorCategory::Authentication => RemoteDesktopErrorCategory::Authentication,
        HelperErrorCategory::Protocol | HelperErrorCategory::Negotiation => {
            RemoteDesktopErrorCategory::Protocol
        }
        HelperErrorCategory::Unsupported | HelperErrorCategory::ResourceLimit => {
            RemoteDesktopErrorCategory::Dependency
        }
        HelperErrorCategory::Cancelled | HelperErrorCategory::Internal => {
            RemoteDesktopErrorCategory::Unknown
        }
    }
}

fn send_remote_event(
    delivery_tx: &mpsc::Sender<SpiceWorkerDelivery>,
    worker_id: &oxideterm_remote_desktop::RemoteDesktopWorkerId,
    event: RemoteDesktopHelperEvent,
) {
    send_delivery(
        delivery_tx,
        SpiceWorkerDelivery::RemoteDesktopEvent {
            worker_id: worker_id.clone(),
            event,
        },
    );
}

fn send_helper_event(
    delivery_tx: &mpsc::Sender<SpiceWorkerDelivery>,
    worker_id: &oxideterm_remote_desktop::RemoteDesktopWorkerId,
    event: HelperEvent,
) {
    send_delivery(
        delivery_tx,
        SpiceWorkerDelivery::Event {
            worker_id: worker_id.clone(),
            event,
        },
    );
}

fn send_delivery(delivery_tx: &mpsc::Sender<SpiceWorkerDelivery>, delivery: SpiceWorkerDelivery) {
    let _ = delivery_tx.send(delivery);
}

fn spice_helper_candidates() -> Vec<PathBuf> {
    let executable_name = if cfg!(windows) {
        "oxide-spice-helper.exe"
    } else {
        "oxide-spice-helper"
    };
    let mut candidates = Vec::new();
    if let Some(executable_directory) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        for resources in [
            executable_directory.join("resources"),
            executable_directory.join("..").join("Resources"),
        ] {
            candidates.push(
                resources
                    .join("helpers")
                    .join(target_triple())
                    .join("oxide-spice-helper")
                    .join("bin")
                    .join(executable_name),
            );
        }
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("oxideterm-gpui-app")
            .join("resources")
            .join("helpers")
            .join(target_triple())
            .join("oxide-spice-helper")
            .join("bin")
            .join(executable_name),
    );
    candidates
}

fn target_triple() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => std::env::consts::ARCH,
    }
}

#[derive(Debug, thiserror::Error)]
enum SpiceWorkerError {
    #[error("OxideSpice helper already reported the connection failure")]
    ReportedFailure,
    #[error("OxideSpice helper handshake timed out")]
    HandshakeTimeout,
    #[error("OxideSpice helper request writer is unavailable or not consuming requests")]
    WriterUnavailable,
    #[error("OxideSpice helper I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("OxideSpice helper IPC failed: {0}")]
    Ipc(#[from] oxide_spice_helper_protocol::HelperIpcError),
    #[error("OxideSpice helper handshake failed: {0}")]
    Handshake(#[from] oxide_spice_helper_protocol::HelperHelloAckError),
    #[error("OxideSpice helper closed before the handshake completed")]
    HandshakeEof,
    #[error("OxideSpice helper returned an unexpected handshake event")]
    UnexpectedHandshakeEvent,
    #[error("OxideSpice helper stdin is unavailable")]
    MissingStdin,
    #[error("OxideSpice helper stdout is unavailable")]
    MissingStdout,
    #[error("Hello and Connect are owned by the SPICE worker")]
    InvalidRuntimeRequest,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{SpiceConnectOptions, SpiceSecret};
    use oxideterm_remote_desktop::{
        RemoteDesktopFrameDeliverySlot, RemoteDesktopSessionId, RemoteDesktopWorkerId,
    };
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn reader_delivers_topology_only_repaint_and_hidden_cursor_shape() {
        use oxide_spice_helper_protocol::{
            HelperPixelFormat, HelperRect, HelperTopologyMonitor, write_event,
        };
        let mut wire = Vec::new();
        write_event(
            &mut wire,
            &HelperEvent::Frame {
                connection_generation: 1,
                graphics_epoch: 1,
                display_channel_id: 0,
                surface_id: 0,
                surface_width: 2,
                surface_height: 1,
                rect: HelperRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                full_refresh: true,
                format: HelperPixelFormat::Rgba8,
                pixels: vec![255, 0, 0, 255, 0, 0, 255, 255],
            },
        )
        .unwrap();
        write_event(
            &mut wire,
            &HelperEvent::Topology {
                connection_generation: 1,
                graphics_epoch: 1,
                display_channel_id: 0,
                maximum_allowed: 1,
                monitors: vec![HelperTopologyMonitor {
                    id: 0,
                    surface_id: 0,
                    x: 1,
                    y: 0,
                    width: 1,
                    height: 1,
                    flags: 0,
                }],
            },
        )
        .unwrap();
        for visible in [false, true] {
            write_event(
                &mut wire,
                &HelperEvent::Cursor {
                    connection_generation: 1,
                    cursor_epoch: 1,
                    channel_id: 0,
                    x: 1,
                    y: 0,
                    visible,
                    width: 1,
                    height: 1,
                    hot_spot_x: 0,
                    hot_spot_y: 0,
                    shape_id: Some(1),
                    rgba: if visible {
                        vec![]
                    } else {
                        vec![0, 255, 0, 255]
                    },
                },
            )
            .unwrap();
        }
        let slot = RemoteDesktopFrameDeliverySlot::new();
        let (delivery_tx, delivery_rx) = mpsc::channel();
        let (capture_tx, _capture_rx) = crossbeam_channel::bounded(1);
        read_events(
            RemoteDesktopWorkerId::new(RemoteDesktopSessionId::new(), 1),
            std::io::Cursor::new(wire),
            delivery_tx,
            slot.clone(),
            capture_tx,
            SpiceReaderConfig {
                audio_playback: false,
                audio_capture: false,
                signals: SpiceReaderSignals::default(),
            },
        );
        let RemoteDesktopHelperEvent::Frame { frame } = slot.take().unwrap() else {
            panic!("expected composed frame")
        };
        assert_eq!(frame.bytes, [0, 0, 255, 255]);
        let mut cursor_events = Vec::new();
        for delivery in delivery_rx {
            match delivery {
                SpiceWorkerDelivery::RemoteDesktopEvent {
                    event: RemoteDesktopHelperEvent::CursorShape { shape },
                    ..
                } => {
                    assert_eq!(shape.bytes, [0, 255, 0, 255]);
                    cursor_events.push("shape");
                }
                SpiceWorkerDelivery::RemoteDesktopEvent {
                    event: RemoteDesktopHelperEvent::CursorHidden,
                    ..
                } => cursor_events.push("hidden"),
                SpiceWorkerDelivery::RemoteDesktopEvent {
                    event: RemoteDesktopHelperEvent::Cursor { x, y, .. },
                    ..
                } => {
                    assert_eq!((x, y), (0, 0));
                    cursor_events.push("visible");
                }
                SpiceWorkerDelivery::TransportFailed { message, .. } => {
                    panic!("unexpected failure: {message}")
                }
                _ => {}
            }
        }
        assert_eq!(
            cursor_events,
            ["hidden", "hidden", "shape", "hidden", "visible"]
        );
    }

    #[test]
    fn close_reaps_a_helper_that_never_acknowledges_hello() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("helper");
        let pid_file = directory.path().join("pid");
        std::fs::write(
            &executable,
            "#!/bin/sh\necho $$ > pid\nwhile IFS= read -r line; do :; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = SpiceWorkerConfig {
            worker_id: RemoteDesktopWorkerId::new(RemoteDesktopSessionId::new(), 1),
            helper: SpiceHelperCommand {
                executable,
                working_directory: directory.path().into(),
            },
            connect: SpiceConnectOptions::plain_tcp(
                "127.0.0.1",
                1,
                SpiceSecret::new("test-ticket"),
            ),
            frame_slot: RemoteDesktopFrameDeliverySlot::new(),
            audio_playback: false,
            audio_capture: false,
        };
        let (request_tx, request_rx) = crossbeam_channel::bounded(8);
        let (delivery_tx, delivery_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            run_spice_worker(config, request_rx, delivery_tx);
            done_tx.send(()).unwrap();
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !pid_file.exists() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        request_tx.send(HelperRequest::Close).unwrap();
        let closed = done_rx.recv_timeout(Duration::from_secs(4)).is_ok();
        if !closed {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-TERM", pid.trim()])
                .status();
        }
        worker.join().unwrap();
        assert!(
            closed,
            "Close must interrupt an incomplete helper handshake"
        );
        assert!(matches!(
            delivery_rx.recv().unwrap(),
            SpiceWorkerDelivery::Terminated {
                exit_code: Some(0),
                ..
            }
        ));
    }
}
