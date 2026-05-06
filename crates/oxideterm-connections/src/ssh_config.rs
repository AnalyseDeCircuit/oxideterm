use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SshConfigHost {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub certificate_file: Option<String>,
    pub already_imported: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SshBatchImportResult {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<SshConfigImportError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshConfigImportError {
    pub alias: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
struct SshHostBlock {
    aliases: Vec<String>,
    options: SshHostOptions,
}

#[derive(Clone, Debug, Default)]
struct SshHostOptions {
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    certificate_file: Option<String>,
}

pub fn default_ssh_config_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("config")
}

pub fn list_ssh_config_hosts(existing_names: &HashSet<String>) -> Result<Vec<SshConfigHost>> {
    let path = default_ssh_config_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let blocks = parse_ssh_config_file(&path, &mut HashSet::new())?;
    let mut hosts = Vec::new();
    let mut seen_aliases = HashSet::new();
    for block in blocks {
        for alias in block.aliases {
            if !seen_aliases.insert(alias.clone()) {
                continue;
            }
            if alias_contains_pattern(&alias) {
                continue;
            }
            hosts.push(SshConfigHost {
                already_imported: existing_names.contains(&alias),
                alias,
                hostname: block.options.hostname.clone(),
                user: block.options.user.clone(),
                port: block.options.port,
                identity_file: block.options.identity_file.clone(),
                certificate_file: block.options.certificate_file.clone(),
            });
        }
    }
    hosts.sort_by(|left, right| left.alias.to_lowercase().cmp(&right.alias.to_lowercase()));
    Ok(hosts)
}

pub fn resolve_ssh_config_alias(alias: &str) -> Result<Option<SshConfigHost>> {
    let path = default_ssh_config_path();
    if !path.exists() {
        return Ok(None);
    }
    let blocks = parse_ssh_config_file(&path, &mut HashSet::new())?;
    let mut resolved = SshHostOptions::default();
    let mut matched = false;
    for block in blocks {
        if block.aliases.iter().any(|candidate| candidate == alias) {
            matched = true;
            merge_options(&mut resolved, &block.options);
        }
    }
    Ok(matched.then(|| SshConfigHost {
        alias: alias.to_string(),
        hostname: resolved.hostname,
        user: resolved.user,
        port: resolved.port,
        identity_file: resolved.identity_file,
        certificate_file: resolved.certificate_file,
        already_imported: false,
    }))
}

fn parse_ssh_config_file(path: &Path, seen: &mut HashSet<PathBuf>) -> Result<Vec<SshHostBlock>> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(path.clone()) {
        return Ok(Vec::new());
    }
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut blocks = Vec::new();
    let mut current: Option<SshHostBlock> = None;
    let mut globals = SshHostOptions::default();

    for raw_line in source.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let words = split_ssh_words(line);
        let Some((keyword, values)) = words.split_first() else {
            continue;
        };
        let key = keyword.to_ascii_lowercase();
        if key == "include" {
            flush_block(&mut blocks, &mut current);
            for pattern in values {
                for include_path in expand_include_path(base_dir, pattern) {
                    blocks.extend(parse_ssh_config_file(&include_path, seen)?);
                }
            }
            continue;
        }
        if key == "host" {
            flush_block(&mut blocks, &mut current);
            let aliases = values
                .iter()
                .filter(|alias| !alias.starts_with('!'))
                .cloned()
                .collect::<Vec<_>>();
            current = Some(SshHostBlock {
                aliases,
                options: globals.clone(),
            });
            continue;
        }

        let target = current
            .as_mut()
            .map(|block| &mut block.options)
            .unwrap_or(&mut globals);
        apply_option(target, &key, values);
    }
    flush_block(&mut blocks, &mut current);
    Ok(blocks)
}

fn flush_block(blocks: &mut Vec<SshHostBlock>, current: &mut Option<SshHostBlock>) {
    if let Some(block) = current.take()
        && !block.aliases.is_empty()
    {
        blocks.push(block);
    }
}

fn apply_option(options: &mut SshHostOptions, key: &str, values: &[String]) {
    let Some(value) = values.first() else {
        return;
    };
    match key {
        "hostname" => options.hostname = Some(expand_home(value)),
        "user" => options.user = Some(value.clone()),
        "port" => options.port = value.parse::<u16>().ok(),
        "identityfile" => options.identity_file = Some(expand_home(value)),
        "certificatefile" => options.certificate_file = Some(expand_home(value)),
        _ => {}
    }
}

fn merge_options(base: &mut SshHostOptions, update: &SshHostOptions) {
    if update.hostname.is_some() {
        base.hostname = update.hostname.clone();
    }
    if update.user.is_some() {
        base.user = update.user.clone();
    }
    if update.port.is_some() {
        base.port = update.port;
    }
    if update.identity_file.is_some() {
        base.identity_file = update.identity_file.clone();
    }
    if update.certificate_file.is_some() {
        base.certificate_file = update.certificate_file.clone();
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..index],
            _ => {}
        }
    }
    line
}

fn split_ssh_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => in_quotes = !in_quotes,
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn expand_include_path(base_dir: &Path, pattern: &str) -> Vec<PathBuf> {
    let pattern = expand_home(pattern);
    let path = PathBuf::from(&pattern);
    let path = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Vec::new();
    };
    if !file_name.contains('*') {
        return path.exists().then_some(path).into_iter().collect();
    }
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let prefix = file_name.split('*').next().unwrap_or_default();
    let suffix = file_name.rsplit('*').next().unwrap_or_default();
    let mut paths = fs::read_dir(parent)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn expand_home(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest).display().to_string();
    }
    value.to_string()
}

fn alias_contains_pattern(alias: &str) -> bool {
    alias.contains('*') || alias.contains('?')
}

#[allow(dead_code)]
fn options_by_alias(blocks: &[SshHostBlock]) -> HashMap<String, SshHostOptions> {
    blocks
        .iter()
        .flat_map(|block| {
            block
                .aliases
                .iter()
                .cloned()
                .map(|alias| (alias, block.options.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{split_ssh_words, strip_comment};

    #[test]
    fn ssh_words_keep_quoted_values() {
        assert_eq!(
            split_ssh_words(strip_comment("HostName \"dev box\" # comment")),
            vec!["HostName", "dev box"]
        );
    }
}
