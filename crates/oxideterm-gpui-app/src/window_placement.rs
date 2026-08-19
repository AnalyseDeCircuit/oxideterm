// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{App, Bounds, Pixels, WindowBounds, point, px, size};
use oxideterm_settings::{WindowGeometry, WindowUiState};
use oxideterm_theme::UiMetrics;

// Keep enough surrounding desktop visible for drag-and-drop and window switching.
const FIRST_LAUNCH_WORK_AREA_RATIO: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
struct LogicalWindowRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl LogicalWindowRect {
    fn from_bounds(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        }
    }

    fn to_bounds(self) -> Bounds<Pixels> {
        Bounds::new(
            point(px(self.x), px(self.y)),
            size(px(self.width), px(self.height)),
        )
    }

    fn intersection_area(self, other: Self) -> f32 {
        let width = (self.x + self.width).min(other.x + other.width) - self.x.max(other.x);
        let height = (self.y + self.height).min(other.y + other.height) - self.y.max(other.y);
        width.max(0.0) * height.max(0.0)
    }
}

pub(crate) fn default_window_bounds(cx: &App) -> Bounds<Pixels> {
    initial_window_bounds(cx, &WindowUiState::default()).get_bounds()
}

pub(crate) fn initial_window_bounds(cx: &App, state: &WindowUiState) -> WindowBounds {
    let metrics = UiMetrics::tauri_default();
    let visible_displays = cx
        .displays()
        .into_iter()
        .map(|display| LogicalWindowRect::from_bounds(display.visible_bounds()))
        .filter(valid_display_rect)
        .collect::<Vec<_>>();
    let fallback_display = LogicalWindowRect {
        x: 0.0,
        y: 0.0,
        width: metrics.window_min_width,
        height: metrics.window_min_height,
    };
    let primary_display = cx
        .primary_display()
        .map(|display| LogicalWindowRect::from_bounds(display.visible_bounds()))
        .filter(valid_display_rect)
        .or_else(|| visible_displays.first().copied())
        .unwrap_or(fallback_display);
    let restored =
        restore_window_rect(state.normal_bounds, &visible_displays, primary_display).to_bounds();

    if state.fullscreen {
        WindowBounds::Fullscreen(restored)
    } else if state.maximized {
        WindowBounds::Maximized(restored)
    } else {
        WindowBounds::Windowed(restored)
    }
}

fn restore_window_rect(
    saved: Option<WindowGeometry>,
    visible_displays: &[LogicalWindowRect],
    primary_display: LogicalWindowRect,
) -> LogicalWindowRect {
    let desired = saved
        .filter(|geometry| geometry.width > 0 && geometry.height > 0)
        .map(|geometry| LogicalWindowRect {
            x: geometry.x as f32,
            y: geometry.y as f32,
            width: geometry.width as f32,
            height: geometry.height as f32,
        });
    let Some(desired) = desired else {
        return centered_default_window(primary_display);
    };

    let selected_display = visible_displays
        .iter()
        .copied()
        .map(|display| (display, desired.intersection_area(display)))
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .filter(|(_, area)| *area > 0.0)
        .map(|(display, _)| display);
    match selected_display {
        Some(display) => fit_window_to_display(desired, display),
        None => center_window_on_display(desired, primary_display),
    }
}

fn valid_display_rect(display: &LogicalWindowRect) -> bool {
    display.x.is_finite()
        && display.y.is_finite()
        && display.width.is_finite()
        && display.height.is_finite()
        && display.width > 0.0
        && display.height > 0.0
}

fn centered_default_window(display: LogicalWindowRect) -> LogicalWindowRect {
    center_window_on_display(
        LogicalWindowRect {
            x: display.x,
            y: display.y,
            width: (display.width * FIRST_LAUNCH_WORK_AREA_RATIO).round(),
            height: (display.height * FIRST_LAUNCH_WORK_AREA_RATIO).round(),
        },
        display,
    )
}

fn center_window_on_display(
    window: LogicalWindowRect,
    display: LogicalWindowRect,
) -> LogicalWindowRect {
    let mut fitted = fit_window_to_display(window, display);
    fitted.x = display.x + (display.width - fitted.width) / 2.0;
    fitted.y = display.y + (display.height - fitted.height) / 2.0;
    fitted
}

fn fit_window_to_display(
    window: LogicalWindowRect,
    display: LogicalWindowRect,
) -> LogicalWindowRect {
    let metrics = UiMetrics::tauri_default();
    let minimum_width = metrics.window_min_width.min(display.width);
    let minimum_height = metrics.window_min_height.min(display.height);
    let width = window.width.clamp(minimum_width, display.width);
    let height = window.height.clamp(minimum_height, display.height);
    LogicalWindowRect {
        x: window.x.clamp(display.x, display.x + display.width - width),
        y: window
            .y
            .clamp(display.y, display.y + display.height - height),
        width,
        height,
    }
}
