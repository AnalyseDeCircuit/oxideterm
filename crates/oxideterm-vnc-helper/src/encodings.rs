// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_remote_desktop::{
    RemoteDesktopVncCompression, RemoteDesktopVncImageQuality, RemoteDesktopVncOptions,
};

use super::VNC_ENCODING_EXTENDED_CLIPBOARD;

pub(super) const VNC_ENCODING_RAW: i32 = 0;
pub(super) const VNC_ENCODING_COPY_RECT: i32 = 1;
pub(super) const VNC_ENCODING_HEXTILE: i32 = 5;
pub(super) const VNC_ENCODING_TIGHT: i32 = 7;
pub(super) const VNC_ENCODING_ZRLE: i32 = 16;
pub(super) const VNC_ENCODING_OPEN_H264: i32 = 50;

pub(super) const VNC_ENCODING_DESKTOP_SIZE: i32 = -223;
pub(super) const VNC_ENCODING_LAST_RECT: i32 = -224;
pub(super) const VNC_ENCODING_CURSOR: i32 = -239;
pub(super) const VNC_ENCODING_X_CURSOR: i32 = -240;
pub(super) const VNC_ENCODING_QEMU_EXTENDED_KEY_EVENT: i32 = -258;
pub(super) const VNC_ENCODING_QEMU_AUDIO: i32 = -259;
pub(super) const VNC_ENCODING_QEMU_LED_STATE: i32 = -261;
pub(super) const VNC_ENCODING_EXTENDED_DESKTOP_SIZE: i32 = -308;
pub(super) const VNC_ENCODING_FENCE: i32 = -312;
pub(super) const VNC_ENCODING_CONTINUOUS_UPDATES: i32 = -313;
pub(super) const VNC_ENCODING_EXTENDED_MOUSE_BUTTONS: i32 = -316;
pub(super) const VNC_ENCODING_VMWARE_LED_STATE: i32 = 0x574d_5668;

const VNC_ENCODING_QUALITY_LEVEL_ZERO: i32 = -32;
const VNC_ENCODING_COMPRESSION_LEVEL_ZERO: i32 = -256;
const VNC_LOW_IMAGE_QUALITY_LEVEL: u8 = 3;
const VNC_BALANCED_IMAGE_QUALITY_LEVEL: u8 = 6;
const VNC_BEST_IMAGE_QUALITY_LEVEL: u8 = 9;
const VNC_LOW_COMPRESSION_LEVEL: u8 = 2;
const VNC_BALANCED_COMPRESSION_LEVEL: u8 = 6;
const VNC_HIGH_COMPRESSION_LEVEL: u8 = 9;

/// Keeps server preference hints separate from capabilities that the server
/// later proves by sending extension-specific messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct VncEncodingPreferences {
    pub(super) image_quality_level: u8,
    pub(super) compression_level: u8,
}

impl VncEncodingPreferences {
    pub(super) fn from_options(options: RemoteDesktopVncOptions) -> Self {
        let image_quality_level = match options.image_quality {
            RemoteDesktopVncImageQuality::Performance => VNC_LOW_IMAGE_QUALITY_LEVEL,
            RemoteDesktopVncImageQuality::Balanced => VNC_BALANCED_IMAGE_QUALITY_LEVEL,
            RemoteDesktopVncImageQuality::BestQuality => VNC_BEST_IMAGE_QUALITY_LEVEL,
        };
        let compression_level = match options.compression {
            RemoteDesktopVncCompression::Low => VNC_LOW_COMPRESSION_LEVEL,
            RemoteDesktopVncCompression::Balanced => VNC_BALANCED_COMPRESSION_LEVEL,
            RemoteDesktopVncCompression::High => VNC_HIGH_COMPRESSION_LEVEL,
        };
        Self {
            image_quality_level,
            compression_level,
        }
    }
}

impl Default for VncEncodingPreferences {
    fn default() -> Self {
        Self::from_options(RemoteDesktopVncOptions::default())
    }
}

pub(super) fn advertised_vnc_encodings(
    preferences: VncEncodingPreferences,
    h264_available: bool,
) -> Vec<i32> {
    let mut encodings = Vec::with_capacity(21);

    // CopyRect is lossless and cheaper than transmitting unchanged pixels.
    encodings.push(VNC_ENCODING_COPY_RECT);
    if h264_available {
        encodings.push(VNC_ENCODING_OPEN_H264);
    }
    encodings.extend([
        VNC_ENCODING_TIGHT,
        VNC_ENCODING_ZRLE,
        VNC_ENCODING_HEXTILE,
        VNC_ENCODING_RAW,
        quality_level_encoding(preferences.image_quality_level),
        compression_level_encoding(preferences.compression_level),
        VNC_ENCODING_EXTENDED_DESKTOP_SIZE,
        VNC_ENCODING_DESKTOP_SIZE,
        VNC_ENCODING_LAST_RECT,
        VNC_ENCODING_FENCE,
        VNC_ENCODING_CONTINUOUS_UPDATES,
        VNC_ENCODING_EXTENDED_CLIPBOARD,
        VNC_ENCODING_QEMU_AUDIO,
        VNC_ENCODING_QEMU_EXTENDED_KEY_EVENT,
        VNC_ENCODING_QEMU_LED_STATE,
        VNC_ENCODING_VMWARE_LED_STATE,
        VNC_ENCODING_EXTENDED_MOUSE_BUTTONS,
        VNC_ENCODING_CURSOR,
        VNC_ENCODING_X_CURSOR,
    ]);
    encodings
}

fn quality_level_encoding(level: u8) -> i32 {
    debug_assert!(level <= 9);
    VNC_ENCODING_QUALITY_LEVEL_ZERO + i32::from(level)
}

fn compression_level_encoding(level: u8) -> i32 {
    debug_assert!(level <= 9);
    VNC_ENCODING_COMPRESSION_LEVEL_ZERO + i32::from(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_encodings_only_include_h264_when_decoder_is_available() {
        let without_h264 = advertised_vnc_encodings(VncEncodingPreferences::default(), false);
        let with_h264 = advertised_vnc_encodings(VncEncodingPreferences::default(), true);

        assert!(!without_h264.contains(&VNC_ENCODING_OPEN_H264));
        assert!(with_h264.contains(&VNC_ENCODING_OPEN_H264));
        assert!(without_h264.contains(&VNC_ENCODING_TIGHT));
        assert!(without_h264.contains(&VNC_ENCODING_LAST_RECT));
        assert!(without_h264.contains(&VNC_ENCODING_FENCE));
        assert!(without_h264.contains(&VNC_ENCODING_CONTINUOUS_UPDATES));
    }

    #[test]
    fn vnc_preferences_map_to_quality_and_compression_pseudo_encodings() {
        let options = RemoteDesktopVncOptions {
            image_quality: RemoteDesktopVncImageQuality::BestQuality,
            compression: RemoteDesktopVncCompression::Low,
            ..RemoteDesktopVncOptions::default()
        };
        let encodings =
            advertised_vnc_encodings(VncEncodingPreferences::from_options(options), false);

        assert!(encodings.contains(&-23));
        assert!(encodings.contains(&-254));
    }
}
