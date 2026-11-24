use super::*;

impl<H: 'static> TerminalElement<H> {
    pub(super) fn painted_row_limit(&self) -> usize {
        let overscan_rows = if f32::from(self.scroll_y_offset).abs() > f32::EPSILON {
            1
        } else {
            0
        };
        self.viewport_rows
            .saturating_add(overscan_rows)
            .min(self.snapshot.lines.len())
    }

    pub(super) fn visible_rows_for_bounds(&self, bounds: Bounds<Pixels>) -> Range<usize> {
        let mut rows = terminal_visible_rows_for_limit(bounds, &self.metrics, self.viewport_rows);
        if f32::from(self.scroll_y_offset).abs() > f32::EPSILON {
            rows.end = rows.end.saturating_add(1).min(self.painted_row_limit());
        }
        rows
    }

    pub(super) fn layout_for_rows(
        &self,
        visible_rows: Range<usize>,
        mut cache: Option<&mut TerminalLayoutCache>,
    ) -> TerminalElementLayout {
        if let Some(cache) = cache.as_deref_mut() {
            // A top-to-bottom frame would otherwise evict every next entry in a tall viewport.
            // Keep room for the whole viewport plus a screen of recently scrolled content.
            cache.highlights.set_capacity(
                TERMINAL_HIGHLIGHT_CACHE_CAPACITY
                    .max(self.viewport_rows.saturating_add(1).saturating_mul(2)),
            );
            let metrics = [
                self.metrics.font_size,
                self.metrics.cell_width,
                self.metrics.line_height,
            ]
            .map(|value| f32::from(value).to_bits());
            if cache.reuse_metrics != Some(metrics) {
                cache.recent_row_keys.clear();
                cache.timestamps.clear();
                cache.reuse_metrics = Some(metrics);
            }
        }
        let mut backgrounds = Vec::new();
        let logical_lines = self.logical_lines_for_rows(visible_rows.clone());
        #[cfg(feature = "bench")]
        let roles_started = Instant::now();
        let semantic_roles = self.semantic_roles_for_lines(&logical_lines, cache.as_deref_mut());
        #[cfg(feature = "bench")]
        if let Some(cache) = cache.as_deref_mut() {
            cache.performance.semantic_roles_micros = duration_micros(roles_started.elapsed());
            cache.performance.semantic_command_lines = semantic_roles
                .values()
                .filter(|role| **role == SemanticLineRole::Command)
                .count();
        }
        #[cfg(feature = "bench")]
        let highlights_started = Instant::now();
        // Empty highlight results need neither per-line cache keys nor cache lookups.
        let (mut highlight_layout, cached_highlights) = if !self.semantic_coloring
            && self.transient_command_highlight.is_none()
            && !self.highlight_rules.iter().any(|rule| rule.enabled)
        {
            (TerminalHighlightLayout::empty(), Vec::new())
        } else if let Some(cache) = cache.as_deref_mut() {
            self.cached_highlight_layout_for_rows(&logical_lines, &semantic_roles, cache)
        } else {
            (
                self.highlight_layout_for_logical_lines(&logical_lines),
                Vec::new(),
            )
        };
        let mut foreground_lines = FastHashSet::new();
        #[cfg(feature = "bench")]
        if let Some(cache) = cache.as_deref_mut() {
            cache.performance.highlights_micros = duration_micros(highlights_started.elapsed());
        }
        let search_matches = map_rects_to_visual(
            &self.snapshot,
            self.bidi_enabled,
            if !self.search_matches_precomputed && self.search_matches.is_empty() {
                search_match_rects_for_rows(
                    &self.snapshot,
                    self.search_query.as_deref(),
                    visible_rows.clone(),
                )
            } else {
                visible_search_match_rects(
                    &self.search_matches,
                    self.snapshot.display_offset,
                    visible_rows.clone(),
                    self.selected_search_match,
                )
            },
        );
        let selection_matches = map_rects_to_visual(
            &self.snapshot,
            self.bidi_enabled,
            crate::highlight::selection_match_rects(
                &self.snapshot,
                self.selection_highlight_query
                    .as_ref()
                    .map(|query| query.as_str()),
                visible_rows.clone(),
                rgba((self.theme.tokens.ui.warning << 8) | TRANSIENT_COMMAND_HIGHLIGHT_ALPHA)
                    .into(),
            ),
        );
        let command_mark_overlays = command_mark_overlays_for_rows(
            &self.snapshot,
            &self.command_marks,
            self.selected_command_mark_id.as_deref(),
            self.hovered_command_mark_id.as_deref(),
            &self.collapsed_commands,
            &self.fold_rotations,
        );
        let mut selections = Vec::new();
        let mut images = self
            .rendered_images
            .iter()
            .filter(|image| {
                image.snapshot.row < self.painted_row_limit()
                    && image.snapshot.row + image.snapshot.rows > visible_rows.start
                    && image.snapshot.row < visible_rows.end
            })
            .cloned()
            .map(|image| TerminalImageLayout { image })
            .collect::<Vec<_>>();
        images.sort_by_key(|image| {
            (
                image.image.snapshot.z_index,
                image
                    .image
                    .snapshot
                    .kitty
                    .as_ref()
                    .map_or(image.image.snapshot.id.0, |k| u64::from(k.image_id)),
            )
        });
        let mut text_runs = Vec::new();
        let mut timestamp_runs = Vec::new();
        let mut cached_rows = Vec::new();
        let mut cursor = None;
        let scrollbar = terminal_scrollbar_for_viewport_display_offset(
            &self.snapshot,
            &self.metrics,
            self.viewport_rows,
            self.scrollbar_display_offset,
        );
        let terminal_background = terminal_background(&self.theme);
        let cursor_row_visible = visible_rows.contains(&self.snapshot.cursor_row);
        let ime_cursor_bounds = cursor_row_visible
            .then(|| ime_cursor_bounds_for_snapshot(&self.snapshot, &self.metrics))
            .flatten();
        let marked_text = self.marked_text_run(ime_cursor_bounds.is_some());
        #[cfg(feature = "bench")]
        let links_started = Instant::now();
        let mut link_ranges = if let Some(cache) = cache.as_deref_mut() {
            self.cached_link_ranges_for_rows(visible_rows.clone(), cache)
        } else {
            persistent_link_ranges_for_rows(&self.snapshot, visible_rows.clone())
        };
        // Paths are hit-tested by the pointer handlers, not by the frame layout.
        // Keep persistent links first so an explicit hyperlink still wins.
        if let Some(path) = self.hovered_link.as_ref().filter(|link| {
            self.detect_file_paths_as_links
                && visible_rows.contains(&link.row)
                && self.snapshot.lines.get(link.row).is_some_and(|row| {
                    !row.active_input && link.detected_path_row_signature == Some(row.signature)
                })
        }) {
            let index = link_ranges.partition_point(|range| range.row <= path.row);
            link_ranges.insert(index, path.clone());
        }

        #[cfg(feature = "bench")]
        if let Some(cache) = cache.as_deref_mut() {
            cache.performance.links_micros = duration_micros(links_started.elapsed());
        }
        let row_terminal_background = self
            .transition_colors
            .as_ref()
            .map_or(terminal_background, |transition| {
                transition.target_background_hsla
            });
        let row_style_hasher = self.row_layout_style_hasher();
        let timestamp_style_signature = self
            .row_timestamps
            .is_some()
            .then(|| self.timestamp_style_signature());
        for row_index in visible_rows {
            let Some(row) = self.snapshot.lines.get(row_index) else {
                continue;
            };
            if let Some(cache) = cache.as_deref_mut() {
                let logical_line = logical_lines
                    .line_for_row(row_index)
                    .expect("visible snapshot rows must have logical line metadata");
                let semantic_role = semantic_roles.get(&logical_line.range).copied();
                let key = self.row_layout_cache_key_with_logical_line(
                    row_index,
                    logical_line,
                    semantic_role,
                    &row_style_hasher,
                );
                let row_layout = cache.get_or_insert_row_with(
                    key,
                    row.line_id,
                    self.performance_metrics_enabled,
                    |previous| {
                        // Cached rows already contain their resolved text colors. Materialize a logical
                        // line's foreground lookup only if one of its rows actually needs rebuilding.
                        if !cached_highlights.is_empty()
                            && foreground_lines.insert(logical_line.range.start)
                            && let Some((start_row, relative)) = cached_highlights
                                .iter()
                                .find(|(start, _)| *start == logical_line.range.start)
                        {
                            highlight_layout.foregrounds.extend(
                                relative
                                    .foregrounds
                                    .iter()
                                    .map(|((row, col), color)| ((start_row + row, *col), *color)),
                            );
                        }
                        self.row_layout(
                            row_index,
                            row,
                            &highlight_layout,
                            &link_ranges,
                            row_terminal_background,
                            previous,
                        )
                    },
                );
                if let Some(row_cursor) = row_layout.cursor {
                    cursor = Some(TerminalCursor {
                        row: row_index,
                        col: row_cursor.col,
                        shape: row_cursor.shape,
                    });
                }
                // Keep cached row data shared through paint instead of rebuilding flattened
                // vectors after every cache hit.
                cached_rows.push(CachedTerminalRowLayout {
                    row: row_index,
                    layout: row_layout,
                });
            } else {
                let row_layout = self.row_layout(
                    row_index,
                    row,
                    &highlight_layout,
                    &link_ranges,
                    row_terminal_background,
                    None,
                );
                append_cached_row_layout(
                    row_index,
                    &row_layout,
                    self.transition_colors.as_ref(),
                    terminal_background,
                    &mut backgrounds,
                    &mut selections,
                    &mut text_runs,
                    &mut cursor,
                );
            }
            if let Some(style_signature) = timestamp_style_signature
                && let Some(timestamp_run) = self.timestamp_run_for_row(
                    row_index,
                    terminal_row_timestamp_identity(row),
                    style_signature,
                    cache.as_deref_mut(),
                )
            {
                timestamp_runs.push(timestamp_run);
            }
        }
        if let Some(marked_text) = &marked_text {
            cursor = self.composition_caret(marked_text);
        }

        TerminalElementLayout {
            backgrounds,
            highlight_backgrounds: map_rects_to_visual(
                &self.snapshot,
                self.bidi_enabled,
                highlight_layout.backgrounds,
            ),
            highlight_underlines: map_rects_to_visual(
                &self.snapshot,
                self.bidi_enabled,
                highlight_layout.underlines,
            ),
            highlight_outlines: map_rects_to_visual(
                &self.snapshot,
                self.bidi_enabled,
                highlight_layout.outlines,
            ),
            search_scrollbar_markers: if self
                .search_query
                .as_ref()
                .is_some_and(|query| !query.is_empty())
            {
                search_scrollbar_markers(
                    &self.search_matches,
                    self.snapshot.scrollback_lines,
                    self.viewport_rows,
                    self.viewport_rows as f32 * self.metrics.line_height_f32(),
                    self.selected_search_match,
                )
            } else {
                Vec::new()
            },
            search_matches,
            selection_matches,
            command_mark_overlays,
            selections,
            images,
            text_runs,
            timestamp_runs,
            marked_text,
            ghost_text: self.ghost_text_run(cursor_row_visible),
            ime_cursor_bounds,
            cursor,
            scrollbar,
            cached_rows,
        }
    }

    fn timestamp_style_signature(&self) -> u64 {
        let mut hasher = new_cache_key_hasher();
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
        self.theme.header_foreground.hash(&mut hasher);
        hasher.finish()
    }

    fn timestamp_run_for_row(
        &self,
        row_index: usize,
        index: u64,
        style_signature: u64,
        cache: Option<&mut TerminalLayoutCache>,
    ) -> Option<BatchedTextRun> {
        let label = self.row_timestamps.as_ref()?.get(index)?.label.clone();
        let text = SharedString::from(label);
        // Many rows from the same PTY chunk share an identical formatted timestamp; reuse the
        // shaped `LineLayout` and glyph paint cache across both matching rows and consecutive frames.
        let run_cache = cache.map(|cache| {
            cache
                .timestamps
                .get_or_insert_with((text.clone(), style_signature), || {
                    Arc::new(TerminalTextRunCache::default())
                })
                .0
        });
        Some(BatchedTextRun {
            row: row_index,
            col: 0,
            cells: TERMINAL_TIMESTAMP_LABEL_CELLS,
            style: canvas_timestamp_text_run(&text, &self.theme, &self.metrics),
            custom_underline: None,
            text,
            cache: run_cache,
        })
    }

    fn frame_layout_cache_key(&self, visible_rows: &Range<usize>) -> u64 {
        let mut hasher = self.row_layout_style_hasher();
        visible_rows.start.hash(&mut hasher);
        visible_rows.end.hash(&mut hasher);
        self.viewport_rows.hash(&mut hasher);
        let painted_limit = self.painted_row_limit();
        painted_limit.hash(&mut hasher);
        self.scrollbar_display_offset.to_bits().hash(&mut hasher);
        self.metrics
            .letter_spacing
            .map(|value| f32::from(value).to_bits())
            .hash(&mut hasher);
        f32::from(self.metrics.primary_ascent)
            .to_bits()
            .hash(&mut hasher);
        f32::from(self.metrics.primary_descent)
            .to_bits()
            .hash(&mut hasher);
        self.highlight_ui_colors().hash(&mut hasher);

        self.snapshot.generation.hash(&mut hasher);
        self.snapshot.display_offset.hash(&mut hasher);
        self.snapshot.scrollback_lines.hash(&mut hasher);
        self.snapshot.rows.hash(&mut hasher);
        self.snapshot.cursor_row.hash(&mut hasher);
        self.snapshot.cursor_col.hash(&mut hasher);
        self.snapshot.cursor_shape.hash(&mut hasher);
        self.snapshot.lines.len().hash(&mut hasher);

        // Include wrapped neighbors outside `visible_rows` so clipped logical-line edits
        // invalidate semantic/highlight layout even when synthetic snapshots reuse generation.
        let mut content_start = visible_rows.start.min(self.snapshot.lines.len());
        while content_start > 0 && self.snapshot.lines[content_start - 1].wrapped {
            content_start -= 1;
        }
        let mut content_end = visible_rows.end.min(self.snapshot.lines.len());
        while content_end > content_start
            && content_end < self.snapshot.lines.len()
            && self.snapshot.lines[content_end - 1].wrapped
        {
            content_end += 1;
        }
        content_start.hash(&mut hasher);
        content_end.hash(&mut hasher);
        for row in &self.snapshot.lines[content_start..content_end] {
            hash_row_content(row, &mut hasher);
            row.wrapped.hash(&mut hasher);
            row.active_input.hash(&mut hasher);
        }

        self.cursor_visible.hash(&mut hasher);
        self.marked_text.hash(&mut hasher);
        self.marked_text_caret_utf16.hash(&mut hasher);
        self.ghost_text.hash(&mut hasher);
        if self.marked_text.is_some() || self.ghost_text.is_some() {
            self.theme.foreground.hash(&mut hasher);
            self.theme.background.hash(&mut hasher);
        }

        self.selection
            .map(|selection| {
                (
                    selection.anchor.line,
                    selection.anchor.col,
                    selection.head.line,
                    selection.head.col,
                    std::mem::discriminant(&selection.mode),
                )
            })
            .hash(&mut hasher);

        self.search_query.hash(&mut hasher);
        self.search_matches_precomputed.hash(&mut hasher);
        self.selected_search_match.hash(&mut hasher);
        (
            Arc::as_ptr(&self.search_matches) as *const () as usize,
            self.search_matches.len(),
        )
            .hash(&mut hasher);
        self.search_matches
            .first()
            .map(|m| (m.line, m.start_col, m.end_col))
            .hash(&mut hasher);
        self.search_matches
            .last()
            .map(|m| (m.line, m.start_col, m.end_col))
            .hash(&mut hasher);
        self.selection_highlight_query
            .as_deref()
            .map(|query| query.as_str())
            .hash(&mut hasher);
        if self.selection_highlight_query.is_some() {
            self.theme.tokens.ui.warning.hash(&mut hasher);
        }

        (
            Arc::as_ptr(&self.command_marks) as *const () as usize,
            self.command_marks.len(),
        )
            .hash(&mut hasher);
        self.command_marks
            .first()
            .map(|mark| {
                (
                    mark.command_id.as_str(),
                    mark.start_line,
                    mark.end_line,
                    mark.is_closed,
                    mark.exit_code,
                    mark.stale,
                    mark.finished_at,
                )
            })
            .hash(&mut hasher);
        self.command_marks
            .last()
            .map(|mark| {
                (
                    mark.command_id.as_str(),
                    mark.start_line,
                    mark.end_line,
                    mark.is_closed,
                    mark.exit_code,
                    mark.stale,
                    mark.finished_at,
                )
            })
            .hash(&mut hasher);
        self.selected_command_mark_id.hash(&mut hasher);
        self.hovered_command_mark_id.hash(&mut hasher);
        self.collapsed_commands.len().hash(&mut hasher);
        let mut collapsed_acc = 0u64;
        for id in &self.collapsed_commands {
            let mut item_hasher = new_cache_key_hasher();
            id.hash(&mut item_hasher);
            collapsed_acc = collapsed_acc.wrapping_add(item_hasher.finish());
        }
        collapsed_acc.hash(&mut hasher);
        self.fold_rotations.len().hash(&mut hasher);
        let mut rotation_acc = 0u64;
        for (id, rotation) in &self.fold_rotations {
            let mut item_hasher = new_cache_key_hasher();
            id.hash(&mut item_hasher);
            rotation.to_bits().hash(&mut item_hasher);
            rotation_acc = rotation_acc.wrapping_add(item_hasher.finish());
        }
        rotation_acc.hash(&mut hasher);

        self.highlight_rules
            .iter()
            .any(|rule| rule.enabled)
            .hash(&mut hasher);
        self.transient_command_highlight.is_some().hash(&mut hasher);
        self.semantic_coloring.hash(&mut hasher);
        self.semantic_shell.hash(&mut hasher);
        self.semantic_classification_signature.hash(&mut hasher);
        self.semantic_style_signature.hash(&mut hasher);
        self.semantic_scheme
            .as_ref()
            .map(|scheme| Arc::as_ptr(scheme) as usize)
            .hash(&mut hasher);
        self.hovered_link.hash(&mut hasher);

        let mut visible_image_count = 0usize;
        for image in &self.rendered_images {
            if image.snapshot.row < painted_limit
                && image.snapshot.row + image.snapshot.rows > visible_rows.start
                && image.snapshot.row < visible_rows.end
            {
                visible_image_count += 1;
                image.snapshot.id.0.hash(&mut hasher);
                image.snapshot.version.hash(&mut hasher);
                image.snapshot.row.hash(&mut hasher);
                image.snapshot.col.hash(&mut hasher);
                image.snapshot.rows.hash(&mut hasher);
                image.snapshot.cols.hash(&mut hasher);
                image.snapshot.z_index.hash(&mut hasher);
                image.snapshot.clipped_rows.hash(&mut hasher);
                image.snapshot.source_x.hash(&mut hasher);
                image.snapshot.source_y.hash(&mut hasher);
                image.snapshot.source_width.hash(&mut hasher);
                image.snapshot.source_height.hash(&mut hasher);
                image.snapshot.pixel_width.hash(&mut hasher);
                image.snapshot.pixel_height.hash(&mut hasher);
                image.snapshot.placeholder.hash(&mut hasher);
                image
                    .snapshot
                    .kitty
                    .as_ref()
                    .map(|placement| {
                        (
                            placement.serial,
                            placement.id,
                            placement.image_id,
                            placement.offset_x,
                            placement.offset_y,
                            placement.clip_top,
                            placement.clip_bottom,
                            placement.column_offset,
                            placement.hidden,
                            placement.scaled,
                        )
                    })
                    .hash(&mut hasher);
                image
                    .render_image
                    .as_ref()
                    .map(|img| Arc::as_ptr(img) as usize)
                    .hash(&mut hasher);
                image.playback.frame.hash(&mut hasher);
            }
        }
        visible_image_count.hash(&mut hasher);

        if let Some(store) = &self.row_timestamps {
            true.hash(&mut hasher);
            self.theme.header_foreground.hash(&mut hasher);
            for row_index in visible_rows.clone() {
                let label = self
                    .snapshot
                    .lines
                    .get(row_index)
                    .and_then(|row| store.get(terminal_row_timestamp_identity(row)))
                    .map(|timestamp| timestamp.label.as_str());
                label.hash(&mut hasher);
            }
        } else {
            false.hash(&mut hasher);
        }

        hasher.finish()
    }

    pub(super) fn cached_layout_for_bounds(
        &self,
        bounds: Bounds<Pixels>,
    ) -> Arc<TerminalElementLayout> {
        let Some(cache) = &self.layout_cache else {
            return Arc::new(self.layout_for_bounds(bounds));
        };
        let visible_rows = self.visible_rows_for_bounds(bounds);
        let frame_key = self.frame_layout_cache_key(&visible_rows);
        let mut cache = cache.borrow_mut();
        if let Some((cached_key, layout)) = &cache.last_frame
            && *cached_key == frame_key
        {
            let layout = Arc::clone(layout);
            #[cfg(feature = "bench")]
            {
                cache.performance.semantic_roles_micros = 0;
                cache.performance.links_micros = 0;
                cache.performance.highlights_micros = 0;
            }
            cache.record_cache_hits(
                visible_rows.len().max(1) as u64,
                self.performance_metrics_enabled,
            );
            return layout;
        }
        let layout = Arc::new(self.layout_for_rows(visible_rows, Some(&mut cache)));
        cache.last_frame = Some((frame_key, Arc::clone(&layout)));
        layout
    }

    pub(super) fn cached_highlight_layout_for_rows(
        &self,
        logical_lines: &TerminalLogicalLineIndex,
        semantic_roles: &FastHashMap<Range<usize>, SemanticLineRole>,
        cache: &mut TerminalLayoutCache,
    ) -> (
        TerminalHighlightLayout,
        Vec<(usize, Arc<TerminalLogicalHighlightLayout>)>,
    ) {
        let mut layout = TerminalHighlightLayout::empty();
        let mut cached = Vec::new();

        for logical_line in &logical_lines.lines {
            let semantic_role = semantic_roles.get(&logical_line.range).copied();
            let key =
                self.logical_highlight_cache_key_with_logical_line(logical_line, semantic_role);
            let relative_layout = if let Some(layout) = cache.highlights.get(&key) {
                cache.record_cache_access(true, self.performance_metrics_enabled);
                layout
            } else {
                // Classification is only needed when the visible layout cannot be reused.
                let spans = self.snapshot.logical_context_for_row(logical_line.range.start)
                    .filter(|_| self.semantic_coloring)
                    .map(|context| {
                        let role = semantic_role.unwrap_or(SemanticLineRole::Unknown);
                        cache.semantic_spans_for(
                            (context.signature, role, self.semantic_classification_signature),
                            || {
                                oxideterm_terminal_semantic::classify_line_with_compiled_scheme_and_shell(
                                    &context.text, role, self.semantic_scheme(), self.semantic_shell,
                                )
                            },
                        )
                    });
                cache.get_or_insert_highlight_with(key, self.performance_metrics_enabled, || {
                    let line_layout = self.highlight_layout_for_rows_with_spans(
                        logical_line.range.clone(),
                        spans.as_deref().map(Vec::as_slice),
                    );
                    relative_highlight_layout(logical_line.range.start, line_layout)
                })
            };
            append_relative_highlight_layout(
                logical_line.range.start,
                &relative_layout,
                &mut layout,
            );
            if !relative_layout.foregrounds.is_empty() {
                cached.push((logical_line.range.start, relative_layout));
            }
        }

        (layout, cached)
    }

    fn highlight_layout_for_logical_lines(
        &self,
        logical_lines: &TerminalLogicalLineIndex,
    ) -> TerminalHighlightLayout {
        let mut layout = TerminalHighlightLayout::empty();
        for logical_line in &logical_lines.lines {
            let mut line_layout = self.highlight_layout_for_rows(logical_line.range.clone());
            layout.backgrounds.append(&mut line_layout.backgrounds);
            layout.underlines.append(&mut line_layout.underlines);
            layout.outlines.append(&mut line_layout.outlines);
            layout.foregrounds.extend(line_layout.foregrounds);
        }
        layout
    }

    fn highlight_layout_for_rows(&self, rows: Range<usize>) -> TerminalHighlightLayout {
        self.highlight_layout_for_rows_with_spans(rows, None)
    }

    fn highlight_ui_colors(&self) -> (u32, u32) {
        self.transition_colors.as_ref().map_or(
            (self.theme.tokens.ui.error, self.theme.tokens.ui.warning),
            |transition| (transition.target_ui_error, transition.target_ui_warning),
        )
    }

    fn highlight_layout_for_rows_with_spans(
        &self,
        rows: Range<usize>,
        spans: Option<&[oxideterm_terminal_semantic::SemanticSpan]>,
    ) -> TerminalHighlightLayout {
        let (ui_error, ui_warning) = self.highlight_ui_colors();
        let transient = self.transient_command_highlight.as_ref().map(|highlight| {
            (
                highlight,
                rgba((ui_warning << 8) | TRANSIENT_COMMAND_HIGHLIGHT_ALPHA).into(),
            )
        });
        let mut layout = terminal_highlights_for_rows(
            &self.snapshot,
            &self.highlight_rules,
            transient,
            rows.clone(),
        );
        if self.semantic_coloring {
            let terminal_palette = self
                .transition_colors
                .as_ref()
                .map_or(self.theme.tokens.terminal, |transition| {
                    transition.target_terminal
                });
            append_terminal_semantics_with_palette(
                &self.snapshot,
                &self.command_marks,
                rows,
                terminal_palette,
                ui_error,
                ui_warning,
                self.semantic_scheme(),
                self.semantic_shell,
                &mut layout,
                spans,
            );
        }
        layout
    }

    fn cached_link_ranges_for_rows(
        &self,
        visible_rows: Range<usize>,
        cache: &mut TerminalLayoutCache,
    ) -> Vec<TerminalLinkRange> {
        let mut ranges = Vec::new();
        for row_index in visible_rows {
            if self.snapshot.lines.get(row_index).is_none() {
                continue;
            }
            let key = self.row_link_cache_key(row_index);
            let row_layout =
                cache.get_or_insert_links_with(key, self.performance_metrics_enabled, || {
                    relative_link_layout(persistent_link_ranges_for_rows(
                        &self.snapshot,
                        row_index..row_index + 1,
                    ))
                });
            ranges.extend(row_layout.ranges.iter().map(|range| TerminalLinkRange {
                row: row_index,
                start_col: range.start_col,
                end_col: range.end_col,
                target: range.target.clone(),
                kind: range.kind,
                detected_path_row_signature: None,
            }));
        }
        ranges
    }

    pub(super) fn logical_highlight_cache_key_with_logical_line(
        &self,
        logical_line: &TerminalLogicalLine,
        semantic_role: Option<SemanticLineRole>,
    ) -> TerminalLogicalHighlightCacheKey {
        let mut hasher = new_cache_key_hasher();
        self.snapshot.cols.hash(&mut hasher);
        self.highlight_rules_signature.hash(&mut hasher);
        self.transient_command_highlight_signature.hash(&mut hasher);
        // UI colors affect the cached line bands, not token classifications or row glyphs.
        if self.semantic_coloring {
            self.highlight_ui_colors().hash(&mut hasher);
        } else if self.transient_command_highlight.is_some() {
            self.highlight_ui_colors().1.hash(&mut hasher);
        }
        self.hash_semantic_layout(logical_line.range.clone(), semantic_role, &mut hasher);
        logical_line.signature.hash(&mut hasher);
        TerminalLogicalHighlightCacheKey {
            signature: hasher.finish(),
        }
    }

    pub(super) fn hash_semantic_layout(
        &self,
        rows: Range<usize>,
        semantic_role: Option<SemanticLineRole>,
        hasher: &mut impl Hasher,
    ) {
        if let Some(context) = self.snapshot.logical_context_for_row(rows.start) {
            context.signature.hash(hasher);
            for row in rows.clone() {
                if let Some(mapped) = context
                    .rows
                    .iter()
                    .find(|mapped| mapped.source_id == self.snapshot.lines[row].source_id)
                {
                    mapped.columns.hash(hasher);
                }
            }
        }
        self.semantic_style_signature.hash(hasher);
        if !self.semantic_coloring {
            return;
        }
        semantic_role
            .unwrap_or_else(|| {
                semantic_line_role_for_rows(&self.snapshot, &self.command_marks, rows)
            })
            .hash(hasher);
    }

    pub(super) fn logical_lines_for_rows(
        &self,
        visible_rows: Range<usize>,
    ) -> TerminalLogicalLineIndex {
        let visible_rows = visible_rows.start.min(self.snapshot.lines.len())
            ..visible_rows.end.min(self.snapshot.lines.len());
        let mut line_for_visible_row = vec![0; visible_rows.len()];
        let mut lines = Vec::new();
        let mut row_index = visible_rows.start;

        while row_index < visible_rows.end {
            let Some(range) = crate::highlight::logical_line_range(&self.snapshot, row_index)
            else {
                break;
            };
            let mut hasher = new_cache_key_hasher();
            range.len().hash(&mut hasher);
            if let Some(highlight) = &self.transient_command_highlight {
                logical_line_overlaps_transient_output(&self.snapshot, range.clone(), highlight)
                    .hash(&mut hasher);
            }
            for row in self.snapshot.lines.get(range.clone()).unwrap_or(&[]) {
                hash_row_content(row, &mut hasher);
            }
            let line_index = lines.len();
            let mapped_start = range.start.max(visible_rows.start);
            let mapped_end = range.end.min(visible_rows.end);
            for mapped_row in mapped_start..mapped_end {
                line_for_visible_row[mapped_row - visible_rows.start] = line_index;
            }
            lines.push(TerminalLogicalLine {
                range: range.clone(),
                signature: hasher.finish(),
            });
            row_index = range.end.max(row_index + 1);
        }

        TerminalLogicalLineIndex {
            visible_rows,
            line_for_visible_row,
            lines,
        }
    }

    pub(super) fn semantic_roles_for_lines(
        &self,
        logical_lines: &TerminalLogicalLineIndex,
        mut cache: Option<&mut TerminalLayoutCache>,
    ) -> FastHashMap<Range<usize>, SemanticLineRole> {
        if !self.semantic_coloring {
            return FastHashMap::new();
        }
        let mut roles = FastHashMap::new();
        if let Some(cache) = cache.as_deref_mut() {
            if cache
                .semantic_marks
                .as_ref()
                .is_none_or(|marks| !std::ptr::eq(marks.as_ptr(), Arc::as_ptr(&self.command_marks)))
            {
                cache.semantic_roles.entries.clear();
                cache.semantic_roles.insertion_order.clear();
                // Weak identity versions the immutable mark snapshot without keeping it alive.
                cache.semantic_marks = Some(Arc::downgrade(&self.command_marks));
            }
        }
        let viewport_start = self
            .snapshot
            .scrollback_lines
            .saturating_sub(self.snapshot.display_offset);
        // Layout already resolved wrapped ranges; sharing them avoids rescanning
        // the same logical line for each physical row in the viewport.
        for line in &logical_lines.lines {
            let classify = || {
                semantic_line_role_for_rows(&self.snapshot, &self.command_marks, line.range.clone())
            };
            let role = if let Some(cache) = cache.as_deref_mut().filter(|_| {
                !self.command_marks.is_empty()
                    && self
                        .snapshot
                        .logical_context_for_row(line.range.start)
                        .is_none()
            }) {
                let active_input = self
                    .snapshot
                    .lines
                    .get(line.range.clone())
                    .unwrap_or(&[])
                    .iter()
                    .any(|row| row.active_input);
                let key = (
                    viewport_start.saturating_add(line.range.start),
                    viewport_start.saturating_add(line.range.end.saturating_sub(1)),
                    active_input,
                );
                cache.semantic_roles.get_or_insert_with(key, classify).0
            } else {
                classify()
            };
            roles.insert(line.range.clone(), role);
        }
        roles
    }

    pub(super) fn row_link_cache_key(&self, row_index: usize) -> TerminalRowLinkCacheKey {
        let mut hasher = new_cache_key_hasher();
        if let Some(row) = self.snapshot.lines.get(row_index) {
            hash_row_content(row, &mut hasher);
        }
        TerminalRowLinkCacheKey {
            signature: hasher.finish(),
        }
    }
}

fn relative_highlight_layout(
    start_row: usize,
    layout: TerminalHighlightLayout,
) -> TerminalLogicalHighlightLayout {
    TerminalLogicalHighlightLayout {
        backgrounds: relative_highlight_rects(start_row, layout.backgrounds),
        underlines: relative_highlight_rects(start_row, layout.underlines),
        outlines: relative_highlight_rects(start_row, layout.outlines),
        foregrounds: layout
            .foregrounds
            .into_iter()
            .filter_map(|((row, col), color)| {
                row.checked_sub(start_row)
                    .map(|row_offset| ((row_offset, col), color))
            })
            .collect(),
    }
}

fn relative_highlight_rects(
    start_row: usize,
    rects: Vec<TerminalRect>,
) -> Vec<TerminalRowOffsetRect> {
    rects
        .into_iter()
        .filter_map(|rect| {
            rect.row
                .checked_sub(start_row)
                .map(|row_offset| TerminalRowOffsetRect {
                    row_offset,
                    col: rect.col,
                    cells: rect.cells,
                    color: rect.color,
                })
        })
        .collect()
}

fn append_relative_highlight_layout(
    start_row: usize,
    relative: &TerminalLogicalHighlightLayout,
    layout: &mut TerminalHighlightLayout,
) {
    layout.backgrounds.extend(
        relative
            .backgrounds
            .iter()
            .map(|rect| absolute_highlight_rect(start_row, rect)),
    );
    layout.underlines.extend(
        relative
            .underlines
            .iter()
            .map(|rect| absolute_highlight_rect(start_row, rect)),
    );
    layout.outlines.extend(
        relative
            .outlines
            .iter()
            .map(|rect| absolute_highlight_rect(start_row, rect)),
    );
}

fn absolute_highlight_rect(start_row: usize, rect: &TerminalRowOffsetRect) -> TerminalRect {
    TerminalRect {
        row: start_row + rect.row_offset,
        col: rect.col,
        cells: rect.cells,
        color: rect.color,
        powerline_cutout: None,
    }
}

fn relative_link_layout(ranges: Vec<TerminalLinkRange>) -> TerminalRowLinkLayout {
    TerminalRowLinkLayout {
        ranges: ranges
            .into_iter()
            .map(|range| TerminalRelativeLinkRange {
                start_col: range.start_col,
                end_col: range.end_col,
                target: range.target,
                kind: range.kind,
            })
            .collect(),
    }
}

fn hash_highlight_rules(rules: &[TerminalHighlightRule], hasher: &mut impl Hasher) {
    rules.len().hash(hasher);
    for rule in rules {
        rule.id.hash(hasher);
        rule.pattern.hash(hasher);
        rule.is_regex.hash(hasher);
        rule.case_sensitive.hash(hasher);
        rule.foreground.hash(hasher);
        rule.background.hash(hasher);
        match rule.render_mode {
            TerminalHighlightRenderMode::Background => 0u8,
            TerminalHighlightRenderMode::Underline => 1,
            TerminalHighlightRenderMode::Outline => 2,
        }
        .hash(hasher);
        match rule.match_scope {
            crate::theme::TerminalHighlightMatchScope::Match => 0u8,
            crate::theme::TerminalHighlightMatchScope::LogicalLine => 1,
        }
        .hash(hasher);
        rule.preserve_background.hash(hasher);
        rule.enabled.hash(hasher);
        rule.priority.hash(hasher);
    }
}

pub(super) fn terminal_highlight_rules_signature(rules: &[TerminalHighlightRule]) -> u64 {
    let mut hasher = new_cache_key_hasher();
    hash_highlight_rules(rules, &mut hasher);
    hasher.finish()
}

pub(super) fn terminal_transient_command_highlight_signature(
    highlight: Option<&TransientCommandHighlight>,
) -> u64 {
    let mut hasher = new_cache_key_hasher();
    highlight.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn terminal_semantic_classification_signature(
    enabled: bool,
    scheme: &CompiledSemanticScheme,
    shell: SemanticShellDialect,
) -> u64 {
    let mut hasher = new_cache_key_hasher();
    enabled.hash(&mut hasher);
    if enabled {
        scheme.signature().hash(&mut hasher);
        shell.hash(&mut hasher);
    }
    hasher.finish()
}

pub(super) fn terminal_semantic_style_signature_from_palette(
    enabled: bool,
    terminal: oxideterm_theme::TerminalTheme,
    scheme: &CompiledSemanticScheme,
    shell: SemanticShellDialect,
) -> u64 {
    let mut hasher = new_cache_key_hasher();
    enabled.hash(&mut hasher);
    if enabled {
        terminal.red.hash(&mut hasher);
        terminal.bright_red.hash(&mut hasher);
        terminal.green.hash(&mut hasher);
        terminal.bright_green.hash(&mut hasher);
        terminal.yellow.hash(&mut hasher);
        terminal.bright_yellow.hash(&mut hasher);
        terminal.blue.hash(&mut hasher);
        terminal.bright_blue.hash(&mut hasher);
        terminal.magenta.hash(&mut hasher);
        terminal.bright_magenta.hash(&mut hasher);
        terminal.cyan.hash(&mut hasher);
        terminal.bright_cyan.hash(&mut hasher);
        terminal.bright_black.hash(&mut hasher);
        scheme.signature().hash(&mut hasher);
        shell.hash(&mut hasher);
    }
    hasher.finish()
}

fn command_mark_overlays_for_rows(
    snapshot: &TerminalSnapshot,
    marks: &[CommandMarkAnnotation],
    selected_command_mark_id: Option<&str>,
    hovered_command_mark_id: Option<&str>,
    collapsed_commands: &HashSet<String>,
    fold_rotations: &HashMap<String, f32>,
) -> Vec<TerminalCommandMarkOverlay> {
    let viewport_start = snapshot
        .scrollback_lines
        .saturating_sub(snapshot.display_offset);
    let viewport_end = viewport_start.saturating_add(snapshot.lines.len().saturating_sub(1));
    let total_lines = snapshot.scrollback_lines.saturating_add(snapshot.rows);
    let mut overlays = Vec::new();

    for mark in marks {
        let start_line = mark.start_line;
        let end_line = mark.end_line.unwrap_or_else(|| {
            if mark.has_output_start {
                return snapshot_absolute_cursor_line(snapshot).max(mark.start_line);
            }
            snapshot_prompt_block_start_line(snapshot, snapshot_absolute_cursor_line(snapshot))
                .saturating_sub(1)
                .max(mark.start_line)
        });
        if end_line < start_line || end_line < viewport_start || start_line > viewport_end {
            continue;
        }

        let selected = selected_command_mark_id == Some(mark.command_id.as_str());
        let hovered = !selected && hovered_command_mark_id == Some(mark.command_id.as_str());
        let visible_start_line = start_line.max(viewport_start);
        let visible_end_line = end_line.min(viewport_end);
        let collapsed = collapsed_commands.contains(&mark.command_id);
        let foldable = collapsed
            || mark
                .foldable_output
                .as_ref()
                .is_some_and(|range| range.end <= total_lines);
        let fold_rotation = fold_rotations
            .get(&mark.command_id)
            .copied()
            .unwrap_or(if collapsed { 1.0 } else { 0.0 })
            .clamp(0.0, 1.0);
        overlays.push(TerminalCommandMarkOverlay {
            start_row: visible_start_line.saturating_sub(viewport_start),
            end_row: visible_end_line.saturating_sub(viewport_start),
            has_top: start_line >= viewport_start && !mark.command_line_clipped,
            has_bottom: end_line <= viewport_end,
            stale: mark.stale,
            selected,
            hovered,
            running: !mark.is_closed,
            exit_code: mark.exit_code,
            collapsed,
            foldable,
            fold_rotation,
        });
    }

    // Passive command edges should stay behind hover/selection fills when ranges overlap.
    overlays.sort_by_key(|overlay| (overlay.selected, overlay.hovered));
    overlays
}

fn snapshot_absolute_cursor_line(snapshot: &TerminalSnapshot) -> usize {
    snapshot
        .scrollback_lines
        .saturating_add(snapshot.cursor_row)
        .saturating_sub(snapshot.display_offset)
}

fn snapshot_prompt_block_start_line(snapshot: &TerminalSnapshot, command_line: usize) -> usize {
    if !snapshot_line_text(snapshot, command_line).is_some_and(is_likely_prompt_input_line) {
        return command_line;
    }

    let mut start_line = command_line;
    let min_line = command_line.saturating_sub(3);
    for line in (min_line..command_line).rev() {
        if !snapshot_line_text(snapshot, line).is_some_and(is_likely_prompt_preamble_line) {
            break;
        }
        start_line = line;
    }
    start_line
}

fn snapshot_line_text(snapshot: &TerminalSnapshot, absolute_line: usize) -> Option<String> {
    let viewport_start = snapshot
        .scrollback_lines
        .saturating_sub(snapshot.display_offset);
    let row = absolute_line.checked_sub(viewport_start)?;
    snapshot.lines.get(row).map(|line| line.text())
}

fn is_likely_prompt_input_line(text: String) -> bool {
    let trimmed = text.trim();
    trimmed.is_empty()
        || trimmed.chars().next().is_some_and(|ch| {
            matches!(
                ch,
                '❯' | '➜' | 'λ' | '>' | '$' | '#' | '%' | '❮' | '›' | '»'
            )
        })
}

fn is_likely_prompt_preamble_line(text: String) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let has_private_use_glyph = trimmed
        .chars()
        .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch));
    let has_powerline_glyph = trimmed
        .chars()
        .any(|ch| matches!(ch, '' | '' | '' | ''));
    let has_ruler = has_repeated_ruler(trimmed);
    let has_clock = has_clock_like_text(trimmed);
    let has_prompt_context = trimmed.contains('@')
        || trimmed.contains('~')
        || trimmed.contains('/')
        || trimmed.contains('$');

    has_powerline_glyph
        || (has_private_use_glyph && (has_clock || has_ruler || has_prompt_context))
        || (has_ruler && (has_clock || has_prompt_context))
}

fn has_repeated_ruler(text: &str) -> bool {
    let mut count = 0;
    for ch in text.chars() {
        if matches!(ch, '·' | '•' | '∙' | '.') {
            count += 1;
            if count >= 6 {
                return true;
            }
        } else {
            count = 0;
        }
    }
    false
}

fn has_clock_like_text(text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_digit() && ch != ':')
        .any(|part| {
            let pieces = part.split(':').collect::<Vec<_>>();
            match pieces.as_slice() {
                [hour, minute] | [hour, minute, ..] => {
                    (1..=2).contains(&hour.len()) && minute.len() == 2
                }
                _ => false,
            }
        })
}
