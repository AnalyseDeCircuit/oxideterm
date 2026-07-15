// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::VecDeque,
    fmt, future,
    io::{self, BufRead, BufReader},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
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
    connector::ConnectionResult,
    connector::connection_activation::ConnectionActivationState,
    connector::{self, ConnectorErrorKind, Credentials},
    displaycontrol::client::DisplayControlClient,
    dvc::DrdynvcClient,
    graphics::image_processing::PixelFormat,
    input::{
        Database as RdpInputDatabase, MousePosition, Operation as RdpInputOperation,
        synchronize_event as rdp_synchronize_event,
    },
    pdu::{
        gcc::KeyboardType,
        geometry::InclusiveRectangle,
        input::fast_path::FastPathInputEvent,
        rdp::{
            capability_sets::{
                BitmapCodecs, CODEC_ID_NONE, CODEC_ID_QOI, CODEC_ID_QOIZ, CODEC_ID_REMOTEFX,
                CodecId, MajorPlatformType, client_codecs_capabilities,
            },
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

mod client_session;
mod clipboard;
mod config;
mod control;
mod event_writer;
mod frame;
mod input;
mod runtime;
mod session_loop;

use client_session::*;
use clipboard::*;
use config::*;
use control::*;
use runtime::*;
use session_loop::*;

use event_writer::{SharedEventWriter, send_event};
use frame::*;
use input::*;

const RDP_CLIENT_NAME: &str = "OxideTerm";
const RDP_CLIENT_LOOP_POLL_INTERVAL: Duration = Duration::from_millis(8);
const RDP_CLIENT_REQUEST_DRAIN_LIMIT: usize = 128;
const RDP_CLIENT_OUTPUT_DRAIN_LIMIT: usize = 32;
const RDP_CLIENT_OUTPUT_QUEUE_CAPACITY: usize = 64;
const RDP_GRAPHICS_DIAGNOSTICS_ENV: &str = "OXIDETERM_REMOTE_DESKTOP_DIAGNOSTICS";
const RDP_GRAPHICS_DIAGNOSTICS_REPORT_INTERVAL: Duration = Duration::from_secs(2);
// Keep the default bitmap codec configuration delegated to IronRDP. In the
// pinned version this advertises RemoteFX while still allowing raw/RDP6 bitmap
// fallback when the server does not select that codec.
const RDP_CLIENT_BITMAP_CODECS: &[&str] = &[];
// IronRDP writes BgrA32 as BGRA bytes. Using it as the decoded desktop format
// lets OxideTerm pass RDP frames to GPUI without an RGBA-to-BGRA channel swap.
const RDP_DECODED_FRAME_PIXEL_FORMAT: PixelFormat = PixelFormat::BgrA32;
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
    "该服务器只支持旧版 RDP 安全模式，当前版本不再内置 legacy RDP 引擎";

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
        writer,
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

#[derive(Debug)]
enum ClientRdpSessionExit {
    Closed,
    ReconnectRequested,
    RemoteEnded(Option<String>),
    ConnectionFailed {
        message: String,
        category: RemoteDesktopErrorCategory,
    },
}

struct ClientRdpSession {
    input_tx: tokio_mpsc::UnboundedSender<RdpInputEvent>,
    output_rx: ClientRdpOutputReceiver,
    join_handle: thread::JoinHandle<()>,
}

#[derive(Clone)]
struct ClientRdpOutputSender {
    control_tx: mpsc::Sender<ClientRdpOutput>,
    graphics_tx: ClientRdpGraphicsSender,
}

struct ClientRdpOutputReceiver {
    control_rx: mpsc::Receiver<ClientRdpOutput>,
    graphics_rx: ClientRdpGraphicsReceiver,
}

struct ClientRdpGraphicsQueue {
    queue: Mutex<VecDeque<ClientRdpOutput>>,
    capacity: usize,
    receiver_alive: AtomicBool,
}

#[derive(Clone)]
struct ClientRdpGraphicsSender {
    inner: Arc<ClientRdpGraphicsQueue>,
}

struct ClientRdpGraphicsReceiver {
    inner: Arc<ClientRdpGraphicsQueue>,
}

impl Drop for ClientRdpGraphicsReceiver {
    fn drop(&mut self) {
        self.inner.receiver_alive.store(false, Ordering::Release);
    }
}

impl ClientRdpOutputSender {
    fn send_control(
        &self,
        output: ClientRdpOutput,
    ) -> Result<(), mpsc::SendError<ClientRdpOutput>> {
        self.control_tx.send(output)
    }

    fn try_send_graphics(
        &self,
        output: ClientRdpOutput,
    ) -> Result<(), mpsc::TrySendError<ClientRdpOutput>> {
        self.graphics_tx.try_send(output)
    }
}

impl ClientRdpGraphicsSender {
    fn try_send(&self, output: ClientRdpOutput) -> Result<(), mpsc::TrySendError<ClientRdpOutput>> {
        if !self.inner.receiver_alive.load(Ordering::Acquire) {
            return Err(mpsc::TrySendError::Disconnected(output));
        }
        let Ok(mut queue) = self.inner.queue.lock() else {
            return Err(mpsc::TrySendError::Disconnected(output));
        };
        if client_rdp_output_is_base_frame(&output) {
            // A base frame supersedes every older dirty event because it
            // re-establishes the full UI backing buffer.
            queue.clear();
            queue.push_back(output);
            return Ok(());
        }
        if queue.len() >= self.inner.capacity {
            return Err(mpsc::TrySendError::Full(output));
        }
        queue.push_back(output);
        Ok(())
    }
}

impl ClientRdpGraphicsReceiver {
    fn try_recv(&self) -> Result<ClientRdpOutput, mpsc::TryRecvError> {
        let Ok(mut queue) = self.inner.queue.lock() else {
            return Err(mpsc::TryRecvError::Disconnected);
        };
        if let Some(output) = queue.pop_front() {
            return Ok(output);
        }
        if Arc::strong_count(&self.inner) == 1 {
            Err(mpsc::TryRecvError::Disconnected)
        } else {
            Err(mpsc::TryRecvError::Empty)
        }
    }
}

fn client_rdp_output_channel(capacity: usize) -> (ClientRdpOutputSender, ClientRdpOutputReceiver) {
    let (control_tx, control_rx) = mpsc::channel();
    let graphics = Arc::new(ClientRdpGraphicsQueue {
        queue: Mutex::new(VecDeque::new()),
        capacity,
        receiver_alive: AtomicBool::new(true),
    });
    (
        ClientRdpOutputSender {
            control_tx,
            graphics_tx: ClientRdpGraphicsSender {
                inner: graphics.clone(),
            },
        },
        ClientRdpOutputReceiver {
            control_rx,
            graphics_rx: ClientRdpGraphicsReceiver { inner: graphics },
        },
    )
}

fn client_rdp_output_is_base_frame(output: &ClientRdpOutput) -> bool {
    matches!(
        output,
        ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame { .. })
    )
}

fn native_rdp_desktop_ready_events(size: RemoteDesktopSize) -> [RemoteDesktopHelperEvent; 2] {
    [
        RemoteDesktopHelperEvent::Connected { size },
        RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connected,
            message: Some("RDP desktop frame received.".to_string()),
        },
    ]
}

fn unsupported_resize_connected_event(image: &DecodedImage) -> RemoteDesktopHelperEvent {
    RemoteDesktopHelperEvent::Connected {
        size: RemoteDesktopSize {
            width: u32::from(image.width()),
            height: u32::from(image.height()),
        },
    }
}

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
    RequestFrame,
    Close,
}

enum ClientRdpControlFlow {
    TerminatedGracefully(GracefulDisconnectReason),
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
mod tests;
