pub(crate) struct TerminalGraphicsState {
    images: HashMap<TerminalImageId, Arc<TerminalImageData>>,
    next_image_version: u64,
    placements: Vec<TerminalImagePlacement>,
    image_order: VecDeque<TerminalImageId>,
    storage_bytes: usize,
    storage_limit_bytes: usize,
}

impl Default for TerminalGraphicsState {
    fn default() -> Self {
        Self {
            images: HashMap::new(),
            next_image_version: 1,
            placements: Vec::new(),
            image_order: VecDeque::new(),
            storage_bytes: 0,
            storage_limit_bytes: DEFAULT_STORAGE_LIMIT_MB as usize * 1024 * 1024,
        }
    }
}

impl TerminalGraphicsState {
    pub(crate) fn handle_event(&mut self, event: TerminalGraphicsEvent) -> Option<Vec<u8>> {
        match event {
            TerminalGraphicsEvent::ImageReady(mut image) => {
                if let Some(previous) = self.images.remove(&image.id) {
                    self.storage_bytes = self
                        .storage_bytes
                        .saturating_sub(image_storage_bytes(&previous));
                    self.image_order.retain(|id| *id != image.id);
                    self.placements.retain(|placement| placement.id != image.id);
                }
                let next_version = self.allocate_image_version();
                image.version = next_version;
                self.storage_bytes += image_storage_bytes(&image);
                self.image_order.push_back(image.id);
                self.images.insert(image.id, Arc::new(image));
                self.evict_images_over_budget();
                None
            }
            TerminalGraphicsEvent::ImageUpdated(mut image) => {
                if let Some(previous) = self.images.remove(&image.id) {
                    self.storage_bytes = self
                        .storage_bytes
                        .saturating_sub(image_storage_bytes(&previous));
                }
                let next_version = self.allocate_image_version();
                image.version = next_version;
                self.storage_bytes += image_storage_bytes(&image);
                if !self.image_order.iter().any(|id| *id == image.id) {
                    self.image_order.push_back(image.id);
                }
                self.images.insert(image.id, Arc::new(image));
                self.evict_images_over_budget();
                None
            }
            TerminalGraphicsEvent::Place(placement) => {
                self.placements
                    .retain(|existing| existing.id != placement.id);
                self.placements.push(placement);
                None
            }
            TerminalGraphicsEvent::Delete { id } => {
                if let Some(id) = id {
                    self.remove_image(id);
                    self.placements.retain(|placement| placement.id != id);
                } else {
                    self.clear();
                }
                None
            }
            TerminalGraphicsEvent::Respond(bytes) => Some(bytes),
            TerminalGraphicsEvent::Error(error) => {
                tracing::debug!(%error, "terminal graphics protocol error");
                None
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.images.clear();
        self.placements.clear();
        self.image_order.clear();
        self.storage_bytes = 0;
    }

    fn allocate_image_version(&mut self) -> u64 {
        // A global content revision avoids retaining a tombstone for every image id ever seen.
        let version = self.next_image_version;
        self.next_image_version = self.next_image_version.wrapping_add(1).max(1);
        version
    }

    pub(crate) fn clear_for_alt_screen_transition<T: EventListener>(
        &mut self,
        term: &Term<T>,
        alt_screen_active: &mut bool,
    ) -> bool {
        let next_active = term.mode().contains(TermMode::ALT_SCREEN);
        if next_active == *alt_screen_active {
            return false;
        }

        *alt_screen_active = next_active;
        // Graphics placements are not screen-buffer scoped yet, so clear them on
        // normal/alternate buffer switches to avoid drawing TUI images on the
        // restored shell screen.
        self.clear();
        true
    }

    fn visible_images(&self, display_offset: usize, rows: usize) -> Vec<TerminalImageSnapshot> {
        self.placements
            .iter()
            .filter_map(|placement| {
                let row = viewport_row_for_grid_line(placement.line, display_offset)?;
                if row >= rows {
                    return None;
                }
                Some(TerminalImageSnapshot {
                    id: placement.id,
                    protocol: placement.protocol,
                    row,
                    col: placement.col,
                    cols: placement.cols,
                    rows: placement.rows,
                    pixel_width: placement.pixel_width,
                    pixel_height: placement.pixel_height,
                    source_x: placement.source_x,
                    source_y: placement.source_y,
                    source_width: placement.source_width,
                    source_height: placement.source_height,
                    z_index: placement.z_index,
                    placeholder: placement.placeholder,
                    version: self
                        .images
                        .get(&placement.id)
                        .map(|image| image.version)
                        .unwrap_or_default(),
                    data: self.images.get(&placement.id).cloned(),
                })
            })
            .collect()
    }

    fn visible_images_for_grid_lines(&self, grid_lines: &[i32]) -> Vec<TerminalImageSnapshot> {
        let mut images = Vec::new();
        for placement in &self.placements {
            let placement_end = placement.line.saturating_add(placement.rows as i32);
            let mut segment_start = None;
            let mut previous_viewport_row = 0;
            let mut previous_source_row = 0;

            for (viewport_row, &grid_line) in grid_lines.iter().enumerate() {
                if grid_line < placement.line || grid_line >= placement_end {
                    if let Some((start_viewport_row, start_source_row)) = segment_start.take() {
                        images.push(self.projected_image_segment(
                            placement,
                            start_viewport_row,
                            start_source_row,
                            previous_viewport_row - start_viewport_row + 1,
                        ));
                    }
                    continue;
                }

                let source_row = (grid_line - placement.line) as usize;
                let continues_segment = segment_start.is_some()
                    && viewport_row == previous_viewport_row + 1
                    && source_row == previous_source_row + 1;
                if !continues_segment {
                    if let Some((start_viewport_row, start_source_row)) = segment_start.take() {
                        images.push(self.projected_image_segment(
                            placement,
                            start_viewport_row,
                            start_source_row,
                            previous_viewport_row - start_viewport_row + 1,
                        ));
                    }
                    segment_start = Some((viewport_row, source_row));
                }
                previous_viewport_row = viewport_row;
                previous_source_row = source_row;
            }

            if let Some((start_viewport_row, start_source_row)) = segment_start {
                images.push(self.projected_image_segment(
                    placement,
                    start_viewport_row,
                    start_source_row,
                    previous_viewport_row - start_viewport_row + 1,
                ));
            }
        }
        images
    }

    fn projected_image_segment(
        &self,
        placement: &TerminalImagePlacement,
        viewport_row: usize,
        source_row: usize,
        rows: usize,
    ) -> TerminalImageSnapshot {
        // Folding changes row adjacency without changing image storage, so each visible
        // contiguous segment receives the matching vertical source crop.
        let source_y_offset = proportional_image_extent(
            placement.source_height,
            source_row,
            placement.rows,
        );
        let source_height = proportional_image_extent(
            placement.source_height,
            rows,
            placement.rows,
        );
        TerminalImageSnapshot {
            id: placement.id,
            protocol: placement.protocol,
            row: viewport_row,
            col: placement.col,
            cols: placement.cols,
            rows,
            pixel_width: placement.pixel_width,
            pixel_height: placement.pixel_height,
            source_x: placement.source_x,
            source_y: placement.source_y.saturating_add(source_y_offset),
            source_width: placement.source_width,
            source_height,
            z_index: placement.z_index,
            placeholder: placement.placeholder,
            version: self
                .images
                .get(&placement.id)
                .map(|image| image.version)
                .unwrap_or_default(),
            data: self.images.get(&placement.id).cloned(),
        }
    }

    fn evict_images_over_budget(&mut self) {
        while self.storage_bytes > self.storage_limit_bytes {
            let Some(id) = self.image_order.pop_front() else {
                self.storage_bytes = 0;
                break;
            };
            self.remove_image(id);
            self.placements.retain(|placement| placement.id != id);
        }
    }

    fn remove_image(&mut self, id: TerminalImageId) {
        if let Some(image) = self.images.remove(&id) {
            self.storage_bytes = self
                .storage_bytes
                .saturating_sub(image_storage_bytes(&image));
        }
        self.image_order.retain(|existing| *existing != id);
    }
}

fn image_storage_bytes(image: &TerminalImageData) -> usize {
    if image.frames.is_empty() {
        image.rgba.len()
    } else {
        // Animated images keep frame zero in `frames`, so count the frame set
        // once instead of adding the still-preview buffer again.
        image.frames.iter().map(|frame| frame.rgba.len()).sum()
    }
}

fn proportional_image_extent(source_extent: u32, cells: usize, total_cells: usize) -> u32 {
    if total_cells == 0 {
        return 0;
    }
    ((u64::from(source_extent) * cells as u64) / total_cells as u64)
        .min(u64::from(u32::MAX)) as u32
}

pub(crate) fn graphics_cursor_from_term<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
) -> GraphicsCursor {
    let content = term.renderable_content();
    let line = content.cursor.point.line.0;
    GraphicsCursor {
        line,
        row: viewport_row_for_grid_line(line, content.display_offset).unwrap_or_default(),
        col: content.cursor.point.column.0,
        cols: size.cols,
        rows: size.rows,
        cell_width: size.cell_width,
        cell_height: size.cell_height,
    }
}
