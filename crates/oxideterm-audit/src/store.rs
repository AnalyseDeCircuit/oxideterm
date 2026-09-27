use crate::instance::InstanceLease;
use crate::{
    AuditCapture, AuditError, AuditKeyProvider, AuditPage, AuditPhase, AuditPolicy, AuditQuery,
    AuditRecord,
};

mod operations;
mod recordings;
pub(crate) use recordings::PRUNE_BATCH_CHUNKS;
pub use recordings::{DurableRecordingFiles, RecordingFiles};
mod sessions;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::{RngCore, rngs::OsRng};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use zeroize::Zeroizing;

const FORMAT_VERSION: i64 = 1;
pub(crate) const MAX_RECORD_BYTES: usize = 64 * 1024;

pub struct AuditStore {
    db: Connection,
    cipher: XChaCha20Poly1305,
    key_id: String,
    index_key: Zeroizing<Vec<u8>>,
    lease: Option<std::sync::Arc<InstanceLease>>,
    recording_dir: PathBuf,
    pub(crate) recording_files: std::sync::Arc<dyn RecordingFiles>,
}

impl AuditStore {
    pub fn open(path: &Path, keys: &impl AuditKeyProvider) -> Result<Self, AuditError> {
        let recording_dir = path.with_extension("recordings");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AuditError::Storage)?;
        }
        let mut db = Connection::open(path).map_err(|_| AuditError::Storage)?;
        db.busy_timeout(Duration::from_millis(500))
            .map_err(|_| AuditError::Storage)?;
        db.pragma_update(None, "journal_mode", "WAL")
            .map_err(|_| AuditError::Storage)?;
        db.pragma_update(None, "foreign_keys", true)
            .map_err(|_| AuditError::Storage)?;
        db.pragma_update(None, "synchronous", "FULL")
            .map_err(|_| AuditError::Storage)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AuditError::Storage)?;
        let version: i64 = tx
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| AuditError::Storage)?;
        if version != 0 && version != FORMAT_VERSION {
            return Err(AuditError::UnsupportedVersion);
        }
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_meta(key_id TEXT NOT NULL, key_check BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS audit_policy(id INTEGER PRIMARY KEY CHECK(id=1), policy TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS audit_events(
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE, occurred_at INTEGER NOT NULL,
                category TEXT NOT NULL, severity TEXT NOT NULL, payload BLOB NOT NULL,
                session_key TEXT, operation_id TEXT, transport_id TEXT);
            CREATE INDEX IF NOT EXISTS audit_session_latest ON audit_events(session_key, sequence DESC);
            CREATE INDEX IF NOT EXISTS audit_session_operation ON audit_events(session_key, operation_id, sequence DESC);
            CREATE INDEX IF NOT EXISTS audit_operation_latest ON audit_events(operation_id, sequence DESC);
            CREATE TABLE IF NOT EXISTS audit_operation_heads(
                operation_id TEXT PRIMARY KEY, instance_id TEXT NOT NULL,
                event_sequence INTEGER NOT NULL REFERENCES audit_events(sequence) ON DELETE CASCADE,
                pending INTEGER NOT NULL CHECK(pending IN (0,1)));
            CREATE INDEX IF NOT EXISTS audit_operation_event ON audit_operation_heads(event_sequence);
            CREATE INDEX IF NOT EXISTS audit_pending_instance ON audit_operation_heads(instance_id, pending);
            CREATE INDEX IF NOT EXISTS audit_time ON audit_events(occurred_at);
            CREATE INDEX IF NOT EXISTS audit_filter ON audit_events(category, severity, sequence);
            CREATE TABLE IF NOT EXISTS audit_recordings(
                id TEXT PRIMARY KEY, instance_id TEXT NOT NULL, started_at INTEGER NOT NULL,
                ended_at INTEGER, state INTEGER NOT NULL, details BLOB NOT NULL,
                state_tag BLOB NOT NULL, session_key TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS audit_recordings_time ON audit_recordings(started_at DESC, id DESC);
            CREATE INDEX IF NOT EXISTS audit_recordings_session ON audit_recordings(session_key, started_at DESC, id DESC);
            CREATE TABLE IF NOT EXISTS audit_recording_chunks(
                recording_id TEXT NOT NULL REFERENCES audit_recordings(id),
                sequence INTEGER NOT NULL, occurred_at INTEGER NOT NULL,
                kind INTEGER NOT NULL, bytes INTEGER NOT NULL, state INTEGER NOT NULL,
                PRIMARY KEY(recording_id, sequence));
            CREATE INDEX IF NOT EXISTS audit_recording_retention ON audit_recording_chunks(state, occurred_at);
            PRAGMA user_version=1;",
        )
        .map_err(|_| AuditError::Storage)?;
        let metadata: Option<(String, Vec<u8>)> = tx
            .query_row("SELECT key_id,key_check FROM audit_meta", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .map_err(|_| AuditError::Storage)?;
        let (key_id, cipher, index_key) = match metadata {
            Some((id, check)) => {
                let key = keys.load(&id)?;
                let cipher = XChaCha20Poly1305::new_from_slice(&key)
                    .map_err(|_| AuditError::KeyUnavailable)?;
                if check.len() < 40 {
                    return Err(AuditError::Integrity);
                }
                let clear = Zeroizing::new(
                    cipher
                        .decrypt(
                            XNonce::from_slice(&check[..24]),
                            Payload {
                                msg: &check[24..],
                                aad: id.as_bytes(),
                            },
                        )
                        .map_err(|_| AuditError::Integrity)?,
                );
                if clear.as_slice() != b"oxideterm-audit-v1" {
                    return Err(AuditError::Integrity);
                }
                (id, cipher, key)
            }
            None => {
                let count: i64 = tx
                    .query_row("SELECT count(*) FROM audit_events", [], |r| r.get(0))
                    .map_err(|_| AuditError::Storage)?;
                if count != 0 {
                    return Err(AuditError::Integrity);
                }
                let id = uuid::Uuid::new_v4().to_string();
                let key = keys.create(&id)?;
                let cipher = XChaCha20Poly1305::new_from_slice(&key)
                    .map_err(|_| AuditError::KeyUnavailable)?;
                let mut nonce = [0; 24];
                OsRng
                    .try_fill_bytes(&mut nonce)
                    .map_err(|_| AuditError::Storage)?;
                let mut check = nonce.to_vec();
                check.extend(
                    cipher
                        .encrypt(
                            XNonce::from_slice(&nonce),
                            Payload {
                                msg: b"oxideterm-audit-v1",
                                aad: id.as_bytes(),
                            },
                        )
                        .map_err(|_| AuditError::Integrity)?,
                );
                tx.execute("INSERT INTO audit_meta VALUES (?,?)", params![id, check])
                    .map_err(|_| AuditError::Storage)?;
                (id, cipher, key)
            }
        };
        tx.commit().map_err(|_| AuditError::Storage)?;
        std::fs::create_dir_all(&recording_dir).map_err(|_| AuditError::Storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&recording_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| AuditError::Storage)?;
        }
        Ok(Self {
            db,
            cipher,
            key_id,
            index_key,
            lease: None,
            recording_dir,
            recording_files: std::sync::Arc::new(DurableRecordingFiles),
        })
    }

    pub(crate) fn reader(&self, path: &Path) -> Result<Self, AuditError> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| AuditError::Storage)?;
        db.busy_timeout(Duration::from_millis(500))
            .map_err(|_| AuditError::Storage)?;
        db.pragma_update(None, "query_only", true)
            .map_err(|_| AuditError::Storage)?;
        Ok(Self {
            db,
            cipher: self.cipher.clone(),
            key_id: self.key_id.clone(),
            index_key: self.index_key.clone(),
            lease: None,
            recording_dir: path.with_extension("recordings"),
            recording_files: self.recording_files.clone(),
        })
    }

    pub(crate) fn recording_writer(&self, path: &Path) -> Result<Self, AuditError> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|_| AuditError::Storage)?;
        db.busy_timeout(Duration::from_millis(500))
            .map_err(|_| AuditError::Storage)?;
        db.pragma_update(None, "foreign_keys", true)
            .map_err(|_| AuditError::Storage)?;
        db.pragma_update(None, "synchronous", "FULL")
            .map_err(|_| AuditError::Storage)?;
        Ok(Self {
            db,
            cipher: self.cipher.clone(),
            key_id: self.key_id.clone(),
            index_key: self.index_key.clone(),
            // Recovery must see this instance as live until both writers have stopped.
            lease: self.lease.clone(),
            recording_dir: self.recording_dir.clone(),
            recording_files: self.recording_files.clone(),
        })
    }

    pub(crate) fn read_snapshot(&self) -> Result<rusqlite::Transaction<'_>, AuditError> {
        self.db
            .unchecked_transaction()
            .map_err(|_| AuditError::Storage)
    }

    pub(crate) fn wal_path(&self) -> Result<Option<PathBuf>, AuditError> {
        let path: String = self
            .db
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name='main'",
                [],
                |row| row.get(0),
            )
            .map_err(|_| AuditError::Storage)?;
        if path.is_empty() {
            return Ok(None);
        }
        let mut name = std::ffi::OsString::from(path);
        name.push("-wal");
        Ok(Some(PathBuf::from(name)))
    }

    /// Returns false when collection is disabled; this is not a lost event.
    pub fn append(&mut self, record: &AuditRecord) -> Result<bool, AuditError> {
        self.append_record(record, false, None)
    }

    pub(crate) fn append_pending_batch(
        &mut self,
        records: &[crate::model::PendingRecord],
    ) -> Vec<Result<bool, AuditError>> {
        use std::sync::atomic::Ordering;
        let mut transaction =
            match rusqlite::Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate) {
                Ok(transaction) => transaction,
                Err(_) => return vec![Err(AuditError::Storage); records.len()],
            };
        let mut results = Vec::with_capacity(records.len());
        for pending in records {
            if !pending.start
                && pending
                    .suppressed
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                results.push(Ok(false));
                continue;
            }
            // Isolate an invalid event without discarding valid neighbors in the batch.
            let savepoint = match transaction.savepoint() {
                Ok(savepoint) => savepoint,
                Err(_) => return vec![Err(AuditError::Storage); records.len()],
            };
            let result = self
                .append_record(&pending.record, false, None)
                .and_then(|written| {
                    if written {
                        return Ok(true);
                    }
                    if pending.start {
                        if let Some(suppressed) = &pending.suppressed {
                            suppressed.store(true, Ordering::Release);
                        }
                        Ok(false)
                    } else if pending.suppressed.is_some()
                        && pending
                            .record
                            .details
                            .operation
                            .as_ref()
                            .is_some_and(|operation| operation.phase == Some(AuditPhase::Result))
                    {
                        self.finish_disabled_operation(&pending.record)
                    } else {
                        Ok(false)
                    }
                });
            results.push(match result {
                Ok(written) => savepoint
                    .commit()
                    .map(|()| written)
                    .map_err(|_| AuditError::Storage),
                Err(error) => {
                    drop(savepoint);
                    Err(error)
                }
            });
        }
        if transaction.commit().is_err() {
            return vec![Err(AuditError::Storage); records.len()];
        }
        results
    }

    fn append_record(
        &self,
        record: &AuditRecord,
        maintenance: bool,
        expected_pending: Option<i64>,
    ) -> Result<bool, AuditError> {
        let mut record = record.clone();
        let session_key = record
            .details
            .operation
            .as_ref()
            .and_then(|op| op.session_id.as_deref())
            .map(|id| self.session_key(id));
        let aad = self.aad(
            &record.id,
            record.occurred_at_ms,
            record.category.key(),
            record.severity.key(),
        );
        let transaction = self
            .db
            .is_autocommit()
            .then(|| rusqlite::Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate))
            .transpose()
            .map_err(|_| AuditError::Storage)?;
        let tx = &self.db;
        if !maintenance
            && !read_policy(&tx)?.enabled
            && record.category != crate::AuditCategory::Audit
        {
            return Ok(false);
        }
        let mut pending = false;
        if let Some(operation) = &mut record.details.operation {
            let previous: Option<(String, i64, bool)> = tx.prepare_cached(
                "SELECT instance_id,event_sequence,pending FROM audit_operation_heads WHERE operation_id=?",
            ).map_err(|_| AuditError::Storage)?.query_row(
                [&operation.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).optional().map_err(|_| AuditError::Storage)?;
            if previous
                .as_ref()
                .is_some_and(|(instance, _, _)| instance != &operation.instance_id)
            {
                return Err(AuditError::Integrity);
            }
            // Competing recovery processes can append only against the same
            // still-pending head they authenticated, inside this transaction.
            if expected_pending.is_some_and(|expected| {
                previous
                    .as_ref()
                    .is_none_or(|(_, sequence, pending)| *sequence != expected || !pending)
            }) {
                return Ok(false);
            }
            pending = match operation.phase {
                Some(AuditPhase::Start) => true,
                Some(AuditPhase::Authorization | AuditPhase::Progress) => {
                    previous.as_ref().is_none_or(|(_, _, pending)| *pending)
                }
                _ => false,
            };
            if operation.phase == Some(AuditPhase::Result)
                && operation.capture == Some(AuditCapture::Complete)
                && previous.as_ref().is_none_or(|(_, _, pending)| !pending)
            {
                operation.capture = Some(AuditCapture::Partial);
            }
        }
        let clear = Zeroizing::new(serde_json::to_vec(&record).map_err(|_| AuditError::Integrity)?);
        if clear.len() > MAX_RECORD_BYTES {
            return Err(AuditError::TooLarge);
        }
        let mut nonce = [0u8; 24];
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| AuditError::Storage)?;
        let encrypted = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &clear,
                    aad: &aad,
                },
            )
            .map_err(|_| AuditError::Integrity)?;
        let mut payload = nonce.to_vec();
        payload.extend(encrypted);
        tx.prepare_cached("INSERT INTO audit_events(event_id,occurred_at,category,severity,payload,session_key,operation_id,transport_id) VALUES (?,?,?,?,?,?,?,?)").map_err(|_| AuditError::Storage)?.execute(
            params![record.id, record.occurred_at_ms, record.category.key(), record.severity.key(), payload,
                session_key,
                record.details.operation.as_ref().map(|op| op.id.as_str()),
                record.details.operation.as_ref().and_then(|op| op.transport_id.as_deref())])
            .map_err(|_| AuditError::Storage)?;
        if let Some(operation) = &record.details.operation {
            tx.prepare_cached("INSERT INTO audit_operation_heads(operation_id,instance_id,event_sequence,pending) VALUES (?,?,?,?)
                ON CONFLICT(operation_id) DO UPDATE SET event_sequence=excluded.event_sequence,pending=excluded.pending").map_err(|_| AuditError::Storage)?.execute(
                params![operation.id, operation.instance_id, tx.last_insert_rowid(), pending]).map_err(|_| AuditError::Storage)?;
        }
        if let Some(transaction) = transaction {
            transaction.commit().map_err(|_| AuditError::Storage)?;
        }
        Ok(true)
    }

    pub fn query(&self, query: &AuditQuery) -> Result<AuditPage, AuditError> {
        self.query_until_cancelled(query, &|| false)
    }

    pub(crate) fn query_until_cancelled(
        &self,
        query: &AuditQuery,
        cancelled: &impl Fn() -> bool,
    ) -> Result<AuditPage, AuditError> {
        if cancelled() {
            return Err(AuditError::Closed);
        }
        let limit = query.limit.clamp(1, 200);
        let mut stmt = self
            .db
            .prepare(
                "SELECT sequence,event_id,occurred_at,category,severity,payload FROM audit_events
            WHERE (?1 IS NULL OR category=?1) AND (?2 IS NULL OR severity=?2)
              AND (?3 IS NULL OR sequence<?3) AND (?4 IS NULL OR occurred_at>=?4)
              AND (?5 IS NULL OR occurred_at<=?5)
              AND (?7 IS NULL OR sequence<?7)
              AND (?6 = 0 OR operation_id IS NULL OR NOT EXISTS (
                  SELECT 1 FROM audit_events newer WHERE newer.operation_id=audit_events.operation_id
                  AND newer.sequence>audit_events.sequence AND (?7 IS NULL OR newer.sequence<?7)))
            ORDER BY sequence DESC",
            )
            .map_err(|_| AuditError::Storage)?;
        let mut rows = stmt
            .query(params![
                query.category.map(|c| c.key()),
                query.severity.map(|s| s.key()),
                query.before_sequence,
                query.after_ms,
                query.until_ms,
                query.latest_only,
                query.sequence_ceiling
            ])
            .map_err(|_| AuditError::Storage)?;
        let mut records = Vec::new();
        let mut cursor = None;
        while let Some(row) = rows.next().map_err(|_| AuditError::Storage)? {
            if cancelled() {
                return Err(AuditError::Closed);
            }
            let sequence: i64 = row.get(0).map_err(|_| AuditError::Storage)?;
            let record = self.decode_event(row)?;
            if !matches_query(&record, query) {
                continue;
            }
            if records.len() == limit {
                return Ok(AuditPage {
                    records,
                    next_cursor: cursor,
                });
            }
            records.push(record);
            cursor = Some(sequence);
        }
        Ok(AuditPage {
            records,
            next_cursor: None,
        })
    }

    /// Retention is explicit to callers so the UI can report removed records.
    pub fn prune(&mut self, now_ms: i64) -> Result<usize, AuditError> {
        let policy = self.policy()?;
        let tx = self.db.transaction().map_err(|_| AuditError::Storage)?;
        let mut removed = tx
            .execute(
                "DELETE FROM audit_events WHERE occurred_at < ? AND sequence NOT IN (SELECT event_sequence FROM audit_operation_heads WHERE pending=1)",
                [now_ms.saturating_sub(i64::from(policy.retention_days) * 86_400_000)],
            )
            .map_err(|_| AuditError::Storage)?;
        let total: i64 = tx
            .query_row(
                "SELECT coalesce(sum(length(payload)),0) FROM audit_events",
                [],
                |r| r.get(0),
            )
            .map_err(|_| AuditError::Storage)?;
        if total > policy.max_bytes as i64 {
            removed += tx.execute("DELETE FROM audit_events WHERE sequence IN (
                SELECT sequence FROM (SELECT sequence,sum(length(payload)) OVER (ORDER BY sequence DESC) AS retained FROM audit_events)
                WHERE retained > ? AND sequence NOT IN (SELECT event_sequence FROM audit_operation_heads WHERE pending=1))", [policy.max_bytes as i64]).map_err(|_| AuditError::Storage)?;
        }
        tx.commit().map_err(|_| AuditError::Storage)?;
        Ok(removed)
    }

    pub fn policy(&self) -> Result<AuditPolicy, AuditError> {
        read_policy(&self.db)
    }

    pub fn set_policy(&mut self, policy: AuditPolicy) -> Result<(), AuditError> {
        if !(AuditPolicy::MIN_RETENTION_DAYS..=AuditPolicy::MAX_RETENTION_DAYS)
            .contains(&policy.retention_days)
            || !(AuditPolicy::MIN_BYTES..=AuditPolicy::MAX_BYTES).contains(&policy.max_bytes)
            || !(AuditPolicy::MIN_RETENTION_DAYS..=AuditPolicy::MAX_RETENTION_DAYS)
                .contains(&policy.output_retention_days)
            || !(AuditPolicy::MIN_BYTES..=AuditPolicy::MAX_BYTES).contains(&policy.output_max_bytes)
        {
            return Err(AuditError::InvalidPolicy);
        }
        let json = serde_json::to_string(&policy).map_err(|_| AuditError::Integrity)?;
        self.db.execute("INSERT INTO audit_policy VALUES(1,?) ON CONFLICT(id) DO UPDATE SET policy=excluded.policy", [json])
            .map_err(|_| AuditError::Storage)?;
        Ok(())
    }

    pub fn clear_before(&mut self, before_ms: i64) -> Result<usize, AuditError> {
        self.db
            .execute(
                "DELETE FROM audit_events WHERE occurred_at < ?",
                [before_ms],
            )
            .map_err(|_| AuditError::Storage)
    }

    pub(crate) fn next_sequence(&self) -> Result<i64, AuditError> {
        self.db
            .query_row(
                "SELECT coalesce(max(sequence),0)+1 FROM audit_events",
                [],
                |row| row.get(0),
            )
            .map_err(|_| AuditError::Storage)
    }

    pub(crate) fn clear_before_except(
        &mut self,
        before_ms: i64,
        event_id: &str,
    ) -> Result<usize, AuditError> {
        self.db
            .execute(
                "DELETE FROM audit_events WHERE occurred_at < ? AND event_id != ?",
                params![before_ms, event_id],
            )
            .map_err(|_| AuditError::Storage)
    }

    fn decode_event(&self, row: &rusqlite::Row<'_>) -> Result<AuditRecord, AuditError> {
        let id: String = row.get(1).map_err(|_| AuditError::Storage)?;
        let occurred_at: i64 = row.get(2).map_err(|_| AuditError::Storage)?;
        let category: String = row.get(3).map_err(|_| AuditError::Storage)?;
        let severity: String = row.get(4).map_err(|_| AuditError::Storage)?;
        let payload: Vec<u8> = row.get(5).map_err(|_| AuditError::Storage)?;
        if payload.len() < 40 {
            return Err(AuditError::Integrity);
        }
        let aad = self.aad(&id, occurred_at, &category, &severity);
        let clear = Zeroizing::new(
            self.cipher
                .decrypt(
                    XNonce::from_slice(&payload[..24]),
                    Payload {
                        msg: &payload[24..],
                        aad: &aad,
                    },
                )
                .map_err(|_| AuditError::Integrity)?,
        );
        let record: AuditRecord =
            serde_json::from_slice(&clear).map_err(|_| AuditError::Integrity)?;
        if record.id != id
            || record.occurred_at_ms != occurred_at
            || record.category.key() != category
            || record.severity.key() != severity
        {
            return Err(AuditError::Integrity);
        }
        Ok(record)
    }

    fn aad(&self, id: &str, time: i64, category: &str, severity: &str) -> Vec<u8> {
        format!("audit-v1:{}:{id}:{time}:{category}:{severity}", self.key_id).into_bytes()
    }

    fn session_key(&self, id: &str) -> String {
        // A keyed digest keeps caller-provided session identities out of SQLite indexes and WAL.
        let mut digest = Sha256::new();
        digest.update(&self.index_key);
        digest.update(b"oxideterm-audit-session-v1\0");
        digest.update(id.as_bytes());
        format!("{:x}", digest.finalize())
    }
}

fn matches_query(record: &AuditRecord, query: &AuditQuery) -> bool {
    let operation = record.details.operation.as_ref();
    if query.operation_id.as_ref().is_some_and(|id| {
        operation.is_none_or(|op| &op.id != id && op.parent_id.as_ref() != Some(id))
    }) || query
        .session_id
        .as_ref()
        .is_some_and(|id| operation.is_none_or(|op| op.session_id.as_ref() != Some(id)))
        || query
            .source
            .is_some_and(|source| operation.is_none_or(|op| op.source != source))
        || query
            .outcome
            .is_some_and(|outcome| operation.is_none_or(|op| op.outcome != outcome))
        || query.protocol.as_ref().is_some_and(|protocol| {
            operation.is_none_or(|op| op.protocol.as_ref() != Some(protocol))
        })
        || query
            .connection_id
            .as_ref()
            .is_some_and(|id| record.details.connection_id.as_ref() != Some(id))
        || query
            .remote_account
            .as_ref()
            .is_some_and(|account| record.details.remote_account.as_ref() != Some(account))
        || query
            .local_account
            .as_ref()
            .is_some_and(|account| &record.details.actor != account)
        || query
            .parent_id
            .as_ref()
            .is_some_and(|id| operation.is_none_or(|op| op.parent_id.as_ref() != Some(id)))
    {
        return false;
    }
    query.search.as_ref().is_none_or(|search| {
        let search = Zeroizing::new(search.to_lowercase());
        [
            &record.details.title,
            &record.details.actor,
            &record.details.device,
            &record.details.source,
        ]
        .into_iter()
        .chain(record.details.target.iter())
        .chain(record.details.detail.iter())
        .any(|value| Zeroizing::new(value.to_lowercase()).contains(search.as_str()))
    })
}

fn read_policy(db: &Connection) -> Result<AuditPolicy, AuditError> {
    let value: Option<String> = db
        .prepare_cached("SELECT policy FROM audit_policy WHERE id=1")
        .map_err(|_| AuditError::Storage)?
        .query_row([], |row| row.get(0))
        .optional()
        .map_err(|_| AuditError::Storage)?;
    match value {
        Some(value) => serde_json::from_str(&value).map_err(|_| AuditError::Integrity),
        None => Ok(AuditPolicy::default()),
    }
}
