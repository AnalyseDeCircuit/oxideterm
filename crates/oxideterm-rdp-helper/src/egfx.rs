// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};

use ironrdp::pdu::codecs::rfx::progressive::{
    ProgressiveBlock, ProgressiveContextPdu, decode_progressive_stream, encode_progressive_stream,
};
use ironrdp::pdu::geometry::{ExclusiveRectangle, Rectangle as _};
use ironrdp_egfx::{
    client::{BitmapUpdate, GraphicsPipelineClient, GraphicsPipelineHandler, Surface},
    decode::OpenH264Decoder,
    pdu::{
        CacheToSurfacePdu, CapabilitiesV8Flags, CapabilitiesV81Flags, CapabilitySet, Codec1Type,
        DeleteEncodingContextPdu, EvictCacheEntryPdu, GfxPdu, SolidFillPdu, SurfaceToCachePdu,
        SurfaceToSurfacePdu, WireToSurface2Pdu,
    },
};
use ironrdp_graphics::{
    clearcodec::ClearCodecDecoder,
    progressive::{ProgressiveDecodeError, ProgressiveDecoder},
};
use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopFrameUpdateBatch, RemoteDesktopHelperEvent, RemoteDesktopRect, RemoteDesktopSize,
};

use super::*;

const EGFX_BYTES_PER_PIXEL: usize = 4;
const EGFX_PROGRESSIVE_TILE_EDGE: u16 = 64;
const EGFX_MAX_SURFACE_DIMENSION: u16 = 16_384;
const EGFX_MAX_DESKTOP_BYTES: usize = 256 * 1024 * 1024;
const EGFX_MAX_SINGLE_SURFACE_BYTES: usize = 256 * 1024 * 1024;
const EGFX_MAX_TOTAL_SURFACE_BYTES: usize = 512 * 1024 * 1024;
const EGFX_MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
const OPENH264_LIBRARY_PATH_ENV: &str = "OXIDETERM_OPENH264_LIBRARY";

/// Gives the session loop a safe way to request the latest EGFX base frame.
#[derive(Clone)]
pub(super) struct EgfxSessionBridge {
    renderer: Option<Arc<Mutex<EgfxRenderer>>>,
}

impl EgfxSessionBridge {
    /// Creates a no-op bridge for sessions that intentionally omit the EGFX channel.
    pub(super) fn disabled() -> Self {
        Self { renderer: None }
    }

    pub(super) fn request_base_frame(&self) -> Result<bool, io::Error> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(false);
        };
        let mut renderer = renderer
            .lock()
            .map_err(|_| io::Error::other("EGFX renderer lock is poisoned"))?;
        if renderer.awaiting_reactivation {
            return Ok(true);
        }
        if !renderer.has_desktop() {
            return Ok(false);
        }
        renderer
            .publish_base_frame(false)
            .map_err(io::Error::other)?;
        Ok(true)
    }

    pub(super) fn begin_frame_transition(&self, graphics_epoch: u64) -> Result<(), io::Error> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(());
        };
        let mut renderer = renderer
            .lock()
            .map_err(|_| io::Error::other("EGFX renderer lock is poisoned"))?;
        renderer.graphics_epoch = graphics_epoch;
        renderer.awaiting_reactivation = true;
        renderer.pending_desktop_dirty.clear();
        renderer.needs_base_frame = true;
        Ok(())
    }

    pub(super) fn prepare_for_reactivation(&self, graphics_epoch: u64) -> Result<(), io::Error> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(());
        };
        let mut renderer = renderer
            .lock()
            .map_err(|_| io::Error::other("EGFX renderer lock is poisoned"))?;
        renderer.discard_graphics_state();
        renderer.graphics_epoch = graphics_epoch;
        renderer.awaiting_reactivation = false;
        Ok(())
    }
}

/// Builds a handler and its session-side bridge around one renderer state.
pub(super) fn new_egfx_channel(
    output_tx: ClientRdpOutputSender,
    graphics_epoch: u64,
) -> (GraphicsPipelineClient, EgfxSessionBridge) {
    let h264_decoder = std::env::var_os(OPENH264_LIBRARY_PATH_ENV).and_then(|path| {
        OpenH264Decoder::from_library_path(std::path::Path::new(&path))
            .map(|decoder| Box::new(decoder) as Box<dyn ironrdp_egfx::decode::H264Decoder>)
            .map_err(|error| {
                if remote_rdp_helper_graphics_diagnostics_enabled() {
                    eprintln!("[oxideterm:rdp-helper-capabilities] OpenH264 unavailable: {error}");
                }
            })
            .ok()
    });
    let h264_available = h264_decoder.is_some();
    let mut renderer = EgfxRenderer::new(output_tx.clone());
    renderer.graphics_epoch = graphics_epoch;
    let renderer = Arc::new(Mutex::new(renderer));
    let bridge = EgfxSessionBridge {
        renderer: Some(renderer.clone()),
    };
    let handler = OxideTermGraphicsPipelineHandler {
        renderer,
        output_tx,
        reported_lock_failure: false,
        h264_available,
    };

    (
        GraphicsPipelineClient::new(Box::new(handler), h264_decoder),
        bridge,
    )
}

struct OxideTermGraphicsPipelineHandler {
    renderer: Arc<Mutex<EgfxRenderer>>,
    output_tx: ClientRdpOutputSender,
    reported_lock_failure: bool,
    h264_available: bool,
}

impl OxideTermGraphicsPipelineHandler {
    fn update_renderer(&mut self, update: impl FnOnce(&mut EgfxRenderer) -> Result<(), String>) {
        let Ok(mut renderer) = self.renderer.lock() else {
            if !self.reported_lock_failure {
                self.reported_lock_failure = true;
                let _ = self
                    .output_tx
                    .send_control(ClientRdpOutput::ProtocolFailure(
                        "RDP Graphics Pipeline renderer state became unavailable.".to_string(),
                    ));
            }
            return;
        };
        if renderer.failed {
            return;
        }
        if let Err(message) = update(&mut renderer) {
            renderer.fail(message);
        }
    }
}

impl GraphicsPipelineHandler for OxideTermGraphicsPipelineHandler {
    fn capabilities(&self) -> Vec<CapabilitySet> {
        let mut capabilities = Vec::with_capacity(2);
        if self.h264_available {
            // V8.1 is the newest capability set whose complete AVC surface is
            // implemented by the pinned IronRDP client. V10.x would also
            // advertise AVC444, which is intentionally unsupported here.
            capabilities.push(CapabilitySet::V8_1 {
                flags: CapabilitiesV81Flags::AVC420_ENABLED | CapabilitiesV81Flags::SMALL_CACHE,
            });
        }
        capabilities.push(CapabilitySet::V8 {
            flags: CapabilitiesV8Flags::SMALL_CACHE,
        });
        capabilities
    }

    fn on_capabilities_confirmed(&mut self, capabilities: &CapabilitySet) {
        self.update_renderer(|renderer| renderer.confirm_capabilities(capabilities));
    }

    fn on_reset_graphics(&mut self, width: u32, height: u32) {
        self.update_renderer(|renderer| renderer.reset_graphics(width, height));
    }

    fn on_surface_created(&mut self, surface: &Surface) {
        self.update_renderer(|renderer| renderer.create_surface(surface));
    }

    fn on_surface_deleted(&mut self, surface_id: u16) {
        self.update_renderer(|renderer| renderer.delete_surface(surface_id));
    }

    fn on_surface_mapped(&mut self, surface_id: u16, origin_x: u32, origin_y: u32) {
        self.update_renderer(|renderer| renderer.map_surface(surface_id, origin_x, origin_y));
    }

    fn on_bitmap_updated(&mut self, update: &BitmapUpdate) {
        self.update_renderer(|renderer| renderer.apply_bitmap_update(update));
    }

    fn on_frame_complete(&mut self, _frame_id: u32) {
        self.update_renderer(EgfxRenderer::publish_completed_frame);
    }

    fn on_solid_fill(&mut self, pdu: &SolidFillPdu) {
        self.update_renderer(|renderer| renderer.solid_fill(pdu));
    }

    fn on_surface_to_surface(&mut self, pdu: &SurfaceToSurfacePdu) {
        self.update_renderer(|renderer| renderer.surface_to_surface(pdu));
    }

    fn on_surface_to_cache(&mut self, pdu: &SurfaceToCachePdu) {
        self.update_renderer(|renderer| renderer.surface_to_cache(pdu));
    }

    fn on_cache_to_surface(&mut self, pdu: &CacheToSurfacePdu) {
        self.update_renderer(|renderer| renderer.cache_to_surface(pdu));
    }

    fn on_evict_cache_entry(&mut self, pdu: &EvictCacheEntryPdu) {
        self.update_renderer(|renderer| {
            renderer.evict_cache_entry(pdu.cache_slot);
            Ok(())
        });
    }

    fn on_wire_to_surface2(&mut self, pdu: &WireToSurface2Pdu) {
        self.update_renderer(|renderer| renderer.apply_progressive_update(pdu));
    }

    fn on_delete_encoding_context(&mut self, pdu: &DeleteEncodingContextPdu) {
        self.update_renderer(|renderer| {
            renderer
                .progressive_decoder
                .delete_context(pdu.codec_context_id);
            Ok(())
        });
    }

    fn on_unhandled_pdu(&mut self, pdu: &GfxPdu) {
        if let GfxPdu::WireToSurface1(update) = pdu {
            self.update_renderer(|renderer| {
                if update.codec_id != Codec1Type::ClearCodec {
                    return Err(format!(
                        "RDP Graphics Pipeline sent unsupported codec {:?}.",
                        update.codec_id
                    ));
                }
                renderer.apply_clearcodec_update(
                    update.surface_id,
                    &update.destination_rectangle,
                    &update.bitmap_data,
                )
            });
        }
    }
}

struct EgfxRenderer {
    output_tx: ClientRdpOutputSender,
    desktop_size: Option<RemoteDesktopSize>,
    desktop_pixels: Vec<u8>,
    surfaces: BTreeMap<u16, EgfxSurface>,
    allocated_surface_bytes: usize,
    cache: BTreeMap<u16, EgfxCacheEntry>,
    allocated_cache_bytes: usize,
    progressive_decoder: ProgressiveDecoder,
    clearcodec_decoder: ClearCodecDecoder,
    pending_desktop_dirty: Vec<RemoteDesktopRect>,
    needs_base_frame: bool,
    graphics_epoch: u64,
    awaiting_reactivation: bool,
    published_first_frame: bool,
    next_trace_id: u64,
    failed: bool,
}

impl EgfxRenderer {
    fn new(output_tx: ClientRdpOutputSender) -> Self {
        Self {
            output_tx,
            desktop_size: None,
            desktop_pixels: Vec::new(),
            surfaces: BTreeMap::new(),
            allocated_surface_bytes: 0,
            cache: BTreeMap::new(),
            allocated_cache_bytes: 0,
            progressive_decoder: ProgressiveDecoder::new(),
            clearcodec_decoder: ClearCodecDecoder::new(),
            pending_desktop_dirty: Vec::new(),
            needs_base_frame: true,
            graphics_epoch: 0,
            awaiting_reactivation: false,
            published_first_frame: false,
            next_trace_id: 0,
            failed: false,
        }
    }

    fn has_desktop(&self) -> bool {
        self.desktop_size.is_some()
    }

    fn confirm_capabilities(&self, capabilities: &CapabilitySet) -> Result<(), String> {
        match capabilities {
            CapabilitySet::V8 { .. } => Ok(()),
            CapabilitySet::V8_1 { flags }
                if flags.contains(CapabilitiesV81Flags::AVC420_ENABLED) =>
            {
                Ok(())
            }
            _ => Err(
                "RDP server confirmed an EGFX capability OxideTerm did not advertise.".to_string(),
            ),
        }
    }

    fn reset_graphics(&mut self, width: u32, height: u32) -> Result<(), String> {
        let width = u16::try_from(width)
            .map_err(|_| "RDP Graphics Pipeline desktop width exceeds the protocol limit.")?;
        let height = u16::try_from(height)
            .map_err(|_| "RDP Graphics Pipeline desktop height exceeds the protocol limit.")?;
        let byte_len = checked_pixel_bytes(width, height, EGFX_MAX_DESKTOP_BYTES, "desktop")?;

        self.desktop_size = Some(RemoteDesktopSize {
            width: u32::from(width),
            height: u32::from(height),
        });
        self.desktop_pixels = opaque_black_pixels(byte_len);
        self.discard_surface_state();
        self.pending_desktop_dirty = vec![RemoteDesktopRect::new(
            0,
            0,
            u32::from(width),
            u32::from(height),
        )];
        self.needs_base_frame = true;
        Ok(())
    }

    fn discard_graphics_state(&mut self) {
        self.desktop_size = None;
        self.desktop_pixels.clear();
        self.discard_surface_state();
        self.pending_desktop_dirty.clear();
        self.needs_base_frame = true;
    }

    fn discard_surface_state(&mut self) {
        self.surfaces.clear();
        self.allocated_surface_bytes = 0;
        self.cache.clear();
        self.allocated_cache_bytes = 0;
        // RESET_GRAPHICS discards surfaces, but progressive codec contexts
        // remain valid until DELETE_ENCODING_CONTEXT explicitly removes them.
        self.clearcodec_decoder = ClearCodecDecoder::new();
    }

    fn create_surface(&mut self, surface: &Surface) -> Result<(), String> {
        self.create_surface_with_dimensions(surface.id, surface.width, surface.height)
    }

    fn create_surface_with_dimensions(
        &mut self,
        surface_id: u16,
        width: u16,
        height: u16,
    ) -> Result<(), String> {
        let byte_len =
            checked_pixel_bytes(width, height, EGFX_MAX_SINGLE_SURFACE_BYTES, "surface")?;
        let replaced_len = self
            .surfaces
            .get(&surface_id)
            .map_or(0, |existing| existing.pixels.len());
        let next_total = self
            .allocated_surface_bytes
            .saturating_sub(replaced_len)
            .checked_add(byte_len)
            .ok_or_else(|| {
                "RDP Graphics Pipeline surface memory accounting overflowed.".to_string()
            })?;
        if next_total > EGFX_MAX_TOTAL_SURFACE_BYTES {
            return Err("RDP Graphics Pipeline surfaces exceed the memory limit.".to_string());
        }

        self.allocated_surface_bytes = next_total;
        self.surfaces.insert(
            surface_id,
            EgfxSurface {
                width,
                height,
                pixels: opaque_black_pixels(byte_len),
                output_origin: None,
            },
        );
        Ok(())
    }

    fn delete_surface(&mut self, surface_id: u16) -> Result<(), String> {
        let surface = self.surfaces.remove(&surface_id).ok_or_else(|| {
            format!("RDP Graphics Pipeline deleted unknown surface {surface_id}.")
        })?;
        self.allocated_surface_bytes = self
            .allocated_surface_bytes
            .saturating_sub(surface.pixels.len());
        Ok(())
    }

    fn map_surface(&mut self, surface_id: u16, origin_x: u32, origin_y: u32) -> Result<(), String> {
        let (width, height) = {
            let surface = self.surfaces.get_mut(&surface_id).ok_or_else(|| {
                format!("RDP Graphics Pipeline mapped unknown surface {surface_id}.")
            })?;
            surface.output_origin = Some((origin_x, origin_y));
            (surface.width, surface.height)
        };
        self.composite_surface_region(surface_id, SurfaceRegion::new(0, 0, width, height))
    }

    fn apply_bitmap_update(&mut self, update: &BitmapUpdate) -> Result<(), String> {
        let region = SurfaceRegion::from_exclusive(&update.destination_rectangle)?;
        if region.width != update.width || region.height != update.height {
            return Err(
                "RDP Graphics Pipeline bitmap dimensions do not match its destination.".to_string(),
            );
        }
        let expected_len = region.byte_len()?;
        if update.data.len() != expected_len {
            return Err("RDP Graphics Pipeline bitmap payload length is invalid.".to_string());
        }
        self.write_surface_region(update.surface_id, region, &update.data)
    }

    fn apply_clearcodec_update(
        &mut self,
        surface_id: u16,
        destination: &ExclusiveRectangle,
        bitmap_data: &[u8],
    ) -> Result<(), String> {
        let region = SurfaceRegion::from_exclusive(destination)?;
        let decoded = self
            .clearcodec_decoder
            .decode(bitmap_data, region.width, region.height)
            .map_err(|error| format!("RDP ClearCodec decode failed: {error}"))?;
        let rgba = bgra_to_rgba(decoded);
        self.write_surface_region(surface_id, region, &rgba)
    }

    fn apply_progressive_update(&mut self, pdu: &WireToSurface2Pdu) -> Result<(), String> {
        let (surface_width, surface_height) = self.surface_dimensions(pdu.surface_id)?;
        let decode = self.progressive_decoder.decode_bitmap(
            pdu.codec_context_id,
            surface_width,
            surface_height,
            &pdu.bitmap_data,
        );
        let tiles = match decode {
            Ok(tiles) => tiles,
            Err(ProgressiveDecodeError::MissingBlock("CONTEXT")) => {
                // MS-RDPEGFX makes RFX_PROGRESSIVE_CONTEXT optional. Retry
                // first-use streams with the protocol default context instead
                // of terminating the remote desktop session.
                let bitmap_data = progressive_bitmap_with_default_context(&pdu.bitmap_data)?;
                self.progressive_decoder
                    .decode_bitmap(
                        pdu.codec_context_id,
                        surface_width,
                        surface_height,
                        &bitmap_data,
                    )
                    .map_err(|error| format!("RDP Progressive decode failed: {error}"))?
            }
            Err(error) => return Err(format!("RDP Progressive decode failed: {error}")),
        };

        for tile in tiles {
            let x = tile.x_idx.saturating_mul(EGFX_PROGRESSIVE_TILE_EDGE);
            let y = tile.y_idx.saturating_mul(EGFX_PROGRESSIVE_TILE_EDGE);
            if x >= surface_width || y >= surface_height {
                return Err("RDP Progressive tile lies outside its surface.".to_string());
            }
            let width = EGFX_PROGRESSIVE_TILE_EDGE.min(surface_width - x);
            let height = EGFX_PROGRESSIVE_TILE_EDGE.min(surface_height - y);
            let region = SurfaceRegion::new(x, y, width, height);
            if width == EGFX_PROGRESSIVE_TILE_EDGE && height == EGFX_PROGRESSIVE_TILE_EDGE {
                self.write_surface_region(pdu.surface_id, region, &tile.pixels)?;
            } else {
                let cropped = crop_progressive_tile(&tile.pixels, width, height)?;
                self.write_surface_region(pdu.surface_id, region, &cropped)?;
            }
        }
        Ok(())
    }

    fn solid_fill(&mut self, pdu: &SolidFillPdu) -> Result<(), String> {
        let pixel = [pdu.fill_pixel.r, pdu.fill_pixel.g, pdu.fill_pixel.b, 0xff];
        for rectangle in &pdu.rectangles {
            let region = SurfaceRegion::from_exclusive(rectangle)?;
            let mut pixels = Vec::with_capacity(region.byte_len()?);
            for _ in 0..region.pixel_count() {
                pixels.extend_from_slice(&pixel);
            }
            self.write_surface_region(pdu.surface_id, region, &pixels)?;
        }
        Ok(())
    }

    fn surface_to_surface(&mut self, pdu: &SurfaceToSurfacePdu) -> Result<(), String> {
        let source_region = SurfaceRegion::from_exclusive(&pdu.source_rectangle)?;
        let source_pixels = self.copy_surface_region(pdu.source_surface_id, source_region)?;
        for destination in &pdu.destination_points {
            let destination_region = SurfaceRegion::new(
                destination.x,
                destination.y,
                source_region.width,
                source_region.height,
            );
            self.write_surface_region(
                pdu.destination_surface_id,
                destination_region,
                &source_pixels,
            )?;
        }
        Ok(())
    }

    fn surface_to_cache(&mut self, pdu: &SurfaceToCachePdu) -> Result<(), String> {
        let source_region = SurfaceRegion::from_exclusive(&pdu.source_rectangle)?;
        let pixels = self.copy_surface_region(pdu.surface_id, source_region)?;
        let replaced_len = self
            .cache
            .get(&pdu.cache_slot)
            .map_or(0, |entry| entry.pixels.len());
        let next_total = self
            .allocated_cache_bytes
            .saturating_sub(replaced_len)
            .checked_add(pixels.len())
            .ok_or_else(|| {
                "RDP Graphics Pipeline cache memory accounting overflowed.".to_string()
            })?;
        if next_total > EGFX_MAX_CACHE_BYTES {
            return Err("RDP Graphics Pipeline cache exceeds the memory limit.".to_string());
        }
        self.allocated_cache_bytes = next_total;
        self.cache.insert(
            pdu.cache_slot,
            EgfxCacheEntry {
                width: source_region.width,
                height: source_region.height,
                pixels,
            },
        );
        Ok(())
    }

    fn cache_to_surface(&mut self, pdu: &CacheToSurfacePdu) -> Result<(), String> {
        let entry = self
            .cache
            .get(&pdu.cache_slot)
            .ok_or_else(|| {
                format!(
                    "RDP Graphics Pipeline referenced missing cache slot {}.",
                    pdu.cache_slot
                )
            })?
            .clone();
        for destination in &pdu.destination_points {
            self.write_surface_region(
                pdu.surface_id,
                SurfaceRegion::new(destination.x, destination.y, entry.width, entry.height),
                &entry.pixels,
            )?;
        }
        Ok(())
    }

    fn evict_cache_entry(&mut self, cache_slot: u16) {
        if let Some(entry) = self.cache.remove(&cache_slot) {
            self.allocated_cache_bytes = self
                .allocated_cache_bytes
                .saturating_sub(entry.pixels.len());
        }
    }

    fn write_surface_region(
        &mut self,
        surface_id: u16,
        region: SurfaceRegion,
        source_pixels: &[u8],
    ) -> Result<(), String> {
        {
            let surface = self.surfaces.get_mut(&surface_id).ok_or_else(|| {
                format!("RDP Graphics Pipeline updated unknown surface {surface_id}.")
            })?;
            validate_surface_region(surface, region)?;
            if source_pixels.len() != region.byte_len()? {
                return Err("RDP Graphics Pipeline surface update length is invalid.".to_string());
            }
            blit_rgba_region(
                source_pixels,
                region.width,
                &mut surface.pixels,
                surface.width,
                region.x,
                region.y,
                region.width,
                region.height,
            );
        }
        self.composite_surface_region(surface_id, region)
    }

    fn copy_surface_region(
        &self,
        surface_id: u16,
        region: SurfaceRegion,
    ) -> Result<Vec<u8>, String> {
        let surface = self
            .surfaces
            .get(&surface_id)
            .ok_or_else(|| format!("RDP Graphics Pipeline read unknown surface {surface_id}."))?;
        validate_surface_region(surface, region)?;
        Ok(extract_rgba_region(
            &surface.pixels,
            surface.width,
            region.x,
            region.y,
            region.width,
            region.height,
        ))
    }

    fn surface_dimensions(&self, surface_id: u16) -> Result<(u16, u16), String> {
        self.surfaces
            .get(&surface_id)
            .map(|surface| (surface.width, surface.height))
            .ok_or_else(|| {
                format!("RDP Graphics Pipeline referenced unknown surface {surface_id}.")
            })
    }

    fn composite_surface_region(
        &mut self,
        surface_id: u16,
        region: SurfaceRegion,
    ) -> Result<(), String> {
        let Some(desktop_size) = self.desktop_size else {
            return Ok(());
        };
        let surface = self.surfaces.get(&surface_id).ok_or_else(|| {
            format!("RDP Graphics Pipeline composited unknown surface {surface_id}.")
        })?;
        let Some((origin_x, origin_y)) = surface.output_origin else {
            return Ok(());
        };
        validate_surface_region(surface, region)?;

        let destination_x = origin_x.saturating_add(u32::from(region.x));
        let destination_y = origin_y.saturating_add(u32::from(region.y));
        if destination_x >= desktop_size.width || destination_y >= desktop_size.height {
            return Ok(());
        }
        let copy_width = u32::from(region.width).min(desktop_size.width - destination_x);
        let copy_height = u32::from(region.height).min(desktop_size.height - destination_y);
        let copy_width_u16 = u16::try_from(copy_width)
            .map_err(|_| "RDP Graphics Pipeline composite width overflowed.")?;
        let copy_height_u16 = u16::try_from(copy_height)
            .map_err(|_| "RDP Graphics Pipeline composite height overflowed.")?;
        let desktop_width = u16::try_from(desktop_size.width)
            .map_err(|_| "RDP Graphics Pipeline desktop width overflowed.")?;
        let destination_x_u16 = u16::try_from(destination_x)
            .map_err(|_| "RDP Graphics Pipeline destination X overflowed.")?;
        let destination_y_u16 = u16::try_from(destination_y)
            .map_err(|_| "RDP Graphics Pipeline destination Y overflowed.")?;
        let source_stride = usize::from(surface.width) * EGFX_BYTES_PER_PIXEL;
        let destination_stride = usize::from(desktop_width) * EGFX_BYTES_PER_PIXEL;
        let row_bytes = usize::from(copy_width_u16) * EGFX_BYTES_PER_PIXEL;
        for row in 0..usize::from(copy_height_u16) {
            let source_start = (usize::from(region.y) + row) * source_stride
                + usize::from(region.x) * EGFX_BYTES_PER_PIXEL;
            let destination_start = (usize::from(destination_y_u16) + row) * destination_stride
                + usize::from(destination_x_u16) * EGFX_BYTES_PER_PIXEL;
            self.desktop_pixels[destination_start..destination_start + row_bytes]
                .copy_from_slice(&surface.pixels[source_start..source_start + row_bytes]);
        }
        self.queue_desktop_dirty(RemoteDesktopRect::new(
            destination_x,
            destination_y,
            copy_width,
            copy_height,
        ));
        Ok(())
    }

    fn queue_desktop_dirty(&mut self, rect: RemoteDesktopRect) {
        crate::frame::queue_bounded_dirty_rect(
            &mut self.pending_desktop_dirty,
            rect,
            crate::frame::RDP_GRAPHICS_MAX_DIRTY_RECTS,
        );
    }

    fn publish_completed_frame(&mut self) -> Result<(), String> {
        if self.awaiting_reactivation {
            self.pending_desktop_dirty.clear();
            return Ok(());
        }
        if self.pending_desktop_dirty.is_empty() {
            return Ok(());
        }
        let dirty_rects = std::mem::take(&mut self.pending_desktop_dirty);
        let Some(size) = self.desktop_size else {
            return Ok(());
        };
        let dirty_pixels = dirty_rects
            .iter()
            .map(|dirty| u64::from(dirty.width).saturating_mul(u64::from(dirty.height)))
            .fold(0_u64, u64::saturating_add);
        let frame_pixels = u64::from(size.width).saturating_mul(u64::from(size.height));
        if self.needs_base_frame
            || dirty_pixels >= frame_pixels
            || dirty_rects.iter().any(|dirty| {
                dirty.x == 0
                    && dirty.y == 0
                    && dirty.width == size.width
                    && dirty.height == size.height
            })
        {
            return self.publish_base_frame(true);
        }

        self.next_trace_id = self.next_trace_id.saturating_add(1).max(1);
        let desktop_width = u16::try_from(size.width)
            .map_err(|_| "RDP Graphics Pipeline desktop width overflowed.")?;
        let updates = dirty_rects
            .into_iter()
            .map(|dirty| {
                Ok(RemoteDesktopFrameUpdate::new(
                    size,
                    dirty,
                    RemoteDesktopFrameFormat::Rgba8,
                    copy_rgba_region_u32(&self.desktop_pixels, desktop_width, dirty)?,
                )
                .with_graphics_epoch(self.graphics_epoch)
                .with_trace_id(self.next_trace_id))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let event = if updates.len() == 1 {
            RemoteDesktopHelperEvent::FrameUpdate {
                update: updates.into_iter().next().expect("one update was checked"),
            }
        } else {
            RemoteDesktopHelperEvent::FrameUpdateBatch {
                batch: RemoteDesktopFrameUpdateBatch::new(updates),
            }
        };
        match self
            .output_tx
            .try_send_graphics(ClientRdpOutput::Event(event))
        {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => {
                // A dropped delta invalidates the UI backing frame; recover on the next EndFrame.
                self.needs_base_frame = true;
                Ok(())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("RDP Graphics Pipeline output channel closed.".to_string())
            }
        }
    }

    fn publish_base_frame(&mut self, publish_ready: bool) -> Result<(), String> {
        if self.awaiting_reactivation {
            self.needs_base_frame = true;
            return Ok(());
        }
        let size = self
            .desktop_size
            .ok_or_else(|| "RDP Graphics Pipeline has no desktop surface.".to_string())?;
        self.next_trace_id = self.next_trace_id.saturating_add(1).max(1);
        let event = RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                self.desktop_pixels.clone(),
            )
            .with_graphics_epoch(self.graphics_epoch)
            .with_trace_id(self.next_trace_id),
        };
        match self
            .output_tx
            .try_send_graphics(ClientRdpOutput::Event(event))
        {
            Ok(()) => {
                self.needs_base_frame = false;
                self.pending_desktop_dirty.clear();
                if publish_ready && !self.published_first_frame {
                    for event in native_rdp_desktop_ready_events(size) {
                        self.output_tx
                            .send_control(ClientRdpOutput::Event(event))
                            .map_err(|_| {
                                "RDP Graphics Pipeline control channel closed.".to_string()
                            })?;
                    }
                    self.published_first_frame = true;
                }
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.needs_base_frame = true;
                Ok(())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("RDP Graphics Pipeline output channel closed.".to_string())
            }
        }
    }

    fn fail(&mut self, message: String) {
        self.failed = true;
        let _ = self
            .output_tx
            .send_control(ClientRdpOutput::ProtocolFailure(message));
    }
}

fn progressive_bitmap_with_default_context(bitmap_data: &[u8]) -> Result<Vec<u8>, String> {
    let mut blocks = decode_progressive_stream(bitmap_data)
        .map_err(|error| format!("RDP Progressive stream decode failed: {error}"))?;
    let insert_index = blocks
        .iter()
        .position(|block| matches!(block, ProgressiveBlock::FrameBegin(_)))
        .unwrap_or(blocks.len());
    blocks.insert(
        insert_index,
        ProgressiveBlock::Context(ProgressiveContextPdu {
            context_id: 0,
            tile_size: EGFX_PROGRESSIVE_TILE_EDGE,
            flags: 0,
        }),
    );
    encode_progressive_stream(&blocks)
        .map_err(|error| format!("RDP Progressive stream encode failed: {error}"))
}

struct EgfxSurface {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
    output_origin: Option<(u32, u32)>,
}

#[derive(Clone)]
struct EgfxCacheEntry {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
}

#[derive(Clone, Copy)]
struct SurfaceRegion {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl SurfaceRegion {
    fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn from_exclusive(rectangle: &ExclusiveRectangle) -> Result<Self, String> {
        if rectangle.left >= rectangle.right || rectangle.top >= rectangle.bottom {
            return Err("RDP Graphics Pipeline rectangle is empty or inverted.".to_string());
        }
        Ok(Self::new(
            rectangle.left,
            rectangle.top,
            rectangle.width(),
            rectangle.height(),
        ))
    }

    fn pixel_count(self) -> usize {
        usize::from(self.width).saturating_mul(usize::from(self.height))
    }

    fn byte_len(self) -> Result<usize, String> {
        self.pixel_count()
            .checked_mul(EGFX_BYTES_PER_PIXEL)
            .ok_or_else(|| "RDP Graphics Pipeline rectangle size overflowed.".to_string())
    }
}

fn checked_pixel_bytes(
    width: u16,
    height: u16,
    max_bytes: usize,
    resource_name: &str,
) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err(format!(
            "RDP Graphics Pipeline {resource_name} dimensions must be non-zero."
        ));
    }
    if width > EGFX_MAX_SURFACE_DIMENSION || height > EGFX_MAX_SURFACE_DIMENSION {
        return Err(format!(
            "RDP Graphics Pipeline {resource_name} exceeds the dimension limit."
        ));
    }
    let byte_len = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|pixels| pixels.checked_mul(EGFX_BYTES_PER_PIXEL))
        .ok_or_else(|| format!("RDP Graphics Pipeline {resource_name} size overflowed."))?;
    if byte_len > max_bytes {
        return Err(format!(
            "RDP Graphics Pipeline {resource_name} exceeds the memory limit."
        ));
    }
    Ok(byte_len)
}

fn opaque_black_pixels(byte_len: usize) -> Vec<u8> {
    let mut pixels = vec![0; byte_len];
    for pixel in pixels.chunks_exact_mut(EGFX_BYTES_PER_PIXEL) {
        pixel[3] = 0xff;
    }
    pixels
}

fn validate_surface_region(surface: &EgfxSurface, region: SurfaceRegion) -> Result<(), String> {
    let right = u32::from(region.x).saturating_add(u32::from(region.width));
    let bottom = u32::from(region.y).saturating_add(u32::from(region.height));
    if region.width == 0
        || region.height == 0
        || right > u32::from(surface.width)
        || bottom > u32::from(surface.height)
    {
        return Err("RDP Graphics Pipeline region exceeds its surface.".to_string());
    }
    Ok(())
}

fn extract_rgba_region(
    pixels: &[u8],
    source_width: u16,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
) -> Vec<u8> {
    let row_bytes = usize::from(width) * EGFX_BYTES_PER_PIXEL;
    let source_stride = usize::from(source_width) * EGFX_BYTES_PER_PIXEL;
    let mut output = Vec::with_capacity(row_bytes * usize::from(height));
    for row in 0..usize::from(height) {
        let start = (usize::from(y) + row) * source_stride + usize::from(x) * EGFX_BYTES_PER_PIXEL;
        output.extend_from_slice(&pixels[start..start + row_bytes]);
    }
    output
}

#[expect(
    clippy::too_many_arguments,
    reason = "the source and destination rectangles are clearer as explicit coordinates"
)]
fn blit_rgba_region(
    source: &[u8],
    source_width: u16,
    destination: &mut [u8],
    destination_width: u16,
    destination_x: u16,
    destination_y: u16,
    width: u16,
    height: u16,
) {
    let row_bytes = usize::from(width) * EGFX_BYTES_PER_PIXEL;
    let source_stride = usize::from(source_width) * EGFX_BYTES_PER_PIXEL;
    let destination_stride = usize::from(destination_width) * EGFX_BYTES_PER_PIXEL;
    for row in 0..usize::from(height) {
        let source_start = row * source_stride;
        let destination_start = (usize::from(destination_y) + row) * destination_stride
            + usize::from(destination_x) * EGFX_BYTES_PER_PIXEL;
        destination[destination_start..destination_start + row_bytes]
            .copy_from_slice(&source[source_start..source_start + row_bytes]);
    }
}

fn copy_rgba_region_u32(
    pixels: &[u8],
    source_width: u16,
    rect: RemoteDesktopRect,
) -> Result<Vec<u8>, String> {
    let x =
        u16::try_from(rect.x).map_err(|_| "RDP Graphics Pipeline dirty rectangle X overflowed.")?;
    let y =
        u16::try_from(rect.y).map_err(|_| "RDP Graphics Pipeline dirty rectangle Y overflowed.")?;
    let width = u16::try_from(rect.width)
        .map_err(|_| "RDP Graphics Pipeline dirty rectangle width overflowed.")?;
    let height = u16::try_from(rect.height)
        .map_err(|_| "RDP Graphics Pipeline dirty rectangle height overflowed.")?;
    Ok(extract_rgba_region(
        pixels,
        source_width,
        x,
        y,
        width,
        height,
    ))
}

fn crop_progressive_tile(pixels: &[u8], width: u16, height: u16) -> Result<Vec<u8>, String> {
    let full_tile_bytes = usize::from(EGFX_PROGRESSIVE_TILE_EDGE)
        * usize::from(EGFX_PROGRESSIVE_TILE_EDGE)
        * EGFX_BYTES_PER_PIXEL;
    if pixels.len() != full_tile_bytes {
        return Err("RDP Progressive decoder returned an invalid tile length.".to_string());
    }
    Ok(extract_rgba_region(
        pixels,
        EGFX_PROGRESSIVE_TILE_EDGE,
        0,
        0,
        width,
        height,
    ))
}

fn bgra_to_rgba(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.chunks_exact_mut(EGFX_BYTES_PER_PIXEL) {
        pixel.swap(0, 2);
        pixel[3] = 0xff;
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_bridge_defers_frames_to_the_bitmap_path() {
        let bridge = EgfxSessionBridge::disabled();

        assert!(!bridge.request_base_frame().unwrap());
        bridge.begin_frame_transition(2).unwrap();
        bridge.prepare_for_reactivation(3).unwrap();
    }

    #[test]
    fn handler_advertises_only_non_avc_v8() {
        let (output_tx, _output_rx) = client_rdp_output_channel(4);
        let renderer = Arc::new(Mutex::new(EgfxRenderer::new(output_tx.clone())));
        let handler = OxideTermGraphicsPipelineHandler {
            renderer,
            output_tx,
            reported_lock_failure: false,
            h264_available: false,
        };

        assert_eq!(
            handler.capabilities(),
            vec![CapabilitySet::V8 {
                flags: CapabilitiesV8Flags::SMALL_CACHE,
            }]
        );
    }

    #[test]
    fn handler_advertises_avc420_only_when_decoder_is_available() {
        let (output_tx, _output_rx) = client_rdp_output_channel(4);
        let renderer = Arc::new(Mutex::new(EgfxRenderer::new(output_tx.clone())));
        let handler = OxideTermGraphicsPipelineHandler {
            renderer,
            output_tx,
            reported_lock_failure: false,
            h264_available: true,
        };

        assert_eq!(
            handler.capabilities(),
            vec![
                CapabilitySet::V8_1 {
                    flags: CapabilitiesV81Flags::AVC420_ENABLED | CapabilitiesV81Flags::SMALL_CACHE,
                },
                CapabilitySet::V8 {
                    flags: CapabilitiesV8Flags::SMALL_CACHE,
                },
            ]
        );
    }

    #[test]
    fn mapped_surface_publishes_atomic_rgba_base_frame() {
        let (output_tx, output_rx) = client_rdp_output_channel(4);
        let mut renderer = EgfxRenderer::new(output_tx);
        renderer.reset_graphics(2, 2).unwrap();
        renderer.create_surface_with_dimensions(7, 2, 2).unwrap();
        renderer.map_surface(7, 0, 0).unwrap();
        let red = [255, 0, 0, 255].repeat(4);
        renderer
            .write_surface_region(7, SurfaceRegion::new(0, 0, 2, 2), &red)
            .unwrap();

        renderer.publish_completed_frame().unwrap();

        let ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame { frame }) =
            output_rx.graphics_rx.try_recv().unwrap()
        else {
            panic!("expected an EGFX base frame");
        };
        assert_eq!(frame.format, RemoteDesktopFrameFormat::Rgba8);
        assert_eq!(frame.bytes, red);
    }

    #[test]
    fn completed_egfx_frame_preserves_separated_dirty_regions() {
        let (output_tx, output_rx) = client_rdp_output_channel(4);
        let mut renderer = EgfxRenderer::new(output_tx);
        renderer.reset_graphics(8, 1).unwrap();
        renderer.publish_completed_frame().unwrap();
        let _base = output_rx.graphics_rx.try_recv().unwrap();
        renderer.queue_desktop_dirty(RemoteDesktopRect::new(0, 0, 1, 1));
        renderer.queue_desktop_dirty(RemoteDesktopRect::new(7, 0, 1, 1));

        renderer.publish_completed_frame().unwrap();

        let ClientRdpOutput::Event(RemoteDesktopHelperEvent::FrameUpdateBatch { batch }) =
            output_rx.graphics_rx.try_recv().unwrap()
        else {
            panic!("expected an EGFX sparse frame batch");
        };
        assert_eq!(batch.updates.len(), 2);
        assert_eq!(batch.byte_len(), 8);
        assert_eq!(batch.updates[0].rect, RemoteDesktopRect::new(0, 0, 1, 1));
        assert_eq!(batch.updates[1].rect, RemoteDesktopRect::new(7, 0, 1, 1));
    }

    #[test]
    fn surface_cache_round_trip_preserves_pixels() {
        let (output_tx, _output_rx) = client_rdp_output_channel(4);
        let mut renderer = EgfxRenderer::new(output_tx);
        renderer.reset_graphics(4, 2).unwrap();
        renderer.create_surface_with_dimensions(1, 4, 2).unwrap();
        let source = [1, 2, 3, 255, 4, 5, 6, 255];
        renderer
            .write_surface_region(1, SurfaceRegion::new(0, 0, 2, 1), &source)
            .unwrap();
        renderer
            .surface_to_cache(&SurfaceToCachePdu {
                surface_id: 1,
                cache_key: 11,
                cache_slot: 3,
                source_rectangle: ExclusiveRectangle {
                    left: 0,
                    top: 0,
                    right: 2,
                    bottom: 1,
                },
            })
            .unwrap();
        renderer
            .cache_to_surface(&CacheToSurfacePdu {
                cache_slot: 3,
                surface_id: 1,
                destination_points: vec![ironrdp_egfx::pdu::Point { x: 2, y: 1 }],
            })
            .unwrap();

        assert_eq!(
            renderer
                .copy_surface_region(1, SurfaceRegion::new(2, 1, 2, 1))
                .unwrap(),
            source
        );
    }

    #[test]
    fn edge_progressive_tile_is_cropped_to_surface() {
        let mut tile = vec![0; 64 * 64 * 4];
        tile[..4].copy_from_slice(&[9, 8, 7, 255]);

        let cropped = crop_progressive_tile(&tile, 1, 1).unwrap();

        assert_eq!(cropped, vec![9, 8, 7, 255]);
    }

    #[test]
    fn dropped_dirty_update_recovers_with_latest_base_frame() {
        let (output_tx, output_rx) = client_rdp_output_channel(1);
        let mut renderer = EgfxRenderer::new(output_tx);
        renderer.reset_graphics(2, 1).unwrap();
        renderer.create_surface_with_dimensions(1, 2, 1).unwrap();
        renderer.map_surface(1, 0, 0).unwrap();
        renderer
            .write_surface_region(
                1,
                SurfaceRegion::new(0, 0, 2, 1),
                &[1, 2, 3, 255, 10, 11, 12, 255],
            )
            .unwrap();
        renderer.publish_completed_frame().unwrap();

        renderer
            .write_surface_region(1, SurfaceRegion::new(0, 0, 1, 1), &[4, 5, 6, 255])
            .unwrap();
        renderer.publish_completed_frame().unwrap();
        assert!(renderer.needs_base_frame);

        renderer
            .write_surface_region(1, SurfaceRegion::new(0, 0, 1, 1), &[7, 8, 9, 255])
            .unwrap();
        renderer.publish_completed_frame().unwrap();

        let ClientRdpOutput::Event(RemoteDesktopHelperEvent::Frame { frame }) =
            output_rx.graphics_rx.try_recv().unwrap()
        else {
            panic!("expected the recovery base frame");
        };
        assert_eq!(frame.bytes, vec![7, 8, 9, 255, 10, 11, 12, 255]);
    }
}
