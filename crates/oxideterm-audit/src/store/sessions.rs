use super::*;
use crate::{AuditCategory, AuditSessionPage, AuditSessionQuery, AuditSessionSummary};

impl AuditStore {
    pub fn list_sessions(&self, query: &AuditSessionQuery) -> Result<AuditSessionPage, AuditError> {
        self.list_sessions_until_cancelled(query, &|| false)
    }

    pub(crate) fn list_sessions_until_cancelled(
        &self,
        query: &AuditSessionQuery,
        cancelled: &impl Fn() -> bool,
    ) -> Result<AuditSessionPage, AuditError> {
        let limit = query.limit.clamp(1, 100);
        let mut statement = self
            .db
            .prepare(
                "SELECT session_key, max(sequence) AS latest FROM audit_events
             WHERE session_key IS NOT NULL
               AND (?1 IS NULL OR occurred_at >= ?1)
               AND (?2 IS NULL OR occurred_at <= ?2)
             GROUP BY session_key
             HAVING (?3 IS NULL OR latest < ?3 OR (latest = ?3 AND session_key < ?4))
             ORDER BY latest DESC, session_key DESC LIMIT ?5",
            )
            .map_err(|_| AuditError::Storage)?;
        let mut rows = statement
            .query(params![
                query.after_ms,
                query.until_ms,
                query.before.as_ref().map(|cursor| cursor.0),
                query.before.as_ref().map(|cursor| cursor.1.as_str()),
                (limit + 1) as i64,
            ])
            .map_err(|_| AuditError::Storage)?;
        let mut keys = Vec::with_capacity(limit + 1);
        while let Some(row) = rows.next().map_err(|_| AuditError::Storage)? {
            if cancelled() {
                return Err(AuditError::Closed);
            }
            keys.push((
                row.get::<_, String>(0).map_err(|_| AuditError::Storage)?,
                row.get::<_, i64>(1).map_err(|_| AuditError::Storage)?,
            ));
        }
        let has_more = keys.len() > limit;
        keys.truncate(limit);
        let mut sessions = Vec::with_capacity(keys.len());
        for (key, _) in &keys {
            if cancelled() {
                return Err(AuditError::Closed);
            }
            sessions.push(self.summarize_session(key, cancelled)?);
        }
        let next_cursor = if has_more {
            keys.last().map(|(key, sequence)| (*sequence, key.clone()))
        } else {
            None
        };
        Ok(AuditSessionPage {
            sessions,
            next_cursor,
        })
    }

    fn summarize_session(
        &self,
        key: &str,
        cancelled: &impl Fn() -> bool,
    ) -> Result<AuditSessionSummary, AuditError> {
        let mut statement = self
            .db
            .prepare(
                "SELECT sequence,event_id,occurred_at,category,severity,payload
             FROM audit_events WHERE session_key=?
             ORDER BY sequence DESC LIMIT 1",
            )
            .map_err(|_| AuditError::Storage)?;
        let mut rows = statement.query([key]).map_err(|_| AuditError::Storage)?;
        let latest = self.decode_event(
            rows.next()
                .map_err(|_| AuditError::Storage)?
                .ok_or(AuditError::Integrity)?,
        )?;
        let operation = latest
            .details
            .operation
            .as_ref()
            .ok_or(AuditError::Integrity)?;
        let session_id = operation
            .session_id
            .as_ref()
            .ok_or(AuditError::Integrity)?
            .clone();
        if self.session_key(&session_id) != key {
            return Err(AuditError::Integrity);
        }

        let (started_at_ms, last_event_at_ms): (i64, i64) = self
            .db
            .query_row(
                "SELECT min(occurred_at), max(occurred_at)
             FROM audit_events WHERE session_key=?",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| AuditError::Storage)?;
        let mut summary = AuditSessionSummary {
            session_id,
            started_at_ms,
            last_event_at_ms,
            protocol: operation.protocol.clone(),
            target: latest.details.target.clone(),
            local_account: latest.details.actor.clone(),
            remote_account: latest.details.remote_account.clone(),
            transport_count: 0,
            command_count: 0,
            file_count: 0,
            operation_count: 0,
            capture: crate::AuditCapture::Complete,
        };
        let mut statement = self
            .db
            .prepare(
                "SELECT sequence,event_id,occurred_at,category,severity,payload,operation_id
             FROM audit_events WHERE session_key=? AND operation_id IS NOT NULL
               AND sequence IN (
                   SELECT max(sequence) FROM audit_events
                   WHERE session_key=? GROUP BY operation_id
               ) ORDER BY sequence DESC",
            )
            .map_err(|_| AuditError::Storage)?;
        let mut rows = statement
            .query(params![key, key])
            .map_err(|_| AuditError::Storage)?;
        while let Some(row) = rows.next().map_err(|_| AuditError::Storage)? {
            if cancelled() {
                return Err(AuditError::Closed);
            }
            let record = self.decode_event(row)?;
            let op = record
                .details
                .operation
                .as_ref()
                .ok_or(AuditError::Integrity)?;
            let indexed_id: String = row.get(6).map_err(|_| AuditError::Storage)?;
            if op.session_id.as_ref() != Some(&summary.session_id) || op.id != indexed_id {
                return Err(AuditError::Integrity);
            }
            summary.operation_count += 1;
            match record.category {
                AuditCategory::Command => summary.command_count += 1,
                AuditCategory::File => summary.file_count += 1,
                _ => {}
            }
            summary.capture = weaker_capture(summary.capture, op.capture);
        }
        let mut statement = self.db.prepare(
            "SELECT e.sequence,e.event_id,e.occurred_at,e.category,e.severity,e.payload,e.transport_id
             FROM audit_events e JOIN (
                 SELECT transport_id,max(sequence) AS latest FROM audit_events
                 WHERE session_key=? AND transport_id IS NOT NULL GROUP BY transport_id
             ) t ON e.sequence=t.latest",
        ).map_err(|_| AuditError::Storage)?;
        let mut rows = statement.query([key]).map_err(|_| AuditError::Storage)?;
        while let Some(row) = rows.next().map_err(|_| AuditError::Storage)? {
            if cancelled() {
                return Err(AuditError::Closed);
            }
            let record = self.decode_event(row)?;
            let op = record
                .details
                .operation
                .as_ref()
                .ok_or(AuditError::Integrity)?;
            let indexed_id: String = row.get(6).map_err(|_| AuditError::Storage)?;
            if op.session_id.as_ref() != Some(&summary.session_id)
                || op.transport_id.as_ref() != Some(&indexed_id)
            {
                return Err(AuditError::Integrity);
            }
            summary.transport_count += 1;
        }
        Ok(summary)
    }
}

fn weaker_capture(
    current: crate::AuditCapture,
    observed: Option<crate::AuditCapture>,
) -> crate::AuditCapture {
    use crate::AuditCapture;
    let observed = observed.unwrap_or(AuditCapture::Partial);
    let rank = |capture| match capture {
        AuditCapture::Complete => 0,
        AuditCapture::Disabled => 1,
        AuditCapture::Partial => 2,
        AuditCapture::Interrupted => 3,
    };
    if rank(observed) > rank(current) {
        observed
    } else {
        current
    }
}
