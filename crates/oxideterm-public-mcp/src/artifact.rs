use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zeroize::Zeroizing;

use crate::handles::{ArtifactRef, ClientRef};

const ARTIFACT_TTL: Duration = Duration::from_secs(15 * 60);
const ARTIFACT_CAPACITY: usize = 256;
const ARTIFACT_CAPACITY_PER_CLIENT: usize = 64;
const ARTIFACT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const ARTIFACT_BYTES_PER_CLIENT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactProjection {
    pub artifact_ref: ArtifactRef,
    pub size: u64,
    pub digest: String,
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub expires_at_ms: u128,
}

pub struct ArtifactPage {
    pub projection: ArtifactProjection,
    pub offset: u64,
    pub bytes: Zeroizing<Vec<u8>>,
    pub next_offset: Option<u64>,
}

pub struct ArtifactContent {
    pub projection: ArtifactProjection,
    pub bytes: Zeroizing<Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("the temporary artifact store is unavailable")]
    Unavailable,
    #[error("the artifact store capacity has been reached")]
    CapacityReached,
    #[error("the artifact is not available to this client")]
    NotFound,
    #[error("the requested artifact range is invalid")]
    InvalidRange,
    #[error("the artifact exceeds the supported size for this operation")]
    TooLarge,
    #[error("the artifact name is invalid")]
    InvalidName,
    #[error("the artifact media type is invalid")]
    InvalidMediaType,
    #[error("failed to write temporary artifact data: {0}")]
    Write(std::io::Error),
    #[error("failed to read temporary artifact data: {0}")]
    Read(std::io::Error),
}

struct ArtifactRecord {
    client_ref: ClientRef,
    projection: ArtifactProjection,
    private_path: PathBuf,
    expires_at: Instant,
}

struct ArtifactState {
    root: Option<TempDir>,
    records: HashMap<ArtifactRef, ArtifactRecord>,
    total_bytes: u64,
}

pub struct ArtifactStore {
    state: Mutex<ArtifactState>,
}

impl ArtifactStore {
    pub fn new() -> Self {
        // A failed temporary directory creation disables artifacts without weakening the
        // boundary to an in-memory or caller-selected path fallback.
        let root = tempfile::Builder::new()
            .prefix("oxideterm-public-mcp-")
            .tempdir()
            .ok();
        Self {
            state: Mutex::new(ArtifactState {
                root,
                records: HashMap::new(),
                total_bytes: 0,
            }),
        }
    }

    pub fn stage(
        &self,
        client_ref: ClientRef,
        bytes: &[u8],
        media_type: String,
        name: Option<String>,
    ) -> Result<ArtifactProjection, ArtifactError> {
        validate_media_type(&media_type)?;
        if let Some(name) = name.as_deref() {
            validate_name(name)?;
        }

        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let mut state = self.state.lock();
        cleanup_expired(&mut state);
        let root_path = state
            .root
            .as_ref()
            .map(|root| root.path().to_owned())
            .ok_or(ArtifactError::Unavailable)?;
        enforce_capacity(&state, &client_ref, size)?;

        let artifact_ref = ArtifactRef::new();
        let private_path = root_path.join(artifact_ref.as_str());
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&private_path)
            .map_err(ArtifactError::Write)?;
        if let Err(error) = file.write_all(bytes) {
            let _ = fs::remove_file(&private_path);
            return Err(ArtifactError::Write(error));
        }
        let digest = hex_digest(bytes);
        let expires_at = Instant::now() + ARTIFACT_TTL;
        let projection = ArtifactProjection {
            artifact_ref: artifact_ref.clone(),
            size,
            digest,
            media_type,
            name,
            expires_at_ms: unix_time_ms() + ARTIFACT_TTL.as_millis(),
        };
        state.total_bytes = state.total_bytes.saturating_add(size);
        state.records.insert(
            artifact_ref,
            ArtifactRecord {
                client_ref,
                projection: projection.clone(),
                private_path,
                expires_at,
            },
        );
        Ok(projection)
    }

    pub fn read(
        &self,
        client_ref: &ClientRef,
        artifact_ref: &ArtifactRef,
        offset: u64,
        length: u32,
    ) -> Result<ArtifactPage, ArtifactError> {
        let mut state = self.state.lock();
        cleanup_expired(&mut state);
        let record = state
            .records
            .get(artifact_ref)
            .filter(|record| &record.client_ref == client_ref)
            .ok_or(ArtifactError::NotFound)?;
        if offset > record.projection.size {
            return Err(ArtifactError::InvalidRange);
        }
        let remaining = record.projection.size - offset;
        let read_length = remaining.min(u64::from(length));
        let buffer_length =
            usize::try_from(read_length).map_err(|_| ArtifactError::InvalidRange)?;
        let mut file = OpenOptions::new()
            .read(true)
            .open(&record.private_path)
            .map_err(ArtifactError::Read)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(ArtifactError::Read)?;
        let mut bytes = Zeroizing::new(vec![0; buffer_length]);
        file.read_exact(&mut bytes).map_err(ArtifactError::Read)?;
        let next = offset.saturating_add(read_length);
        Ok(ArtifactPage {
            projection: record.projection.clone(),
            offset,
            bytes,
            next_offset: (next < record.projection.size).then_some(next),
        })
    }

    /// Reads an owned artifact for a bounded internal product operation.
    pub fn read_all(
        &self,
        client_ref: &ClientRef,
        artifact_ref: &ArtifactRef,
        maximum_size: u64,
    ) -> Result<ArtifactContent, ArtifactError> {
        let mut state = self.state.lock();
        cleanup_expired(&mut state);
        let record = state
            .records
            .get(artifact_ref)
            .filter(|record| &record.client_ref == client_ref)
            .ok_or(ArtifactError::NotFound)?;
        if record.projection.size > maximum_size {
            return Err(ArtifactError::TooLarge);
        }
        let buffer_length =
            usize::try_from(record.projection.size).map_err(|_| ArtifactError::TooLarge)?;
        let mut file = OpenOptions::new()
            .read(true)
            .open(&record.private_path)
            .map_err(ArtifactError::Read)?;
        let mut bytes = Zeroizing::new(vec![0; buffer_length]);
        file.read_exact(&mut bytes).map_err(ArtifactError::Read)?;
        Ok(ArtifactContent {
            projection: record.projection.clone(),
            bytes,
        })
    }

    pub fn revoke_client(&self, client_ref: &ClientRef) {
        let mut state = self.state.lock();
        let removed = state
            .records
            .extract_if(|_, record| &record.client_ref == client_ref)
            .map(|(_, record)| record)
            .collect::<Vec<_>>();
        remove_records(&mut state, removed);
    }

    pub fn revoke(&self, client_ref: &ClientRef, artifact_ref: &ArtifactRef) -> bool {
        let mut state = self.state.lock();
        let Some(record) = state
            .records
            .get(artifact_ref)
            .filter(|record| &record.client_ref == client_ref)
        else {
            return false;
        };
        let private_ref = record.projection.artifact_ref.clone();
        let Some(record) = state.records.remove(&private_ref) else {
            return false;
        };
        // Individual revocation is used by resource-scoped handles such as
        // remote desktop frames without invalidating unrelated client artifacts.
        remove_records(&mut state, vec![record]);
        true
    }

    /// Reports whether a client-scoped artifact is still live after TTL cleanup.
    pub fn is_available(&self, client_ref: &ClientRef, artifact_ref: &ArtifactRef) -> bool {
        let mut state = self.state.lock();
        cleanup_expired(&mut state);
        state
            .records
            .get(artifact_ref)
            .is_some_and(|record| &record.client_ref == client_ref)
    }

    /// Removes expired content independently of client traffic.
    pub fn expire(&self) {
        cleanup_expired(&mut self.state.lock());
    }
}

impl Default for ArtifactStore {
    fn default() -> Self {
        Self::new()
    }
}

fn enforce_capacity(
    state: &ArtifactState,
    client_ref: &ClientRef,
    incoming_bytes: u64,
) -> Result<(), ArtifactError> {
    let client_records = state
        .records
        .values()
        .filter(|record| &record.client_ref == client_ref)
        .collect::<Vec<_>>();
    let client_bytes = client_records.iter().fold(0_u64, |total, record| {
        total.saturating_add(record.projection.size)
    });
    if state.records.len() >= ARTIFACT_CAPACITY
        || client_records.len() >= ARTIFACT_CAPACITY_PER_CLIENT
        || state.total_bytes.saturating_add(incoming_bytes) > ARTIFACT_TOTAL_BYTES
        || client_bytes.saturating_add(incoming_bytes) > ARTIFACT_BYTES_PER_CLIENT
    {
        return Err(ArtifactError::CapacityReached);
    }
    Ok(())
}

fn cleanup_expired(state: &mut ArtifactState) {
    let now = Instant::now();
    let removed = state
        .records
        .extract_if(|_, record| record.expires_at <= now)
        .map(|(_, record)| record)
        .collect::<Vec<_>>();
    remove_records(state, removed);
}

fn remove_records(state: &mut ArtifactState, records: Vec<ArtifactRecord>) {
    for record in records {
        state.total_bytes = state.total_bytes.saturating_sub(record.projection.size);
        let _ = fs::remove_file(record.private_path);
    }
}

fn validate_name(name: &str) -> Result<(), ArtifactError> {
    if name.is_empty()
        || name.len() > 255
        || name.chars().any(char::is_control)
        || name.contains('/')
        || name.contains('\\')
        || matches!(name, "." | "..")
    {
        return Err(ArtifactError::InvalidName);
    }
    Ok(())
}

fn validate_media_type(media_type: &str) -> Result<(), ArtifactError> {
    if media_type.is_empty()
        || media_type.len() > 255
        || !media_type.contains('/')
        || media_type.chars().any(char::is_control)
    {
        return Err(ArtifactError::InvalidMediaType);
    }
    Ok(())
}

fn hex_digest(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn unix_time_ms() -> u128 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .map_or(0, |elapsed| elapsed.as_millis())
}
