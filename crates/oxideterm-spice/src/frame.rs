// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use oxide_spice_helper_protocol::{HelperEvent, HelperRect, HelperTopologyMonitor};
use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopHelperEvent, RemoteDesktopRect, RemoteDesktopSize,
};

pub(crate) enum SpiceFrameMapping {
    Frame(RemoteDesktopHelperEvent),
    Other(HelperEvent),
    Invalid,
}

#[derive(Default)]
pub(crate) struct SpiceFrameComposer {
    surfaces: HashMap<u32, SurfacePlacement>,
    canvas_size: Option<RemoteDesktopSize>,
    canvas: Vec<u8>,
    graphics_epoch: Option<u64>,
    base_emitted: bool,
}

#[derive(Clone, Copy)]
struct SurfacePlacement {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl SpiceFrameComposer {
    pub(crate) fn observe_topology(&mut self, event: &HelperEvent) {
        let HelperEvent::Topology {
            graphics_epoch,
            monitors,
            ..
        } = event
        else {
            return;
        };
        self.set_topology(*graphics_epoch, monitors);
    }

    pub(crate) fn map_event(&mut self, event: HelperEvent) -> SpiceFrameMapping {
        let HelperEvent::Frame {
            graphics_epoch,
            surface_id,
            surface_width,
            surface_height,
            rect,
            pixels,
            ..
        } = event
        else {
            return SpiceFrameMapping::Other(event);
        };
        let placement = self
            .surfaces
            .get(&surface_id)
            .copied()
            .unwrap_or(SurfacePlacement {
                x: 0,
                y: 0,
                width: surface_width,
                height: surface_height,
            });
        if placement.width != surface_width
            || placement.height != surface_height
            || !rect_fits_surface(rect, surface_width, surface_height)
            || rect_byte_len(rect) != Some(pixels.len())
        {
            return SpiceFrameMapping::Invalid;
        }
        let required_size = RemoteDesktopSize {
            width: placement.x.saturating_add(surface_width),
            height: placement.y.saturating_add(surface_height),
        };
        let current_size = self.canvas_size.unwrap_or(required_size);
        let canvas_size = RemoteDesktopSize {
            width: current_size.width.max(required_size.width),
            height: current_size.height.max(required_size.height),
        };
        if canvas_size.width > RemoteDesktopSize::MAX_DIMENSION
            || canvas_size.height > RemoteDesktopSize::MAX_DIMENSION
        {
            return SpiceFrameMapping::Invalid;
        }
        if (self.graphics_epoch != Some(graphics_epoch) || self.canvas_size != Some(canvas_size))
            && !self.reset_canvas(graphics_epoch, canvas_size)
        {
            return SpiceFrameMapping::Invalid;
        }
        let target = RemoteDesktopRect::new(
            placement.x.saturating_add(rect.x),
            placement.y.saturating_add(rect.y),
            rect.width,
            rect.height,
        );
        if !copy_rect_into_canvas(&mut self.canvas, canvas_size, target, &pixels) {
            return SpiceFrameMapping::Invalid;
        }
        if !self.base_emitted {
            self.base_emitted = true;
            return SpiceFrameMapping::Frame(RemoteDesktopHelperEvent::Frame {
                frame: RemoteDesktopFrame::new(
                    canvas_size,
                    RemoteDesktopFrameFormat::Rgba8,
                    self.canvas.clone(),
                )
                .with_graphics_epoch(graphics_epoch),
            });
        }
        SpiceFrameMapping::Frame(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                canvas_size,
                target,
                RemoteDesktopFrameFormat::Rgba8,
                pixels,
            )
            .with_graphics_epoch(graphics_epoch),
        })
    }

    fn set_topology(&mut self, graphics_epoch: u64, monitors: &[HelperTopologyMonitor]) {
        let surfaces = monitors
            .iter()
            .map(|monitor| {
                (
                    monitor.surface_id,
                    SurfacePlacement {
                        x: monitor.x,
                        y: monitor.y,
                        width: monitor.width,
                        height: monitor.height,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let canvas_size = surfaces.values().fold(
            RemoteDesktopSize {
                width: 0,
                height: 0,
            },
            |size, surface| RemoteDesktopSize {
                width: size.width.max(surface.x.saturating_add(surface.width)),
                height: size.height.max(surface.y.saturating_add(surface.height)),
            },
        );
        self.surfaces = surfaces;
        if canvas_size.width > 0
            && canvas_size.height > 0
            && canvas_size.width <= RemoteDesktopSize::MAX_DIMENSION
            && canvas_size.height <= RemoteDesktopSize::MAX_DIMENSION
        {
            let _ = self.reset_canvas(graphics_epoch, canvas_size);
        } else {
            self.canvas_size = None;
            self.canvas.clear();
            self.base_emitted = false;
        }
    }

    fn reset_canvas(&mut self, graphics_epoch: u64, size: RemoteDesktopSize) -> bool {
        let Some(byte_len) = RemoteDesktopFrame::expected_len(size) else {
            return false;
        };
        self.canvas_size = Some(size);
        self.canvas.clear();
        self.canvas.resize(byte_len, 0);
        self.graphics_epoch = Some(graphics_epoch);
        self.base_emitted = false;
        true
    }
}

fn rect_fits_surface(rect: HelperRect, width: u32, height: u32) -> bool {
    rect.width > 0
        && rect.height > 0
        && rect
            .x
            .checked_add(rect.width)
            .is_some_and(|right| right <= width)
        && rect
            .y
            .checked_add(rect.height)
            .is_some_and(|bottom| bottom <= height)
}

fn rect_byte_len(rect: HelperRect) -> Option<usize> {
    usize::try_from(rect.width)
        .ok()?
        .checked_mul(usize::try_from(rect.height).ok()?)?
        .checked_mul(RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel())
}

fn copy_rect_into_canvas(
    canvas: &mut [u8],
    canvas_size: RemoteDesktopSize,
    target: RemoteDesktopRect,
    pixels: &[u8],
) -> bool {
    if !target.fits_in(canvas_size) {
        return false;
    }
    let bytes_per_pixel = RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel();
    let Some(source_stride) = usize::try_from(target.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
    else {
        return false;
    };
    let Some(canvas_stride) = usize::try_from(canvas_size.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
    else {
        return false;
    };
    for row in 0..usize::try_from(target.height).unwrap_or(0) {
        let Some(source_start) = row.checked_mul(source_stride) else {
            return false;
        };
        let Some(target_row) = usize::try_from(target.y)
            .ok()
            .and_then(|y| y.checked_add(row))
        else {
            return false;
        };
        let Some(target_start) = target_row.checked_mul(canvas_stride).and_then(|offset| {
            usize::try_from(target.x)
                .ok()
                .and_then(|x| x.checked_mul(bytes_per_pixel))
                .and_then(|x| offset.checked_add(x))
        }) else {
            return false;
        };
        let Some(source_end) = source_start.checked_add(source_stride) else {
            return false;
        };
        let Some(target_end) = target_start.checked_add(source_stride) else {
            return false;
        };
        let (Some(source), Some(target)) = (
            pixels.get(source_start..source_end),
            canvas.get_mut(target_start..target_end),
        ) else {
            return false;
        };
        target.copy_from_slice(source);
    }
    true
}

#[cfg(test)]
mod tests {
    use oxide_spice_helper_protocol::HelperPixelFormat;

    use super::*;

    fn frame_event(full_refresh: bool, rect: HelperRect, pixels: Vec<u8>) -> HelperEvent {
        HelperEvent::Frame {
            connection_generation: 1,
            graphics_epoch: 7,
            display_channel_id: 0,
            surface_id: 0,
            surface_width: 2,
            surface_height: 1,
            rect,
            full_refresh,
            format: HelperPixelFormat::Rgba8,
            pixels,
        }
    }

    #[test]
    fn complete_refresh_becomes_a_shared_base_frame() {
        let mut composer = SpiceFrameComposer::default();
        let SpiceFrameMapping::Frame(event) = composer.map_event(frame_event(
            true,
            HelperRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            vec![0; 8],
        )) else {
            panic!("expected a mapped frame event");
        };

        let RemoteDesktopHelperEvent::Frame { frame } = event else {
            panic!("expected a base frame");
        };
        assert!(frame.is_complete());
        assert_eq!(frame.graphics_epoch, 7);
    }

    #[test]
    fn partial_refresh_after_base_preserves_its_dirty_rectangle() {
        let mut composer = SpiceFrameComposer::default();
        let _ = composer.map_event(frame_event(
            true,
            HelperRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            vec![0; 8],
        ));
        let SpiceFrameMapping::Frame(event) = composer.map_event(frame_event(
            false,
            HelperRect {
                x: 1,
                y: 0,
                width: 1,
                height: 1,
            },
            vec![0; 4],
        )) else {
            panic!("expected a mapped frame event");
        };

        let RemoteDesktopHelperEvent::FrameUpdate { update } = event else {
            panic!("expected a frame update");
        };
        assert!(update.is_complete());
        assert_eq!(update.rect, RemoteDesktopRect::new(1, 0, 1, 1));
    }
}
