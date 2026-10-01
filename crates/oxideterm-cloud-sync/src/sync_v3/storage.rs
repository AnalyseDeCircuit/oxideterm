// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use fs2::FileExt;
use oxideterm_atomic_file::{durable_remove, durable_write};
use oxideterm_connections::oxide_file::{
    OxideDocumentKind, open_local_oxide_document, seal_local_oxide_document,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{PublicationId, PublishedReplica, ReplicaSnapshot, SyncReplica, SyncValues};
use crate::secrets::{CloudSyncSecretProvider, SecretReadMode};

#[derive(Clone, Serialize, Deserialize)]
pub struct PendingPublication {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LocalReplica {
    schema: u32,
    pub writer: Uuid,
    pub snapshot: ReplicaSnapshot,
    pub effective: SyncValues,
    pub next_sequence: u64,
    pub pending: Option<PendingPublication>,
    published_document: Option<String>,
    pending_document: Option<String>,
    pub observed: std::collections::BTreeMap<Uuid, PublicationId>,
    publications: Vec<(PublicationId, i64)>,
    sync_password_tag: Option<Vec<u8>>,
    pub legacy_imported: bool,
    pub created_gist_id: Option<String>,
    pub pending_password_change: Option<crate::CloudSyncSettings>,
}

impl LocalReplica {
    pub fn new() -> Self {
        let writer = Uuid::new_v4();
        Self {
            schema: 3,
            writer,
            snapshot: SyncReplica::new(writer).snapshot(),
            effective: SyncValues::new(),
            next_sequence: 1,
            pending: None,
            published_document: None,
            pending_document: None,
            observed: Default::default(),
            publications: Vec::new(),
            sync_password_tag: None,
            legacy_imported: false,
            created_gist_id: None,
            pending_password_change: None,
        }
    }

    pub fn needs_publication(&self) -> bool {
        self.pending.is_some()
            || self.published_document.as_ref() != Some(&self.snapshot.document_digest())
    }

    pub fn request_publication(&mut self) {
        self.published_document = None;
    }

    pub fn superseded_publications(&self, now_ms: i64) -> Vec<PublicationId> {
        let retained = self.publications.len().saturating_sub(5);
        self.publications
            .iter()
            .take(retained)
            .filter(|(id, time)| {
                id.writer == self.writer && now_ms.saturating_sub(*time) > 24 * 60 * 60 * 1000
            })
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Default for LocalReplica {
    fn default() -> Self {
        Self::new()
    }
}

/// Holding the store holds the process lock. The native/portable secret provider
/// owns the persistent key; only a zeroizing copy lives for this operation.
pub struct ReplicaStore {
    directory: PathBuf,
    key: Zeroizing<[u8; 32]>,
    _lock: File,
}

impl ReplicaStore {
    pub fn bind_sync_password(&self, record: &mut LocalReplica, password: &str) -> Result<()> {
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(self.key.as_slice())
            .map_err(|_| anyhow::anyhow!("Invalid local sync key"))?;
        mac.update(b"oxide-sync-v3-password");
        mac.update(password.as_bytes());
        if let Some(tag) = &record.sync_password_tag {
            mac.verify_slice(tag).map_err(|_| {
                anyhow::anyhow!("The sync password changed; use a new namespace before publishing")
            })?;
        } else {
            let mut next = record.clone();
            next.sync_password_tag = Some(mac.finalize().into_bytes().to_vec());
            self.save(&next)?;
            *record = next;
        }
        Ok(())
    }

    pub(super) fn read_recovery(&self) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let bytes = match fs::read(self.directory.join("recovery.oxide")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("Cannot read cloud sync recovery record"),
        };
        Ok(Some(open_local_oxide_document(
            &bytes,
            OxideDocumentKind::Recovery,
            &self.key,
        )?))
    }

    pub(super) fn write_recovery(&self, plaintext: &[u8]) -> Result<()> {
        let bytes = seal_local_oxide_document(OxideDocumentKind::Recovery, plaintext, &self.key)?;
        durable_write(&self.directory.join("recovery.oxide"), &bytes)
            .context("Cannot persist cloud sync recovery record")
    }

    pub(super) fn clear_recovery(&self) -> Result<()> {
        durable_remove(&self.directory.join("recovery.oxide"))
            .context("Cannot remove completed cloud sync recovery record")
    }

    pub fn open(directory: &Path, secrets: &mut impl CloudSyncSecretProvider) -> Result<Self> {
        fs::create_dir_all(directory).context("Cannot create cloud sync replica directory")?;
        let directory = directory
            .canonicalize()
            .context("Cannot locate cloud sync replica directory")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("replica.lock"))
            .context("Cannot open cloud sync lock")?;
        lock.try_lock_exclusive()
            .context("Cloud sync replica is already in use")?;
        // The reference travels with the replica so moving a portable data
        // directory does not change which protected key it owns.
        let reference_path = directory.join("local-key-id");
        let (key_id, new_reference) = match fs::read_to_string(&reference_path) {
            Ok(reference) => (
                Uuid::parse_str(reference.trim())
                    .map_err(|_| anyhow::anyhow!("Invalid cloud sync key reference"))?,
                false,
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if contains_encrypted_state(&directory)? {
                    bail!("Cloud sync key reference is missing; existing replica was preserved");
                }
                (Uuid::new_v4(), true)
            }
            Err(error) => return Err(error).context("Cannot read cloud sync key reference"),
        };
        let account = format!("sync-v3-local-key-{key_id}");
        let secret = secrets.get_secret(&account, SecretReadMode::Prompt)?;
        let key = if let Some(secret) = secret {
            let decoded = Zeroizing::new(
                STANDARD
                    .decode(secret.as_bytes())
                    .map_err(|_| anyhow::anyhow!("Invalid cloud sync local key"))?,
            );
            Zeroizing::new(
                decoded
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid cloud sync local key"))?,
            )
        } else {
            if contains_encrypted_state(&directory)? {
                bail!("Cloud sync local key is unavailable; existing replica was preserved");
            }
            let mut key = Zeroizing::new([0; 32]);
            rand::rngs::OsRng.fill_bytes(key.as_mut());
            let encoded = Zeroizing::new(STANDARD.encode(key.as_slice()));
            secrets.store_secret(&account, Some(encoded.as_str()))?;
            key
        };
        if new_reference {
            durable_write(&reference_path, key_id.to_string().as_bytes())
                .context("Cannot persist cloud sync key reference")?;
        }
        Ok(Self {
            directory,
            key,
            _lock: lock,
        })
    }

    pub fn load(&self) -> Result<Option<LocalReplica>> {
        let bytes = match fs::read(self.directory.join("replica.oxide")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("Cannot read cloud sync replica"),
        };
        let plaintext =
            open_local_oxide_document(&bytes, OxideDocumentKind::LocalReplica, &self.key)?;
        let record: LocalReplica = rmp_serde::from_slice(&plaintext)
            .map_err(|_| anyhow::anyhow!("Invalid encrypted cloud sync replica"))?;
        validate_record(&record)?;
        Ok(Some(record))
    }

    pub fn save(&self, record: &LocalReplica) -> Result<()> {
        validate_record(record)?;
        let plaintext = Zeroizing::new(
            rmp_serde::to_vec_named(record)
                .map_err(|_| anyhow::anyhow!("Cannot encode cloud sync replica"))?,
        );
        let bytes =
            seal_local_oxide_document(OxideDocumentKind::LocalReplica, &plaintext, &self.key)?;
        durable_write(&self.directory.join("replica.oxide"), &bytes)
            .context("Cannot persist cloud sync replica")
    }

    pub fn stage_publication(
        &self,
        record: &mut LocalReplica,
        context: &oxideterm_connections::oxide_file::OxideBatchEncryptionContext,
    ) -> Result<()> {
        if record.pending.is_some() {
            return Ok(());
        }
        let next_sequence = record
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Cloud sync publication sequence exhausted"))?;
        let pending = PublishedReplica::encode(
            record.writer,
            record.next_sequence,
            record.snapshot.clone(),
            context,
        )?;
        let mut staged = record.clone();
        staged.next_sequence = next_sequence;
        staged.pending = Some(pending);
        staged.pending_document = Some(record.snapshot.document_digest());
        self.save(&staged)?;
        *record = staged;
        Ok(())
    }

    pub fn confirm_publication(&self, record: &mut LocalReplica, path: &str) -> Result<()> {
        if record
            .pending
            .as_ref()
            .is_none_or(|pending| pending.path != path)
        {
            bail!("Cloud sync acknowledgement does not match the pending publication");
        }
        let mut committed = record.clone();
        committed.pending = None;
        committed.published_document = committed.pending_document.take();
        committed
            .observed
            .insert(record.writer, PublicationId::parse(path)?);
        committed.publications.push((
            PublicationId::parse(path)?,
            chrono::Utc::now().timestamp_millis(),
        ));
        self.save(&committed)?;
        *record = committed;
        Ok(())
    }

    pub fn confirm_removal(&self, record: &mut LocalReplica, id: &PublicationId) -> Result<()> {
        if id.writer != record.writer {
            bail!("Cannot remove another device's publication");
        }
        let mut updated = record.clone();
        updated
            .publications
            .retain(|(publication, _)| publication != id);
        self.save(&updated)?;
        *record = updated;
        Ok(())
    }
}

fn contains_encrypted_state(directory: &Path) -> Result<bool> {
    for entry in fs::read_dir(directory)? {
        if entry?
            .path()
            .extension()
            .is_some_and(|extension| extension == "oxide")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_record(record: &LocalReplica) -> Result<()> {
    if record.schema != 3 || record.writer.is_nil() || record.next_sequence == 0 {
        bail!("Invalid cloud sync local replica identity");
    }
    SyncReplica::load(record.snapshot.clone(), record.writer)?;
    if let Some(pending) = &record.pending {
        let id = PublicationId::parse(&pending.path)?;
        if id.writer != record.writer
            || id.sequence.checked_add(1) != Some(record.next_sequence)
            || super::model::content_digest(&pending.bytes) != id.digest
        {
            bail!("Invalid cloud sync pending publication");
        }
    }
    Ok(())
}
