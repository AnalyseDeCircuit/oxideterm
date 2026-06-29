// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use des::{
    Des,
    cipher::{Block, BlockCipherEncrypt, KeyInit},
};
use oxideterm_remote_desktop::{
    RemoteDesktopCursorShape, RemoteDesktopEndpoint, RemoteDesktopErrorCategory,
    RemoteDesktopFakeBackend, RemoteDesktopFrame, RemoteDesktopFrameFormat,
    RemoteDesktopFrameUpdate, RemoteDesktopHelperEvent, RemoteDesktopHelperRequest,
    RemoteDesktopKey, RemoteDesktopKeyState, RemoteDesktopMouseButton,
    RemoteDesktopMouseButtonState, RemoteDesktopProtocol, RemoteDesktopRect, RemoteDesktopSecret,
    RemoteDesktopSessionStatus, RemoteDesktopSize, RemoteDesktopWheelDelta, read_request_line,
    run_fake_backend_stdio, write_event_line,
};
use zeroize::Zeroizing;

const VNC_PROTOCOL_VERSION_33: &[u8; 12] = b"RFB 003.003\n";
const VNC_PROTOCOL_VERSION_38: &[u8; 12] = b"RFB 003.008\n";
const VNC_SECURITY_NONE: u8 = 1;
const VNC_SECURITY_VNC_AUTH: u8 = 2;
const VNC_ENCODING_RAW: i32 = 0;
const VNC_ENCODING_COPY_RECT: i32 = 1;
const VNC_ENCODING_DESKTOP_SIZE: i32 = -223;
const VNC_ENCODING_CURSOR: i32 = -239;
const VNC_ENCODING_X_CURSOR: i32 = -240;
const VNC_BUTTON_LEFT: u8 = 1;
const VNC_BUTTON_MIDDLE: u8 = 2;
const VNC_BUTTON_RIGHT: u8 = 4;
const VNC_WHEEL_UP: u8 = 8;
const VNC_WHEEL_DOWN: u8 = 16;
const VNC_WHEEL_LEFT: u8 = 32;
const VNC_WHEEL_RIGHT: u8 = 64;
const VNC_SCROLL_STEP: f32 = 120.0;
const MAX_VNC_FRAME_BYTES: usize =
    RemoteDesktopSize::MAX_DIMENSION as usize * RemoteDesktopSize::MAX_DIMENSION as usize * 4;

type SharedEventWriter = Arc<Mutex<io::Stdout>>;
type SharedVncWriter = Arc<Mutex<TcpStream>>;

struct VncSessionConfig {
    endpoint: RemoteDesktopEndpoint,
    password: Option<RemoteDesktopSecret>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VncRequestAction {
    Continue,
    Close,
    Reconnect,
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
    let Some(first_request) = read_request_line(reader).map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let RemoteDesktopHelperRequest::Connect {
        protocol,
        endpoint,
        username: _username,
        password,
        domain: _domain,
        size: _size,
        scale_factor: _scale_factor,
        read_only,
    } = first_request
    else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "VNC helper expected an initial connect request.".to_string(),
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

    send_event(
        &writer,
        RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connecting,
            message: Some("Opening VNC session.".to_string()),
        },
    )?;

    let session_config = VncSessionConfig { endpoint, password };
    let mut connection = match VncConnection::connect(&session_config, writer.clone()) {
        Ok(connection) => connection,
        Err(error) => {
            send_event(
                &writer,
                RemoteDesktopHelperEvent::ConnectionFailure {
                    category: Some(vnc_error_category_from_message(&error)),
                    message: error,
                },
            )?;
            return Ok(());
        }
    };

    send_event(
        &writer,
        RemoteDesktopHelperEvent::Connected {
            size: RemoteDesktopSize {
                width: connection.width as u32,
                height: connection.height as u32,
            },
        },
    )?;
    connection.start_reader();
    connection.request_framebuffer_update(false)?;

    let mut input_state = VncInputState::default();
    while let Some(request) = read_request_line(reader).map_err(|error| error.to_string())? {
        match handle_real_vnc_request(
            &writer,
            &mut connection,
            &mut input_state,
            request,
            read_only,
        )? {
            VncRequestAction::Continue => {}
            VncRequestAction::Close => break,
            VncRequestAction::Reconnect => {
                send_event(
                    &writer,
                    RemoteDesktopHelperEvent::Status {
                        status: RemoteDesktopSessionStatus::Reconnecting,
                        message: Some("Reopening VNC session.".to_string()),
                    },
                )?;
                connection.shutdown();
                input_state = VncInputState::default();
                connection = match VncConnection::connect(&session_config, writer.clone()) {
                    Ok(connection) => connection,
                    Err(error) => {
                        send_event(
                            &writer,
                            RemoteDesktopHelperEvent::ConnectionFailure {
                                category: Some(vnc_error_category_from_message(&error)),
                                message: error,
                            },
                        )?;
                        break;
                    }
                };
                send_event(
                    &writer,
                    RemoteDesktopHelperEvent::Connected {
                        size: RemoteDesktopSize {
                            width: connection.width as u32,
                            height: connection.height as u32,
                        },
                    },
                )?;
                connection.start_reader();
                connection.request_framebuffer_update(false)?;
            }
        }
    }

    connection.shutdown();
    send_event(
        &writer,
        RemoteDesktopHelperEvent::Disconnected {
            reason: Some("VNC session closed.".to_string()),
        },
    )?;
    Ok(())
}

fn handle_real_vnc_request(
    event_writer: &SharedEventWriter,
    connection: &mut VncConnection,
    input_state: &mut VncInputState,
    request: RemoteDesktopHelperRequest,
    read_only: bool,
) -> Result<VncRequestAction, String> {
    match request {
        RemoteDesktopHelperRequest::Close => return Ok(VncRequestAction::Close),
        RemoteDesktopHelperRequest::Reconnect => {
            return Ok(VncRequestAction::Reconnect);
        }
        RemoteDesktopHelperRequest::Resize { .. } => {
            // RFB clients cannot resize arbitrary servers unless the server
            // advertises a resize extension. The first helper slice keeps this
            // as a no-op instead of lying about server-side support.
        }
        RemoteDesktopHelperRequest::Connect { .. } => {
            return Err("VNC helper received a second connect request.".to_string());
        }
        RemoteDesktopHelperRequest::MouseMove { x, y } if !read_only => {
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
        RemoteDesktopHelperRequest::MouseButton { button, state } if !read_only => {
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
        RemoteDesktopHelperRequest::Wheel { delta } if !read_only => {
            for mask in vnc_scroll_masks(delta) {
                connection.send_pointer(
                    input_state.pointer.x,
                    input_state.pointer.y,
                    input_state.pointer.buttons | mask,
                )?;
                connection.send_pointer(
                    input_state.pointer.x,
                    input_state.pointer.y,
                    input_state.pointer.buttons,
                )?;
            }
        }
        RemoteDesktopHelperRequest::Key { key, state } if !read_only => {
            for event in input_state.keyboard.operations(&key, state) {
                connection.send_key(event.keysym, event.down)?;
            }
        }
        RemoteDesktopHelperRequest::Text { text } if !read_only => {
            for character in text.chars().filter(|character| !character.is_control()) {
                let keysym = character as u32;
                connection.send_key(keysym, true)?;
                connection.send_key(keysym, false)?;
            }
        }
        RemoteDesktopHelperRequest::ClipboardText { text } if !read_only => {
            connection.send_client_cut_text(&text)?;
        }
        RemoteDesktopHelperRequest::ClipboardData { .. } => {
            // Baseline RFB clipboard messages are text-only. Keep binary
            // clipboard data as an RDP capability unless a VNC extension is
            // negotiated explicitly.
        }
        RemoteDesktopHelperRequest::SynchronizeLockKeys { .. } => {
            // RFB has no equivalent lock-key synchronization message in the
            // baseline protocol, so this request is RDP-only.
        }
        RemoteDesktopHelperRequest::RequestFrame => {
            // VNC framebuffer recovery is driven by framebuffer-update
            // requests in the reader loop; this explicit recovery request is
            // currently an RDP helper capability.
        }
        RemoteDesktopHelperRequest::ReleaseAllInputs if !read_only => {
            if input_state.pointer.buttons != 0 {
                input_state.pointer.buttons = 0;
                connection.send_pointer(input_state.pointer.x, input_state.pointer.y, 0)?;
            }
            for event in input_state.keyboard.release_all_events() {
                connection.send_key(event.keysym, event.down)?;
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
    buttons: u8,
}

struct VncConnection {
    writer: SharedVncWriter,
    reader: Option<TcpStream>,
    event_writer: SharedEventWriter,
    closed: Arc<AtomicBool>,
    width: u16,
    height: u16,
}

fn vnc_error_category_from_message(message: &str) -> RemoteDesktopErrorCategory {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("password") || normalized.contains("authentication") {
        RemoteDesktopErrorCategory::Authentication
    } else if normalized.contains("tcp")
        || normalized.contains("connection")
        || normalized.contains("read failed")
        || normalized.contains("write failed")
    {
        RemoteDesktopErrorCategory::Network
    } else if normalized.contains("security")
        || normalized.contains("protocol")
        || normalized.contains("unsupported")
    {
        RemoteDesktopErrorCategory::Protocol
    } else {
        RemoteDesktopErrorCategory::Unknown
    }
}

impl VncConnection {
    fn connect(config: &VncSessionConfig, event_writer: SharedEventWriter) -> Result<Self, String> {
        let mut stream = TcpStream::connect((config.endpoint.host.as_str(), config.endpoint.port))
            .map_err(|error| format!("VNC TCP connection failed: {error}"))?;
        stream
            .set_nodelay(true)
            .map_err(|error| format!("VNC TCP option setup failed: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| format!("VNC read timeout setup failed: {error}"))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(|error| format!("VNC write timeout setup failed: {error}"))?;

        handshake_vnc(&mut stream, config.password.as_ref())?;
        let (width, height) = read_server_init(&mut stream)?;
        write_pixel_format(&mut stream)?;
        write_encodings(&mut stream)?;

        let reader = stream
            .try_clone()
            .map_err(|error| format!("VNC stream clone failed: {error}"))?;
        Ok(Self {
            writer: Arc::new(Mutex::new(stream)),
            reader: Some(reader),
            event_writer,
            closed: Arc::new(AtomicBool::new(false)),
            width,
            height,
        })
    }

    fn start_reader(&mut self) {
        let Some(reader) = self.reader.take() else {
            return;
        };
        let writer = self.writer.clone();
        let event_writer = self.event_writer.clone();
        let closed = self.closed.clone();
        let width = self.width;
        let height = self.height;
        thread::Builder::new()
            .name("oxideterm-vnc-reader".to_string())
            .spawn(move || read_vnc_events(reader, writer, event_writer, closed, width, height))
            .ok();
    }

    fn request_framebuffer_update(&self, incremental: bool) -> Result<(), String> {
        request_framebuffer_update(&self.writer, incremental, self.width, self.height)
    }

    fn send_pointer(&self, x: u16, y: u16, buttons: u8) -> Result<(), String> {
        let mut message = Vec::with_capacity(6);
        message.push(5);
        message.push(buttons);
        push_be_u16(&mut message, x);
        push_be_u16(&mut message, y);
        write_vnc_message(&self.writer, &message)
    }

    fn send_key(&self, keysym: u32, down: bool) -> Result<(), String> {
        let mut message = Vec::with_capacity(8);
        message.push(4);
        message.push(u8::from(down));
        message.extend_from_slice(&[0, 0]);
        push_be_u32(&mut message, keysym);
        write_vnc_message(&self.writer, &message)
    }

    fn send_client_cut_text(&self, text: &str) -> Result<(), String> {
        let bytes = text.as_bytes();
        let len = u32::try_from(bytes.len())
            .map_err(|_| "VNC clipboard text is too large to send.".to_string())?;
        let mut message = Vec::with_capacity(8 + bytes.len());
        message.push(6);
        message.extend_from_slice(&[0, 0, 0]);
        push_be_u32(&mut message, len);
        message.extend_from_slice(bytes);
        write_vnc_message(&self.writer, &message)
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(stream) = self.writer.lock() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

fn handshake_vnc(
    stream: &mut TcpStream,
    password: Option<&RemoteDesktopSecret>,
) -> Result<(), String> {
    let server_version = read_exact_array::<12, _>(stream)
        .map_err(|error| format!("VNC protocol banner read failed: {error}"))?;
    if !server_version.starts_with(b"RFB ") {
        return Err("VNC server did not send an RFB protocol banner.".to_string());
    }

    let legacy_security = server_version.starts_with(b"RFB 003.003");
    let client_version = if legacy_security {
        VNC_PROTOCOL_VERSION_33
    } else {
        VNC_PROTOCOL_VERSION_38
    };
    stream
        .write_all(client_version)
        .map_err(|error| format!("VNC protocol banner write failed: {error}"))?;

    if legacy_security {
        negotiate_legacy_security(stream, password)
    } else {
        negotiate_modern_security(stream, password)
    }
}

fn negotiate_legacy_security(
    stream: &mut TcpStream,
    password: Option<&RemoteDesktopSecret>,
) -> Result<(), String> {
    let security_type =
        read_be_u32(stream).map_err(|error| format!("VNC security type read failed: {error}"))?;
    match security_type {
        0 => Err(read_reason(stream)
            .unwrap_or_else(|_| "VNC server rejected security negotiation.".to_string())),
        1 => write_client_init(stream),
        2 => authenticate_vnc_password(stream, password).and_then(|_| write_client_init(stream)),
        other => Err(format!("Unsupported VNC security type {other}.")),
    }
}

fn negotiate_modern_security(
    stream: &mut TcpStream,
    password: Option<&RemoteDesktopSecret>,
) -> Result<(), String> {
    let count =
        read_u8(stream).map_err(|error| format!("VNC security list read failed: {error}"))?;
    if count == 0 {
        return Err(read_reason(stream)
            .unwrap_or_else(|_| "VNC server rejected security negotiation.".to_string()));
    }

    let mut security_types = vec![0; count as usize];
    stream
        .read_exact(&mut security_types)
        .map_err(|error| format!("VNC security list read failed: {error}"))?;
    if security_types.contains(&VNC_SECURITY_NONE)
        && password.is_none_or(|secret| secret.is_empty())
    {
        stream
            .write_all(&[VNC_SECURITY_NONE])
            .map_err(|error| format!("VNC security selection failed: {error}"))?;
        let result = read_be_u32(stream)
            .map_err(|error| format!("VNC security result read failed: {error}"))?;
        if result != 0 {
            return Err(read_reason(stream)
                .unwrap_or_else(|_| "VNC security negotiation failed.".to_string()));
        }
        return write_client_init(stream);
    }

    if security_types.contains(&VNC_SECURITY_VNC_AUTH) {
        stream
            .write_all(&[VNC_SECURITY_VNC_AUTH])
            .map_err(|error| format!("VNC security selection failed: {error}"))?;
        authenticate_vnc_password(stream, password)?;
        return write_client_init(stream);
    }

    Err(format!(
        "Unsupported VNC security types: {:?}.",
        security_types
    ))
}

fn authenticate_vnc_password(
    stream: &mut TcpStream,
    password: Option<&RemoteDesktopSecret>,
) -> Result<(), String> {
    let password = password
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| "VNC server requires password authentication.".to_string())?;
    let challenge = read_exact_array::<16, _>(stream)
        .map_err(|error| format!("VNC password challenge read failed: {error}"))?;
    // VNC authentication derives a request-scoped DES key from the configured
    // password. Keep both the key and response zeroized after the handshake.
    let key = vnc_auth_key(password);
    let mut response = Zeroizing::new(challenge);
    encrypt_vnc_challenge(&key, &mut response)?;
    stream
        .write_all(response.as_slice())
        .map_err(|error| format!("VNC password response write failed: {error}"))?;
    let result =
        read_be_u32(stream).map_err(|error| format!("VNC security result read failed: {error}"))?;
    if result == 0 {
        Ok(())
    } else {
        Err(read_reason(stream)
            .unwrap_or_else(|_| "VNC password authentication failed.".to_string()))
    }
}

fn vnc_auth_key(password: &RemoteDesktopSecret) -> Zeroizing<[u8; 8]> {
    let mut key = Zeroizing::new([0u8; 8]);
    for (slot, byte) in key
        .iter_mut()
        .zip(password.expose_secret().as_bytes().iter().copied().take(8))
    {
        *slot = byte.reverse_bits();
    }
    key
}

fn encrypt_vnc_challenge(
    key: &Zeroizing<[u8; 8]>,
    response: &mut Zeroizing<[u8; 16]>,
) -> Result<(), String> {
    let cipher = Des::new_from_slice(key.as_slice())
        .map_err(|_| "VNC password cipher setup failed.".to_string())?;
    for block in response.chunks_exact_mut(8) {
        let mut cipher_block = Block::<Des>::default();
        cipher_block.copy_from_slice(block);
        cipher.encrypt_block(&mut cipher_block);
        block.copy_from_slice(&cipher_block);
    }
    Ok(())
}

fn write_client_init(stream: &mut TcpStream) -> Result<(), String> {
    // Shared mode avoids disconnecting external viewers when the server allows
    // multiple clients.
    stream
        .write_all(&[1])
        .map_err(|error| format!("VNC client init failed: {error}"))
}

fn read_server_init(stream: &mut TcpStream) -> Result<(u16, u16), String> {
    let init = read_exact_array::<24, _>(stream)
        .map_err(|error| format!("VNC init read failed: {error}"))?;
    let width = be_u16(&init[0..2]);
    let height = be_u16(&init[2..4]);
    let name_len = be_u32(&init[20..24]) as usize;
    if name_len > 0 {
        read_exact_vec(stream, name_len)
            .map_err(|error| format!("VNC desktop name read failed: {error}"))?;
    }
    Ok((width, height))
}

fn write_pixel_format(stream: &mut TcpStream) -> Result<(), String> {
    let mut message = Vec::with_capacity(20);
    message.extend_from_slice(&[0, 0, 0, 0]);
    message.extend_from_slice(&[
        32, 24, 0, 1, // 32-bit little-endian true color.
        0, 255, 0, 255, 0, 255, // color max values.
        16, 8, 0, // red, green, blue shifts => BGRA bytes.
        0, 0, 0,
    ]);
    stream
        .write_all(&message)
        .map_err(|error| format!("VNC pixel format write failed: {error}"))
}

fn write_encodings(stream: &mut TcpStream) -> Result<(), String> {
    let mut message = Vec::with_capacity(24);
    message.push(2);
    message.push(0);
    push_be_u16(&mut message, 5);
    push_be_i32(&mut message, VNC_ENCODING_DESKTOP_SIZE);
    push_be_i32(&mut message, VNC_ENCODING_CURSOR);
    push_be_i32(&mut message, VNC_ENCODING_X_CURSOR);
    push_be_i32(&mut message, VNC_ENCODING_COPY_RECT);
    push_be_i32(&mut message, VNC_ENCODING_RAW);
    stream
        .write_all(&message)
        .map_err(|error| format!("VNC encoding write failed: {error}"))
}

fn read_vnc_events(
    mut reader: TcpStream,
    writer: SharedVncWriter,
    event_writer: SharedEventWriter,
    closed: Arc<AtomicBool>,
    width: u16,
    height: u16,
) {
    let mut framebuffer = VncFramebuffer::new(width, height);
    let mut sent_initial_frame = false;
    loop {
        match read_vnc_event(&mut reader) {
            Ok(event) => {
                for helper_event in vnc_helper_events(&event) {
                    let _ = send_event(&event_writer, helper_event);
                }
                if let Some(change) = framebuffer.apply(event) {
                    let frame_event = match change {
                        VncFramebufferChange::Full => {
                            sent_initial_frame = true;
                            RemoteDesktopHelperEvent::Frame {
                                frame: framebuffer.frame(),
                            }
                        }
                        VncFramebufferChange::Rect(rect) if sent_initial_frame => {
                            if let Some(update) = framebuffer.frame_update(rect) {
                                RemoteDesktopHelperEvent::FrameUpdate { update }
                            } else {
                                sent_initial_frame = true;
                                RemoteDesktopHelperEvent::Frame {
                                    frame: framebuffer.frame(),
                                }
                            }
                        }
                        VncFramebufferChange::Rect(_) => {
                            sent_initial_frame = true;
                            RemoteDesktopHelperEvent::Frame {
                                frame: framebuffer.frame(),
                            }
                        }
                    };
                    let _ = send_event(&event_writer, frame_event);
                }
                let _ = request_framebuffer_update(
                    &writer,
                    true,
                    framebuffer.width as u16,
                    framebuffer.height as u16,
                );
            }
            Err(error) => {
                if !closed.load(Ordering::Acquire) {
                    let _ = send_event(
                        &event_writer,
                        RemoteDesktopHelperEvent::Disconnected {
                            reason: Some(error),
                        },
                    );
                }
                return;
            }
        }
    }
}

fn vnc_helper_events(event: &VncServerEvent) -> Vec<RemoteDesktopHelperEvent> {
    // Framebuffer updates can carry non-frame pseudo-rectangles, so collect
    // side-channel helper events before mutating the backing framebuffer.
    match event {
        VncServerEvent::ClipboardText(text) => {
            vec![RemoteDesktopHelperEvent::ClipboardText { text: text.clone() }]
        }
        VncServerEvent::CursorShape(shape) => {
            vec![RemoteDesktopHelperEvent::CursorShape {
                shape: shape.clone(),
            }]
        }
        VncServerEvent::CursorHidden => vec![RemoteDesktopHelperEvent::CursorHidden],
        VncServerEvent::Batch(events) => {
            let mut helper_events = Vec::new();
            for event in events {
                helper_events.extend(vnc_helper_events(event));
            }
            helper_events
        }
        _ => Vec::new(),
    }
}

fn read_vnc_event(reader: &mut TcpStream) -> Result<VncServerEvent, String> {
    let message_type =
        read_u8(reader).map_err(|error| format!("VNC server message read failed: {error}"))?;
    match message_type {
        0 => read_framebuffer_update(reader),
        1 => {
            skip_color_map_entries(reader)?;
            Ok(VncServerEvent::Noop)
        }
        2 => Ok(VncServerEvent::Noop),
        3 => read_server_cut_text(reader).map(VncServerEvent::ClipboardText),
        other => Err(format!("Unsupported VNC server message type {other}.")),
    }
}

fn read_framebuffer_update(reader: &mut TcpStream) -> Result<VncServerEvent, String> {
    let _padding =
        read_u8(reader).map_err(|error| format!("VNC framebuffer padding read failed: {error}"))?;
    let rect_count = read_be_u16(reader)
        .map_err(|error| format!("VNC framebuffer rect count read failed: {error}"))?;
    let mut events = Vec::with_capacity(rect_count as usize);

    for _ in 0..rect_count {
        let header = read_exact_array::<12, _>(reader)
            .map_err(|error| format!("VNC framebuffer rect header read failed: {error}"))?;
        let rect = RfbRect {
            x: be_u16(&header[0..2]),
            y: be_u16(&header[2..4]),
            width: be_u16(&header[4..6]),
            height: be_u16(&header[6..8]),
        };
        let encoding = be_i32(&header[8..12]);
        match encoding {
            VNC_ENCODING_RAW => {
                let byte_len = rect_byte_len(rect)?;
                let data = read_exact_vec(reader, byte_len)
                    .map_err(|error| format!("VNC raw rectangle read failed: {error}"))?;
                events.push(VncServerEvent::RawImage(rect, data));
            }
            VNC_ENCODING_COPY_RECT => {
                let source = read_exact_array::<4, _>(reader)
                    .map_err(|error| format!("VNC copy-rect source read failed: {error}"))?;
                events.push(VncServerEvent::CopyRect {
                    dst: rect,
                    src_x: be_u16(&source[0..2]),
                    src_y: be_u16(&source[2..4]),
                });
            }
            VNC_ENCODING_DESKTOP_SIZE => {
                events.push(VncServerEvent::SetResolution {
                    width: rect.width,
                    height: rect.height,
                });
            }
            VNC_ENCODING_CURSOR => {
                events.push(read_rich_cursor(reader, rect)?);
            }
            VNC_ENCODING_X_CURSOR => {
                events.push(read_x_cursor(reader, rect)?);
            }
            other => return Err(format!("Unsupported VNC rectangle encoding {other}.")),
        }
    }

    Ok(VncServerEvent::Batch(events))
}

fn skip_color_map_entries(reader: &mut TcpStream) -> Result<(), String> {
    let _padding =
        read_u8(reader).map_err(|error| format!("VNC color-map padding read failed: {error}"))?;
    let _first = read_be_u16(reader)
        .map_err(|error| format!("VNC color-map first index read failed: {error}"))?;
    let count =
        read_be_u16(reader).map_err(|error| format!("VNC color-map count read failed: {error}"))?;
    read_exact_vec(reader, count as usize * 6)
        .map(|_| ())
        .map_err(|error| format!("VNC color-map entries read failed: {error}"))
}

fn read_server_cut_text(reader: &mut TcpStream) -> Result<String, String> {
    let _padding = read_exact_array::<3, _>(reader)
        .map_err(|error| format!("VNC clipboard padding read failed: {error}"))?;
    let len = read_be_u32(reader)
        .map_err(|error| format!("VNC clipboard length read failed: {error}"))?
        as usize;
    let bytes = read_exact_vec(reader, len)
        .map_err(|error| format!("VNC clipboard text read failed: {error}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn request_framebuffer_update(
    writer: &SharedVncWriter,
    incremental: bool,
    width: u16,
    height: u16,
) -> Result<(), String> {
    let mut message = Vec::with_capacity(10);
    message.push(3);
    message.push(u8::from(incremental));
    push_be_u16(&mut message, 0);
    push_be_u16(&mut message, 0);
    push_be_u16(&mut message, width);
    push_be_u16(&mut message, height);
    write_vnc_message(writer, &message)
}

fn write_vnc_message(writer: &SharedVncWriter, message: &[u8]) -> Result<(), String> {
    let mut stream = writer
        .lock()
        .map_err(|_| "VNC writer lock is poisoned.".to_string())?;
    stream
        .write_all(message)
        .map_err(|error| format!("VNC message write failed: {error}"))
}

fn send_event(writer: &SharedEventWriter, event: RemoteDesktopHelperEvent) -> Result<(), String> {
    let mut writer = writer
        .lock()
        .map_err(|_| "VNC event writer lock is poisoned.".to_string())?;
    write_event_line(&mut *writer, &event).map_err(|error| error.to_string())
}

fn rect_byte_len(rect: RfbRect) -> Result<usize, String> {
    let pixels = usize::from(rect.width)
        .checked_mul(usize::from(rect.height))
        .ok_or_else(|| "VNC rectangle dimensions overflowed.".to_string())?;
    let bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| "VNC rectangle byte count overflowed.".to_string())?;
    if bytes > MAX_VNC_FRAME_BYTES {
        return Err("VNC rectangle is larger than the helper limit.".to_string());
    }
    Ok(bytes)
}

fn read_rich_cursor(reader: &mut TcpStream, rect: RfbRect) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }

    let byte_len = rect_byte_len(rect)?;
    let bytes = read_exact_vec(reader, byte_len)
        .map_err(|error| format!("VNC cursor pixels read failed: {error}"))?;
    let mask = read_exact_vec(reader, cursor_mask_len(rect)?)
        .map_err(|error| format!("VNC cursor mask read failed: {error}"))?;

    rich_cursor_event(rect, bytes, &mask)
}

fn read_x_cursor(reader: &mut TcpStream, rect: RfbRect) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }

    let colors = read_exact_array::<6, _>(reader)
        .map_err(|error| format!("VNC X cursor colors read failed: {error}"))?;
    let mask_len = cursor_mask_len(rect)?;
    let bitmap = read_exact_vec(reader, mask_len)
        .map_err(|error| format!("VNC X cursor bitmap read failed: {error}"))?;
    let mask = read_exact_vec(reader, mask_len)
        .map_err(|error| format!("VNC X cursor mask read failed: {error}"))?;
    x_cursor_event(rect, colors, &bitmap, &mask)
}

fn rich_cursor_event(
    rect: RfbRect,
    mut bytes: Vec<u8>,
    mask: &[u8],
) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }
    if bytes.len() < rect_byte_len(rect)? || mask.len() < cursor_mask_len(rect)? {
        return Err("VNC cursor payload is incomplete.".to_string());
    }

    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel_start = cursor_pixel_offset(rect.width, x, y)?;
            bytes[pixel_start + 3] = if cursor_mask_bit(mask, rect.width, x, y) {
                u8::MAX
            } else {
                0
            };
        }
    }

    cursor_shape_event(rect, bytes)
}

fn x_cursor_event(
    rect: RfbRect,
    colors: [u8; 6],
    bitmap: &[u8],
    mask: &[u8],
) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }
    let mask_len = cursor_mask_len(rect)?;
    if bitmap.len() < mask_len || mask.len() < mask_len {
        return Err("VNC X cursor payload is incomplete.".to_string());
    }

    let mut bytes = vec![0; rect_byte_len(rect)?];

    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel_start = cursor_pixel_offset(rect.width, x, y)?;
            if !cursor_mask_bit(mask, rect.width, x, y) {
                continue;
            }

            let color_start = if cursor_mask_bit(bitmap, rect.width, x, y) {
                0
            } else {
                3
            };
            bytes[pixel_start] = colors[color_start + 2];
            bytes[pixel_start + 1] = colors[color_start + 1];
            bytes[pixel_start + 2] = colors[color_start];
            bytes[pixel_start + 3] = u8::MAX;
        }
    }

    cursor_shape_event(rect, bytes)
}

fn cursor_shape_event(rect: RfbRect, bytes: Vec<u8>) -> Result<VncServerEvent, String> {
    // Cursor pseudo-encoding uses rect x/y as the hotspot, not screen position.
    let shape = RemoteDesktopCursorShape::new(
        RemoteDesktopSize {
            width: u32::from(rect.width),
            height: u32::from(rect.height),
        },
        u32::from(rect.x.min(rect.width.saturating_sub(1))),
        u32::from(rect.y.min(rect.height.saturating_sub(1))),
        RemoteDesktopFrameFormat::Bgra8,
        bytes,
    );
    if shape.is_complete() {
        Ok(VncServerEvent::CursorShape(shape))
    } else {
        Err("VNC cursor shape is incomplete.".to_string())
    }
}

fn cursor_mask_len(rect: RfbRect) -> Result<usize, String> {
    let row_bytes = usize::from(rect.width)
        .checked_add(7)
        .ok_or_else(|| "VNC cursor mask row length overflowed.".to_string())?
        / 8;
    row_bytes
        .checked_mul(usize::from(rect.height))
        .ok_or_else(|| "VNC cursor mask length overflowed.".to_string())
}

fn cursor_mask_bit(mask: &[u8], width: u16, x: u16, y: u16) -> bool {
    let row_bytes = (usize::from(width) + 7) / 8;
    let byte_index = usize::from(y) * row_bytes + usize::from(x) / 8;
    let bit_index = 7 - usize::from(x) % 8;
    mask.get(byte_index)
        .is_some_and(|byte| (byte & (1u8 << bit_index)) != 0)
}

fn cursor_pixel_offset(width: u16, x: u16, y: u16) -> Result<usize, String> {
    usize::from(y)
        .checked_mul(usize::from(width))
        .and_then(|row| row.checked_add(usize::from(x)))
        .and_then(|pixel| pixel.checked_mul(4))
        .ok_or_else(|| "VNC cursor pixel offset overflowed.".to_string())
}

fn read_reason(stream: &mut TcpStream) -> io::Result<String> {
    let len = read_be_u32(stream)? as usize;
    let data = read_exact_vec(stream, len)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

fn read_u8(reader: &mut impl Read) -> io::Result<u8> {
    let mut byte = [0; 1];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn read_be_u16(reader: &mut impl Read) -> io::Result<u16> {
    let bytes = read_exact_array::<2, _>(reader)?;
    Ok(be_u16(&bytes))
}

fn read_be_u32(reader: &mut impl Read) -> io::Result<u32> {
    let bytes = read_exact_array::<4, _>(reader)?;
    Ok(be_u32(&bytes))
}

fn read_exact_array<const N: usize, R: Read>(reader: &mut R) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_exact_vec(reader: &mut impl Read, len: usize) -> io::Result<Vec<u8>> {
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn be_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn be_i32(bytes: &[u8]) -> i32 {
    i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn push_be_u16(message: &mut Vec<u8>, value: u16) {
    message.extend_from_slice(&value.to_be_bytes());
}

fn push_be_u32(message: &mut Vec<u8>, value: u32) {
    message.extend_from_slice(&value.to_be_bytes());
}

fn push_be_i32(message: &mut Vec<u8>, value: i32) {
    message.extend_from_slice(&value.to_be_bytes());
}

fn clamp_u32_to_u16(value: u32) -> u16 {
    value.min(u16::MAX as u32) as u16
}

fn vnc_button_mask(button: RemoteDesktopMouseButton) -> u8 {
    match button {
        RemoteDesktopMouseButton::Left => VNC_BUTTON_LEFT,
        RemoteDesktopMouseButton::Middle => VNC_BUTTON_MIDDLE,
        RemoteDesktopMouseButton::Right => VNC_BUTTON_RIGHT,
        // The base RFB button mask only has reliable room for buttons 1-7,
        // and 6/7 are commonly used for horizontal wheel events.
        RemoteDesktopMouseButton::Back | RemoteDesktopMouseButton::Forward => 0,
    }
}

fn vnc_scroll_masks(delta: RemoteDesktopWheelDelta) -> Vec<u8> {
    let mut masks = vnc_scroll_axis_masks(delta.y, VNC_WHEEL_UP, VNC_WHEEL_DOWN);
    masks.extend(vnc_scroll_axis_masks(
        delta.x,
        VNC_WHEEL_LEFT,
        VNC_WHEEL_RIGHT,
    ));
    masks
}

fn vnc_scroll_axis_masks(delta: f32, negative_mask: u8, positive_mask: u8) -> Vec<u8> {
    if delta.abs() < f32::EPSILON {
        return Vec::new();
    }
    let steps = (delta.abs() / VNC_SCROLL_STEP).ceil().clamp(1.0, 6.0) as usize;
    let mask = if delta > 0.0 {
        positive_mask
    } else {
        negative_mask
    };
    vec![mask; steps]
}

fn vnc_keysym(key: &RemoteDesktopKey) -> Option<u32> {
    if vnc_key_code_prefers_physical_keysym(&key.code) {
        return vnc_keysym_for_normalized_code(&normalize_vnc_key_code(&key.code));
    }

    if let Some(text) = key.text.as_deref()
        && let Some(character) = text.chars().next()
        && !character.is_control()
    {
        return Some(character as u32);
    }

    let normalized = normalize_vnc_key_code(&key.code);
    if let Some(character) = single_ascii_vnc_keysym(&normalized) {
        return Some(character);
    }

    vnc_keysym_for_normalized_code(&normalized)
}

fn vnc_key_code_prefers_physical_keysym(code: &str) -> bool {
    let normalized = normalize_vnc_key_code(code);
    normalized.starts_with("numpad")
}

fn vnc_keysym_for_normalized_code(normalized: &str) -> Option<u32> {
    match normalized {
        "shift" | "shiftleft" => Some(0xffe1),
        "shiftright" => Some(0xffe2),
        "control" | "ctrl" | "controlleft" | "ctrlleft" => Some(0xffe3),
        "controlright" | "ctrlright" => Some(0xffe4),
        "alt" | "altleft" => Some(0xffe9),
        "altright" | "altgraph" | "altgr" => Some(0xffea),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" => Some(0xffeb),
        "metaright" | "superright" | "winright" => Some(0xffec),
        "space" => Some(0x20),
        "enter" | "return" => Some(0xff0d),
        "numpadenter" => Some(0xff8d),
        "tab" => Some(0xff09),
        "escape" | "esc" => Some(0xff1b),
        "backspace" => Some(0xff08),
        "delete" => Some(0xffff),
        "insert" => Some(0xff63),
        "arrowleft" | "left" => Some(0xff51),
        "arrowup" | "up" => Some(0xff52),
        "arrowright" | "right" => Some(0xff53),
        "arrowdown" | "down" => Some(0xff54),
        "pageup" => Some(0xff55),
        "pagedown" => Some(0xff56),
        "home" => Some(0xff50),
        "end" => Some(0xff57),
        "capslock" | "caps_lock" => Some(0xffe5),
        "numlock" | "num_lock" => Some(0xff7f),
        "scrolllock" | "scroll_lock" => Some(0xff14),
        "pause" | "break" => Some(0xff13),
        "printscreen" | "print" | "snapshot" => Some(0xff61),
        "contextmenu" | "context_menu" | "menu" | "apps" => Some(0xff67),
        "numpad0" | "numpadinsert" => Some(0xffb0),
        "numpad1" | "numpadend" => Some(0xffb1),
        "numpad2" | "numpaddown" => Some(0xffb2),
        "numpad3" | "numpadpagedown" => Some(0xffb3),
        "numpad4" | "numpadleft" => Some(0xffb4),
        "numpad5" | "numpadclear" => Some(0xffb5),
        "numpad6" | "numpadright" => Some(0xffb6),
        "numpad7" | "numpadhome" => Some(0xffb7),
        "numpad8" | "numpadup" => Some(0xffb8),
        "numpad9" | "numpadpageup" => Some(0xffb9),
        "numpaddecimal" | "numpaddelete" => Some(0xffae),
        "numpadadd" => Some(0xffab),
        "numpadsubtract" => Some(0xffad),
        "numpadmultiply" => Some(0xffaa),
        "numpaddivide" => Some(0xffaf),
        "numpadequal" => Some(0xffbd),
        "f1" => Some(0xffbe),
        "f2" => Some(0xffbf),
        "f3" => Some(0xffc0),
        "f4" => Some(0xffc1),
        "f5" => Some(0xffc2),
        "f6" => Some(0xffc3),
        "f7" => Some(0xffc4),
        "f8" => Some(0xffc5),
        "f9" => Some(0xffc6),
        "f10" => Some(0xffc7),
        "f11" => Some(0xffc8),
        "f12" => Some(0xffc9),
        _ => None,
    }
}

fn normalize_vnc_key_code(code: &str) -> String {
    let normalized = code.trim().to_ascii_lowercase();
    if let Some(letter) = normalized.strip_prefix("key")
        && letter.len() == 1
        && letter.as_bytes()[0].is_ascii_lowercase()
    {
        return letter.to_string();
    }
    if let Some(digit) = normalized.strip_prefix("digit")
        && digit.len() == 1
        && digit.as_bytes()[0].is_ascii_digit()
    {
        return digit.to_string();
    }
    // GPUI normally sends compact names, but platform bridges can use browser-
    // style or toolkit-style names. Normalize them before keysym lookup.
    match normalized.as_str() {
        "enterkey" | "returnkey" | "newline" | "linefeed" | "carriagereturn" => "enter".to_string(),
        "keypadenter" | "keypad_enter" | "kpenter" | "kp_enter" | "num_enter" | "numpad_enter" => {
            "numpadenter".to_string()
        }
        _ => normalized,
    }
}

fn single_ascii_vnc_keysym(code: &str) -> Option<u32> {
    let mut chars = code.chars();
    let character = chars.next()?;
    if chars.next().is_none() && character.is_ascii_graphic() {
        Some(character as u32)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VncKeyEvent {
    keysym: u32,
    down: bool,
}

#[derive(Default)]
struct VncKeyboardInputMapper {
    physical_modifiers: HashSet<u32>,
    pressed_keysyms: HashSet<u32>,
    synthetic_modifiers_by_key: HashMap<u32, Vec<u32>>,
}

impl VncKeyboardInputMapper {
    fn operations(
        &mut self,
        key: &RemoteDesktopKey,
        state: RemoteDesktopKeyState,
    ) -> Vec<VncKeyEvent> {
        let Some(keysym) = vnc_keysym(key) else {
            return Vec::new();
        };
        if vnc_modifier_keysym_for_code(&key.code).is_some() {
            return self.modifier_events(keysym, state);
        }
        match state {
            RemoteDesktopKeyState::Pressed => {
                let synthetic_modifiers = vnc_modifier_keysyms(key)
                    .into_iter()
                    .filter(|modifier| !self.physical_modifier_equivalent_pressed(*modifier))
                    .collect::<Vec<_>>();
                let mut events = synthetic_modifiers
                    .iter()
                    .copied()
                    .map(|keysym| VncKeyEvent { keysym, down: true })
                    .collect::<Vec<_>>();
                events.push(VncKeyEvent { keysym, down: true });
                self.pressed_keysyms.insert(keysym);
                if !synthetic_modifiers.is_empty() {
                    // VNC does not have a client-side input database like
                    // IronRDP's primary path, so keep ownership for synthesized
                    // modifier presses locally and release only those later.
                    self.synthetic_modifiers_by_key
                        .insert(keysym, synthetic_modifiers);
                }
                events
            }
            RemoteDesktopKeyState::Released => {
                let mut events = vec![VncKeyEvent {
                    keysym,
                    down: false,
                }];
                self.pressed_keysyms.remove(&keysym);
                if let Some(mut modifiers) = self.synthetic_modifiers_by_key.remove(&keysym) {
                    modifiers.reverse();
                    events.extend(modifiers.into_iter().map(|keysym| VncKeyEvent {
                        keysym,
                        down: false,
                    }));
                }
                events
            }
        }
    }

    fn release_all_events(&mut self) -> Vec<VncKeyEvent> {
        let mut events = self
            .pressed_keysyms
            .drain()
            .map(|keysym| VncKeyEvent {
                keysym,
                down: false,
            })
            .collect::<Vec<_>>();
        let mut released_synthetic_modifiers = HashSet::new();
        for modifier in self
            .synthetic_modifiers_by_key
            .drain()
            .flat_map(|(_, modifiers)| modifiers)
        {
            if released_synthetic_modifiers.insert(modifier) {
                events.push(VncKeyEvent {
                    keysym: modifier,
                    down: false,
                });
            }
        }
        self.physical_modifiers.clear();
        events
    }

    fn modifier_events(&mut self, keysym: u32, state: RemoteDesktopKeyState) -> Vec<VncKeyEvent> {
        match state {
            RemoteDesktopKeyState::Pressed => {
                self.physical_modifiers.insert(keysym);
                self.pressed_keysyms.insert(keysym);
                vec![VncKeyEvent { keysym, down: true }]
            }
            RemoteDesktopKeyState::Released => {
                self.physical_modifiers.remove(&keysym);
                self.pressed_keysyms.remove(&keysym);
                vec![VncKeyEvent {
                    keysym,
                    down: false,
                }]
            }
        }
    }

    fn physical_modifier_equivalent_pressed(&self, modifier: u32) -> bool {
        self.physical_modifiers
            .iter()
            .any(|pressed| vnc_modifier_equivalent(*pressed, modifier))
    }
}

fn vnc_modifier_equivalent(left: u32, right: u32) -> bool {
    match (left, right) {
        (0xffe1 | 0xffe2, 0xffe1 | 0xffe2) => true,
        (0xffe3 | 0xffe4, 0xffe3 | 0xffe4) => true,
        (0xffe9 | 0xffea, 0xffe9 | 0xffea) => true,
        (0xffeb | 0xffec, 0xffeb | 0xffec) => true,
        _ => left == right,
    }
}

#[cfg(test)]
fn vnc_key_events(key: &RemoteDesktopKey, state: RemoteDesktopKeyState) -> Vec<VncKeyEvent> {
    let Some(keysym) = vnc_keysym(key) else {
        return Vec::new();
    };
    let modifiers = vnc_modifier_keysyms(key);
    match state {
        RemoteDesktopKeyState::Pressed => modifiers
            .iter()
            .copied()
            .map(|keysym| VncKeyEvent { keysym, down: true })
            .chain([VncKeyEvent { keysym, down: true }])
            .collect(),
        RemoteDesktopKeyState::Released => [VncKeyEvent {
            keysym,
            down: false,
        }]
        .into_iter()
        .chain(modifiers.into_iter().rev().map(|keysym| VncKeyEvent {
            keysym,
            down: false,
        }))
        .collect(),
    }
}

fn vnc_modifier_keysyms(key: &RemoteDesktopKey) -> Vec<u32> {
    let current = vnc_modifier_keysym_for_code(&key.code);
    let mut modifiers = Vec::with_capacity(4);
    if key.ctrl && current != Some(0xffe3) {
        modifiers.push(0xffe3);
    }
    if key.shift && current != Some(0xffe1) {
        modifiers.push(0xffe1);
    }
    if key.alt && current != Some(0xffe9) {
        modifiers.push(0xffe9);
    }
    if key.meta && current != Some(0xffeb) {
        modifiers.push(0xffeb);
    }
    modifiers
}

fn vnc_modifier_keysym_for_code(code: &str) -> Option<u32> {
    match normalize_vnc_key_code(code).as_str() {
        "shift" | "shiftleft" => Some(0xffe1),
        "shiftright" => Some(0xffe2),
        "control" | "ctrl" | "controlleft" | "ctrlleft" => Some(0xffe3),
        "controlright" | "ctrlright" => Some(0xffe4),
        "alt" | "altleft" => Some(0xffe9),
        "altright" | "altgraph" | "altgr" => Some(0xffea),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" => Some(0xffeb),
        "metaright" | "superright" | "winright" => Some(0xffec),
        _ => None,
    }
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
    RawImage(RfbRect, Vec<u8>),
    CopyRect {
        dst: RfbRect,
        src_x: u16,
        src_y: u16,
    },
    ClipboardText(String),
    CursorShape(RemoteDesktopCursorShape),
    CursorHidden,
    Batch(Vec<VncServerEvent>),
    Noop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VncFramebufferChange {
    Full,
    Rect(RfbRect),
}

struct VncFramebuffer {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

impl VncFramebuffer {
    fn new(width: u16, height: u16) -> Self {
        let width = width as u32;
        let height = height as u32;
        Self {
            width,
            height,
            bgra: opaque_bgra_buffer(width, height),
        }
    }

    fn apply(&mut self, event: VncServerEvent) -> Option<VncFramebufferChange> {
        match event {
            VncServerEvent::SetResolution { width, height } => {
                self.width = width as u32;
                self.height = height as u32;
                self.bgra = opaque_bgra_buffer(self.width, self.height);
                Some(VncFramebufferChange::Full)
            }
            VncServerEvent::RawImage(rect, data) => self.draw_rect(rect, &data),
            VncServerEvent::CopyRect { dst, src_x, src_y } => self.copy_rect(dst, src_x, src_y),
            VncServerEvent::Batch(events) => {
                let mut change = None;
                for event in events {
                    change = merge_vnc_framebuffer_change(change, self.apply(event));
                }
                change
            }
            VncServerEvent::ClipboardText(_)
            | VncServerEvent::CursorShape(_)
            | VncServerEvent::CursorHidden
            | VncServerEvent::Noop => None,
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
        Some(RemoteDesktopFrameUpdate::new(
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
        ))
    }

    fn draw_rect(&mut self, rect: RfbRect, data: &[u8]) -> Option<VncFramebufferChange> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let clipped = self.clipped_rect(rect)?;
        let needed = rect.width as usize * rect.height as usize * 4;
        if data.len() < needed {
            return None;
        }

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
            set_bgra_alpha_opaque(dst_row);
        }
        Some(VncFramebufferChange::Rect(clipped))
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
        Some(VncFramebufferChange::Rect(RfbRect {
            x: dst.x,
            y: dst.y,
            width: copy_w as u16,
            height: copy_h as u16,
        }))
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
}

fn opaque_bgra_buffer(width: u32, height: u32) -> Vec<u8> {
    let len = width as usize * height as usize * 4;
    let mut bytes = vec![0; len];
    set_bgra_alpha_opaque(&mut bytes);
    bytes
}

fn set_bgra_alpha_opaque(bytes: &mut [u8]) {
    for pixel in bytes.chunks_exact_mut(4) {
        // VNC requests 32-bit/24-depth true color, so the fourth byte is
        // transport padding rather than framebuffer transparency.
        pixel[3] = 0xff;
    }
}

fn merge_vnc_framebuffer_change(
    existing: Option<VncFramebufferChange>,
    incoming: Option<VncFramebufferChange>,
) -> Option<VncFramebufferChange> {
    match (existing, incoming) {
        (Some(VncFramebufferChange::Full), _) | (_, Some(VncFramebufferChange::Full)) => {
            Some(VncFramebufferChange::Full)
        }
        (Some(VncFramebufferChange::Rect(left)), Some(VncFramebufferChange::Rect(right))) => {
            union_rfb_rect(left, right).map(VncFramebufferChange::Rect)
        }
        (Some(change), None) | (None, Some(change)) => Some(change),
        (None, None) => None,
    }
}

fn union_rfb_rect(left: RfbRect, right: RfbRect) -> Option<RfbRect> {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = u32::from(left.x)
        .checked_add(u32::from(left.width))?
        .max(u32::from(right.x).checked_add(u32::from(right.width))?);
    let bottom_edge = u32::from(left.y)
        .checked_add(u32::from(left.height))?
        .max(u32::from(right.y).checked_add(u32::from(right.height))?);
    Some(RfbRect {
        x,
        y,
        width: right_edge.checked_sub(u32::from(x))?.min(u16::MAX as u32) as u16,
        height: bottom_edge.checked_sub(u32::from(y))?.min(u16::MAX as u32) as u16,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_draws_bgra_rect() {
        let mut framebuffer = VncFramebuffer::new(2, 2);
        let rect = RfbRect {
            x: 1,
            y: 0,
            width: 1,
            height: 2,
        };

        assert_eq!(
            framebuffer.apply(VncServerEvent::RawImage(
                rect,
                vec![1, 2, 3, 255, 4, 5, 6, 255],
            )),
            Some(VncFramebufferChange::Rect(rect))
        );

        assert_eq!(
            framebuffer.frame().bytes,
            vec![0, 0, 0, 255, 1, 2, 3, 255, 0, 0, 0, 255, 4, 5, 6, 255]
        );
    }

    #[test]
    fn framebuffer_treats_raw_padding_as_opaque_alpha() {
        let mut framebuffer = VncFramebuffer::new(1, 1);
        let rect = RfbRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };

        let _ = framebuffer.apply(VncServerEvent::RawImage(rect, vec![1, 2, 3, 0]));

        assert_eq!(framebuffer.frame().bytes, vec![1, 2, 3, 255]);
        assert_eq!(
            framebuffer.frame_update(rect).unwrap().bytes,
            vec![1, 2, 3, 255]
        );
    }

    #[test]
    fn framebuffer_copies_rect_without_overlapping_corruption() {
        let mut framebuffer = VncFramebuffer::new(3, 1);
        let _ = framebuffer.apply(VncServerEvent::RawImage(
            RfbRect {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255],
        ));
        let dst = RfbRect {
            x: 1,
            y: 0,
            width: 2,
            height: 1,
        };

        assert_eq!(
            framebuffer.apply(VncServerEvent::CopyRect {
                dst,
                src_x: 0,
                src_y: 0,
            }),
            Some(VncFramebufferChange::Rect(dst))
        );

        assert_eq!(
            framebuffer.frame().bytes,
            vec![1, 0, 0, 255, 1, 0, 0, 255, 2, 0, 0, 255]
        );
    }

    #[test]
    fn framebuffer_update_contains_only_changed_rect() {
        let mut framebuffer = VncFramebuffer::new(3, 2);
        let rect = RfbRect {
            x: 1,
            y: 1,
            width: 2,
            height: 1,
        };

        assert_eq!(
            framebuffer.apply(VncServerEvent::RawImage(
                rect,
                vec![7, 8, 9, 255, 10, 11, 12, 255],
            )),
            Some(VncFramebufferChange::Rect(rect))
        );

        let update = framebuffer.frame_update(rect).unwrap();
        assert_eq!(
            update.size,
            RemoteDesktopSize {
                width: 3,
                height: 2,
            }
        );
        assert_eq!(update.rect, RemoteDesktopRect::new(1, 1, 2, 1));
        assert_eq!(update.bytes, vec![7, 8, 9, 255, 10, 11, 12, 255]);
    }

    #[test]
    fn rich_cursor_applies_visibility_mask_to_alpha() {
        let event = rich_cursor_event(
            RfbRect {
                x: 1,
                y: 0,
                width: 2,
                height: 1,
            },
            vec![10, 20, 30, 0, 40, 50, 60, 0],
            &[0b1000_0000],
        )
        .unwrap();

        let VncServerEvent::CursorShape(shape) = event else {
            panic!("expected cursor shape");
        };
        assert_eq!(
            shape,
            RemoteDesktopCursorShape::new(
                RemoteDesktopSize {
                    width: 2,
                    height: 1,
                },
                1,
                0,
                RemoteDesktopFrameFormat::Bgra8,
                vec![10, 20, 30, 255, 40, 50, 60, 0],
            )
        );
    }

    #[test]
    fn x_cursor_expands_bitmap_and_mask_to_bgra_pixels() {
        let event = x_cursor_event(
            RfbRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            [0x30, 0x20, 0x10, 0x03, 0x02, 0x01],
            &[0b1000_0000],
            &[0b1100_0000],
        )
        .unwrap();

        let VncServerEvent::CursorShape(shape) = event else {
            panic!("expected cursor shape");
        };
        assert_eq!(
            shape.bytes,
            vec![0x10, 0x20, 0x30, 255, 0x01, 0x02, 0x03, 255]
        );
        assert_eq!(shape.hotspot_x, 0);
        assert_eq!(shape.hotspot_y, 0);
    }

    #[test]
    fn batch_exposes_nested_cursor_helper_events() {
        let shape = RemoteDesktopCursorShape::new(
            RemoteDesktopSize {
                width: 1,
                height: 1,
            },
            0,
            0,
            RemoteDesktopFrameFormat::Bgra8,
            vec![1, 2, 3, 255],
        );

        assert_eq!(
            vnc_helper_events(&VncServerEvent::Batch(vec![
                VncServerEvent::ClipboardText("copied".to_string()),
                VncServerEvent::CursorShape(shape.clone()),
                VncServerEvent::CursorHidden,
            ])),
            vec![
                RemoteDesktopHelperEvent::ClipboardText {
                    text: "copied".to_string(),
                },
                RemoteDesktopHelperEvent::CursorShape { shape },
                RemoteDesktopHelperEvent::CursorHidden,
            ]
        );
    }

    #[test]
    fn key_mapping_prefers_printable_text() {
        let key = RemoteDesktopKey {
            code: "KeyA".to_string(),
            text: Some("a".to_string()),
            alt: false,
            ctrl: false,
            shift: false,
            meta: false,
        };

        assert_eq!(vnc_keysym(&key), Some('a' as u32));
    }

    #[test]
    fn key_mapping_accepts_physical_code_without_text() {
        let key = RemoteDesktopKey {
            code: "KeyV".to_string(),
            text: None,
            alt: false,
            ctrl: true,
            shift: false,
            meta: false,
        };

        assert_eq!(vnc_keysym(&key), Some('v' as u32));
    }

    #[test]
    fn key_mapping_prefers_keypad_keysym_over_printable_text() {
        let key = RemoteDesktopKey {
            code: "Numpad1".to_string(),
            text: Some("1".to_string()),
            alt: false,
            ctrl: false,
            shift: false,
            meta: false,
        };

        assert_eq!(vnc_keysym(&key), Some(0xffb1));
    }

    #[test]
    fn key_mapping_accepts_desktop_special_keys() {
        let cases = [
            ("Return", 0xff0d),
            ("EnterKey", 0xff0d),
            ("NumpadEnter", 0xff8d),
            ("KP_Enter", 0xff8d),
            ("NumpadDivide", 0xffaf),
            ("Insert", 0xff63),
            ("ContextMenu", 0xff67),
            ("PrintScreen", 0xff61),
            ("NumLock", 0xff7f),
            ("ScrollLock", 0xff14),
            ("Pause", 0xff13),
        ];

        for (code, expected) in cases {
            let key = RemoteDesktopKey {
                code: code.to_string(),
                text: None,
                alt: false,
                ctrl: false,
                shift: false,
                meta: false,
            };
            assert_eq!(vnc_keysym(&key), Some(expected), "code {code}");
        }
    }

    #[test]
    fn key_events_wrap_modified_shortcut() {
        let key = RemoteDesktopKey {
            code: "KeyC".to_string(),
            text: Some("c".to_string()),
            alt: false,
            ctrl: true,
            shift: false,
            meta: false,
        };

        assert_eq!(
            vnc_key_events(&key, RemoteDesktopKeyState::Pressed),
            vec![
                VncKeyEvent {
                    keysym: 0xffe3,
                    down: true,
                },
                VncKeyEvent {
                    keysym: 'c' as u32,
                    down: true,
                },
            ]
        );
        assert_eq!(
            vnc_key_events(&key, RemoteDesktopKeyState::Released),
            vec![
                VncKeyEvent {
                    keysym: 'c' as u32,
                    down: false,
                },
                VncKeyEvent {
                    keysym: 0xffe3,
                    down: false,
                },
            ]
        );
    }

    #[test]
    fn keyboard_mapper_keeps_physical_modifier_pressed_until_release() {
        let mut mapper = VncKeyboardInputMapper::default();
        let control = RemoteDesktopKey {
            code: "ControlRight".to_string(),
            text: None,
            alt: false,
            ctrl: true,
            shift: false,
            meta: false,
        };
        let shortcut = RemoteDesktopKey {
            code: "KeyV".to_string(),
            text: Some("v".to_string()),
            alt: false,
            ctrl: true,
            shift: false,
            meta: false,
        };

        assert_eq!(
            mapper.operations(&control, RemoteDesktopKeyState::Pressed),
            vec![VncKeyEvent {
                keysym: 0xffe4,
                down: true,
            }]
        );
        assert_eq!(
            mapper.operations(&shortcut, RemoteDesktopKeyState::Pressed),
            vec![VncKeyEvent {
                keysym: 'v' as u32,
                down: true,
            }]
        );
        assert_eq!(
            mapper.operations(&shortcut, RemoteDesktopKeyState::Released),
            vec![VncKeyEvent {
                keysym: 'v' as u32,
                down: false,
            }]
        );
    }

    #[test]
    fn keyboard_mapper_release_all_releases_tracked_inputs() {
        let mut mapper = VncKeyboardInputMapper::default();
        let control = RemoteDesktopKey {
            code: "ControlLeft".to_string(),
            text: None,
            alt: false,
            ctrl: true,
            shift: false,
            meta: false,
        };
        let key = RemoteDesktopKey {
            code: "KeyA".to_string(),
            text: Some("a".to_string()),
            alt: false,
            ctrl: false,
            shift: false,
            meta: false,
        };

        let _ = mapper.operations(&control, RemoteDesktopKeyState::Pressed);
        let _ = mapper.operations(&key, RemoteDesktopKeyState::Pressed);
        let released = mapper.release_all_events();

        assert!(released.contains(&VncKeyEvent {
            keysym: 0xffe3,
            down: false,
        }));
        assert!(released.contains(&VncKeyEvent {
            keysym: 'a' as u32,
            down: false,
        }));
        assert!(mapper.release_all_events().is_empty());
    }

    #[test]
    fn scroll_masks_include_horizontal_wheel_buttons() {
        assert_eq!(
            vnc_scroll_masks(RemoteDesktopWheelDelta {
                x: 120.0,
                y: -240.0
            }),
            vec![VNC_WHEEL_UP, VNC_WHEEL_UP, VNC_WHEEL_RIGHT]
        );
        assert_eq!(
            vnc_scroll_masks(RemoteDesktopWheelDelta { x: -1.0, y: 0.0 }),
            vec![VNC_WHEEL_LEFT]
        );
    }

    #[test]
    fn vnc_error_category_identifies_authentication_and_network_errors() {
        assert_eq!(
            vnc_error_category_from_message("VNC password authentication failed."),
            RemoteDesktopErrorCategory::Authentication
        );
        assert_eq!(
            vnc_error_category_from_message("VNC TCP connection failed: refused"),
            RemoteDesktopErrorCategory::Network
        );
    }

    #[test]
    fn vnc_auth_key_reverses_bits_and_truncates_password() {
        let secret = RemoteDesktopSecret::from("abcdefghijk");

        assert_eq!(
            vnc_auth_key(&secret).as_slice(),
            &[0x86, 0x46, 0xc6, 0x26, 0xa6, 0x66, 0xe6, 0x16]
        );
    }
}
