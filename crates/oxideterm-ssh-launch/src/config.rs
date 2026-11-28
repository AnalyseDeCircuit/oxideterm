//! Connection metadata from a single OpenSSH config. File traversal and executable directives stay with the native host.
use std::collections::{HashMap, HashSet};

pub const MAX_CONFIG_BYTES: usize = 4 * 1024 * 1024;
const MAX_PATTERNS: usize = 2000;

#[derive(Clone, PartialEq, Eq)]
pub struct ConfigEndpoint {
    pub alias: String,
    pub host: String,
    pub user: Option<String>,
    pub port: u16,
}

impl ConfigEndpoint {
    pub fn address(&self) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        match &self.user {
            Some(user) => format!("{user}@{host}:{}", self.port),
            None => format!("{host}:{}", self.port),
        }
    }
}

#[derive(Default)]
struct Options {
    host: Option<String>,
    user: Option<String>,
    port: Option<String>,
}

pub struct ConfigPreview {
    pub hosts: Vec<ConfigEndpoint>,
    pub skipped: usize,
}

/// Includes and Match predicates depend on the local machine. Never expand them or collect authentication data here.
pub fn preview_endpoints(source: &str) -> Result<ConfigPreview, ConfigLimit> {
    if source.len() > MAX_CONFIG_BYTES {
        return Err(ConfigLimit::Bytes);
    }
    let mut blocks = vec![(vec!["*".to_owned()], Options::default())];
    let mut active = Some(0);
    let mut aliases = Vec::new();
    let mut seen = HashSet::new();
    let mut literals: HashMap<String, Vec<usize>> = HashMap::new();
    let mut patterns = vec![0];
    let mut pattern_count = 0;
    for line in source.trim_start_matches('\u{feff}').lines() {
        let line = strip_comment(line).trim();
        let split = line
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(line.len());
        let key = &line[..split];
        let rest = line[split..].trim_start();
        let value = rest.strip_prefix('=').unwrap_or(rest).trim_start();
        if key.eq_ignore_ascii_case("match") {
            active = None;
            continue;
        }
        if key.eq_ignore_ascii_case("host") {
            let words = split_ssh_words(value);
            if words.is_empty() {
                active = None;
                continue;
            }
            pattern_count += words.len();
            if pattern_count > MAX_PATTERNS {
                return Err(ConfigLimit::Hosts);
            }
            let id = blocks.len();
            let glob = words
                .iter()
                .any(|s| s.starts_with('!') || s.contains(['*', '?']));
            if glob {
                patterns.push(id);
            }
            for alias in &words {
                if alias.is_empty() || alias.starts_with('!') || alias.contains(['*', '?']) {
                    continue;
                }
                let folded = alias.to_ascii_lowercase();
                if !glob {
                    literals.entry(folded.clone()).or_default().push(id);
                }
                if seen.insert(folded) {
                    aliases.push(alias.clone());
                }
            }
            active = Some(id);
            blocks.push((words, Options::default()));
        } else if let Some(id) = active {
            let opts = &mut blocks[id].1;
            let slot = if key.eq_ignore_ascii_case("hostname") {
                &mut opts.host
            } else if key.eq_ignore_ascii_case("user") {
                &mut opts.user
            } else if key.eq_ignore_ascii_case("port") {
                &mut opts.port
            } else {
                continue;
            };
            if slot.is_none() {
                *slot = split_ssh_words(value).into_iter().next();
            }
        }
    }
    let mut preview = ConfigPreview {
        hosts: vec![],
        skipped: 0,
    };
    let mut expanded_bytes = 0;
    for alias in aliases {
        let mut ids = literals
            .remove(&alias.to_ascii_lowercase())
            .unwrap_or_default();
        ids.extend(
            patterns
                .iter()
                .copied()
                .filter(|&i| host_patterns_match(&blocks[i].0, &alias)),
        );
        ids.sort_unstable();
        let opts = || ids.iter().map(|&i| &blocks[i].1);
        let hostname = opts().find_map(|o| o.host.as_deref()).unwrap_or(&alias);
        let mut host = String::new();
        let mut chars = hostname.chars();
        let mut valid = true;
        while let Some(c) = chars.next() {
            if host.len() > 255 {
                valid = false;
                break;
            }
            match c {
                '%' => match chars.next() {
                    Some('h' | 'n') => host.push_str(&alias),
                    Some('%') => host.push('%'),
                    _ => {
                        valid = false;
                        break;
                    }
                },
                _ => host.push(c),
            }
        }
        let port = opts()
            .find_map(|o| o.port.as_deref())
            .unwrap_or("22")
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0);
        if let Some(port) = port
            && valid
            && (1..=255).contains(&host.len())
            && !host.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            let user = opts().find_map(|o| o.user.as_deref());
            // A global User value may be inherited by every explicit alias.
            expanded_bytes += alias.len() + host.len() + user.map_or(0, str::len);
            if expanded_bytes > MAX_CONFIG_BYTES {
                return Err(ConfigLimit::Bytes);
            }
            preview.hosts.push(ConfigEndpoint {
                alias,
                host,
                user: user.map(str::to_owned),
                port,
            });
        } else {
            preview.skipped += 1;
        }
    }
    Ok(preview)
}

#[derive(Debug)]
pub enum ConfigLimit {
    Bytes,
    Hosts,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_file_preview_resolves_aliases_without_following_match_or_includes() {
        let preview = preview_endpoints(
            r#"
Include another-file
Host edge-* !edge-v6
    Port 2222
    HostName %h.example
Host edge-one
    User ops
    Port 23
Host EDGE-ONE
    User ignored
Match exec "ignored"
    HostName wrong
    User wrong
Host edge-v6
    HostName=2001:db8::7
Host invalid
    Port 0
Host *
    User viewer
"#,
        )
        .unwrap();
        assert_eq!(preview.skipped, 1);
        let hosts: Vec<_> = preview
            .hosts
            .iter()
            .map(|h| (h.alias.as_str(), h.address()))
            .collect();
        assert_eq!(
            hosts,
            [
                ("edge-one", "ops@edge-one.example:2222".to_owned()),
                ("edge-v6", "viewer@[2001:db8::7]:22".to_owned()),
            ]
        );
    }
}

pub fn host_patterns_match(patterns: &[String], alias: &str) -> bool {
    let mut positive_match = false;
    for pattern in patterns {
        let (negated, pattern) = pattern
            .strip_prefix('!')
            .map(|pattern| (true, pattern))
            .unwrap_or((false, pattern.as_str()));
        if wildcard_match(pattern, alias) {
            if negated {
                return false;
            }
            positive_match = true;
        }
    }
    positive_match
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let mut pattern_index = 0;
    let mut value_index = 0;
    let mut star_index = None;
    let mut star_value_index = 0;

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?'
                || pattern[pattern_index].eq_ignore_ascii_case(&value[value_index]))
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }
    pattern[pattern_index..].iter().all(|byte| *byte == b'*')
}

pub fn strip_comment(line: &str) -> &str {
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

pub fn split_ssh_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut word_started = false;
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                word_started = true;
                // An escaped final backslash must not consume a quoted word's
                // closing quote. Interior drive/UNC separators stay verbatim.
                let mut remaining = chars.clone();
                if in_quotes
                    && remaining.next() == Some('\\')
                    && remaining.next() == Some('"')
                    && remaining.next().is_none_or(char::is_whitespace)
                {
                    chars.next();
                    current.push('\\');
                    continue;
                }
                // OpenSSH escaping is meaningful only for token delimiters here.
                // Preserve Windows drive and UNC path separators verbatim.
                if chars
                    .peek()
                    .is_some_and(|next| next.is_whitespace() || matches!(*next, '"' | '#'))
                {
                    current.push(chars.next().expect("peeked SSH config character"));
                } else {
                    current.push('\\');
                }
            }
            '"' => {
                word_started = true;
                in_quotes = !in_quotes;
            }
            ch if ch.is_whitespace() && !in_quotes => {
                if word_started {
                    words.push(std::mem::take(&mut current));
                    word_started = false;
                }
            }
            _ => {
                word_started = true;
                current.push(ch);
            }
        }
    }
    if word_started {
        words.push(current);
    }
    words
}
