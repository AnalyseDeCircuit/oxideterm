// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(test)]
use std::cell::Cell;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ForwardRule, ForwardStatus, ForwardType};

pub const FORWARD_TOMBSTONE_RETENTION_DAYS: i64 = 30;
const MAX_ATOMIC_TEMP_ATTEMPTS: usize = 32;

static ATOMIC_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_ATOMIC_REPLACE: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedForward {
    pub id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_connection_id: Option<String>,
    pub forward_type: ForwardType,
    pub rule: ForwardRule,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    pub auto_start: bool,
    #[serde(default = "persisted_forward_version")]
    pub version: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedPersistedForwardTombstone {
    pub id: String,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedForwardDto {
    pub id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_connection_name: Option<String>,
    pub forward_type: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub auto_start: bool,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedForwardImportRecord {
    pub owner_connection_id: String,
    pub forward_type: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub description: Option<String>,
    pub auto_start: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedForwardSyncRecord {
    pub id: String,
    pub revision: String,
    pub updated_at: String,
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<PersistedForwardDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedForwardsSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    pub records: Vec<SavedForwardSyncRecord>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySavedForwardsSyncSnapshotResult {
    pub applied: usize,
    pub skipped: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SavedForwardError {
    #[error("saved forward not found: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid saved forward sync timestamp '{value}' in {field}: {message}")]
    InvalidTimestamp {
        field: &'static str,
        value: String,
        message: String,
    },
    #[error("unsupported forward type: {0}")]
    UnsupportedForwardType(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedForwardData {
    #[serde(default)]
    forwards: Vec<PersistedForward>,
    #[serde(default)]
    tombstones: Vec<DeletedPersistedForwardTombstone>,
}

#[derive(Clone, Eq, PartialEq)]
enum SavedForwardFileState {
    Missing,
    Present(Vec<u8>),
}

/// Opaque owner checkpoint used to restore every saved-forward record exactly.
#[derive(Clone)]
pub struct SavedForwardCheckpoint {
    data: SavedForwardData,
    file_state: SavedForwardFileState,
}

#[derive(Debug)]
pub struct SavedForwardStore {
    path: PathBuf,
    data: Mutex<SavedForwardData>,
}

impl PersistedForward {
    pub fn new(
        id: String,
        session_id: String,
        owner_connection_id: Option<String>,
        forward_type: ForwardType,
        mut rule: ForwardRule,
        auto_start: bool,
    ) -> Self {
        let now = Utc::now();
        rule.status = ForwardStatus::Stopped;
        Self {
            id,
            session_id,
            owner_connection_id,
            forward_type,
            rule,
            created_at: now,
            updated_at: Some(now),
            auto_start,
            version: persisted_forward_version(),
        }
    }

    pub fn sync_updated_at(&self) -> DateTime<Utc> {
        self.updated_at.unwrap_or(self.created_at)
    }

    pub fn mark_updated(&mut self) {
        self.updated_at = Some(Utc::now());
    }
}

impl SavedForwardStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, SavedForwardError> {
        let path = path.into();
        let data = if path.exists() {
            let source = fs::read_to_string(&path)?;
            serde_json::from_str(&source)?
        } else {
            SavedForwardData::default()
        };
        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Captures all forwards, tombstones, and whether the backing file exists.
    pub fn checkpoint(&self) -> Result<SavedForwardCheckpoint, SavedForwardError> {
        let data = self.lock_data();
        let file_state = match fs::read(&self.path) {
            Ok(bytes) => SavedForwardFileState::Present(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => SavedForwardFileState::Missing,
            Err(error) => return Err(error.into()),
        };
        Ok(SavedForwardCheckpoint {
            data: data.clone(),
            file_state,
        })
    }

    /// Replaces the complete owner state only after its file state is committed.
    pub fn replace_from_checkpoint(
        &self,
        checkpoint: &SavedForwardCheckpoint,
    ) -> Result<(), SavedForwardError> {
        let mut data = self.lock_data();
        self.persist_checkpoint(checkpoint)?;
        *data = checkpoint.data.clone();
        Ok(())
    }

    /// Restores a prior owner checkpoint for transaction compensation.
    pub fn restore_checkpoint(
        &self,
        checkpoint: &SavedForwardCheckpoint,
    ) -> Result<(), SavedForwardError> {
        self.replace_from_checkpoint(checkpoint)
    }

    pub fn sync_persisted_forward_rule(
        &self,
        forward_id: &str,
        session_id: &str,
        owner_connection_id: Option<String>,
        rule: ForwardRule,
    ) -> Result<Option<PersistedForward>, SavedForwardError> {
        self.commit_update(|data| {
            let saved = if let Some(existing) = data
                .forwards
                .iter_mut()
                .find(|forward| forward.id == forward_id)
            {
                existing.session_id = session_id.to_string();
                if owner_connection_id.is_some() {
                    existing.owner_connection_id = owner_connection_id;
                }
                existing.forward_type = rule.forward_type;
                existing.rule = stopped_rule(rule);
                existing.mark_updated();
                Some(existing.clone())
            } else if let Some(owner_connection_id) = owner_connection_id {
                let forward = PersistedForward::new(
                    forward_id.to_string(),
                    session_id.to_string(),
                    Some(owner_connection_id),
                    rule.forward_type,
                    rule,
                    false,
                );
                data.tombstones
                    .retain(|tombstone| tombstone.id != forward_id);
                data.forwards.push(forward.clone());
                Some(forward)
            } else {
                None
            };
            Ok(saved)
        })
    }

    pub fn persist_forward(&self, forward: PersistedForward) -> Result<(), SavedForwardError> {
        self.commit_update(|data| {
            upsert_forward(&mut data.forwards, forward);
            let forward_ids: HashSet<String> = data
                .forwards
                .iter()
                .map(|forward| forward.id.clone())
                .collect();
            data.tombstones
                .retain(|tombstone| !forward_ids.contains(&tombstone.id));
            Ok(())
        })
    }

    pub fn delete_persisted_forward(&self, forward_id: &str) -> Result<(), SavedForwardError> {
        self.commit_update_if(|data| {
            let Some(index) = data
                .forwards
                .iter()
                .position(|forward| forward.id == forward_id)
            else {
                return Ok(((), false));
            };
            let forward = data.forwards.remove(index);
            if forward.owner_connection_id.is_some() {
                upsert_tombstone(
                    &mut data.tombstones,
                    DeletedPersistedForwardTombstone {
                        id: forward_id.to_string(),
                        deleted_at: Utc::now(),
                    },
                );
            }
            Ok(((), true))
        })
    }

    pub fn update_auto_start(
        &self,
        forward_id: &str,
        auto_start: bool,
    ) -> Result<(), SavedForwardError> {
        self.commit_update(|data| {
            let Some(forward) = data
                .forwards
                .iter_mut()
                .find(|forward| forward.id == forward_id)
            else {
                return Err(SavedForwardError::NotFound(forward_id.to_string()));
            };
            forward.auto_start = auto_start;
            forward.mark_updated();
            Ok(())
        })
    }

    pub fn load_owned_forwards(&self, owner_connection_id: &str) -> Vec<PersistedForward> {
        self.sorted_forwards(|forward| {
            forward.owner_connection_id.as_deref() == Some(owner_connection_id)
        })
    }

    pub fn load_persisted_forwards(&self, session_id: &str) -> Vec<PersistedForward> {
        self.sorted_forwards(|forward| forward.session_id == session_id)
    }

    pub fn load_syncable_forwards(&self) -> Vec<PersistedForward> {
        self.sorted_forwards(|forward| forward.owner_connection_id.is_some())
    }

    pub fn load_sync_state(
        &self,
    ) -> (Vec<PersistedForward>, Vec<DeletedPersistedForwardTombstone>) {
        let data = self.lock_data();
        let mut forwards: Vec<_> = data
            .forwards
            .iter()
            .filter(|forward| forward.owner_connection_id.is_some())
            .cloned()
            .collect();
        let mut tombstones = active_tombstones(&data.tombstones);
        forwards.sort_by_key(|forward| forward.created_at);
        tombstones.sort_by_key(|tombstone| tombstone.deleted_at);
        (forwards, tombstones)
    }

    pub fn bind_owned_forwards_to_session(
        &self,
        owner_connection_id: &str,
        session_id: &str,
    ) -> Result<usize, SavedForwardError> {
        self.commit_update(|data| {
            let mut count = 0;
            for forward in &mut data.forwards {
                if forward.owner_connection_id.as_deref() == Some(owner_connection_id)
                    && forward.session_id != session_id
                {
                    forward.session_id = session_id.to_string();
                    forward.mark_updated();
                    count += 1;
                }
            }
            Ok(count)
        })
    }

    pub fn delete_owned_forwards(
        &self,
        owner_connection_id: &str,
    ) -> Result<usize, SavedForwardError> {
        self.commit_update(|data| {
            let now = Utc::now();
            let mut removed = Vec::new();
            data.forwards.retain(|forward| {
                if forward.owner_connection_id.as_deref() == Some(owner_connection_id) {
                    removed.push(forward.id.clone());
                    false
                } else {
                    true
                }
            });
            let count = removed.len();
            for id in removed {
                upsert_tombstone(
                    &mut data.tombstones,
                    DeletedPersistedForwardTombstone {
                        id,
                        deleted_at: now,
                    },
                );
            }
            Ok(count)
        })
    }

    pub fn apply_owned_forward_import_records(
        &self,
        records: &[OwnedForwardImportRecord],
        replace_owner_connection_ids: &HashSet<String>,
        merge_owner_connection_ids: &HashSet<String>,
    ) -> Result<usize, SavedForwardError> {
        self.commit_update(|data| {
            let SavedForwardData {
                forwards,
                tombstones,
            } = data;
            apply_owned_forward_import_records_locked(
                forwards,
                tombstones,
                records,
                replace_owner_connection_ids,
                merge_owner_connection_ids,
            )?;
            Ok(records.len())
        })
    }

    pub fn export_snapshot(&self) -> Result<SavedForwardsSyncSnapshot, SavedForwardError> {
        let (forwards, tombstones) = self.load_sync_state();
        build_saved_forwards_sync_snapshot(forwards, tombstones)
    }

    pub fn apply_snapshot(
        &self,
        snapshot: SavedForwardsSyncSnapshot,
        valid_owner_connection_ids: &HashSet<String>,
    ) -> Result<ApplySavedForwardsSyncSnapshotResult, SavedForwardError> {
        self.commit_update_if(|data| {
            let existing_by_id: HashMap<String, PersistedForward> = data
                .forwards
                .iter()
                .cloned()
                .map(|forward| (forward.id.clone(), forward))
                .collect();
            let tombstones_by_id: HashMap<String, DeletedPersistedForwardTombstone> = data
                .tombstones
                .iter()
                .cloned()
                .map(|tombstone| (tombstone.id.clone(), tombstone))
                .collect();
            let mut result = ApplySavedForwardsSyncSnapshotResult::default();

            for record in snapshot.records {
                let record_updated_at = parse_sync_timestamp(&record.updated_at, "updated_at")?;
                if record.deleted {
                    if existing_by_id
                        .get(&record.id)
                        .is_some_and(|existing| existing.sync_updated_at() > record_updated_at)
                    {
                        result.skipped += 1;
                        continue;
                    }
                    data.forwards.retain(|forward| forward.id != record.id);
                    upsert_tombstone(
                        &mut data.tombstones,
                        DeletedPersistedForwardTombstone {
                            id: record.id,
                            deleted_at: record_updated_at,
                        },
                    );
                    result.applied += 1;
                    continue;
                }

                let Some(payload) = record.payload else {
                    result.skipped += 1;
                    continue;
                };
                if tombstones_by_id
                    .get(&record.id)
                    .is_some_and(|tombstone| tombstone.deleted_at >= record_updated_at)
                {
                    result.skipped += 1;
                    continue;
                }
                if existing_by_id
                    .get(&record.id)
                    .is_some_and(|existing| existing.sync_updated_at() > record_updated_at)
                {
                    result.skipped += 1;
                    continue;
                }
                let Some(owner_connection_id) = payload.owner_connection_id.as_ref() else {
                    result.skipped += 1;
                    continue;
                };
                if !valid_owner_connection_ids.contains(owner_connection_id) {
                    result.skipped += 1;
                    continue;
                }

                upsert_forward(
                    &mut data.forwards,
                    persisted_forward_from_sync_payload(payload, record_updated_at)?,
                );
                data.tombstones
                    .retain(|tombstone| tombstone.id != record.id);
                result.applied += 1;
            }

            let should_persist = result.applied > 0;
            Ok((result, should_persist))
        })
    }

    fn sorted_forwards(
        &self,
        predicate: impl Fn(&PersistedForward) -> bool,
    ) -> Vec<PersistedForward> {
        let data = self.lock_data();
        let mut forwards: Vec<_> = data
            .forwards
            .iter()
            .filter(|forward| predicate(forward))
            .cloned()
            .collect();
        forwards.sort_by_key(|forward| forward.created_at);
        forwards
    }

    fn lock_data(&self) -> std::sync::MutexGuard<'_, SavedForwardData> {
        self.data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn commit_update<T>(
        &self,
        update: impl FnOnce(&mut SavedForwardData) -> Result<T, SavedForwardError>,
    ) -> Result<T, SavedForwardError> {
        self.commit_update_if(|data| update(data).map(|result| (result, true)))
    }

    fn commit_update_if<T>(
        &self,
        update: impl FnOnce(&mut SavedForwardData) -> Result<(T, bool), SavedForwardError>,
    ) -> Result<T, SavedForwardError> {
        let mut current = self.lock_data();
        let mut candidate = current.clone();
        let (result, should_persist) = update(&mut candidate)?;
        if should_persist {
            candidate.tombstones = active_tombstones(&candidate.tombstones);
            self.persist_data(&candidate)?;
            // Memory becomes visible only after the durable file replacement succeeds.
            *current = candidate;
        }
        Ok(result)
    }

    fn persist_data(&self, data: &SavedForwardData) -> Result<(), SavedForwardError> {
        let json = serde_json::to_vec_pretty(data)?;
        atomic_write_file(&self.path, &json)?;
        Ok(())
    }

    fn persist_checkpoint(
        &self,
        checkpoint: &SavedForwardCheckpoint,
    ) -> Result<(), SavedForwardError> {
        match &checkpoint.file_state {
            SavedForwardFileState::Present(bytes) => {
                atomic_write_file(&self.path, bytes).map_err(Into::into)
            }
            SavedForwardFileState::Missing => {
                fail_before_atomic_replace_for_tests()?;
                match fs::remove_file(&self.path) {
                    Ok(()) => {
                        let parent = parent_directory(&self.path);
                        sync_parent_directory(parent)?;
                        Ok(())
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(error.into()),
                }
            }
        }
    }
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = parent_directory(path);
    fs::create_dir_all(parent)?;
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "saved forward path has no file name",
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

    // The destination remains untouched until the complete temp file is durable.
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

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(not(windows))]
fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    // Unix requires the directory entry itself to be synced after rename or unlink.
    fs::File::open(parent)?.sync_all()
}

#[cfg(windows)]
fn sync_parent_directory(_parent: &Path) -> io::Result<()> {
    // MoveFileExW uses WRITE_THROUGH below, which flushes the replacement operation.
    Ok(())
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
fn inject_atomic_replace_failure() {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| fail.set(true));
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

impl From<PersistedForward> for PersistedForwardDto {
    fn from(forward: PersistedForward) -> Self {
        persisted_forward_to_dto(forward, None)
    }
}

fn persisted_forward_to_dto(
    forward: PersistedForward,
    owner_connection_name: Option<String>,
) -> PersistedForwardDto {
    PersistedForwardDto {
        id: forward.id,
        session_id: forward.session_id,
        owner_connection_id: forward.owner_connection_id,
        owner_connection_name,
        forward_type: forward.forward_type.as_str().to_string(),
        bind_address: forward.rule.bind_address,
        bind_port: forward.rule.bind_port,
        target_host: forward.rule.target_host,
        target_port: forward.rule.target_port,
        auto_start: forward.auto_start,
        created_at: forward.created_at.to_rfc3339(),
        description: (!forward.rule.description.is_empty()).then_some(forward.rule.description),
    }
}

fn build_saved_forward_sync_record(
    forward: PersistedForward,
) -> Result<SavedForwardSyncRecord, SavedForwardError> {
    let updated_at = forward.sync_updated_at().to_rfc3339();
    let payload = persisted_forward_to_dto(forward, None);
    let revision = sha256_hex(&payload)?;
    Ok(SavedForwardSyncRecord {
        id: payload.id.clone(),
        revision,
        updated_at,
        deleted: false,
        payload: Some(payload),
    })
}

fn build_saved_forward_tombstone_record(
    tombstone: &DeletedPersistedForwardTombstone,
) -> Result<SavedForwardSyncRecord, SavedForwardError> {
    let revision = sha256_hex(&(
        tombstone.id.as_str(),
        tombstone.deleted_at.to_rfc3339(),
        true,
    ))?;
    Ok(SavedForwardSyncRecord {
        id: tombstone.id.clone(),
        revision,
        updated_at: tombstone.deleted_at.to_rfc3339(),
        deleted: true,
        payload: None,
    })
}

fn build_saved_forwards_sync_snapshot(
    forwards: Vec<PersistedForward>,
    tombstones: Vec<DeletedPersistedForwardTombstone>,
) -> Result<SavedForwardsSyncSnapshot, SavedForwardError> {
    let mut records: Vec<SavedForwardSyncRecord> = forwards
        .into_iter()
        .map(build_saved_forward_sync_record)
        .collect::<Result<_, _>>()?;
    records.extend(
        tombstones
            .iter()
            .map(build_saved_forward_tombstone_record)
            .collect::<Result<Vec<_>, _>>()?,
    );
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let revision = sha256_hex(
        &records
            .iter()
            .map(|record| (&record.id, &record.revision, record.deleted))
            .collect::<Vec<_>>(),
    )?;
    Ok(SavedForwardsSyncSnapshot {
        revision,
        exported_at: Utc::now().to_rfc3339(),
        records,
    })
}

fn persisted_forward_from_sync_payload(
    payload: PersistedForwardDto,
    record_updated_at: DateTime<Utc>,
) -> Result<PersistedForward, SavedForwardError> {
    let forward_type = ForwardType::try_from_tauri_str(&payload.forward_type)?;
    let created_at = parse_sync_timestamp(&payload.created_at, "created_at")?;
    let id = payload.id.clone();
    Ok(PersistedForward {
        id: id.clone(),
        session_id: String::new(),
        owner_connection_id: payload.owner_connection_id,
        forward_type,
        rule: ForwardRule {
            id,
            forward_type,
            bind_address: payload.bind_address,
            bind_port: payload.bind_port,
            target_host: payload.target_host,
            target_port: payload.target_port,
            status: ForwardStatus::Stopped,
            description: payload.description.unwrap_or_default(),
        },
        created_at,
        updated_at: Some(record_updated_at),
        auto_start: payload.auto_start,
        version: persisted_forward_version(),
    })
}

fn parse_sync_timestamp(
    value: &str,
    field: &'static str,
) -> Result<DateTime<Utc>, SavedForwardError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| SavedForwardError::InvalidTimestamp {
            field,
            value: value.to_string(),
            message: error.to_string(),
        })
}

fn sha256_hex<T: Serialize>(value: &T) -> Result<String, SavedForwardError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ForwardIdentity {
    forward_type: String,
    bind_address: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
}

impl ForwardIdentity {
    fn from_import(record: &OwnedForwardImportRecord) -> Self {
        Self {
            forward_type: record.forward_type.clone(),
            bind_address: record.bind_address.clone(),
            bind_port: record.bind_port,
            target_host: record.target_host.clone(),
            target_port: record.target_port,
        }
    }

    fn from_persisted(forward: &PersistedForward) -> Self {
        Self {
            forward_type: forward.forward_type.as_str().to_string(),
            bind_address: forward.rule.bind_address.clone(),
            bind_port: forward.rule.bind_port,
            target_host: forward.rule.target_host.clone(),
            target_port: forward.rule.target_port,
        }
    }
}

fn apply_owned_forward_import_records_locked(
    forwards: &mut Vec<PersistedForward>,
    tombstones: &mut Vec<DeletedPersistedForwardTombstone>,
    records: &[OwnedForwardImportRecord],
    replace_owner_connection_ids: &HashSet<String>,
    merge_owner_connection_ids: &HashSet<String>,
) -> Result<(), SavedForwardError> {
    let imported_by_owner = records.iter().fold(
        HashMap::<&str, Vec<&OwnedForwardImportRecord>>::new(),
        |mut map, record| {
            map.entry(record.owner_connection_id.as_str())
                .or_default()
                .push(record);
            map
        },
    );
    let now = Utc::now();

    for owner_connection_id in replace_owner_connection_ids {
        let mut removed = Vec::new();
        forwards.retain(|forward| {
            if forward.owner_connection_id.as_deref() == Some(owner_connection_id.as_str()) {
                removed.push(forward.id.clone());
                false
            } else {
                true
            }
        });
        for id in removed {
            upsert_tombstone(
                tombstones,
                DeletedPersistedForwardTombstone {
                    id,
                    deleted_at: now,
                },
            );
        }
    }

    for (owner_connection_id, owner_records) in imported_by_owner {
        let should_merge = merge_owner_connection_ids.contains(owner_connection_id)
            && !replace_owner_connection_ids.contains(owner_connection_id);
        if should_merge {
            merge_imported_owner_forwards(forwards, owner_connection_id, owner_records)?;
        } else {
            for record in owner_records {
                forwards.push(import_record_to_persisted(record, Utc::now())?);
            }
        }
    }

    let forward_ids = forwards
        .iter()
        .map(|forward| forward.id.clone())
        .collect::<HashSet<_>>();
    tombstones.retain(|tombstone| !forward_ids.contains(&tombstone.id));
    Ok(())
}

fn merge_imported_owner_forwards(
    forwards: &mut Vec<PersistedForward>,
    owner_connection_id: &str,
    records: Vec<&OwnedForwardImportRecord>,
) -> Result<(), SavedForwardError> {
    let mut existing_by_identity = forwards
        .iter()
        .enumerate()
        .filter(|(_, forward)| forward.owner_connection_id.as_deref() == Some(owner_connection_id))
        .map(|(index, forward)| (ForwardIdentity::from_persisted(forward), index))
        .collect::<HashMap<_, _>>();
    let merged_at = Utc::now();

    for record in records {
        let identity = ForwardIdentity::from_import(record);
        if let Some(index) = existing_by_identity.get(&identity).copied() {
            if let Some(existing) = forwards.get_mut(index) {
                existing.auto_start = record.auto_start;
                existing.rule.description = record.description.clone().unwrap_or_default();
                existing.owner_connection_id = Some(owner_connection_id.to_string());
                existing.updated_at = Some(merged_at);
            }
        } else {
            let persisted = import_record_to_persisted(record, merged_at)?;
            existing_by_identity.insert(identity, forwards.len());
            forwards.push(persisted);
        }
    }
    Ok(())
}

fn import_record_to_persisted(
    record: &OwnedForwardImportRecord,
    persisted_at: DateTime<Utc>,
) -> Result<PersistedForward, SavedForwardError> {
    let forward_type = ForwardType::try_from_tauri_str(&record.forward_type)?;
    let rule = ForwardRule {
        id: uuid::Uuid::new_v4().to_string(),
        forward_type,
        bind_address: record.bind_address.clone(),
        bind_port: record.bind_port,
        target_host: record.target_host.clone(),
        target_port: record.target_port,
        status: ForwardStatus::Stopped,
        description: record.description.clone().unwrap_or_default(),
    };
    Ok(PersistedForward {
        id: rule.id.clone(),
        session_id: String::new(),
        owner_connection_id: Some(record.owner_connection_id.clone()),
        forward_type,
        rule,
        created_at: persisted_at,
        updated_at: Some(persisted_at),
        auto_start: record.auto_start,
        version: persisted_forward_version(),
    })
}

fn upsert_forward(forwards: &mut Vec<PersistedForward>, forward: PersistedForward) {
    if let Some(existing) = forwards
        .iter_mut()
        .find(|existing| existing.id == forward.id)
    {
        *existing = forward;
    } else {
        forwards.push(forward);
    }
}

fn upsert_tombstone(
    tombstones: &mut Vec<DeletedPersistedForwardTombstone>,
    tombstone: DeletedPersistedForwardTombstone,
) {
    if let Some(existing) = tombstones
        .iter_mut()
        .find(|existing| existing.id == tombstone.id)
    {
        if existing.deleted_at < tombstone.deleted_at {
            *existing = tombstone;
        }
    } else {
        tombstones.push(tombstone);
    }
}

fn active_tombstones(
    tombstones: &[DeletedPersistedForwardTombstone],
) -> Vec<DeletedPersistedForwardTombstone> {
    let cutoff = Utc::now() - Duration::days(FORWARD_TOMBSTONE_RETENTION_DAYS);
    tombstones
        .iter()
        .filter(|tombstone| tombstone.deleted_at >= cutoff)
        .cloned()
        .collect()
}

fn stopped_rule(mut rule: ForwardRule) -> ForwardRule {
    rule.status = ForwardStatus::Stopped;
    rule
}

const fn persisted_forward_version() -> u32 {
    1
}

impl ForwardType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Dynamic => "dynamic",
        }
    }

    pub fn try_from_tauri_str(value: &str) -> Result<Self, SavedForwardError> {
        match value {
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            "dynamic" => Ok(Self::Dynamic),
            other => Err(SavedForwardError::UnsupportedForwardType(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rule(id: &str, port: u16) -> ForwardRule {
        let mut rule = ForwardRule::local("127.0.0.1", port, "localhost", 3000);
        rule.id = id.to_string();
        rule.description = "web".to_string();
        rule
    }

    fn import_record(owner: &str, port: u16, description: &str) -> OwnedForwardImportRecord {
        OwnedForwardImportRecord {
            owner_connection_id: owner.to_string(),
            forward_type: "local".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: port,
            target_host: "localhost".to_string(),
            target_port: 3000,
            description: Some(description.to_string()),
            auto_start: true,
        }
    }

    fn persisted_forward(
        id: &str,
        session_id: &str,
        owner_connection_id: Option<&str>,
        port: u16,
    ) -> PersistedForward {
        // Tests use both owner-bound and local-only records to prove checkpoints are complete.
        PersistedForward::new(
            id.to_string(),
            session_id.to_string(),
            owner_connection_id.map(str::to_string),
            ForwardType::Local,
            sample_rule(id, port),
            false,
        )
    }

    #[test]
    fn atomic_save_failure_preserves_file_and_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let store = SavedForwardStore::load(&path).unwrap();
        store
            .persist_forward(persisted_forward(
                "forward-1",
                "session-1",
                Some("connection-1"),
                8080,
            ))
            .unwrap();
        let checkpoint = store.checkpoint().unwrap();
        let original_file = fs::read(&path).unwrap();

        inject_atomic_replace_failure();
        let error = store.update_auto_start("forward-1", true).unwrap_err();

        assert!(matches!(error, SavedForwardError::Io(_)));
        assert_eq!(fs::read(&path).unwrap(), original_file);
        assert_eq!(*store.lock_data(), checkpoint.data);
        assert!(!store.load_persisted_forwards("session-1")[0].auto_start);
    }

    #[test]
    fn checkpoint_restore_preserves_all_records_and_tombstones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let store = SavedForwardStore::load(&path).unwrap();
        store
            .persist_forward(persisted_forward("local-only", "session-local", None, 7000))
            .unwrap();
        store
            .persist_forward(persisted_forward(
                "deleted-owner",
                "session-owner",
                Some("connection-1"),
                8000,
            ))
            .unwrap();
        store.delete_persisted_forward("deleted-owner").unwrap();
        let checkpoint = store.checkpoint().unwrap();

        store
            .persist_forward(persisted_forward(
                "replacement",
                "session-new",
                Some("connection-2"),
                9000,
            ))
            .unwrap();
        store.restore_checkpoint(&checkpoint).unwrap();

        assert_eq!(*store.lock_data(), checkpoint.data);
        assert_eq!(
            serde_json::from_slice::<SavedForwardData>(&fs::read(&path).unwrap()).unwrap(),
            checkpoint.data
        );
        assert_eq!(store.load_persisted_forwards("session-local").len(), 1);
        assert!(
            store
                .lock_data()
                .tombstones
                .iter()
                .any(|tombstone| tombstone.id == "deleted-owner")
        );
    }

    #[test]
    fn checkpoint_restore_preserves_unknown_fields_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let original_file = br#"{
  "forwards": [],
  "tombstones": [],
  "futureField": { "mustSurvive": true }
}
"#;
        fs::write(&path, original_file).unwrap();
        let store = SavedForwardStore::load(&path).unwrap();
        let checkpoint = store.checkpoint().unwrap();

        store
            .persist_forward(persisted_forward("forward-1", "session-1", None, 8080))
            .unwrap();
        assert_ne!(fs::read(&path).unwrap(), original_file);

        store.restore_checkpoint(&checkpoint).unwrap();

        assert_eq!(fs::read(&path).unwrap(), original_file);
        assert_eq!(*store.lock_data(), checkpoint.data);
    }

    #[test]
    fn checkpoint_restore_recovers_missing_file_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let store = SavedForwardStore::load(&path).unwrap();
        let checkpoint = store.checkpoint().unwrap();

        store
            .persist_forward(persisted_forward("forward-1", "session-1", None, 8080))
            .unwrap();
        assert!(path.exists());

        store.replace_from_checkpoint(&checkpoint).unwrap();

        assert!(!path.exists());
        assert_eq!(*store.lock_data(), checkpoint.data);
        assert!(store.load_persisted_forwards("session-1").is_empty());
    }

    #[test]
    fn checkpoint_restore_failure_preserves_current_file_and_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let store = SavedForwardStore::load(&path).unwrap();
        store
            .persist_forward(persisted_forward(
                "forward-1",
                "session-1",
                Some("connection-1"),
                8080,
            ))
            .unwrap();
        let prior_checkpoint = store.checkpoint().unwrap();
        store.update_auto_start("forward-1", true).unwrap();
        let current_data = store.lock_data().clone();
        let current_file = fs::read(&path).unwrap();

        inject_atomic_replace_failure();
        let error = store.restore_checkpoint(&prior_checkpoint).unwrap_err();

        assert!(matches!(error, SavedForwardError::Io(_)));
        assert_eq!(*store.lock_data(), current_data);
        assert_eq!(fs::read(&path).unwrap(), current_file);
        assert!(store.load_persisted_forwards("session-1")[0].auto_start);
    }

    #[test]
    fn sync_new_runtime_forward_requires_owner_like_tauri_node_command() {
        let dir = tempfile::tempdir().unwrap();
        let store = SavedForwardStore::load(dir.path().join("forwards.json")).unwrap();

        let saved = store
            .sync_persisted_forward_rule(
                "forward-1",
                "session-1",
                None,
                sample_rule("forward-1", 8080),
            )
            .unwrap();

        assert!(saved.is_none());
        assert!(store.load_persisted_forwards("session-1").is_empty());
    }

    #[test]
    fn sync_owner_bound_forward_preserves_auto_start_on_update() {
        let dir = tempfile::tempdir().unwrap();
        let store = SavedForwardStore::load(dir.path().join("forwards.json")).unwrap();
        store
            .sync_persisted_forward_rule(
                "forward-1",
                "session-1",
                Some("connection-1".to_string()),
                sample_rule("forward-1", 8080),
            )
            .unwrap();
        store.update_auto_start("forward-1", true).unwrap();
        store
            .sync_persisted_forward_rule(
                "forward-1",
                "session-2",
                Some("connection-1".to_string()),
                sample_rule("forward-1", 9090),
            )
            .unwrap();

        let saved = store.load_owned_forwards("connection-1");
        assert_eq!(saved.len(), 1);
        assert!(saved[0].auto_start);
        assert_eq!(saved[0].session_id, "session-2");
        assert_eq!(saved[0].rule.bind_port, 9090);
    }

    #[test]
    fn delete_owned_forwards_tombstones_saved_connection_rules() {
        let dir = tempfile::tempdir().unwrap();
        let store = SavedForwardStore::load(dir.path().join("forwards.json")).unwrap();
        store
            .sync_persisted_forward_rule(
                "forward-1",
                "node:prod",
                Some("connection-1".to_string()),
                sample_rule("forward-1", 8080),
            )
            .unwrap();
        store
            .sync_persisted_forward_rule(
                "forward-2",
                "node:dev",
                Some("connection-2".to_string()),
                sample_rule("forward-2", 9090),
            )
            .unwrap();

        let deleted = store.delete_owned_forwards("connection-1").unwrap();
        let (_forwards, tombstones) = store.load_sync_state();

        assert_eq!(deleted, 1);
        assert!(store.load_owned_forwards("connection-1").is_empty());
        assert_eq!(store.load_owned_forwards("connection-2").len(), 1);
        assert!(
            tombstones
                .iter()
                .any(|tombstone| tombstone.id == "forward-1")
        );
    }

    #[test]
    fn apply_owned_forward_import_records_replaces_owner_rules() {
        let dir = tempfile::tempdir().unwrap();
        let store = SavedForwardStore::load(dir.path().join("forwards.json")).unwrap();
        store
            .sync_persisted_forward_rule(
                "old-forward",
                "node:prod",
                Some("connection-1".to_string()),
                sample_rule("old-forward", 8080),
            )
            .unwrap();
        let mut replace = HashSet::new();
        replace.insert("connection-1".to_string());

        let imported = store
            .apply_owned_forward_import_records(
                &[import_record("connection-1", 9090, "imported")],
                &replace,
                &HashSet::new(),
            )
            .unwrap();
        let saved = store.load_owned_forwards("connection-1");
        let (_forwards, tombstones) = store.load_sync_state();

        assert_eq!(imported, 1);
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].rule.bind_port, 9090);
        assert_eq!(saved[0].rule.description, "imported");
        assert!(
            tombstones
                .iter()
                .any(|tombstone| tombstone.id == "old-forward")
        );
    }

    #[test]
    fn apply_owned_forward_import_records_merges_by_forward_identity() {
        let dir = tempfile::tempdir().unwrap();
        let store = SavedForwardStore::load(dir.path().join("forwards.json")).unwrap();
        store
            .sync_persisted_forward_rule(
                "forward-1",
                "node:prod",
                Some("connection-1".to_string()),
                sample_rule("forward-1", 8080),
            )
            .unwrap();
        let mut merge = HashSet::new();
        merge.insert("connection-1".to_string());

        store
            .apply_owned_forward_import_records(
                &[
                    import_record("connection-1", 8080, "merged"),
                    import_record("connection-1", 9090, "new"),
                ],
                &HashSet::new(),
                &merge,
            )
            .unwrap();
        let saved = store.load_owned_forwards("connection-1");

        assert_eq!(saved.len(), 2);
        let merged = saved
            .iter()
            .find(|forward| forward.rule.bind_port == 8080)
            .unwrap();
        assert_eq!(merged.id, "forward-1");
        assert_eq!(merged.rule.description, "merged");
        assert!(merged.auto_start);
        assert!(
            saved
                .iter()
                .any(|forward| forward.rule.bind_port == 9090 && forward.rule.description == "new")
        );
    }

    #[test]
    fn export_snapshot_includes_owner_bound_forward_and_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let store = SavedForwardStore::load(dir.path().join("forwards.json")).unwrap();
        store
            .sync_persisted_forward_rule(
                "forward-1",
                "session-1",
                Some("connection-1".to_string()),
                sample_rule("forward-1", 8080),
            )
            .unwrap();
        store.delete_persisted_forward("forward-1").unwrap();

        let snapshot = store.export_snapshot().unwrap();

        assert_eq!(snapshot.records.len(), 1);
        assert!(snapshot.records[0].deleted);
        assert!(snapshot.records[0].payload.is_none());
    }

    #[test]
    fn apply_snapshot_skips_unknown_owner_and_imports_known_owner() {
        let dir = tempfile::tempdir().unwrap();
        let source = SavedForwardStore::load(dir.path().join("source.json")).unwrap();
        source
            .sync_persisted_forward_rule(
                "forward-1",
                "session-1",
                Some("connection-1".to_string()),
                sample_rule("forward-1", 8080),
            )
            .unwrap();
        let snapshot = source.export_snapshot().unwrap();
        let target = SavedForwardStore::load(dir.path().join("target.json")).unwrap();

        let skipped = target
            .apply_snapshot(snapshot.clone(), &HashSet::new())
            .unwrap();
        assert_eq!(skipped.skipped, 1);
        assert!(target.load_owned_forwards("connection-1").is_empty());

        let mut valid = HashSet::new();
        valid.insert("connection-1".to_string());
        let applied = target.apply_snapshot(snapshot, &valid).unwrap();
        assert_eq!(applied.applied, 1);
        assert_eq!(target.load_owned_forwards("connection-1").len(), 1);
    }
}
