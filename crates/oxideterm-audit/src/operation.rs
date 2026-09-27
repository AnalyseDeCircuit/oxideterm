use crate::{
    AuditAuthorization, AuditCapture, AuditCategory, AuditClient, AuditDetails, AuditEvidence,
    AuditOutcome, AuditPhase, AuditRecord, AuditSeverity, AuditSource, OperationDetails, redact,
};
use std::sync::{Arc, LazyLock, RwLock, atomic::AtomicBool};
use std::time::Instant;
use zeroize::Zeroizing;

static PROCESS: LazyLock<RwLock<Option<Arc<AuditContext>>>> = LazyLock::new(|| RwLock::new(None));

/// The application owns registration; its writer closes even if a task retains a context.
pub struct AuditRegistration {
    context: Arc<AuditContext>,
}

impl Drop for AuditRegistration {
    fn drop(&mut self) {
        self.context.observe(
            AuditCategory::System,
            "application_stop",
            None,
            AuditOutcome::Succeeded,
            AuditEvidence::Lifecycle,
            AuditAuthorization::NotRequired,
        );
        let mut current = PROCESS.write().unwrap_or_else(|e| e.into_inner());
        if current
            .as_ref()
            .is_some_and(|context| Arc::ptr_eq(context, &self.context))
        {
            *current = None;
        }
    }
}

#[derive(Clone)]
pub struct AuditContext {
    client: AuditClient,
    instance_id: String,
    pub session_id: Option<String>,
    pub consumer_id: Option<String>,
    pub transport_id: Option<String>,
    pub protocol: Option<String>,
    pub parent_id: Option<String>,
    pub source: AuditSource,
    pub agent_id: Option<Zeroizing<String>>,
    pub target: Option<Zeroizing<String>>,
    pub node_id: Option<Zeroizing<String>>,
    pub connection_id: Option<Zeroizing<String>>,
    pub remote_account: Option<Zeroizing<String>>,
    actor: Zeroizing<String>,
    device: Zeroizing<String>,
}

impl AuditContext {
    pub fn new(client: AuditClient, source: AuditSource) -> Self {
        let instance_id = client.instance_id().to_string();
        Self {
            client,
            instance_id,
            session_id: None,
            consumer_id: None,
            transport_id: None,
            protocol: None,
            parent_id: None,
            source,
            agent_id: None,
            target: None,
            node_id: None,
            connection_id: None,
            remote_account: None,
            actor: Zeroizing::new(whoami::username()),
            device: Zeroizing::new(whoami::fallible::hostname().unwrap_or_default()),
        }
    }

    /// Starts a transport-owned identity, independent of subsequent UI focus.
    pub fn session(&self, protocol: &str, target: &str) -> Self {
        let mut context = self.clone();
        context.session_id = Some(uuid::Uuid::new_v4().to_string());
        context.consumer_id = None;
        context.transport_id = None;
        context.protocol = Some(protocol.to_string());
        context.target = Some(redact(target));
        context.node_id = None;
        context.connection_id = None;
        context.remote_account = None;
        context
    }

    pub fn consumer(&self) -> Self {
        let mut context = self.clone();
        context.consumer_id = Some(uuid::Uuid::new_v4().to_string());
        context
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn recording_sink(&self) -> crate::RecordingSink {
        self.client.recording_sink(self)
    }

    pub fn install(self) -> AuditRegistration {
        let context = Arc::new(self);
        context.observe(
            AuditCategory::System,
            "application_start",
            None,
            AuditOutcome::Succeeded,
            AuditEvidence::Lifecycle,
            AuditAuthorization::NotRequired,
        );
        *PROCESS.write().unwrap_or_else(|e| e.into_inner()) = Some(context.clone());
        AuditRegistration { context }
    }

    pub fn current() -> Option<Self> {
        PROCESS
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|context| context.as_ref().clone())
    }

    pub fn operation(
        &self,
        category: AuditCategory,
        action: &str,
        detail: Option<&str>,
    ) -> AuditOperation {
        let mut record = self.event_record(category, action, detail);
        // Queue failures update the same health state observed by the application.
        let (suppressed, result) = self.client.record_start(record.clone());
        if result == Err(crate::AuditError::TooLarge) {
            return AuditOperation { state: None };
        }
        if result.is_err() {
            if let Some(operation) = &mut record.details.operation {
                operation.capture = Some(AuditCapture::Partial);
            }
        }
        AuditOperation {
            state: Some((self.client.clone(), record, Instant::now(), suppressed)),
        }
    }
    pub fn observe(
        &self,
        category: AuditCategory,
        action: &str,
        detail: Option<&str>,
        outcome: AuditOutcome,
        evidence: AuditEvidence,
        authorization: AuditAuthorization,
    ) {
        let mut record = self.event_record(category, action, detail);
        if let Some(op) = &mut record.details.operation {
            op.outcome = outcome;
            op.evidence = evidence;
            op.authorization = authorization;
            op.phase = Some(AuditPhase::Observation);
        }
        if outcome == AuditOutcome::Failed {
            record.severity = AuditSeverity::Error;
        }
        let _ = self.client.record(record);
    }

    pub(crate) fn event_record(
        &self,
        category: AuditCategory,
        action: &str,
        detail: Option<&str>,
    ) -> AuditRecord {
        let operation = OperationDetails {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: self.parent_id.clone(),
            instance_id: self.instance_id.clone(),
            session_id: self.session_id.clone(),
            consumer_id: self.consumer_id.clone(),
            transport_id: self.transport_id.clone(),
            protocol: self.protocol.clone(),
            agent_id: self.agent_id.clone(),
            source: self.source,
            action: action.to_string(),
            outcome: AuditOutcome::Started,
            evidence: AuditEvidence::Request,
            authorization: if matches!(
                self.source,
                AuditSource::Ai | AuditSource::Mcp | AuditSource::Plugin
            ) {
                AuditAuthorization::Unknown
            } else {
                AuditAuthorization::NotRequired
            },
            authorization_ref: None,
            duration_ms: None,
            exit_code: None,
            bytes: None,
            phase: Some(AuditPhase::Start),
            capture: Some(AuditCapture::Complete),
            recovered_by: None,
        };
        AuditRecord::new(
            category,
            AuditSeverity::Info,
            AuditDetails {
                title: Zeroizing::new(format!("event_log.actions.{action}")),
                detail: detail.map(redact),
                source: Zeroizing::new(self.source.key().to_string()),
                actor: self.actor.clone(),
                device: self.device.clone(),
                target: self.target.as_deref().map(|s| redact(s)),
                node_id: self.node_id.clone(),
                connection_id: self.connection_id.clone(),
                remote_account: self.remote_account.clone(),
                operation: Some(operation),
            },
        )
    }
}

/// Retained by the actual executor, including across UI teardown. Dropping an
/// unfinished future ends observation; it cannot prove a remote effect stopped.
pub struct AuditOperation {
    state: Option<(AuditClient, AuditRecord, Instant, Arc<AtomicBool>)>,
}

impl AuditOperation {
    pub fn in_context(
        context: Option<&AuditContext>,
        category: AuditCategory,
        action: &str,
        detail: Option<&str>,
    ) -> Self {
        context
            .map(|context| context.operation(category, action, detail))
            .unwrap_or(Self { state: None })
    }

    pub fn begin(
        category: AuditCategory,
        action: &str,
        target: Option<&str>,
        detail: Option<&str>,
    ) -> Self {
        match AuditContext::current_request().or_else(AuditContext::current) {
            Some(mut context) => {
                context.target = target.map(redact);
                context.operation(category, action, detail)
            }
            None => Self { state: None },
        }
    }

    pub fn summary(&mut self, detail: &str) {
        if let Some((_, record, _, _)) = &mut self.state {
            record.details.detail = Some(redact(detail));
        }
    }

    /// Append a decision on the same operation before dispatching any authorized work.
    pub fn authorization(&mut self, decision: AuditAuthorization, policy_ref: Option<&str>) {
        let Some((client, record, _, suppressed)) = &mut self.state else {
            return;
        };
        let Some(operation) = &mut record.details.operation else {
            return;
        };
        operation.authorization = decision;
        operation.authorization_ref = policy_ref.map(str::to_owned);
        let mut event = record.clone();
        event.id = uuid::Uuid::new_v4().to_string();
        event.occurred_at_ms = crate::model::now_ms();
        if let Some(operation) = &mut event.details.operation {
            operation.phase = Some(AuditPhase::Authorization);
        }
        let _ = client.record_result(event, suppressed.clone());
    }

    pub fn id(&self) -> Option<&str> {
        self.state
            .as_ref()?
            .1
            .details
            .operation
            .as_ref()
            .map(|op| op.id.as_str())
    }

    pub fn finish(
        mut self,
        outcome: AuditOutcome,
        evidence: AuditEvidence,
        exit_code: Option<i32>,
        bytes: Option<u64>,
    ) {
        self.complete(outcome, evidence, exit_code, bytes);
    }

    pub fn changed<E>(self, result: &Result<bool, E>) {
        let outcome = match result {
            Ok(true) => AuditOutcome::Succeeded,
            Ok(false) => AuditOutcome::Unchanged,
            Err(_) => AuditOutcome::Failed,
        };
        self.finish(outcome, AuditEvidence::Protocol, None, None);
    }

    pub fn result<T, E>(self, result: &Result<T, E>) {
        self.finish(
            if result.is_ok() {
                AuditOutcome::Succeeded
            } else {
                AuditOutcome::Failed
            },
            AuditEvidence::Protocol,
            None,
            None,
        );
    }

    fn complete(
        &mut self,
        outcome: AuditOutcome,
        evidence: AuditEvidence,
        exit_code: Option<i32>,
        bytes: Option<u64>,
    ) {
        let Some((client, mut record, started, suppressed)) = self.state.take() else {
            return;
        };
        record.finish_operation(started, outcome, evidence, exit_code, bytes);
        let _ = client.record_result(record, suppressed);
    }
}

impl AuditRecord {
    pub(crate) fn finish_operation(
        &mut self,
        started: Instant,
        outcome: AuditOutcome,
        evidence: AuditEvidence,
        exit_code: Option<i32>,
        bytes: Option<u64>,
    ) {
        self.id = uuid::Uuid::new_v4().to_string();
        self.occurred_at_ms = crate::model::now_ms();
        self.severity = match outcome {
            AuditOutcome::Failed => AuditSeverity::Error,
            AuditOutcome::Denied | AuditOutcome::Interrupted | AuditOutcome::Partial => {
                AuditSeverity::Warning
            }
            _ => AuditSeverity::Info,
        };
        if self.details.operation.as_ref().is_some_and(|op| {
            matches!(
                op.capture,
                Some(AuditCapture::Partial | AuditCapture::Interrupted)
            )
        }) && self.severity != AuditSeverity::Error
        {
            self.severity = AuditSeverity::Warning;
        }
        if let Some(op) = &mut self.details.operation {
            op.outcome = outcome;
            op.evidence = evidence;
            op.exit_code = exit_code;
            op.bytes = bytes;
            op.duration_ms = Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
            op.phase = Some(AuditPhase::Result);
        }
    }
}

impl Drop for AuditOperation {
    fn drop(&mut self) {
        if let Some((_, record, _, _)) = &mut self.state {
            if let Some(operation) = &mut record.details.operation {
                operation.capture = Some(AuditCapture::Interrupted);
            }
        }
        self.complete(AuditOutcome::Unknown, AuditEvidence::Lifecycle, None, None);
    }
}

impl std::fmt::Debug for AuditOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuditOperation")
            .field("id", &self.id())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for AuditContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuditContext")
            .field("instance_id", &self.instance_id)
            .field("session_id", &self.session_id)
            .field("consumer_id", &self.consumer_id)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}
