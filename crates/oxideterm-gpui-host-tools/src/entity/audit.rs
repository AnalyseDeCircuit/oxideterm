use super::*;
use oxideterm_audit::{AuditEvidence, AuditOperation, AuditOutcome};

pub(super) enum AuditCaptureKind {
    Query,
    Action,
}

pub(super) struct HostAudit(AuditOperation, AuditCaptureKind);

impl HostAudit {
    pub(super) fn finish_capture(self, result: &Result<HostCommandOutput, ()>) {
        let output = result.as_ref().ok();
        let code = output.and_then(|o| o.exit_code);
        let success = if result.is_err() {
            Some(false)
        } else {
            code.map(|code| code == 0)
        };
        let bytes = match self.1 {
            AuditCaptureKind::Action => None,
            AuditCaptureKind::Query => {
                output.map(|o| o.stdout.len().saturating_add(o.stderr.len()) as u64)
            }
        };
        self.finish(success, code, bytes);
    }

    pub(super) fn unavailable(self) {
        self.0
            .finish(AuditOutcome::Failed, AuditEvidence::Request, None, None);
    }
    pub(super) fn finish(self, success: Option<bool>, code: Option<i32>, bytes: Option<u64>) {
        let outcome = match success {
            Some(true) => AuditOutcome::Succeeded,
            Some(false) => AuditOutcome::Failed,
            None => AuditOutcome::Unknown,
        };
        self.0.finish(
            outcome,
            if code.is_some() {
                AuditEvidence::ExitCode
            } else {
                AuditEvidence::Protocol
            },
            code,
            bytes,
        );
    }
}
impl HostToolsEntity {
    pub(super) fn host_tool_audit(
        &self,
        kind: AuditCaptureKind,
        connection_id: &str,
        action: &str,
        detail: Option<&str>,
    ) -> HostAudit {
        let context = self
            .desktop_registry()
            .and_then(|registry| registry.get(connection_id))
            .and_then(|handle| handle.audit_context());
        HostAudit(
            if let Some(context) = context {
                oxideterm_audit::AuditOperation::in_request(
                    Some(&context),
                    oxideterm_audit::AuditCategory::Host,
                    action,
                    detail,
                )
            } else {
                oxideterm_audit::AuditOperation::begin(
                    oxideterm_audit::AuditCategory::Host,
                    action,
                    Some(connection_id),
                    detail,
                )
            },
            kind,
        )
    }
}
