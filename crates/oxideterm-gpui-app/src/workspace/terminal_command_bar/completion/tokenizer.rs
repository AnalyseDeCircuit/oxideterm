fn tokenize_terminal_command_line(input: &str, cursor_index: usize) -> TerminalShellParseResult {
    let cursor = cursor_index.min(input.len());
    let mut tokens = Vec::new();
    let mut token_start: Option<usize> = None;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut reliable = true;
    let mut token_quote: Option<char> = None;

    let push_token = |tokens: &mut Vec<TerminalShellToken>,
                      token_start: &mut Option<usize>,
                      token_quote: &mut Option<char>,
                      end: usize| {
        let Some(start) = *token_start else {
            return;
        };
        let value = unescape_terminal_token(&input[start..end], *token_quote);
        tokens.push(TerminalShellToken {
            value,
            start,
            end,
            quote: *token_quote,
        });
        *token_start = None;
        *token_quote = None;
    };

    for (index, char) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if char == '\\' {
            if token_start.is_none() {
                token_start = Some(index);
            }
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if char == active_quote {
                quote = None;
            }
            continue;
        }
        if char == '"' || char == '\'' {
            if token_start.is_none() {
                token_start = Some(index);
                token_quote = Some(char);
            }
            quote = Some(char);
            continue;
        }
        if char.is_whitespace() {
            push_token(&mut tokens, &mut token_start, &mut token_quote, index);
            continue;
        }
        if token_start.is_none() {
            token_start = Some(index);
            token_quote = None;
        }
    }

    push_token(&mut tokens, &mut token_start, &mut token_quote, input.len());
    if quote.is_some() || escaped {
        reliable = false;
    }

    let current_token_index = tokens
        .iter()
        .position(|token| cursor >= token.start && cursor <= token.end)
        .map(|index| index as isize)
        .unwrap_or(-1);
    let current_token = if current_token_index >= 0 {
        tokens[current_token_index as usize].clone()
    } else {
        TerminalShellToken {
            value: String::new(),
            start: cursor,
            end: cursor,
            quote: None,
        }
    };
    TerminalShellParseResult {
        reliable,
        command_name: tokens.first().map(|token| token.value.clone()),
        tokens,
        current_token,
        current_token_index,
    }
}

fn unescape_terminal_token(raw: &str, quote: Option<char>) -> String {
    let mut value = raw.to_string();
    if let Some(quote) = quote {
        if value.starts_with(quote) {
            value.remove(0);
        }
        if value.ends_with(quote) {
            value.pop();
        }
    }
    let mut output = String::new();
    let mut escaped = false;
    for char in value.chars() {
        if escaped {
            output.push(char);
            escaped = false;
        } else if char == '\\' {
            escaped = true;
        } else {
            output.push(char);
        }
    }
    if escaped {
        output.push('\\');
    }
    output
}
