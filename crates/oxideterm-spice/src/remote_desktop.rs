// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use oxide_spice_helper_protocol::{
    HelperButtonState, HelperClipboardFormat, HelperClipboardSelection, HelperEvent,
    HelperKeyState, HelperMonitor, HelperMouseButton, HelperMouseMode, HelperRequest,
    HelperTopologyMonitor,
};
use oxideterm_remote_desktop::{
    RemoteDesktopClipboardData, RemoteDesktopClipboardFormat, RemoteDesktopHelperEvent,
    RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState, RemoteDesktopLockKeys,
    RemoteDesktopMonitorLayout, RemoteDesktopMouseButton, RemoteDesktopMouseButtonState,
    RemoteDesktopSize,
};
use zeroize::{Zeroize, Zeroizing};

const SPICE_PRIMARY_DISPLAY_ID: u8 = 0;
const SPICE_DISPLAY_DEPTH: u32 = 32;
const SPICE_SCROLL_LOCK_BIT: u16 = 1 << 0;
const SPICE_NUM_LOCK_BIT: u16 = 1 << 1;
const SPICE_CAPS_LOCK_BIT: u16 = 1 << 2;

pub struct SpiceRemoteDesktopEventActions {
    pub shared_event: Option<RemoteDesktopHelperEvent>,
    pub response: Option<HelperRequest>,
    pub event: Option<HelperEvent>,
}

/// Adapts the common remote-desktop controls without hiding SPICE-only events.
pub struct SpiceRemoteDesktopAdapter {
    pressed_buttons: u16,
    pressed_keys: HashSet<u32>,
    mouse_mode: HelperMouseMode,
    last_pointer_position: Option<(u32, u32)>,
    local_clipboard: HashMap<HelperClipboardFormat, Zeroizing<Vec<u8>>>,
    monitor_layout: Vec<HelperMonitor>,
    topology: Vec<HelperTopologyMonitor>,
    fallback_size: RemoteDesktopSize,
}

impl SpiceRemoteDesktopAdapter {
    pub fn new(
        initial_size: RemoteDesktopSize,
        monitor_layout: RemoteDesktopMonitorLayout,
    ) -> Self {
        let monitor_layout = spice_monitors(monitor_layout, initial_size);
        Self {
            pressed_buttons: 0,
            pressed_keys: HashSet::new(),
            mouse_mode: HelperMouseMode::Client,
            last_pointer_position: None,
            local_clipboard: HashMap::new(),
            monitor_layout,
            topology: Vec::new(),
            fallback_size: initial_size,
        }
    }

    pub fn map_request(&mut self, request: RemoteDesktopHelperRequest) -> Vec<HelperRequest> {
        match request {
            RemoteDesktopHelperRequest::Resize { size, .. } => {
                self.fallback_size = size;
                self.monitor_layout = vec![single_monitor(size)];
                vec![HelperRequest::MonitorLayout {
                    monitors: self.monitor_layout.clone(),
                }]
            }
            RemoteDesktopHelperRequest::UpdateDisplayLayout { layout } => {
                self.monitor_layout = spice_monitors(layout, self.fallback_size);
                vec![HelperRequest::MonitorLayout {
                    monitors: self.monitor_layout.clone(),
                }]
            }
            RemoteDesktopHelperRequest::MouseMove { x, y } => {
                if self.mouse_mode == HelperMouseMode::Server {
                    let previous = self.last_pointer_position.replace((x, y));
                    let Some((previous_x, previous_y)) = previous else {
                        // Establish a local baseline before sending relative server-mode motion.
                        return Vec::new();
                    };
                    vec![HelperRequest::PointerMotion {
                        dx: pointer_delta(previous_x, x),
                        dy: pointer_delta(previous_y, y),
                        buttons: self.pressed_buttons,
                    }]
                } else {
                    self.last_pointer_position = Some((x, y));
                    let (x, y, display_id) = self.pointer_target(x, y);
                    vec![HelperRequest::PointerPosition {
                        x,
                        y,
                        buttons: self.pressed_buttons,
                        display_id,
                    }]
                }
            }
            RemoteDesktopHelperRequest::MouseButton { button, state } => {
                let (button, bit) = spice_mouse_button(button);
                match state {
                    RemoteDesktopMouseButtonState::Pressed => self.pressed_buttons |= bit,
                    RemoteDesktopMouseButtonState::Released => self.pressed_buttons &= !bit,
                }
                vec![HelperRequest::MouseButton {
                    button,
                    state: match state {
                        RemoteDesktopMouseButtonState::Pressed => HelperButtonState::Pressed,
                        RemoteDesktopMouseButtonState::Released => HelperButtonState::Released,
                    },
                    buttons: self.pressed_buttons,
                }]
            }
            RemoteDesktopHelperRequest::Wheel { delta } => {
                if delta.y == 0.0 {
                    return Vec::new();
                }
                let button = if delta.y > 0.0 {
                    HelperMouseButton::WheelUp
                } else {
                    HelperMouseButton::WheelDown
                };
                let bit = spice_mouse_button_bit(button);
                vec![
                    HelperRequest::MouseButton {
                        button,
                        state: HelperButtonState::Pressed,
                        buttons: self.pressed_buttons | bit,
                    },
                    HelperRequest::MouseButton {
                        button,
                        state: HelperButtonState::Released,
                        buttons: self.pressed_buttons,
                    },
                ]
            }
            RemoteDesktopHelperRequest::Key { key, state } => {
                let Some(code) = spice_set1_scancode(&key) else {
                    return Vec::new();
                };
                match state {
                    RemoteDesktopKeyState::Pressed => {
                        self.pressed_keys.insert(code);
                    }
                    RemoteDesktopKeyState::Released => {
                        self.pressed_keys.remove(&code);
                    }
                }
                vec![HelperRequest::KeyCode {
                    code,
                    state: match state {
                        RemoteDesktopKeyState::Pressed => HelperKeyState::Pressed,
                        RemoteDesktopKeyState::Released => HelperKeyState::Released,
                    },
                }]
            }
            RemoteDesktopHelperRequest::ClipboardText { text } => {
                self.local_clipboard.clear();
                self.local_clipboard.insert(
                    HelperClipboardFormat::Utf8Text,
                    Zeroizing::new(text.into_bytes()),
                );
                vec![HelperRequest::ClipboardOffer {
                    selection: HelperClipboardSelection::Clipboard,
                    formats: vec![HelperClipboardFormat::Utf8Text],
                }]
            }
            RemoteDesktopHelperRequest::ClipboardData { data } => {
                let Some(format) = spice_clipboard_format(data.format) else {
                    return Vec::new();
                };
                self.local_clipboard.clear();
                self.local_clipboard
                    .insert(format, Zeroizing::new(data.bytes));
                vec![HelperRequest::ClipboardOffer {
                    selection: HelperClipboardSelection::Clipboard,
                    formats: vec![format],
                }]
            }
            RemoteDesktopHelperRequest::SynchronizeLockKeys { keys } => {
                vec![HelperRequest::Modifiers {
                    bits: spice_lock_key_bits(keys),
                }]
            }
            RemoteDesktopHelperRequest::ReleaseAllInputs => self.release_all_inputs(),
            RemoteDesktopHelperRequest::RequestFrame => vec![HelperRequest::MonitorLayout {
                // Repeating the active layout asks the guest agent to repaint after a lost delta.
                monitors: self.monitor_layout.clone(),
            }],
            RemoteDesktopHelperRequest::Close => vec![HelperRequest::Close],
            RemoteDesktopHelperRequest::Text { .. }
            | RemoteDesktopHelperRequest::ClipboardFiles { .. }
            | RemoteDesktopHelperRequest::VncListRemoteFiles { .. }
            | RemoteDesktopHelperRequest::VncDownloadRemoteFiles { .. }
            | RemoteDesktopHelperRequest::CancelVncFileTransfer { .. }
            | RemoteDesktopHelperRequest::CancelClipboardTransfer { .. }
            | RemoteDesktopHelperRequest::StartConnect { .. }
            | RemoteDesktopHelperRequest::Connect { .. }
            | RemoteDesktopHelperRequest::Authenticate { .. }
            | RemoteDesktopHelperRequest::Reconnect => Vec::new(),
        }
    }

    pub fn map_event(&mut self, event: HelperEvent) -> SpiceRemoteDesktopEventActions {
        match event {
            HelperEvent::MouseMode { mode } => {
                self.mouse_mode = mode;
                self.last_pointer_position = None;
                SpiceRemoteDesktopEventActions {
                    shared_event: None,
                    response: None,
                    event: Some(HelperEvent::MouseMode { mode }),
                }
            }
            HelperEvent::Topology {
                connection_generation,
                graphics_epoch,
                display_channel_id,
                maximum_allowed,
                monitors,
            } => {
                self.topology.clone_from(&monitors);
                SpiceRemoteDesktopEventActions {
                    shared_event: None,
                    response: None,
                    event: Some(HelperEvent::Topology {
                        connection_generation,
                        graphics_epoch,
                        display_channel_id,
                        maximum_allowed,
                        monitors,
                    }),
                }
            }
            HelperEvent::ClipboardRequest {
                request_id,
                selection,
                format,
            } => {
                let response =
                    self.local_clipboard
                        .get(&format)
                        .map(|data| HelperRequest::ClipboardProvide {
                            request_id,
                            data: data.to_vec(),
                        });
                SpiceRemoteDesktopEventActions {
                    shared_event: None,
                    response,
                    event: Some(HelperEvent::ClipboardRequest {
                        request_id,
                        selection,
                        format,
                    }),
                }
            }
            HelperEvent::ClipboardOffer {
                selection,
                revision,
                formats,
            } => {
                let response = preferred_clipboard_format(&formats)
                    .map(|format| HelperRequest::ClipboardRequest { selection, format });
                SpiceRemoteDesktopEventActions {
                    shared_event: None,
                    response,
                    event: Some(HelperEvent::ClipboardOffer {
                        selection,
                        revision,
                        formats,
                    }),
                }
            }
            HelperEvent::ClipboardData {
                selection: _,
                format,
                mut data,
            } => {
                let shared_event = remote_clipboard_event(format, &data);
                // The shared clipboard event becomes the sole owner after this adapter boundary.
                data.zeroize();
                SpiceRemoteDesktopEventActions {
                    shared_event,
                    response: None,
                    event: None,
                }
            }
            event => SpiceRemoteDesktopEventActions {
                shared_event: None,
                response: None,
                event: Some(event),
            },
        }
    }

    fn release_all_inputs(&mut self) -> Vec<HelperRequest> {
        self.last_pointer_position = None;
        let mut requests = self
            .pressed_keys
            .drain()
            .map(|code| HelperRequest::KeyCode {
                code,
                state: HelperKeyState::Released,
            })
            .collect::<Vec<_>>();
        for button in [
            HelperMouseButton::Left,
            HelperMouseButton::Middle,
            HelperMouseButton::Right,
            HelperMouseButton::Side,
            HelperMouseButton::Extra,
        ] {
            let bit = spice_mouse_button_bit(button);
            if self.pressed_buttons & bit == 0 {
                continue;
            }
            self.pressed_buttons &= !bit;
            requests.push(HelperRequest::MouseButton {
                button,
                state: HelperButtonState::Released,
                buttons: self.pressed_buttons,
            });
        }
        requests
    }

    fn pointer_target(&self, x: u32, y: u32) -> (u32, u32, u8) {
        self.topology
            .iter()
            .find_map(|monitor| {
                let right = monitor.x.saturating_add(monitor.width);
                let bottom = monitor.y.saturating_add(monitor.height);
                let display_id = u8::try_from(monitor.id).ok()?;
                (monitor.x <= x && x < right && monitor.y <= y && y < bottom).then_some((
                    x.saturating_sub(monitor.x),
                    y.saturating_sub(monitor.y),
                    display_id,
                ))
            })
            .unwrap_or((x, y, SPICE_PRIMARY_DISPLAY_ID))
    }
}

fn spice_monitors(
    layout: RemoteDesktopMonitorLayout,
    fallback: RemoteDesktopSize,
) -> Vec<HelperMonitor> {
    if layout.monitors.is_empty() {
        return vec![single_monitor(fallback)];
    }
    layout
        .monitors
        .into_iter()
        .map(|monitor| HelperMonitor {
            width: monitor.width,
            height: monitor.height,
            depth: SPICE_DISPLAY_DEPTH,
            x: monitor.left,
            y: monitor.top,
            width_mm: monitor
                .physical_width_mm
                .and_then(|width| u16::try_from(width).ok()),
            height_mm: monitor
                .physical_height_mm
                .and_then(|height| u16::try_from(height).ok()),
        })
        .collect()
}

fn single_monitor(size: RemoteDesktopSize) -> HelperMonitor {
    HelperMonitor {
        width: size.width,
        height: size.height,
        depth: SPICE_DISPLAY_DEPTH,
        x: 0,
        y: 0,
        width_mm: None,
        height_mm: None,
    }
}

fn spice_mouse_button(button: RemoteDesktopMouseButton) -> (HelperMouseButton, u16) {
    let button = match button {
        RemoteDesktopMouseButton::Left => HelperMouseButton::Left,
        RemoteDesktopMouseButton::Middle => HelperMouseButton::Middle,
        RemoteDesktopMouseButton::Right => HelperMouseButton::Right,
        RemoteDesktopMouseButton::Back => HelperMouseButton::Side,
        RemoteDesktopMouseButton::Forward => HelperMouseButton::Extra,
    };
    (button, spice_mouse_button_bit(button))
}

fn spice_mouse_button_bit(button: HelperMouseButton) -> u16 {
    match button {
        HelperMouseButton::Left => 1 << 0,
        HelperMouseButton::Middle => 1 << 1,
        HelperMouseButton::Right => 1 << 2,
        HelperMouseButton::WheelUp => 1 << 3,
        HelperMouseButton::WheelDown => 1 << 4,
        HelperMouseButton::Side => 1 << 5,
        HelperMouseButton::Extra => 1 << 6,
    }
}

fn pointer_delta(previous: u32, current: u32) -> i32 {
    // GPUI coordinates are unsigned, while SPICE relative motion is signed and bounded.
    i64::from(current)
        .saturating_sub(i64::from(previous))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn spice_lock_key_bits(keys: RemoteDesktopLockKeys) -> u16 {
    (u16::from(keys.scroll_lock) * SPICE_SCROLL_LOCK_BIT)
        | (u16::from(keys.num_lock) * SPICE_NUM_LOCK_BIT)
        | (u16::from(keys.caps_lock) * SPICE_CAPS_LOCK_BIT)
}

fn spice_clipboard_format(format: RemoteDesktopClipboardFormat) -> Option<HelperClipboardFormat> {
    match format {
        RemoteDesktopClipboardFormat::ImagePng => Some(HelperClipboardFormat::ImagePng),
        RemoteDesktopClipboardFormat::ImageJpeg => Some(HelperClipboardFormat::ImageJpeg),
        RemoteDesktopClipboardFormat::ImageBmp => Some(HelperClipboardFormat::ImageBmp),
        RemoteDesktopClipboardFormat::ImageTiff => Some(HelperClipboardFormat::ImageTiff),
        RemoteDesktopClipboardFormat::ImageWebp
        | RemoteDesktopClipboardFormat::ImageGif
        | RemoteDesktopClipboardFormat::ImageSvg => None,
    }
}

fn preferred_clipboard_format(formats: &[HelperClipboardFormat]) -> Option<HelperClipboardFormat> {
    [
        HelperClipboardFormat::Utf8Text,
        HelperClipboardFormat::ImagePng,
        HelperClipboardFormat::ImageJpeg,
        HelperClipboardFormat::ImageBmp,
        HelperClipboardFormat::ImageTiff,
    ]
    .into_iter()
    .find(|format| formats.contains(format))
}

fn remote_clipboard_event(
    format: HelperClipboardFormat,
    data: &[u8],
) -> Option<RemoteDesktopHelperEvent> {
    if format == HelperClipboardFormat::Utf8Text {
        return std::str::from_utf8(data).ok().map(|text| {
            RemoteDesktopHelperEvent::ClipboardText {
                text: text.to_string(),
            }
        });
    }
    let format = match format {
        HelperClipboardFormat::ImagePng => RemoteDesktopClipboardFormat::ImagePng,
        HelperClipboardFormat::ImageJpeg => RemoteDesktopClipboardFormat::ImageJpeg,
        HelperClipboardFormat::ImageBmp => RemoteDesktopClipboardFormat::ImageBmp,
        HelperClipboardFormat::ImageTiff => RemoteDesktopClipboardFormat::ImageTiff,
        HelperClipboardFormat::Utf8Text | HelperClipboardFormat::FileList => return None,
    };
    Some(RemoteDesktopHelperEvent::ClipboardData {
        data: RemoteDesktopClipboardData::new(format, data.to_vec()),
    })
}

fn spice_set1_scancode(key: &RemoteDesktopKey) -> Option<u32> {
    let normalized = normalize_key_code(&key.code);
    match normalized.as_str() {
        "escape" | "esc" => Some(0x01),
        "backspace" => Some(0x0e),
        "tab" => Some(0x0f),
        "enter" | "return" => Some(0x1c),
        "space" | " " => Some(0x39),
        "shift" | "shiftleft" => Some(0x2a),
        "shiftright" => Some(0x36),
        "control" | "ctrl" | "controlleft" | "ctrlleft" => Some(0x1d),
        "controlright" | "ctrlright" => Some(0xe01d),
        "alt" | "altleft" => Some(0x38),
        "altright" | "altgraph" | "altgr" => Some(0xe038),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" => Some(0xe05b),
        "metaright" | "superright" | "winright" => Some(0xe05c),
        "capslock" | "caps_lock" => Some(0x3a),
        "numlock" | "num_lock" => Some(0x45),
        "scrolllock" | "scroll_lock" => Some(0x46),
        "printscreen" | "print" | "snapshot" => Some(0xe037),
        "contextmenu" | "context_menu" | "menu" | "apps" => Some(0xe05d),
        "delete" => Some(0xe053),
        "insert" => Some(0xe052),
        "home" => Some(0xe047),
        "end" => Some(0xe04f),
        "pageup" | "page_up" => Some(0xe049),
        "pagedown" | "page_down" => Some(0xe051),
        "arrowup" | "up" => Some(0xe048),
        "arrowdown" | "down" => Some(0xe050),
        "arrowleft" | "left" => Some(0xe04b),
        "arrowright" | "right" => Some(0xe04d),
        "numpad0" | "numpadinsert" => Some(0x52),
        "numpad1" | "numpadend" => Some(0x4f),
        "numpad2" | "numpaddown" => Some(0x50),
        "numpad3" | "numpadpagedown" => Some(0x51),
        "numpad4" | "numpadleft" => Some(0x4b),
        "numpad5" | "numpadclear" => Some(0x4c),
        "numpad6" | "numpadright" => Some(0x4d),
        "numpad7" | "numpadhome" => Some(0x47),
        "numpad8" | "numpadup" => Some(0x48),
        "numpad9" | "numpadpageup" => Some(0x49),
        "numpaddecimal" | "numpaddelete" => Some(0x53),
        "numpadadd" => Some(0x4e),
        "numpadsubtract" => Some(0x4a),
        "numpadmultiply" => Some(0x37),
        "numpaddivide" => Some(0xe035),
        "numpadenter" => Some(0xe01c),
        "f1" => Some(0x3b),
        "f2" => Some(0x3c),
        "f3" => Some(0x3d),
        "f4" => Some(0x3e),
        "f5" => Some(0x3f),
        "f6" => Some(0x40),
        "f7" => Some(0x41),
        "f8" => Some(0x42),
        "f9" => Some(0x43),
        "f10" => Some(0x44),
        "f11" => Some(0x57),
        "f12" => Some(0x58),
        _ => ascii_set1_scancode(&normalized),
    }
}

fn normalize_key_code(code: &str) -> String {
    if matches!(code, "\n" | "\r") {
        return "enter".to_string();
    }
    let normalized = code.trim().to_ascii_lowercase();
    if let Some(letter) = normalized.strip_prefix("key")
        && letter.len() == 1
        && letter.as_bytes()[0].is_ascii_lowercase()
    {
        return letter.to_string();
    }
    if let Some(digit) = normalized.strip_prefix("digit")
        && digit.len() == 1
        && digit.as_bytes()[0].is_ascii_digit()
    {
        return digit.to_string();
    }
    match normalized.as_str() {
        "enterkey" | "returnkey" | "newline" | "linefeed" | "carriagereturn" => "enter".to_string(),
        "keypadenter" | "keypad_enter" | "kpenter" | "kp_enter" | "num_enter" | "numpad_enter" => {
            "numpadenter".to_string()
        }
        "del" => "delete".to_string(),
        "pgup" => "pageup".to_string(),
        "pgdn" => "pagedown".to_string(),
        "minus" => "-".to_string(),
        "equal" => "=".to_string(),
        "bracketleft" => "[".to_string(),
        "bracketright" => "]".to_string(),
        "backslash" | "intlbackslash" => "\\".to_string(),
        "semicolon" => ";".to_string(),
        "quote" => "'".to_string(),
        "backquote" | "backtick" => "`".to_string(),
        "comma" => ",".to_string(),
        "period" => ".".to_string(),
        "slash" => "/".to_string(),
        _ => normalized,
    }
}

fn ascii_set1_scancode(code: &str) -> Option<u32> {
    Some(match code {
        "a" => 0x1e,
        "b" => 0x30,
        "c" => 0x2e,
        "d" => 0x20,
        "e" => 0x12,
        "f" => 0x21,
        "g" => 0x22,
        "h" => 0x23,
        "i" => 0x17,
        "j" => 0x24,
        "k" => 0x25,
        "l" => 0x26,
        "m" => 0x32,
        "n" => 0x31,
        "o" => 0x18,
        "p" => 0x19,
        "q" => 0x10,
        "r" => 0x13,
        "s" => 0x1f,
        "t" => 0x14,
        "u" => 0x16,
        "v" => 0x2f,
        "w" => 0x11,
        "x" => 0x2d,
        "y" => 0x15,
        "z" => 0x2c,
        "1" => 0x02,
        "2" => 0x03,
        "3" => 0x04,
        "4" => 0x05,
        "5" => 0x06,
        "6" => 0x07,
        "7" => 0x08,
        "8" => 0x09,
        "9" => 0x0a,
        "0" => 0x0b,
        "-" => 0x0c,
        "=" => 0x0d,
        "[" => 0x1a,
        "]" => 0x1b,
        "\\" => 0x2b,
        ";" => 0x27,
        "'" => 0x28,
        "`" => 0x29,
        "," => 0x33,
        "." => 0x34,
        "/" => 0x35,
        _ => return None,
    })
}
