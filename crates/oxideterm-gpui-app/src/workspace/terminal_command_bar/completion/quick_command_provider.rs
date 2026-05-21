impl WorkspaceApp {
    fn terminal_command_quick_command_suggestions(
        &self,
        input: &str,
        context: &TerminalCommandContext,
    ) -> Vec<TerminalCommandSuggestion> {
        let command_bar_settings = &self.settings_store.settings().terminal.command_bar;
        if !command_bar_settings.quick_commands_enabled {
            return Vec::new();
        }
        let query = input.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        let target_fields = context.target_fields();
        self.quick_commands
            .commands
            .iter()
            .filter(|command| {
                match_quick_command_host_pattern(command.host_pattern.as_deref(), &target_fields)
                    && (command.name.to_lowercase().contains(&query)
                        || command.command.to_lowercase().contains(&query)
                        || command
                            .description
                            .as_ref()
                            .is_some_and(|description| description.to_lowercase().contains(&query)))
            })
            .take(8)
            .map(|command| {
                let risk = classify_command_risk(&command.command);
                let starts_with_input = command
                    .command
                    .to_lowercase()
                    .starts_with(&input.trim_start().to_lowercase());
                TerminalCommandSuggestion {
                    kind: TerminalCommandSuggestionKind::QuickCommand,
                    label: command.command.clone(),
                    insert_text: command.command.clone(),
                    description: Some(command.name.clone()),
                    executable: true,
                    replacement: 0..input.len(),
                    group_label_key: "terminal.command_bar.group_quick_commands",
                    source_label_key: "terminal.command_bar.source_quick_command",
                    score: 860.0 - terminal_command_risk_score_penalty(risk),
                    risk,
                    inline_safe: starts_with_input && risk != Some("high"),
                }
            })
            .collect()
    }
}

fn terminal_cwd_looks_remote(cwd: &str) -> bool {
    cwd.starts_with("/home/")
        || cwd.starts_with("/root/")
        || cwd.starts_with("/srv/")
        || cwd.starts_with("/var/www/")
}

fn infer_terminal_ssh_identity_from_buffer(buffer: &str) -> Option<String> {
    let tail_start = buffer
        .char_indices()
        .rev()
        .nth(8000)
        .map(|(index, _)| index)
        .unwrap_or(0);
    buffer[tail_start..]
        .split_whitespace()
        .filter_map(terminal_ssh_identity_candidate)
        .last()
}

fn terminal_ssh_identity_candidate(token: &str) -> Option<String> {
    if token.contains('=') {
        return None;
    }
    let end = token
        .char_indices()
        .find_map(|(index, ch)| {
            (matches!(ch, ':' | '~' | '#' | '$' | '>') && token[..index].contains('@'))
                .then_some(index)
        })
        .unwrap_or(token.len());
    let candidate = token[..end].trim_matches(|ch: char| {
        !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '@'))
    });
    let (user, host) = candidate.split_once('@')?;
    if !(1..=64).contains(&user.len()) || !(1..=128).contains(&host.len()) {
        return None;
    }
    if !user
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return None;
    }
    let mut host_chars = host.chars();
    if !host_chars.next().is_some_and(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }
    if !host_chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')) {
        return None;
    }
    Some(format!("{user}@{host}"))
}

#[cfg(test)]
mod terminal_quick_command_provider_tests {
    use super::*;

    #[test]
    fn infers_last_ssh_identity_from_terminal_buffer() {
        let buffer = "Last login\nuser@example.com:~$ ssh deploy@prod-box\n\
            deploy@prod-box:/srv/app$ ";

        assert_eq!(
            infer_terminal_ssh_identity_from_buffer(buffer),
            Some("deploy@prod-box".to_string())
        );
    }

    #[test]
    fn rejects_secret_like_or_malformed_identity_tokens() {
        assert_eq!(terminal_ssh_identity_candidate("token@example.com=abc"), None);
        assert_eq!(terminal_ssh_identity_candidate("@example.com:~$"), None);
        assert_eq!(
            terminal_ssh_identity_candidate("user@example.com:~$"),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn remote_cwd_prefixes_match_tauri_command_bar_heuristic() {
        assert!(terminal_cwd_looks_remote("/home/dev/project"));
        assert!(terminal_cwd_looks_remote("/srv/app"));
        assert!(terminal_cwd_looks_remote("/var/www/site"));
        assert!(!terminal_cwd_looks_remote("/Users/dev/project"));
    }
}
