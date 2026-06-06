// Terminal input encoding follows public VT/xterm/Kitty control-sequence
// protocols. Keep this module protocol-driven so key and mouse tables stay
// auditable and do not grow into vendor-specific copied blocks.

use std::borrow::Cow;

use gpui::{MouseButton, ScrollDelta, ScrollWheelEvent, px};
use oxideterm_terminal::TermMode;

use crate::terminal_ui::TerminalBlinkMode;
use crate::terminal_view::selection::TerminalPoint;
use oxideterm_terminal::TerminalCursorShape;

pub(crate) enum MouseFormat {
    Sgr,
    Normal { utf8: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalScrollAction {
    PageUp,
    PageDown,
    LineUp,
    LineDown,
    Top,
    Bottom,
}

pub(crate) fn oxideterm_terminal_scroll_action(
    keystroke: &gpui::Keystroke,
) -> Option<TerminalScrollAction> {
    let modifiers = keystroke.modifiers;
    let shift_only = modifiers.shift && !modifiers.control && !modifiers.alt && !modifiers.platform;
    let platform_only =
        modifiers.platform && !modifiers.shift && !modifiers.control && !modifiers.alt;

    match keystroke.key.as_str() {
        "pageup" if shift_only => Some(TerminalScrollAction::PageUp),
        "pagedown" if shift_only => Some(TerminalScrollAction::PageDown),
        "up" if shift_only => Some(TerminalScrollAction::LineUp),
        "down" if shift_only => Some(TerminalScrollAction::LineDown),
        "home" if shift_only => Some(TerminalScrollAction::Top),
        "end" if shift_only => Some(TerminalScrollAction::Bottom),
        "up" if platform_only => Some(TerminalScrollAction::PageUp),
        "down" if platform_only => Some(TerminalScrollAction::PageDown),
        _ => None,
    }
}

pub(crate) fn terminal_scroll_delta(scroll_lines: i32) -> i32 {
    scroll_lines
}

pub(crate) fn should_blink_cursor_for_mode(
    mode: TerminalBlinkMode,
    focused: bool,
    terminal_enabled: bool,
    alt_screen: bool,
    cursor_shape: TerminalCursorShape,
) -> bool {
    focused
        && !alt_screen
        && cursor_shape != TerminalCursorShape::Hidden
        && match mode {
            TerminalBlinkMode::Off => false,
            TerminalBlinkMode::TerminalControlled => terminal_enabled,
            TerminalBlinkMode::On => true,
        }
}

impl MouseFormat {
    fn from_mode(mode: TermMode) -> Self {
        if mode.contains(TermMode::SGR_MOUSE) {
            MouseFormat::Sgr
        } else {
            MouseFormat::Normal {
                utf8: mode.contains(TermMode::UTF8_MOUSE),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalMouseButtonCode(u8);

impl TerminalMouseButtonCode {
    const LEFT: Self = Self(0);
    const MIDDLE: Self = Self(1);
    const RIGHT: Self = Self(2);
    const LEFT_MOVE: Self = Self(32);
    const MIDDLE_MOVE: Self = Self(33);
    const RIGHT_MOVE: Self = Self(34);
    const POINTER_MOVE: Self = Self(35);
    const SCROLL_UP: Self = Self(64);
    const SCROLL_DOWN: Self = Self(65);
}

impl TerminalMouseButtonCode {
    fn from_move_button(button: Option<MouseButton>) -> Option<Self> {
        match button {
            Some(MouseButton::Left) => Some(Self::LEFT_MOVE),
            Some(MouseButton::Middle) => Some(Self::MIDDLE_MOVE),
            Some(MouseButton::Right) => Some(Self::RIGHT_MOVE),
            Some(MouseButton::Navigate(_)) => None,
            None => Some(Self::POINTER_MOVE),
        }
    }

    fn from_button(button: MouseButton) -> Option<Self> {
        match button {
            MouseButton::Left => Some(Self::LEFT),
            MouseButton::Middle => Some(Self::MIDDLE),
            MouseButton::Right => Some(Self::RIGHT),
            MouseButton::Navigate(_) => None,
        }
    }

    fn from_scroll(event: &ScrollWheelEvent) -> Self {
        let is_positive = match event.delta {
            ScrollDelta::Pixels(pixels) => pixels.y > px(0.0),
            ScrollDelta::Lines(lines) => lines.y > 0.0,
        };

        if is_positive {
            Self::SCROLL_UP
        } else {
            Self::SCROLL_DOWN
        }
    }
}

pub(crate) fn mouse_mode(mode: TermMode, shift: bool) -> bool {
    mode.intersects(TermMode::MOUSE_MODE) && !shift
}

pub(crate) fn mouse_button_report(
    point: TerminalPoint,
    button: MouseButton,
    modifiers: gpui::Modifiers,
    pressed: bool,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if let Some(button) = TerminalMouseButtonCode::from_button(button)
        && mode.intersects(TermMode::MOUSE_MODE)
    {
        return mouse_report(
            point,
            button,
            pressed,
            modifiers,
            MouseFormat::from_mode(mode),
        );
    }
    None
}

pub(crate) fn mouse_moved_report(
    point: TerminalPoint,
    button: Option<MouseButton>,
    modifiers: gpui::Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let button = TerminalMouseButtonCode::from_move_button(button)?;

    if mode.intersects(TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG) {
        if mode.contains(TermMode::MOUSE_DRAG) && button == TerminalMouseButtonCode::POINTER_MOVE {
            None
        } else {
            mouse_report(point, button, true, modifiers, MouseFormat::from_mode(mode))
        }
    } else {
        None
    }
}

pub(crate) fn mouse_scroll_report(
    point: TerminalPoint,
    event: &ScrollWheelEvent,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if mode.intersects(TermMode::MOUSE_MODE) {
        mouse_report(
            point,
            TerminalMouseButtonCode::from_scroll(event),
            true,
            event.modifiers,
            MouseFormat::from_mode(mode),
        )
    } else {
        None
    }
}

pub(crate) fn mouse_report(
    point: TerminalPoint,
    button: TerminalMouseButtonCode,
    pressed: bool,
    modifiers: gpui::Modifiers,
    format: MouseFormat,
) -> Option<Vec<u8>> {
    let button_code = button.0 + mouse_modifier_bits(modifiers);

    match format {
        MouseFormat::Sgr => Some(
            format!(
                "\x1b[<{};{};{}{}",
                button_code,
                point.col + 1,
                point.row + 1,
                if pressed { 'M' } else { 'm' },
            )
            .into_bytes(),
        ),
        MouseFormat::Normal { utf8 } => {
            let release_code = 3 + mouse_modifier_bits(modifiers);
            normal_mouse_report(
                point,
                if pressed { button_code } else { release_code },
                utf8,
            )
        }
    }
}

fn mouse_modifier_bits(modifiers: gpui::Modifiers) -> u8 {
    let mut bits = 0;
    bits += if modifiers.shift { 4 } else { 0 };
    bits += if modifiers.alt { 8 } else { 0 };
    bits += if modifiers.control { 16 } else { 0 };
    bits
}

pub(crate) fn normal_mouse_report(point: TerminalPoint, button: u8, utf8: bool) -> Option<Vec<u8>> {
    const REPORT_PREFIX: &[u8] = b"\x1b[M";
    const REPORT_OFFSET: u8 = 32;

    let coordinate_limit = if utf8 { 2015 } else { 223 };
    if point.row >= coordinate_limit || point.col >= coordinate_limit {
        return None;
    }

    let mut msg = Vec::with_capacity(8);
    msg.extend_from_slice(REPORT_PREFIX);
    msg.push(REPORT_OFFSET + button);
    append_mouse_position(&mut msg, point.col, utf8);
    append_mouse_position(&mut msg, point.row, utf8);
    Some(msg)
}

pub(crate) fn append_mouse_position(msg: &mut Vec<u8>, position: usize, utf8: bool) {
    let encoded = 32 + 1 + position;
    if utf8 && position >= 95 {
        msg.push((0xc0 + encoded / 64) as u8);
        msg.push((0x80 + (encoded & 63)) as u8);
    } else {
        msg.push(encoded as u8);
    }
}

pub(crate) fn alt_scroll(scroll_lines: i32) -> Vec<u8> {
    let suffix = if scroll_lines > 0 { b'A' } else { b'B' };
    std::iter::repeat_n([0x1b, b'O', suffix], scroll_lines.unsigned_abs() as usize)
        .flatten()
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalKeyModifiers {
    shift: bool,
    alt: bool,
    control: bool,
    platform: bool,
}

impl TerminalKeyModifiers {
    fn from_keystroke(keystroke: &gpui::Keystroke) -> Self {
        Self {
            shift: keystroke.modifiers.shift,
            alt: keystroke.modifiers.alt,
            control: keystroke.modifiers.control,
            platform: keystroke.modifiers.platform,
        }
    }

    fn is_none(self) -> bool {
        !self.shift && !self.alt && !self.control && !self.platform
    }

    fn is_shift_only(self) -> bool {
        self.shift && !self.alt && !self.control && !self.platform
    }

    fn is_alt_only(self) -> bool {
        self.alt && !self.shift && !self.control && !self.platform
    }

    fn is_ctrl_only(self) -> bool {
        self.control && !self.shift && !self.alt && !self.platform
    }

    fn is_ctrl_shift_only(self) -> bool {
        self.control && self.shift && !self.alt && !self.platform
    }

    fn has_any(self) -> bool {
        self.shift || self.alt || self.control || self.platform
    }
}

pub(crate) fn oxideterm_key_escape_sequence(
    keystroke: &gpui::Keystroke,
    mode: &TermMode,
    option_as_meta: bool,
    event_type: KittyKeyEventType,
) -> Option<Cow<'static, str>> {
    let modifiers = TerminalKeyModifiers::from_keystroke(keystroke);

    if let Some(sequence) = kitty_keyboard_escape_sequence(keystroke, mode, event_type) {
        return Some(sequence);
    }

    if event_type == KittyKeyEventType::Release {
        return None;
    }

    if let Some(sequence) = basic_key_sequence(keystroke.key.as_str(), modifiers) {
        return Some(sequence);
    }

    if let Some(sequence) = navigation_key_sequence(keystroke.key.as_str(), mode, modifiers) {
        return Some(sequence);
    }

    if let Some(sequence) = editing_key_sequence(keystroke.key.as_str(), modifiers) {
        return Some(sequence);
    }

    if let Some(sequence) = function_key_sequence(keystroke.key.as_str(), modifiers) {
        return Some(sequence);
    }

    if let Some(sequence) = control_key_sequence(keystroke.key.as_str(), modifiers) {
        return Some(sequence);
    }

    if modifiers.has_any()
        && let Some(sequence) = modified_key_sequence(keystroke.key.as_str(), modifiers)
    {
        return Some(sequence);
    }

    if (!cfg!(target_os = "macos") || option_as_meta)
        && is_meta_printable_ascii(keystroke, modifiers)
    {
        let key = if modifiers.shift {
            keystroke.key.to_ascii_uppercase()
        } else {
            keystroke.key.to_string()
        };
        return Some(Cow::Owned(format!("\x1b{}", key)));
    }

    None
}

fn basic_key_sequence(key: &str, modifiers: TerminalKeyModifiers) -> Option<Cow<'static, str>> {
    match key {
        "tab" if modifiers.is_none() => Some(Cow::Borrowed("\x09")),
        "tab" if modifiers.is_shift_only() => Some(Cow::Borrowed("\x1b[Z")),
        "escape" if modifiers.is_none() => Some(Cow::Borrowed("\x1b")),
        "enter" if modifiers.is_none() => Some(Cow::Borrowed("\x0d")),
        "enter" if modifiers.is_shift_only() => Some(Cow::Borrowed("\x0a")),
        "enter" if modifiers.is_alt_only() => Some(Cow::Borrowed("\x1b\x0d")),
        "backspace" | "back" if modifiers.is_none() => Some(Cow::Borrowed("\x7f")),
        "backspace" if modifiers.is_ctrl_only() => Some(Cow::Borrowed("\x08")),
        "backspace" if modifiers.is_alt_only() => Some(Cow::Borrowed("\x1b\x7f")),
        "backspace" if modifiers.is_shift_only() => Some(Cow::Borrowed("\x7f")),
        "space" if modifiers.is_ctrl_only() => Some(Cow::Borrowed("\x00")),
        _ => None,
    }
}

fn navigation_key_sequence(
    key: &str,
    mode: &TermMode,
    modifiers: TerminalKeyModifiers,
) -> Option<Cow<'static, str>> {
    if !modifiers.is_none() {
        return None;
    }

    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let suffix = match key {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    }?;

    Some(Cow::Owned(if app_cursor {
        format!("\x1bO{}", suffix)
    } else {
        format!("\x1b[{}", suffix)
    }))
}

fn editing_key_sequence(key: &str, modifiers: TerminalKeyModifiers) -> Option<Cow<'static, str>> {
    if !modifiers.is_none() {
        return None;
    }

    csi_tilde_number(key).map(|number| Cow::Owned(format!("\x1b[{}~", number)))
}

fn function_key_sequence(key: &str, modifiers: TerminalKeyModifiers) -> Option<Cow<'static, str>> {
    if !modifiers.is_none() {
        return None;
    }

    if let Some(suffix) = ss3_function_suffix(key) {
        return Some(Cow::Owned(format!("\x1bO{}", suffix)));
    }

    function_key_csi_number(key).map(|number| Cow::Owned(format!("\x1b[{}~", number)))
}

fn control_key_sequence(key: &str, modifiers: TerminalKeyModifiers) -> Option<Cow<'static, str>> {
    if !(modifiers.is_ctrl_only() || modifiers.is_ctrl_shift_only()) {
        return None;
    }

    let ch = key.chars().next()?;
    if key.chars().nth(1).is_some() {
        return None;
    }

    if ch.is_ascii_alphabetic() {
        let byte = (ch.to_ascii_lowercase() as u8) & 0x1f;
        return Some(Cow::Owned(String::from_utf8(vec![byte]).ok()?));
    }

    let byte = match ch {
        '@' | ' ' => 0x00,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '^' => 0x1e,
        '_' => 0x1f,
        '?' => 0x7f,
        _ => return None,
    };
    Some(Cow::Owned(String::from_utf8(vec![byte]).ok()?))
}

fn modified_key_sequence(key: &str, modifiers: TerminalKeyModifiers) -> Option<Cow<'static, str>> {
    let modifier = xterm_modifier_parameter(modifiers);
    if let Some(suffix) = navigation_suffix(key).or_else(|| ss3_function_suffix(key)) {
        return Some(Cow::Owned(format!("\x1b[1;{}{}", modifier, suffix)));
    }

    csi_tilde_number(key)
        .or_else(|| function_key_csi_number(key))
        .map(|number| Cow::Owned(format!("\x1b[{};{}~", number, modifier)))
}

fn navigation_suffix(key: &str) -> Option<char> {
    match key {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    }
}

fn ss3_function_suffix(key: &str) -> Option<char> {
    match key {
        "f1" => Some('P'),
        "f2" => Some('Q'),
        "f3" => Some('R'),
        "f4" => Some('S'),
        _ => None,
    }
}

fn csi_tilde_number(key: &str) -> Option<u32> {
    match key {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        _ => None,
    }
}

fn function_key_csi_number(key: &str) -> Option<u32> {
    let n = key.strip_prefix('f')?.parse::<u32>().ok()?;
    let number = match n {
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        13 => 25,
        14 => 26,
        15 => 28,
        16 => 29,
        17 => 31,
        18 => 32,
        19 => 33,
        20 => 34,
        _ => return None,
    };
    Some(number)
}

fn is_meta_printable_ascii(keystroke: &gpui::Keystroke, modifiers: TerminalKeyModifiers) -> bool {
    keystroke.key.is_ascii()
        && (modifiers.is_alt_only()
            || (modifiers.alt && modifiers.shift && !modifiers.control && !modifiers.platform))
}

fn xterm_modifier_parameter(modifiers: TerminalKeyModifiers) -> u32 {
    let mut bits = 0;
    bits += if modifiers.shift { 1 } else { 0 };
    bits += if modifiers.alt { 2 } else { 0 };
    bits += if modifiers.control { 4 } else { 0 };
    bits + 1
}

pub(crate) fn modifier_code(keystroke: &gpui::Keystroke) -> u32 {
    xterm_modifier_parameter(TerminalKeyModifiers::from_keystroke(keystroke))
}

pub(crate) fn kitty_keyboard_escape_sequence(
    keystroke: &gpui::Keystroke,
    mode: &TermMode,
    event_type: KittyKeyEventType,
) -> Option<Cow<'static, str>> {
    if !mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) {
        return None;
    }

    let has_reportable_modifier =
        keystroke.modifiers.shift || keystroke.modifiers.alt || keystroke.modifiers.control;
    let reports_event = mode.contains(TermMode::REPORT_EVENT_TYPES);
    if event_type != KittyKeyEventType::Press && !reports_event {
        return None;
    }
    if event_type != KittyKeyEventType::Press
        && keystroke.key_char.is_some()
        && !mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC)
    {
        return None;
    }

    if !has_reportable_modifier
        && !mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC)
        && event_type == KittyKeyEventType::Press
    {
        return None;
    }

    if let Some(sequence) = kitty_keyboard_legacy_functional_sequence(keystroke, mode, event_type) {
        return Some(sequence);
    }

    let codepoint = kitty_keyboard_codepoint(keystroke)?;
    let alternate = mode
        .contains(TermMode::REPORT_ALTERNATE_KEYS)
        .then(|| kitty_keyboard_alternate_codepoint(keystroke, codepoint))
        .flatten();
    let associated_text = mode
        .contains(TermMode::REPORT_ASSOCIATED_TEXT)
        .then(|| kitty_keyboard_associated_text(keystroke))
        .flatten();

    let mut key_field = codepoint.to_string();
    if let Some(alternate) = alternate {
        key_field.push(':');
        key_field.push_str(&alternate.to_string());
    }
    let mut modifier_field = modifier_code(keystroke).to_string();
    if reports_event {
        modifier_field.push(':');
        modifier_field.push_str(&(event_type as u32).to_string());
    }

    if let Some(associated_text) = associated_text {
        return Some(Cow::Owned(format!(
            "\x1b[{};{};{}u",
            key_field, modifier_field, associated_text
        )));
    }

    Some(Cow::Owned(format!(
        "\x1b[{};{}u",
        key_field, modifier_field
    )))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KittyKeyEventType {
    Press = 1,
    Repeat = 2,
    Release = 3,
}

pub(crate) fn kitty_keyboard_codepoint(keystroke: &gpui::Keystroke) -> Option<u32> {
    if let Some(text) = keystroke.key_char.as_deref()
        && text.chars().count() == 1
    {
        return text.chars().next().map(|ch| ch as u32);
    }

    match keystroke.key.as_str() {
        "enter" => Some(13),
        "tab" => Some(9),
        "backspace" | "back" => Some(127),
        "escape" => Some(27),
        "space" => Some(32),
        key if key.starts_with('f') => key
            .strip_prefix('f')?
            .parse::<u32>()
            .ok()
            .and_then(|n| (13..=35).contains(&n).then_some(57376 + n - 13)),
        _ => None,
    }
}

pub(crate) fn kitty_keyboard_legacy_functional_sequence(
    keystroke: &gpui::Keystroke,
    mode: &TermMode,
    event_type: KittyKeyEventType,
) -> Option<Cow<'static, str>> {
    let suffix = match keystroke.key.as_str() {
        "up" => Some("A"),
        "down" => Some("B"),
        "right" => Some("C"),
        "left" => Some("D"),
        "end" => Some("F"),
        "home" => Some("H"),
        "f1" => Some("P"),
        "f2" => Some("Q"),
        "f3" => Some("R"),
        "f4" => Some("S"),
        _ => None,
    };
    if let Some(suffix) = suffix {
        let modifier = kitty_modifier_field(keystroke, mode, event_type);
        return Some(Cow::Owned(format!("\x1b[1;{}{}", modifier, suffix)));
    }

    let number = match keystroke.key.as_str() {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        _ => None,
    }?;
    let modifier = kitty_modifier_field(keystroke, mode, event_type);
    Some(Cow::Owned(format!("\x1b[{};{}~", number, modifier)))
}

pub(crate) fn kitty_modifier_field(
    keystroke: &gpui::Keystroke,
    mode: &TermMode,
    event_type: KittyKeyEventType,
) -> String {
    let mut modifier = modifier_code(keystroke).to_string();
    if mode.contains(TermMode::REPORT_EVENT_TYPES) {
        modifier.push(':');
        modifier.push_str(&(event_type as u32).to_string());
    }
    modifier
}

pub(crate) fn kitty_keyboard_alternate_codepoint(
    keystroke: &gpui::Keystroke,
    codepoint: u32,
) -> Option<u32> {
    let text = keystroke.key_char.as_deref()?;
    if text.chars().count() != 1 {
        return None;
    }

    let key = keystroke.key.chars().next()?;
    let typed = text.chars().next()?;
    let key_codepoint = key as u32;
    (key_codepoint != codepoint && typed as u32 != key_codepoint).then_some(key_codepoint)
}

pub(crate) fn kitty_keyboard_associated_text(keystroke: &gpui::Keystroke) -> Option<String> {
    let text = keystroke.key_char.as_deref()?;
    let mut codepoints = Vec::new();
    for ch in text.chars() {
        let codepoint = ch as u32;
        if codepoint < 0x20 || (0x80..=0x9f).contains(&codepoint) {
            return None;
        }
        codepoints.push(codepoint.to_string());
    }
    (!codepoints.is_empty()).then(|| codepoints.join(":"))
}
