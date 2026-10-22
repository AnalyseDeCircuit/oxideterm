// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{net::Ipv4Addr, sync::Arc};

use oxideterm_public_mcp::{
    ApprovalStore, AuditStore, ClientApprovalMode, ClientRegistry, DomainBroker, DomainMessage,
    PublicMcpState, ToolEnvelope, ToolGroup, start_http_server,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::Zeroizing;

#[tokio::test]
async fn loopback_endpoint_rejects_missing_credentials_and_accepts_registered_client() {
    let clients = Arc::new(ClientRegistry::default());
    let registered = clients
        .register(
            "HTTP boundary",
            ClientApprovalMode::Standard,
            [ToolGroup::Basic],
        )
        .expect("register client");
    let (broker, _receiver) = DomainBroker::channel(1);
    let state = Arc::new(PublicMcpState {
        clients,
        approvals: Arc::new(ApprovalStore::default()),
        audit: Arc::new(AuditStore::new(8)),
        artifacts: Arc::default(),
        broker,
    });
    let server = start_http_server(&tokio::runtime::Handle::current(), state, 0)
        .expect("start loopback server");

    let unauthorized = send_request(server.port(), None, initialize_body(), None).await;
    assert!(unauthorized.starts_with("HTTP/1.1 401"));

    let authorization = Zeroizing::new(format!("Bearer {}", registered.credential.expose()));
    let authorized =
        send_request(server.port(), Some(&authorization), initialize_body(), None).await;
    assert!(authorized.starts_with("HTTP/1.1 200"), "{authorized}");
    assert!(authorized.contains("oxideterm-public-mcp"));

    let tools = send_request(
        server.port(),
        Some(&authorization),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        Some("2025-06-18"),
    )
    .await;
    assert!(tools.starts_with("HTTP/1.1 200"), "{tools}");
    assert!(tools.contains("mcp_overview"));
    assert!(!tools.contains("connections_browse"));
}

#[tokio::test]
async fn tool_dispatch_survives_notifications_business_errors_and_malformed_calls() {
    let clients = Arc::new(ClientRegistry::default());
    let registered = clients
        .register(
            "Dispatch boundary",
            ClientApprovalMode::Unattended,
            [ToolGroup::Basic, ToolGroup::NodeSession],
        )
        .expect("register client");
    let authorization = Zeroizing::new(format!("Bearer {}", registered.credential.expose()));
    let (broker, mut receiver) = DomainBroker::channel(1);
    let state = Arc::new(PublicMcpState {
        clients,
        approvals: Arc::new(ApprovalStore::default()),
        audit: Arc::new(AuditStore::new(8)),
        artifacts: Arc::default(),
        broker,
    });
    let server = start_http_server(&tokio::runtime::Handle::current(), state, 0)
        .expect("start loopback server");

    for protocol in [None, Some("2025-06-18")] {
        let response = send_request(
            server.port(),
            Some(&authorization),
            initialize_body(),
            protocol,
        )
        .await;
        assert_eq!(
            response_json(&response)["result"]["protocolVersion"],
            "2025-06-18"
        );
        assert_overview(server.port(), &authorization, protocol).await;
        for initialized in [false, true] {
            if initialized {
                let response = send_request(
                    server.port(),
                    Some(&authorization),
                    r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                    protocol,
                )
                .await;
                assert!(response.starts_with("HTTP/1.1 202"), "{response}");
                assert_overview(server.port(), &authorization, protocol).await;
            }

            // The real HTTP/broker boundary receives a domain failure without needing a private SSH host.
            let failure = send_request(
                server.port(),
                Some(&authorization),
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nodes_connect","arguments":{"connection_ref":"connection_00000000000000000000000000000000"}}}"#,
                protocol,
            );
            let finish = async {
                let Some(DomainMessage::Request(request)) = receiver.recv().await else {
                    panic!("expected a domain request");
                };
                assert!(matches!(
                    request.call,
                    oxideterm_public_mcp::PublicToolCall::ConnectNode(_)
                ));
                request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
            };
            let (response, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                tokio::join!(failure, finish)
            })
            .await
            .expect("tool call completed");
            let result = &response_json(&response)["result"];
            assert_eq!(result["isError"], true);
            assert_eq!(
                result["structuredContent"],
                json!({"outcome":"failed","error":"The connection handle is unavailable"})
            );
            assert_overview(server.port(), &authorization, protocol).await;
        }

        for (params, field, expected, actual) in [
            (
                json!({"name":"mcp_overview","arguments":[]}),
                "params.arguments",
                "object",
                "array",
            ),
            (
                json!({"name":"mcp_overview","arguments":"secret-diagnostic-fixture"}),
                "params.arguments",
                "object",
                "string",
            ),
            (
                json!({"arguments":{"password":"secret-diagnostic-fixture"}}),
                "params.name",
                "string",
                "missing",
            ),
            (
                json!({"name":42,"arguments":{}}),
                "params.name",
                "string",
                "number",
            ),
            (
                json!({"name":"mcp_overview","requestState":false}),
                "params.requestState",
                "string",
                "boolean",
            ),
            (
                json!({"name":"mcp_overview","inputResponses":[]}),
                "params.inputResponses",
                "object",
                "array",
            ),
        ] {
            let body = Zeroizing::new(
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":params}).to_string(),
            );
            let response = send_request(server.port(), Some(&authorization), &body, protocol).await;
            let error = &response_json(&response)["error"];
            assert_eq!(
                error,
                &json!({
                    "code": -32602,
                    "message": "Invalid tools/call parameters",
                    "data": {
                        "stage": "tools_call_dispatch", "reason": "parameter_type_mismatch",
                        "field": field, "expected": expected, "actual": actual,
                    }
                })
            );
            assert_overview(server.port(), &authorization, protocol).await;
        }
        let response = send_request(
            server.port(),
            Some(&authorization),
            r#"{"jsonrpc":"2.0","id":5,"method":"unknown/method","params":{}}"#,
            protocol,
        )
        .await;
        assert_eq!(
            response_json(&response)["error"],
            json!({"code":-32601,"message":"unknown/method"})
        );
    }
    server.shutdown().await;
}

fn response_json(response: &str) -> Value {
    let (_, body) = response.split_once("\r\n\r\n").expect("HTTP response body");
    serde_json::from_str(body).expect("JSON response")
}

async fn assert_overview(port: u16, authorization: &str, protocol: Option<&str>) {
    let response = send_request(port, Some(authorization),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"mcp_overview","arguments":{}}}"#, protocol).await;
    let result = &response_json(&response)["result"]["structuredContent"];
    assert_eq!(result["server"], "OxideTerm Public MCP", "{response}");
    assert_eq!(result["approval_policy"], "unattended_for_enabled_groups");
}

fn initialize_body() -> &'static str {
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"boundary-test","version":"1"}}}"#
}

async fn send_request(
    port: u16,
    authorization: Option<&str>,
    body: &str,
    protocol_version: Option<&str>,
) -> String {
    let authorization_header = Zeroizing::new(
        authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default(),
    );
    let protocol_header = protocol_version
        .map(|value| format!("MCP-Protocol-Version: {value}\r\n"))
        .unwrap_or_default();
    let request = Zeroizing::new(format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{}{protocol_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        authorization_header.as_str(),
        body.len()
    ));
    let mut stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .await
        .expect("connect to loopback server");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write HTTP request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("read HTTP response");
    String::from_utf8_lossy(&response).into_owned()
}
