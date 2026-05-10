use super::actions::classify_command_risk;
use super::ime::WorkspaceImeTarget;
use super::*;
use crate::assets::LucideIcon;
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const QUICK_COMMANDS_FILENAME: &str = "quick-commands.json";
const QUICK_COMMANDS_SCHEMA_VERSION: u32 = 1;
const MAX_QUICK_COMMANDS_FILE_BYTES: u64 = 512 * 1024;
const MAX_CATEGORIES: usize = 100;
const MAX_COMMANDS: usize = 1000;
const MAX_ID_LEN: usize = 128;
const MAX_NAME_LEN: usize = 160;
const MAX_COMMAND_LEN: usize = 4096;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_HOST_PATTERN_LEN: usize = 256;
static QUICK_COMMAND_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum QuickCommandInput {
    Search,
    CommandName,
    CommandText,
    CommandDescription,
    CommandHostPattern,
    CategoryName,
}

impl QuickCommandInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::CommandName => 2,
            Self::CommandText => 3,
            Self::CommandDescription => 4,
            Self::CommandHostPattern => 5,
            Self::CategoryName => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum QuickCommandIcon {
    Terminal,
    Server,
    Folder,
    Docker,
    Zap,
}

impl QuickCommandIcon {
    fn as_source_id(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Server => "server",
            Self::Folder => "folder",
            Self::Docker => "docker",
            Self::Zap => "zap",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommandCategory {
    pub id: String,
    pub name: String,
    pub icon: QuickCommandIcon,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommand {
    pub id: String,
    pub name: String,
    pub command: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_pattern: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuickCommandsSnapshot {
    pub version: u32,
    pub categories: Vec<QuickCommandCategory>,
    pub commands: Vec<QuickCommand>,
    pub updated_at: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum QuickCommandImportStrategy {
    Rename,
    Skip,
    Replace,
    Merge,
}

#[derive(Clone, Debug)]
pub(super) struct QuickCommandDraft {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub category: String,
    pub description: String,
    pub host_pattern: String,
}

#[derive(Clone, Debug)]
pub(super) struct QuickCommandCategoryDraft {
    pub id: Option<String>,
    pub name: String,
    pub icon: QuickCommandIcon,
}

#[derive(Clone, Debug)]
pub(super) struct QuickCommandsState {
    path: PathBuf,
    pub categories: Vec<QuickCommandCategory>,
    pub commands: Vec<QuickCommand>,
    pub active_category: String,
    pub query: String,
    pub focused_input: Option<QuickCommandInput>,
    pub command_editor: Option<QuickCommandDraft>,
    pub category_editor: Option<QuickCommandCategoryDraft>,
    pub last_persist_error: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct QuickCommandsImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

impl QuickCommandsState {
    pub(super) fn load(settings_path: &Path) -> Self {
        let path = settings_path
            .parent()
            .unwrap_or(settings_path)
            .join(QUICK_COMMANDS_FILENAME);
        let mut state = Self {
            path,
            categories: default_quick_command_categories(),
            commands: default_quick_commands(),
            active_category: "system".to_string(),
            query: String::new(),
            focused_input: None,
            command_editor: None,
            category_editor: None,
            last_persist_error: None,
        };

        match load_snapshot_from_path(&state.path) {
            Ok(Some(snapshot)) => {
                state.categories = snapshot.categories;
                state.commands = snapshot.commands;
                state.ensure_active_category();
            }
            Ok(None) => {}
            Err(error) => {
                state.last_persist_error = Some(error);
            }
        }
        state
    }

    #[allow(dead_code)]
    pub(super) fn visible_commands(&self) -> Vec<QuickCommand> {
        self.visible_commands_for_targets(&[])
    }

    pub(super) fn visible_commands_for_targets(
        &self,
        target_fields: &[String],
    ) -> Vec<QuickCommand> {
        let query = self.query.trim().to_lowercase();
        self.commands
            .iter()
            .filter(|command| command.category == self.active_category)
            .filter(|command| {
                match_quick_command_host_pattern(command.host_pattern.as_deref(), target_fields)
            })
            .filter(|command| {
                query.is_empty()
                    || command.name.to_lowercase().contains(&query)
                    || command.command.to_lowercase().contains(&query)
                    || command
                        .description
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(super) fn upsert_command(&mut self, draft: QuickCommandDraft) {
        let now = now_ms();
        let existing = draft
            .id
            .as_ref()
            .and_then(|id| self.commands.iter().find(|command| &command.id == id));
        let command = QuickCommand {
            id: draft.id.unwrap_or_else(new_quick_command_id),
            name: draft.name.trim().to_string(),
            command: draft.command.trim().to_string(),
            category: if self.categories.iter().any(|c| c.id == draft.category) {
                draft.category
            } else {
                "custom".to_string()
            },
            description: trim_optional(&draft.description),
            host_pattern: trim_optional(&draft.host_pattern),
            created_at: existing.map(|command| command.created_at).unwrap_or(now),
            updated_at: now,
        };
        if command.name.is_empty() || command.command.is_empty() {
            return;
        }
        if self
            .commands
            .iter()
            .any(|candidate| candidate.id == command.id)
        {
            self.commands = self
                .commands
                .iter()
                .map(|candidate| {
                    if candidate.id == command.id {
                        command.clone()
                    } else {
                        candidate.clone()
                    }
                })
                .collect();
        } else {
            self.commands.push(command);
        }
        self.persist();
    }

    pub(super) fn delete_command(&mut self, id: &str) {
        self.commands.retain(|command| command.id != id);
        self.persist();
    }

    pub(super) fn upsert_category(&mut self, draft: QuickCommandCategoryDraft) -> String {
        let category = QuickCommandCategory {
            id: draft.id.unwrap_or_else(new_quick_category_id),
            name: draft.name.trim().to_string(),
            icon: draft.icon,
        };
        if category.name.is_empty() {
            return self.active_category.clone();
        }
        if self
            .categories
            .iter()
            .any(|candidate| candidate.id == category.id)
        {
            self.categories = self
                .categories
                .iter()
                .map(|candidate| {
                    if candidate.id == category.id {
                        category.clone()
                    } else {
                        candidate.clone()
                    }
                })
                .collect();
        } else if self.categories.len() < MAX_CATEGORIES {
            self.categories.push(category.clone());
        }
        self.active_category = category.id.clone();
        self.persist();
        category.id
    }

    pub(super) fn delete_category(&mut self, id: &str) -> bool {
        if default_quick_command_categories()
            .iter()
            .any(|category| category.id == id)
            || self.commands.iter().any(|command| command.category == id)
        {
            return false;
        }
        let before = self.categories.len();
        self.categories.retain(|category| category.id != id);
        if self.categories.len() == before {
            return false;
        }
        self.ensure_active_category();
        self.persist();
        true
    }

    #[allow(dead_code)]
    pub(super) fn reset_defaults(&mut self) {
        self.categories = default_quick_command_categories();
        self.commands = default_quick_commands();
        self.active_category = "system".to_string();
        self.command_editor = None;
        self.category_editor = None;
        self.persist();
    }

    #[allow(dead_code)]
    pub(super) fn export_snapshot_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.snapshot()).map_err(|err| err.to_string())
    }

    #[allow(dead_code)]
    pub(super) fn apply_snapshot_json(
        &mut self,
        snapshot_json: &str,
        strategy: QuickCommandImportStrategy,
    ) -> QuickCommandsImportResult {
        let parsed = serde_json::from_str::<QuickCommandsSnapshot>(snapshot_json)
            .map_err(|err| err.to_string())
            .and_then(sanitize_snapshot);
        match parsed {
            Ok(snapshot) => self.apply_snapshot(snapshot, strategy),
            Err(error) => QuickCommandsImportResult {
                imported: 0,
                skipped: 0,
                errors: vec![error],
            },
        }
    }

    #[allow(dead_code)]
    fn apply_snapshot(
        &mut self,
        snapshot: QuickCommandsSnapshot,
        strategy: QuickCommandImportStrategy,
    ) -> QuickCommandsImportResult {
        let MergeResult {
            categories,
            commands,
            imported,
            skipped,
        } = merge_quick_commands_snapshot(&self.categories, &self.commands, snapshot, strategy);
        self.categories = categories;
        self.commands = commands;
        self.ensure_active_category();
        self.persist();
        QuickCommandsImportResult {
            imported,
            skipped,
            errors: Vec::new(),
        }
    }

    fn snapshot(&self) -> QuickCommandsSnapshot {
        QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: self.categories.clone(),
            commands: self.commands.clone(),
            updated_at: now_ms(),
        }
    }

    fn persist(&mut self) {
        let snapshot = self.snapshot();
        self.last_persist_error = save_snapshot_to_path(&self.path, &snapshot).err();
    }

    fn ensure_active_category(&mut self) {
        if !self
            .categories
            .iter()
            .any(|category| category.id == self.active_category)
        {
            self.active_category = self
                .categories
                .first()
                .map(|category| category.id.clone())
                .unwrap_or_else(|| "custom".to_string());
        }
    }
}

#[allow(dead_code)]
struct MergeResult {
    categories: Vec<QuickCommandCategory>,
    commands: Vec<QuickCommand>,
    imported: usize,
    skipped: usize,
}

#[allow(dead_code)]
fn merge_quick_commands_snapshot(
    current_categories: &[QuickCommandCategory],
    current_commands: &[QuickCommand],
    incoming: QuickCommandsSnapshot,
    strategy: QuickCommandImportStrategy,
) -> MergeResult {
    let now = now_ms();
    let mut imported = 0;
    let mut skipped = 0;
    let mut categories = current_categories.to_vec();
    let mut commands = current_commands.to_vec();
    let mut category_remap = HashMap::new();

    for imported_category in incoming.categories {
        let conflict = categories
            .iter()
            .find(|category| {
                category.id == imported_category.id
                    || category
                        .name
                        .trim()
                        .eq_ignore_ascii_case(imported_category.name.trim())
            })
            .cloned();
        let Some(conflict) = conflict else {
            category_remap.insert(imported_category.id.clone(), imported_category.id.clone());
            categories.push(imported_category);
            continue;
        };

        match strategy {
            QuickCommandImportStrategy::Skip => {
                category_remap.insert(imported_category.id, conflict.id);
                skipped += 1;
            }
            QuickCommandImportStrategy::Rename => {
                let renamed = QuickCommandCategory {
                    id: new_quick_category_id(),
                    name: unique_category_name(
                        &categories,
                        &format!("{} (Imported)", imported_category.name),
                    ),
                    icon: imported_category.icon,
                };
                category_remap.insert(imported_category.id, renamed.id.clone());
                categories.push(renamed);
                imported += 1;
            }
            QuickCommandImportStrategy::Replace | QuickCommandImportStrategy::Merge => {
                category_remap.insert(imported_category.id, conflict.id.clone());
                categories = categories
                    .into_iter()
                    .map(|category| {
                        if category.id == conflict.id {
                            QuickCommandCategory {
                                id: conflict.id.clone(),
                                name: imported_category.name.clone(),
                                icon: imported_category.icon,
                            }
                        } else {
                            category
                        }
                    })
                    .collect();
                imported += 1;
            }
        }
    }

    let category_ids = categories
        .iter()
        .map(|category| category.id.clone())
        .collect::<HashSet<_>>();
    for imported_command in incoming.commands {
        let category = category_remap
            .get(&imported_command.category)
            .cloned()
            .unwrap_or_else(|| imported_command.category.clone());
        let category = if category_ids.contains(&category) {
            category
        } else {
            "custom".to_string()
        };
        let imported_command = QuickCommand {
            category,
            ..imported_command
        };
        let conflict = commands
            .iter()
            .find(|command| {
                command.id == imported_command.id
                    || (command.category == imported_command.category
                        && command
                            .name
                            .trim()
                            .eq_ignore_ascii_case(imported_command.name.trim()))
            })
            .cloned();
        let Some(conflict) = conflict else {
            commands.push(imported_command);
            imported += 1;
            continue;
        };

        match strategy {
            QuickCommandImportStrategy::Skip => skipped += 1,
            QuickCommandImportStrategy::Rename => {
                commands.push(QuickCommand {
                    id: new_quick_command_id(),
                    name: unique_command_name(
                        &commands,
                        &imported_command.category,
                        &format!("{} (Imported)", imported_command.name),
                    ),
                    ..imported_command
                });
                imported += 1;
            }
            QuickCommandImportStrategy::Merge => {
                commands = commands
                    .into_iter()
                    .map(|command| {
                        if command.id == conflict.id {
                            QuickCommand {
                                id: conflict.id.clone(),
                                created_at: conflict.created_at,
                                updated_at: now,
                                ..imported_command.clone()
                            }
                        } else {
                            command
                        }
                    })
                    .collect();
                imported += 1;
            }
            QuickCommandImportStrategy::Replace => {
                commands = commands
                    .into_iter()
                    .map(|command| {
                        if command.id == conflict.id {
                            QuickCommand {
                                id: conflict.id.clone(),
                                updated_at: now,
                                ..imported_command.clone()
                            }
                        } else {
                            command
                        }
                    })
                    .collect();
                imported += 1;
            }
        }
    }

    MergeResult {
        categories,
        commands,
        imported,
        skipped,
    }
}

fn load_snapshot_from_path(path: &Path) -> Result<Option<QuickCommandsSnapshot>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Failed to stat Quick Commands file: {error}")),
    };
    if metadata.len() > MAX_QUICK_COMMANDS_FILE_BYTES {
        return Err("Quick Commands file exceeds size limit".to_string());
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read Quick Commands file: {error}"))?;
    if contents.trim().is_empty() {
        return Ok(None);
    }
    let snapshot = serde_json::from_str::<QuickCommandsSnapshot>(&contents)
        .map_err(|error| format!("Failed to parse Quick Commands file: {error}"))?;
    sanitize_snapshot(snapshot).map(Some)
}

fn save_snapshot_to_path(path: &Path, snapshot: &QuickCommandsSnapshot) -> Result<(), String> {
    let snapshot = sanitize_snapshot(snapshot.clone())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create Quick Commands directory: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(&snapshot)
        .map_err(|error| format!("Failed to serialize Quick Commands: {error}"))?;
    if json.len() as u64 > MAX_QUICK_COMMANDS_FILE_BYTES {
        return Err("Quick Commands snapshot exceeds size limit".to_string());
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json)
        .map_err(|error| format!("Failed to write Quick Commands temp file: {error}"))?;
    fs::rename(&temp_path, path)
        .map_err(|error| format!("Failed to replace Quick Commands file: {error}"))?;
    Ok(())
}

fn sanitize_snapshot(snapshot: QuickCommandsSnapshot) -> Result<QuickCommandsSnapshot, String> {
    if snapshot.version != QUICK_COMMANDS_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Quick Commands schema version {}",
            snapshot.version
        ));
    }
    if snapshot.categories.len() > MAX_CATEGORIES {
        return Err(format!(
            "Quick Commands category count exceeds limit {MAX_CATEGORIES}"
        ));
    }
    if snapshot.commands.len() > MAX_COMMANDS {
        return Err(format!(
            "Quick Commands command count exceeds limit {MAX_COMMANDS}"
        ));
    }
    let categories = snapshot
        .categories
        .into_iter()
        .map(sanitize_category)
        .collect::<Result<Vec<_>, _>>()?;
    let category_ids = categories
        .iter()
        .map(|category| category.id.clone())
        .collect::<HashSet<_>>();
    let commands = snapshot
        .commands
        .into_iter()
        .map(|command| sanitize_command(command, &category_ids))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(QuickCommandsSnapshot {
        version: QUICK_COMMANDS_SCHEMA_VERSION,
        categories,
        commands,
        updated_at: snapshot.updated_at,
    })
}

fn sanitize_category(category: QuickCommandCategory) -> Result<QuickCommandCategory, String> {
    Ok(QuickCommandCategory {
        id: bounded_required(category.id, "category.id", MAX_ID_LEN)?,
        name: bounded_required(category.name, "category.name", MAX_NAME_LEN)?,
        icon: category.icon,
    })
}

fn sanitize_command(
    command: QuickCommand,
    category_ids: &HashSet<String>,
) -> Result<QuickCommand, String> {
    let category = bounded_required(command.category, "command.category", MAX_ID_LEN)?;
    Ok(QuickCommand {
        id: bounded_required(command.id, "command.id", MAX_ID_LEN)?,
        name: bounded_required(command.name, "command.name", MAX_NAME_LEN)?,
        command: bounded_required(command.command, "command.command", MAX_COMMAND_LEN)?,
        category: if category_ids.contains(&category) {
            category
        } else {
            "custom".to_string()
        },
        description: bounded_optional(
            command.description,
            "command.description",
            MAX_DESCRIPTION_LEN,
        )?,
        host_pattern: bounded_optional(
            command.host_pattern,
            "command.hostPattern",
            MAX_HOST_PATTERN_LEN,
        )?,
        created_at: command.created_at,
        updated_at: command.updated_at,
    })
}

fn bounded_required(value: String, field: &str, max_len: usize) -> Result<String, String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(format!("Quick Commands field {field} cannot be empty"));
    }
    if trimmed.len() > max_len {
        return Err(format!(
            "Quick Commands field {field} exceeds limit {max_len}"
        ));
    }
    Ok(trimmed)
}

fn bounded_optional(
    value: Option<String>,
    field: &str,
    max_len: usize,
) -> Result<Option<String>, String> {
    match value.map(|item| item.trim().to_string()) {
        Some(item) if item.is_empty() => Ok(None),
        Some(item) if item.len() > max_len => Err(format!(
            "Quick Commands field {field} exceeds limit {max_len}"
        )),
        Some(item) => Ok(Some(item)),
        None => Ok(None),
    }
}

pub(super) fn default_quick_command_categories() -> Vec<QuickCommandCategory> {
    vec![
        quick_category("system", "System", QuickCommandIcon::Server),
        quick_category("network", "Network", QuickCommandIcon::Terminal),
        quick_category("files", "Files", QuickCommandIcon::Folder),
        quick_category("docker", "Docker", QuickCommandIcon::Docker),
        quick_category("custom", "Custom", QuickCommandIcon::Zap),
    ]
}

pub(super) fn default_quick_commands() -> Vec<QuickCommand> {
    vec![
        quick_command(
            "qc-pwd",
            "Print Working Directory",
            "pwd",
            "files",
            "Show the current directory.",
        ),
        quick_command(
            "qc-ls-la",
            "List Files",
            "ls -la",
            "files",
            "List files with details.",
        ),
        quick_command(
            "qc-df-h",
            "Disk Usage",
            "df -h",
            "system",
            "Show mounted filesystem usage.",
        ),
        quick_command(
            "qc-free-h",
            "Memory Usage",
            "free -h",
            "system",
            "Show memory usage.",
        ),
        quick_command(
            "qc-uptime",
            "Uptime",
            "uptime",
            "system",
            "Show uptime and load average.",
        ),
        quick_command(
            "qc-whoami",
            "Current User",
            "whoami",
            "system",
            "Show the current user.",
        ),
        quick_command(
            "qc-ip-addr",
            "IP Addresses",
            "ip addr",
            "network",
            "Show network interface addresses.",
        ),
        quick_command(
            "qc-ifconfig",
            "Interface Config",
            "ifconfig",
            "network",
            "Show network interfaces on systems without iproute2.",
        ),
        quick_command(
            "qc-docker-ps",
            "Docker Containers",
            "docker ps",
            "docker",
            "List running containers.",
        ),
        quick_command(
            "qc-git-status",
            "Git Status",
            "git status",
            "files",
            "Show repository status.",
        ),
        quick_command(
            "qc-journal-errors",
            "Recent Journal Errors",
            "journalctl -xe --no-pager",
            "system",
            "Show recent system journal errors.",
        ),
    ]
}

fn quick_category(id: &str, name: &str, icon: QuickCommandIcon) -> QuickCommandCategory {
    QuickCommandCategory {
        id: id.to_string(),
        name: name.to_string(),
        icon,
    }
}

fn quick_command(
    id: &str,
    name: &str,
    command: &str,
    category: &str,
    description: &str,
) -> QuickCommand {
    QuickCommand {
        id: id.to_string(),
        name: name.to_string(),
        command: command.to_string(),
        category: category.to_string(),
        description: Some(description.to_string()),
        host_pattern: None,
        created_at: 0,
        updated_at: 0,
    }
}

fn trim_optional(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn new_quick_command_id() -> String {
    format!(
        "qc-{}-{}",
        now_ms(),
        QUICK_COMMAND_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn new_quick_category_id() -> String {
    format!(
        "qcg-{}-{}",
        now_ms(),
        QUICK_COMMAND_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[allow(dead_code)]
fn unique_category_name(categories: &[QuickCommandCategory], desired_name: &str) -> String {
    let existing = categories
        .iter()
        .map(|category| category.name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    unique_name(desired_name, &existing)
}

#[allow(dead_code)]
fn unique_command_name(commands: &[QuickCommand], category: &str, desired_name: &str) -> String {
    let existing = commands
        .iter()
        .filter(|command| command.category == category)
        .map(|command| command.name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    unique_name(desired_name, &existing)
}

#[allow(dead_code)]
fn unique_name(desired_name: &str, existing_lower_names: &HashSet<String>) -> String {
    if !existing_lower_names.contains(&desired_name.trim().to_lowercase()) {
        return desired_name.to_string();
    }
    for index in 2..1000 {
        let candidate = format!("{desired_name} ({index})");
        if !existing_lower_names.contains(&candidate.trim().to_lowercase()) {
            return candidate;
        }
    }
    format!("{desired_name} ({})", now_ms())
}

pub(super) fn quick_command_lucide_icon(icon: QuickCommandIcon) -> LucideIcon {
    match icon {
        QuickCommandIcon::Server => LucideIcon::Server,
        QuickCommandIcon::Folder => LucideIcon::Folder,
        QuickCommandIcon::Docker => LucideIcon::Server,
        QuickCommandIcon::Zap => LucideIcon::Zap,
        QuickCommandIcon::Terminal => LucideIcon::Monitor,
    }
}

pub(super) fn quick_command_icon_label_key(icon: QuickCommandIcon) -> String {
    format!("terminal.quick_commands.icon_{}", icon.as_source_id())
}

impl WorkspaceApp {
    pub(super) fn handle_quick_commands_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.quick_commands.focused_input else {
            return;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        match key {
            "escape" => {
                self.quick_commands.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
            }
            "enter" if input == QuickCommandInput::CategoryName => {
                self.save_quick_command_category_editor(cx);
            }
            "enter"
                if matches!(
                    input,
                    QuickCommandInput::CommandName
                        | QuickCommandInput::CommandText
                        | QuickCommandInput::CommandDescription
                        | QuickCommandInput::CommandHostPattern
                ) =>
            {
                self.save_quick_command_editor(cx);
            }
            "backspace" if !modifiers.platform && !modifiers.control => {
                self.quick_command_input_value_mut(input).pop();
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn quick_command_input_value(&self, input: QuickCommandInput) -> Option<String> {
        match input {
            QuickCommandInput::Search => Some(self.quick_commands.query.clone()),
            QuickCommandInput::CommandName => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.name.clone()),
            QuickCommandInput::CommandText => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.command.clone()),
            QuickCommandInput::CommandDescription => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.description.clone()),
            QuickCommandInput::CommandHostPattern => self
                .quick_commands
                .command_editor
                .as_ref()
                .map(|draft| draft.host_pattern.clone()),
            QuickCommandInput::CategoryName => self
                .quick_commands
                .category_editor
                .as_ref()
                .map(|draft| draft.name.clone()),
        }
    }

    pub(super) fn quick_command_input_value_mut(
        &mut self,
        input: QuickCommandInput,
    ) -> &mut String {
        match input {
            QuickCommandInput::Search => &mut self.quick_commands.query,
            QuickCommandInput::CommandName => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .name
            }
            QuickCommandInput::CommandText => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .command
            }
            QuickCommandInput::CommandDescription => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .description
            }
            QuickCommandInput::CommandHostPattern => {
                &mut self
                    .quick_commands
                    .command_editor
                    .as_mut()
                    .expect("quick command editor is open")
                    .host_pattern
            }
            QuickCommandInput::CategoryName => {
                &mut self
                    .quick_commands
                    .category_editor
                    .as_mut()
                    .expect("quick command category editor is open")
                    .name
            }
        }
    }

    pub(super) fn render_quick_commands_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active_label = self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_default();
        let visible_commands = self
            .quick_commands
            .visible_commands_for_targets(&[active_label]);
        let mut popover = div()
            .absolute()
            .bottom(px(56.0))
            .right(px(12.0))
            .w(px(860.0))
            .max_w(px(860.0))
            .max_h(px(520.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_elevated << 8) | 0xf2))
            .shadow_lg()
            .flex()
            .text_size(px(12.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            });

        let sidebar = self.render_quick_command_category_sidebar(cx);
        let body = self.render_quick_command_body(visible_commands, cx);
        popover = popover.child(sidebar).child(body);
        popover.into_any_element()
    }

    fn render_quick_command_category_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut sidebar = div()
            .w(px(160.0))
            .flex_none()
            .overflow_hidden()
            .rounded_l(px(self.tokens.radii.lg))
            .border_r_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x73))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .mb(px(2.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("terminal.quick_commands.title").to_uppercase()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                quick_command_icon_button(&self.tokens, LucideIcon::Plus)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.start_quick_command_category_create(cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                quick_command_icon_button(&self.tokens, LucideIcon::X)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.terminal_quick_commands_open = false;
                                            this.terminal_quick_command_pending = None;
                                            this.quick_commands.focused_input = None;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    ),
            );

        for category in &self.quick_commands.categories {
            let category_id = category.id.clone();
            let active = category.id == self.quick_commands.active_category;
            let count = self
                .quick_commands
                .commands
                .iter()
                .filter(|command| command.category == category.id)
                .count();
            let can_delete = !default_quick_command_categories()
                .iter()
                .any(|default| default.id == category.id)
                && count == 0;
            sidebar = sidebar.child(
                div()
                    .group("quick-command-category")
                    .rounded(px(self.tokens.radii.md))
                    .px(px(8.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1f)
                    } else {
                        rgba(0x00000000)
                    })
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .hover(move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener({
                                    let category_id = category_id.clone();
                                    move |this, _event, _window, cx| {
                                        this.quick_commands.active_category = category_id.clone();
                                        this.quick_commands.command_editor = None;
                                        this.quick_commands.category_editor = None;
                                        this.quick_commands.focused_input = None;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                quick_command_lucide_icon(category.icon),
                                14.0,
                                if active {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                            ))
                            .child(div().flex_1().truncate().child(category.name.clone()))
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.md))
                                    .bg(rgb(theme.bg_panel))
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(count.to_string()),
                            ),
                    )
                    .child(
                        quick_command_mini_button(&self.tokens, LucideIcon::Pencil).on_mouse_down(
                            MouseButton::Left,
                            cx.listener({
                                let category = category.clone();
                                move |this, _event, _window, cx| {
                                    this.start_quick_command_category_edit(category.clone(), cx);
                                    cx.stop_propagation();
                                }
                            }),
                        ),
                    )
                    .when(can_delete, |row| {
                        row.child(
                            quick_command_mini_button(&self.tokens, LucideIcon::Trash2)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener({
                                        let category_id = category_id.clone();
                                        move |this, _event, _window, cx| {
                                            this.quick_commands.delete_category(&category_id);
                                            cx.stop_propagation();
                                            cx.notify();
                                        }
                                    }),
                                ),
                        )
                    }),
            );
        }

        sidebar
            .child(div().flex_1())
            .when_some(
                self.quick_commands.last_persist_error.as_ref(),
                |sidebar, error| {
                    sidebar.child(
                        div()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgba(0xef444480))
                            .bg(rgba(0xef44441a))
                            .p(px(6.0))
                            .text_size(px(10.0))
                            .text_color(rgba(0xfca5a5ff))
                            .child(error.clone()),
                    )
                },
            )
            .into_any_element()
    }

    fn render_quick_command_body(
        &self,
        visible_commands: Vec<QuickCommand>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded_r(px(self.tokens.radii.lg))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x99))
                    .p(px(8.0))
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::Search,
                            self.quick_commands.query.clone(),
                            self.i18n.t("terminal.quick_commands.search_placeholder"),
                            cx,
                        ),
                    ))
                    .child(
                        div()
                            .h(px(32.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0x99))
                            .cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| {
                                style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.start_quick_command_create(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::Plus,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            .child(self.i18n.t("terminal.quick_commands.add")),
                    ),
            )
            .when_some(self.quick_commands.category_editor.as_ref(), |body, _| {
                body.child(self.render_quick_command_category_editor(cx))
            })
            .when_some(self.quick_commands.command_editor.as_ref(), |body, _| {
                body.child(self.render_quick_command_editor(cx))
            })
            .child(self.render_quick_command_rows(visible_commands, cx))
            .into_any_element()
    }

    fn render_quick_command_rows(
        &self,
        visible_commands: Vec<QuickCommand>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if visible_commands.is_empty() {
            return div()
                .h(px(180.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_color(rgb(theme.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::Zap,
                    20.0,
                    rgb(theme.text_muted),
                ))
                .child(if self.quick_commands.query.trim().is_empty() {
                    self.i18n.t("terminal.quick_commands.empty_category")
                } else {
                    self.i18n.t("terminal.quick_commands.empty_search")
                })
                .into_any_element();
        }

        let mut list = div()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0));
        for command in visible_commands {
            list = list.child(self.render_quick_command_row(command, cx));
        }
        list.into_any_element()
    }

    fn render_quick_command_row(
        &self,
        command: QuickCommand,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let risk = classify_command_risk(&command.command);
        let command_for_insert = command.command.clone();
        let command_for_run = command.command.clone();
        let command_for_edit = command.clone();
        let command_id = command.id.clone();
        div()
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(rgb(theme.text_muted))
            .hover(move |style| {
                style
                    .bg(rgba((theme.bg_hover << 8) | 0xb3))
                    .text_color(rgb(theme.text))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.terminal_command_bar_draft = command_for_insert.clone();
                            this.terminal_command_bar_focused = true;
                            this.terminal_quick_commands_open = false;
                            this.quick_commands.focused_input = None;
                            window.focus(&this.focus_handle);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(command.name.clone()),
                            )
                            .when_some(risk, |row, risk: &'static str| {
                                row.child(
                                    div()
                                        .rounded(px(self.tokens.radii.md))
                                        .px(px(6.0))
                                        .py(px(1.0))
                                        .text_size(px(10.0))
                                        .text_color(if risk == "high" {
                                            rgba(0xfca5a5ff)
                                        } else {
                                            rgba(0xfcd34dff)
                                        })
                                        .bg(if risk == "high" {
                                            rgba(0xef444426)
                                        } else {
                                            rgba(0xf59e0b26)
                                        })
                                        .child(risk.to_uppercase()),
                                )
                            })
                            .when_some(command.host_pattern.as_ref(), |row, pattern| {
                                row.child(
                                    div()
                                        .rounded(px(self.tokens.radii.md))
                                        .px(px(6.0))
                                        .py(px(1.0))
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme.text_muted))
                                        .bg(rgb(theme.bg_panel))
                                        .child(pattern.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .text_color(rgba((theme.accent << 8) | 0xd9))
                            .child(command.command.clone()),
                    )
                    .when_some(command.description.as_ref(), |row, description| {
                        row.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(rgba((theme.text_muted << 8) | 0xb3))
                                .child(description.clone()),
                        )
                    }),
            )
            .child(
                quick_command_action_button(&self.tokens, LucideIcon::Play).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.run_quick_command(&command_for_run, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                quick_command_action_button(&self.tokens, LucideIcon::Pencil).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.start_quick_command_edit(command_for_edit.clone(), cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                quick_command_action_button(&self.tokens, LucideIcon::Trash2).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.quick_commands.delete_command(&command_id);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_quick_command_category_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = self.quick_commands.category_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = !draft.name.trim().is_empty();
        let mut icon_options = div().flex().items_center().gap(px(4.0));
        for icon in [
            QuickCommandIcon::Terminal,
            QuickCommandIcon::Server,
            QuickCommandIcon::Folder,
            QuickCommandIcon::Docker,
            QuickCommandIcon::Zap,
        ] {
            let active = draft.icon == icon;
            icon_options = icon_options.child(
                div()
                    .h(px(30.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(draft) = this.quick_commands.category_editor.as_mut() {
                                draft.icon = icon;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        quick_command_lucide_icon(icon),
                        13.0,
                        if active {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .child(self.i18n.t(&quick_command_icon_label_key(icon))),
            );
        }

        div()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x59))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(8.0))
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CategoryName,
                            draft.name.clone(),
                            self.i18n
                                .t("terminal.quick_commands.group_name_placeholder"),
                            cx,
                        ),
                    )
                    .child(icon_options),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save_group",
                |this, cx| this.save_quick_command_category_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_command_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(draft) = self.quick_commands.command_editor.as_ref() else {
            return div().into_any_element();
        };
        let can_save = !draft.name.trim().is_empty() && !draft.command.trim().is_empty();
        let mut categories = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for category in &self.quick_commands.categories {
            let category_id = category.id.clone();
            let active = draft.category == category.id;
            categories = categories.child(
                div()
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    })
                    .text_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .bg(if active {
                        rgba((theme.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(draft) = this.quick_commands.command_editor.as_mut() {
                                draft.category = category_id.clone();
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(category.name.clone()),
            );
        }

        div()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .bg(rgba((theme.bg << 8) | 0x59))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
                    .gap(px(8.0))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandName,
                        draft.name.clone(),
                        self.i18n.t("terminal.quick_commands.name_placeholder"),
                        cx,
                    ))
                    .child(self.render_quick_command_text_input(
                        QuickCommandInput::CommandText,
                        draft.command.clone(),
                        self.i18n.t("terminal.quick_commands.command_placeholder"),
                        cx,
                    ))
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CommandDescription,
                            draft.description.clone(),
                            self.i18n
                                .t("terminal.quick_commands.description_placeholder"),
                            cx,
                        ),
                    )
                    .child(
                        self.render_quick_command_text_input(
                            QuickCommandInput::CommandHostPattern,
                            draft.host_pattern.clone(),
                            self.i18n
                                .t("terminal.quick_commands.host_pattern_placeholder"),
                            cx,
                        ),
                    )
                    .child(categories),
            )
            .child(self.render_quick_editor_buttons(
                can_save,
                "terminal.quick_commands.save",
                |this, cx| this.save_quick_command_editor(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_quick_editor_buttons(
        &self,
        can_save: bool,
        save_key: &'static str,
        save: fn(&mut WorkspaceApp, &mut Context<WorkspaceApp>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                quick_command_text_button(
                    &self.tokens,
                    self.i18n.t("terminal.quick_commands.cancel"),
                    false,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.quick_commands.command_editor = None;
                        this.quick_commands.category_editor = None;
                        this.quick_commands.focused_input = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            )
            .child(
                quick_command_text_button(&self.tokens, self.i18n.t(save_key), can_save)
                    .bg(if can_save {
                        rgba((theme.accent << 8) | 0x26)
                    } else {
                        rgba(0x00000000)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if can_save {
                                save(this, cx);
                            }
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_quick_command_text_input(
        &self,
        input: QuickCommandInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.quick_commands.focused_input == Some(input);
        let target = WorkspaceImeTarget::QuickCommand(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(32.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.quick_commands.focused_input = Some(input);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn start_quick_command_create(&mut self, cx: &mut Context<Self>) {
        self.quick_commands.category_editor = None;
        self.quick_commands.command_editor = Some(QuickCommandDraft {
            id: None,
            name: String::new(),
            command: String::new(),
            category: self.quick_commands.active_category.clone(),
            description: String::new(),
            host_pattern: String::new(),
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CommandName);
        cx.notify();
    }

    fn start_quick_command_edit(&mut self, command: QuickCommand, cx: &mut Context<Self>) {
        self.quick_commands.category_editor = None;
        self.quick_commands.command_editor = Some(QuickCommandDraft {
            id: Some(command.id),
            name: command.name,
            command: command.command,
            category: command.category,
            description: command.description.unwrap_or_default(),
            host_pattern: command.host_pattern.unwrap_or_default(),
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CommandName);
        cx.notify();
    }

    fn start_quick_command_category_create(&mut self, cx: &mut Context<Self>) {
        self.quick_commands.command_editor = None;
        self.quick_commands.category_editor = Some(QuickCommandCategoryDraft {
            id: None,
            name: String::new(),
            icon: QuickCommandIcon::Zap,
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CategoryName);
        cx.notify();
    }

    fn start_quick_command_category_edit(
        &mut self,
        category: QuickCommandCategory,
        cx: &mut Context<Self>,
    ) {
        self.quick_commands.command_editor = None;
        self.quick_commands.category_editor = Some(QuickCommandCategoryDraft {
            id: Some(category.id),
            name: category.name,
            icon: category.icon,
        });
        self.quick_commands.focused_input = Some(QuickCommandInput::CategoryName);
        cx.notify();
    }

    fn save_quick_command_editor(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.quick_commands.command_editor.take() else {
            return;
        };
        self.quick_commands.upsert_command(draft);
        self.quick_commands.focused_input = None;
        cx.notify();
    }

    fn save_quick_command_category_editor(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.quick_commands.category_editor.take() else {
            return;
        };
        self.quick_commands.upsert_category(draft);
        self.quick_commands.focused_input = None;
        cx.notify();
    }
}

fn quick_command_icon_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    div()
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.sm))
        .cursor_pointer()
        .text_color(rgb(tokens.ui.text_muted))
        .hover({
            let theme = tokens.ui;
            move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
        })
        .child(WorkspaceApp::render_lucide_icon(
            icon,
            14.0,
            rgb(tokens.ui.text_muted),
        ))
}

fn quick_command_mini_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    div()
        .size(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.sm))
        .cursor_pointer()
        .text_color(rgb(tokens.ui.text_muted))
        .hover({
            let theme = tokens.ui;
            move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.accent))
        })
        .child(WorkspaceApp::render_lucide_icon(
            icon,
            12.0,
            rgb(tokens.ui.text_muted),
        ))
}

fn quick_command_action_button(tokens: &ThemeTokens, icon: LucideIcon) -> gpui::Div {
    div()
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .cursor_pointer()
        .text_color(rgb(tokens.ui.text_muted))
        .hover({
            let theme = tokens.ui;
            move |style| style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
        })
        .child(WorkspaceApp::render_lucide_icon(
            icon,
            14.0,
            rgb(tokens.ui.text_muted),
        ))
}

fn quick_command_text_button(tokens: &ThemeTokens, label: String, enabled: bool) -> gpui::Div {
    div()
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .text_color(if enabled {
            rgb(tokens.ui.text)
        } else {
            rgba((tokens.ui.text_muted << 8) | 0x80)
        })
        .when(enabled, |button| {
            let theme = tokens.ui;
            button
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.bg_hover)))
        })
        .child(label)
}

#[cfg(test)]
mod quick_command_tests {
    use super::*;

    fn temp_settings_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oxideterm-quick-commands-{name}-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn upsert_command_persists_to_quick_commands_json() {
        let settings_path = temp_settings_path("persist");
        let mut state = QuickCommandsState::load(&settings_path);
        state.upsert_command(QuickCommandDraft {
            id: None,
            name: "List root".to_string(),
            command: "ls /".to_string(),
            category: "files".to_string(),
            description: "root listing".to_string(),
            host_pattern: String::new(),
        });

        let reloaded = QuickCommandsState::load(&settings_path);
        assert!(reloaded.commands.iter().any(|command| {
            command.name == "List root"
                && command.command == "ls /"
                && command.description.as_deref() == Some("root listing")
        }));
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn default_categories_cannot_be_deleted_while_custom_empty_categories_can() {
        let settings_path = temp_settings_path("delete-category");
        let mut state = QuickCommandsState::load(&settings_path);
        assert!(!state.delete_category("system"));
        let custom = state.upsert_category(QuickCommandCategoryDraft {
            id: None,
            name: "Ops".to_string(),
            icon: QuickCommandIcon::Zap,
        });
        assert!(state.delete_category(&custom));
        assert!(
            !state
                .categories
                .iter()
                .any(|category| category.id == custom)
        );
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn upsert_category_allows_multiple_user_custom_groups() {
        let settings_path = temp_settings_path("multiple-custom-groups");
        let mut state = QuickCommandsState::load(&settings_path);

        let first = state.upsert_category(QuickCommandCategoryDraft {
            id: None,
            name: "Custom".to_string(),
            icon: QuickCommandIcon::Zap,
        });
        let second = state.upsert_category(QuickCommandCategoryDraft {
            id: None,
            name: "Custom".to_string(),
            icon: QuickCommandIcon::Zap,
        });

        assert_ne!(first, second);
        assert_ne!(first, "custom");
        assert_ne!(second, "custom");
        assert_eq!(state.active_category, second);
        assert_eq!(
            state
                .categories
                .iter()
                .filter(|category| category.name == "Custom")
                .count(),
            3
        );

        let reloaded = QuickCommandsState::load(&settings_path);
        assert!(
            reloaded
                .categories
                .iter()
                .any(|category| category.id == first)
        );
        assert!(
            reloaded
                .categories
                .iter()
                .any(|category| category.id == second)
        );
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }

    #[test]
    fn import_snapshot_rename_preserves_conflicting_existing_command() {
        let settings_path = temp_settings_path("import-rename");
        let mut state = QuickCommandsState::load(&settings_path);
        let snapshot = QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: vec![QuickCommandCategory {
                id: "files".to_string(),
                name: "Files".to_string(),
                icon: QuickCommandIcon::Folder,
            }],
            commands: vec![QuickCommand {
                id: "qc-ls-la".to_string(),
                name: "List Files".to_string(),
                command: "exa -la".to_string(),
                category: "files".to_string(),
                description: None,
                host_pattern: None,
                created_at: 1,
                updated_at: 1,
            }],
            updated_at: 1,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let result = state.apply_snapshot_json(&json, QuickCommandImportStrategy::Rename);

        assert_eq!(result.errors, Vec::<String>::new());
        assert!(result.imported > 0);
        assert!(
            state
                .commands
                .iter()
                .any(|command| command.command == "ls -la")
        );
        assert!(
            state
                .commands
                .iter()
                .any(|command| command.command == "exa -la")
        );
        let _ = fs::remove_dir_all(settings_path.parent().unwrap());
    }
}

fn match_quick_command_host_pattern(pattern: Option<&str>, target_fields: &[String]) -> bool {
    let Some(pattern) = pattern.map(str::trim).filter(|pattern| !pattern.is_empty()) else {
        return true;
    };
    let pattern = pattern.to_lowercase();
    target_fields.iter().any(|field| {
        let field = field.to_lowercase();
        wildcard_match(&pattern, &field)
    })
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }
    let mut cursor = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[cursor..].find(part) else {
            return false;
        };
        if index == 0 && found != 0 {
            return false;
        }
        cursor += found + part.len();
    }
    pattern.ends_with('*') || parts.last().is_none_or(|last| value.ends_with(last))
}
