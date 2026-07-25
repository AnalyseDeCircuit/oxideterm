// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashMap, path::Path};

use openh264::{
    OpenH264API,
    decoder::{Decoder, DecoderConfig},
    formats::YUVSource,
};

use super::*;

pub(super) const OPENH264_LIBRARY_PATH_ENV: &str = "OXIDETERM_OPENH264_LIBRARY";
const OPEN_H264_RESET_CONTEXT: u32 = 1 << 0;
const OPEN_H264_RESET_ALL_CONTEXTS: u32 = 1 << 1;
const MAX_OPEN_H264_CONTEXTS: usize = 64;
const MAX_OPEN_H264_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct DecodedH264Frame {
    pub(super) rgba: Vec<u8>,
    pub(super) width: usize,
    pub(super) height: usize,
}

pub(super) trait VncH264Decoder: Send {
    fn decode(&mut self, bitstream: &[u8]) -> Result<Option<DecodedH264Frame>, String>;
}

pub(super) trait VncH264DecoderFactory: Send {
    fn create(&mut self) -> Result<Box<dyn VncH264Decoder>, String>;
}

struct OpenH264RectangleDecoder {
    decoder: Decoder,
}

impl OpenH264RectangleDecoder {
    fn from_library_path(path: &Path) -> Result<Self, String> {
        let api = OpenH264API::from_blob_path(path)
            .map_err(|error| format!("OpenH264 library load failed: {error}"))?;
        let decoder = Decoder::with_api_config(api, DecoderConfig::new())
            .map_err(|error| format!("OpenH264 decoder setup failed: {error}"))?;
        Ok(Self { decoder })
    }
}

impl VncH264Decoder for OpenH264RectangleDecoder {
    fn decode(&mut self, bitstream: &[u8]) -> Result<Option<DecodedH264Frame>, String> {
        let Some(yuv) = self
            .decoder
            .decode(bitstream)
            .map_err(|error| format!("OpenH264 rectangle decode failed: {error}"))?
        else {
            return Ok(None);
        };
        let (width, height) = yuv.dimensions();
        let byte_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "OpenH264 output dimensions overflowed.".to_string())?;
        if byte_len > MAX_VNC_FRAME_BYTES {
            return Err("OpenH264 output exceeds the helper frame limit.".to_string());
        }
        let mut rgba = vec![0; byte_len];
        yuv.write_rgba8(&mut rgba);
        Ok(Some(DecodedH264Frame {
            rgba,
            width,
            height,
        }))
    }
}

struct OpenH264DecoderFactory {
    library_path: std::path::PathBuf,
    preloaded: Option<Box<dyn VncH264Decoder>>,
}

impl OpenH264DecoderFactory {
    fn from_library_path(path: &Path) -> Result<Self, String> {
        // Probe and retain the first decoder before advertising encoding 50 so
        // the advertised set always reflects a real decoder instance.
        let decoder = OpenH264RectangleDecoder::from_library_path(path)?;
        Ok(Self {
            library_path: path.to_path_buf(),
            preloaded: Some(Box::new(decoder)),
        })
    }
}

impl VncH264DecoderFactory for OpenH264DecoderFactory {
    fn create(&mut self) -> Result<Box<dyn VncH264Decoder>, String> {
        if let Some(decoder) = self.preloaded.take() {
            return Ok(decoder);
        }
        OpenH264RectangleDecoder::from_library_path(&self.library_path)
            .map(|decoder| Box::new(decoder) as Box<dyn VncH264Decoder>)
    }
}

type VncH264ContextKey = (u16, u16, u16, u16);

pub(super) struct VncH264State {
    factory: Box<dyn VncH264DecoderFactory>,
    contexts: HashMap<VncH264ContextKey, Box<dyn VncH264Decoder>>,
}

impl VncH264State {
    pub(super) fn from_env() -> Result<Option<Self>, String> {
        let Some(path) = std::env::var_os(OPENH264_LIBRARY_PATH_ENV) else {
            return Ok(None);
        };
        Self::from_library_path(Path::new(&path)).map(Some)
    }

    pub(super) fn from_library_path(path: &Path) -> Result<Self, String> {
        let factory = OpenH264DecoderFactory::from_library_path(path)?;
        Ok(Self::with_factory(Box::new(factory)))
    }

    pub(super) fn with_factory(factory: Box<dyn VncH264DecoderFactory>) -> Self {
        Self {
            factory,
            contexts: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn context_count(&self) -> usize {
        self.contexts.len()
    }

    pub(super) fn decode_rectangle(
        &mut self,
        reader: &mut impl Read,
        rect: RfbRect,
    ) -> Result<Option<Vec<u8>>, String> {
        let payload_len = read_be_u32(reader)
            .map_err(|error| format!("VNC Open H.264 length read failed: {error}"))?
            as usize;
        let flags = read_be_u32(reader)
            .map_err(|error| format!("VNC Open H.264 flags read failed: {error}"))?;
        if payload_len > MAX_OPEN_H264_PAYLOAD_BYTES || payload_len > MAX_VNC_FRAME_BYTES {
            return Err("VNC Open H.264 payload exceeds the helper limit.".to_string());
        }
        let payload = read_exact_vec(reader, payload_len)
            .map_err(|error| format!("VNC Open H.264 payload read failed: {error}"))?;
        self.decode_payload(rect, flags, &payload)
    }

    pub(super) fn decode_payload(
        &mut self,
        rect: RfbRect,
        flags: u32,
        payload: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        let key = h264_context_key(rect);
        if flags & OPEN_H264_RESET_ALL_CONTEXTS != 0 {
            self.contexts.clear();
        }
        if flags & OPEN_H264_RESET_CONTEXT != 0 {
            self.contexts.remove(&key);
        }
        if payload.is_empty() {
            return Ok(None);
        }
        if rect.width == 0 || rect.height == 0 {
            return Err("VNC Open H.264 data rectangle has an empty area.".to_string());
        }

        if !self.contexts.contains_key(&key) {
            if self.contexts.len() >= MAX_OPEN_H264_CONTEXTS {
                return Err("VNC Open H.264 context count exceeds the helper limit.".to_string());
            }
            let decoder = self.factory.create()?;
            self.contexts.insert(key, decoder);
        }
        let decoder = self
            .contexts
            .get_mut(&key)
            .ok_or_else(|| "VNC Open H.264 context became unavailable.".to_string())?;
        let frame = match decoder.decode(payload) {
            Ok(frame) => frame,
            Err(error) => {
                // A corrupted differential stream must not remain reusable.
                self.contexts.remove(&key);
                return Err(error);
            }
        };
        let Some(frame) = frame else {
            return Ok(None);
        };
        h264_frame_to_bgra(rect, frame).map(Some)
    }
}

fn h264_context_key(rect: RfbRect) -> VncH264ContextKey {
    (rect.x, rect.y, rect.width, rect.height)
}

fn h264_frame_to_bgra(rect: RfbRect, frame: DecodedH264Frame) -> Result<Vec<u8>, String> {
    let rect_width = usize::from(rect.width);
    let rect_height = usize::from(rect.height);
    if frame.width < rect_width || frame.height < rect_height {
        return Err("OpenH264 output is smaller than its VNC rectangle.".to_string());
    }
    let expected_frame_len = frame
        .width
        .checked_mul(frame.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "OpenH264 output dimensions overflowed.".to_string())?;
    if frame.rgba.len() != expected_frame_len {
        return Err("OpenH264 output buffer length is invalid.".to_string());
    }
    let output_len = rect_byte_len(rect)?;
    let mut bgra = vec![0; output_len];
    for y in 0..rect_height {
        let source_row_start = y * frame.width * 4;
        let target_row_start = y * rect_width * 4;
        for x in 0..rect_width {
            let source = source_row_start + x * 4;
            let target = target_row_start + x * 4;
            bgra[target] = frame.rgba[source + 2];
            bgra[target + 1] = frame.rgba[source + 1];
            bgra[target + 2] = frame.rgba[source];
            bgra[target + 3] = u8::MAX;
        }
    }
    Ok(bgra)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    struct FakeDecoderFactory {
        create_count: Arc<AtomicUsize>,
        fail_decode: bool,
    }

    impl VncH264DecoderFactory for FakeDecoderFactory {
        fn create(&mut self) -> Result<Box<dyn VncH264Decoder>, String> {
            self.create_count.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(FakeDecoder {
                fail_decode: self.fail_decode,
            }))
        }
    }

    struct FakeDecoder {
        fail_decode: bool,
    }

    impl VncH264Decoder for FakeDecoder {
        fn decode(&mut self, _bitstream: &[u8]) -> Result<Option<DecodedH264Frame>, String> {
            if self.fail_decode {
                return Err("synthetic H.264 failure".to_string());
            }
            Ok(Some(DecodedH264Frame {
                rgba: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                width: 2,
                height: 2,
            }))
        }
    }

    fn rect(x: u16, y: u16) -> RfbRect {
        RfbRect {
            x,
            y,
            width: 1,
            height: 1,
        }
    }

    fn state(create_count: Arc<AtomicUsize>, fail_decode: bool) -> VncH264State {
        VncH264State::with_factory(Box::new(FakeDecoderFactory {
            create_count,
            fail_decode,
        }))
    }

    #[test]
    fn h264_contexts_are_keyed_by_rectangle_and_reused() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let mut state = state(create_count.clone(), false);

        state.decode_payload(rect(0, 0), 0, &[1]).unwrap();
        state.decode_payload(rect(0, 0), 0, &[2]).unwrap();
        state.decode_payload(rect(1, 0), 0, &[3]).unwrap();

        assert_eq!(create_count.load(Ordering::Relaxed), 2);
        assert_eq!(state.context_count(), 2);
    }

    #[test]
    fn h264_reset_flags_drop_the_requested_contexts_before_decode() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let mut state = state(create_count.clone(), false);
        state.decode_payload(rect(0, 0), 0, &[1]).unwrap();
        state.decode_payload(rect(1, 0), 0, &[2]).unwrap();

        state
            .decode_payload(rect(0, 0), OPEN_H264_RESET_CONTEXT, &[3])
            .unwrap();
        assert_eq!(create_count.load(Ordering::Relaxed), 3);
        assert_eq!(state.context_count(), 2);

        state
            .decode_payload(rect(0, 0), OPEN_H264_RESET_ALL_CONTEXTS, &[4])
            .unwrap();
        assert_eq!(create_count.load(Ordering::Relaxed), 4);
        assert_eq!(state.context_count(), 1);
    }

    #[test]
    fn h264_empty_reset_payload_does_not_create_a_context() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let mut state = state(create_count.clone(), false);
        state.decode_payload(rect(0, 0), 0, &[1]).unwrap();

        assert_eq!(
            state
                .decode_payload(rect(0, 0), OPEN_H264_RESET_CONTEXT, &[])
                .unwrap(),
            None
        );
        assert_eq!(create_count.load(Ordering::Relaxed), 1);
        assert_eq!(state.context_count(), 0);
    }

    #[test]
    fn h264_decode_failure_discards_the_poisoned_context() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let mut state = state(create_count.clone(), true);

        assert!(
            state
                .decode_payload(rect(0, 0), 0, &[1])
                .unwrap_err()
                .contains("synthetic")
        );
        assert_eq!(create_count.load(Ordering::Relaxed), 1);
        assert_eq!(state.context_count(), 0);
    }

    #[test]
    fn h264_context_count_is_bounded() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let mut state = state(create_count.clone(), false);
        for x in 0..MAX_OPEN_H264_CONTEXTS as u16 {
            state.decode_payload(rect(x, 0), 0, &[1]).unwrap();
        }

        assert!(
            state
                .decode_payload(rect(MAX_OPEN_H264_CONTEXTS as u16, 0), 0, &[1])
                .unwrap_err()
                .contains("context count")
        );
        assert_eq!(create_count.load(Ordering::Relaxed), MAX_OPEN_H264_CONTEXTS);
        assert_eq!(state.context_count(), MAX_OPEN_H264_CONTEXTS);
    }

    #[test]
    fn h264_wire_payload_length_is_bounded_before_allocation() {
        let create_count = Arc::new(AtomicUsize::new(0));
        let mut state = state(create_count, false);
        let mut payload = Vec::new();
        push_be_u32(&mut payload, (MAX_OPEN_H264_PAYLOAD_BYTES as u32) + 1);
        push_be_u32(&mut payload, 0);

        assert!(
            state
                .decode_rectangle(&mut std::io::Cursor::new(payload), rect(0, 0))
                .unwrap_err()
                .contains("payload exceeds")
        );
        assert_eq!(state.context_count(), 0);
    }

    #[test]
    fn h264_output_is_cropped_and_converted_to_bgra() {
        let bgra = h264_frame_to_bgra(
            RfbRect {
                x: 0,
                y: 0,
                width: 1,
                height: 2,
            },
            DecodedH264Frame {
                rgba: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                width: 2,
                height: 2,
            },
        )
        .unwrap();

        assert_eq!(bgra, vec![3, 2, 1, 0xff, 11, 10, 9, 0xff]);
    }
}
