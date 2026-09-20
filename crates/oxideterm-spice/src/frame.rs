// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxide_spice_helper_protocol::{HelperEvent, HelperTopologyMonitor};
use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopHelperEvent, RemoteDesktopRect,
    RemoteDesktopSize,
};
use std::collections::BTreeMap;

const MAX_SURFACE_BYTES: usize = 256 * 1024 * 1024;

pub(crate) enum SpiceFrameMapping {
    Frame(RemoteDesktopHelperEvent),
    Other(HelperEvent),
    Ignored,
    Invalid,
}

/// Painting and input share the same mapping from independent SPICE surfaces to the desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpiceDisplayLayout {
    regions: Vec<MonitorRegion>,
    size: RemoteDesktopSize,
}

impl Default for SpiceDisplayLayout {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
            size: RemoteDesktopSize {
                width: 0,
                height: 0,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MonitorRegion {
    channel: u8,
    surface: u32,
    display: u8,
    source: RemoteDesktopRect,
    target: RemoteDesktopRect,
}

impl SpiceDisplayLayout {
    pub fn pointer_target(&self, x: u32, y: u32) -> Option<(u32, u32, u8)> {
        self.regions.iter().find_map(|region| {
            contains(region.target, x, y)
                .then(|| (x - region.target.x, y - region.target.y, region.display))
        })
    }

    pub(crate) fn cursor_position(&self, channel: u8, x: u32, y: u32) -> Option<(u32, u32)> {
        self.regions.iter().find_map(|region| {
            (region.channel == channel && contains(region.source, x, y)).then(|| {
                (
                    region.target.x + x - region.source.x,
                    region.target.y + y - region.source.y,
                )
            })
        })
    }
}

fn contains(rect: RemoteDesktopRect, x: u32, y: u32) -> bool {
    x >= rect.x && y >= rect.y && x - rect.x < rect.width && y - rect.y < rect.height
}

#[derive(Default)]
pub(crate) struct SpiceFrameComposer {
    channels: BTreeMap<u8, DisplayChannel>,
    generation: u64,
    epoch: u64,
    layout: SpiceDisplayLayout,
}

#[derive(Default)]
struct DisplayChannel {
    epoch: u64,
    monitors: Option<Vec<HelperTopologyMonitor>>,
    surfaces: BTreeMap<u32, Surface>,
}

struct Surface {
    size: RemoteDesktopSize,
    pixels: Vec<u8>,
}

impl SpiceFrameComposer {
    pub(crate) fn layout(&self) -> &SpiceDisplayLayout {
        &self.layout
    }

    fn channel(&mut self, generation: u64, id: u8, epoch: u64) -> Option<&mut DisplayChannel> {
        if generation < self.generation {
            return None;
        }
        if generation > self.generation {
            self.channels.clear();
            self.generation = generation;
            self.epoch = self.epoch.saturating_add(1);
        }
        let state = self.channels.entry(id).or_default();
        if epoch < state.epoch {
            return None;
        }
        if epoch > state.epoch {
            *state = DisplayChannel {
                epoch,
                ..Default::default()
            };
            self.epoch = self.epoch.saturating_add(1);
        }
        Some(state)
    }

    pub(crate) fn observe_topology(&mut self, event: &HelperEvent) -> SpiceFrameMapping {
        let HelperEvent::Topology {
            connection_generation,
            graphics_epoch,
            display_channel_id,
            monitors,
            ..
        } = event
        else {
            return SpiceFrameMapping::Ignored;
        };
        let Some(channel) =
            self.channel(*connection_generation, *display_channel_id, *graphics_epoch)
        else {
            return SpiceFrameMapping::Ignored;
        };
        channel.monitors = Some(monitors.clone());
        // Removed surfaces must not survive unplugging and later reuse of their ids.
        channel
            .surfaces
            .retain(|id, _| monitors.iter().any(|monitor| monitor.surface_id == *id));
        self.compose()
    }

    pub(crate) fn map_event(&mut self, event: HelperEvent) -> SpiceFrameMapping {
        let HelperEvent::Frame {
            connection_generation,
            graphics_epoch,
            display_channel_id,
            surface_id,
            surface_width,
            surface_height,
            rect,
            full_refresh,
            pixels,
            ..
        } = event
        else {
            return SpiceFrameMapping::Other(event);
        };
        let size = RemoteDesktopSize {
            width: surface_width,
            height: surface_height,
        };
        let target = RemoteDesktopRect::new(rect.x, rect.y, rect.width, rect.height);
        let Some(bytes) = RemoteDesktopFrame::expected_len(size) else {
            return SpiceFrameMapping::Invalid;
        };
        if surface_width == 0
            || surface_height == 0
            || surface_width > RemoteDesktopSize::MAX_DIMENSION
            || surface_height > RemoteDesktopSize::MAX_DIMENSION
            || !target.fits_in(size)
            || rect.width == 0
            || rect.height == 0
            || target.expected_len(RemoteDesktopFrameFormat::Rgba8) != Some(pixels.len())
            || (full_refresh
                && (rect.x != 0
                    || rect.y != 0
                    || rect.width != surface_width
                    || rect.height != surface_height))
        {
            return SpiceFrameMapping::Invalid;
        }
        if self
            .channel(connection_generation, display_channel_id, graphics_epoch)
            .is_none()
        {
            return SpiceFrameMapping::Ignored;
        }
        let retained: usize = self
            .channels
            .values()
            .flat_map(|channel| channel.surfaces.values())
            .map(|surface| surface.pixels.len())
            .sum();
        let channel = self.channels.get_mut(&display_channel_id).unwrap();
        let previous = channel.surfaces.get(&surface_id);
        if retained
            .saturating_sub(previous.map_or(0, |surface| surface.pixels.len()))
            .saturating_add(bytes)
            > MAX_SURFACE_BYTES
        {
            return SpiceFrameMapping::Invalid;
        }
        if previous.is_none_or(|surface| surface.size != size) {
            if !full_refresh {
                return SpiceFrameMapping::Invalid;
            }
            channel.surfaces.insert(
                surface_id,
                Surface {
                    size,
                    pixels: vec![0; bytes],
                },
            );
        }
        let surface = channel.surfaces.get_mut(&surface_id).unwrap();
        if full_refresh {
            surface.pixels = pixels;
        } else {
            copy_pixels(
                &pixels,
                rect.width,
                0,
                0,
                &mut surface.pixels,
                size.width,
                target,
            );
        }
        self.compose()
    }

    fn compose(&mut self) -> SpiceFrameMapping {
        let Some(layout) = self.build_layout() else {
            return SpiceFrameMapping::Invalid;
        };
        if self.layout != layout {
            self.epoch = self.epoch.saturating_add(1);
            self.layout = layout;
        }
        self.snapshot()
            .map_or(SpiceFrameMapping::Invalid, SpiceFrameMapping::Frame)
    }

    fn build_layout(&self) -> Option<SpiceDisplayLayout> {
        let mut layout = SpiceDisplayLayout::default();
        let mut channel_left = 0u32;
        for (&id, channel) in &self.channels {
            let fallback;
            let monitors = if let Some(monitors) = &channel.monitors {
                monitors
            } else {
                fallback = channel
                    .surfaces
                    .iter()
                    .map(|(&surface_id, surface)| HelperTopologyMonitor {
                        id: 0,
                        surface_id,
                        width: surface.size.width,
                        height: surface.size.height,
                        x: 0,
                        y: 0,
                        flags: 0,
                    })
                    .collect::<Vec<_>>();
                &fallback
            };
            let left = monitors
                .iter()
                .filter(|m| m.width > 0 && m.height > 0)
                .map(|m| m.x)
                .min()
                .unwrap_or(0);
            let top = monitors
                .iter()
                .filter(|m| m.width > 0 && m.height > 0)
                .map(|m| m.y)
                .min()
                .unwrap_or(0);
            let mut channel_width = 0;
            for monitor in monitors.iter().filter(|m| m.width > 0 && m.height > 0) {
                let source =
                    RemoteDesktopRect::new(monitor.x, monitor.y, monitor.width, monitor.height);
                source.x.checked_add(source.width)?;
                source.y.checked_add(source.height)?;
                let target = RemoteDesktopRect::new(
                    channel_left.checked_add(monitor.x - left)?,
                    monitor.y - top,
                    monitor.width,
                    monitor.height,
                );
                let right = target.x.checked_add(target.width)?;
                let bottom = target.y.checked_add(target.height)?;
                if right > RemoteDesktopSize::MAX_DIMENSION
                    || bottom > RemoteDesktopSize::MAX_DIMENSION
                {
                    return None;
                }
                channel_width = channel_width.max(right - channel_left);
                layout.size.width = layout.size.width.max(right);
                layout.size.height = layout.size.height.max(bottom);
                // A multi-head device uses monitor ids on channel zero; separate devices use channel ids.
                let display = if id == 0 {
                    u8::try_from(monitor.id).ok()?
                } else {
                    id
                };
                layout.regions.push(MonitorRegion {
                    channel: id,
                    surface: monitor.surface_id,
                    display,
                    source,
                    target,
                });
            }
            channel_left = channel_left.checked_add(channel_width)?;
        }
        Some(layout)
    }

    pub(crate) fn snapshot(&self) -> Option<RemoteDesktopHelperEvent> {
        if self.layout.size.width == 0 || self.layout.size.height == 0 {
            // The desktop frame contract requires nonzero dimensions. A transparent
            // pixel clears the old image while the input layout has no active regions.
            return Some(RemoteDesktopHelperEvent::Frame {
                frame: RemoteDesktopFrame::new(
                    RemoteDesktopSize {
                        width: 1,
                        height: 1,
                    },
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![0; 4],
                )
                .with_graphics_epoch(self.epoch),
            });
        }
        let bytes = RemoteDesktopFrame::expected_len(self.layout.size)?;
        if bytes > MAX_SURFACE_BYTES {
            return None;
        }
        if let [region] = self.layout.regions.as_slice()
            && region.source.x == 0
            && region.source.y == 0
            && region.target.x == 0
            && region.target.y == 0
            && let Some(surface) = self
                .channels
                .get(&region.channel)?
                .surfaces
                .get(&region.surface)
            && surface.size == self.layout.size
        {
            return Some(RemoteDesktopHelperEvent::Frame {
                frame: RemoteDesktopFrame::new(
                    self.layout.size,
                    RemoteDesktopFrameFormat::Rgba8,
                    surface.pixels.clone(),
                )
                .with_graphics_epoch(self.epoch),
            });
        }
        let mut pixels = vec![0; bytes];
        for region in &self.layout.regions {
            if let Some(surface) = self
                .channels
                .get(&region.channel)?
                .surfaces
                .get(&region.surface)
                && region.source.fits_in(surface.size)
            {
                copy_pixels(
                    &surface.pixels,
                    surface.size.width,
                    region.source.x,
                    region.source.y,
                    &mut pixels,
                    self.layout.size.width,
                    region.target,
                );
            }
        }
        Some(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                self.layout.size,
                RemoteDesktopFrameFormat::Rgba8,
                pixels,
            )
            .with_graphics_epoch(self.epoch),
        })
    }
}

fn copy_pixels(
    source: &[u8],
    source_width: u32,
    source_x: u32,
    source_y: u32,
    target: &mut [u8],
    target_width: u32,
    rect: RemoteDesktopRect,
) {
    // Rectangles are checked against their surface dimensions before copying.
    for row in 0..rect.height as usize {
        let from = ((source_y as usize + row) * source_width as usize + source_x as usize) * 4;
        let to = ((rect.y as usize + row) * target_width as usize + rect.x as usize) * 4;
        let count = rect.width as usize * 4;
        target[to..to + count].copy_from_slice(&source[from..from + count]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_spice_helper_protocol::{HelperPixelFormat, HelperRect};
    use oxideterm_remote_desktop::RemoteDesktopFrameDeliverySlot;

    fn frame(channel: u8, width: u32, pixels: Vec<u8>) -> HelperEvent {
        HelperEvent::Frame {
            connection_generation: 1,
            graphics_epoch: 1,
            display_channel_id: channel,
            surface_id: 0,
            surface_width: width,
            surface_height: 1,
            rect: HelperRect {
                x: 0,
                y: 0,
                width,
                height: 1,
            },
            full_refresh: true,
            format: HelperPixelFormat::Rgba8,
            pixels,
        }
    }
    fn mapped(composer: &mut SpiceFrameComposer, event: HelperEvent) -> RemoteDesktopHelperEvent {
        match composer.map_event(event) {
            SpiceFrameMapping::Frame(event) => event,
            _ => panic!("expected valid frame"),
        }
    }

    #[test]
    fn independent_channels_keep_pixels_and_pointer_identity() {
        let mut composer = SpiceFrameComposer::default();
        mapped(&mut composer, frame(0, 1, vec![255, 0, 0, 255]));
        let RemoteDesktopHelperEvent::Frame { frame } =
            mapped(&mut composer, frame(1, 1, vec![0, 0, 255, 255]))
        else {
            panic!()
        };
        assert_eq!(frame.bytes, vec![255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(composer.layout.pointer_target(0, 0), Some((0, 0, 0)));
        assert_eq!(composer.layout.pointer_target(1, 0), Some((0, 0, 1)));
    }

    fn topology(epoch: u64, monitors: Vec<HelperTopologyMonitor>) -> HelperEvent {
        HelperEvent::Topology {
            connection_generation: 1,
            graphics_epoch: epoch,
            display_channel_id: 0,
            maximum_allowed: 2,
            monitors,
        }
    }

    fn monitor(id: u32, x: u32, width: u32) -> HelperTopologyMonitor {
        HelperTopologyMonitor {
            id,
            surface_id: 0,
            width,
            height: 1,
            x,
            y: 0,
            flags: 0,
        }
    }

    fn topology_frame(composer: &mut SpiceFrameComposer, event: HelperEvent) -> RemoteDesktopFrame {
        match composer.observe_topology(&event) {
            SpiceFrameMapping::Frame(RemoteDesktopHelperEvent::Frame { frame }) => frame,
            _ => panic!("expected topology to repaint without another draw"),
        }
    }

    #[test]
    fn topology_changes_repaint_and_remap_without_waiting_for_a_draw() {
        let mut composer = SpiceFrameComposer::default();
        mapped(
            &mut composer,
            frame(0, 2, vec![255, 0, 0, 255, 0, 0, 255, 255]),
        );
        mapped(&mut composer, frame(1, 1, vec![0, 255, 0, 255]));
        let cropped = topology_frame(&mut composer, topology(1, vec![monitor(3, 1, 1)]));
        assert_eq!(cropped.bytes, [0, 0, 255, 255, 0, 255, 0, 255]);
        assert_eq!(composer.layout.pointer_target(0, 0), Some((0, 0, 3)));
        assert_eq!(composer.layout.cursor_position(0, 1, 0), Some((0, 0)));
        assert_eq!(composer.layout.pointer_target(1, 0), Some((0, 0, 1)));
        let unplugged = topology_frame(&mut composer, topology(1, vec![]));
        assert_eq!(unplugged.bytes, [0, 255, 0, 255]);
        assert_eq!(composer.layout.pointer_target(0, 0), Some((0, 0, 1)));
    }

    #[test]
    fn resize_waits_safely_for_new_surface_and_reset_rejects_old_epoch() {
        let mut composer = SpiceFrameComposer::default();
        mapped(&mut composer, frame(0, 1, vec![255, 0, 0, 255]));
        let resized = topology_frame(&mut composer, topology(1, vec![monitor(0, 0, 2)]));
        assert_eq!(resized.bytes, [0; 8]);
        let pixels = vec![0, 255, 0, 255, 0, 0, 255, 255];
        assert!(matches!(mapped(&mut composer, frame(0, 2, pixels.clone())),
            RemoteDesktopHelperEvent::Frame { frame } if frame.bytes == pixels));
        let cleared = topology_frame(&mut composer, topology(2, vec![]));
        assert_eq!(cleared.bytes, [0; 4]);
        assert_eq!(composer.layout.pointer_target(0, 0), None);
        assert!(matches!(
            composer.map_event(frame(0, 1, vec![255; 4])),
            SpiceFrameMapping::Ignored
        ));
        assert!(matches!(
            composer.observe_topology(&topology(1, vec![monitor(0, 0, 1)])),
            SpiceFrameMapping::Ignored
        ));
        let awaiting = topology_frame(&mut composer, topology(2, vec![monitor(0, 0, 1)]));
        assert_eq!(awaiting.bytes, [0; 4]);
        let mut fresh = frame(0, 1, vec![0, 0, 255, 255]);
        if let HelperEvent::Frame { graphics_epoch, .. } = &mut fresh {
            *graphics_epoch = 2;
        }
        assert!(
            matches!(mapped(&mut composer, fresh), RemoteDesktopHelperEvent::Frame { frame }
            if frame.bytes == [0, 0, 255, 255])
        );
    }

    #[test]
    fn shared_surface_supports_multiple_heads_and_recovers_after_hiding() {
        let mut composer = SpiceFrameComposer::default();
        composer.observe_topology(&HelperEvent::Topology {
            connection_generation: 1,
            graphics_epoch: 1,
            display_channel_id: 0,
            maximum_allowed: 2,
            monitors: (0..2)
                .map(|id| HelperTopologyMonitor {
                    id,
                    surface_id: 0,
                    width: 1,
                    height: 1,
                    x: id,
                    y: 0,
                    flags: 0,
                })
                .collect(),
        });
        let slot = RemoteDesktopFrameDeliverySlot::new();
        let pixels = vec![255, 0, 0, 255, 0, 0, 255, 255];
        slot.push(mapped(&mut composer, frame(0, 2, pixels.clone())));
        assert!(
            matches!(slot.take(), Some(RemoteDesktopHelperEvent::Frame { frame }) if frame.bytes == pixels)
        );
        slot.set_visible(false);
        let latest = vec![0, 255, 0, 255, 255, 255, 0, 255];
        slot.push(mapped(&mut composer, frame(0, 2, latest.clone())));
        slot.set_visible(true);
        assert!(
            matches!(slot.take(), Some(RemoteDesktopHelperEvent::Frame { frame }) if frame.bytes == latest)
        );
        assert_eq!(composer.layout.pointer_target(1, 0), Some((0, 0, 1)));
    }
}
