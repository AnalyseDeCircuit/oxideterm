// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopProtocol {
    #[default]
    Rdp,
    Vnc,
}

impl RemoteDesktopProtocol {
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
    #[serde(with = "base64_frame_bytes")]
    pub bytes: Vec<u8>,
}

impl RemoteDesktopFrame {
    pub fn new(size: RemoteDesktopSize, format: RemoteDesktopFrameFormat, bytes: Vec<u8>) -> Self {
        Self {
            size,
            format,
            bytes,
        }
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
        if self.size != update.size
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
            compression: RemoteDesktopFrameCompression::None,
            bytes,
        }
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
        if self.size != incoming.size
            || self.format != incoming.format
            || self.compression != RemoteDesktopFrameCompression::None
            || incoming.compression != RemoteDesktopFrameCompression::None
            || !self.is_complete()
            || !incoming.is_complete()
        {
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
    fn frame_updates_merge_into_union_rect() {
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
            RemoteDesktopRect::new(2, 0, 1, 1),
            RemoteDesktopFrameFormat::Rgba8,
            vec![2, 2, 2, 2],
        );

        assert!(update.merge(&incoming));

        assert_eq!(update.rect, RemoteDesktopRect::new(0, 0, 3, 1));
        assert_eq!(update.bytes, vec![1, 1, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2]);
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
