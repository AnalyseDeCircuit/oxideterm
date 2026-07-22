use std::fmt;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

/// Describes one application-owned tool exposed to an ACP agent through MCP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpHostToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl AcpHostToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

/// Carries an MCP tool call to the application executor without importing UI types.
pub struct AcpHostToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    response_tx: oneshot::Sender<AcpHostToolResponse>,
}

impl AcpHostToolCall {
    pub(crate) fn new(
        id: String,
        name: String,
        arguments: Value,
        response_tx: oneshot::Sender<AcpHostToolResponse>,
    ) -> Self {
        Self {
            id,
            name,
            arguments,
            response_tx,
        }
    }

    /// Completes the pending MCP request. A closed receiver means the ACP turn ended.
    pub fn respond(self, response: AcpHostToolResponse) -> Result<(), AcpHostToolResponse> {
        self.response_tx.send(response)
    }
}

impl fmt::Debug for AcpHostToolCall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Tool arguments can contain commands or interactive input and must not reach logs.
        formatter
            .debug_struct("AcpHostToolCall")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("arguments", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// The application sends only already-redacted model-facing text across this boundary.
pub struct AcpHostToolResponse {
    pub content: String,
    pub is_error: bool,
}

impl AcpHostToolResponse {
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

impl fmt::Debug for AcpHostToolResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Model-facing tool output can contain redacted terminal material; keep it out of logs.
        formatter
            .debug_struct("AcpHostToolResponse")
            .field("content", &"[redacted]")
            .field("is_error", &self.is_error)
            .finish()
    }
}

/// Receives application-owned tool calls for the lifetime of one ACP runtime.
pub struct AcpHostToolCallReceiver {
    pub(crate) inner: mpsc::UnboundedReceiver<AcpHostToolCall>,
}

impl AcpHostToolCallReceiver {
    pub async fn recv(&mut self) -> Option<AcpHostToolCall> {
        self.inner.recv().await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AcpHostToolsError {
    #[error("failed to bind the ACP host-tools server")]
    Bind(#[source] std::io::Error),
    #[error("failed to resolve the ACP host-tools server address")]
    LocalAddress(#[source] std::io::Error),
}
