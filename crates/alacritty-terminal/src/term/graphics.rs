//! Ordered grid changes and sparse anchors for terminal graphics consumers.

use crate::grid::{Dimensions, Grid};
use crate::index::{Column, Line, Point};
use crate::term::cell::{Cell, Flags};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use vte::ansi::{Color, NamedColor};

mod placeholder;
pub use placeholder::PlaceholderRun;

pub const SIXEL_MAX_COLORS: usize = 256;
pub const SIXEL_MAX_DIMENSION: u32 = 1_000_000;

#[derive(Clone, Debug)]
pub enum GraphicsGridChange {
    Erase { alternate: bool, top: i32, bottom: i32, left: usize, right: usize },
    ShiftColumns { alternate: bool, line: i32, start: usize, end: usize, delta: i32 },
    SixelPaletteReset,
    Scroll {
        alternate: bool,
        start: i32,
        end: i32,
        delta: i32,
        topmost: i32,
        history: bool,
    },
    Screen {
        alternate: bool,
    },
    Clear {
        alternate: bool,
        history: bool,
    },
    Reset,
    TrimHistory {
        topmost: i32,
    },
    Reflow {
        alternate: bool,
        anchors: Vec<(u64, Point)>,
        columns: usize,
    },
    Placeholders {
        alternate: bool,
        line: i32,
        runs: Vec<PlaceholderRun>,
    },
    DropImages {
        alternate: bool,
    },
}

#[derive(Default)]
struct State {
    changes: Vec<GraphicsGridChange>,
    anchors: HashMap<(bool, u64), Point>,
    dirty_rows: HashSet<(bool, i32)>,
    alternate: bool,
    queued_bytes: usize,
    sixel_background: Option<Color>,
    sixel_colors: Option<usize>,
    sixel_size: Option<[u32; 2]>,
}

#[derive(Default)]
struct Events {
    state: Mutex<State>,
    placeholders: AtomicBool,
    anchored: AtomicBool,
    dirty_line: AtomicU64,
    rescan: AtomicBool,
    byte_limit: AtomicUsize,
    sixel: AtomicBool,
    sixel_modes: AtomicUsize,
    sixel_screens: AtomicUsize,
    fixed_sixel_screens: AtomicUsize,
}

/// Shared only between a terminal model and its ordered graphics parser.
#[derive(Clone, Default)]
pub struct GraphicsGridEvents(Arc<Events>);

impl GraphicsGridEvents {
    pub fn set_sixel_screens(&self, active: [bool; 2], fixed: [bool; 2]) {
        self.0.sixel_screens.store(
            usize::from(active[0]) | (usize::from(active[1]) << 1),
            Ordering::Relaxed,
        );
        self.0.fixed_sixel_screens.store(
            usize::from(fixed[0]) | (usize::from(fixed[1]) << 1),
            Ordering::Relaxed,
        );
    }

    pub(super) fn has_fixed_sixel(&self, alternate: bool) -> bool {
        self.0.fixed_sixel_screens.load(Ordering::Relaxed) & (1 << usize::from(alternate)) != 0
    }

    pub(super) fn has_sixel(&self, alternate: bool) -> bool {
        self.0.sixel_screens.load(Ordering::Relaxed) & (1 << usize::from(alternate)) != 0
    }

    pub(super) fn erase_sixel(
        &self,
        alternate: bool,
        top: i32,
        bottom: i32,
        left: usize,
        right: usize,
    ) {
        if top < bottom && left < right && self.has_sixel(alternate) {
            self.record(GraphicsGridChange::Erase {
                alternate,
                top,
                bottom,
                left,
                right,
            });
        }
    }

    pub(super) fn shift_sixel(
        &self,
        alternate: bool,
        line: i32,
        start: usize,
        end: usize,
        delta: i32,
    ) {
        if start < end && delta != 0 && self.has_sixel(alternate) {
            self.record(GraphicsGridChange::ShiftColumns {
                alternate,
                line,
                start,
                end,
                delta,
            });
        }
    }
    pub fn sixel_limits(&self) -> (usize, [u32; 2]) {
        let state = self.0.state.lock();
        (
            state.sixel_colors.unwrap_or(SIXEL_MAX_COLORS),
            state.sixel_size.unwrap_or([SIXEL_MAX_DIMENSION; 2]),
        )
    }

    pub(super) fn sixel_attribute(
        &self,
        item: u16,
        action: u16,
        value: [u16; 2],
    ) -> Option<String> {
        if !self.sixel_enabled() {
            return None;
        }
        let mut state = self.0.state.lock();
        let mut status = 0;
        match (item, action) {
            (1, 2) => state.sixel_colors = None,
            (1, 3) if value[0] > 0 && usize::from(value[0]) <= SIXEL_MAX_COLORS => {
                state.sixel_colors = Some(value[0] as usize)
            }
            (2, 2) => state.sixel_size = None,
            (2, 3) if value[0] > 0 && value[1] > 0 => {
                state.sixel_size = Some([u32::from(value[0]), u32::from(value[1])])
            }
            (1 | 2, 1 | 4) => {}
            (1 | 2, 3) => status = 3,
            (1 | 2, _) => status = 2,
            _ => status = 1,
        }
        let result = match item {
            1 => if action == 4 {
                SIXEL_MAX_COLORS
            } else {
                state.sixel_colors.unwrap_or(SIXEL_MAX_COLORS)
            }
            .to_string(),
            2 => {
                let [w, h] = if action == 4 {
                    [SIXEL_MAX_DIMENSION; 2]
                } else {
                    state.sixel_size.unwrap_or([SIXEL_MAX_DIMENSION; 2])
                };
                format!("{w};{h}")
            }
            _ => "0".to_owned(),
        };
        drop(state);
        if item == 1 && action == 2 {
            self.record(GraphicsGridChange::SixelPaletteReset);
        }
        Some(format!("\x1b[?{item};{status};{result}S"))
    }
    pub fn set_sixel_enabled(&self, enabled: bool) {
        self.0.sixel.store(enabled, Ordering::Relaxed);
    }

    pub fn sixel_enabled(&self) -> bool {
        self.0.sixel.load(Ordering::Relaxed)
    }

    pub fn sixel_mode(&self, mode: u16) -> Option<bool> {
        if !self.sixel_enabled() {
            return None;
        }
        let flags = self.0.sixel_modes.load(Ordering::Relaxed);
        match mode {
            80 => Some(flags & 1 != 0),
            1070 => Some(flags & 2 == 0),
            8452 => Some(flags & 4 != 0),
            _ => None,
        }
    }

    pub(super) fn set_sixel_mode(&self, mode: u16, set: bool) -> bool {
        if !self.sixel_enabled() {
            return false;
        }
        let (flag, set) = match mode {
            80 => (1, set),
            1070 => (2, !set),
            8452 => (4, set),
            _ => return false,
        };
        if set {
            self.0.sixel_modes.fetch_or(flag, Ordering::Relaxed);
        } else {
            self.0.sixel_modes.fetch_and(!flag, Ordering::Relaxed);
        }
        true
    }

    pub fn capture_sixel_background(&self, background: Color) {
        if self.sixel_enabled() {
            self.0.state.lock().sixel_background = Some(background);
        }
    }

    pub fn sixel_background(&self) -> Color {
        self.0
            .state
            .lock()
            .sixel_background
            .unwrap_or(Color::Named(NamedColor::Background))
    }
    pub fn drain(&self) -> Vec<GraphicsGridChange> {
        let mut state = self.0.state.lock();
        state.queued_bytes = 0;
        std::mem::take(&mut state.changes)
    }

    pub fn set_byte_limit(&self, bytes: usize) {
        self.0.byte_limit.store(bytes, Ordering::Relaxed);
    }
    pub fn invalidate_placeholders(&self) {
        self.0.rescan.store(true, Ordering::Relaxed);
    }

    pub fn set_anchor(&self, alternate: bool, serial: u64, point: Option<Point>) {
        let mut state = self.0.state.lock();
        if let Some(point) = point {
            state.anchors.insert((alternate, serial), point);
        } else {
            state.anchors.remove(&(alternate, serial));
        }
        self.0
            .anchored
            .store(!state.anchors.is_empty(), Ordering::Relaxed);
    }

    pub(super) fn record(&self, change: GraphicsGridChange) {
        if matches!(change, GraphicsGridChange::Scroll { .. })
            && !self.has_placeholders()
            && !self.0.anchored.load(Ordering::Relaxed)
        {
            return;
        }
        self.0.dirty_line.store(0, Ordering::Relaxed);
        let mut state = self.0.state.lock();
        match &change {
            GraphicsGridChange::Erase {
                alternate,
                top,
                bottom,
                left,
                right,
            } => {
                if let Some(GraphicsGridChange::Erase {
                    alternate: screen,
                    top: a,
                    bottom: b,
                    left: x,
                    right: y,
                }) = state.changes.last_mut()
                    && screen == alternate
                {
                    // Only merge exact rectangular unions; an L-shaped union would erase extra pixels.
                    if a == top && b == bottom && *x <= *right && *left <= *y {
                        *x = (*x).min(*left);
                        *y = (*y).max(*right);
                        return;
                    }
                    if x == left && y == right && *a <= *bottom && *top <= *b {
                        *a = (*a).min(*top);
                        *b = (*b).max(*bottom);
                        return;
                    }
                }
            }
            GraphicsGridChange::Scroll {
                alternate,
                start,
                end,
                delta,
                topmost,
                history,
            } => {
                state.dirty_rows = state
                    .dirty_rows
                    .drain()
                    .filter_map(|(screen, mut line)| {
                        if screen == *alternate
                            && ((*history && *delta < 0) || (*start <= line && line < *end))
                        {
                            line = line.saturating_add(*delta);
                        }
                        (screen != *alternate || line >= *topmost).then_some((screen, line))
                    })
                    .collect();
                state.anchors.retain(|(screen, _), point| {
                    if screen != alternate {
                        return true;
                    }
                    if (*history && *delta < 0) || (*start <= point.line.0 && point.line.0 < *end) {
                        point.line.0 = point.line.0.saturating_add(*delta);
                    }
                    point.line.0 >= *topmost
                });
                if let Some(GraphicsGridChange::Scroll {
                    alternate: old_screen,
                    start: old_start,
                    end: old_end,
                    delta: old_delta,
                    topmost: old_top,
                    history: old_history,
                }) = state.changes.last_mut()
                    && old_screen == alternate
                    && old_history == history
                    && old_start == start
                    && old_end == end
                    && old_delta.signum() == delta.signum()
                {
                    *old_delta = old_delta.saturating_add(*delta);
                    *old_top = *topmost;
                    return;
                }
            }
            GraphicsGridChange::Screen { alternate } => {
                state.alternate = *alternate;
                if *alternate {
                    state.anchors.retain(|(screen, _), _| !screen);
                }
            }
            GraphicsGridChange::Clear { alternate, history } => state
                .anchors
                .retain(|(screen, _), point| screen != alternate || !*history || point.line.0 >= 0),
            GraphicsGridChange::Reset => {
                self.0.sixel_screens.store(0, Ordering::Relaxed);
                self.0.fixed_sixel_screens.store(0, Ordering::Relaxed);
                self.0.sixel_modes.store(0, Ordering::Relaxed);
                state.sixel_background = None;
                state.sixel_colors = None;
                state.sixel_size = None;
                state.anchors.clear();
                state.alternate = false;
            }
            GraphicsGridChange::TrimHistory { topmost } => state
                .anchors
                .retain(|(screen, _), point| *screen || point.line.0 >= *topmost),
            _ => {}
        }
        self.0
            .anchored
            .store(!state.anchors.is_empty(), Ordering::Relaxed);
        let extra = std::mem::size_of::<GraphicsGridChange>()
            + match &change {
                GraphicsGridChange::Placeholders { runs, .. } => {
                    runs.capacity() * std::mem::size_of::<PlaceholderRun>()
                }
                GraphicsGridChange::Reflow { anchors, .. } => {
                    anchors.capacity() * std::mem::size_of::<(u64, Point)>()
                }
                _ => 0,
            };
        let limit = self.0.byte_limit.load(Ordering::Relaxed);
        if limit != 0 && state.queued_bytes.saturating_add(extra) > limit {
            state.changes.clear();
            state.anchors.clear();
            self.0.anchored.store(false, Ordering::Relaxed);
            let alternate = state.alternate;
            state
                .changes
                .push(GraphicsGridChange::DropImages { alternate });
            state.queued_bytes = std::mem::size_of::<GraphicsGridChange>();
        } else {
            state.queued_bytes += extra;
            state.changes.push(change);
        }
    }

    pub(super) fn anchors(&self, alternate: bool) -> Vec<(u64, Point)> {
        self.0
            .state
            .lock()
            .anchors
            .iter()
            .filter_map(|((screen, id), point)| (*screen == alternate).then_some((*id, *point)))
            .collect()
    }

    pub(super) fn reflow(
        &self,
        alternate: bool,
        grid: &Grid<Cell>,
        old: &[(u64, Option<(i32, usize)>)],
        cursor_group: i32,
    ) {
        let new_cursor = logical_positions(grid, &[grid.cursor.point])[0].map_or(0, |p| p.0);
        let shifted = old
            .iter()
            .map(|(_, point)| {
                point.map(|(group, offset)| (group + new_cursor - cursor_group, offset))
            })
            .collect::<Vec<_>>();
        let mapped = physical_positions(grid, &shifted);
        let mut anchors = Vec::new();
        for ((id, _), point) in old.iter().zip(mapped) {
            self.set_anchor(alternate, *id, point);
            if let Some(point) = point {
                anchors.push((*id, point));
            }
        }
        self.record(GraphicsGridChange::Reflow {
            alternate,
            anchors,
            columns: grid.columns(),
        });
        if self.has_placeholders() {
            let mut state = self.0.state.lock();
            for line in grid.topmost_line().0..=grid.bottommost_line().0 {
                state.dirty_rows.insert((alternate, line));
            }
        }
    }

    pub(super) fn enable_placeholders(&self) {
        self.0.placeholders.store(true, Ordering::Relaxed);
    }
    pub(super) fn has_placeholders(&self) -> bool {
        self.0.placeholders.load(Ordering::Relaxed)
    }
    pub(super) fn dirty_row(&self, line: i32) {
        if self.has_placeholders() {
            let key = u64::from(line as u32) + 1;
            // A printable run dirties a row once, even when scalar glyphs share it.
            if self.0.dirty_line.load(Ordering::Relaxed) == key {
                return;
            }
            self.0.dirty_line.store(key, Ordering::Relaxed);
            let mut state = self.0.state.lock();
            let screen = state.alternate;
            state.dirty_rows.insert((screen, line));
        }
    }
}

impl<T> super::Term<T> {
    pub fn capture_graphics_placeholders(&self) {
        let Some(events) = &self.graphics_events else {
            return;
        };
        if !events.has_placeholders() {
            return;
        }
        events.0.dirty_line.store(0, Ordering::Relaxed);
        let mut dirty = std::mem::take(&mut events.0.state.lock().dirty_rows);
        let active = self.mode.contains(super::TermMode::ALT_SCREEN);
        if events.0.rescan.swap(false, Ordering::Relaxed) {
            for (screen, grid) in [(active, &self.grid), (!active, &self.inactive_grid)] {
                for line in grid.topmost_line().0..=grid.bottommost_line().0 {
                    dirty.insert((screen, line));
                }
            }
        }
        for (alternate, line) in dirty {
            let grid = if alternate == active {
                &self.grid
            } else {
                &self.inactive_grid
            };
            if line < grid.topmost_line().0 || line > grid.bottommost_line().0 {
                continue;
            }
            let runs = placeholder::decode_row(&grid[Line(line)]);
            events.record(GraphicsGridChange::Placeholders {
                alternate,
                line,
                runs,
            });
        }
    }
}

/// Resolve only registered anchors; no per-cell metadata or text copies are needed.
pub(super) fn logical_positions(grid: &Grid<Cell>, points: &[Point]) -> Vec<Option<(i32, usize)>> {
    let mut ordered = points.iter().copied().enumerate().collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|(_, p)| (p.line, p.column));
    let mut result = vec![None; points.len()];
    let mut pending = ordered.into_iter().peekable();
    let (mut group, mut offset) = (0, 0);
    for line in grid.topmost_line().0..=grid.bottommost_line().0 {
        let last = grid[Line(line)].cell(Column(grid.columns() - 1));
        let width =
            grid.columns() - usize::from(last.flags().contains(Flags::LEADING_WIDE_CHAR_SPACER));
        while pending.peek().is_some_and(|(_, p)| p.line.0 <= line) {
            let (index, point) = pending.next().unwrap();
            if point.line.0 == line {
                result[index] = Some((group, offset + point.column.0.min(width.saturating_sub(1))));
            }
        }
        if last.flags().contains(Flags::WRAPLINE) {
            offset += width;
        } else {
            group += 1;
            offset = 0;
        }
        if pending.peek().is_none() {
            break;
        }
    }
    result
}

fn physical_positions(grid: &Grid<Cell>, points: &[Option<(i32, usize)>]) -> Vec<Option<Point>> {
    let mut ordered = points
        .iter()
        .enumerate()
        .filter_map(|(i, point)| point.map(|point| (i, point)))
        .collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|(_, point)| *point);
    let mut pending = ordered.into_iter().peekable();
    let mut result = vec![None; points.len()];
    let (mut group, mut offset) = (0, 0);
    for line in grid.topmost_line().0..=grid.bottommost_line().0 {
        let last = grid[Line(line)].cell(Column(grid.columns() - 1));
        let width =
            grid.columns() - usize::from(last.flags().contains(Flags::LEADING_WIDE_CHAR_SPACER));
        let wrapped = last.flags().contains(Flags::WRAPLINE);
        while pending.peek().is_some_and(|(_, (g, column))| {
            *g < group || (*g == group && (*column < offset + width || !wrapped))
        }) {
            let (index, (g, column)) = pending.next().unwrap();
            if g == group {
                result[index] = Some(Point::new(
                    Line(line),
                    Column(column.saturating_sub(offset).min(width.saturating_sub(1))),
                ));
            }
        }
        if wrapped {
            offset += width;
        } else {
            group += 1;
            offset = 0;
        }
        if pending.peek().is_none() {
            break;
        }
    }
    result
}
