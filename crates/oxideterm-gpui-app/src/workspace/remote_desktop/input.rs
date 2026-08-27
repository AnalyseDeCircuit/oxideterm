// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn remote_desktop_status_color(
    tokens: &ThemeTokens,
    status: RemoteDesktopSessionStatus,
) -> u32 {
    // The footer uses a color-only status marker so the remote desktop title can
    // stay in the tab chrome without adding another always-visible label.
    match status {
        RemoteDesktopSessionStatus::Connected => tokens.ui.success,
        RemoteDesktopSessionStatus::Failed => tokens.ui.error,
        RemoteDesktopSessionStatus::Connecting | RemoteDesktopSessionStatus::Reconnecting => {
            tokens.ui.warning
        }
        RemoteDesktopSessionStatus::Idle | RemoteDesktopSessionStatus::Disconnected => {
            tokens.ui.text_muted
        }
    }
}

pub(super) fn remote_desktop_reconnect_enabled(status: RemoteDesktopSessionStatus) -> bool {
    !matches!(
        status,
        RemoteDesktopSessionStatus::Connecting | RemoteDesktopSessionStatus::Reconnecting
    )
}

pub(super) fn remote_desktop_mouse_button_from_gpui(
    button: gpui::MouseButton,
) -> Option<RemoteDesktopMouseButton> {
    match button {
        gpui::MouseButton::Left => Some(RemoteDesktopMouseButton::Left),
        gpui::MouseButton::Middle => Some(RemoteDesktopMouseButton::Middle),
        gpui::MouseButton::Right => Some(RemoteDesktopMouseButton::Right),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Back) => {
            Some(RemoteDesktopMouseButton::Back)
        }
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward) => {
            Some(RemoteDesktopMouseButton::Forward)
        }
    }
}

pub(super) fn remote_desktop_empty_wheel_delta() -> RemoteDesktopWheelDelta {
    RemoteDesktopWheelDelta { x: 0.0, y: 0.0 }
}

pub(super) fn remote_desktop_wheel_delta_from_scroll(
    delta: &gpui::ScrollDelta,
    pixel_remainder: &mut RemoteDesktopWheelDelta,
) -> Option<RemoteDesktopWheelDelta> {
    match delta {
        gpui::ScrollDelta::Pixels(point) => remote_desktop_pixel_wheel_delta(
            pixel_remainder,
            f32::from(point.x),
            f32::from(point.y),
        ),
        gpui::ScrollDelta::Lines(point) => {
            *pixel_remainder = remote_desktop_empty_wheel_delta();
            remote_desktop_nonzero_wheel_delta(RemoteDesktopWheelDelta {
                x: point.x * REMOTE_DESKTOP_SCROLL_LINE,
                y: point.y * REMOTE_DESKTOP_SCROLL_LINE,
            })
        }
    }
}

pub(super) fn remote_desktop_pixel_wheel_delta(
    pixel_remainder: &mut RemoteDesktopWheelDelta,
    x: f32,
    y: f32,
) -> Option<RemoteDesktopWheelDelta> {
    let x = remote_desktop_pixel_wheel_axis(&mut pixel_remainder.x, x);
    let y = remote_desktop_pixel_wheel_axis(&mut pixel_remainder.y, y);
    remote_desktop_nonzero_wheel_delta(RemoteDesktopWheelDelta { x, y })
}

pub(super) fn remote_desktop_pixel_wheel_axis(remainder: &mut f32, delta: f32) -> f32 {
    if delta.abs() < f32::EPSILON {
        return 0.0;
    }
    if remainder.signum() != delta.signum() {
        // A new gesture in the opposite direction should not pay off stale
        // sub-notch pixels from the previous direction.
        *remainder = 0.0;
    }
    *remainder += delta;
    let steps = (*remainder / REMOTE_DESKTOP_SCROLL_PIXEL_STEP).trunc();
    if steps.abs() < 1.0 {
        return 0.0;
    }
    let consumed = steps * REMOTE_DESKTOP_SCROLL_PIXEL_STEP;
    *remainder -= consumed;
    consumed
}

pub(super) fn remote_desktop_nonzero_wheel_delta(
    delta: RemoteDesktopWheelDelta,
) -> Option<RemoteDesktopWheelDelta> {
    if delta.x.abs() < f32::EPSILON && delta.y.abs() < f32::EPSILON {
        None
    } else {
        Some(delta)
    }
}

pub(super) fn remote_desktop_diagnostics_enabled() -> bool {
    std::env::var_os(REMOTE_DESKTOP_DIAGNOSTICS_ENV).is_some()
}

pub(super) fn duration_micros_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

pub(super) fn next_remote_desktop_worker_generation(current: u64) -> u64 {
    current.saturating_add(1).max(1)
}

pub(super) fn remote_desktop_paste_shortcut(keystroke: &gpui::Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    remote_desktop_key_matches(keystroke, "v")
        && !modifiers.alt
        && (modifiers.platform || modifiers.control)
}

pub(super) fn remote_desktop_copy_shortcut(keystroke: &gpui::Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    remote_desktop_key_matches(keystroke, "c")
        && !modifiers.alt
        && (modifiers.platform || modifiers.control)
}

pub(super) fn remote_desktop_key_matches(keystroke: &gpui::Keystroke, key: &str) -> bool {
    let event_key = keystroke.key.as_str();
    event_key.eq_ignore_ascii_case(key)
        || (event_key.len() == key.len() + "Key".len()
            && event_key
                .get(.."Key".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Key"))
            && event_key
                .get("Key".len()..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(key)))
}

pub(super) fn remote_desktop_shortcut_modifier_release_codes(
    keystroke: &gpui::Keystroke,
) -> Vec<&'static str> {
    let mut codes = Vec::new();
    let modifiers = keystroke.modifiers;
    if modifiers.control {
        codes.push("control");
    }
    if modifiers.platform {
        codes.push("meta");
    }
    if modifiers.shift {
        codes.push("shift");
    }
    codes
}

pub(super) fn remote_desktop_modifier_sync_requests(
    previous: RemoteDesktopModifierState,
    next: RemoteDesktopModifierState,
) -> Vec<RemoteDesktopHelperRequest> {
    let mut requests = Vec::new();
    push_remote_desktop_modifier_sync(&mut requests, "ShiftLeft", previous.shift, next.shift);
    push_remote_desktop_modifier_sync(&mut requests, "ControlLeft", previous.ctrl, next.ctrl);
    push_remote_desktop_modifier_sync(&mut requests, "AltLeft", previous.alt, next.alt);
    push_remote_desktop_modifier_sync(&mut requests, "MetaLeft", previous.meta, next.meta);
    requests
}

pub(super) fn push_remote_desktop_modifier_sync(
    requests: &mut Vec<RemoteDesktopHelperRequest>,
    code: &'static str,
    previous: bool,
    next: bool,
) {
    if previous == next {
        return;
    }
    let state = if next {
        RemoteDesktopKeyState::Pressed
    } else {
        RemoteDesktopKeyState::Released
    };
    requests.push(RemoteDesktopHelperRequest::Key {
        key: RemoteDesktopKey {
            code: code.to_string(),
            text: None,
            alt: false,
            ctrl: false,
            shift: false,
            meta: false,
        },
        state,
    });
}

pub(super) fn remote_desktop_lock_keys_with_capslock(
    previous: Option<RemoteDesktopLockKeys>,
    capslock: gpui::Capslock,
) -> RemoteDesktopLockKeys {
    // GPUI exposes CapsLock as a real platform snapshot. Preserve estimated
    // lock keys that GPUI does not expose, but let the platform own CapsLock.
    let mut keys = previous.unwrap_or_default();
    keys.caps_lock = capslock.on;
    keys
}

pub(super) fn remote_desktop_lock_keys_after_pressed_code(
    previous: Option<RemoteDesktopLockKeys>,
    code: &str,
) -> Option<RemoteDesktopLockKeys> {
    let mut keys = previous.unwrap_or_default();
    match normalize_remote_desktop_key_code(code).as_str() {
        "numlock" => keys.num_lock = !keys.num_lock,
        "scrolllock" => keys.scroll_lock = !keys.scroll_lock,
        "kana" | "kanamode" | "kanalock" => keys.kana_lock = !keys.kana_lock,
        _ => return None,
    }
    Some(keys)
}

pub(super) fn normalize_remote_desktop_key_code(code: &str) -> String {
    code.chars()
        .filter(|character| *character != '_' && *character != '-' && !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn remote_desktop_lock_key_sync_request(
    previous: Option<RemoteDesktopLockKeys>,
    next: RemoteDesktopLockKeys,
) -> Option<RemoteDesktopHelperRequest> {
    if previous == Some(next) {
        None
    } else {
        Some(RemoteDesktopHelperRequest::SynchronizeLockKeys { keys: next })
    }
}
