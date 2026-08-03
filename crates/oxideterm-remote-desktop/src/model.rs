// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopVncSecurityPolicy {
    /// Rejects sessions unless transport encryption and peer identity are verified.
    #[default]
    RequireVerifiedEncryption,
    /// Allows encrypted VeNCrypt sessions whose anonymous TLS certificate is not an identity.
    AllowUnverifiedEncryption,
    /// Allows classic VNC security when the user accepts its weaker guarantees.
    AllowLegacy,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopVncSessionMode {
    #[default]
    Shared,
    Exclusive,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopVncImageQuality {
    Performance,
    #[default]
    Balanced,
    BestQuality,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopVncCompression {
    Low,
    #[default]
    Balanced,
    High,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopVncOptions {
    #[serde(default)]
    pub security_policy: RemoteDesktopVncSecurityPolicy,
    #[serde(default)]
    pub session_mode: RemoteDesktopVncSessionMode,
    #[serde(default)]
    pub image_quality: RemoteDesktopVncImageQuality,
    #[serde(default)]
    pub compression: RemoteDesktopVncCompression,
}

/// Describes the capabilities reported by a server after protocol negotiation.
///
/// RFB extensions are often discovered by observation instead of an exhaustive
/// server declaration, so absence must remain distinct from explicit rejection.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NegotiatedCapabilityStatus {
    #[default]
    Unknown,
    Supported,
    Unsupported,
}

impl NegotiatedCapabilityStatus {
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// Describes the cumulative capability evidence reported by a server session.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NegotiatedCapabilities {
    #[serde(default)]
    pub security_methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_security_method: Option<String>,
    #[serde(default)]
    pub encrypted: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub peer_identity_verified: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub resize: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub multi_monitor: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub extended_clipboard: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub extended_clipboard_formats: Vec<String>,
    #[serde(default)]
    pub tight: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub jpeg: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub continuous_updates: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub fence: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub last_rect: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub h264: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub qemu_audio: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub vendor_files: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub extended_key_events: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub extended_mouse_buttons: NegotiatedCapabilityStatus,
    #[serde(default)]
    pub lock_key_sync: NegotiatedCapabilityStatus,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopClipboardOptions {
    pub text: bool,
    pub images: bool,
    pub files: bool,
}

impl Default for RemoteDesktopClipboardOptions {
    fn default() -> Self {
        Self {
            // Text and image clipboard redirection preserve the existing RDP behavior.
            text: true,
            images: true,
            files: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopAudioOptions {
    pub playback: bool,
    pub capture: bool,
}

impl Default for RemoteDesktopAudioOptions {
    fn default() -> Self {
        Self {
            playback: true,
            // Microphone access must always be an explicit user choice.
            capture: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopDisplayOptions {
    pub use_all_monitors: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopSessionOptions {
    #[serde(default)]
    pub clipboard: RemoteDesktopClipboardOptions,
    #[serde(default)]
    pub audio: RemoteDesktopAudioOptions,
    #[serde(default)]
    pub display: RemoteDesktopDisplayOptions,
    #[serde(default)]
    pub vnc: RemoteDesktopVncOptions,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopMonitorOrientation {
    Landscape,
    Portrait,
    LandscapeFlipped,
    PortraitFlipped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopMonitor {
    pub stable_id: String,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    pub desktop_scale_factor: u32,
    pub device_scale_factor: u32,
    pub physical_width_mm: Option<u32>,
    pub physical_height_mm: Option<u32>,
    pub orientation: RemoteDesktopMonitorOrientation,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopMonitorLayout {
    #[serde(default)]
    pub monitors: Vec<RemoteDesktopMonitor>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopProtocol {
    #[default]
    Rdp,
    Vnc,
}

impl RemoteDesktopProtocol {
    /// Maps a quick-connect URI scheme to a supported protocol.
    pub fn from_scheme(scheme: &str) -> Option<Self> {
        match scheme.to_ascii_lowercase().as_str() {
            "rdp" => Some(Self::Rdp),
            "vnc" => Some(Self::Vnc),
            _ => None,
        }
    }

    pub const fn provider_id(self) -> &'static str {
        match self {
            Self::Rdp => "rdp",
            Self::Vnc => "vnc",
        }
    }

    pub const fn default_port(self) -> u16 {
        match self {
            Self::Rdp => 3389,
            Self::Vnc => 5900,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopEndpoint {
    pub host: String,
    pub port: u16,
}

impl RemoteDesktopEndpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    pub fn for_protocol(host: impl Into<String>, protocol: RemoteDesktopProtocol) -> Self {
        Self::new(host, protocol.default_port())
    }

    /// Parses a host authority without accepting paths, credentials, or fragments.
    pub fn parse_authority(authority: &str, default_port: u16) -> Option<Self> {
        if authority.is_empty()
            || authority.chars().any(|ch| {
                ch.is_whitespace() || ch.is_control() || matches!(ch, '/' | '?' | '#' | '@')
            })
        {
            return None;
        }

        let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
            let end = rest.find(']')?;
            let host = &rest[..end];
            let suffix = &rest[end + 1..];
            let port = if suffix.is_empty() {
                default_port
            } else {
                suffix.strip_prefix(':')?.parse::<u16>().ok()?
            };
            (host, port)
        } else if authority.matches(':').count() > 1 {
            // An unbracketed IPv6 address cannot carry an unambiguous port.
            (authority, default_port)
        } else if let Some((host, port)) = authority.rsplit_once(':') {
            (host, port.parse::<u16>().ok()?)
        } else {
            (authority, default_port)
        };
        if host.is_empty() || port == 0 {
            return None;
        }
        Some(Self::new(host, port))
    }

    /// Formats a host and port with brackets where IPv6 requires them.
    pub fn format_authority(&self) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            // Brackets preserve an IPv6 host/port boundary when rendered as a URI.
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("{host}:{}", self.port)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopConnectionProfile {
    pub id: String,
    pub label: String,
    pub protocol: RemoteDesktopProtocol,
    pub endpoint: RemoteDesktopEndpoint,
    pub username: Option<String>,
    pub domain: Option<String>,
    pub credential_ref: Option<String>,
    pub read_only: bool,
    #[serde(default)]
    pub session_options: RemoteDesktopSessionOptions,
}

impl RemoteDesktopConnectionProfile {
    /// Builds an ephemeral connection profile from an RDP or VNC URI.
    pub fn parse_quick_connect(query: &str) -> Option<Self> {
        let (scheme, authority) = query.split_once("://")?;
        let protocol = RemoteDesktopProtocol::from_scheme(scheme)?;
        let endpoint = RemoteDesktopEndpoint::parse_authority(authority, protocol.default_port())?;
        let label = format!(
            "{}://{}",
            protocol.provider_id(),
            endpoint.format_authority()
        );

        Some(Self {
            id: format!(
                "quick-{}-{}-{}",
                protocol.provider_id(),
                endpoint.host,
                endpoint.port
            ),
            label,
            protocol,
            endpoint,
            username: None,
            domain: None,
            credential_ref: None,
            read_only: false,
            session_options: RemoteDesktopSessionOptions::default(),
        })
    }

    /// Formats the canonical quick-connect URI shown by UI adapters.
    pub fn quick_connect_target(&self) -> String {
        format!(
            "{}://{}",
            self.protocol.provider_id(),
            self.endpoint.format_authority()
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopSize {
    pub width: u32,
    pub height: u32,
}

impl RemoteDesktopSize {
    pub const MIN_WIDTH: u32 = 200;
    pub const MIN_HEIGHT: u32 = 120;
    pub const MAX_DIMENSION: u32 = 8192;

    pub fn clamped(width: u32, height: u32) -> Self {
        Self {
            width: width.clamp(Self::MIN_WIDTH, Self::MAX_DIMENSION),
            height: height.clamp(Self::MIN_HEIGHT, Self::MAX_DIMENSION),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RemoteDesktopSessionId(String);

impl RemoteDesktopSessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RemoteDesktopSessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod quick_connect_tests {
    use super::*;

    #[test]
    fn quick_connect_uses_protocol_default_ports() {
        let vnc = RemoteDesktopConnectionProfile::parse_quick_connect("vnc://example.com").unwrap();
        let rdp = RemoteDesktopConnectionProfile::parse_quick_connect("rdp://example.com").unwrap();

        assert_eq!(vnc.protocol, RemoteDesktopProtocol::Vnc);
        assert_eq!(
            vnc.endpoint,
            RemoteDesktopEndpoint::new("example.com", 5900)
        );
        assert_eq!(vnc.label, "vnc://example.com:5900");
        assert_eq!(rdp.endpoint.port, 3389);
    }

    #[test]
    fn quick_connect_accepts_explicit_port_and_ipv6() {
        let explicit =
            RemoteDesktopConnectionProfile::parse_quick_connect("vnc://example.com:5901").unwrap();
        let ipv6 = RemoteDesktopConnectionProfile::parse_quick_connect("vnc://[::1]:5902").unwrap();

        assert_eq!(explicit.endpoint.port, 5901);
        assert_eq!(ipv6.endpoint, RemoteDesktopEndpoint::new("::1", 5902));
        assert_eq!(ipv6.quick_connect_target(), "vnc://[::1]:5902");
    }

    #[test]
    fn quick_connect_rejects_unsafe_or_ambiguous_authorities() {
        for query in [
            "vnc://example.com/screen",
            "vnc://user@example.com",
            "vnc://example.com:0",
            "vnc://example.com:not-a-port",
            "vnc://example .com",
            "ssh://example.com",
        ] {
            assert!(RemoteDesktopConnectionProfile::parse_quick_connect(query).is_none());
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopSessionStatus {
    Idle,
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopFrameFormat {
    Rgba8,
    Bgra8,
}

impl RemoteDesktopFrameFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 | Self::Bgra8 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl RemoteDesktopRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn expected_len(self, format: RemoteDesktopFrameFormat) -> Option<usize> {
        usize::try_from(self.width)
            .ok()?
            .checked_mul(usize::try_from(self.height).ok()?)?
            .checked_mul(format.bytes_per_pixel())
    }

    pub fn fits_in(self, size: RemoteDesktopSize) -> bool {
        let Some(right) = self.x.checked_add(self.width) else {
            return false;
        };
        let Some(bottom) = self.y.checked_add(self.height) else {
            return false;
        };
        self.width > 0
            && self.height > 0
            && self.x < size.width
            && self.y < size.height
            && right <= size.width
            && bottom <= size.height
    }

    pub fn union(self, other: Self) -> Option<Self> {
        let right = self
            .x
            .checked_add(self.width)?
            .max(other.x.checked_add(other.width)?);
        let bottom = self
            .y
            .checked_add(self.height)?
            .max(other.y.checked_add(other.height)?);
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Some(Self {
            x,
            y,
            width: right.checked_sub(x)?,
            height: bottom.checked_sub(y)?,
        })
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self
            .x
            .checked_add(self.width)?
            .min(other.x.checked_add(other.width)?);
        let bottom = self
            .y
            .checked_add(self.height)?
            .min(other.y.checked_add(other.height)?);
        if right <= x || bottom <= y {
            return None;
        }
        Some(Self {
            x,
            y,
            width: right.checked_sub(x)?,
            height: bottom.checked_sub(y)?,
        })
    }

    fn area(self) -> Option<u64> {
        Some(u64::from(self.width).checked_mul(u64::from(self.height))?)
    }

    fn union_is_fully_covered_by(self, other: Self) -> bool {
        let Some(union) = self.union(other) else {
            return false;
        };
        let Some(union_area) = union.area() else {
            return false;
        };
        let Some(self_area) = self.area() else {
            return false;
        };
        let Some(other_area) = other.area() else {
            return false;
        };
        let overlap_area = self
            .intersection(other)
            .and_then(Self::area)
            .unwrap_or_default();
        let Some(covered_area) = self_area
            .checked_add(other_area)
            .and_then(|area| area.checked_sub(overlap_area))
        else {
            return false;
        };
        union_area == covered_area
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopFrameCompression {
    None,
}

impl Default for RemoteDesktopFrameCompression {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopFrame {
    pub size: RemoteDesktopSize,
    pub format: RemoteDesktopFrameFormat,
    #[serde(default)]
    pub graphics_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<u64>,
    #[serde(with = "base64_frame_bytes")]
    pub bytes: Vec<u8>,
}

impl RemoteDesktopFrame {
    pub fn new(size: RemoteDesktopSize, format: RemoteDesktopFrameFormat, bytes: Vec<u8>) -> Self {
        Self {
            size,
            format,
            graphics_epoch: 0,
            trace_id: None,
            bytes,
        }
    }

    pub fn with_graphics_epoch(mut self, graphics_epoch: u64) -> Self {
        self.graphics_epoch = graphics_epoch;
        self
    }

    pub fn with_trace_id(mut self, trace_id: u64) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    pub fn expected_len(size: RemoteDesktopSize) -> Option<usize> {
        let pixels = usize::try_from(size.width)
            .ok()?
            .checked_mul(usize::try_from(size.height).ok()?)?;
        pixels.checked_mul(RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel())
    }

    pub fn is_complete(&self) -> bool {
        Self::expected_len(self.size).is_some_and(|expected| expected == self.bytes.len())
    }

    pub fn apply_update(&mut self, update: &RemoteDesktopFrameUpdate) -> bool {
        if self.graphics_epoch != update.graphics_epoch
            || self.size != update.size
            || self.format != update.format
            || update.compression != RemoteDesktopFrameCompression::None
            || !self.is_complete()
            || !update.is_complete()
            || !update.rect.fits_in(update.size)
        {
            return false;
        }

        copy_rect_bytes(
            &mut self.bytes,
            self.size.width,
            update.rect,
            &update.bytes,
            update.rect.width,
            self.format.bytes_per_pixel(),
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopFrameUpdate {
    pub size: RemoteDesktopSize,
    pub rect: RemoteDesktopRect,
    pub format: RemoteDesktopFrameFormat,
    #[serde(default)]
    pub graphics_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<u64>,
    #[serde(default)]
    pub compression: RemoteDesktopFrameCompression,
    #[serde(with = "base64_frame_bytes")]
    pub bytes: Vec<u8>,
}

impl RemoteDesktopFrameUpdate {
    pub fn new(
        size: RemoteDesktopSize,
        rect: RemoteDesktopRect,
        format: RemoteDesktopFrameFormat,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            size,
            rect,
            format,
            graphics_epoch: 0,
            trace_id: None,
            compression: RemoteDesktopFrameCompression::None,
            bytes,
        }
    }

    pub fn with_graphics_epoch(mut self, graphics_epoch: u64) -> Self {
        self.graphics_epoch = graphics_epoch;
        self
    }

    pub fn with_trace_id(mut self, trace_id: u64) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    pub fn expected_len(&self) -> Option<usize> {
        self.rect.expected_len(self.format)
    }

    pub fn is_complete(&self) -> bool {
        self.expected_len()
            .is_some_and(|expected| expected == self.bytes.len())
            && self.rect.fits_in(self.size)
    }

    pub fn merge(&mut self, incoming: &Self) -> bool {
        if self.graphics_epoch != incoming.graphics_epoch
            || self.size != incoming.size
            || self.format != incoming.format
            || self.compression != RemoteDesktopFrameCompression::None
            || incoming.compression != RemoteDesktopFrameCompression::None
            || !self.is_complete()
            || !incoming.is_complete()
        {
            return false;
        }
        if !self.rect.union_is_fully_covered_by(incoming.rect) {
            return false;
        }
        let Some(union) = self.rect.union(incoming.rect) else {
            return false;
        };
        let Some(len) = union.expected_len(self.format) else {
            return false;
        };
        let mut bytes = vec![0; len];
        let pixel_size = self.format.bytes_per_pixel();
        if !copy_rect_bytes(
            &mut bytes,
            union.width,
            RemoteDesktopRect::new(
                self.rect.x - union.x,
                self.rect.y - union.y,
                self.rect.width,
                self.rect.height,
            ),
            &self.bytes,
            self.rect.width,
            pixel_size,
        ) {
            return false;
        }
        if !copy_rect_bytes(
            &mut bytes,
            union.width,
            RemoteDesktopRect::new(
                incoming.rect.x - union.x,
                incoming.rect.y - union.y,
                incoming.rect.width,
                incoming.rect.height,
            ),
            &incoming.bytes,
            incoming.rect.width,
            pixel_size,
        ) {
            return false;
        }
        self.rect = union;
        self.trace_id = incoming.trace_id.or(self.trace_id);
        self.bytes = bytes;
        true
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopCursorShape {
    pub size: RemoteDesktopSize,
    pub hotspot_x: u32,
    pub hotspot_y: u32,
    pub format: RemoteDesktopFrameFormat,
    #[serde(with = "base64_frame_bytes")]
    pub bytes: Vec<u8>,
}

impl RemoteDesktopCursorShape {
    pub fn new(
        size: RemoteDesktopSize,
        hotspot_x: u32,
        hotspot_y: u32,
        format: RemoteDesktopFrameFormat,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            size,
            hotspot_x,
            hotspot_y,
            format,
            bytes,
        }
    }

    pub fn expected_len(&self) -> Option<usize> {
        RemoteDesktopFrame::expected_len(self.size)
    }

    pub fn is_complete(&self) -> bool {
        self.expected_len()
            .is_some_and(|expected| expected == self.bytes.len())
            && self.hotspot_x < self.size.width
            && self.hotspot_y < self.size.height
    }
}

fn copy_rect_bytes(
    dst: &mut [u8],
    dst_width: u32,
    dst_rect: RemoteDesktopRect,
    src: &[u8],
    src_width: u32,
    pixel_size: usize,
) -> bool {
    let Ok(dst_width) = usize::try_from(dst_width) else {
        return false;
    };
    let Ok(dst_x) = usize::try_from(dst_rect.x) else {
        return false;
    };
    let Ok(dst_y) = usize::try_from(dst_rect.y) else {
        return false;
    };
    let Ok(rect_width) = usize::try_from(dst_rect.width) else {
        return false;
    };
    let Ok(rect_height) = usize::try_from(dst_rect.height) else {
        return false;
    };
    let Ok(src_width) = usize::try_from(src_width) else {
        return false;
    };
    let Some(dst_stride) = dst_width.checked_mul(pixel_size) else {
        return false;
    };
    let Some(src_stride) = src_width.checked_mul(pixel_size) else {
        return false;
    };
    let Some(row_len) = rect_width.checked_mul(pixel_size) else {
        return false;
    };

    for row in 0..rect_height {
        let Some(dst_offset) = dst_y
            .checked_add(row)
            .and_then(|y| y.checked_mul(dst_stride))
            .and_then(|offset| offset.checked_add(dst_x.checked_mul(pixel_size)?))
        else {
            return false;
        };
        let Some(src_offset) = row.checked_mul(src_stride) else {
            return false;
        };
        let Some(dst_end) = dst_offset.checked_add(row_len) else {
            return false;
        };
        let Some(src_end) = src_offset.checked_add(row_len) else {
            return false;
        };
        let Some(dst_row) = dst.get_mut(dst_offset..dst_end) else {
            return false;
        };
        let Some(src_row) = src.get(src_offset..src_end) else {
            return false;
        };
        dst_row.copy_from_slice(src_row);
    }
    true
}

mod base64_frame_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    use super::BASE64_STANDARD;
    use base64::Engine as _;

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Frame payloads are large, so the JSON helper protocol uses base64
        // instead of expanding every byte into a decimal array element.
        BASE64_STANDARD.encode(bytes).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        BASE64_STANDARD
            .decode(encoded)
            .map_err(|error| D::Error::custom(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocols_provide_provider_ids_and_default_ports() {
        assert_eq!(RemoteDesktopProtocol::Rdp.provider_id(), "rdp");
        assert_eq!(RemoteDesktopProtocol::Rdp.default_port(), 3389);
        assert_eq!(RemoteDesktopProtocol::Vnc.provider_id(), "vnc");
        assert_eq!(RemoteDesktopProtocol::Vnc.default_port(), 5900);
    }

    #[test]
    fn vnc_options_default_to_verified_encryption_and_balanced_rendering() {
        let options = RemoteDesktopVncOptions::default();

        assert_eq!(
            options.security_policy,
            RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption
        );
        assert_eq!(options.session_mode, RemoteDesktopVncSessionMode::Shared);
        assert_eq!(
            options.image_quality,
            RemoteDesktopVncImageQuality::Balanced
        );
        assert_eq!(options.compression, RemoteDesktopVncCompression::Balanced);
    }

    #[test]
    fn older_session_options_receive_safe_vnc_defaults() {
        let options: RemoteDesktopSessionOptions = serde_json::from_str(
            r#"{"clipboard":{"text":true,"images":false,"files":false},"audio":{"playback":false,"capture":false},"display":{"useAllMonitors":false}}"#,
        )
        .unwrap();

        assert_eq!(
            options.vnc.security_policy,
            RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption
        );
        assert_eq!(
            options.vnc.session_mode,
            RemoteDesktopVncSessionMode::Shared
        );
    }

    #[test]
    fn unobserved_negotiated_capabilities_remain_unknown() {
        let capabilities: NegotiatedCapabilities = serde_json::from_str("{}").unwrap();

        assert_eq!(capabilities.resize, NegotiatedCapabilityStatus::Unknown);
        assert_eq!(
            capabilities.extended_clipboard,
            NegotiatedCapabilityStatus::Unknown
        );
        assert_eq!(capabilities.h264, NegotiatedCapabilityStatus::Unknown);
    }

    #[test]
    fn frame_completeness_uses_four_bytes_per_pixel() {
        let size = RemoteDesktopSize {
            width: 2,
            height: 2,
        };
        let complete = RemoteDesktopFrame::new(size, RemoteDesktopFrameFormat::Rgba8, vec![0; 16]);
        let short = RemoteDesktopFrame::new(size, RemoteDesktopFrameFormat::Rgba8, vec![0; 15]);

        assert!(complete.is_complete());
        assert!(!short.is_complete());
    }

    #[test]
    fn frame_json_uses_base64_bytes() {
        let frame = RemoteDesktopFrame::new(
            RemoteDesktopSize {
                width: 1,
                height: 1,
            },
            RemoteDesktopFrameFormat::Rgba8,
            vec![1, 2, 3, 4],
        );

        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: RemoteDesktopFrame = serde_json::from_str(&encoded).unwrap();

        assert!(encoded.contains("\"bytes\":\"AQIDBA==\""));
        assert!(!encoded.contains("[1,2,3,4]"));
        assert_eq!(decoded, frame);
    }

    #[test]
    fn frame_update_patches_backing_frame_rows() {
        let size = RemoteDesktopSize {
            width: 3,
            height: 2,
        };
        let mut frame = RemoteDesktopFrame::new(size, RemoteDesktopFrameFormat::Rgba8, vec![0; 24]);
        let update = RemoteDesktopFrameUpdate::new(
            size,
            RemoteDesktopRect::new(1, 0, 2, 2),
            RemoteDesktopFrameFormat::Rgba8,
            vec![
                1, 1, 1, 1, 2, 2, 2, 2, //
                3, 3, 3, 3, 4, 4, 4, 4,
            ],
        );

        assert!(frame.apply_update(&update));

        assert_eq!(
            frame.bytes,
            vec![
                0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, //
                0, 0, 0, 0, 3, 3, 3, 3, 4, 4, 4, 4,
            ]
        );
    }

    #[test]
    fn adjacent_frame_updates_merge_into_union_rect() {
        let size = RemoteDesktopSize {
            width: 4,
            height: 2,
        };
        let mut update = RemoteDesktopFrameUpdate::new(
            size,
            RemoteDesktopRect::new(0, 0, 1, 1),
            RemoteDesktopFrameFormat::Rgba8,
            vec![1, 1, 1, 1],
        );
        let incoming = RemoteDesktopFrameUpdate::new(
            size,
            RemoteDesktopRect::new(1, 0, 1, 1),
            RemoteDesktopFrameFormat::Rgba8,
            vec![2, 2, 2, 2],
        );

        assert!(update.merge(&incoming));

        assert_eq!(update.rect, RemoteDesktopRect::new(0, 0, 2, 1));
        assert_eq!(update.bytes, vec![1, 1, 1, 1, 2, 2, 2, 2]);
    }

    #[test]
    fn sparse_frame_updates_do_not_merge_into_zero_filled_holes() {
        let size = RemoteDesktopSize {
            width: 4,
            height: 2,
        };
        let mut update = RemoteDesktopFrameUpdate::new(
            size,
            RemoteDesktopRect::new(0, 0, 1, 1),
            RemoteDesktopFrameFormat::Rgba8,
            vec![1, 1, 1, 1],
        );
        let original = update.clone();
        let incoming = RemoteDesktopFrameUpdate::new(
            size,
            RemoteDesktopRect::new(2, 0, 1, 1),
            RemoteDesktopFrameFormat::Rgba8,
            vec![2, 2, 2, 2],
        );

        assert!(!update.merge(&incoming));
        assert_eq!(update, original);
    }

    #[test]
    fn cursor_shape_requires_complete_bytes_and_valid_hotspot() {
        let size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };
        let complete =
            RemoteDesktopCursorShape::new(size, 1, 0, RemoteDesktopFrameFormat::Rgba8, vec![0; 8]);
        let short =
            RemoteDesktopCursorShape::new(size, 1, 0, RemoteDesktopFrameFormat::Rgba8, vec![0; 4]);
        let bad_hotspot =
            RemoteDesktopCursorShape::new(size, 2, 0, RemoteDesktopFrameFormat::Rgba8, vec![0; 8]);

        assert!(complete.is_complete());
        assert!(!short.is_complete());
        assert!(!bad_hotspot.is_complete());
    }
}
