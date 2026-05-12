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

enum KittyDecodeResult {
    Image {
        image: TerminalImageData,
        placement: Option<TerminalImagePlacement>,
        advance: Vec<u8>,
    },
    Placement {
        placement: TerminalImagePlacement,
        advance: Vec<u8>,
    },
}

impl GraphicsIngress {
    pub fn new(options: GraphicsOptions) -> Self {
        Self {
            options,
            state: ParserState::Ground,
            next_image_id: 1,
            kitty_chunks: HashMap::new(),
            kitty_images: HashMap::new(),
        }
    }

    pub fn advance(&mut self, bytes: &[u8], cursor: GraphicsCursor) -> GraphicsAdvance {
        if !self.options.enabled {
            return GraphicsAdvance {
                terminal_bytes: bytes.to_vec(),
                events: Vec::new(),
            };
        }

        let mut result = GraphicsAdvance::default();
        for &byte in bytes {
            self.advance_byte(byte, cursor, &mut result);
        }
        result
    }

    pub fn advance_segments<F>(
        &mut self,
        bytes: &[u8],
        mut cursor: F,
    ) -> Vec<TerminalGraphicsSegment>
    where
        F: FnMut() -> GraphicsCursor,
    {
        if !self.options.enabled {
            return vec![TerminalGraphicsSegment::Terminal(bytes.to_vec())];
        }

        let mut segments = Vec::new();
        let mut terminal_bytes = Vec::new();
        for &byte in bytes {
            let before_state = self.state.clone();
            let mut result = GraphicsAdvance::default();
            self.advance_byte(byte, cursor(), &mut result);
            terminal_bytes.extend(result.terminal_bytes);
            if !result.events.is_empty() {
                if !terminal_bytes.is_empty() {
                    segments.push(TerminalGraphicsSegment::Terminal(std::mem::take(
                        &mut terminal_bytes,
                    )));
                }
                segments.extend(
                    result
                        .events
                        .into_iter()
                        .map(TerminalGraphicsSegment::Event),
                );
            } else if entered_control_sequence(&before_state, &self.state)
                && !terminal_bytes.is_empty()
            {
                segments.push(TerminalGraphicsSegment::Terminal(std::mem::take(
                    &mut terminal_bytes,
                )));
            }
        }

        if !terminal_bytes.is_empty() {
            segments.push(TerminalGraphicsSegment::Terminal(terminal_bytes));
        }
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
        if !self.options.enabled {
            emit_terminal(bytes);
            return Vec::new();
        }

        let mut events = Vec::new();
        let mut terminal_bytes = Vec::new();
        for &byte in bytes {
            let before_state = self.state.clone();
            let mut result = GraphicsAdvance::default();
            self.advance_byte(byte, cursor(), &mut result);
            terminal_bytes.extend(result.terminal_bytes);
            if !result.events.is_empty() {
                if !terminal_bytes.is_empty() {
                    emit_terminal(&terminal_bytes);
                    terminal_bytes.clear();
                }
                events.extend(result.events);
            } else if entered_control_sequence(&before_state, &self.state)
                && !terminal_bytes.is_empty()
            {
                emit_terminal(&terminal_bytes);
                terminal_bytes.clear();
            }
        }

        if !terminal_bytes.is_empty() {
            emit_terminal(&terminal_bytes);
        }
        events
    }

    fn advance_byte(&mut self, byte: u8, cursor: GraphicsCursor, result: &mut GraphicsAdvance) {
        let state = std::mem::replace(&mut self.state, ParserState::Ground);
        match state {
            ParserState::Ground => match byte {
                0x1b => self.state = ParserState::Esc,
                _ => result.terminal_bytes.push(byte),
            },
            ParserState::Esc => match byte {
                b']' => self.state = ParserState::Osc(Vec::new()),
                b'P' => self.state = ParserState::Dcs(Vec::new()),
                b'_' => self.state = ParserState::Apc(Vec::new()),
                _ => {
                    result.terminal_bytes.push(0x1b);
                    result.terminal_bytes.push(byte);
                }
            },
            ParserState::Osc(mut data) => match byte {
                0x07 => self.dispatch_osc(data, cursor, result),
                0x1b => self.state = ParserState::OscEsc(data),
                _ => {
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Osc(data), result);
                }
            },
            ParserState::OscEsc(data) => match byte {
                b'\\' => self.dispatch_osc(data, cursor, result),
                _ => {
                    result.terminal_bytes.extend_from_slice(b"\x1b]");
                    result.terminal_bytes.extend_from_slice(&data);
                    result.terminal_bytes.push(0x1b);
                    result.terminal_bytes.push(byte);
                }
            },
            ParserState::Dcs(mut data) => match byte {
                0x1b => self.state = ParserState::DcsEsc(data),
                _ => {
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Dcs(data), result);
                }
            },
            ParserState::DcsEsc(mut data) => match byte {
                b'\\' => self.dispatch_dcs(data, cursor, result),
                _ => {
                    data.push(0x1b);
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Dcs(data), result);
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
                b'\\' => self.dispatch_apc(data, cursor, result),
                _ => {
                    data.push(0x1b);
                    data.push(byte);
                    self.state = self.parser_state_or_size_error(ParserState::Apc(data), result);
                }
            },
        }
    }

    fn parser_state_or_size_error(
        &self,
        state: ParserState,
        result: &mut GraphicsAdvance,
    ) -> ParserState {
        if parser_state_len(&state) <= encoded_storage_limit(self.options.storage_limit_mb) {
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
                result.terminal_bytes.extend(advance);
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

        let (params, payload) = parse_kitty_command(&data[1..]);
        match params.get("a").map(String::as_str).unwrap_or("t") {
            "d" => {
                let id = params
                    .get("i")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(TerminalImageId);
                if let Some(id) = id {
                    self.kitty_images.remove(&id);
                } else {
                    self.kitty_images.clear();
                }
                result.events.push(TerminalGraphicsEvent::Delete { id });
                return;
            }
            "q" => {
                let id = params
                    .get("i")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or_default();
                result
                    .events
                    .push(TerminalGraphicsEvent::Respond(kitty_query_response(id)));
                return;
            }
            "p" => {
                match self.place_kitty_image(&params, cursor) {
                    Ok((placement, advance)) => {
                        result.events.push(TerminalGraphicsEvent::Place(placement));
                        result.terminal_bytes.extend(advance);
                    }
                    Err(error) => result
                        .events
                        .push(TerminalGraphicsEvent::Error(error.to_string())),
                }
                return;
            }
            _ if payload.is_none() => return,
            _ => {}
        }

        match self.decode_kitty(&data[1..], cursor) {
            Ok(Some(KittyDecodeResult::Image {
                image,
                placement,
                advance,
            })) => {
                result.events.push(TerminalGraphicsEvent::ImageReady(image));
                if let Some(placement) = placement {
                    result.events.push(TerminalGraphicsEvent::Place(placement));
                }
                result.terminal_bytes.extend(advance);
            }
            Ok(Some(KittyDecodeResult::Placement { placement, advance })) => {
                result.events.push(TerminalGraphicsEvent::Place(placement));
                result.terminal_bytes.extend(advance);
            }
            Ok(None) => {}
            Err(error) => result
                .events
                .push(TerminalGraphicsEvent::Error(error.to_string())),
        }
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
        enforce_storage_limit(payload.len(), self.options.storage_limit_mb)?;
        let name = params
            .get("name")
            .and_then(|value| BASE64.decode(value).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok());
        let decoded = decode_image_bytes(&payload, self.options.pixel_limit)?;
        let width = params
            .get("width")
            .and_then(|value| parse_pixel_size(value, decoded.width));
        let height = params
            .get("height")
            .and_then(|value| parse_pixel_size(value, decoded.height));
        let image = TerminalImageData {
            id: self.next_id(),
            protocol: TerminalImageProtocol::Iterm2,
            version: 0,
            width: width.unwrap_or(decoded.width),
            height: height.unwrap_or(decoded.height),
            rgba: decoded.rgba.into(),
            name,
        };
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
    ) -> Result<(TerminalImageData, TerminalImagePlacement, Vec<u8>), GraphicsError> {
        enforce_storage_limit(sequence.len(), self.options.storage_limit_mb)?;
        let decoded = icy_sixel::SixelImage::decode(sequence)
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let width = decoded.width as u32;
        let height = decoded.height as u32;
        enforce_pixel_limit(width, height, self.options.pixel_limit)?;
        let image = TerminalImageData {
            id: self.next_id(),
            protocol: TerminalImageProtocol::Sixel,
            version: 0,
            width,
            height,
            rgba: decoded.pixels.into(),
            name: None,
        };
        let (placement, advance) = self.placement_for_image(
            image.id,
            image.protocol,
            width,
            height,
            cursor,
            PlacementOptions {
                move_cursor: true,
                cols: None,
                rows: None,
                source_x: 0,
                source_y: 0,
                source_width: width,
                source_height: height,
                z_index: 0,
            },
        );
        Ok((image, placement, advance))
    }

    fn decode_kitty(
        &mut self,
        data: &[u8],
        cursor: GraphicsCursor,
    ) -> Result<Option<KittyDecodeResult>, GraphicsError> {
        let Some((params, payload)) = parse_kitty_params_and_payload(data) else {
            return Ok(None);
        };
        let command_action = params.get("a").map(String::as_str).unwrap_or("t");
        if command_action == "d" || command_action == "q" {
            return Ok(None);
        }
        if command_action == "p" {
            let (placement, advance) = self.place_kitty_image(&params, cursor)?;
            return Ok(Some(KittyDecodeResult::Placement { placement, advance }));
        }

        let explicit_id = params
            .get("i")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| {
                if self.kitty_chunks.len() == 1 {
                    *self.kitty_chunks.keys().next().expect("pending kitty chunk")
                } else {
                    self.next_image_id
                }
            });
        let more = params.get("m").is_some_and(|value| value == "1");
        let mut assembly =
            self.kitty_chunks
                .remove(&explicit_id)
                .unwrap_or_else(|| KittyChunkAssembly {
                    params: params.clone(),
                    encoded: Vec::new(),
                });
        for (key, value) in &params {
            assembly.params.insert(key.clone(), value.clone());
        }
        assembly.encoded.extend_from_slice(payload);
        if assembly.encoded.len() > encoded_storage_limit(self.options.storage_limit_mb) {
            return Err(GraphicsError::StorageLimitExceeded);
        }
        if more {
            self.kitty_chunks.insert(explicit_id, assembly);
            return Ok(None);
        }
        let params = assembly.params;
        let action = params.get("a").map(String::as_str).unwrap_or("t");
        if action != "t" && action != "T" {
            return Err(GraphicsError::UnsupportedImage);
        }
        let complete =
            decode_kitty_payload(&params, &assembly.encoded, self.options.storage_limit_mb)?;

        let decoded = match params.get("f").map(String::as_str) {
            Some("24") => decode_raw_rgb(&complete, &params)?,
            Some("32") => decode_raw_rgba(&complete, &params)?,
            _ => decode_image_bytes(&complete, self.options.pixel_limit)?,
        };
        let image_id = TerminalImageId(explicit_id);
        self.next_image_id = self.next_image_id.max(explicit_id + 1);
        self.kitty_images.insert(
            image_id,
            KittyImageRecord {
                width: decoded.width,
                height: decoded.height,
            },
        );
        let image = TerminalImageData {
            id: image_id,
            protocol: TerminalImageProtocol::Kitty,
            version: 0,
            width: decoded.width,
            height: decoded.height,
            rgba: decoded.rgba.into(),
            name: None,
        };
        if action == "t" {
            return Ok(Some(KittyDecodeResult::Image {
                image,
                placement: None,
                advance: Vec::new(),
            }));
        }
        let placement_options = kitty_placement_options(&params, image.width, image.height, cursor);
        let (placement, advance) = self.placement_for_image(
            image.id,
            image.protocol,
            image.width,
            image.height,
            cursor,
            placement_options,
        );
        Ok(Some(KittyDecodeResult::Image {
            image,
            placement: Some(placement),
            advance,
        }))
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
            .copied()
            .ok_or(GraphicsError::UnsupportedImage)?;
        let placement_options = kitty_placement_options(params, image.width, image.height, cursor);
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
        let (default_cols, default_rows) = cursor.image_cells(options.source_width, options.source_height);
        let cols = options.cols.unwrap_or(default_cols).clamp(1, cursor.cols.max(1));
        let rows = options.rows.unwrap_or(default_rows).clamp(1, cursor.rows.max(1));
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
            let pixel_height =
                pixel_width.saturating_mul(u64::from(source_height)) / u64::from(source_width.max(1));
            let rows = pixel_height
                .div_ceil(u64::from(cursor.cell_height.max(1)))
                .max(1) as usize;
            (Some(cols), Some(rows))
        }
        (None, Some(rows)) => {
            let pixel_height = rows as u64 * u64::from(cursor.cell_height.max(1));
            let pixel_width =
                pixel_height.saturating_mul(u64::from(source_width)) / u64::from(source_height.max(1));
            let cols = pixel_width
                .div_ceil(u64::from(cursor.cell_width.max(1)))
                .max(1) as usize;
            (Some(cols), Some(rows))
        }
        both => both,
    }
}

fn parse_positive_usize(value: Option<&String>) -> Option<usize> {
    value.and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn parse_u32(value: Option<&String>) -> Option<u32> {
    value.and_then(|value| value.parse::<u32>().ok())
}
