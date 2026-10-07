use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use parking_lot::Mutex;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{
    auth::ToolGroup,
    calls::PublicToolCall,
    handles::{ApprovalRef, ClientRef},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

#[derive(Clone, Default)]
pub struct ApprovalReview {
    command: Option<Zeroizing<String>>,
    working_directory: Option<Zeroizing<String>>,
}

impl ApprovalReview {
    pub fn command(&self) -> Option<&str> {
        self.command.as_deref().map(String::as_str)
    }

    pub fn working_directory(&self) -> Option<&str> {
        self.working_directory.as_deref().map(String::as_str)
    }
}

impl std::fmt::Debug for ApprovalReview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApprovalReview")
            .field("command", &self.command.as_ref().map(|_| "[REDACTED]"))
            .field(
                "working_directory",
                &self.working_directory.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalProjection {
    pub approval_ref: ApprovalRef,
    pub client_ref: ClientRef,
    pub tool_name: String,
    pub target: String,
    pub status: ApprovalStatus,
    pub created_at_ms: u128,
    pub expires_at_ms: u128,
    #[serde(skip_serializing)]
    pub audit_operation_id: Option<String>,
    /// This material is available only to the local approval UI.
    #[serde(skip_serializing)]
    pub review: ApprovalReview,
}

struct PendingApproval {
    projection: ApprovalProjection,
    call: Option<PublicToolCall>,
    audit: Option<oxideterm_audit::AuditOperation>,
}

#[derive(Default)]
pub struct ApprovalStore {
    entries: Mutex<HashMap<ApprovalRef, PendingApproval>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ApprovalError {
    #[error("the approval request does not exist")]
    NotFound,
    #[error("the approval request belongs to another client")]
    WrongClient,
    #[error("the approval request is still pending")]
    Pending,
    #[error("the approval request was rejected")]
    Rejected,
    #[error("the approved action was already consumed")]
    Consumed,
    #[error("the approval request expired")]
    Expired,
    #[error("too many approval requests are waiting for user action")]
    CapacityReached,
}

const APPROVAL_LIFETIME: Duration = Duration::from_secs(5 * 60);
const APPROVAL_CAPACITY: usize = 256;
const APPROVAL_CAPACITY_PER_CLIENT: usize = 32;

impl ApprovalStore {
    /// Freezes the typed action so commit cannot substitute different parameters later.
    pub fn stage(
        &self,
        client_ref: ClientRef,
        call: PublicToolCall,
    ) -> Result<ApprovalProjection, ApprovalError> {
        self.stage_with_context(
            client_ref,
            call,
            oxideterm_audit::AuditContext::current_request()
                .or_else(oxideterm_audit::AuditContext::current)
                .as_ref(),
        )
    }

    pub fn stage_with_context(
        &self,
        client_ref: ClientRef,
        call: PublicToolCall,
        audit_context: Option<&oxideterm_audit::AuditContext>,
    ) -> Result<ApprovalProjection, ApprovalError> {
        let created_at_ms = unix_time_ms();
        let mut entries = self.entries.lock();
        expire_entries(&mut entries);
        entries.retain(|_, entry| entry.call.is_some());
        let client_entry_count = entries
            .values()
            .filter(|entry| entry.projection.client_ref == client_ref)
            .count();
        if entries.len() >= APPROVAL_CAPACITY || client_entry_count >= APPROVAL_CAPACITY_PER_CLIENT
        {
            return Err(ApprovalError::CapacityReached);
        }
        let mut audit = audit_context.cloned().map(|mut context| {
            context.source = oxideterm_audit::AuditSource::Mcp;
            context.agent_id = Some(oxideterm_audit::redact(client_ref.as_str()));
            context.operation(
                oxideterm_audit::AuditCategory::Automation,
                "mcp_call",
                Some(call.tool_name()),
            )
        });
        if let Some(audit) = audit.as_mut() {
            audit.authorization(
                oxideterm_audit::AuditAuthorization::Pending,
                Some("mcp_app_approval"),
            );
        }
        let projection = ApprovalProjection {
            approval_ref: ApprovalRef::new(),
            client_ref,
            tool_name: call.tool_name().to_owned(),
            target: call.target_summary(),
            status: ApprovalStatus::Pending,
            created_at_ms,
            expires_at_ms: created_at_ms.saturating_add(APPROVAL_LIFETIME.as_millis()),
            audit_operation_id: audit
                .as_ref()
                .and_then(|operation| operation.id().map(str::to_owned)),
            review: approval_review(&call),
        };
        entries.insert(
            projection.approval_ref.clone(),
            PendingApproval {
                projection: projection.clone(),
                call: Some(call),
                audit,
            },
        );
        Ok(projection)
    }

    pub fn list(&self) -> Vec<ApprovalProjection> {
        let mut store = self.entries.lock();
        expire_entries(&mut store);
        let mut entries = store
            .values()
            .map(|entry| entry.projection.clone())
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.created_at_ms);
        entries
    }

    /// Drops frozen action payloads when their approval lifetime ends, even while idle.
    pub fn expire(&self) {
        expire_entries(&mut self.entries.lock());
    }

    pub fn set_status(
        &self,
        approval_ref: &ApprovalRef,
        status: ApprovalStatus,
    ) -> Result<(), ApprovalError> {
        let mut entries = self.entries.lock();
        expire_entries(&mut entries);
        let entry = entries
            .get_mut(approval_ref)
            .ok_or(ApprovalError::NotFound)?;
        match entry.projection.status {
            ApprovalStatus::Expired => return Err(ApprovalError::Expired),
            ApprovalStatus::Rejected => return Err(ApprovalError::Rejected),
            ApprovalStatus::Approved if status == ApprovalStatus::Approved => return Ok(()),
            ApprovalStatus::Approved => return Err(ApprovalError::Consumed),
            ApprovalStatus::Pending => {}
        }
        entry.projection.status = status;
        if let Some(audit) = entry.audit.as_mut() {
            let decision = match status {
                ApprovalStatus::Pending => oxideterm_audit::AuditAuthorization::Pending,
                ApprovalStatus::Approved => oxideterm_audit::AuditAuthorization::Approved,
                ApprovalStatus::Rejected => oxideterm_audit::AuditAuthorization::Denied,
                ApprovalStatus::Expired => oxideterm_audit::AuditAuthorization::TimedOut,
            };
            audit.authorization(decision, Some("mcp_app_approval"));
        }
        if matches!(status, ApprovalStatus::Rejected | ApprovalStatus::Expired) {
            // Dropping the frozen call also zeroizes command text owned by that call.
            entry.call.take();
            if let Some(audit) = entry.audit.take() {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Denied,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
        }
        Ok(())
    }

    pub fn revoke_client(&self, client_ref: &ClientRef) -> usize {
        let mut entries = self.entries.lock();
        let mut revoked = 0;
        for entry in entries.values_mut() {
            if &entry.projection.client_ref == client_ref
                && matches!(
                    entry.projection.status,
                    ApprovalStatus::Pending | ApprovalStatus::Approved
                )
            {
                entry.projection.status = ApprovalStatus::Rejected;
                entry.call.take();
                if let Some(mut audit) = entry.audit.take() {
                    audit.authorization(
                        oxideterm_audit::AuditAuthorization::Cancelled,
                        Some("mcp_app_approval"),
                    );
                    audit.finish(
                        oxideterm_audit::AuditOutcome::Cancelled,
                        oxideterm_audit::AuditEvidence::Lifecycle,
                        None,
                        None,
                    );
                }
                revoked += 1;
            }
        }
        revoked
    }

    pub fn revoke_client_tool_group(&self, client_ref: &ClientRef, tool_group: ToolGroup) -> usize {
        let mut entries = self.entries.lock();
        let mut revoked = 0;
        for entry in entries.values_mut() {
            let matches_group = entry.call.as_ref().is_some_and(|call| {
                call.required_group() == tool_group
                    || call.additional_required_groups().contains(&tool_group)
            });
            if &entry.projection.client_ref == client_ref
                && matches_group
                && matches!(
                    entry.projection.status,
                    ApprovalStatus::Pending | ApprovalStatus::Approved
                )
            {
                entry.projection.status = ApprovalStatus::Rejected;
                entry.call.take();
                if let Some(mut audit) = entry.audit.take() {
                    audit.authorization(
                        oxideterm_audit::AuditAuthorization::Cancelled,
                        Some("mcp_app_approval"),
                    );
                    audit.finish(
                        oxideterm_audit::AuditOutcome::Cancelled,
                        oxideterm_audit::AuditEvidence::Lifecycle,
                        None,
                        None,
                    );
                }
                revoked += 1;
            }
        }
        revoked
    }

    pub fn take_approved(
        &self,
        client_ref: &ClientRef,
        approval_ref: &ApprovalRef,
    ) -> Result<PublicToolCall, ApprovalError> {
        let (call, audit) = self.take_approved_with_audit(client_ref, approval_ref)?;
        if let Some(audit) = audit {
            audit.finish(
                oxideterm_audit::AuditOutcome::Sent,
                oxideterm_audit::AuditEvidence::Dispatch,
                None,
                None,
            );
        }
        Ok(call)
    }

    pub fn take_approved_with_audit(
        &self,
        client_ref: &ClientRef,
        approval_ref: &ApprovalRef,
    ) -> Result<(PublicToolCall, Option<oxideterm_audit::AuditOperation>), ApprovalError> {
        let mut entries = self.entries.lock();
        expire_entries(&mut entries);
        let entry = entries
            .get_mut(approval_ref)
            .ok_or(ApprovalError::NotFound)?;
        if &entry.projection.client_ref != client_ref {
            return Err(ApprovalError::WrongClient);
        }
        match entry.projection.status {
            ApprovalStatus::Pending => Err(ApprovalError::Pending),
            ApprovalStatus::Rejected => Err(ApprovalError::Rejected),
            ApprovalStatus::Expired => Err(ApprovalError::Expired),
            ApprovalStatus::Approved => entry
                .call
                .take()
                .map(|call| (call, entry.audit.take()))
                .ok_or(ApprovalError::Consumed),
        }
    }
}

fn approval_review(call: &PublicToolCall) -> ApprovalReview {
    match call {
        PublicToolCall::StartCommand(args) => ApprovalReview {
            command: Some(Zeroizing::new(args.command.to_string())),
            working_directory: args
                .working_directory
                .as_ref()
                .map(|directory| Zeroizing::new(directory.to_string())),
        },
        PublicToolCall::QuickCommandsSave(args) => ApprovalReview {
            command: Some(Zeroizing::new(args.command.to_string())),
            working_directory: None,
        },
        PublicToolCall::PreparedQuickCommandRun(args) => ApprovalReview {
            // The approved payload is already target-expanded and remains frozen in this entry.
            command: Some(Zeroizing::new(args.command.to_string())),
            working_directory: None,
        },
        _ => ApprovalReview::default(),
    }
}

fn expire_entries(entries: &mut HashMap<ApprovalRef, PendingApproval>) {
    let now = unix_time_ms();
    for entry in entries.values_mut() {
        if matches!(
            entry.projection.status,
            ApprovalStatus::Pending | ApprovalStatus::Approved
        ) && now >= entry.projection.expires_at_ms
        {
            entry.projection.status = ApprovalStatus::Expired;
            entry.call.take();
            if let Some(mut audit) = entry.audit.take() {
                audit.authorization(
                    oxideterm_audit::AuditAuthorization::TimedOut,
                    Some("mcp_app_approval"),
                );
                audit.finish(
                    oxideterm_audit::AuditOutcome::Denied,
                    oxideterm_audit::AuditEvidence::Lifecycle,
                    None,
                    None,
                );
            }
        }
    }
}

fn unix_time_ms() -> u128 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .map_or(0, |elapsed| elapsed.as_millis())
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::*;
    use crate::{
        NodeRef, QuickCommandRef,
        calls::{PreparedQuickCommandRunArgs, StartCommandArgs},
    };

    struct AuditKeys;

    impl oxideterm_audit::AuditKeyProvider for AuditKeys {
        fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            Ok(Zeroizing::new(vec![7; 32]))
        }

        fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
            self.load(id)
        }
    }

    #[tokio::test]
    async fn approval_records_are_client_scoped_and_rejection_cannot_dispatch() {
        use oxideterm_audit::{
            AuditAuthorization, AuditCategory, AuditContext, AuditPhase, AuditQuery, AuditService,
            AuditSource,
        };
        let dir = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&dir.path().join("audit.db"), &AuditKeys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let audit_service =
            AuditService::with_key_provider(dir.path().join("audit.db"), AuditKeys).unwrap();
        let context = AuditContext::new(audit_service.client(), AuditSource::Mcp);
        let approvals = ApprovalStore::default();
        let client_a = ClientRef::new();
        let client_b = ClientRef::new();
        let make_call = || {
            PublicToolCall::StartCommand(StartCommandArgs {
                node_ref: NodeRef::new(),
                command: Zeroizing::new("printf private-command".into()),
                working_directory: None,
            })
        };
        let a = approvals
            .stage_with_context(client_a.clone(), make_call(), Some(&context))
            .unwrap();
        let b = approvals
            .stage_with_context(client_b.clone(), make_call(), Some(&context))
            .unwrap();
        assert_ne!(a.audit_operation_id, b.audit_operation_id);
        approvals
            .set_status(&a.approval_ref, ApprovalStatus::Rejected)
            .unwrap();
        approvals
            .set_status(&b.approval_ref, ApprovalStatus::Approved)
            .unwrap();
        assert!(matches!(
            approvals.take_approved(&client_a, &a.approval_ref),
            Err(ApprovalError::Rejected)
        ));
        assert!(matches!(
            approvals.take_approved(&client_a, &b.approval_ref),
            Err(ApprovalError::WrongClient)
        ));
        let (approved, audit) = approvals
            .take_approved_with_audit(&client_b, &b.approval_ref)
            .unwrap();
        assert!(matches!(approved, PublicToolCall::StartCommand(_)));
        audit.unwrap().finish(
            oxideterm_audit::AuditOutcome::Sent,
            oxideterm_audit::AuditEvidence::Dispatch,
            None,
            None,
        );
        let page = audit_service
            .client()
            .query(AuditQuery {
                category: Some(AuditCategory::Automation),
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        let events = page
            .records
            .iter()
            .map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                (
                    op.id.as_str(),
                    op.agent_id.as_ref().map(|id| id.as_str()),
                    op.phase.unwrap(),
                    op.authorization,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 8);
        let a_id = a.audit_operation_id.as_deref().unwrap();
        let b_id = b.audit_operation_id.as_deref().unwrap();
        assert!(events.contains(&(
            a_id,
            Some(client_a.as_str()),
            AuditPhase::Authorization,
            AuditAuthorization::Denied
        )));
        assert!(events.contains(&(
            b_id,
            Some(client_b.as_str()),
            AuditPhase::Authorization,
            AuditAuthorization::Approved
        )));
        assert!(
            !serde_json::to_string(&page.records)
                .unwrap()
                .contains("private-command")
        );
    }

    #[test]
    fn approved_action_is_client_scoped_frozen_and_one_shot() {
        let approvals = ApprovalStore::default();
        let owner = ClientRef::new();
        let other_client = ClientRef::new();
        let node_ref = NodeRef::new();
        let projection = approvals
            .stage(
                owner.clone(),
                PublicToolCall::StartCommand(StartCommandArgs {
                    node_ref: node_ref.clone(),
                    command: Zeroizing::new("journalctl -n 50".to_owned()),
                    working_directory: None,
                }),
            )
            .expect("stage action");
        let public_projection = serde_json::to_string(&projection).expect("serialize projection");
        assert!(!public_projection.contains("journalctl"));

        approvals
            .set_status(&projection.approval_ref, ApprovalStatus::Approved)
            .expect("approve action");
        assert!(matches!(
            approvals.take_approved(&other_client, &projection.approval_ref),
            Err(ApprovalError::WrongClient)
        ));
        let call = approvals
            .take_approved(&owner, &projection.approval_ref)
            .expect("consume approved action");
        let PublicToolCall::StartCommand(args) = call else {
            panic!("approval returned a different tool call");
        };
        assert_eq!(args.node_ref, node_ref);
        assert_eq!(args.command.as_str(), "journalctl -n 50");
        assert!(matches!(
            approvals.take_approved(&owner, &projection.approval_ref),
            Err(ApprovalError::Consumed)
        ));
    }

    #[test]
    fn prepared_quick_command_approval_shows_and_freezes_expanded_command() {
        let approvals = ApprovalStore::default();
        let owner = ClientRef::new();
        let command = "rm -rf /tmp/example";
        let projection = approvals
            .stage(
                owner.clone(),
                PublicToolCall::PreparedQuickCommandRun(PreparedQuickCommandRunArgs {
                    quickcommand_ref: QuickCommandRef::new(),
                    node_ref: NodeRef::new(),
                    command: Zeroizing::new(command.to_string()),
                }),
            )
            .expect("stage prepared Quick Command");

        assert_eq!(projection.review.command(), Some(command));
        assert!(
            !serde_json::to_string(&projection)
                .unwrap()
                .contains(command)
        );
        approvals
            .set_status(&projection.approval_ref, ApprovalStatus::Approved)
            .expect("approve prepared Quick Command");
        let approved = approvals
            .take_approved(&owner, &projection.approval_ref)
            .expect("take prepared Quick Command");
        let PublicToolCall::PreparedQuickCommandRun(args) = approved else {
            panic!("approval returned a different tool call");
        };
        assert_eq!(args.command.as_str(), command);
    }
}
