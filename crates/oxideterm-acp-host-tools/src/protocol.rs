use std::{collections::HashMap, sync::RwLock};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, oneshot};

use crate::{AcpHostToolCall, AcpHostToolDefinition, AcpHostToolResponse};

pub(crate) const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
pub(crate) const MCP_REQUEST_BODY_LIMIT: usize = 1024 * 1024;

pub(crate) struct AcpHostToolsProtocol {
    catalog: RwLock<AcpHostToolCatalog>,
    call_tx: mpsc::Sender<AcpHostToolCall>,
    authorization_digest: [u8; 32],
}

struct AcpHostToolCatalog {
    definitions: Vec<AcpHostToolDefinition>,
    execution_names: HashMap<String, String>,
}

impl AcpHostToolCatalog {
    fn new(definitions: Vec<AcpHostToolDefinition>) -> Self {
        let execution_names = definitions
            .iter()
            .map(|definition| (definition.name.clone(), definition.execution_name.clone()))
            .collect();
        Self {
            definitions,
            execution_names,
        }
    }
}

impl AcpHostToolsProtocol {
    pub(crate) fn new(
        definitions: Vec<AcpHostToolDefinition>,
        call_tx: mpsc::Sender<AcpHostToolCall>,
        authorization_header: &str,
    ) -> Self {
        Self {
            catalog: RwLock::new(AcpHostToolCatalog::new(definitions)),
            call_tx,
            authorization_digest: authorization_digest(authorization_header.as_bytes()),
        }
    }

    pub(crate) fn replace_definitions(&self, definitions: Vec<AcpHostToolDefinition>) {
        // The provider reconnects to this conversation-scoped server for each
        // turn, so the next tools/list observes current policy and MCP state.
        *self.catalog.write().expect("ACP tool catalog write lock") =
            AcpHostToolCatalog::new(definitions);
    }

    pub(crate) fn authorized(&self, authorization_header: Option<&[u8]>) -> bool {
        let Some(authorization_header) = authorization_header else {
            return false;
        };
        authorization_digest(authorization_header)
            .ct_eq(&self.authorization_digest)
            .into()
    }

    pub(crate) async fn handle_message(&self, request: Value) -> ProtocolResponse {
        let Some(object) = request.as_object() else {
            return ProtocolResponse::json(json_rpc_error(
                Value::Null,
                -32600,
                "Invalid JSON-RPC request.",
            ));
        };
        let id = object.get("id").cloned();
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return ProtocolResponse::json(json_rpc_error(
                id.unwrap_or(Value::Null),
                -32600,
                "JSON-RPC method is required.",
            ));
        };
        if id.is_none() {
            // Stateless MCP notifications do not require a response body.
            return ProtocolResponse::accepted();
        }
        let id = id.unwrap_or(Value::Null);
        match method {
            "initialize" => ProtocolResponse::json(json_rpc_result(
                id,
                json!({
                    "protocolVersion": request
                        .pointer("/params/protocolVersion")
                        .and_then(Value::as_str)
                        .unwrap_or(MCP_PROTOCOL_VERSION),
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": "OxideTerm Application Tools",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                }),
            )),
            "ping" => ProtocolResponse::json(json_rpc_result(id, json!({}))),
            "tools/list" => {
                let catalog = self.catalog.read().expect("ACP tool catalog read lock");
                let tools = catalog
                    .definitions
                    .iter()
                    .map(|definition| {
                        json!({
                            "name": definition.name,
                            "description": definition.description,
                            "inputSchema": definition.input_schema,
                        })
                    })
                    .collect::<Vec<_>>();
                ProtocolResponse::json(json_rpc_result(id, json!({ "tools": tools })))
            }
            "tools/call" => self.handle_tool_call(id, &request).await,
            _ => ProtocolResponse::json(json_rpc_error(id, -32601, "MCP method not found.")),
        }
    }

    async fn handle_tool_call(&self, id: Value, request: &Value) -> ProtocolResponse {
        let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
            return ProtocolResponse::json(json_rpc_error(id, -32602, "Tool name is required."));
        };
        let execution_name = self
            .catalog
            .read()
            .expect("ACP tool catalog read lock")
            .execution_names
            .get(name)
            .cloned();
        let Some(execution_name) = execution_name else {
            return ProtocolResponse::json(json_rpc_error(
                id,
                -32602,
                "Tool is not exposed by OxideTerm.",
            ));
        };
        let arguments = request
            .pointer("/params/arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return ProtocolResponse::json(json_rpc_error(
                id,
                -32602,
                "Tool arguments must be an object.",
            ));
        }
        let (response_tx, response_rx) = oneshot::channel();
        let call = AcpHostToolCall::new(
            uuid::Uuid::new_v4().to_string(),
            execution_name,
            arguments,
            response_tx,
        );
        if let Err(error) = self.call_tx.try_send(call) {
            let message = match error {
                mpsc::error::TrySendError::Full(_) => "OxideTerm tool executor is busy.",
                mpsc::error::TrySendError::Closed(_) => "OxideTerm tool executor is unavailable.",
            };
            return ProtocolResponse::json(json_rpc_error(id, -32603, message));
        }
        let response = response_rx.await.unwrap_or_else(|_| {
            AcpHostToolResponse::error("OxideTerm tool execution was cancelled.")
        });
        ProtocolResponse::json(json_rpc_result(
            id,
            json!({
                "content": [{ "type": "text", "text": response.content }],
                "isError": response.is_error,
            }),
        ))
    }
}

fn authorization_digest(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn json_rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn json_rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

pub(crate) struct ProtocolResponse {
    pub status: http::StatusCode,
    pub body: Option<Value>,
}

impl ProtocolResponse {
    fn json(body: Value) -> Self {
        Self {
            status: http::StatusCode::OK,
            body: Some(body),
        }
    }

    fn accepted() -> Self {
        Self {
            status: http::StatusCode::ACCEPTED,
            body: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AcpHostToolDefinition;
    use std::sync::Arc;

    #[tokio::test]
    async fn full_executor_queue_rejects_additional_tool_calls() {
        let (call_tx, _call_rx) = mpsc::channel(1);
        let protocol = Arc::new(AcpHostToolsProtocol::new(
            vec![AcpHostToolDefinition::new(
                "inspect_host_tools",
                "Inspect Host Tools.",
                json!({ "type": "object" }),
            )],
            call_tx,
            "Bearer test",
        ));
        let first_protocol = protocol.clone();
        let first_call = tokio::spawn(async move {
            first_protocol
                .handle_message(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": "inspect_host_tools",
                        "arguments": {},
                    },
                }))
                .await
        });
        tokio::task::yield_now().await;

        let response = protocol
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "inspect_host_tools",
                    "arguments": {},
                },
            }))
            .await;

        assert_eq!(
            response
                .body
                .as_ref()
                .and_then(|body| body.pointer("/error/message").and_then(Value::as_str)),
            Some("OxideTerm tool executor is busy.")
        );
        first_call.abort();
    }
}
