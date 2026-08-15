use std::{fmt, sync::Arc};

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{
    calls::{PublicToolCall, ToolEnvelope},
    handles::ClientRef,
};

#[derive(Clone)]
pub struct DomainBroker {
    sender: mpsc::Sender<DomainMessage>,
}

#[derive(Debug)]
pub enum DomainMessage {
    Request(Box<DomainRequest>),
    StateChanged,
}

pub struct DomainRequest {
    pub client_ref: ClientRef,
    pub call: PublicToolCall,
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
}

const DOMAIN_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
const CLOUD_SYNC_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl DomainBroker {
    /// Creates the only typed bridge between protocol tasks and the GPUI domain runtime.
    pub fn channel(capacity: usize) -> (Arc<Self>, DomainRequestReceiver) {
        let (sender, receiver) = mpsc::channel(capacity);
        (
            Arc::new(Self { sender }),
            DomainRequestReceiver { receiver },
        )
    }

    pub async fn execute(
        &self,
        client_ref: ClientRef,
        call: PublicToolCall,
    ) -> Result<ToolEnvelope, BrokerError> {
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
        self.sender
            .send(DomainMessage::Request(Box::new(DomainRequest {
                client_ref,
                call,
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
        Ok(response)
    }

    pub fn notify_state_changed(&self) {
        let _ = self.sender.try_send(DomainMessage::StateChanged);
    }
}
