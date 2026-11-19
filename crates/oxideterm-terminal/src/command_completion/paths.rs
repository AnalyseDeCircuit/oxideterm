use super::types::{TerminalPathCacheEntry, TerminalPathCompletionCache};
use super::{
    TerminalCommandSuggestion, TerminalCommandSuggestionKind, TerminalFigArgType,
    TerminalPathEntry, TerminalPathParts,
};
use crate::{TerminalShellParseResult, TerminalShellToken, escape_terminal_path_for_shell};

pub fn should_run_terminal_path_provider(
    token: &TerminalShellToken,
    active_arg_type: TerminalFigArgType,
) -> bool {
    looks_terminal_path_like(&token.value)
        || matches!(
            active_arg_type,
            TerminalFigArgType::Path | TerminalFigArgType::File | TerminalFigArgType::Directory
        )
}

fn looks_terminal_path_like(token: &str) -> bool {
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with('~')
        || token.contains('/')
}

pub fn normalize_terminal_path_token(
    token: &TerminalShellToken,
    cwd: Option<&str>,
) -> Option<TerminalPathParts> {
    let value = token.value.as_str();
    let home = cwd.and_then(infer_terminal_home_from_cwd);
    let expanded = if value.starts_with('~') {
        home.map(|home| format!("{home}{}", &value[1..]))
            .unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    };
    if let Some(slash_index) = expanded.rfind('/') {
        let directory = if slash_index == 0 {
            "/".to_string()
        } else {
            expanded[..slash_index].to_string()
        };
        let query = expanded[slash_index + 1..].to_string();
        let display_prefix = value
            .rfind('/')
            .map(|index| value[..=index].to_string())
            .unwrap_or_default();
        return Some(TerminalPathParts {
            directory,
            query,
            display_prefix,
        });
    }
    let cwd = cwd.unwrap_or(".");
    Some(TerminalPathParts {
        directory: cwd.to_string(),
        query: expanded,
        display_prefix: String::new(),
    })
}

fn infer_terminal_home_from_cwd(cwd: &str) -> Option<String> {
    for prefix in ["/Users/", "/home/"] {
        if let Some(rest) = cwd.strip_prefix(prefix) {
            let user = rest.split('/').next().filter(|user| !user.is_empty())?;
            return Some(format!("{prefix}{user}"));
        }
    }
    cwd.starts_with("/root").then(|| "/root".to_string())
}

pub fn read_local_terminal_path_entries(
    directory: &str,
) -> std::io::Result<Vec<TerminalPathEntry>> {
    Ok(std::fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            Some(TerminalPathEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().to_string_lossy().to_string(),
                is_directory: metadata.is_dir(),
            })
        })
        .collect())
}

pub fn terminal_path_entries_to_suggestions(
    mut entries: Vec<TerminalPathEntry>,
    parts: &TerminalPathParts,
    parsed: &TerminalShellParseResult,
    active_arg_type: TerminalFigArgType,
) -> Vec<TerminalCommandSuggestion> {
    let wanted_directory = active_arg_type == TerminalFigArgType::Directory;
    let wanted_file = active_arg_type == TerminalFigArgType::File;
    let quoted = parsed.current_token.quote.is_some();
    entries.retain(|entry| {
        entry
            .name
            .to_lowercase()
            .starts_with(&parts.query.to_lowercase())
            && (!wanted_directory || entry.is_directory)
            && (!wanted_file || !entry.is_directory)
    });
    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    entries
        .into_iter()
        .take(16)
        .map(|entry| {
            let suffix = if entry.is_directory { "/" } else { "" };
            let insert_text = format!(
                "{}{}{}",
                parts.display_prefix,
                escape_terminal_path_for_shell(&entry.name, quoted),
                suffix
            );
            TerminalCommandSuggestion {
                kind: if entry.is_directory {
                    TerminalCommandSuggestionKind::Directory
                } else {
                    TerminalCommandSuggestionKind::File
                },
                label: format!("{}{}", entry.name, suffix),
                insert_text,
                description: Some(entry.path),
                executable: false,
                replacement: parsed.current_token.start..parsed.current_token.end,
                group_label_key: "terminal.command_bar.group_path",
                source_label_key: "terminal.command_bar.source_path",
                score: if entry.is_directory {
                    560.0 + entry.name.len() as f64
                } else {
                    540.0 + entry.name.len() as f64
                },
                risk: None,
                inline_safe: true,
            }
        })
        .collect()
}

fn terminal_path_completion_cache() -> &'static std::sync::Mutex<TerminalPathCompletionCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<TerminalPathCompletionCache>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(TerminalPathCompletionCache::default()))
}

pub fn terminal_path_cache_key(
    scope: &str,
    target_id: Option<&str>,
    cwd: Option<&str>,
    directory: &str,
) -> String {
    format!(
        "{}::{}::{}::{}",
        scope,
        target_id.unwrap_or_default(),
        cwd.unwrap_or_default(),
        directory
    )
}

pub fn get_cached_terminal_path_entries(cache_key: &str) -> Option<Vec<TerminalPathEntry>> {
    const CACHE_TTL_MS: u128 = 12_000;
    let mut cache = terminal_path_completion_cache().lock().ok()?;
    let entry = cache.entries.get(cache_key)?;
    if entry.created_at.elapsed().as_millis() > CACHE_TTL_MS {
        cache.entries.remove(cache_key);
        return None;
    }
    Some(entry.entries.clone())
}

pub fn put_cached_terminal_path_entries(cache_key: String, entries: Vec<TerminalPathEntry>) {
    if let Ok(mut cache) = terminal_path_completion_cache().lock() {
        cache.entries.insert(
            cache_key,
            TerminalPathCacheEntry {
                created_at: std::time::Instant::now(),
                entries,
            },
        );
    }
}

pub fn mark_terminal_path_request_pending(cache_key: String) -> bool {
    terminal_path_completion_cache()
        .lock()
        .map(|mut cache| cache.pending.insert(cache_key))
        .unwrap_or(false)
}

pub fn clear_terminal_path_request_pending(cache_key: &str) {
    if let Ok(mut cache) = terminal_path_completion_cache().lock() {
        cache.pending.remove(cache_key);
    }
}
