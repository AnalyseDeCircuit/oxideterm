use std::{collections::HashMap, sync::Arc};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, oneshot};

use crate::{AcpHostToolCall, AcpHostToolDefinition, AcpHostToolResponse};

pub(crate) const MCP_PROTOCOL_VERSION: &str = "2025-03-26";
pub(crate) const MCP_REQUEST_BODY_LIMIT: usize = 1024 * 1024;

pub(crate) struct AcpHostToolsProtocol {
    definitions: Arc<Vec<AcpHostToolDefinition>>,
    definitions_by_name: Arc<HashMap<String, AcpHostToolDefinition>>,
    call_tx: mpsc::UnboundedSender<AcpHostToolCall>,
    authorization_digest: [u8; 32],
}

impl AcpHostToolsProtocol {
    pub(crate) fn new(
        definitions: Vec<AcpHostToolDefinition>,
        call_tx: mpsc::UnboundedSender<AcpHostToolCall>,
        authorization_header: &str,
    ) -> Self {
        let definitions_by_name = definitions
            .iter()
            .cloned()
            .map(|definition| (definition.name.clone(), definition))
            .collect();
        Self {
            definitions: Arc::new(definitions),
            definitions_by_name: Arc::new(definitions_by_name),
            call_tx,
            authorization_digest: authorization_digest(authorization_header.as_bytes()),
        }
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
            // MCP notifications are acknowledged without generating a JSON-RPC response.
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
                        "name": "OxideTerm Host Tools",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                }),
            )),
            "ping" => ProtocolResponse::json(json_rpc_result(id, json!({}))),
            "tools/list" => {
                let tools = self
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
        if !self.definitions_by_name.contains_key(name) {
            return ProtocolResponse::json(json_rpc_error(
                id,
                -32602,
                "Tool is not exposed by OxideTerm.",
            ));
        }
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
            name.to_string(),
            arguments,
            response_tx,
        );
        if self.call_tx.send(call).is_err() {
            return ProtocolResponse::json(json_rpc_error(
                id,
                -32603,
                "OxideTerm tool executor is unavailable.",
            ));
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
