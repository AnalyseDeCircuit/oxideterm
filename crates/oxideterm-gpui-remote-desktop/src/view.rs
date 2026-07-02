// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use gpui::{
    AnyElement, Bounds, Corners, CursorStyle, DevicePixels, Div, ObjectFit, ParentElement, Pixels,
    RenderImage, Styled, Window, canvas, div, fill, point, prelude::*, px, rgb, rgba, size,
};
use oxideterm_gpui_ui::{empty_state, error_state};
use oxideterm_remote_desktop::{
    RemoteDesktopCursorShape, RemoteDesktopFrameFormat, RemoteDesktopSessionStatus,
};
use oxideterm_theme::ThemeTokens;

use crate::{
    RemoteDesktopCursorState, RemoteDesktopViewState, SharedRemoteDesktopGeometry,
    state::RemoteDesktopFrameSurface,
};

const VIEW_PADDING: f32 = 14.0;
const FRAME_BORDER_ALPHA: u32 = 0x80;
const FRAME_BG_ALPHA: u32 = 0x66;

pub fn remote_desktop_surface(tokens: &ThemeTokens, state: &RemoteDesktopViewState) -> AnyElement {
    remote_desktop_surface_with_geometry(tokens, state, None)
}

pub fn remote_desktop_surface_with_geometry(
    tokens: &ThemeTokens,
    state: &RemoteDesktopViewState,
    geometry: Option<SharedRemoteDesktopGeometry>,
) -> AnyElement {
    let snapshot = state.snapshot();
    div()
        .size_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .bg(rgb(tokens.ui.bg_panel))
        .flex()
        .child(div().min_h(px(0.0)).flex_1().child(match snapshot.status {
            RemoteDesktopSessionStatus::Failed => error_body(tokens, snapshot.message),
            status if should_render_remote_frame(status, snapshot.has_frame) => {
                // Keep the last framebuffer visible while an engine performs an
                // internal resize reconnect. The footer already exposes the
                // transient status without blanking the desktop surface.
                frame_body(tokens, state, geometry)
            }
            RemoteDesktopSessionStatus::Idle
            | RemoteDesktopSessionStatus::Connecting
            | RemoteDesktopSessionStatus::Reconnecting
            | RemoteDesktopSessionStatus::Disconnected => {
                placeholder_body(tokens, snapshot.status, snapshot.message, geometry)
            }
            RemoteDesktopSessionStatus::Connected => frame_body(tokens, state, geometry),
        }))
        .into_any_element()
}

fn should_render_remote_frame(status: RemoteDesktopSessionStatus, has_frame: bool) -> bool {
    matches!(status, RemoteDesktopSessionStatus::Connected)
        || (status == RemoteDesktopSessionStatus::Reconnecting && has_frame)
}

fn frame_body(
    tokens: &ThemeTokens,
    state: &RemoteDesktopViewState,
    geometry: Option<SharedRemoteDesktopGeometry>,
) -> AnyElement {
    if state.frame_size().is_some() {
        let Some(surface) = state.frame_surface() else {
            if let Some(geometry) = geometry {
                geometry.clear();
            }
            return corrupted_frame_body(tokens, state).into_any_element();
        };
        let cursor = state.cursor().clone();
        let cursor_image = state.cursor_image();

        return div()
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .bg(rgb(0x000000))
            .overflow_hidden()
            .child(remote_desktop_frame_canvas(
                surface,
                cursor,
                cursor_image,
                geometry,
            ))
            .into_any_element();
    }

    div()
        .size_full()
        .relative()
        .child(empty_state(
            tokens,
            "RD",
            "Waiting for the first remote frame",
            Some("The helper is connected, but no desktop frame has arrived yet.".to_string()),
            None,
        ))
        .when_some(geometry, |element, geometry| {
            element.child(remote_desktop_viewport_probe(geometry))
        })
        .into_any_element()
}

fn corrupted_frame_body(tokens: &ThemeTokens, state: &RemoteDesktopViewState) -> Div {
    let details = state
        .corrupted_frame()
        .map(|frame| {
            let format_label = match frame.format {
                RemoteDesktopFrameFormat::Rgba8 => "RGBA",
                RemoteDesktopFrameFormat::Bgra8 => "BGRA",
            };
            format!(
                "{} x {}, {format_label}, {} bytes",
                frame.size.width, frame.size.height, frame.byte_len
            )
        })
        .unwrap_or_else(|| "The framebuffer cache was not available.".to_string());

    div()
        .size_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .border_1()
        .border_color(rgba((tokens.ui.error << 8) | FRAME_BORDER_ALPHA))
        .bg(rgba((tokens.ui.bg_sunken << 8) | FRAME_BG_ALPHA))
        .p(px(VIEW_PADDING))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(tokens.spacing.two))
        .text_color(rgb(tokens.ui.text_muted))
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_heading))
                .child("Remote frame is incomplete"),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .child(details),
        )
}

fn remote_desktop_frame_canvas(
    surface: RemoteDesktopFrameSurface,
    cursor: RemoteDesktopCursorState,
    cursor_image: Option<Arc<RenderImage>>,
    geometry: Option<SharedRemoteDesktopGeometry>,
) -> impl IntoElement {
    let cursor_for_paint = cursor.clone();
    let cursor_image_for_paint = cursor_image.clone();
    let width = surface.size.width;
    let height = surface.size.height;
    canvas(
        move |bounds, _window: &mut Window, _cx| {
            let image_bounds = ObjectFit::Contain.get_bounds(
                bounds,
                size(DevicePixels(width as i32), DevicePixels(height as i32)),
            );
            if let Some(geometry) = geometry.as_ref() {
                geometry.update(
                    Some(image_bounds),
                    Some(oxideterm_remote_desktop::RemoteDesktopSize { width, height }),
                    Some(oxideterm_remote_desktop::RemoteDesktopSize::clamped(
                        f32::from(bounds.size.width).round() as u32,
                        f32::from(bounds.size.height).round() as u32,
                    )),
                );
            }
            image_bounds
        },
        move |bounds, image_bounds, window: &mut Window, _cx| {
            window.paint_quad(fill(bounds, rgb(0x000000)));
            paint_remote_desktop_surface(window, image_bounds, &surface);
            if cursor_for_paint.visible
                && let (Some(shape), Some(cursor_image)) = (
                    cursor_for_paint.shape.as_ref(),
                    cursor_image_for_paint.as_ref(),
                )
                && let Some(cursor_bounds) =
                    cursor_bounds(image_bounds, width, height, &cursor_for_paint, shape)
            {
                let cursor_image: Arc<RenderImage> = Arc::clone(cursor_image);
                let _ = window.paint_image(
                    cursor_bounds,
                    Corners::all(px(0.0)),
                    cursor_image,
                    0,
                    false,
                );
            }
        },
    )
    .when(
        should_hide_system_cursor(&cursor, cursor_image.is_some()),
        |element| element.cursor(CursorStyle::None),
    )
    .size_full()
}

fn paint_remote_desktop_surface(
    window: &mut Window,
    image_bounds: Bounds<Pixels>,
    surface: &RemoteDesktopFrameSurface,
) {
    if let Ok(mut pending_updates) = surface.pending_texture_updates.lock() {
        for update in pending_updates.drain(..) {
            let update_bounds = Bounds::new(
                point(
                    DevicePixels(update.rect.x as i32),
                    DevicePixels(update.rect.y as i32),
                ),
                size(
                    DevicePixels(update.rect.width as i32),
                    DevicePixels(update.rect.height as i32),
                ),
            );
            let _ = window.update_dynamic_texture(&surface.texture, update_bounds, &update.bytes);
        }
    }
    let texture = Arc::clone(&surface.texture);
    let _ = window.paint_dynamic_texture(image_bounds, Corners::all(px(0.0)), texture, false);
}

fn should_hide_system_cursor(
    cursor: &RemoteDesktopCursorState,
    cursor_image_available: bool,
) -> bool {
    !cursor.visible || cursor_image_available
}

fn remote_desktop_viewport_probe(geometry: SharedRemoteDesktopGeometry) -> impl IntoElement {
    canvas(
        move |bounds, _window: &mut Window, _cx| {
            // The placeholder has no remote framebuffer yet, but the app can
            // still use this measured viewport to request the initial desktop
            // size before starting the helper.
            geometry.update(
                None,
                None,
                Some(oxideterm_remote_desktop::RemoteDesktopSize::clamped(
                    f32::from(bounds.size.width).round() as u32,
                    f32::from(bounds.size.height).round() as u32,
                )),
            );
            bounds
        },
        |_bounds, _state, _window: &mut Window, _cx| {},
    )
    .absolute()
    .inset_0()
}

fn cursor_bounds(
    image_bounds: Bounds<Pixels>,
    frame_width: u32,
    frame_height: u32,
    cursor: &RemoteDesktopCursorState,
    shape: &RemoteDesktopCursorShape,
) -> Option<Bounds<Pixels>> {
    if frame_width == 0 || frame_height == 0 {
        return None;
    }
    let scale_x = f32::from(image_bounds.size.width) / frame_width as f32;
    let scale_y = f32::from(image_bounds.size.height) / frame_height as f32;
    let left = (cursor.x as f32 - shape.hotspot_x as f32) * scale_x;
    let top = (cursor.y as f32 - shape.hotspot_y as f32) * scale_y;
    Some(Bounds::new(
        point(
            image_bounds.origin.x + px(left),
            image_bounds.origin.y + px(top),
        ),
        size(
            px(shape.size.width as f32 * scale_x),
            px(shape.size.height as f32 * scale_y),
        ),
    ))
}

fn placeholder_body(
    tokens: &ThemeTokens,
    status: RemoteDesktopSessionStatus,
    message: Option<String>,
    geometry: Option<SharedRemoteDesktopGeometry>,
) -> AnyElement {
    let title = match status {
        RemoteDesktopSessionStatus::Idle => "Remote desktop is idle",
        RemoteDesktopSessionStatus::Connecting => "Opening remote desktop",
        RemoteDesktopSessionStatus::Reconnecting => "Reconnecting remote desktop",
        RemoteDesktopSessionStatus::Disconnected => "Remote desktop disconnected",
        RemoteDesktopSessionStatus::Connected | RemoteDesktopSessionStatus::Failed => {
            "Remote desktop"
        }
    };

    div()
        .size_full()
        .relative()
        .child(empty_state(tokens, "RD", title, message, None))
        .when_some(geometry, |element, geometry| {
            element.child(remote_desktop_viewport_probe(geometry))
        })
        .into_any_element()
}

fn error_body(tokens: &ThemeTokens, message: Option<String>) -> AnyElement {
    error_state(
        tokens,
        "!",
        "Remote desktop failed",
        message.or_else(|| Some("The helper reported a connection failure.".to_string())),
        None,
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnecting_session_keeps_last_frame_visible() {
        assert!(should_render_remote_frame(
            RemoteDesktopSessionStatus::Reconnecting,
            true
        ));
        assert!(!should_render_remote_frame(
            RemoteDesktopSessionStatus::Reconnecting,
            false
        ));
    }

    #[test]
    fn system_cursor_hides_for_remote_hidden_or_custom_cursor() {
        let mut cursor = RemoteDesktopCursorState::default();

        assert!(!should_hide_system_cursor(&cursor, false));
        assert!(should_hide_system_cursor(&cursor, true));

        cursor.visible = false;
        assert!(should_hide_system_cursor(&cursor, false));
    }
}
