const SYNC_BEGIN: &[u8] = b"\x1b[?2026h";
const SYNC_END: &[u8] = b"\x1b[?2026l";
// Match the ANSI processor's bounded synchronized-update window.
const SYNC_LIMIT: usize = 0x20_0000;
const SYNC_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(150);

struct PlacementOptions {
    move_cursor: bool,
    cols: Option<usize>,
    rows: Option<usize>,
    source_x: u32,
    source_y: u32,
    source_width: u32,
    source_height: u32,
    z_index: i32,
}

fn terminal_image_data_from_decoded(
    id: TerminalImageId,
    protocol: TerminalImageProtocol,
    decoded: DecodedPixels,
    name: Option<String>,
) -> TerminalImageData {
    let rgba = decoded.rgba;
    let frames: Vec<TerminalImageFrame> = decoded
        .frames
        .into_iter()
        .map(|frame| TerminalImageFrame {
            memory: Some(frame.memory),
            rgba: frame.rgba,
            delay_ms_numerator: frame.delay_ms_numerator,
            delay_ms_denominator: frame.delay_ms_denominator,
            gapless: false,
        })
        .collect();
    let animation = TerminalImageAnimationState {
        running: !frames.is_empty(),
        loading: false,
        current_frame: 0,
        loop_limit: None,
        ..Default::default()
    };
    TerminalImageData {
        memory: Some(decoded.memory),
        id,
        protocol,
        version: 0,
        width: decoded.width,
        height: decoded.height,
        rgba,
        frames,
        animation,
        name,
    }
}

impl GraphicsIngress {
    /// Incomplete escapes and chunked uploads belong to the preceding shell.
    pub fn reset_stream(&mut self) {
        self.options.kitty_file_transmission.revoke();
        self.state = ParserState::Ground;
        self.utf8_remaining = 0;
        self.kitty_chunks.clear();
        self.sync_buffer = None;
        self.sync_deadline = None;
    }

    pub fn new(options: GraphicsOptions) -> Self {
        Self {
            options,
            state: ParserState::Ground,
            utf8_remaining: 0,
            next_image_id: u64::from(u32::MAX) + 1,
            kitty_chunks: HashMap::new(),
            kitty_images: HashMap::new(),
            kitty_numbers: HashMap::new(),
            kitty_wire_ids: HashMap::new(),
            kitty_placements: Vec::new(),
            sixel_placements: Vec::new(),
            sixel_state: None,
            next_placement_serial: 1,
            grid_events: None,
            alternate_screen: false,
            placeholder_rows: HashMap::new(),
            placeholder_rendered: HashMap::new(),
            placeholders_changed: false,
            placeholder_index_incomplete: false,
            image_order: VecDeque::new(),
            storage_bytes: 0,
            sync_buffer: None,
            sync_deadline: None,
        }
    }

    pub fn advance(&mut self, bytes: &[u8], cursor: GraphicsCursor) -> GraphicsAdvance {
        let mut result = GraphicsAdvance::default();
        self.advance_ordered(
            bytes,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                    result.terminal_bytes.extend_from_slice(&terminal_bytes);
                }
                TerminalGraphicsSegment::Event(event) => result.events.push(event),
            },
            || cursor,
        );
        result
    }

    pub fn advance_segments<'a, F>(
        &mut self,
        bytes: &'a [u8],
        mut cursor: F,
    ) -> Vec<TerminalGraphicsSegment<'a>>
    where
        F: FnMut() -> GraphicsCursor,
    {
        let mut segments = Vec::new();
        self.advance_ordered(bytes, |segment| segments.push(segment), &mut cursor);
        segments
    }

    pub fn advance_with<F, C>(
        &mut self,
        bytes: &[u8],
        mut emit_terminal: F,
        mut cursor: C,
    ) -> Vec<TerminalGraphicsEvent>
    where
        F: FnMut(&[u8]),
        C: FnMut() -> GraphicsCursor,
    {
        let mut events = Vec::new();
        self.advance_ordered(
            bytes,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(terminal_bytes) => emit_terminal(&terminal_bytes),
                TerminalGraphicsSegment::Event(event) => events.push(event),
            },
            &mut cursor,
        );

        events
    }

    pub fn advance_ordered<'a, F, C>(&mut self, bytes: &'a [u8], mut emit: F, mut cursor: C)
    where
        F: FnMut(TerminalGraphicsSegment<'a>),
        C: FnMut() -> GraphicsCursor,
    {
        self.advance_ordered_inner(bytes, &mut emit, &mut cursor);
    }

    fn advance_ordered_inner<'a>(
        &mut self,
        bytes: &'a [u8],
        mut emit: &mut dyn FnMut(TerminalGraphicsSegment<'a>),
        mut cursor: &mut dyn FnMut() -> GraphicsCursor,
    ) {
        self.emit_grid_changes(&mut emit);
        if !self.options.enabled {
            emit(TerminalGraphicsSegment::Terminal(bytes.into()));
            return;
        }

        if self
            .sync_deadline
            .is_some_and(|deadline| deadline <= std::time::Instant::now())
        {
            self.commit_synchronized(&mut emit, &mut cursor);
        }

        if self.sync_buffer.is_none()
            && !bytes.is_empty()
            && matches!(self.state, ParserState::Ground)
        {
            // SIMD substring searches let ordinary CSI output stay borrowed. Leave
            // possible OSC/DCS/APC prefixes and split escapes to the state machine.
            if memchr::memchr2(0x1b, 0x90, bytes).is_none()
                || (bytes.last() != Some(&0x1b)
                    && memchr::memmem::find(bytes, b"\x1b]").is_none()
                    && !memchr::memchr3_iter(b'P', b'?', 0x90, bytes).any(|index| {
                        match bytes[index] {
                            b'P' => index > 0 && bytes[index - 1] == 0x1b,
                            0x90 => utf8_tail_pending(&bytes[..index], self.utf8_remaining) == 0,
                            _ => index >= 2 && bytes[index - 2..].starts_with(SYNC_BEGIN),
                        }
                    })
                    && memchr::memmem::find(bytes, b"\x1b_").is_none()
                    && !(2..SYNC_BEGIN.len()).any(|len| {
                        bytes.len() >= len && SYNC_BEGIN.starts_with(&bytes[bytes.len() - len..])
                    }))
            {
                self.utf8_remaining = utf8_tail_pending(bytes, self.utf8_remaining);
                emit(TerminalGraphicsSegment::Terminal(bytes.into()));
                if self.grid_events.is_some() {
                    let _ = cursor();
                }
                self.emit_grid_changes(&mut emit);
                return;
            }
        }

        let mut terminal_bytes = Vec::with_capacity(bytes.len());
        let mut result = GraphicsAdvance::default();
        let mut index = 0;
        while index < bytes.len() {
            if let Some(buffer) = &mut self.sync_buffer {
                buffer.push(bytes[index]);
                index += 1;
                if buffer.ends_with(SYNC_BEGIN) {
                    let len = buffer.len() - SYNC_BEGIN.len();
                    buffer.truncate(len);
                    self.sync_deadline = Some(std::time::Instant::now() + SYNC_TIMEOUT);
                } else if buffer.ends_with(SYNC_END) {
                    let len = buffer.len() - SYNC_END.len();
                    buffer.truncate(len);
                    self.commit_synchronized(&mut emit, &mut cursor);
                } else if buffer.len() >= SYNC_LIMIT {
                    self.commit_synchronized(&mut emit, &mut cursor);
                }
                continue;
            }
            if matches!(self.state, ParserState::Ground) {
                let remaining = &bytes[index..];
                let escape = memchr::memchr2_iter(0x1b, 0x90, remaining).find(|index| {
                    remaining[*index] == 0x1b
                        || utf8_tail_pending(&remaining[..*index], self.utf8_remaining) == 0
                });
                let Some(escape_offset) = escape else {
                    // Ordinary terminal output does not need graphics state-machine work.
                    self.utf8_remaining = utf8_tail_pending(remaining, self.utf8_remaining);
                    terminal_bytes.extend_from_slice(remaining);
                    break;
                };
                terminal_bytes.extend_from_slice(&remaining[..escape_offset]);
                self.utf8_remaining = 0;
                index += escape_offset;
            }

            let was_terminal_state = matches!(self.state, ParserState::Ground | ParserState::Esc);
            if matches!(
                (&self.state, bytes[index]),
                (
                    ParserState::DcsEsc(_) | ParserState::ApcEsc(_) | ParserState::OscEsc(_),
                    b'\\'
                ) | (ParserState::Dcs(_), 0x9c)
                    | (ParserState::Osc(_), 0x07)
            ) {
                // Apply the preceding grid changes before the command creates new
                // resources. A queued reset must never erase that command's anchors.
                let _ = cursor();
                self.emit_grid_changes(&mut emit);
            }
            self.advance_byte(bytes[index], &mut cursor, &mut result);
            terminal_bytes.append(&mut result.terminal_bytes);
            if !result.events.is_empty() {
                if !terminal_bytes.is_empty() {
                    emit(TerminalGraphicsSegment::Terminal(
                        std::mem::take(&mut terminal_bytes).into(),
                    ));
                }
                // Preserve protocol ordering for callers that must synchronize
                // graphics state with terminal side effects such as screen swaps.
                for event in result.events.drain(..) {
                    self.account_image_event(&event);
                    emit(TerminalGraphicsSegment::Event(event));
                }
                // Eviction must release both the parser's animation source and the
                // consumer's image/placements. Do this after the entire command,
                // including its placement, so an evicted image cannot be re-placed.
                for id in self.evict_images_over_budget() {
                    emit(TerminalGraphicsSegment::Event(
                        TerminalGraphicsEvent::Delete { id: Some(id) },
                    ));
                }
                let mut cascading = Vec::new();
                self.cascade_missing_parents(&mut cascading);
                for event in cascading {
                    self.account_image_event(&event);
                    emit(TerminalGraphicsSegment::Event(event));
                }
            } else if was_terminal_state
                && !matches!(self.state, ParserState::Ground | ParserState::Esc)
                && !terminal_bytes.is_empty()
            {
                emit(TerminalGraphicsSegment::Terminal(
                    std::mem::take(&mut terminal_bytes).into(),
                ));
            }
            index += 1;
        }

        if !terminal_bytes.is_empty() {
            emit(TerminalGraphicsSegment::Terminal(terminal_bytes.into()));
        }
        if self.grid_events.is_some() {
            // The final text segment can change placeholders without another graphics command.
            let _ = cursor();
        }
        self.emit_grid_changes(&mut emit);
    }

    pub fn sync_deadline(&self) -> Option<std::time::Instant> {
        self.sync_deadline
    }

    fn account_image_event(&mut self, event: &TerminalGraphicsEvent) {
        match event {
            TerminalGraphicsEvent::DeletePlacement { serial } => {
                if let Some(grid) = &self.grid_events {
                    grid.set_anchor(false, *serial, None);
                    grid.set_anchor(true, *serial, None);
                }
            }
            TerminalGraphicsEvent::ImageReady(image)
            | TerminalGraphicsEvent::ImageUpdated(image) => {
                let bytes = if image.frames.is_empty() {
                    image.rgba.len()
                } else {
                    image.frames.iter().map(|frame| frame.rgba.len()).sum()
                };
                if let Some(entry) = self.image_order.iter_mut().find(|(id, _)| *id == image.id) {
                    self.storage_bytes -= entry.1;
                    entry.1 = bytes;
                    if matches!(event, TerminalGraphicsEvent::ImageReady(_)) {
                        self.image_order.retain(|(id, _)| *id != image.id);
                        self.image_order.push_back((image.id, bytes));
                    }
                } else {
                    self.image_order.push_back((image.id, bytes));
                }
                self.storage_bytes += bytes;
            }
            TerminalGraphicsEvent::Delete { id: Some(id) } => {
                self.sixel_placements.retain(|p| {
                    if p.id == *id
                        && let Some(grid) = &self.grid_events
                    {
                        grid.set_anchor(p.alternate_screen, p.sixel.as_ref().unwrap().serial, None);
                    }
                    p.id != *id
                });
                self.track_sixel_screens();
                if let Some(grid) = &self.grid_events {
                    for placement in self.kitty_placements.iter().filter(|p| p.id == *id) {
                        grid.set_anchor(
                            placement.alternate_screen,
                            placement.kitty.as_ref().unwrap().serial,
                            None,
                        );
                    }
                }
                if let Some(index) = self.image_order.iter().position(|(entry, _)| entry == id) {
                    self.storage_bytes -= self.image_order.remove(index).unwrap().1;
                }
                self.kitty_images.remove(id);
                self.kitty_wire_ids.retain(|_, resource| resource != id);
                self.kitty_placements.retain(|p| p.id != *id);
                self.kitty_numbers.retain(|_, ids| {
                    ids.retain(|entry| entry != id);
                    !ids.is_empty()
                });
            }
            TerminalGraphicsEvent::Delete { id: None } => {
                if let Some(grid) = &self.grid_events {
                    for p in &self.sixel_placements {
                        grid.set_anchor(p.alternate_screen, p.sixel.as_ref().unwrap().serial, None);
                    }
                }
                self.sixel_placements.clear();
                self.track_sixel_screens();
                self.image_order.clear();
                self.storage_bytes = 0;
                self.kitty_images.clear();
                self.kitty_placements.clear();
                self.kitty_numbers.clear();
                self.kitty_wire_ids.clear();
            }
            _ => {}
        }
    }

    fn evict_images_over_budget(&mut self) -> Vec<TerminalImageId> {
        let limit = self.options.storage_limit_mb.max(1) as usize * 1024 * 1024;
        let mut evicted = Vec::new();
        while self.storage_bytes > limit
            || self
                .sixel_placements
                .len()
                .saturating_add(self.kitty_placements.len())
                .saturating_mul(std::mem::size_of::<TerminalImagePlacement>() * 2)
                > MAX_GRAPHICS_METADATA_MB as usize * 1024 * 1024
            || self
                .image_order
                .len()
                .saturating_mul(std::mem::size_of::<TerminalImageData>() * 2)
                > MAX_GRAPHICS_METADATA_MB as usize * 1024 * 1024
        {
            let unplaced = self.image_order.iter().position(|(id, _)| {
                self.kitty_images.contains_key(id)
                    && self.kitty_placements.iter().all(|p| p.id != *id)
            });
            let Some((id, bytes)) = unplaced
                .and_then(|index| self.image_order.remove(index))
                .or_else(|| self.image_order.pop_front())
            else {
                break;
            };
            self.storage_bytes -= bytes;
            self.sixel_placements.retain(|p| {
                if p.id == id
                    && let Some(grid) = &self.grid_events
                {
                    grid.set_anchor(p.alternate_screen, p.sixel.as_ref().unwrap().serial, None);
                }
                p.id != id
            });
            self.kitty_images.remove(&id);
            self.kitty_wire_ids.retain(|_, resource| *resource != id);
            if let Some(grid) = &self.grid_events {
                for placement in self.kitty_placements.iter().filter(|p| p.id == id) {
                    grid.set_anchor(
                        placement.alternate_screen,
                        placement.kitty.as_ref().unwrap().serial,
                        None,
                    );
                }
            }
            self.kitty_placements.retain(|p| p.id != id);
            self.kitty_numbers.retain(|_, ids| {
                ids.retain(|entry| *entry != id);
                !ids.is_empty()
            });
            evicted.push(id);
        }
        self.track_sixel_screens();
        evicted
    }

    fn commit_synchronized<'a>(
        &mut self,
        emit: &mut impl FnMut(TerminalGraphicsSegment<'a>),
        cursor: &mut impl FnMut() -> GraphicsCursor,
    ) {
        self.sync_deadline = None;
        let Some(buffer) = self.sync_buffer.take() else {
            return;
        };
        // Replay text and graphics together only after the frame is complete. Cursor
        // movement must reach the ANSI model before decoding each image placement.
        self.advance_ordered_inner(
            &buffer,
            &mut |segment| match segment {
                TerminalGraphicsSegment::Terminal(bytes) => {
                    emit(TerminalGraphicsSegment::Terminal(bytes.into_owned().into()))
                }
                TerminalGraphicsSegment::Event(event) => {
                    emit(TerminalGraphicsSegment::Event(event))
                }
            },
            cursor,
        );
    }

    fn advance_byte<C>(&mut self, byte: u8, cursor: &mut C, result: &mut GraphicsAdvance)
    where
        C: FnMut() -> GraphicsCursor,
    {
        let state = std::mem::replace(&mut self.state, ParserState::Ground);
        // Reject before a full buffer's next push doubles its allocation.
        if parser_state_len(&state) > encoded_storage_limit().saturating_sub(2) {
            self.state = ParserState::DiscardString {
                osc: matches!(state, ParserState::Osc(_) | ParserState::OscEsc(_)),
                escape: byte == 0x1b,
            };
            result.events.push(TerminalGraphicsEvent::Error(
                GraphicsError::StorageLimitExceeded.to_string(),
            ));
            return;
        }
        match state {
            ParserState::Ground => match byte {
                0x1b => self.state = ParserState::Esc,
                0x90 => self.state = ParserState::Dcs(Vec::new()),
                _ => result.terminal_bytes.push(byte),
            },
            ParserState::Esc => match byte {
                b'[' => self.state = ParserState::SyncStart(b"\x1b[".to_vec()),
                b']' => self.state = ParserState::Osc(Vec::new()),
                b'P' => self.state = ParserState::Dcs(Vec::new()),
                b'_' => self.state = ParserState::Apc(Vec::new()),
                _ => {
                    result.terminal_bytes.push(0x1b);
                    result.terminal_bytes.push(byte);
                }
            },
            ParserState::SyncStart(mut data) => {
                data.push(byte);
                if data == SYNC_BEGIN {
                    self.sync_buffer = Some(Zeroizing::new(Vec::new()));
                    self.sync_deadline = Some(std::time::Instant::now() + SYNC_TIMEOUT);
                } else if SYNC_BEGIN.starts_with(&data) {
                    self.state = ParserState::SyncStart(data);
                } else {
                    result.terminal_bytes.extend(data);
                }
            }
            ParserState::Osc(mut data) => match byte {
                0x07 => self.dispatch_osc(data, cursor(), result),
                0x1b => self.state = ParserState::OscEsc(data),
                _ => {
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Osc(data), result);
                }
            },
            ParserState::OscEsc(data) => match byte {
                b'\\' => self.dispatch_osc(data, cursor(), result),
                _ => {
                    result.terminal_bytes.extend_from_slice(b"\x1b]");
                    result.terminal_bytes.extend_from_slice(&data);
                    result.terminal_bytes.push(0x1b);
                    result.terminal_bytes.push(byte);
                }
            },
            ParserState::Dcs(mut data) => match byte {
                0x18 | 0x1a => {}
                0x9c => self.dispatch_dcs(data, cursor(), result),
                0x1b => self.state = ParserState::DcsEsc(data),
                _ => {
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Dcs(data), result);
                }
            },
            ParserState::DcsEsc(data) => match byte {
                b'\\' => self.dispatch_dcs(data, cursor(), result),
                _ => {
                    // ESC starts a new control sequence when it is not the string terminator.
                    self.state = ParserState::Esc;
                    self.advance_byte(byte, cursor, result);
                }
            },
            ParserState::Apc(mut data) => match byte {
                0x1b => self.state = ParserState::ApcEsc(data),
                _ => {
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Apc(data), result);
                }
            },
            ParserState::ApcEsc(mut data) => match byte {
                b'\\' => self.dispatch_apc(data, cursor(), result),
                _ => {
                    data.push(0x1b);
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Apc(data), result);
                }
            },
            ParserState::DiscardString { osc, escape } => {
                if !matches!(byte, 0x18 | 0x1a | 0x9c)
                    && !(osc && byte == 0x07)
                    && !(escape && byte == b'\\')
                {
                    self.state = ParserState::DiscardString {
                        osc,
                        escape: byte == 0x1b,
                    };
                }
            }
        }
    }

    fn parser_state_or_size_error(
        &self,
        state: ParserState,
        result: &mut GraphicsAdvance,
    ) -> ParserState {
        if parser_state_len(&state) <= encoded_storage_limit() {
            state
        } else {
            result.events.push(TerminalGraphicsEvent::Error(
                GraphicsError::StorageLimitExceeded.to_string(),
            ));
            ParserState::Ground
        }
    }

    fn next_id(&mut self) -> TerminalImageId {
        let id = TerminalImageId(self.next_image_id);
        self.next_image_id += 1;
        id
    }

    fn dispatch_osc(
        &mut self,
        data: Vec<u8>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) {
        if !self.options.iterm2_inline || !data.starts_with(b"1337;File=") {
            result.terminal_bytes.extend_from_slice(b"\x1b]");
            result.terminal_bytes.extend_from_slice(&data);
            result.terminal_bytes.push(0x07);
            return;
        }

        match self.decode_iterm2(&data[b"1337;File=".len()..], cursor) {
            Ok((image, placement, advance)) => {
                result.events.push(TerminalGraphicsEvent::ImageReady(image));
                result.events.push(TerminalGraphicsEvent::Place(placement));
                result.terminal_bytes.extend(advance);
            }
            Err(error) => result
                .events
                .push(TerminalGraphicsEvent::Error(error.to_string())),
        }
    }

    fn dispatch_dcs(
        &mut self,
        data: Vec<u8>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) {
        if !self.options.sixel || !looks_like_sixel(&data) {
            result.terminal_bytes.extend_from_slice(b"\x1bP");
            result.terminal_bytes.extend_from_slice(&data);
            result.terminal_bytes.extend_from_slice(b"\x1b\\");
            return;
        }

        let mut sequence = Vec::with_capacity(data.len() + 4);
        sequence.extend_from_slice(b"\x1bP");
        sequence.extend_from_slice(&data);
        sequence.extend_from_slice(b"\x1b\\");

        match self.decode_sixel(&sequence, cursor) {
            Ok((image, placement, advance)) => {
                result.events.push(TerminalGraphicsEvent::ImageReady(image));
                result.events.push(TerminalGraphicsEvent::Place(placement));
                if let Some(advance) = advance {
                    result.events.push(advance);
                }
            }
            Err(error) => result
                .events
                .push(TerminalGraphicsEvent::Error(error.to_string())),
        }
    }

    fn dispatch_apc(
        &mut self,
        data: Vec<u8>,
        cursor: GraphicsCursor,
        result: &mut GraphicsAdvance,
    ) {
        if !self.options.kitty || !data.starts_with(b"G") {
            result.terminal_bytes.extend_from_slice(b"\x1b_");
            result.terminal_bytes.extend_from_slice(&data);
            result.terminal_bytes.extend_from_slice(b"\x1b\\");
            return;
        }
        self.dispatch_kitty(&data[1..], cursor, result);
    }

    fn decode_iterm2(
        &mut self,
        data: &[u8],
        cursor: GraphicsCursor,
    ) -> Result<(TerminalImageData, TerminalImagePlacement, Vec<u8>), GraphicsError> {
        let Some(separator) = data.iter().position(|byte| *byte == b':') else {
            return Err(GraphicsError::UnsupportedImage);
        };
        let params = parse_semicolon_params(&data[..separator]);
        let payload = BASE64
            .decode(&data[separator + 1..])
            .map_err(|_| GraphicsError::InvalidBase64)?;
        enforce_storage_limit(payload.len(), MAX_TRANSFER_MB)?;
        let name = params
            .get("name")
            .and_then(|value| BASE64.decode(value).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok());
        let decoded = decode_image_bytes(
            &payload,
            self.options.pixel_limit,
            self.options.storage_limit_mb,
        )?;
        let width = params
            .get("width")
            .and_then(|value| parse_pixel_size(value, decoded.width));
        let height = params
            .get("height")
            .and_then(|value| parse_pixel_size(value, decoded.height));
        let mut image = terminal_image_data_from_decoded(
            self.next_id(),
            TerminalImageProtocol::Iterm2,
            decoded,
            name,
        );
        image.width = width.unwrap_or(image.width);
        image.height = height.unwrap_or(image.height);
        let do_not_move = params
            .get("doNotMoveCursor")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let (placement, advance) = self.placement_for_image(
            image.id,
            image.protocol,
            image.width,
            image.height,
            cursor,
            PlacementOptions {
                move_cursor: !do_not_move,
                cols: None,
                rows: None,
                source_x: 0,
                source_y: 0,
                source_width: image.width,
                source_height: image.height,
                z_index: 0,
            },
        );
        Ok((image, placement, advance))
    }

    fn decode_sixel(
        &mut self,
        sequence: &[u8],
        cursor: GraphicsCursor,
    ) -> Result<
        (
            TerminalImageData,
            TerminalImagePlacement,
            Option<TerminalGraphicsEvent>,
        ),
        GraphicsError,
    > {
        use std::fmt::Write;
        enforce_storage_limit(sequence.len(), MAX_TRANSFER_MB)?;
        let state = self.sixel_state.get_or_insert_with(Default::default);
        let (color_count, size) = self
            .grid_events
            .as_ref()
            .map(|g| g.sixel_limits())
            .unwrap_or((SIXEL_COLORS, [SIXEL_DIMENSION; 2]));
        let scan = scan_sixel(
            sequence,
            self.options.pixel_limit,
            self.options.storage_limit_mb,
            color_count,
        )?;
        if scan.width > size[0] || scan.height > size[1] {
            return Err(GraphicsError::PixelLimitExceeded);
        }
        let bytes = decoded_rgba_bytes(
            scan.width,
            scan.height,
            self.options.pixel_limit,
            self.options.storage_limit_mb,
        )?;
        let memory = reserve_pixels(bytes)?;
        // Preallocate the validated canvas once. The dependency otherwise reallocates
        // and copies the entire canvas on every horizontal or vertical expansion.
        let _workspace = reserve_decode_workspace(
            bytes
                .saturating_add(sequence.len())
                .saturating_add(16 * 1024),
        )?;
        let private = self
            .grid_events
            .as_ref()
            .and_then(|g| g.sixel_mode(1070))
            .unwrap_or(true);
        let mut prepared = String::from("\x1bP9;1q");
        if !private {
            for (index, color) in state.colors.iter().enumerate() {
                if let Some([space, a, b, c]) = color {
                    let _ = write!(prepared, "#{index};{space};{a};{b};{c}");
                }
            }
        }
        let _ = write!(prepared, "#0\"1;1;{};{}", scan.width, scan.height);
        let mut prepared = prepared.into_bytes();
        prepared.extend_from_slice(&sequence[scan.payload..]);
        let decoded = icy_sixel::SixelImage::decode(&prepared)
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let width = decoded.width as u32;
        let height = decoded.height as u32;
        if !private {
            for (old, new) in state.colors.iter_mut().zip(scan.colors) {
                if new.is_some() {
                    *old = new;
                }
            }
        }
        let display_height = (height as usize)
            .checked_mul(scan.aspect[0])
            .map(|n| n.div_ceil(scan.aspect[1]))
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n <= SIXEL_DIMENSION)
            .ok_or(GraphicsError::PixelLimitExceeded)?;
        let fixed = self
            .grid_events
            .as_ref()
            .and_then(|g| g.sixel_mode(80))
            .unwrap_or(false);
        let right = self
            .grid_events
            .as_ref()
            .and_then(|g| g.sixel_mode(8452))
            .unwrap_or(false);
        let cursor_y = scan
            .cursor_y
            .checked_mul(scan.aspect[0])
            .map(|n| n.div_ceil(scan.aspect[1]))
            .filter(|n| *n <= SIXEL_DIMENSION as usize)
            .ok_or(GraphicsError::PixelLimitExceeded)?;
        let mut image_cursor = cursor;
        if fixed {
            image_cursor.line = 0;
            image_cursor.row = 0;
            image_cursor.col = 0;
        }
        let image = TerminalImageData {
            memory: Some(memory),
            id: self.next_id(),
            protocol: TerminalImageProtocol::Sixel,
            version: 0,
            width,
            height,
            rgba: decoded.pixels.into(),
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
            name: None,
        };
        let (mut placement, _) = self.placement_for_image(
            image.id,
            image.protocol,
            width,
            display_height,
            image_cursor,
            PlacementOptions {
                move_cursor: false,
                cols: None,
                rows: None,
                source_x: 0,
                source_y: 0,
                source_width: width,
                source_height: height,
                z_index: 0,
            },
        );
        placement.cols = (width as usize).div_ceil(usize::from(cursor.cell_width.max(1)));
        placement.rows = (display_height as usize).div_ceil(usize::from(cursor.cell_height.max(1)));
        placement.sixel = Some(SixelPlacement {
            serial: self.next_placement_serial,
            fixed,
            clip_top: 0,
            clip_bottom: 0,
            clip_left: 0,
            clip_right: 0,
            column_offset: 0,
            background: (!scan.transparent).then(|| {
                self.grid_events
                    .as_ref()
                    .map(|g| g.sixel_background())
                    .unwrap_or(alacritty_terminal::vte::ansi::Color::Named(
                        alacritty_terminal::vte::ansi::NamedColor::Background,
                    ))
            }),
        });
        self.next_placement_serial += 1;
        enforce_storage_limit(
            (self.sixel_placements.len() + 1)
                .saturating_mul(std::mem::size_of::<TerminalImagePlacement>() * 2),
            MAX_GRAPHICS_METADATA_MB,
        )?;
        if let Some(grid) = &self.grid_events
            && !fixed
        {
            grid.set_anchor(
                self.alternate_screen,
                placement.sixel.as_ref().unwrap().serial,
                Some(alacritty_terminal::index::Point::new(
                    alacritty_terminal::index::Line(placement.line),
                    alacritty_terminal::index::Column(placement.col),
                )),
            );
        }
        self.sixel_placements.push(placement.clone());
        self.track_sixel_screens();
        let advance = (!fixed).then_some(TerminalGraphicsEvent::SixelCursor {
            rows: cursor_y.max(display_height as usize - 1)
                / usize::from(cursor.cell_height.max(1)),
            column: cursor
                .col
                .saturating_add(if right { placement.cols } else { 0 }),
        });
        Ok((image, placement, advance))
    }

    fn add_kitty_animation_frame(
        &mut self,
        params: &HashMap<String, String>,
        decoded: DecodedPixels,
    ) -> Result<TerminalImageData, GraphicsError> {
        let image_id = kitty_image_id(params)?;
        let image = self
            .kitty_images
            .get_mut(&image_id)
            .ok_or(GraphicsError::UnsupportedImage)?;
        let target_frame = parse_kitty_frame_index(params.get("r"));
        let count = image.frames.len().max(1);
        if target_frame.is_some_and(|frame| frame >= count)
            || parse_kitty_frame_index(params.get("c")).is_some_and(|frame| frame >= count)
        {
            return Err(GraphicsError::ImageNotFound);
        }
        let dest_x = parse_u32(params.get("x")).unwrap_or_default();
        let dest_y = parse_u32(params.get("y")).unwrap_or_default();
        if dest_x.saturating_add(decoded.width) > image.width
            || dest_y.saturating_add(decoded.height) > image.height
        {
            return Err(GraphicsError::InvalidCommand);
        }
        if target_frame.is_none() {
            enforce_storage_limit(
                (count + 1).saturating_mul(std::mem::size_of::<TerminalImageFrame>()),
                MAX_GRAPHICS_METADATA_MB,
            )?;
            enforce_storage_limit(
                (count + 1).saturating_mul(
                    image.width as usize * image.height as usize * 4
                        + std::mem::size_of::<TerminalImageFrame>(),
                ),
                self.options.storage_limit_mb,
            )?;
        }
        let canvas_bytes = image.width as usize * image.height as usize * 4;
        let memory = reserve_pixels(canvas_bytes)?;
        let _workspace = reserve_decode_workspace(canvas_bytes)?;
        ensure_kitty_animation_root(image);
        let mut canvas = if let Some(target_frame) = target_frame {
            image
                .frames
                .get(target_frame)
                .map(|frame| frame.rgba.to_vec())
                .ok_or(GraphicsError::UnsupportedImage)?
        } else if let Some(base_frame) = parse_kitty_frame_index(params.get("c")) {
            image
                .frames
                .get(base_frame)
                .map(|frame| frame.rgba.to_vec())
                .ok_or(GraphicsError::UnsupportedImage)?
        } else if let Some(color) = parse_kitty_rgba_color(params.get("Y")) {
            filled_rgba_canvas(image.width, image.height, color)
        } else {
            vec![0; image.width as usize * image.height as usize * 4]
        };
        let replace = params.get("X").is_some_and(|value| value == "1");
        blend_protocol_rgba_rect(
            &mut canvas,
            image.width,
            &decoded.rgba,
            decoded.width,
            decoded.height,
            dest_x,
            dest_y,
            replace,
        );

        let gap = parse_kitty_frame_gap(params.get("z"));
        let frame_index = if let Some(target_frame) = target_frame {
            let frame = image
                .frames
                .get_mut(target_frame)
                .ok_or(GraphicsError::UnsupportedImage)?;
            frame.rgba = canvas.into();
            frame.memory = Some(memory);
            if let Some((delay_ms, gapless)) = gap {
                frame.delay_ms_numerator = delay_ms;
                frame.delay_ms_denominator = 1;
                frame.gapless = gapless;
            }
            target_frame
        } else {
            let (delay_ms, gapless) = gap.unwrap_or((40, false));
            image.frames.push(TerminalImageFrame {
                memory: Some(memory),
                rgba: canvas.into(),
                delay_ms_numerator: delay_ms,
                delay_ms_denominator: 1,
                gapless,
            });
            image.frames.len() - 1
        };
        if frame_index == 0 {
            image.rgba = image.frames[0].rgba.clone();
            image.memory = image.frames[0].memory.clone();
        }
        image.animation.current_frame = image
            .animation
            .current_frame
            .min(image.frames.len().saturating_sub(1));
        Ok(image.clone())
    }

    fn control_kitty_animation(
        &mut self,
        params: &HashMap<String, String>,
    ) -> Result<TerminalImageData, GraphicsError> {
        let image_id = kitty_image_id(params)?;
        let image = self
            .kitty_images
            .get_mut(&image_id)
            .ok_or(GraphicsError::UnsupportedImage)?;
        let count = image.frames.len().max(1);
        let state = parse_u32(params.get("s")).unwrap_or(0);
        let seek = parse_kitty_frame_index(params.get("c"));
        let affected = parse_kitty_frame_index(params.get("r"))
            .unwrap_or(seek.unwrap_or(image.animation.current_frame));
        if state > 3
            || seek.is_some_and(|frame| frame >= count)
            || (parse_kitty_frame_gap(params.get("z")).is_some() && affected >= count)
        {
            return Err(GraphicsError::InvalidCommand);
        }
        ensure_kitty_animation_root(image);
        image.animation.control_serial = image.animation.control_serial.wrapping_add(1);
        image.animation.seek = None;
        image.animation.reset_loops = params.get("s").is_some_and(|s| s == "1");

        if let Some(current_frame) = parse_kitty_frame_index(params.get("c")) {
            image.animation.current_frame = current_frame;
            image.animation.seek = Some(current_frame);
        }
        if let Some((delay_ms, gapless)) = parse_kitty_frame_gap(params.get("z")) {
            let frame_index =
                parse_kitty_frame_index(params.get("r")).unwrap_or(image.animation.current_frame);
            if let Some(frame) = image.frames.get_mut(frame_index) {
                frame.delay_ms_numerator = delay_ms;
                frame.delay_ms_denominator = 1;
                frame.gapless = gapless;
            }
        }
        if let Some(state) = params.get("s").and_then(|value| value.parse::<u32>().ok()) {
            match state {
                1 => {
                    image.animation.running = false;
                    image.animation.loading = false;
                }
                2 => {
                    image.animation.running = true;
                    image.animation.loading = true;
                }
                3 => {
                    image.animation.running = true;
                    image.animation.loading = false;
                }
                0 => {}
                _ => unreachable!(),
            }
        }
        if parse_u32(params.get("v")).is_some_and(|v| v > 0) {
            image.animation.loop_limit = kitty_loop_limit(params.get("v"));
        }
        Ok(image.clone())
    }

    fn compose_kitty_animation_frame(
        &mut self,
        params: &HashMap<String, String>,
    ) -> Result<TerminalImageData, GraphicsError> {
        let image_id = kitty_image_id(params)?;
        let image = self
            .kitty_images
            .get_mut(&image_id)
            .ok_or(GraphicsError::UnsupportedImage)?;
        let source_frame =
            parse_kitty_frame_index(params.get("r")).ok_or(GraphicsError::UnsupportedImage)?;
        let dest_frame =
            parse_kitty_frame_index(params.get("c")).ok_or(GraphicsError::UnsupportedImage)?;
        if source_frame >= image.frames.len().max(1) || dest_frame >= image.frames.len().max(1) {
            return Err(GraphicsError::ImageNotFound);
        }

        let width = parse_u32(params.get("w"))
            .filter(|width| *width > 0)
            .unwrap_or(image.width);
        let height = parse_u32(params.get("h"))
            .filter(|height| *height > 0)
            .unwrap_or(image.height);
        let source_x = parse_u32(params.get("X")).unwrap_or_default();
        let source_y = parse_u32(params.get("Y")).unwrap_or_default();
        let dest_x = parse_u32(params.get("x")).unwrap_or_default();
        let dest_y = parse_u32(params.get("y")).unwrap_or_default();
        if source_x.saturating_add(width) > image.width
            || source_y.saturating_add(height) > image.height
            || dest_x.saturating_add(width) > image.width
            || dest_y.saturating_add(height) > image.height
            || (source_frame == dest_frame
                && rects_overlap(source_x, source_y, dest_x, dest_y, width, height))
        {
            return Err(GraphicsError::UnsupportedImage);
        }

        let canvas_bytes = image.width as usize * image.height as usize * 4;
        let memory = reserve_pixels(canvas_bytes)?;
        let _workspace = reserve_decode_workspace(
            canvas_bytes.saturating_add(width as usize * height as usize * 4),
        )?;
        ensure_kitty_animation_root(image);
        let source_rect = copy_protocol_rgba_rect(
            &image.frames[source_frame].rgba,
            image.width,
            source_x,
            source_y,
            width,
            height,
        );
        let replace = params.get("C").is_some_and(|value| value == "1");
        let dest = image
            .frames
            .get_mut(dest_frame)
            .ok_or(GraphicsError::UnsupportedImage)?;
        let mut dest_rgba = dest.rgba.to_vec();
        blend_protocol_rgba_rect(
            &mut dest_rgba,
            image.width,
            &source_rect,
            width,
            height,
            dest_x,
            dest_y,
            replace,
        );
        dest.rgba = dest_rgba.into();
        dest.memory = Some(memory);
        if dest_frame == 0 {
            image.rgba = dest.rgba.clone();
            image.memory = dest.memory.clone();
        }
        Ok(image.clone())
    }

    fn place_kitty_image(
        &self,
        params: &HashMap<String, String>,
        cursor: GraphicsCursor,
    ) -> Result<(TerminalImagePlacement, Vec<u8>), GraphicsError> {
        let image_id = params
            .get("i")
            .and_then(|value| value.parse::<u64>().ok())
            .map(TerminalImageId)
            .ok_or(GraphicsError::UnsupportedImage)?;
        let image = self
            .kitty_images
            .get(&image_id)
            .ok_or(GraphicsError::UnsupportedImage)?;
        if parse_u32(params.get("X")).unwrap_or(0) >= u32::from(cursor.cell_width.max(1))
            || parse_u32(params.get("Y")).unwrap_or(0) >= u32::from(cursor.cell_height.max(1))
            || parse_u32(params.get("x")).unwrap_or(0) >= image.width
            || parse_u32(params.get("y")).unwrap_or(0) >= image.height
        {
            return Err(GraphicsError::InvalidCommand);
        }
        let mut placement_options =
            kitty_placement_options(params, image.width, image.height, cursor);
        placement_options.move_cursor = false;
        Ok(self.placement_for_image(
            image_id,
            TerminalImageProtocol::Kitty,
            image.width,
            image.height,
            cursor,
            placement_options,
        ))
    }

    fn placement_for_image(
        &self,
        id: TerminalImageId,
        protocol: TerminalImageProtocol,
        pixel_width: u32,
        pixel_height: u32,
        cursor: GraphicsCursor,
        options: PlacementOptions,
    ) -> (TerminalImagePlacement, Vec<u8>) {
        let (cols, rows) = if protocol == TerminalImageProtocol::Kitty {
            (
                options
                    .cols
                    .unwrap_or(
                        options
                            .source_width
                            .div_ceil(u32::from(cursor.cell_width).max(1))
                            as usize,
                    )
                    .max(1),
                options
                    .rows
                    .unwrap_or(
                        options
                            .source_height
                            .div_ceil(u32::from(cursor.cell_height).max(1))
                            as usize,
                    )
                    .max(1),
            )
        } else {
            let (default_cols, default_rows) =
                cursor.image_cells(options.source_width, options.source_height);
            (
                options
                    .cols
                    .unwrap_or(default_cols)
                    .clamp(1, cursor.cols.max(1)),
                options
                    .rows
                    .unwrap_or(default_rows)
                    .clamp(1, cursor.rows.max(1)),
            )
        };
        let placement = TerminalImagePlacement {
            id,
            protocol,
            line: cursor.line,
            row: cursor.row,
            col: cursor.col,
            cols,
            rows,
            pixel_width,
            pixel_height,
            source_x: options.source_x,
            source_y: options.source_y,
            source_width: options.source_width,
            source_height: options.source_height,
            z_index: options.z_index,
            placeholder: self.options.show_placeholder,
            kitty: None,
            sixel: None,
            alternate_screen: self.alternate_screen,
        };
        let advance = if options.move_cursor {
            advance_bytes(cursor.col, cols, rows, cursor.cols)
        } else {
            Vec::new()
        };
        (placement, advance)
    }
}

fn kitty_placement_options(
    params: &HashMap<String, String>,
    image_width: u32,
    image_height: u32,
    cursor: GraphicsCursor,
) -> PlacementOptions {
    let source = kitty_source_rect(params, image_width, image_height);
    let cols = parse_positive_usize(params.get("c"));
    let rows = parse_positive_usize(params.get("r"));
    let (cols, rows) = complete_kitty_display_cells(cols, rows, source.2, source.3, cursor);
    PlacementOptions {
        move_cursor: !params.get("C").is_some_and(|value| value == "1"),
        cols,
        rows,
        source_x: source.0,
        source_y: source.1,
        source_width: source.2,
        source_height: source.3,
        z_index: params
            .get("z")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or_default(),
    }
}

fn kitty_source_rect(
    params: &HashMap<String, String>,
    image_width: u32,
    image_height: u32,
) -> (u32, u32, u32, u32) {
    let source_x = parse_u32(params.get("x"))
        .unwrap_or_default()
        .min(image_width.saturating_sub(1));
    let source_y = parse_u32(params.get("y"))
        .unwrap_or_default()
        .min(image_height.saturating_sub(1));
    let max_width = image_width.saturating_sub(source_x);
    let max_height = image_height.saturating_sub(source_y);
    let source_width = parse_u32(params.get("w"))
        .filter(|width| *width > 0)
        .unwrap_or(max_width)
        .min(max_width)
        .max(1);
    let source_height = parse_u32(params.get("h"))
        .filter(|height| *height > 0)
        .unwrap_or(max_height)
        .min(max_height)
        .max(1);
    (source_x, source_y, source_width, source_height)
}

fn complete_kitty_display_cells(
    cols: Option<usize>,
    rows: Option<usize>,
    source_width: u32,
    source_height: u32,
    cursor: GraphicsCursor,
) -> (Option<usize>, Option<usize>) {
    match (cols, rows) {
        (Some(cols), None) => {
            let pixel_width = cols as u64 * u64::from(cursor.cell_width.max(1));
            let pixel_height = pixel_width.saturating_mul(u64::from(source_height))
                / u64::from(source_width.max(1));
            let rows = pixel_height
                .div_ceil(u64::from(cursor.cell_height.max(1)))
                .max(1) as usize;
            (Some(cols), Some(rows))
        }
        (None, Some(rows)) => {
            let pixel_height = rows as u64 * u64::from(cursor.cell_height.max(1));
            let pixel_width = pixel_height.saturating_mul(u64::from(source_width))
                / u64::from(source_height.max(1));
            let cols = pixel_width
                .div_ceil(u64::from(cursor.cell_width.max(1)))
                .max(1) as usize;
            (Some(cols), Some(rows))
        }
        both => both,
    }
}

fn parse_positive_usize(value: Option<&String>) -> Option<usize> {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn parse_u32(value: Option<&String>) -> Option<u32> {
    value.and_then(|value| value.parse::<u32>().ok())
}

fn kitty_image_id(params: &HashMap<String, String>) -> Result<TerminalImageId, GraphicsError> {
    params
        .get("i")
        .and_then(|value| value.parse::<u64>().ok())
        .map(TerminalImageId)
        .ok_or(GraphicsError::UnsupportedImage)
}

fn parse_kitty_frame_index(value: Option<&String>) -> Option<usize> {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value - 1)
}

fn parse_kitty_frame_gap(value: Option<&String>) -> Option<(u32, bool)> {
    let gap = value.and_then(|value| value.parse::<i32>().ok())?;
    match gap.cmp(&0) {
        std::cmp::Ordering::Less => Some((0, true)),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some((gap as u32, false)),
    }
}

fn kitty_loop_limit(value: Option<&String>) -> Option<u32> {
    match value.and_then(|value| value.parse::<u32>().ok()) {
        Some(0) | Some(1) | None => None,
        Some(limit) => Some(limit - 1),
    }
}

fn ensure_kitty_animation_root(image: &mut TerminalImageData) {
    if image.frames.is_empty() {
        image.frames.push(TerminalImageFrame {
            memory: image.memory.clone(),
            rgba: image.rgba.clone(),
            delay_ms_numerator: 0,
            delay_ms_denominator: 1,
            gapless: true,
        });
    }
}

fn parse_kitty_rgba_color(value: Option<&String>) -> Option<[u8; 4]> {
    let color = value.and_then(|value| value.parse::<u32>().ok())?;
    Some([
        ((color >> 24) & 0xff) as u8,
        ((color >> 16) & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        (color & 0xff) as u8,
    ])
}

fn filled_rgba_canvas(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..width.saturating_mul(height) {
        pixels.extend_from_slice(&color);
    }
    pixels
}

fn copy_protocol_rgba_rect(
    pixels: &[u8],
    source_width: u32,
    source_x: u32,
    source_y: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let row_bytes = width as usize * 4;
    let stride = source_width as usize * 4;
    let mut copied = Vec::with_capacity(row_bytes * height as usize);
    for row in source_y..source_y + height {
        let start = row as usize * stride + source_x as usize * 4;
        copied.extend_from_slice(&pixels[start..start + row_bytes]);
    }
    copied
}

fn blend_protocol_rgba_rect(
    dest: &mut [u8],
    dest_width: u32,
    source: &[u8],
    source_width: u32,
    source_height: u32,
    dest_x: u32,
    dest_y: u32,
    replace: bool,
) {
    for source_row in 0..source_height {
        for source_col in 0..source_width {
            let source_index =
                (source_row as usize * source_width as usize + source_col as usize) * 4;
            let dest_index = ((dest_y + source_row) as usize * dest_width as usize
                + (dest_x + source_col) as usize)
                * 4;
            if replace {
                dest[dest_index..dest_index + 4]
                    .copy_from_slice(&source[source_index..source_index + 4]);
            } else {
                alpha_blend_protocol_rgba(
                    &mut dest[dest_index..dest_index + 4],
                    &source[source_index..source_index + 4],
                );
            }
        }
    }
}

fn alpha_blend_protocol_rgba(dest: &mut [u8], source: &[u8]) {
    let source_alpha = u32::from(source[3]);
    if source_alpha == 255 {
        dest.copy_from_slice(source);
        return;
    }
    if source_alpha == 0 {
        return;
    }
    let dest_alpha = u32::from(dest[3]);
    let inverse_source_alpha = 255 - source_alpha;
    let out_alpha = source_alpha + dest_alpha * inverse_source_alpha / 255;
    if out_alpha == 0 {
        dest.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for channel in 0..3 {
        let source_premultiplied = u32::from(source[channel]) * source_alpha;
        let dest_premultiplied = u32::from(dest[channel]) * dest_alpha * inverse_source_alpha / 255;
        dest[channel] = ((source_premultiplied + dest_premultiplied) / out_alpha) as u8;
    }
    dest[3] = out_alpha as u8;
}

fn rects_overlap(ax: u32, ay: u32, bx: u32, by: u32, width: u32, height: u32) -> bool {
    let a_right = ax.saturating_add(width);
    let a_bottom = ay.saturating_add(height);
    let b_right = bx.saturating_add(width);
    let b_bottom = by.saturating_add(height);
    ax < b_right && bx < a_right && ay < b_bottom && by < a_bottom
}
