impl GraphicsIngress {
    fn track_sixel_screens(&self) {
        if let Some(grid) = &self.grid_events {
            let (mut active, mut fixed) = ([false; 2], [false; 2]);
            for p in &self.sixel_placements {
                // Text cannot overwrite history. Keep its anchors, but avoid generating
                // erase events solely because an off-screen image is still retained.
                if p.line
                    .saturating_add(p.rows as i32)
                    .saturating_sub(p.sixel.as_ref().unwrap().clip_bottom as i32)
                    <= 0
                {
                    continue;
                }
                let screen = usize::from(p.alternate_screen);
                active[screen] = true;
                fixed[screen] |= p.sixel.as_ref().unwrap().fixed;
            }
            grid.set_sixel_screens(active, fixed);
        }
    }

    fn synchronize_sixel_grid(
        &mut self,
        change: &GraphicsGridChange,
        grid: &GraphicsGridEvents,
        events: &mut Vec<TerminalGraphicsEvent>,
    ) {
        match *change {
            GraphicsGridChange::Erase {
                alternate,
                top,
                bottom,
                left,
                right,
            } => {
                self.edit_sixel_cells(
                    alternate,
                    SixelCellRect {
                        top,
                        bottom,
                        left: left as i64,
                        right: right as i64,
                    },
                    0,
                    grid,
                    events,
                );
                return;
            }
            GraphicsGridChange::ShiftColumns {
                alternate,
                line,
                start,
                end,
                delta,
            } => {
                self.edit_sixel_cells(
                    alternate,
                    SixelCellRect {
                        top: line,
                        bottom: line + 1,
                        left: start as i64,
                        right: end as i64,
                    },
                    i64::from(delta),
                    grid,
                    events,
                );
                return;
            }
            _ => {}
        }
        if self.sixel_placements.is_empty()
            || !matches!(
                change,
                GraphicsGridChange::Reset
                    | GraphicsGridChange::DropImages { .. }
                    | GraphicsGridChange::Screen { alternate: true }
                    | GraphicsGridChange::Clear { .. }
                    | GraphicsGridChange::TrimHistory { .. }
                    | GraphicsGridChange::Scroll { .. }
                    | GraphicsGridChange::Reflow { .. }
            )
        {
            return;
        }
        let reflow = match change {
            GraphicsGridChange::Reflow { anchors, .. } => {
                anchors.iter().copied().collect::<HashMap<_, _>>()
            }
            _ => HashMap::new(),
        };
        let old_ids = self
            .sixel_placements
            .iter()
            .map(|p| p.id)
            .collect::<std::collections::HashSet<_>>();
        let mut updated = Vec::new();
        for mut p in std::mem::take(&mut self.sixel_placements) {
            let original = p.clone();
            let meta = p.sixel.as_mut().unwrap();
            let serial = meta.serial;
            let mut remove = false;
            match change {
                GraphicsGridChange::Reset | GraphicsGridChange::DropImages { .. } => remove = true,
                GraphicsGridChange::Screen { alternate: true } if p.alternate_screen => {
                    remove = true
                }
                GraphicsGridChange::Clear { alternate, .. } if p.alternate_screen == *alternate => {
                    remove = p
                        .line
                        .saturating_add(p.rows as i32)
                        .saturating_sub(meta.clip_bottom as i32)
                        > 0;
                }
                GraphicsGridChange::TrimHistory { topmost }
                    if !p.alternate_screen && !meta.fixed =>
                {
                    meta.clip_top = meta
                        .clip_top
                        .max(topmost.saturating_sub(p.line).max(0) as usize);
                    remove = meta.clip_top + meta.clip_bottom >= p.rows;
                }
                GraphicsGridChange::Scroll {
                    alternate,
                    start,
                    end,
                    delta,
                    topmost,
                    history,
                } if p.alternate_screen == *alternate && !meta.fixed => {
                    let top = p.line.saturating_add(meta.clip_top as i32);
                    let bottom = p
                        .line
                        .saturating_add(p.rows as i32)
                        .saturating_sub(meta.clip_bottom as i32);
                    if *history && *delta < 0 {
                        p.line = p.line.saturating_add(*delta);
                        meta.clip_top = meta
                            .clip_top
                            .max(topmost.saturating_sub(p.line).max(0) as usize);
                    } else if top < *end && bottom > *start {
                        // Preserve portions outside a local scrolling region instead of moving
                        // or deleting the whole image. The fragments still share the same pixels.
                        for (a, b) in [(top, bottom.min(*start)), (top.max(*end), bottom)] {
                            if a >= b {
                                continue;
                            }
                            let mut fragment = original.clone();
                            let f = fragment.sixel.as_mut().unwrap();
                            f.serial = self.next_placement_serial;
                            self.next_placement_serial += 1;
                            f.clip_top = a.saturating_sub(fragment.line).max(0) as usize;
                            f.clip_bottom = fragment
                                .line
                                .saturating_add(fragment.rows as i32)
                                .saturating_sub(b)
                                .max(0) as usize;
                            register_sixel_anchor(grid, &fragment);
                            events.push(TerminalGraphicsEvent::Place(fragment.clone()));
                            updated.push(fragment);
                        }
                        meta.clip_top = top.max(*start).saturating_sub(p.line).max(0) as usize;
                        meta.clip_bottom = p
                            .line
                            .saturating_add(p.rows as i32)
                            .saturating_sub(bottom.min(*end))
                            .max(0) as usize;
                        p.line = p.line.saturating_add(*delta);
                        meta.clip_top = meta
                            .clip_top
                            .max(start.saturating_sub(p.line).max(0) as usize);
                        meta.clip_bottom = meta.clip_bottom.max(
                            p.line
                                .saturating_add(p.rows as i32)
                                .saturating_sub(*end)
                                .max(0) as usize,
                        );
                    }
                    remove = meta.clip_top + meta.clip_bottom >= p.rows;
                }
                GraphicsGridChange::Reflow { alternate, .. }
                    if p.alternate_screen == *alternate && !meta.fixed =>
                {
                    if let Some(point) = reflow.get(&serial) {
                        p.line = point.line.0.saturating_sub(meta.clip_top as i32);
                        meta.column_offset = -(meta.clip_left as i64);
                        p.col = point.column.0;
                    } else {
                        remove = true;
                    }
                }
                _ => {}
            }
            if remove {
                grid.set_anchor(p.alternate_screen, serial, None);
                events.push(TerminalGraphicsEvent::DeletePlacement { serial });
            } else {
                if p != original {
                    register_sixel_anchor(grid, &p);
                    events.push(TerminalGraphicsEvent::Place(p.clone()));
                }
                updated.push(p);
            }
        }
        self.sixel_placements = updated;
        self.retire_sixel_images(old_ids, events);
    }

    fn retire_sixel_images(
        &mut self,
        old_ids: std::collections::HashSet<TerminalImageId>,
        events: &mut Vec<TerminalGraphicsEvent>,
    ) {
        let remaining = self
            .sixel_placements
            .iter()
            .map(|p| p.id)
            .collect::<std::collections::HashSet<_>>();
        for id in old_ids {
            if !remaining.contains(&id) {
                self.account_image_event(&TerminalGraphicsEvent::Delete { id: Some(id) });
                events.push(TerminalGraphicsEvent::Delete { id: Some(id) });
            }
        }
        for id in self.evict_images_over_budget() {
            events.push(TerminalGraphicsEvent::Delete { id: Some(id) });
        }
    }

    fn edit_sixel_cells(
        &mut self,
        alternate: bool,
        area: SixelCellRect,
        delta: i64,
        grid: &GraphicsGridEvents,
        events: &mut Vec<TerminalGraphicsEvent>,
    ) {
        if !self.sixel_placements.iter().any(|p| {
            p.alternate_screen == alternate
                && SixelCellRect::visible(p).intersection(area).is_some()
        }) {
            return;
        }
        let old_ids = self.sixel_placements.iter().map(|p| p.id).collect();
        let mut updated = Vec::new();
        for p in std::mem::take(&mut self.sixel_placements) {
            let visible = SixelCellRect::visible(&p);
            let overlap = visible.intersection(area);
            if p.alternate_screen != alternate || overlap.is_none() {
                updated.push(p);
                continue;
            }
            let overlap = overlap.unwrap();
            // Cut visibility into disjoint rectangles instead of editing or copying
            // the decoded canvas. Shifted pieces retain their original texture origin.
            let outside = [
                SixelCellRect {
                    bottom: overlap.top,
                    ..visible
                },
                SixelCellRect {
                    top: overlap.bottom,
                    ..visible
                },
                SixelCellRect {
                    top: overlap.top,
                    bottom: overlap.bottom,
                    right: overlap.left,
                    ..visible
                },
                SixelCellRect {
                    top: overlap.top,
                    bottom: overlap.bottom,
                    left: overlap.right,
                    ..visible
                },
            ];
            let moved = (delta != 0)
                .then(|| {
                    let target = SixelCellRect {
                        left: overlap.left + delta,
                        right: overlap.right + delta,
                        ..overlap
                    };
                    target.intersection(area)
                })
                .flatten();
            let mut retained = false;
            for (rect, shift) in outside
                .into_iter()
                .map(|r| (r, 0))
                .chain(moved.map(|r| (r, delta)))
            {
                if rect.top >= rect.bottom || rect.left >= rect.right {
                    continue;
                }
                let mut fragment = p.clone();
                let s = fragment.sixel.as_mut().unwrap();
                if retained {
                    s.serial = self.next_placement_serial;
                    self.next_placement_serial += 1;
                }
                s.column_offset += shift;
                let origin = fragment.col as i64 + s.column_offset;
                s.clip_top = rect.top.saturating_sub(fragment.line) as usize;
                s.clip_bottom = fragment
                    .line
                    .saturating_add(fragment.rows as i32)
                    .saturating_sub(rect.bottom) as usize;
                s.clip_left = (rect.left - origin) as usize;
                s.clip_right = (origin + fragment.cols as i64 - rect.right) as usize;
                register_sixel_anchor(grid, &fragment);
                events.push(TerminalGraphicsEvent::Place(fragment.clone()));
                updated.push(fragment);
                retained = true;
            }
            if !retained {
                let serial = p.sixel.as_ref().unwrap().serial;
                grid.set_anchor(p.alternate_screen, serial, None);
                events.push(TerminalGraphicsEvent::DeletePlacement { serial });
            }
        }
        self.sixel_placements = updated;
        self.retire_sixel_images(old_ids, events);
    }
}

#[derive(Clone, Copy)]
struct SixelCellRect {
    top: i32,
    bottom: i32,
    left: i64,
    right: i64,
}

impl SixelCellRect {
    fn visible(p: &TerminalImagePlacement) -> Self {
        let s = p.sixel.as_ref().unwrap();
        let origin = p.col as i64 + s.column_offset;
        Self {
            top: p.line + s.clip_top as i32,
            bottom: p.line + p.rows as i32 - s.clip_bottom as i32,
            left: origin + s.clip_left as i64,
            right: origin + p.cols as i64 - s.clip_right as i64,
        }
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let rect = Self {
            top: self.top.max(other.top),
            bottom: self.bottom.min(other.bottom),
            left: self.left.max(other.left),
            right: self.right.min(other.right),
        };
        (rect.top < rect.bottom && rect.left < rect.right).then_some(rect)
    }
}

fn register_sixel_anchor(grid: &GraphicsGridEvents, p: &TerminalImagePlacement) {
    let meta = p.sixel.as_ref().unwrap();
    if !meta.fixed {
        grid.set_anchor(
            p.alternate_screen,
            meta.serial,
            Some(alacritty_terminal::index::Point::new(
                alacritty_terminal::index::Line(p.line.saturating_add(meta.clip_top as i32)),
                alacritty_terminal::index::Column(
                    (p.col as i64 + meta.column_offset + meta.clip_left as i64).max(0) as usize,
                ),
            )),
        );
    }
}
