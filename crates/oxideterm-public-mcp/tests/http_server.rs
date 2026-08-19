// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{net::Ipv4Addr, sync::Arc};

use oxideterm_public_mcp::{
    ApprovalStore, AuditStore, ClientApprovalMode, ClientRegistry, DomainBroker, PublicMcpState,
    ToolGroup, start_http_server,
};
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

fn initialize_body() -> &'static str {
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"boundary-test","version":"1"}}}"#
}

async fn send_request(
    port: u16,
    authorization: Option<&str>,
    body: &str,
    protocol_version: Option<&str>,
) -> String {
    let authorization_header = authorization
        .map(|value| format!("Authorization: {value}\r\n"))
        .unwrap_or_default();
    let protocol_header = protocol_version
        .map(|value| format!("MCP-Protocol-Version: {value}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{authorization_header}{protocol_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
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
