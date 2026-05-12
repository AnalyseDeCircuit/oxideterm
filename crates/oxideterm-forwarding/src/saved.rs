// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ForwardRule, ForwardStatus, ForwardType};

pub const FORWARD_TOMBSTONE_RETENTION_DAYS: i64 = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedForwardData {
    #[serde(default)]
    forwards: Vec<PersistedForward>,
    #[serde(default)]
    tombstones: Vec<DeletedPersistedForwardTombstone>,
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

    pub fn sync_persisted_forward_rule(
        &self,
        forward_id: &str,
        session_id: &str,
        owner_connection_id: Option<String>,
        rule: ForwardRule,
    ) -> Result<Option<PersistedForward>, SavedForwardError> {
        let mut data = self.lock_data();
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
        self.save_locked(&mut data)?;
        Ok(saved)
    }

    pub fn persist_forward(&self, forward: PersistedForward) -> Result<(), SavedForwardError> {
        let mut data = self.lock_data();
        upsert_forward(&mut data.forwards, forward);
        let forward_ids: HashSet<String> = data
            .forwards
            .iter()
            .map(|forward| forward.id.clone())
            .collect();
        data.tombstones
            .retain(|tombstone| !forward_ids.contains(&tombstone.id));
        self.save_locked(&mut data)
    }

    pub fn delete_persisted_forward(&self, forward_id: &str) -> Result<(), SavedForwardError> {
        let mut data = self.lock_data();
        let Some(index) = data
            .forwards
            .iter()
            .position(|forward| forward.id == forward_id)
        else {
            return Ok(());
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
        self.save_locked(&mut data)
    }

    pub fn update_auto_start(
        &self,
        forward_id: &str,
        auto_start: bool,
    ) -> Result<(), SavedForwardError> {
        let mut data = self.lock_data();
        let Some(forward) = data
            .forwards
            .iter_mut()
            .find(|forward| forward.id == forward_id)
        else {
            return Err(SavedForwardError::NotFound(forward_id.to_string()));
        };
        forward.auto_start = auto_start;
        forward.mark_updated();
        self.save_locked(&mut data)
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
        let mut data = self.lock_data();
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
        self.save_locked(&mut data)?;
        Ok(count)
    }

    pub fn delete_owned_forwards(
        &self,
        owner_connection_id: &str,
    ) -> Result<usize, SavedForwardError> {
        let mut data = self.lock_data();
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
        self.save_locked(&mut data)?;
        Ok(count)
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
        let mut data = self.lock_data();
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

        if result.applied > 0 {
            self.save_locked(&mut data)?;
        }
        Ok(result)
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

    fn save_locked(&self, data: &mut SavedForwardData) -> Result<(), SavedForwardError> {
        data.tombstones = active_tombstones(&data.tombstones);
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(data)?;
        fs::write(&self.path, json)?;
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
