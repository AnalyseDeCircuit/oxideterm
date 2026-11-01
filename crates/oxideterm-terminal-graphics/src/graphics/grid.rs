use alacritty_terminal::term::graphics::{GraphicsGridChange, GraphicsGridEvents};

impl GraphicsIngress {
    pub fn attach_grid(&mut self, events: GraphicsGridEvents) {
        events.set_sixel_enabled(self.options.enabled && self.options.sixel);
        events.set_byte_limit(MAX_GRAPHICS_METADATA_MB as usize * 1024 * 1024);
        self.grid_events = Some(events);
        self.track_sixel_screens();
    }

    /// Apply model changes before the next graphics command, not at repaint time.
    pub fn synchronize_grid(&mut self) -> Vec<TerminalGraphicsEvent> {
        let Some(grid) = self.grid_events.clone() else {
            return Vec::new();
        };
        let changes = grid.drain();
        if changes.is_empty() {
            return Vec::new();
        }
        let mut events = Vec::new();
        for change in changes {
            let change = match change {
                GraphicsGridChange::Clear {
                    alternate: false,
                    history: true,
                } => GraphicsGridChange::TrimHistory { topmost: 0 },
                GraphicsGridChange::Clear {
                    alternate: true,
                    history: true,
                } => continue,
                change => change,
            };
            self.synchronize_sixel_grid(&change, &grid, &mut events);
            match change {
                GraphicsGridChange::Erase { .. } | GraphicsGridChange::ShiftColumns { .. } => {}
                GraphicsGridChange::SixelPaletteReset => {
                    if let Some(state) = &mut self.sixel_state {
                        state.colors.fill(None);
                    }
                }
                GraphicsGridChange::Placeholders {
                    alternate,
                    line,
                    runs,
                } => self.update_placeholder_row(alternate, line, runs, &mut events),
                GraphicsGridChange::DropImages { alternate } => {
                    self.drop_kitty_resources(&mut events);
                    self.alternate_screen = alternate;
                    events.push(TerminalGraphicsEvent::Screen { alternate });
                }
                GraphicsGridChange::Reset => {
                    self.sixel_state = None;
                    self.placeholder_rows.clear();
                    self.placeholder_rendered.clear();
                    self.kitty_placements.clear();
                    self.kitty_images.clear();
                    self.kitty_numbers.clear();
                    self.kitty_wire_ids.clear();
                    self.image_order.clear();
                    self.storage_bytes = 0;
                    self.alternate_screen = false;
                    events.push(TerminalGraphicsEvent::Delete { id: None });
                    events.push(TerminalGraphicsEvent::Screen { alternate: false });
                }
                GraphicsGridChange::TrimHistory { topmost } => {
                    self.placeholder_rows.retain(|(screen, line), runs| {
                        let keep = *screen || *line >= topmost;
                        if !keep {
                            for run in runs {
                                grid.set_anchor(*screen, run.serial, None);
                            }
                        }
                        keep
                    });
                    self.placeholders_changed = true;
                    self.kitty_placements.retain_mut(|p| {
                        if p.alternate_screen
                            || p.kitty
                                .as_ref()
                                .is_some_and(|k| k.virtual_placement || k.parent.is_some())
                        {
                            return true;
                        }
                        let kitty = p.kitty.as_mut().unwrap();
                        if p.line.saturating_add(p.rows.min(i32::MAX as usize) as i32) <= topmost {
                            grid.set_anchor(false, kitty.serial, None);
                            events.push(TerminalGraphicsEvent::DeletePlacement {
                                serial: kitty.serial,
                            });
                            false
                        } else {
                            kitty.clip_top = kitty
                                .clip_top
                                .max(topmost.saturating_sub(p.line).max(0) as usize);
                            grid.set_anchor(
                                false,
                                kitty.serial,
                                Some(alacritty_terminal::index::Point::new(
                                    alacritty_terminal::index::Line(
                                        p.line.saturating_add(kitty.clip_top as i32),
                                    ),
                                    alacritty_terminal::index::Column(p.col),
                                )),
                            );
                            events.push(TerminalGraphicsEvent::Place(p.clone()));
                            true
                        }
                    });
                }
                GraphicsGridChange::Screen { alternate } => {
                    self.alternate_screen = alternate;
                    if alternate {
                        self.placeholder_rows.retain(|(screen, _), runs| {
                            if *screen {
                                for run in runs {
                                    grid.set_anchor(*screen, run.serial, None);
                                }
                            }
                            !screen
                        });
                        let resources = self
                            .kitty_wire_ids
                            .iter()
                            .filter_map(|((screen, _), id)| (*screen).then_some(*id))
                            .collect::<Vec<_>>();
                        for id in resources {
                            self.kitty_images.remove(&id);
                            self.account_image_event(&TerminalGraphicsEvent::Delete {
                                id: Some(id),
                            });
                            events.push(TerminalGraphicsEvent::Delete { id: Some(id) });
                        }
                        self.kitty_placements.retain(|p| {
                            if p.alternate_screen {
                                events.push(TerminalGraphicsEvent::DeletePlacement {
                                    serial: p.kitty.as_ref().unwrap().serial,
                                });
                            }
                            !p.alternate_screen
                        });
                    }
                    events.push(TerminalGraphicsEvent::Screen { alternate });
                    self.placeholders_changed = true;
                }
                GraphicsGridChange::Clear { alternate, .. } => {
                    self.placeholders_changed = true;
                    self.kitty_placements.retain(|p| {
                        let visible =
                            p.line.saturating_add(p.rows.min(i32::MAX as usize) as i32) > 0;
                        let remove = p.alternate_screen == alternate
                            && visible
                            && !p.kitty.as_ref().unwrap().virtual_placement;
                        if remove {
                            let serial = p.kitty.as_ref().unwrap().serial;
                            grid.set_anchor(alternate, serial, None);
                            events.push(TerminalGraphicsEvent::DeletePlacement { serial });
                        }
                        !remove
                    });
                }
                GraphicsGridChange::Scroll {
                    alternate,
                    start,
                    end,
                    delta,
                    topmost,
                    history,
                } => {
                    self.scroll_placeholders(alternate, start, end, delta, topmost, history);
                    self.kitty_placements.retain_mut(|p| {
                        if p.alternate_screen != alternate
                            || p.kitty
                                .as_ref()
                                .is_some_and(|k| k.virtual_placement || k.parent.is_some())
                        {
                            return true;
                        }
                        let bottom = p.line.saturating_add(p.rows.min(i32::MAX as usize) as i32);
                        let kitty = p.kitty.as_ref().unwrap();
                        let visible_top = p
                            .line
                            .saturating_add(kitty.clip_top.min(i32::MAX as usize) as i32);
                        let visible_bottom =
                            bottom.saturating_sub(kitty.clip_bottom.min(i32::MAX as usize) as i32);
                        let history_scroll = history && delta < 0;
                        if history_scroll || (start <= visible_top && visible_bottom <= end) {
                            p.line = p.line.saturating_add(delta);
                            let kitty = p.kitty.as_mut().unwrap();
                            if !history_scroll {
                                kitty.clip_top = kitty
                                    .clip_top
                                    .max(start.saturating_sub(p.line).max(0) as usize);
                                kitty.clip_bottom = kitty.clip_bottom.max(
                                    p.line
                                        .saturating_add(p.rows.min(i32::MAX as usize) as i32)
                                        .saturating_sub(end)
                                        .max(0) as usize,
                                );
                            }
                            if p.line.saturating_add(p.rows.min(i32::MAX as usize) as i32)
                                <= topmost
                                || kitty.clip_top + kitty.clip_bottom >= p.rows
                            {
                                grid.set_anchor(alternate, kitty.serial, None);
                                events.push(TerminalGraphicsEvent::DeletePlacement {
                                    serial: kitty.serial,
                                });
                                return false;
                            }
                            grid.set_anchor(
                                alternate,
                                kitty.serial,
                                Some(alacritty_terminal::index::Point::new(
                                    alacritty_terminal::index::Line(
                                        p.line.saturating_add(kitty.clip_top as i32),
                                    ),
                                    alacritty_terminal::index::Column(p.col),
                                )),
                            );
                            events.push(TerminalGraphicsEvent::Place(p.clone()));
                        }
                        true
                    });
                }
                GraphicsGridChange::Reflow {
                    alternate,
                    anchors,
                    columns,
                } => {
                    let anchors: HashMap<_, _> = anchors.into_iter().collect();
                    self.reflow_placeholders(alternate, &anchors, columns);
                    self.kitty_placements.retain_mut(|p| {
                        if p.alternate_screen != alternate
                            || p.kitty
                                .as_ref()
                                .is_some_and(|k| k.virtual_placement || k.parent.is_some())
                        {
                            return true;
                        }
                        let serial = p.kitty.as_ref().unwrap().serial;
                        if let Some(point) = anchors.get(&serial) {
                            p.line = point.line.0.saturating_sub(
                                p.kitty.as_ref().unwrap().clip_top.min(i32::MAX as usize) as i32,
                            );
                            p.col = point.column.0;
                            events.push(TerminalGraphicsEvent::Place(p.clone()));
                            true
                        } else {
                            events.push(TerminalGraphicsEvent::DeletePlacement { serial });
                            false
                        }
                    });
                }
            }
        }
        self.refresh_placeholders(&mut events);
        self.cascade_missing_parents(&mut events);
        self.refresh_relatives(&mut events);
        events
    }

    fn emit_grid_changes<'a>(&mut self, emit: &mut dyn FnMut(TerminalGraphicsSegment<'a>)) {
        for event in self.synchronize_grid() {
            emit(TerminalGraphicsSegment::Event(event));
        }
    }
}
