use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};

use crate::ssh_paths::{default_ssh_dir, expand_home_path};
use crate::{ConnectionStore, saved_connection_from_ssh_host};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SshConfigHost {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub certificate_file: Option<String>,
    pub proxy_chain: Vec<SshConfigProxyHop>,
    pub already_imported: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SshConfigProxyHop {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub certificate_file: Option<String>,
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
    patterns: Vec<String>,
    options: SshHostOptions,
}

#[derive(Clone, Debug, Default)]
struct SshHostOptions {
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    certificate_file: Option<String>,
    proxy_jump: Option<String>,
}

pub fn default_ssh_config_path() -> PathBuf {
    default_ssh_dir().join("config")
}

pub fn list_ssh_config_hosts(existing_names: &HashSet<String>) -> Result<Vec<SshConfigHost>> {
    let path = default_ssh_config_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let blocks = parse_ssh_config_file(&path)?;
    let existing_names = existing_names
        .iter()
        .map(|name| name.to_lowercase())
        .collect::<HashSet<_>>();
    let mut hosts = Vec::new();
    let mut seen_aliases = HashSet::new();
    for block in &blocks {
        for alias in &block.patterns {
            let alias = alias.trim_start_matches('!');
            if !seen_aliases.insert(alias.to_lowercase()) {
                continue;
            }
            if alias_contains_pattern(&alias) {
                continue;
            }
            let Some(mut host) = resolve_ssh_config_alias_from_blocks(alias, &blocks)? else {
                continue;
            };
            host.already_imported = existing_names.contains(&alias.to_lowercase());
            hosts.push(host);
        }
    }
    Ok(hosts)
}

pub fn resolve_ssh_config_alias(alias: &str) -> Result<Option<SshConfigHost>> {
    let path = default_ssh_config_path();
    if !path.exists() {
        return Ok(None);
    }
    let blocks = parse_ssh_config_file(&path)?;
    resolve_ssh_config_alias_from_blocks(alias, &blocks)
}

/// Resolves and imports one literal SSH config alias as one store transaction.
pub fn import_ssh_config_alias(store: &mut ConnectionStore, alias: &str) -> Result<bool> {
    if store
        .connections()
        .iter()
        .any(|connection| connection.name.eq_ignore_ascii_case(alias))
    {
        return Ok(false);
    }
    let Some(host) = resolve_ssh_config_alias(alias)? else {
        return Ok(false);
    };
    import_resolved_ssh_config_host(store, host)
}

fn import_resolved_ssh_config_host(
    store: &mut ConnectionStore,
    host: SshConfigHost,
) -> Result<bool> {
    if store
        .connections()
        .iter()
        .any(|connection| connection.name.eq_ignore_ascii_case(&host.alias))
    {
        return Ok(false);
    }
    let connection = saved_connection_from_ssh_host(host)?;
    store.import_ssh_connection(connection)?;
    Ok(true)
}

/// Returns whether text can represent one literal SSH config alias.
pub fn is_literal_ssh_config_alias_query(query: &str) -> bool {
    !query.is_empty()
        && !query
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '@' | ':'))
}

/// Resolves a case-insensitive query to the canonical alias stored in the SSH config.
pub fn canonical_ssh_config_alias<'a>(hosts: &'a [SshConfigHost], query: &str) -> Option<&'a str> {
    if !is_literal_ssh_config_alias_query(query) {
        return None;
    }
    hosts
        .iter()
        .find(|host| host.alias.eq_ignore_ascii_case(query))
        .map(|host| host.alias.as_str())
}

fn parse_ssh_config_file(path: &Path) -> Result<Vec<SshHostBlock>> {
    // Includes are parsed in place because an included Host block changes the
    // active context for the lines that follow it, just as textual inclusion does.
    let mut blocks = vec![SshHostBlock::default()];
    let mut active_files = HashSet::new();
    let mut current_block = 0;
    parse_ssh_config_file_into(path, &mut active_files, &mut blocks, &mut current_block)?;
    Ok(blocks)
}

fn parse_ssh_config_file_into(
    path: &Path,
    active_files: &mut HashSet<PathBuf>,
    blocks: &mut Vec<SshHostBlock>,
    current_block: &mut usize,
) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !active_files.insert(path.clone()) {
        bail!(
            "recursive SSH config Include detected at {}",
            path.display()
        );
    }
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    for raw_line in source.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let mut words = split_ssh_words(line);
        let Some(keyword) = words.first().cloned() else {
            continue;
        };
        // OpenSSH accepts both `Keyword value` and `Keyword=value` forms.
        if words.len() == 1
            && let Some((key, value)) = keyword.split_once('=')
        {
            words = vec![key.to_string(), value.to_string()];
        }
        let (keyword, values) = words.split_first().expect("words is known to be non-empty");
        let key = keyword.to_ascii_lowercase();
        if key == "include" {
            for pattern in values {
                for include_path in expand_include_path(base_dir, pattern) {
                    parse_ssh_config_file_into(&include_path, active_files, blocks, current_block)?;
                }
            }
            continue;
        }
        if key == "host" {
            blocks.push(SshHostBlock {
                patterns: values.to_vec(),
                options: SshHostOptions::default(),
            });
            *current_block = blocks.len() - 1;
            continue;
        }
        if key == "match" {
            // Match supports runtime predicates such as exec and canonical host.
            // Do not leak its conditional options into the preceding Host block.
            blocks.push(SshHostBlock {
                patterns: vec!["!__oxideterm_unsupported_match__".to_string()],
                options: SshHostOptions::default(),
            });
            *current_block = blocks.len() - 1;
            continue;
        }

        apply_option(&mut blocks[*current_block].options, &key, values);
    }
    active_files.remove(&path);
    Ok(())
}

fn apply_option(options: &mut SshHostOptions, key: &str, values: &[String]) {
    let Some(value) = values.first() else {
        return;
    };
    match key {
        // OpenSSH keeps the first obtained value for these scalar options.
        "hostname" if options.hostname.is_none() => options.hostname = Some(value.clone()),
        "user" if options.user.is_none() => options.user = Some(value.clone()),
        "port" if options.port.is_none() => options.port = value.parse::<u16>().ok(),
        "identityfile" if options.identity_file.is_none() => {
            options.identity_file = Some(value.clone())
        }
        "certificatefile" if options.certificate_file.is_none() => {
            options.certificate_file = Some(value.clone())
        }
        "proxyjump" if options.proxy_jump.is_none() => options.proxy_jump = Some(value.clone()),
        _ => {}
    }
}

fn merge_first_options(base: &mut SshHostOptions, update: &SshHostOptions) {
    base.hostname = base.hostname.clone().or_else(|| update.hostname.clone());
    base.user = base.user.clone().or_else(|| update.user.clone());
    base.port = base.port.or(update.port);
    base.identity_file = base
        .identity_file
        .clone()
        .or_else(|| update.identity_file.clone());
    base.certificate_file = base
        .certificate_file
        .clone()
        .or_else(|| update.certificate_file.clone());
    base.proxy_jump = base
        .proxy_jump
        .clone()
        .or_else(|| update.proxy_jump.clone());
}

fn resolve_ssh_config_alias_from_blocks(
    alias: &str,
    blocks: &[SshHostBlock],
) -> Result<Option<SshConfigHost>> {
    let literal_alias_exists = blocks.iter().any(|block| {
        block
            .patterns
            .iter()
            .any(|pattern| !pattern.starts_with('!') && pattern.eq_ignore_ascii_case(alias))
    });
    if !literal_alias_exists {
        return Ok(None);
    }

    resolve_ssh_config_host(alias, blocks).map(Some)
}

fn resolve_ssh_config_host(alias: &str, blocks: &[SshHostBlock]) -> Result<SshConfigHost> {
    let options = resolve_options(alias, blocks);
    let hostname = options
        .hostname
        .as_deref()
        .map(|value| expand_connection_tokens(value, alias, options.user.as_deref(), options.port));
    let identity_file = options.identity_file.as_deref().map(|value| {
        expand_home(&expand_connection_tokens(
            value,
            alias,
            options.user.as_deref(),
            options.port,
        ))
    });
    let certificate_file = options.certificate_file.as_deref().map(|value| {
        expand_home(&expand_connection_tokens(
            value,
            alias,
            options.user.as_deref(),
            options.port,
        ))
    });
    let mut proxy_chain = Vec::new();
    if let Some(proxy_jump) = options.proxy_jump.as_deref()
        && !proxy_jump.eq_ignore_ascii_case("none")
    {
        for jump in proxy_jump
            .split(',')
            .map(str::trim)
            .filter(|jump| !jump.is_empty())
        {
            let jump = expand_connection_tokens(jump, alias, options.user.as_deref(), options.port);
            let target = parse_proxy_jump_target(&jump)?;
            let jump_options = resolve_options(&target.host, blocks);
            let jump_hostname = jump_options
                .hostname
                .as_deref()
                .map(|value| {
                    expand_connection_tokens(
                        value,
                        &target.host,
                        jump_options.user.as_deref(),
                        jump_options.port,
                    )
                })
                .unwrap_or_else(|| target.host.clone());
            let jump_identity = jump_options.identity_file.as_deref().map(|value| {
                expand_home(&expand_connection_tokens(
                    value,
                    &target.host,
                    jump_options.user.as_deref(),
                    jump_options.port,
                ))
            });
            let jump_certificate = jump_options.certificate_file.as_deref().map(|value| {
                expand_home(&expand_connection_tokens(
                    value,
                    &target.host,
                    jump_options.user.as_deref(),
                    jump_options.port,
                ))
            });
            proxy_chain.push(SshConfigProxyHop {
                host: jump_hostname,
                user: target.user.or(jump_options.user),
                port: target.port.or(jump_options.port),
                identity_file: jump_identity,
                certificate_file: jump_certificate,
            });
        }
    }

    Ok(SshConfigHost {
        alias: alias.to_string(),
        hostname,
        user: options.user,
        port: options.port,
        identity_file,
        certificate_file,
        proxy_chain,
        already_imported: false,
    })
}

fn resolve_options(alias: &str, blocks: &[SshHostBlock]) -> SshHostOptions {
    let mut resolved = SshHostOptions::default();
    for block in blocks {
        if block.patterns.is_empty() || host_patterns_match(&block.patterns, alias) {
            merge_first_options(&mut resolved, &block.options);
        }
    }
    resolved
}

fn host_patterns_match(patterns: &[String], alias: &str) -> bool {
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
    let pattern = pattern.to_ascii_lowercase().into_bytes();
    let value = value.to_ascii_lowercase().into_bytes();
    let mut pattern_index = 0;
    let mut value_index = 0;
    let mut star_index = None;
    let mut star_value_index = 0;

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProxyJumpTarget {
    host: String,
    user: Option<String>,
    port: Option<u16>,
}

fn parse_proxy_jump_target(value: &str) -> Result<ProxyJumpTarget> {
    let (user, host_port) = value
        .rsplit_once('@')
        .map(|(user, host)| (Some(user.to_string()), host))
        .unwrap_or((None, value));
    if host_port.is_empty() {
        bail!("empty ProxyJump host");
    }

    let (host, port) = if let Some(bracketed) = host_port.strip_prefix('[') {
        let (host, suffix) = bracketed
            .split_once(']')
            .ok_or_else(|| anyhow!("invalid bracketed ProxyJump host: {value}"))?;
        let port = suffix
            .strip_prefix(':')
            .filter(|value| !value.is_empty())
            .map(str::parse::<u16>)
            .transpose()
            .with_context(|| format!("invalid ProxyJump port in {value}"))?;
        (host.to_string(), port)
    } else if host_port.matches(':').count() == 1 {
        let (host, port) = host_port.rsplit_once(':').unwrap_or((host_port, ""));
        let port = (!port.is_empty())
            .then(|| port.parse::<u16>())
            .transpose()
            .with_context(|| format!("invalid ProxyJump port in {value}"))?;
        (host.to_string(), port)
    } else {
        (host_port.to_string(), None)
    };
    if host.is_empty() {
        bail!("empty ProxyJump host");
    }
    Ok(ProxyJumpTarget { host, user, port })
}

fn expand_connection_tokens(
    value: &str,
    alias: &str,
    user: Option<&str>,
    port: Option<u16>,
) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            expanded.push(character);
            continue;
        }
        match characters.next() {
            Some('%') => expanded.push('%'),
            Some('h' | 'n') => expanded.push_str(alias),
            Some('r') => expanded.push_str(user.unwrap_or_default()),
            Some('p') => expanded.push_str(&port.unwrap_or(22).to_string()),
            Some(token) => {
                expanded.push('%');
                expanded.push(token);
            }
            None => expanded.push('%'),
        }
    }
    expanded
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
    expand_home_path(value)
}

fn alias_contains_pattern(alias: &str) -> bool {
    alias.contains('*') || alias.contains('?')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(patterns: &[&str], options: SshHostOptions) -> SshHostBlock {
        SshHostBlock {
            patterns: patterns
                .iter()
                .map(|pattern| (*pattern).to_string())
                .collect(),
            options,
        }
    }

    #[test]
    fn ssh_words_keep_quoted_values() {
        assert_eq!(
            split_ssh_words(strip_comment("HostName \"dev box\" # comment")),
            vec!["HostName", "dev box"]
        );
    }

    #[test]
    fn parser_accepts_equals_separated_options() {
        let directory = std::env::temp_dir().join(format!(
            "oxideterm-ssh-config-equals-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("config"),
            "Host=production\nHostName=prod.example.com\nPort=2200\n",
        )
        .unwrap();

        let blocks = parse_ssh_config_file(&directory.join("config")).unwrap();
        let host = resolve_ssh_config_alias_from_blocks("production", &blocks)
            .unwrap()
            .unwrap();

        assert_eq!(host.hostname.as_deref(), Some("prod.example.com"));
        assert_eq!(host.port, Some(2200));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn alias_query_returns_the_canonical_config_spelling() {
        let hosts = vec![SshConfigHost {
            alias: "Production-DB".to_string(),
            ..SshConfigHost::default()
        }];

        assert_eq!(
            canonical_ssh_config_alias(&hosts, "production-db"),
            Some("Production-DB")
        );
        assert_eq!(canonical_ssh_config_alias(&hosts, "user@host"), None);
        assert_eq!(canonical_ssh_config_alias(&hosts, "host:22"), None);
    }

    #[test]
    fn resolved_alias_import_skips_duplicates_and_writes_once() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-ssh-alias-import-{}-connections.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut store = ConnectionStore::load(&path).unwrap();
        let host = SshConfigHost {
            alias: "production".to_string(),
            hostname: Some("prod.example.com".to_string()),
            user: Some("operator".to_string()),
            ..SshConfigHost::default()
        };

        assert!(import_resolved_ssh_config_host(&mut store, host.clone()).unwrap());
        assert!(!import_resolved_ssh_config_host(&mut store, host).unwrap());
        assert_eq!(store.connections().len(), 1);
        assert_eq!(store.connections()[0].host, "prod.example.com");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn effective_options_use_first_matching_value_and_wildcard_defaults() {
        let blocks = vec![
            block(
                &["production-*"],
                SshHostOptions {
                    user: Some("deployer".to_string()),
                    port: Some(2200),
                    ..SshHostOptions::default()
                },
            ),
            block(
                &["*", "!production-admin"],
                SshHostOptions {
                    user: Some("fallback".to_string()),
                    identity_file: Some("~/.ssh/id_default".to_string()),
                    ..SshHostOptions::default()
                },
            ),
            block(
                &["production-db"],
                SshHostOptions {
                    user: Some("late-value".to_string()),
                    port: Some(22),
                    ..SshHostOptions::default()
                },
            ),
        ];

        let resolved = resolve_options("production-db", &blocks);

        assert_eq!(resolved.user.as_deref(), Some("deployer"));
        assert_eq!(resolved.port, Some(2200));
        assert_eq!(resolved.identity_file.as_deref(), Some("~/.ssh/id_default"));
        assert!(!host_patterns_match(
            &["*".to_string(), "!production-admin".to_string()],
            "production-admin"
        ));
    }

    #[test]
    fn proxy_jump_target_supports_user_port_and_ipv6() {
        assert_eq!(
            parse_proxy_jump_target("ops@jump.example.com:2200").unwrap(),
            ProxyJumpTarget {
                host: "jump.example.com".to_string(),
                user: Some("ops".to_string()),
                port: Some(2200),
            }
        );
        assert_eq!(
            parse_proxy_jump_target("[2001:db8::1]:2222").unwrap(),
            ProxyJumpTarget {
                host: "2001:db8::1".to_string(),
                user: None,
                port: Some(2222),
            }
        );
    }

    #[test]
    fn include_is_parsed_in_the_active_host_context() {
        let directory = std::env::temp_dir().join(format!(
            "oxideterm-ssh-config-include-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("included.conf"), "Port 2201\n").unwrap();
        fs::write(
            directory.join("config"),
            "Host production\n  User deployer\n  Include included.conf\n  HostName prod.example.com\n",
        )
        .unwrap();

        let blocks = parse_ssh_config_file(&directory.join("config")).unwrap();
        let host = resolve_ssh_config_alias_from_blocks("production", &blocks)
            .unwrap()
            .unwrap();

        assert_eq!(host.hostname.as_deref(), Some("prod.example.com"));
        assert_eq!(host.user.as_deref(), Some("deployer"));
        assert_eq!(host.port, Some(2201));
        let _ = fs::remove_dir_all(directory);
    }
}
