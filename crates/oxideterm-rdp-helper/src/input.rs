use std::collections::{HashMap, HashSet};

use ironrdp::input::{
    MouseButton as RdpMouseButton, Operation as RdpInputOperation, Scancode, WheelRotations,
};
use oxideterm_remote_desktop::{
    RemoteDesktopKey, RemoteDesktopKeyState, RemoteDesktopMouseButton, RemoteDesktopWheelDelta,
};

const WHEEL_UNIT: f32 = 120.0;

pub(crate) fn rdp_mouse_button(button: RemoteDesktopMouseButton) -> Option<RdpMouseButton> {
    match button {
        RemoteDesktopMouseButton::Left => Some(RdpMouseButton::Left),
        RemoteDesktopMouseButton::Middle => Some(RdpMouseButton::Middle),
        RemoteDesktopMouseButton::Right => Some(RdpMouseButton::Right),
        RemoteDesktopMouseButton::Back => Some(RdpMouseButton::X1),
        RemoteDesktopMouseButton::Forward => Some(RdpMouseButton::X2),
    }
}

pub(crate) fn rdp_wheel_operations(delta: RemoteDesktopWheelDelta) -> Vec<RdpInputOperation> {
    let mut operations = Vec::new();
    if delta.x.abs() > f32::EPSILON {
        operations.push(RdpInputOperation::WheelRotations(WheelRotations {
            is_vertical: false,
            rotation_units: rdp_wheel_units(delta.x),
        }));
    }
    if delta.y.abs() > f32::EPSILON {
        operations.push(RdpInputOperation::WheelRotations(WheelRotations {
            is_vertical: true,
            rotation_units: rdp_wheel_units(delta.y),
        }));
    }
    operations
}

pub(crate) fn rdp_wheel_units(delta: f32) -> i16 {
    let units = if delta.abs() < WHEEL_UNIT {
        delta.signum() * WHEEL_UNIT
    } else {
        delta
    };
    units
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[derive(Default)]
pub(crate) struct RdpKeyboardInputMapper {
    physical_modifiers: HashSet<Scancode>,
    pressed_scancodes: HashSet<Scancode>,
    pressed_unicode: HashSet<char>,
    pressed_unicode_by_code: HashMap<String, char>,
    synthetic_modifiers_by_key: HashMap<Scancode, Vec<Scancode>>,
}

impl RdpKeyboardInputMapper {
    #[cfg_attr(not(any(feature = "legacy-freerdp", test)), allow(dead_code))]
    pub(crate) fn release_all_operations(&mut self) -> Vec<RdpInputOperation> {
        let mut operations = Vec::new();
        for scancode in self.pressed_scancodes.drain() {
            operations.push(RdpInputOperation::KeyReleased(scancode));
        }
        for character in self.pressed_unicode.drain() {
            operations.push(RdpInputOperation::UnicodeKeyReleased(character));
        }
        self.pressed_unicode_by_code.clear();
        let mut released_synthetic_modifiers = HashSet::new();
        for modifier in self
            .synthetic_modifiers_by_key
            .drain()
            .flat_map(|(_, modifiers)| modifiers)
        {
            if released_synthetic_modifiers.insert(modifier) {
                operations.push(RdpInputOperation::KeyReleased(modifier));
            }
        }
        self.physical_modifiers.clear();
        operations
    }

    pub(crate) fn operations(
        &mut self,
        key: &RemoteDesktopKey,
        state: RemoteDesktopKeyState,
    ) -> Vec<RdpInputOperation> {
        if let Some(scancode) = rdp_modifier_scancode_for_key(&key.code) {
            return self.modifier_operations(scancode, state);
        }

        if let Some(scancode) = rdp_scancode(&key.code) {
            return self.scancode_operations(scancode, rdp_modifier_scancodes(key), state);
        }

        let unicode_key_code = tracked_unicode_key_code(&key.code);
        if let Some(character) = printable_remote_text(key) {
            return self.unicode_operations(unicode_key_code, character, state);
        }

        if state == RemoteDesktopKeyState::Released
            && let Some(character) = self.pressed_unicode_by_code.remove(&unicode_key_code)
        {
            self.pressed_unicode.remove(&character);
            return unicode_key_operations(character, state);
        }

        key.text
            .as_deref()
            .and_then(single_non_control_char)
            .map(|character| self.unicode_operations(unicode_key_code, character, state))
            .unwrap_or_default()
    }

    fn unicode_operations(
        &mut self,
        key_code: String,
        character: char,
        state: RemoteDesktopKeyState,
    ) -> Vec<RdpInputOperation> {
        match state {
            RemoteDesktopKeyState::Pressed => {
                self.pressed_unicode.insert(character);
                self.pressed_unicode_by_code.insert(key_code, character);
            }
            RemoteDesktopKeyState::Released => {
                self.pressed_unicode.remove(&character);
                self.pressed_unicode_by_code.remove(&key_code);
            }
        }
        unicode_key_operations(character, state)
    }

    fn modifier_operations(
        &mut self,
        scancode: Scancode,
        state: RemoteDesktopKeyState,
    ) -> Vec<RdpInputOperation> {
        match state {
            RemoteDesktopKeyState::Pressed => {
                self.physical_modifiers.insert(scancode);
                self.pressed_scancodes.insert(scancode);
                vec![RdpInputOperation::KeyPressed(scancode)]
            }
            RemoteDesktopKeyState::Released => {
                self.physical_modifiers.remove(&scancode);
                self.pressed_scancodes.remove(&scancode);
                vec![RdpInputOperation::KeyReleased(scancode)]
            }
        }
    }

    fn scancode_operations(
        &mut self,
        scancode: Scancode,
        modifiers: Vec<Scancode>,
        state: RemoteDesktopKeyState,
    ) -> Vec<RdpInputOperation> {
        match state {
            RemoteDesktopKeyState::Pressed => {
                let synthetic_modifiers = modifiers
                    .into_iter()
                    .filter(|modifier| !self.physical_modifier_equivalent_pressed(*modifier))
                    .collect::<Vec<_>>();
                let mut operations = synthetic_modifiers
                    .iter()
                    .copied()
                    .map(RdpInputOperation::KeyPressed)
                    .collect::<Vec<_>>();
                operations.push(RdpInputOperation::KeyPressed(scancode));
                self.pressed_scancodes.insert(scancode);
                if !synthetic_modifiers.is_empty() {
                    // Store ownership per primary key so a physical Ctrl held
                    // by the user is not released when only the letter key is
                    // released. This mirrors IronRDP's stateful input database
                    // instead of treating every shortcut as an isolated chord.
                    self.synthetic_modifiers_by_key
                        .insert(scancode, synthetic_modifiers);
                }
                operations
            }
            RemoteDesktopKeyState::Released => {
                let mut operations = vec![RdpInputOperation::KeyReleased(scancode)];
                self.pressed_scancodes.remove(&scancode);
                if let Some(mut synthetic_modifiers) =
                    self.synthetic_modifiers_by_key.remove(&scancode)
                {
                    synthetic_modifiers.reverse();
                    operations.extend(
                        synthetic_modifiers
                            .into_iter()
                            .map(RdpInputOperation::KeyReleased),
                    );
                }
                operations
            }
        }
    }

    fn physical_modifier_equivalent_pressed(&self, modifier: Scancode) -> bool {
        self.physical_modifiers
            .iter()
            .any(|pressed| modifier_equivalent(*pressed, modifier))
    }
}

fn modifier_equivalent(left: Scancode, right: Scancode) -> bool {
    match (left.as_u16(), right.as_u16()) {
        (0x1d | 0xe01d, 0x1d | 0xe01d) => true,
        (0x2a | 0x36, 0x2a | 0x36) => true,
        (0x38 | 0xe038, 0x38 | 0xe038) => true,
        (0xe05b | 0xe05c, 0xe05b | 0xe05c) => true,
        _ => left == right,
    }
}

#[cfg(test)]
pub(crate) fn rdp_key_operations(
    key: &RemoteDesktopKey,
    state: RemoteDesktopKeyState,
) -> Vec<RdpInputOperation> {
    if let Some(scancode) = rdp_scancode(&key.code) {
        let mut operations = Vec::new();
        let is_modifier_key = rdp_modifier_scancode_for_key(&key.code).is_some();
        let mut modifiers = if is_modifier_key {
            Vec::new()
        } else {
            rdp_modifier_scancodes(key)
        };
        match state {
            RemoteDesktopKeyState::Pressed => {
                operations.extend(modifiers.iter().copied().map(RdpInputOperation::KeyPressed));
                operations.push(RdpInputOperation::KeyPressed(scancode));
            }
            RemoteDesktopKeyState::Released => {
                operations.push(RdpInputOperation::KeyReleased(scancode));
                modifiers.reverse();
                operations.extend(modifiers.into_iter().map(RdpInputOperation::KeyReleased));
            }
        }
        return operations;
    }

    if let Some(character) = printable_remote_text(key) {
        return vec![match state {
            RemoteDesktopKeyState::Pressed => RdpInputOperation::UnicodeKeyPressed(character),
            RemoteDesktopKeyState::Released => RdpInputOperation::UnicodeKeyReleased(character),
        }];
    }

    key.text
        .as_deref()
        .and_then(single_non_control_char)
        .map(|character| {
            vec![match state {
                RemoteDesktopKeyState::Pressed => RdpInputOperation::UnicodeKeyPressed(character),
                RemoteDesktopKeyState::Released => RdpInputOperation::UnicodeKeyReleased(character),
            }]
        })
        .unwrap_or_default()
}

fn unicode_key_operations(character: char, state: RemoteDesktopKeyState) -> Vec<RdpInputOperation> {
    vec![match state {
        RemoteDesktopKeyState::Pressed => RdpInputOperation::UnicodeKeyPressed(character),
        RemoteDesktopKeyState::Released => RdpInputOperation::UnicodeKeyReleased(character),
    }]
}

fn tracked_unicode_key_code(code: &str) -> String {
    // GPUI can omit key_char on key-up. Track printable Unicode presses by the
    // normalized physical-ish key code so the release still matches the press.
    normalize_rdp_key_code(code)
}

fn printable_remote_text(key: &RemoteDesktopKey) -> Option<char> {
    if key.ctrl || key.alt || key.meta {
        return None;
    }
    if rdp_key_code_prefers_physical_scancode(&key.code) {
        return None;
    }
    key.text.as_deref().and_then(single_non_control_char)
}

fn rdp_key_code_prefers_physical_scancode(code: &str) -> bool {
    let normalized = normalize_rdp_key_code(code);
    normalized.starts_with("numpad")
        || matches!(
            normalized.as_str(),
            "escape"
                | "esc"
                | "backspace"
                | "tab"
                | "enter"
                | "return"
                | "capslock"
                | "caps_lock"
                | "numlock"
                | "num_lock"
                | "scrolllock"
                | "scroll_lock"
                | "printscreen"
                | "print"
                | "snapshot"
                | "contextmenu"
                | "context_menu"
                | "menu"
                | "apps"
                | "delete"
                | "insert"
                | "home"
                | "end"
                | "pageup"
                | "page_up"
                | "pagedown"
                | "page_down"
                | "arrowup"
                | "arrowdown"
                | "arrowleft"
                | "arrowright"
        )
        || normalized
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=12).contains(&number))
}

pub(crate) fn single_non_control_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let character = chars.next()?;
    if chars.next().is_some() || character.is_control() {
        None
    } else {
        Some(character)
    }
}

pub(crate) fn rdp_scancode(code: &str) -> Option<Scancode> {
    let normalized = normalize_rdp_key_code(code);
    let scancode = match normalized.as_str() {
        "escape" | "esc" => Scancode::from_u8(false, 0x01),
        "backspace" => Scancode::from_u8(false, 0x0e),
        "tab" => Scancode::from_u8(false, 0x0f),
        "enter" | "return" => Scancode::from_u8(false, 0x1c),
        "space" | " " => Scancode::from_u8(false, 0x39),
        "shift" | "shiftleft" => Scancode::from_u8(false, 0x2a),
        "shiftright" => Scancode::from_u8(false, 0x36),
        "control" | "ctrl" | "controlleft" | "ctrlleft" => Scancode::from_u8(false, 0x1d),
        "controlright" | "ctrlright" => Scancode::from_u8(true, 0x1d),
        "alt" | "altleft" => Scancode::from_u8(false, 0x38),
        "altright" | "altgraph" | "altgr" => Scancode::from_u8(true, 0x38),
        "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" => Scancode::from_u16(0xe05b),
        "metaright" | "superright" | "winright" => Scancode::from_u16(0xe05c),
        "capslock" | "caps_lock" => Scancode::from_u8(false, 0x3a),
        "numlock" | "num_lock" => Scancode::from_u8(false, 0x45),
        "scrolllock" | "scroll_lock" => Scancode::from_u8(false, 0x46),
        "printscreen" | "print" | "snapshot" => Scancode::from_u16(0xe037),
        "contextmenu" | "context_menu" | "menu" | "apps" => Scancode::from_u16(0xe05d),
        "delete" => Scancode::from_u8(true, 0x53),
        "insert" => Scancode::from_u8(true, 0x52),
        "home" => Scancode::from_u8(true, 0x47),
        "end" => Scancode::from_u8(true, 0x4f),
        "pageup" | "page_up" => Scancode::from_u8(true, 0x49),
        "pagedown" | "page_down" => Scancode::from_u8(true, 0x51),
        "arrowup" | "up" => Scancode::from_u8(true, 0x48),
        "arrowdown" | "down" => Scancode::from_u8(true, 0x50),
        "arrowleft" | "left" => Scancode::from_u8(true, 0x4b),
        "arrowright" | "right" => Scancode::from_u8(true, 0x4d),
        "numpad0" | "numpadinsert" => Scancode::from_u8(false, 0x52),
        "numpad1" | "numpadend" => Scancode::from_u8(false, 0x4f),
        "numpad2" | "numpaddown" => Scancode::from_u8(false, 0x50),
        "numpad3" | "numpadpagedown" => Scancode::from_u8(false, 0x51),
        "numpad4" | "numpadleft" => Scancode::from_u8(false, 0x4b),
        "numpad5" | "numpadclear" => Scancode::from_u8(false, 0x4c),
        "numpad6" | "numpadright" => Scancode::from_u8(false, 0x4d),
        "numpad7" | "numpadhome" => Scancode::from_u8(false, 0x47),
        "numpad8" | "numpadup" => Scancode::from_u8(false, 0x48),
        "numpad9" | "numpadpageup" => Scancode::from_u8(false, 0x49),
        "numpaddecimal" | "numpaddelete" => Scancode::from_u8(false, 0x53),
        "numpadadd" => Scancode::from_u8(false, 0x4e),
        "numpadsubtract" => Scancode::from_u8(false, 0x4a),
        "numpadmultiply" => Scancode::from_u8(false, 0x37),
        "numpaddivide" => Scancode::from_u8(true, 0x35),
        "numpadenter" => Scancode::from_u8(true, 0x1c),
        "f1" => Scancode::from_u8(false, 0x3b),
        "f2" => Scancode::from_u8(false, 0x3c),
        "f3" => Scancode::from_u8(false, 0x3d),
        "f4" => Scancode::from_u8(false, 0x3e),
        "f5" => Scancode::from_u8(false, 0x3f),
        "f6" => Scancode::from_u8(false, 0x40),
        "f7" => Scancode::from_u8(false, 0x41),
        "f8" => Scancode::from_u8(false, 0x42),
        "f9" => Scancode::from_u8(false, 0x43),
        "f10" => Scancode::from_u8(false, 0x44),
        "f11" => Scancode::from_u8(false, 0x57),
        "f12" => Scancode::from_u8(false, 0x58),
        _ => return ascii_scancode(normalized.as_str()),
    };
    Some(scancode)
}

fn normalize_rdp_key_code(code: &str) -> String {
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

    // GPUI normally reports compact names, but platform bridges and helper
    // tests can surface browser-style physical codes. Normalize those aliases
    // before falling back to the US set-1 scan table.
    match normalized.as_str() {
        "enterkey" | "returnkey" | "newline" | "linefeed" | "carriagereturn" => "enter".to_string(),
        "keypadenter" | "keypad_enter" | "kpenter" | "kp_enter" | "num_enter" | "numpad_enter" => {
            "numpadenter".to_string()
        }
        "left" => "arrowleft".to_string(),
        "right" => "arrowright".to_string(),
        "up" => "arrowup".to_string(),
        "down" => "arrowdown".to_string(),
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

fn rdp_modifier_scancodes(key: &RemoteDesktopKey) -> Vec<Scancode> {
    let mut scancodes = Vec::with_capacity(4);
    if key.ctrl {
        scancodes.push(Scancode::from_u8(false, 0x1d));
    }
    if key.shift {
        scancodes.push(Scancode::from_u8(false, 0x2a));
    }
    if key.alt {
        scancodes.push(Scancode::from_u8(false, 0x38));
    }
    if key.meta {
        scancodes.push(Scancode::from_u16(0xe05b));
    }
    scancodes
}

fn rdp_modifier_scancode_for_key(code: &str) -> Option<Scancode> {
    let normalized = normalize_rdp_key_code(code);
    match normalized.as_str() {
        "shift" | "shiftleft" | "shiftright" | "control" | "ctrl" | "controlleft" | "ctrlleft"
        | "controlright" | "ctrlright" | "alt" | "altleft" | "altright" | "altgraph" | "altgr"
        | "command" | "cmd" | "meta" | "super" | "win" | "windows" | "metaleft" | "superleft"
        | "winleft" | "metaright" | "superright" | "winright" => rdp_scancode(&normalized),
        _ => None,
    }
}

fn ascii_scancode(code: &str) -> Option<Scancode> {
    let scan_code = match code {
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
    };
    Some(Scancode::from_u8(false, scan_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: &str, text: Option<&str>, ctrl: bool) -> RemoteDesktopKey {
        RemoteDesktopKey {
            code: code.to_string(),
            text: text.map(ToOwned::to_owned),
            alt: false,
            ctrl,
            shift: false,
            meta: false,
        }
    }

    fn scancode(operation: &RdpInputOperation) -> u16 {
        match operation {
            RdpInputOperation::KeyPressed(scancode) | RdpInputOperation::KeyReleased(scancode) => {
                scancode.as_u16()
            }
            operation => panic!("unexpected operation: {operation:?}"),
        }
    }

    #[test]
    fn keyboard_mapper_keeps_physical_modifier_pressed_until_modifier_releases() {
        let mut mapper = RdpKeyboardInputMapper::default();

        let control_down = mapper.operations(
            &key("ControlLeft", None, true),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(control_down.len(), 1);
        assert_eq!(scancode(&control_down[0]), 0x1d);

        let letter_down = mapper.operations(
            &key("KeyV", Some("v"), true),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(letter_down.len(), 1);
        assert_eq!(scancode(&letter_down[0]), 0x2f);

        let letter_up = mapper.operations(
            &key("KeyV", Some("v"), true),
            RemoteDesktopKeyState::Released,
        );
        assert_eq!(letter_up.len(), 1);
        assert_eq!(scancode(&letter_up[0]), 0x2f);

        let control_up = mapper.operations(
            &key("ControlLeft", None, false),
            RemoteDesktopKeyState::Released,
        );
        assert_eq!(control_up.len(), 1);
        assert_eq!(scancode(&control_up[0]), 0x1d);
    }

    #[test]
    fn keyboard_mapper_temporarily_synthesizes_missing_modifier() {
        let mut mapper = RdpKeyboardInputMapper::default();

        let letter_down = mapper.operations(
            &key("KeyC", Some("c"), true),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(letter_down.len(), 2);
        assert_eq!(scancode(&letter_down[0]), 0x1d);
        assert_eq!(scancode(&letter_down[1]), 0x2e);

        let letter_up = mapper.operations(
            &key("KeyC", Some("c"), true),
            RemoteDesktopKeyState::Released,
        );
        assert_eq!(letter_up.len(), 2);
        assert_eq!(scancode(&letter_up[0]), 0x2e);
        assert_eq!(scancode(&letter_up[1]), 0x1d);
    }

    #[test]
    fn keyboard_mapper_treats_left_and_right_modifiers_as_equivalent() {
        let mut mapper = RdpKeyboardInputMapper::default();

        let right_control_down = mapper.operations(
            &key("ControlRight", None, true),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(right_control_down.len(), 1);
        assert_eq!(scancode(&right_control_down[0]), 0xe01d);

        let letter_down = mapper.operations(
            &key("KeyV", Some("v"), true),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(letter_down.len(), 1);
        assert_eq!(scancode(&letter_down[0]), 0x2f);
    }

    #[test]
    fn keyboard_mapper_prefers_keypad_scancode_over_printable_text() {
        let mut mapper = RdpKeyboardInputMapper::default();

        let keypad_down = mapper.operations(
            &key("Numpad1", Some("1"), false),
            RemoteDesktopKeyState::Pressed,
        );

        assert_eq!(keypad_down.len(), 1);
        assert_eq!(scancode(&keypad_down[0]), 0x4f);
    }

    #[test]
    fn keyboard_mapper_prefers_special_key_scancode_over_text() {
        let mut mapper = RdpKeyboardInputMapper::default();

        let enter_down = mapper.operations(
            &key("Enter", Some("\n"), false),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(enter_down.len(), 1);
        assert_eq!(scancode(&enter_down[0]), 0x1c);

        let backspace_down = mapper.operations(
            &key("Backspace", Some("\u{8}"), false),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(backspace_down.len(), 1);
        assert_eq!(scancode(&backspace_down[0]), 0x0e);
    }

    #[test]
    fn keyboard_mapper_prefers_physical_scancode_for_printable_key_events() {
        let mut mapper = RdpKeyboardInputMapper::default();

        let key_down = mapper.operations(
            &key("KeyA", Some("a"), false),
            RemoteDesktopKeyState::Pressed,
        );
        assert_eq!(key_down.len(), 1);
        assert_eq!(scancode(&key_down[0]), 0x1e);

        let key_up = mapper.operations(&key("KeyA", None, false), RemoteDesktopKeyState::Released);
        assert_eq!(key_up.len(), 1);
        assert_eq!(scancode(&key_up[0]), 0x1e);
    }

    #[test]
    fn keyboard_mapper_maps_extended_desktop_keys() {
        assert_eq!(rdp_scancode("Return").unwrap().as_u16(), 0x1c);
        assert_eq!(rdp_scancode("EnterKey").unwrap().as_u16(), 0x1c);
        assert_eq!(rdp_scancode("\n").unwrap().as_u16(), 0x1c);
        assert_eq!(rdp_scancode("\r").unwrap().as_u16(), 0x1c);
        assert_eq!(rdp_scancode("NumpadDivide").unwrap().as_u16(), 0xe035);
        assert_eq!(rdp_scancode("NumpadEnter").unwrap().as_u16(), 0xe01c);
        assert_eq!(rdp_scancode("KP_Enter").unwrap().as_u16(), 0xe01c);
        assert_eq!(rdp_scancode("ContextMenu").unwrap().as_u16(), 0xe05d);
        assert_eq!(rdp_scancode("PrintScreen").unwrap().as_u16(), 0xe037);
        assert_eq!(rdp_scancode("NumLock").unwrap().as_u16(), 0x45);
    }

    #[test]
    fn keyboard_mapper_release_all_releases_tracked_inputs() {
        let mut mapper = RdpKeyboardInputMapper::default();

        let _ = mapper.operations(
            &key("ControlLeft", None, true),
            RemoteDesktopKeyState::Pressed,
        );
        let _ = mapper.operations(
            &key("KeyA", Some("a"), false),
            RemoteDesktopKeyState::Pressed,
        );

        let released = mapper.release_all_operations();
        assert!(released.iter().any(|operation| {
            matches!(operation, RdpInputOperation::KeyReleased(scancode) if scancode.as_u16() == 0x1d)
        }));
        assert!(released.iter().any(|operation| {
            matches!(operation, RdpInputOperation::KeyReleased(scancode) if scancode.as_u16() == 0x1e)
        }));
        assert!(mapper.release_all_operations().is_empty());
    }
}
