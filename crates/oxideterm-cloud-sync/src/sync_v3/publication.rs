// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::{Result, bail};
use oxideterm_connections::oxide_file::{
    OxideBatchDecryptionContext, OxideBatchEncryptionContext, OxideDocumentKind,
    open_oxide_document, seal_oxide_document,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{PendingPublication, ReplicaSnapshot, SyncReplica};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PublicationId {
    pub writer: Uuid,
    pub sequence: u64,
    pub digest: String,
}

impl PublicationId {
    pub fn path(&self) -> String {
        format!(
            "sync-v3/{}/{}-{}.oxide",
            self.writer, self.sequence, self.digest
        )
    }

    pub fn parse(path: &str) -> Result<Self> {
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.len() != 3 || parts[0] != "sync-v3" {
            bail!("Invalid sync publication path");
        }
        let writer = Uuid::parse_str(parts[1])
            .map_err(|_| anyhow::anyhow!("Invalid sync writer identity"))?;
        let (sequence, digest) = parts[2]
            .strip_suffix(".oxide")
            .and_then(|name| name.split_once('-'))
            .ok_or_else(|| anyhow::anyhow!("Invalid sync publication filename"))?;
        let sequence = sequence
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid sync publication sequence"))?;
        if writer.is_nil()
            || sequence == 0
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!("Invalid sync publication identity");
        }
        let id = Self {
            writer,
            sequence,
            digest: digest.into(),
        };
        if id.path() != path {
            bail!("Noncanonical sync publication path");
        }
        Ok(id)
    }
}

#[derive(Serialize, Deserialize)]
pub struct PublishedReplica {
    schema: u32,
    pub writer: Uuid,
    pub sequence: u64,
    pub created_at_ms: i64,
    pub snapshot: ReplicaSnapshot,
}

impl PublishedReplica {
    pub fn encode(
        writer: Uuid,
        sequence: u64,
        snapshot: ReplicaSnapshot,
        context: &OxideBatchEncryptionContext,
    ) -> Result<PendingPublication> {
        if writer.is_nil() || sequence == 0 {
            bail!("Invalid sync publication identity");
        }
        let publication = Self {
            schema: 3,
            writer,
            sequence,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            snapshot,
        };
        let plaintext = Zeroizing::new(
            rmp_serde::to_vec_named(&publication)
                .map_err(|_| anyhow::anyhow!("Cannot encode cloud sync snapshot"))?,
        );
        let bytes = seal_oxide_document(OxideDocumentKind::SyncSnapshot, &plaintext, context)?;
        if bytes.len() > crate::MAX_REMOTE_SNAPSHOT_BYTES {
            bail!("Cloud sync snapshot exceeds the size limit");
        }
        let id = PublicationId {
            writer,
            sequence,
            digest: super::model::content_digest(&bytes),
        };
        Ok(PendingPublication {
            path: id.path(),
            bytes,
        })
    }

    pub fn decode(
        id: &PublicationId,
        bytes: &[u8],
        context: &mut OxideBatchDecryptionContext,
    ) -> Result<Self> {
        if bytes.len() > crate::MAX_REMOTE_SNAPSHOT_BYTES
            || super::model::content_digest(bytes) != id.digest
        {
            bail!("Cloud sync snapshot size or digest mismatch");
        }
        let plaintext = open_oxide_document(bytes, OxideDocumentKind::SyncSnapshot, context)?;
        let publication: Self = rmp_serde::from_slice(&plaintext)
            .map_err(|_| anyhow::anyhow!("Invalid cloud sync snapshot"))?;
        if publication.schema != 3
            || publication.writer != id.writer
            || publication.sequence != id.sequence
        {
            bail!("Cloud sync publication identity mismatch");
        }
        SyncReplica::load(publication.snapshot.clone(), publication.writer)?;
        Ok(publication)
    }
}
