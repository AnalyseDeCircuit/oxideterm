use super::*;
use crate::{AuditEvidence, AuditOutcome, AuditSeverity};

impl AuditStore {
    pub(crate) fn open_owned(
        path: &Path,
        keys: &impl AuditKeyProvider,
        instance_id: &str,
    ) -> Result<Self, AuditError> {
        let lease = InstanceLease::try_acquire(path, instance_id)?.ok_or(AuditError::Storage)?;
        let mut store = Self::open(path, keys)?;
        store.lease = Some(std::sync::Arc::new(lease));
        store.recover_abandoned(path)?;
        Ok(store)
    }

    pub(crate) fn recover_abandoned(&mut self, path: &Path) -> Result<usize, AuditError> {
        let Some(current_id) = self.lease.as_ref().map(|lease| lease.id.clone()) else {
            return Ok(0);
        };
        let mut after = String::new();
        let mut count = 0;
        loop {
            let instances = {
                let mut statement = self.db.prepare("SELECT DISTINCT instance_id FROM audit_operation_heads
                    WHERE pending=1 AND instance_id>? AND instance_id<>? ORDER BY instance_id LIMIT 64")
                    .map_err(|_| AuditError::Storage)?;
                statement
                    .query_map(params![after, current_id], |row| row.get::<_, String>(0))
                    .map_err(|_| AuditError::Storage)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| AuditError::Storage)?
            };
            if instances.is_empty() {
                break;
            }
            for id in instances {
                after = id.clone();
                if let Some(_lease) = InstanceLease::try_acquire(path, &id)? {
                    count += self.finish_abandoned_instance(&id, &current_id)?;
                }
            }
        }
        // Crash remnants without pending operations also have no reason to
        // retain an empty lock file. Active owners remain locked and untouched.
        for entry in
            std::fs::read_dir(InstanceLease::directory(path)?).map_err(|_| AuditError::Storage)?
        {
            let entry = entry.map_err(|_| AuditError::Storage)?;
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".lock")) else {
                continue;
            };
            if id != current_id && uuid::Uuid::parse_str(id).is_ok() {
                // A new writer creates its file before it can lock it. Only
                // touch owners already seen in committed database records;
                // otherwise cleanup could unlink that startup file mid-acquire.
                let registered: bool = self
                    .db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM audit_operation_heads WHERE instance_id=?)",
                        [id],
                        |row| row.get(0),
                    )
                    .map_err(|_| AuditError::Storage)?;
                if registered {
                    let _ = InstanceLease::try_acquire(path, id)?;
                }
            }
        }
        Ok(count)
    }

    pub(crate) fn finish_owned_operations(&mut self) -> Result<usize, AuditError> {
        match self.lease.as_ref().map(|lease| lease.id.clone()) {
            Some(id) => self.finish_abandoned_instance(&id, &id),
            None => Ok(0),
        }
    }

    fn finish_abandoned_instance(
        &mut self,
        instance_id: &str,
        observer_id: &str,
    ) -> Result<usize, AuditError> {
        let mut count = 0;
        loop {
            let candidates = {
                let mut statement = self
                    .db
                    .prepare(
                        "SELECT event_sequence,operation_id FROM audit_operation_heads
                    WHERE instance_id=? AND pending=1 ORDER BY event_sequence LIMIT 64",
                    )
                    .map_err(|_| AuditError::Storage)?;
                statement
                    .query_map([instance_id], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(|_| AuditError::Storage)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| AuditError::Storage)?
            };
            if candidates.is_empty() {
                break;
            }
            for (sequence, operation_id) in candidates {
                if self.finish_pending(
                    &operation_id,
                    instance_id,
                    sequence,
                    AuditCapture::Interrupted,
                    Some(observer_id),
                )? {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    pub(crate) fn finish_disabled_operation(
        &self,
        incoming: &AuditRecord,
    ) -> Result<bool, AuditError> {
        let operation = incoming
            .details
            .operation
            .as_ref()
            .ok_or(AuditError::Integrity)?;
        let sequence: Option<i64> = self.db.query_row(
            "SELECT event_sequence FROM audit_operation_heads WHERE operation_id=? AND instance_id=? AND pending=1",
            params![operation.id, operation.instance_id], |row| row.get(0),
        ).optional().map_err(|_| AuditError::Storage)?;
        match sequence {
            Some(sequence) => self.finish_pending(
                &operation.id,
                &operation.instance_id,
                sequence,
                AuditCapture::Disabled,
                None,
            ),
            None => Ok(false),
        }
    }

    fn finish_pending(
        &self,
        operation_id: &str,
        instance_id: &str,
        sequence: i64,
        capture: AuditCapture,
        observer_id: Option<&str>,
    ) -> Result<bool, AuditError> {
        // Only previously captured data is copied. A result received after
        // collection was disabled must not introduce a new exit code or detail.
        let mut record = {
            let mut statement = self.db.prepare("SELECT sequence,event_id,occurred_at,category,severity,payload FROM audit_events WHERE sequence=?")
                .map_err(|_| AuditError::Storage)?;
            let mut rows = statement
                .query([sequence])
                .map_err(|_| AuditError::Storage)?;
            match rows.next().map_err(|_| AuditError::Storage)? {
                Some(row) => self.decode_event(row)?,
                None => {
                    // A concurrent clear cascades to the head. A dangling head
                    // instead indicates corruption and must not cause a retry loop.
                    let dangling: bool = self.db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM audit_operation_heads WHERE operation_id=? AND event_sequence=? AND pending=1)",
                        params![operation_id, sequence], |row| row.get(0),
                    ).map_err(|_| AuditError::Storage)?;
                    return if dangling {
                        Err(AuditError::Integrity)
                    } else {
                        Ok(false)
                    };
                }
            }
        };
        let operation = record
            .details
            .operation
            .as_mut()
            .ok_or(AuditError::Integrity)?;
        if operation.id != operation_id
            || operation.instance_id != instance_id
            || !matches!(
                operation.phase,
                Some(AuditPhase::Start | AuditPhase::Progress | AuditPhase::Authorization)
            )
        {
            return Err(AuditError::Integrity);
        }
        operation.phase = Some(AuditPhase::Result);
        operation.outcome = AuditOutcome::Unknown;
        operation.evidence = AuditEvidence::Lifecycle;
        operation.capture = Some(capture);
        operation.recovered_by = observer_id.map(str::to_string);
        operation.duration_ms = None;
        operation.exit_code = None;
        record.id = uuid::Uuid::new_v4().to_string();
        record.occurred_at_ms = crate::model::now_ms();
        record.severity = AuditSeverity::Warning;
        self.append_record(&record, true, Some(sequence))
    }
}
