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
        pixels.checked_mul(4)
    }

    pub fn is_complete(&self) -> bool {
        Self::expected_len(self.size).is_some_and(|expected| expected == self.bytes.len())
    }
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
}
