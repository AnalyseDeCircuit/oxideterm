use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use gpui::{
    App, Bounds, Corners, DevicePixels, IntoElement, ObjectFit, Pixels, Point, RenderImage,
    ScrollDelta, Styled, Window, canvas, fill, px, rgb, size,
};
use image::{Frame, RgbaImage};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, tcp::OwnedReadHalf, tcp::OwnedWriteHalf},
    sync::{mpsc, oneshot},
    time::MissedTickBehavior,
};

const VNC_FRAME_TICK: Duration = Duration::from_millis(33);
const VNC_PROTOCOL_VERSION: &[u8; 12] = b"RFB 003.008\n";
const VNC_SECURITY_NONE: u8 = 1;
const VNC_ENCODING_RAW: i32 = 0;
const VNC_ENCODING_COPY_RECT: i32 = 1;
const VNC_ENCODING_DESKTOP_SIZE: i32 = -223;
const VNC_BUTTON_LEFT: u8 = 1;
const VNC_BUTTON_MIDDLE: u8 = 2;
const VNC_BUTTON_RIGHT: u8 = 4;
const VNC_WHEEL_UP: u8 = 8;
const VNC_WHEEL_DOWN: u8 = 16;
const VNC_SCROLL_LINE: f32 = 38.0;

#[derive(Clone, Debug)]
pub(super) struct GraphicsVncFrame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl GraphicsVncFrame {
    pub(super) fn render_image(&self) -> Option<Arc<RenderImage>> {
        // The native RFB client negotiates 32-bit little-endian true color:
        // bytes arrive as B,G,R,unused. GPUI's atlas accepts that BGRA layout.
        let buffer = RgbaImage::from_raw(self.width, self.height, self.bgra.clone())?;
        Some(Arc::new(RenderImage::new(vec![Frame::new(buffer)])))
    }
}

#[derive(Clone, Debug)]
pub(super) enum GraphicsVncWorkerEvent {
    Connected {
        session_id: String,
    },
    Frame {
        session_id: String,
        frame: GraphicsVncFrame,
    },
    Disconnected {
        session_id: String,
        reason: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub(super) enum GraphicsVncInput {
    Pointer { x: u16, y: u16, buttons: u8 },
    Key { keysym: u32, down: bool },
}

#[derive(Clone, Default)]
pub(super) struct SharedGraphicsVncGeometry(Rc<RefCell<GraphicsVncGeometry>>);

#[derive(Clone, Copy, Debug, Default)]
struct GraphicsVncGeometry {
    image_bounds: Option<Bounds<Pixels>>,
    frame_size: Option<(u32, u32)>,
}

impl SharedGraphicsVncGeometry {
    pub(super) fn clear(&self) {
        *self.0.borrow_mut() = GraphicsVncGeometry::default();
    }

    fn update(&self, image_bounds: Option<Bounds<Pixels>>, frame_size: Option<(u32, u32)>) {
        *self.0.borrow_mut() = GraphicsVncGeometry {
            image_bounds,
            frame_size,
        };
    }

    pub(super) fn pointer(&self, position: Point<Pixels>) -> Option<(u16, u16)> {
        let geometry = self.0.borrow();
        let bounds = geometry.image_bounds?;
        let (width, height) = geometry.frame_size?;
        let left = f32::from(bounds.origin.x);
        let top = f32::from(bounds.origin.y);
        let image_w = f32::from(bounds.size.width).max(1.0);
        let image_h = f32::from(bounds.size.height).max(1.0);
        let local_x = f32::from(position.x) - left;
        let local_y = f32::from(position.y) - top;
        if local_x < 0.0 || local_y < 0.0 || local_x > image_w || local_y > image_h {
            return None;
        }
        let x = ((local_x / image_w) * width as f32).clamp(0.0, width.saturating_sub(1) as f32);
        let y = ((local_y / image_h) * height as f32).clamp(0.0, height.saturating_sub(1) as f32);
        Some((x.round() as u16, y.round() as u16))
    }
}

pub(super) fn graphics_vnc_canvas(
    frame: Option<GraphicsVncFrame>,
    image: Option<Arc<RenderImage>>,
    geometry: SharedGraphicsVncGeometry,
    background: u32,
) -> impl IntoElement {
    let frame_size = frame.as_ref().map(|frame| (frame.width, frame.height));
    canvas(
        move |bounds, _window: &mut Window, _cx: &mut App| {
            let image_bounds = frame_size.map(|(width, height)| {
                ObjectFit::Contain.get_bounds(
                    bounds,
                    size(DevicePixels(width as i32), DevicePixels(height as i32)),
                )
            });
            geometry.update(image_bounds, frame_size);
            image_bounds
        },
        move |bounds, image_bounds, window: &mut Window, _cx: &mut App| {
            window.paint_quad(fill(bounds, rgb(background)));
            if let (Some(image), Some(image_bounds)) = (image, image_bounds) {
                let _ = window.paint_image(image_bounds, Corners::all(px(0.0)), image, 0, false);
            }
        },
    )
    .size_full()
}

pub(super) fn vnc_button_mask(button: gpui::MouseButton) -> u8 {
    match button {
        gpui::MouseButton::Left => VNC_BUTTON_LEFT,
        gpui::MouseButton::Middle => VNC_BUTTON_MIDDLE,
        gpui::MouseButton::Right => VNC_BUTTON_RIGHT,
        gpui::MouseButton::Navigate(_) => 0,
    }
}

pub(super) fn vnc_scroll_masks(delta: &ScrollDelta) -> Vec<u8> {
    let y_delta = match delta {
        ScrollDelta::Pixels(point) => f32::from(point.y),
        ScrollDelta::Lines(point) => point.y * VNC_SCROLL_LINE,
    };
    if y_delta.abs() < f32::EPSILON {
        return Vec::new();
    }
    let steps = (y_delta.abs() / VNC_SCROLL_LINE).ceil().clamp(1.0, 6.0) as usize;
    let mask = if y_delta > 0.0 {
        VNC_WHEEL_DOWN
    } else {
        VNC_WHEEL_UP
    };
    vec![mask; steps]
}

pub(super) async fn run_graphics_vnc_worker(
    session_id: String,
    vnc_port: u16,
    mut input_rx: mpsc::UnboundedReceiver<GraphicsVncInput>,
    mut stop_rx: oneshot::Receiver<()>,
    event_sink: impl Fn(GraphicsVncWorkerEvent) + Send + 'static,
) {
    let (mut client, mut server_rx) = match connect_graphics_vnc(vnc_port).await {
        Ok(client) => client,
        Err(error) => {
            event_sink(GraphicsVncWorkerEvent::Disconnected {
                session_id,
                reason: Some(error),
            });
            return;
        }
    };

    let mut framebuffer = GraphicsVncFramebuffer::new(client.width, client.height);
    event_sink(GraphicsVncWorkerEvent::Connected {
        session_id: session_id.clone(),
    });

    let mut ticker = tokio::time::interval(VNC_FRAME_TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let _ = client.request_framebuffer_update(false).await;

    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                let _ = client.shutdown().await;
                return;
            }
            Some(input) = input_rx.recv() => {
                if let Err(error) = client.send_input(input).await {
                    event_sink(GraphicsVncWorkerEvent::Disconnected {
                        session_id,
                        reason: Some(error),
                    });
                    return;
                }
            }
            Some(server_event) = server_rx.recv() => {
                match server_event {
                    Ok(event) => {
                        if framebuffer.apply(event) {
                            if let Some(frame) = framebuffer.frame() {
                                event_sink(GraphicsVncWorkerEvent::Frame {
                                    session_id: session_id.clone(),
                                    frame,
                                });
                            }
                        }
                    }
                    Err(error) => {
                        event_sink(GraphicsVncWorkerEvent::Disconnected {
                            session_id,
                            reason: Some(error),
                        });
                        return;
                    }
                }
            }
            _ = ticker.tick() => {
                if let Err(error) = client.request_framebuffer_update(true).await {
                    event_sink(GraphicsVncWorkerEvent::Disconnected {
                        session_id,
                        reason: Some(error),
                    });
                    return;
                }
            }
        }
    }
}

async fn connect_graphics_vnc(
    vnc_port: u16,
) -> Result<
    (
        GraphicsVncClient,
        mpsc::UnboundedReceiver<Result<VncServerEvent, String>>,
    ),
    String,
> {
    let mut stream = TcpStream::connect(("127.0.0.1", vnc_port))
        .await
        .map_err(|error| error.to_string())?;

    handshake_vnc(&mut stream).await?;
    let (width, height) = read_server_init(&mut stream).await?;
    write_pixel_format(&mut stream).await?;
    write_encodings(&mut stream).await?;

    let (reader, writer) = stream.into_split();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        read_vnc_events(reader, event_tx).await;
    });

    Ok((
        GraphicsVncClient {
            writer,
            width,
            height,
        },
        event_rx,
    ))
}

struct GraphicsVncClient {
    writer: OwnedWriteHalf,
    width: u16,
    height: u16,
}

impl GraphicsVncClient {
    async fn send_input(&mut self, input: GraphicsVncInput) -> Result<(), String> {
        match input {
            GraphicsVncInput::Pointer { x, y, buttons } => {
                let mut message = Vec::with_capacity(6);
                message.push(5);
                message.push(buttons);
                push_be_u16(&mut message, x);
                push_be_u16(&mut message, y);
                self.writer
                    .write_all(&message)
                    .await
                    .map_err(|error| error.to_string())
            }
            GraphicsVncInput::Key { keysym, down } => {
                let mut message = Vec::with_capacity(8);
                message.push(4);
                message.push(u8::from(down));
                message.extend_from_slice(&[0, 0]);
                push_be_u32(&mut message, keysym);
                self.writer
                    .write_all(&message)
                    .await
                    .map_err(|error| error.to_string())
            }
        }
    }

    async fn request_framebuffer_update(&mut self, incremental: bool) -> Result<(), String> {
        let mut message = Vec::with_capacity(10);
        message.push(3);
        message.push(u8::from(incremental));
        push_be_u16(&mut message, 0);
        push_be_u16(&mut message, 0);
        push_be_u16(&mut message, self.width);
        push_be_u16(&mut message, self.height);
        self.writer
            .write_all(&message)
            .await
            .map_err(|error| error.to_string())
    }

    async fn shutdown(&mut self) -> Result<(), std::io::Error> {
        self.writer.shutdown().await
    }
}

async fn handshake_vnc(stream: &mut TcpStream) -> Result<(), String> {
    let server_version = read_exact_array::<12, _>(stream)
        .await
        .map_err(|error| error.to_string())?;
    if !server_version.starts_with(b"RFB ") {
        return Err("VNC server did not send an RFB protocol banner".to_string());
    }

    stream
        .write_all(VNC_PROTOCOL_VERSION)
        .await
        .map_err(|error| error.to_string())?;

    // RFB 3.7/3.8 sends a list of security types. Xtigervnc uses this path.
    let count = read_u8(stream).await.map_err(|error| error.to_string())?;
    if count == 0 {
        let reason = read_reason(stream)
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        return Err(format!(
            "VNC server rejected security negotiation: {reason}"
        ));
    }

    let mut security_types = vec![0; count as usize];
    stream
        .read_exact(&mut security_types)
        .await
        .map_err(|error| error.to_string())?;
    if !security_types.contains(&VNC_SECURITY_NONE) {
        return Err("VNC server requires authentication, but WSL Graphics starts Xtigervnc with SecurityTypes None".to_string());
    }

    stream
        .write_all(&[VNC_SECURITY_NONE])
        .await
        .map_err(|error| error.to_string())?;
    let security_result = read_be_u32(stream)
        .await
        .map_err(|error| error.to_string())?;
    if security_result != 0 {
        let reason = read_reason(stream)
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        return Err(format!("VNC security negotiation failed: {reason}"));
    }

    // Shared mode matches the previous embedded viewer and avoids disconnecting external viewers.
    stream
        .write_all(&[1])
        .await
        .map_err(|error| error.to_string())
}

async fn read_server_init(stream: &mut TcpStream) -> Result<(u16, u16), String> {
    let init = read_exact_array::<24, _>(stream)
        .await
        .map_err(|error| error.to_string())?;
    let width = be_u16(&init[0..2]);
    let height = be_u16(&init[2..4]);
    let name_len = be_u32(&init[20..24]) as usize;
    if name_len > 0 {
        let mut name = vec![0; name_len];
        stream
            .read_exact(&mut name)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok((width, height))
}

async fn write_pixel_format(stream: &mut TcpStream) -> Result<(), String> {
    let mut message = Vec::with_capacity(20);
    message.extend_from_slice(&[0, 0, 0, 0]);
    message.extend_from_slice(&[
        32, 24, 0, 1, // bits-per-pixel, depth, little-endian, true-color
        0, 255, 0, 255, 0, 255, // color max values
        16, 8, 0, // red, green, blue shifts => BGRA byte order on little endian
        0, 0, 0,
    ]);
    stream
        .write_all(&message)
        .await
        .map_err(|error| error.to_string())
}

async fn write_encodings(stream: &mut TcpStream) -> Result<(), String> {
    let mut message = Vec::with_capacity(12);
    message.push(2);
    message.push(0);
    push_be_u16(&mut message, 2);
    push_be_i32(&mut message, VNC_ENCODING_COPY_RECT);
    push_be_i32(&mut message, VNC_ENCODING_RAW);
    stream
        .write_all(&message)
        .await
        .map_err(|error| error.to_string())
}

async fn read_vnc_events(
    mut reader: OwnedReadHalf,
    event_tx: mpsc::UnboundedSender<Result<VncServerEvent, String>>,
) {
    loop {
        let result = read_vnc_event(&mut reader).await;
        let should_stop = result.is_err();
        if event_tx.send(result).is_err() || should_stop {
            return;
        }
    }
}

async fn read_vnc_event(reader: &mut OwnedReadHalf) -> Result<VncServerEvent, String> {
    let message_type = read_u8(reader).await.map_err(|error| error.to_string())?;
    match message_type {
        0 => read_framebuffer_update(reader).await,
        1 => {
            skip_color_map_entries(reader).await?;
            Ok(VncServerEvent::Noop)
        }
        2 => Ok(VncServerEvent::Noop),
        3 => {
            skip_server_cut_text(reader).await?;
            Ok(VncServerEvent::Noop)
        }
        other => Err(format!("Unsupported VNC server message type {other}")),
    }
}

async fn read_framebuffer_update(reader: &mut OwnedReadHalf) -> Result<VncServerEvent, String> {
    let _padding = read_u8(reader).await.map_err(|error| error.to_string())?;
    let rect_count = read_be_u16(reader)
        .await
        .map_err(|error| error.to_string())?;
    let mut events = Vec::with_capacity(rect_count as usize);

    for _ in 0..rect_count {
        let header = read_exact_array::<12, _>(reader)
            .await
            .map_err(|error| error.to_string())?;
        let rect = RfbRect {
            x: be_u16(&header[0..2]),
            y: be_u16(&header[2..4]),
            width: be_u16(&header[4..6]),
            height: be_u16(&header[6..8]),
        };
        let encoding = be_i32(&header[8..12]);
        match encoding {
            VNC_ENCODING_RAW => {
                let byte_len = rect.width as usize * rect.height as usize * 4;
                let mut data = vec![0; byte_len];
                reader
                    .read_exact(&mut data)
                    .await
                    .map_err(|error| error.to_string())?;
                events.push(VncServerEvent::RawImage(rect, data));
            }
            VNC_ENCODING_COPY_RECT => {
                let source = read_exact_array::<4, _>(reader)
                    .await
                    .map_err(|error| error.to_string())?;
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
            other => return Err(format!("Unsupported VNC rectangle encoding {other}")),
        }
    }

    Ok(VncServerEvent::Batch(events))
}

async fn skip_color_map_entries(reader: &mut OwnedReadHalf) -> Result<(), String> {
    let _padding = read_u8(reader).await.map_err(|error| error.to_string())?;
    let _first = read_be_u16(reader)
        .await
        .map_err(|error| error.to_string())?;
    let count = read_be_u16(reader)
        .await
        .map_err(|error| error.to_string())?;
    let mut skip = vec![0; count as usize * 6];
    reader
        .read_exact(&mut skip)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

async fn skip_server_cut_text(reader: &mut OwnedReadHalf) -> Result<(), String> {
    let _padding = read_exact_array::<3, _>(reader)
        .await
        .map_err(|error| error.to_string())?;
    let len = read_be_u32(reader)
        .await
        .map_err(|error| error.to_string())? as usize;
    let mut skip = vec![0; len];
    reader
        .read_exact(&mut skip)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

async fn read_reason(stream: &mut TcpStream) -> std::io::Result<String> {
    let len = read_be_u32(stream).await? as usize;
    let mut data = vec![0; len];
    stream.read_exact(&mut data).await?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

async fn read_u8(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<u8> {
    let mut byte = [0; 1];
    reader.read_exact(&mut byte).await?;
    Ok(byte[0])
}

async fn read_be_u16(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<u16> {
    let bytes = read_exact_array::<2, _>(reader).await?;
    Ok(be_u16(&bytes))
}

async fn read_be_u32(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<u32> {
    let bytes = read_exact_array::<4, _>(reader).await?;
    Ok(be_u32(&bytes))
}

async fn read_exact_array<const N: usize, R: AsyncRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<[u8; N]> {
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes).await?;
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
    Batch(Vec<VncServerEvent>),
    Noop,
}

struct GraphicsVncFramebuffer {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

impl GraphicsVncFramebuffer {
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
            VncServerEvent::Noop => false,
        }
    }

    fn frame(&self) -> Option<GraphicsVncFrame> {
        if self.width == 0 || self.height == 0 || self.bgra.is_empty() {
            return None;
        }
        Some(GraphicsVncFrame {
            width: self.width,
            height: self.height,
            bgra: self.bgra.clone(),
        })
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

pub(super) fn graphics_vnc_keysyms(key: &str, key_char: Option<&str>) -> Option<u32> {
    if let Some(key_char) = key_char
        && let Some(ch) = key_char.chars().next()
        && !ch.is_control()
    {
        return Some(ch as u32);
    }

    match key {
        "space" => Some(0x20),
        "enter" => Some(0xff0d),
        "tab" => Some(0xff09),
        "escape" => Some(0xff1b),
        "backspace" => Some(0xff08),
        "delete" => Some(0xffff),
        "left" => Some(0xff51),
        "up" => Some(0xff52),
        "right" => Some(0xff53),
        "down" => Some(0xff54),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_draws_bgra_rect() {
        let mut fb = GraphicsVncFramebuffer::new(2, 2);
        assert!(fb.apply(VncServerEvent::RawImage(
            RfbRect {
                x: 1,
                y: 0,
                width: 1,
                height: 2,
            },
            vec![1, 2, 3, 255, 4, 5, 6, 255],
        )));
        assert_eq!(
            fb.frame().unwrap().bgra,
            vec![0, 0, 0, 0, 1, 2, 3, 255, 0, 0, 0, 0, 4, 5, 6, 255]
        );
    }

    #[test]
    fn framebuffer_copies_rect_without_overlapping_corruption() {
        let mut fb = GraphicsVncFramebuffer::new(3, 1);
        fb.apply(VncServerEvent::RawImage(
            RfbRect {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255],
        ));
        assert!(fb.apply(VncServerEvent::CopyRect {
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
            fb.frame().unwrap().bgra,
            vec![1, 0, 0, 255, 1, 0, 0, 255, 2, 0, 0, 255]
        );
    }
}
