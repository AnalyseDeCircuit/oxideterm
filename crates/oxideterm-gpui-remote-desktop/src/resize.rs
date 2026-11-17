// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_remote_desktop::RemoteDesktopSize;

const REMOTE_DESKTOP_INITIAL_WIDTH: u32 = 1280;
const REMOTE_DESKTOP_INITIAL_HEIGHT: u32 = 720;
const REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD: u32 = 16;
const REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT: u32 = 500;
const REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER: f32 = 100.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteDesktopResizeRequestState {
    pub size: RemoteDesktopSize,
    pub scale_factor: Option<u32>,
}

pub fn default_remote_desktop_initial_size() -> RemoteDesktopSize {
    RemoteDesktopSize::clamped(REMOTE_DESKTOP_INITIAL_WIDTH, REMOTE_DESKTOP_INITIAL_HEIGHT)
}

pub fn remote_desktop_scale_factor_percent(scale_factor: f32) -> u32 {
    let percent = (scale_factor * REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER).round();
    if percent.is_finite() {
        let percent = percent as u32;
        if (REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT..=REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT)
            .contains(&percent)
        {
            return percent;
        }
    }
    REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT
}

pub fn remote_desktop_monitor_scale_percent(scale_factor: f32) -> u32 {
    (scale_factor * REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER)
        .round()
        .clamp(
            REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT as f32,
            REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT as f32,
        ) as u32
}

pub fn remote_desktop_requested_size_for_viewport(
    viewport_size: RemoteDesktopSize,
    scale_factor: Option<u32>,
) -> RemoteDesktopSize {
    let viewport_size = RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
    let Some(scale_factor) = scale_factor else {
        return viewport_size;
    };
    if !(REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT..=REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT)
        .contains(&scale_factor)
    {
        return viewport_size;
    }

    // GPUI canvas bounds are logical pixels; RDP desktop_size is the remote
    // framebuffer pixel size, so high-DPI windows need an explicit conversion.
    let denominator = u64::from(REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT);
    let scale_factor = u64::from(scale_factor);
    let width = remote_desktop_scaled_dimension(viewport_size.width, scale_factor, denominator);
    let height = remote_desktop_scaled_dimension(viewport_size.height, scale_factor, denominator);
    RemoteDesktopSize::clamped(width, height)
}

fn remote_desktop_scaled_dimension(value: u32, scale_factor: u64, denominator: u64) -> u32 {
    let scaled = (u64::from(value) * scale_factor + denominator / 2) / denominator;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

fn remote_desktop_resize_request_needed(
    current_frame_size: Option<RemoteDesktopSize>,
    pending_resize: Option<RemoteDesktopSize>,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    viewport_size: RemoteDesktopSize,
    request_size: RemoteDesktopSize,
    viewport_scale_factor: Option<u32>,
) -> bool {
    let next_request = RemoteDesktopResizeRequestState {
        size: request_size,
        scale_factor: viewport_scale_factor,
    };
    if Some(next_request) == last_sent_resize {
        return false;
    }

    let frame_mismatch = remote_desktop_size_delta_is_meaningful(current_frame_size, request_size)
        && Some(request_size) != current_frame_size;
    let viewport_changed = Some(viewport_size) != last_viewport_size;
    let scale_changed = viewport_scale_factor.is_some()
        && last_sent_resize
            .is_some_and(|last_sent| last_sent.scale_factor != viewport_scale_factor);
    if !viewport_changed && !frame_mismatch && !scale_changed {
        return false;
    }
    if !frame_mismatch {
        return scale_changed;
    }
    if Some(request_size) == pending_resize {
        return scale_changed && last_sent_resize.is_some();
    }
    let last_sent_size = last_sent_resize.map(|last_sent| last_sent.size);
    if !remote_desktop_size_delta_is_meaningful(last_sent_size, request_size) && !scale_changed {
        return false;
    }
    true
}

pub fn remote_desktop_resize_request_needed_for_capability(
    resize_supported: bool,
    current_frame_size: Option<RemoteDesktopSize>,
    pending_resize: Option<RemoteDesktopSize>,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    viewport_size: RemoteDesktopSize,
    request_size: RemoteDesktopSize,
    viewport_scale_factor: Option<u32>,
) -> bool {
    // VNC's built-in provider has a fixed server framebuffer; viewport changes
    // still update local geometry, but they must not create remote resize state.
    resize_supported
        && remote_desktop_resize_request_needed(
            current_frame_size,
            pending_resize,
            last_viewport_size,
            last_sent_resize,
            viewport_size,
            request_size,
            viewport_scale_factor,
        )
}

fn remote_desktop_size_delta_is_meaningful(
    previous: Option<RemoteDesktopSize>,
    next: RemoteDesktopSize,
) -> bool {
    previous.is_none_or(|previous| {
        previous.width.abs_diff(next.width) >= REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD
            || previous.height.abs_diff(next.height) >= REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_requests_preserve_retry_and_scale_boundaries() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };
        let smaller_frame = RemoteDesktopSize {
            width: 1280,
            height: 720,
        };
        let scaled_frame = RemoteDesktopSize {
            width: 3200,
            height: 1800,
        };
        let cases = [
            (
                "initial mismatch",
                smaller_frame,
                None,
                None,
                viewport,
                Some(100),
                true,
            ),
            (
                "pending retry",
                smaller_frame,
                Some(viewport),
                None,
                viewport,
                Some(100),
                false,
            ),
            (
                "ignored retry",
                smaller_frame,
                None,
                Some(resize_state(viewport, Some(100))),
                viewport,
                Some(100),
                false,
            ),
            (
                "matching frame",
                viewport,
                None,
                None,
                viewport,
                None,
                false,
            ),
            (
                "scaled initial connect",
                scaled_frame,
                None,
                None,
                scaled_frame,
                Some(200),
                false,
            ),
            (
                "changed scale",
                viewport,
                None,
                Some(resize_state(viewport, Some(100))),
                viewport,
                Some(125),
                true,
            ),
            (
                "already sent scale",
                viewport,
                None,
                Some(resize_state(viewport, Some(125))),
                viewport,
                Some(125),
                false,
            ),
            (
                "replace pending scale",
                smaller_frame,
                Some(viewport),
                Some(resize_state(viewport, Some(100))),
                viewport,
                Some(125),
                true,
            ),
        ];
        for (scenario, frame, pending, last_sent, request, scale, expected) in cases {
            assert_eq!(
                remote_desktop_resize_request_needed(
                    Some(frame),
                    pending,
                    Some(viewport),
                    last_sent,
                    viewport,
                    request,
                    scale,
                ),
                expected,
                "{scenario}",
            );
        }
    }

    #[test]
    fn resize_request_is_blocked_without_negotiated_resize_support() {
        let current = RemoteDesktopSize {
            width: 1280,
            height: 720,
        };
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed_for_capability(
            false,
            Some(current),
            None,
            Some(current),
            None,
            viewport,
            viewport,
            Some(100),
        ));
        assert!(remote_desktop_resize_request_needed_for_capability(
            true,
            Some(current),
            None,
            Some(current),
            None,
            viewport,
            viewport,
            Some(100),
        ));
    }

    fn resize_state(
        size: RemoteDesktopSize,
        scale_factor: Option<u32>,
    ) -> RemoteDesktopResizeRequestState {
        RemoteDesktopResizeRequestState { size, scale_factor }
    }
}
