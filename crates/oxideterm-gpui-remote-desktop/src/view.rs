// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use gpui::{
    AnyElement, Corners, DevicePixels, Div, FontWeight, ObjectFit, ParentElement, RenderImage,
    Styled, Window, canvas, div, fill, prelude::*, px, rgb, rgba, size,
};
use image::{Frame as ImageFrame, RgbaImage};
use oxideterm_gpui_ui::{
    SurfaceKind, SurfaceOptions, SurfacePadding, empty_state, error_state, semantic_surface,
};
use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopProtocol, RemoteDesktopSessionStatus,
};
use oxideterm_theme::ThemeTokens;

use crate::RemoteDesktopViewState;

const VIEW_PADDING: f32 = 14.0;
const FRAME_BORDER_ALPHA: u32 = 0x80;
const FRAME_BG_ALPHA: u32 = 0x66;

pub fn remote_desktop_surface(tokens: &ThemeTokens, state: &RemoteDesktopViewState) -> AnyElement {
    let snapshot = state.snapshot();
    semantic_surface(
        tokens,
        SurfaceOptions::new(SurfaceKind::Panel).padding(SurfacePadding::Normal),
    )
    .size_full()
    .min_w(px(0.0))
    .min_h(px(0.0))
    .flex()
    .flex_col()
    .gap(px(tokens.spacing.three))
    .child(header(
        tokens,
        &snapshot.title,
        snapshot.protocol,
        snapshot.status,
        snapshot.read_only,
        snapshot.pending_resize.is_some(),
    ))
    .child(div().min_h(px(0.0)).flex_1().child(match snapshot.status {
        RemoteDesktopSessionStatus::Failed => error_body(tokens, snapshot.message),
        RemoteDesktopSessionStatus::Idle
        | RemoteDesktopSessionStatus::Connecting
        | RemoteDesktopSessionStatus::Reconnecting
        | RemoteDesktopSessionStatus::Disconnected => {
            placeholder_body(tokens, snapshot.status, snapshot.message)
        }
        RemoteDesktopSessionStatus::Connected => frame_body(tokens, state),
    }))
    .into_any_element()
}

fn header(
    tokens: &ThemeTokens,
    title: &str,
    protocol: RemoteDesktopProtocol,
    status: RemoteDesktopSessionStatus,
    read_only: bool,
    pending_resize: bool,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(tokens.spacing.three))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.two))
                .child(protocol_badge(tokens, protocol))
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(tokens.ui.text_heading))
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(tokens.spacing.one))
                .when(read_only, |row| row.child(aux_badge(tokens, "Read only")))
                .when(pending_resize, |row| {
                    row.child(aux_badge(tokens, "Resizing"))
                })
                .child(status_badge(tokens, status)),
        )
}

fn protocol_badge(tokens: &ThemeTokens, protocol: RemoteDesktopProtocol) -> Div {
    let label = match protocol {
        RemoteDesktopProtocol::Rdp => "RDP",
        RemoteDesktopProtocol::Vnc => "VNC",
    };

    div()
        .h(px(22.0))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.sm))
        .bg(rgba((tokens.ui.accent << 8) | 0x1f))
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(tokens.ui.accent))
        .child(label)
}

fn status_badge(tokens: &ThemeTokens, status: RemoteDesktopSessionStatus) -> Div {
    let (label, color) = match status {
        RemoteDesktopSessionStatus::Idle => ("Idle", tokens.ui.text_muted),
        RemoteDesktopSessionStatus::Connecting => ("Connecting", tokens.ui.warning),
        RemoteDesktopSessionStatus::Connected => ("Connected", tokens.ui.success),
        RemoteDesktopSessionStatus::Reconnecting => ("Reconnecting", tokens.ui.warning),
        RemoteDesktopSessionStatus::Disconnected => ("Disconnected", tokens.ui.text_muted),
        RemoteDesktopSessionStatus::Failed => ("Failed", tokens.ui.error),
    };

    div()
        .h(px(22.0))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.sm))
        .bg(rgba((color << 8) | 0x18))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(color))
        .child(label)
}

fn aux_badge(tokens: &ThemeTokens, label: &'static str) -> Div {
    div()
        .h(px(22.0))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.sm))
        .bg(rgba((tokens.ui.border << 8) | 0x33))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(label)
}

fn frame_body(tokens: &ThemeTokens, state: &RemoteDesktopViewState) -> AnyElement {
    if let Some(frame) = state.frame() {
        let Some(image) = render_image_for_frame(frame) else {
            return corrupted_frame_body(tokens, frame).into_any_element();
        };
        let frame_size = frame.size;

        return div()
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgba((tokens.ui.border << 8) | FRAME_BORDER_ALPHA))
            .bg(rgba((tokens.ui.bg_sunken << 8) | FRAME_BG_ALPHA))
            .overflow_hidden()
            .child(remote_desktop_frame_canvas(
                image,
                frame_size.width,
                frame_size.height,
            ))
            .into_any_element();
    }

    empty_state(
        tokens,
        "RD",
        "Waiting for the first remote frame",
        Some("The helper is connected, but no desktop frame has arrived yet.".to_string()),
        None,
    )
    .into_any_element()
}

fn corrupted_frame_body(tokens: &ThemeTokens, frame: &RemoteDesktopFrame) -> Div {
    let format_label = match frame.format {
        RemoteDesktopFrameFormat::Rgba8 => "RGBA",
        RemoteDesktopFrameFormat::Bgra8 => "BGRA",
    };

    div()
        .size_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .rounded(px(tokens.radii.md))
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
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(tokens.ui.text_heading))
                .child("Remote frame is incomplete"),
        )
        .child(
            div()
                .text_size(px(tokens.metrics.ui_text_xs))
                .child(format!(
                    "{} x {}, {format_label}, {} bytes",
                    frame.size.width,
                    frame.size.height,
                    frame.bytes.len()
                )),
        )
}

fn remote_desktop_frame_canvas(
    image: Arc<RenderImage>,
    width: u32,
    height: u32,
) -> impl IntoElement {
    canvas(
        move |bounds, _window: &mut Window, _cx| {
            ObjectFit::Contain.get_bounds(
                bounds,
                size(DevicePixels(width as i32), DevicePixels(height as i32)),
            )
        },
        move |bounds, image_bounds, window: &mut Window, _cx| {
            window.paint_quad(fill(bounds, rgb(0x000000)));
            let _ = window.paint_image(image_bounds, Corners::all(px(0.0)), image, 0, false);
        },
    )
    .size_full()
}

fn render_image_for_frame(frame: &RemoteDesktopFrame) -> Option<Arc<RenderImage>> {
    if !frame.is_complete() {
        return None;
    }

    let mut bytes = frame.bytes.clone();
    if frame.format == RemoteDesktopFrameFormat::Bgra8 {
        // GPUI's image path expects RGBA. Keep the protocol boundary format
        // explicit so future helpers can choose the cheapest transport format.
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }

    let buffer = RgbaImage::from_raw(frame.size.width, frame.size.height, bytes)?;
    Some(Arc::new(RenderImage::new(vec![ImageFrame::new(buffer)])))
}

fn placeholder_body(
    tokens: &ThemeTokens,
    status: RemoteDesktopSessionStatus,
    message: Option<String>,
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

    empty_state(tokens, "RD", title, message, None).into_any_element()
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
