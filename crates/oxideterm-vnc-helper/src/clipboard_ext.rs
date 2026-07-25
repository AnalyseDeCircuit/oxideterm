// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fmt,
    io::{Cursor, Read, Write},
};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use ironrdp_cliprdr_format::bitmap::{dibv5_to_png, png_to_cf_dibv5};
use oxideterm_remote_desktop::{
    RemoteDesktopClipboardData, RemoteDesktopClipboardFormat, RemoteDesktopHelperEvent,
};

use super::*;

pub(super) const VNC_ENCODING_EXTENDED_CLIPBOARD: i32 = -0x3f5e_1a32;
pub(super) const MAX_VNC_CLIPBOARD_BYTES: usize = 20 * 1024 * 1024;
const MAX_VNC_CLIPBOARD_DIMENSION: u32 = 8192;

const CLIPBOARD_FORMAT_TEXT: u32 = 1 << 0;
const CLIPBOARD_FORMAT_RTF: u32 = 1 << 1;
const CLIPBOARD_FORMAT_HTML: u32 = 1 << 2;
const CLIPBOARD_FORMAT_DIB: u32 = 1 << 3;
const CLIPBOARD_FORMAT_FILES_RESERVED: u32 = 1 << 4;
const CLIPBOARD_KNOWN_FORMATS: u32 =
    CLIPBOARD_FORMAT_TEXT | CLIPBOARD_FORMAT_RTF | CLIPBOARD_FORMAT_HTML | CLIPBOARD_FORMAT_DIB;
const CLIPBOARD_FORMAT_MASK: u32 = 0x0000_ffff;
const CLIPBOARD_ACTION_CAPS: u32 = 1 << 24;
const CLIPBOARD_ACTION_REQUEST: u32 = 1 << 25;
const CLIPBOARD_ACTION_PEEK: u32 = 1 << 26;
const CLIPBOARD_ACTION_NOTIFY: u32 = 1 << 27;
const CLIPBOARD_ACTION_PROVIDE: u32 = 1 << 28;
const CLIPBOARD_ACTION_MASK: u32 = 0xff00_0000;
const CLIPBOARD_KNOWN_ACTIONS: u32 = CLIPBOARD_ACTION_CAPS
    | CLIPBOARD_ACTION_REQUEST
    | CLIPBOARD_ACTION_PEEK
    | CLIPBOARD_ACTION_NOTIFY
    | CLIPBOARD_ACTION_PROVIDE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExtendedClipboardFormat {
    Text,
    Rtf,
    Html,
    DibV5,
}

impl ExtendedClipboardFormat {
    const ALL: [Self; 4] = [Self::Text, Self::Rtf, Self::Html, Self::DibV5];

    const fn flag(self) -> u32 {
        match self {
            Self::Text => CLIPBOARD_FORMAT_TEXT,
            Self::Rtf => CLIPBOARD_FORMAT_RTF,
            Self::Html => CLIPBOARD_FORMAT_HTML,
            Self::DibV5 => CLIPBOARD_FORMAT_DIB,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Text => 0,
            Self::Rtf => 1,
            Self::Html => 2,
            Self::DibV5 => 3,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Rtf => "rtf",
            Self::Html => "html",
            Self::DibV5 => "dib-v5",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExtendedClipboardAction {
    Request,
    Peek,
    Notify,
    Provide,
}

impl ExtendedClipboardAction {
    const fn flag(self) -> u32 {
        match self {
            Self::Request => CLIPBOARD_ACTION_REQUEST,
            Self::Peek => CLIPBOARD_ACTION_PEEK,
            Self::Notify => CLIPBOARD_ACTION_NOTIFY,
            Self::Provide => CLIPBOARD_ACTION_PROVIDE,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct ExtendedClipboardContent {
    pub(super) text: Option<String>,
    pub(super) rtf: Option<Vec<u8>>,
    pub(super) html: Option<Vec<u8>>,
    pub(super) dib_v5: Option<Vec<u8>>,
}

impl ExtendedClipboardContent {
    pub(super) fn empty() -> Self {
        Self {
            text: None,
            rtf: None,
            html: None,
            dib_v5: None,
        }
    }

    pub(super) fn formats(&self) -> u32 {
        let mut formats = 0;
        if self.text.is_some() {
            formats |= CLIPBOARD_FORMAT_TEXT;
        }
        if self.rtf.is_some() {
            formats |= CLIPBOARD_FORMAT_RTF;
        }
        if self.html.is_some() {
            formats |= CLIPBOARD_FORMAT_HTML;
        }
        if self.dib_v5.is_some() {
            formats |= CLIPBOARD_FORMAT_DIB;
        }
        formats
    }

    fn bytes(&self, format: ExtendedClipboardFormat) -> Option<Vec<u8>> {
        match format {
            ExtendedClipboardFormat::Text => self.text.as_ref().map(|text| {
                // Extended Clipboard requires CRLF and an explicit trailing NUL.
                let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
                let mut bytes = normalized.into_bytes();
                bytes.push(0);
                bytes
            }),
            ExtendedClipboardFormat::Rtf => self.rtf.clone(),
            ExtendedClipboardFormat::Html => self.html.clone(),
            ExtendedClipboardFormat::DibV5 => self.dib_v5.clone(),
        }
    }

    fn set_bytes(&mut self, format: ExtendedClipboardFormat, bytes: Vec<u8>) -> Result<(), String> {
        match format {
            ExtendedClipboardFormat::Text => {
                let Some((&0, text_bytes)) = bytes.split_last() else {
                    return Err(
                        "VNC Extended Clipboard UTF-8 text is missing its trailing NUL."
                            .to_string(),
                    );
                };
                let text = std::str::from_utf8(text_bytes)
                    .map_err(|_| "VNC Extended Clipboard text is not valid UTF-8.".to_string())?
                    .replace("\r\n", "\n");
                self.text = Some(text);
            }
            ExtendedClipboardFormat::Rtf => self.rtf = Some(bytes),
            ExtendedClipboardFormat::Html => self.html = Some(bytes),
            ExtendedClipboardFormat::DibV5 => self.dib_v5 = Some(bytes),
        }
        Ok(())
    }

    fn from_validated_png(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_VNC_CLIPBOARD_BYTES {
            return Err("VNC clipboard image exceeds the 20 MiB limit.".to_string());
        }
        let dib_v5 = png_to_cf_dibv5(bytes)
            .map_err(|error| format!("VNC clipboard PNG conversion failed: {error}"))?;
        if dib_v5.len() > MAX_VNC_CLIPBOARD_BYTES {
            return Err("VNC clipboard DIB exceeds the 20 MiB limit.".to_string());
        }
        let mut content = Self::empty();
        content.dib_v5 = Some(dib_v5);
        Ok(content)
    }

    pub(super) fn from_clipboard_data(data: &RemoteDesktopClipboardData) -> Result<Self, String> {
        let source_format = match data.format {
            RemoteDesktopClipboardFormat::ImagePng => image::ImageFormat::Png,
            RemoteDesktopClipboardFormat::ImageJpeg => image::ImageFormat::Jpeg,
            RemoteDesktopClipboardFormat::ImageWebp => image::ImageFormat::WebP,
            RemoteDesktopClipboardFormat::ImageGif => image::ImageFormat::Gif,
            RemoteDesktopClipboardFormat::ImageBmp => image::ImageFormat::Bmp,
            RemoteDesktopClipboardFormat::ImageTiff => image::ImageFormat::Tiff,
            RemoteDesktopClipboardFormat::ImageSvg => {
                return Err("VNC Extended Clipboard cannot rasterize SVG images.".to_string());
            }
        };
        if data.bytes.len() > MAX_VNC_CLIPBOARD_BYTES {
            return Err("VNC clipboard image exceeds the 20 MiB limit.".to_string());
        }
        let mut reader =
            image::ImageReader::with_format(Cursor::new(data.bytes.as_slice()), source_format);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(MAX_VNC_CLIPBOARD_BYTES as u64);
        limits.max_image_width = Some(MAX_VNC_CLIPBOARD_DIMENSION);
        limits.max_image_height = Some(MAX_VNC_CLIPBOARD_DIMENSION);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|error| format!("VNC clipboard image decode failed: {error}"))?;
        validate_clipboard_image_dimensions(image.width(), image.height())?;
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| format!("VNC clipboard PNG encode failed: {error}"))?;
        Self::from_validated_png(png.get_ref())
    }

    pub(super) fn dib_png(&self) -> Result<Option<Vec<u8>>, String> {
        self.dib_v5
            .as_ref()
            .map(|bytes| {
                dibv5_to_png(bytes)
                    .map_err(|error| format!("VNC clipboard DIB conversion failed: {error}"))
                    .and_then(|png| {
                        ensure_clipboard_size(png.len())?;
                        Ok(png)
                    })
            })
            .transpose()
    }
}

impl fmt::Debug for ExtendedClipboardContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExtendedClipboardContent")
            .field("text", &self.text.as_ref().map(String::len))
            .field("rtf", &self.rtf.as_ref().map(Vec::len))
            .field("html", &self.html.as_ref().map(Vec::len))
            .field("dib_v5", &self.dib_v5.as_ref().map(Vec::len))
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExtendedClipboardCapabilities {
    formats: u32,
    actions: u32,
    maximum_unsolicited_sizes: [u32; 4],
}

impl ExtendedClipboardCapabilities {
    pub(super) fn local() -> Self {
        Self {
            formats: CLIPBOARD_KNOWN_FORMATS,
            actions: CLIPBOARD_ACTION_REQUEST
                | CLIPBOARD_ACTION_PEEK
                | CLIPBOARD_ACTION_NOTIFY
                | CLIPBOARD_ACTION_PROVIDE,
            // Zero forces the unambiguous notify/request/provide flow.
            maximum_unsolicited_sizes: [0; 4],
        }
    }

    pub(super) fn supports_format(&self, format: ExtendedClipboardFormat) -> bool {
        self.formats & format.flag() != 0
    }

    pub(super) fn supports_action(&self, action: ExtendedClipboardAction) -> bool {
        self.actions & action.flag() != 0
    }

    pub(super) fn format_labels(&self) -> Vec<String> {
        ExtendedClipboardFormat::ALL
            .into_iter()
            .filter(|format| self.supports_format(*format))
            .map(|format| format.label().to_string())
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExtendedClipboardMessage {
    Caps(ExtendedClipboardCapabilities),
    Request(u32),
    Peek,
    Notify(u32),
    Provide(ExtendedClipboardContent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum VncClipboardMessage {
    Legacy(String),
    Extended(ExtendedClipboardMessage),
}

#[derive(Default)]
pub(super) struct VncClipboardSession {
    server_capabilities: Option<ExtendedClipboardCapabilities>,
    local_content: Option<ExtendedClipboardContent>,
}

pub(super) struct VncClipboardOutcome {
    pub(super) messages: Vec<Vec<u8>>,
    pub(super) helper_events: Vec<RemoteDesktopHelperEvent>,
    pub(super) capabilities_changed: bool,
}

impl VncClipboardOutcome {
    fn empty() -> Self {
        Self {
            messages: Vec::new(),
            helper_events: Vec::new(),
            capabilities_changed: false,
        }
    }
}

impl VncClipboardSession {
    pub(super) fn server_capabilities(&self) -> Option<&ExtendedClipboardCapabilities> {
        self.server_capabilities.as_ref()
    }

    pub(super) fn set_local_text(&mut self, text: String) -> Result<Vec<Vec<u8>>, String> {
        let mut content = ExtendedClipboardContent::empty();
        content.text = Some(text);
        self.local_content = Some(content);
        self.local_content_announcement()
    }

    pub(super) fn set_local_data(
        &mut self,
        data: &RemoteDesktopClipboardData,
    ) -> Result<Vec<Vec<u8>>, String> {
        self.local_content = Some(ExtendedClipboardContent::from_clipboard_data(data)?);
        self.local_content_announcement()
    }

    pub(super) fn observe_server_message(
        &mut self,
        message: VncClipboardMessage,
    ) -> Result<VncClipboardOutcome, String> {
        let mut outcome = VncClipboardOutcome::empty();
        match message {
            VncClipboardMessage::Legacy(text) => {
                outcome
                    .helper_events
                    .push(RemoteDesktopHelperEvent::ClipboardText { text });
            }
            VncClipboardMessage::Extended(ExtendedClipboardMessage::Caps(capabilities)) => {
                outcome.capabilities_changed =
                    self.server_capabilities.as_ref() != Some(&capabilities);
                self.server_capabilities = Some(capabilities);
                outcome.messages.push(extended_clipboard_caps_message()?);
                if self
                    .server_capabilities
                    .as_ref()
                    .is_some_and(|capabilities| {
                        capabilities.supports_action(ExtendedClipboardAction::Peek)
                    })
                {
                    outcome.messages.push(extended_clipboard_action_message(
                        ExtendedClipboardAction::Peek,
                        0,
                    )?);
                }
                outcome.messages.extend(self.local_content_announcement()?);
            }
            VncClipboardMessage::Extended(ExtendedClipboardMessage::Request(formats)) => {
                let Some(capabilities) = &self.server_capabilities else {
                    return Err(
                        "VNC server requested Extended Clipboard before sending caps.".to_string(),
                    );
                };
                if !capabilities.supports_action(ExtendedClipboardAction::Provide) {
                    return Err(
                        "VNC server requested a clipboard action it did not negotiate.".to_string(),
                    );
                }
                if let Some(content) = &self.local_content {
                    let formats = formats & self.outgoing_formats(content);
                    if formats != 0 {
                        outcome
                            .messages
                            .push(extended_clipboard_provide_message(formats, content)?);
                    }
                }
            }
            VncClipboardMessage::Extended(ExtendedClipboardMessage::Peek) => {
                let Some(capabilities) = &self.server_capabilities else {
                    return Err(
                        "VNC server peeked Extended Clipboard before sending caps.".to_string()
                    );
                };
                if !capabilities.supports_action(ExtendedClipboardAction::Notify) {
                    return Err(
                        "VNC server requested a clipboard action it did not negotiate.".to_string(),
                    );
                }
                outcome.messages.extend(self.local_content_announcement()?);
            }
            VncClipboardMessage::Extended(ExtendedClipboardMessage::Notify(formats)) => {
                let Some(capabilities) = &self.server_capabilities else {
                    return Err(
                        "VNC server notified Extended Clipboard before sending caps.".to_string(),
                    );
                };
                if capabilities.supports_action(ExtendedClipboardAction::Request) {
                    let requested = formats & CLIPBOARD_KNOWN_FORMATS;
                    if requested != 0 {
                        outcome.messages.push(extended_clipboard_action_message(
                            ExtendedClipboardAction::Request,
                            requested,
                        )?);
                    }
                }
            }
            VncClipboardMessage::Extended(ExtendedClipboardMessage::Provide(content)) => {
                if self.server_capabilities.is_none() {
                    return Err(
                        "VNC server provided Extended Clipboard before sending caps.".to_string(),
                    );
                }
                if let Some(text) = &content.text {
                    outcome
                        .helper_events
                        .push(RemoteDesktopHelperEvent::ClipboardText { text: text.clone() });
                } else if let Some(html) = &content.html {
                    // GPUI currently has no native rich-text clipboard entry, so preserve
                    // the payload as UTF-8 text instead of silently discarding it.
                    outcome
                        .helper_events
                        .push(RemoteDesktopHelperEvent::ClipboardText {
                            text: String::from_utf8_lossy(html).into_owned(),
                        });
                } else if let Some(rtf) = &content.rtf {
                    outcome
                        .helper_events
                        .push(RemoteDesktopHelperEvent::ClipboardText {
                            text: String::from_utf8_lossy(rtf).into_owned(),
                        });
                }
                if let Some(png) = content.dib_png()? {
                    outcome
                        .helper_events
                        .push(RemoteDesktopHelperEvent::ClipboardData {
                            data: RemoteDesktopClipboardData::new(
                                RemoteDesktopClipboardFormat::ImagePng,
                                png,
                            ),
                        });
                }
            }
        }
        Ok(outcome)
    }

    fn local_content_announcement(&self) -> Result<Vec<Vec<u8>>, String> {
        let Some(content) = &self.local_content else {
            return Ok(Vec::new());
        };
        let Some(capabilities) = &self.server_capabilities else {
            if let Some(text) = &content.text {
                return client_cut_text_message(text).map(|message| vec![message]);
            }
            // Binary vendor traffic is forbidden until the server confirms caps.
            return Ok(Vec::new());
        };
        let formats = self.outgoing_formats(content);
        if formats == 0 {
            return Ok(Vec::new());
        }
        if capabilities.supports_action(ExtendedClipboardAction::Notify) {
            return extended_clipboard_action_message(ExtendedClipboardAction::Notify, formats)
                .map(|message| vec![message]);
        }
        if capabilities.supports_action(ExtendedClipboardAction::Provide) {
            return extended_clipboard_provide_message(formats, content)
                .map(|message| vec![message]);
        }
        Ok(Vec::new())
    }

    fn outgoing_formats(&self, content: &ExtendedClipboardContent) -> u32 {
        let Some(capabilities) = &self.server_capabilities else {
            return 0;
        };
        ExtendedClipboardFormat::ALL
            .into_iter()
            .filter(|format| capabilities.supports_format(*format))
            .fold(0, |formats, format| {
                formats | (content.formats() & format.flag())
            })
    }
}

pub(super) fn read_server_cut_text(reader: &mut impl Read) -> Result<VncClipboardMessage, String> {
    let _padding = read_exact_array::<3, _>(reader)
        .map_err(|error| format!("VNC clipboard padding read failed: {error}"))?;
    let length_bytes = read_exact_array::<4, _>(reader)
        .map_err(|error| format!("VNC clipboard length read failed: {error}"))?;
    let length = i32::from_be_bytes(length_bytes);

    if length >= 0 {
        let length =
            usize::try_from(length).map_err(|_| "VNC clipboard length is invalid.".to_string())?;
        ensure_clipboard_size(length)?;
        let bytes = read_exact_vec(reader, length)
            .map_err(|error| format!("VNC clipboard text read failed: {error}"))?;
        return Ok(VncClipboardMessage::Legacy(decode_vnc_clipboard_text(
            &bytes,
        )));
    }

    let length = length
        .checked_abs()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "VNC Extended Clipboard length is invalid.".to_string())?;
    ensure_clipboard_size(length)?;
    let bytes = read_exact_vec(reader, length)
        .map_err(|error| format!("VNC Extended Clipboard payload read failed: {error}"))?;
    parse_extended_clipboard_message(&bytes).map(VncClipboardMessage::Extended)
}

pub(super) fn extended_clipboard_caps_message() -> Result<Vec<u8>, String> {
    let capabilities = ExtendedClipboardCapabilities::local();
    let mut payload = Vec::with_capacity(20);
    push_be_u32(
        &mut payload,
        CLIPBOARD_ACTION_CAPS | capabilities.formats | capabilities.actions,
    );
    for format in ExtendedClipboardFormat::ALL {
        push_be_u32(
            &mut payload,
            capabilities.maximum_unsolicited_sizes[format.index()],
        );
    }
    extended_client_cut_text_message(&payload)
}

pub(super) fn extended_clipboard_action_message(
    action: ExtendedClipboardAction,
    formats: u32,
) -> Result<Vec<u8>, String> {
    if action == ExtendedClipboardAction::Provide {
        return Err("VNC Extended Clipboard provide requires clipboard data.".to_string());
    }
    let formats = formats & CLIPBOARD_KNOWN_FORMATS;
    let mut payload = Vec::with_capacity(4);
    push_be_u32(&mut payload, action.flag() | formats);
    extended_client_cut_text_message(&payload)
}

pub(super) fn extended_clipboard_provide_message(
    requested_formats: u32,
    content: &ExtendedClipboardContent,
) -> Result<Vec<u8>, String> {
    let formats = requested_formats & content.formats() & CLIPBOARD_KNOWN_FORMATS;
    if formats == 0 {
        return Err("VNC Extended Clipboard provide has no requested data.".to_string());
    }

    let mut uncompressed = Vec::new();
    for format in ExtendedClipboardFormat::ALL {
        if formats & format.flag() == 0 {
            continue;
        }
        let bytes = content
            .bytes(format)
            .ok_or_else(|| "VNC Extended Clipboard data changed while encoding.".to_string())?;
        ensure_clipboard_size(bytes.len())?;
        let size = u32::try_from(bytes.len())
            .map_err(|_| "VNC Extended Clipboard item is too large.".to_string())?;
        let next_size = uncompressed
            .len()
            .checked_add(4)
            .and_then(|size| size.checked_add(bytes.len()))
            .ok_or_else(|| "VNC Extended Clipboard payload size overflowed.".to_string())?;
        ensure_clipboard_size(next_size)?;
        push_be_u32(&mut uncompressed, size);
        uncompressed.extend_from_slice(&bytes);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&uncompressed)
        .map_err(|error| format!("VNC Extended Clipboard compression failed: {error}"))?;
    let compressed = encoder
        .finish()
        .map_err(|error| format!("VNC Extended Clipboard compression failed: {error}"))?;
    let total_size = compressed
        .len()
        .checked_add(4)
        .ok_or_else(|| "VNC Extended Clipboard payload size overflowed.".to_string())?;
    ensure_clipboard_size(total_size)?;

    let mut payload = Vec::with_capacity(total_size);
    push_be_u32(&mut payload, CLIPBOARD_ACTION_PROVIDE | formats);
    payload.extend_from_slice(&compressed);
    extended_client_cut_text_message(&payload)
}

fn parse_extended_clipboard_message(bytes: &[u8]) -> Result<ExtendedClipboardMessage, String> {
    if bytes.len() < 4 {
        return Err("VNC Extended Clipboard payload is shorter than its flags.".to_string());
    }
    let flags = be_u32(&bytes[..4]);
    if flags & !(CLIPBOARD_FORMAT_MASK | CLIPBOARD_ACTION_MASK) != 0 {
        return Err("VNC Extended Clipboard contains reserved flag bits.".to_string());
    }
    let action_flags = flags & CLIPBOARD_ACTION_MASK;
    let formats = flags & CLIPBOARD_FORMAT_MASK;
    let body = &bytes[4..];

    if action_flags & CLIPBOARD_ACTION_CAPS != 0 {
        if action_flags & !CLIPBOARD_KNOWN_ACTIONS != 0 {
            return Err("VNC Extended Clipboard caps contain unknown actions.".to_string());
        }
        return parse_extended_clipboard_caps(formats, action_flags, body);
    }

    if action_flags.count_ones() != 1 || action_flags & !CLIPBOARD_KNOWN_ACTIONS != 0 {
        return Err(
            "VNC Extended Clipboard message must contain exactly one known action.".to_string(),
        );
    }
    if formats & !CLIPBOARD_KNOWN_FORMATS != 0 {
        let detail = if formats & CLIPBOARD_FORMAT_FILES_RESERVED != 0 {
            "reserved files format"
        } else {
            "unknown format"
        };
        return Err(format!(
            "VNC Extended Clipboard {detail} has no defined wire format."
        ));
    }

    match action_flags {
        CLIPBOARD_ACTION_REQUEST => {
            require_empty_clipboard_action_body(body)?;
            Ok(ExtendedClipboardMessage::Request(formats))
        }
        CLIPBOARD_ACTION_PEEK => {
            require_empty_clipboard_action_body(body)?;
            if formats != 0 {
                return Err("VNC Extended Clipboard peek must not name formats.".to_string());
            }
            Ok(ExtendedClipboardMessage::Peek)
        }
        CLIPBOARD_ACTION_NOTIFY => {
            require_empty_clipboard_action_body(body)?;
            Ok(ExtendedClipboardMessage::Notify(formats))
        }
        CLIPBOARD_ACTION_PROVIDE => {
            parse_extended_clipboard_provide(formats, body).map(ExtendedClipboardMessage::Provide)
        }
        _ => Err("VNC Extended Clipboard action is unsupported.".to_string()),
    }
}

fn parse_extended_clipboard_caps(
    formats: u32,
    actions: u32,
    bytes: &[u8],
) -> Result<ExtendedClipboardMessage, String> {
    let size_count = usize::try_from(formats.count_ones())
        .map_err(|_| "VNC Extended Clipboard format count is invalid.".to_string())?;
    let expected_size = size_count
        .checked_mul(4)
        .ok_or_else(|| "VNC Extended Clipboard caps size overflowed.".to_string())?;
    if bytes.len() != expected_size {
        return Err("VNC Extended Clipboard caps size array is incomplete.".to_string());
    }

    let mut maximum_unsolicited_sizes = [0; 4];
    let mut offset = 0;
    for bit in 0..16 {
        let format_flag = 1u32 << bit;
        if formats & format_flag == 0 {
            continue;
        }
        let size = be_u32(&bytes[offset..offset + 4]);
        if size as usize > MAX_VNC_CLIPBOARD_BYTES {
            return Err("VNC Extended Clipboard advertised size exceeds 20 MiB.".to_string());
        }
        if bit < 4 {
            maximum_unsolicited_sizes[bit] = size;
        }
        offset += 4;
    }

    Ok(ExtendedClipboardMessage::Caps(
        ExtendedClipboardCapabilities {
            formats: formats & CLIPBOARD_KNOWN_FORMATS,
            actions: actions & !CLIPBOARD_ACTION_CAPS,
            maximum_unsolicited_sizes,
        },
    ))
}

fn parse_extended_clipboard_provide(
    formats: u32,
    bytes: &[u8],
) -> Result<ExtendedClipboardContent, String> {
    if formats == 0 {
        return Err("VNC Extended Clipboard provide has no formats.".to_string());
    }
    ensure_clipboard_size(bytes.len())?;

    let decoder = ZlibDecoder::new(Cursor::new(bytes));
    let mut limited = decoder.take((MAX_VNC_CLIPBOARD_BYTES + 1) as u64);
    let mut uncompressed = Vec::new();
    limited
        .read_to_end(&mut uncompressed)
        .map_err(|error| format!("VNC Extended Clipboard decompression failed: {error}"))?;
    ensure_clipboard_size(uncompressed.len())?;

    let mut content = ExtendedClipboardContent::empty();
    let mut offset = 0usize;
    for format in ExtendedClipboardFormat::ALL {
        if formats & format.flag() == 0 {
            continue;
        }
        let size_end = offset
            .checked_add(4)
            .ok_or_else(|| "VNC Extended Clipboard item offset overflowed.".to_string())?;
        if size_end > uncompressed.len() {
            return Err("VNC Extended Clipboard provide is missing an item size.".to_string());
        }
        let size = be_u32(&uncompressed[offset..size_end]) as usize;
        ensure_clipboard_size(size)?;
        let data_end = size_end
            .checked_add(size)
            .ok_or_else(|| "VNC Extended Clipboard item size overflowed.".to_string())?;
        if data_end > uncompressed.len() {
            return Err("VNC Extended Clipboard provide item is truncated.".to_string());
        }
        content.set_bytes(format, uncompressed[size_end..data_end].to_vec())?;
        offset = data_end;
    }
    if offset != uncompressed.len() {
        return Err("VNC Extended Clipboard provide has trailing data.".to_string());
    }
    Ok(content)
}

fn require_empty_clipboard_action_body(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        Ok(())
    } else {
        Err("VNC Extended Clipboard action contains unexpected data.".to_string())
    }
}

fn extended_client_cut_text_message(payload: &[u8]) -> Result<Vec<u8>, String> {
    ensure_clipboard_size(payload.len())?;
    let length = i32::try_from(payload.len())
        .map_err(|_| "VNC Extended Clipboard payload is too large.".to_string())?
        .checked_neg()
        .ok_or_else(|| "VNC Extended Clipboard length overflowed.".to_string())?;
    let mut message = Vec::with_capacity(8 + payload.len());
    message.push(6);
    message.extend_from_slice(&[0, 0, 0]);
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(payload);
    Ok(message)
}

fn ensure_clipboard_size(size: usize) -> Result<(), String> {
    if size <= MAX_VNC_CLIPBOARD_BYTES {
        Ok(())
    } else {
        Err("VNC clipboard payload exceeds the 20 MiB limit.".to_string())
    }
}

fn validate_clipboard_image_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_VNC_CLIPBOARD_DIMENSION
        || height > MAX_VNC_CLIPBOARD_DIMENSION
    {
        return Err("VNC clipboard image dimensions exceed the helper limit.".to_string());
    }
    let decoded_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "VNC clipboard image dimensions overflowed.".to_string())?;
    ensure_clipboard_size(decoded_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_payload_from_client_message(message: &[u8]) -> &[u8] {
        // The parser is called after the one-byte ServerCutText message type.
        &message[1..]
    }

    #[test]
    fn extended_clipboard_caps_round_trip_all_supported_formats() {
        let message = extended_clipboard_caps_message().unwrap();
        let mut payload = server_payload_from_client_message(&message);
        let parsed = read_server_cut_text(&mut payload).unwrap();
        let VncClipboardMessage::Extended(ExtendedClipboardMessage::Caps(capabilities)) = parsed
        else {
            panic!("expected Extended Clipboard caps");
        };

        assert!(capabilities.supports_format(ExtendedClipboardFormat::Text));
        assert!(capabilities.supports_format(ExtendedClipboardFormat::Rtf));
        assert!(capabilities.supports_format(ExtendedClipboardFormat::Html));
        assert!(capabilities.supports_format(ExtendedClipboardFormat::DibV5));
        assert!(capabilities.supports_action(ExtendedClipboardAction::Request));
        assert!(capabilities.supports_action(ExtendedClipboardAction::Provide));
    }

    #[test]
    fn extended_clipboard_provide_uses_independent_bounded_zlib_stream() {
        let content = ExtendedClipboardContent {
            text: Some("line one\nline two".to_string()),
            rtf: Some(br"{\rtf1 test}".to_vec()),
            html: Some(b"<b>test</b>".to_vec()),
            dib_v5: None,
        };
        let message =
            extended_clipboard_provide_message(CLIPBOARD_KNOWN_FORMATS, &content).unwrap();
        let mut payload = server_payload_from_client_message(&message);
        let parsed = read_server_cut_text(&mut payload).unwrap();
        let VncClipboardMessage::Extended(ExtendedClipboardMessage::Provide(parsed)) = parsed
        else {
            panic!("expected Extended Clipboard provide");
        };

        assert_eq!(parsed.text.as_deref(), Some("line one\nline two"));
        assert_eq!(parsed.rtf.as_deref(), Some(br"{\rtf1 test}".as_slice()));
        assert_eq!(parsed.html.as_deref(), Some(b"<b>test</b>".as_slice()));
    }

    #[test]
    fn extended_clipboard_rejects_minimum_signed_length_and_reserved_files() {
        let mut minimum = Vec::from([0, 0, 0]);
        minimum.extend_from_slice(&i32::MIN.to_be_bytes());
        assert!(read_server_cut_text(&mut minimum.as_slice()).is_err());

        let flags = CLIPBOARD_ACTION_NOTIFY | CLIPBOARD_FORMAT_FILES_RESERVED;
        let mut payload = Vec::from([0, 0, 0]);
        payload.extend_from_slice(&(-4i32).to_be_bytes());
        payload.extend_from_slice(&flags.to_be_bytes());
        assert!(read_server_cut_text(&mut payload.as_slice()).is_err());
    }

    #[test]
    fn extended_clipboard_rejects_truncated_and_oversized_payloads() {
        let mut truncated = [0, 0, 0, 0xff, 0xff, 0xff, 0xf8, 0, 0, 0, 0].as_slice();
        assert!(read_server_cut_text(&mut truncated).is_err());

        let oversized = -((MAX_VNC_CLIPBOARD_BYTES as i32) + 1);
        let mut header = Vec::from([0, 0, 0]);
        header.extend_from_slice(&oversized.to_be_bytes());
        assert!(read_server_cut_text(&mut header.as_slice()).is_err());
    }

    #[test]
    fn unnegotiated_image_is_non_fatal_and_sends_no_vendor_message() {
        let image = image::DynamicImage::new_rgba8(1, 1);
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let mut session = VncClipboardSession::default();
        let data = RemoteDesktopClipboardData::new(
            RemoteDesktopClipboardFormat::ImagePng,
            png.into_inner(),
        );

        assert!(session.set_local_data(&data).unwrap().is_empty());
    }

    #[test]
    fn clipboard_image_dimensions_and_decoded_allocation_are_bounded() {
        assert!(validate_clipboard_image_dimensions(1, 1).is_ok());
        assert!(validate_clipboard_image_dimensions(MAX_VNC_CLIPBOARD_DIMENSION + 1, 1).is_err());
        assert!(validate_clipboard_image_dimensions(4096, 4096).is_err());
    }
}
