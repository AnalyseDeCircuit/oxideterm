// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{rc::Rc, sync::Arc};

use gpui::{
    AnyElement, Context, Rgba, ScrollHandle, SharedString, UniformListScrollHandle, Window, div,
    prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    icons::LucideIcon,
    virtual_list::{TauriVirtualListSpec, tauri_virtual_uniform_list},
};
use oxideterm_i18n::I18n;
use oxideterm_sftp::{
    TextDiffLine as SftpDiffLine, TextDiffLineKind as SftpDiffLineKind, TextDiffStats,
    compute_text_diff, text_diff_stats,
};
use oxideterm_theme::ThemeTokens;

use crate::style::SftpUiMetrics;

const SFTP_ICON_SM: f32 = 12.0; // Tauri h-3 w-3
const SFTP_DIFF_ROW_HEIGHT: f32 = 21.0; // Tauri FileDiffDialog text-xs py-0.5 border row
const SFTP_DIFF_VIRTUAL_OVERSCAN: usize = 15; // Diff dialog keeps the same file-list overdraw budget.
const SFTP_DIFF_LINE_NUMBER_COL: f32 = 48.0; // Tauri w-12
const SFTP_DIFF_WRAP_COLUMNS: usize = 64; // max-w-5xl split diff leaves roughly this many mono chars per side.
const SFTP_GREEN: u32 = 0x22c55e; // Tauri text-green-500
const SFTP_RED: u32 = 0xf87171; // Tauri text-red-400
const SFTP_DIALOG_BORDER_HALF_ALPHA: u32 = 0x80; // Tauri border-theme-border/50
const SFTP_DIFF_HEADER_BG_ALPHA: u32 = 0x33; // Tauri bg-red/green-950/20
const SFTP_DIFF_LINE_BG_ALPHA: u32 = 0x4d; // Tauri bg-red/green-950/30

pub struct SftpDiffFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub mono_font: SharedString,
    pub selectable_text: Rc<dyn Fn(&str, Option<&str>, String, u32) -> AnyElement>,
}

pub struct SftpDiffView {
    local_path: String,
    remote_path: String,
    lines: Arc<Vec<SftpDiffVisualLine>>,
    stats: TextDiffStats,
    scroll: UniformListScrollHandle,
    document_scroll: ScrollHandle,
    frame: Option<SftpDiffFrame>,
}

impl SftpDiffView {
    pub fn new(local_path: String, local: &str, remote_path: String, remote: &str) -> Self {
        let lines = compute_text_diff(local, remote);
        Self {
            local_path,
            remote_path,
            stats: text_diff_stats(&lines),
            lines: Arc::new(sftp_diff_visual_lines(&lines)),
            scroll: UniformListScrollHandle::new(),
            document_scroll: ScrollHandle::new(),
            frame: None,
        }
    }

    pub fn stats(&self) -> TextDiffStats {
        self.stats
    }

    pub fn document_scroll(&self) -> ScrollHandle {
        self.document_scroll.clone()
    }

    pub fn configure(&mut self, frame: SftpDiffFrame, cx: &mut Context<Self>) {
        self.frame = Some(frame);
        cx.notify();
    }
}

impl Render for SftpDiffView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = &self.frame else {
            return div().into_any_element();
        };
        let ui = SftpUiMetrics::from_tokens(&frame.tokens);
        let theme = frame.tokens.ui;
        let local_path = self.local_path.as_str();
        let remote_path = self.remote_path.as_str();
        let stats = self.stats;
        let diff_lines = self.lines.clone();
        let line_count = diff_lines.len();
        let diff_scroll = self.scroll.clone();
        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_sunken))
            .child(
                div()
                    .w_full()
                    .flex()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .text_size(px(ui.text_xs))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba((0x7f1d1d << 8) | SFTP_DIFF_HEADER_BG_ALPHA))
                            .child(render_lucide_icon(
                                LucideIcon::File,
                                ui.size(SFTP_ICON_SM),
                                rgb(SFTP_RED),
                            ))
                            .child(
                                div()
                                    .text_color(rgb(0xfca5a5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child((frame.selectable_text)(
                                        "sftp-diff-local-label",
                                        None,
                                        format!("{}:", frame.i18n.t("sftp.diff.local")),
                                        0xfca5a5,
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child((frame.selectable_text)(
                                        "sftp-diff-local-path",
                                        Some(local_path),
                                        sftp_file_name(local_path),
                                        theme.text_muted,
                                    )),
                            )
                            .child(div().ml_auto().text_color(rgb(SFTP_RED)).child(
                                (frame.selectable_text)(
                                    "sftp-diff-local-removed",
                                    None,
                                    format!("-{}", stats.removed),
                                    SFTP_RED,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba((0x14532d << 8) | SFTP_DIFF_HEADER_BG_ALPHA))
                            .child(render_lucide_icon(
                                LucideIcon::File,
                                ui.size(SFTP_ICON_SM),
                                rgb(SFTP_GREEN),
                            ))
                            .child(
                                div()
                                    .text_color(rgb(0x86efac))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child((frame.selectable_text)(
                                        "sftp-diff-remote-label",
                                        None,
                                        format!("{}:", frame.i18n.t("sftp.diff.remote")),
                                        0x86efac,
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(rgb(theme.text_muted))
                                    .child((frame.selectable_text)(
                                        "sftp-diff-remote-path",
                                        Some(remote_path),
                                        sftp_file_name(remote_path),
                                        theme.text_muted,
                                    )),
                            )
                            .child(div().ml_auto().text_color(rgb(SFTP_GREEN)).child(
                                (frame.selectable_text)(
                                    "sftp-diff-remote-added",
                                    None,
                                    format!("+{}", stats.added),
                                    SFTP_GREEN,
                                ),
                            )),
                    ),
            )
            .child(
                div()
                    .id("sftp-diff-scroll")
                    .w_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.document_scroll)
                    .font_family(frame.mono_font.clone())
                    .text_size(px(ui.text_xs))
                    .on_scroll(|_, _, cx| cx.stop_propagation())
                    .when(line_count == 0, |body| {
                        body.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(theme.text_muted))
                                .child(frame.i18n.t("sftp.diff.identical")),
                        )
                    })
                    .when(line_count > 0, |body| {
                        let diff_lines = diff_lines.clone();
                        body.child(
                            tauri_virtual_uniform_list(
                                "sftp-diff-virtual-list",
                                line_count,
                                diff_scroll,
                                TauriVirtualListSpec::new(
                                    px(ui.size(SFTP_DIFF_ROW_HEIGHT)),
                                    SFTP_DIFF_VIRTUAL_OVERSCAN,
                                ),
                                move |range, _window, _cx| {
                                    range
                                        .map(|index| {
                                            let line = diff_lines[index].clone();
                                            let removed = line.kind == SftpDiffLineKind::Removed;
                                            let added = line.kind == SftpDiffLineKind::Added;
                                            div()
                                                .w_full()
                                                .h(px(ui.size(SFTP_DIFF_ROW_HEIGHT)))
                                                .flex()
                                                .border_b_1()
                                                .border_color(rgba(
                                                    (theme.border << 8)
                                                        | SFTP_DIALOG_BORDER_HALF_ALPHA,
                                                ))
                                                .child(diff_cell(
                                                    &line.left_line_num,
                                                    &line.left_content,
                                                    removed,
                                                    theme.border,
                                                    true,
                                                    ui.size(SFTP_DIFF_LINE_NUMBER_COL),
                                                ))
                                                .child(diff_cell(
                                                    &line.right_line_num,
                                                    &line.right_content,
                                                    added,
                                                    theme.border,
                                                    false,
                                                    ui.size(SFTP_DIFF_LINE_NUMBER_COL),
                                                ))
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )
                            .on_scroll(|_, _, cx| cx.stop_propagation()),
                        )
                    }),
            )
            .into_any_element()
    }
}

fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

fn sftp_file_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[derive(Clone, Debug)]
struct SftpDiffVisualLine {
    kind: SftpDiffLineKind,
    left_line_num: String,
    right_line_num: String,
    left_content: String,
    right_content: String,
}

fn sftp_diff_visual_lines(lines: &[SftpDiffLine]) -> Vec<SftpDiffVisualLine> {
    let mut visual_lines = Vec::new();
    for line in lines {
        let removed = line.kind == SftpDiffLineKind::Removed;
        let added = line.kind == SftpDiffLineKind::Added;
        let left_content = if added {
            String::new()
        } else if removed {
            format!("- {}", line.content)
        } else {
            line.content.clone()
        };
        let right_content = if removed {
            String::new()
        } else if added {
            format!("+ {}", line.content)
        } else {
            line.content.clone()
        };
        let left_chunks = wrap_sftp_virtual_text_line(&left_content, SFTP_DIFF_WRAP_COLUMNS);
        let right_chunks = wrap_sftp_virtual_text_line(&right_content, SFTP_DIFF_WRAP_COLUMNS);
        let row_count = left_chunks.len().max(right_chunks.len()).max(1);

        for chunk_index in 0..row_count {
            visual_lines.push(SftpDiffVisualLine {
                kind: line.kind,
                left_line_num: if chunk_index == 0 {
                    line.left_line_num
                        .map(|number| number.to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                },
                right_line_num: if chunk_index == 0 {
                    line.right_line_num
                        .map(|number| number.to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                },
                left_content: left_chunks.get(chunk_index).cloned().unwrap_or_default(),
                right_content: right_chunks.get(chunk_index).cloned().unwrap_or_default(),
            });
        }
    }
    visual_lines
}

fn wrap_sftp_virtual_text_line(line: &str, max_columns: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    // Tauri uses CSS overflow for long `whitespace-pre` lines. GPUI's virtual
    // lists here have fixed row heights, so we pre-split by character columns
    // to keep long preview/diff lines readable without letting them bleed out
    // of the modal or forcing the UI tree to render every source line at once.
    let max_columns = max_columns.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut width = 0usize;
    for ch in line.chars() {
        if width >= max_columns {
            chunks.push(std::mem::take(&mut current));
            width = 0;
        }
        current.push(ch);
        width += 1;
    }
    chunks.push(current);
    chunks
}

fn diff_cell(
    number: &str,
    content: &str,
    highlighted: bool,
    border: u32,
    left: bool,
    number_column_width: f32,
) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .border_r_1()
        .border_color(rgb(border))
        .bg(if highlighted {
            if left {
                rgba((0x7f1d1d << 8) | SFTP_DIFF_LINE_BG_ALPHA)
            } else {
                rgba((0x14532d << 8) | SFTP_DIFF_LINE_BG_ALPHA)
            }
        } else {
            rgba(0x00000000)
        })
        .child(
            div()
                .w(px(number_column_width))
                .flex_none()
                .px(px(8.0))
                .py(px(2.0))
                .text_align(gpui::TextAlign::Right)
                .text_color(if highlighted {
                    if left { rgb(SFTP_RED) } else { rgb(SFTP_GREEN) }
                } else {
                    rgb(0xa1a1aa)
                })
                .border_r_1()
                .border_color(rgb(border))
                .child(number.to_string()),
        )
        .child(
            div()
                .flex_1()
                .px(px(8.0))
                .py(px(2.0))
                .child(content.to_string()),
        )
        .into_any_element()
}
