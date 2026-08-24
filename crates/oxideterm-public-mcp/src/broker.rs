use std::{collections::HashMap, fmt, sync::Arc};

use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    auth::{ClientApprovalMode, ClientRegistry, ToolGroup},
    calls::{PublicToolCall, ToolEnvelope},
    handles::ClientRef,
};

#[derive(Clone)]
pub struct DomainBroker {
    sender: mpsc::Sender<DomainMessage>,
    active_requests: Arc<Mutex<HashMap<Uuid, ActiveDomainRequest>>>,
}

struct ActiveDomainRequest {
    client_ref: ClientRef,
    required_groups: Vec<ToolGroup>,
    cancellation: CancellationToken,
}

struct ActiveDomainRequestGuard {
    request_id: Uuid,
    active_requests: Arc<Mutex<HashMap<Uuid, ActiveDomainRequest>>>,
}

impl Drop for ActiveDomainRequestGuard {
    fn drop(&mut self) {
        self.active_requests.lock().remove(&self.request_id);
    }
}

#[derive(Debug)]
pub enum DomainMessage {
    Request(Box<DomainRequest>),
    StateChanged,
}

pub struct DomainRequest {
    pub client_ref: ClientRef,
    pub call: PublicToolCall,
    approval_mode: ClientApprovalMode,
    response: oneshot::Sender<ToolEnvelope>,
    cancellation: CancellationToken,
}

impl fmt::Debug for DomainRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DomainRequest")
            .field("client_ref", &self.client_ref)
            .field("call", &self.call)
            .finish_non_exhaustive()
    }
}

impl DomainRequest {
    /// Finishes a broker call without exposing the response channel to domain code.
    pub fn finish(self, response: ToolEnvelope) {
        let _ = self.response.send(response);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn requires_standard_approval(&self) -> bool {
        self.approval_mode == ClientApprovalMode::Standard
    }

    /// Retargets a protocol-level alias while preserving the original response and cancellation.
    pub fn with_call(mut self, call: PublicToolCall) -> Self {
        self.call = call;
        self
    }
}

pub struct DomainRequestReceiver {
    receiver: mpsc::Receiver<DomainMessage>,
}

impl DomainRequestReceiver {
    pub async fn recv(&mut self) -> Option<DomainMessage> {
        self.receiver.recv().await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BrokerError {
    #[error("the OxideTerm workspace is unavailable")]
    WorkspaceUnavailable,
    #[error("the OxideTerm workspace stopped before completing the request")]
    ResponseDropped,
    #[error("the OxideTerm workspace did not complete the request in time")]
    TimedOut,
    #[error("the MCP client authorization changed before the request was delivered")]
    AuthorizationChanged,
}

const DOMAIN_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
const CLOUD_SYNC_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl DomainBroker {
    /// Creates the only typed bridge between protocol tasks and the GPUI domain runtime.
    pub fn channel(capacity: usize) -> (Arc<Self>, DomainRequestReceiver) {
        let (sender, receiver) = mpsc::channel(capacity);
        (
            Arc::new(Self {
                sender,
                active_requests: Arc::default(),
            }),
            DomainRequestReceiver { receiver },
        )
    }

    pub async fn execute(
        &self,
        clients: &ClientRegistry,
        expected_approval_mode: ClientApprovalMode,
        client_ref: ClientRef,
        call: PublicToolCall,
    ) -> Result<ToolEnvelope, BrokerError> {
        let required_groups = std::iter::once(call.required_group())
            .chain(call.additional_required_groups().iter().copied())
            .collect::<Vec<_>>();
        // Network-backed sync plans may legitimately exceed the interactive broker timeout.
        let timeout = if matches!(
            &call,
            PublicToolCall::SyncPullPreview(_)
                | PublicToolCall::SyncPublishPreview(_)
                | PublicToolCall::SyncApplyPlan(_)
                | PublicToolCall::SyncRestore(_)
        ) {
            CLOUD_SYNC_REQUEST_TIMEOUT
        } else {
            DOMAIN_REQUEST_TIMEOUT
        };
        let (response, receiver) = oneshot::channel();
        let cancellation = CancellationToken::new();
        let cancellation_guard = cancellation.clone().drop_guard();
        let active_request_guard = self.track_active_request(
            client_ref.clone(),
            required_groups.clone(),
            cancellation.clone(),
        );
        let authorized = clients.get(&client_ref).is_some_and(|client| {
            client.enabled
                && client.approval_mode == expected_approval_mode
                && required_groups
                    .iter()
                    .all(|group| client.tool_groups.contains(group))
        });
        if !authorized {
            return Err(BrokerError::AuthorizationChanged);
        }
        self.sender
            .send(DomainMessage::Request(Box::new(DomainRequest {
                client_ref,
                call,
                approval_mode: expected_approval_mode,
                response,
                cancellation,
            })))
            .await
            .map_err(|_| BrokerError::WorkspaceUnavailable)?;
        let response = tokio::time::timeout(timeout, receiver)
            .await
            .map_err(|_| BrokerError::TimedOut)?
            .map_err(|_| BrokerError::ResponseDropped)?;
        cancellation_guard.disarm();
        drop(active_request_guard);
        Ok(response)
    }

    pub fn cancel_client(&self, client_ref: &ClientRef) -> usize {
        self.cancel_matching_requests(|request| &request.client_ref == client_ref)
    }

    pub fn cancel_client_tool_group(&self, client_ref: &ClientRef, tool_group: ToolGroup) -> usize {
        self.cancel_matching_requests(|request| {
            &request.client_ref == client_ref && request.required_groups.contains(&tool_group)
        })
    }

    pub fn notify_state_changed(&self) {
        let _ = self.sender.try_send(DomainMessage::StateChanged);
    }

    fn track_active_request(
        &self,
        client_ref: ClientRef,
        required_groups: Vec<ToolGroup>,
        cancellation: CancellationToken,
    ) -> ActiveDomainRequestGuard {
        let request_id = Uuid::new_v4();
        self.active_requests.lock().insert(
            request_id,
            ActiveDomainRequest {
                client_ref,
                required_groups,
                cancellation,
            },
        );
        ActiveDomainRequestGuard {
            request_id,
            active_requests: self.active_requests.clone(),
        }
    }

    fn cancel_matching_requests(&self, matches: impl Fn(&ActiveDomainRequest) -> bool) -> usize {
        let active_requests = self.active_requests.lock();
        let mut cancelled = 0;
        for request in active_requests.values().filter(|request| matches(request)) {
            // Domain owners observe the same token used for disconnect and timeout cancellation.
            request.cancellation.cancel();
            cancelled += 1;
        }
        cancelled
    }
}
