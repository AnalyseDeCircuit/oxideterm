// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{self, BufRead, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU16, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender},
    },
    thread,
    time::Duration,
};

use des::{
    Des,
    cipher::{Block, BlockCipherEncrypt, KeyInit},
};
use flate2::{Decompress, FlushDecompress, Status};
use oxideterm_remote_desktop::{
    NegotiatedCapabilities, NegotiatedCapabilityStatus, RemoteDesktopClipboardData,
    RemoteDesktopCursorShape, RemoteDesktopEndpoint, RemoteDesktopErrorCategory,
    RemoteDesktopFakeBackend, RemoteDesktopFileConflictPolicy,
    RemoteDesktopFileTransferFailureKind, RemoteDesktopFrame, RemoteDesktopFrameFormat,
    RemoteDesktopFrameUpdate, RemoteDesktopFrameUpdateBatch, RemoteDesktopHelperEvent,
    RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState, RemoteDesktopLockKeys,
    RemoteDesktopMonitorLayout, RemoteDesktopMouseButton, RemoteDesktopMouseButtonState,
    RemoteDesktopProtocol, RemoteDesktopRect, RemoteDesktopRemoteFileEntry,
    RemoteDesktopRemoteFileKind, RemoteDesktopSecret, RemoteDesktopServerCertificate,
    RemoteDesktopServerIdentityKind, RemoteDesktopSessionOptions, RemoteDesktopSessionStatus,
    RemoteDesktopSize, RemoteDesktopWheelDelta, read_request_line, run_fake_backend_stdio,
    write_event_line,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

const VNC_PROTOCOL_VERSION_33: &[u8; 12] = b"RFB 003.003\n";
const VNC_PROTOCOL_VERSION_37: &[u8; 12] = b"RFB 003.007\n";
const VNC_PROTOCOL_VERSION_38: &[u8; 12] = b"RFB 003.008\n";
const VNC_SECURITY_NONE: u8 = 1;
const VNC_SECURITY_VNC_AUTH: u8 = 2;
const VNC_HEXTILE_TILE_SIZE: u16 = 16;
const VNC_HEXTILE_RAW: u8 = 1;
const VNC_HEXTILE_BACKGROUND_SPECIFIED: u8 = 2;
const VNC_HEXTILE_FOREGROUND_SPECIFIED: u8 = 4;
const VNC_HEXTILE_ANY_SUBRECTS: u8 = 8;
const VNC_HEXTILE_SUBRECTS_COLORED: u8 = 16;
const VNC_ZRLE_TILE_SIZE: u16 = 64;
const VNC_TRLE_RAW: u8 = 0;
const VNC_TRLE_SOLID: u8 = 1;
const VNC_TRLE_PLAIN_RLE: u8 = 128;
const VNC_BUTTON_LEFT: u16 = 1;
const VNC_BUTTON_MIDDLE: u16 = 2;
const VNC_BUTTON_RIGHT: u16 = 4;
const VNC_BUTTON_BACK: u16 = 1 << 7;
const VNC_BUTTON_FORWARD: u16 = 1 << 8;
const VNC_WHEEL_UP: u8 = 8;
const VNC_WHEEL_DOWN: u8 = 16;
const VNC_WHEEL_LEFT: u8 = 32;
const VNC_WHEEL_RIGHT: u8 = 64;
const VNC_SCROLL_STEP: f32 = 120.0;
const REMOTE_DESKTOP_DIAGNOSTICS_ENV: &str = "OXIDETERM_REMOTE_DESKTOP_DIAGNOSTICS";
const MAX_VNC_FRAME_BYTES: usize = 128 * 1024 * 1024;
const VNC_MAX_FRAME_UPDATE_REGIONS: usize = 16;

type SharedEventWriter = Arc<Mutex<io::Stdout>>;
type SharedVncWriter = SyncSender<VncIoCommand>;

struct VncSessionConfig {
    endpoint: RemoteDesktopEndpoint,
    transport_endpoint: Option<RemoteDesktopEndpoint>,
    read_only: bool,
    session_options: RemoteDesktopSessionOptions,
    initial_size: RemoteDesktopSize,
    monitor_layout: RemoteDesktopMonitorLayout,
    password_available: bool,
    username_available: bool,
}

struct VncAuthentication {
    // The helper owns this transient copy only until the TLS authentication
    // exchange completes, then Zeroizing clears it before framebuffer I/O.
    username: Option<Zeroizing<String>>,
    password: Option<RemoteDesktopSecret>,
}

impl VncAuthentication {
    fn username(&self) -> Option<&str> {
        self.username.as_deref().map(String::as_str)
    }

    fn password(&self) -> Option<&RemoteDesktopSecret> {
        self.password.as_ref()
    }
}

enum VncIoCommand {
    Write(Vec<u8>),
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VncRequestAction {
    Continue,
    Close,
    Reconnect,
}

#[derive(Clone, Copy, Debug, Default)]
struct VncDiagnostics {
    // Diagnostics stay opt-in because helper stderr can be collected by parent
    // processes and must never include user payloads by default.
    enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VncProtocolVersion {
    Rfb003003,
    Rfb003007,
    Rfb003008,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VncSecuritySelection {
    None,
    VncAuth,
    TlsNone,
    TlsVnc,
    TlsPlain,
    TlsSasl,
    X509None,
    X509Vnc,
    X509Plain,
    X509Sasl,
}

#[derive(Default)]
struct VncReaderDiagnosticsCounters {
    // These counters intentionally track protocol volume, not frame bytes or
    // clipboard contents.
    server_messages: u64,
    helper_frames: u64,
    helper_frame_updates: u64,
    helper_side_events: u64,
    dirty_rects: u64,
    dirty_pixels: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct VncServerEventSummary {
    // The summary is safe for logs: it records counts and areas only.
    dirty_rects: u64,
    dirty_pixels: u64,
    side_events: u64,
}

impl VncDiagnostics {
    fn from_env() -> Self {
        Self {
            enabled: std::env::var_os(REMOTE_DESKTOP_DIAGNOSTICS_ENV).is_some(),
        }
    }

    fn log(&self, message: impl AsRef<str>) {
        if self.enabled {
            eprintln!("[oxideterm:vnc-helper] {}", message.as_ref());
        }
    }
}

impl VncProtocolVersion {
    fn banner(self) -> &'static [u8; 12] {
        match self {
            Self::Rfb003003 => VNC_PROTOCOL_VERSION_33,
            Self::Rfb003007 => VNC_PROTOCOL_VERSION_37,
            Self::Rfb003008 => VNC_PROTOCOL_VERSION_38,
        }
    }
}

impl VncSecuritySelection {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::VncAuth => "vnc-auth",
            Self::TlsNone => "tls-none",
            Self::TlsVnc => "tls-vnc",
            Self::TlsPlain => "tls-plain",
            Self::TlsSasl => "tls-sasl",
            Self::X509None => "x509-none",
            Self::X509Vnc => "x509-vnc",
            Self::X509Plain => "x509-plain",
            Self::X509Sasl => "x509-sasl",
        }
    }

    fn requires_password(self) -> bool {
        matches!(
            self,
            Self::VncAuth
                | Self::TlsVnc
                | Self::TlsPlain
                | Self::TlsSasl
                | Self::X509Vnc
                | Self::X509Plain
                | Self::X509Sasl
        )
    }

    fn requires_username(self) -> bool {
        matches!(
            self,
            Self::TlsPlain | Self::TlsSasl | Self::X509Plain | Self::X509Sasl
        )
    }

    fn uses_vnc_password_challenge(self) -> bool {
        matches!(self, Self::VncAuth | Self::TlsVnc | Self::X509Vnc)
    }

    fn uses_sasl(self) -> bool {
        matches!(self, Self::TlsSasl | Self::X509Sasl)
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("oxideterm-vnc-helper: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !args.iter().any(|arg| arg == "--stdio") {
        return Err("pass --stdio to run the helper protocol boundary".to_string());
    }

    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    if args.iter().any(|arg| arg == "--fake") {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        let mut backend = RemoteDesktopFakeBackend::new(RemoteDesktopProtocol::Vnc);

        // The fake backend stays available for preview and deterministic tests.
        run_fake_backend_stdio(&mut backend, &mut reader, &mut writer)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    run_real_vnc_stdio(&mut reader)
}

fn run_real_vnc_stdio(reader: &mut impl BufRead) -> Result<(), String> {
    let writer = Arc::new(Mutex::new(io::stdout()));
    let diagnostics = VncDiagnostics::from_env();
    let Some(first_request) = read_request_line(reader).map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let RemoteDesktopHelperRequest::StartConnect {
        protocol,
        endpoint,
        transport_endpoint,
        password_available,
        username_available,
        size,
        scale_factor: _scale_factor,
        read_only,
        session_options,
        monitor_layout,
    } = first_request
    else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "VNC helper expected an initial preflight request.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Configuration),
            },
        )?;
        return Ok(());
    };

    if protocol != RemoteDesktopProtocol::Vnc {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "VNC helper received a non-VNC connect request.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Configuration),
            },
        )?;
        return Ok(());
    }

    let session_config = VncSessionConfig {
        endpoint,
        transport_endpoint,
        read_only,
        session_options,
        initial_size: size,
        monitor_layout,
        password_available,
        username_available,
    };
    let control = Arc::new(VncSessionControl::default());
    let (request_tx, request_rx) = std::sync::mpsc::sync_channel(128);
    let worker_writer = writer.clone();
    let worker_control = control.clone();
    let worker = thread::Builder::new()
        .name("oxideterm-vnc-session".to_string())
        .spawn(move || {
            run_vnc_session(
                session_config,
                request_rx,
                worker_writer,
                diagnostics,
                worker_control,
            )
        })
        .map_err(|error| format!("VNC session worker start failed: {error}"))?;

    while let Some(request) = read_request_line(reader).map_err(|error| error.to_string())? {
        match request {
            RemoteDesktopHelperRequest::Close => {
                control.request_close();
                break;
            }
            RemoteDesktopHelperRequest::Reconnect => {
                control.request_reconnect();
            }
            request => request_tx.try_send(request).map_err(|error| {
                format!("VNC session request queue rejected a request: {error}")
            })?,
        }
    }
    control.request_close();
    drop(request_tx);
    worker
        .join()
        .map_err(|_| "VNC session worker panicked.".to_string())?
}

#[derive(Default)]
struct VncSessionControl {
    close_requested: AtomicBool,
    reconnect_requested: AtomicBool,
    current_attempt: Mutex<Option<Arc<AtomicBool>>>,
}

impl VncSessionControl {
    fn install_attempt(&self, canceled: Arc<AtomicBool>) {
        if let Ok(mut current) = self.current_attempt.lock() {
            *current = Some(canceled);
        }
    }

    fn clear_attempt(&self) {
        if let Ok(mut current) = self.current_attempt.lock() {
            *current = None;
        }
    }

    fn cancel_current_attempt(&self) {
        if let Ok(current) = self.current_attempt.lock()
            && let Some(canceled) = current.as_ref()
        {
            canceled.store(true, Ordering::Release);
        }
    }

    fn request_close(&self) {
        self.close_requested.store(true, Ordering::Release);
        self.cancel_current_attempt();
    }

    fn request_reconnect(&self) {
        self.reconnect_requested.store(true, Ordering::Release);
        self.cancel_current_attempt();
    }

    fn take_reconnect(&self) -> bool {
        self.reconnect_requested.swap(false, Ordering::AcqRel)
    }

    fn reconnect_requested(&self) -> bool {
        self.reconnect_requested.load(Ordering::Acquire)
    }
}

fn run_vnc_session(
    mut config: VncSessionConfig,
    request_rx: Receiver<RemoteDesktopHelperRequest>,
    writer: SharedEventWriter,
    diagnostics: VncDiagnostics,
    control: Arc<VncSessionControl>,
) -> Result<(), String> {
    let active_generation = Arc::new(AtomicU64::new(0));
    let mut generation = 0_u64;
    let mut reconnect_count = 0_u64;
    let mut deferred_requests = VecDeque::new();

    loop {
        if control.close_requested.load(Ordering::Acquire) {
            break;
        }
        control.take_reconnect();
        generation = generation.saturating_add(1);
        active_generation.store(generation, Ordering::Release);
        let canceled = Arc::new(AtomicBool::new(false));
        control.install_attempt(canceled.clone());
        send_event(
            &writer,
            RemoteDesktopHelperEvent::Status {
                status: if reconnect_count == 0 {
                    RemoteDesktopSessionStatus::Connecting
                } else {
                    RemoteDesktopSessionStatus::Reconnecting
                },
                message: Some(if reconnect_count == 0 {
                    "Opening VNC session.".to_string()
                } else {
                    "Reopening VNC session.".to_string()
                }),
            },
        )?;
        diagnostics.log(format!(
            "connect attempt reconnects={reconnect_count} read_only={}",
            config.read_only
        ));

        let mut preflight = match connect_vnc_security_preflight(
            &config.endpoint,
            config.transport_endpoint.as_ref(),
            config.session_options.vnc.security_policy,
            config.username_available,
            config.password_available,
            canceled.clone(),
        ) {
            Ok(preflight) => preflight,
            Err(_error) if control.close_requested.load(Ordering::Acquire) => break,
            Err(error) if control.take_reconnect() => {
                diagnostics.log(format!(
                    "connect canceled for reconnect: {:?}",
                    error.kind()
                ));
                reconnect_count = reconnect_count.saturating_add(1);
                continue;
            }
            Err(error) => {
                send_vnc_failure(&writer, &error)?;
                break;
            }
        };
        if control.close_requested.load(Ordering::Acquire) {
            break;
        }
        if control.take_reconnect() {
            reconnect_count = reconnect_count.saturating_add(1);
            continue;
        }
        let certificate = vnc_identity_challenge(&config, &preflight);
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ServerCertificate {
                certificate: certificate.clone(),
            },
        )?;
        let authentication = match wait_for_vnc_authentication(
            &request_rx,
            &control,
            &certificate,
            preflight.requires_username(),
            preflight.requires_password(),
            &mut deferred_requests,
        ) {
            Ok(authentication) => authentication,
            Err(_error) if control.close_requested.load(Ordering::Acquire) => break,
            Err(error) if control.take_reconnect() => {
                diagnostics.log(format!("authentication canceled for reconnect: {error}"));
                reconnect_count = reconnect_count.saturating_add(1);
                continue;
            }
            Err(error) => {
                send_vnc_failure(&writer, &error)?;
                break;
            }
        };
        if control.close_requested.load(Ordering::Acquire) {
            break;
        }
        if control.take_reconnect() {
            reconnect_count = reconnect_count.saturating_add(1);
            continue;
        }

        let mut connection = match VncConnection::complete(
            &config,
            &mut preflight,
            authentication.username(),
            authentication.password(),
            writer.clone(),
            diagnostics,
            canceled,
            generation,
            active_generation.clone(),
        ) {
            Ok(connection) => connection,
            Err(error) => {
                send_vnc_failure(&writer, &error)?;
                break;
            }
        };
        // The VNC handshake has consumed the credentials by this point. Release
        // both transient owners before the long-lived framebuffer loop starts.
        drop(authentication);
        control.clear_attempt();
        send_event(
            &writer,
            RemoteDesktopHelperEvent::Connected {
                size: RemoteDesktopSize {
                    width: u32::from(connection.width),
                    height: u32::from(connection.height),
                },
            },
        )?;
        send_event(&writer, connection.capabilities_event()?)?;
        connection.start_reader()?;
        connection.request_framebuffer_update(false)?;
        let mut input_state = VncInputState::default();
        let action = run_vnc_connected_requests(
            &mut config,
            &request_rx,
            &control,
            &writer,
            &mut connection,
            &mut input_state,
            &mut deferred_requests,
        )?;
        connection.shutdown_and_join()?;
        // Retire the generation only after its owner drained ordered shutdown
        // messages and joined; the next connection cannot overlap it.
        active_generation.fetch_add(1, Ordering::AcqRel);
        match action {
            VncRequestAction::Reconnect => {
                reconnect_count = reconnect_count.saturating_add(1);
            }
            VncRequestAction::Close => break,
            VncRequestAction::Continue => {}
        }
    }
    control.clear_attempt();
    send_event(
        &writer,
        RemoteDesktopHelperEvent::Disconnected {
            reason: Some("VNC session closed.".to_string()),
        },
    )
}

fn run_vnc_connected_requests(
    config: &mut VncSessionConfig,
    request_rx: &Receiver<RemoteDesktopHelperRequest>,
    control: &VncSessionControl,
    event_writer: &SharedEventWriter,
    connection: &mut VncConnection,
    input_state: &mut VncInputState,
    deferred_requests: &mut VecDeque<RemoteDesktopHelperRequest>,
) -> Result<VncRequestAction, String> {
    loop {
        if control.close_requested.load(Ordering::Acquire) {
            return Ok(VncRequestAction::Close);
        }
        if control.take_reconnect() {
            return Ok(VncRequestAction::Reconnect);
        }
        let request = if let Some(request) = deferred_requests.pop_front() {
            request
        } else {
            match request_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(request) => request,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(VncRequestAction::Close);
                }
            }
        };
        match handle_real_vnc_request(event_writer, connection, input_state, request, config)? {
            VncRequestAction::Continue => {}
            action => return Ok(action),
        }
    }
}

fn wait_for_vnc_authentication(
    request_rx: &Receiver<RemoteDesktopHelperRequest>,
    control: &VncSessionControl,
    certificate: &RemoteDesktopServerCertificate,
    username_required: bool,
    password_required: bool,
    deferred_requests: &mut VecDeque<RemoteDesktopHelperRequest>,
) -> VncResult<VncAuthentication> {
    loop {
        if control.close_requested.load(Ordering::Acquire) || control.reconnect_requested() {
            return Err(VncError::cancelled());
        }
        let request = match request_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(request) => request,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(VncError::cancelled());
            }
        };
        match request {
            RemoteDesktopHelperRequest::Authenticate {
                challenge_id,
                sha256_fingerprint,
                mut username,
                password,
                mut domain,
            } => {
                if challenge_id != certificate.challenge_id
                    || sha256_fingerprint != certificate.sha256_fingerprint
                {
                    if let Some(username) = username.as_mut() {
                        username.zeroize();
                    }
                    if let Some(domain) = domain.as_mut() {
                        domain.zeroize();
                    }
                    // A response can race with Reconnect and belong to the
                    // retired challenge. Discard it without affecting the new
                    // generation or releasing its secret.
                    drop(password);
                    continue;
                }
                if let Some(domain) = domain.as_mut() {
                    domain.zeroize();
                }
                let username = username
                    .map(Zeroizing::new)
                    .filter(|username| !username.is_empty());
                if username_required && username.is_none() {
                    return Err(VncError::authentication(
                        "VNC server requires username authentication.",
                    ));
                }
                if password_required && password.as_ref().is_none_or(RemoteDesktopSecret::is_empty)
                {
                    return Err(VncError::authentication(
                        "VNC server requires password authentication.",
                    ));
                }
                return Ok(VncAuthentication {
                    username: username_required.then_some(username).flatten(),
                    password: password_required.then_some(password).flatten(),
                });
            }
            RemoteDesktopHelperRequest::Close => return Err(VncError::cancelled()),
            RemoteDesktopHelperRequest::Reconnect => return Err(VncError::cancelled()),
            RemoteDesktopHelperRequest::StartConnect { .. }
            | RemoteDesktopHelperRequest::Connect { .. } => {
                return Err(VncError::configuration(
                    "VNC helper received a second connection request.",
                ));
            }
            request => deferred_requests.push_back(request),
        }
    }
}

fn vnc_identity_challenge(
    config: &VncSessionConfig,
    preflight: &VncSecurityPreflight,
) -> RemoteDesktopServerCertificate {
    let challenge_id = Uuid::new_v4().to_string();
    let identity_kind = if preflight.peer_identity_verified() {
        RemoteDesktopServerIdentityKind::X509Certificate
    } else if preflight.encrypted() {
        RemoteDesktopServerIdentityKind::AnonymousTls
    } else {
        RemoteDesktopServerIdentityKind::InsecureLegacy
    };
    let sha256_fingerprint = preflight
        .peer_certificate_fingerprint
        .clone()
        .unwrap_or_else(|| {
            let material = format!(
                "{}|{}|{}",
                config.endpoint.format_authority(),
                preflight.security.as_str(),
                challenge_id
            );
            Sha256::digest(material.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join(":")
        });
    RemoteDesktopServerCertificate {
        challenge_id,
        protocol: RemoteDesktopProtocol::Vnc,
        endpoint: config.endpoint.clone(),
        identity_kind,
        security_method: preflight.security.as_str().to_string(),
        sha256_fingerprint,
        subject: None,
        issuer: None,
        valid_from: None,
        valid_to: None,
    }
}

fn send_vnc_failure(writer: &SharedEventWriter, error: &VncError) -> Result<(), String> {
    send_event(
        writer,
        RemoteDesktopHelperEvent::ConnectionFailure {
            message: error.to_string(),
            category: Some(error.category()),
        },
    )
}

fn handle_real_vnc_request(
    event_writer: &SharedEventWriter,
    connection: &mut VncConnection,
    input_state: &mut VncInputState,
    request: RemoteDesktopHelperRequest,
    config: &mut VncSessionConfig,
) -> Result<VncRequestAction, String> {
    match request {
        RemoteDesktopHelperRequest::Close => return Ok(VncRequestAction::Close),
        RemoteDesktopHelperRequest::Reconnect => {
            return Ok(VncRequestAction::Reconnect);
        }
        RemoteDesktopHelperRequest::Resize { size, .. } => {
            config.initial_size = size;
            // A viewport resize must not collapse an active multi-monitor
            // topology into a synthetic single screen.
            if !config.session_options.display.use_all_monitors {
                match VncDesktopLayout::single(size) {
                    Ok(layout) => connection.request_desktop_layout(layout)?,
                    Err(message) => send_vnc_resize_status(event_writer, message)?,
                }
            }
        }
        RemoteDesktopHelperRequest::UpdateDisplayLayout { layout } => {
            // Only opt-in sessions may replace the server screen topology.
            if config.session_options.display.use_all_monitors {
                match initial_vnc_desktop_layout(config.initial_size, true, &layout) {
                    Ok(desktop_layout) => {
                        config.monitor_layout = layout;
                        connection.request_desktop_layout(desktop_layout)?;
                    }
                    Err(message) => send_vnc_resize_status(event_writer, message)?,
                }
            }
        }
        RemoteDesktopHelperRequest::StartConnect { .. }
        | RemoteDesktopHelperRequest::Connect { .. }
        | RemoteDesktopHelperRequest::Authenticate { .. } => {
            return Err("VNC helper received a second connect request.".to_string());
        }
        RemoteDesktopHelperRequest::MouseMove { x, y } if !config.read_only => {
            input_state.pointer.x = clamp_u32_to_u16(x);
            input_state.pointer.y = clamp_u32_to_u16(y);
            connection.send_pointer(
                input_state.pointer.x,
                input_state.pointer.y,
                input_state.pointer.buttons,
            )?;
            send_event(
                event_writer,
                RemoteDesktopHelperEvent::Cursor {
                    x: u32::from(input_state.pointer.x),
                    y: u32::from(input_state.pointer.y),
                    width: 0,
                    height: 0,
                },
            )?;
        }
        RemoteDesktopHelperRequest::MouseButton { button, state } if !config.read_only => {
            let mask = vnc_button_mask(button);
            match state {
                RemoteDesktopMouseButtonState::Pressed => input_state.pointer.buttons |= mask,
                RemoteDesktopMouseButtonState::Released => input_state.pointer.buttons &= !mask,
            }
            connection.send_pointer(
                input_state.pointer.x,
                input_state.pointer.y,
                input_state.pointer.buttons,
            )?;
        }
        RemoteDesktopHelperRequest::Wheel { delta } if !config.read_only => {
            for mask in vnc_scroll_masks(delta) {
                connection.send_pointer(
                    input_state.pointer.x,
                    input_state.pointer.y,
                    input_state.pointer.buttons | u16::from(mask),
                )?;
                connection.send_pointer(
                    input_state.pointer.x,
                    input_state.pointer.y,
                    input_state.pointer.buttons,
                )?;
            }
        }
        RemoteDesktopHelperRequest::Key { key, state } if !config.read_only => {
            for event in input_state.keyboard.operations(&key, state) {
                connection.send_key_event(event)?;
            }
        }
        RemoteDesktopHelperRequest::Text { text } if !config.read_only => {
            for event in vnc_text_key_events(&text) {
                connection.send_key_event(event)?;
            }
        }
        RemoteDesktopHelperRequest::ClipboardText { text } if !config.read_only => {
            connection.send_client_cut_text(&text)?;
        }
        RemoteDesktopHelperRequest::ClipboardData { data } if !config.read_only => {
            if let Err(message) = connection.send_client_clipboard_data(&data) {
                // Clipboard conversion or negotiation failures are scoped to
                // the paste operation and must not tear down the desktop.
                send_event(
                    event_writer,
                    RemoteDesktopHelperEvent::Status {
                        status: RemoteDesktopSessionStatus::Connected,
                        message: Some(message),
                    },
                )?;
            }
        }
        RemoteDesktopHelperRequest::ClipboardFiles { transfer_id, paths } if !config.read_only => {
            if let Err(message) = connection.send_clipboard_files(&transfer_id, &paths) {
                send_event(
                    event_writer,
                    RemoteDesktopHelperEvent::ClipboardTransferFailed {
                        transfer_id,
                        message,
                    },
                )?;
            }
        }
        RemoteDesktopHelperRequest::CancelClipboardTransfer { transfer_id } => {
            connection.cancel_clipboard_transfer(transfer_id)?;
        }
        RemoteDesktopHelperRequest::VncListRemoteFiles { request_id, path } => {
            if connection
                .request_remote_files(request_id.clone(), path)
                .is_err()
            {
                send_event(
                    event_writer,
                    RemoteDesktopHelperEvent::VncRemoteFileListFailed { request_id },
                )?;
            }
        }
        RemoteDesktopHelperRequest::VncDownloadRemoteFiles {
            transfer_id,
            remote_paths,
            destination,
            conflict_policy,
        } => {
            if connection
                .download_remote_files(
                    transfer_id.clone(),
                    remote_paths,
                    destination,
                    conflict_policy,
                )
                .is_err()
            {
                send_event(
                    event_writer,
                    RemoteDesktopHelperEvent::VncFileTransferFailed {
                        transfer_id,
                        kind: RemoteDesktopFileTransferFailureKind::Local,
                    },
                )?;
            }
        }
        RemoteDesktopHelperRequest::CancelVncFileTransfer { transfer_id } => {
            connection.cancel_file_transfer(transfer_id)?;
        }
        RemoteDesktopHelperRequest::SynchronizeLockKeys { keys } if !config.read_only => {
            connection.synchronize_lock_keys(keys)?;
        }
        RemoteDesktopHelperRequest::RequestFrame => {
            connection.request_full_frame_recovery()?;
        }
        RemoteDesktopHelperRequest::ReleaseAllInputs if !config.read_only => {
            if input_state.pointer.buttons != 0 {
                input_state.pointer.buttons = 0;
                connection.send_pointer(input_state.pointer.x, input_state.pointer.y, 0)?;
            }
            for event in input_state.keyboard.release_all_events() {
                connection.send_key_event(event)?;
            }
        }
        _ => {}
    }

    Ok(VncRequestAction::Continue)
}

#[derive(Default)]
struct VncInputState {
    pointer: VncPointerState,
    keyboard: VncKeyboardInputMapper,
}

#[derive(Default)]
struct VncPointerState {
    x: u16,
    y: u16,
    buttons: u16,
}

struct VncConnection {
    transport: Option<Box<dyn VncTransport>>,
    writer: SharedVncWriter,
    io_rx: Option<Receiver<VncIoCommand>>,
    event_writer: SharedEventWriter,
    diagnostics: VncDiagnostics,
    canceled: Arc<AtomicBool>,
    generation: u64,
    active_generation: Arc<AtomicU64>,
    session_state: Arc<VncSessionSharedState>,
    audio: Arc<Mutex<QemuAudioSession>>,
    clipboard: Arc<Mutex<VncClipboardSession>>,
    vendor_files: Arc<Mutex<VncVendorFileSession>>,
    capabilities: SharedVncCapabilities,
    desktop_resize: SharedVncDesktopResize,
    h264: Option<VncH264State>,
    reader_handle: Option<thread::JoinHandle<()>>,
    reader_done: Option<Receiver<()>>,
    width: u16,
    height: u16,
}

struct VncSessionSharedState {
    width: AtomicU16,
    height: AtomicU16,
    force_next_base_frame: AtomicBool,
    qemu_extended_key_events: AtomicU8,
    extended_mouse_buttons: AtomicU8,
    remote_lock_keys: AtomicU8,
    pending_lock_keys: AtomicU8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RfbRect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

#[derive(Debug, PartialEq)]
enum VncServerEvent {
    SetResolution {
        width: u16,
        height: u16,
    },
    ExtendedDesktopSize(VncExtendedDesktopSize),
    RawImage(RfbRect, Vec<u8>),
    CopyRect {
        dst: RfbRect,
        src_x: u16,
        src_y: u16,
    },
    ClipboardText(String),
    ClipboardExtended(ExtendedClipboardMessage),
    CursorShape(RemoteDesktopCursorShape),
    CursorHidden,
    QemuAudioCapability,
    QemuAudio(QemuAudioServerMessage),
    QemuExtendedKeyEvents,
    ExtendedMouseButtons,
    LockKeys(RemoteDesktopLockKeys),
    ObservedCapability(VncObservedCapability),
    ServerFence(VncServerFence),
    EndOfContinuousUpdates,
    Batch(Vec<VncServerEvent>),
    Noop,
}

#[derive(Debug, Eq, PartialEq)]
enum VncFramebufferChange {
    Full,
    Updates(Vec<RemoteDesktopFrameUpdate>),
}

struct VncFramebuffer {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

impl VncFramebuffer {
    #[cfg(test)]
    fn new(width: u16, height: u16) -> Self {
        Self::try_new(width, height).expect("test framebuffer allocation should succeed")
    }

    fn try_new(width: u16, height: u16) -> Result<Self, String> {
        let width = width as u32;
        let height = height as u32;
        Ok(Self {
            width,
            height,
            bgra: try_opaque_bgra_buffer(width, height)?,
        })
    }

    #[cfg(test)]
    fn apply(&mut self, event: VncServerEvent) -> Option<VncFramebufferChange> {
        self.try_apply(event)
            .expect("test framebuffer update allocation should succeed")
    }

    fn try_apply(&mut self, event: VncServerEvent) -> Result<Option<VncFramebufferChange>, String> {
        match event {
            VncServerEvent::SetResolution { width, height } => {
                let next = try_opaque_bgra_buffer(u32::from(width), u32::from(height))?;
                self.width = width as u32;
                self.height = height as u32;
                self.bgra = next;
                Ok(Some(VncFramebufferChange::Full))
            }
            VncServerEvent::ExtendedDesktopSize(update) if update.applies_layout() => {
                let next = try_opaque_bgra_buffer(
                    u32::from(update.layout.width),
                    u32::from(update.layout.height),
                )?;
                self.width = u32::from(update.layout.width);
                self.height = u32::from(update.layout.height);
                self.bgra = next;
                Ok(Some(VncFramebufferChange::Full))
            }
            VncServerEvent::RawImage(rect, data) => Ok(self.draw_rect(rect, data)),
            VncServerEvent::CopyRect { dst, src_x, src_y } => Ok(self.copy_rect(dst, src_x, src_y)),
            VncServerEvent::Batch(events) => {
                let mut change = None;
                for event in events {
                    let incoming = self.try_apply(event)?;
                    change = self.merge_framebuffer_change(change, incoming)?;
                }
                Ok(change)
            }
            VncServerEvent::ClipboardText(_)
            | VncServerEvent::ClipboardExtended(_)
            | VncServerEvent::CursorShape(_)
            | VncServerEvent::CursorHidden
            | VncServerEvent::ExtendedDesktopSize(_)
            | VncServerEvent::QemuAudioCapability
            | VncServerEvent::QemuAudio(_)
            | VncServerEvent::QemuExtendedKeyEvents
            | VncServerEvent::ExtendedMouseButtons
            | VncServerEvent::LockKeys(_)
            | VncServerEvent::ObservedCapability(_)
            | VncServerEvent::ServerFence(_)
            | VncServerEvent::EndOfContinuousUpdates
            | VncServerEvent::Noop => Ok(None),
        }
    }

    fn frame(&self) -> RemoteDesktopFrame {
        RemoteDesktopFrame::new(
            RemoteDesktopSize {
                width: self.width,
                height: self.height,
            },
            RemoteDesktopFrameFormat::Bgra8,
            self.bgra.clone(),
        )
    }

    fn frame_update(&self, rect: RfbRect) -> Option<RemoteDesktopFrameUpdate> {
        let rect = self.clipped_rect(rect)?;
        let bytes = self.rect_bytes(rect)?;
        Some(self.frame_update_from_bytes(rect, bytes))
    }

    fn frame_update_from_bytes(&self, rect: RfbRect, bytes: Vec<u8>) -> RemoteDesktopFrameUpdate {
        RemoteDesktopFrameUpdate::new(
            RemoteDesktopSize {
                width: self.width,
                height: self.height,
            },
            RemoteDesktopRect::new(
                rect.x as u32,
                rect.y as u32,
                rect.width as u32,
                rect.height as u32,
            ),
            RemoteDesktopFrameFormat::Bgra8,
            bytes,
        )
    }

    fn draw_rect(&mut self, rect: RfbRect, mut data: Vec<u8>) -> Option<VncFramebufferChange> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let clipped = self.clipped_rect(rect)?;
        let needed = rect.width as usize * rect.height as usize * 4;
        if data.len() < needed {
            return None;
        }
        data.truncate(needed);
        set_bgra_alpha_opaque(&mut data);
        let mut clipped_bytes = (clipped != rect).then(|| {
            Vec::with_capacity(usize::from(clipped.width) * usize::from(clipped.height) * 4)
        });

        for y in 0..u32::from(clipped.height) {
            let src_y = u32::from(clipped.y - rect.y) + y;
            let src_start =
                ((src_y * u32::from(rect.width) + u32::from(clipped.x - rect.x)) * 4) as usize;
            let src_end = src_start + (u32::from(clipped.width) * 4) as usize;
            let dst_start =
                (((u32::from(clipped.y) + y) * self.width + u32::from(clipped.x)) * 4) as usize;
            let dst_end = dst_start + (u32::from(clipped.width) * 4) as usize;
            let dst_row = &mut self.bgra[dst_start..dst_end];
            dst_row.copy_from_slice(&data[src_start..src_end]);
            if let Some(clipped_bytes) = clipped_bytes.as_mut() {
                clipped_bytes.extend_from_slice(&data[src_start..src_end]);
            }
        }
        let update_bytes = clipped_bytes.unwrap_or(data);
        Some(VncFramebufferChange::Updates(vec![
            self.frame_update_from_bytes(clipped, update_bytes),
        ]))
    }

    fn copy_rect(&mut self, dst: RfbRect, src_x: u16, src_y: u16) -> Option<VncFramebufferChange> {
        if self.width == 0 || self.height == 0 || dst.width == 0 || dst.height == 0 {
            return None;
        }
        let copy_w = dst.width as u32;
        let copy_h = dst.height as u32;
        let src_x = src_x as u32;
        let src_y = src_y as u32;
        let dst_x = dst.x as u32;
        let dst_y = dst.y as u32;
        if src_x >= self.width
            || src_y >= self.height
            || dst_x >= self.width
            || dst_y >= self.height
        {
            return None;
        }
        let copy_w = copy_w.min(self.width - src_x).min(self.width - dst_x);
        let copy_h = copy_h.min(self.height - src_y).min(self.height - dst_y);
        if copy_w == 0 || copy_h == 0 {
            return None;
        }
        let mut scratch = vec![0; copy_w as usize * copy_h as usize * 4];
        for y in 0..copy_h {
            let src_start = (((src_y + y) * self.width + src_x) * 4) as usize;
            let src_end = src_start + (copy_w * 4) as usize;
            let tmp_start = (y * copy_w * 4) as usize;
            let tmp_end = tmp_start + (copy_w * 4) as usize;
            scratch[tmp_start..tmp_end].copy_from_slice(&self.bgra[src_start..src_end]);
        }
        for y in 0..copy_h {
            let tmp_start = (y * copy_w * 4) as usize;
            let tmp_end = tmp_start + (copy_w * 4) as usize;
            let dst_start = (((dst_y + y) * self.width + dst_x) * 4) as usize;
            let dst_end = dst_start + (copy_w * 4) as usize;
            self.bgra[dst_start..dst_end].copy_from_slice(&scratch[tmp_start..tmp_end]);
        }
        let rect = RfbRect {
            x: dst.x,
            y: dst.y,
            width: copy_w as u16,
            height: copy_h as u16,
        };
        Some(VncFramebufferChange::Updates(vec![
            self.frame_update_from_bytes(rect, scratch),
        ]))
    }

    fn clipped_rect(&self, rect: RfbRect) -> Option<RfbRect> {
        let rect_x = u32::from(rect.x);
        let rect_y = u32::from(rect.y);
        let rect_w = u32::from(rect.width);
        let rect_h = u32::from(rect.height);
        if rect_x >= self.width || rect_y >= self.height || rect_w == 0 || rect_h == 0 {
            return None;
        }
        Some(RfbRect {
            x: rect.x,
            y: rect.y,
            width: rect_w.min(self.width - rect_x) as u16,
            height: rect_h.min(self.height - rect_y) as u16,
        })
    }

    fn rect_bytes(&self, rect: RfbRect) -> Option<Vec<u8>> {
        let rect = self.clipped_rect(rect)?;
        let width = usize::from(rect.width);
        let height = usize::from(rect.height);
        let mut bytes = vec![0; width.checked_mul(height)?.checked_mul(4)?];
        for y in 0..height {
            let src_start =
                ((usize::from(rect.y) + y) * self.width as usize + usize::from(rect.x)) * 4;
            let src_end = src_start + width * 4;
            let dst_start = y * width * 4;
            let dst_end = dst_start + width * 4;
            bytes[dst_start..dst_end].copy_from_slice(&self.bgra[src_start..src_end]);
        }
        Some(bytes)
    }

    fn merge_framebuffer_change(
        &self,
        existing: Option<VncFramebufferChange>,
        incoming: Option<VncFramebufferChange>,
    ) -> Result<Option<VncFramebufferChange>, String> {
        match (existing, incoming) {
            (Some(VncFramebufferChange::Full), _) | (_, Some(VncFramebufferChange::Full)) => {
                Ok(Some(VncFramebufferChange::Full))
            }
            (
                Some(VncFramebufferChange::Updates(mut updates)),
                Some(VncFramebufferChange::Updates(incoming_updates)),
            ) => {
                for update in incoming_updates {
                    self.push_bounded_frame_update(&mut updates, update)?;
                }
                let updated_pixels = updates
                    .iter()
                    .map(|update| {
                        u64::from(update.rect.width).saturating_mul(u64::from(update.rect.height))
                    })
                    .fold(0_u64, u64::saturating_add);
                let framebuffer_pixels =
                    u64::from(self.width).saturating_mul(u64::from(self.height));
                if updated_pixels >= framebuffer_pixels {
                    Ok(Some(VncFramebufferChange::Full))
                } else {
                    Ok(Some(VncFramebufferChange::Updates(updates)))
                }
            }
            (Some(change), None) | (None, Some(change)) => Ok(Some(change)),
            (None, None) => Ok(None),
        }
    }

    fn push_bounded_frame_update(
        &self,
        updates: &mut Vec<RemoteDesktopFrameUpdate>,
        incoming: RemoteDesktopFrameUpdate,
    ) -> Result<(), String> {
        for existing in updates.iter_mut().rev() {
            if existing.merge(&incoming) {
                // The merge call already copied the newer pixels into this update.
                return Ok(());
            }
        }
        updates.push(incoming);
        while updates.len() > VNC_MAX_FRAME_UPDATE_REGIONS {
            let (first, second, union) = smallest_frame_update_union(updates)
                .ok_or_else(|| "VNC dirty regions could not be bounded.".to_string())?;
            updates.swap_remove(second);
            updates.swap_remove(first);
            let union = remote_rect_to_rfb(union)
                .ok_or_else(|| "VNC merged dirty region exceeds protocol bounds.".to_string())?;
            let merged = self
                .frame_update(union)
                .ok_or_else(|| "VNC merged dirty region is outside the framebuffer.".to_string())?;
            // The framebuffer contains every update applied so far, so the
            // merged snapshot safely supersedes all earlier overlapping regions.
            updates.push(merged);
        }
        Ok(())
    }
}

fn try_opaque_bgra_buffer(width: u32, height: u32) -> Result<Vec<u8>, String> {
    if width == 0
        || height == 0
        || width > RemoteDesktopSize::MAX_DIMENSION
        || height > RemoteDesktopSize::MAX_DIMENSION
    {
        return Err("VNC framebuffer dimensions exceed the helper limit.".to_string());
    }
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|len| *len <= MAX_VNC_FRAME_BYTES)
        .ok_or_else(|| "VNC framebuffer allocation exceeds the helper limit.".to_string())?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| "VNC framebuffer allocation failed.".to_string())?;
    bytes.resize(len, 0);
    set_bgra_alpha_opaque(&mut bytes);
    Ok(bytes)
}

fn set_bgra_alpha_opaque(bytes: &mut [u8]) {
    for pixel in bytes.chunks_exact_mut(4) {
        // VNC requests 32-bit/24-depth true color, so the fourth byte is
        // transport padding rather than framebuffer transparency.
        pixel[3] = 0xff;
    }
}

fn smallest_frame_update_union(
    updates: &[RemoteDesktopFrameUpdate],
) -> Option<(usize, usize, RemoteDesktopRect)> {
    let mut best = None;
    for first in 0..updates.len() {
        for second in (first + 1)..updates.len() {
            let union = updates[first].rect.union(updates[second].rect)?;
            let union_pixels = u64::from(union.width).saturating_mul(u64::from(union.height));
            let source_pixels = [updates[first].rect, updates[second].rect]
                .into_iter()
                .map(|rect| u64::from(rect.width).saturating_mul(u64::from(rect.height)))
                .fold(0_u64, u64::saturating_add);
            let inflation = union_pixels.saturating_sub(source_pixels);
            if best
                .as_ref()
                .is_none_or(|(_, _, _, best_inflation)| inflation < *best_inflation)
            {
                best = Some((first, second, union, inflation));
            }
        }
    }
    best.map(|(first, second, union, _)| (first, second, union))
}

fn remote_rect_to_rfb(rect: RemoteDesktopRect) -> Option<RfbRect> {
    Some(RfbRect {
        x: u16::try_from(rect.x).ok()?,
        y: u16::try_from(rect.y).ok()?,
        width: u16::try_from(rect.width).ok()?,
        height: u16::try_from(rect.height).ok()?,
    })
}

mod audio;
mod capabilities;
mod clipboard_ext;
mod connection;
mod decode;
mod desktop_size;
mod encodings;
mod error;
mod h264;
mod input;
mod input_ext;
mod performance;
mod protocol;
mod security;
mod transport;
mod vendor_files;

use audio::*;
use capabilities::*;
use clipboard_ext::*;
use connection::*;
use decode::*;
use desktop_size::*;
use encodings::*;
use error::*;
use h264::*;
use input::*;
use input_ext::*;
use performance::*;
use protocol::*;
use security::*;
use transport::*;
use vendor_files::*;

#[cfg(test)]
mod tests;
