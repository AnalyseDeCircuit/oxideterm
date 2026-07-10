use std::{path::Path, sync::OnceLock, time::SystemTime};

#[derive(Clone, Debug)]
pub struct TerminalShellToken {
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub quote: Option<char>,
}

#[derive(Clone, Debug)]
pub struct TerminalShellParseResult {
    pub reliable: bool,
    pub tokens: Vec<TerminalShellToken>,
    pub current_token: TerminalShellToken,
    pub current_token_index: isize,
    pub command_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HistoryFileSnapshot {
    path: std::path::PathBuf,
    modified: Option<SystemTime>,
    len: u64,
}

#[derive(Clone, Debug)]
struct LocalShellHistoryCache {
    home: std::path::PathBuf,
    files: Vec<HistoryFileSnapshot>,
    commands: Vec<String>,
}

pub fn tokenize_terminal_command_line(
    input: &str,
    cursor_index: usize,
) -> TerminalShellParseResult {
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
        tokens.push(TerminalShellToken {
            value: unescape_terminal_token(&input[start..end], *token_quote),
            start,
            end,
            quote: *token_quote,
        });
        *token_start = None;
        *token_quote = None;
    };

    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            token_start.get_or_insert(index);
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            continue;
        }
        if character == '"' || character == '\'' {
            if token_start.is_none() {
                token_start = Some(index);
                token_quote = Some(character);
            }
            quote = Some(character);
            continue;
        }
        if character.is_whitespace() {
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
    let current_token = current_token_index
        .try_into()
        .ok()
        .and_then(|index: usize| tokens.get(index).cloned())
        .unwrap_or(TerminalShellToken {
            value: String::new(),
            start: cursor,
            end: cursor,
            quote: None,
        });
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
    for character in value.chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        output.push('\\');
    }
    output
}

pub fn escape_terminal_path_for_shell(value: &str, quoted: bool) -> String {
    let special = if quoted {
        "\"\\$`"
    } else {
        " \"'\\$`!&|;<>[]{}()*?"
    };
    let mut escaped = String::new();
    for character in value.chars() {
        if special.contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

pub fn normalize_terminal_autosuggest_command(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn terminal_autosuggest_fuzzy_score(command: &str, query: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    if command.starts_with(query) {
        return 1000.0 + query.len() as f64 * 8.0;
    }
    let lower_command = command.to_lowercase();
    let lower_query = query.to_lowercase();
    if lower_command.starts_with(&lower_query) {
        return 850.0 + query.len() as f64 * 6.0;
    }
    if lower_command.contains(&lower_query) {
        return 450.0 + query.len() as f64 * 4.0;
    }

    let query_characters = lower_query.chars().collect::<Vec<_>>();
    let mut query_index = 0usize;
    let mut score = 0.0;
    for character in lower_command.chars() {
        if query_index < query_characters.len() && character == query_characters[query_index] {
            query_index += 1;
            score += 20.0;
        }
    }
    if query_index == query_characters.len() {
        score
    } else {
        0.0
    }
}

pub fn is_likely_secret_terminal_command(command: &str) -> bool {
    let normalized = command.trim();
    if normalized.is_empty() {
        return false;
    }
    let lower = normalized.to_lowercase();
    lower.contains("authorization")
        || lower.contains("bearer")
        || lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("passphrase")
        || lower.contains("private_key")
        || lower.contains("private-key")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("access_key")
        || lower.contains("access-key")
        || lower.contains("github_token")
        || lower.contains("openai_api_key")
        || lower.contains("anthropic_api_key")
        || lower.contains("aws_secret_access_key")
        || has_secret_assignment(normalized)
        || has_password_flag(normalized)
}

fn has_secret_assignment(command: &str) -> bool {
    command.split_whitespace().any(|token| {
        let lower = token.to_lowercase();
        token.contains('=')
            && (lower.contains("token")
                || lower.contains("secret")
                || lower.contains("password")
                || lower.contains("api_key")
                || lower.contains("api-key"))
    })
}

fn has_password_flag(command: &str) -> bool {
    let mut tokens = command.split_whitespace();
    while let Some(token) = tokens.next() {
        if matches!(token, "-p" | "--password" | "--passphrase") && tokens.next().is_some() {
            return true;
        }
    }
    false
}

pub fn load_local_shell_history_commands() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    load_local_shell_history_commands_from_home(Path::new(&home))
}

fn load_local_shell_history_commands_from_home(home: &Path) -> Vec<String> {
    const MAX_HISTORY_BYTES: usize = 512 * 1024;
    const MAX_COMMANDS: usize = 500;
    static LOCAL_SHELL_HISTORY: OnceLock<std::sync::Mutex<Option<LocalShellHistoryCache>>> =
        OnceLock::new();
    let files = [
        ".zsh_history",
        ".bash_history",
        ".zhistory",
        ".local/share/fish/fish_history",
    ];
    let home = home.to_path_buf();
    let snapshots = history_file_snapshots(&home, &files);
    let cache = LOCAL_SHELL_HISTORY.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(guard) = cache.lock()
        && let Some(cache) = guard.as_ref()
        && cache.home == home
        && cache.files == snapshots
    {
        return cache.commands.clone();
    }

    let mut commands = Vec::new();
    for file in files {
        let path = home.join(file);
        let Ok(mut content) = std::fs::read(&path) else {
            continue;
        };
        if content.len() > MAX_HISTORY_BYTES {
            content = content[content.len() - MAX_HISTORY_BYTES..].to_vec();
        }
        commands.extend(parse_terminal_history_file(
            file,
            &String::from_utf8_lossy(&content),
        ));
    }
    if commands.len() > MAX_COMMANDS {
        commands = commands.split_off(commands.len() - MAX_COMMANDS);
    }
    // Secret-like history remains excluded before the cache owns command text.
    commands.retain(|command| !is_likely_secret_terminal_command(command));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(LocalShellHistoryCache {
            home,
            files: snapshots,
            commands: commands.clone(),
        });
    }
    commands
}

fn history_file_snapshots(home: &Path, files: &[&str]) -> Vec<HistoryFileSnapshot> {
    files
        .iter()
        .filter_map(|file| {
            let path = home.join(file);
            let metadata = std::fs::metadata(&path).ok()?;
            Some(HistoryFileSnapshot {
                path,
                modified: metadata.modified().ok(),
                len: metadata.len(),
            })
        })
        .collect()
}

fn parse_terminal_history_file(path: &str, content: &str) -> Vec<String> {
    if path.contains("fish_history") {
        return content
            .lines()
            .filter_map(|line| line.strip_prefix("- cmd: "))
            .map(|line| line.replace("\\n", "\n").trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
    }
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(": ")
                && let Some((_, command)) = rest.split_once(';')
            {
                return Some(command.trim().to_string());
            }
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_quoted_and_escaped_shell_input() {
        let parsed = tokenize_terminal_command_line("git add 'two words' three\\ four", 8);

        assert!(parsed.reliable);
        assert_eq!(
            parsed
                .tokens
                .iter()
                .map(|token| token.value.as_str())
                .collect::<Vec<_>>(),
            ["git", "add", "two words", "three four"]
        );
    }

    #[test]
    fn parses_tauri_history_formats() {
        assert_eq!(
            parse_terminal_history_file(".zsh_history", ": 1700000000:0;git status\ncargo test\n"),
            ["git status", "cargo test"]
        );
        assert_eq!(
            parse_terminal_history_file(
                ".local/share/fish/fish_history",
                "- cmd: echo hello\\nworld\n- when: 1700000000\n- cmd: ls -la\n"
            ),
            ["echo hello\nworld", "ls -la"]
        );
    }

    #[test]
    fn rejects_secret_like_history_before_caching() {
        assert!(is_likely_secret_terminal_command(
            "curl -H 'Authorization: Bearer abc'"
        ));
        assert!(is_likely_secret_terminal_command("TOKEN=value deploy"));
        assert!(!is_likely_secret_terminal_command("cargo test --workspace"));
    }
}
