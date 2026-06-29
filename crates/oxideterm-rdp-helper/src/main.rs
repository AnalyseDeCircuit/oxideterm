// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fmt,
    io::{self, BufRead, BufReader},
    sync::mpsc,
    thread,
    time::Duration,
};

use ironrdp::{
    cliprdr::{
        CliprdrClient,
        backend::{ClipboardMessage, CliprdrBackend},
        pdu::{
            ClipboardFormat, ClipboardFormatId, ClipboardFormatName,
            ClipboardGeneralCapabilityFlags, FileContentsRequest, FileContentsResponse,
            FormatDataRequest, FormatDataResponse, LockDataId,
        },
    },
    connector::connection_activation::ConnectionActivationState,
    connector::{self, ConnectorErrorKind, Credentials},
    connector::{ConnectionResult, DesktopSize},
    displaycontrol::client::DisplayControlClient,
    dvc::DrdynvcClient,
    graphics::image_processing::PixelFormat,
    input::{
        Database as RdpInputDatabase, MousePosition, Operation as RdpInputOperation,
        synchronize_event as rdp_synchronize_event,
    },
    pdu::{
        gcc::KeyboardType,
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
use ironrdp_cliprdr_format::bitmap::{dib_to_png, dibv5_to_png, png_to_cf_dib, png_to_cf_dibv5};
use ironrdp_core::{IntoOwned as _, WriteBuf, impl_as_any};
use ironrdp_displaycontrol::pdu::MonitorLayoutEntry;
use ironrdp_tokio::{FramedWrite, single_sequence_step_read, split_tokio_framed};
use oxideterm_remote_desktop::{
    RemoteDesktopClipboardData, RemoteDesktopClipboardFormat, RemoteDesktopCursorShape,
    RemoteDesktopEndpoint, RemoteDesktopErrorCategory, RemoteDesktopFakeBackend,
    RemoteDesktopFrameFormat, RemoteDesktopHelperEvent, RemoteDesktopHelperRequest,
    RemoteDesktopLockKeys, RemoteDesktopMouseButtonState, RemoteDesktopProtocol,
    RemoteDesktopSecret, RemoteDesktopSessionStatus, RemoteDesktopSize, read_request_line,
    run_fake_backend_stdio,
};
use smallvec::SmallVec;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use zeroize::Zeroize;

mod event_writer;
mod frame;
mod input;

use event_writer::{SharedEventWriter, send_event};
use frame::*;
use input::*;

const RDP_CLIENT_NAME: &str = "OxideTerm";
const RDP_CLIENT_LOOP_POLL_INTERVAL: Duration = Duration::from_millis(8);
const RDP_CLIENT_REQUEST_DRAIN_LIMIT: usize = 128;
const RDP_CLIENT_OUTPUT_DRAIN_LIMIT: usize = 32;
const RDP_CLIENT_OUTPUT_QUEUE_CAPACITY: usize = 64;
const RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT: u32 = 0;
const RDP_DISPLAYCONTROL_DEFAULT_SCALE_FACTOR_PERCENT: u32 = 100;
const RDP_MIN_SCALE_FACTOR_PERCENT: u32 = 100;
const RDP_MAX_SCALE_FACTOR_PERCENT: u32 = 500;
const RDP_CLIPBOARD_TIMEOUT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const RDP_CLIPBOARD_TEMPORARY_DIRECTORY: &str = ".cliprdr";
const RDP_CLIPBOARD_FORMAT_IMAGE_PNG: ClipboardFormatId = ClipboardFormatId(0xc001);
const RDP_CLIPBOARD_FORMAT_IMAGE_JPEG: ClipboardFormatId = ClipboardFormatId(0xc002);
const RDP_CLIPBOARD_FORMAT_IMAGE_WEBP: ClipboardFormatId = ClipboardFormatId(0xc003);
const RDP_CLIPBOARD_FORMAT_IMAGE_GIF: ClipboardFormatId = ClipboardFormatId(0xc004);
const RDP_CLIPBOARD_FORMAT_IMAGE_SVG: ClipboardFormatId = ClipboardFormatId(0xc005);
const RDP_CLIPBOARD_FORMAT_IMAGE_BMP: ClipboardFormatId = ClipboardFormatId(0xc006);
const RDP_CLIPBOARD_FORMAT_IMAGE_TIFF: ClipboardFormatId = ClipboardFormatId(0xc007);
const LEGACY_RDP_SECURITY_MESSAGE: &str =
    "该服务器只支持旧版 RDP 安全模式，需要启用 legacy RDP 支持";
const LEGACY_RDP_ENGINE_UNAVAILABLE_MESSAGE: &str =
    "该服务器只支持旧版 RDP 安全模式，但当前 helper 构建未包含 FreeRDP legacy 引擎";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RdpClipboardDataFormat {
    id: ClipboardFormatId,
    format: RemoteDesktopClipboardFormat,
    encoding: RdpClipboardDataEncoding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RdpClipboardDataEncoding {
    Encoded,
    Dib,
    DibV5,
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
        scale_factor,
        read_only,
    } = first_request
    else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP helper expected an initial connect request.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Configuration),
            },
        )?;
        return Ok(());
    };

    if protocol != RemoteDesktopProtocol::Rdp {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP helper received a non-RDP connect request.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Configuration),
            },
        )?;
        return Ok(());
    }

    let Some(username) = username.filter(|username| !username.trim().is_empty()) else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP username is required.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Configuration),
            },
        )?;
        return Ok(());
    };
    let Some(password) = password else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP password is required.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Configuration),
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
            scale_factor: rdp_connector_scale_factor(scale_factor),
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
    scale_factor: u32,
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
                    RemoteDesktopHelperEvent::ConnectionFailure {
                        category: Some(remote_desktop_error_category_from_message(&error)),
                        message: error,
                    },
                );
            }
        })
        .expect("failed to start RDP helper worker")
}

fn run_rdp_worker(
    mut config: RdpWorkerConfig,
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
        let read_only = config.read_only;
        let exit = run_client_rdp_loop(
            &writer,
            &request_rx,
            &client.input_tx,
            client.output_rx,
            &mut config,
            read_only,
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
            ClientRdpSessionExit::RemoteEnded(reason) => {
                return send_event(
                    &writer,
                    RemoteDesktopHelperEvent::Disconnected {
                        reason: sanitize_rdp_disconnect_reason(reason.as_deref()),
                    },
                );
            }
            ClientRdpSessionExit::ConnectionFailed { message, category } => {
                return send_event(
                    &writer,
                    RemoteDesktopHelperEvent::ConnectionFailure {
                        message,
                        category: Some(category),
                    },
                );
            }
            ClientRdpSessionExit::LegacySecurityRequired => {
                return run_legacy_rdp_worker(config, writer, request_rx);
            }
        }
    }
}

#[derive(Debug)]
enum ClientRdpSessionExit {
    Closed,
    ReconnectRequested,
    RemoteEnded(Option<String>),
    ConnectionFailed {
        message: String,
        category: RemoteDesktopErrorCategory,
    },
    LegacySecurityRequired,
}

struct ClientRdpSession {
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_rx: mpsc::Receiver<ClientRdpOutput>,
    join_handle: thread::JoinHandle<()>,
}

type ClientRdpOutputTx = mpsc::SyncSender<ClientRdpOutput>;

#[derive(Default)]
struct ClientRdpRequestCoalescer {
    pending_mouse_move: Option<RemoteDesktopHelperRequest>,
}

impl ClientRdpRequestCoalescer {
    fn push(
        &mut self,
        request: RemoteDesktopHelperRequest,
        output: &mut Vec<RemoteDesktopHelperRequest>,
    ) {
        match request {
            RemoteDesktopHelperRequest::MouseMove { .. } => {
                // Pointer motion can be much more frequent than the helper
                // polling loop. Keep only the newest position so button and
                // keyboard input is not delayed behind stale cursor samples.
                self.pending_mouse_move = Some(request);
            }
            request => {
                self.flush(output);
                output.push(request);
            }
        }
    }

    fn flush(&mut self, output: &mut Vec<RemoteDesktopHelperRequest>) {
        if let Some(request) = self.pending_mouse_move.take() {
            output.push(request);
        }
    }
}

#[derive(Debug)]
enum ClientRdpOutput {
    Event(RemoteDesktopHelperEvent),
    ConnectionFailure(connector::ConnectorError),
    Terminated(String),
    OutputEnded,
}

#[derive(Debug, Default)]
struct ClientRdpOutputDrain {
    drained: usize,
    exit: Option<ClientRdpSessionExit>,
}

fn start_client_rdp_session(config: &RdpWorkerConfig) -> Result<ClientRdpSession, String> {
    let client_config = build_client_rdp_config(config)?;
    let (input_tx, input_rx) = tokio_mpsc::unbounded_channel();
    let client_input_tx = input_tx.clone();
    let (client_output_tx, client_output_rx) =
        mpsc::sync_channel::<ClientRdpOutput>(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);

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
    client_output_tx: ClientRdpOutputTx,
) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        let _ = client_output_tx.send(ClientRdpOutput::Event(
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "RDP async runtime startup failed.".to_string(),
                category: Some(RemoteDesktopErrorCategory::Dependency),
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
                Ok(ClientRdpControlFlow::ReconnectWithNewSize {
                    width,
                    height,
                    scale_factor,
                }) => {
                    config.connector.desktop_size = DesktopSize { width, height };
                    config.connector.desktop_scale_factor = scale_factor;
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
    output_tx: ClientRdpOutputTx,
    local_text: Option<String>,
    local_data: Option<RemoteDesktopClipboardData>,
    remote_text_format: Option<ClipboardFormatId>,
    remote_data_format: Option<RdpClipboardDataFormat>,
}

impl fmt::Debug for ClientClipboardBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientClipboardBackend")
            .field("has_local_text", &self.local_text.is_some())
            .field("has_local_data", &self.local_data.is_some())
            .field("remote_text_format", &self.remote_text_format)
            .field("remote_data_format", &self.remote_data_format)
            .finish()
    }
}

impl_as_any!(ClientClipboardBackend);

impl ClientClipboardBackend {
    fn new(
        input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
        output_tx: ClientRdpOutputTx,
    ) -> Self {
        Self {
            input_tx,
            output_tx,
            local_text: None,
            local_data: None,
            remote_text_format: None,
            remote_data_format: None,
        }
    }

    fn set_local_text(&mut self, text: String) {
        self.local_text = Some(text);
        self.local_data = None;
    }

    fn set_local_data(&mut self, data: RemoteDesktopClipboardData) {
        self.local_text = None;
        self.local_data = Some(data);
    }

    fn send_clipboard_message(&self, message: ClipboardMessage) {
        let _ = self.input_tx.send(RdpInputEvent::Clipboard(message));
    }

    fn send_local_format_list(&self) {
        let formats = if let Some(data) = self.local_data.as_ref() {
            image_clipboard_formats(data.format)
        } else if self.local_text.is_some() {
            text_clipboard_formats()
        } else {
            Vec::new()
        };
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

    fn on_ready(&mut self) {
        // CLIPRDR may become ready after the UI has already supplied local
        // clipboard text. Advertise the cached formats once the channel is
        // usable so the server can request that text immediately.
        self.send_local_format_list();
    }

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
        self.remote_text_format = None;
        self.remote_data_format = None;

        if let Some(format) = preferred_image_clipboard_format(available_formats) {
            self.remote_data_format = Some(format);
            self.send_clipboard_message(ClipboardMessage::SendInitiatePaste(format.id));
            return;
        }

        if let Some(format) = preferred_text_clipboard_format(available_formats) {
            self.remote_text_format = Some(format);
            self.send_clipboard_message(ClipboardMessage::SendInitiatePaste(format));
        }
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let response = if let Some(data) = self.local_data.as_ref() {
            if let Some(bytes) = encode_local_clipboard_data(data, request.format) {
                FormatDataResponse::new_data(bytes).into_owned()
            } else {
                FormatDataResponse::new_error().into_owned()
            }
        } else {
            match (request.format, self.local_text.as_deref()) {
                (ClipboardFormatId::CF_UNICODETEXT, Some(text)) => {
                    FormatDataResponse::new_unicode_string(text).into_owned()
                }
                (ClipboardFormatId::CF_TEXT, Some(text)) => {
                    FormatDataResponse::new_string(text).into_owned()
                }
                _ => FormatDataResponse::new_error().into_owned(),
            }
        };
        self.send_clipboard_message(ClipboardMessage::SendFormatData(response));
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }

        if let Some(format) = self.remote_data_format.take() {
            let data = decode_remote_clipboard_data(format, response.data().to_vec());
            if let Some(data) = data {
                let _ = self.output_tx.send(ClientRdpOutput::Event(
                    RemoteDesktopHelperEvent::ClipboardData { data },
                ));
            }
            return;
        }

        let text = match self.remote_text_format.take() {
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
    SetClipboardData(RemoteDesktopClipboardData),
    Close,
}

enum ClientRdpControlFlow {
    TerminatedGracefully(GracefulDisconnectReason),
    ReconnectWithNewSize {
        width: u16,
        height: u16,
        scale_factor: u32,
    },
}

async fn connect_native_rdp(
    config: &ClientRdpConfig,
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_tx: ClientRdpOutputTx,
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
    output_tx: ClientRdpOutputTx,
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
    output_tx: &ClientRdpOutputTx,
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
    let mut pending_base_frame = false;

    let disconnect_reason = 'session: loop {
        flush_pending_rdp_base_frame(output_tx, &image, &mut pending_base_frame)?;

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
                        if let Some(response_frame) =
                            active_stage.encode_resize(
                                u32::from(width),
                                u32::from(height),
                                Some(scale_factor),
                                physical_size,
                            )
                        {
                            vec![ActiveStageOutput::ResponseFrame(response_frame?)]
                        } else {
                            return Ok(ClientRdpControlFlow::ReconnectWithNewSize {
                                width,
                                height,
                                scale_factor,
                            });
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
                    RdpInputEvent::SetClipboardData(data) => {
                        advertise_local_clipboard_data(&mut active_stage, data)?
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
                    if let Some(event) =
                        graphics_update_event(&image, region, &mut sent_initial_frame)?
                    {
                        send_client_rdp_graphics_event(output_tx, event, &mut pending_base_frame)?;
                    }
                }
                ActiveStageOutput::PointerPosition { x, y } => {
                    send_client_rdp_event(
                        output_tx,
                        RemoteDesktopHelperEvent::Cursor {
                            x: u32::from(x),
                            y: u32::from(y),
                            width: 0,
                            height: 0,
                        },
                    )?;
                }
                ActiveStageOutput::PointerDefault => {
                    send_client_rdp_event(output_tx, RemoteDesktopHelperEvent::CursorDefault)?;
                }
                ActiveStageOutput::PointerHidden => {
                    send_client_rdp_event(output_tx, RemoteDesktopHelperEvent::CursorHidden)?;
                }
                ActiveStageOutput::PointerBitmap(pointer) => {
                    send_client_rdp_event(
                        output_tx,
                        RemoteDesktopHelperEvent::CursorShape {
                            shape: RemoteDesktopCursorShape::new(
                                RemoteDesktopSize {
                                    width: u32::from(pointer.width),
                                    height: u32::from(pointer.height),
                                },
                                u32::from(pointer.hotspot_x),
                                u32::from(pointer.hotspot_y),
                                RemoteDesktopFrameFormat::Rgba8,
                                pointer.bitmap_data.clone(),
                            ),
                        },
                    )?;
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
                    // Reactivation resets the server-side backing surface. Give
                    // the UI a fresh full-frame base before accepting new dirty
                    // rectangles so updates are not applied to the old desktop.
                    send_client_rdp_graphics_event(
                        output_tx,
                        base_frame_event(&image),
                        &mut pending_base_frame,
                    )?;
                    sent_initial_frame = true;
                }
                ActiveStageOutput::Terminate(reason) => break 'session reason,
                ActiveStageOutput::MultitransportRequest(_) | ActiveStageOutput::AutoDetect(_) => {}
            }
        }
    };

    Ok(ClientRdpControlFlow::TerminatedGracefully(
        disconnect_reason,
    ))
}

fn flush_pending_rdp_base_frame(
    output_tx: &ClientRdpOutputTx,
    image: &DecodedImage,
    pending_base_frame: &mut bool,
) -> SessionResult<()> {
    if !*pending_base_frame {
        return Ok(());
    }

    match output_tx.try_send(ClientRdpOutput::Event(base_frame_event(image))) {
        Ok(()) => {
            *pending_base_frame = false;
            Ok(())
        }
        Err(mpsc::TrySendError::Full(_)) => Ok(()),
        Err(mpsc::TrySendError::Disconnected(_)) => {
            Err(session::general_err!("RDP output channel closed"))
        }
    }
}

fn send_client_rdp_graphics_event(
    output_tx: &ClientRdpOutputTx,
    event: RemoteDesktopHelperEvent,
    pending_base_frame: &mut bool,
) -> SessionResult<()> {
    if *pending_base_frame {
        return Ok(());
    }

    match output_tx.try_send(ClientRdpOutput::Event(event)) {
        Ok(()) => Ok(()),
        Err(mpsc::TrySendError::Full(_)) => {
            // Dirty rectangles are relative to the UI's backing frame. If the
            // bridge is saturated, recover by sending the latest complete image
            // once capacity returns instead of silently dropping a delta.
            *pending_base_frame = true;
            Ok(())
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            Err(session::general_err!("RDP output channel closed"))
        }
    }
}

fn send_client_rdp_event(
    output_tx: &ClientRdpOutputTx,
    event: RemoteDesktopHelperEvent,
) -> SessionResult<()> {
    if client_rdp_event_can_be_dropped_under_backpressure(&event) {
        match output_tx.try_send(ClientRdpOutput::Event(event)) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => return Ok(()),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(session::general_err!("RDP output channel closed"));
            }
        }
    }

    // Base frames and control-like visual events must not be dropped because
    // the UI relies on them to establish backing state and cursor shape.
    output_tx
        .send(ClientRdpOutput::Event(event))
        .map_err(|error| session::custom_err!("send RDP client event", error))
}

fn client_rdp_event_can_be_dropped_under_backpressure(event: &RemoteDesktopHelperEvent) -> bool {
    matches!(event, RemoteDesktopHelperEvent::Cursor { .. })
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

fn advertise_local_clipboard_data(
    active_stage: &mut ActiveStage,
    data: RemoteDesktopClipboardData,
) -> SessionResult<Vec<ActiveStageOutput>> {
    let Some(cliprdr) = active_stage.get_svc_processor_mut::<CliprdrClient>() else {
        return Ok(Vec::new());
    };
    let formats = image_clipboard_formats(data.format);
    if let Some(backend) = cliprdr.downcast_backend_mut::<ClientClipboardBackend>() {
        backend.set_local_data(data);
    }

    // If CLIPRDR is not fully ready yet, the backend keeps the data and the
    // initialization callback will advertise it later.
    let Ok(svc_messages) = cliprdr.initiate_copy(&formats) else {
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
    config: &mut RdpWorkerConfig,
    read_only: bool,
) -> Result<ClientRdpSessionExit, String> {
    let mut input_database = RdpInputDatabase::new();
    let mut keyboard_mapper = RdpKeyboardInputMapper::default();
    loop {
        let mut handled_requests = false;
        let mut coalesced_requests = Vec::new();
        let mut request_coalescer = ClientRdpRequestCoalescer::default();
        // Bound request draining so display output still advances during input bursts.
        for _ in 0..RDP_CLIENT_REQUEST_DRAIN_LIMIT {
            match request_rx.try_recv() {
                Ok(RemoteDesktopHelperRequest::Close) => return Ok(ClientRdpSessionExit::Closed),
                Ok(RemoteDesktopHelperRequest::Reconnect) => {
                    return Ok(ClientRdpSessionExit::ReconnectRequested);
                }
                Ok(request) => {
                    handled_requests = true;
                    request_coalescer.push(request, &mut coalesced_requests);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(ClientRdpSessionExit::Closed),
            }
        }
        request_coalescer.flush(&mut coalesced_requests);
        for request in coalesced_requests {
            remember_rdp_reconnect_state(&request, config);
            forward_client_rdp_request(
                input_tx,
                &mut input_database,
                &mut keyboard_mapper,
                request,
                read_only,
            )?;
        }

        // User input should not sit behind a burst of frame events. Drain the
        // helper output after forwarding queued requests so high-update
        // desktops cannot add avoidable keyboard and pointer latency.
        let output_drain = drain_client_rdp_outputs(writer, &output_rx)?;
        if let Some(exit) = output_drain.exit {
            return Ok(exit);
        }

        if output_drain.drained < RDP_CLIENT_OUTPUT_DRAIN_LIMIT && !handled_requests {
            thread::sleep(RDP_CLIENT_LOOP_POLL_INTERVAL);
        }
    }
}

fn drain_client_rdp_outputs(
    writer: &SharedEventWriter,
    output_rx: &mpsc::Receiver<ClientRdpOutput>,
) -> Result<ClientRdpOutputDrain, String> {
    let mut drain = ClientRdpOutputDrain::default();
    while drain.drained < RDP_CLIENT_OUTPUT_DRAIN_LIMIT {
        match output_rx.try_recv() {
            Ok(output) => {
                drain.drained += 1;
                match output {
                    ClientRdpOutput::Event(event) => send_event(writer, event)?,
                    ClientRdpOutput::ConnectionFailure(error) => {
                        if connector_error_requires_legacy_security(&error) {
                            drain.exit = Some(ClientRdpSessionExit::LegacySecurityRequired);
                            return Ok(drain);
                        }
                        // Keep the typed connector error available until the
                        // helper event is built; string messages are only the
                        // display surface, not the classification source.
                        drain.exit = Some(ClientRdpSessionExit::ConnectionFailed {
                            message: format_connector_error("RDP connection failed", &error),
                            category: connector_error_category(&error),
                        });
                        return Ok(drain);
                    }
                    ClientRdpOutput::Terminated(message) => {
                        drain.exit = Some(ClientRdpSessionExit::RemoteEnded(Some(message)));
                        return Ok(drain);
                    }
                    ClientRdpOutput::OutputEnded => {
                        drain.exit = Some(ClientRdpSessionExit::RemoteEnded(None));
                        return Ok(drain);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => return Ok(drain),
            Err(mpsc::TryRecvError::Disconnected) => {
                drain.exit = Some(ClientRdpSessionExit::RemoteEnded(None));
                return Ok(drain);
            }
        }
    }
    Ok(drain)
}

fn remember_rdp_reconnect_state(
    request: &RemoteDesktopHelperRequest,
    config: &mut RdpWorkerConfig,
) {
    if let RemoteDesktopHelperRequest::Resize { size, scale_factor } = request {
        // Reconnects rebuild the IronRDP connector from RdpWorkerConfig, so the
        // last requested display size must live there instead of only in the
        // active client thread.
        config.size = normalized_rdp_desktop_size(*size);
        config.scale_factor = rdp_connector_scale_factor(*scale_factor);
    }
}

fn normalized_rdp_desktop_size(size: RemoteDesktopSize) -> RemoteDesktopSize {
    let size = RemoteDesktopSize::clamped(size.width, size.height);
    let (width, height) = MonitorLayoutEntry::adjust_display_size(size.width, size.height);
    RemoteDesktopSize { width, height }
}

fn rdp_connector_scale_factor(scale_factor: Option<u32>) -> u32 {
    rdp_valid_scale_factor(scale_factor).unwrap_or(RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT)
}

fn rdp_displaycontrol_scale_factor(scale_factor: Option<u32>) -> u32 {
    rdp_valid_scale_factor(scale_factor).unwrap_or(RDP_DISPLAYCONTROL_DEFAULT_SCALE_FACTOR_PERCENT)
}

fn rdp_valid_scale_factor(scale_factor: Option<u32>) -> Option<u32> {
    match scale_factor {
        Some(scale_factor)
            if (RDP_MIN_SCALE_FACTOR_PERCENT..=RDP_MAX_SCALE_FACTOR_PERCENT)
                .contains(&scale_factor) =>
        {
            Some(scale_factor)
        }
        _ => None,
    }
}

fn sanitize_rdp_disconnect_reason(reason: Option<&str>) -> Option<String> {
    let reason = reason?.trim();
    if reason.is_empty() || reason.contains("/Users/") || reason.contains(".cargo/git/checkouts") {
        return Some("RDP session ended.".to_string());
    }
    Some(format!("RDP session ended: {reason}."))
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

fn build_client_rdp_config(config: &RdpWorkerConfig) -> Result<ClientRdpConfig, String> {
    let requested_size = normalized_rdp_desktop_size(config.size);
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
        desktop_scale_factor: config.scale_factor,
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
    keyboard_mapper: &mut RdpKeyboardInputMapper,
    request: RemoteDesktopHelperRequest,
    read_only: bool,
) -> Result<(), String> {
    match request {
        RemoteDesktopHelperRequest::Resize { size, scale_factor } => {
            let requested_size = normalized_rdp_desktop_size(size);
            input_tx
                .send(RdpInputEvent::Resize {
                    width: clamp_u32_to_u16(requested_size.width),
                    height: clamp_u32_to_u16(requested_size.height),
                    scale_factor: rdp_displaycontrol_scale_factor(scale_factor),
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
                keyboard_mapper.operations(&key, state),
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
        RemoteDesktopHelperRequest::ClipboardData { data } if !read_only => {
            input_tx
                .send(RdpInputEvent::SetClipboardData(data))
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::SynchronizeLockKeys { keys } if !read_only => {
            send_client_rdp_lock_key_state(input_tx, keys)?;
        }
        RemoteDesktopHelperRequest::ReleaseAllInputs if !read_only => {
            // Release mapper-owned Unicode and synthetic modifier state before
            // asking IronRDP's database to release the protocol-owned state.
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                keyboard_mapper.release_all_operations(),
            )?;
            let events = input_database.release_all();
            if !events.is_empty() {
                input_tx
                    .send(RdpInputEvent::FastPath(events))
                    .map_err(|_| "RDP input channel is closed.".to_string())?;
            }
        }
        RemoteDesktopHelperRequest::Connect { .. }
        | RemoteDesktopHelperRequest::Close
        | RemoteDesktopHelperRequest::Reconnect
        | RemoteDesktopHelperRequest::MouseMove { .. }
        | RemoteDesktopHelperRequest::MouseButton { .. }
        | RemoteDesktopHelperRequest::Wheel { .. }
        | RemoteDesktopHelperRequest::Key { .. }
        | RemoteDesktopHelperRequest::Text { .. }
        | RemoteDesktopHelperRequest::ClipboardText { .. }
        | RemoteDesktopHelperRequest::ClipboardData { .. }
        | RemoteDesktopHelperRequest::SynchronizeLockKeys { .. }
        | RemoteDesktopHelperRequest::ReleaseAllInputs => {}
    }
    Ok(())
}

fn send_client_rdp_lock_key_state(
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    keys: RemoteDesktopLockKeys,
) -> Result<(), String> {
    let mut events = SmallVec::new();
    // IronRDP owns the exact fast-path synchronize flag mapping. Keep this
    // helper as a transport boundary instead of duplicating the protocol bits.
    events.push(rdp_synchronize_event(
        keys.scroll_lock,
        keys.num_lock,
        keys.caps_lock,
        keys.kana_lock,
    ));
    input_tx
        .send(RdpInputEvent::FastPath(events))
        .map_err(|_| "RDP input channel is closed.".to_string())
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

fn connector_error_category(error: &connector::ConnectorError) -> RemoteDesktopErrorCategory {
    match error.kind() {
        _ if connector_error_requires_legacy_security(error) => {
            RemoteDesktopErrorCategory::LegacySecurity
        }
        ConnectorErrorKind::Credssp(_) | ConnectorErrorKind::AccessDenied => {
            RemoteDesktopErrorCategory::Authentication
        }
        ConnectorErrorKind::Negotiation(_)
        | ConnectorErrorKind::Encode(_)
        | ConnectorErrorKind::Decode(_)
        | ConnectorErrorKind::Reason(_) => RemoteDesktopErrorCategory::Protocol,
        ConnectorErrorKind::Custom => RemoteDesktopErrorCategory::Unknown,
        ConnectorErrorKind::General => RemoteDesktopErrorCategory::Unknown,
        _ => RemoteDesktopErrorCategory::Unknown,
    }
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

fn remote_desktop_error_category_from_message(message: &str) -> RemoteDesktopErrorCategory {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("standard_rdp_security") || message.contains(LEGACY_RDP_SECURITY_MESSAGE)
    {
        RemoteDesktopErrorCategory::LegacySecurity
    } else if normalized.contains("authentication")
        || normalized.contains("access denied")
        || normalized.contains("password")
        || normalized.contains("credssp")
    {
        RemoteDesktopErrorCategory::Authentication
    } else if normalized.contains("tcp")
        || normalized.contains("socket")
        || normalized.contains("network")
        || normalized.contains("transport")
    {
        RemoteDesktopErrorCategory::Network
    } else if normalized.contains("protocol")
        || normalized.contains("decode")
        || normalized.contains("encode")
        || normalized.contains("negotiation")
    {
        RemoteDesktopErrorCategory::Protocol
    } else if normalized.contains("unavailable")
        || normalized.contains("not available")
        || normalized.contains("runtime startup")
    {
        RemoteDesktopErrorCategory::Dependency
    } else {
        RemoteDesktopErrorCategory::Unknown
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

fn text_clipboard_formats() -> Vec<ClipboardFormat> {
    vec![
        ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
        ClipboardFormat::new(ClipboardFormatId::CF_TEXT),
    ]
}

fn image_clipboard_formats(format: RemoteDesktopClipboardFormat) -> Vec<ClipboardFormat> {
    let (id, name) = local_image_clipboard_format(format);
    let mut formats = vec![ClipboardFormat::new(id).with_name(ClipboardFormatName::new(name))];
    if format == RemoteDesktopClipboardFormat::ImagePng {
        // Windows peers commonly request bitmap clipboard data even when a PNG
        // registered format is also available.
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_DIBV5));
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_DIB));
    }
    if format == RemoteDesktopClipboardFormat::ImageTiff {
        // TIFF is one of the standard Win32 clipboard formats. Advertise it
        // alongside the registered MIME name so older peers can request it.
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_TIFF));
    }
    formats
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

fn preferred_image_clipboard_format(formats: &[ClipboardFormat]) -> Option<RdpClipboardDataFormat> {
    formats
        .iter()
        .find_map(rdp_clipboard_data_format_from_named_format)
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_DIBV5)
                .map(|format| RdpClipboardDataFormat {
                    id: format.id,
                    format: RemoteDesktopClipboardFormat::ImagePng,
                    encoding: RdpClipboardDataEncoding::DibV5,
                })
        })
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_DIB)
                .map(|format| RdpClipboardDataFormat {
                    id: format.id,
                    format: RemoteDesktopClipboardFormat::ImagePng,
                    encoding: RdpClipboardDataEncoding::Dib,
                })
        })
        .or_else(|| {
            formats
                .iter()
                .find(|format| format.id == ClipboardFormatId::CF_TIFF)
                .map(|format| RdpClipboardDataFormat {
                    id: format.id,
                    format: RemoteDesktopClipboardFormat::ImageTiff,
                    encoding: RdpClipboardDataEncoding::Encoded,
                })
        })
}

fn decode_remote_clipboard_data(
    format: RdpClipboardDataFormat,
    bytes: Vec<u8>,
) -> Option<RemoteDesktopClipboardData> {
    if bytes.is_empty() {
        return None;
    }
    let bytes = match format.encoding {
        RdpClipboardDataEncoding::Encoded => bytes,
        RdpClipboardDataEncoding::Dib => dib_to_png(&bytes).ok()?,
        RdpClipboardDataEncoding::DibV5 => dibv5_to_png(&bytes).ok()?,
    };
    Some(RemoteDesktopClipboardData::new(format.format, bytes))
}

fn local_image_clipboard_format(
    format: RemoteDesktopClipboardFormat,
) -> (ClipboardFormatId, &'static str) {
    match format {
        RemoteDesktopClipboardFormat::ImagePng => (RDP_CLIPBOARD_FORMAT_IMAGE_PNG, "PNG"),
        RemoteDesktopClipboardFormat::ImageJpeg => (RDP_CLIPBOARD_FORMAT_IMAGE_JPEG, "JFIF"),
        RemoteDesktopClipboardFormat::ImageWebp => (RDP_CLIPBOARD_FORMAT_IMAGE_WEBP, "image/webp"),
        RemoteDesktopClipboardFormat::ImageGif => (RDP_CLIPBOARD_FORMAT_IMAGE_GIF, "GIF"),
        RemoteDesktopClipboardFormat::ImageSvg => (RDP_CLIPBOARD_FORMAT_IMAGE_SVG, "image/svg+xml"),
        RemoteDesktopClipboardFormat::ImageBmp => (RDP_CLIPBOARD_FORMAT_IMAGE_BMP, "image/bmp"),
        RemoteDesktopClipboardFormat::ImageTiff => (RDP_CLIPBOARD_FORMAT_IMAGE_TIFF, "image/tiff"),
    }
}

fn local_image_clipboard_format_ids(
    format: RemoteDesktopClipboardFormat,
) -> Vec<ClipboardFormatId> {
    let (id, _) = local_image_clipboard_format(format);
    let mut ids = vec![id];
    if format == RemoteDesktopClipboardFormat::ImagePng {
        ids.push(ClipboardFormatId::CF_DIBV5);
        ids.push(ClipboardFormatId::CF_DIB);
    }
    if format == RemoteDesktopClipboardFormat::ImageTiff {
        ids.push(ClipboardFormatId::CF_TIFF);
    }
    ids
}

fn encode_local_clipboard_data(
    data: &RemoteDesktopClipboardData,
    format: ClipboardFormatId,
) -> Option<Vec<u8>> {
    if !local_image_clipboard_format_ids(data.format).contains(&format) {
        return None;
    }
    match (data.format, format) {
        (RemoteDesktopClipboardFormat::ImagePng, ClipboardFormatId::CF_DIB) => {
            png_to_cf_dib(&data.bytes).ok()
        }
        (RemoteDesktopClipboardFormat::ImagePng, ClipboardFormatId::CF_DIBV5) => {
            png_to_cf_dibv5(&data.bytes).ok()
        }
        _ => Some(data.bytes.clone()),
    }
}

fn rdp_clipboard_data_format_from_named_format(
    format: &ClipboardFormat,
) -> Option<RdpClipboardDataFormat> {
    let name = format.name()?.value();
    let clipboard_format = remote_desktop_clipboard_format_from_rdp_name(name)?;
    Some(RdpClipboardDataFormat {
        id: format.id,
        format: clipboard_format,
        encoding: RdpClipboardDataEncoding::Encoded,
    })
}

fn remote_desktop_clipboard_format_from_rdp_name(
    name: &str,
) -> Option<RemoteDesktopClipboardFormat> {
    if name.eq_ignore_ascii_case("PNG") || name.eq_ignore_ascii_case("image/png") {
        return Some(RemoteDesktopClipboardFormat::ImagePng);
    }
    if name.eq_ignore_ascii_case("JFIF")
        || name.eq_ignore_ascii_case("JPEG")
        || name.eq_ignore_ascii_case("JPG")
        || name.eq_ignore_ascii_case("image/jpeg")
        || name.eq_ignore_ascii_case("image/jpg")
    {
        return Some(RemoteDesktopClipboardFormat::ImageJpeg);
    }
    if name.eq_ignore_ascii_case("image/webp") {
        return Some(RemoteDesktopClipboardFormat::ImageWebp);
    }
    if name.eq_ignore_ascii_case("GIF") || name.eq_ignore_ascii_case("image/gif") {
        return Some(RemoteDesktopClipboardFormat::ImageGif);
    }
    if name.eq_ignore_ascii_case("image/svg+xml") {
        return Some(RemoteDesktopClipboardFormat::ImageSvg);
    }
    if name.eq_ignore_ascii_case("image/bmp") {
        return Some(RemoteDesktopClipboardFormat::ImageBmp);
    }
    if name.eq_ignore_ascii_case("image/tiff") || name.eq_ignore_ascii_case("image/tif") {
        return Some(RemoteDesktopClipboardFormat::ImageTiff);
    }
    None
}

#[cfg(feature = "legacy-freerdp")]
mod legacy_freerdp {
    use std::{
        ffi::{CStr, CString},
        sync::{Arc, Mutex},
    };

    use freerdp2::{
        PIXEL_FORMAT_BGRA32, RdpError, Settings,
        channels::cliprdr::{
            Format as FreeRdpClipboardFormat, GeneralCapabilities as FreeRdpClipboardCapabilities,
        },
        client::{CliprdrClientContext, CliprdrFormat, CliprdrHandler, Context, Handler},
        input::{KbdFlags, PtrFlags, PtrXFlags, SyncFlags, WHEEL_ROTATION_MASK},
        locale::keyboard_init_ex,
        sys,
        update::UpdateHandler,
        winpr::{WaitResult, wait_for_multiple_objects},
    };
    use ironrdp::input::Scancode;
    use oxideterm_remote_desktop::{
        RemoteDesktopKeyState, RemoteDesktopMouseButton, RemoteDesktopWheelDelta,
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

        let clipboard = Arc::new(Mutex::new(LegacyClipboardState::default()));
        let mut context = Context::new(LegacyFreeRdpHandler {
            writer: writer.clone(),
            clipboard,
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
        let mut keyboard_mapper = RdpKeyboardInputMapper::default();
        let result = run_event_loop(
            &mut context,
            &request_rx,
            config.read_only,
            &mut mouse_position,
            &mut keyboard_mapper,
        );
        let _ = context.client_stop();
        result
    }

    fn run_event_loop(
        context: &mut Context<LegacyFreeRdpHandler>,
        request_rx: &mpsc::Receiver<RemoteDesktopHelperRequest>,
        read_only: bool,
        mouse_position: &mut MousePositionCache,
        keyboard_mapper: &mut RdpKeyboardInputMapper,
    ) -> Result<(), String> {
        loop {
            process_pending_requests(
                context,
                request_rx,
                read_only,
                mouse_position,
                keyboard_mapper,
            )?;
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
        let requested_size = normalized_rdp_desktop_size(config.size);
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
        set_freerdp_bool(settings, sys::FreeRDP_RedirectClipboard, true)?;
        settings.set_support_display_control(false);
        Ok(())
    }

    fn process_pending_requests(
        context: &mut Context<LegacyFreeRdpHandler>,
        request_rx: &mpsc::Receiver<RemoteDesktopHelperRequest>,
        read_only: bool,
        mouse_position: &mut MousePositionCache,
        keyboard_mapper: &mut RdpKeyboardInputMapper,
    ) -> Result<(), String> {
        let mut coalesced_requests = Vec::new();
        let mut request_coalescer = ClientRdpRequestCoalescer::default();
        loop {
            match request_rx.try_recv() {
                Ok(request) => request_coalescer.push(request, &mut coalesced_requests),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    context.instance.disconnect().map_err(|error| {
                        format_freerdp_error("Legacy RDP disconnect failed", &error)
                    })?;
                    return Ok(());
                }
            }
        }
        request_coalescer.flush(&mut coalesced_requests);
        for request in coalesced_requests {
            handle_request(context, request, read_only, mouse_position, keyboard_mapper)?;
        }
        Ok(())
    }

    fn handle_request(
        context: &mut Context<LegacyFreeRdpHandler>,
        request: RemoteDesktopHelperRequest,
        read_only: bool,
        mouse_position: &mut MousePositionCache,
        keyboard_mapper: &mut RdpKeyboardInputMapper,
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
            RemoteDesktopHelperRequest::Resize { size, .. } => {
                resize_legacy_session(context, size)?;
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
                for operation in keyboard_mapper.operations(&key, state) {
                    send_input_operation(context, operation)?;
                }
            }
            RemoteDesktopHelperRequest::Text { text } if !read_only => {
                for character in text.chars().filter(|character| !character.is_control()) {
                    send_unicode_key(context, character, RemoteDesktopKeyState::Pressed)?;
                    send_unicode_key(context, character, RemoteDesktopKeyState::Released)?;
                }
            }
            RemoteDesktopHelperRequest::ClipboardText { text } if !read_only => {
                set_legacy_clipboard_text(context, text)?;
            }
            RemoteDesktopHelperRequest::SynchronizeLockKeys { keys } if !read_only => {
                input(context)?
                    .send_synchronize_event(legacy_sync_flags(keys))
                    .map_err(|error| {
                        format_freerdp_error("Legacy RDP synchronize event failed", &error)
                    })?;
            }
            RemoteDesktopHelperRequest::ReleaseAllInputs if !read_only => {
                for operation in keyboard_mapper.release_all_operations() {
                    send_input_operation(context, operation)?;
                }
            }
            RemoteDesktopHelperRequest::Connect { .. } => {
                return Err("Legacy RDP helper received a second connect request.".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    fn resize_legacy_session(
        context: &mut Context<LegacyFreeRdpHandler>,
        size: RemoteDesktopSize,
    ) -> Result<(), String> {
        let requested_size = normalized_rdp_desktop_size(size);
        set_freerdp_u32(
            &mut context.settings,
            sys::FreeRDP_DesktopWidth,
            requested_size.width,
        )?;
        set_freerdp_u32(
            &mut context.settings,
            sys::FreeRDP_DesktopHeight,
            requested_size.height,
        )?;
        send_event(
            &context.handler.writer,
            RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Reconnecting,
                message: Some(
                    "Reopening legacy RDP session with the new display size.".to_string(),
                ),
            },
        )?;
        // FreeRDP 2 does not give the legacy fallback a reliable
        // display-control path, so apply the requested desktop size by
        // reconnecting with updated settings.
        context
            .instance
            .reconnect()
            .map_err(|error| format_freerdp_error("Legacy RDP resize reconnect failed", &error))
    }

    fn set_legacy_clipboard_text(
        context: &mut Context<LegacyFreeRdpHandler>,
        text: String,
    ) -> Result<(), String> {
        {
            let mut clipboard = context
                .handler
                .clipboard
                .lock()
                .map_err(|_| "Legacy RDP clipboard state lock failed.".to_string())?;
            clipboard.local_text = Some(text);
        }
        advertise_legacy_clipboard_formats(context)
    }

    fn advertise_legacy_clipboard_formats(
        context: &mut Context<LegacyFreeRdpHandler>,
    ) -> Result<(), String> {
        let formats = {
            let clipboard = context
                .handler
                .clipboard
                .lock()
                .map_err(|_| "Legacy RDP clipboard state lock failed.".to_string())?;
            legacy_clipboard_formats(clipboard.local_text.is_some())
        };
        let Some(cliprdr) = context.cliprdr.as_mut() else {
            return Ok(());
        };
        cliprdr.send_client_format_list(&formats).map_err(|error| {
            format_freerdp_error("Legacy RDP clipboard format list failed", &error)
        })
    }

    fn legacy_clipboard_formats(has_text: bool) -> Vec<CliprdrFormat> {
        if has_text {
            vec![
                CliprdrFormat {
                    id: Some(FreeRdpClipboardFormat::UnicodeText),
                    name: None,
                },
                CliprdrFormat {
                    id: Some(FreeRdpClipboardFormat::Text),
                    name: None,
                },
            ]
        } else {
            Vec::new()
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum LegacyClipboardTextFormat {
        UnicodeText,
        Text,
    }

    impl LegacyClipboardTextFormat {
        fn from_freerdp(format: FreeRdpClipboardFormat) -> Option<Self> {
            match format {
                FreeRdpClipboardFormat::UnicodeText => Some(Self::UnicodeText),
                FreeRdpClipboardFormat::Text | FreeRdpClipboardFormat::OemText => Some(Self::Text),
                _ => None,
            }
        }

        fn as_freerdp(self) -> FreeRdpClipboardFormat {
            match self {
                Self::UnicodeText => FreeRdpClipboardFormat::UnicodeText,
                Self::Text => FreeRdpClipboardFormat::Text,
            }
        }
    }

    #[derive(Default)]
    struct LegacyClipboardState {
        local_text: Option<String>,
        remote_text_format: Option<LegacyClipboardTextFormat>,
    }

    struct LegacyClipboardHandler {
        writer: SharedEventWriter,
        state: Arc<Mutex<LegacyClipboardState>>,
    }

    impl CliprdrHandler for LegacyClipboardHandler {
        fn monitor_ready(&mut self, context: &mut CliprdrClientContext) -> freerdp2::Result<()> {
            context.send_client_general_capabilities(&FreeRdpClipboardCapabilities::empty())?;
            send_legacy_clipboard_format_list(context, &self.state)
        }

        fn server_format_list(
            &mut self,
            context: &mut CliprdrClientContext,
            formats: &[CliprdrFormat],
        ) -> freerdp2::Result<()> {
            let remote_text_format = preferred_legacy_clipboard_text_format(formats);
            {
                let mut state = self.state.lock().map_err(|_| {
                    RdpError::Failed("Legacy RDP clipboard state lock failed.".into())
                })?;
                state.remote_text_format = remote_text_format;
            }
            context.send_client_format_list_response(true)?;
            if let Some(format) = remote_text_format {
                context.send_client_format_data_request(format.as_freerdp())?;
            }
            Ok(())
        }

        fn server_format_data_request(
            &mut self,
            context: &mut CliprdrClientContext,
            format: FreeRdpClipboardFormat,
        ) -> freerdp2::Result<()> {
            let response = {
                let state = self.state.lock().map_err(|_| {
                    RdpError::Failed("Legacy RDP clipboard state lock failed.".into())
                })?;
                let Some(text) = state.local_text.as_deref() else {
                    return context.send_client_format_data_response(None);
                };
                let Some(format) = LegacyClipboardTextFormat::from_freerdp(format) else {
                    return context.send_client_format_data_response(None);
                };
                encode_legacy_clipboard_text(text, format)
            };
            context.send_client_format_data_response(Some(&response))
        }

        fn server_format_data_response(
            &mut self,
            _context: &mut CliprdrClientContext,
            data: &[u8],
        ) -> freerdp2::Result<()> {
            let format = {
                let state = self.state.lock().map_err(|_| {
                    RdpError::Failed("Legacy RDP clipboard state lock failed.".into())
                })?;
                state.remote_text_format
            };
            let Some(format) = format else {
                return Ok(());
            };
            if let Some(text) = decode_legacy_clipboard_text(data, format) {
                send_event(
                    &self.writer,
                    RemoteDesktopHelperEvent::ClipboardText { text },
                )
                .map_err(RdpError::Failed)?;
            }
            Ok(())
        }
    }

    fn send_legacy_clipboard_format_list(
        context: &mut CliprdrClientContext,
        state: &Arc<Mutex<LegacyClipboardState>>,
    ) -> freerdp2::Result<()> {
        let formats = {
            let state = state
                .lock()
                .map_err(|_| RdpError::Failed("Legacy RDP clipboard state lock failed.".into()))?;
            legacy_clipboard_formats(state.local_text.is_some())
        };
        context.send_client_format_list(&formats)
    }

    fn preferred_legacy_clipboard_text_format(
        formats: &[CliprdrFormat],
    ) -> Option<LegacyClipboardTextFormat> {
        formats
            .iter()
            .find_map(|format| match format.id {
                Some(FreeRdpClipboardFormat::UnicodeText) => {
                    Some(LegacyClipboardTextFormat::UnicodeText)
                }
                _ => None,
            })
            .or_else(|| {
                formats.iter().find_map(|format| match format.id {
                    Some(FreeRdpClipboardFormat::Text | FreeRdpClipboardFormat::OemText) => {
                        Some(LegacyClipboardTextFormat::Text)
                    }
                    _ => None,
                })
            })
    }

    fn encode_legacy_clipboard_text(text: &str, format: LegacyClipboardTextFormat) -> Vec<u8> {
        match format {
            LegacyClipboardTextFormat::UnicodeText => {
                let mut bytes = Vec::with_capacity(text.len().saturating_mul(2).saturating_add(2));
                for code_unit in text.encode_utf16().chain(std::iter::once(0)) {
                    bytes.extend_from_slice(&code_unit.to_le_bytes());
                }
                bytes
            }
            LegacyClipboardTextFormat::Text => {
                let mut bytes = text.as_bytes().to_vec();
                bytes.push(0);
                bytes
            }
        }
    }

    fn decode_legacy_clipboard_text(
        data: &[u8],
        format: LegacyClipboardTextFormat,
    ) -> Option<String> {
        match format {
            LegacyClipboardTextFormat::UnicodeText => {
                let mut code_units = Vec::with_capacity(data.len() / 2);
                for chunk in data.chunks_exact(2) {
                    let code_unit = u16::from_le_bytes([chunk[0], chunk[1]]);
                    if code_unit == 0 {
                        break;
                    }
                    code_units.push(code_unit);
                }
                Some(String::from_utf16_lossy(&code_units))
            }
            LegacyClipboardTextFormat::Text => {
                let end = data
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(data.len());
                Some(String::from_utf8_lossy(&data[..end]).into_owned())
            }
        }
    }

    fn legacy_sync_flags(keys: RemoteDesktopLockKeys) -> SyncFlags {
        let mut flags = SyncFlags::empty();
        if keys.scroll_lock {
            flags |= SyncFlags::SCROLL;
        }
        if keys.num_lock {
            flags |= SyncFlags::NUM;
        }
        if keys.caps_lock {
            flags |= SyncFlags::CAPS;
        }
        if keys.kana_lock {
            flags |= SyncFlags::KANA;
        }
        flags
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

    fn send_input_operation(
        context: &mut Context<LegacyFreeRdpHandler>,
        operation: RdpInputOperation,
    ) -> Result<(), String> {
        match operation {
            RdpInputOperation::KeyPressed(scancode) => {
                let (flags, code) = legacy_keyboard_event(scancode, RemoteDesktopKeyState::Pressed);
                input(context)?
                    .send_keyboard_event(flags, code)
                    .map_err(|error| {
                        format_freerdp_error("Legacy RDP keyboard event failed", &error)
                    })?;
            }
            RdpInputOperation::KeyReleased(scancode) => {
                let (flags, code) =
                    legacy_keyboard_event(scancode, RemoteDesktopKeyState::Released);
                input(context)?
                    .send_keyboard_event(flags, code)
                    .map_err(|error| {
                        format_freerdp_error("Legacy RDP keyboard event failed", &error)
                    })?;
            }
            RdpInputOperation::UnicodeKeyPressed(character) => {
                send_unicode_key(context, character, RemoteDesktopKeyState::Pressed)?;
            }
            RdpInputOperation::UnicodeKeyReleased(character) => {
                send_unicode_key(context, character, RemoteDesktopKeyState::Released)?;
            }
            RdpInputOperation::MouseButtonPressed(_)
            | RdpInputOperation::MouseButtonReleased(_)
            | RdpInputOperation::MouseMove(_)
            | RdpInputOperation::WheelRotations(_) => {}
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
            let mut pixels = buffer
                .get(..frame_len)
                .map(ToOwned::to_owned)
                .ok_or_else(|| "Legacy RDP frame buffer is shorter than expected.".to_string())?;
            set_bgra_alpha_opaque(&mut pixels);
            return Ok(pixels);
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
        set_bgra_alpha_opaque(&mut pixels);
        Ok(pixels)
    }

    fn set_bgra_alpha_opaque(bytes: &mut [u8]) {
        for pixel in bytes.chunks_exact_mut(4) {
            // FreeRDP's BGRA32 desktop surface uses the last byte as padding
            // for our opaque framebuffer contract.
            pixel[3] = 0xff;
        }
    }

    struct LegacyFreeRdpHandler {
        writer: SharedEventWriter,
        clipboard: Arc<Mutex<LegacyClipboardState>>,
    }

    impl Handler for LegacyFreeRdpHandler {
        fn clipboard_connected(&mut self, clip: &mut CliprdrClientContext) {
            // FreeRDP owns the CLIPRDR context, while the helper event loop owns
            // protocol requests. Shared state keeps text-only clipboard parity
            // without exposing file or image clipboard formats.
            clip.register_handler(LegacyClipboardHandler {
                writer: self.writer.clone(),
                state: Arc::clone(&self.clipboard),
            });
        }

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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn legacy_clipboard_prefers_unicode_text() {
            let formats = vec![
                CliprdrFormat {
                    id: Some(FreeRdpClipboardFormat::Text),
                    name: None,
                },
                CliprdrFormat {
                    id: Some(FreeRdpClipboardFormat::UnicodeText),
                    name: None,
                },
            ];

            assert_eq!(
                preferred_legacy_clipboard_text_format(&formats),
                Some(LegacyClipboardTextFormat::UnicodeText)
            );
        }

        #[test]
        fn legacy_unicode_clipboard_text_round_trips_with_nul_terminator() {
            let encoded =
                encode_legacy_clipboard_text("hello 世界", LegacyClipboardTextFormat::UnicodeText);

            assert_eq!(encoded[encoded.len() - 2..], [0, 0]);
            assert_eq!(
                decode_legacy_clipboard_text(&encoded, LegacyClipboardTextFormat::UnicodeText),
                Some("hello 世界".to_string())
            );
        }

        #[test]
        fn legacy_ansi_clipboard_text_round_trips_with_nul_terminator() {
            let encoded = encode_legacy_clipboard_text("plain", LegacyClipboardTextFormat::Text);

            assert_eq!(encoded.last(), Some(&0));
            assert_eq!(
                decode_legacy_clipboard_text(&encoded, LegacyClipboardTextFormat::Text),
                Some("plain".to_string())
            );
        }
    }
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

    use ironrdp::pdu::{geometry::InclusiveRectangle, input::fast_path::SynchronizeFlags};
    use oxideterm_remote_desktop::{
        RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate, RemoteDesktopKey,
        RemoteDesktopKeyState, RemoteDesktopMouseButton, RemoteDesktopMouseButtonState,
        RemoteDesktopRect, RemoteDesktopWheelDelta,
    };

    use super::*;

    fn tiny_png_bytes() -> Vec<u8> {
        vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, b'I', b'D', b'A', b'T', 0x78,
            0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
            0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
        ]
    }

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
    fn keyboard_mapping_accepts_physical_letter_codes_for_shortcuts() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "KeyV".to_string(),
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
    fn keyboard_mapping_accepts_physical_digit_codes_for_shortcuts() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "Digit1".to_string(),
                text: Some("1".to_string()),
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
            RdpInputOperation::KeyPressed(scancode) => assert_eq!(scancode.as_u16(), 0x02),
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn keyboard_mapping_does_not_duplicate_physical_modifier_keys() {
        let operations = rdp_key_operations(
            &RemoteDesktopKey {
                code: "ControlRight".to_string(),
                text: None,
                alt: false,
                ctrl: true,
                shift: false,
                meta: false,
            },
            RemoteDesktopKeyState::Pressed,
        );

        assert_eq!(operations.len(), 1);
        match &operations[0] {
            RdpInputOperation::KeyPressed(scancode) => assert_eq!(scancode.as_u16(), 0xe01d),
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn client_request_coalescer_keeps_latest_mouse_move_before_clicks() {
        let mut coalescer = ClientRdpRequestCoalescer::default();
        let mut output = Vec::new();

        coalescer.push(
            RemoteDesktopHelperRequest::MouseMove { x: 10, y: 20 },
            &mut output,
        );
        coalescer.push(
            RemoteDesktopHelperRequest::MouseMove { x: 30, y: 40 },
            &mut output,
        );
        assert!(output.is_empty());

        coalescer.push(
            RemoteDesktopHelperRequest::MouseButton {
                button: RemoteDesktopMouseButton::Left,
                state: RemoteDesktopMouseButtonState::Pressed,
            },
            &mut output,
        );
        coalescer.push(
            RemoteDesktopHelperRequest::MouseMove { x: 50, y: 60 },
            &mut output,
        );
        coalescer.flush(&mut output);

        assert_eq!(
            output,
            vec![
                RemoteDesktopHelperRequest::MouseMove { x: 30, y: 40 },
                RemoteDesktopHelperRequest::MouseButton {
                    button: RemoteDesktopMouseButton::Left,
                    state: RemoteDesktopMouseButtonState::Pressed,
                },
                RemoteDesktopHelperRequest::MouseMove { x: 50, y: 60 },
            ]
        );
    }

    #[test]
    fn client_output_drain_yields_after_budget() {
        let writer = SharedEventWriter::inert_for_tests();
        let (output_tx, output_rx) = mpsc::sync_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
        for _ in 0..=RDP_CLIENT_OUTPUT_DRAIN_LIMIT {
            output_tx
                .send(ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame {
                    frame: test_frame(),
                }))
                .unwrap();
        }

        let drain = drain_client_rdp_outputs(&writer, &output_rx).unwrap();

        assert_eq!(drain.drained, RDP_CLIENT_OUTPUT_DRAIN_LIMIT);
        assert!(drain.exit.is_none());
        assert!(matches!(
            output_rx.try_recv(),
            Ok(ClientRdpOutput::Event(_))
        ));
    }

    #[test]
    fn saturated_graphics_queue_defers_latest_base_frame_without_dropping_state() {
        let (output_tx, output_rx) = mpsc::sync_channel(1);
        output_tx
            .send(ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame {
                frame: test_frame(),
            }))
            .unwrap();
        let image = DecodedImage::new(PixelFormat::RgbA32, 2, 2);
        let mut pending_base_frame = false;

        send_client_rdp_graphics_event(
            &output_tx,
            RemoteDesktopHelperEvent::FrameUpdate {
                update: RemoteDesktopFrameUpdate::new(
                    RemoteDesktopSize {
                        width: 2,
                        height: 2,
                    },
                    RemoteDesktopRect::new(0, 0, 1, 1),
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![1, 1, 1, 0xff],
                ),
            },
            &mut pending_base_frame,
        )
        .unwrap();

        assert!(pending_base_frame);
        assert!(matches!(
            output_rx.try_recv(),
            Ok(ClientRdpOutput::Event(
                RemoteDesktopHelperEvent::Frame { .. }
            ))
        ));
        assert!(matches!(
            output_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));

        flush_pending_rdp_base_frame(&output_tx, &image, &mut pending_base_frame).unwrap();

        assert!(!pending_base_frame);
        assert!(matches!(
            output_rx.try_recv(),
            Ok(ClientRdpOutput::Event(
                RemoteDesktopHelperEvent::Frame { .. }
            ))
        ));
    }

    #[test]
    fn client_loop_prioritizes_queued_close_over_pending_output_error() {
        let writer = SharedEventWriter::inert_for_tests();
        let (request_tx, request_rx) = mpsc::channel();
        let (input_tx, _input_rx) = tokio_mpsc::unbounded_channel();
        let (output_tx, output_rx) = mpsc::sync_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
        output_tx
            .send(ClientRdpOutput::ConnectionFailure(
                connector::ConnectorError::new(
                    "test",
                    ConnectorErrorKind::Reason("queued failure".to_string()),
                ),
            ))
            .unwrap();
        request_tx.send(RemoteDesktopHelperRequest::Close).unwrap();
        let mut config = RdpWorkerConfig {
            endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
            username: "alice".to_string(),
            password: RemoteDesktopSecret::from("secret"),
            domain: None,
            size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
            scale_factor: RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT,
            read_only: false,
        };

        let exit = run_client_rdp_loop(
            &writer,
            &request_rx,
            &input_tx,
            output_rx,
            &mut config,
            false,
        )
        .unwrap();

        assert!(matches!(exit, ClientRdpSessionExit::Closed));
    }

    #[test]
    fn connector_failure_exit_preserves_structured_category() {
        let writer = SharedEventWriter::inert_for_tests();
        let (output_tx, output_rx) = mpsc::sync_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
        output_tx
            .send(ClientRdpOutput::ConnectionFailure(
                connector::ConnectorError::new("Authentication", ConnectorErrorKind::AccessDenied),
            ))
            .unwrap();

        let drain = drain_client_rdp_outputs(&writer, &output_rx).unwrap();

        // The helper event should classify from the connector error kind, not
        // from a localized display string.
        match drain.exit {
            Some(ClientRdpSessionExit::ConnectionFailed { message, category }) => {
                assert_eq!(category, RemoteDesktopErrorCategory::Authentication);
                assert!(message.contains("access denied"));
            }
            other => panic!("unexpected drain exit: {other:?}"),
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
    fn clipboard_ready_advertises_cached_local_text() {
        let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
        let (output_tx, _output_rx) = mpsc::sync_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
        let mut backend = ClientClipboardBackend::new(input_tx, output_tx);
        backend.set_local_text("hello".to_string());

        backend.on_ready();

        match input_rx.try_recv().unwrap() {
            RdpInputEvent::Clipboard(ClipboardMessage::SendInitiateCopy(formats)) => {
                assert!(
                    formats
                        .iter()
                        .any(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
                );
                assert!(
                    formats
                        .iter()
                        .any(|format| format.id == ClipboardFormatId::CF_TEXT)
                );
            }
            message => panic!("unexpected clipboard message: {message:?}"),
        }
    }

    #[test]
    fn clipboard_ready_advertises_cached_local_image_data() {
        let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
        let (output_tx, _output_rx) = mpsc::sync_channel(RDP_CLIENT_OUTPUT_QUEUE_CAPACITY);
        let mut backend = ClientClipboardBackend::new(input_tx, output_tx);
        backend.set_local_data(RemoteDesktopClipboardData::new(
            RemoteDesktopClipboardFormat::ImagePng,
            vec![1, 2, 3],
        ));

        backend.on_ready();

        match input_rx.try_recv().unwrap() {
            RdpInputEvent::Clipboard(ClipboardMessage::SendInitiateCopy(formats)) => {
                assert!(formats.iter().any(|format| {
                    format.id == RDP_CLIPBOARD_FORMAT_IMAGE_PNG
                        && format
                            .name
                            .as_ref()
                            .is_some_and(|name| name.value() == "PNG")
                }));
                assert!(
                    !formats
                        .iter()
                        .any(|format| format.id == ClipboardFormatId::CF_UNICODETEXT)
                );
            }
            message => panic!("unexpected clipboard message: {message:?}"),
        }
    }

    #[test]
    fn clipboard_image_format_prefers_named_png_before_text() {
        let formats = vec![
            ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
            ClipboardFormat::new(ClipboardFormatId::new(0xc080))
                .with_name(ClipboardFormatName::new("image/png")),
        ];

        assert_eq!(
            preferred_image_clipboard_format(&formats),
            Some(RdpClipboardDataFormat {
                id: ClipboardFormatId::new(0xc080),
                format: RemoteDesktopClipboardFormat::ImagePng,
                encoding: RdpClipboardDataEncoding::Encoded,
            })
        );
    }

    #[test]
    fn local_png_clipboard_can_answer_dib_requests() {
        let data = RemoteDesktopClipboardData::new(
            RemoteDesktopClipboardFormat::ImagePng,
            tiny_png_bytes(),
        );

        assert!(encode_local_clipboard_data(&data, ClipboardFormatId::CF_DIB).is_some());
        assert!(encode_local_clipboard_data(&data, ClipboardFormatId::CF_DIBV5).is_some());
        assert_eq!(
            encode_local_clipboard_data(&data, RDP_CLIPBOARD_FORMAT_IMAGE_PNG),
            Some(data.bytes.clone())
        );
    }

    #[test]
    fn remote_dib_clipboard_decodes_to_png_data() {
        let data = RemoteDesktopClipboardData::new(
            RemoteDesktopClipboardFormat::ImagePng,
            tiny_png_bytes(),
        );
        let dib = encode_local_clipboard_data(&data, ClipboardFormatId::CF_DIB).unwrap();

        let decoded = decode_remote_clipboard_data(
            RdpClipboardDataFormat {
                id: ClipboardFormatId::CF_DIB,
                format: RemoteDesktopClipboardFormat::ImagePng,
                encoding: RdpClipboardDataEncoding::Dib,
            },
            dib,
        )
        .unwrap();

        assert_eq!(decoded.format, RemoteDesktopClipboardFormat::ImagePng);
        assert!(decoded.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn clipboard_data_request_enters_client_loop() {
        let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
        let mut input_database = RdpInputDatabase::new();
        let mut keyboard_mapper = RdpKeyboardInputMapper::default();
        let data =
            RemoteDesktopClipboardData::new(RemoteDesktopClipboardFormat::ImageGif, vec![9, 8, 7]);

        forward_client_rdp_request(
            &input_tx,
            &mut input_database,
            &mut keyboard_mapper,
            RemoteDesktopHelperRequest::ClipboardData { data: data.clone() },
            false,
        )
        .unwrap();

        match input_rx.try_recv().unwrap() {
            RdpInputEvent::SetClipboardData(received) => assert_eq!(received, data),
            event => panic!("expected clipboard data event, got {event:?}"),
        }
    }

    #[test]
    fn lock_key_sync_request_emits_fastpath_sync_event() {
        let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
        let mut input_database = RdpInputDatabase::new();
        let mut keyboard_mapper = RdpKeyboardInputMapper::default();

        forward_client_rdp_request(
            &input_tx,
            &mut input_database,
            &mut keyboard_mapper,
            RemoteDesktopHelperRequest::SynchronizeLockKeys {
                keys: RemoteDesktopLockKeys {
                    scroll_lock: true,
                    num_lock: false,
                    caps_lock: true,
                    kana_lock: false,
                },
            },
            false,
        )
        .unwrap();

        match input_rx.try_recv().unwrap() {
            RdpInputEvent::FastPath(events) => {
                assert_eq!(events.len(), 1);
                let FastPathInputEvent::SyncEvent(flags) = events[0] else {
                    panic!("expected synchronize event, got {:?}", events[0]);
                };
                assert!(flags.contains(SynchronizeFlags::SCROLL_LOCK));
                assert!(flags.contains(SynchronizeFlags::CAPS_LOCK));
                assert!(!flags.contains(SynchronizeFlags::NUM_LOCK));
                assert!(!flags.contains(SynchronizeFlags::KANA_LOCK));
            }
            message => panic!("unexpected RDP input message: {message:?}"),
        }
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
            scale_factor: RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT,
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
        assert_eq!(
            client_config.connector.desktop_scale_factor,
            RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT
        );
        let bitmap = client_config.connector.bitmap.as_ref().unwrap();
        assert!(bitmap.lossy_compression);
        assert_eq!(bitmap.color_depth, 32);
    }

    #[test]
    fn client_config_adjusts_initial_display_size_for_rdp() {
        let config = RdpWorkerConfig {
            endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
            username: "alice".to_string(),
            password: RemoteDesktopSecret::from("secret"),
            domain: None,
            size: RemoteDesktopSize {
                width: 1601,
                height: 899,
            },
            scale_factor: RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT,
            read_only: false,
        };

        let client_config = build_client_rdp_config(&config).unwrap();

        assert_eq!(client_config.connector.desktop_size.width % 2, 0);
        assert_eq!(client_config.connector.desktop_size.height, 899);
    }

    #[test]
    fn rdp_desktop_size_normalization_matches_displaycontrol_bounds() {
        assert_eq!(
            normalized_rdp_desktop_size(RemoteDesktopSize {
                width: 201,
                height: 121,
            }),
            RemoteDesktopSize {
                width: 200,
                height: 200,
            }
        );
        assert_eq!(
            normalized_rdp_desktop_size(RemoteDesktopSize {
                width: 9001,
                height: 9001,
            }),
            RemoteDesktopSize {
                width: 8192,
                height: 8192,
            }
        );
    }

    #[test]
    fn resize_request_enters_client_loop_with_normalized_rdp_size() {
        let (input_tx, mut input_rx) = tokio_mpsc::unbounded_channel();
        let mut input_database = RdpInputDatabase::new();
        let mut keyboard_mapper = RdpKeyboardInputMapper::default();

        forward_client_rdp_request(
            &input_tx,
            &mut input_database,
            &mut keyboard_mapper,
            RemoteDesktopHelperRequest::Resize {
                size: RemoteDesktopSize {
                    width: 201,
                    height: 121,
                },
                scale_factor: Some(125),
            },
            false,
        )
        .unwrap();

        match input_rx.try_recv().unwrap() {
            RdpInputEvent::Resize {
                width,
                height,
                scale_factor,
                ..
            } => {
                assert_eq!(width, 200);
                assert_eq!(height, 200);
                assert_eq!(scale_factor, 125);
            }
            event => panic!("expected resize event, got {event:?}"),
        }
    }

    #[test]
    fn reconnect_state_remembers_latest_resize() {
        let mut config = RdpWorkerConfig {
            endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
            username: "alice".to_string(),
            password: RemoteDesktopSecret::from("secret"),
            domain: None,
            size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
            scale_factor: RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT,
            read_only: false,
        };

        remember_rdp_reconnect_state(
            &RemoteDesktopHelperRequest::Resize {
                size: RemoteDesktopSize {
                    width: 1600,
                    height: 900,
                },
                scale_factor: Some(150),
            },
            &mut config,
        );

        assert_eq!(
            config.size,
            RemoteDesktopSize {
                width: 1600,
                height: 900
            }
        );
        assert_eq!(config.scale_factor, 150);
    }

    #[test]
    fn scale_factor_defaults_match_connector_and_displaycontrol_contexts() {
        assert_eq!(rdp_connector_scale_factor(Some(100)), 100);
        assert_eq!(rdp_connector_scale_factor(Some(500)), 500);
        assert_eq!(
            rdp_connector_scale_factor(Some(99)),
            RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT
        );
        assert_eq!(
            rdp_connector_scale_factor(None),
            RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT
        );
        assert_eq!(
            rdp_displaycontrol_scale_factor(None),
            RDP_DISPLAYCONTROL_DEFAULT_SCALE_FACTOR_PERCENT
        );
    }

    #[test]
    fn disconnect_reason_hides_local_dependency_paths() {
        let message = sanitize_rdp_disconnect_reason(Some(
            "RDP session ended: /Users/example/.cargo/git/checkouts/ironrdp/src/lib.rs",
        ));

        assert_eq!(message.as_deref(), Some("RDP session ended."));
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
    fn initial_partial_black_update_waits_for_base_frame() {
        let image = DecodedImage::new(PixelFormat::RgbA32, 4, 4);
        let mut sent_initial_frame = false;

        let event = graphics_update_event(
            &image,
            InclusiveRectangle {
                left: 0,
                top: 0,
                right: 1,
                bottom: 1,
            },
            &mut sent_initial_frame,
        )
        .expect("graphics update maps");

        assert!(event.is_none());
        assert!(!sent_initial_frame);
    }

    #[test]
    fn stale_graphics_region_is_skipped_without_failing_session() {
        let image = DecodedImage::new(PixelFormat::RgbA32, 4, 4);
        let mut sent_initial_frame = true;

        let event = graphics_update_event(
            &image,
            InclusiveRectangle {
                left: 3,
                top: 3,
                right: 4,
                bottom: 4,
            },
            &mut sent_initial_frame,
        )
        .expect("stale graphics regions should be skippable");

        assert!(event.is_none());
        assert!(sent_initial_frame);
    }

    #[test]
    fn initial_full_black_update_can_start_base_frame() {
        let image = DecodedImage::new(PixelFormat::RgbA32, 4, 4);
        let mut sent_initial_frame = false;

        let event = graphics_update_event(
            &image,
            InclusiveRectangle {
                left: 0,
                top: 0,
                right: 3,
                bottom: 3,
            },
            &mut sent_initial_frame,
        )
        .expect("graphics update maps");

        match event {
            Some(RemoteDesktopHelperEvent::Frame { frame }) => {
                assert_eq!(
                    frame.size,
                    RemoteDesktopSize {
                        width: 4,
                        height: 4
                    }
                );
                assert!(sent_initial_frame);
            }
            other => panic!("expected initial frame, got {other:?}"),
        }
    }

    #[test]
    fn full_screen_update_refreshes_base_frame_after_initial_frame() {
        let image = DecodedImage::new(PixelFormat::RgbA32, 4, 4);
        let mut sent_initial_frame = true;

        let event = graphics_update_event(
            &image,
            InclusiveRectangle {
                left: 0,
                top: 0,
                right: 3,
                bottom: 3,
            },
            &mut sent_initial_frame,
        )
        .expect("graphics update maps");

        assert!(matches!(
            event,
            Some(RemoteDesktopHelperEvent::Frame { .. })
        ));
        assert!(sent_initial_frame);
    }

    #[test]
    fn base_frame_event_uses_current_image_as_complete_backing_frame() {
        let image = DecodedImage::new(PixelFormat::RgbA32, 2, 1);

        match base_frame_event(&image) {
            RemoteDesktopHelperEvent::Frame { frame } => {
                assert_eq!(
                    frame.size,
                    RemoteDesktopSize {
                        width: 2,
                        height: 1
                    }
                );
                assert_eq!(frame.format, RemoteDesktopFrameFormat::Rgba8);
                assert_eq!(frame.bytes, vec![0, 0, 0, 0xff, 0, 0, 0, 0xff]);
            }
            other => panic!("expected base frame, got {other:?}"),
        }
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
        assert_eq!(
            connector_error_category(&error),
            RemoteDesktopErrorCategory::LegacySecurity
        );
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

    #[test]
    fn access_denied_connector_error_is_authentication_category() {
        let error =
            connector::ConnectorError::new("Authentication", ConnectorErrorKind::AccessDenied);

        // Access denied is a stable authentication failure constructor for tests.
        assert_eq!(
            connector_error_category(&error),
            RemoteDesktopErrorCategory::Authentication
        );
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
            scale_factor: RDP_CONNECT_DEFAULT_SCALE_FACTOR_PERCENT,
            read_only: false,
        };
        let writer = SharedEventWriter::inert_for_tests();

        let error = run_legacy_rdp_worker(config, writer, request_rx).unwrap_err();

        assert_eq!(error, LEGACY_RDP_ENGINE_UNAVAILABLE_MESSAGE);
    }

    fn test_frame() -> RemoteDesktopFrame {
        RemoteDesktopFrame::new(
            RemoteDesktopSize {
                width: 1,
                height: 1,
            },
            RemoteDesktopFrameFormat::Bgra8,
            vec![0, 0, 0, 0xff],
        )
    }
}
