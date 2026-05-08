fn parser_state_len(state: &ParserState) -> usize {
    match state {
        ParserState::Osc(data)
        | ParserState::OscEsc(data)
        | ParserState::Dcs(data)
        | ParserState::DcsEsc(data)
        | ParserState::Apc(data)
        | ParserState::ApcEsc(data) => data.len(),
        ParserState::Ground | ParserState::Esc => 0,
    }
}

fn parse_semicolon_params(data: &[u8]) -> HashMap<String, String> {
    split_params(data, b';')
}

fn parse_comma_params(data: &[u8]) -> HashMap<String, String> {
    split_params(data, b',')
}

fn parse_kitty_params_and_payload(data: &[u8]) -> Option<(HashMap<String, String>, &[u8])> {
    let separator = data.iter().position(|byte| *byte == b';')?;
    Some((
        parse_comma_params(&data[..separator]),
        &data[separator + 1..],
    ))
}

fn parse_kitty_command(data: &[u8]) -> (HashMap<String, String>, Option<&[u8]>) {
    match data.iter().position(|byte| *byte == b';') {
        Some(separator) => (
            parse_comma_params(&data[..separator]),
            Some(&data[separator + 1..]),
        ),
        None => (parse_comma_params(data), None),
    }
}

fn kitty_query_response(id: u64) -> Vec<u8> {
    format!("\x1b_Gi={id};OK\x1b\\").into_bytes()
}

fn split_params(data: &[u8], separator: u8) -> HashMap<String, String> {
    data.split(|byte| *byte == separator)
        .filter_map(|part| {
            let index = part.iter().position(|byte| *byte == b'=')?;
            let (key, rest) = part.split_at(index);
            let value = &rest[1..];
            Some((
                String::from_utf8_lossy(key).to_string(),
                String::from_utf8_lossy(value).to_string(),
            ))
        })
        .collect()
}

fn parse_pixel_size(value: &str, fallback: u32) -> Option<u32> {
    if let Some(px) = value.strip_suffix("px") {
        px.parse().ok()
    } else if value == "auto" {
        Some(fallback)
    } else {
        value.parse().ok()
    }
}

fn advance_bytes(
    start_col: usize,
    image_cols: usize,
    image_rows: usize,
    terminal_cols: usize,
) -> Vec<u8> {
    if image_cols == 0 || image_rows == 0 {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    let mut remaining_rows = image_rows;
    let mut col = start_col;
    while remaining_rows > 0 {
        let cols_this_row = image_cols.min(terminal_cols.saturating_sub(col).max(1));
        bytes.extend(std::iter::repeat_n(b' ', cols_this_row));
        remaining_rows -= 1;
        if remaining_rows > 0 {
            bytes.extend_from_slice(b"\r\n");
            col = 0;
        }
    }
    bytes
}

