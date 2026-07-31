use serde_json::json;

use crate::{AcpHostToolDefinition, AcpHostToolResponse, start_acp_host_tools_server};

fn test_definition() -> AcpHostToolDefinition {
    AcpHostToolDefinition::new(
        "inspect_host_tools",
        "Inspect one explicit Host Tools surface.",
        json!({
            "type": "object",
            "properties": { "surface": { "type": "string" } },
            "required": ["surface"],
        }),
    )
}

#[tokio::test]
async fn server_requires_authorization_and_lists_tools() {
    let (server, _calls) =
        start_acp_host_tools_server(&tokio::runtime::Handle::current(), vec![test_definition()])
            .expect("host tools server");
    let McpServerView { url, authorization } = mcp_server_view(server.mcp_server());
    let client = reqwest::Client::new();
    let unauthorized = client
        .post(&url)
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .send()
        .await
        .expect("unauthorized response");
    assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);

    let response = client
        .post(&url)
        .header("Authorization", authorization)
        .json(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
        .send()
        .await
        .expect("tools/list response");
    assert!(response.status().is_success());
    let body: serde_json::Value = response.json().await.expect("tools/list body");
    assert_eq!(body["result"]["tools"][0]["name"], "inspect_host_tools");
    server.shutdown().await;
}

#[tokio::test]
async fn tool_calls_are_forwarded_and_completed() {
    let (server, mut calls) =
        start_acp_host_tools_server(&tokio::runtime::Handle::current(), vec![test_definition()])
            .expect("host tools server");
    let McpServerView { url, authorization } = mcp_server_view(server.mcp_server());
    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .post(url)
            .header("Authorization", authorization)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "inspect_host_tools",
                    "arguments": { "surface": "processes" },
                },
            }))
            .send()
            .await
            .expect("tools/call response")
            .json::<serde_json::Value>()
            .await
            .expect("tools/call body")
    });
    let call = calls.recv().await.expect("forwarded tool call");
    assert_eq!(call.name, "inspect_host_tools");
    assert_eq!(call.arguments["surface"], "processes");
    call.respond(AcpHostToolResponse::success("sanitized output"))
        .expect("tool response receiver");
    let response = request.await.expect("request task");
    assert_eq!(response["result"]["content"][0]["text"], "sanitized output");
    assert_eq!(response["result"]["isError"], false);
    server.shutdown().await;
}

#[test]
fn debug_output_redacts_tool_arguments_and_content() {
    let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
    let call = crate::AcpHostToolCall::new(
        "call-1".to_string(),
        "run_command".to_string(),
        json!({ "command": "TOKEN=supersecret" }),
        response_tx,
    );
    assert!(!format!("{call:?}").contains("supersecret"));
    let response = AcpHostToolResponse::success("PASSWORD=supersecret");
    assert!(!format!("{response:?}").contains("supersecret"));
}

struct McpServerView {
    url: String,
    authorization: String,
}

fn mcp_server_view(server: agent_client_protocol::schema::v1::McpServer) -> McpServerView {
    let agent_client_protocol::schema::v1::McpServer::Http(server) = server else {
        panic!("HTTP MCP server")
    };
    McpServerView {
        url: server.url,
        authorization: server
            .headers
            .into_iter()
            .find(|header| header.name == "Authorization")
            .expect("authorization header")
            .value,
    }
}
