// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn vnc_button_mask(button: RemoteDesktopMouseButton) -> u16 {
    match button {
        RemoteDesktopMouseButton::Left => VNC_BUTTON_LEFT,
        RemoteDesktopMouseButton::Middle => VNC_BUTTON_MIDDLE,
        RemoteDesktopMouseButton::Right => VNC_BUTTON_RIGHT,
        RemoteDesktopMouseButton::Back => VNC_BUTTON_BACK,
        RemoteDesktopMouseButton::Forward => VNC_BUTTON_FORWARD,
    }
}

pub(super) fn vnc_scroll_masks(delta: RemoteDesktopWheelDelta) -> Vec<u8> {
    let mut masks = vnc_scroll_axis_masks(delta.y, VNC_WHEEL_UP, VNC_WHEEL_DOWN);
    masks.extend(vnc_scroll_axis_masks(
        delta.x,
        VNC_WHEEL_LEFT,
        VNC_WHEEL_RIGHT,
    ));
    masks
}

pub(super) fn vnc_scroll_axis_masks(delta: f32, negative_mask: u8, positive_mask: u8) -> Vec<u8> {
    if delta.abs() < f32::EPSILON {
        return Vec::new();
    }
    let steps = (delta.abs() / VNC_SCROLL_STEP).ceil().clamp(1.0, 6.0) as usize;
    let mask = if delta > 0.0 {
        positive_mask
    } else {
        negative_mask
    };
    vec![mask; steps]
}

pub(super) fn vnc_keysym(key: &RemoteDesktopKey) -> Option<u32> {
    if vnc_key_code_prefers_physical_keysym(&key.code) {
        return vnc_keysym_for_normalized_code(&normalize_vnc_key_code(&key.code));
    }

    if let Some(text) = key.text.as_deref()
        && let Some(character) = text.chars().next()
        && !character.is_control()
    {
        return Some(vnc_unicode_keysym(character));
    }

    let normalized = normalize_vnc_key_code(&key.code);
    if let Some(character) = single_ascii_vnc_keysym(&normalized) {
        return Some(character);
    }

    vnc_keysym_for_normalized_code(&normalized)
}

pub(super) fn vnc_unicode_keysym(character: char) -> u32 {
    let codepoint = character as u32;
    if codepoint <= 0xff {
        codepoint
    } else {
        // X11 encodes non-Latin-1 Unicode characters in the 0x01xxxxxx range.
        0x0100_0000 | codepoint
    }
}

pub(super) fn vnc_text_key_events(text: &str) -> Vec<VncKeyEvent> {
    text.chars()
        .filter(|character| !character.is_control())
        .flat_map(|character| {
            let keysym = vnc_unicode_keysym(character);
            [
                VncKeyEvent {
                    keysym,
                    raw_keycode: None,
                    down: true,
                },
                VncKeyEvent {
                    keysym,
                    raw_keycode: None,
                    down: false,
                },
            ]
        })
        .collect()
}

pub(super) fn vnc_key_code_prefers_physical_keysym(code: &str) -> bool {
    let normalized = normalize_vnc_key_code(code);
    normalized.starts_with("numpad")
}

pub(super) fn vnc_keysym_for_normalized_code(normalized: &str) -> Option<u32> {
    match normalized {
        "shift" | "shiftleft" => Some(0xffe1),
        "shiftright" => Some(0xffe2),
        "control" | "ctrl" | "controlleft" | "ctrlleft" => Some(0xffe3),
        "controlright" | "ctrlright" => Some(0xffe4),
        "alt" | "altleft" => Some(0xffe9),
        "altright" | "altgraph" | "altgr" => Some(0xffea),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" => Some(0xffeb),
        "metaright" | "superright" | "winright" => Some(0xffec),
        "space" => Some(0x20),
        "enter" | "return" => Some(0xff0d),
        "numpadenter" => Some(0xff8d),
        "tab" => Some(0xff09),
        "escape" | "esc" => Some(0xff1b),
        "backspace" => Some(0xff08),
        "delete" => Some(0xffff),
        "insert" => Some(0xff63),
        "arrowleft" | "left" => Some(0xff51),
        "arrowup" | "up" => Some(0xff52),
        "arrowright" | "right" => Some(0xff53),
        "arrowdown" | "down" => Some(0xff54),
        "pageup" => Some(0xff55),
        "pagedown" => Some(0xff56),
        "home" => Some(0xff50),
        "end" => Some(0xff57),
        "capslock" | "caps_lock" => Some(0xffe5),
        "numlock" | "num_lock" => Some(0xff7f),
        "scrolllock" | "scroll_lock" => Some(0xff14),
        "pause" | "break" => Some(0xff13),
        "printscreen" | "print" | "snapshot" => Some(0xff61),
        "contextmenu" | "context_menu" | "menu" | "apps" => Some(0xff67),
        "numpad0" | "numpadinsert" => Some(0xffb0),
        "numpad1" | "numpadend" => Some(0xffb1),
        "numpad2" | "numpaddown" => Some(0xffb2),
        "numpad3" | "numpadpagedown" => Some(0xffb3),
        "numpad4" | "numpadleft" => Some(0xffb4),
        "numpad5" | "numpadclear" => Some(0xffb5),
        "numpad6" | "numpadright" => Some(0xffb6),
        "numpad7" | "numpadhome" => Some(0xffb7),
        "numpad8" | "numpadup" => Some(0xffb8),
        "numpad9" | "numpadpageup" => Some(0xffb9),
        "numpaddecimal" | "numpaddelete" => Some(0xffae),
        "numpadadd" => Some(0xffab),
        "numpadsubtract" => Some(0xffad),
        "numpadmultiply" => Some(0xffaa),
        "numpaddivide" => Some(0xffaf),
        "numpadequal" => Some(0xffbd),
        "f1" => Some(0xffbe),
        "f2" => Some(0xffbf),
        "f3" => Some(0xffc0),
        "f4" => Some(0xffc1),
        "f5" => Some(0xffc2),
        "f6" => Some(0xffc3),
        "f7" => Some(0xffc4),
        "f8" => Some(0xffc5),
        "f9" => Some(0xffc6),
        "f10" => Some(0xffc7),
        "f11" => Some(0xffc8),
        "f12" => Some(0xffc9),
        _ => None,
    }
}

pub(super) fn normalize_vnc_key_code(code: &str) -> String {
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
    // GPUI normally sends compact names, but platform bridges can use browser-
    // style or toolkit-style names. Normalize them before keysym lookup.
    match normalized.as_str() {
        "enterkey" | "returnkey" | "newline" | "linefeed" | "carriagereturn" => "enter".to_string(),
        "keypadenter" | "keypad_enter" | "kpenter" | "kp_enter" | "num_enter" | "numpad_enter" => {
            "numpadenter".to_string()
        }
        _ => normalized,
    }
}

pub(super) fn single_ascii_vnc_keysym(code: &str) -> Option<u32> {
    let mut chars = code.chars();
    let character = chars.next()?;
    if chars.next().is_none() && character.is_ascii_graphic() {
        Some(character as u32)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct VncKeyEvent {
    pub(super) keysym: u32,
    pub(super) raw_keycode: Option<u32>,
    pub(super) down: bool,
}

#[derive(Default)]
pub(super) struct VncKeyboardInputMapper {
    physical_modifiers: HashSet<u32>,
    pressed_keys: HashSet<VncKeyIdentity>,
    synthetic_modifiers_by_key: HashMap<VncKeyIdentity, Vec<VncKeyIdentity>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct VncKeyIdentity {
    keysym: u32,
    raw_keycode: Option<u32>,
}

impl VncKeyIdentity {
    fn event(self, down: bool) -> VncKeyEvent {
        VncKeyEvent {
            keysym: self.keysym,
            raw_keycode: self.raw_keycode,
            down,
        }
    }
}

impl From<VncKeyEvent> for VncKeyIdentity {
    fn from(event: VncKeyEvent) -> Self {
        Self {
            keysym: event.keysym,
            raw_keycode: event.raw_keycode,
        }
    }
}

impl VncKeyboardInputMapper {
    pub(super) fn operations(
        &mut self,
        key: &RemoteDesktopKey,
        state: RemoteDesktopKeyState,
    ) -> Vec<VncKeyEvent> {
        let Some(key_event) = vnc_key_event(key, state == RemoteDesktopKeyState::Pressed) else {
            return Vec::new();
        };
        if vnc_modifier_keysym_for_code(&key.code).is_some() {
            return self.modifier_events(key_event, state);
        }
        let key_identity = VncKeyIdentity::from(key_event);
        match state {
            RemoteDesktopKeyState::Pressed => {
                let synthetic_modifiers = vnc_modifier_keysyms(key)
                    .into_iter()
                    .filter(|modifier| !self.physical_modifier_equivalent_pressed(*modifier))
                    .map(vnc_key_identity_for_keysym)
                    .collect::<Vec<_>>();
                let mut events = synthetic_modifiers
                    .iter()
                    .copied()
                    .map(|modifier| modifier.event(true))
                    .collect::<Vec<_>>();
                events.push(key_event);
                self.pressed_keys.insert(key_identity);
                if !synthetic_modifiers.is_empty() {
                    // VNC does not have a client-side input database like
                    // IronRDP's primary path, so keep ownership for synthesized
                    // modifier presses locally and release only those later.
                    self.synthetic_modifiers_by_key
                        .insert(key_identity, synthetic_modifiers);
                }
                events
            }
            RemoteDesktopKeyState::Released => {
                let mut events = vec![key_event];
                self.pressed_keys.remove(&key_identity);
                if let Some(mut modifiers) = self.synthetic_modifiers_by_key.remove(&key_identity) {
                    modifiers.reverse();
                    events.extend(modifiers.into_iter().map(|modifier| modifier.event(false)));
                }
                events
            }
        }
    }

    pub(super) fn release_all_events(&mut self) -> Vec<VncKeyEvent> {
        let mut events = self
            .pressed_keys
            .drain()
            .map(|key| key.event(false))
            .collect::<Vec<_>>();
        let mut released_synthetic_modifiers = HashSet::new();
        for modifier in self
            .synthetic_modifiers_by_key
            .drain()
            .flat_map(|(_, modifiers)| modifiers)
        {
            if released_synthetic_modifiers.insert(modifier) {
                events.push(modifier.event(false));
            }
        }
        self.physical_modifiers.clear();
        events
    }

    pub(super) fn modifier_events(
        &mut self,
        key_event: VncKeyEvent,
        state: RemoteDesktopKeyState,
    ) -> Vec<VncKeyEvent> {
        let key_identity = VncKeyIdentity::from(key_event);
        match state {
            RemoteDesktopKeyState::Pressed => {
                self.physical_modifiers.insert(key_event.keysym);
                self.pressed_keys.insert(key_identity);
                vec![key_event]
            }
            RemoteDesktopKeyState::Released => {
                self.physical_modifiers.remove(&key_event.keysym);
                self.pressed_keys.remove(&key_identity);
                vec![key_event]
            }
        }
    }

    pub(super) fn physical_modifier_equivalent_pressed(&self, modifier: u32) -> bool {
        self.physical_modifiers
            .iter()
            .any(|pressed| vnc_modifier_equivalent(*pressed, modifier))
    }
}

pub(super) fn vnc_key_event(key: &RemoteDesktopKey, down: bool) -> Option<VncKeyEvent> {
    let raw_keycode = vnc_raw_keycode_for_code(&key.code);
    let keysym = vnc_keysym(key).or_else(|| raw_keycode.map(|_| 0))?;
    Some(VncKeyEvent {
        keysym,
        raw_keycode,
        down,
    })
}

fn vnc_key_identity_for_keysym(keysym: u32) -> VncKeyIdentity {
    VncKeyIdentity {
        keysym,
        raw_keycode: vnc_raw_keycode_for_keysym(keysym),
    }
}

pub(super) fn vnc_modifier_equivalent(left: u32, right: u32) -> bool {
    match (left, right) {
        (0xffe1 | 0xffe2, 0xffe1 | 0xffe2) => true,
        (0xffe3 | 0xffe4, 0xffe3 | 0xffe4) => true,
        (0xffe9 | 0xffea, 0xffe9 | 0xffea) => true,
        (0xffeb | 0xffec, 0xffeb | 0xffec) => true,
        _ => left == right,
    }
}

#[cfg(test)]
pub(super) fn vnc_key_events(
    key: &RemoteDesktopKey,
    state: RemoteDesktopKeyState,
) -> Vec<VncKeyEvent> {
    let Some(key_event) = vnc_key_event(key, state == RemoteDesktopKeyState::Pressed) else {
        return Vec::new();
    };
    let modifiers = vnc_modifier_keysyms(key);
    match state {
        RemoteDesktopKeyState::Pressed => modifiers
            .iter()
            .copied()
            .map(|keysym| vnc_key_identity_for_keysym(keysym).event(true))
            .chain([key_event])
            .collect(),
        RemoteDesktopKeyState::Released => [key_event]
            .into_iter()
            .chain(
                modifiers
                    .into_iter()
                    .rev()
                    .map(|keysym| vnc_key_identity_for_keysym(keysym).event(false)),
            )
            .collect(),
    }
}

pub(super) fn vnc_modifier_keysyms(key: &RemoteDesktopKey) -> Vec<u32> {
    let current = vnc_modifier_keysym_for_code(&key.code);
    let mut modifiers = Vec::with_capacity(4);
    if key.ctrl && current != Some(0xffe3) {
        modifiers.push(0xffe3);
    }
    if key.shift && current != Some(0xffe1) {
        modifiers.push(0xffe1);
    }
    if key.alt && current != Some(0xffe9) {
        modifiers.push(0xffe9);
    }
    if key.meta && current != Some(0xffeb) {
        modifiers.push(0xffeb);
    }
    modifiers
}

pub(super) fn vnc_modifier_keysym_for_code(code: &str) -> Option<u32> {
    match normalize_vnc_key_code(code).as_str() {
        "shift" | "shiftleft" => Some(0xffe1),
        "shiftright" => Some(0xffe2),
        "control" | "ctrl" | "controlleft" | "ctrlleft" => Some(0xffe3),
        "controlright" | "ctrlright" => Some(0xffe4),
        "alt" | "altleft" => Some(0xffe9),
        "altright" | "altgraph" | "altgr" => Some(0xffea),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" => Some(0xffeb),
        "metaright" | "superright" | "winright" => Some(0xffec),
        _ => None,
    }
}
