use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::SystemTime,
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone)]
pub struct TerminalShellToken {
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub quote: Option<char>,
}

impl Drop for TerminalShellToken {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

#[derive(Clone)]
pub struct TerminalShellParseResult {
    pub reliable: bool,
    pub tokens: Vec<TerminalShellToken>,
    pub current_token: TerminalShellToken,
    pub current_token_index: isize,
    pub command_name: Option<String>,
}

impl Drop for TerminalShellParseResult {
    fn drop(&mut self) {
        self.command_name.zeroize();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HistoryFileSnapshot {
    path: std::path::PathBuf,
    modified: Option<SystemTime>,
    len: u64,
}

struct LocalShellHistoryCache {
    files: Vec<HistoryFileSnapshot>,
    commands: Zeroizing<Vec<String>>,
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
    let token_at_cursor = tokens
        .iter()
        .position(|token| cursor >= token.start && cursor <= token.end);
    // Whitespace after a command starts the next argument, including an empty one.
    let current_token_index =
        token_at_cursor.unwrap_or_else(|| tokens.partition_point(|token| token.end < cursor));
    let current_token = token_at_cursor
        .map(|index| tokens[index].clone())
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
        current_token_index: current_token_index as isize,
    }
}

fn unescape_terminal_token(raw: &str, quote: Option<char>) -> String {
    let value = if let Some(quote) = quote {
        let unquoted = raw.strip_prefix(quote).unwrap_or(raw);
        unquoted.strip_suffix(quote).unwrap_or(unquoted)
    } else {
        raw
    };
    let mut output = String::with_capacity(value.len());
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

pub fn load_local_shell_history_commands() -> Vec<String> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    #[cfg(windows)]
    let app_data = std::env::var_os("APPDATA");
    #[cfg(not(windows))]
    let app_data: Option<std::ffi::OsString> = None;
    load_local_shell_history_commands_from_paths(&local_shell_history_paths(
        home.as_deref().map(Path::new),
        app_data.as_deref().map(Path::new),
    ))
}

fn local_shell_history_paths(home: Option<&Path>, app_data: Option<&Path>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(home) = home {
        files.extend(
            [
                ".zsh_history",
                ".bash_history",
                ".zhistory",
                ".local/share/fish/fish_history",
            ]
            .map(|file| home.join(file)),
        );
    }
    if let Some(app_data) = app_data {
        files
            .push(app_data.join("Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt"));
    }
    files
}

fn load_local_shell_history_commands_from_paths(files: &[PathBuf]) -> Vec<String> {
    const MAX_HISTORY_BYTES: usize = 512 * 1024;
    const MAX_COMMANDS: usize = 500;
    static LOCAL_SHELL_HISTORY: OnceLock<std::sync::Mutex<Option<LocalShellHistoryCache>>> =
        OnceLock::new();
    let snapshots = history_file_snapshots(files);
    let cache = LOCAL_SHELL_HISTORY.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(guard) = cache.lock()
        && let Some(cache) = guard.as_ref()
        && cache.files == snapshots
    {
        return cache.commands.to_vec();
    }

    let mut commands = Vec::new();
    for path in files {
        let Ok(content) = std::fs::read(path) else {
            continue;
        };
        // History remains shell-owned; temporary reads and cached copies are cleared on drop.
        let content = Zeroizing::new(content);
        let tail = &content[content.len().saturating_sub(MAX_HISTORY_BYTES)..];
        let text = Zeroizing::new(String::from_utf8_lossy(tail).into_owned());
        commands.extend(parse_terminal_history_file(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
            &text,
        ));
    }
    if commands.len() > MAX_COMMANDS {
        commands = commands.split_off(commands.len() - MAX_COMMANDS);
    }
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(LocalShellHistoryCache {
            files: snapshots,
            commands: Zeroizing::new(commands.clone()),
        });
    }
    commands
}

fn history_file_snapshots(files: &[PathBuf]) -> Vec<HistoryFileSnapshot> {
    files
        .iter()
        .filter_map(|path| {
            let metadata = std::fs::metadata(&path).ok()?;
            Some(HistoryFileSnapshot {
                path: path.clone(),
                modified: metadata.modified().ok(),
                len: metadata.len(),
            })
        })
        .collect()
}

fn parse_terminal_history_file(path: &str, content: &str) -> Vec<String> {
    if path == "ConsoleHost_history.txt" {
        let mut commands = Vec::new();
        let mut command = Zeroizing::new(String::new());
        for line in content.trim_start_matches('\u{feff}').lines() {
            // PSReadLine appends a backtick to each non-final line of a history entry.
            if let Some(line) = line.strip_suffix('`') {
                command.push_str(line);
                command.push('\n');
            } else {
                command.push_str(line);
                if !command.trim().is_empty() {
                    commands.push(command.trim().to_owned());
                }
                command.zeroize();
            }
        }
        return commands;
    }
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
    fn windows_history_loads_without_home_and_refreshes_after_shell_writes() {
        let app_data = tempfile::tempdir().unwrap();
        let paths = local_shell_history_paths(None, Some(app_data.path()));
        let path = app_data
            .path()
            .join("Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "Get-Date\r\n").unwrap();
        assert_eq!(
            load_local_shell_history_commands_from_paths(&paths),
            ["Get-Date"]
        );
        std::fs::write(&path, "Get-Date\r\nGet-Process\r\n").unwrap();
        assert_eq!(
            load_local_shell_history_commands_from_paths(&paths),
            ["Get-Date", "Get-Process"]
        );
    }

    #[test]
    fn psreadline_history_restores_multiline_commands() {
        assert_eq!(
            parse_terminal_history_file(
                "ConsoleHost_history.txt",
                "Get-Date\r\nWrite-Output `\r\n  'hello'\r\n"
            ),
            ["Get-Date", "Write-Output \n  'hello'"]
        );
    }

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
    fn preserves_shell_owned_history_without_content_filtering() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join(".bash_history"),
            "TOKEN=value deploy\ncargo test --workspace\n",
        )
        .unwrap();

        assert_eq!(
            load_local_shell_history_commands_from_paths(&local_shell_history_paths(
                Some(home.path()),
                None
            )),
            ["TOKEN=value deploy", "cargo test --workspace"]
        );
    }
}
