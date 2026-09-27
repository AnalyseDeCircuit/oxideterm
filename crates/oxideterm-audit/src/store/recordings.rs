use super::*;
use crate::recording::*;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

const MAX_CHUNK_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_FRAMES: usize = 4096;
pub(crate) const PRUNE_BATCH_CHUNKS: usize = 128;
const HEADER_LEN: usize = 8 + 36 + 36 + 8 + 8 + 8 + 4 + 24;
const MAGIC: &[u8; 8] = b"OTREC001";

// Keep the durable file stage separate from index publication and database locks.
/// Writes an immutable encrypted chunk. Implementations must reject an existing
/// destination and durably sync the file and its directory before returning success.
/// SQLite publication remains the audit service's responsibility.
pub trait RecordingFiles: Send + Sync {
    fn write_chunk(
        &self,
        destination: &Path,
        header: &[u8],
        encrypted: &[u8],
    ) -> std::io::Result<()>;
}

pub struct DurableRecordingFiles;
impl RecordingFiles for DurableRecordingFiles {
    fn write_chunk(
        &self,
        destination: &Path,
        header: &[u8],
        encrypted: &[u8],
    ) -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // The index commit publishes this immutable file. Exclusive creation prevents
        // a concurrent or retried append from overwriting an existing chunk.
        let mut file = options.open(destination)?;
        let result = (|| {
            file.write_all(header)?;
            file.write_all(encrypted)?;
            file.sync_all()?;
            oxideterm_atomic_file::sync_directory(
                destination
                    .parent()
                    .ok_or(std::io::ErrorKind::InvalidInput)?,
            )
        })();
        if result.is_err() {
            drop(file);
            let _ = fs::remove_file(destination);
        }
        result
    }
}

impl AuditStore {
    pub fn create_recording(
        &mut self,
        instance_id: &str,
        started_at_ms: i64,
        details: &RecordingDetails,
    ) -> Result<Option<String>, AuditError> {
        let policy = self.policy()?;
        if !policy.enabled || !policy.record_output {
            return Ok(None);
        }
        let instance_id = canonical_id(instance_id)?;
        let id = uuid::Uuid::new_v4().to_string();
        let clear = Zeroizing::new(serde_json::to_vec(details).map_err(|_| AuditError::Integrity)?);
        if clear.len() > 64 * 1024 {
            return Err(AuditError::TooLarge);
        }
        let aad = format!(
            "recording-meta-v1:{}:{id}:{instance_id}:{started_at_ms}",
            self.key_id
        );
        let encrypted = seal(&self.cipher, &clear, aad.as_bytes())?;
        let state_tag = self.make_state_tag(&id, &instance_id, started_at_ms, None, 0)?;
        let session_key = self.session_key(&details.session_id);
        self.db.execute(
            "INSERT INTO audit_recordings(id,instance_id,started_at,state,details,state_tag,session_key) VALUES(?,?,?,0,?,?,?)",
            params![id, instance_id, started_at_ms, encrypted, state_tag, session_key],
        ).map_err(|_| AuditError::Storage)?;
        Ok(Some(id))
    }

    /// One encrypted file commits at most 4 MiB of decoded frames.
    pub fn append_recording_chunk(
        &mut self,
        recording_id: &str,
        frames: &[RecordingFrame<'_>],
    ) -> Result<i64, AuditError> {
        let id = canonical_id(recording_id)?;
        if frames.is_empty() || frames.len() > MAX_FRAMES {
            return Err(AuditError::TooLarge);
        }
        if frames
            .windows(2)
            .any(|pair| pair[0].occurred_at_ms > pair[1].occurred_at_ms)
        {
            return Err(AuditError::Integrity);
        }
        let clear = encode_frames(frames)?;
        let first = frames[0].occurred_at_ms;
        let last = frames.last().unwrap().occurred_at_ms;
        let has_gap = frames
            .iter()
            .any(|frame| matches!(frame.kind, RecordingFrameKind::Gap { .. }));
        let recording_dir = self.recording_dir.clone();
        let policy = read_policy(&self.db)?;
        if !policy.enabled || !policy.record_output {
            return Err(AuditError::Closed);
        }
        let status: Option<(String, i64, Option<i64>, i64, Vec<u8>)> = self.db.query_row(
            "SELECT instance_id,started_at,ended_at,state,state_tag FROM audit_recordings WHERE id=?",
            [&id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        ).optional().map_err(|_| AuditError::Storage)?;
        let Some((instance, started, ended, state, tag)) = status else {
            return Err(AuditError::Closed);
        };
        verify_state_tag(
            &self.cipher,
            &self.key_id,
            &id,
            &instance,
            started,
            ended,
            state,
            &tag,
        )?;
        if state != 0 {
            return Err(AuditError::Closed);
        }
        let sequence: i64 = self.db.query_row(
            "SELECT coalesce(max(sequence),0)+1 FROM audit_recording_chunks WHERE recording_id=?",
            [&id], |row| row.get(0),
        ).map_err(|_| AuditError::Storage)?;
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(self.key_id.as_bytes());
        header.extend_from_slice(id.as_bytes());
        header.extend_from_slice(&sequence.to_le_bytes());
        header.extend_from_slice(&first.to_le_bytes());
        header.extend_from_slice(&last.to_le_bytes());
        header.extend_from_slice(&(clear.len() as u32).to_le_bytes());
        let mut nonce = [0u8; 24];
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| AuditError::Storage)?;
        header.extend_from_slice(&nonce);
        let encrypted = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &clear,
                    aad: &header,
                },
            )
            .map_err(|_| AuditError::Integrity)?;
        let path = recording_dir.join(format!("{id}-{sequence:016x}.chunk"));
        let written = (header.len() + encrypted.len()) as i64;
        self.recording_files
            .write_chunk(&path, &header, &encrypted)
            .map_err(|_| AuditError::Storage)?;
        let mut commit_attempted = false;
        let result = (|| {
            // File I/O has completed before taking SQLite's single-writer lock.
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| AuditError::Storage)?;
            let policy = read_policy(&tx)?;
            let current_tag: Option<Vec<u8>> = tx
                .query_row(
                    "SELECT state_tag FROM audit_recordings WHERE id=? AND state=0",
                    [&id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| AuditError::Storage)?;
            if !policy.enabled
                || !policy.record_output
                || current_tag.as_deref() != Some(tag.as_slice())
            {
                return Err(AuditError::Closed);
            }
            tx.execute("INSERT INTO audit_recording_chunks(recording_id,sequence,occurred_at,kind,bytes,state) VALUES(?,?,?,?,?,0)",
            params![id, sequence, last, i64::from(has_gap), written]).map_err(|_| AuditError::Storage)?;
            // An ambiguous failed commit leaves the encrypted file for orphan recovery.
            commit_attempted = true;
            tx.commit().map_err(|_| AuditError::Storage)?;
            Ok(sequence)
        })();
        if result.is_err() && !commit_attempted {
            // No index commit was attempted, so this owned file cannot be visible.
            let _ = fs::remove_file(&path);
        }
        result
    }

    pub fn finish_recording(
        &mut self,
        recording_id: &str,
        ended_at_ms: i64,
        interrupted: bool,
    ) -> Result<(), AuditError> {
        let id = canonical_id(recording_id)?;
        let expired: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM audit_recording_chunks WHERE recording_id=? AND state<>0)",
            [&id], |r| r.get(0),
        ).map_err(|_| AuditError::Storage)?;
        let gaps: bool = self.recording_has_gap(&id)?;
        let state = if interrupted {
            2
        } else if expired || gaps {
            3
        } else {
            1
        };
        let changed = self.update_recording_state(&id, state, Some(ended_at_ms), Some(0))?;
        if changed == 0 {
            return Err(AuditError::Closed);
        }
        Ok(())
    }

    pub fn read_recording_page(
        &self,
        recording_id: &str,
        after_sequence: Option<i64>,
        after_ms: Option<i64>,
        limit: usize,
    ) -> Result<RecordingPage, AuditError> {
        let id = canonical_id(recording_id)?;
        if !(1..=16).contains(&limit) {
            return Err(AuditError::TooLarge);
        }
        let status = self.recording_state(&id)?;
        let state = decode_state(status.3)?;
        let mut statement = self.db.prepare("SELECT sequence,occurred_at,bytes,state,kind FROM audit_recording_chunks
            WHERE recording_id=? AND sequence>? AND (? IS NULL OR occurred_at>=?) ORDER BY sequence LIMIT ?")
            .map_err(|_| AuditError::Storage)?;
        let mut rows = statement
            .query(params![
                id,
                after_sequence.unwrap_or(0),
                after_ms,
                after_ms,
                limit as i64 + 1
            ])
            .map_err(|_| AuditError::Storage)?;
        let mut chunks = Vec::new();
        let mut expired_sequences = Vec::new();
        let mut decoded_bytes = 0usize;
        let mut more = false;
        let mut last_scanned = None;
        while let Some(row) = rows.next().map_err(|_| AuditError::Storage)? {
            if chunks.len() + expired_sequences.len() >= limit {
                more = true;
                break;
            }
            let sequence: i64 = row.get(0).map_err(|_| AuditError::Storage)?;
            let last: i64 = row.get(1).map_err(|_| AuditError::Storage)?;
            let bytes: i64 = row.get(2).map_err(|_| AuditError::Storage)?;
            let state: i64 = row.get(3).map_err(|_| AuditError::Storage)?;
            let kind: i64 = row.get(4).map_err(|_| AuditError::Storage)?;
            last_scanned = Some(sequence);
            if state != 0 {
                if state != 1 && state != 2 {
                    return Err(AuditError::Integrity);
                }
                expired_sequences.push(sequence);
                continue;
            }
            let chunk = self.read_chunk(&id, sequence, last, bytes)?;
            if chunk
                .frames
                .iter()
                .any(|frame| matches!(frame.kind, StoredRecordingFrameKind::Gap { .. }))
                != (kind == 1)
            {
                return Err(AuditError::Integrity);
            }
            let size = chunk
                .frames
                .iter()
                .map(|f| match &f.kind {
                    StoredRecordingFrameKind::Output(v) => v.len(),
                    _ => 16,
                })
                .sum::<usize>();
            if decoded_bytes + size > MAX_PAGE_BYTES && !chunks.is_empty() {
                more = true;
                last_scanned = Some(sequence - 1);
                break;
            }
            decoded_bytes += size;
            chunks.push(chunk);
        }
        let next_cursor = if more { last_scanned } else { None };
        Ok(RecordingPage {
            chunks,
            expired_sequences,
            state,
            ended_at_ms: status.2,
            next_cursor,
        })
    }

    pub fn list_recordings(
        &self,
        before: Option<(i64, String)>,
        limit: usize,
    ) -> Result<RecordingListPage, AuditError> {
        self.list_recordings_filtered(None, before, limit)
    }

    pub fn list_recordings_for_session(
        &self,
        session_id: &str,
        before: Option<(i64, String)>,
        limit: usize,
    ) -> Result<RecordingListPage, AuditError> {
        self.list_recordings_filtered(Some(session_id), before, limit)
    }

    fn list_recordings_filtered(
        &self,
        session_id: Option<&str>,
        before: Option<(i64, String)>,
        limit: usize,
    ) -> Result<RecordingListPage, AuditError> {
        if !(1..=100).contains(&limit) {
            return Err(AuditError::TooLarge);
        }
        let (time, id) = before.unwrap_or((i64::MAX, String::new()));
        let session_key = session_id.map(|id| self.session_key(id));
        let sql = if session_key.is_some() {
            "SELECT r.id,r.instance_id,r.started_at,r.ended_at,r.state,r.details,
            EXISTS(SELECT 1 FROM audit_recording_chunks c WHERE c.recording_id=r.id AND c.state<>0),r.state_tag,r.session_key
            FROM audit_recordings r WHERE r.session_key=?1 AND (r.started_at<?2 OR (r.started_at=?3 AND r.id<?4))
            ORDER BY r.started_at DESC,r.id DESC LIMIT ?5"
        } else {
            "SELECT r.id,r.instance_id,r.started_at,r.ended_at,r.state,r.details,
            EXISTS(SELECT 1 FROM audit_recording_chunks c WHERE c.recording_id=r.id AND c.state<>0),r.state_tag,r.session_key
            FROM audit_recordings r WHERE r.started_at<?1 OR (r.started_at=?2 AND r.id<?3)
            ORDER BY r.started_at DESC,r.id DESC LIMIT ?4"
        };
        let mut statement = self.db.prepare(sql).map_err(|_| AuditError::Storage)?;
        let cursor_id = if id.is_empty() { "~" } else { &id };
        let mut rows = match &session_key {
            Some(key) => statement.query(params![key, time, time, cursor_id, limit as i64 + 1]),
            None => statement.query(params![time, time, cursor_id, limit as i64 + 1]),
        }
        .map_err(|_| AuditError::Storage)?;
        let mut recordings = Vec::new();
        let mut more = false;
        while let Some(row) = rows.next().map_err(|_| AuditError::Storage)? {
            if recordings.len() == limit {
                more = true;
                break;
            }
            let id: String = row.get(0).map_err(|_| AuditError::Storage)?;
            let instance_id: String = row.get(1).map_err(|_| AuditError::Storage)?;
            let started_at_ms: i64 = row.get(2).map_err(|_| AuditError::Storage)?;
            let ended_at_ms: Option<i64> = row.get(3).map_err(|_| AuditError::Storage)?;
            let state: i64 = row.get(4).map_err(|_| AuditError::Storage)?;
            let encrypted: Vec<u8> = row.get(5).map_err(|_| AuditError::Storage)?;
            let has_expired_content: bool = row.get(6).map_err(|_| AuditError::Storage)?;
            let state_tag: Vec<u8> = row.get(7).map_err(|_| AuditError::Storage)?;
            let stored_session_key: String = row.get(8).map_err(|_| AuditError::Storage)?;
            verify_state_tag(
                &self.cipher,
                &self.key_id,
                &id,
                &instance_id,
                started_at_ms,
                ended_at_ms,
                state,
                &state_tag,
            )?;
            let aad = format!(
                "recording-meta-v1:{}:{id}:{instance_id}:{started_at_ms}",
                self.key_id
            );
            let clear = open_sealed(&self.cipher, &encrypted, aad.as_bytes())?;
            let details: RecordingDetails =
                serde_json::from_slice(&clear).map_err(|_| AuditError::Integrity)?;
            if self.session_key(&details.session_id) != stored_session_key
                || session_id.is_some_and(|expected| details.session_id != expected)
            {
                return Err(AuditError::Integrity);
            }
            recordings.push(RecordingSummary {
                id,
                instance_id,
                started_at_ms,
                ended_at_ms,
                state: decode_state(state)?,
                has_expired_content,
                details,
            });
        }
        let next_cursor = if more {
            recordings.last().map(|r| (r.started_at_ms, r.id.clone()))
        } else {
            None
        };
        Ok(RecordingListPage {
            recordings,
            next_cursor,
        })
    }

    /// Tombstones precede unlink; a later call resumes interrupted deletion.
    pub fn prune_recordings(&mut self, now_ms: i64) -> Result<usize, AuditError> {
        let policy = self.policy()?;
        let cutoff = now_ms.saturating_sub(i64::from(policy.output_retention_days) * 86_400_000);
        let total: i64 = self
            .db
            .query_row(
                "SELECT coalesce(sum(bytes),0) FROM audit_recording_chunks WHERE state=0",
                [],
                |r| r.get(0),
            )
            .map_err(|_| AuditError::Storage)?;
        let mut candidates: Vec<(String, i64, i64, i64)> = Vec::with_capacity(PRUNE_BATCH_CHUNKS);
        for (sql, parameter) in [
            (
                "SELECT recording_id,sequence,bytes,state FROM audit_recording_chunks WHERE state=1 ORDER BY occurred_at,recording_id,sequence LIMIT ?",
                None,
            ),
            (
                "SELECT recording_id,sequence,bytes,state FROM audit_recording_chunks WHERE state=0 AND occurred_at<? ORDER BY occurred_at,recording_id,sequence LIMIT ?",
                Some(cutoff),
            ),
        ] {
            if candidates.len() == PRUNE_BATCH_CHUNKS {
                break;
            }
            let mut statement = self.db.prepare(sql).map_err(|_| AuditError::Storage)?;
            let limit = (PRUNE_BATCH_CHUNKS - candidates.len()) as i64;
            let rows = match parameter {
                Some(value) => statement.query_map(params![value, limit], chunk_index_row),
                None => statement.query_map([limit], chunk_index_row),
            }
            .map_err(|_| AuditError::Storage)?;
            candidates.extend(
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|_| AuditError::Storage)?,
            );
        }
        let mut remaining = total;
        for (_, _, bytes, state) in &candidates {
            if *state == 0 {
                remaining = remaining.saturating_sub(*bytes);
            }
        }
        if candidates.len() < PRUNE_BATCH_CHUNKS && remaining > policy.output_max_bytes as i64 {
            let mut statement = self.db.prepare(
                "SELECT recording_id,sequence,bytes,state FROM audit_recording_chunks WHERE state=0 AND occurred_at>=? ORDER BY occurred_at,recording_id,sequence LIMIT ?",
            ).map_err(|_| AuditError::Storage)?;
            let rows = statement
                .query_map(
                    params![cutoff, (PRUNE_BATCH_CHUNKS - candidates.len()) as i64],
                    chunk_index_row,
                )
                .map_err(|_| AuditError::Storage)?;
            for row in rows {
                if remaining <= policy.output_max_bytes as i64 {
                    break;
                }
                let candidate = row.map_err(|_| AuditError::Storage)?;
                remaining = remaining.saturating_sub(candidate.2);
                candidates.push(candidate);
            }
        }
        if candidates.is_empty() {
            return Ok(0);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AuditError::Storage)?;
        for (id, sequence, _, state) in &candidates {
            if *state == 0 {
                tx.execute("UPDATE audit_recording_chunks SET state=1 WHERE recording_id=? AND sequence=? AND state=0",
                    params![id, sequence]).map_err(|_| AuditError::Storage)?;
            }
        }
        tx.commit().map_err(|_| AuditError::Storage)?;
        for (id, sequence, _, _) in &candidates {
            let path = self.chunk_path(id, *sequence);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(AuditError::Storage),
            }
        }
        oxideterm_atomic_file::sync_directory(&self.recording_dir)
            .map_err(|_| AuditError::Storage)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AuditError::Storage)?;
        for (id, sequence, _, _) in &candidates {
            tx.execute("UPDATE audit_recording_chunks SET state=2 WHERE recording_id=? AND sequence=? AND state=1",
                params![id, sequence]).map_err(|_| AuditError::Storage)?;
        }
        tx.commit().map_err(|_| AuditError::Storage)?;
        let ids = candidates
            .iter()
            .map(|(id, _, _, _)| id)
            .collect::<std::collections::HashSet<_>>();
        for id in ids {
            let live: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM audit_recording_chunks WHERE recording_id=? AND state=0)",
                [id], |r| r.get(0)).map_err(|_| AuditError::Storage)?;
            let status = self.recording_state(id)?;
            if matches!(status.3, 1 | 3 | 4) {
                self.update_recording_state(id, if live { 3 } else { 4 }, status.2, None)?;
            }
        }
        Ok(candidates.len())
    }

    /// Call after acquiring this process's instance lease, before accepting new output.
    pub fn recover_abandoned_recordings(&mut self, database: &Path) -> Result<usize, AuditError> {
        let mut statement = self.db.prepare(
            "SELECT DISTINCT instance_id FROM audit_recordings WHERE state=0 ORDER BY instance_id",
        ).map_err(|_| AuditError::Storage)?;
        let instances = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| AuditError::Storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AuditError::Storage)?;
        drop(statement);
        let mut recovered = 0;
        for id in instances {
            if self.lease.as_ref().is_some_and(|lease| lease.id == id) {
                continue;
            }
            if let Some(_lease) = InstanceLease::try_acquire(database, &id)? {
                let mut stmt = self
                    .db
                    .prepare("SELECT id FROM audit_recordings WHERE instance_id=? AND state=0")
                    .map_err(|_| AuditError::Storage)?;
                let ids = stmt
                    .query_map([&id], |r| r.get::<_, String>(0))
                    .map_err(|_| AuditError::Storage)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| AuditError::Storage)?;
                drop(stmt);
                for recording_id in ids {
                    recovered += self.update_recording_state(&recording_id, 2, None, Some(0))?;
                }
            }
        }
        Ok(recovered)
    }

    /// Remove files left after a crash between durable rename and index commit.
    /// Active instance leases are never scanned for deletion.
    pub fn recover_orphan_recording_files(&self, database: &Path) -> Result<usize, AuditError> {
        let mut removed = 0;
        for entry in fs::read_dir(&self.recording_dir).map_err(|_| AuditError::Storage)? {
            let entry = entry.map_err(|_| AuditError::Storage)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let (id, sequence) = if let Some((id, sequence)) = parse_chunk_name(name) {
                (id, Some(sequence))
            } else if let Some(id) = parse_temporary_name(name) {
                (id, None)
            } else {
                continue;
            };
            let instance: Option<String> = self
                .db
                .query_row(
                    "SELECT instance_id FROM audit_recordings WHERE id=?",
                    [&id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| AuditError::Storage)?;
            let Some(instance) = instance else {
                continue;
            };
            if self
                .lease
                .as_ref()
                .is_some_and(|lease| lease.id == instance)
            {
                continue;
            }
            let Some(_lease) = InstanceLease::try_acquire(database, &instance)? else {
                continue;
            };
            let indexed: bool = if let Some(sequence) = sequence {
                self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM audit_recording_chunks WHERE recording_id=? AND sequence=?)",
                    params![id, sequence], |row| row.get(0),
                ).map_err(|_| AuditError::Storage)?
            } else {
                false
            };
            if !indexed {
                fs::remove_file(entry.path()).map_err(|_| AuditError::Storage)?;
                removed += 1;
            }
        }
        if removed > 0 {
            oxideterm_atomic_file::sync_directory(&self.recording_dir)
                .map_err(|_| AuditError::Storage)?;
        }
        Ok(removed)
    }

    fn chunk_path(&self, id: &str, sequence: i64) -> PathBuf {
        self.recording_dir
            .join(format!("{id}-{sequence:016x}.chunk"))
    }

    fn recording_state(
        &self,
        id: &str,
    ) -> Result<(String, i64, Option<i64>, i64, Vec<u8>), AuditError> {
        let status: (String, i64, Option<i64>, i64, Vec<u8>) = self.db.query_row(
            "SELECT instance_id,started_at,ended_at,state,state_tag FROM audit_recordings WHERE id=?",
            [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
        ).optional().map_err(|_| AuditError::Storage)?.ok_or(AuditError::Integrity)?;
        verify_state_tag(
            &self.cipher,
            &self.key_id,
            id,
            &status.0,
            status.1,
            status.2,
            status.3,
            &status.4,
        )?;
        Ok(status)
    }

    fn make_state_tag(
        &self,
        id: &str,
        instance: &str,
        started: i64,
        ended: Option<i64>,
        state: i64,
    ) -> Result<Vec<u8>, AuditError> {
        let aad = state_aad(&self.key_id, id, instance, started, ended, state);
        seal(&self.cipher, &[], aad.as_bytes())
    }

    fn update_recording_state(
        &mut self,
        id: &str,
        state: i64,
        ended: Option<i64>,
        expected: Option<i64>,
    ) -> Result<usize, AuditError> {
        let (instance, started, _, previous, old_tag) = self.recording_state(id)?;
        if expected.is_some_and(|expected| expected != previous) {
            return Ok(0);
        }
        let tag = self.make_state_tag(id, &instance, started, ended, state)?;
        self.db.execute("UPDATE audit_recordings SET state=?,ended_at=?,state_tag=? WHERE id=? AND state_tag=?",
            params![state, ended, tag, id, old_tag]).map_err(|_| AuditError::Storage)
    }

    fn read_chunk(
        &self,
        id: &str,
        sequence: i64,
        last: i64,
        bytes: i64,
    ) -> Result<RecordingChunk, AuditError> {
        if bytes < HEADER_LEN as i64 || bytes > (HEADER_LEN + MAX_CHUNK_BYTES + 16) as i64 {
            return Err(AuditError::Integrity);
        }
        let path = self.chunk_path(id, sequence);
        let mut file = fs::File::open(&path).map_err(|_| AuditError::Integrity)?;
        if file.metadata().map_err(|_| AuditError::Storage)?.len() != bytes as u64 {
            return Err(AuditError::Integrity);
        }
        let mut data = vec![0; bytes as usize];
        file.read_exact(&mut data)
            .map_err(|_| AuditError::Integrity)?;
        let header = &data[..HEADER_LEN];
        if &header[..8] != MAGIC
            || &header[8..44] != self.key_id.as_bytes()
            || &header[44..80] != id.as_bytes()
            || header[80..88] != sequence.to_le_bytes()
            || header[96..104] != last.to_le_bytes()
        {
            return Err(AuditError::Integrity);
        }
        let declared = u32::from_le_bytes(header[104..108].try_into().unwrap()) as usize;
        if declared > MAX_CHUNK_BYTES || declared + HEADER_LEN + 16 != data.len() {
            return Err(AuditError::Integrity);
        }
        let clear = Zeroizing::new(
            self.cipher
                .decrypt(
                    XNonce::from_slice(&header[108..132]),
                    Payload {
                        msg: &data[HEADER_LEN..],
                        aad: header,
                    },
                )
                .map_err(|_| AuditError::Integrity)?,
        );
        let frames = decode_frames(&clear)?;
        if frames.first().map(|f| f.occurred_at_ms.to_le_bytes())
            != Some(header[88..96].try_into().unwrap())
            || frames.last().map(|f| f.occurred_at_ms) != Some(last)
        {
            return Err(AuditError::Integrity);
        }
        Ok(RecordingChunk { sequence, frames })
    }

    fn recording_has_gap(&self, id: &str) -> Result<bool, AuditError> {
        self.db.query_row("SELECT EXISTS(SELECT 1 FROM audit_recording_chunks WHERE recording_id=? AND kind=1)",
            [id], |r| r.get(0)).map_err(|_| AuditError::Storage)
    }
}

fn canonical_id(value: &str) -> Result<String, AuditError> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| AuditError::Integrity)?;
    if id.to_string() != value {
        return Err(AuditError::Integrity);
    }
    Ok(value.to_owned())
}

fn parse_chunk_name(name: &str) -> Option<(String, i64)> {
    let stem = name.strip_suffix(".chunk")?;
    // UUIDs contain hyphens, so split at their fixed canonical boundary.
    let id = stem.get(..36)?;
    let suffix = stem.get(36..)?.strip_prefix('-')?;
    if id.len() != 36 || suffix.len() != 16 || canonical_id(id).is_err() {
        return None;
    }
    let sequence = i64::try_from(u64::from_str_radix(suffix, 16).ok()?).ok()?;
    if sequence <= 0 {
        return None;
    }
    Some((id.to_owned(), sequence))
}

fn chunk_index_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, i64, i64, i64)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn parse_temporary_name(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".tmp")?;
    let id = stem.get(..36)?;
    let rest = stem.get(36..)?.strip_prefix('-')?;
    let (sequence, temporary_id) = rest.split_once('-')?;
    if sequence.len() != 16
        || u64::from_str_radix(sequence, 16).ok()? == 0
        || uuid::Uuid::parse_str(temporary_id).ok()?.to_string() != temporary_id
        || canonical_id(id).is_err()
    {
        return None;
    }
    Some(id.to_owned())
}

fn state_aad(
    key_id: &str,
    id: &str,
    instance: &str,
    started: i64,
    ended: Option<i64>,
    state: i64,
) -> String {
    format!(
        "recording-state-v1:{key_id}:{id}:{instance}:{started}:{}:{state}",
        ended.map_or_else(|| "none".to_owned(), |value| value.to_string())
    )
}

fn verify_state_tag(
    cipher: &XChaCha20Poly1305,
    key_id: &str,
    id: &str,
    instance: &str,
    started: i64,
    ended: Option<i64>,
    state: i64,
    tag: &[u8],
) -> Result<(), AuditError> {
    let aad = state_aad(key_id, id, instance, started, ended, state);
    if !open_sealed(cipher, tag, aad.as_bytes())?.is_empty() {
        return Err(AuditError::Integrity);
    }
    Ok(())
}

fn decode_state(state: i64) -> Result<RecordingState, AuditError> {
    match state {
        0 => Ok(RecordingState::InProgress),
        1 => Ok(RecordingState::Finished),
        2 => Ok(RecordingState::Interrupted),
        3 => Ok(RecordingState::Gaps),
        4 => Ok(RecordingState::Expired),
        _ => Err(AuditError::Integrity),
    }
}

fn seal(cipher: &XChaCha20Poly1305, clear: &[u8], aad: &[u8]) -> Result<Vec<u8>, AuditError> {
    let mut nonce = [0u8; 24];
    OsRng
        .try_fill_bytes(&mut nonce)
        .map_err(|_| AuditError::Storage)?;
    let mut result = nonce.to_vec();
    result.extend(
        cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: clear, aad })
            .map_err(|_| AuditError::Integrity)?,
    );
    Ok(result)
}

fn open_sealed(
    cipher: &XChaCha20Poly1305,
    data: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, AuditError> {
    if data.len() < 40 || data.len() > 64 * 1024 + 40 {
        return Err(AuditError::Integrity);
    }
    Ok(Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&data[..24]),
                Payload {
                    msg: &data[24..],
                    aad,
                },
            )
            .map_err(|_| AuditError::Integrity)?,
    ))
}

fn encode_frames(frames: &[RecordingFrame<'_>]) -> Result<Zeroizing<Vec<u8>>, AuditError> {
    let mut clear = Zeroizing::new(Vec::with_capacity(4096));
    clear.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    for frame in frames {
        let (kind, data): (u8, &[u8]) = match &frame.kind {
            RecordingFrameKind::Output(data) => (0, data),
            RecordingFrameKind::Resize { .. } => (1, &[]),
            RecordingFrameKind::Gap { .. } => (2, &[]),
        };
        let length = match frame.kind {
            RecordingFrameKind::Output(_) => data.len(),
            RecordingFrameKind::Resize { .. } => 4,
            RecordingFrameKind::Gap { .. } => 9,
        };
        if length > MAX_CHUNK_BYTES || clear.len() + 13 + length > MAX_CHUNK_BYTES {
            return Err(AuditError::TooLarge);
        }
        clear.push(kind);
        clear.extend_from_slice(&frame.occurred_at_ms.to_le_bytes());
        clear.extend_from_slice(&(length as u32).to_le_bytes());
        match frame.kind {
            RecordingFrameKind::Output(_) => clear.extend_from_slice(data),
            RecordingFrameKind::Resize { columns, rows } => {
                clear.extend_from_slice(&columns.to_le_bytes());
                clear.extend_from_slice(&rows.to_le_bytes());
            }
            RecordingFrameKind::Gap { lost_bytes } => {
                clear.push(u8::from(lost_bytes.is_some()));
                clear.extend_from_slice(&lost_bytes.unwrap_or(0).to_le_bytes());
            }
        }
    }
    Ok(clear)
}

fn decode_frames(clear: &[u8]) -> Result<Vec<StoredRecordingFrame>, AuditError> {
    if clear.len() < 2 || clear.len() > MAX_CHUNK_BYTES {
        return Err(AuditError::Integrity);
    }
    let count = u16::from_le_bytes(clear[..2].try_into().unwrap()) as usize;
    if count == 0 || count > MAX_FRAMES {
        return Err(AuditError::Integrity);
    }
    let mut offset = 2;
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        if clear.len() - offset < 13 {
            return Err(AuditError::Integrity);
        }
        let kind = clear[offset];
        let occurred_at_ms = i64::from_le_bytes(clear[offset + 1..offset + 9].try_into().unwrap());
        let length =
            u32::from_le_bytes(clear[offset + 9..offset + 13].try_into().unwrap()) as usize;
        offset += 13;
        if length > clear.len() - offset {
            return Err(AuditError::Integrity);
        }
        let data = &clear[offset..offset + length];
        let kind = match kind {
            0 => StoredRecordingFrameKind::Output(Zeroizing::new(data.to_vec())),
            1 if length == 4 => StoredRecordingFrameKind::Resize {
                columns: u16::from_le_bytes(data[..2].try_into().unwrap()),
                rows: u16::from_le_bytes(data[2..4].try_into().unwrap()),
            },
            2 if length == 9 && data[0] <= 1 => StoredRecordingFrameKind::Gap {
                lost_bytes: if data[0] == 1 {
                    Some(u64::from_le_bytes(data[1..9].try_into().unwrap()))
                } else {
                    None
                },
            },
            _ => return Err(AuditError::Integrity),
        };
        if frames
            .last()
            .is_some_and(|previous: &StoredRecordingFrame| previous.occurred_at_ms > occurred_at_ms)
        {
            return Err(AuditError::Integrity);
        }
        frames.push(StoredRecordingFrame {
            occurred_at_ms,
            kind,
        });
        offset += length;
    }
    if offset != clear.len() {
        return Err(AuditError::Integrity);
    }
    Ok(frames)
}
