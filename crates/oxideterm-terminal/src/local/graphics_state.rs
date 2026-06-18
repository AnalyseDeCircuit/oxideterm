pub(crate) struct TerminalGraphicsState {
    images: HashMap<TerminalImageId, Arc<TerminalImageData>>,
    image_versions: HashMap<TerminalImageId, u64>,
    placements: Vec<TerminalImagePlacement>,
    image_order: VecDeque<TerminalImageId>,
    storage_bytes: usize,
    storage_limit_bytes: usize,
}

impl Default for TerminalGraphicsState {
    fn default() -> Self {
        Self {
            images: HashMap::new(),
            image_versions: HashMap::new(),
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
                let next_version = self
                    .image_versions
                    .get(&image.id)
                    .copied()
                    .unwrap_or_default()
                    + 1;
                image.version = next_version;
                self.image_versions.insert(image.id, next_version);
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
                let next_version = self
                    .image_versions
                    .get(&image.id)
                    .copied()
                    .unwrap_or_default()
                    + 1;
                image.version = next_version;
                self.image_versions.insert(image.id, next_version);
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
                if row >= rows || placement.col >= usize::MAX {
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
