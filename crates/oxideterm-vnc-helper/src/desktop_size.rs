// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_remote_desktop::{NegotiatedCapabilityStatus, RemoteDesktopMonitorLayout};

const VNC_SET_DESKTOP_SIZE_MESSAGE_TYPE: u8 = 251;
const VNC_MAX_SCREEN_COUNT: usize = u8::MAX as usize;
const VNC_SCREEN_FLAGS_NONE: u32 = 0;
const VNC_SCREEN_ID_HASH_OFFSET: u32 = 0x811c_9dc5;
const VNC_SCREEN_ID_HASH_PRIME: u32 = 0x0100_0193;
const VNC_BYTES_PER_PIXEL: u64 = 4;
const VNC_MAX_FRAMEBUFFER_PIXELS: u64 = MAX_VNC_FRAME_BYTES as u64 / VNC_BYTES_PER_PIXEL;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VncDesktopSizeReason {
    Server,
    Client,
    OtherClient,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VncDesktopSizeResult {
    Success,
    ResizeProhibited,
    OutOfResources,
    InvalidScreenLayout,
}

impl VncDesktopSizeResult {
    fn rejection_message(self) -> Option<&'static str> {
        match self {
            Self::Success => None,
            Self::ResizeProhibited => Some("The VNC server does not permit desktop resizing."),
            Self::OutOfResources => {
                Some("The VNC server could not allocate the requested desktop layout.")
            }
            Self::InvalidScreenLayout => {
                Some("The VNC server rejected the requested screen layout.")
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VncDesktopScreen {
    pub(super) id: u32,
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) flags: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VncDesktopLayout {
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) screens: Vec<VncDesktopScreen>,
}

impl VncDesktopLayout {
    pub(super) fn single(size: RemoteDesktopSize) -> Result<Self, String> {
        let width = u16::try_from(size.width)
            .map_err(|_| "VNC framebuffer width exceeds the protocol limit.".to_string())?;
        let height = u16::try_from(size.height)
            .map_err(|_| "VNC framebuffer height exceeds the protocol limit.".to_string())?;
        let layout = Self {
            width,
            height,
            screens: vec![VncDesktopScreen {
                id: 0,
                x: 0,
                y: 0,
                width,
                height,
                flags: VNC_SCREEN_FLAGS_NONE,
            }],
        };
        layout.validate_wire_layout()?;
        Ok(layout)
    }

    pub(super) fn from_remote_layout(layout: &RemoteDesktopMonitorLayout) -> Result<Self, String> {
        if layout.monitors.is_empty() {
            return Err("VNC monitor layout must contain at least one screen.".to_string());
        }
        if layout.monitors.len() > VNC_MAX_SCREEN_COUNT {
            return Err(format!(
                "VNC monitor layout exceeds the {VNC_MAX_SCREEN_COUNT}-screen protocol limit."
            ));
        }

        let primary_count = layout
            .monitors
            .iter()
            .filter(|monitor| monitor.primary)
            .count();
        if primary_count != 1 {
            return Err("VNC monitor layout must contain exactly one primary screen.".to_string());
        }

        let mut stable_ids = HashSet::with_capacity(layout.monitors.len());
        let mut screen_ids = HashSet::with_capacity(layout.monitors.len());
        let mut left = i64::MAX;
        let mut top = i64::MAX;
        let mut right = i64::MIN;
        let mut bottom = i64::MIN;

        for monitor in &layout.monitors {
            if monitor.stable_id.trim().is_empty() {
                return Err("VNC monitor stable identifiers must not be empty.".to_string());
            }
            if !stable_ids.insert(monitor.stable_id.as_str()) {
                return Err("VNC monitor stable identifiers must be unique.".to_string());
            }
            if monitor.width == 0 || monitor.height == 0 {
                return Err("VNC monitor dimensions must be greater than zero.".to_string());
            }
            if monitor.width > u16::MAX.into() || monitor.height > u16::MAX.into() {
                return Err("VNC monitor dimensions exceed the protocol limit.".to_string());
            }

            let monitor_right = i64::from(monitor.left)
                .checked_add(i64::from(monitor.width))
                .ok_or_else(|| "VNC monitor horizontal coordinates overflowed.".to_string())?;
            let monitor_bottom = i64::from(monitor.top)
                .checked_add(i64::from(monitor.height))
                .ok_or_else(|| "VNC monitor vertical coordinates overflowed.".to_string())?;
            left = left.min(i64::from(monitor.left));
            top = top.min(i64::from(monitor.top));
            right = right.max(monitor_right);
            bottom = bottom.max(monitor_bottom);

            let screen_id = stable_vnc_screen_id(&monitor.stable_id);
            if !screen_ids.insert(screen_id) {
                return Err(
                    "VNC monitor stable identifiers produced duplicate protocol screen IDs."
                        .to_string(),
                );
            }
        }

        validate_non_overlapping_monitors(layout)?;

        let width = u16::try_from(
            right
                .checked_sub(left)
                .ok_or_else(|| "VNC framebuffer width overflowed.".to_string())?,
        )
        .map_err(|_| "VNC framebuffer width exceeds the protocol limit.".to_string())?;
        let height = u16::try_from(
            bottom
                .checked_sub(top)
                .ok_or_else(|| "VNC framebuffer height overflowed.".to_string())?,
        )
        .map_err(|_| "VNC framebuffer height exceeds the protocol limit.".to_string())?;

        let mut monitors = layout.monitors.iter().collect::<Vec<_>>();
        // RFB has no standard primary-screen flag, so preserve the primary as
        // the first screen while keeping all wire flags at zero.
        monitors.sort_by_key(|monitor| {
            (
                !monitor.primary,
                monitor.top,
                monitor.left,
                monitor.stable_id.as_str(),
            )
        });

        let screens = monitors
            .into_iter()
            .map(|monitor| {
                let x = i64::from(monitor.left)
                    .checked_sub(left)
                    .and_then(|value| u16::try_from(value).ok())
                    .ok_or_else(|| "VNC monitor horizontal position is invalid.".to_string())?;
                let y = i64::from(monitor.top)
                    .checked_sub(top)
                    .and_then(|value| u16::try_from(value).ok())
                    .ok_or_else(|| "VNC monitor vertical position is invalid.".to_string())?;
                Ok(VncDesktopScreen {
                    id: stable_vnc_screen_id(&monitor.stable_id),
                    x,
                    y,
                    width: monitor.width as u16,
                    height: monitor.height as u16,
                    flags: VNC_SCREEN_FLAGS_NONE,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let layout = Self {
            width,
            height,
            screens,
        };
        layout.validate_wire_layout()?;
        Ok(layout)
    }

    pub(super) fn validate_wire_layout(&self) -> Result<(), String> {
        validate_vnc_framebuffer_size(self.width, self.height)?;
        if self.screens.is_empty() {
            return Err("VNC desktop layout must contain at least one screen.".to_string());
        }
        if self.screens.len() > VNC_MAX_SCREEN_COUNT {
            return Err(format!(
                "VNC desktop layout exceeds the {VNC_MAX_SCREEN_COUNT}-screen protocol limit."
            ));
        }

        let mut screen_ids = HashSet::with_capacity(self.screens.len());
        for screen in &self.screens {
            if !screen_ids.insert(screen.id) {
                return Err("VNC desktop screen identifiers must be unique.".to_string());
            }
            if screen.width == 0 || screen.height == 0 {
                return Err("VNC desktop screen dimensions must be greater than zero.".to_string());
            }
            let right = u32::from(screen.x)
                .checked_add(u32::from(screen.width))
                .ok_or_else(|| "VNC desktop screen horizontal bounds overflowed.".to_string())?;
            let bottom = u32::from(screen.y)
                .checked_add(u32::from(screen.height))
                .ok_or_else(|| "VNC desktop screen vertical bounds overflowed.".to_string())?;
            if right > u32::from(self.width) || bottom > u32::from(self.height) {
                return Err("VNC desktop screen lies outside the framebuffer.".to_string());
            }
        }
        validate_non_overlapping_vnc_screens(&self.screens)?;
        Ok(())
    }
}

pub(super) fn initial_vnc_desktop_layout(
    size: RemoteDesktopSize,
    use_all_monitors: bool,
    monitor_layout: &RemoteDesktopMonitorLayout,
) -> Result<VncDesktopLayout, String> {
    if use_all_monitors && !monitor_layout.monitors.is_empty() {
        VncDesktopLayout::from_remote_layout(monitor_layout)
    } else {
        VncDesktopLayout::single(size)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VncExtendedDesktopSize {
    pub(super) reason: VncDesktopSizeReason,
    pub(super) result: VncDesktopSizeResult,
    pub(super) layout: VncDesktopLayout,
}

impl VncExtendedDesktopSize {
    pub(super) fn applies_layout(&self) -> bool {
        self.reason != VncDesktopSizeReason::Client || self.result == VncDesktopSizeResult::Success
    }
}

#[derive(Debug)]
pub(super) struct VncDesktopResizeState {
    capability: NegotiatedCapabilityStatus,
    pending_layout: Option<VncDesktopLayout>,
    request_in_flight: bool,
    server_layout: Option<VncDesktopLayout>,
}

pub(super) type SharedVncDesktopResize = Arc<Mutex<VncDesktopResizeState>>;

impl VncDesktopResizeState {
    pub(super) fn new(initial_layout: VncDesktopLayout) -> Self {
        Self {
            capability: NegotiatedCapabilityStatus::Unknown,
            pending_layout: Some(initial_layout),
            request_in_flight: false,
            server_layout: None,
        }
    }

    pub(super) fn queue_layout(
        &mut self,
        layout: VncDesktopLayout,
    ) -> Result<Option<Vec<u8>>, String> {
        layout.validate_wire_layout()?;
        match self.capability {
            NegotiatedCapabilityStatus::Unknown => {
                self.pending_layout = Some(layout);
                Err(
                    "The VNC server has not negotiated remote desktop resizing yet; the current server framebuffer size remains active."
                        .to_string(),
                )
            }
            NegotiatedCapabilityStatus::Unsupported => {
                Err("The VNC server did not negotiate remote desktop resizing.".to_string())
            }
            NegotiatedCapabilityStatus::Supported if self.request_in_flight => {
                // Coalesce rapid viewport changes while the server owns the
                // previous SetDesktopSize request.
                self.pending_layout = Some(layout);
                Ok(None)
            }
            NegotiatedCapabilityStatus::Supported => {
                self.request_in_flight = true;
                set_desktop_size_message(&layout).map(Some)
            }
        }
    }

    pub(super) fn observe_framebuffer_update(
        &mut self,
        event: &VncServerEvent,
    ) -> Result<VncDesktopResizeTransition, String> {
        let mut extended_updates = Vec::new();
        collect_extended_desktop_sizes(event, &mut extended_updates);

        if extended_updates.is_empty() {
            // Absence is not a negative capability signal. Servers advertise
            // ExtendedDesktopSize asynchronously in a framebuffer update.
            return Ok(VncDesktopResizeTransition::default());
        }

        let previous_capability = self.capability;
        let explicitly_prohibited = extended_updates.iter().any(|update| {
            update.reason == VncDesktopSizeReason::Client
                && update.result == VncDesktopSizeResult::ResizeProhibited
        });
        self.capability = if explicitly_prohibited {
            NegotiatedCapabilityStatus::Unsupported
        } else {
            NegotiatedCapabilityStatus::Supported
        };
        let capability_changed =
            (self.capability != previous_capability).then_some(self.capability);
        let mut rejection = None;
        for update in extended_updates {
            if update.applies_layout() {
                self.server_layout = Some(update.layout.clone());
            }
            if update.reason == VncDesktopSizeReason::Client {
                self.request_in_flight = false;
                if let Some(message) = update.result.rejection_message() {
                    rejection = Some(message.to_string());
                }
            }
        }

        if self.capability == NegotiatedCapabilityStatus::Unsupported {
            self.pending_layout = None;
        }
        let next_request = if self.capability == NegotiatedCapabilityStatus::Supported
            && !self.request_in_flight
        {
            self.pending_layout
                .take()
                .map(|layout| {
                    self.request_in_flight = true;
                    set_desktop_size_message(&layout)
                })
                .transpose()?
        } else {
            None
        };

        Ok(VncDesktopResizeTransition {
            next_request,
            rejection,
            capability_changed,
        })
    }

    #[cfg(test)]
    pub(super) fn server_layout(&self) -> Option<&VncDesktopLayout> {
        self.server_layout.as_ref()
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct VncDesktopResizeTransition {
    pub(super) next_request: Option<Vec<u8>>,
    pub(super) rejection: Option<String>,
    pub(super) capability_changed: Option<NegotiatedCapabilityStatus>,
}

pub(super) fn request_vnc_desktop_layout(
    desktop_resize: &SharedVncDesktopResize,
    writer: &SharedVncWriter,
    event_writer: &SharedEventWriter,
    layout: VncDesktopLayout,
) -> Result<(), String> {
    let request = match desktop_resize
        .lock()
        .map_err(|_| "VNC desktop resize state lock is poisoned.".to_string())?
        .queue_layout(layout)
    {
        Ok(request) => request,
        Err(message) => {
            send_vnc_resize_status(event_writer, message)?;
            return Ok(());
        }
    };
    if let Some(request) = request {
        write_vnc_message(writer, &request)?;
    }
    Ok(())
}

pub(super) fn handle_vnc_desktop_size_event(
    event: &VncServerEvent,
    desktop_resize: &SharedVncDesktopResize,
    capabilities: &SharedVncCapabilities,
    writer: &SharedVncWriter,
    event_writer: &SharedEventWriter,
) -> Result<(), String> {
    let transition = desktop_resize
        .lock()
        .map_err(|_| "VNC desktop resize state lock is poisoned.".to_string())?
        .observe_framebuffer_update(event)?;

    if let Some(status) = transition.capability_changed
        && let Some(event) = update_vnc_capabilities(capabilities, |snapshot| {
            snapshot.resize = status;
            snapshot.multi_monitor = status;
        })?
    {
        send_event(event_writer, event)?;
    }
    if let Some(request) = transition.next_request {
        write_vnc_message(writer, &request)?;
    }
    if let Some(message) = transition.rejection {
        send_vnc_resize_status(event_writer, message)?;
    }
    Ok(())
}

pub(super) fn set_desktop_size_message(layout: &VncDesktopLayout) -> Result<Vec<u8>, String> {
    layout.validate_wire_layout()?;
    let screen_count = u8::try_from(layout.screens.len())
        .map_err(|_| "VNC desktop layout has too many screens.".to_string())?;
    let mut message = Vec::with_capacity(8 + layout.screens.len() * 16);
    message.push(VNC_SET_DESKTOP_SIZE_MESSAGE_TYPE);
    message.push(0);
    push_be_u16(&mut message, layout.width);
    push_be_u16(&mut message, layout.height);
    message.push(screen_count);
    message.push(0);
    for screen in &layout.screens {
        push_be_u32(&mut message, screen.id);
        push_be_u16(&mut message, screen.x);
        push_be_u16(&mut message, screen.y);
        push_be_u16(&mut message, screen.width);
        push_be_u16(&mut message, screen.height);
        push_be_u32(&mut message, screen.flags);
    }
    Ok(message)
}

pub(super) fn read_extended_desktop_size(
    reader: &mut impl Read,
    rect: RfbRect,
) -> Result<VncExtendedDesktopSize, String> {
    let reason = match rect.x {
        0 => VncDesktopSizeReason::Server,
        1 => VncDesktopSizeReason::Client,
        2 => VncDesktopSizeReason::OtherClient,
        other => {
            return Err(format!(
                "VNC ExtendedDesktopSize used unknown reason code {other}."
            ));
        }
    };
    let result = match rect.y {
        0 => VncDesktopSizeResult::Success,
        1 => VncDesktopSizeResult::ResizeProhibited,
        2 => VncDesktopSizeResult::OutOfResources,
        3 => VncDesktopSizeResult::InvalidScreenLayout,
        other => {
            return Err(format!(
                "VNC ExtendedDesktopSize used unknown result code {other}."
            ));
        }
    };
    if reason != VncDesktopSizeReason::Client && result != VncDesktopSizeResult::Success {
        return Err(
            "VNC ExtendedDesktopSize result codes are only valid for client requests.".to_string(),
        );
    }

    let screen_count = read_u8(reader)
        .map_err(|error| format!("VNC ExtendedDesktopSize screen count read failed: {error}"))?;
    let _padding = read_exact_array::<3, _>(reader)
        .map_err(|error| format!("VNC ExtendedDesktopSize padding read failed: {error}"))?;
    let mut screens = Vec::with_capacity(usize::from(screen_count));
    for _ in 0..screen_count {
        let bytes = read_exact_array::<16, _>(reader)
            .map_err(|error| format!("VNC ExtendedDesktopSize screen read failed: {error}"))?;
        screens.push(VncDesktopScreen {
            id: be_u32(&bytes[0..4]),
            x: be_u16(&bytes[4..6]),
            y: be_u16(&bytes[6..8]),
            width: be_u16(&bytes[8..10]),
            height: be_u16(&bytes[10..12]),
            flags: be_u32(&bytes[12..16]),
        });
    }

    let layout = VncDesktopLayout {
        width: rect.width,
        height: rect.height,
        screens,
    };
    layout.validate_wire_layout()?;
    Ok(VncExtendedDesktopSize {
        reason,
        result,
        layout,
    })
}

pub(super) fn send_vnc_resize_status(
    event_writer: &SharedEventWriter,
    message: String,
) -> Result<(), String> {
    send_event(
        event_writer,
        RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connected,
            message: Some(message),
        },
    )
}

fn validate_vnc_framebuffer_size(width: u16, height: u16) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("VNC framebuffer dimensions must be greater than zero.".to_string());
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "VNC framebuffer pixel count overflowed.".to_string())?;
    if pixels > VNC_MAX_FRAMEBUFFER_PIXELS {
        return Err("VNC framebuffer exceeds the helper memory limit.".to_string());
    }
    Ok(())
}

fn validate_non_overlapping_monitors(layout: &RemoteDesktopMonitorLayout) -> Result<(), String> {
    for (index, left) in layout.monitors.iter().enumerate() {
        let left_right = i64::from(left.left) + i64::from(left.width);
        let left_bottom = i64::from(left.top) + i64::from(left.height);
        for right in layout.monitors.iter().skip(index + 1) {
            let right_right = i64::from(right.left) + i64::from(right.width);
            let right_bottom = i64::from(right.top) + i64::from(right.height);
            let overlaps = i64::from(left.left) < right_right
                && i64::from(right.left) < left_right
                && i64::from(left.top) < right_bottom
                && i64::from(right.top) < left_bottom;
            if overlaps {
                return Err("VNC monitor rectangles must not overlap.".to_string());
            }
        }
    }
    Ok(())
}

fn validate_non_overlapping_vnc_screens(screens: &[VncDesktopScreen]) -> Result<(), String> {
    for (index, left) in screens.iter().enumerate() {
        let left_right = u32::from(left.x) + u32::from(left.width);
        let left_bottom = u32::from(left.y) + u32::from(left.height);
        for right in screens.iter().skip(index + 1) {
            let right_right = u32::from(right.x) + u32::from(right.width);
            let right_bottom = u32::from(right.y) + u32::from(right.height);
            let overlaps = u32::from(left.x) < right_right
                && u32::from(right.x) < left_right
                && u32::from(left.y) < right_bottom
                && u32::from(right.y) < left_bottom;
            if overlaps {
                return Err("VNC desktop screen rectangles must not overlap.".to_string());
            }
        }
    }
    Ok(())
}

fn stable_vnc_screen_id(stable_id: &str) -> u32 {
    stable_id
        .as_bytes()
        .iter()
        .fold(VNC_SCREEN_ID_HASH_OFFSET, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(VNC_SCREEN_ID_HASH_PRIME)
        })
}

fn collect_extended_desktop_sizes<'a>(
    event: &'a VncServerEvent,
    updates: &mut Vec<&'a VncExtendedDesktopSize>,
) {
    match event {
        VncServerEvent::ExtendedDesktopSize(update) => updates.push(update),
        VncServerEvent::Batch(events) => {
            for event in events {
                collect_extended_desktop_sizes(event, updates);
            }
        }
        _ => {}
    }
}
