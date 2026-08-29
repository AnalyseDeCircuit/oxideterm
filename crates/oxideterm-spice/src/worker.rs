// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    io::{BufReader, Write},
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
    let mut reader = BufReader::new(stdout);

    let hello = HelperHello::current(FULL_HELPER_CAPABILITIES.to_vec());
    write_request(
        &mut stdin,
        &HelperRequest::Hello {
            hello: hello.clone(),
        },
    )?;
    stdin.flush()?;
    let acknowledgement = match read_event(&mut reader)? {
        Some(HelperEvent::HelloAck { acknowledgement }) => acknowledgement,
        Some(_) => return Err(SpiceWorkerError::UnexpectedHandshakeEvent),
        None => return Err(SpiceWorkerError::HandshakeEof),
    };
    acknowledgement.validate_for(&hello)?;

    // Credentials are moved into the child only after its version and complete capability
    // contract have been authenticated by the application-owned handshake.
    write_request(
        &mut stdin,
        &HelperRequest::Connect {
            options: config.connect.into_helper_options(),
        },
    )?;
    stdin.flush()?;

    let reader_worker_id = config.worker_id.clone();
    let reader_delivery = delivery_tx.clone();
    let reader_frame_slot = config.frame_slot.clone();
    let audio_playback = config.audio_playback;
    let audio_capture = config.audio_capture;
    let (capture_request_tx, capture_request_rx) =
        crossbeam_channel::bounded(CAPTURE_REQUEST_QUEUE_CAPACITY);
    let reader_signals = SpiceReaderSignals::default();
    let reader_thread_signals = reader_signals.clone();
    let reader_thread = thread::Builder::new()
        .name(format!(
            "oxide-spice-reader-{}",
            reader_worker_id.request_id
        ))
        .spawn(move || {
            read_events(
                reader_worker_id,
                reader,
                reader_delivery,
                reader_frame_slot,
                capture_request_tx,
                SpiceReaderConfig {
                    audio_playback,
                    audio_capture,
                    signals: reader_thread_signals.clone(),
                },
            );
            reader_thread_signals
                .finished
                .store(true, Ordering::Release);
        })?;

    let runtime_result = loop {
        if reader_signals.finished.load(Ordering::Acquire) {
            break Ok(());
        }
        match child.try_wait() {
            Ok(Some(_)) => break Ok(()),
            Ok(None) => {}
            Err(error) => break Err(SpiceWorkerError::Io(error)),
        }
        let mut request = crossbeam_channel::select! {
            recv(request_rx) -> request => request.unwrap_or(HelperRequest::Close),
            recv(capture_request_rx) -> request => request.unwrap_or(HelperRequest::Close),
            default(HELPER_REQUEST_POLL_INTERVAL) => continue,
        };
        if matches!(
            request,
            HelperRequest::Hello { .. } | HelperRequest::Connect { .. }
        ) {
            break Err(SpiceWorkerError::InvalidRuntimeRequest);
        }
        let close = matches!(request, HelperRequest::Close);
        if let Err(error) = write_request(&mut stdin, &request) {
            zeroize_request_payload(&mut request);
            break Err(SpiceWorkerError::Ipc(error));
        }
        if let Err(error) = stdin.flush() {
            zeroize_request_payload(&mut request);
            break Err(SpiceWorkerError::Io(error));
        }
        zeroize_request_payload(&mut request);
        if close {
            break Ok(());
        }
    };
    drop(stdin);
    let exit_status = helper_process::wait_or_terminate(
        &mut child,
        HELPER_CLOSE_GRACE_PERIOD,
        HELPER_LIVENESS_CHECK_INTERVAL,
    );
    let exit_code = exit_status.as_ref().and_then(|status| status.code());
    let _ = reader_thread.join();
    if reader_signals
        .helper_failure_reported
        .load(Ordering::Acquire)
    {
        // The structured helper error is already queued for the session. A later EOF or broken
        // pipe is only a consequence of that failure and must not replace its actionable message.
        return Err(SpiceWorkerError::ReportedFailure);
    }
    if runtime_result.is_err()
        && let Some(exit_status) = exit_status
    {
        // A closed child is the primary failure; the pipe error only describes the next write.
        return Err(SpiceWorkerError::UnexpectedHelperExit(
            exit_status.to_string(),
        ));
    }
    if runtime_result.is_ok() {
        // Failure is delivered by the outer boundary before the owner tears down this worker.
        send_delivery(
            delivery_tx,
            SpiceWorkerDelivery::Terminated {
                worker_id: config.worker_id,
                exit_code,
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
        frame_composer.observe_topology(&event);
        let event = match frame_composer.map_event(event) {
            SpiceFrameMapping::Frame(frame_event) => {
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
                if !visible {
                    send_remote_event(
                        &delivery_tx,
                        &worker_id,
                        RemoteDesktopHelperEvent::CursorHidden,
                    );
                    continue;
                }
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
                send_remote_event(
                    &delivery_tx,
                    &worker_id,
                    RemoteDesktopHelperEvent::Cursor {
                        x: u32::try_from(x).unwrap_or(0),
                        y: u32::try_from(y).unwrap_or(0),
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
    #[error("OxideSpice helper exited unexpectedly ({0})")]
    UnexpectedHelperExit(String),
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
