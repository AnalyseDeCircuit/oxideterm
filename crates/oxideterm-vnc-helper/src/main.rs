// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    io::{self, BufRead, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use oxideterm_remote_desktop::{
    RemoteDesktopEndpoint, RemoteDesktopFakeBackend, RemoteDesktopFrame, RemoteDesktopFrameFormat,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState,
    RemoteDesktopMouseButton, RemoteDesktopMouseButtonState, RemoteDesktopProtocol,
    RemoteDesktopSecret, RemoteDesktopSessionStatus, RemoteDesktopSize, read_request_line,
    run_fake_backend_stdio, write_event_line,
};

const VNC_PROTOCOL_VERSION_33: &[u8; 12] = b"RFB 003.003\n";
const VNC_PROTOCOL_VERSION_38: &[u8; 12] = b"RFB 003.008\n";
const VNC_SECURITY_NONE: u8 = 1;
const VNC_SECURITY_VNC_AUTH: u8 = 2;
const VNC_ENCODING_RAW: i32 = 0;
const VNC_ENCODING_COPY_RECT: i32 = 1;
const VNC_ENCODING_DESKTOP_SIZE: i32 = -223;
const VNC_BUTTON_LEFT: u8 = 1;
const VNC_BUTTON_MIDDLE: u8 = 2;
const VNC_BUTTON_RIGHT: u8 = 4;
const VNC_WHEEL_UP: u8 = 8;
const VNC_WHEEL_DOWN: u8 = 16;
const VNC_SCROLL_STEP: f32 = 120.0;
const MAX_VNC_FRAME_BYTES: usize =
    RemoteDesktopSize::MAX_DIMENSION as usize * RemoteDesktopSize::MAX_DIMENSION as usize * 4;

type SharedEventWriter = Arc<Mutex<io::Stdout>>;
type SharedVncWriter = Arc<Mutex<TcpStream>>;

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
        read_only,
    } = first_request
    else {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "VNC helper expected an initial connect request.".to_string(),
            },
        )?;
        return Ok(());
    };

    if protocol != RemoteDesktopProtocol::Vnc {
        send_event(
            &writer,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "VNC helper received a non-VNC connect request.".to_string(),
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

    let mut connection = match VncConnection::connect(endpoint, password, writer.clone()) {
        Ok(connection) => connection,
        Err(error) => {
            send_event(
                &writer,
                RemoteDesktopHelperEvent::ConnectionFailure { message: error },
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

    let mut pointer = VncPointerState::default();
    while let Some(request) = read_request_line(reader).map_err(|error| error.to_string())? {
        if handle_real_vnc_request(&mut connection, &mut pointer, request, read_only)? {
            break;
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
    connection: &mut VncConnection,
    pointer: &mut VncPointerState,
    request: RemoteDesktopHelperRequest,
    read_only: bool,
) -> Result<bool, String> {
    match request {
        RemoteDesktopHelperRequest::Close => return Ok(true),
        RemoteDesktopHelperRequest::Reconnect => {
            return Err("VNC reconnect is not implemented in the helper yet.".to_string());
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
            pointer.x = clamp_u32_to_u16(x);
            pointer.y = clamp_u32_to_u16(y);
            connection.send_pointer(pointer.x, pointer.y, pointer.buttons)?;
        }
        RemoteDesktopHelperRequest::MouseButton { button, state } if !read_only => {
            let mask = vnc_button_mask(button);
            match state {
                RemoteDesktopMouseButtonState::Pressed => pointer.buttons |= mask,
                RemoteDesktopMouseButtonState::Released => pointer.buttons &= !mask,
            }
            connection.send_pointer(pointer.x, pointer.y, pointer.buttons)?;
        }
        RemoteDesktopHelperRequest::Wheel { delta } if !read_only => {
            for mask in vnc_scroll_masks(delta.y) {
                connection.send_pointer(pointer.x, pointer.y, pointer.buttons | mask)?;
                connection.send_pointer(pointer.x, pointer.y, pointer.buttons)?;
            }
        }
        RemoteDesktopHelperRequest::Key { key, state } if !read_only => {
            if let Some(keysym) = vnc_keysym(&key) {
                connection.send_key(keysym, matches!(state, RemoteDesktopKeyState::Pressed))?;
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
        _ => {}
    }

    Ok(false)
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
    width: u16,
    height: u16,
}

impl VncConnection {
    fn connect(
        endpoint: RemoteDesktopEndpoint,
        password: Option<RemoteDesktopSecret>,
        event_writer: SharedEventWriter,
    ) -> Result<Self, String> {
        let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
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

        handshake_vnc(&mut stream, password)?;
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
        let width = self.width;
        let height = self.height;
        thread::Builder::new()
            .name("oxideterm-vnc-reader".to_string())
            .spawn(move || read_vnc_events(reader, writer, event_writer, width, height))
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
        if let Ok(stream) = self.writer.lock() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

fn handshake_vnc(
    stream: &mut TcpStream,
    password: Option<RemoteDesktopSecret>,
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
    password: Option<RemoteDesktopSecret>,
) -> Result<(), String> {
    let security_type =
        read_be_u32(stream).map_err(|error| format!("VNC security type read failed: {error}"))?;
    match security_type {
        0 => Err(read_reason(stream)
            .unwrap_or_else(|_| "VNC server rejected security negotiation.".to_string())),
        1 => {
            drop(password);
            write_client_init(stream)
        }
        2 => Err("VNC password authentication is not implemented yet.".to_string()),
        other => Err(format!("Unsupported VNC security type {other}.")),
    }
}

fn negotiate_modern_security(
    stream: &mut TcpStream,
    password: Option<RemoteDesktopSecret>,
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
    if security_types.contains(&VNC_SECURITY_NONE) {
        drop(password);
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
        return Err("VNC password authentication is not implemented yet.".to_string());
    }

    Err(format!(
        "Unsupported VNC security types: {:?}.",
        security_types
    ))
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
    let mut message = Vec::with_capacity(16);
    message.push(2);
    message.push(0);
    push_be_u16(&mut message, 3);
    push_be_i32(&mut message, VNC_ENCODING_DESKTOP_SIZE);
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
    width: u16,
    height: u16,
) {
    let mut framebuffer = VncFramebuffer::new(width, height);
    loop {
        match read_vnc_event(&mut reader) {
            Ok(event) => {
                if let VncServerEvent::ClipboardText(text) = &event {
                    let _ = send_event(
                        &event_writer,
                        RemoteDesktopHelperEvent::ClipboardText { text: text.clone() },
                    );
                }
                if framebuffer.apply(event) {
                    let _ = send_event(
                        &event_writer,
                        RemoteDesktopHelperEvent::Frame {
                            frame: framebuffer.frame(),
                        },
                    );
                }
                let _ = request_framebuffer_update(
                    &writer,
                    true,
                    framebuffer.width as u16,
                    framebuffer.height as u16,
                );
            }
            Err(error) => {
                let _ = send_event(
                    &event_writer,
                    RemoteDesktopHelperEvent::Disconnected {
                        reason: Some(error),
                    },
                );
                return;
            }
        }
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
        RemoteDesktopMouseButton::Back | RemoteDesktopMouseButton::Forward => 0,
    }
}

fn vnc_scroll_masks(delta_y: f32) -> Vec<u8> {
    if delta_y.abs() < f32::EPSILON {
        return Vec::new();
    }
    let steps = (delta_y.abs() / VNC_SCROLL_STEP).ceil().clamp(1.0, 6.0) as usize;
    let mask = if delta_y > 0.0 {
        VNC_WHEEL_DOWN
    } else {
        VNC_WHEEL_UP
    };
    vec![mask; steps]
}

fn vnc_keysym(key: &RemoteDesktopKey) -> Option<u32> {
    if let Some(text) = key.text.as_deref()
        && let Some(character) = text.chars().next()
        && !character.is_control()
    {
        return Some(character as u32);
    }

    match key.code.to_ascii_lowercase().as_str() {
        "space" => Some(0x20),
        "enter" | "numpadenter" => Some(0xff0d),
        "tab" => Some(0xff09),
        "escape" | "esc" => Some(0xff1b),
        "backspace" => Some(0xff08),
        "delete" => Some(0xffff),
        "arrowleft" | "left" => Some(0xff51),
        "arrowup" | "up" => Some(0xff52),
        "arrowright" | "right" => Some(0xff53),
        "arrowdown" | "down" => Some(0xff54),
        "pageup" => Some(0xff55),
        "pagedown" => Some(0xff56),
        "home" => Some(0xff50),
        "end" => Some(0xff57),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RfbRect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

#[derive(Debug)]
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
    Batch(Vec<VncServerEvent>),
    Noop,
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
            bgra: vec![0; width as usize * height as usize * 4],
        }
    }

    fn apply(&mut self, event: VncServerEvent) -> bool {
        match event {
            VncServerEvent::SetResolution { width, height } => {
                self.width = width as u32;
                self.height = height as u32;
                self.bgra = vec![0; self.width as usize * self.height as usize * 4];
                true
            }
            VncServerEvent::RawImage(rect, data) => self.draw_rect(rect, &data),
            VncServerEvent::CopyRect { dst, src_x, src_y } => self.copy_rect(dst, src_x, src_y),
            VncServerEvent::Batch(events) => {
                let mut changed = false;
                for event in events {
                    changed |= self.apply(event);
                }
                changed
            }
            VncServerEvent::ClipboardText(_) | VncServerEvent::Noop => false,
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

    fn draw_rect(&mut self, rect: RfbRect, data: &[u8]) -> bool {
        if self.width == 0 || self.height == 0 {
            return false;
        }
        let rect_x = rect.x as u32;
        let rect_y = rect.y as u32;
        let rect_w = rect.width as u32;
        let rect_h = rect.height as u32;
        if rect_x >= self.width || rect_y >= self.height || rect_w == 0 || rect_h == 0 {
            return false;
        }
        let copy_w = rect_w.min(self.width - rect_x);
        let copy_h = rect_h.min(self.height - rect_y);
        let needed = rect_w as usize * rect_h as usize * 4;
        if data.len() < needed {
            return false;
        }

        for y in 0..copy_h {
            let src_start = ((y * rect_w) * 4) as usize;
            let src_end = src_start + (copy_w * 4) as usize;
            let dst_start = (((rect_y + y) * self.width + rect_x) * 4) as usize;
            let dst_end = dst_start + (copy_w * 4) as usize;
            self.bgra[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
        }
        true
    }

    fn copy_rect(&mut self, dst: RfbRect, src_x: u16, src_y: u16) -> bool {
        if self.width == 0 || self.height == 0 || dst.width == 0 || dst.height == 0 {
            return false;
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
            return false;
        }
        let copy_w = copy_w.min(self.width - src_x).min(self.width - dst_x);
        let copy_h = copy_h.min(self.height - src_y).min(self.height - dst_y);
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
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_draws_bgra_rect() {
        let mut framebuffer = VncFramebuffer::new(2, 2);

        assert!(framebuffer.apply(VncServerEvent::RawImage(
            RfbRect {
                x: 1,
                y: 0,
                width: 1,
                height: 2,
            },
            vec![1, 2, 3, 255, 4, 5, 6, 255],
        )));

        assert_eq!(
            framebuffer.frame().bytes,
            vec![0, 0, 0, 0, 1, 2, 3, 255, 0, 0, 0, 0, 4, 5, 6, 255]
        );
    }

    #[test]
    fn framebuffer_copies_rect_without_overlapping_corruption() {
        let mut framebuffer = VncFramebuffer::new(3, 1);
        framebuffer.apply(VncServerEvent::RawImage(
            RfbRect {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255],
        ));

        assert!(framebuffer.apply(VncServerEvent::CopyRect {
            dst: RfbRect {
                x: 1,
                y: 0,
                width: 2,
                height: 1,
            },
            src_x: 0,
            src_y: 0,
        }));

        assert_eq!(
            framebuffer.frame().bytes,
            vec![1, 0, 0, 255, 1, 0, 0, 255, 2, 0, 0, 255]
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
}
