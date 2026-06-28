// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fmt,
    io::{self, BufRead, BufReader},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use ironrdp::{
    cliprdr::{
        CliprdrClient,
        backend::{ClipboardMessage, CliprdrBackend},
        pdu::{
            ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags,
            FileContentsRequest, FileContentsResponse, FormatDataRequest, FormatDataResponse,
            LockDataId,
        },
    },
    connector::connection_activation::ConnectionActivationState,
    connector::{self, ConnectorErrorKind, Credentials},
    connector::{ConnectionResult, DesktopSize},
    displaycontrol::client::DisplayControlClient,
    dvc::DrdynvcClient,
    graphics::image_processing::PixelFormat,
    input::{
        Database as RdpInputDatabase, MouseButton as RdpMouseButton, MousePosition,
        Operation as RdpInputOperation, Scancode, WheelRotations,
    },
    pdu::{
        gcc::KeyboardType,
        geometry::{InclusiveRectangle, Rectangle as _},
        input::fast_path::FastPathInputEvent,
        rdp::{
            capability_sets::{MajorPlatformType, client_codecs_capabilities},
            client_info::{CompressionType, PerformanceFlags, TimezoneInfo},
        },
    },
    session::{
        self, ActiveStage, ActiveStageOutput, GracefulDisconnectReason, SessionResult, fast_path,
        image::DecodedImage,
    },
};
use ironrdp_core::{IntoOwned as _, WriteBuf, impl_as_any};
use ironrdp_displaycontrol::pdu::MonitorLayoutEntry;
use ironrdp_tokio::{FramedWrite, single_sequence_step_read, split_tokio_framed};
use oxideterm_remote_desktop::{
    RemoteDesktopEndpoint, RemoteDesktopFakeBackend, RemoteDesktopFrame, RemoteDesktopFrameFormat,
    RemoteDesktopFrameUpdate, RemoteDesktopHelperEvent, RemoteDesktopHelperRequest,
    RemoteDesktopKey, RemoteDesktopKeyState, RemoteDesktopMouseButton,
    RemoteDesktopMouseButtonState, RemoteDesktopProtocol, RemoteDesktopRect, RemoteDesktopSecret,
    RemoteDesktopSessionStatus, RemoteDesktopSize, RemoteDesktopWheelDelta, read_request_line,
    run_fake_backend_stdio, write_event_line,
};
use smallvec::SmallVec;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use zeroize::Zeroize;

const RDP_CLIENT_NAME: &str = "OxideTerm";
const RDP_CLIENT_LOOP_POLL_INTERVAL: Duration = Duration::from_millis(8);
const RDP_WHEEL_UNIT: f32 = 120.0;
const RDP_FRAME_COALESCE_WINDOW: Duration = Duration::from_millis(16);
const RDP_CLIPBOARD_TIMEOUT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const RDP_CLIPBOARD_TEMPORARY_DIRECTORY: &str = ".cliprdr";
const LEGACY_RDP_SECURITY_MESSAGE: &str =
    "该服务器只支持旧版 RDP 安全模式，需要启用 legacy RDP 支持";
const LEGACY_RDP_ENGINE_UNAVAILABLE_MESSAGE: &str =
    "该服务器只支持旧版 RDP 安全模式，但当前 helper 构建未包含 FreeRDP legacy 引擎";

#[derive(Clone)]
struct SharedEventWriter {
    stdout: Arc<Mutex<io::Stdout>>,
    queue: Arc<(Mutex<EventWriterQueue>, Condvar)>,
}

#[derive(Default)]
struct EventWriterQueue {
    frame: Option<RemoteDesktopHelperEvent>,
}

impl SharedEventWriter {
    fn stdio() -> Self {
        let writer = Self {
            stdout: Arc::new(Mutex::new(io::stdout())),
            queue: Arc::new((Mutex::new(EventWriterQueue::default()), Condvar::new())),
        };
        writer.start_stdout_thread();
        writer
    }

    fn send(&self, event: RemoteDesktopHelperEvent) -> Result<(), String> {
        let (queue, wake) = &*self.queue;
        let mut queue = queue
            .lock()
            .map_err(|_| "RDP event queue lock is poisoned.".to_string())?;
        if is_frame_event(&event) {
            if let Some(frame) = queue.frame.as_mut() {
                merge_frame_event(frame, event);
            } else {
                queue.frame = Some(event);
            }
            wake.notify_one();
            return Ok(());
        }
        let pending_frame = queue.frame.take();
        drop(queue);

        // Control events are written synchronously so short-lived failures are
        // not lost when the helper exits immediately after reporting them.
        let mut stdout = self
            .stdout
            .lock()
            .map_err(|_| "RDP stdout writer lock is poisoned.".to_string())?;
        if let Some(frame) = pending_frame {
            write_event_line(&mut *stdout, &frame)
                .map_err(|error| format!("RDP event write failed: {error}"))?;
        }
        write_event_line(&mut *stdout, &event)
            .map_err(|error| format!("RDP event write failed: {error}"))?;
        Ok(())
    }

    fn start_stdout_thread(&self) {
        let queue = self.queue.clone();
        let stdout = self.stdout.clone();
        thread::Builder::new()
            .name("oxideterm-rdp-event-writer".to_string())
            .spawn(move || {
                while let Some(event) = next_frame_for_stdout(&queue) {
                    let Ok(mut stdout) = stdout.lock() else {
                        break;
                    };
                    if write_event_line(&mut *stdout, &event).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to start RDP event writer");
    }
}

fn next_frame_for_stdout(
    queue: &Arc<(Mutex<EventWriterQueue>, Condvar)>,
) -> Option<RemoteDesktopHelperEvent> {
    let (queue_lock, wake) = &**queue;
    let mut queue = queue_lock.lock().ok()?;
    loop {
        if queue.frame.is_none() {
            queue = wake.wait(queue).ok()?;
            continue;
        }

        // Keep only one pending frame and give fast bursts one refresh tick to
        // merge into a smaller stdout write workload.
        let deadline = Instant::now() + RDP_FRAME_COALESCE_WINDOW;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            let (next_queue, timeout) = wake.wait_timeout(queue, remaining).ok()?;
            queue = next_queue;
            if timeout.timed_out() {
                break;
            }
        }
        if let Some(frame) = queue.frame.take() {
            return Some(frame);
        }
    }
}

fn is_frame_event(event: &RemoteDesktopHelperEvent) -> bool {
    matches!(
        event,
        RemoteDesktopHelperEvent::Frame { .. } | RemoteDesktopHelperEvent::FrameUpdate { .. }
    )
}

fn merge_frame_event(existing: &mut RemoteDesktopHelperEvent, incoming: RemoteDesktopHelperEvent) {
    match existing {
        RemoteDesktopHelperEvent::Frame { frame } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if !frame.apply_update(&update) {
                    *existing = RemoteDesktopHelperEvent::FrameUpdate { update };
                }
            }
            incoming => {
                *existing = incoming;
            }
        },
        RemoteDesktopHelperEvent::FrameUpdate { update } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate {
                update: incoming_update,
            } => {
                if !update.merge(&incoming_update) {
                    *existing = RemoteDesktopHelperEvent::FrameUpdate {
                        update: incoming_update,
                    };
                }
            }
            incoming => {
                *existing = incoming;
            }
        },
        slot => {
            *slot = incoming;
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("oxideterm-rdp-helper: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !args.iter().any(|arg| arg == "--stdio") {
        return Err("pass --stdio to run the helper protocol boundary".to_string());
    }

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    if args.iter().any(|arg| arg == "--fake") {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        let mut backend = RemoteDesktopFakeBackend::new(RemoteDesktopProtocol::Rdp);

        // The fake backend stays available for previews and deterministic tests.
        run_fake_backend_stdio(&mut backend, &mut reader, &mut writer)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    run_real_rdp_stdio(&mut reader)
}

fn run_real_rdp_stdio(reader: &mut impl BufRead) -> Result<(), String> {
    let writer = SharedEventWriter::stdio();
    let Some(first_request) = read_request_line(reader).map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let RemoteDesktopHelperRequest::Connect {
        protocol,
        endpoint,
        username,
        password,
        domain,
        size,
        read_only,
    } = first_request
    else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP helper expected an initial connect request.".to_string(),
            },
        )?;
        return Ok(());
    };

    if protocol != RemoteDesktopProtocol::Rdp {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP helper received a non-RDP connect request.".to_string(),
            },
        )?;
        return Ok(());
    }

    let Some(username) = username.filter(|username| !username.trim().is_empty()) else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP username is required.".to_string(),
            },
        )?;
        return Ok(());
    };
    let Some(password) = password else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP password is required.".to_string(),
            },
        )?;
        return Ok(());
    };

    let (request_tx, request_rx) = mpsc::channel();
    let handle = start_rdp_worker(
        RdpWorkerConfig {
            endpoint,
            username,
            password,
            domain,
            size,
            read_only,
        },
        writer.clone(),
        request_rx,
    );

    while let Some(request) = read_request_line(reader).map_err(|error| error.to_string())? {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        if request_tx.send(request).is_err() {
            break;
        }
        if should_close {
            break;
        }
    }

    let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
    let _ = handle.join();
    Ok(())
}

struct RdpWorkerConfig {
    endpoint: RemoteDesktopEndpoint,
    username: String,
    password: RemoteDesktopSecret,
    domain: Option<String>,
    size: RemoteDesktopSize,
    read_only: bool,
}

fn start_rdp_worker(
    config: RdpWorkerConfig,
    writer: SharedEventWriter,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("oxideterm-rdp-session".to_string())
        .spawn(move || {
            if let Err(error) = run_rdp_worker(config, writer.clone(), request_rx) {
                let _ = send_event(
                    &writer,
                    RemoteDesktopHelperEvent::ConnectionFailure { message: error },
                );
            }
        })
        .expect("failed to start RDP helper worker")
}

fn run_rdp_worker(
    config: RdpWorkerConfig,
    writer: SharedEventWriter,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
) -> Result<(), String> {
    let mut reconnecting = false;
    loop {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::Status {
                status: if reconnecting {
                    RemoteDesktopSessionStatus::Reconnecting
                } else {
                    RemoteDesktopSessionStatus::Connecting
                },
                message: Some(if reconnecting {
                    "Reopening RDP session.".to_string()
                } else {
                    "Opening RDP session.".to_string()
                }),
            },
        )?;

        let client = start_client_rdp_session(&config)?;
        let exit = run_client_rdp_loop(
            &writer,
            &request_rx,
            &client.input_tx,
            client.output_rx,
            config.read_only,
        )?;
        let _ = client.input_tx.send(RdpInputEvent::Close);
        let _ = client.join_handle.join();

        match exit {
            ClientRdpSessionExit::Closed => {
                return send_event(
                    &writer,
                    RemoteDesktopHelperEvent::Disconnected {
                        reason: Some("RDP session closed.".to_string()),
                    },
                );
            }
            ClientRdpSessionExit::ReconnectRequested => {
                reconnecting = true;
            }
            ClientRdpSessionExit::LegacySecurityRequired => {
                return run_legacy_rdp_worker(config, writer, request_rx);
            }
        }
    }
}

enum ClientRdpSessionExit {
    Closed,
    ReconnectRequested,
    LegacySecurityRequired,
}

struct ClientRdpSession {
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_rx: mpsc::Receiver<ClientRdpOutput>,
    join_handle: thread::JoinHandle<()>,
}

#[derive(Debug)]
enum ClientRdpOutput {
    Event(RemoteDesktopHelperEvent),
    ConnectionFailure(connector::ConnectorError),
    Terminated(String),
    OutputEnded,
}

fn start_client_rdp_session(config: &RdpWorkerConfig) -> Result<ClientRdpSession, String> {
    let client_config = build_client_rdp_config(config)?;
    let (input_tx, input_rx) = tokio_mpsc::unbounded_channel();
    let client_input_tx = input_tx.clone();
    let (client_output_tx, client_output_rx) = mpsc::channel::<ClientRdpOutput>();

    let join_handle = thread::Builder::new()
        .name("oxideterm-rdp-client".to_string())
        .spawn(move || {
            run_client_rdp_thread(client_config, input_rx, client_input_tx, client_output_tx)
        })
        .map_err(|error| format!("RDP client thread startup failed: {error}"))?;

    Ok(ClientRdpSession {
        input_tx,
        output_rx: client_output_rx,
        join_handle,
    })
}

fn run_client_rdp_thread(
    mut config: ClientRdpConfig,
    mut input_rx: tokio_mpsc::UnboundedReceiver<RdpInputEvent>,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    client_output_tx: mpsc::Sender<ClientRdpOutput>,
) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        let _ = client_output_tx.send(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP async runtime startup failed.".to_string(),
            },
        ));
        return;
    };

    runtime.block_on(async move {
        loop {
            let (connection_result, framed) =
                match connect_native_rdp(&config, input_tx.clone(), client_output_tx.clone()).await
                {
                    Ok(result) => result,
                    Err(error) => {
                        let _ = client_output_tx.send(ClientRdpOutput::ConnectionFailure(error));
                        break;
                    }
                };

            let connected_size = remote_size_from_desktop(connection_result.desktop_size);
            if client_output_tx
                .send(ClientRdpOutput::Event(
                    RemoteDesktopHelperEvent::Connected {
                        size: connected_size,
                    },
                ))
                .is_err()
            {
                break;
            }

            match run_native_rdp_active_session(
                framed,
                connection_result,
                &mut input_rx,
                &client_output_tx,
            )
            .await
            {
                Ok(ClientRdpControlFlow::TerminatedGracefully(reason)) => {
                    let _ = client_output_tx.send(ClientRdpOutput::Terminated(
                        format_graceful_disconnect(reason),
                    ));
                    break;
                }
                Ok(ClientRdpControlFlow::ReconnectWithNewSize { width, height }) => {
                    config.connector.desktop_size = DesktopSize { width, height };
                    let _ = client_output_tx.send(ClientRdpOutput::Event(
                        RemoteDesktopHelperEvent::Status {
                            status: RemoteDesktopSessionStatus::Reconnecting,
                            message: Some(
                                "Reopening RDP session with the new display size.".to_string(),
                            ),
                        },
                    ));
                }
                Err(error) => {
                    let _ = client_output_tx.send(ClientRdpOutput::Terminated(format!(
                        "RDP session ended: {error}"
                    )));
                    break;
                }
            }
        }
        let _ = client_output_tx.send(ClientRdpOutput::OutputEnded);
    });
}

trait AsyncReadWrite: AsyncRead + AsyncWrite {}

impl<T> AsyncReadWrite for T where T: AsyncRead + AsyncWrite {}

type UpgradedRdpFramed = ironrdp_tokio::TokioFramed<Box<dyn AsyncReadWrite + Unpin + Send + Sync>>;

#[derive(Clone, Debug)]
struct ClientRdpConfig {
    destination: ClientRdpDestination,
    connector: connector::Config,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClientRdpDestination {
    host: String,
    port: u16,
}

impl ClientRdpDestination {
    fn from_parts(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    fn host(&self) -> &str {
        &self.host
    }

    fn port(&self) -> u16 {
        self.port
    }
}

struct ClientClipboardBackend {
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: mpsc::Sender<ClientRdpOutput>,
    local_text: Option<String>,
    remote_text_format: Option<ClipboardFormatId>,
}

impl fmt::Debug for ClientClipboardBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientClipboardBackend")
            .field("has_local_text", &self.local_text.is_some())
            .field("remote_text_format", &self.remote_text_format)
            .finish()
    }
}

impl_as_any!(ClientClipboardBackend);

impl ClientClipboardBackend {
    fn new(
        input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
        output_tx: mpsc::Sender<ClientRdpOutput>,
    ) -> Self {
        Self {
            input_tx,
            output_tx,
            local_text: None,
            remote_text_format: None,
        }
    }

    fn set_local_text(&mut self, text: String) {
        self.local_text = Some(text);
    }

    fn send_clipboard_message(&self, message: ClipboardMessage) {
        let _ = self.input_tx.send(RdpInputEvent::Clipboard(message));
    }

    fn send_local_format_list(&self) {
        let formats = self
            .local_text
            .as_ref()
            .map(|_| text_clipboard_formats())
            .unwrap_or_default();
        self.send_clipboard_message(ClipboardMessage::SendInitiateCopy(formats));
    }
}

impl CliprdrBackend for ClientClipboardBackend {
    fn temporary_directory(&self) -> &str {
        RDP_CLIPBOARD_TEMPORARY_DIRECTORY
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_ready(&mut self) {}

    fn on_request_format_list(&mut self) {
        // The CLIPRDR initialization sequence requires the client to advertise
        // its current clipboard formats, even when the list is empty.
        self.send_local_format_list();
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        _capabilities: ClipboardGeneralCapabilityFlags,
    ) {
    }

    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        let Some(format) = preferred_text_clipboard_format(available_formats) else {
            return;
        };
        self.remote_text_format = Some(format);
        self.send_clipboard_message(ClipboardMessage::SendInitiatePaste(format));
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let response = match (request.format, self.local_text.as_deref()) {
            (ClipboardFormatId::CF_UNICODETEXT, Some(text)) => {
                FormatDataResponse::new_unicode_string(text).into_owned()
            }
            (ClipboardFormatId::CF_TEXT, Some(text)) => {
                FormatDataResponse::new_string(text).into_owned()
            }
            _ => FormatDataResponse::new_error().into_owned(),
        };
        self.send_clipboard_message(ClipboardMessage::SendFormatData(response));
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }

        let text = match self.remote_text_format {
            Some(ClipboardFormatId::CF_UNICODETEXT) => response.to_unicode_string().ok(),
            Some(ClipboardFormatId::CF_TEXT) => response.to_string().ok(),
            _ => response
                .to_unicode_string()
                .or_else(|_| response.to_string())
                .ok(),
        };
        if let Some(text) = text {
            let _ = self.output_tx.send(ClientRdpOutput::Event(
                RemoteDesktopHelperEvent::ClipboardText { text },
            ));
        }
    }

    fn on_file_contents_request(&mut self, _request: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _response: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _data_id: LockDataId) {}

    fn on_unlock(&mut self, _data_id: LockDataId) {}
}

#[derive(Debug)]
enum RdpInputEvent {
    Resize {
        width: u16,
        height: u16,
        scale_factor: u32,
        physical_size: Option<(u32, u32)>,
    },
    FastPath(SmallVec<[FastPathInputEvent; 2]>),
    Clipboard(ClipboardMessage),
    SetClipboardText(String),
    Close,
}

enum ClientRdpControlFlow {
    TerminatedGracefully(GracefulDisconnectReason),
    ReconnectWithNewSize { width: u16, height: u16 },
}

async fn connect_native_rdp(
    config: &ClientRdpConfig,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: mpsc::Sender<ClientRdpOutput>,
) -> connector::ConnectorResult<(ConnectionResult, UpgradedRdpFramed)> {
    let socket = TcpStream::connect((config.destination.host(), config.destination.port()))
        .await
        .map_err(|error| connector::custom_err!("TCP connect", error))?;
    socket
        .set_nodelay(true)
        .map_err(|error| connector::custom_err!("set TCP_NODELAY", error))?;
    let client_addr = socket
        .local_addr()
        .map_err(|error| connector::custom_err!("get socket local address", error))?;
    let mut framed = ironrdp_tokio::TokioFramed::new(socket);
    let mut connector = connector::ClientConnector::new(config.connector.clone(), client_addr);
    attach_client_virtual_channels(&mut connector, input_tx, output_tx);
    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector).await?;
    let (initial_stream, leftover_bytes) = framed.into_inner();
    let (upgraded_stream, tls_cert) =
        ironrdp_tls::upgrade(initial_stream, config.destination.host())
            .await
            .map_err(|error| connector::custom_err!("TLS upgrade", error))?;
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);
    let erased_stream: Box<dyn AsyncReadWrite + Unpin + Send + Sync> = Box::new(upgraded_stream);
    let mut upgraded_framed =
        ironrdp_tokio::TokioFramed::new_with_leftover(erased_stream, leftover_bytes);
    let server_public_key = ironrdp_tls::extract_tls_server_public_key(&tls_cert)
        .ok_or_else(|| connector::general_err!("unable to extract TLS server public key"))?;
    let connection_result = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut upgraded_framed,
        &mut ironrdp_tokio::reqwest::ReqwestNetworkClient::new(),
        connector::ServerName::new(config.destination.host().to_string()),
        server_public_key.to_owned(),
        None,
    )
    .await?;

    Ok((connection_result, upgraded_framed))
}

fn attach_client_virtual_channels(
    connector: &mut connector::ClientConnector,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: mpsc::Sender<ClientRdpOutput>,
) {
    let display_control =
        DrdynvcClient::new().with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new())));
    connector.attach_static_channel(display_control);

    // CLIPRDR is attached as a normal static channel while the backend itself
    // bridges callbacks into OxideTerm's helper protocol.
    let clipboard = ClientClipboardBackend::new(input_tx, output_tx);
    connector.attach_static_channel(CliprdrClient::new(Box::new(clipboard)));
}

async fn run_native_rdp_active_session(
    framed: UpgradedRdpFramed,
    connection_result: ConnectionResult,
    input_rx: &mut tokio_mpsc::UnboundedReceiver<RdpInputEvent>,
    output_tx: &mpsc::Sender<ClientRdpOutput>,
) -> SessionResult<ClientRdpControlFlow> {
    let (mut reader, mut writer) = split_tokio_framed(framed);
    let mut image = DecodedImage::new(
        PixelFormat::RgbA32,
        connection_result.desktop_size.width,
        connection_result.desktop_size.height,
    );
    let mut active_stage = ActiveStage::new(connection_result);
    let mut clipboard_cleanup = tokio::time::interval(RDP_CLIPBOARD_TIMEOUT_POLL_INTERVAL);
    let mut sent_initial_frame = false;

    let disconnect_reason = 'session: loop {
        let outputs = tokio::select! {
            frame = reader.read_pdu() => {
                let (action, payload) = frame
                    .map_err(|error| session::custom_err!("read frame", error))?;
                active_stage.process(&mut image, action, &payload)?
            }
            input = input_rx.recv() => {
                let input = input.ok_or_else(|| session::general_err!("RDP input channel closed"))?;
                match input {
                    RdpInputEvent::Resize {
                        width,
                        height,
                        scale_factor,
                        physical_size,
                    } => {
                        let (width, height) = MonitorLayoutEntry::adjust_display_size(
                            u32::from(width),
                            u32::from(height),
                        );
                        if let Some(response_frame) =
                            active_stage.encode_resize(width, height, Some(scale_factor), physical_size)
                        {
                            vec![ActiveStageOutput::ResponseFrame(response_frame?)]
                        } else {
                            let width = u16::try_from(width)
                                .map_err(|error| session::custom_err!("resize width", error))?;
                            let height = u16::try_from(height)
                                .map_err(|error| session::custom_err!("resize height", error))?;
                            return Ok(ClientRdpControlFlow::ReconnectWithNewSize { width, height });
                        }
                    }
                    RdpInputEvent::FastPath(events) => {
                        active_stage.process_fastpath_input(&mut image, &events)?
                    }
                    RdpInputEvent::Clipboard(message) => {
                        process_clipboard_message(&mut active_stage, message)?
                    }
                    RdpInputEvent::SetClipboardText(text) => {
                        advertise_local_clipboard_text(&mut active_stage, text)?
                    }
                    RdpInputEvent::Close => active_stage.graceful_shutdown()?,
                }
            }
            _ = clipboard_cleanup.tick() => {
                drive_clipboard_timeouts(&mut active_stage)?
            }
        };

        for output in outputs {
            match output {
                ActiveStageOutput::ResponseFrame(frame) => writer
                    .write_all(&frame)
                    .await
                    .map_err(|error| session::custom_err!("write response", error))?,
                ActiveStageOutput::GraphicsUpdate(region) => {
                    let event = graphics_update_event(&image, region, &mut sent_initial_frame)?;
                    output_tx
                        .send(ClientRdpOutput::Event(event))
                        .map_err(|error| session::custom_err!("send graphics update", error))?;
                }
                ActiveStageOutput::PointerPosition { x, y } => {
                    output_tx
                        .send(ClientRdpOutput::Event(RemoteDesktopHelperEvent::Cursor {
                            x: u32::from(x),
                            y: u32::from(y),
                            width: 0,
                            height: 0,
                        }))
                        .map_err(|error| session::custom_err!("send pointer position", error))?;
                }
                ActiveStageOutput::DeactivateAll(connection_activation) => {
                    handle_deactivate_all(
                        &mut reader,
                        &mut writer,
                        &mut active_stage,
                        &mut image,
                        connection_activation,
                    )
                    .await?;
                    sent_initial_frame = false;
                }
                ActiveStageOutput::Terminate(reason) => break 'session reason,
                ActiveStageOutput::PointerDefault
                | ActiveStageOutput::PointerHidden
                | ActiveStageOutput::PointerBitmap(_)
                | ActiveStageOutput::MultitransportRequest(_)
                | ActiveStageOutput::AutoDetect(_) => {}
            }
        }
    };

    Ok(ClientRdpControlFlow::TerminatedGracefully(
        disconnect_reason,
    ))
}

fn process_clipboard_message(
    active_stage: &mut ActiveStage,
    message: ClipboardMessage,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(svc_messages) = ({
        let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
            return Ok(Vec::new());
        };
        match message {
            ClipboardMessage::SendInitiateCopy(formats) => Some(
                cliprdr
                    .initiate_copy(&formats)
                    .map_err(|error| session::custom_err!("CLIPRDR initiate copy", error))?,
            ),
            ClipboardMessage::SendFormatData(response) => Some(
                cliprdr
                    .submit_format_data(response)
                    .map_err(|error| session::custom_err!("CLIPRDR format data", error))?,
            ),
            ClipboardMessage::SendInitiatePaste(format) => Some(
                cliprdr
                    .initiate_paste(format)
                    .map_err(|error| session::custom_err!("CLIPRDR initiate paste", error))?,
            ),
            ClipboardMessage::SendFileContentsRequest(request) => Some(
                cliprdr
                    .request_file_contents(request)
                    .map_err(|error| session::custom_err!("CLIPRDR file request", error))?,
            ),
            ClipboardMessage::SendFileContentsResponse(response) => Some(
                cliprdr
                    .submit_file_contents(response)
                    .map_err(|error| session::custom_err!("CLIPRDR file response", error))?,
            ),
            ClipboardMessage::Error(_) => None,
        }
    }) else {
        return Ok(Vec::new());
    };

    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

fn advertise_local_clipboard_text(
    active_stage: &mut ActiveStage,
    text: String,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
        return Ok(Vec::new());
    };
    if let Some(backend) = cliprdr.downcast_backend_mut::<ClientClipboardBackend>() {
        backend.set_local_text(text);
    }

    // If CLIPRDR is not fully ready yet, the backend keeps the text and the
    // initialization callback will advertise it later.
    let Ok(svc_messages) = cliprdr.initiate_copy(&text_clipboard_formats()) else {
        return Ok(Vec::new());
    };
    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

fn drive_clipboard_timeouts(
    active_stage: &mut ActiveStage,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(svc_messages) = ({
        let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
            return Ok(Vec::new());
        };
        Some(
            cliprdr
                .drive_timeouts()
                .map_err(|error| session::custom_err!("CLIPRDR timeout cleanup", error))?,
        )
    }) else {
        return Ok(Vec::new());
    };
    let frame = active_stage.process_svc_processor_messages(svc_messages)?;
    response_frame_output(frame)
}

fn response_frame_output(frame: Vec<u8>) -> SessionResult<Vec<ActiveStageOutput>> {
    if frame.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(vec![ActiveStageOutput::ResponseFrame(frame)])
    }
}

fn run_client_rdp_loop(
    writer: &SharedEventWriter,
    request_rx: &mpsc::Receiver<RemoteDesktopHelperRequest>,
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_rx: mpsc::Receiver<ClientRdpOutput>,
    read_only: bool,
) -> Result<ClientRdpSessionExit, String> {
    let mut input_database = RdpInputDatabase::new();
    loop {
        while let Ok(output) = output_rx.try_recv() {
            match output {
                ClientRdpOutput::Event(event) => send_event(writer, event)?,
                ClientRdpOutput::ConnectionFailure(error) => {
                    if connector_error_requires_legacy_security(&error) {
                        return Ok(ClientRdpSessionExit::LegacySecurityRequired);
                    }
                    return Err(format_connector_error("RDP connection failed", &error));
                }
                ClientRdpOutput::Terminated(message) => {
                    send_event(
                        writer,
                        RemoteDesktopHelperEvent::Disconnected {
                            reason: Some(message),
                        },
                    )?;
                    return Ok(ClientRdpSessionExit::Closed);
                }
                ClientRdpOutput::OutputEnded => return Ok(ClientRdpSessionExit::Closed),
            }
        }

        loop {
            match request_rx.try_recv() {
                Ok(RemoteDesktopHelperRequest::Close) => return Ok(ClientRdpSessionExit::Closed),
                Ok(RemoteDesktopHelperRequest::Reconnect) => {
                    return Ok(ClientRdpSessionExit::ReconnectRequested);
                }
                Ok(request) => {
                    forward_client_rdp_request(input_tx, &mut input_database, request, read_only)?
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(ClientRdpSessionExit::Closed),
            }
        }

        thread::sleep(RDP_CLIENT_LOOP_POLL_INTERVAL);
    }
}

async fn handle_deactivate_all<ReadStream, WriteStream>(
    reader: &mut ironrdp_tokio::TokioFramed<ReadStream>,
    writer: &mut ironrdp_tokio::TokioFramed<WriteStream>,
    active_stage: &mut ActiveStage,
    image: &mut DecodedImage,
    mut connection_activation: Box<
        ironrdp::connector::connection_activation::ConnectionActivationSequence,
    >,
) -> SessionResult<()>
where
    ReadStream: AsyncRead + Send + Sync + Unpin,
    WriteStream: AsyncWrite + Send + Sync + Unpin,
{
    let mut buffer = WriteBuf::new();
    loop {
        let written = single_sequence_step_read(reader, &mut *connection_activation, &mut buffer)
            .await
            .map_err(|error| {
                session::custom_err!("read deactivation-reactivation sequence step", error)
            })?;
        if written.size().is_some() {
            writer.write_all(buffer.filled()).await.map_err(|error| {
                session::custom_err!("write deactivation-reactivation sequence step", error)
            })?;
        }

        if let ConnectionActivationState::Finalized {
            io_channel_id,
            user_channel_id,
            desktop_size,
            share_id,
            enable_server_pointer,
            pointer_software_rendering,
        } = connection_activation.connection_activation_state()
        {
            // The server can assign new channel IDs after reactivation; reset
            // both the decoded image and active stage before accepting pixels.
            *image =
                DecodedImage::new(PixelFormat::RgbA32, desktop_size.width, desktop_size.height);
            active_stage.set_fastpath_processor(
                fast_path::ProcessorBuilder {
                    io_channel_id,
                    user_channel_id,
                    share_id,
                    enable_server_pointer,
                    pointer_software_rendering,
                    bulk_decompressor: None,
                }
                .build(),
            );
            active_stage.set_share_id(share_id);
            active_stage.set_enable_server_pointer(enable_server_pointer);
            return Ok(());
        }
    }
}

fn graphics_update_event(
    image: &DecodedImage,
    region: InclusiveRectangle,
    sent_initial_frame: &mut bool,
) -> SessionResult<RemoteDesktopHelperEvent> {
    if !*sent_initial_frame {
        *sent_initial_frame = true;
        return Ok(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                remote_size_for_image(image),
                RemoteDesktopFrameFormat::Rgba8,
                opaque_rgba_bytes(image.data()),
            ),
        });
    }

    let rect = normalized_update_rect(image, region)?;
    Ok(RemoteDesktopHelperEvent::FrameUpdate {
        update: RemoteDesktopFrameUpdate::new(
            remote_size_for_image(image),
            rect,
            RemoteDesktopFrameFormat::Rgba8,
            copy_image_rect(image.data(), image.width(), rect),
        ),
    })
}

fn remote_size_for_image(image: &DecodedImage) -> RemoteDesktopSize {
    RemoteDesktopSize {
        width: u32::from(image.width()),
        height: u32::from(image.height()),
    }
}

fn remote_size_from_desktop(size: DesktopSize) -> RemoteDesktopSize {
    RemoteDesktopSize {
        width: u32::from(size.width),
        height: u32::from(size.height),
    }
}

fn normalized_update_rect(
    image: &DecodedImage,
    region: InclusiveRectangle,
) -> SessionResult<RemoteDesktopRect> {
    if region.right >= image.width()
        || region.bottom >= image.height()
        || region.left > region.right
        || region.top > region.bottom
    {
        return Err(session::general_err!(
            "RDP graphics update region is outside the image"
        ));
    }
    Ok(RemoteDesktopRect::new(
        u32::from(region.left),
        u32::from(region.top),
        u32::from(region.width()),
        u32::from(region.height()),
    ))
}

fn copy_image_rect(rgba_bytes: &[u8], image_width: u16, rect: RemoteDesktopRect) -> Vec<u8> {
    let pixel_size = RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel();
    let image_width = usize::from(image_width);
    let rect_x = usize::try_from(rect.x).unwrap_or(usize::MAX);
    let rect_y = usize::try_from(rect.y).unwrap_or(usize::MAX);
    let rect_width = usize::try_from(rect.width).unwrap_or(0);
    let rect_height = usize::try_from(rect.height).unwrap_or(0);
    let mut bytes = Vec::with_capacity(rect_width * rect_height * pixel_size);
    for row in 0..rect_height {
        let start = ((rect_y + row) * image_width + rect_x) * pixel_size;
        let end = start + rect_width * pixel_size;
        bytes.extend_from_slice(&rgba_bytes[start..end]);
    }
    set_rgba_alpha_opaque(&mut bytes);
    bytes
}

fn opaque_rgba_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    set_rgba_alpha_opaque(&mut bytes);
    bytes
}

fn set_rgba_alpha_opaque(bytes: &mut [u8]) {
    for pixel in bytes.chunks_exact_mut(RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel()) {
        pixel[3] = 0xff;
    }
}

fn build_client_rdp_config(config: &RdpWorkerConfig) -> Result<ClientRdpConfig, String> {
    let requested_size = RemoteDesktopSize::clamped(config.size.width, config.size.height);
    let width = u16::try_from(requested_size.width).unwrap_or(u16::MAX);
    let height = u16::try_from(requested_size.height).unwrap_or(u16::MAX);
    let codecs = client_codecs_capabilities(&[])
        .map_err(|error| format!("RDP bitmap codec setup failed: {error}"))?;
    let password = config.password.expose_secret().to_string();

    // IronRDP requires owned credential strings in its connector config. That
    // downstream copy lives only inside this helper process for the session,
    // is never logged, and is dropped with the native client config; the
    // worker config still zeroizes the UI-provided secret wrapper.
    let connector = connector::Config {
        credentials: Credentials::UsernamePassword {
            username: config.username.clone(),
            password,
        },
        domain: config.domain.clone(),
        enable_tls: true,
        enable_credssp: true,
        desktop_size: connector::DesktopSize { width, height },
        desktop_scale_factor: 0,
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_layout: 0,
        keyboard_functional_keys_count: 12,
        ime_file_name: String::new(),
        bitmap: Some(connector::BitmapConfig {
            lossy_compression: true,
            color_depth: 32,
            codecs,
        }),
        dig_product_id: String::new(),
        client_build: client_build_number()?,
        client_name: RDP_CLIENT_NAME.to_string(),
        client_dir: "C:\\Windows\\System32\\mstscax.dll".to_string(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        platform: current_platform_type(),
        hardware_id: None,
        license_cache: None,
        request_data: None,
        autologon: true,
        enable_audio_playback: false,
        enable_server_pointer: true,
        pointer_software_rendering: false,
        multitransport_flags: None,
        compression_type: Some(CompressionType::Rdp61),
        performance_flags: PerformanceFlags::default(),
        timezone_info: TimezoneInfo::default(),
    };

    Ok(ClientRdpConfig {
        destination: ClientRdpDestination::from_parts(&config.endpoint.host, config.endpoint.port),
        connector,
    })
}

fn forward_client_rdp_request(
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    input_database: &mut RdpInputDatabase,
    request: RemoteDesktopHelperRequest,
    read_only: bool,
) -> Result<(), String> {
    match request {
        RemoteDesktopHelperRequest::Resize { size } => {
            let (width, height) = MonitorLayoutEntry::adjust_display_size(size.width, size.height);
            input_tx
                .send(RdpInputEvent::Resize {
                    width: clamp_u32_to_u16(width),
                    height: clamp_u32_to_u16(height),
                    scale_factor: 100,
                    physical_size: None,
                })
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::MouseMove { x, y } if !read_only => {
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                [RdpInputOperation::MouseMove(MousePosition {
                    x: clamp_u32_to_u16(x),
                    y: clamp_u32_to_u16(y),
                })],
            )?;
        }
        RemoteDesktopHelperRequest::MouseButton { button, state } if !read_only => {
            if let Some(button) = rdp_mouse_button(button) {
                let operation = match state {
                    RemoteDesktopMouseButtonState::Pressed => {
                        RdpInputOperation::MouseButtonPressed(button)
                    }
                    RemoteDesktopMouseButtonState::Released => {
                        RdpInputOperation::MouseButtonReleased(button)
                    }
                };
                send_client_rdp_input_operations(input_tx, input_database, [operation])?;
            }
        }
        RemoteDesktopHelperRequest::Wheel { delta } if !read_only => {
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                rdp_wheel_operations(delta),
            )?;
        }
        RemoteDesktopHelperRequest::Key { key, state } if !read_only => {
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                rdp_key_operations(&key, state),
            )?;
        }
        RemoteDesktopHelperRequest::Text { text } if !read_only => {
            for character in text.chars().filter(|character| !character.is_control()) {
                send_client_rdp_input_operations(
                    input_tx,
                    input_database,
                    [
                        RdpInputOperation::UnicodeKeyPressed(character),
                        RdpInputOperation::UnicodeKeyReleased(character),
                    ],
                )?;
            }
        }
        RemoteDesktopHelperRequest::ClipboardText { text } if !read_only => {
            input_tx
                .send(RdpInputEvent::SetClipboardText(text))
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::Connect { .. }
        | RemoteDesktopHelperRequest::Close
        | RemoteDesktopHelperRequest::Reconnect
        | RemoteDesktopHelperRequest::MouseMove { .. }
        | RemoteDesktopHelperRequest::MouseButton { .. }
        | RemoteDesktopHelperRequest::Wheel { .. }
        | RemoteDesktopHelperRequest::Key { .. }
        | RemoteDesktopHelperRequest::Text { .. }
        | RemoteDesktopHelperRequest::ClipboardText { .. } => {}
    }
    Ok(())
}

fn send_client_rdp_input_operations<I>(
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    input_database: &mut RdpInputDatabase,
    operations: I,
) -> Result<(), String>
where
    I: IntoIterator<Item = RdpInputOperation>,
{
    let events = input_database.apply(operations);
    if events.is_empty() {
        return Ok(());
    }
    input_tx
        .send(RdpInputEvent::FastPath(events))
        .map_err(|_| "RDP input channel is closed.".to_string())
}

fn client_build_number() -> Result<u32, String> {
    let version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("RDP client version parse failed: {error}"))?;
    let build = version
        .major
        .saturating_mul(100)
        .saturating_add(version.minor.saturating_mul(10))
        .saturating_add(version.patch);
    u32::try_from(build).map_err(|error| format!("RDP client build number overflowed: {error}"))
}

fn format_graceful_disconnect(reason: GracefulDisconnectReason) -> String {
    reason.to_string()
}

#[cfg(feature = "legacy-freerdp")]
fn run_legacy_rdp_worker(
    config: RdpWorkerConfig,
    writer: SharedEventWriter,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
) -> Result<(), String> {
    legacy_freerdp::run(config, writer, request_rx)
}

#[cfg(not(feature = "legacy-freerdp"))]
fn run_legacy_rdp_worker(
    _config: RdpWorkerConfig,
    _writer: SharedEventWriter,
    _request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
) -> Result<(), String> {
    Err(LEGACY_RDP_ENGINE_UNAVAILABLE_MESSAGE.to_string())
}

fn connector_error_requires_legacy_security(error: &connector::ConnectorError) -> bool {
    connector_error_search_text(error).contains("STANDARD_RDP_SECURITY")
}

fn format_connector_error(stage: &str, error: &connector::ConnectorError) -> String {
    match error.kind() {
        _ if connector_error_requires_legacy_security(error) => {
            LEGACY_RDP_SECURITY_MESSAGE.to_string()
        }
        ConnectorErrorKind::Reason(reason) => format!("{stage}: {reason}"),
        ConnectorErrorKind::Negotiation(failure) => format!("{stage}: {failure}"),
        ConnectorErrorKind::Credssp(_) => format!("{stage}: CredSSP authentication failed."),
        ConnectorErrorKind::Encode(_) => {
            format!("{stage}: failed to encode an RDP protocol message.")
        }
        ConnectorErrorKind::Decode(_) => {
            format!("{stage}: failed to decode an RDP protocol message.")
        }
        ConnectorErrorKind::AccessDenied => format!("{stage}: access denied by the RDP server."),
        ConnectorErrorKind::General => format!("{stage}: general RDP connector error."),
        ConnectorErrorKind::Custom => connector_error_source_summary(error)
            .map(|summary| format!("{stage}: {summary}"))
            .unwrap_or_else(|| format!("{stage}: RDP connector error.")),
        _ => connector_error_source_summary(error)
            .map(|summary| format!("{stage}: {summary}"))
            .unwrap_or_else(|| format!("{stage}: RDP connector error.")),
    }
}

fn connector_error_search_text(error: &connector::ConnectorError) -> String {
    let mut parts = vec![error.kind().to_string()];
    parts.extend(connector_error_source_messages(error));
    parts.join(" | ")
}

fn connector_error_source_summary(error: &connector::ConnectorError) -> Option<String> {
    let messages = connector_error_source_messages(error);
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("; caused by: "))
    }
}

fn connector_error_source_messages(error: &connector::ConnectorError) -> Vec<String> {
    use std::error::Error as _;

    let mut messages = Vec::new();
    let mut source = error.source();
    while let Some(current) = source {
        let message = sanitize_connector_error_text(&current.to_string());
        if !message.is_empty() && !messages.iter().any(|existing| existing == &message) {
            messages.push(message);
        }
        source = current.source();
    }
    messages
}

fn sanitize_connector_error_text(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let mut cursor = 0;
    while let Some(relative_at) = message[cursor..].find(" @ ") {
        let at = cursor + relative_at;
        let Some(close_relative) = message[at..].find(']') else {
            break;
        };
        let close = at + close_relative;
        let location = &message[at + 3..close];
        if looks_like_source_location(location) {
            // IronRDP's Display includes construction locations. Keep the
            // protocol context but strip local checkout paths before UI output.
            output.push_str(&message[cursor..at]);
            cursor = close;
        } else {
            output.push_str(&message[cursor..at + 3]);
            cursor = at + 3;
        }
    }
    output.push_str(&message[cursor..]);
    output
}

fn looks_like_source_location(value: &str) -> bool {
    let Some((path, line)) = value.rsplit_once(':') else {
        return false;
    };
    !path.is_empty()
        && line.chars().all(|character| character.is_ascii_digit())
        && (path.contains('/') || path.contains('\\') || path.ends_with(".rs"))
}

fn current_platform_type() -> MajorPlatformType {
    if cfg!(target_os = "windows") {
        MajorPlatformType::WINDOWS
    } else if cfg!(target_os = "macos") {
        MajorPlatformType::MACINTOSH
    } else if cfg!(target_os = "ios") {
        MajorPlatformType::IOS
    } else if cfg!(target_os = "android") {
        MajorPlatformType::ANDROID
    } else {
        MajorPlatformType::UNIX
    }
}

fn rdp_mouse_button(button: RemoteDesktopMouseButton) -> Option<RdpMouseButton> {
    match button {
        RemoteDesktopMouseButton::Left => Some(RdpMouseButton::Left),
        RemoteDesktopMouseButton::Middle => Some(RdpMouseButton::Middle),
        RemoteDesktopMouseButton::Right => Some(RdpMouseButton::Right),
        RemoteDesktopMouseButton::Back => Some(RdpMouseButton::X1),
        RemoteDesktopMouseButton::Forward => Some(RdpMouseButton::X2),
    }
}

fn rdp_wheel_operations(delta: RemoteDesktopWheelDelta) -> Vec<RdpInputOperation> {
    let mut operations = Vec::new();
    if delta.x.abs() > f32::EPSILON {
        operations.push(RdpInputOperation::WheelRotations(WheelRotations {
            is_vertical: false,
            rotation_units: rdp_wheel_units(delta.x),
        }));
    }
    if delta.y.abs() > f32::EPSILON {
        operations.push(RdpInputOperation::WheelRotations(WheelRotations {
            is_vertical: true,
            rotation_units: rdp_wheel_units(delta.y),
        }));
    }
    operations
}

fn rdp_wheel_units(delta: f32) -> i16 {
    let units = if delta.abs() < RDP_WHEEL_UNIT {
        delta.signum() * RDP_WHEEL_UNIT
    } else {
        delta
    };
    units
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

fn rdp_key_operations(
    key: &RemoteDesktopKey,
    state: RemoteDesktopKeyState,
) -> Vec<RdpInputOperation> {
    if let Some(character) = printable_remote_text(key) {
        return vec![match state {
            RemoteDesktopKeyState::Pressed => RdpInputOperation::UnicodeKeyPressed(character),
            RemoteDesktopKeyState::Released => RdpInputOperation::UnicodeKeyReleased(character),
        }];
    }

    if let Some(scancode) = rdp_scancode(&key.code) {
        let mut operations = Vec::new();
        let is_modifier_key = rdp_modifier_scancode_for_key(&key.code).is_some();
        let mut modifiers = if is_modifier_key {
            Vec::new()
        } else {
            rdp_modifier_scancodes(key)
        };
        match state {
            RemoteDesktopKeyState::Pressed => {
                operations.extend(modifiers.iter().copied().map(RdpInputOperation::KeyPressed));
                operations.push(RdpInputOperation::KeyPressed(scancode));
            }
            RemoteDesktopKeyState::Released => {
                operations.push(RdpInputOperation::KeyReleased(scancode));
                modifiers.reverse();
                operations.extend(modifiers.into_iter().map(RdpInputOperation::KeyReleased));
            }
        }
        return operations;
    }

    key.text
        .as_deref()
        .and_then(single_non_control_char)
        .map(|character| {
            vec![match state {
                RemoteDesktopKeyState::Pressed => RdpInputOperation::UnicodeKeyPressed(character),
                RemoteDesktopKeyState::Released => RdpInputOperation::UnicodeKeyReleased(character),
            }]
        })
        .unwrap_or_default()
}

fn printable_remote_text(key: &RemoteDesktopKey) -> Option<char> {
    if key.ctrl || key.alt || key.meta {
        return None;
    }
    key.text.as_deref().and_then(single_non_control_char)
}

fn single_non_control_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let character = chars.next()?;
    if chars.next().is_some() || character.is_control() {
        None
    } else {
        Some(character)
    }
}

fn rdp_scancode(code: &str) -> Option<Scancode> {
    let normalized = code.to_ascii_lowercase();
    let scancode = match normalized.as_str() {
        "escape" | "esc" => Scancode::from_u8(false, 0x01),
        "backspace" => Scancode::from_u8(false, 0x0e),
        "tab" => Scancode::from_u8(false, 0x0f),
        "enter" | "return" => Scancode::from_u8(false, 0x1c),
        "space" | " " => Scancode::from_u8(false, 0x39),
        "shift" => Scancode::from_u8(false, 0x2a),
        "control" | "ctrl" => Scancode::from_u8(false, 0x1d),
        "alt" => Scancode::from_u8(false, 0x38),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" => Scancode::from_u16(0xe05b),
        "capslock" | "caps_lock" => Scancode::from_u8(false, 0x3a),
        "delete" => Scancode::from_u8(true, 0x53),
        "insert" => Scancode::from_u8(true, 0x52),
        "home" => Scancode::from_u8(true, 0x47),
        "end" => Scancode::from_u8(true, 0x4f),
        "pageup" | "page_up" => Scancode::from_u8(true, 0x49),
        "pagedown" | "page_down" => Scancode::from_u8(true, 0x51),
        "arrowup" | "up" => Scancode::from_u8(true, 0x48),
        "arrowdown" | "down" => Scancode::from_u8(true, 0x50),
        "arrowleft" | "left" => Scancode::from_u8(true, 0x4b),
        "arrowright" | "right" => Scancode::from_u8(true, 0x4d),
        "f1" => Scancode::from_u8(false, 0x3b),
        "f2" => Scancode::from_u8(false, 0x3c),
        "f3" => Scancode::from_u8(false, 0x3d),
        "f4" => Scancode::from_u8(false, 0x3e),
        "f5" => Scancode::from_u8(false, 0x3f),
        "f6" => Scancode::from_u8(false, 0x40),
        "f7" => Scancode::from_u8(false, 0x41),
        "f8" => Scancode::from_u8(false, 0x42),
        "f9" => Scancode::from_u8(false, 0x43),
        "f10" => Scancode::from_u8(false, 0x44),
        "f11" => Scancode::from_u8(false, 0x57),
        "f12" => Scancode::from_u8(false, 0x58),
        _ => return ascii_scancode(normalized.as_str()),
    };
    Some(scancode)
}

fn rdp_modifier_scancodes(key: &RemoteDesktopKey) -> Vec<Scancode> {
    let mut scancodes = Vec::with_capacity(4);
    if key.ctrl {
        scancodes.push(Scancode::from_u8(false, 0x1d));
    }
    if key.shift {
        scancodes.push(Scancode::from_u8(false, 0x2a));
    }
    if key.alt {
        scancodes.push(Scancode::from_u8(false, 0x38));
    }
    if key.meta {
        scancodes.push(Scancode::from_u16(0xe05b));
    }
    scancodes
}

fn rdp_modifier_scancode_for_key(code: &str) -> Option<Scancode> {
    match code.to_ascii_lowercase().as_str() {
        "shift" => Some(Scancode::from_u8(false, 0x2a)),
        "control" | "ctrl" => Some(Scancode::from_u8(false, 0x1d)),
        "alt" => Some(Scancode::from_u8(false, 0x38)),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" => {
            Some(Scancode::from_u16(0xe05b))
        }
        _ => None,
    }
}

fn text_clipboard_formats() -> Vec<ClipboardFormat> {
    vec![
        ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
        ClipboardFormat::new(ClipboardFormatId::CF_TEXT),
    ]
}

fn preferred_text_clipboard_format(formats: &[ClipboardFormat]) -> Option<ClipboardFormatId> {
    formats
        .iter()
        .find(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_TEXT)
        })
        .map(|format| format.id)
}

fn ascii_scancode(code: &str) -> Option<Scancode> {
    let scan_code = match code {
        "a" => 0x1e,
        "b" => 0x30,
        "c" => 0x2e,
        "d" => 0x20,
        "e" => 0x12,
        "f" => 0x21,
        "g" => 0x22,
        "h" => 0x23,
        "i" => 0x17,
        "j" => 0x24,
        "k" => 0x25,
        "l" => 0x26,
        "m" => 0x32,
        "n" => 0x31,
        "o" => 0x18,
        "p" => 0x19,
        "q" => 0x10,
        "r" => 0x13,
        "s" => 0x1f,
        "t" => 0x14,
        "u" => 0x16,
        "v" => 0x2f,
        "w" => 0x11,
        "x" => 0x2d,
        "y" => 0x15,
        "z" => 0x2c,
        "1" => 0x02,
        "2" => 0x03,
        "3" => 0x04,
        "4" => 0x05,
        "5" => 0x06,
        "6" => 0x07,
        "7" => 0x08,
        "8" => 0x09,
        "9" => 0x0a,
        "0" => 0x0b,
        "-" => 0x0c,
        "=" => 0x0d,
        "[" => 0x1a,
        "]" => 0x1b,
        "\\" => 0x2b,
        ";" => 0x27,
        "'" => 0x28,
        "`" => 0x29,
        "," => 0x33,
        "." => 0x34,
        "/" => 0x35,
        _ => return None,
    };
    Some(Scancode::from_u8(false, scan_code))
}

#[cfg(feature = "legacy-freerdp")]
mod legacy_freerdp {
    use std::ffi::{CStr, CString};

    use freerdp2::{
        PIXEL_FORMAT_BGRA32, RdpError, Settings,
        client::{Context, Handler},
        input::{KbdFlags, PtrFlags, PtrXFlags, WHEEL_ROTATION_MASK},
        locale::keyboard_init_ex,
        sys,
        update::UpdateHandler,
        winpr::{WaitResult, wait_for_multiple_objects},
    };
    use zeroize::Zeroizing;

    use super::*;

    const LEGACY_EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(25);

    pub(super) fn run(
        config: RdpWorkerConfig,
        writer: SharedEventWriter,
        request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    ) -> Result<(), String> {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Connecting,
                message: Some("Opening legacy RDP session with FreeRDP.".to_string()),
            },
        )?;

        let mut context = Context::new(LegacyFreeRdpHandler {
            writer: writer.clone(),
        });
        context
            .client_start()
            .map_err(|error| format_freerdp_error("Legacy RDP client startup failed", &error))?;
        configure_settings(&mut context.settings, &config)?;

        if let Err(error) = context.instance.connect() {
            let message =
                format_freerdp_context_error("Legacy RDP connection failed", &context, &error);
            let _ = context.client_stop();
            return Err(message);
        }

        let mut mouse_position = MousePositionCache::default();
        let result = run_event_loop(
            &mut context,
            &request_rx,
            config.read_only,
            &mut mouse_position,
        );
        let _ = context.client_stop();
        result
    }

    fn run_event_loop(
        context: &mut Context<LegacyFreeRdpHandler>,
        request_rx: &mpsc::Receiver<RemoteDesktopHelperRequest>,
        read_only: bool,
        mouse_position: &mut MousePositionCache,
    ) -> Result<(), String> {
        loop {
            process_pending_requests(context, request_rx, read_only, mouse_position)?;
            if context.instance.shall_disconnect() {
                return send_event(
                    &context.handler.writer,
                    RemoteDesktopHelperEvent::Disconnected {
                        reason: Some("Legacy RDP session closed.".to_string()),
                    },
                );
            }

            let handles = context.event_handles().map_err(|error| {
                format_freerdp_error("Legacy RDP event handle setup failed", &error)
            })?;
            if handles.is_empty() {
                thread::sleep(LEGACY_EVENT_POLL_TIMEOUT);
                continue;
            }

            let wait_handles = handles.iter().collect::<Vec<_>>();
            match wait_for_multiple_objects(&wait_handles, false, Some(&LEGACY_EVENT_POLL_TIMEOUT))
                .map_err(|error| format_freerdp_error("Legacy RDP wait failed", &error))?
            {
                WaitResult::Timeout => continue,
                WaitResult::Object(_) | WaitResult::Abandoned(_) => {}
            }

            if !context.check_event_handles() {
                if let Some(error) = context.last_error() {
                    return Err(format!("Legacy RDP event processing failed: {error:?}"));
                }
                return Err("Legacy RDP event processing failed.".to_string());
            }
        }
    }

    fn configure_settings(settings: &mut Settings, config: &RdpWorkerConfig) -> Result<(), String> {
        let requested_size = RemoteDesktopSize::clamped(config.size.width, config.size.height);
        settings
            .set_server_hostname(Some(&config.endpoint.host))
            .map_err(|error| format_freerdp_error("Legacy RDP hostname setup failed", &error))?;
        settings.set_server_port(u32::from(config.endpoint.port));
        settings
            .set_username(Some(&config.username))
            .map_err(|error| format_freerdp_error("Legacy RDP username setup failed", &error))?;
        if let Some(domain) = config.domain.as_deref().filter(|domain| !domain.is_empty()) {
            set_freerdp_string(settings, sys::FreeRDP_Domain, domain)
                .map_err(|error| format!("Legacy RDP domain setup failed: {error}"))?;
        }

        // FreeRDP owns a copied password inside its settings object until the
        // context is dropped. The temporary C buffer is zeroized immediately
        // after the settings handoff returns.
        set_freerdp_secret_string(
            settings,
            sys::FreeRDP_Password,
            config.password.expose_secret(),
        )
        .map_err(|error| format!("Legacy RDP password setup failed: {error}"))?;

        set_freerdp_u32(settings, sys::FreeRDP_DesktopWidth, requested_size.width)?;
        set_freerdp_u32(settings, sys::FreeRDP_DesktopHeight, requested_size.height)?;
        set_freerdp_u32(settings, sys::FreeRDP_ColorDepth, 32)?;

        // Force the fallback onto classic Standard RDP Security. TLS, NLA and
        // negotiation stay disabled so a server that only offered Standard RDP
        // does not reject the second attempt again.
        set_freerdp_bool(settings, sys::FreeRDP_RdpSecurity, true)?;
        set_freerdp_bool(settings, sys::FreeRDP_UseRdpSecurityLayer, true)?;
        set_freerdp_bool(settings, sys::FreeRDP_TlsSecurity, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_NlaSecurity, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_ExtSecurity, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_NegotiateSecurityLayer, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_Authentication, true)?;
        set_freerdp_bool(settings, sys::FreeRDP_IgnoreCertificate, true)?;
        set_freerdp_bool(settings, sys::FreeRDP_AutoAcceptCertificate, true)?;

        // The legacy path prefers server bitmap updates over modern graphics
        // codecs because old Standard RDP servers often do not advertise GFX.
        set_freerdp_bool(settings, sys::FreeRDP_SupportGraphicsPipeline, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_GfxThinClient, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_GfxSmallCache, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_GfxProgressive, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_GfxH264, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_GfxAVC444, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_GfxAVC444v2, false)?;
        set_freerdp_bool(settings, sys::FreeRDP_NetworkAutoDetect, false)?;
        settings.set_support_display_control(false);
        Ok(())
    }

    fn process_pending_requests(
        context: &mut Context<LegacyFreeRdpHandler>,
        request_rx: &mpsc::Receiver<RemoteDesktopHelperRequest>,
        read_only: bool,
        mouse_position: &mut MousePositionCache,
    ) -> Result<(), String> {
        loop {
            match request_rx.try_recv() {
                Ok(request) => handle_request(context, request, read_only, mouse_position)?,
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::TryRecvError::Disconnected) => {
                    context.instance.disconnect().map_err(|error| {
                        format_freerdp_error("Legacy RDP disconnect failed", &error)
                    })?;
                    return Ok(());
                }
            }
        }
    }

    fn handle_request(
        context: &mut Context<LegacyFreeRdpHandler>,
        request: RemoteDesktopHelperRequest,
        read_only: bool,
        mouse_position: &mut MousePositionCache,
    ) -> Result<(), String> {
        match request {
            RemoteDesktopHelperRequest::Close => {
                context.instance.disconnect().map_err(|error| {
                    format_freerdp_error("Legacy RDP disconnect failed", &error)
                })?;
            }
            RemoteDesktopHelperRequest::Reconnect => {
                send_event(
                    &context.handler.writer,
                    RemoteDesktopHelperEvent::Status {
                        status: RemoteDesktopSessionStatus::Reconnecting,
                        message: Some("Reopening legacy RDP session.".to_string()),
                    },
                )?;
                context
                    .instance
                    .reconnect()
                    .map_err(|error| format_freerdp_error("Legacy RDP reconnect failed", &error))?;
            }
            RemoteDesktopHelperRequest::Resize { .. } => {
                // FreeRDP 2 display-control support is not reliable enough for
                // the legacy fallback yet; keep the existing desktop size.
            }
            RemoteDesktopHelperRequest::MouseMove { x, y } if !read_only => {
                mouse_position.update(x, y);
                input(context)?
                    .send_mouse_event(PtrFlags::MOVE, mouse_position.x, mouse_position.y)
                    .map_err(|error| {
                        format_freerdp_error("Legacy RDP mouse move failed", &error)
                    })?;
            }
            RemoteDesktopHelperRequest::MouseButton { button, state } if !read_only => {
                send_mouse_button(context, button, state, *mouse_position)?;
            }
            RemoteDesktopHelperRequest::Wheel { delta } if !read_only => {
                send_wheel(context, delta, *mouse_position)?;
            }
            RemoteDesktopHelperRequest::Key { key, state } if !read_only => {
                send_key(context, &key, state)?;
            }
            RemoteDesktopHelperRequest::Text { text } if !read_only => {
                for character in text.chars().filter(|character| !character.is_control()) {
                    send_unicode_key(context, character, RemoteDesktopKeyState::Pressed)?;
                    send_unicode_key(context, character, RemoteDesktopKeyState::Released)?;
                }
            }
            RemoteDesktopHelperRequest::ClipboardText { .. } => {
                // Clipboard transport is intentionally left with the primary
                // IronRDP path until the helper protocol grows CLIPRDR parity.
            }
            RemoteDesktopHelperRequest::Connect { .. } => {
                return Err("Legacy RDP helper received a second connect request.".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    fn send_mouse_button(
        context: &mut Context<LegacyFreeRdpHandler>,
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
        position: MousePositionCache,
    ) -> Result<(), String> {
        if let Some(flags) = legacy_mouse_button_flags(button, state) {
            input(context)?
                .send_mouse_event(flags, position.x, position.y)
                .map_err(|error| format_freerdp_error("Legacy RDP mouse button failed", &error))?;
            return Ok(());
        }

        let Some(flags) = legacy_extended_mouse_button_flags(button, state) else {
            return Ok(());
        };
        input(context)?
            .send_extended_mouse_event(flags, position.x, position.y)
            .map_err(|error| {
                format_freerdp_error("Legacy RDP extended mouse button failed", &error)
            })
    }

    fn send_wheel(
        context: &mut Context<LegacyFreeRdpHandler>,
        delta: RemoteDesktopWheelDelta,
        position: MousePositionCache,
    ) -> Result<(), String> {
        if delta.x.abs() > f32::EPSILON {
            input(context)?
                .send_mouse_event(legacy_wheel_flags(false, delta.x), position.x, position.y)
                .map_err(|error| {
                    format_freerdp_error("Legacy RDP horizontal wheel failed", &error)
                })?;
        }
        if delta.y.abs() > f32::EPSILON {
            input(context)?
                .send_mouse_event(legacy_wheel_flags(true, delta.y), position.x, position.y)
                .map_err(|error| {
                    format_freerdp_error("Legacy RDP vertical wheel failed", &error)
                })?;
        }
        Ok(())
    }

    fn send_key(
        context: &mut Context<LegacyFreeRdpHandler>,
        key: &RemoteDesktopKey,
        state: RemoteDesktopKeyState,
    ) -> Result<(), String> {
        if let Some(scancode) = rdp_scancode(&key.code) {
            let (flags, code) = legacy_keyboard_event(scancode, state);
            input(context)?
                .send_keyboard_event(flags, code)
                .map_err(|error| {
                    format_freerdp_error("Legacy RDP keyboard event failed", &error)
                })?;
            return Ok(());
        }

        if let Some(character) = key.text.as_deref().and_then(single_non_control_char) {
            send_unicode_key(context, character, state)?;
        }
        Ok(())
    }

    fn send_unicode_key(
        context: &mut Context<LegacyFreeRdpHandler>,
        character: char,
        state: RemoteDesktopKeyState,
    ) -> Result<(), String> {
        let Some(code) = legacy_unicode_code_unit(character) else {
            return Ok(());
        };
        let flags = legacy_key_flags(false, state);
        input(context)?
            .send_unicode_keyboard_event(flags, code)
            .map_err(|error| format_freerdp_error("Legacy RDP unicode key failed", &error))
    }

    fn legacy_mouse_button_flags(
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
    ) -> Option<PtrFlags> {
        let button_flag = match button {
            RemoteDesktopMouseButton::Left => PtrFlags::BUTTON1,
            RemoteDesktopMouseButton::Right => PtrFlags::BUTTON2,
            RemoteDesktopMouseButton::Middle => PtrFlags::BUTTON3,
            RemoteDesktopMouseButton::Back | RemoteDesktopMouseButton::Forward => return None,
        };
        let mut flags = button_flag;
        if state == RemoteDesktopMouseButtonState::Pressed {
            flags |= PtrFlags::DOWN;
        }
        Some(flags)
    }

    fn legacy_extended_mouse_button_flags(
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
    ) -> Option<PtrXFlags> {
        let button_flag = match button {
            RemoteDesktopMouseButton::Back => PtrXFlags::BUTTON1,
            RemoteDesktopMouseButton::Forward => PtrXFlags::BUTTON2,
            RemoteDesktopMouseButton::Left
            | RemoteDesktopMouseButton::Middle
            | RemoteDesktopMouseButton::Right => return None,
        };
        let mut flags = button_flag;
        if state == RemoteDesktopMouseButtonState::Pressed {
            flags |= PtrXFlags::DOWN;
        }
        Some(flags)
    }

    fn legacy_wheel_flags(is_vertical: bool, delta: f32) -> PtrFlags {
        let units = rdp_wheel_units(delta);
        let mut flags = if is_vertical {
            PtrFlags::WHEEL
        } else {
            PtrFlags::HWHEEL
        };
        if units < 0 {
            flags |= PtrFlags::WHEEL_NEGATIVE;
        }
        flags | PtrFlags::from_bits_truncate(units.unsigned_abs() & WHEEL_ROTATION_MASK)
    }

    fn legacy_keyboard_event(scancode: Scancode, state: RemoteDesktopKeyState) -> (KbdFlags, u16) {
        let raw = scancode.as_u16();
        let code = raw & 0x00ff;
        let extended = (raw & 0xff00) != 0;
        (legacy_key_flags(extended, state), code)
    }

    fn legacy_key_flags(extended: bool, state: RemoteDesktopKeyState) -> KbdFlags {
        let mut flags = if state == RemoteDesktopKeyState::Pressed {
            KbdFlags::DOWN
        } else {
            KbdFlags::RELEASE
        };
        if extended {
            flags |= KbdFlags::EXTENDED;
        }
        flags
    }

    fn legacy_unicode_code_unit(character: char) -> Option<u16> {
        let mut buffer = [0; 2];
        let encoded = character.encode_utf16(&mut buffer);
        if encoded.len() == 1 {
            Some(encoded[0])
        } else {
            None
        }
    }

    fn input(
        context: &mut Context<LegacyFreeRdpHandler>,
    ) -> Result<freerdp2::input::Input<'_>, String> {
        context
            .input()
            .ok_or_else(|| "Legacy RDP input channel is not available.".to_string())
    }

    fn set_freerdp_bool(settings: &mut Settings, id: u32, value: bool) -> Result<(), String> {
        if unsafe { sys::freerdp_settings_set_bool(settings.as_ptr(), id as _, value as _) } != 0 {
            Ok(())
        } else {
            Err(format!("FreeRDP bool setting {id} failed"))
        }
    }

    fn set_freerdp_u32(settings: &mut Settings, id: u32, value: u32) -> Result<(), String> {
        if unsafe { sys::freerdp_settings_set_uint32(settings.as_ptr(), id as _, value) } != 0 {
            Ok(())
        } else {
            Err(format!("FreeRDP integer setting {id} failed"))
        }
    }

    fn set_freerdp_string(settings: &mut Settings, id: u32, value: &str) -> Result<(), String> {
        let value = CString::new(value).map_err(|error| error.to_string())?;
        if unsafe { sys::freerdp_settings_set_string(settings.as_ptr(), id as _, value.as_ptr()) }
            != 0
        {
            Ok(())
        } else {
            Err(format!("FreeRDP string setting {id} failed"))
        }
    }

    fn set_freerdp_secret_string(
        settings: &mut Settings,
        id: u32,
        value: &str,
    ) -> Result<(), String> {
        let mut bytes = Zeroizing::new(value.as_bytes().to_vec());
        bytes.push(0);
        let value = CStr::from_bytes_with_nul(&bytes).map_err(|error| error.to_string())?;
        if unsafe { sys::freerdp_settings_set_string(settings.as_ptr(), id as _, value.as_ptr()) }
            != 0
        {
            Ok(())
        } else {
            Err(format!("FreeRDP secret string setting {id} failed"))
        }
    }

    fn format_freerdp_context_error(
        stage: &str,
        context: &Context<LegacyFreeRdpHandler>,
        error: &RdpError,
    ) -> String {
        if let Some(last_error) = context.last_error() {
            format!("{stage}: {error}; last error: {last_error:?}")
        } else {
            format_freerdp_error(stage, error)
        }
    }

    fn format_freerdp_error(stage: &str, error: &RdpError) -> String {
        format!("{stage}: {error}")
    }

    fn frame_from_gdi(
        context: &mut Context<LegacyFreeRdpHandler>,
    ) -> Result<RemoteDesktopFrame, String> {
        let gdi = context
            .gdi()
            .ok_or_else(|| "Legacy RDP GDI surface is not available.".to_string())?;
        let width = gdi
            .width()
            .ok_or_else(|| "Legacy RDP GDI width is invalid.".to_string())?;
        let height = gdi
            .height()
            .ok_or_else(|| "Legacy RDP GDI height is invalid.".to_string())?;
        let stride = usize::try_from(gdi.stride())
            .map_err(|error| format!("Legacy RDP GDI stride is invalid: {error}"))?;
        let buffer = gdi
            .primary_buffer()
            .ok_or_else(|| "Legacy RDP GDI primary buffer is not available.".to_string())?;
        let pixels = copy_bgra_frame(buffer, width, height, stride)?;
        Ok(RemoteDesktopFrame::new(
            RemoteDesktopSize { width, height },
            RemoteDesktopFrameFormat::Bgra8,
            pixels,
        ))
    }

    fn copy_bgra_frame(
        buffer: &[u8],
        width: u32,
        height: u32,
        stride: usize,
    ) -> Result<Vec<u8>, String> {
        let width = usize::try_from(width)
            .map_err(|error| format!("Legacy RDP frame width is invalid: {error}"))?;
        let height = usize::try_from(height)
            .map_err(|error| format!("Legacy RDP frame height is invalid: {error}"))?;
        let row_len = width
            .checked_mul(4)
            .ok_or_else(|| "Legacy RDP frame row size overflowed.".to_string())?;
        let frame_len = row_len
            .checked_mul(height)
            .ok_or_else(|| "Legacy RDP frame size overflowed.".to_string())?;
        if stride < row_len {
            return Err("Legacy RDP frame stride is smaller than the row width.".to_string());
        }
        if stride == row_len {
            return buffer
                .get(..frame_len)
                .map(ToOwned::to_owned)
                .ok_or_else(|| "Legacy RDP frame buffer is shorter than expected.".to_string());
        }

        let mut pixels = Vec::with_capacity(frame_len);
        for row in 0..height {
            let start = row
                .checked_mul(stride)
                .ok_or_else(|| "Legacy RDP frame stride offset overflowed.".to_string())?;
            let end = start
                .checked_add(row_len)
                .ok_or_else(|| "Legacy RDP frame row offset overflowed.".to_string())?;
            let row_bytes = buffer
                .get(start..end)
                .ok_or_else(|| "Legacy RDP frame buffer is shorter than expected.".to_string())?;
            pixels.extend_from_slice(row_bytes);
        }
        Ok(pixels)
    }

    struct LegacyFreeRdpHandler {
        writer: SharedEventWriter,
    }

    impl Handler for LegacyFreeRdpHandler {
        fn post_connect(&mut self, context: &mut Context<Self>) -> freerdp2::Result<()> {
            context.instance.gdi_init(PIXEL_FORMAT_BGRA32)?;
            let mut update = context.update().ok_or(RdpError::Unsupported)?;
            update.register::<LegacyUpdateHandler>();
            let _ = keyboard_init_ex(
                context.settings.keyboard_layout(),
                context.settings.keyboard_remapping_list().as_deref(),
            );

            let gdi = context.gdi().ok_or(RdpError::Unsupported)?;
            let width = gdi.width().ok_or(RdpError::Unsupported)?;
            let height = gdi.height().ok_or(RdpError::Unsupported)?;
            send_event(
                &self.writer,
                RemoteDesktopHelperEvent::Connected {
                    size: RemoteDesktopSize { width, height },
                },
            )
            .map_err(RdpError::Failed)?;
            Ok(())
        }
    }

    struct LegacyUpdateHandler;

    impl UpdateHandler for LegacyUpdateHandler {
        type ContextHandler = LegacyFreeRdpHandler;

        fn begin_paint(context: &mut Context<Self::ContextHandler>) -> freerdp2::Result<()> {
            let gdi = context.gdi().ok_or(RdpError::Unsupported)?;
            let mut primary = gdi.primary().ok_or(RdpError::Unsupported)?;
            primary.hdc().hwnd().invalid().set_null(true);
            Ok(())
        }

        fn end_paint(context: &mut Context<Self::ContextHandler>) -> freerdp2::Result<()> {
            let invalid_is_empty = {
                let gdi = context.gdi().ok_or(RdpError::Unsupported)?;
                let mut primary = gdi.primary().ok_or(RdpError::Unsupported)?;
                primary.hdc().hwnd().invalid().null()
            };
            if invalid_is_empty {
                return Ok(());
            }
            let frame = frame_from_gdi(context).map_err(RdpError::Failed)?;
            send_event(
                &context.handler.writer,
                RemoteDesktopHelperEvent::Frame { frame },
            )
            .map_err(RdpError::Failed)
        }

        fn desktop_resize(context: &mut Context<Self::ContextHandler>) -> freerdp2::Result<()> {
            let width = context.settings.desktop_width();
            let height = context.settings.desktop_height();
            let mut gdi = context.gdi().ok_or(RdpError::Unsupported)?;
            gdi.resize(width, height)
        }
    }

    #[derive(Clone, Copy, Default)]
    struct MousePositionCache {
        x: u16,
        y: u16,
    }

    impl MousePositionCache {
        fn update(&mut self, x: u32, y: u32) {
            self.x = clamp_u32_to_u16(x);
            self.y = clamp_u32_to_u16(y);
        }
    }
}

fn send_event(writer: &SharedEventWriter, event: RemoteDesktopHelperEvent) -> Result<(), String> {
    writer.send(event)
}

fn clamp_u32_to_u16(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

impl Drop for RdpWorkerConfig {
    fn drop(&mut self) {
        // The form-to-helper boundary converts the UI draft into
        // RemoteDesktopSecret. Clear the remaining username/domain drafts here
        // together with the secret wrapper when the worker config leaves scope.
        self.username.zeroize();
        if let Some(domain) = self.domain.as_mut() {
            domain.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::*;

    #[derive(Debug)]
    struct StaticConnectorSource(&'static str);

    impl fmt::Display for StaticConnectorSource {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.0)
        }
    }

    impl std::error::Error for StaticConnectorSource {}

    #[test]
    fn wheel_units_preserve_direction_and_minimum_notch() {
        assert_eq!(rdp_wheel_units(1.0), 120);
        assert_eq!(rdp_wheel_units(-1.0), -120);
        assert_eq!(rdp_wheel_units(240.0), 240);
    }

    #[test]
    fn wheel_delta_emits_horizontal_and_vertical_operations() {
        let operations = rdp_wheel_operations(RemoteDesktopWheelDelta { x: 1.0, y: -240.0 });

        assert_eq!(operations.len(), 2);
        match &operations[0] {
            RdpInputOperation::WheelRotations(rotations) => {
                assert!(!rotations.is_vertical);
                assert_eq!(rotations.rotation_units, 120);
            }
            operation => panic!("unexpected operation: {operation:?}"),
        }
        match &operations[1] {
            RdpInputOperation::WheelRotations(rotations) => {
                assert!(rotations.is_vertical);
                assert_eq!(rotations.rotation_units, -240);
            }
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn keyboard_mapping_prefers_scancode_for_navigation_keys() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "ArrowLeft".to_string(),
                text: None,
                alt: false,
                ctrl: false,
                shift: false,
                meta: false,
            },
            RemoteDesktopKeyState::Pressed,
        );

        assert_eq!(operations.len(), 1);
        match &operations[0] {
            RdpInputOperation::KeyPressed(scancode) => assert_eq!(scancode.as_u16(), 0xe04b),
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn keyboard_mapping_falls_back_to_unicode_text() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "Dead".to_string(),
                text: Some("é".to_string()),
                alt: false,
                ctrl: false,
                shift: false,
                meta: false,
            },
            RemoteDesktopKeyState::Released,
        );

        assert_eq!(operations.len(), 1);
        match &operations[0] {
            RdpInputOperation::UnicodeKeyReleased(character) => assert_eq!(*character, 'é'),
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn printable_key_uses_text_instead_of_us_scancode() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "a".to_string(),
                text: Some("A".to_string()),
                alt: false,
                ctrl: false,
                shift: true,
                meta: false,
            },
            RemoteDesktopKeyState::Pressed,
        );

        assert_eq!(operations.len(), 1);
        match &operations[0] {
            RdpInputOperation::UnicodeKeyPressed(character) => assert_eq!(*character, 'A'),
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn modified_shortcut_presses_modifier_before_key() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "v".to_string(),
                text: Some("v".to_string()),
                alt: false,
                ctrl: true,
                shift: false,
                meta: false,
            },
            RemoteDesktopKeyState::Pressed,
        );

        assert_eq!(operations.len(), 2);
        match &operations[0] {
            RdpInputOperation::KeyPressed(scancode) => assert_eq!(scancode.as_u16(), 0x1d),
            operation => panic!("unexpected operation: {operation:?}"),
        }
        match &operations[1] {
            RdpInputOperation::KeyPressed(scancode) => assert_eq!(scancode.as_u16(), 0x2f),
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn modified_shortcut_releases_key_before_modifier() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "v".to_string(),
                text: Some("v".to_string()),
                alt: false,
                ctrl: true,
                shift: false,
                meta: false,
            },
            RemoteDesktopKeyState::Released,
        );

        assert_eq!(operations.len(), 2);
        match &operations[0] {
            RdpInputOperation::KeyReleased(scancode) => assert_eq!(scancode.as_u16(), 0x2f),
            operation => panic!("unexpected operation: {operation:?}"),
        }
        match &operations[1] {
            RdpInputOperation::KeyReleased(scancode) => assert_eq!(scancode.as_u16(), 0x1d),
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn clipboard_formats_prefer_unicode_text() {
        let formats = text_clipboard_formats();

        assert_eq!(
            preferred_text_clipboard_format(&formats),
            Some(ClipboardFormatId::CF_UNICODETEXT)
        );
    }

    #[test]
    fn client_config_enables_modern_rdp_security_and_bitmap_output() {
        let config = RdpWorkerConfig {
            endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
            username: "alice".to_string(),
            password: RemoteDesktopSecret::from("secret"),
            domain: None,
            size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
            read_only: false,
        };

        let client_config = build_client_rdp_config(&config).unwrap();

        assert_eq!(client_config.destination.host(), "example.test");
        assert_eq!(client_config.destination.port(), 3389);
        assert!(client_config.connector.enable_tls);
        assert!(client_config.connector.enable_credssp);
        assert!(client_config.connector.autologon);
        assert!(client_config.connector.enable_server_pointer);
        assert!(!client_config.connector.pointer_software_rendering);
        assert_eq!(client_config.connector.desktop_size.width, 1280);
        assert_eq!(client_config.connector.desktop_size.height, 720);
        let bitmap = client_config.connector.bitmap.as_ref().unwrap();
        assert!(bitmap.lossy_compression);
        assert_eq!(bitmap.color_depth, 32);
    }

    #[test]
    fn dirty_rect_copy_extracts_only_region_and_sets_alpha() {
        let pixels = [
            [0, 1, 2, 0],
            [10, 11, 12, 0],
            [20, 21, 22, 0],
            [30, 31, 32, 0],
            [40, 41, 42, 0],
            [50, 51, 52, 0],
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let bytes = copy_image_rect(
            &pixels,
            3,
            RemoteDesktopRect {
                x: 1,
                y: 0,
                width: 2,
                height: 2,
            },
        );

        assert_eq!(
            bytes,
            vec![
                10, 11, 12, 0xff, 20, 21, 22, 0xff, 40, 41, 42, 0xff, 50, 51, 52, 0xff,
            ]
        );
    }

    #[test]
    fn full_frame_copy_sets_alpha_opaque() {
        let bytes = opaque_rgba_bytes(&[1, 2, 3, 0, 4, 5, 6, 7]);

        assert_eq!(bytes, vec![1, 2, 3, 0xff, 4, 5, 6, 0xff]);
    }

    #[test]
    fn standard_security_error_is_actionable_and_path_free() {
        let error = connector::ConnectorError::new(
            "Initiation",
            ConnectorErrorKind::Reason(
                "client advertised SSL | HYBRID | HYBRID_EX, but server selected STANDARD_RDP_SECURITY"
                    .to_string(),
            ),
        );

        let message = format_connector_error("RDP negotiation failed", &error);

        assert_eq!(message, LEGACY_RDP_SECURITY_MESSAGE);
        assert!(!message.contains("/Users/"));
        assert!(!message.contains(".cargo"));
    }

    #[test]
    fn custom_connector_error_includes_source_without_local_path() {
        let error = connector::ConnectorError::new("Initiation", ConnectorErrorKind::Custom)
            .with_source(StaticConnectorSource(
                "[license verification @ /Users/example/.cargo/git/checkouts/ironrdp/src/lib.rs:42] invalid server license",
            ));

        let message = format_connector_error("RDP negotiation failed", &error);

        assert_eq!(
            message,
            "RDP negotiation failed: [license verification] invalid server license"
        );
        assert!(!message.contains("/Users/"));
        assert!(!message.contains(".cargo"));
    }

    #[test]
    fn custom_standard_security_source_requests_legacy_fallback() {
        let error = connector::ConnectorError::new("Initiation", ConnectorErrorKind::Custom)
            .with_source(StaticConnectorSource(
                "[Initiation @ /Users/example/.cargo/git/checkouts/ironrdp/src/lib.rs:409] client advertised SSL | HYBRID | HYBRID_EX, but server selected STANDARD_RDP_SECURITY",
            ));

        let message = format_connector_error("RDP negotiation failed", &error);

        assert!(connector_error_requires_legacy_security(&error));
        assert_eq!(message, LEGACY_RDP_SECURITY_MESSAGE);
        assert!(!message.contains("/Users/"));
        assert!(!message.contains(".cargo"));
    }

    #[test]
    fn standard_security_error_requests_legacy_fallback() {
        let error = connector::ConnectorError::new(
            "Initiation",
            ConnectorErrorKind::Reason(
                "client advertised SSL | HYBRID | HYBRID_EX, but server selected STANDARD_RDP_SECURITY"
                    .to_string(),
            ),
        );

        let message = format_connector_error("RDP negotiation failed", &error);

        assert!(connector_error_requires_legacy_security(&error));
        assert_eq!(message, LEGACY_RDP_SECURITY_MESSAGE);
    }

    #[cfg(not(feature = "legacy-freerdp"))]
    #[test]
    fn legacy_fallback_without_freerdp_feature_returns_guidance() {
        let (_request_tx, request_rx) = mpsc::channel();
        let config = RdpWorkerConfig {
            endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
            username: "alice".to_string(),
            password: RemoteDesktopSecret::from("secret"),
            domain: None,
            size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
            read_only: false,
        };
        let writer = SharedEventWriter {
            stdout: Arc::new(Mutex::new(io::stdout())),
            queue: Arc::new((Mutex::new(EventWriterQueue::default()), Condvar::new())),
        };

        let error = run_legacy_rdp_worker(config, writer, request_rx).unwrap_err();

        assert_eq!(error, LEGACY_RDP_ENGINE_UNAVAILABLE_MESSAGE);
    }
}
