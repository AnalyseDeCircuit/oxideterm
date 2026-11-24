use std::sync::Arc;

use alacritty_terminal::{
    event::EventListener,
    grid::{Dimensions, Scroll},
    index::Line,
    term::{
        Term, TermDamage,
        cell::{Cell as AlacrittyCell, Flags},
        color::Colors,
    },
};

use crate::{
    TerminalAttrs, TerminalCell, TerminalCursorShape, TerminalGraphicsState, TerminalPalette,
    TerminalRow, TerminalSize, TerminalSnapshot, TerminalStyleOrigin,
    graphics::resolve_sixel_backgrounds,
    palette::{attrs_from_flags, style_colors_for_cell, style_origin_for_cell},
};

pub fn snapshot_from_term<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    palette: &TerminalPalette,
) -> TerminalSnapshot {
    snapshot_from_term_with_display_offset(
        term,
        size,
        graphics,
        palette,
        term.grid().display_offset(),
        size.rows,
        None,
    )
}

pub fn incremental_snapshot_from_term<T: EventListener>(
    term: &mut Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    palette: &TerminalPalette,
    previous: &TerminalSnapshot,
) -> TerminalSnapshot {
    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let display_offset = term.grid().display_offset().min(scrollback_lines);
    let shape_compatible = previous.cols == size.cols
        && previous.rows == size.rows
        && previous.display_offset == display_offset
        && previous.lines.len() == size.rows;

    let (dirty_rows, scroll_up) = match term.damage() {
        TermDamage::Full => (None, None),
        TermDamage::ScrollUp { lines, damage } => (
            Some(
                damage
                    .filter(|line| line.is_damaged() && line.line < size.rows)
                    .map(|line| line.line)
                    .collect::<Vec<_>>(),
            ),
            Some(lines),
        ),
        TermDamage::Partial(damage) => (
            Some(
                damage
                    .filter(|line| line.is_damaged() && line.line < size.rows)
                    .map(|line| line.line)
                    .collect::<Vec<_>>(),
            ),
            None,
        ),
    };
    term.reset_damage();

    if let (Some(scroll_up), Some(dirty_rows)) = (scroll_up, dirty_rows.as_deref())
        && shape_compatible
        && display_offset == 0
        && scroll_up < size.rows
    {
        return snapshot_from_term_after_scroll_up(
            term,
            size,
            graphics,
            palette,
            scrollback_lines,
            scroll_up,
            dirty_rows,
            previous,
        );
    }

    let compatible = shape_compatible && previous.scrollback_lines == scrollback_lines;
    if !compatible || dirty_rows.is_none() || scroll_up.is_some() {
        return snapshot_from_term_with_display_offset(
            term,
            size,
            graphics,
            palette,
            display_offset,
            size.rows,
            Some(previous),
        );
    }

    let mut snapshot = previous.clone();
    let dirty_rows = dirty_rows.expect("partial terminal damage must contain row indexes");
    for &row in &dirty_rows {
        let line_id = snapshot.lines[row].line_id;
        let mut next_row = snapshot_row_from_term(term, size, palette, display_offset, row);
        next_row.line_id = line_id;
        snapshot.lines[row] = next_row;
    }
    refresh_snapshot_metadata(
        &mut snapshot,
        term,
        size,
        graphics,
        palette,
        display_offset,
        dirty_rows,
    );
    snapshot
}

fn snapshot_from_term_after_scroll_up<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    palette: &TerminalPalette,
    scrollback_lines: usize,
    scroll_up: usize,
    dirty_rows: &[usize],
    previous: &TerminalSnapshot,
) -> TerminalSnapshot {
    let mut rebuild_rows = vec![false; size.rows];
    rebuild_rows[size.rows - scroll_up..].fill(true);
    for &dirty_row in dirty_rows {
        // Damage can be recorded on either side of one or more scrolls. Rebuilding every possible
        // destination keeps the transform correct without adding per-cell generations to parsing.
        let first_possible_row = dirty_row.saturating_sub(scroll_up);
        let last_possible_row = dirty_row.min(size.rows - 1);
        rebuild_rows[first_possible_row..=last_possible_row].fill(true);
    }

    let mut snapshot = previous.clone();
    snapshot.display_offset = 0;
    snapshot.scrollback_lines = scrollback_lines;
    let mut dirty_rows = Vec::new();
    for (row, rebuild) in rebuild_rows.into_iter().enumerate() {
        let previous_row = row + scroll_up;
        if !rebuild {
            snapshot.lines[row] = previous.lines[previous_row].clone();
            snapshot.lines[row].absolute_line = row as i64;
            continue;
        }

        dirty_rows.push(row);
        let mut next_row = snapshot_row_from_term(term, size, palette, 0, row);
        if previous_row < previous.lines.len() {
            next_row.line_id = previous.lines[previous_row].line_id;
        }
        snapshot.lines[row] = next_row;
    }
    refresh_snapshot_metadata(&mut snapshot, term, size, graphics, palette, 0, dirty_rows);
    snapshot
}

pub fn scroll_snapshot_from_term<T: EventListener>(
    term: &mut Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    palette: &TerminalPalette,
    delta: i32,
    previous: &TerminalSnapshot,
) -> TerminalSnapshot {
    // The parser can advance before the UI observes its wakeup. Capture damage
    // under the same terminal lock, before scrolling adds full-viewport damage.
    let previous_offset = term.grid().display_offset();
    let dirty_rows = match term.damage() {
        TermDamage::Partial(damage) => {
            let mut dirty_rows = vec![false; size.rows];
            for line in damage {
                if let Some(dirty) = dirty_rows.get_mut(line.line) {
                    *dirty = true;
                }
            }
            Some(dirty_rows)
        }
        TermDamage::Full | TermDamage::ScrollUp { .. } => None,
    };
    if delta != 0 {
        term.scroll_display(Scroll::Delta(delta));
    }

    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let display_offset = term.grid().display_offset().min(scrollback_lines);
    let compatible = previous.cols == size.cols
        && previous.rows == size.rows
        && previous.display_offset == previous_offset
        && previous.scrollback_lines == scrollback_lines
        && previous.lines.len() == size.rows;
    let offset_distance = previous.display_offset.abs_diff(display_offset);
    if !compatible || dirty_rows.is_none() || offset_distance >= size.rows {
        let snapshot = snapshot_from_term_with_display_offset(
            term,
            size,
            graphics,
            palette,
            display_offset,
            size.rows,
            Some(previous),
        );
        term.reset_damage();
        return snapshot;
    }

    let dirty_rows = dirty_rows.expect("partial damage permits row reuse");
    let previous_row_index = |row: usize| {
        if display_offset >= previous.display_offset {
            row.checked_sub(offset_distance)
        } else {
            row.checked_add(offset_distance)
                .filter(|row| *row < previous.lines.len())
        }
    };
    let mut rebuilt_rows = Vec::new();
    let lines = (0..size.rows)
        .map(|row| {
            previous_row_index(row)
                .filter(|previous_row| !dirty_rows[*previous_row])
                .and_then(|previous_row| previous.lines.get(previous_row))
                .cloned()
                .unwrap_or_else(|| {
                    rebuilt_rows.push(row);
                    snapshot_row_from_term(term, size, palette, display_offset, row)
                })
        })
        .collect::<Vec<_>>();

    // Only overlapping rows without pending damage retain their shared cell buffers.
    let mapped_cursor_row = if display_offset >= previous.display_offset {
        previous.cursor_row.checked_add(offset_distance)
    } else {
        previous.cursor_row.checked_sub(offset_distance)
    }
    .unwrap_or(usize::MAX);
    let mut snapshot = TerminalSnapshot {
        row_reuse: None,
        logical_context: previous.logical_context.clone(),
        generation: 0,
        cols: size.cols,
        rows: size.rows,
        cursor_col: previous.cursor_col,
        cursor_row: mapped_cursor_row,
        cursor_shape: previous.cursor_shape,
        display_offset,
        scrollback_lines,
        lines,
        images: Vec::new(),
    };
    refresh_snapshot_metadata(
        &mut snapshot,
        term,
        size,
        graphics,
        palette,
        display_offset,
        rebuilt_rows,
    );
    term.reset_damage();
    snapshot
}

pub fn snapshot_from_term_with_display_offset<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    palette: &TerminalPalette,
    display_offset: usize,
    rows: usize,
    previous: Option<&TerminalSnapshot>,
) -> TerminalSnapshot {
    let scrollback_lines = term.total_lines().saturating_sub(term.screen_lines());
    let display_offset = display_offset.min(scrollback_lines);
    let requested_rows = rows.max(1);
    let reusable = previous
        .filter(|previous| previous.cols == size.cols)
        .and_then(|previous| Some((previous, previous.row_reuse.as_ref()?)))
        .filter(|(_, source)| {
            source.palette == *palette
                && source
                    .overrides
                    .iter()
                    .enumerate()
                    .all(|(index, color)| *color == term.colors()[index])
        });
    let mut previous_cursor = None;
    let mut rebuilt_rows = Vec::new();
    let lines = (0..requested_rows)
        .map(|index| {
            let grid_line = index as i64 - display_offset as i64;
            // These snapshots are read-only: row revisions are independent of who last
            // consumed emulator damage, and survive movement through the viewport.
            if let Some((previous, source)) = reusable
                && let Ok(previous_index) =
                    usize::try_from(grid_line + previous.display_offset as i64)
                && let Some(row) = previous.lines.get(previous_index)
                && row.source_id != 0
                && row.absolute_line == grid_line
                && grid_line >= -(scrollback_lines as i64)
                && grid_line < term.screen_lines() as i64
            {
                let current = &term.grid()[Line(grid_line as i32)];
                if row.source_id == current.identity()
                    && source.revisions.get(previous_index) == Some(&current.revision())
                {
                    if previous_index == previous.cursor_row {
                        previous_cursor = Some((index, previous.cursor_col));
                    }
                    return row.clone();
                }
            }
            rebuilt_rows.push(index);
            snapshot_row_from_term(term, size, palette, display_offset, index)
        })
        .collect();

    let mut snapshot = TerminalSnapshot {
        row_reuse: None,
        logical_context: previous
            .map_or_else(Vec::new, |snapshot| snapshot.logical_context.clone()),
        generation: 0,
        cols: size.cols,
        rows: size.rows,
        cursor_col: previous_cursor.map_or(0, |(_, col)| col),
        cursor_row: previous_cursor.map_or(usize::MAX, |(row, _)| row),
        cursor_shape: TerminalCursorShape::Hidden,
        display_offset,
        scrollback_lines,
        lines,
        images: Vec::new(),
    };
    refresh_snapshot_metadata(
        &mut snapshot,
        term,
        size,
        graphics,
        palette,
        display_offset,
        rebuilt_rows,
    );
    snapshot
}

/// Builds cells whose signature is finalized after cursor and input metadata are applied.
fn snapshot_row_from_term<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    palette: &TerminalPalette,
    display_offset: usize,
    row: usize,
) -> TerminalRow {
    let Some(source) = snapshot_row_source(term, size, display_offset, row) else {
        return blank_snapshot_row(
            size,
            display_offset,
            row,
            default_terminal_cell(palette, term.colors()),
        );
    };
    snapshot_row_from_source(term, size, palette, display_offset, row, source)
}

#[derive(Clone, Copy)]
struct SnapshotRowSource {
    source_id: usize,
    populated_cols: usize,
}

fn snapshot_row_source<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    display_offset: usize,
    row: usize,
) -> Option<SnapshotRowSource> {
    let grid_line = row as i32 - display_offset as i32;
    if grid_line < -(term.grid().history_size() as i32) || grid_line >= term.screen_lines() as i32 {
        return None;
    }

    let terminal_row = &term.grid()[Line(grid_line)];
    let default_cell = AlacrittyCell::default();
    let populated_cols = terminal_row
        .iter()
        .take(size.cols)
        .rposition(|cell| cell != &default_cell)
        .map_or(0, |column| column + 1);
    Some(SnapshotRowSource {
        // Emulator-owned identity survives scrolling and cannot alias recycled cell storage.
        source_id: terminal_row.identity(),
        populated_cols,
    })
}

fn snapshot_row_from_source<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    palette: &TerminalPalette,
    display_offset: usize,
    row: usize,
    source: SnapshotRowSource,
) -> TerminalRow {
    let grid_line = row as i32 - display_offset as i32;
    let terminal_row = &term.grid()[Line(grid_line)];
    let overrides = term.colors();
    let mut cells = Vec::with_capacity(size.cols);
    let mut recent_styles = [None, None];
    let mut next_style_slot = 0;
    let mut previous_extra = None;
    let mut wrapped = false;
    // Default trailing cells have fixed paint data, so skip color and metadata conversion.
    for cell in terminal_row.iter().take(source.populated_cols) {
        // A wrap flag makes the cell non-default, even on a wide-character spacer.
        wrapped |= cell.flags.contains(Flags::WRAPLINE);
        let spacer = cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
        // Spacers keep their own colors so a wide glyph's background covers both cells.
        let ch = if spacer || cell.c == '\0' {
            ' '
        } else {
            cell.c
        };
        let attrs = attrs_from_flags(cell.flags);
        let effective_fg = if attrs.inverse() { cell.bg } else { cell.fg };
        let ((fg, bg), style_origin) = if crate::palette::is_app_chosen_exact_color(&effective_fg) {
            (
                style_colors_for_cell(cell.fg, cell.bg, ch, attrs, palette, overrides),
                style_origin_for_cell(cell.fg, cell.bg, attrs),
            )
        } else {
            // Decoration glyphs bypass contrast adjustment even inside the same ANSI style run.
            let style_key = (
                cell.fg,
                cell.bg,
                attrs,
                crate::palette::is_terminal_decoration_glyph(ch),
            );
            if let Some((_, colors, origin)) = recent_styles
                .iter()
                .flatten()
                .find(|(key, _, _)| *key == style_key)
            {
                (*colors, *origin)
            } else {
                let colors = style_colors_for_cell(cell.fg, cell.bg, ch, attrs, palette, overrides);
                let origin = style_origin_for_cell(cell.fg, cell.bg, attrs);
                recent_styles[next_style_slot] = Some((style_key, colors, origin));
                next_style_slot = (next_style_slot + 1) % recent_styles.len();
                (colors, origin)
            }
        };
        let mut snapshot_cell = TerminalCell {
            ch,
            wide: !spacer && cell.flags.contains(Flags::WIDE_CHAR),
            fg,
            bg,
            style_origin,
            attrs,
            extra: None,
            cursor: false,
        };
        if spacer {
            cells.push(snapshot_cell);
            continue;
        }
        let zerowidth = cell.zerowidth().unwrap_or_default();
        let hyperlink = cell.hyperlink();
        let underline_color = cell
            .underline_color()
            .map(|color| crate::palette::color_to_rgb(color, palette, overrides));
        if !zerowidth.is_empty() || hyperlink.is_some() || underline_color.is_some() {
            if let Some((previous_zerowidth, previous_hyperlink, previous_underline_color, extra)) =
                &previous_extra
                && previous_zerowidth == &zerowidth
                && previous_hyperlink == &hyperlink
                && previous_underline_color == &underline_color
            {
                snapshot_cell.extra = Some(Arc::clone(extra));
            } else {
                snapshot_cell.set_extra_with_underline_color(
                    zerowidth.iter().copied().collect(),
                    hyperlink.as_ref().map(|link| link.uri().to_owned()),
                    underline_color,
                );
                previous_extra = Some((
                    zerowidth,
                    hyperlink,
                    underline_color,
                    snapshot_cell.extra.as_ref().unwrap().clone(),
                ));
            }
        }
        cells.push(snapshot_cell);
    }
    cells.resize(size.cols, default_terminal_cell(palette, overrides));
    TerminalRow {
        line_id: 0,
        source_id: source.source_id,
        absolute_line: i64::from(grid_line),
        wrapped,
        active_input: false,
        signature: 0,
        cells: Arc::new(cells),
    }
}

pub fn blank_snapshot_row(
    size: TerminalSize,
    display_offset: usize,
    row: usize,
    blank: TerminalCell,
) -> TerminalRow {
    let mut snapshot_row = TerminalRow {
        line_id: 0,
        source_id: 0,
        absolute_line: row as i64 - display_offset as i64,
        wrapped: false,
        active_input: false,
        signature: 0,
        cells: Arc::new(vec![blank; size.cols]),
    };
    snapshot_row.refresh_signature();
    snapshot_row
}

/// Builds an empty cell painted with the emulator's current default colors.
pub fn default_terminal_cell(palette: &TerminalPalette, overrides: &Colors) -> TerminalCell {
    let (fg, bg) = crate::palette::default_colors(palette, overrides);
    TerminalCell {
        ch: ' ',
        wide: false,
        fg,
        bg,
        style_origin: TerminalStyleOrigin::default(),
        attrs: TerminalAttrs::default(),
        extra: None,
        cursor: false,
    }
}

fn refresh_snapshot_metadata<T: EventListener>(
    snapshot: &mut TerminalSnapshot,
    term: &Term<T>,
    size: TerminalSize,
    graphics: &TerminalGraphicsState,
    palette: &TerminalPalette,
    display_offset: usize,
    mut metadata_rows: Vec<usize>,
) {
    crate::logical_context::capture(term, snapshot);
    let content = term.renderable_content();
    let cursor_row = (content.cursor.point.line.0 + display_offset as i32).max(0) as usize;
    let cursor_col = content.cursor.point.column.0;
    let cursor_position_changed =
        snapshot.cursor_row != cursor_row || snapshot.cursor_col != cursor_col;
    if cursor_position_changed
        && snapshot.cursor_row < snapshot.lines.len()
        && snapshot.cursor_col < size.cols
    {
        let previous_cursor_row = snapshot.cursor_row;
        if snapshot.lines[previous_cursor_row].cells[snapshot.cursor_col].cursor {
            snapshot.lines[previous_cursor_row].cells_mut()[snapshot.cursor_col].cursor = false;
            metadata_rows.push(previous_cursor_row);
        }
    }
    let active_input_rows = if cursor_row < snapshot.lines.len() && cursor_col < size.cols {
        active_input_row_range(&snapshot.lines, cursor_row)
    } else {
        0..0
    };
    for (row_index, row) in snapshot.lines.iter_mut().enumerate() {
        let active_input = active_input_rows.contains(&row_index);
        if row.active_input != active_input {
            row.active_input = active_input;
            metadata_rows.push(row_index);
        }
    }
    if cursor_row < snapshot.lines.len()
        && cursor_col < size.cols
        && !snapshot.lines[cursor_row].cells[cursor_col].cursor
    {
        snapshot.lines[cursor_row].cells_mut()[cursor_col].cursor = true;
        metadata_rows.push(cursor_row);
    }
    snapshot.cursor_col = cursor_col;
    snapshot.cursor_row = cursor_row;
    snapshot.cursor_shape = content.cursor.shape.into();
    snapshot.images = graphics.visible_images(display_offset, snapshot.lines.len());
    resolve_sixel_backgrounds(&mut snapshot.images, palette, term.colors());
    // New rows and metadata changes share a single hash pass after cursor/input flags are final.
    // Shared, unchanged rows retain their signatures without visiting their cells.
    metadata_rows.sort_unstable();
    metadata_rows.dedup();
    for row_index in metadata_rows {
        snapshot.lines[row_index].refresh_signature();
    }
    snapshot.row_reuse = Some(Arc::new(crate::snapshot::TerminalSnapshotRowReuse {
        palette: *palette,
        overrides: std::array::from_fn(|index| term.colors()[index]),
        revisions: snapshot
            .lines
            .iter()
            .map(|row| {
                if row.source_id == 0 {
                    0
                } else {
                    term.grid()[Line(row.absolute_line as i32)].revision()
                }
            })
            .collect(),
    }));
}

fn active_input_row_range(rows: &[TerminalRow], cursor_row: usize) -> std::ops::Range<usize> {
    let mut start = cursor_row;
    while start > 0 && rows.get(start - 1).is_some_and(|row| row.wrapped) {
        start -= 1;
    }

    let mut end = cursor_row + 1;
    while end < rows.len() && rows.get(end - 1).is_some_and(|row| row.wrapped) {
        end += 1;
    }
    start..end
}

#[cfg(test)]
mod tests;
