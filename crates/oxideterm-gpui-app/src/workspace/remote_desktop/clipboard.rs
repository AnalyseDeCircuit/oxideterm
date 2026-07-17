// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn remote_desktop_clipboard_data_from_item(
    item: &ClipboardItem,
) -> Option<RemoteDesktopClipboardData> {
    item.entries().iter().find_map(|entry| {
        let ClipboardEntry::Image(image) = entry else {
            return None;
        };
        if image.bytes.is_empty() {
            return None;
        }
        let format = remote_desktop_clipboard_format_from_gpui(image.format)?;
        Some(RemoteDesktopClipboardData::new(format, image.bytes.clone()))
    })
}

pub(super) fn remote_desktop_clipboard_item_from_data(
    data: &RemoteDesktopClipboardData,
) -> Option<ClipboardItem> {
    if data.bytes.is_empty() {
        return None;
    }
    let format = gpui_image_format_from_remote_desktop(data.format)?;
    Some(ClipboardItem::new_image(&Image::from_bytes(
        format,
        data.bytes.clone(),
    )))
}

pub(super) fn remote_desktop_clipboard_format_from_gpui(
    format: ImageFormat,
) -> Option<RemoteDesktopClipboardFormat> {
    Some(match format {
        ImageFormat::Png => RemoteDesktopClipboardFormat::ImagePng,
        ImageFormat::Jpeg => RemoteDesktopClipboardFormat::ImageJpeg,
        ImageFormat::Webp => RemoteDesktopClipboardFormat::ImageWebp,
        ImageFormat::Gif => RemoteDesktopClipboardFormat::ImageGif,
        ImageFormat::Svg => RemoteDesktopClipboardFormat::ImageSvg,
        ImageFormat::Bmp => RemoteDesktopClipboardFormat::ImageBmp,
        ImageFormat::Tiff => RemoteDesktopClipboardFormat::ImageTiff,
        // The remote-desktop protocol model does not currently advertise ICO or Netpbm.
        ImageFormat::Ico | ImageFormat::Pnm => return None,
    })
}

pub(super) fn gpui_image_format_from_remote_desktop(
    format: RemoteDesktopClipboardFormat,
) -> Option<ImageFormat> {
    Some(match format {
        RemoteDesktopClipboardFormat::ImagePng => ImageFormat::Png,
        RemoteDesktopClipboardFormat::ImageJpeg => ImageFormat::Jpeg,
        RemoteDesktopClipboardFormat::ImageWebp => ImageFormat::Webp,
        RemoteDesktopClipboardFormat::ImageGif => ImageFormat::Gif,
        RemoteDesktopClipboardFormat::ImageSvg => ImageFormat::Svg,
        RemoteDesktopClipboardFormat::ImageBmp => ImageFormat::Bmp,
        RemoteDesktopClipboardFormat::ImageTiff => ImageFormat::Tiff,
    })
}

pub(super) fn remote_desktop_protocol_chip(
    tokens: &ThemeTokens,
    protocol: RemoteDesktopProtocol,
) -> gpui::Div {
    let label = match protocol {
        RemoteDesktopProtocol::Rdp => "RDP",
        RemoteDesktopProtocol::Vnc => "VNC",
    };

    div()
        .h(px(20.0))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.sm))
        .bg(rgba((tokens.ui.accent << 8) | 0x1f))
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(rgb(tokens.ui.accent))
        .child(label)
}

pub(super) fn remote_desktop_capability_chip(
    tokens: &ThemeTokens,
    label: impl Into<String>,
) -> gpui::Div {
    div()
        .min_w(px(0.0))
        .flex_shrink()
        .h(px(20.0))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgba((tokens.ui.border << 8) | 0x99))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text_muted))
        .child(div().min_w(px(0.0)).truncate().child(label.into()))
}
