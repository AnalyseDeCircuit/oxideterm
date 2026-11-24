use super::*;

impl<H: 'static> TerminalElement<H> {
    pub(super) fn row_layout(
        &self,
        row_index: usize,
        row: &oxideterm_terminal_emulator::TerminalRow,
        highlight_layout: &TerminalHighlightLayout,
        link_ranges: &[TerminalLinkRange],
        terminal_background: Hsla,
        previous: Option<&TerminalRowLayout>,
    ) -> TerminalRowLayout {
        // Visible link ranges are ordered by row. Restrict cell styling to this row so output in
        // other rows cannot multiply every character lookup in a link-dense terminal.
        let row_links_start = link_ranges.partition_point(|range| range.row < row_index);
        let row_links_end = row_links_start
            + link_ranges[row_links_start..].partition_point(|range| range.row == row_index);
        let link_ranges = &link_ranges[row_links_start..row_links_end];
        let mut backgrounds = Vec::new();
        let mut selections = Vec::new();
        let mut text_runs = Vec::new();
        let mut cursor = None;
        let mut current_background: Option<TerminalRowRect> = None;
        let mut current_selection: Option<TerminalRowRect> = None;
        let mut current_run: Option<PendingTerminalRowTextRun> = None;
        let mut current_run_is_drawing = false;
        let visual_line = visual_line_for_row_with_bidi(row, self.bidi_enabled);
        let transition_colors = self.transition_colors.as_ref();

        for (col_index, cell) in row.cells.iter().enumerate() {
            let paint_col = visual_line
                .as_ref()
                .map(|line| line.visual_col_for_logical_col(col_index))
                .unwrap_or(col_index);
            if self.grid_cursor_visible()
                && cell.cursor
                && self.snapshot.cursor_shape != TerminalCursorShape::Hidden
            {
                cursor = Some(TerminalRowCursor {
                    col: paint_col,
                    shape: self.snapshot.cursor_shape,
                });
            }

            let block_cursor = self.grid_cursor_visible()
                && cell.cursor
                && self.snapshot.cursor_shape == TerminalCursorShape::Block;
            let (bg, bg_key) = resolve_cell_background(
                cell,
                block_cursor,
                &self.theme,
                terminal_background,
                transition_colors,
            );
            let cell_width = if cell.wide { 2 } else { 1 };

            let cutout = powerline_background_cutout(
                cell,
                self.transparent_background,
                block_cursor
                    || highlight_layout
                        .foreground_for_cell(row_index, col_index)
                        .is_some(),
            );
            let emits_background_rect = bg_key != TerminalTransitionColorKey::TerminalBackground
                && (bg != terminal_background
                    || matches!(bg_key, TerminalTransitionColorKey::PaletteColor(_)));

            if let Some(ch) = cutout {
                if let Some(rect) = current_background.take() {
                    backgrounds.push(rect);
                }
                // Paint only the part outside the glyph; a full-cell quad would hide the image.
                if bg_key != TerminalTransitionColorKey::TerminalBackground
                    && (bg != terminal_background || cell.style_origin.background_explicit())
                {
                    backgrounds.push(TerminalRowRect {
                        col: paint_col,
                        cells: cell_width,
                        color: bg,
                        transition_key: bg_key,
                        powerline_cutout: Some(ch),
                    });
                }
            } else if emits_background_rect {
                extend_or_push_row_rect(
                    &mut current_background,
                    &mut backgrounds,
                    paint_col,
                    cell_width,
                    bg,
                    bg_key,
                );
            } else if let Some(rect) = current_background.take() {
                backgrounds.push(rect);
            }

            if self.selection.is_some_and(|selection| {
                selection.contains_viewport_cell(row_index, col_index, self.snapshot.display_offset)
            }) {
                let (sel_color, sel_key) = match transition_colors {
                    Some(transition) => (
                        transition.target_selection_background,
                        TerminalTransitionColorKey::SelectionBackground,
                    ),
                    None => (
                        self.theme.selection_background,
                        TerminalTransitionColorKey::Static,
                    ),
                };
                extend_or_push_row_rect(
                    &mut current_selection,
                    &mut selections,
                    paint_col,
                    cell_width,
                    sel_color,
                    sel_key,
                );
            } else if let Some(rect) = current_selection.take() {
                selections.push(rect);
            }

            if visual_line.is_some() {
                continue;
            }

            if cutout.is_some() || cell.ch == '\u{10eeee}' {
                if let Some(run) = current_run.take() {
                    text_runs.push(run);
                }
                continue;
            }
            if cell.ch != ' '
                || !cell.zerowidth().is_empty()
                || (self.grid_cursor_visible() && cell.cursor)
            {
                let (fg, fg_key) = resolve_cell_foreground(
                    cell,
                    block_cursor,
                    highlight_layout.foreground_for_cell(row_index, col_index),
                    terminal_background,
                    transition_colors,
                );
                let link = !block_cursor
                    && (cell.hyperlink().is_some() || is_link_stylable_cell(cell))
                    && link_should_be_styled(
                        link_ranges,
                        self.hovered_link.as_ref(),
                        row_index,
                        col_index,
                    );
                let style = canvas_text_run_for_cell(cell, fg, link, &self.metrics);
                let custom_underline = custom_underline_for_cell(cell, fg, link);
                let is_drawing =
                    cell.zerowidth().is_empty() && cell_drawing::is_cell_drawing(cell.ch);
                if cell.zerowidth().is_empty() && powerline_separator(cell.ch).is_some() {
                    if let Some(run) = current_run.take() {
                        text_runs.push(run);
                    }
                    text_runs.push(PendingTerminalRowTextRun {
                        col: col_index,
                        text: cell_text(cell),
                        cells: cell_width,
                        style,
                        custom_underline,
                        transition_key: fg_key,
                    });
                    continue;
                }
                if let Some(run) = &mut current_run
                    && run.col + run.cells == col_index
                    && text_run_style_matches(&run.style, &style)
                    && run.custom_underline == custom_underline
                    && run.transition_key == fg_key
                    && current_run_is_drawing == is_drawing
                {
                    // Append directly to the existing run so ordinary rows do not
                    // allocate a temporary String for every visible cell.
                    push_cell_text(&mut run.text, cell);
                    run.cells += cell_width;
                    run.style.len += cell_text_len(cell);
                    continue;
                }

                if let Some(run) = current_run.take() {
                    text_runs.push(run);
                }
                current_run_is_drawing = is_drawing;
                current_run = Some(PendingTerminalRowTextRun {
                    col: col_index,
                    text: cell_text(cell),
                    cells: cell_width,
                    style,
                    custom_underline,
                    transition_key: fg_key,
                });
            } else if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
        }

        if let Some(visual_line) = visual_line.as_ref() {
            push_visual_text_runs(
                row_index,
                row,
                visual_line,
                link_ranges,
                self.hovered_link.as_ref(),
                &self.metrics,
                self.grid_cursor_visible(),
                self.snapshot.cursor_shape,
                terminal_background,
                transition_colors,
                self.transparent_background,
                highlight_layout,
                &mut text_runs,
            );
        }

        if let Some(rect) = current_background.take() {
            backgrounds.push(rect);
        }
        if let Some(rect) = current_selection.take() {
            selections.push(rect);
        }
        if let Some(run) = current_run.take() {
            text_runs.push(run);
        }

        let mut previous_runs = previous
            .into_iter()
            .flat_map(|row| row.text_runs.iter())
            .peekable();
        let text_runs = text_runs
            .into_iter()
            .map(|run| {
                while previous_runs
                    .peek()
                    .is_some_and(|previous| previous.col < run.col)
                {
                    previous_runs.next();
                }
                let previous = previous_runs
                    .peek()
                    .copied()
                    .filter(|previous| previous.col == run.col);
                run.finish(previous)
            })
            .collect();
        TerminalRowLayout {
            backgrounds,
            selections,
            text_runs,
            cursor,
        }
    }

    pub(super) fn row_layout_style_hasher(&self) -> CacheKeyHasher {
        // Every row in this layout shares these settings. Preserve the hash state so only
        // row-dependent data is appended for each cache key, without repeating font hashing.
        let mut hasher = new_cache_key_hasher();
        self.snapshot.cols.hash(&mut hasher);
        f32::from(self.metrics.font_size)
            .to_bits()
            .hash(&mut hasher);
        f32::from(self.metrics.cell_width)
            .to_bits()
            .hash(&mut hasher);
        f32::from(self.metrics.line_height)
            .to_bits()
            .hash(&mut hasher);
        self.metrics.font.hash(&mut hasher);
        if let Some(transition) = &self.transition_colors {
            // Keep row layouts cached under a transition-invariant key while live colors remap at paint time.
            true.hash(&mut hasher);
            transition.layout_signature.hash(&mut hasher);
        } else {
            false.hash(&mut hasher);
            self.theme.background.hash(&mut hasher);
            self.theme.foreground.hash(&mut hasher);
            self.theme.header_foreground.hash(&mut hasher);
            self.theme.selection_background.hash(&mut hasher);
        }
        self.transparent_background.hash(&mut hasher);
        self.bidi_enabled.hash(&mut hasher);
        self.detect_file_paths_as_links.hash(&mut hasher);
        self.highlight_rules_signature.hash(&mut hasher);
        self.transient_command_highlight_signature.hash(&mut hasher);
        hasher
    }

    pub(super) fn row_layout_cache_key_with_logical_line(
        &self,
        row_index: usize,
        logical_line: &TerminalLogicalLine,
        semantic_role: Option<SemanticLineRole>,
        style_hasher: &CacheKeyHasher,
    ) -> TerminalRowLayoutCacheKey {
        let mut hasher = style_hasher.clone();
        if let Some(row) = self.snapshot.lines.get(row_index) {
            hash_row_content(row, &mut hasher);
            let has_cursor = row.cells.iter().any(|cell| cell.cursor);
            has_cursor.hash(&mut hasher);
            if has_cursor {
                self.snapshot.cursor_shape.hash(&mut hasher);
                self.grid_cursor_visible().hash(&mut hasher);
            }
        }
        logical_line.signature.hash(&mut hasher);
        self.hash_semantic_layout(logical_line.range.clone(), semantic_role, &mut hasher);
        if let Some(hovered_link) = self
            .hovered_link
            .as_ref()
            .filter(|hovered_link| hovered_link.row == row_index)
        {
            hovered_link.hash(&mut hasher);
        }
        hash_selection_for_row(
            self.selection,
            row_index,
            self.snapshot.display_offset,
            self.snapshot.cols,
            &mut hasher,
        );
        TerminalRowLayoutCacheKey {
            signature: hasher.finish(),
        }
    }

    pub(super) fn marked_text_run(&self, cursor_cell_visible: bool) -> Option<BatchedTextRun> {
        if !cursor_cell_visible {
            return None;
        }
        let text = self.marked_text.as_ref()?;
        let marked_col = self
            .snapshot
            .lines
            .get(self.snapshot.cursor_row)
            .and_then(|row| visual_line_for_row_with_bidi(row, self.bidi_enabled))
            .map(|line| line.visual_col_for_logical_col(self.snapshot.cursor_col))
            .unwrap_or(self.snapshot.cursor_col);
        Some(BatchedTextRun {
            row: self.snapshot.cursor_row,
            col: marked_col,
            text: SharedString::from(text.clone()),
            cells: marked_text_cells_before_utf16(text, usize::MAX).max(1),
            style: canvas_marked_text_run(text, &self.theme, &self.metrics),
            custom_underline: None,
            cache: None,
        })
    }

    pub(super) fn composition_caret(&self, marked_text: &BatchedTextRun) -> Option<TerminalCursor> {
        if !self.cursor_visible {
            return None;
        }
        let caret_cells = self
            .marked_text_caret_utf16
            .map(|caret| marked_text_cells_before_utf16(&marked_text.text, caret))
            .unwrap_or(marked_text.cells);
        // A bar keeps preedit glyphs readable when the IME caret sits between
        // composed characters, independent of the program's grid cursor shape.
        Some(TerminalCursor {
            row: marked_text.row,
            col: marked_text.col + caret_cells,
            shape: TerminalCursorShape::Bar,
        })
    }

    fn grid_cursor_visible(&self) -> bool {
        // The IME preedit is painted over the terminal cursor cell, so the
        // composition caret replaces the grid cursor until text is committed.
        self.cursor_visible && self.marked_text.is_none()
    }

    pub(super) fn ghost_text_run(&self, cursor_row_visible: bool) -> Option<BatchedTextRun> {
        if self.marked_text.is_some()
            || !cursor_row_visible
            || self.snapshot.cursor_shape == TerminalCursorShape::Hidden
        {
            return None;
        }

        let text = self.ghost_text.as_deref().filter(|text| !text.is_empty())?;
        let row = self.snapshot.lines.get(self.snapshot.cursor_row)?;
        let visual_line = visual_line_for_row_with_bidi(row, self.bidi_enabled);
        let col = visual_line
            .as_ref()
            .map(|line| line.visual_col_for_logical_col(self.snapshot.cursor_col))
            .unwrap_or(self.snapshot.cursor_col);
        let remaining_cells = self.snapshot.cols.saturating_sub(col);
        if remaining_cells == 0 {
            return None;
        }

        let (visible_text, visible_cells) = ghost_text_prefix_for_cells(text, remaining_cells);
        if visible_text.is_empty() {
            return None;
        }

        Some(BatchedTextRun {
            row: self.snapshot.cursor_row,
            col,
            cells: visible_cells,
            style: canvas_ghost_text_run(&visible_text, &self.theme, &self.metrics),
            custom_underline: None,
            text: SharedString::from(visible_text),
            cache: None,
        })
    }
}

fn ghost_text_prefix_for_cells(text: &str, max_cells: usize) -> (String, usize) {
    // Clipping and painting must agree on complete graphemes so a viewport edge
    // cannot split a joined emoji or detach a combining mark.
    let mut prefix = String::new();
    let mut cells = 0;
    for grapheme in text.graphemes(true) {
        let width = grapheme.width();
        if cells + width > max_cells {
            break;
        }
        prefix.push_str(grapheme);
        cells += width;
    }
    (prefix, cells)
}

fn visual_line_for_row_with_bidi(
    row: &oxideterm_terminal_emulator::TerminalRow,
    bidi_enabled: bool,
) -> Option<TerminalVisualLine> {
    // Ordinary rows use their logical cell order without allocating a visual map.
    if bidi_enabled {
        visual_line_for_row_if_bidi(row)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_cached_row_layout(
    row_index: usize,
    row_layout: &TerminalRowLayout,
    transition_colors: Option<&TerminalTransitionColors>,
    live_terminal_background: Hsla,
    backgrounds: &mut Vec<TerminalRect>,
    selections: &mut Vec<TerminalRect>,
    text_runs: &mut Vec<BatchedTextRun>,
    cursor: &mut Option<TerminalCursor>,
) {
    backgrounds.extend(row_layout.backgrounds.iter().map(|rect| TerminalRect {
        row: row_index,
        col: rect.col,
        cells: rect.cells,
        color: transition_colors.map_or(rect.color, |transition| {
            transition.resolve_row_color(rect.transition_key, rect.color, live_terminal_background)
        }),
        powerline_cutout: rect.powerline_cutout,
    }));
    selections.extend(row_layout.selections.iter().map(|rect| TerminalRect {
        row: row_index,
        col: rect.col,
        cells: rect.cells,
        color: transition_colors.map_or(rect.color, |transition| {
            transition.resolve_row_color(rect.transition_key, rect.color, live_terminal_background)
        }),
        powerline_cutout: None,
    }));
    text_runs.extend(row_layout.text_runs.iter().map(|run| {
        let mut style = run.style.clone();
        let mut custom_underline = run.custom_underline;
        if let Some(transition) = transition_colors {
            style.color = transition.resolve_row_color(
                run.transition_key,
                run.style.color,
                live_terminal_background,
            );
            if let Some(ul) = custom_underline.as_mut()
                && !ul.explicit_color
            {
                ul.color = style.color;
            }
        }
        BatchedTextRun {
            row: row_index,
            col: run.col,
            text: run.text.clone(),
            cells: run.cells,
            style,
            custom_underline,
            cache: Some(run.cache.clone()),
        }
    }));
    if let Some(row_cursor) = row_layout.cursor {
        *cursor = Some(TerminalCursor {
            row: row_index,
            col: row_cursor.col,
            shape: row_cursor.shape,
        });
    }
}

fn extend_or_push_row_rect(
    current: &mut Option<TerminalRowRect>,
    rects: &mut Vec<TerminalRowRect>,
    col: usize,
    cells: usize,
    color: Hsla,
    transition_key: TerminalTransitionColorKey,
) {
    if let Some(rect) = current
        && col <= rect.col + rect.cells
        && rect.col <= col + cells
        && rect.color == color
        && rect.transition_key == transition_key
    {
        // A wide glyph and its spacer overlap by one column. Union their
        // coverage instead of painting translucent selection color twice.
        let end = (rect.col + rect.cells).max(col + cells);
        rect.col = rect.col.min(col);
        rect.cells = end - rect.col;
        return;
    }

    if let Some(rect) = current.take() {
        rects.push(rect);
    }
    *current = Some(TerminalRowRect {
        col,
        cells,
        color,
        transition_key,
        powerline_cutout: None,
    });
}

pub(super) fn hash_row_content(
    row: &oxideterm_terminal_emulator::TerminalRow,
    hasher: &mut impl Hasher,
) {
    // The pane may assign a new line ID when a row re-enters the viewport.
    // The emulator's row ID survives that round trip; the signature tracks edits and styles.
    if row.source_id != 0 {
        true.hash(hasher);
        row.source_id.hash(hasher);
    } else {
        false.hash(hasher);
        row.line_id.hash(hasher);
    }
    row.signature.hash(hasher);
}

fn hash_selection_for_row(
    selection: Option<TerminalSelection>,
    row: usize,
    display_offset: usize,
    cols: usize,
    hasher: &mut impl Hasher,
) {
    let Some(selection) = selection else {
        0u8.hash(hasher);
        return;
    };
    let line = row as i32 - display_offset as i32;
    let selected_span = if selection.mode == crate::selection::TerminalSelectionMode::Block {
        let row_start = selection.anchor.line.min(selection.head.line);
        let row_end = selection.anchor.line.max(selection.head.line);
        (line >= row_start && line <= row_end).then(|| {
            (
                selection.anchor.col.min(selection.head.col),
                selection.anchor.col.max(selection.head.col),
            )
        })
    } else {
        let (start, end) = selection.normalized();
        if line < start.line || line > end.line {
            None
        } else {
            Some((
                if line == start.line { start.col } else { 0 },
                if line == end.line {
                    end.col
                } else {
                    cols.saturating_sub(1)
                },
            ))
        }
    };
    let Some((start_col, end_col)) = selected_span else {
        0u8.hash(hasher);
        return;
    };
    1u8.hash(hasher);
    start_col.hash(hasher);
    end_col.hash(hasher);
    match selection.mode {
        crate::selection::TerminalSelectionMode::Simple => 0u8,
        crate::selection::TerminalSelectionMode::Block => 1,
        crate::selection::TerminalSelectionMode::Semantic => 2,
        crate::selection::TerminalSelectionMode::Lines => 3,
    }
    .hash(hasher);
}

fn resolve_cell_background(
    cell: &TerminalCell,
    block_cursor: bool,
    theme: &TerminalUiTheme,
    terminal_background: Hsla,
    transition_colors: Option<&TerminalTransitionColors>,
) -> (Hsla, TerminalTransitionColorKey) {
    if block_cursor {
        return match transition_colors {
            Some(transition) => (
                transition.target_header_foreground,
                TerminalTransitionColorKey::CursorBackground,
            ),
            None => (
                to_hsla(terminal_color_from_hex(theme.header_foreground)),
                TerminalTransitionColorKey::Static,
            ),
        };
    }
    if let Some(transition) = transition_colors {
        if !cell.style_origin.background_explicit() && cell.bg == transition.target_background {
            return (
                terminal_background,
                TerminalTransitionColorKey::TerminalBackground,
            );
        }
        if cell.style_origin.foreground_is_default_background()
            && cell.bg == transition.target_foreground
        {
            return (
                transition.target_foreground_hsla,
                TerminalTransitionColorKey::DefaultForeground,
            );
        }
        if cell.style_origin.background_explicit() && transition.map_ansi_color(cell.bg).is_some() {
            return (
                to_hsla(cell.bg),
                TerminalTransitionColorKey::PaletteColor(cell.bg),
            );
        }
        return (to_hsla(cell.bg), TerminalTransitionColorKey::Static);
    }
    if cell.bg == terminal_color_from_hex(theme.background) {
        (terminal_background, TerminalTransitionColorKey::Static)
    } else {
        (to_hsla(cell.bg), TerminalTransitionColorKey::Static)
    }
}

fn resolve_cell_foreground(
    cell: &TerminalCell,
    block_cursor: bool,
    highlight_fg: Option<TerminalHighlightForeground>,
    terminal_background: Hsla,
    transition_colors: Option<&TerminalTransitionColors>,
) -> (Hsla, TerminalTransitionColorKey) {
    if block_cursor {
        return (
            terminal_background,
            if transition_colors.is_some() {
                TerminalTransitionColorKey::TerminalBackground
            } else {
                TerminalTransitionColorKey::Static
            },
        );
    }
    if let Some(highlight_fg) = highlight_fg {
        if let Some(transition) = transition_colors
            && let Some(ansi) = highlight_fg.ansi_color
            && transition.map_ansi_color(ansi).is_some()
        {
            return (
                highlight_fg.color,
                TerminalTransitionColorKey::PaletteColor(ansi),
            );
        }
        return (highlight_fg.color, TerminalTransitionColorKey::Static);
    }
    if let Some(transition) = transition_colors {
        if cell.style_origin.foreground_is_default_background()
            && cell.fg == transition.target_background
        {
            return (
                transition.target_background_hsla,
                TerminalTransitionColorKey::TerminalBackground,
            );
        }
        if !cell.style_origin.foreground_explicit() {
            if cell.fg == transition.target_foreground {
                return (
                    transition.target_foreground_hsla,
                    TerminalTransitionColorKey::DefaultForeground,
                );
            }
            if cell.fg == transition.target_dim_foreground {
                return (
                    transition.target_dim_foreground_hsla,
                    TerminalTransitionColorKey::DefaultDimForeground,
                );
            }
            return (to_hsla(cell.fg), TerminalTransitionColorKey::Static);
        }
        if transition.map_ansi_color(cell.fg).is_some() {
            return (
                to_hsla(cell.fg),
                TerminalTransitionColorKey::PaletteColor(cell.fg),
            );
        }
        return (to_hsla(cell.fg), TerminalTransitionColorKey::Static);
    }
    (to_hsla(cell.fg), TerminalTransitionColorKey::Static)
}

#[allow(clippy::too_many_arguments)]
fn push_visual_text_runs(
    source_row_index: usize,
    row: &oxideterm_terminal_emulator::TerminalRow,
    visual_line: &TerminalVisualLine,
    link_ranges: &[TerminalLinkRange],
    hovered_link: Option<&TerminalLinkRange>,
    metrics: &TerminalMetrics,
    cursor_visible: bool,
    cursor_shape: TerminalCursorShape,
    terminal_background: Hsla,
    transition_colors: Option<&TerminalTransitionColors>,
    transparent_background: bool,
    highlight_layout: &TerminalHighlightLayout,
    text_runs: &mut Vec<PendingTerminalRowTextRun>,
) {
    let mut current_run: Option<PendingTerminalRowTextRun> = None;
    let mut current_run_is_drawing = false;
    for cluster in &visual_line.clusters {
        let Some(cell) = row.cells.get(cluster.logical_col) else {
            continue;
        };
        if (cell.ch == ' ' && cell.zerowidth().is_empty()) || cell.ch == '\u{10eeee}' {
            if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
            continue;
        }

        let block_cursor =
            cursor_visible && cell.cursor && cursor_shape == TerminalCursorShape::Block;
        if powerline_background_cutout(
            cell,
            transparent_background,
            block_cursor
                || highlight_layout
                    .foreground_for_cell(source_row_index, cluster.logical_col)
                    .is_some(),
        )
        .is_some()
        {
            if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
            continue;
        }
        let (fg, fg_key) = resolve_cell_foreground(
            cell,
            block_cursor,
            highlight_layout.foreground_for_cell(source_row_index, cluster.logical_col),
            terminal_background,
            transition_colors,
        );
        let link = !block_cursor
            && (cell.hyperlink().is_some() || is_link_stylable_cell(cell))
            && link_should_be_styled(
                link_ranges,
                hovered_link,
                source_row_index,
                cluster.logical_col,
            );
        let style = canvas_text_run_for_cell(cell, fg, link, metrics);
        let custom_underline = custom_underline_for_cell(cell, fg, link);
        let is_drawing = cell.zerowidth().is_empty() && cell_drawing::is_cell_drawing(cell.ch);
        if cell.zerowidth().is_empty() && powerline_separator(cell.ch).is_some() {
            if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
            text_runs.push(PendingTerminalRowTextRun {
                col: cluster.visual_col,
                text: cluster.text.clone(),
                cells: cluster.cells,
                style,
                custom_underline,
                transition_key: fg_key,
            });
            continue;
        }

        // Runs are painted with a one-cell forced advance, so a wide glyph must stay alone
        // in its run, matching the logical path where its spacer cell ends the run.
        let wide = cluster.cells > 1;
        if !wide
            && let Some(run) = &mut current_run
            && run.col + run.cells == cluster.visual_col
            && text_run_style_matches(&run.style, &style)
            && run.custom_underline == custom_underline
            && run.transition_key == fg_key
            && current_run_is_drawing == is_drawing
        {
            run.text.push_str(&cluster.text);
            run.cells += cluster.cells;
            run.style.len += cluster.text.len();
            continue;
        }

        if let Some(run) = current_run.take() {
            text_runs.push(run);
        }
        let run = PendingTerminalRowTextRun {
            col: cluster.visual_col,
            text: cluster.text.clone(),
            cells: cluster.cells,
            style,
            custom_underline,
            transition_key: fg_key,
        };
        if wide {
            text_runs.push(run);
        } else {
            current_run_is_drawing = is_drawing;
            current_run = Some(run);
        }
    }

    if let Some(run) = current_run.take() {
        text_runs.push(run);
    }
}

pub(super) fn map_rects_to_visual(
    snapshot: &TerminalSnapshot,
    bidi_enabled: bool,
    rects: Vec<TerminalRect>,
) -> Vec<TerminalRect> {
    let mut mapped = Vec::with_capacity(rects.len());
    let mut mapped_row = None;
    let mut visual_line = None;
    for rect in rects {
        let Some(row) = snapshot.lines.get(rect.row) else {
            continue;
        };
        if mapped_row != Some(rect.row) {
            // Dense matches share a row; bidi detection and mapping only need to run once for it.
            visual_line = visual_line_for_row_with_bidi(row, bidi_enabled);
            mapped_row = Some(rect.row);
        }
        let Some(visual_line) = &visual_line else {
            mapped.push(rect);
            continue;
        };

        for range in visual_line.visual_rects_for_logical_range(rect.col..rect.col + rect.cells) {
            mapped.push(TerminalRect {
                row: rect.row,
                col: range.start,
                cells: range.end.saturating_sub(range.start),
                color: rect.color,
                powerline_cutout: rect.powerline_cutout,
            });
        }
    }
    mapped
}

fn cell_text(cell: &oxideterm_terminal_emulator::TerminalCell) -> String {
    let mut text = String::with_capacity(cell_text_len(cell));
    push_cell_text(&mut text, cell);
    text
}

fn push_cell_text(text: &mut String, cell: &oxideterm_terminal_emulator::TerminalCell) {
    text.push(cell.ch);
    text.push_str(cell.zerowidth());
}

fn cell_text_len(cell: &oxideterm_terminal_emulator::TerminalCell) -> usize {
    cell.ch.len_utf8() + cell.zerowidth().len()
}
