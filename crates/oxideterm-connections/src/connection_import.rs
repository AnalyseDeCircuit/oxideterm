use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    CONFIG_VERSION, ConnectionOptions, ConnectionStore, SavedAuth, SavedConnection, SavedProxyHop,
    SavedUpstreamProxyPolicy,
};

const DEFAULT_IMPORTED_GROUP: &str = "Imported";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionImportSource {
    #[serde(rename = "securecrt")]
    SecureCrt,
    Xshell,
    Termius,
}

impl ConnectionImportSource {
    pub fn tag(self) -> &'static str {
        match self {
            Self::SecureCrt => "securecrt",
            Self::Xshell => "xshell",
            Self::Termius => "termius",
        }
    }

    pub fn default_group(self) -> &'static str {
        match self {
            Self::SecureCrt => "Imported/SecureCRT",
            Self::Xshell => "Imported/Xshell",
            Self::Termius => "Imported/Termius",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionImportDuplicateStrategy {
    Skip,
    Rename,
}

impl ConnectionImportDuplicateStrategy {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Rename => "rename",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportedConnectionAuthType {
    Password,
    Key,
    Certificate,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedProxyHopDraft {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: ImportedConnectionAuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    #[serde(default)]
    pub agent_forwarding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedConnectionDraft {
    pub id: String,
    pub source: ConnectionImportSource,
    pub source_path: String,
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: ImportedConnectionAuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    #[serde(default)]
    pub proxy_chain: Vec<ImportedProxyHopDraft>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub unsupported_fields: Vec<String>,
    #[serde(default)]
    pub duplicate: bool,
    #[serde(default)]
    pub importable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportPreview {
    pub source: ConnectionImportSource,
    pub total: usize,
    pub importable: usize,
    pub duplicates: usize,
    pub warnings: usize,
    pub errors: Vec<ConnectionImportErrorInfo>,
    pub drafts: Vec<ImportedConnectionDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportErrorInfo {
    pub source_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportApplyRequest {
    pub source: ConnectionImportSource,
    pub paths: Vec<String>,
    pub selected_draft_ids: Vec<String>,
    pub duplicate_strategy: ConnectionImportDuplicateStrategy,
    pub target_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionImportApplyResult {
    pub imported: usize,
    pub skipped: usize,
    pub renamed: usize,
    pub errors: Vec<ConnectionImportErrorInfo>,
}

#[derive(Debug, Error)]
pub enum ConnectionImportError {
    #[error("No import paths were provided")]
    EmptyPaths,
    #[error("Unsupported or unreadable import path: {0}")]
    InvalidPath(String),
    #[error("Failed to read {path}: {message}")]
    Read { path: String, message: String },
    #[error("Failed to parse {path}: {message}")]
    Parse { path: String, message: String },
    #[error(transparent)]
    Store(#[from] anyhow::Error),
}

pub fn preview_connection_import(
    source: ConnectionImportSource,
    paths: &[String],
    existing_names: &HashSet<String>,
) -> Result<ConnectionImportPreview, ConnectionImportError> {
    if paths.is_empty() {
        return Err(ConnectionImportError::EmptyPaths);
    }

    let mut drafts = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        match parse_import_path(source, Path::new(path)) {
            Ok(mut parsed) => drafts.append(&mut parsed),
            Err(error) => errors.push(ConnectionImportErrorInfo {
                source_path: path.clone(),
                message: error.to_string(),
            }),
        }
    }

    for draft in &mut drafts {
        draft.duplicate = existing_names.contains(&draft.name);
        draft.importable = !draft.host.trim().is_empty() && draft.port > 0;
    }

    Ok(ConnectionImportPreview {
        source,
        total: drafts.len(),
        importable: drafts.iter().filter(|draft| draft.importable).count(),
        duplicates: drafts.iter().filter(|draft| draft.duplicate).count(),
        warnings: drafts.iter().map(|draft| draft.warnings.len()).sum(),
        errors,
        drafts,
    })
}

pub fn apply_connection_import(
    store: &mut ConnectionStore,
    request: ConnectionImportApplyRequest,
) -> Result<ConnectionImportApplyResult, ConnectionImportError> {
    let mut existing_names = store
        .connections()
        .iter()
        .map(|connection| connection.name.clone())
        .collect::<HashSet<_>>();
    let selected_ids = request
        .selected_draft_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let preview = preview_connection_import(request.source, &request.paths, &existing_names)?;
    let mut skipped = 0usize;
    let mut renamed = 0usize;
    let mut errors = preview.errors;
    let mut connections = Vec::new();

    for draft in preview.drafts {
        if !selected_ids.contains(&draft.id) {
            continue;
        }
        if !draft.importable {
            errors.push(ConnectionImportErrorInfo {
                source_path: draft.source_path.clone(),
                message: "Connection draft is not importable".to_string(),
            });
            continue;
        }

        let mut name = draft.name.clone();
        if existing_names.contains(&name) {
            match request.duplicate_strategy {
                ConnectionImportDuplicateStrategy::Skip => {
                    skipped += 1;
                    continue;
                }
                ConnectionImportDuplicateStrategy::Rename => {
                    name = unique_import_name(&name, &existing_names);
                    renamed += 1;
                }
            }
        }

        let group = normalized_import_group(
            request.target_group.as_ref(),
            draft.group.as_ref(),
            draft.source,
        );
        existing_names.insert(name.clone());
        connections.push(imported_draft_to_saved_connection(&draft, name, group));
    }

    let imported = connections.len();
    if imported > 0 {
        store.upsert_imported_connections_transaction(connections)?;
    }

    Ok(ConnectionImportApplyResult {
        imported,
        skipped,
        renamed,
        errors,
    })
}

fn parse_import_path(
    source: ConnectionImportSource,
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    match source {
        ConnectionImportSource::SecureCrt => parse_securecrt_path(path),
        ConnectionImportSource::Xshell => parse_xshell_path(path),
        ConnectionImportSource::Termius => parse_termius_path(path),
    }
}

fn parse_securecrt_path(
    path: &Path,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return parse_directory(path, |file, root| parse_securecrt_file(file, Some(root)));
    }
    parse_securecrt_file(path, None).map(|draft| vec![draft])
}

fn parse_xshell_path(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return parse_directory(path, |file, root| parse_xshell_file(file, Some(root)));
    }
    parse_xshell_file(path, None).map(|draft| vec![draft])
}

fn parse_termius_path(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    if path.is_dir() {
        return Err(ConnectionImportError::InvalidPath(
            path.display().to_string(),
        ));
    }
    parse_termius_file(path)
}

fn parse_directory<F>(
    root: &Path,
    mut parse_file: F,
) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError>
where
    F: FnMut(&Path, &Path) -> Result<ImportedConnectionDraft, ConnectionImportError>,
{
    let mut drafts = Vec::new();
    visit_files(root, &mut |path| match parse_file(path, root) {
        Ok(draft) => {
            drafts.push(draft);
            Ok(())
        }
        Err(ConnectionImportError::Parse { .. }) => Ok(()),
        Err(error) => Err(error),
    })?;
    Ok(drafts)
}

fn visit_files<F>(root: &Path, visit: &mut F) -> Result<(), ConnectionImportError>
where
    F: FnMut(&Path) -> Result<(), ConnectionImportError>,
{
    for entry in fs::read_dir(root).map_err(|error| ConnectionImportError::Read {
        path: root.display().to_string(),
        message: error.to_string(),
    })? {
        let entry = entry.map_err(|error| ConnectionImportError::Read {
            path: root.display().to_string(),
            message: error.to_string(),
        })?;
        let path = entry.path();
        if path.is_dir() {
            visit_files(&path, visit)?;
        } else if path.is_file() {
            visit(&path)?;
        }
    }
    Ok(())
}

fn read_text_file(path: &Path) -> Result<String, ConnectionImportError> {
    fs::read_to_string(path).map_err(|error| ConnectionImportError::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

fn parse_securecrt_file(
    path: &Path,
    root: Option<&Path>,
) -> Result<ImportedConnectionDraft, ConnectionImportError> {
    let content = read_text_file(path)?;
    let mut fields = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut unsupported_fields = Vec::new();

    for line in content.lines() {
        let Some((key, value)) = parse_securecrt_setting(line) else {
            continue;
        };
        let normalized = normalize_key(&key);
        if looks_like_secret_key(&normalized) {
            warnings.push("Password was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        if looks_like_proxy_key(&normalized) {
            warnings.push("Proxy/jump setting was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        fields.insert(normalized, value);
    }

    draft_from_fields(
        ConnectionImportSource::SecureCrt,
        path,
        root,
        fields,
        warnings,
        unsupported_fields,
    )
}

fn parse_xshell_file(
    path: &Path,
    root: Option<&Path>,
) -> Result<ImportedConnectionDraft, ConnectionImportError> {
    let content = read_text_file(path)?;
    let mut fields = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut unsupported_fields = Vec::new();

    for line in content.lines() {
        let Some((key, value)) = parse_plain_setting(line) else {
            continue;
        };
        let normalized = normalize_key(&key);
        if looks_like_secret_key(&normalized) {
            warnings.push("Password was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        if looks_like_proxy_key(&normalized) {
            warnings.push("Proxy/jump setting was not imported".to_string());
            unsupported_fields.push(key);
            continue;
        }
        fields.insert(normalized, value);
    }

    draft_from_fields(
        ConnectionImportSource::Xshell,
        path,
        root,
        fields,
        warnings,
        unsupported_fields,
    )
}

fn parse_termius_file(path: &Path) -> Result<Vec<ImportedConnectionDraft>, ConnectionImportError> {
    let content = read_text_file(path)?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|error| ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;

    let mut drafts = Vec::new();
    collect_termius_drafts(path, &value, None, &mut drafts);
    if drafts.is_empty() {
        return Err(ConnectionImportError::Parse {
            path: path.display().to_string(),
            message: "No hosts found in Termius export".to_string(),
        });
    }
    Ok(drafts)
}

fn draft_from_fields(
    source: ConnectionImportSource,
    path: &Path,
    root: Option<&Path>,
    fields: BTreeMap<String, String>,
    mut warnings: Vec<String>,
    unsupported_fields: Vec<String>,
) -> Result<ImportedConnectionDraft, ConnectionImportError> {
    let host =
        pick_field(&fields, &["hostname", "host", "address", "ssh2hostname"]).ok_or_else(|| {
            ConnectionImportError::Parse {
                path: path.display().to_string(),
                message: "Missing host".to_string(),
            }
        })?;
    let username = pick_field(
        &fields,
        &["username", "user", "loginname", "account", "userid"],
    )
    .unwrap_or_else(whoami::username);
    let raw_port = pick_field(&fields, &["port", "sshport"]);
    let port = raw_port
        .as_deref()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(|| {
            if raw_port.is_some() {
                warnings.push("Invalid port; defaulted to 22".to_string());
            }
            22
        });
    let name =
        pick_field(&fields, &["name", "sessionname", "label"]).unwrap_or_else(|| file_stem(path));
    let key_path = pick_field(
        &fields,
        &[
            "identityfilename",
            "identityfilenamev2",
            "privatekey",
            "privatekeypath",
            "keyfile",
            "keypath",
            "publickeyfile",
        ],
    );
    let cert_path = pick_field(&fields, &["certificatefile", "certificatepath", "certpath"]);
    let auth_type = if cert_path.is_some() {
        ImportedConnectionAuthType::Certificate
    } else if key_path.is_some() {
        ImportedConnectionAuthType::Key
    } else {
        ImportedConnectionAuthType::Password
    };
    let mut draft = ImportedConnectionDraft {
        id: String::new(),
        source,
        source_path: path.display().to_string(),
        name,
        group: group_from_path(path, root).or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string())),
        host,
        port,
        username,
        auth_type,
        key_path,
        cert_path,
        proxy_chain: Vec::new(),
        tags: vec![source.tag().to_string()],
        warnings: dedupe(warnings),
        unsupported_fields: dedupe(unsupported_fields),
        duplicate: false,
        importable: true,
    };
    draft.id = draft_id(&draft);
    Ok(draft)
}

fn collect_termius_drafts(
    path: &Path,
    value: &serde_json::Value,
    group: Option<String>,
    drafts: &mut Vec<ImportedConnectionDraft>,
) {
    match value {
        serde_json::Value::Object(map) => {
            let next_group = map
                .get("group")
                .or_else(|| map.get("folder"))
                .or_else(|| map.get("folderName"))
                .and_then(value_as_string)
                .or(group);

            if let Some(host) = pick_json_string(map, &["hostname", "host", "address"]) {
                let username = pick_json_string(map, &["username", "user", "login"])
                    .unwrap_or_else(whoami::username);
                let port = pick_json_u16(map, &["port"]).unwrap_or(22);
                let name = pick_json_string(map, &["label", "name", "title"])
                    .unwrap_or_else(|| host.clone());
                let key_path =
                    pick_json_string(map, &["identityFile", "keyPath", "privateKeyPath"]);
                let cert_path = pick_json_string(map, &["certificateFile", "certPath"]);
                let mut warnings = Vec::new();
                let mut unsupported_fields = Vec::new();
                collect_secret_json_keys(map, &mut unsupported_fields);
                if !unsupported_fields.is_empty() {
                    warnings.push("Password was not imported".to_string());
                }
                let auth_type = if cert_path.is_some() {
                    ImportedConnectionAuthType::Certificate
                } else if key_path.is_some() {
                    ImportedConnectionAuthType::Key
                } else {
                    ImportedConnectionAuthType::Password
                };
                let mut draft = ImportedConnectionDraft {
                    id: String::new(),
                    source: ConnectionImportSource::Termius,
                    source_path: path.display().to_string(),
                    name,
                    group: next_group
                        .clone()
                        .or_else(|| Some(DEFAULT_IMPORTED_GROUP.to_string())),
                    host,
                    port,
                    username,
                    auth_type,
                    key_path,
                    cert_path,
                    proxy_chain: Vec::new(),
                    tags: vec![ConnectionImportSource::Termius.tag().to_string()],
                    warnings: dedupe(warnings),
                    unsupported_fields: dedupe(unsupported_fields),
                    duplicate: false,
                    importable: true,
                };
                draft.id = draft_id(&draft);
                drafts.push(draft);
                return;
            }

            for child in map.values() {
                collect_termius_drafts(path, child, next_group.clone(), drafts);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_termius_drafts(path, item, group.clone(), drafts);
            }
        }
        _ => {}
    }
}

fn imported_auth_to_saved(
    auth_type: ImportedConnectionAuthType,
    key_path: Option<&String>,
    cert_path: Option<&String>,
) -> SavedAuth {
    match auth_type {
        ImportedConnectionAuthType::Certificate => match (key_path, cert_path) {
            (Some(key_path), Some(cert_path)) => SavedAuth::Certificate {
                key_path: key_path.clone(),
                cert_path: cert_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            (Some(key_path), None) => SavedAuth::Key {
                key_path: key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            _ => SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            },
        },
        ImportedConnectionAuthType::Key => match key_path {
            Some(key_path) => SavedAuth::Key {
                key_path: key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            None => SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            },
        },
        ImportedConnectionAuthType::Agent => SavedAuth::Agent,
        ImportedConnectionAuthType::Password => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
        },
    }
}

fn imported_proxy_hop_to_saved(hop: &ImportedProxyHopDraft) -> SavedProxyHop {
    SavedProxyHop {
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth: imported_auth_to_saved(hop.auth_type, hop.key_path.as_ref(), hop.cert_path.as_ref()),
        agent_forwarding: hop.agent_forwarding,
    }
}

fn imported_draft_to_saved_connection(
    draft: &ImportedConnectionDraft,
    name: String,
    group: Option<String>,
) -> SavedConnection {
    // Imported third-party sessions intentionally contain no secret material.
    // The normal connection prompt remains the password/passphrase boundary.
    SavedConnection {
        id: Uuid::new_v4().to_string(),
        version: CONFIG_VERSION,
        name,
        group,
        host: draft.host.clone(),
        port: draft.port,
        username: draft.username.clone(),
        auth: imported_auth_to_saved(
            draft.auth_type,
            draft.key_path.as_ref(),
            draft.cert_path.as_ref(),
        ),
        proxy_chain: draft
            .proxy_chain
            .iter()
            .map(imported_proxy_hop_to_saved)
            .collect(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        options: ConnectionOptions::default(),
        created_at: Utc::now(),
        last_used_at: None,
        updated_at: Some(Utc::now()),
        color: None,
        tags: draft.tags.clone(),
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    }
}

fn normalized_import_group(
    request_group: Option<&String>,
    draft_group: Option<&String>,
    source: ConnectionImportSource,
) -> Option<String> {
    request_group
        .and_then(|group| {
            let trimmed = group.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .or_else(|| draft_group.cloned())
        .or_else(|| Some(source.default_group().to_string()))
}

fn unique_import_name(base_name: &str, existing_names: &HashSet<String>) -> String {
    if !existing_names.contains(base_name) {
        return base_name.to_string();
    }
    let mut index = 2usize;
    loop {
        let candidate = format!("{} ({})", base_name, index);
        if !existing_names.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn parse_securecrt_setting(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }
    let (_, rest) = trimmed.split_once(':')?;
    let (key_part, value_part) = rest.split_once('=')?;
    Some((unquote(key_part.trim()), unquote(value_part.trim())))
}

fn parse_plain_setting(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with(';')
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    Some((key.trim().to_string(), unquote(value.trim())))
}

fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn looks_like_secret_key(key: &str) -> bool {
    key.contains("password")
        || key.contains("passphrase")
        || key.contains("credential")
        || key.contains("vault")
        || key.contains("ciphertext")
        || key.contains("secret")
}

fn looks_like_proxy_key(key: &str) -> bool {
    key.contains("proxy")
        || key.contains("firewall")
        || key.contains("jumphost")
        || key.contains("jumpserver")
        || key.contains("bastion")
        || key.contains("gateway")
        || key.contains("socks")
}

fn pick_field(fields: &BTreeMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| fields.get(*key).filter(|value| !value.trim().is_empty()))
        .cloned()
}

fn pick_json_string(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(value_as_string))
        .filter(|value| !value.trim().is_empty())
}

fn pick_json_u16(map: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u16> {
    keys.iter().find_map(|key| match map.get(*key)? {
        serde_json::Value::Number(number) => {
            number.as_u64().and_then(|value| u16::try_from(value).ok())
        }
        serde_json::Value::String(value) => value.parse::<u16>().ok(),
        _ => None,
    })
}

fn value_as_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn collect_secret_json_keys(
    map: &serde_json::Map<String, serde_json::Value>,
    unsupported_fields: &mut Vec<String>,
) {
    for key in map.keys() {
        if looks_like_secret_key(&normalize_key(key)) {
            unsupported_fields.push(key.clone());
        }
    }
}

fn group_from_path(path: &Path, root: Option<&Path>) -> Option<String> {
    let root = root?;
    let relative = path.strip_prefix(root).ok()?;
    let parent = relative.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(format!(
        "{}/{}",
        DEFAULT_IMPORTED_GROUP,
        parent
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    ))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported connection".to_string())
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(trimmed)
        .to_string()
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn draft_id(draft: &ImportedConnectionDraft) -> String {
    let mut hasher = Sha256::new();
    hasher.update(draft.source.tag().as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.source_path.as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.name.as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.host.as_bytes());
    hasher.update(b"\0");
    hasher.update(draft.username.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("connection_import")
            .join(name)
    }

    fn temp_import_file(extension: &str, content: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("oxideterm-import-{id}.{extension}"));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn previews_securecrt_session_without_importing_password() {
        let path = fixture_path("securecrt/basic.ini");
        let preview = preview_connection_import(
            ConnectionImportSource::SecureCrt,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(draft.host, "gpu.example.com");
        assert_eq!(draft.port, 2222);
        assert_eq!(draft.username, "alice");
        assert_eq!(draft.auth_type, ImportedConnectionAuthType::Key);
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Password was not imported")
        );
    }

    #[test]
    fn previews_xshell_session_without_importing_password() {
        let path = fixture_path("xshell/model.xsh");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(draft.name, "model");
        assert_eq!(draft.host, "10.0.0.8");
        assert_eq!(draft.username, "ubuntu");
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Password was not imported")
        );
    }

    #[test]
    fn previews_termius_export_hosts() {
        let path = fixture_path("termius/export.json");
        let preview = preview_connection_import(
            ConnectionImportSource::Termius,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(preview.total, 2);
        assert!(
            preview
                .drafts
                .iter()
                .any(|draft| draft.name == "Inference A")
        );
        assert!(
            preview
                .drafts
                .iter()
                .any(|draft| draft.host == "gpu-b.example.com")
        );
        assert_eq!(preview.warnings, 1);
    }

    #[test]
    fn applies_selected_imports_with_rename_strategy() {
        let store_path = std::env::temp_dir().join(format!(
            "oxideterm-connection-import-test-{}.json",
            Uuid::new_v4()
        ));
        let mut store = ConnectionStore::load(store_path).unwrap();
        let path = fixture_path("xshell/model.xsh");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();
        let draft_id = preview.drafts[0].id.clone();

        let result = apply_connection_import(
            &mut store,
            ConnectionImportApplyRequest {
                source: ConnectionImportSource::Xshell,
                paths: vec![path.display().to_string()],
                selected_draft_ids: vec![draft_id.clone()],
                duplicate_strategy: ConnectionImportDuplicateStrategy::Rename,
                target_group: Some("Imported/Xshell".to_string()),
            },
        )
        .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(store.connections()[0].name, "model");

        let result = apply_connection_import(
            &mut store,
            ConnectionImportApplyRequest {
                source: ConnectionImportSource::Xshell,
                paths: vec![path.display().to_string()],
                selected_draft_ids: vec![draft_id],
                duplicate_strategy: ConnectionImportDuplicateStrategy::Rename,
                target_group: Some("Imported/Xshell".to_string()),
            },
        )
        .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(result.renamed, 1);
        assert!(
            store
                .connections()
                .iter()
                .any(|conn| conn.name == "model (2)")
        );
        let _ = fs::remove_file(store.path());
    }

    #[test]
    fn records_missing_host_as_preview_error() {
        let path = temp_import_file("xsh", "UserName=alice\n");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert!(preview.drafts.is_empty());
        assert_eq!(preview.errors.len(), 1);
        assert!(preview.errors[0].message.contains("Missing host"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_port_warns_and_defaults_to_ssh_port() {
        let path = temp_import_file("xsh", "Host=gpu.invalid\nUserName=alice\nPort=abc\n");
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert_eq!(draft.port, 22);
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Invalid port; defaulted to 22")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsupported_proxy_fields_warn_instead_of_silent_import() {
        let path = temp_import_file(
            "xsh",
            "Host=gpu.invalid\nUserName=alice\nProxyServer=jump.example.com\n",
        );
        let preview = preview_connection_import(
            ConnectionImportSource::Xshell,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        let draft = &preview.drafts[0];
        assert!(draft.proxy_chain.is_empty());
        assert!(
            draft
                .warnings
                .iter()
                .any(|warning| warning == "Proxy/jump setting was not imported")
        );
        assert_eq!(draft.unsupported_fields, vec!["ProxyServer".to_string()]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn records_malformed_termius_json_as_preview_error() {
        let path = temp_import_file("json", "{");
        let preview = preview_connection_import(
            ConnectionImportSource::Termius,
            &[path.display().to_string()],
            &HashSet::new(),
        )
        .unwrap();

        assert!(preview.drafts.is_empty());
        assert_eq!(preview.errors.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn native_model_deserializes_tauri_preview_payload() {
        let json = fs::read_to_string(fixture_path("tauri_preview.json")).unwrap();
        let preview: ConnectionImportPreview = serde_json::from_str(&json).unwrap();

        assert_eq!(preview.source, ConnectionImportSource::SecureCrt);
        assert_eq!(preview.total, 1);
        assert_eq!(preview.importable, 1);
        let draft = &preview.drafts[0];
        assert_eq!(draft.source_path, "/Users/example/Sessions/GPU/basic.ini");
        assert_eq!(draft.auth_type, ImportedConnectionAuthType::Key);
        assert_eq!(draft.unsupported_fields, vec!["Password V2".to_string()]);
    }
}
