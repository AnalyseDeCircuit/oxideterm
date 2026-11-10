//! Batch creation of saved SSH connections from an OpenSSH-style template plus a
//! per-host data list.
//!
//! The template owns shared settings (user, port, identity, ProxyCommand, timeouts)
//! and may reference per-row values through `{host}`, `{user}`, `{port}`, `{name}`,
//! `{group}` and `{profile}`. Each data line owns one target and its display
//! metadata. Nothing here persists the template: the resolved per-row settings are
//! written into ordinary saved connections.

use std::{collections::HashSet, fmt};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AuthType, CONFIG_VERSION, ConnectionInfo, ConnectionOptions, ConnectionStore, SavedAuth,
    SavedConnection, SavedProxyCommand, SavedUpstreamProxyPolicy, SecretString,
    draft::saved_auth_from_ssh_paths,
    ssh_config::{parse_ssh_time_seconds, parse_yes_no, split_ssh_words, strip_comment},
    ssh_paths::expand_home_path,
    validate_group_name,
};

/// A paste must not turn into an unbounded store write.
pub const MAX_BATCH_ROWS: usize = 2_000;
const HOST_PLACEHOLDER: &str = "{host}";
const KNOWN_PLACEHOLDERS: [&str; 6] = ["host", "user", "port", "name", "group", "profile"];

#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchIssueKind {
    #[error("the template is empty")]
    EmptyTemplate,
    #[error("only one Host block is supported")]
    MultipleHostBlocks,
    #[error("HostName must be {HOST_PLACEHOLDER} or omitted")]
    HostNameRequiresPlaceholder,
    #[error("ProxyCommand needs a command")]
    ProxyCommandEmpty,
    #[error("invalid port")]
    PortInvalid,
    #[error("invalid timeout")]
    TimeoutInvalid,
    #[error("expected yes or no")]
    FlagInvalid,
    #[error("unsupported directive {keyword}")]
    DirectiveUnsupported { keyword: String },
    #[error("{keyword} is set more than once")]
    DirectiveDuplicate { keyword: String },
    #[error("unknown placeholder {{{placeholder}}}")]
    PlaceholderUnknown { placeholder: String },
    #[error("unterminated placeholder")]
    PlaceholderUnterminated,
    #[error("stray }} in a template value; braces are reserved for placeholders")]
    BraceUnmatched,
    #[error("host is missing")]
    HostEmpty,
    #[error("invalid host or address")]
    HostInvalid,
    #[error("unknown field {key}")]
    QueryKeyUnknown { key: String },
    #[error("{key} is set more than once")]
    QueryKeyDuplicate { key: String },
    #[error("{key} has no value")]
    QueryValueEmpty { key: String },
    #[error("a batch is limited to {limit} rows")]
    RowLimitExceeded { limit: usize },
    #[error("the template uses {{profile}} but this row has no profile")]
    ProfileMissing,
    #[error("username is missing")]
    UsernameMissing,
    #[error("invalid group name")]
    GroupInvalid,
    #[error("no authentication method")]
    AuthMissing,
    #[error("authentication is empty after the colon")]
    AuthEmpty,
}

impl BatchIssueKind {
    /// Stable catalog suffix under `sessionManager.batchAdd.errors`. The visible
    /// wording lives in the i18n locales; `{...}` slots are filled by the caller.
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::EmptyTemplate => "empty_template",
            Self::MultipleHostBlocks => "multiple_host_blocks",
            Self::HostNameRequiresPlaceholder => "host_name_requires_placeholder",
            Self::ProxyCommandEmpty => "proxy_command_empty",
            Self::PortInvalid => "port_invalid",
            Self::TimeoutInvalid => "timeout_invalid",
            Self::FlagInvalid => "flag_invalid",
            Self::DirectiveUnsupported { .. } => "directive_unsupported",
            Self::DirectiveDuplicate { .. } => "directive_duplicate",
            Self::PlaceholderUnknown { .. } => "placeholder_unknown",
            Self::PlaceholderUnterminated => "placeholder_unterminated",
            Self::BraceUnmatched => "brace_unmatched",
            Self::HostEmpty => "host_empty",
            Self::HostInvalid => "host_invalid",
            Self::QueryKeyUnknown { .. } => "query_key_unknown",
            Self::QueryKeyDuplicate { .. } => "query_key_duplicate",
            Self::QueryValueEmpty { .. } => "query_value_empty",
            Self::RowLimitExceeded { .. } => "row_limit_exceeded",
            Self::ProfileMissing => "profile_missing",
            Self::UsernameMissing => "username_missing",
            Self::GroupInvalid => "group_invalid",
            Self::AuthMissing => "auth_missing",
            Self::AuthEmpty => "auth_empty",
        }
    }
}

/// One parse or resolve problem, located in the text the user edited.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchIssue {
    /// One-based line inside the template or data text.
    pub line: usize,
    pub kind: BatchIssueKind,
}

impl BatchIssue {
    fn new(line: usize, kind: BatchIssueKind) -> Self {
        Self { line, kind }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BatchTemplate {
    pub username: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub certificate_file: Option<String>,
    pub proxy_command: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    /// `Some` only when the template states `ForwardAgent`; `no` must be able
    /// to disable forwarding that a stored connection enables.
    pub forward_agent: Option<bool>,
}

impl BatchTemplate {
    pub fn uses_profile(&self) -> bool {
        self.proxy_command
            .as_deref()
            .is_some_and(|command| command.contains("{profile}"))
    }

    /// Resolves the per-row placeholders a shared setting may reference.
    ///
    /// `include_identity` is false when the row carries its own credential: the
    /// template identity is then unused, so its placeholders must not reject a
    /// row that never needed them.
    fn resolve_for_row(
        &self,
        substitution: &BatchSubstitution<'_>,
        include_identity: bool,
    ) -> Result<ResolvedTemplate, BatchIssue> {
        Ok(ResolvedTemplate {
            identity_file: self
                .identity_file
                .as_deref()
                .filter(|_| include_identity)
                .map(|value| substitute_placeholders(value, substitution))
                .transpose()?,
            certificate_file: self
                .certificate_file
                .as_deref()
                .filter(|_| include_identity)
                .map(|value| substitute_placeholders(value, substitution))
                .transpose()?,
            proxy_command: self
                .proxy_command
                .as_deref()
                .map(|value| substitute_placeholders(value, substitution))
                .transpose()?,
        })
    }
}

/// Shared settings after one row's placeholders were substituted.
struct ResolvedTemplate {
    identity_file: Option<String>,
    certificate_file: Option<String>,
    proxy_command: Option<String>,
}

impl ResolvedTemplate {
    fn has_identity_path(&self) -> bool {
        self.identity_file.is_some() || self.certificate_file.is_some()
    }
}

/// One parsed data line before it is merged with the template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchDataRow {
    pub line: usize,
    pub host: String,
    pub username: Option<String>,
    /// `user:<auth>@host`: the most specific authentication for the row.
    pub auth: Option<BatchRowAuth>,
    pub port: Option<u16>,
    pub name: Option<String>,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub profile: Option<String>,
}

/// Authentication written inline in a data line. OpenSSH has no inline
/// credential syntax, so the slot is classified by shape: a value that looks
/// like a filesystem path is a private key, anything else is a password. The
/// `key=` / `pass=` prefixes override the classification when a password
/// happens to look like a path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchRowAuth {
    Password(SecretString),
    KeyPath(String),
}

impl BatchRowAuth {
    fn to_saved_auth(&self) -> SavedAuth {
        match self {
            Self::Password(value) => SavedAuth::Password {
                empty_password: value.is_empty(),
                keychain_id: None,
                plaintext_password: Some(value.clone()),
            },
            Self::KeyPath(path) => saved_auth_from_ssh_paths(Some(path.clone()), None),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BatchDataParse {
    pub rows: Vec<BatchDataRow>,
    pub issues: Vec<BatchIssue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchRowStatus {
    Add,
    /// The row carries changes for an existing connection with the same
    /// target, so applying it rewrites that connection.
    Update,
    /// The row repeats a stored connection exactly; nothing would change.
    Unchanged,
    Renamed,
    SkippedDuplicateTarget,
    SkippedExistingName,
}

impl BatchRowStatus {
    /// Stable catalog suffix under `sessionManager.batchAdd.status`.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Update => "update",
            Self::Unchanged => "unchanged",
            Self::Renamed => "renamed",
            Self::SkippedDuplicateTarget => "skipped_duplicate_target",
            Self::SkippedExistingName => "skipped_existing_name",
        }
    }
}

/// A row merged with the template and ready to persist.
#[derive(Clone)]
pub struct ResolvedBatchRow {
    pub line: usize,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub auth: SavedAuth,
    pub proxy_command: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub forward_agent: Option<bool>,
    /// Set when the row updates a saved connection instead of adding one.
    replace_id: Option<String>,
}

/// The saved-connection identity a batch row can collide with. Only the fields
/// that decide whether a row means "the same connection" are carried, so a
/// preview never holds stored credentials.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchExistingConnection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group: Option<String>,
    pub tags: Vec<String>,
    /// Credentials are compared by shape only: the stored secret lives in the
    /// keychain and never reaches the preview.
    pub auth_type: AuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    pub managed_key_id: Option<String>,
    pub has_proxy_command: bool,
    pub connect_timeout_seconds: Option<u64>,
    pub agent_forwarding: bool,
}

impl BatchExistingConnection {
    pub fn from_saved(connection: &SavedConnection) -> Self {
        Self {
            id: connection.id.clone(),
            name: connection.name.clone(),
            host: connection.host.clone(),
            port: connection.port,
            username: connection.username.clone(),
            group: connection.group.clone(),
            tags: connection.tags.clone(),
            auth_type: connection.auth.auth_type(),
            key_path: connection.auth.key_path().map(str::to_owned),
            cert_path: connection.auth.cert_path().map(str::to_owned),
            managed_key_id: connection.auth.managed_key_id().map(str::to_owned),
            has_proxy_command: connection.proxy_command.is_some(),
            connect_timeout_seconds: connection.options.connect_timeout_seconds,
            agent_forwarding: connection.options.agent_forwarding,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BatchPreviewRow {
    pub line: usize,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub has_proxy_command: bool,
    /// Authentication the row will be saved with, so the preview shows which
    /// classification the inline credential received.
    pub auth_type: AuthType,
    pub status: BatchRowStatus,
    resolved: Option<ResolvedBatchRow>,
}

#[derive(Clone, Debug, Default)]
pub struct BatchPreview {
    pub rows: Vec<BatchPreviewRow>,
    pub issues: Vec<BatchIssue>,
}

impl BatchPreview {
    pub fn addable_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.resolved.is_some())
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.resolved.is_none())
            .count()
    }

    pub fn invalid_count(&self) -> usize {
        self.issues.len()
    }

    /// Returns the rows that `apply_batch` will persist, in preview order.
    pub fn addable_rows(&self) -> Vec<ResolvedBatchRow> {
        self.rows
            .iter()
            .filter_map(|row| row.resolved.clone())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchDuplicateNamePolicy {
    /// Leave an existing connection untouched.
    #[default]
    Skip,
    /// Add a numeric suffix until the display name is free.
    Rename,
}

impl BatchDuplicateNamePolicy {
    fn rename_on_conflict(self) -> bool {
        matches!(self, Self::Rename)
    }
}

#[derive(Clone, Debug, Default)]
pub struct BatchAddResult {
    /// Connections written by the batch, updates included.
    pub added: Vec<ConnectionInfo>,
    /// How many of them rewrote an existing connection.
    pub updated: usize,
}

pub fn parse_batch_template(text: &str) -> Result<BatchTemplate, BatchIssue> {
    // Editors on Windows still write a BOM; it is not part of the first line.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut template = BatchTemplate::default();
    let mut host_blocks = 0usize;
    let mut saw_directive = false;

    for (index, raw_line) in text.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        saw_directive = true;
        let line_number = index + 1;
        let mut words = split_ssh_words(line);
        // OpenSSH also accepts `Keyword=value`, and the value may contain
        // spaces (`ProxyCommand=ssh -W %h:%p jump`), so only the first token is
        // split and the remaining tokens stay part of the value.
        if let Some((keyword, value)) = words.first().and_then(|word| word.split_once('=')) {
            let keyword = keyword.to_owned();
            let value = value.to_owned();
            let rest = words.split_off(1);
            words = std::iter::once(keyword)
                .chain(std::iter::once(value))
                .chain(rest)
                .collect();
        }
        let Some(token) = words.first().cloned() else {
            continue;
        };
        let keyword = token.to_ascii_lowercase();
        let Some(value) = directive_value(line, &token, &words) else {
            return Err(BatchIssue::new(
                line_number,
                BatchIssueKind::DirectiveUnsupported { keyword },
            ));
        };
        // OpenSSH accepts a quoted scalar (`IdentityFile "~/.ssh/key"`); the
        // quotes are syntax, so a value wrapped in matching quotes loses them
        // before it becomes a path or a command.
        let value = unquote(value.trim());
        match keyword.as_str() {
            "host" => {
                host_blocks += 1;
                if host_blocks > 1 {
                    return Err(BatchIssue::new(
                        line_number,
                        BatchIssueKind::MultipleHostBlocks,
                    ));
                }
            }
            "hostname" => {
                if value.trim() != HOST_PLACEHOLDER {
                    return Err(BatchIssue::new(
                        line_number,
                        BatchIssueKind::HostNameRequiresPlaceholder,
                    ));
                }
            }
            "user" => {
                if template.username.is_some() {
                    return Err(BatchIssue::new(
                        line_number,
                        BatchIssueKind::DirectiveDuplicate { keyword },
                    ));
                }
                let value = non_empty(&value).map(str::to_owned);
                if let Some(value) = value.as_deref() {
                    validate_template_placeholders(value, line_number)?;
                }
                template.username = value;
            }
            "port" => {
                if template.port.is_some() {
                    return Err(BatchIssue::new(
                        line_number,
                        BatchIssueKind::DirectiveDuplicate { keyword },
                    ));
                }
                template.port = Some(
                    non_empty(&value)
                        .and_then(|value| value.parse::<u16>().ok())
                        .filter(|port| *port > 0)
                        .ok_or_else(|| BatchIssue::new(line_number, BatchIssueKind::PortInvalid))?,
                );
            }
            "identityfile" => {
                if let Some(value) = non_empty(&value) {
                    validate_template_placeholders(value, line_number)?;
                }
                let value = non_empty(&value).map(expand_home_path);
                set_unique_directive(
                    &mut template.identity_file,
                    value,
                    "IdentityFile",
                    line_number,
                )?;
            }
            "certificatefile" => {
                if let Some(value) = non_empty(&value) {
                    validate_template_placeholders(value, line_number)?;
                }
                let value = non_empty(&value).map(expand_home_path);
                set_unique_directive(
                    &mut template.certificate_file,
                    value,
                    "CertificateFile",
                    line_number,
                )?;
            }
            "proxycommand" => {
                if template.proxy_command.is_some() {
                    return Err(BatchIssue::new(
                        line_number,
                        BatchIssueKind::DirectiveDuplicate { keyword },
                    ));
                }
                let value = non_empty(&value).ok_or_else(|| {
                    BatchIssue::new(line_number, BatchIssueKind::ProxyCommandEmpty)
                })?;
                validate_template_placeholders(value, line_number)?;
                template.proxy_command = Some(value.to_owned());
            }
            "connecttimeout" => {
                if template.connect_timeout_seconds.is_some() {
                    return Err(BatchIssue::new(
                        line_number,
                        BatchIssueKind::DirectiveDuplicate { keyword },
                    ));
                }
                template.connect_timeout_seconds =
                    Some(parse_connect_timeout(&value).map_err(|_| {
                        BatchIssue::new(line_number, BatchIssueKind::TimeoutInvalid)
                    })?);
            }
            "forwardagent" => {
                if template.forward_agent.is_some() {
                    return Err(BatchIssue::new(
                        line_number,
                        BatchIssueKind::DirectiveDuplicate { keyword },
                    ));
                }
                let declared = parse_yes_no(&value)
                    .map_err(|_| BatchIssue::new(line_number, BatchIssueKind::FlagInvalid))?;
                // `ForwardAgent no` is a statement, not the absence of one: it
                // must be able to disable forwarding on an update.
                template.forward_agent = Some(declared);
            }
            _ => {
                return Err(BatchIssue::new(
                    line_number,
                    BatchIssueKind::DirectiveUnsupported { keyword },
                ));
            }
        }
    }

    if !saw_directive {
        return Err(BatchIssue::new(1, BatchIssueKind::EmptyTemplate));
    }
    Ok(template)
}

pub fn parse_batch_data(text: &str) -> Result<BatchDataParse, BatchIssue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parsed = BatchDataParse::default();
    for (index, raw_line) in text.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if parsed.rows.len() + parsed.issues.len() >= MAX_BATCH_ROWS {
            return Err(BatchIssue::new(
                index + 1,
                BatchIssueKind::RowLimitExceeded {
                    limit: MAX_BATCH_ROWS,
                },
            ));
        }
        match parse_data_line(line) {
            Ok(mut row) => {
                row.line = index + 1;
                parsed.rows.push(row);
            }
            Err(kind) => parsed.issues.push(BatchIssue::new(index + 1, kind)),
        }
    }
    Ok(parsed)
}

/// Merges every data row with the template and marks duplicates.
pub fn resolve_batch_rows(
    template: &BatchTemplate,
    data: &BatchDataParse,
    default_auth: Option<&SavedAuth>,
    existing: &[BatchExistingConnection],
    duplicate_name_policy: BatchDuplicateNamePolicy,
) -> BatchPreview {
    let mut preview = BatchPreview {
        // Line-level data problems stay visible next to the resolved rows.
        issues: data.issues.clone(),
        ..BatchPreview::default()
    };
    // Case-insensitive: callers may hand over display names in any casing.
    let mut occupied_names = HashSet::new();
    let mut existing_by_name = std::collections::HashMap::new();
    for connection in existing {
        occupied_names.insert(connection.name.to_lowercase());
        existing_by_name
            .entry(connection.name.to_lowercase())
            .or_insert(connection);
    }
    // Names this batch has already claimed; kept apart from the stored names
    // because only the stored ones can be updated in place.
    let mut batch_names = HashSet::new();
    // Two rows are one connection only when host, account and credential match:
    // the same host reached with two different keys is two connections.
    let mut seen_targets: HashSet<(String, u16, String, u64)> = HashSet::new();

    for row in &data.rows {
        let port = row.port.or(template.port).unwrap_or(22);
        let name = row
            .name
            .as_deref()
            .unwrap_or(row.host.as_str())
            .trim()
            .to_owned();
        let group = match row.group.as_deref() {
            Some(group) => match validate_group_name(group) {
                Ok(group) => Some(group),
                Err(_) => {
                    preview
                        .issues
                        .push(BatchIssue::new(row.line, BatchIssueKind::GroupInvalid));
                    continue;
                }
            },
            None => None,
        };
        // The template username may itself carry placeholders (`User {host}`),
        // so resolve it before the row settings that reference `{user}`.
        let template_username = match template.username.as_deref() {
            Some(raw) => {
                let pre_username = BatchSubstitution {
                    line: row.line,
                    host: row.host.as_str(),
                    username: "",
                    port,
                    name: &name,
                    group: group.as_deref().unwrap_or_default(),
                    profile: row.profile.as_deref(),
                };
                match substitute_placeholders(raw, &pre_username) {
                    Ok(value) => Some(value),
                    Err(issue) => {
                        preview.issues.push(issue);
                        continue;
                    }
                }
            }
            None => None,
        };
        let username = row
            .username
            .as_deref()
            .or(template_username.as_deref())
            .unwrap_or_default()
            .trim()
            .to_owned();
        if username.is_empty() {
            preview
                .issues
                .push(BatchIssue::new(row.line, BatchIssueKind::UsernameMissing));
            continue;
        }
        let substitution = BatchSubstitution {
            line: row.line,
            host: row.host.as_str(),
            username: &username,
            port,
            name: &name,
            group: group.as_deref().unwrap_or_default(),
            profile: row.profile.as_deref(),
        };
        let resolved_template = match template.resolve_for_row(&substitution, row.auth.is_none()) {
            Ok(resolved) => resolved,
            Err(issue) => {
                preview.issues.push(issue);
                continue;
            }
        };
        let auth = if let Some(row_auth) = row.auth.as_ref() {
            // The most specific source wins: the row's inline credential beats
            // the template IdentityFile, which beats the dialog-level default.
            row_auth.to_saved_auth()
        } else if resolved_template.has_identity_path() {
            saved_auth_from_ssh_paths(
                resolved_template.identity_file,
                resolved_template.certificate_file,
            )
        } else if let Some(auth) = default_auth {
            auth.clone()
        } else {
            preview
                .issues
                .push(BatchIssue::new(row.line, BatchIssueKind::AuthMissing));
            continue;
        };

        let tags = normalize_batch_tags(row.tags.clone());
        let mut status = BatchRowStatus::Add;
        let mut replace_id = None;
        let mut stored_name = name.clone();
        let stored_key = stored_name.to_lowercase();
        if !seen_targets.insert(batch_connection_key(&row.host, port, &username, &auth)) {
            status = BatchRowStatus::SkippedDuplicateTarget;
        } else if batch_names.contains(&stored_key) {
            // Another row of this batch claims the name.
            if duplicate_name_policy.rename_on_conflict() {
                stored_name = renamed_candidate(&stored_name, &mut occupied_names);
                status = BatchRowStatus::Renamed;
            } else {
                status = BatchRowStatus::SkippedExistingName;
            }
        } else if let Some(saved) = existing_by_name.get(&stored_key) {
            let same_target = saved.port == port
                && saved.username == username
                && saved.host.eq_ignore_ascii_case(&row.host);
            if same_target
                && !batch_row_repeats_saved(
                    saved,
                    group.as_deref(),
                    &tags,
                    &auth,
                    resolved_template.proxy_command.is_some(),
                    template.connect_timeout_seconds,
                    template.forward_agent,
                )
            {
                // Only a difference is worth writing: the row rewrites the
                // stored connection, so a newly added group or tag lands on it
                // instead of on a renamed copy.
                status = BatchRowStatus::Update;
                replace_id = Some(saved.id.clone());
            } else if same_target && !duplicate_name_policy.rename_on_conflict() {
                status = BatchRowStatus::Unchanged;
            } else if duplicate_name_policy.rename_on_conflict() {
                stored_name = renamed_candidate(&stored_name, &mut occupied_names);
                status = BatchRowStatus::Renamed;
            } else {
                status = BatchRowStatus::SkippedExistingName;
            }
        }
        if matches!(
            status,
            BatchRowStatus::Add | BatchRowStatus::Renamed | BatchRowStatus::Update
        ) {
            // A skipped row stores nothing, so it must not reserve its name and
            // push a later, valid row out of it.
            batch_names.insert(stored_name.to_lowercase());
            occupied_names.insert(stored_name.to_lowercase());
        }

        let has_proxy_command = resolved_template.proxy_command.is_some();
        let auth_type = auth.auth_type();
        let resolved = (status == BatchRowStatus::Add
            || status == BatchRowStatus::Renamed
            || status == BatchRowStatus::Update)
            .then(|| ResolvedBatchRow {
                line: row.line,
                name: stored_name.clone(),
                host: row.host.clone(),
                port,
                username: username.clone(),
                group: group.clone(),
                tags: tags.clone(),
                auth,
                proxy_command: resolved_template.proxy_command,
                connect_timeout_seconds: template.connect_timeout_seconds,
                forward_agent: template.forward_agent,
                replace_id,
            });

        preview.rows.push(BatchPreviewRow {
            line: row.line,
            name: stored_name,
            host: row.host.clone(),
            port,
            username,
            group,
            tags,
            has_proxy_command,
            auth_type,
            status,
            resolved,
        });
    }
    preview
}

/// Writes every resolved row in one store transaction, so a failed row leaves the
/// connection file and the protected store untouched.
pub fn apply_batch(
    store: &mut ConnectionStore,
    rows: Vec<ResolvedBatchRow>,
) -> anyhow::Result<BatchAddResult> {
    let now = Utc::now();
    let mut connections = Vec::with_capacity(rows.len());
    let mut updated = 0usize;
    for row in rows {
        let existing = row
            .replace_id
            .as_deref()
            .and_then(|id| store.get(id).cloned());
        match existing {
            Some(existing) => {
                updated += 1;
                connections.push(merge_batch_update(existing, row, now));
            }
            // The previewed connection was deleted before applying; recreating
            // it silently would surprise the user, so the batch stops.
            None if row.replace_id.is_some() => {
                return Err(anyhow::anyhow!(
                    "the connection to update no longer exists; reload the preview before applying"
                ));
            }
            None => connections.push(saved_connection_from_row(row, now)),
        }
    }
    if connections.is_empty() {
        return Ok(BatchAddResult::default());
    }
    Ok(BatchAddResult {
        added: store.upsert_imported_connections_transaction(connections)?,
        updated,
    })
}

/// Applies a batch row to a stored connection. Only the settings the batch
/// expresses are replaced, so a jump chain, proxy or advanced option the user
/// configured by hand survives a re-run.
fn merge_batch_update(
    existing: SavedConnection,
    row: ResolvedBatchRow,
    now: chrono::DateTime<Utc>,
) -> SavedConnection {
    let mut connection = existing;
    connection.version = CONFIG_VERSION;
    connection.name = row.name;
    // Only declared metadata is written; an omitted group or tag list leaves
    // what the user configured by hand alone.
    if row.group.is_some() {
        connection.group = row.group;
    }
    if !row.tags.is_empty() {
        connection.tags = row.tags;
    }
    connection.host = row.host;
    connection.port = row.port;
    connection.username = row.username;
    connection.auth = row.auth;
    if let Some(command) = row.proxy_command {
        connection.proxy_command = Some(SavedProxyCommand {
            keychain_id: None,
            plaintext_command: Some(SecretString::new(command)),
        });
    }
    if row.connect_timeout_seconds.is_some() {
        connection.options.connect_timeout_seconds = row.connect_timeout_seconds;
    }
    if let Some(forward) = row.forward_agent {
        connection.options.agent_forwarding = forward;
    }
    connection.updated_at = Some(now);
    connection
}

/// Whether applying the row would leave the stored connection exactly as it is.
fn batch_row_repeats_saved(
    saved: &BatchExistingConnection,
    group: Option<&str>,
    tags: &[String],
    auth: &SavedAuth,
    has_proxy_command: bool,
    connect_timeout_seconds: Option<u64>,
    forward_agent: Option<bool>,
) -> bool {
    let mut saved_tags = saved
        .tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .collect::<Vec<_>>();
    let mut row_tags = tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .collect::<Vec<_>>();
    saved_tags.sort_unstable();
    row_tags.sort_unstable();
    // A field the row does not mention keeps whatever the stored connection
    // has, so an omitted `group=`/`tags=` never clears it.
    let group_matches = match group {
        Some(group) => saved
            .group
            .as_deref()
            .is_some_and(|saved| saved.trim().eq_ignore_ascii_case(group.trim())),
        None => true,
    };
    group_matches
        && (row_tags.is_empty() || saved_tags == row_tags)
        && saved.auth_type == auth.auth_type()
        && saved.key_path.as_deref() == auth.key_path()
        && saved.cert_path.as_deref() == auth.cert_path()
        && saved.managed_key_id.as_deref() == auth.managed_key_id()
        // A template without a ProxyCommand leaves a stored one alone.
        && (!has_proxy_command || saved.has_proxy_command)
        && connect_timeout_seconds
            .map(|timeout| saved.connect_timeout_seconds == Some(timeout))
            .unwrap_or(true)
        && forward_agent
            .map(|forward| saved.agent_forwarding == forward)
            .unwrap_or(true)
}

fn batch_connection_key(
    host: &str,
    port: u16,
    username: &str,
    auth: &SavedAuth,
) -> (String, u16, String, u64) {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    auth_fingerprint(auth, &mut hasher);
    (
        host.to_lowercase(),
        port,
        username.to_owned(),
        hasher.finish(),
    )
}

/// Feeds everything that distinguishes two credentials into the hash. Secret
/// bytes are hashed, never kept as text, so the key is safe to hold in memory.
fn auth_fingerprint(auth: &SavedAuth, hasher: &mut std::collections::hash_map::DefaultHasher) {
    use std::hash::{Hash, Hasher};
    auth.auth_type().as_str().hash(hasher);
    match auth {
        SavedAuth::Password {
            empty_password,
            keychain_id,
            plaintext_password,
        } => {
            empty_password.hash(hasher);
            keychain_id.hash(hasher);
            plaintext_password
                .as_ref()
                .map(|secret| secret.expose_secret())
                .hash(hasher);
        }
        SavedAuth::Key { key_path, .. } => key_path.hash(hasher),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            ..
        } => {
            key_path.hash(hasher);
            cert_path.hash(hasher);
        }
        SavedAuth::ManagedKey { key_id, .. } => key_id.hash(hasher),
        SavedAuth::KeyboardInteractive | SavedAuth::Agent => {}
        SavedAuth::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback,
        } => {
            server_identity.hash(hasher);
            delegate_credentials.hash(hasher);
            auth_fingerprint(fallback, hasher);
        }
    }
    // `finish` is called by the caller once; keep the hasher moving here too.
    hasher.write_u8(0);
}

pub fn normalize_batch_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() || !seen.insert(tag.to_lowercase()) {
            continue;
        }
        normalized.push(tag.to_owned());
    }
    normalized
}

fn saved_connection_from_row(row: ResolvedBatchRow, now: chrono::DateTime<Utc>) -> SavedConnection {
    let mut options = ConnectionOptions::default();
    options.connect_timeout_seconds = row.connect_timeout_seconds;
    options.agent_forwarding = row.forward_agent.unwrap_or(false);
    SavedConnection {
        id: String::new(),
        version: CONFIG_VERSION,
        name: row.name,
        group: row.group,
        notes: None,
        host: row.host,
        port: row.port,
        username: row.username,
        auth: row.auth,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        proxy_command: row.proxy_command.map(|command| SavedProxyCommand {
            keychain_id: None,
            plaintext_command: Some(SecretString::new(command)),
        }),
        options,
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        icon_background_color: None,
        icon: None,
        tags: row.tags,
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    }
}

struct BatchSubstitution<'a> {
    /// Line of the data text the substituted settings belong to.
    line: usize,
    host: &'a str,
    username: &'a str,
    port: u16,
    name: &'a str,
    group: &'a str,
    profile: Option<&'a str>,
}
impl BatchSubstitution<'_> {
    fn value(&self, placeholder: &str) -> Option<String> {
        match placeholder {
            "host" => Some(self.host.to_owned()),
            "user" => Some(self.username.to_owned()),
            "port" => Some(self.port.to_string()),
            "name" => Some(self.name.to_owned()),
            "group" => Some(self.group.to_owned()),
            "profile" => self.profile.map(str::to_owned),
            _ => None,
        }
    }
}

fn substitute_placeholders(
    value: &str,
    substitution: &BatchSubstitution<'_>,
) -> Result<String, BatchIssue> {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        let Some(end_offset) = rest[start..].find('}') else {
            return Err(BatchIssue::new(
                substitution.line,
                BatchIssueKind::PlaceholderUnterminated,
            ));
        };
        let placeholder = &rest[start + 1..start + end_offset];
        let Some(replacement) = substitution.value(placeholder) else {
            return Err(BatchIssue::new(
                substitution.line,
                if placeholder == "profile" {
                    BatchIssueKind::ProfileMissing
                } else {
                    BatchIssueKind::PlaceholderUnknown {
                        placeholder: placeholder.to_owned(),
                    }
                },
            ));
        };
        result.push_str(&replacement);
        rest = &rest[start + end_offset + 1..];
    }
    result.push_str(rest);
    // Row values are substituted into template text, so the brace policy has
    // to be checked on the completed value as well.
    if result.contains('}') {
        return Err(BatchIssue::new(
            substitution.line,
            BatchIssueKind::BraceUnmatched,
        ));
    }
    Ok(result)
}

/// Rejects placeholders that no template value can resolve, on the line that
/// carries the value.
fn validate_template_placeholders(value: &str, line: usize) -> Result<(), BatchIssue> {
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        let Some(end_offset) = rest[start..].find('}') else {
            return Err(BatchIssue::new(
                line,
                BatchIssueKind::PlaceholderUnterminated,
            ));
        };
        let placeholder = &rest[start + 1..start + end_offset];
        if !KNOWN_PLACEHOLDERS.contains(&placeholder) {
            return Err(BatchIssue::new(
                line,
                BatchIssueKind::PlaceholderUnknown {
                    placeholder: placeholder.to_owned(),
                },
            ));
        }
        rest = &rest[start + end_offset + 1..];
    }
    if rest.contains('}') {
        // Braces are only meaningful as `{placeholder}`; a lone one would be
        // copied into the saved connection and silently mislead.
        return Err(BatchIssue::new(line, BatchIssueKind::BraceUnmatched));
    }
    Ok(())
}

/// A saved connection carries one identity, so a second, different
/// `IdentityFile`/`CertificateFile` would be dropped silently.
fn set_unique_directive(
    slot: &mut Option<String>,
    value: Option<String>,
    keyword: &str,
    line: usize,
) -> Result<(), BatchIssue> {
    let Some(value) = value else {
        return Ok(());
    };
    if slot.as_deref().is_some_and(|existing| existing != value) {
        return Err(BatchIssue::new(
            line,
            BatchIssueKind::DirectiveDuplicate {
                keyword: keyword.to_owned(),
            },
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn directive_value(line: &str, token: &str, words: &[String]) -> Option<String> {
    if let Some(rest) = line.strip_prefix(token) {
        let rest = rest.trim_start();
        return Some(
            rest.strip_prefix('=')
                .map(|value| value.trim_start().to_owned())
                .unwrap_or_else(|| rest.to_owned()),
        );
    }
    // A quoted keyword is not a byte prefix of the line; rebuild from the tokens.
    words
        .get(1..)
        .filter(|values| !values.is_empty())
        .map(|values| values.join(" "))
}

fn parse_connect_timeout(value: &str) -> anyhow::Result<u64> {
    Ok(u64::from(parse_ssh_time_seconds(value)?))
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn renamed_candidate(name: &str, occupied: &mut HashSet<String>) -> String {
    for suffix in 2usize.. {
        let candidate = format!("{name}-{suffix}");
        if !occupied.contains(&candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("an unbounded suffix search always finds a free name")
}

fn parse_data_line(line: &str) -> Result<BatchDataRow, BatchIssueKind> {
    let (target, query) = match line.split_once('?') {
        Some((target, query)) => (target, Some(query)),
        None => (line, None),
    };
    let (username, auth, remainder) = match target.rsplit_once('@') {
        Some((userinfo, host)) if !userinfo.trim().is_empty() => {
            if userinfo.contains('@') {
                // `alice@corp@host` cannot be told apart from a different
                // account, so it is rejected instead of saved as a username.
                return Err(BatchIssueKind::HostInvalid);
            }
            match userinfo.trim().split_once(':') {
                Some((username, slot)) => (
                    Some(username.trim().to_owned()),
                    Some(parse_auth_slot(slot)?),
                    host,
                ),
                None => (Some(userinfo.trim().to_owned()), None, host),
            }
        }
        Some(_) => return Err(BatchIssueKind::HostInvalid),
        None => (None, None, target),
    };
    let (host, port) = parse_target_host_port(remainder.trim())?;
    let mut row = BatchDataRow {
        line: 0,
        host,
        username,
        auth,
        port,
        name: None,
        group: None,
        tags: Vec::new(),
        profile: None,
    };
    if let Some(query) = query {
        let mut seen_keys: Vec<String> = Vec::new();
        for pair in query.split('&').filter(|pair| !pair.trim().is_empty()) {
            let Some((key, value)) = pair.split_once('=') else {
                return Err(BatchIssueKind::QueryKeyUnknown {
                    key: pair.trim().to_owned(),
                });
            };
            let value = unquote(value.trim());
            let key = key.trim().to_ascii_lowercase();
            if seen_keys.contains(&key) {
                // Two values for one field would silently drop the first.
                return Err(BatchIssueKind::QueryKeyDuplicate { key });
            }
            seen_keys.push(key.clone());
            match key.as_str() {
                "name" => {
                    row.name = Some(
                        non_empty(&value)
                            .ok_or(BatchIssueKind::QueryValueEmpty {
                                key: "name".to_owned(),
                            })?
                            .to_owned(),
                    )
                }
                "group" => row.group = non_empty(&value).map(str::to_owned),
                "profile" => row.profile = non_empty(&value).map(str::to_owned),
                "tags" => {
                    if value.trim().is_empty() {
                        return Err(BatchIssueKind::QueryValueEmpty {
                            key: "tags".to_owned(),
                        });
                    }
                    row.tags = value.split(',').map(str::to_owned).collect();
                }
                other => {
                    return Err(BatchIssueKind::QueryKeyUnknown {
                        key: other.to_owned(),
                    });
                }
            }
        }
    }
    Ok(row)
}

/// Classifies the `user:<slot>@host` value.
///
/// OpenSSH itself has no inline credential syntax (`ssh://` URIs carry only a
/// user), and key authentication is always a file, so the slot is split by
/// shape: a path-looking value is a private key, everything else is a password.
/// A password that contains `/` or `\` therefore needs the explicit
/// `pass=` prefix, and `key=` forces a key path that does not look like one
/// (for example an ssh-agent-style short name).
fn parse_auth_slot(slot: &str) -> Result<BatchRowAuth, BatchIssueKind> {
    let slot = unquote(slot.trim());
    let slot = slot.trim();
    if slot.is_empty() {
        return Err(BatchIssueKind::AuthEmpty);
    }
    if let Some(key_path) = slot.strip_prefix("key=") {
        return key_path_from_slot(key_path);
    }
    if let Some(password) = slot.strip_prefix("pass=") {
        if password.is_empty() {
            return Err(BatchIssueKind::AuthEmpty);
        }
        return Ok(BatchRowAuth::Password(SecretString::new(
            password.to_owned().into_boxed_str(),
        )));
    }
    if looks_like_key_path(slot) {
        return key_path_from_slot(slot);
    }
    Ok(BatchRowAuth::Password(SecretString::new(
        slot.to_owned().into_boxed_str(),
    )))
}

fn password_from_slot(value: &str) -> Result<BatchRowAuth, BatchIssueKind> {
    let value = value.trim();
    if value.is_empty() {
        return Err(BatchIssueKind::AuthEmpty);
    }
    Ok(BatchRowAuth::Password(SecretString::new(value)))
}

fn key_path_from_slot(key_path: &str) -> Result<BatchRowAuth, BatchIssueKind> {
    let key_path = key_path.trim();
    if key_path.is_empty() {
        return Err(BatchIssueKind::AuthEmpty);
    }
    Ok(BatchRowAuth::KeyPath(expand_home_path(key_path)))
}

/// Path shape, not existence: `~`, absolute paths, explicit relative paths and
/// any value with a separator is a key; everything else is a password.
fn looks_like_key_path(value: &str) -> bool {
    value.starts_with('~')
        || value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.contains('/')
        || value.contains('\\')
}

fn parse_target_host_port(target: &str) -> Result<(String, Option<u16>), BatchIssueKind> {
    if target.is_empty() {
        return Err(BatchIssueKind::HostEmpty);
    }
    if let Some(rest) = target.strip_prefix('[') {
        let (address, suffix) = rest.split_once(']').ok_or(BatchIssueKind::HostInvalid)?;
        let port = match suffix {
            "" => None,
            remainder => Some(parse_port(
                remainder
                    .strip_prefix(':')
                    .ok_or(BatchIssueKind::HostInvalid)?,
            )?),
        };
        return Ok((require_valid_host(address)?, port));
    }
    // More than one bare colon is an IPv6 literal; OpenSSH has no port suffix
    // for one, so `host:22:23` must be rejected instead of saved as a host.
    if target.matches(':').count() > 1 {
        if parse_ipv6_literal(target).is_none() {
            return Err(BatchIssueKind::HostInvalid);
        }
        return Ok((require_valid_host(target)?, None));
    }
    match target.split_once(':') {
        Some((host, port)) => Ok((require_valid_host(host)?, Some(parse_port(port)?))),
        None => Ok((require_valid_host(target)?, None)),
    }
}

fn parse_ipv6_literal(value: &str) -> Option<std::net::Ipv6Addr> {
    // A zone id (`fe80::1%eth0`) is legal in OpenSSH even though the address
    // parser stops at the `%`; an empty or malformed zone is not a host.
    match value.split_once('%') {
        Some((address, zone)) => {
            if zone.is_empty()
                || !zone.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
                })
            {
                return None;
            }
            address.parse().ok()
        }
        None => value.parse().ok(),
    }
}

fn parse_port(value: &str) -> Result<u16, BatchIssueKind> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or(BatchIssueKind::PortInvalid)
}

fn require_valid_host(value: &str) -> Result<String, BatchIssueKind> {
    let host = value.trim();
    if host.is_empty() {
        return Err(BatchIssueKind::HostEmpty);
    }
    if host.chars().any(|character| {
        character.is_whitespace() || matches!(character, '@' | '#' | '&' | ',' | '?')
    }) {
        return Err(BatchIssueKind::HostInvalid);
    }
    Ok(host.to_owned())
}

fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(value)
        .to_owned()
}

impl fmt::Debug for ResolvedBatchRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedBatchRow")
            .field("line", &self.line)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("group", &self.group)
            .field("tags", &self.tags)
            .field("auth", &"[redacted]")
            .field(
                "proxy_command",
                &self.proxy_command.as_ref().map(|_| "[redacted]"),
            )
            .field("connect_timeout_seconds", &self.connect_timeout_seconds)
            .field("forward_agent", &self.forward_agent)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_auth() -> SavedAuth {
        SavedAuth::Key {
            key_path: "/tmp/id_ed25519".to_owned(),
            has_passphrase: false,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        }
    }

    fn stored_connection(id: &str, name: &str, host: &str) -> BatchExistingConnection {
        BatchExistingConnection {
            id: id.to_owned(),
            name: name.to_owned(),
            host: host.to_owned(),
            port: 22,
            username: "ops".to_owned(),
            group: None,
            tags: Vec::new(),
            auth_type: AuthType::Key,
            key_path: Some(expand_home_path("~/.ssh/kn")),
            cert_path: None,
            managed_key_id: None,
            has_proxy_command: false,
            connect_timeout_seconds: None,
            agent_forwarding: false,
        }
    }

    fn names(rows: &[BatchPreviewRow]) -> Vec<&str> {
        rows.iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn template_directives_map_to_shared_settings() {
        let template = parse_batch_template(
            "Host --\n  HostName {host}\n  User jaime.su\n  Port 2200\n  IdentityFile ~/.ssh/kn\n  ConnectTimeout 30\n  ForwardAgent yes\n",
        )
        .expect("template parses");
        assert_eq!(template.username.as_deref(), Some("jaime.su"));
        assert_eq!(template.port, Some(2200));
        assert_eq!(template.connect_timeout_seconds, Some(30));
        assert_eq!(template.forward_agent, Some(true));
        assert!(
            template
                .identity_file
                .as_deref()
                .expect("identity file")
                .ends_with(".ssh/kn")
        );
        let preview = resolve_batch_rows(
            &template,
            &parse_batch_data("hktkosl1100").expect("data parses"),
            None,
            &[],
            BatchDuplicateNamePolicy::Skip,
        );
        let row = preview.rows[0]
            .resolved
            .as_ref()
            .expect("a template with IdentityFile needs no dialog authentication");
        assert!(
            matches!(row.auth, SavedAuth::Key { .. }),
            "IdentityFile must select key authentication: {:?}",
            row.auth
        );
        assert_eq!(row.port, 2200, "the template port must reach the row");
        assert_eq!(row.username, "jaime.su");
        assert_eq!(row.connect_timeout_seconds, Some(30));
        assert_eq!(row.forward_agent, Some(true));
        assert!(row.proxy_command.is_none());
    }

    #[test]
    fn template_rejects_unknown_directives_and_literal_host_name() {
        let issue = parse_batch_template("Host --\n  ProxyJump jump\n")
            .expect_err("ProxyJump is not supported");
        assert_eq!(
            issue.kind,
            BatchIssueKind::DirectiveUnsupported {
                keyword: "proxyjump".to_owned()
            }
        );
        assert_eq!(
            issue.line, 2,
            "the directive line number must point at the rejected line"
        );
        let issue = parse_batch_template("Host --\n  HostName 10.0.0.1\n")
            .expect_err("a fixed HostName would silently override the data host");
        assert_eq!(issue.kind, BatchIssueKind::HostNameRequiresPlaceholder);
    }

    #[test]
    fn inline_credentials_split_password_from_key_path() {
        let parsed = parse_batch_data(
            "ops:example-pw@i-0db60e8069afa1197?name=helphub&group=helphub\n\
             ops:~/.ssh/id_ed25519@db1\n\
             ops:/etc/ssh/id_rsa@db2\n\
             ops:key=deploy@db3\n\
             ops:pass=ab/cd@db4\n\
             ops:@db5\n",
        )
        .expect("data parses");
        let row = &parsed.rows[0];
        assert_eq!(row.username.as_deref(), Some("ops"));
        assert_eq!(row.host, "i-0db60e8069afa1197");
        assert_eq!(row.name.as_deref(), Some("helphub"));
        assert_eq!(row.group.as_deref(), Some("helphub"));
        assert!(
            matches!(&row.auth, Some(BatchRowAuth::Password(value)) if value == "example-pw"),
            "an opaque value must be a password: {:?}",
            row.auth
        );
        let key_path = |row: &BatchDataRow| match row.auth.as_ref() {
            Some(BatchRowAuth::KeyPath(path)) => path.clone(),
            other => panic!("expected a key path, got {other:?}"),
        };
        assert!(
            key_path(&parsed.rows[1]).ends_with(".ssh/id_ed25519"),
            "a path-shaped value is a key and expands ~"
        );
        assert_eq!(key_path(&parsed.rows[2]), "/etc/ssh/id_rsa");
        assert_eq!(
            key_path(&parsed.rows[3]),
            "deploy",
            "key= forces a key path that would otherwise read as a password"
        );
        assert!(
            matches!(&parsed.rows[4].auth, Some(BatchRowAuth::Password(value)) if value == "ab/cd"),
            "pass= keeps a slash-bearing password a password"
        );
        assert_eq!(
            parsed
                .issues
                .iter()
                .map(|issue| issue.kind.clone())
                .collect::<Vec<_>>(),
            vec![BatchIssueKind::AuthEmpty],
            "an empty credential slot is reported instead of silently ignored"
        );
        assert!(
            !format!("{:?}", parsed.rows[0].auth).contains("example-pw"),
            "the inline password must not appear in debug output"
        );
    }

    #[test]
    fn inline_credentials_beat_template_identity_and_dialog_default() {
        let template = parse_batch_template(
            "Host --\n  HostName {host}\n  User ops\n  IdentityFile ~/.ssh/kn\n",
        )
        .expect("template parses");
        let data = parse_batch_data("ops:example-pw@db1\nops:~/.ssh/other@db2\nops@db3\n")
            .expect("data parses");
        let existing: [BatchExistingConnection; 0] = [];
        let preview = resolve_batch_rows(
            &template,
            &data,
            None,
            &existing,
            BatchDuplicateNamePolicy::Skip,
        );
        assert!(preview.issues.is_empty(), "issues: {:?}", preview.issues);
        let rows = preview.addable_rows();
        assert_eq!(rows.len(), 3);
        assert!(
            matches!(rows[0].auth, SavedAuth::Password { .. }),
            "an inline password must override the template IdentityFile"
        );
        match &rows[1].auth {
            SavedAuth::Key { key_path, .. } => {
                assert!(key_path.ends_with(".ssh/other"), "row key wins: {key_path}")
            }
            other => panic!("expected the row key, got {other:?}"),
        }
        match &rows[2].auth {
            SavedAuth::Key { key_path, .. } => assert!(key_path.ends_with(".ssh/kn")),
            other => panic!("expected the template key, got {other:?}"),
        }
    }

    #[test]
    fn template_username_and_keyboard_free_directives_resolve_per_row() {
        let template = parse_batch_template(
            "Host --\n  HostName {host}\n  User {host}\n  IdentityFile ~/.ssh/kn\n  ProxyCommand=ssh -W %h:%p jump\n",
        )
        .expect("Keyword=value with a multiword command parses");
        assert_eq!(
            template.proxy_command.as_deref(),
            Some("ssh -W %h:%p jump"),
            "only the first token is split, the rest stays in the value"
        );
        let preview = resolve_batch_rows(
            &template,
            &parse_batch_data("hktkosl1100?profile=prod\nops@hktkosl1101\n").expect("data parses"),
            None,
            &[],
            BatchDuplicateNamePolicy::Skip,
        );
        assert!(preview.issues.is_empty(), "issues: {:?}", preview.issues);
        let rows = preview.addable_rows();
        assert_eq!(
            rows[0].username, "hktkosl1100",
            "a template username must resolve its placeholders instead of saving the literal"
        );
        assert_eq!(
            rows[1].username, "ops",
            "a row username wins over the template"
        );
    }

    #[test]
    fn repeated_identity_directives_are_rejected_instead_of_dropped() {
        let issue = parse_batch_template(
            "Host --\n  HostName {host}\n  IdentityFile ~/.ssh/a\n  IdentityFile ~/.ssh/b\n",
        )
        .expect_err("a saved connection carries one identity");
        assert_eq!(
            issue.kind,
            BatchIssueKind::DirectiveDuplicate {
                keyword: "IdentityFile".to_owned()
            }
        );
        assert_eq!(issue.line, 4, "the error points at the second directive");
    }

    #[test]
    fn skipped_rows_do_not_reserve_their_display_name() {
        let template = parse_batch_template("Host --\n  User ops\n  IdentityFile ~/.ssh/kn\n")
            .expect("template parses");
        // Row 1 conflicts with an existing name that differs only by case, row 2
        // repeats row 1's target and is skipped, so row 3 must still get the name
        // row 2 asked for.
        let data = parse_batch_data("db1?name=shared\ndb1?name=taken\ndb2?name=taken\n")
            .expect("data parses");
        let existing_names = [stored_connection("shared-id", "SHARED", "other-host")];
        let preview = resolve_batch_rows(
            &template,
            &data,
            None,
            &existing_names,
            BatchDuplicateNamePolicy::Skip,
        );
        assert_eq!(
            preview
                .rows
                .iter()
                .map(|row| row.status)
                .collect::<Vec<_>>(),
            vec![
                BatchRowStatus::SkippedExistingName,
                BatchRowStatus::SkippedDuplicateTarget,
                BatchRowStatus::Add,
            ],
            "a skipped row must not consume its name, and existing names must match case-insensitively"
        );
        assert_eq!(names(&preview.rows), vec!["shared", "taken", "taken"]);
        assert_eq!(preview.addable_count(), 1);
    }

    #[test]
    fn data_lines_carry_overrides_and_quoted_metadata() {
        let parsed = parse_batch_data(
            "hktkosl1100?name=\"abc\"&group=\"prod\"\njaime.su@hktkosl1101:2222?name=\"abc d\"&group=prod&tags=prod,uat\n# comment\n",
        )
        .expect("data parses");
        assert_eq!(
            parsed.rows.len(),
            2,
            "the comment line must not create a row"
        );
        assert!(parsed.issues.is_empty());
        assert_eq!(parsed.rows[0].host, "hktkosl1100");
        assert_eq!(parsed.rows[0].name.as_deref(), Some("abc"));
        assert_eq!(parsed.rows[0].group.as_deref(), Some("prod"));
        assert_eq!(parsed.rows[1].username.as_deref(), Some("jaime.su"));
        assert_eq!(parsed.rows[1].port, Some(2222));
        assert_eq!(parsed.rows[1].name.as_deref(), Some("abc d"));
        assert_eq!(
            parsed.rows[1].tags,
            vec!["prod".to_owned(), "uat".to_owned()]
        );
    }

    #[test]
    fn data_rejects_malformed_targets_and_unknown_fields() {
        let parsed = parse_batch_data("  \nbad host\nok.example?owner=me\n:22\n")
            .expect("parse continues past bad lines");
        assert!(parsed.rows.is_empty());
        let kinds = parsed
            .issues
            .iter()
            .map(|issue| issue.kind.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                BatchIssueKind::HostInvalid,
                BatchIssueKind::QueryKeyUnknown {
                    key: "owner".to_owned()
                },
                BatchIssueKind::HostEmpty,
            ]
        );
        assert_eq!(
            parsed
                .issues
                .iter()
                .map(|issue| issue.line)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
    }

    #[test]
    fn profile_placeholder_is_required_per_row() {
        let template = parse_batch_template(
            "Host --\n  User jaime.su\n  IdentityFile ~/.ssh/kn\n  ProxyCommand powershell.exe \"aws ssm start-session --profile {profile} --target %h --parameters 'portNumber=%p'\"\n",
        )
        .expect("template parses");
        assert!(template.uses_profile());
        let data = parse_batch_data(
            "i-0c3ee871a280e4077?name=\"abc\"&group=\"prod\"&profile=prod-main\njaime.su@hktkosl1100:22?name=\"abc d\"&group=prod\n",
        )
        .expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(
            preview.issues,
            vec![BatchIssue::new(2, BatchIssueKind::ProfileMissing)]
        );
        assert_eq!(preview.addable_count(), 1);
        let command = preview.rows[0]
            .resolved
            .as_ref()
            .expect("first row is addable")
            .proxy_command
            .as_deref()
            .expect("proxy resolved");
        assert!(
            command.contains("--profile prod-main"),
            "the profile placeholder must be resolved: {command}"
        );
        assert!(
            command.contains("%h") && command.contains("%p"),
            "OpenSSH runtime tokens must survive substitution: {command}"
        );
    }

    #[test]
    fn rows_without_a_username_are_reported() {
        let template = parse_batch_template("Host --\n  IdentityFile ~/.ssh/kn\n")
            .expect("a template may omit User");
        let data = parse_batch_data("hktkosl1100\nroot@hktkosl1101\n").expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(
            preview.issues,
            vec![BatchIssue::new(1, BatchIssueKind::UsernameMissing)],
            "only the row without any username source must fail"
        );
        assert_eq!(preview.rows[0].username, "root");
        assert_eq!(preview.addable_count(), 1);
    }

    #[test]
    fn rows_inherit_template_defaults_and_data_overrides_them() {
        let template =
            parse_batch_template("Host --\n  User jaime.su\n  Port 22\n  IdentityFile ~/.ssh/kn\n")
                .expect("template parses");
        let data = parse_batch_data("hktkosl1100\nroot@hktkosl1101:2222\n").expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(preview.addable_count(), 2);
        assert_eq!(preview.rows[0].username, "jaime.su");
        assert_eq!(preview.rows[0].port, 22);
        assert_eq!(preview.rows[1].username, "root");
        assert_eq!(preview.rows[1].port, 2222);
        assert_eq!(names(&preview.rows), vec!["hktkosl1100", "hktkosl1101"]);
    }

    #[test]
    fn inline_credentials_reach_the_store_without_plaintext_on_disk() {
        let secret = ["inline", "secret", "value"].join("-");
        let directory = tempfile::tempdir().expect("temp store directory");
        let path = directory.path().join("connections.json");
        let mut store = ConnectionStore::load(&path).expect("store loads");
        let template = parse_batch_template("Host --\n  HostName {host}\n  User ops\n")
            .expect("template without identity parses");
        let data = parse_batch_data(&format!(
            "ops:{secret}@db1?name=pw-row\nops:~/.ssh/id_ed25519@db2?name=key-row\n"
        ))
        .expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(preview.addable_count(), 2);
        apply_batch(&mut store, preview.addable_rows()).expect("batch applies");

        let reloaded = ConnectionStore::load(&path).expect("store reloads");
        let row = |name: &str| {
            reloaded
                .connections()
                .iter()
                .find(|connection| connection.name == name)
                .unwrap_or_else(|| panic!("{name} must be saved"))
        };
        // The protected store lives in this instance, so the round-trip is checked
        // here rather than through a reloaded handle.
        let stored_auth = store
            .connections()
            .iter()
            .find(|connection| connection.name == "pw-row")
            .map(|connection| connection.auth.clone())
            .expect("pw-row is saved");
        let auth = &stored_auth;
        match auth {
            SavedAuth::Password {
                plaintext_password, ..
            } => assert!(
                plaintext_password.is_none(),
                "the persisted form must not carry the plaintext secret"
            ),
            other => panic!("inline secret must stay a password: {other:?}"),
        }
        assert_eq!(
            store
                .get_saved_auth_password(auth)
                .expect("the secret is protected")
                .expose_secret(),
            secret,
            "the protected store must keep the exact inline secret"
        );
        match &row("key-row").auth {
            SavedAuth::Key { key_path, .. } => {
                assert!(key_path.ends_with(".ssh/id_ed25519"), "{key_path}");
            }
            other => panic!("path-shaped inline value must be a key: {other:?}"),
        }
        assert_eq!(row("key-row").username, "ops");

        let on_disk = std::fs::read_to_string(&path).expect("connection file is readable");
        assert!(
            !on_disk.contains(&secret),
            "the connection file must never contain the inline secret"
        );
    }

    #[test]
    fn input_shapes_that_would_save_a_wrong_value_are_rejected() {
        // Quoting is syntax: the saved path must not keep the quote characters.
        let template = parse_batch_template(
            "Host --\n  HostName \"{host}\"\n  User \"jaime.su\"\n  IdentityFile \"~/.ssh/kn\"\n",
        )
        .expect("a quoted template parses");
        assert_eq!(template.username.as_deref(), Some("jaime.su"));
        let identity = template.identity_file.expect("identity file is kept");
        assert!(
            identity.ends_with(".ssh/kn") && !identity.contains('"'),
            "{identity}"
        );

        // A BOM from a Windows editor must not become part of the host.
        let data = parse_batch_data("\u{feff}ops@db1\n").expect("a BOM is tolerated");
        assert_eq!(data.rows[0].host, "db1");
        assert_eq!(data.rows[0].username.as_deref(), Some("ops"));

        // Two values for one field would silently drop the first.
        let duplicate = parse_batch_data("db1?name=first&name=second").expect("lines parse");
        // The line is reported instead of saved with whichever value came last.
        assert!(duplicate.rows.is_empty());
        assert_eq!(duplicate.issues.len(), 1);
        assert_eq!(duplicate.issues[0].line, 1);
        assert!(matches!(
            duplicate.issues[0].kind,
            BatchIssueKind::QueryKeyDuplicate { ref key } if key == "name"
        ));

        // Several colons must be a real IPv6 address, not a mistyped port.
        assert_eq!(
            parse_batch_data("[::1]:2222")
                .expect("bracketed literal")
                .rows[0]
                .port,
            Some(2222)
        );
        assert_eq!(
            parse_batch_data("fe80::1").expect("bare literal").rows[0].host,
            "fe80::1"
        );
        assert!(
            parse_batch_data("host:22:23")
                .expect("lines parse")
                .issues
                .iter()
                .any(|issue| matches!(issue.kind, BatchIssueKind::HostInvalid)),
            "a mistyped port after a host must be reported"
        );

        // A row value carrying a brace cannot smuggle one into the saved value.
        let template = parse_batch_template(
            "Host --\n  HostName {host}\n  User ops\n  ProxyCommand sh -c \"echo {profile}\"\n",
        )
        .expect("template parses");
        let data = parse_batch_data("db1?profile=prod}").expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(preview.addable_count(), 0);
        assert!(
            preview
                .issues
                .iter()
                .any(|issue| matches!(issue.kind, BatchIssueKind::BraceUnmatched))
        );
    }

    #[test]
    fn two_credentials_for_one_host_are_two_connections() {
        let template = parse_batch_template(
            "Host --\n  HostName {host}\n  User ops\n  IdentityFile ~/.ssh/kn\n",
        )
        .expect("template parses");
        let data = parse_batch_data(concat!(
            "i-0c3ee871a280e4077?name=helphub.uat&group=helphub\n",
            "i-0db60e8069afa1197?name=helphub.prod&group=helphub\n",
            "ops:~/.ssh/helphub-prod-ops@i-0db60e8069afa1197?name=helphub.prod.ops&group=helphub\n",
            "ops:~/.ssh/helphub-uat-ops@i-0c3ee871a280e4077?name=helphub.uat.ops&group=helphub\n",
        ))
        .expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert!(
            preview.issues.is_empty(),
            "the reported batch must not carry issues: {:?}",
            preview.issues
        );
        assert_eq!(
            names(&preview.rows),
            vec![
                "helphub.uat",
                "helphub.prod",
                "helphub.prod.ops",
                "helphub.uat.ops"
            ]
        );
        assert_eq!(preview.addable_count(), 4, "each credential is its own row");

        // A certificate file is part of the credential: the same key with a
        // different certificate is a change, the same pair is not.
        let cert_template = parse_batch_template(concat!(
            "Host --\n  HostName {host}\n  User ops\n",
            "  CertificateFile ~/.ssh/kn-cert.pub\n",
            "  IdentityFile ~/.ssh/kn\n",
        ))
        .expect("certificate template parses");
        let data = parse_batch_data("i-1?name=cert-host\n").expect("data parses");
        let certificate_row = BatchExistingConnection {
            id: "cert-id".to_owned(),
            name: "cert-host".to_owned(),
            host: "i-1".to_owned(),
            port: 22,
            username: "ops".to_owned(),
            group: None,
            tags: Vec::new(),
            auth_type: AuthType::Certificate,
            key_path: Some(expand_home_path("~/.ssh/kn")),
            cert_path: Some(expand_home_path("~/.ssh/kn-cert.pub")),
            managed_key_id: None,
            has_proxy_command: false,
            connect_timeout_seconds: None,
            agent_forwarding: false,
        };
        let preview = resolve_batch_rows(
            &cert_template,
            &data,
            None,
            &[certificate_row.clone()],
            BatchDuplicateNamePolicy::Skip,
        );
        assert_eq!(preview.rows[0].status, BatchRowStatus::Unchanged);
        let rotated = parse_batch_template(concat!(
            "Host --\n  HostName {host}\n  User ops\n",
            "  CertificateFile ~/.ssh/kn-cert-2027.pub\n",
            "  IdentityFile ~/.ssh/kn\n",
        ))
        .expect("rotated template parses");
        let preview = resolve_batch_rows(
            &rotated,
            &data,
            None,
            &[certificate_row],
            BatchDuplicateNamePolicy::Skip,
        );
        assert_eq!(preview.rows[0].status, BatchRowStatus::Update);

        // Repeating a row with the same credential is still a duplicate target.
        let repeated =
            parse_batch_data("ops:~/.ssh/kn@i-1\nops:~/.ssh/kn@i-1\n").expect("data parses");
        let preview = resolve_batch_rows(
            &template,
            &repeated,
            None,
            &[],
            BatchDuplicateNamePolicy::Skip,
        );
        assert_eq!(
            preview.rows[1].status,
            BatchRowStatus::SkippedDuplicateTarget
        );
        assert_eq!(preview.addable_count(), 1);
    }

    #[test]
    fn a_rerun_updates_the_connection_it_created_instead_of_renaming_it() {
        let directory = tempfile::tempdir().expect("temp store directory");
        let path = directory.path().join("connections.json");
        let mut store = ConnectionStore::load(&path).expect("store loads");
        let template = parse_batch_template(
            "Host --\n  HostName {host}\n  User ops\n  IdentityFile ~/.ssh/kn\n",
        )
        .expect("template parses");
        let first =
            parse_batch_data("i-0c3ee871a280e4077?name=helphub.uat\n").expect("data parses");
        let preview =
            resolve_batch_rows(&template, &first, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(
            apply_batch(&mut store, preview.addable_rows())
                .expect("batch applies")
                .updated,
            0
        );

        let created_id = store.connections()[0].id.clone();
        let existing = store
            .connections()
            .iter()
            .map(BatchExistingConnection::from_saved)
            .collect::<Vec<_>>();
        let second =
            parse_batch_data("i-0c3ee871a280e4077?name=helphub.uat&group=helphub&tags=uat\n")
                .expect("data parses");
        let preview = resolve_batch_rows(
            &template,
            &second,
            None,
            &existing,
            BatchDuplicateNamePolicy::Rename,
        );
        assert_eq!(preview.rows[0].status, BatchRowStatus::Update);
        let applied = apply_batch(&mut store, preview.addable_rows()).expect("update applies");
        assert_eq!(applied.updated, 1);
        let stored = store.connections().to_vec();
        assert_eq!(stored.len(), 1, "an update must not add a renamed copy");
        assert_eq!(
            stored[0].id, created_id,
            "the stored identity keeps its keychain references"
        );
        assert_eq!(stored[0].name, "helphub.uat");
        assert_eq!(stored[0].group.as_deref(), Some("helphub"));
        assert_eq!(stored[0].tags, vec!["uat".to_owned()]);
        assert_eq!(
            stored[0].auth.auth_type(),
            AuthType::Key,
            "the update keeps the authentication the row resolves to"
        );

        // Feeding the same data again must read as "already there", not as a
        // change, and the rename policy must still create a renamed copy.
        let existing = store
            .connections()
            .iter()
            .map(BatchExistingConnection::from_saved)
            .collect::<Vec<_>>();
        let preview = resolve_batch_rows(
            &template,
            &second,
            None,
            &existing,
            BatchDuplicateNamePolicy::Skip,
        );
        assert_eq!(preview.rows[0].status, BatchRowStatus::Unchanged);
        assert_eq!(preview.addable_count(), 0, "nothing to write");
        let preview = resolve_batch_rows(
            &template,
            &second,
            None,
            &existing,
            BatchDuplicateNamePolicy::Rename,
        );
        assert_eq!(preview.rows[0].status, BatchRowStatus::Renamed);
        assert_eq!(preview.rows[0].name, "helphub.uat-2");

        // A name owned by a different target is still a conflict, so the policy
        // keeps deciding between skipping and renaming.
        let other = stored_connection("other-id", "helphub.uat", "i-other");
        let preview = resolve_batch_rows(
            &template,
            &second,
            None,
            &[other],
            BatchDuplicateNamePolicy::Rename,
        );
        assert_eq!(preview.rows[0].status, BatchRowStatus::Renamed);
        assert_eq!(preview.rows[0].name, "helphub.uat-2");
    }

    #[test]
    fn duplicate_targets_and_names_are_skipped_or_renamed() {
        let template = parse_batch_template("Host --\n  User jaime.su\n  IdentityFile ~/.ssh/kn\n")
            .expect("template parses");
        let data = parse_batch_data("hktkosl1100\nhktkosl1100?group=prod\nprod-box\n")
            .expect("data parses");
        let existing = [stored_connection("prod-id", "prod-box", "other-host")];
        let preview = resolve_batch_rows(
            &template,
            &data,
            None,
            &existing,
            BatchDuplicateNamePolicy::Rename,
        );
        assert_eq!(
            preview
                .rows
                .iter()
                .map(|row| row.status)
                .collect::<Vec<_>>(),
            vec![
                BatchRowStatus::Add,
                BatchRowStatus::SkippedDuplicateTarget,
                BatchRowStatus::Renamed,
            ]
        );
        assert_eq!(
            names(&preview.rows),
            vec!["hktkosl1100", "hktkosl1100", "prod-box-2"]
        );
        assert_eq!(preview.addable_count(), 2);

        let preview = resolve_batch_rows(
            &template,
            &data,
            None,
            &existing,
            BatchDuplicateNamePolicy::Skip,
        );
        assert_eq!(
            preview.rows[2].status,
            BatchRowStatus::SkippedExistingName,
            "the skip policy must never overwrite an existing name"
        );
        assert_eq!(preview.addable_count(), 1);
    }

    #[test]
    fn rows_without_any_authentication_are_reported() {
        let template = parse_batch_template("Host --\n  User jaime.su\n").expect("template parses");
        let data = parse_batch_data("hktkosl1100\n").expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(
            preview.issues,
            vec![BatchIssue::new(1, BatchIssueKind::AuthMissing)]
        );
        assert_eq!(preview.addable_count(), 0);

        let preview = resolve_batch_rows(
            &template,
            &data,
            Some(&key_auth()),
            &[],
            BatchDuplicateNamePolicy::Skip,
        );
        assert!(preview.issues.is_empty());
        assert_eq!(preview.addable_count(), 1);
    }

    #[test]
    fn tags_are_trimmed_and_deduplicated_case_insensitively() {
        assert_eq!(
            normalize_batch_tags(vec![
                " Prod ".to_owned(),
                "prod".to_owned(),
                "".to_owned(),
                "uat".to_owned()
            ]),
            vec!["Prod".to_owned(), "uat".to_owned()]
        );
    }

    #[test]
    fn batch_row_limit_is_enforced() {
        let data = (0..=MAX_BATCH_ROWS)
            .map(|index| format!("host-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let issue = parse_batch_data(&data).expect_err("an oversized paste must not be accepted");
        assert_eq!(
            issue.kind,
            BatchIssueKind::RowLimitExceeded {
                limit: MAX_BATCH_ROWS
            }
        );
    }

    #[test]
    fn batch_apply_persists_each_rows_resolved_settings() {
        let directory = tempfile::tempdir().expect("temp store directory");
        let path = directory.path().join("connections.json");
        let mut store = ConnectionStore::load(&path).expect("store loads");
        let template = parse_batch_template(
            "Host --\n  User jaime.su\n  IdentityFile ~/.ssh/kn\n  ProxyCommand sh -c \"aws ssm start-session --profile {profile} --target %h\"\n",
        )
        .expect("template parses");
        let data = parse_batch_data("i-1?profile=prod&tags=prod\ni-2?profile=uat&group=uat\n")
            .expect("data parses");
        let preview =
            resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
        assert_eq!(preview.addable_count(), 2);

        let added = apply_batch(&mut store, preview.addable_rows())
            .expect("batch applies")
            .added;
        assert_eq!(added.len(), 2);

        let mut stored = store.connections().to_vec();
        stored.sort_by(|left, right| left.name.cmp(&right.name));
        for (row, profile) in stored.iter().zip(["prod", "uat"]) {
            let command = store
                .get_saved_proxy_command(
                    row.proxy_command
                        .as_ref()
                        .unwrap_or_else(|| panic!("{} must persist a ProxyCommand", row.name)),
                )
                .expect("proxy command secret is protected");
            assert_eq!(
                command.expose_secret(),
                format!("sh -c \"aws ssm start-session --profile {profile} --target %h\""),
                "each row must keep its own substituted profile"
            );
        }

        let reloaded = ConnectionStore::load(&path).expect("store reloads");
        let mut persisted = reloaded.connections().to_vec();
        persisted.sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(
            persisted
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<&str>>(),
            vec!["i-1", "i-2"],
            "a batch must be readable after restart"
        );
        assert!(
            persisted
                .iter()
                .all(|row| matches!(row.auth, SavedAuth::Key { .. })),
            "the template identity file must select key authentication for every row"
        );
        assert_eq!(persisted[0].tags, vec!["prod".to_owned()]);
        assert_eq!(persisted[1].group.as_deref(), Some("uat"));
    }

    #[test]
    fn resolved_row_debug_never_renders_the_proxy_command_text() {
        let row = ResolvedBatchRow {
            line: 1,
            name: "i-1".to_owned(),
            host: "i-1".to_owned(),
            port: 22,
            username: "root".to_owned(),
            group: None,
            tags: Vec::new(),
            auth: key_auth(),
            proxy_command: Some("aws ssm start-session --profile secret-profile".to_owned()),
            connect_timeout_seconds: None,
            forward_agent: None,
            replace_id: None,
        };
        let rendered = format!("{row:?}");
        assert!(
            !rendered.contains("secret-profile"),
            "batch rows must stay safe to log: {rendered}"
        );
    }
}
