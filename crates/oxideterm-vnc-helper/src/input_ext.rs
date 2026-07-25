// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

const VNC_QEMU_CLIENT_MESSAGE: u8 = 255;
const VNC_QEMU_EXTENDED_KEY_SUBTYPE: u8 = 0;
const VNC_POINTER_MESSAGE: u8 = 5;
const VNC_EXTENDED_POINTER_MARKER: u8 = 0x80;
const VNC_POINTER_BASE_BUTTON_MASK: u16 = 0x007f;
const VNC_POINTER_EXTENDED_BUTTON_MASK: u16 = 0x7f80;
const VNC_LOCK_KEYS_OBSERVED: u8 = 0x80;
const VNC_LOCK_SCROLL: u8 = 1 << 0;
const VNC_LOCK_NUM: u8 = 1 << 1;
const VNC_LOCK_CAPS: u8 = 1 << 2;
const VNC_LOCK_KEYS_PENDING: u8 = 0x80;
pub(super) const VNC_CAPABILITY_UNKNOWN: u8 = 0;
const VNC_CAPABILITY_SUPPORTED: u8 = 1;
const VNC_CAPABILITY_UNSUPPORTED: u8 = 2;

/// Reads a protocol capability without treating silence as rejection.
fn load_vnc_capability_status(state: &AtomicU8) -> NegotiatedCapabilityStatus {
    match state.load(Ordering::Acquire) {
        VNC_CAPABILITY_SUPPORTED => NegotiatedCapabilityStatus::Supported,
        VNC_CAPABILITY_UNSUPPORTED => NegotiatedCapabilityStatus::Unsupported,
        _ => NegotiatedCapabilityStatus::Unknown,
    }
}

/// Maps normalized physical key names to XT Set 1 make codes in RFB form.
pub(super) fn vnc_raw_keycode_for_code(code: &str) -> Option<u32> {
    let normalized = normalize_vnc_key_code(code);
    let normalized = normalized.as_str();

    if let Some(letter) = normalized
        .as_bytes()
        .first()
        .copied()
        .filter(|_| normalized.len() == 1)
        .filter(u8::is_ascii_lowercase)
    {
        const LETTER_SCANCODES: [u8; 26] = [
            0x1e, 0x30, 0x2e, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31,
            0x18, 0x19, 0x10, 0x13, 0x1f, 0x14, 0x16, 0x2f, 0x11, 0x2d, 0x15, 0x2c,
        ];
        return Some(u32::from(LETTER_SCANCODES[usize::from(letter - b'a')]));
    }

    let xt_scancode = match normalized {
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
        "escape" | "esc" => 0x01,
        "backspace" => 0x0e,
        "tab" => 0x0f,
        "enter" | "return" => 0x1c,
        "control" | "ctrl" | "controlleft" | "ctrlleft" => 0x1d,
        "shift" | "shiftleft" => 0x2a,
        "shiftright" => 0x36,
        "alt" | "altleft" => 0x38,
        "space" => 0x39,
        "capslock" | "caps_lock" => 0x3a,
        "f1" => 0x3b,
        "f2" => 0x3c,
        "f3" => 0x3d,
        "f4" => 0x3e,
        "f5" => 0x3f,
        "f6" => 0x40,
        "f7" => 0x41,
        "f8" => 0x42,
        "f9" => 0x43,
        "f10" => 0x44,
        "numlock" | "num_lock" => 0x45,
        "scrolllock" | "scroll_lock" => 0x46,
        "numpad7" | "numpadhome" => 0x47,
        "numpad8" | "numpadup" => 0x48,
        "numpad9" | "numpadpageup" => 0x49,
        "numpadsubtract" => 0x4a,
        "numpad4" | "numpadleft" => 0x4b,
        "numpad5" | "numpadclear" => 0x4c,
        "numpad6" | "numpadright" => 0x4d,
        "numpadadd" => 0x4e,
        "numpad1" | "numpadend" => 0x4f,
        "numpad2" | "numpaddown" => 0x50,
        "numpad3" | "numpadpagedown" => 0x51,
        "numpad0" | "numpadinsert" => 0x52,
        "numpaddecimal" | "numpaddelete" => 0x53,
        "printscreen" | "print" | "snapshot" => 0x54,
        "intlbackslash" => 0x56,
        "f11" => 0x57,
        "f12" => 0x58,
        "numpadequal" => 0x59,
        "minus" => 0x0c,
        "equal" => 0x0d,
        "bracketleft" => 0x1a,
        "bracketright" => 0x1b,
        "semicolon" => 0x27,
        "quote" => 0x28,
        "backquote" => 0x29,
        "backslash" => 0x2b,
        "comma" => 0x33,
        "period" => 0x34,
        "slash" => 0x35,
        "numpadmultiply" => 0x37,
        "controlright" | "ctrlright" => 0xe01d,
        "altright" | "altgraph" | "altgr" => 0xe038,
        "numpadenter" => 0xe01c,
        "numpaddivide" => 0xe035,
        "home" => 0xe047,
        "arrowup" | "up" => 0xe048,
        "pageup" => 0xe049,
        "arrowleft" | "left" => 0xe04b,
        "arrowright" | "right" => 0xe04d,
        "end" => 0xe04f,
        "arrowdown" | "down" => 0xe050,
        "pagedown" => 0xe051,
        "insert" => 0xe052,
        "delete" => 0xe053,
        "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" => 0xe05b,
        "metaright" | "superright" | "winright" => 0xe05c,
        "contextmenu" | "context_menu" | "menu" | "apps" => 0xe05d,
        "pause" | "break" => 0xe046,
        _ => return None,
    };
    Some(vnc_rfb_keycode_from_xt(xt_scancode))
}

/// Collapses an E0-prefixed XT make code into the QEMU RFB keycode field.
pub(super) fn vnc_rfb_keycode_from_xt(xt_scancode: u32) -> u32 {
    let prefix = xt_scancode >> 8;
    let code = xt_scancode & 0xff;
    if prefix == 0xe0 && code < 0x7f {
        code | 0x80
    } else {
        xt_scancode
    }
}

pub(super) fn vnc_raw_keycode_for_keysym(keysym: u32) -> Option<u32> {
    match keysym {
        0xffe1 => vnc_raw_keycode_for_code("ShiftLeft"),
        0xffe2 => vnc_raw_keycode_for_code("ShiftRight"),
        0xffe3 => vnc_raw_keycode_for_code("ControlLeft"),
        0xffe4 => vnc_raw_keycode_for_code("ControlRight"),
        0xffe9 => vnc_raw_keycode_for_code("AltLeft"),
        0xffea => vnc_raw_keycode_for_code("AltRight"),
        0xffeb => vnc_raw_keycode_for_code("MetaLeft"),
        0xffec => vnc_raw_keycode_for_code("MetaRight"),
        0xffe5 => vnc_raw_keycode_for_code("CapsLock"),
        0xff7f => vnc_raw_keycode_for_code("NumLock"),
        0xff14 => vnc_raw_keycode_for_code("ScrollLock"),
        _ => None,
    }
}

/// Builds the optional QEMU message only when the physical code is known.
pub(super) fn qemu_extended_key_event_message(event: VncKeyEvent) -> Option<Vec<u8>> {
    let raw_keycode = event.raw_keycode?;
    let mut message = Vec::with_capacity(12);
    message.push(VNC_QEMU_CLIENT_MESSAGE);
    message.push(VNC_QEMU_EXTENDED_KEY_SUBTYPE);
    push_be_u16(&mut message, u16::from(event.down));
    push_be_u32(&mut message, event.keysym);
    push_be_u32(&mut message, raw_keycode);
    Some(message)
}

pub(super) fn vnc_standard_key_event_message(keysym: u32, down: bool) -> Vec<u8> {
    let mut message = Vec::with_capacity(8);
    message.push(4);
    message.push(u8::from(down));
    message.extend_from_slice(&[0, 0]);
    push_be_u32(&mut message, keysym);
    message
}

/// Encodes the base or negotiated extended PointerEvent wire shape.
pub(super) fn vnc_pointer_event_message(
    x: u16,
    y: u16,
    buttons: u16,
    extended_mouse_buttons: bool,
) -> Vec<u8> {
    let has_extended_buttons = buttons & VNC_POINTER_EXTENDED_BUTTON_MASK != 0;
    let mut message = Vec::with_capacity(if extended_mouse_buttons && has_extended_buttons {
        7
    } else {
        6
    });
    message.push(VNC_POINTER_MESSAGE);

    if extended_mouse_buttons && has_extended_buttons {
        let base_buttons =
            ((buttons & VNC_POINTER_BASE_BUTTON_MASK) as u8) | VNC_EXTENDED_POINTER_MARKER;
        message.push(base_buttons);
        push_be_u16(&mut message, x);
        push_be_u16(&mut message, y);
        message.push(((buttons & VNC_POINTER_EXTENDED_BUTTON_MASK) >> 7) as u8);
    } else {
        // Until the server confirms the extension, bit 7 must remain clear.
        message.push((buttons & VNC_POINTER_BASE_BUTTON_MASK) as u8);
        push_be_u16(&mut message, x);
        push_be_u16(&mut message, y);
    }
    message
}

/// Emits lock-key taps only for server-observed states that differ.
pub(super) fn vnc_lock_key_sync_events(
    current: RemoteDesktopLockKeys,
    target: RemoteDesktopLockKeys,
) -> Vec<VncKeyEvent> {
    let mut events = Vec::with_capacity(6);
    for (changed, keysym) in [
        (current.scroll_lock != target.scroll_lock, 0xff14),
        (current.num_lock != target.num_lock, 0xff7f),
        (current.caps_lock != target.caps_lock, 0xffe5),
    ] {
        if changed {
            let raw_keycode = vnc_raw_keycode_for_keysym(keysym);
            events.push(VncKeyEvent {
                keysym,
                raw_keycode,
                down: true,
            });
            events.push(VncKeyEvent {
                keysym,
                raw_keycode,
                down: false,
            });
        }
    }
    events
}

pub(super) fn vnc_lock_keys_from_bits(bits: u8) -> RemoteDesktopLockKeys {
    RemoteDesktopLockKeys {
        scroll_lock: bits & VNC_LOCK_SCROLL != 0,
        num_lock: bits & VNC_LOCK_NUM != 0,
        caps_lock: bits & VNC_LOCK_CAPS != 0,
        // Neither LED extension defines a Kana Lock bit.
        kana_lock: false,
    }
}

fn vnc_lock_key_bits(keys: RemoteDesktopLockKeys) -> u8 {
    u8::from(keys.scroll_lock) * VNC_LOCK_SCROLL
        | u8::from(keys.num_lock) * VNC_LOCK_NUM
        | u8::from(keys.caps_lock) * VNC_LOCK_CAPS
}

impl VncSessionSharedState {
    pub(super) fn observe_input_extensions(&self, event: &VncServerEvent) {
        match event {
            VncServerEvent::QemuExtendedKeyEvents => {
                self.qemu_extended_key_events
                    .store(VNC_CAPABILITY_SUPPORTED, Ordering::Release);
            }
            VncServerEvent::ExtendedMouseButtons => {
                self.extended_mouse_buttons
                    .store(VNC_CAPABILITY_SUPPORTED, Ordering::Release);
            }
            VncServerEvent::LockKeys(keys) => self.store_remote_lock_keys(*keys),
            VncServerEvent::Batch(events) => {
                for event in events {
                    self.observe_input_extensions(event);
                }
            }
            _ => {}
        }
    }

    pub(super) fn qemu_extended_key_event_support(&self) -> NegotiatedCapabilityStatus {
        load_vnc_capability_status(&self.qemu_extended_key_events)
    }

    pub(super) fn extended_mouse_button_support(&self) -> NegotiatedCapabilityStatus {
        load_vnc_capability_status(&self.extended_mouse_buttons)
    }

    pub(super) fn input_extension_capabilities(&self) -> NegotiatedCapabilities {
        let mut capabilities = NegotiatedCapabilities::default();
        self.merge_input_extension_capabilities(&mut capabilities);
        capabilities
    }

    /// Merges only the input fields into the session-wide capability snapshot.
    pub(super) fn merge_input_extension_capabilities(
        &self,
        capabilities: &mut NegotiatedCapabilities,
    ) {
        if self.qemu_extended_key_event_support() == NegotiatedCapabilityStatus::Supported {
            capabilities.extended_key_events = NegotiatedCapabilityStatus::Supported;
        }
        if self.extended_mouse_button_support() == NegotiatedCapabilityStatus::Supported {
            capabilities.extended_mouse_buttons = NegotiatedCapabilityStatus::Supported;
        }
        if self.remote_lock_keys().is_some() {
            capabilities.lock_key_sync = NegotiatedCapabilityStatus::Supported;
        }
    }

    pub(super) fn remote_lock_keys(&self) -> Option<RemoteDesktopLockKeys> {
        let state = self.remote_lock_keys.load(Ordering::Acquire);
        (state & VNC_LOCK_KEYS_OBSERVED != 0).then(|| vnc_lock_keys_from_bits(state))
    }

    pub(super) fn store_remote_lock_keys(&self, keys: RemoteDesktopLockKeys) {
        self.remote_lock_keys.store(
            VNC_LOCK_KEYS_OBSERVED | vnc_lock_key_bits(keys),
            Ordering::Release,
        );
    }

    pub(super) fn store_pending_lock_keys(&self, keys: RemoteDesktopLockKeys) {
        self.pending_lock_keys.store(
            VNC_LOCK_KEYS_PENDING | vnc_lock_key_bits(keys),
            Ordering::Release,
        );
    }

    pub(super) fn take_pending_lock_key_sync(
        &self,
    ) -> Option<(RemoteDesktopLockKeys, RemoteDesktopLockKeys)> {
        let current = self.remote_lock_keys()?;
        let pending = self.pending_lock_keys.swap(0, Ordering::AcqRel);
        (pending & VNC_LOCK_KEYS_PENDING != 0).then(|| (current, vnc_lock_keys_from_bits(pending)))
    }
}
