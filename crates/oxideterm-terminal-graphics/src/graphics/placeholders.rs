use alacritty_terminal::term::graphics::PlaceholderRun;

struct IndexedPlaceholder {
    run: PlaceholderRun,
    serial: u64,
}

impl GraphicsIngress {
    fn update_placeholder_row(
        &mut self,
        alternate: bool,
        line: i32,
        runs: Vec<PlaceholderRun>,
        events: &mut Vec<TerminalGraphicsEvent>,
    ) {
        if self
            .placeholder_rows
            .get(&(alternate, line))
            .is_some_and(|old| {
                old.len() == runs.len() && old.iter().zip(&runs).all(|(a, b)| a.run == *b)
            })
        {
            return;
        }
        if let Some(old) = self.placeholder_rows.remove(&(alternate, line)) {
            for item in old {
                if let Some(grid) = &self.grid_events {
                    grid.set_anchor(alternate, item.serial, None);
                }
                if self.placeholder_rendered.remove(&item.serial).is_some() {
                    events.push(TerminalGraphicsEvent::DeletePlacement {
                        serial: item.serial,
                    });
                }
            }
        }
        if !runs.is_empty() {
            let indexed = runs
                .into_iter()
                .map(|run| {
                    let serial = self.next_placement_serial;
                    self.next_placement_serial += 1;
                    if let Some(grid) = &self.grid_events {
                        grid.set_anchor(
                            alternate,
                            serial,
                            Some(alacritty_terminal::index::Point::new(
                                alacritty_terminal::index::Line(line),
                                alacritty_terminal::index::Column(run.col),
                            )),
                        );
                    }
                    IndexedPlaceholder { run, serial }
                })
                .collect();
            self.placeholder_rows.insert((alternate, line), indexed);
        }
        self.placeholders_changed = true;
    }

    fn placeholder_placement(
        &self,
        alternate: bool,
        line: i32,
        item: &IndexedPlaceholder,
    ) -> Option<TerminalImagePlacement> {
        let id = *self.kitty_wire_ids.get(&(alternate, item.run.image))?;
        let prototype = self.kitty_placements.iter().rev().find(|p| {
            p.id == id
                && p.alternate_screen == alternate
                && p.kitty.as_ref().is_some_and(|k| {
                    k.virtual_placement && (item.run.placement == 0 || k.id == item.run.placement)
                })
        })?;
        if item.run.image_col >= prototype.cols || item.run.image_row >= prototype.rows {
            return None;
        }
        let mut placement = prototype.clone();
        placement.line = line;
        placement.col = item.run.col;
        let kitty = placement.kitty.as_mut()?;
        kitty.serial = item.serial;
        kitty.virtual_placement = false;
        kitty.hidden = false;
        kitty.unicode = Some(KittyPlaceholderRegion {
            row: item.run.image_row,
            col: item.run.image_col,
            cols: item.run.len.min(prototype.cols - item.run.image_col),
        });
        Some(placement)
    }

    fn refresh_placeholders(&mut self, events: &mut Vec<TerminalGraphicsEvent>) {
        if !self.placeholders_changed {
            return;
        }
        self.placeholders_changed = false;
        let metadata = self
            .placeholder_rows
            .values()
            .map(|runs| {
                runs.len()
                    * (std::mem::size_of::<IndexedPlaceholder>()
                        + std::mem::size_of::<TerminalImagePlacement>() * 3)
            })
            .sum::<usize>();
        if enforce_storage_limit(metadata, MAX_GRAPHICS_METADATA_MB).is_err() {
            self.drop_kitty_resources(events);
            return;
        }
        let mut retained = std::collections::HashSet::new();
        for ((alternate, line), runs) in &self.placeholder_rows {
            for run in runs {
                if let Some(placement) = self.placeholder_placement(*alternate, *line, run) {
                    retained.insert(run.serial);
                    if self.placeholder_rendered.get(&run.serial) != Some(&placement) {
                        events.push(TerminalGraphicsEvent::Place(placement.clone()));
                        self.placeholder_rendered.insert(run.serial, placement);
                    }
                }
            }
        }
        self.placeholder_rendered.retain(|serial, _| {
            if !retained.contains(serial) {
                events.push(TerminalGraphicsEvent::DeletePlacement { serial: *serial });
                false
            } else {
                true
            }
        });
    }

    fn drop_kitty_resources(&mut self, events: &mut Vec<TerminalGraphicsEvent>) {
        for id in self.kitty_images.keys().copied().collect::<Vec<_>>() {
            self.account_image_event(&TerminalGraphicsEvent::Delete { id: Some(id) });
            events.push(TerminalGraphicsEvent::Delete { id: Some(id) });
        }
        if let Some(grid) = &self.grid_events {
            for ((alternate, _), runs) in &self.placeholder_rows {
                for run in runs {
                    grid.set_anchor(*alternate, run.serial, None);
                }
            }
        }
        self.placeholder_rows.clear();
        self.placeholder_rendered.clear();
        self.placeholder_index_incomplete = true;
    }

    fn scroll_placeholders(
        &mut self,
        alternate: bool,
        start: i32,
        end: i32,
        delta: i32,
        topmost: i32,
        history: bool,
    ) {
        let rows = std::mem::take(&mut self.placeholder_rows);
        for ((screen, mut line), runs) in rows {
            let moved =
                screen == alternate && ((history && delta < 0) || (start <= line && line < end));
            if moved {
                line = line.saturating_add(delta);
            }
            let retained = !moved
                || (line >= topmost && ((history && delta < 0) || line >= start && line < end));
            if retained {
                self.placeholder_rows.insert((screen, line), runs);
            } else if let Some(grid) = &self.grid_events {
                for run in runs {
                    grid.set_anchor(screen, run.serial, None);
                }
            }
        }
        self.placeholders_changed = true;
    }

    fn reflow_placeholders(
        &mut self,
        alternate: bool,
        anchors: &HashMap<u64, alacritty_terminal::index::Point>,
        columns: usize,
    ) {
        let rows = std::mem::take(&mut self.placeholder_rows);
        for ((screen, line), runs) in rows {
            if screen != alternate {
                self.placeholder_rows.insert((screen, line), runs);
                continue;
            }
            for item in runs {
                let Some(point) = anchors.get(&item.serial) else {
                    if let Some(grid) = &self.grid_events {
                        grid.set_anchor(alternate, item.serial, None);
                    }
                    continue;
                };
                let mut remaining = item.run.len;
                let (mut row, mut col, mut source_col, mut serial) = (
                    point.line.0,
                    point.column.0,
                    item.run.image_col,
                    item.serial,
                );
                while remaining > 0 && columns > col {
                    let count = remaining.min(columns - col);
                    let mut run = item.run.clone();
                    run.col = col;
                    run.len = count;
                    run.image_col = source_col;
                    if let Some(grid) = &self.grid_events {
                        grid.set_anchor(
                            alternate,
                            serial,
                            Some(alacritty_terminal::index::Point::new(
                                alacritty_terminal::index::Line(row),
                                alacritty_terminal::index::Column(col),
                            )),
                        );
                    }
                    self.placeholder_rows
                        .entry((alternate, row))
                        .or_default()
                        .push(IndexedPlaceholder { run, serial });
                    remaining -= count;
                    source_col += count;
                    row += 1;
                    col = 0;
                    if remaining > 0 {
                        serial = self.next_placement_serial;
                        self.next_placement_serial += 1;
                    }
                }
            }
        }
        self.placeholders_changed = true;
    }
}
