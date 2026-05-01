// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Experimental native terminal engine lifecycle.
//!
//! This module is intentionally wired into the existing Tauri app/backend. It
//! must never create a second SSH shell or local PTY for a pane. Native surfaces
//! attach to already-created terminal sessions and consume the same output
//! broadcast as xterm.js.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::point_to_viewport;
use alacritty_terminal::term::{Config as AlacrittyConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Color, CursorShape, NamedColor, Rgb};
use dashmap::DashMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::session::SessionRegistry;
use crate::session::scroll_buffer::TerminalLine;
use crate::ssh::SessionCommand;

const NATIVE_TERMINAL_HISTORY_LINES: usize = 10_000;
const NATIVE_TERMINAL_REPLAY_LINES: usize = 2_000;
const MIN_NATIVE_COLUMNS: usize = 2;
const MIN_NATIVE_ROWS: usize = 1;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeTerminalType {
    Terminal,
    LocalTerminal,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_dpr")]
    pub dpr: f64,
}

fn default_dpr() -> f64 {
    1.0
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalFont {
    pub family: String,
    #[serde(default)]
    pub family_css: Option<String>,
    #[serde(default)]
    pub primary_family: Option<String>,
    #[serde(default)]
    pub fallback_families: Vec<String>,
    pub size: f64,
    pub line_height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalFontMetrics {
    pub resolved_family: String,
    pub cell_width_px: f64,
    pub cell_height_px: f64,
    pub baseline_y_px: f64,
    pub ascent_px: f64,
    pub descent_px: f64,
    pub leading_px: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalTheme {
    pub foreground: String,
    pub background: String,
    pub cursor: String,
    pub selection: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalAttachRequest {
    pub pane_id: String,
    pub terminal_type: NativeTerminalType,
    pub session_id: String,
    pub node_id: Option<String>,
    pub bounds: NativeTerminalBounds,
    pub font: NativeTerminalFont,
    pub theme: NativeTerminalTheme,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalUpdateBoundsRequest {
    pub surface_id: String,
    pub bounds: NativeTerminalBounds,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalUpdateVisibilityRequest {
    pub surface_id: String,
    pub visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalUpdateSettingsRequest {
    pub surface_id: String,
    pub font: NativeTerminalFont,
    pub theme: NativeTerminalTheme,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalWriteRequest {
    pub surface_id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeTerminalSurfaceStatus {
    Ready,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalAttachResponse {
    pub surface_id: String,
    pub status: NativeTerminalSurfaceStatus,
    pub backend: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeTerminalActiveBuffer {
    Normal,
    AltScreen,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalSnapshot {
    pub surface_id: String,
    pub pane_id: String,
    pub node_id: Option<String>,
    pub visible: bool,
    pub bounds: NativeTerminalBounds,
    pub status: NativeTerminalSurfaceStatus,
    pub columns: usize,
    pub rows: usize,
    pub revision: u64,
    pub parsed_bytes: u64,
    pub output_bytes: u64,
    pub dropped_output_frames: u64,
    pub lines: Vec<String>,
    pub styled_rows: Vec<NativeTerminalStyledRow>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    pub active_buffer: NativeTerminalActiveBuffer,
    pub scrollback_len: usize,
    pub viewport_rows: usize,
    pub viewport_top: usize,
    pub follow_tail: bool,
    pub pinned_to_bottom: bool,
    pub can_scroll_up: bool,
    pub can_scroll_down: bool,
    pub font_metrics: NativeTerminalFontMetrics,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalStyledRow {
    pub visible_row: usize,
    pub cells: Vec<NativeTerminalCellRun>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeTerminalCellRun {
    pub text: String,
    pub visible_row: usize,
    pub start_col: usize,
    pub width_cols: usize,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

struct NativeTerminalSurface {
    pane_id: String,
    terminal_type: NativeTerminalType,
    session_id: String,
    node_id: Option<String>,
    bounds: NativeTerminalBounds,
    font: NativeTerminalFont,
    theme: NativeTerminalTheme,
    status: NativeTerminalSurfaceStatus,
    visible: bool,
    output_bytes: Arc<AtomicU64>,
    dropped_output_frames: Arc<AtomicU64>,
    runtime: Arc<Mutex<NativeTerminalRuntime>>,
    pump: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct NativeTerminalState {
    surfaces: DashMap<String, NativeTerminalSurface>,
}

impl NativeTerminalState {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, id: String, surface: NativeTerminalSurface) {
        self.surfaces.insert(id, surface);
    }

    fn remove(&self, surface_id: &str) -> Option<NativeTerminalSurface> {
        self.surfaces.remove(surface_id).map(|(_, surface)| surface)
    }
}

#[derive(Clone, Copy, Debug)]
struct NativeTerminalEventProxy;

impl EventListener for NativeTerminalEventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(payload) => {
                tracing::trace!(bytes = payload.len(), "native terminal requested PTY write");
            }
            Event::Title(title) => {
                tracing::trace!(title, "native terminal title changed");
            }
            Event::Bell => {
                tracing::trace!("native terminal bell");
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct NativeTermSize {
    columns: usize,
    rows: usize,
}

impl Dimensions for NativeTermSize {
    fn total_lines(&self) -> usize {
        self.rows + NATIVE_TERMINAL_HISTORY_LINES
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

struct NativeTerminalRuntime {
    term: Term<NativeTerminalEventProxy>,
    parser: ansi::Processor,
    columns: usize,
    rows: usize,
    font_metrics: NativeTerminalFontMetrics,
    parsed_bytes: u64,
    revision: u64,
    follow_tail: bool,
}

impl NativeTerminalRuntime {
    fn new(bounds: &NativeTerminalBounds, font: &NativeTerminalFont) -> Self {
        let font_metrics = native_font_metrics(font);
        let size = native_size_from_bounds_with_metrics(bounds, &font_metrics);
        let config = AlacrittyConfig {
            scrolling_history: NATIVE_TERMINAL_HISTORY_LINES,
            ..Default::default()
        };
        let term = Term::new(config, &size, NativeTerminalEventProxy);

        Self {
            term,
            parser: ansi::Processor::new(),
            columns: size.columns,
            rows: size.rows,
            font_metrics,
            parsed_bytes: 0,
            revision: 0,
            follow_tail: true,
        }
    }

    fn ingest(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.parser.advance(&mut self.term, bytes);
        if self.follow_tail {
            self.term.scroll_display(Scroll::Bottom);
        }
        self.parsed_bytes = self.parsed_bytes.saturating_add(bytes.len() as u64);
        self.revision = self.revision.saturating_add(1);
    }

    fn resize(&mut self, bounds: &NativeTerminalBounds, font: &NativeTerminalFont) -> bool {
        let font_metrics = native_font_metrics(font);
        let size = native_size_from_bounds_with_metrics(bounds, &font_metrics);
        let metrics_changed = !font_metrics_approx_eq(&self.font_metrics, &font_metrics);
        if size.columns == self.columns && size.rows == self.rows && !metrics_changed {
            return false;
        }
        let grid_changed = size.columns != self.columns || size.rows != self.rows;
        if size.columns != self.columns || size.rows != self.rows {
            self.term.resize(size);
        }
        if self.follow_tail {
            self.term.scroll_display(Scroll::Bottom);
        }
        self.columns = size.columns;
        self.rows = size.rows;
        self.font_metrics = font_metrics;
        self.revision = self.revision.saturating_add(1);
        grid_changed
    }

    fn scroll_delta(&mut self, delta_rows: i32) {
        if delta_rows == 0 || self.is_alt_screen() {
            return;
        }
        let target_top = self
            .viewport_top()
            .saturating_sub_signed(delta_rows as isize)
            .min(self.max_viewport_top());
        self.set_viewport_top(target_top);
    }

    fn scroll_toward_bottom(&mut self, rows: usize) {
        if rows == 0 || self.is_alt_screen() {
            return;
        }
        let target_top = self
            .viewport_top()
            .saturating_add(rows)
            .min(self.max_viewport_top());
        self.set_viewport_top(target_top);
    }

    fn set_viewport_top(&mut self, viewport_top: usize) {
        if self.is_alt_screen() {
            return;
        }
        let max_top = self.max_viewport_top();
        let target_top = viewport_top.min(max_top);
        let target_offset = max_top.saturating_sub(target_top);
        let current_offset = self.term.grid().display_offset();
        let delta = target_offset as i64 - current_offset as i64;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(
                delta.clamp(i32::MIN as i64, i32::MAX as i64) as i32
            ));
        }
        self.follow_tail = target_top == max_top;
        self.revision = self.revision.saturating_add(1);
    }

    fn page_up(&mut self) {
        if self.is_alt_screen() {
            return;
        }
        self.scroll_delta(self.page_scroll_rows());
    }

    fn page_down(&mut self) {
        if self.is_alt_screen() {
            return;
        }
        self.scroll_toward_bottom(self.page_scroll_rows() as usize);
    }

    fn page_scroll_rows(&self) -> i32 {
        self.rows.saturating_sub(1).max(1).min(i32::MAX as usize) as i32
    }

    fn scroll_to_bottom(&mut self) {
        if self.is_alt_screen() {
            return;
        }
        self.set_viewport_top(self.max_viewport_top());
        self.follow_tail = true;
    }

    fn history_size(&self) -> usize {
        self.term
            .grid()
            .total_lines()
            .saturating_sub(self.term.grid().screen_lines())
    }

    fn scrollback_len(&self) -> usize {
        self.history_size().saturating_add(self.rows)
    }

    fn max_viewport_top(&self) -> usize {
        self.scrollback_len().saturating_sub(self.rows)
    }

    fn viewport_top(&self) -> usize {
        self.max_viewport_top()
            .saturating_sub(self.term.grid().display_offset())
    }

    fn visible_lines(&self) -> Vec<String> {
        self.visible_styled_rows()
            .into_iter()
            .map(|row| {
                let mut line = row
                    .cells
                    .into_iter()
                    .map(|run| run.text)
                    .collect::<String>();
                while line.ends_with(' ') {
                    line.pop();
                }
                line
            })
            .collect()
    }

    fn visible_styled_rows(&self) -> Vec<NativeTerminalStyledRow> {
        let mut rows: Vec<Vec<CellSnapshot>> = (0..self.rows)
            .map(|visible_row| {
                (0..self.columns)
                    .map(|col| CellSnapshot {
                        visible_row,
                        start_col: col,
                        width_cols: 1,
                        ..CellSnapshot::default()
                    })
                    .collect()
            })
            .collect();
        let content = self.term.renderable_content();
        let colors = content.colors;
        let display_offset = content.display_offset;

        for indexed in content.display_iter {
            let Some(point) = point_to_viewport(display_offset, indexed.point) else {
                continue;
            };
            let visible_row = point.line;
            let column = point.column.0;
            if visible_row >= self.rows || column >= self.columns {
                continue;
            }

            let cell = indexed.cell;
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER)
                || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }

            let ch = if cell.flags.contains(Flags::HIDDEN) {
                ' '
            } else {
                cell.c
            };

            // Keep the snapshot aligned to Alacritty's grid columns. The
            // WebView bridge is temporary until the macOS MTKView glyph
            // renderer lands, but it must still respect terminal cell
            // positions; appending cells sequentially compresses spaces and
            // makes wide/CJK output drift.
            let width_cols = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            rows[visible_row][column] = CellSnapshot {
                text: ch.to_string(),
                visible_row,
                start_col: column,
                width_cols,
                fg: color_to_hex(cell.fg, colors),
                bg: color_to_hex(cell.bg, colors),
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
                inverse: cell.flags.contains(Flags::INVERSE),
                skip: false,
            };
            if let Some(zerowidth) = cell.zerowidth() {
                rows[visible_row][column]
                    .text
                    .extend(zerowidth.iter().copied());
            }
            if width_cols > 1 && column + 1 < self.columns {
                rows[visible_row][column + 1].skip = true;
            }
        }

        rows.into_iter()
            .enumerate()
            .map(|(visible_row, cells)| NativeTerminalStyledRow {
                visible_row,
                cells: trim_cell_runs(merge_cell_runs(cells)),
            })
            .collect()
    }

    fn cursor_position(&self) -> (usize, usize) {
        let cursor = self.term.renderable_content().cursor;
        (cursor.point.line.0.max(0) as usize, cursor.point.column.0)
    }

    fn cursor_shape(&self) -> CursorShape {
        self.term.renderable_content().cursor.shape
    }

    fn mode_snapshot(&self) -> (bool, bool) {
        let mode = self.term.mode();
        (
            mode.contains(TermMode::BRACKETED_PASTE),
            mode.contains(TermMode::ALT_SCREEN),
        )
    }

    fn is_alt_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    fn viewport_snapshot(&self) -> NativeTerminalViewportSnapshot {
        let alt_screen = self.is_alt_screen();
        if alt_screen {
            return NativeTerminalViewportSnapshot {
                active_buffer: NativeTerminalActiveBuffer::AltScreen,
                scrollback_len: self.rows,
                viewport_rows: self.rows,
                viewport_top: 0,
                follow_tail: true,
                pinned_to_bottom: true,
                can_scroll_up: false,
                can_scroll_down: false,
            };
        }

        let history_size = self.history_size();
        let display_offset = self.term.grid().display_offset();
        let viewport_top = self.viewport_top();
        NativeTerminalViewportSnapshot {
            active_buffer: NativeTerminalActiveBuffer::Normal,
            scrollback_len: self.scrollback_len(),
            viewport_rows: self.rows,
            viewport_top,
            follow_tail: self.follow_tail,
            pinned_to_bottom: viewport_top == self.max_viewport_top(),
            can_scroll_up: display_offset < history_size,
            can_scroll_down: display_offset > 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeTerminalViewportSnapshot {
    active_buffer: NativeTerminalActiveBuffer,
    scrollback_len: usize,
    viewport_rows: usize,
    viewport_top: usize,
    follow_tail: bool,
    pinned_to_bottom: bool,
    can_scroll_up: bool,
    can_scroll_down: bool,
}

#[derive(Clone, Default)]
struct CellSnapshot {
    text: String,
    visible_row: usize,
    start_col: usize,
    width_cols: usize,
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
    skip: bool,
}

fn merge_cell_runs(cells: Vec<CellSnapshot>) -> Vec<NativeTerminalCellRun> {
    let mut runs: Vec<NativeTerminalCellRun> = Vec::new();
    for cell in cells {
        if cell.skip {
            continue;
        }
        // Renderer correctness rule: never merge adjacent terminal cells into a
        // natural text run. AppKit/CoreText may apply glyph advances, kerning,
        // fallback shaping, or ligatures inside a string. Terminal layout must
        // come only from Alacritty's grid coordinates: x = col * cell_width.
        runs.push(NativeTerminalCellRun {
            text: if cell.text.is_empty() {
                " ".to_string()
            } else {
                cell.text
            },
            visible_row: cell.visible_row,
            start_col: cell.start_col,
            width_cols: cell.width_cols.max(1),
            fg: cell.fg,
            bg: cell.bg,
            bold: cell.bold,
            italic: cell.italic,
            underline: cell.underline,
            inverse: cell.inverse,
        });
    }
    runs
}

fn trim_cell_runs(mut runs: Vec<NativeTerminalCellRun>) -> Vec<NativeTerminalCellRun> {
    while let Some(last) = runs.last_mut() {
        let trimmed_len = last.text.trim_end_matches(' ').len();
        if trimmed_len == last.text.len() {
            break;
        }
        let removed_chars = last.text[trimmed_len..].chars().count();
        last.text.truncate(trimmed_len);
        last.width_cols = last.width_cols.saturating_sub(removed_chars);
        if !last.text.is_empty() {
            break;
        }
        runs.pop();
    }
    runs
}

fn color_to_hex(color: Color, colors: &Colors) -> Option<String> {
    let rgb = match color {
        Color::Spec(rgb) => Some(rgb),
        Color::Indexed(index) => indexed_color(index),
        Color::Named(named) => colors[named as usize].or_else(|| named_color(named)),
    }?;
    Some(rgb_to_hex(rgb))
}

fn rgb_to_hex(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

fn named_color(named: NamedColor) -> Option<Rgb> {
    match named {
        NamedColor::Black => Some(Rgb {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        }),
        NamedColor::Red => Some(Rgb {
            r: 0xcd,
            g: 0x31,
            b: 0x31,
        }),
        NamedColor::Green => Some(Rgb {
            r: 0x0d,
            g: 0xbc,
            b: 0x79,
        }),
        NamedColor::Yellow => Some(Rgb {
            r: 0xe5,
            g: 0xe5,
            b: 0x10,
        }),
        NamedColor::Blue => Some(Rgb {
            r: 0x24,
            g: 0x71,
            b: 0xa3,
        }),
        NamedColor::Magenta => Some(Rgb {
            r: 0xbc,
            g: 0x3f,
            b: 0xbc,
        }),
        NamedColor::Cyan => Some(Rgb {
            r: 0x11,
            g: 0xa8,
            b: 0xcd,
        }),
        NamedColor::White => Some(Rgb {
            r: 0xe5,
            g: 0xe5,
            b: 0xe5,
        }),
        NamedColor::BrightBlack | NamedColor::DimBlack => Some(Rgb {
            r: 0x66,
            g: 0x66,
            b: 0x66,
        }),
        NamedColor::BrightRed | NamedColor::DimRed => Some(Rgb {
            r: 0xf1,
            g: 0x4c,
            b: 0x4c,
        }),
        NamedColor::BrightGreen | NamedColor::DimGreen => Some(Rgb {
            r: 0x23,
            g: 0xd1,
            b: 0x8b,
        }),
        NamedColor::BrightYellow | NamedColor::DimYellow => Some(Rgb {
            r: 0xf5,
            g: 0xf5,
            b: 0x43,
        }),
        NamedColor::BrightBlue | NamedColor::DimBlue => Some(Rgb {
            r: 0x3b,
            g: 0x8e,
            b: 0xea,
        }),
        NamedColor::BrightMagenta | NamedColor::DimMagenta => Some(Rgb {
            r: 0xd6,
            g: 0x70,
            b: 0xd6,
        }),
        NamedColor::BrightCyan | NamedColor::DimCyan => Some(Rgb {
            r: 0x29,
            g: 0xb8,
            b: 0xdb,
        }),
        NamedColor::BrightWhite | NamedColor::DimWhite => Some(Rgb {
            r: 0xff,
            g: 0xff,
            b: 0xff,
        }),
        NamedColor::Foreground
        | NamedColor::Background
        | NamedColor::Cursor
        | NamedColor::BrightForeground
        | NamedColor::DimForeground => None,
    }
}

fn indexed_color(index: u8) -> Option<Rgb> {
    if index < 16 {
        return named_color(match index {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::BrightBlack,
            9 => NamedColor::BrightRed,
            10 => NamedColor::BrightGreen,
            11 => NamedColor::BrightYellow,
            12 => NamedColor::BrightBlue,
            13 => NamedColor::BrightMagenta,
            14 => NamedColor::BrightCyan,
            _ => NamedColor::BrightWhite,
        });
    }

    if (16..=231).contains(&index) {
        let i = index - 16;
        let channel = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
        return Some(Rgb {
            r: channel(i / 36),
            g: channel((i / 6) % 6),
            b: channel(i % 6),
        });
    }

    if index >= 232 {
        let value = 8 + (index - 232) * 10;
        return Some(Rgb {
            r: value,
            g: value,
            b: value,
        });
    }

    None
}

#[cfg(test)]
fn native_size_from_bounds(
    bounds: &NativeTerminalBounds,
    font: &NativeTerminalFont,
) -> NativeTermSize {
    native_size_from_bounds_with_metrics(bounds, &native_font_metrics(font))
}

fn native_size_from_bounds_with_metrics(
    bounds: &NativeTerminalBounds,
    metrics: &NativeTerminalFontMetrics,
) -> NativeTermSize {
    let cell_width = metrics.cell_width_px.max(1.0);
    let cell_height = metrics.cell_height_px.max(1.0);
    let columns = (bounds.width / cell_width)
        .floor()
        .max(MIN_NATIVE_COLUMNS as f64) as usize;
    let rows = (bounds.height / cell_height)
        .floor()
        .max(MIN_NATIVE_ROWS as f64) as usize;

    NativeTermSize { columns, rows }
}

fn native_font_metrics(font: &NativeTerminalFont) -> NativeTerminalFontMetrics {
    platform_font_metrics(font).unwrap_or_else(|| fallback_font_metrics(font))
}

fn fallback_font_metrics(font: &NativeTerminalFont) -> NativeTerminalFontMetrics {
    let font_size = font.size.max(1.0);
    let line_height = font.line_height.max(0.8);
    let cell_height = (font_size * line_height).ceil().max(1.0);
    NativeTerminalFontMetrics {
        resolved_family: font
            .primary_family
            .clone()
            .unwrap_or_else(|| font.family.clone()),
        cell_width_px: (font_size * 0.62).ceil().max(1.0),
        cell_height_px: cell_height,
        baseline_y_px: ((cell_height - font_size) / 2.0 + font_size * 0.82)
            .round()
            .clamp(1.0, cell_height),
        ascent_px: font_size * 0.8,
        descent_px: font_size * 0.2,
        leading_px: (cell_height - font_size).max(0.0),
    }
}

fn font_metrics_approx_eq(a: &NativeTerminalFontMetrics, b: &NativeTerminalFontMetrics) -> bool {
    a.resolved_family == b.resolved_family
        && (a.cell_width_px - b.cell_width_px).abs() < 0.01
        && (a.cell_height_px - b.cell_height_px).abs() < 0.01
        && (a.baseline_y_px - b.baseline_y_px).abs() < 0.01
}

#[cfg(not(target_os = "macos"))]
fn platform_font_metrics(_font: &NativeTerminalFont) -> Option<NativeTerminalFontMetrics> {
    None
}

fn spawn_lossy_output_drain(
    app: AppHandle,
    surface_id: String,
    runtime: Arc<Mutex<NativeTerminalRuntime>>,
    mut rx: broadcast::Receiver<Vec<u8>>,
) -> (JoinHandle<()>, Arc<AtomicU64>, Arc<AtomicU64>) {
    let output_bytes = Arc::new(AtomicU64::new(0));
    let dropped_output_frames = Arc::new(AtomicU64::new(0));
    let output_bytes_for_task = output_bytes.clone();
    let dropped_for_task = dropped_output_frames.clone();

    let handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(bytes) => {
                    output_bytes_for_task.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    runtime.lock().ingest(&bytes);
                    platform_request_redraw(&app, &surface_id);
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    dropped_for_task.fetch_add(skipped, Ordering::Relaxed);
                    tracing::debug!(
                        surface_id,
                        skipped,
                        "native terminal mirror lagged; dropping mirror frames"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    (handle, output_bytes, dropped_output_frames)
}

async fn get_remote_attach_source(
    app: &AppHandle,
    session_id: &str,
) -> Result<(broadcast::Sender<Vec<u8>>, Vec<TerminalLine>), String> {
    let registry = app
        .try_state::<Arc<SessionRegistry>>()
        .ok_or_else(|| "Session registry is not available".to_string())?;

    let (output_tx, scroll_buffer) = registry
        .with_session(session_id, |entry| entry.output_tx.clone())
        .and_then(|output_tx| {
            registry
                .with_session(session_id, |entry| entry.scroll_buffer.clone())
                .map(|scroll_buffer| (output_tx, scroll_buffer))
        })
        .ok_or_else(|| format!("Remote terminal session not found: {}", session_id))?;

    let (lines, _) = scroll_buffer.get_capped(NATIVE_TERMINAL_REPLAY_LINES).await;
    Ok((output_tx, lines))
}

#[cfg(feature = "local-terminal")]
async fn get_local_attach_source(
    app: &AppHandle,
    session_id: &str,
) -> Result<(broadcast::Sender<Vec<u8>>, Vec<TerminalLine>), String> {
    let state = app
        .try_state::<Arc<crate::commands::local::LocalTerminalState>>()
        .ok_or_else(|| "Local terminal state is not available".to_string())?;

    let output_tx = state
        .registry
        .with_session_output_tx(session_id)
        .await
        .map_err(|e| e.to_string())?;
    let (lines, _) = state
        .registry
        .get_capped_buffer(session_id, NATIVE_TERMINAL_REPLAY_LINES)
        .await
        .map_err(|e| e.to_string())?;

    Ok((output_tx, lines))
}

#[cfg(not(feature = "local-terminal"))]
async fn get_local_attach_source(
    _app: &AppHandle,
    session_id: &str,
) -> Result<(broadcast::Sender<Vec<u8>>, Vec<TerminalLine>), String> {
    Err(format!(
        "Local terminal support is not enabled; cannot attach {}",
        session_id
    ))
}

fn encode_replay_lines(lines: &[TerminalLine]) -> Vec<u8> {
    let mut replay = Vec::new();
    for line in lines {
        let text = line.ansi_text.as_deref().unwrap_or(&line.text);
        replay.extend_from_slice(text.as_bytes());
        if !text.ends_with('\n') {
            replay.extend_from_slice(b"\r\n");
        }
    }
    replay
}

#[cfg(target_os = "macos")]
fn platform_font_metrics(font: &NativeTerminalFont) -> Option<NativeTerminalFontMetrics> {
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSFont, NSFontAttributeName, NSStringDrawing};
    use objc2_foundation::{NSAttributedStringKey, NSDictionary, NSString};

    let font_size = font.size.max(1.0);
    let resolved = resolve_native_font(font, font_size)?;
    let ns_font = resolved.font;
    let font_obj: &AnyObject = unsafe { &*((&*ns_font) as *const NSFont).cast::<AnyObject>() };
    let attrs = unsafe {
        NSDictionary::<NSAttributedStringKey, AnyObject>::from_slices(
            &[NSFontAttributeName],
            &[font_obj],
        )
    };
    // Terminal cells need a fixed grid advance. Measure one representative
    // monospace glyph instead of a string, so CoreText kerning/natural text
    // layout can never influence the terminal cell width.
    let probe = NSString::from_str("M");
    let measured = unsafe { probe.sizeWithAttributes(Some(&attrs)) };
    let measured_cell = measured.width.ceil().max(1.0);
    let ascent = ns_font.ascender().max(1.0);
    let descent = ns_font.descender().abs().max(0.0);
    let leading = ns_font.leading().max(0.0);
    let natural_height = (ascent + descent + leading).ceil().max(1.0);
    let requested_height = (font_size * font.line_height.max(0.8)).ceil().max(1.0);
    let cell_height = requested_height.max(natural_height);
    let extra_leading = (cell_height - (ascent + descent)).max(0.0);

    Some(NativeTerminalFontMetrics {
        resolved_family: resolved.name,
        cell_width_px: measured_cell,
        cell_height_px: cell_height,
        baseline_y_px: (extra_leading / 2.0 + ascent)
            .round()
            .clamp(1.0, cell_height),
        ascent_px: ascent,
        descent_px: descent,
        leading_px: extra_leading,
    })
}

#[cfg(target_os = "macos")]
struct ResolvedNativeFont {
    font: objc2::rc::Retained<objc2_app_kit::NSFont>,
    name: String,
}

#[cfg(target_os = "macos")]
fn resolve_native_font(font: &NativeTerminalFont, size: f64) -> Option<ResolvedNativeFont> {
    use objc2_app_kit::NSFont;
    use objc2_foundation::NSString;

    let mut candidates = Vec::new();
    if let Some(primary) = &font.primary_family {
        candidates.push(primary.clone());
    }
    candidates.extend(font.fallback_families.iter().cloned());
    candidates.extend(parse_font_stack(
        font.family_css.as_deref().unwrap_or(&font.family),
    ));
    candidates.push(font.family.clone());

    for candidate in candidates {
        let normalized = normalize_font_family_candidate(&candidate);
        if normalized.is_empty() || normalized.eq_ignore_ascii_case("monospace") {
            continue;
        }
        if let Some(ns_font) = NSFont::fontWithName_size(&NSString::from_str(&normalized), size) {
            return Some(ResolvedNativeFont {
                font: ns_font,
                name: normalized,
            });
        }
    }

    let fallback = NSFont::userFixedPitchFontOfSize(size)
        .unwrap_or_else(|| NSFont::monospacedSystemFontOfSize_weight(size, 0.0));
    Some(ResolvedNativeFont {
        font: fallback,
        name: "system-monospace".to_string(),
    })
}

fn parse_font_stack(stack: &str) -> Vec<String> {
    let mut families = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in stack.chars() {
        if (ch == '"' || ch == '\'') && quote.map_or(true, |q| q == ch) {
            quote = if quote.is_some() { None } else { Some(ch) };
            continue;
        }
        if ch == ',' && quote.is_none() {
            let family = normalize_font_family_candidate(&current);
            if !family.is_empty() {
                families.push(family);
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }

    let family = normalize_font_family_candidate(&current);
    if !family.is_empty() {
        families.push(family);
    }
    families
}

fn normalize_font_family_candidate(candidate: &str) -> String {
    candidate
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

#[cfg(test)]
fn parse_font_stack_for_test(stack: &str) -> Vec<String> {
    parse_font_stack(stack)
}

#[cfg(target_os = "macos")]
fn platform_attach_status() -> (NativeTerminalSurfaceStatus, String, Option<String>) {
    // macOS is the first native_alacritty target. This status means the pane is
    // attached to the existing session and Alacritty owns parsing/grid state.
    // The Metal glyph surface is still isolated behind this boundary so xterm.js
    // remains a one-click fallback while the native renderer matures.
    (
        NativeTerminalSurfaceStatus::Ready,
        "macos_native".to_string(),
        Some("native_alacritty runtime attached to the existing session".to_string()),
    )
}

#[cfg(not(target_os = "macos"))]
fn platform_attach_status() -> (NativeTerminalSurfaceStatus, String, Option<String>) {
    (
        NativeTerminalSurfaceStatus::Unsupported,
        "unsupported".to_string(),
        Some("native_alacritty is currently only planned for macOS + Metal".to_string()),
    )
}

#[cfg(not(target_os = "macos"))]
fn platform_request_redraw(_app: &AppHandle, _surface_id: &str) {}

#[cfg(not(target_os = "macos"))]
fn platform_update_bounds(
    _app: &AppHandle,
    _surface_id: &str,
    _bounds: &NativeTerminalBounds,
) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn platform_update_visibility(
    _app: &AppHandle,
    _surface_id: &str,
    _visible: bool,
) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn platform_focus(_app: &AppHandle, _surface_id: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn platform_detach(_app: &AppHandle, _surface_id: &str) {}

#[cfg(target_os = "macos")]
fn platform_attach_view(
    app: &AppHandle,
    state: Arc<NativeTerminalState>,
    surface_id: &str,
    bounds: &NativeTerminalBounds,
) -> Result<(), String> {
    macos_surface::attach_view(app, state, surface_id, bounds)
}

#[cfg(target_os = "macos")]
fn platform_request_redraw(app: &AppHandle, surface_id: &str) {
    macos_surface::request_redraw(app, surface_id);
}

#[cfg(target_os = "macos")]
fn platform_update_bounds(
    app: &AppHandle,
    surface_id: &str,
    bounds: &NativeTerminalBounds,
) -> Result<(), String> {
    macos_surface::update_bounds(app, surface_id, bounds)
}

#[cfg(target_os = "macos")]
fn platform_update_visibility(
    app: &AppHandle,
    surface_id: &str,
    visible: bool,
) -> Result<(), String> {
    macos_surface::update_visibility(app, surface_id, visible)
}

#[cfg(target_os = "macos")]
fn platform_focus(app: &AppHandle, surface_id: &str) -> Result<(), String> {
    macos_surface::focus(app, surface_id)
}

#[cfg(target_os = "macos")]
fn platform_detach(app: &AppHandle, surface_id: &str) {
    macos_surface::detach(app, surface_id);
}

#[cfg(target_os = "macos")]
mod macos_surface {
    use std::cell::Cell;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use dashmap::DashMap;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSColor, NSEvent, NSEventModifierFlags, NSFont, NSFontAttributeName,
        NSForegroundColorAttributeName, NSPasteboard, NSPasteboardTypeString, NSRectFill,
        NSStringDrawing, NSView, NSWindow,
    };
    use objc2_foundation::{
        NSAttributedStringKey, NSDictionary, NSPoint, NSRect, NSSize, NSString,
    };
    use tauri::{AppHandle, Manager};

    use super::{
        NativeTerminalBounds, NativeTerminalFontMetrics, NativeTerminalState, resolve_native_font,
        write_existing_session,
    };

    static NEXT_BRIDGE_ID: AtomicUsize = AtomicUsize::new(1);
    static BRIDGES: std::sync::LazyLock<DashMap<usize, NativeTerminalMacBridge>> =
        std::sync::LazyLock::new(DashMap::new);
    static VIEW_BY_SURFACE: std::sync::LazyLock<DashMap<String, usize>> =
        std::sync::LazyLock::new(DashMap::new);
    static BRIDGE_BY_SURFACE: std::sync::LazyLock<DashMap<String, usize>> =
        std::sync::LazyLock::new(DashMap::new);

    #[derive(Clone)]
    struct NativeTerminalMacBridge {
        app: AppHandle,
        state: Arc<NativeTerminalState>,
        surface_id: String,
    }

    #[derive(Debug, Default)]
    struct OxideNativeTerminalViewIvars {
        bridge_id: Cell<usize>,
    }

    define_class!(
        /// Native terminal drawing/input view. The view is deliberately tiny:
        /// terminal facts, parser/grid, input routing and resize ownership live
        /// in Rust state; React only supplies bounds/settings and fallback UI.
        #[unsafe(super = NSView)]
        #[thread_kind = MainThreadOnly]
        #[ivars = OxideNativeTerminalViewIvars]
        struct OxideNativeTerminalView;

        impl OxideNativeTerminalView {
            #[unsafe(method(isFlipped))]
            fn is_flipped(&self) -> bool {
                true
            }

            #[unsafe(method(isOpaque))]
            fn is_opaque(&self) -> bool {
                false
            }

            #[unsafe(method(acceptsFirstResponder))]
            fn accepts_first_responder(&self) -> bool {
                true
            }

            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, dirty_rect: NSRect) {
                draw_terminal_view(self.ivars().bridge_id.get(), dirty_rect);
            }

            #[unsafe(method(keyDown:))]
            fn key_down(&self, event: &NSEvent) {
                handle_key_down(self.ivars().bridge_id.get(), event);
            }

            #[unsafe(method(scrollWheel:))]
            fn scroll_wheel(&self, event: &NSEvent) {
                handle_scroll_wheel(self.ivars().bridge_id.get(), event);
            }

            #[unsafe(method(mouseDown:))]
            fn mouse_down(&self, _event: &NSEvent) {
                unsafe {
                    let view: &NSView = &*(self as *const Self).cast::<NSView>();
                    if let Some(window) = view.window() {
                        window.makeFirstResponder(Some(view.as_ref()));
                    }
                }
            }
        }
    );

    impl OxideNativeTerminalView {
        fn new(frame: NSRect, bridge_id: usize) -> Retained<Self> {
            let mtm = objc2_foundation::MainThreadMarker::new()
                .expect("native terminal view must be created on the main thread");
            let this = Self::alloc(mtm).set_ivars(OxideNativeTerminalViewIvars {
                bridge_id: Cell::new(bridge_id),
            });
            unsafe { msg_send![super(this), initWithFrame: frame] }
        }
    }

    pub(super) fn attach_view(
        app: &AppHandle,
        state: Arc<NativeTerminalState>,
        surface_id: &str,
        bounds: &NativeTerminalBounds,
    ) -> Result<(), String> {
        let window = app
            .get_webview_window("main")
            .ok_or_else(|| "main WebView window is not available".to_string())?;
        let bridge_id = NEXT_BRIDGE_ID.fetch_add(1, Ordering::Relaxed);
        BRIDGES.insert(
            bridge_id,
            NativeTerminalMacBridge {
                app: app.clone(),
                state,
                surface_id: surface_id.to_string(),
            },
        );
        BRIDGE_BY_SURFACE.insert(surface_id.to_string(), bridge_id);
        let surface_id_for_view = surface_id.to_string();
        let bounds_for_view = bounds.clone();

        window
            .with_webview(move |platform| unsafe {
                // Mount inside the WKWebView NSView, not the NSWindow contentView.
                // getBoundingClientRect() is relative to the WebView viewport; adding
                // the native terminal to the window contentView makes those coordinates
                // drift and can cover the entire React app.
                let webview: &NSView = &*(platform.inner().cast::<NSView>());
                let frame = appkit_frame_from_css_bounds(webview, &bounds_for_view);
                log_bounds(surface_id_for_view.as_str(), "attach", webview, &bounds_for_view, frame);
                let view = OxideNativeTerminalView::new(frame, bridge_id);
                let ns_view: &NSView = &*(Retained::as_ptr(&view).cast::<NSView>());
                ns_view.setWantsLayer(true);
                if let Some(layer) = ns_view.layer() {
                    layer.setOpaque(false);
                    layer.setBackgroundColor(None);
                }
                webview.addSubview(ns_view);
                let ns_window = &*(platform.ns_window().cast::<NSWindow>());
                ns_window.makeFirstResponder(Some(ns_view.as_ref()));
                VIEW_BY_SURFACE.insert(
                    surface_id_for_view.clone(),
                    Retained::as_ptr(&view) as usize,
                );
                let _ = Retained::into_raw(view);
            })
            .map_err(|e| format!("failed to attach native terminal view: {}", e))
    }

    pub(super) fn update_bounds(
        app: &AppHandle,
        surface_id: &str,
        bounds: &NativeTerminalBounds,
    ) -> Result<(), String> {
        let Some(view_ptr) = VIEW_BY_SURFACE.get(surface_id).map(|entry| *entry) else {
            return Ok(());
        };
        let bounds = bounds.clone();
        let surface_id_for_log = surface_id.to_string();
        app.run_on_main_thread(move || unsafe {
            let view = &*(view_ptr as *mut NSView);
            if let Some(superview) = view.superview() {
                let frame = appkit_frame_from_css_bounds(&superview, &bounds);
                log_bounds(&surface_id_for_log, "update_bounds", &superview, &bounds, frame);
                view.setFrame(frame);
                if bounds.width <= 0.0 || bounds.height <= 0.0 {
                    let _: () = msg_send![view, setHidden: true];
                }
            } else {
                view.setFrame(NSRect::new(
                    NSPoint::new(bounds.x, bounds.y),
                    NSSize::new(bounds.width, bounds.height),
                ));
            }
            view.setNeedsDisplay(true);
        })
        .map_err(|e| format!("failed to update native terminal bounds: {}", e))
    }

    pub(super) fn update_visibility(
        app: &AppHandle,
        surface_id: &str,
        visible: bool,
    ) -> Result<(), String> {
        let Some(view_ptr) = VIEW_BY_SURFACE.get(surface_id).map(|entry| *entry) else {
            return Ok(());
        };
        let surface_id_for_log = surface_id.to_string();
        app.run_on_main_thread(move || unsafe {
            let view = &*(view_ptr as *mut NSView);
            let _: () = msg_send![view, setHidden: !visible];
            tracing::debug!(
                surface_id = surface_id_for_log,
                visible,
                "native terminal AppKit view visibility updated"
            );
            if visible {
                view.setNeedsDisplay(true);
            }
        })
        .map_err(|e| format!("failed to update native terminal visibility: {}", e))
    }

    pub(super) fn focus(app: &AppHandle, surface_id: &str) -> Result<(), String> {
        let Some(view_ptr) = VIEW_BY_SURFACE.get(surface_id).map(|entry| *entry) else {
            return Ok(());
        };
        app.run_on_main_thread(move || unsafe {
            let view = &*(view_ptr as *mut NSView);
            if let Some(window) = view.window() {
                window.makeFirstResponder(Some(view.as_ref()));
            }
        })
        .map_err(|e| format!("failed to focus native terminal: {}", e))
    }

    pub(super) fn detach(app: &AppHandle, surface_id: &str) {
        let view_ptr = VIEW_BY_SURFACE.remove(surface_id).map(|(_, ptr)| ptr);
        if let Some((_, bridge_id)) = BRIDGE_BY_SURFACE.remove(surface_id) {
            BRIDGES.remove(&bridge_id);
        }
        if let Some(view_ptr) = view_ptr {
            let _ = app.run_on_main_thread(move || unsafe {
                let view = &*(view_ptr as *mut NSView);
                view.removeFromSuperview();
                drop(Retained::from_raw(view_ptr as *mut OxideNativeTerminalView));
            });
        }
    }

    pub(super) fn request_redraw(app: &AppHandle, surface_id: &str) {
        let Some(view_ptr) = VIEW_BY_SURFACE.get(surface_id).map(|entry| *entry) else {
            return;
        };
        let _ = app.run_on_main_thread(move || unsafe {
            let view = &*(view_ptr as *mut NSView);
            view.setNeedsDisplay(true);
        });
    }

    fn appkit_frame_from_css_bounds(
        content_view: &NSView,
        bounds: &NativeTerminalBounds,
    ) -> NSRect {
        let parent_bounds = content_view.bounds();
        let parent_width = parent_bounds.size.width.max(1.0);
        let parent_height = parent_bounds.size.height.max(1.0);
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        }
        let width = bounds.width.clamp(1.0, parent_width);
        let height = bounds.height.clamp(1.0, parent_height);
        let x = bounds.x.clamp(0.0, (parent_width - width).max(0.0));
        let y = (parent_height - bounds.y - height).clamp(0.0, (parent_height - height).max(0.0));
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
    }

    fn log_bounds(
        surface_id: &str,
        phase: &str,
        content_view: &NSView,
        bounds: &NativeTerminalBounds,
        frame: NSRect,
    ) {
        let parent_bounds = content_view.bounds();
        tracing::debug!(
            surface_id,
            phase,
            css_x = bounds.x,
            css_y = bounds.y,
            css_width = bounds.width,
            css_height = bounds.height,
            css_dpr = bounds.dpr,
            appkit_x = frame.origin.x,
            appkit_y = frame.origin.y,
            appkit_width = frame.size.width,
            appkit_height = frame.size.height,
            parent_width = parent_bounds.size.width,
            parent_height = parent_bounds.size.height,
            "native terminal bounds converted"
        );
    }

    fn draw_terminal_view(bridge_id: usize, dirty_rect: NSRect) {
        let Some(bridge) = BRIDGES.get(&bridge_id).map(|entry| entry.clone()) else {
            return;
        };
        let Some(surface) = bridge.state.surfaces.get(&bridge.surface_id) else {
            return;
        };

        let metrics = surface.runtime.lock().font_metrics.clone();
        let font = font_for_surface(&surface.font, &metrics);
        // Do not fill the whole native view here. The React host owns the pane
        // background. Keeping this view transparent prevents an experimental
        // frame bug from visually covering the rest of the WebView UI.
        let _ = dirty_rect;

        let runtime = surface.runtime.lock();
        let styled_rows = runtime.visible_styled_rows();
        let (cursor_row, cursor_col) = runtime.cursor_position();
        let cursor_shape = runtime.cursor_shape();
        let cursor = color_from_hex(&surface.theme.cursor)
            .unwrap_or_else(|| NSColor::colorWithSRGBRed_green_blue_alpha(0.6, 0.8, 1.0, 1.0));

        for row in styled_rows.iter() {
            let y = row.visible_row as f64 * metrics.cell_height_px;
            for run in &row.cells {
                // Do not let CoreText/AppKit lay out terminal text. It is only
                // allowed to paint the glyph for this cell/run. The terminal
                // grid is authoritative: x = col * cell_width, y = row *
                // cell_height. This keeps ASCII columns, CJK wide cells, Nerd
                // Font fallback glyphs, p10k prompts, and the cursor aligned.
                let x = run.start_col as f64 * metrics.cell_width_px;
                let width = run.width_cols.max(1) as f64 * metrics.cell_width_px;
                if let Some(bg_hex) = &run.bg {
                    if let Some(bg_color) = color_from_hex(bg_hex) {
                        bg_color.setFill();
                        NSRectFill(NSRect::new(
                            NSPoint::new(x, y),
                            NSSize::new(width, metrics.cell_height_px),
                        ));
                    }
                }

                if !run.text.is_empty() {
                    let fg = run
                        .fg
                        .as_deref()
                        .and_then(color_from_hex)
                        .or_else(|| color_from_hex(&surface.theme.foreground))
                        .unwrap_or_else(|| {
                            NSColor::colorWithSRGBRed_green_blue_alpha(0.85, 0.9, 1.0, 1.0)
                        });
                    draw_text_run(&run.text, x, y + metrics.baseline_y_px, &font, &fg);
                }
            }
        }

        draw_cursor(cursor_shape, cursor_row, cursor_col, &metrics, &cursor);
    }

    fn draw_text_run(text: &str, x: f64, baseline_y: f64, font: &NSFont, color: &NSColor) {
        let ns_text = NSString::from_str(text);
        let font_obj: &AnyObject = unsafe { &*(font as *const NSFont).cast::<AnyObject>() };
        let color_obj: &AnyObject = unsafe { &*(color as *const NSColor).cast::<AnyObject>() };
        let attrs = unsafe {
            NSDictionary::<NSAttributedStringKey, AnyObject>::from_slices(
                &[NSFontAttributeName, NSForegroundColorAttributeName],
                &[font_obj, color_obj],
            )
        };
        unsafe {
            // AppKit text drawing takes a top-left point, while terminal grid
            // metrics are easier to reason about as a baseline. Convert through
            // the resolved font ascent so glyphs and cursor share one cell model.
            ns_text.drawAtPoint_withAttributes(
                NSPoint::new(x, (baseline_y - font.ascender()).round()),
                Some(&attrs),
            );
        }
    }

    fn font_for_surface(
        font: &super::NativeTerminalFont,
        _metrics: &NativeTerminalFontMetrics,
    ) -> Retained<NSFont> {
        resolve_native_font(font, font.size.max(1.0))
            .map(|resolved| resolved.font)
            .or_else(|| NSFont::userFixedPitchFontOfSize(font.size.max(1.0)))
            .unwrap_or_else(|| NSFont::monospacedSystemFontOfSize_weight(font.size.max(1.0), 0.0))
    }

    fn draw_cursor(
        shape: super::CursorShape,
        row: usize,
        col: usize,
        metrics: &NativeTerminalFontMetrics,
        color: &NSColor,
    ) {
        if matches!(shape, super::CursorShape::Hidden) {
            return;
        }
        let x = col as f64 * metrics.cell_width_px;
        let y = row as f64 * metrics.cell_height_px;
        color.setFill();

        let rect = match shape {
            super::CursorShape::Beam => NSRect::new(
                NSPoint::new(x, y),
                NSSize::new(
                    2.0_f64.max(metrics.cell_width_px * 0.14),
                    metrics.cell_height_px,
                ),
            ),
            super::CursorShape::Underline => NSRect::new(
                NSPoint::new(
                    x,
                    y + metrics.cell_height_px - 2.0_f64.max(metrics.cell_height_px * 0.12),
                ),
                NSSize::new(
                    metrics.cell_width_px,
                    2.0_f64.max(metrics.cell_height_px * 0.12),
                ),
            ),
            super::CursorShape::HollowBlock | super::CursorShape::Block => NSRect::new(
                NSPoint::new(x, y),
                NSSize::new(metrics.cell_width_px, metrics.cell_height_px),
            ),
            super::CursorShape::Hidden => return,
        };
        NSRectFill(rect);
    }

    fn color_from_hex(hex: &str) -> Option<Retained<NSColor>> {
        let trimmed = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
        if trimmed.len() != 6 {
            return None;
        }
        let value = u32::from_str_radix(trimmed, 16).ok()?;
        let r = ((value >> 16) & 0xff) as f64 / 255.0;
        let g = ((value >> 8) & 0xff) as f64 / 255.0;
        let b = (value & 0xff) as f64 / 255.0;
        Some(NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0))
    }

    fn handle_key_down(bridge_id: usize, event: &NSEvent) {
        let Some(bridge) = BRIDGES.get(&bridge_id).map(|entry| entry.clone()) else {
            return;
        };
        let Some(surface) = bridge.state.surfaces.get(&bridge.surface_id) else {
            return;
        };
        let terminal_type = surface.terminal_type;
        let session_id = surface.session_id.clone();
        let (bracketed_paste, _) = surface.runtime.lock().mode_snapshot();
        let is_alt_screen = surface.runtime.lock().is_alt_screen();
        drop(surface);

        let key_code = event.keyCode();
        let modifiers = event.modifierFlags();
        let command = modifiers.contains(NSEventModifierFlags::Command);
        let control = modifiers.contains(NSEventModifierFlags::Control);

        if key_code == 116 && !is_alt_screen {
            if let Some(surface) = bridge.state.surfaces.get(&bridge.surface_id) {
                surface.runtime.lock().page_up();
            }
            request_redraw(&bridge.app, &bridge.surface_id);
            return;
        }
        if key_code == 121 && !is_alt_screen {
            if let Some(surface) = bridge.state.surfaces.get(&bridge.surface_id) {
                surface.runtime.lock().page_down();
            }
            request_redraw(&bridge.app, &bridge.surface_id);
            return;
        }

        let bytes = if command && key_code == 9 {
            pasteboard_text().map(|text| {
                if bracketed_paste {
                    let mut bytes = Vec::with_capacity(text.len() + 12);
                    bytes.extend_from_slice(b"\x1b[200~");
                    bytes.extend_from_slice(text.as_bytes());
                    bytes.extend_from_slice(b"\x1b[201~");
                    bytes
                } else {
                    text.into_bytes()
                }
            })
        } else {
            match key_code {
                36 => Some(b"\r".to_vec()),
                48 => Some(b"\t".to_vec()),
                51 => Some(vec![0x7f]),
                53 => Some(vec![0x1b]),
                115 => Some(b"\x1b[H".to_vec()),
                119 => Some(b"\x1b[F".to_vec()),
                116 => Some(b"\x1b[5~".to_vec()),
                121 => Some(b"\x1b[6~".to_vec()),
                123 => Some(b"\x1b[D".to_vec()),
                124 => Some(b"\x1b[C".to_vec()),
                125 => Some(b"\x1b[B".to_vec()),
                126 => Some(b"\x1b[A".to_vec()),
                _ if command => None,
                _ => event.characters().and_then(|chars| {
                    let text = chars.to_string();
                    if text.is_empty() {
                        return None;
                    }
                    if control {
                        let ch = text.chars().next()?;
                        if ch.is_ascii_alphabetic() {
                            return Some(vec![(ch.to_ascii_lowercase() as u8) & 0x1f]);
                        }
                    }
                    Some(text.into_bytes())
                }),
            }
        };

        if let Some(bytes) = bytes {
            let app = bridge.app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    write_existing_session(&app, terminal_type, &session_id, bytes).await
                {
                    tracing::warn!(error, "failed to write native terminal input");
                }
            });
        }
    }

    fn pasteboard_text() -> Option<String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let value = unsafe { pasteboard.stringForType(NSPasteboardTypeString) }?;
        Some(value.to_string())
    }

    fn handle_scroll_wheel(bridge_id: usize, event: &NSEvent) {
        let Some(bridge) = BRIDGES.get(&bridge_id).map(|entry| entry.clone()) else {
            return;
        };
        let delta = event.scrollingDeltaY();
        if delta.abs() < 1.0 {
            return;
        }

        if let Some(surface) = bridge.state.surfaces.get(&bridge.surface_id) {
            let mut runtime = surface.runtime.lock();
            if runtime.is_alt_screen() {
                let terminal_type = surface.terminal_type;
                let session_id = surface.session_id.clone();
                let repeats = (delta.abs() / 16.0).ceil().clamp(1.0, 6.0) as usize;
                let seq = if delta > 0.0 { b"\x1b[A" } else { b"\x1b[B" };
                let mut bytes = Vec::with_capacity(seq.len() * repeats);
                for _ in 0..repeats {
                    bytes.extend_from_slice(seq);
                }
                let app = bridge.app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = write_existing_session(&app, terminal_type, &session_id, bytes).await;
                });
            } else {
                let rows =
                    delta.signum() as i32 * (delta.abs() / 16.0).ceil().clamp(1.0, 24.0) as i32;
                runtime.scroll_delta(rows);
            }
        }
        request_redraw(&bridge.app, &bridge.surface_id);
    }
}

#[tauri::command]
pub async fn native_terminal_attach(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    request: NativeTerminalAttachRequest,
) -> Result<NativeTerminalAttachResponse, String> {
    let (output_tx, replay_lines) = match request.terminal_type {
        NativeTerminalType::Terminal => get_remote_attach_source(&app, &request.session_id).await?,
        NativeTerminalType::LocalTerminal => {
            get_local_attach_source(&app, &request.session_id).await?
        }
    };

    let surface_id = format!("native-{}", Uuid::new_v4());
    let output_rx = output_tx.subscribe();
    let runtime = Arc::new(Mutex::new(NativeTerminalRuntime::new(
        &request.bounds,
        &request.font,
    )));
    let replay = encode_replay_lines(&replay_lines);
    runtime.lock().ingest(&replay);
    let (pump, output_bytes, dropped_output_frames) =
        spawn_lossy_output_drain(app.clone(), surface_id.clone(), runtime.clone(), output_rx);
    let (status, backend, message) = platform_attach_status();
    let initial_bounds = request.bounds.clone();

    state.insert(
        surface_id.clone(),
        NativeTerminalSurface {
            pane_id: request.pane_id,
            terminal_type: request.terminal_type,
            session_id: request.session_id,
            node_id: request.node_id,
            bounds: request.bounds,
            font: request.font,
            theme: request.theme,
            status: status.clone(),
            visible: true,
            output_bytes,
            dropped_output_frames,
            runtime,
            pump: Some(pump),
        },
    );

    #[cfg(target_os = "macos")]
    if status == NativeTerminalSurfaceStatus::Ready {
        if let Err(error) =
            platform_attach_view(&app, state.inner().clone(), &surface_id, &initial_bounds)
        {
            tracing::warn!(
                surface_id,
                error,
                "failed to attach native terminal AppKit view"
            );
            if let Some(mut surface) = state.surfaces.get_mut(&surface_id) {
                surface.status = NativeTerminalSurfaceStatus::Failed;
            }
            return Ok(NativeTerminalAttachResponse {
                surface_id,
                status: NativeTerminalSurfaceStatus::Failed,
                backend,
                message: Some(error),
            });
        }
        platform_request_redraw(&app, &surface_id);
    }

    Ok(NativeTerminalAttachResponse {
        surface_id,
        status,
        backend,
        message,
    })
}

#[tauri::command]
pub async fn native_terminal_get_snapshot(
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
) -> Result<NativeTerminalSnapshot, String> {
    let surface = state
        .surfaces
        .get(&surface_id)
        .ok_or_else(|| format!("Native terminal surface not found: {}", surface_id))?;
    let runtime = surface.runtime.lock();

    let lines = runtime.visible_lines();
    let styled_rows = runtime.visible_styled_rows();
    let (cursor_row, cursor_col) = runtime.cursor_position();
    let (bracketed_paste, alt_screen) = runtime.mode_snapshot();
    let viewport = runtime.viewport_snapshot();

    Ok(NativeTerminalSnapshot {
        surface_id,
        pane_id: surface.pane_id.clone(),
        node_id: surface.node_id.clone(),
        visible: surface.visible,
        bounds: surface.bounds.clone(),
        status: surface.status.clone(),
        columns: runtime.columns,
        rows: runtime.rows,
        revision: runtime.revision,
        parsed_bytes: runtime.parsed_bytes,
        output_bytes: surface.output_bytes.load(Ordering::Relaxed),
        dropped_output_frames: surface.dropped_output_frames.load(Ordering::Relaxed),
        lines,
        styled_rows,
        cursor_row,
        cursor_col,
        bracketed_paste,
        alt_screen,
        active_buffer: viewport.active_buffer,
        scrollback_len: viewport.scrollback_len,
        viewport_rows: viewport.viewport_rows,
        viewport_top: viewport.viewport_top,
        follow_tail: viewport.follow_tail,
        pinned_to_bottom: viewport.pinned_to_bottom,
        can_scroll_up: viewport.can_scroll_up,
        can_scroll_down: viewport.can_scroll_down,
        font_metrics: runtime.font_metrics.clone(),
    })
}

#[tauri::command]
pub async fn native_terminal_get_viewport_snapshot(
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
) -> Result<NativeTerminalSnapshot, String> {
    native_terminal_get_snapshot(state, surface_id).await
}

#[tauri::command]
pub async fn native_terminal_update_visibility(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    request: NativeTerminalUpdateVisibilityRequest,
) -> Result<(), String> {
    {
        let mut surface = state
            .surfaces
            .get_mut(&request.surface_id)
            .ok_or_else(|| format!("Native terminal surface not found: {}", request.surface_id))?;
        if surface.visible == request.visible {
            return Ok(());
        }
        surface.visible = request.visible;
        tracing::debug!(
            surface_id = request.surface_id.as_str(),
            pane_id = surface.pane_id.as_str(),
            session_id = surface.session_id.as_str(),
            visible = request.visible,
            "native terminal surface visibility state updated"
        );
    }
    platform_update_visibility(&app, &request.surface_id, request.visible)
}

#[tauri::command]
pub async fn native_terminal_scroll(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
    delta_rows: i32,
) -> Result<(), String> {
    let surface = state
        .surfaces
        .get(&surface_id)
        .ok_or_else(|| format!("Native terminal surface not found: {}", surface_id))?;
    surface.runtime.lock().scroll_delta(delta_rows);
    platform_request_redraw(&app, &surface_id);
    Ok(())
}

#[tauri::command]
pub async fn native_terminal_page_up(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
) -> Result<(), String> {
    let surface = state
        .surfaces
        .get(&surface_id)
        .ok_or_else(|| format!("Native terminal surface not found: {}", surface_id))?;
    surface.runtime.lock().page_up();
    platform_request_redraw(&app, &surface_id);
    Ok(())
}

#[tauri::command]
pub async fn native_terminal_page_down(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
) -> Result<(), String> {
    let surface = state
        .surfaces
        .get(&surface_id)
        .ok_or_else(|| format!("Native terminal surface not found: {}", surface_id))?;
    surface.runtime.lock().page_down();
    platform_request_redraw(&app, &surface_id);
    Ok(())
}

#[tauri::command]
pub async fn native_terminal_scroll_to_bottom(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
) -> Result<(), String> {
    let surface = state
        .surfaces
        .get(&surface_id)
        .ok_or_else(|| format!("Native terminal surface not found: {}", surface_id))?;
    surface.runtime.lock().scroll_to_bottom();
    platform_request_redraw(&app, &surface_id);
    Ok(())
}

#[tauri::command]
pub async fn native_terminal_update_bounds(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    request: NativeTerminalUpdateBoundsRequest,
) -> Result<(), String> {
    let platform_bounds = request.bounds.clone();
    let resize_target = {
        let mut surface = state
            .surfaces
            .get_mut(&request.surface_id)
            .ok_or_else(|| format!("Native terminal surface not found: {}", request.surface_id))?;
        surface.bounds = request.bounds;
        let grid_changed = surface
            .runtime
            .lock()
            .resize(&surface.bounds, &surface.font);
        let runtime = surface.runtime.lock();
        grid_changed.then(|| {
            (
                surface.terminal_type,
                surface.session_id.clone(),
                runtime.columns as u16,
                runtime.rows as u16,
            )
        })
    };

    if let Some((terminal_type, session_id, columns, rows)) = resize_target {
        resize_existing_session(&app, terminal_type, &session_id, columns, rows).await?;
    }
    platform_update_bounds(&app, &request.surface_id, &platform_bounds)?;
    Ok(())
}

#[tauri::command]
pub async fn native_terminal_focus(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
) -> Result<(), String> {
    if state.surfaces.contains_key(&surface_id) {
        platform_focus(&app, &surface_id)
    } else {
        Err(format!("Native terminal surface not found: {}", surface_id))
    }
}

#[tauri::command]
pub async fn native_terminal_detach(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    surface_id: String,
) -> Result<(), String> {
    if let Some(mut surface) = state.remove(&surface_id) {
        if let Some(pump) = surface.pump.take() {
            pump.abort();
        }
        tracing::debug!(
            surface_id,
            pane_id = surface.pane_id,
            session_id = surface.session_id,
            terminal_type = ?surface.terminal_type,
            node_id = ?surface.node_id,
            status = ?surface.status,
            output_bytes = surface.output_bytes.load(Ordering::Relaxed),
            dropped_output_frames = surface.dropped_output_frames.load(Ordering::Relaxed),
            "native terminal surface detached"
        );
    }
    platform_detach(&app, &surface_id);
    Ok(())
}

#[tauri::command]
pub async fn native_terminal_update_settings(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    request: NativeTerminalUpdateSettingsRequest,
) -> Result<(), String> {
    let resize_target = {
        let mut surface = state
            .surfaces
            .get_mut(&request.surface_id)
            .ok_or_else(|| format!("Native terminal surface not found: {}", request.surface_id))?;
        surface.font = request.font;
        surface.theme = request.theme;
        let grid_changed = surface
            .runtime
            .lock()
            .resize(&surface.bounds, &surface.font);
        let runtime = surface.runtime.lock();
        grid_changed.then(|| {
            (
                surface.terminal_type,
                surface.session_id.clone(),
                runtime.columns as u16,
                runtime.rows as u16,
            )
        })
    };

    if let Some((terminal_type, session_id, columns, rows)) = resize_target {
        resize_existing_session(&app, terminal_type, &session_id, columns, rows).await?;
    }
    platform_request_redraw(&app, &request.surface_id);
    Ok(())
}

#[tauri::command]
pub async fn native_terminal_write(
    app: AppHandle,
    state: State<'_, Arc<NativeTerminalState>>,
    request: NativeTerminalWriteRequest,
) -> Result<(), String> {
    if request.data.is_empty() {
        return Ok(());
    }

    let write_target = {
        let surface = state
            .surfaces
            .get(&request.surface_id)
            .ok_or_else(|| format!("Native terminal surface not found: {}", request.surface_id))?;
        (
            surface.terminal_type,
            surface.session_id.clone(),
            request.data,
        )
    };

    write_existing_session(&app, write_target.0, &write_target.1, write_target.2).await
}

async fn write_existing_session(
    app: &AppHandle,
    terminal_type: NativeTerminalType,
    session_id: &str,
    data: Vec<u8>,
) -> Result<(), String> {
    match terminal_type {
        NativeTerminalType::Terminal => {
            let registry = app
                .try_state::<Arc<SessionRegistry>>()
                .ok_or_else(|| "Session registry is not available".to_string())?;
            let tx = registry
                .get_cmd_tx(session_id)
                .ok_or_else(|| format!("Remote terminal session not found: {}", session_id))?;
            tx.send(SessionCommand::Data(data))
                .await
                .map_err(|e| format!("Failed to write to remote session: {}", e))
        }
        NativeTerminalType::LocalTerminal => {
            write_local_existing_session(app, session_id, &data).await
        }
    }
}

async fn resize_existing_session(
    app: &AppHandle,
    terminal_type: NativeTerminalType,
    session_id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    match terminal_type {
        NativeTerminalType::Terminal => {
            let registry = app
                .try_state::<Arc<SessionRegistry>>()
                .ok_or_else(|| "Session registry is not available".to_string())?;
            registry.resize(session_id, cols, rows).await
        }
        NativeTerminalType::LocalTerminal => {
            resize_local_existing_session(app, session_id, cols, rows).await
        }
    }
}

#[cfg(feature = "local-terminal")]
async fn write_local_existing_session(
    app: &AppHandle,
    session_id: &str,
    data: &[u8],
) -> Result<(), String> {
    let state = app
        .try_state::<Arc<crate::commands::local::LocalTerminalState>>()
        .ok_or_else(|| "Local terminal state is not available".to_string())?;
    state
        .registry
        .write_to_session(session_id, data)
        .await
        .map_err(|e| format!("Failed to write to local session: {}", e))
}

#[cfg(not(feature = "local-terminal"))]
async fn write_local_existing_session(
    _app: &AppHandle,
    session_id: &str,
    _data: &[u8],
) -> Result<(), String> {
    Err(format!(
        "Local terminal support is not enabled; cannot write to {}",
        session_id
    ))
}

#[cfg(feature = "local-terminal")]
async fn resize_local_existing_session(
    app: &AppHandle,
    session_id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let state = app
        .try_state::<Arc<crate::commands::local::LocalTerminalState>>()
        .ok_or_else(|| "Local terminal state is not available".to_string())?;
    state
        .registry
        .resize_session(session_id, cols, rows)
        .await
        .map_err(|e| format!("Failed to resize local session: {}", e))
}

#[cfg(not(feature = "local-terminal"))]
async fn resize_local_existing_session(
    _app: &AppHandle,
    session_id: &str,
    _cols: u16,
    _rows: u16,
) -> Result<(), String> {
    Err(format!(
        "Local terminal support is not enabled; cannot resize {}",
        session_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(width: f64, height: f64) -> NativeTerminalBounds {
        NativeTerminalBounds {
            x: 0.0,
            y: 0.0,
            width,
            height,
            dpr: 2.0,
        }
    }

    fn font() -> NativeTerminalFont {
        NativeTerminalFont {
            family: "monospace".to_string(),
            family_css: Some("\"JetBrains Mono\", \"Maple Mono NF CN\", monospace".to_string()),
            primary_family: Some("JetBrains Mono".to_string()),
            fallback_families: vec!["Maple Mono NF CN".to_string()],
            size: 14.0,
            line_height: 1.2,
        }
    }

    #[test]
    fn native_size_uses_font_metrics_and_clamps_minimums() {
        let size = native_size_from_bounds(&bounds(840.0, 336.0), &font());
        assert!(size.columns >= 90);
        assert!(size.rows >= 16);

        let tiny = native_size_from_bounds(&bounds(0.0, 0.0), &font());
        assert_eq!(tiny.columns, MIN_NATIVE_COLUMNS);
        assert_eq!(tiny.rows, MIN_NATIVE_ROWS);
    }

    #[test]
    fn native_font_stack_parser_keeps_ordered_candidates() {
        let parsed = parse_font_stack_for_test(
            "\"JetBrainsMono Nerd Font\", 'Maple Mono NF CN', ui-monospace, monospace",
        );
        assert_eq!(
            parsed,
            vec![
                "JetBrainsMono Nerd Font".to_string(),
                "Maple Mono NF CN".to_string(),
                "ui-monospace".to_string(),
                "monospace".to_string(),
            ]
        );
    }

    #[test]
    fn runtime_ingests_bytes_and_resizes_grid() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(840.0, 336.0), &font());
        let initial_columns = runtime.columns;
        let initial_rows = runtime.rows;

        runtime.ingest(b"hello\r\nworld");
        assert_eq!(runtime.parsed_bytes, 12);
        assert_eq!(runtime.revision, 1);
        assert_eq!(runtime.visible_lines()[0], "hello");
        assert_eq!(runtime.visible_lines()[1], "world");

        runtime.resize(&bounds(420.0, 168.0), &font());
        assert!(runtime.columns < initial_columns);
        assert!(runtime.rows < initial_rows);
        assert_eq!(runtime.revision, 2);
    }

    #[test]
    fn runtime_snapshot_preserves_grid_columns() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(840.0, 336.0), &font());
        runtime.ingest(b"a\x1b[10Gb");

        let line = runtime.visible_lines()[0].clone();
        assert!(line.starts_with("a        b"));
        let rows = runtime.visible_styled_rows();
        assert_eq!(rows.len(), runtime.rows);
        assert_eq!(rows[0].visible_row, 0);
        let b_run = rows[0]
            .cells
            .iter()
            .find(|run| run.text.contains('b'))
            .expect("b cell");
        assert_eq!(b_run.visible_row, 0);
        assert_eq!(b_run.start_col, 9);
        assert_eq!(b_run.width_cols, 1);
    }

    #[test]
    fn runtime_draw_model_keeps_ascii_cells_unmerged() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(840.0, 336.0), &font());
        runtime.ingest(b"abc");

        let row = runtime.visible_styled_rows().remove(0);
        assert_eq!(row.visible_row, 0);
        let row = row.cells;
        let a = row.iter().find(|run| run.text == "a").expect("a cell");
        let b = row.iter().find(|run| run.text == "b").expect("b cell");
        let c = row.iter().find(|run| run.text == "c").expect("c cell");
        assert_eq!(a.start_col, 0);
        assert_eq!(b.start_col, 1);
        assert_eq!(c.start_col, 2);
        assert_eq!(a.width_cols, 1);
        assert_eq!(b.width_cols, 1);
        assert_eq!(c.width_cols, 1);
    }

    #[test]
    fn runtime_draw_model_tracks_wide_cell_columns() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(840.0, 336.0), &font());
        runtime.ingest("中x".as_bytes());

        let row = runtime.visible_styled_rows().remove(0);
        assert_eq!(row.visible_row, 0);
        let row = row.cells;
        let wide = row
            .iter()
            .find(|run| run.text.contains('中'))
            .expect("wide run");
        assert_eq!(wide.start_col, 0);
        assert!(wide.width_cols >= 2);
        let x = row
            .iter()
            .find(|run| run.text.contains('x'))
            .expect("x run");
        assert_eq!(x.start_col, 2);
    }

    #[test]
    fn runtime_snapshot_exports_basic_styles() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(840.0, 336.0), &font());
        runtime.ingest(b"\x1b[1;31mred\x1b[0m");

        let row = runtime.visible_styled_rows().remove(0);
        assert_eq!(row.visible_row, 0);
        let red_cells: Vec<_> = row
            .cells
            .iter()
            .filter(|run| matches!(run.text.as_str(), "r" | "e" | "d"))
            .collect();
        assert_eq!(red_cells.len(), 3);
        for cell in red_cells {
            assert_eq!(cell.fg.as_deref(), Some("#cd3131"));
            assert!(cell.bold);
            assert_eq!(cell.width_cols, 1);
        }
    }

    #[test]
    fn runtime_viewport_scroll_pins_and_restores_tail_follow() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(420.0, 120.0), &font());
        for i in 0..40 {
            runtime.ingest(format!("line-{i}\r\n").as_bytes());
        }

        let initial = runtime.viewport_snapshot();
        assert_eq!(initial.active_buffer, NativeTerminalActiveBuffer::Normal);
        assert!(initial.pinned_to_bottom);
        assert!(initial.follow_tail);
        assert!(initial.can_scroll_up);

        runtime.scroll_delta(4);
        let scrolled = runtime.viewport_snapshot();
        assert!(!scrolled.pinned_to_bottom);
        assert!(!scrolled.follow_tail);
        assert!(scrolled.can_scroll_down);

        let before_output = scrolled.viewport_top;
        runtime.ingest(b"new-tail\r\n");
        let after_output = runtime.viewport_snapshot();
        assert!(!after_output.pinned_to_bottom);
        assert!(!after_output.follow_tail);
        assert_eq!(after_output.viewport_top, before_output);

        runtime.scroll_to_bottom();
        let bottom = runtime.viewport_snapshot();
        assert!(bottom.pinned_to_bottom);
        assert!(bottom.follow_tail);
    }

    #[test]
    fn runtime_page_navigation_clamps_to_history() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(420.0, 120.0), &font());
        for i in 0..80 {
            runtime.ingest(format!("line-{i}\r\n").as_bytes());
        }

        runtime.page_up();
        let page_up = runtime.viewport_snapshot();
        assert!(!page_up.pinned_to_bottom);
        assert!(page_up.can_scroll_down);

        for _ in 0..20 {
            runtime.page_up();
        }
        let top = runtime.viewport_snapshot();
        assert!(top.can_scroll_down);
        assert!(!top.can_scroll_up);

        for _ in 0..20 {
            runtime.page_down();
        }
        let bottom = runtime.viewport_snapshot();
        assert!(bottom.pinned_to_bottom);
        assert!(bottom.follow_tail);
    }

    #[test]
    fn runtime_viewport_rows_are_continuous_after_page_up() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(420.0, 120.0), &font());
        for i in 0..300 {
            runtime.ingest(format!("{i}\r\n").as_bytes());
        }

        runtime.page_up();
        let viewport = runtime.viewport_snapshot();
        let rows = runtime.visible_styled_rows();

        assert_eq!(rows.len(), viewport.viewport_rows);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row.visible_row, index);
            for cell in &row.cells {
                assert_eq!(cell.visible_row, index);
            }
        }
        assert!(viewport.viewport_top <= viewport.scrollback_len);
        assert!(!viewport.pinned_to_bottom);
    }

    #[test]
    fn runtime_alt_screen_disables_history_viewport() {
        let mut runtime = NativeTerminalRuntime::new(&bounds(420.0, 120.0), &font());
        for i in 0..20 {
            runtime.ingest(format!("line-{i}\r\n").as_bytes());
        }
        runtime.ingest(b"\x1b[?1049h");
        runtime.scroll_delta(5);

        let viewport = runtime.viewport_snapshot();
        assert_eq!(
            viewport.active_buffer,
            NativeTerminalActiveBuffer::AltScreen
        );
        assert_eq!(viewport.viewport_top, 0);
        assert!(!viewport.can_scroll_up);
        assert!(!viewport.can_scroll_down);
    }
}
