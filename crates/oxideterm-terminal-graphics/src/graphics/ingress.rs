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
    ImageUpdated {
        image: TerminalImageData,
    },
    Placement {
        placement: TerminalImagePlacement,
        advance: Vec<u8>,
    },
}

fn terminal_image_data_from_decoded(
    id: TerminalImageId,
    protocol: TerminalImageProtocol,
    decoded: DecodedPixels,
    name: Option<String>,
) -> TerminalImageData {
    let rgba: Arc<[u8]> = decoded.rgba.into();
    let frames: Vec<TerminalImageFrame> = decoded
        .frames
        .into_iter()
        .enumerate()
        .map(|(index, frame)| {
            // Reuse the primary static buffer for the first animation frame so
            // the always-present still preview does not duplicate frame zero.
            let frame_rgba = if index == 0 && frame.rgba.as_slice() == rgba.as_ref() {
                rgba.clone()
            } else {
                frame.rgba.into()
            };
            TerminalImageFrame {
                rgba: frame_rgba,
                delay_ms_numerator: frame.delay_ms_numerator,
                delay_ms_denominator: frame.delay_ms_denominator,
                gapless: false,
            }
        })
        .collect();
    let animation = TerminalImageAnimationState {
        running: !frames.is_empty(),
        loading: false,
        current_frame: 0,
        loop_limit: None,
    };
    TerminalImageData {
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
            "a" => {
                match self.control_kitty_animation(&params) {
                    Ok(image) => result.events.push(TerminalGraphicsEvent::ImageUpdated(image)),
                    Err(error) => result
                        .events
                        .push(TerminalGraphicsEvent::Error(error.to_string())),
                }
                return;
            }
            "c" => {
                match self.compose_kitty_animation_frame(&params) {
                    Ok(image) => result.events.push(TerminalGraphicsEvent::ImageUpdated(image)),
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
            Ok(Some(KittyDecodeResult::ImageUpdated { image })) => {
                result.events.push(TerminalGraphicsEvent::ImageUpdated(image));
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
        let mut image =
            terminal_image_data_from_decoded(self.next_id(), TerminalImageProtocol::Iterm2, decoded, name);
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
            frames: Vec::new(),
            animation: TerminalImageAnimationState::default(),
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
        if action != "t" && action != "T" && action != "f" {
            return Err(GraphicsError::UnsupportedImage);
        }
        let complete =
            decode_kitty_payload(&params, &assembly.encoded, self.options.storage_limit_mb)?;

        let decoded = match params.get("f").map(String::as_str) {
            Some("24") => decode_raw_rgb(&complete, &params)?,
            Some("32") => decode_raw_rgba(&complete, &params)?,
            _ => decode_image_bytes(&complete, self.options.pixel_limit)?,
        };
        if action == "f" {
            let image = self.add_kitty_animation_frame(&params, decoded)?;
            return Ok(Some(KittyDecodeResult::ImageUpdated { image }));
        }
        let image_id = TerminalImageId(explicit_id);
        self.next_image_id = self.next_image_id.max(explicit_id + 1);
        let image =
            terminal_image_data_from_decoded(image_id, TerminalImageProtocol::Kitty, decoded, None);
        self.kitty_images.insert(image_id, image.clone());
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
        ensure_kitty_animation_root(image);
        let target_frame = parse_kitty_frame_index(params.get("r"));
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
        let dest_x = parse_u32(params.get("x")).unwrap_or_default();
        let dest_y = parse_u32(params.get("y")).unwrap_or_default();
        if dest_x.saturating_add(decoded.width) > image.width
            || dest_y.saturating_add(decoded.height) > image.height
        {
            return Err(GraphicsError::UnsupportedImage);
        }
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
            if let Some((delay_ms, gapless)) = gap {
                frame.delay_ms_numerator = delay_ms;
                frame.delay_ms_denominator = 1;
                frame.gapless = gapless;
            }
            target_frame
        } else {
            let (delay_ms, gapless) = gap.unwrap_or((40, false));
            image.frames.push(TerminalImageFrame {
                rgba: canvas.into(),
                delay_ms_numerator: delay_ms,
                delay_ms_denominator: 1,
                gapless,
            });
            image.frames.len() - 1
        };
        if frame_index == 0 {
            image.rgba = image.frames[0].rgba.clone();
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
        ensure_kitty_animation_root(image);

        if let Some(current_frame) = parse_kitty_frame_index(params.get("c")) {
            if current_frame >= image.frames.len() {
                return Err(GraphicsError::UnsupportedImage);
            }
            image.animation.current_frame = current_frame;
            image.animation.running = false;
        }
        if let Some((delay_ms, gapless)) = parse_kitty_frame_gap(params.get("z")) {
            let frame_index = parse_kitty_frame_index(params.get("r"))
                .unwrap_or(image.animation.current_frame)
                .min(image.frames.len().saturating_sub(1));
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
                    image.animation.loop_limit = Some(1);
                }
                3 => {
                    image.animation.running = true;
                    image.animation.loading = false;
                    image.animation.loop_limit = kitty_loop_limit(params.get("v"));
                }
                _ => return Err(GraphicsError::UnsupportedImage),
            }
        } else if let Some(loop_limit) = kitty_loop_limit(params.get("v")) {
            image.animation.loop_limit = Some(loop_limit);
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
        ensure_kitty_animation_root(image);
        let source_frame =
            parse_kitty_frame_index(params.get("r")).ok_or(GraphicsError::UnsupportedImage)?;
        let dest_frame =
            parse_kitty_frame_index(params.get("c")).ok_or(GraphicsError::UnsupportedImage)?;
        if source_frame >= image.frames.len() || dest_frame >= image.frames.len() {
            return Err(GraphicsError::UnsupportedImage);
        }

        let width = parse_u32(params.get("w"))
            .filter(|width| *width > 0)
            .unwrap_or(image.width);
        let height = parse_u32(params.get("h"))
            .filter(|height| *height > 0)
            .unwrap_or(image.height);
        let source_x = parse_u32(params.get("x")).unwrap_or_default();
        let source_y = parse_u32(params.get("y")).unwrap_or_default();
        let dest_x = parse_u32(params.get("X")).unwrap_or_default();
        let dest_y = parse_u32(params.get("Y")).unwrap_or_default();
        if source_x.saturating_add(width) > image.width
            || source_y.saturating_add(height) > image.height
            || dest_x.saturating_add(width) > image.width
            || dest_y.saturating_add(height) > image.height
            || (source_frame == dest_frame
                && rects_overlap(source_x, source_y, dest_x, dest_y, width, height))
        {
            return Err(GraphicsError::UnsupportedImage);
        }

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
        if dest_frame == 0 {
            image.rgba = dest.rgba.clone();
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
        Some(limit) => Some(limit),
    }
}

fn ensure_kitty_animation_root(image: &mut TerminalImageData) {
    if image.frames.is_empty() {
        image.frames.push(TerminalImageFrame {
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
        let dest_premultiplied =
            u32::from(dest[channel]) * dest_alpha * inverse_source_alpha / 255;
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
