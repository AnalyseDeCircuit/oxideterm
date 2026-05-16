fn terminal_command_fig_suggestions(
    parsed: &TerminalShellParseResult,
    specs: &[TerminalFigSpec],
) -> Vec<TerminalCommandSuggestion> {
    if !parsed.reliable {
        return Vec::new();
    }
    let query = parsed.current_token.value.as_str();
    if parsed.current_token_index <= 0 {
        if query.is_empty() {
            return Vec::new();
        }
        return specs
            .iter()
            .filter(|spec| terminal_matches_prefix(&spec.name, query))
            .take(12)
            .map(|spec| TerminalCommandSuggestion {
                kind: TerminalCommandSuggestionKind::Command,
                label: spec.name.clone(),
                insert_text: format!("{} ", spec.name),
                description: Some(spec.description.clone()),
                executable: false,
                replacement: parsed.current_token.start..parsed.current_token.end,
                group_label_key: "terminal.command_bar.group_command",
                source_label_key: "terminal.command_bar.source_command",
                score: 700.0 + spec.name.len() as f64,
                risk: None,
                inline_safe: true,
            })
            .collect();
    }

    let Some(command_name) = parsed.command_name.as_deref() else {
        return Vec::new();
    };
    let Some(spec) = specs
        .iter()
        .find(|candidate| candidate.name == command_name)
    else {
        return Vec::new();
    };

    let active_subcommand = active_fig_subcommand(parsed, spec);
    let mut suggestions = Vec::new();
    if query.starts_with('-') {
        let mut options = Vec::new();
        if let Some(subcommand) = active_subcommand {
            options.extend(subcommand.options.iter().cloned());
        }
        options.extend(spec.options.iter().cloned());
        for option in options {
            if !terminal_matches_prefix(&option.name, query) {
                continue;
            }
            suggestions.push(TerminalCommandSuggestion {
                kind: TerminalCommandSuggestionKind::Option,
                label: option.name.clone(),
                insert_text: if option.args != TerminalFigArgType::None {
                    format!("{} ", option.name)
                } else {
                    option.name.clone()
                },
                description: option.description,
                executable: false,
                replacement: parsed.current_token.start..parsed.current_token.end,
                group_label_key: "terminal.command_bar.group_option",
                source_label_key: "terminal.command_bar.source_option",
                score: 620.0 + option.name.len() as f64,
                risk: None,
                inline_safe: true,
            });
        }
        suggestions.truncate(12);
        return suggestions;
    }

    if active_subcommand.is_none() {
        for subcommand in &spec.subcommands {
            if !query.is_empty() && !terminal_matches_prefix(&subcommand.name, query) {
                continue;
            }
            suggestions.push(TerminalCommandSuggestion {
                kind: TerminalCommandSuggestionKind::Subcommand,
                label: subcommand.name.clone(),
                insert_text: format!("{} ", subcommand.name),
                description: subcommand.description.clone(),
                executable: false,
                replacement: parsed.current_token.start..parsed.current_token.end,
                group_label_key: "terminal.command_bar.group_command",
                source_label_key: "terminal.command_bar.source_command",
                score: 640.0 + subcommand.name.len() as f64,
                risk: None,
                inline_safe: true,
            });
        }
    }
    suggestions.truncate(12);
    suggestions
}

fn active_fig_arg_type(
    parsed: &TerminalShellParseResult,
    specs: &[TerminalFigSpec],
) -> TerminalFigArgType {
    if !parsed.reliable || parsed.command_name.is_none() {
        return TerminalFigArgType::None;
    }
    let Some(spec) = specs
        .iter()
        .find(|candidate| candidate.name.as_str() == parsed.command_name.as_deref().unwrap_or(""))
    else {
        return TerminalFigArgType::None;
    };
    if parsed.current_token_index <= 0 {
        return TerminalFigArgType::None;
    }

    let active_subcommand = active_fig_subcommand(parsed, spec);
    let previous = parsed
        .tokens
        .get(parsed.current_token_index as usize - 1)
        .map(|token| token.value.as_str());
    if let Some(option) = previous.and_then(|previous| {
        active_subcommand
            .into_iter()
            .flat_map(|subcommand| subcommand.options.iter())
            .chain(spec.options.iter())
            .find(|option| option.name == previous && option.args != TerminalFigArgType::None)
    }) {
        return option.args;
    }

    if let Some(subcommand) = active_subcommand
        && parsed.current_token_index as usize
            > subcommand_token_index(parsed, spec).unwrap_or(usize::MAX)
        && matches!(
            subcommand.args,
            TerminalFigArgType::Path | TerminalFigArgType::File | TerminalFigArgType::Directory
        )
    {
        return subcommand.args;
    }

    match spec.args {
        TerminalFigArgType::Path | TerminalFigArgType::File | TerminalFigArgType::Directory => {
            spec.args
        }
        _ => TerminalFigArgType::None,
    }
}

fn active_fig_subcommand<'a>(
    parsed: &TerminalShellParseResult,
    spec: &'a TerminalFigSpec,
) -> Option<&'a TerminalFigSubcommandSpec> {
    let index = subcommand_token_index(parsed, spec)?;
    let value = parsed.tokens.get(index)?.value.as_str();
    spec.subcommands
        .iter()
        .find(|subcommand| subcommand.name == value)
}

fn subcommand_token_index(
    parsed: &TerminalShellParseResult,
    spec: &TerminalFigSpec,
) -> Option<usize> {
    if parsed.current_token_index <= 1 {
        return None;
    }
    parsed.tokens[1..parsed.current_token_index as usize]
        .iter()
        .position(|token| {
            spec.subcommands
                .iter()
                .any(|subcommand| subcommand.name == token.value)
        })
        .map(|index| index + 1)
}

fn terminal_matches_prefix(value: &str, query: &str) -> bool {
    value.to_lowercase().starts_with(&query.to_lowercase())
}
