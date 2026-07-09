// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::model::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandCategory, QuickCommandIcon,
    QuickCommandImportResult, QuickCommandImportStrategy, QuickCommandsSnapshot,
};

const QUICK_COMMANDS_FILENAME: &str = "quick-commands.json";
const MAX_QUICK_COMMANDS_FILE_BYTES: u64 = 512 * 1024;
const MAX_CATEGORIES: usize = 100;
const MAX_COMMANDS: usize = 1000;
const MAX_ID_LEN: usize = 128;
const MAX_NAME_LEN: usize = 160;
const MAX_COMMAND_LEN: usize = 4096;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_HOST_PATTERN_LEN: usize = 256;
const MAX_ATOMIC_TEMP_ATTEMPTS: usize = 128;
const BUILTIN_CATEGORY_IDS: &[&str] = &["system", "network", "files", "docker", "custom"];
static QUICK_COMMAND_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
static ATOMIC_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_ATOMIC_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_NEXT_CHECKPOINT_REMOVAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// An opaque copy of the exact Quick Commands file state used for rollback.
pub struct QuickCommandsCheckpoint {
    state: QuickCommandsCheckpointState,
}

enum QuickCommandsCheckpointState {
    Missing,
    Present(Vec<u8>),
}

pub fn quick_commands_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(QUICK_COMMANDS_FILENAME)
}

pub fn export_snapshot_json(settings_path: &Path) -> Result<String, String> {
    let path = quick_commands_path(settings_path);
    let snapshot = load_snapshot_from_path(&path)?.unwrap_or_else(default_snapshot);
    serde_json::to_string_pretty(&snapshot).map_err(|error| error.to_string())
}

pub fn load_snapshot(settings_path: &Path) -> Result<QuickCommandsSnapshot, String> {
    let path = quick_commands_path(settings_path);
    load_snapshot_from_path(&path).map(|snapshot| snapshot.unwrap_or_else(default_snapshot))
}

pub fn save_snapshot(settings_path: &Path, snapshot: &QuickCommandsSnapshot) -> Result<(), String> {
    let path = quick_commands_path(settings_path);
    save_snapshot_to_path(&path, snapshot)
}

/// Captures whether the Quick Commands file exists and its complete contents.
pub fn capture_checkpoint(settings_path: &Path) -> Result<QuickCommandsCheckpoint, String> {
    let path = quick_commands_path(settings_path);
    let state = match fs::metadata(&path) {
        Ok(metadata) => {
            if metadata.len() > MAX_QUICK_COMMANDS_FILE_BYTES {
                return Err("Quick Commands file exceeds size limit".to_string());
            }
            let bytes = fs::read(&path)
                .map_err(|error| format!("failed to read Quick Commands checkpoint: {error}"))?;
            QuickCommandsCheckpointState::Present(bytes)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            QuickCommandsCheckpointState::Missing
        }
        Err(error) => {
            return Err(format!(
                "failed to stat Quick Commands checkpoint source: {error}"
            ));
        }
    };
    Ok(QuickCommandsCheckpoint { state })
}

/// Restores the exact file state represented by a previously captured checkpoint.
pub fn restore_checkpoint(
    settings_path: &Path,
    checkpoint: &QuickCommandsCheckpoint,
) -> Result<(), String> {
    let path = quick_commands_path(settings_path);
    match &checkpoint.state {
        QuickCommandsCheckpointState::Present(bytes) => atomic_write_file(&path, bytes)
            .map_err(|error| format!("failed to restore Quick Commands checkpoint: {error}")),
        QuickCommandsCheckpointState::Missing => remove_file_if_present(&path)
            .map_err(|error| format!("failed to restore missing Quick Commands state: {error}")),
    }
}

pub fn apply_snapshot_json(
    settings_path: &Path,
    snapshot_json: &str,
    strategy: QuickCommandImportStrategy,
) -> QuickCommandImportResult {
    let incoming = serde_json::from_str::<QuickCommandsSnapshot>(snapshot_json)
        .map_err(|error| error.to_string())
        .and_then(sanitize_snapshot);
    let Ok(incoming) = incoming else {
        return QuickCommandImportResult {
            imported: 0,
            skipped: 0,
            errors: vec![
                incoming
                    .err()
                    .unwrap_or_else(|| "invalid snapshot".to_string()),
            ],
        };
    };
    let path = quick_commands_path(settings_path);
    let current = load_snapshot_from_path(&path)
        .ok()
        .flatten()
        .unwrap_or_else(default_snapshot);
    let merge = merge_snapshot(&current, incoming, strategy);
    if let Err(error) = save_snapshot_to_path(&path, &merge.snapshot) {
        return QuickCommandImportResult {
            imported: 0,
            skipped: merge.skipped,
            errors: vec![error],
        };
    }
    QuickCommandImportResult {
        imported: merge.imported,
        skipped: merge.skipped,
        errors: Vec::new(),
    }
}

fn default_snapshot() -> QuickCommandsSnapshot {
    QuickCommandsSnapshot {
        version: QUICK_COMMANDS_SCHEMA_VERSION,
        categories: default_quick_command_categories(),
        commands: default_quick_commands(),
        updated_at: now_ms(),
    }
}

fn load_snapshot_from_path(path: &Path) -> Result<Option<QuickCommandsSnapshot>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to stat Quick Commands file: {error}")),
    };
    if metadata.len() > MAX_QUICK_COMMANDS_FILE_BYTES {
        return Err("Quick Commands file exceeds size limit".to_string());
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read Quick Commands file: {error}"))?;
    if contents.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<QuickCommandsSnapshot>(&contents)
        .map_err(|error| format!("failed to parse Quick Commands file: {error}"))
        .and_then(sanitize_snapshot)
        .map(Some)
}

fn save_snapshot_to_path(path: &Path, snapshot: &QuickCommandsSnapshot) -> Result<(), String> {
    let snapshot = sanitize_snapshot(snapshot.clone())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create Quick Commands directory: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(&snapshot)
        .map_err(|error| format!("failed to serialize Quick Commands: {error}"))?;
    if json.len() as u64 > MAX_QUICK_COMMANDS_FILE_BYTES {
        return Err("Quick Commands snapshot exceeds size limit".to_string());
    }
    atomic_write_file(path, &json)
        .map_err(|error| format!("failed to replace Quick Commands file: {error}"))?;
    Ok(())
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic write path has no file name",
        )
    })?;
    let (temp_path, mut temp_file) = (0..MAX_ATOMIC_TEMP_ATTEMPTS)
        .find_map(|_| {
            let sequence = ATOMIC_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let mut temp_name = OsString::from(".");
            temp_name.push(file_name);
            temp_name.push(format!(".{}.{sequence}.tmp", std::process::id()));
            let temp_path = parent.join(temp_name);
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)
            {
                Ok(file) => Some(Ok((temp_path, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()?
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::AlreadyExists, "atomic temp path exhausted")
        })?;

    // The destination changes only after the complete temporary file reaches durable storage.
    let write_result = (|| {
        temp_file.write_all(bytes)?;
        temp_file.flush()?;
        temp_file.sync_all()?;
        drop(temp_file);
        fail_before_atomic_replace_for_tests()?;
        atomic_replace_file(&temp_path, path)?;
        sync_parent_directory(parent)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    fail_before_checkpoint_removal_for_tests()?;
    match fs::remove_file(path) {
        Ok(()) => {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            sync_parent_directory(parent)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other("injected failure before atomic replace"))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn fail_before_checkpoint_removal_for_tests() -> io::Result<()> {
    FAIL_NEXT_CHECKPOINT_REMOVAL.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other(
                "injected failure before checkpoint removal",
            ))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_checkpoint_removal_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn inject_atomic_replace_failure() {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| fail.set(true));
}

#[cfg(test)]
fn inject_checkpoint_removal_failure() {
    FAIL_NEXT_CHECKPOINT_REMOVAL.with(|fail| fail.set(true));
}

#[cfg(not(windows))]
fn atomic_replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> io::Result<()> {
    // MoveFileExW uses WRITE_THROUGH on Windows, which flushes the replacement operation.
    Ok(())
}

struct MergeResult {
    snapshot: QuickCommandsSnapshot,
    imported: usize,
    skipped: usize,
}

fn merge_snapshot(
    current: &QuickCommandsSnapshot,
    incoming: QuickCommandsSnapshot,
    strategy: QuickCommandImportStrategy,
) -> MergeResult {
    let now = now_ms();
    let mut imported = 0;
    let mut skipped = 0;
    let mut categories = current.categories.clone();
    let mut commands = current.commands.clone();
    let mut category_remap = HashMap::new();

    for incoming_category in incoming.categories {
        let conflict = categories
            .iter()
            .find(|category| {
                category.id == incoming_category.id
                    || category
                        .name
                        .trim()
                        .eq_ignore_ascii_case(incoming_category.name.trim())
            })
            .cloned();
        match (conflict, strategy) {
            (None, _) => {
                category_remap.insert(incoming_category.id.clone(), incoming_category.id.clone());
                categories.push(incoming_category);
            }
            (Some(conflict), QuickCommandImportStrategy::Rename)
                if is_builtin_category_id(&incoming_category.id) =>
            {
                // Built-in category ids are stable containers, not importable user records.
                // Reusing the local container prevents .oxide round-trips from creating
                // duplicate System/Network/Files groups when the global strategy is Rename.
                category_remap.insert(incoming_category.id, conflict.id);
                skipped += 1;
            }
            (Some(conflict), QuickCommandImportStrategy::Skip) => {
                category_remap.insert(incoming_category.id, conflict.id);
                skipped += 1;
            }
            (Some(_), QuickCommandImportStrategy::Rename) => {
                let renamed = QuickCommandCategory {
                    id: new_quick_category_id(),
                    name: unique_category_name(
                        &categories,
                        &format!("{} (Imported)", incoming_category.name),
                    ),
                    icon: incoming_category.icon,
                };
                category_remap.insert(incoming_category.id, renamed.id.clone());
                categories.push(renamed);
                imported += 1;
            }
            (
                Some(conflict),
                QuickCommandImportStrategy::Replace | QuickCommandImportStrategy::Merge,
            ) => {
                category_remap.insert(incoming_category.id, conflict.id.clone());
                for category in &mut categories {
                    if category.id == conflict.id {
                        category.name = incoming_category.name.clone();
                        category.icon = incoming_category.icon;
                    }
                }
                imported += 1;
            }
        }
    }

    let category_ids = categories
        .iter()
        .map(|category| category.id.clone())
        .collect::<HashSet<_>>();
    for mut incoming_command in incoming.commands {
        incoming_command.category = category_remap
            .get(&incoming_command.category)
            .cloned()
            .unwrap_or(incoming_command.category);
        if !category_ids.contains(&incoming_command.category) {
            incoming_command.category = "custom".to_string();
        }
        let conflict = commands
            .iter()
            .find(|command| {
                command.id == incoming_command.id
                    || (command.category == incoming_command.category
                        && command
                            .name
                            .trim()
                            .eq_ignore_ascii_case(incoming_command.name.trim()))
            })
            .cloned();
        match (conflict, strategy) {
            (None, _) => {
                commands.push(incoming_command);
                imported += 1;
            }
            (Some(_), QuickCommandImportStrategy::Skip) => skipped += 1,
            (Some(conflict), QuickCommandImportStrategy::Rename)
                if same_command_content(&conflict, &incoming_command) =>
            {
                // Rename preserves distinct user commands, but exact snapshot round-trips
                // should not duplicate the same command under a reused built-in category.
                skipped += 1;
            }
            (Some(_), QuickCommandImportStrategy::Rename) => {
                incoming_command.id = new_quick_command_id();
                incoming_command.name = unique_command_name(
                    &commands,
                    &incoming_command.category,
                    &format!("{} (Imported)", incoming_command.name),
                );
                commands.push(incoming_command);
                imported += 1;
            }
            (
                Some(conflict),
                QuickCommandImportStrategy::Replace | QuickCommandImportStrategy::Merge,
            ) => {
                for command in &mut commands {
                    if command.id == conflict.id {
                        let created_at = if matches!(strategy, QuickCommandImportStrategy::Merge) {
                            conflict.created_at
                        } else {
                            incoming_command.created_at
                        };
                        *command = QuickCommand {
                            id: conflict.id.clone(),
                            created_at,
                            updated_at: now,
                            ..incoming_command.clone()
                        };
                    }
                }
                imported += 1;
            }
        }
    }

    MergeResult {
        snapshot: QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories,
            commands,
            updated_at: now,
        },
        imported,
        skipped,
    }
}

fn is_builtin_category_id(id: &str) -> bool {
    BUILTIN_CATEGORY_IDS.contains(&id)
}

fn same_command_content(a: &QuickCommand, b: &QuickCommand) -> bool {
    a.id == b.id
        && a.name.trim() == b.name.trim()
        && a.command.trim() == b.command.trim()
        && a.category == b.category
        && a.description.as_deref().map(str::trim) == b.description.as_deref().map(str::trim)
        && a.host_pattern.as_deref().map(str::trim) == b.host_pattern.as_deref().map(str::trim)
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

pub fn default_quick_command_categories() -> Vec<QuickCommandCategory> {
    vec![
        quick_category("system", "System", QuickCommandIcon::Server),
        quick_category("network", "Network", QuickCommandIcon::Terminal),
        quick_category("files", "Files", QuickCommandIcon::Folder),
        quick_category("docker", "Docker", QuickCommandIcon::Docker),
        quick_category("custom", "Custom", QuickCommandIcon::Zap),
    ]
}

pub fn default_quick_commands() -> Vec<QuickCommand> {
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

fn unique_category_name(categories: &[QuickCommandCategory], desired_name: &str) -> String {
    let existing = categories
        .iter()
        .map(|category| category.name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    unique_name(desired_name, &existing)
}

fn unique_command_name(commands: &[QuickCommand], category: &str, desired_name: &str) -> String {
    let existing = commands
        .iter()
        .filter(|command| command.category == category)
        .map(|command| command.name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    unique_name(desired_name, &existing)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oxideterm-quick-commands-{name}-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn export_uses_defaults_when_file_is_missing() {
        let settings_path = temp_settings_path("defaults");
        let json = export_snapshot_json(&settings_path).unwrap();
        let snapshot = serde_json::from_str::<QuickCommandsSnapshot>(&json).unwrap();

        assert_eq!(snapshot.version, QUICK_COMMANDS_SCHEMA_VERSION);
        assert!(!snapshot.categories.is_empty());
        assert!(!snapshot.commands.is_empty());
    }

    #[test]
    fn apply_snapshot_persists_imported_commands() {
        let settings_path = temp_settings_path("apply");
        let incoming = QuickCommandsSnapshot {
            version: QUICK_COMMANDS_SCHEMA_VERSION,
            categories: vec![quick_category("ops", "Ops", QuickCommandIcon::Zap)],
            commands: vec![quick_command(
                "ops-uptime",
                "Ops Uptime",
                "uptime",
                "ops",
                "Check uptime",
            )],
            updated_at: 1,
        };
        let json = serde_json::to_string(&incoming).unwrap();

        let result = apply_snapshot_json(&settings_path, &json, QuickCommandImportStrategy::Merge);
        let exported = export_snapshot_json(&settings_path).unwrap();

        assert!(result.imported > 0);
        assert!(exported.contains("Ops Uptime"));
    }

    #[test]
    fn failed_atomic_save_preserves_existing_file() {
        let settings_path = temp_settings_path("atomic-existing");
        let path = quick_commands_path(&settings_path);
        let mut snapshot = default_snapshot();
        save_snapshot(&settings_path, &snapshot).unwrap();
        let previous = fs::read(&path).unwrap();
        snapshot.updated_at = snapshot.updated_at.saturating_add(1);
        inject_atomic_replace_failure();

        assert!(save_snapshot(&settings_path, &snapshot).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_no_temporary_files(path.parent().unwrap());
    }

    #[test]
    fn failed_atomic_save_preserves_missing_file_state() {
        let settings_path = temp_settings_path("atomic-missing");
        let path = quick_commands_path(&settings_path);
        inject_atomic_replace_failure();

        assert!(save_snapshot(&settings_path, &default_snapshot()).is_err());
        assert!(!path.exists());
        assert_no_temporary_files(path.parent().unwrap());
    }

    #[test]
    fn checkpoint_restores_exact_present_file_contents() {
        let settings_path = temp_settings_path("checkpoint-present");
        let path = quick_commands_path(&settings_path);
        let original = b"{ not a parsed snapshot, but exact persisted state }";
        fs::write(&path, original).unwrap();
        let checkpoint = capture_checkpoint(&settings_path).unwrap();
        fs::write(&path, b"replacement").unwrap();

        restore_checkpoint(&settings_path, &checkpoint).unwrap();

        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn present_checkpoint_restore_recreates_removed_parent_directory() {
        let settings_path = temp_settings_path("checkpoint-parent");
        let path = quick_commands_path(&settings_path);
        let original = b"checkpoint contents";
        fs::write(&path, original).unwrap();
        let checkpoint = capture_checkpoint(&settings_path).unwrap();
        fs::remove_dir_all(path.parent().unwrap()).unwrap();

        restore_checkpoint(&settings_path, &checkpoint).unwrap();

        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn checkpoint_restores_missing_file_state() {
        let settings_path = temp_settings_path("checkpoint-missing");
        let path = quick_commands_path(&settings_path);
        let checkpoint = capture_checkpoint(&settings_path).unwrap();
        save_snapshot(&settings_path, &default_snapshot()).unwrap();

        restore_checkpoint(&settings_path, &checkpoint).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn failed_present_checkpoint_restore_preserves_current_file() {
        let settings_path = temp_settings_path("checkpoint-present-failure");
        let path = quick_commands_path(&settings_path);
        fs::write(&path, b"checkpoint").unwrap();
        let checkpoint = capture_checkpoint(&settings_path).unwrap();
        let current = b"current state";
        fs::write(&path, current).unwrap();
        inject_atomic_replace_failure();

        assert!(restore_checkpoint(&settings_path, &checkpoint).is_err());
        assert_eq!(fs::read(&path).unwrap(), current);
        assert_no_temporary_files(path.parent().unwrap());
    }

    #[test]
    fn failed_missing_checkpoint_restore_preserves_current_file() {
        let settings_path = temp_settings_path("checkpoint-missing-failure");
        let path = quick_commands_path(&settings_path);
        let checkpoint = capture_checkpoint(&settings_path).unwrap();
        let current = b"current state";
        fs::write(&path, current).unwrap();
        inject_checkpoint_removal_failure();

        assert!(restore_checkpoint(&settings_path, &checkpoint).is_err());
        assert_eq!(fs::read(&path).unwrap(), current);
    }

    #[test]
    fn rename_import_does_not_duplicate_builtin_roundtrip_records() {
        let source_settings_path = temp_settings_path("roundtrip-source");
        let target_settings_path = temp_settings_path("roundtrip-target");
        let json = export_snapshot_json(&source_settings_path).unwrap();

        let result = apply_snapshot_json(
            &target_settings_path,
            &json,
            QuickCommandImportStrategy::Rename,
        );
        let exported = export_snapshot_json(&target_settings_path).unwrap();
        let snapshot = serde_json::from_str::<QuickCommandsSnapshot>(&exported).unwrap();

        assert_eq!(result.errors, Vec::<String>::new());
        assert_eq!(result.imported, 0);
        assert_eq!(
            snapshot.categories.len(),
            default_quick_command_categories().len()
        );
        assert_eq!(snapshot.commands.len(), default_quick_commands().len());
        assert_eq!(
            snapshot
                .categories
                .iter()
                .filter(|category| category.id == "system")
                .count(),
            1
        );

        let _ = fs::remove_dir_all(source_settings_path.parent().unwrap());
        let _ = fs::remove_dir_all(target_settings_path.parent().unwrap());
    }

    fn assert_no_temporary_files(directory: &Path) {
        let has_temporary_file = fs::read_dir(directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        });
        assert!(!has_temporary_file);
    }
}
