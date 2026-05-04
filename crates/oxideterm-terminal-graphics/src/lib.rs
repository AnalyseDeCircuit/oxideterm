// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::AnimationDecoder;
use image::ImageReader;
use image::codecs::gif::GifDecoder;
use thiserror::Error;

pub const DEFAULT_PIXEL_LIMIT: u32 = 16_777_216;
pub const DEFAULT_STORAGE_LIMIT_MB: u32 = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsOptions {
    pub enabled: bool,
    pub sixel: bool,
    pub iterm2_inline: bool,
    pub kitty: bool,
    pub pixel_limit: u32,
    pub storage_limit_mb: u32,
    pub show_placeholder: bool,
}

impl Default for GraphicsOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            sixel: true,
            iterm2_inline: true,
            kitty: true,
            pixel_limit: DEFAULT_PIXEL_LIMIT,
            storage_limit_mb: DEFAULT_STORAGE_LIMIT_MB,
            show_placeholder: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalImageId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalImageProtocol {
    Iterm2,
    Kitty,
    Sixel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalImageData {
    pub id: TerminalImageId,
    pub protocol: TerminalImageProtocol,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalImagePlacement {
    pub id: TerminalImageId,
    pub protocol: TerminalImageProtocol,
    pub line: i32,
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub z_index: i32,
    pub placeholder: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalGraphicsEvent {
    ImageReady(TerminalImageData),
    Place(TerminalImagePlacement),
    Delete { id: Option<TerminalImageId> },
    Respond(Vec<u8>),
    Error(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphicsAdvance {
    pub terminal_bytes: Vec<u8>,
    pub events: Vec<TerminalGraphicsEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphicsCursor {
    pub line: i32,
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl GraphicsCursor {
    pub fn image_cells(self, pixel_width: u32, pixel_height: u32) -> (usize, usize) {
        let cell_width = u32::from(self.cell_width).max(1);
        let cell_height = u32::from(self.cell_height).max(1);
        let cols = pixel_width.div_ceil(cell_width).max(1) as usize;
        let rows = pixel_height.div_ceil(cell_height).max(1) as usize;
        (cols.min(self.cols.max(1)), rows.min(self.rows.max(1)))
    }
}

#[derive(Debug, Error)]
pub enum GraphicsError {
    #[error("image is larger than the configured pixel limit")]
    PixelLimitExceeded,
    #[error("invalid base64 image payload")]
    InvalidBase64,
    #[error("unsupported image payload")]
    UnsupportedImage,
    #[error("{0}")]
    Decode(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParserState {
    Ground,
    Esc,
    Osc(Vec<u8>),
    OscEsc(Vec<u8>),
    Dcs(Vec<u8>),
    DcsEsc(Vec<u8>),
    Apc(Vec<u8>),
    ApcEsc(Vec<u8>),
}

pub struct GraphicsIngress {
    options: GraphicsOptions,
    state: ParserState,
    next_image_id: u64,
    kitty_chunks: HashMap<u64, Vec<u8>>,
}

impl GraphicsIngress {
    pub fn new(options: GraphicsOptions) -> Self {
        Self {
            options,
            state: ParserState::Ground,
            next_image_id: 1,
            kitty_chunks: HashMap::new(),
        }
    }

    pub fn advance(&mut self, bytes: &[u8], cursor: GraphicsCursor) -> GraphicsAdvance {
        if !self.options.enabled {
            return GraphicsAdvance {
                terminal_bytes: bytes.to_vec(),
                events: Vec::new(),
            };
        }

        let mut result = GraphicsAdvance::default();
        for &byte in bytes {
            self.advance_byte(byte, cursor, &mut result);
        }
        result
    }

    fn advance_byte(&mut self, byte: u8, cursor: GraphicsCursor, result: &mut GraphicsAdvance) {
        let state = std::mem::replace(&mut self.state, ParserState::Ground);
        match state {
            ParserState::Ground => match byte {
                0x1b => self.state = ParserState::Esc,
                0x90 => self.state = ParserState::Dcs(Vec::new()),
                0x9d => self.state = ParserState::Osc(Vec::new()),
                0x9f => self.state = ParserState::Apc(Vec::new()),
                _ => result.terminal_bytes.push(byte),
            },
            ParserState::Esc => match byte {
                b']' => self.state = ParserState::Osc(Vec::new()),
                b'P' => self.state = ParserState::Dcs(Vec::new()),
                b'_' => self.state = ParserState::Apc(Vec::new()),
                _ => {
                    result.terminal_bytes.push(0x1b);
                    result.terminal_bytes.push(byte);
                }
            },
            ParserState::Osc(mut data) => match byte {
                0x07 => self.dispatch_osc(data, cursor, result),
                0x9c => self.dispatch_osc(data, cursor, result),
                0x1b => self.state = ParserState::OscEsc(data),
                _ => {
                    data.push(byte);
                    self.state = ParserState::Osc(data);
                }
            },
            ParserState::OscEsc(data) => match byte {
                b'\\' => self.dispatch_osc(data, cursor, result),
                _ => {
                    result.terminal_bytes.extend_from_slice(b"\x1b]");
                    result.terminal_bytes.extend_from_slice(&data);
                    result.terminal_bytes.push(0x1b);
                    result.terminal_bytes.push(byte);
                }
            },
            ParserState::Dcs(mut data) => match byte {
                0x9c => self.dispatch_dcs(data, cursor, result),
                0x1b => self.state = ParserState::DcsEsc(data),
                _ => {
                    data.push(byte);
                    self.state = ParserState::Dcs(data);
                }
            },
            ParserState::DcsEsc(mut data) => match byte {
                b'\\' => self.dispatch_dcs(data, cursor, result),
                _ => {
                    data.push(0x1b);
                    data.push(byte);
                    self.state = ParserState::Dcs(data);
                }
            },
            ParserState::Apc(mut data) => match byte {
                0x9c => self.dispatch_apc(data, cursor, result),
                0x1b => self.state = ParserState::ApcEsc(data),
                _ => {
                    data.push(byte);
                    self.state = ParserState::Apc(data);
                }
            },
            ParserState::ApcEsc(mut data) => match byte {
                b'\\' => self.dispatch_apc(data, cursor, result),
                _ => {
                    data.push(0x1b);
                    data.push(byte);
                    self.state = ParserState::Apc(data);
                }
            },
        }
    }

    fn next_id(&mut self) -> TerminalImageId {
        let id = TerminalImageId(self.next_image_id);
        self.next_image_id += 1;
        id
    }

    fn dispatch_osc(
        &mut self,
        data: Vec<u8>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) {
        if !self.options.iterm2_inline || !data.starts_with(b"1337;File=") {
            result.terminal_bytes.extend_from_slice(b"\x1b]");
            result.terminal_bytes.extend_from_slice(&data);
            result.terminal_bytes.push(0x07);
            return;
        }

        match self.decode_iterm2(&data[b"1337;File=".len()..], cursor) {
            Ok((image, placement, advance)) => {
                result.events.push(TerminalGraphicsEvent::ImageReady(image));
                result.events.push(TerminalGraphicsEvent::Place(placement));
                result.terminal_bytes.extend(advance);
            }
            Err(error) => result
                .events
                .push(TerminalGraphicsEvent::Error(error.to_string())),
        }
    }

    fn dispatch_dcs(
        &mut self,
        data: Vec<u8>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) {
        if !self.options.sixel || !looks_like_sixel(&data) {
            result.terminal_bytes.extend_from_slice(b"\x1bP");
            result.terminal_bytes.extend_from_slice(&data);
            result.terminal_bytes.extend_from_slice(b"\x1b\\");
            return;
        }

        let mut sequence = Vec::with_capacity(data.len() + 4);
        sequence.extend_from_slice(b"\x1bP");
        sequence.extend_from_slice(&data);
        sequence.extend_from_slice(b"\x1b\\");

        match self.decode_sixel(&sequence, cursor) {
            Ok((image, placement, advance)) => {
                result.events.push(TerminalGraphicsEvent::ImageReady(image));
                result.events.push(TerminalGraphicsEvent::Place(placement));
                result.terminal_bytes.extend(advance);
            }
            Err(error) => result
                .events
                .push(TerminalGraphicsEvent::Error(error.to_string())),
        }
    }

    fn dispatch_apc(
        &mut self,
        data: Vec<u8>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) {
        if !self.options.kitty || !data.starts_with(b"G") {
            result.terminal_bytes.extend_from_slice(b"\x1b_");
            result.terminal_bytes.extend_from_slice(&data);
            result.terminal_bytes.extend_from_slice(b"\x1b\\");
            return;
        }

        if let Some((params, _)) = parse_kitty_params_and_payload(&data[1..]) {
            match params.get("a").map(String::as_str).unwrap_or("t") {
                "d" => {
                    let id = params
                        .get("i")
                        .and_then(|value| value.parse::<u64>().ok())
                        .map(TerminalImageId);
                    result.events.push(TerminalGraphicsEvent::Delete { id });
                    return;
                }
                "q" => {
                    let id = params
                        .get("i")
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or_default();
                    result
                        .events
                        .push(TerminalGraphicsEvent::Respond(kitty_query_response(id)));
                    return;
                }
                _ => {}
            }
        }

        match self.decode_kitty(&data[1..], cursor) {
            Ok(Some((image, placement, advance))) => {
                result.events.push(TerminalGraphicsEvent::ImageReady(image));
                result.events.push(TerminalGraphicsEvent::Place(placement));
                result.terminal_bytes.extend(advance);
            }
            Ok(None) => {}
            Err(error) => result
                .events
                .push(TerminalGraphicsEvent::Error(error.to_string())),
        }
    }

    fn decode_iterm2(
        &mut self,
        data: &[u8],
        cursor: GraphicsCursor,
    ) -> Result<(TerminalImageData, TerminalImagePlacement, Vec<u8>), GraphicsError> {
        let Some(separator) = data.iter().position(|byte| *byte == b':') else {
            return Err(GraphicsError::UnsupportedImage);
        };
        let params = parse_semicolon_params(&data[..separator]);
        let payload = BASE64
            .decode(&data[separator + 1..])
            .map_err(|_| GraphicsError::InvalidBase64)?;
        let name = params
            .get("name")
            .and_then(|value| BASE64.decode(value).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok());
        let decoded = decode_image_bytes(&payload, self.options.pixel_limit)?;
        let width = params
            .get("width")
            .and_then(|value| parse_pixel_size(value, decoded.width));
        let height = params
            .get("height")
            .and_then(|value| parse_pixel_size(value, decoded.height));
        let image = TerminalImageData {
            id: self.next_id(),
            protocol: TerminalImageProtocol::Iterm2,
            width: width.unwrap_or(decoded.width),
            height: height.unwrap_or(decoded.height),
            rgba: decoded.rgba,
            name,
        };
        let do_not_move = params
            .get("doNotMoveCursor")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let (placement, advance) = self.placement_for_image(
            image.id,
            image.protocol,
            image.width,
            image.height,
            cursor,
            !do_not_move,
        );
        Ok((image, placement, advance))
    }

    fn decode_sixel(
        &mut self,
        sequence: &[u8],
        cursor: GraphicsCursor,
    ) -> Result<(TerminalImageData, TerminalImagePlacement, Vec<u8>), GraphicsError> {
        let decoded = icy_sixel::SixelImage::decode(sequence)
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let width = decoded.width as u32;
        let height = decoded.height as u32;
        enforce_pixel_limit(width, height, self.options.pixel_limit)?;
        let image = TerminalImageData {
            id: self.next_id(),
            protocol: TerminalImageProtocol::Sixel,
            width,
            height,
            rgba: decoded.pixels,
            name: None,
        };
        let (placement, advance) =
            self.placement_for_image(image.id, image.protocol, width, height, cursor, true);
        Ok((image, placement, advance))
    }

    fn decode_kitty(
        &mut self,
        data: &[u8],
        cursor: GraphicsCursor,
    ) -> Result<Option<(TerminalImageData, TerminalImagePlacement, Vec<u8>)>, GraphicsError> {
        let Some((params, payload)) = parse_kitty_params_and_payload(data) else {
            return Ok(None);
        };
        let action = params.get("a").map(String::as_str).unwrap_or("t");
        if action == "d" || action == "q" {
            return Ok(None);
        }

        let explicit_id = params
            .get("i")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| self.next_image_id);
        let more = params.get("m").is_some_and(|value| value == "1");
        let mut encoded = self.kitty_chunks.remove(&explicit_id).unwrap_or_default();
        encoded.extend_from_slice(payload);
        if more {
            self.kitty_chunks.insert(explicit_id, encoded);
            return Ok(None);
        }
        let complete = BASE64
            .decode(encoded)
            .map_err(|_| GraphicsError::InvalidBase64)?;

        let decoded = match params.get("f").map(String::as_str) {
            Some("24") => decode_raw_rgb(&complete, &params)?,
            Some("32") => decode_raw_rgba(&complete, &params)?,
            _ => decode_image_bytes(&complete, self.options.pixel_limit)?,
        };
        let image_id = TerminalImageId(explicit_id);
        self.next_image_id = self.next_image_id.max(explicit_id + 1);
        let image = TerminalImageData {
            id: image_id,
            protocol: TerminalImageProtocol::Kitty,
            width: decoded.width,
            height: decoded.height,
            rgba: decoded.rgba,
            name: None,
        };
        let move_cursor = !params.get("C").is_some_and(|value| value == "0");
        let (placement, advance) = self.placement_for_image(
            image.id,
            image.protocol,
            image.width,
            image.height,
            cursor,
            move_cursor,
        );
        Ok(Some((image, placement, advance)))
    }

    fn placement_for_image(
        &self,
        id: TerminalImageId,
        protocol: TerminalImageProtocol,
        pixel_width: u32,
        pixel_height: u32,
        cursor: GraphicsCursor,
        move_cursor: bool,
    ) -> (TerminalImagePlacement, Vec<u8>) {
        let (cols, rows) = cursor.image_cells(pixel_width, pixel_height);
        let placement = TerminalImagePlacement {
            id,
            protocol,
            line: cursor.line,
            row: cursor.row,
            col: cursor.col,
            cols,
            rows,
            pixel_width,
            pixel_height,
            z_index: 0,
            placeholder: self.options.show_placeholder,
        };
        let advance = if move_cursor {
            advance_bytes(cursor.col, cols, rows, cursor.cols)
        } else {
            Vec::new()
        };
        (placement, advance)
    }
}

#[derive(Clone, Debug)]
struct DecodedPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn looks_like_sixel(data: &[u8]) -> bool {
    data.iter()
        .position(|byte| *byte == b'q')
        .is_some_and(|index| {
            data[..=index]
                .iter()
                .all(|byte| byte.is_ascii() && !byte.is_ascii_alphabetic() || *byte == b'q')
        })
}

fn decode_image_bytes(bytes: &[u8], pixel_limit: u32) -> Result<DecodedPixels, GraphicsError> {
    let format = image::guess_format(bytes).map_err(|_| GraphicsError::UnsupportedImage)?;
    if format == image::ImageFormat::Gif {
        let decoder = GifDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let mut frames = decoder.into_frames();
        let frame = frames
            .next()
            .ok_or(GraphicsError::UnsupportedImage)?
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let image = frame.into_buffer();
        let (width, height) = image.dimensions();
        enforce_pixel_limit(width, height, pixel_limit)?;
        return Ok(DecodedPixels {
            width,
            height,
            rgba: image.into_raw(),
        });
    }

    let image = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| GraphicsError::Decode(error.to_string()))?
        .decode()
        .map_err(|error| GraphicsError::Decode(error.to_string()))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    enforce_pixel_limit(width, height, pixel_limit)?;
    Ok(DecodedPixels {
        width,
        height,
        rgba: image.into_raw(),
    })
}

fn decode_raw_rgb(
    bytes: &[u8],
    params: &HashMap<String, String>,
) -> Result<DecodedPixels, GraphicsError> {
    let (width, height) = raw_dimensions(params)?;
    enforce_raw_len(bytes, width, height, 3)?;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for chunk in bytes.chunks_exact(3) {
        rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 0xff]);
    }
    Ok(DecodedPixels {
        width,
        height,
        rgba,
    })
}

fn decode_raw_rgba(
    bytes: &[u8],
    params: &HashMap<String, String>,
) -> Result<DecodedPixels, GraphicsError> {
    let (width, height) = raw_dimensions(params)?;
    enforce_raw_len(bytes, width, height, 4)?;
    Ok(DecodedPixels {
        width,
        height,
        rgba: bytes.to_vec(),
    })
}

fn raw_dimensions(params: &HashMap<String, String>) -> Result<(u32, u32), GraphicsError> {
    let width = params
        .get("s")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(GraphicsError::UnsupportedImage)?;
    let height = params
        .get("v")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(GraphicsError::UnsupportedImage)?;
    Ok((width, height))
}

fn enforce_raw_len(
    bytes: &[u8],
    width: u32,
    height: u32,
    channels: usize,
) -> Result<(), GraphicsError> {
    let expected = width as usize * height as usize * channels;
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(GraphicsError::UnsupportedImage)
    }
}

fn enforce_pixel_limit(width: u32, height: u32, pixel_limit: u32) -> Result<(), GraphicsError> {
    if width.saturating_mul(height) <= pixel_limit {
        Ok(())
    } else {
        Err(GraphicsError::PixelLimitExceeded)
    }
}

fn parse_semicolon_params(data: &[u8]) -> HashMap<String, String> {
    split_params(data, b';')
}

fn parse_comma_params(data: &[u8]) -> HashMap<String, String> {
    split_params(data, b',')
}

fn parse_kitty_params_and_payload(data: &[u8]) -> Option<(HashMap<String, String>, &[u8])> {
    let separator = data.iter().position(|byte| *byte == b';')?;
    Some((
        parse_comma_params(&data[..separator]),
        &data[separator + 1..],
    ))
}

fn kitty_query_response(id: u64) -> Vec<u8> {
    format!("\x1b_Gi={id};OK\x1b\\").into_bytes()
}

fn split_params(data: &[u8], separator: u8) -> HashMap<String, String> {
    data.split(|byte| *byte == separator)
        .filter_map(|part| {
            let index = part.iter().position(|byte| *byte == b'=')?;
            let (key, rest) = part.split_at(index);
            let value = &rest[1..];
            Some((
                String::from_utf8_lossy(key).to_string(),
                String::from_utf8_lossy(value).to_string(),
            ))
        })
        .collect()
}

fn parse_pixel_size(value: &str, fallback: u32) -> Option<u32> {
    if let Some(px) = value.strip_suffix("px") {
        px.parse().ok()
    } else if value == "auto" {
        Some(fallback)
    } else {
        value.parse().ok()
    }
}

fn advance_bytes(
    start_col: usize,
    image_cols: usize,
    image_rows: usize,
    terminal_cols: usize,
) -> Vec<u8> {
    if image_cols == 0 || image_rows == 0 {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    let mut remaining_rows = image_rows;
    let mut col = start_col;
    while remaining_rows > 0 {
        let cols_this_row = image_cols.min(terminal_cols.saturating_sub(col).max(1));
        bytes.extend(std::iter::repeat_n(b' ', cols_this_row));
        remaining_rows -= 1;
        if remaining_rows > 0 {
            bytes.extend_from_slice(b"\r\n");
            col = 0;
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn cursor() -> GraphicsCursor {
        GraphicsCursor {
            row: 0,
            line: 0,
            col: 0,
            cols: 80,
            rows: 24,
            cell_width: 10,
            cell_height: 20,
        }
    }

    #[test]
    fn plain_text_passes_through() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(b"hello", cursor());
        assert_eq!(result.terminal_bytes, b"hello");
        assert!(result.events.is_empty());
    }

    #[test]
    fn split_osc_sequence_is_consumed() {
        let mut png = RgbaImage::new(1, 1);
        png.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(png)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let payload = BASE64.encode(bytes);
        let seq = format!("\x1b]1337;File=inline=1:{payload}\x07");
        let first = ingress_advance_chunks(seq.as_bytes());
        assert!(first.terminal_bytes.contains(&b' '));
        assert!(
            first
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
    }

    #[test]
    fn invalid_iterm2_base64_does_not_leak_escape_sequence() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(b"\x1b]1337;File=inline=1:not base64\x07", cursor());

        assert!(result.terminal_bytes.is_empty());
        assert!(
            result
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::Error(_)))
        );
    }

    #[test]
    fn kitty_raw_rgba_image_is_placed_and_respects_no_cursor_move() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let payload = BASE64.encode([0, 255, 0, 255]);
        let seq = format!("\x1b_Ga=t,f=32,s=1,v=1,i=42,C=0;{payload}\x1b\\");
        let result = ingress.advance(seq.as_bytes(), cursor());

        assert!(result.terminal_bytes.is_empty());
        assert!(result.events.iter().any(|event| {
            matches!(
                event,
                TerminalGraphicsEvent::ImageReady(TerminalImageData {
                    id: TerminalImageId(42),
                    protocol: TerminalImageProtocol::Kitty,
                    width: 1,
                    height: 1,
                    ..
                })
            )
        }));
    }

    #[test]
    fn kitty_chunked_png_waits_until_final_chunk() {
        let mut png = RgbaImage::new(1, 1);
        png.put_pixel(0, 0, image::Rgba([0, 0, 255, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(png)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let payload = BASE64.encode(bytes);
        let split = payload.len() / 2;
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let first = format!("\x1b_Ga=t,f=100,i=7,m=1;{}\x1b\\", &payload[..split]);
        let second = format!("\x1b_Ga=t,f=100,i=7,m=0;{}\x1b\\", &payload[split..]);

        let first = ingress.advance(first.as_bytes(), cursor());
        assert!(first.events.is_empty());

        let second = ingress.advance(second.as_bytes(), cursor());
        assert!(
            second
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
    }

    #[test]
    fn kitty_delete_and_query_emit_control_events() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());

        let delete = ingress.advance(b"\x1b_Ga=d,i=9;\x1b\\", cursor());
        assert_eq!(
            delete.events,
            vec![TerminalGraphicsEvent::Delete {
                id: Some(TerminalImageId(9))
            }]
        );

        let query = ingress.advance(b"\x1b_Ga=q,i=9;\x1b\\", cursor());
        assert_eq!(
            query.events,
            vec![TerminalGraphicsEvent::Respond(
                b"\x1b_Gi=9;OK\x1b\\".to_vec()
            )]
        );
    }

    #[test]
    fn sixel_sequence_is_decoded() {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(b"\x1bPq#0;2;100;0;0#0~-\x1b\\", cursor());

        assert!(result.terminal_bytes.contains(&b' '));
        assert!(
            result
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
    }

    #[test]
    fn eight_bit_c1_terminators_are_consumed() {
        let mut png = RgbaImage::new(1, 1);
        png.put_pixel(0, 0, image::Rgba([255, 255, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(png)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let payload = BASE64.encode(bytes);
        let mut seq = b"\x9d1337;File=inline=1:".to_vec();
        seq.extend_from_slice(payload.as_bytes());
        seq.push(0x9c);
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let result = ingress.advance(&seq, cursor());

        assert!(
            result
                .events
                .iter()
                .any(|event| matches!(event, TerminalGraphicsEvent::ImageReady(_)))
        );
        assert!(!result.terminal_bytes.starts_with(b"\x1b]"));
    }

    fn ingress_advance_chunks(bytes: &[u8]) -> GraphicsAdvance {
        let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
        let mid = bytes.len() / 2;
        let mut first = ingress.advance(&bytes[..mid], cursor());
        let second = ingress.advance(&bytes[mid..], cursor());
        first.terminal_bytes.extend(second.terminal_bytes);
        first.events.extend(second.events);
        first
    }
}
