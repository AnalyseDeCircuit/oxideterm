#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt, duplex},
        net::TcpListener,
    };

    #[tokio::test]
    async fn stdout_reader_dispatches_content_length_framed_response() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(7, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "test".to_string(),
        ));
        let body = r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        server.write_all(message.as_bytes()).await.unwrap();
        let result = rx.await.unwrap().unwrap();
        assert_eq!(result["ok"].as_bool(), Some(true));
        drop(server);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_dispatches_line_delimited_response() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(3, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "line-json".to_string(),
        ));

        server
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"ok\":true}}\n")
            .await
            .unwrap();
        let result = rx.await.unwrap().unwrap();
        assert_eq!(result["ok"].as_bool(), Some(true));
        drop(server);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_rejects_pending_when_stdout_closes() {
        let (client, server) = duplex(256);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(1, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "close".to_string(),
        ));

        drop(server);

        let error = rx.await.unwrap().unwrap_err();
        assert_eq!(error.to_string(), "MCP server closed stdout");
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_treats_invalid_content_length_as_fatal() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(9, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "invalid-length".to_string(),
        ));

        server
            .write_all(b"Content-Length: 999999999\r\n\r\n{}")
            .await
            .unwrap();
        drop(server);

        let error = rx.await.unwrap().unwrap_err();
        assert_eq!(error.to_string(), "MCP server closed stdout");
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_rejects_response_without_result_or_error() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(11, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "missing-result".to_string(),
        ));

        let body = r#"{"jsonrpc":"2.0","id":11}"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        server.write_all(message.as_bytes()).await.unwrap();

        let error = rx.await.unwrap().unwrap_err();
        assert_eq!(error.to_string(), "MCP response missing result");
        drop(server);
        let _ = task.await;
    }

    #[tokio::test]
    async fn stdout_reader_accepts_content_length_after_other_headers() {
        let (client, mut server) = duplex(1024);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert(12, tx);
        let task = tokio::spawn(stdout_reader_loop(
            BufReader::new(client),
            pending,
            "header-order".to_string(),
        ));

        let body = r#"{"jsonrpc":"2.0","id":12,"result":{"ok":true}}"#;
        let message = format!(
            "Content-Type: application/json\r\nContent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        server.write_all(message.as_bytes()).await.unwrap();

        let result = rx.await.unwrap().unwrap();
        assert_eq!(result["ok"].as_bool(), Some(true));
        drop(server);
        let _ = task.await;
    }

    #[test]
    fn validate_mcp_command_rejects_paths_and_unknown_binaries() {
        assert!(validate_mcp_command("npx").is_ok());
        assert!(validate_mcp_command("uvx").is_ok());
        assert!(validate_mcp_command("docker").is_ok());
        assert!(validate_mcp_command("../npx").is_err());
        assert!(validate_mcp_command("node").is_err());
        assert!(validate_mcp_command("python3").is_err());
        assert!(validate_mcp_command("uv").is_err());
        assert!(validate_mcp_command("bash").is_err());
        assert!(validate_mcp_command("/usr/bin/python3").is_err());
    }

    #[test]
    fn validate_mcp_env_blocks_injection_variables() {
        let mut env = HashMap::new();
        env.insert("LD_PRELOAD".to_string(), "evil.so".to_string());
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert(
            "Node_Options".to_string(),
            "--require ./evil.js".to_string(),
        );
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert("PYTHONPATH".to_string(), "/tmp/evil".to_string());
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert("PYTHONSTARTUP".to_string(), "/tmp/startup.py".to_string());
        assert!(validate_mcp_env(&env).is_err());
        env.clear();
        env.insert("SAFE".to_string(), "1".to_string());
        assert!(validate_mcp_env(&env).is_ok());
    }

    #[test]
    fn duplicate_server_names_are_disambiguated() {
        let config_a = McpServerConfig {
            id: "a".to_string(),
            name: "shared".to_string(),
            transport: McpTransport::Stdio,
            url: None,
            command: Some("npx".to_string()),
            args: Vec::new(),
            env: HashMap::new(),
            auth_header_name: None,
            auth_header_mode: None,
            headers: HashMap::new(),
            enabled: true,
            retry_on_disconnect: false,
            auth_token: None,
        };
        let mut servers = HashMap::new();
        servers.insert(
            "a".to_string(),
            McpServerState {
                config: config_a.clone(),
                status: McpServerStatus::Connected,
                error: None,
                capabilities: None,
                tools: Vec::new(),
                resources: Vec::new(),
                runtime_id: None,
                endpoint_url: None,
                resolved_transport: None,
                session_id: None,
                generation: 1,
            },
        );
        let mut config_b = config_a;
        config_b.id = "b".to_string();
        servers.insert(
            "b".to_string(),
            McpServerState {
                config: config_b,
                status: McpServerStatus::Connected,
                error: None,
                capabilities: None,
                tools: Vec::new(),
                resources: Vec::new(),
                runtime_id: None,
                endpoint_url: None,
                resolved_transport: None,
                session_id: None,
                generation: 1,
            },
        );
        assert_eq!(
            server_namespace(servers.get("a").unwrap(), &servers),
            "shared#a"
        );
    }

    #[test]
    fn mcp_resource_tools_are_exposed_without_connected_resources() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let names = registry
            .tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"list_mcp_resources".to_string()));
        assert!(names.contains(&"read_mcp_resource".to_string()));
    }

    #[test]
    fn mcp_tools_and_resources_follow_config_order() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let mut state = registry.state.write();
        state.server_order = vec!["b".to_string(), "a".to_string()];
        let mut server_a = connected_http_state(
            http_test_config("a", McpTransport::StreamableHttp, "http://127.0.0.1/a"),
            1,
            "tool-a",
        );
        server_a.resources = vec![McpResource {
            uri: "test://a".to_string(),
            name: "A".to_string(),
            description: None,
            mime_type: None,
        }];
        let mut server_b = connected_http_state(
            http_test_config("b", McpTransport::StreamableHttp, "http://127.0.0.1/b"),
            1,
            "tool-b",
        );
        server_b.resources = vec![McpResource {
            uri: "test://b".to_string(),
            name: "B".to_string(),
            description: None,
            mime_type: None,
        }];
        state.servers.insert("a".to_string(), server_a);
        state.servers.insert("b".to_string(), server_b);
        drop(state);

        let tool_names = registry
            .tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        let dynamic_names = tool_names
            .into_iter()
            .filter(|name| name.starts_with("mcp::"))
            .collect::<Vec<_>>();
        assert_eq!(dynamic_names, vec!["mcp::b::tool-b", "mcp::a::tool-a"]);

        let resource_uris = registry
            .resources()
            .into_iter()
            .map(|(resource, _, _)| resource.uri)
            .collect::<Vec<_>>();
        assert_eq!(resource_uris, vec!["test://b", "test://a"]);
    }

    #[tokio::test]
    async fn streamable_http_server_connects_and_exposes_tools() {
        let (url, task) = spawn_streamable_http_mcp_server(false).await;
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        registry
            .connect_config(http_test_config("http", McpTransport::StreamableHttp, &url))
            .await;
        let snapshots = registry.snapshots();
        let snapshot = snapshots
            .iter()
            .find(|server| server.config.id == "http")
            .unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(
            snapshot.resolved_transport.as_deref(),
            Some("streamable-http")
        );
        assert_eq!(snapshot.session_id.as_deref(), Some("resources-session"));
        assert_eq!(snapshot.tools[0].name, "ping");
        assert!(
            registry
                .tool_definitions()
                .iter()
                .any(|tool| tool.name == "mcp::http::ping")
        );
        stop_streamable_http_mcp_server(task).await;
    }

    #[tokio::test]
    async fn connect_all_values_waits_for_enabled_connections() {
        let (url, task) = spawn_streamable_http_mcp_server(false).await;
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let config =
            serde_json::to_value(http_test_config("http", McpTransport::StreamableHttp, &url))
                .unwrap();

        registry.connect_all_values(&[config]).await;

        let snapshot = registry
            .snapshots()
            .into_iter()
            .find(|server| server.config.id == "http")
            .unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(snapshot.tools[0].name, "ping");
        stop_streamable_http_mcp_server(task).await;
    }

    #[tokio::test]
    async fn streamable_http_falls_back_to_legacy_sse() {
        let (url, task) = spawn_streamable_http_mcp_server(true).await;
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        registry
            .connect_config(http_test_config("http", McpTransport::StreamableHttp, &url))
            .await;
        let snapshots = registry.snapshots();
        let snapshot = snapshots
            .iter()
            .find(|server| server.config.id == "http")
            .unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(snapshot.resolved_transport.as_deref(), Some("legacy-sse"));
        assert!(
            snapshot
                .endpoint_url
                .as_deref()
                .unwrap()
                .ends_with("/message")
        );
        stop_streamable_http_mcp_server(task).await;
    }

    #[tokio::test]
    async fn synchronize_disconnects_removed_servers() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let mut state = registry.state.write();
        state.servers.insert(
            "old".to_string(),
            McpServerState {
                config: http_test_config("old", McpTransport::StreamableHttp, "http://127.0.0.1"),
                status: McpServerStatus::Connected,
                error: None,
                capabilities: None,
                tools: Vec::new(),
                resources: Vec::new(),
                runtime_id: None,
                endpoint_url: Some("http://127.0.0.1".to_string()),
                resolved_transport: Some(McpEffectiveTransport::StreamableHttp),
                session_id: None,
                generation: 1,
            },
        );
        drop(state);
        registry.synchronize_configs(Vec::new()).await;
        assert!(registry.snapshots().is_empty());
    }

    #[tokio::test]
    async fn stale_runtime_error_does_not_clobber_new_generation() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        {
            let mut state = registry.state.write();
            state.generations.insert("srv".to_string(), 2);
            state.servers.insert(
                "srv".to_string(),
                connected_http_state(
                    http_test_config("srv", McpTransport::StreamableHttp, "http://127.0.0.1"),
                    2,
                    "new-tool",
                ),
            );
        }

        registry
            .apply_runtime_error("srv", 1, "old socket closed".to_string())
            .await;

        let snapshot = registry.snapshots().pop().unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(snapshot.tools[0].name, "new-tool");
        assert!(snapshot.error.is_none());
    }

    #[tokio::test]
    async fn runtime_error_preserves_http_transport_metadata() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        {
            let mut state = registry.state.write();
            state.generations.insert("srv".to_string(), 1);
            let mut server = connected_http_state(
                http_test_config("srv", McpTransport::StreamableHttp, "http://127.0.0.1"),
                1,
                "ping",
            );
            server.endpoint_url = Some("http://127.0.0.1/message".to_string());
            server.session_id = Some("session-1".to_string());
            server.resolved_transport = Some(McpEffectiveTransport::LegacySse);
            state.servers.insert("srv".to_string(), server);
        }

        registry
            .apply_runtime_error("srv", 1, "socket closed".to_string())
            .await;

        let snapshot = registry.snapshots().pop().unwrap();
        assert_eq!(snapshot.status, "error");
        assert_eq!(
            snapshot.endpoint_url.as_deref(),
            Some("http://127.0.0.1/message")
        );
        assert_eq!(snapshot.session_id.as_deref(), Some("session-1"));
        assert_eq!(snapshot.resolved_transport.as_deref(), Some("legacy-sse"));
        assert!(snapshot.tools.is_empty());
    }

    #[test]
    fn validate_http_url_rejects_non_http_transports() {
        assert!(validate_mcp_http_url("http://localhost:3000").is_ok());
        assert!(validate_mcp_http_url("https://example.com/mcp").is_ok());
        assert!(validate_mcp_http_url("file:///tmp/mcp").is_err());
    }

    #[test]
    fn http_headers_match_tauri_auth_and_reserved_filtering() {
        let mut config = http_test_config("auth", McpTransport::StreamableHttp, "http://127.0.0.1");
        config.auth_header_name = Some("X-API-Key".to_string());
        config.auth_header_mode = Some(McpAuthHeaderMode::Raw);
        config
            .headers
            .insert("X-Workspace".to_string(), "prod".to_string());
        config
            .headers
            .insert("Accept".to_string(), "text/plain".to_string());
        config
            .headers
            .insert("MCP-Session-Id".to_string(), "bad".to_string());

        let headers = build_http_headers(
            &config,
            Some("token-123"),
            None,
            STREAMABLE_HTTP_PROTOCOL_VERSION,
            true,
            "application/json, text/event-stream",
        )
        .unwrap();

        assert_eq!(headers["X-API-Key"], "token-123");
        assert_eq!(headers["X-Workspace"], "prod");
        assert_eq!(headers["accept"], "application/json, text/event-stream");
        assert!(!headers.contains_key("authorization"));
        assert!(!headers.contains_key("mcp-session-id"));
    }

    #[test]
    fn sse_parser_matches_tauri_line_semantics() {
        let mut parser = SseEventParser::default();
        parser.push_str(": keepalive\r\n");
        parser.push_str("event: message\r\n");
        parser.push_str("data: {\"a\":1}\r\n");
        parser.push_str("data: {\"b\":2}\r\n\r\n");
        parser.push_str("data: tail");

        let events = parser.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "message");
        assert_eq!(events[0].data, "{\"a\":1}\n{\"b\":2}");

        let events = parser.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "message");
        assert_eq!(events[0].data, "tail");
    }

    #[test]
    fn mcp_tool_output_keeps_error_text_out_of_truncation_meta() {
        let result = McpCallToolResult {
            is_error: true,
            content: vec![McpCallContent {
                content_type: "text".to_string(),
                text: Some("bad input".to_string()),
                data: None,
                mime_type: None,
            }],
        };

        let (ok, output, truncated) = mcp_tool_output(&result);
        assert!(!ok);
        assert_eq!(output, "bad input");
        assert!(!truncated);
    }

    #[test]
    fn mcp_output_truncates_like_tauri_char_slice() {
        let text = "你".repeat(MCP_TOOL_OUTPUT_MAX_CHARS + 1);
        let result = McpCallToolResult {
            is_error: false,
            content: vec![McpCallContent {
                content_type: "text".to_string(),
                text: Some(text),
                data: None,
                mime_type: None,
            }],
        };

        let (ok, output, truncated) = mcp_tool_output(&result);
        assert!(ok);
        assert!(truncated);
        assert_eq!(output.chars().count(), MCP_TOOL_OUTPUT_MAX_CHARS);
    }

    fn http_test_config(id: &str, transport: McpTransport, url: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            name: id.to_string(),
            transport,
            url: Some(url.to_string()),
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            auth_header_name: None,
            auth_header_mode: None,
            headers: HashMap::new(),
            enabled: true,
            retry_on_disconnect: false,
            auth_token: None,
        }
    }

    fn connected_http_state(
        config: McpServerConfig,
        generation: u64,
        tool_name: &str,
    ) -> McpServerState {
        McpServerState {
            config,
            status: McpServerStatus::Connected,
            error: None,
            capabilities: Some(McpServerCapabilities {
                tools: Some(serde_json::json!({})),
                resources: None,
                prompts: None,
            }),
            tools: vec![McpToolSchema {
                name: tool_name.to_string(),
                description: None,
                input_schema: serde_json::json!({ "type": "object" }),
            }],
            resources: Vec::new(),
            runtime_id: None,
            endpoint_url: Some("http://127.0.0.1".to_string()),
            resolved_transport: Some(McpEffectiveTransport::StreamableHttp),
            session_id: None,
            generation,
        }
    }

    async fn spawn_streamable_http_mcp_server(
        force_legacy: bool,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let header_end = loop {
                        let mut chunk = [0_u8; 1024];
                        let Ok(read) = stream.read(&mut chunk).await else {
                            return;
                        };
                        if read == 0 {
                            return;
                        }
                        buffer.extend_from_slice(&chunk[..read]);
                        if let Some(index) = find_header_end(&buffer) {
                            break index;
                        }
                    };
                    let headers = String::from_utf8_lossy(&buffer[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .or_else(|| line.strip_prefix("Content-Length:"))
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or_default();
                    let mut body = buffer[(header_end + 4)..].to_vec();
                    while body.len() < content_length {
                        let mut chunk = vec![0_u8; content_length - body.len()];
                        let Ok(read) = stream.read(&mut chunk).await else {
                            return;
                        };
                        if read == 0 {
                            return;
                        }
                        body.extend_from_slice(&chunk[..read]);
                    }
                    let request_line = headers.lines().next().unwrap_or_default();
                    if request_line.starts_with("GET ") {
                        let body = "event: endpoint\ndata: /message\n\n";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    if force_legacy && request_line.starts_with("POST / ") {
                        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    let request: Value = serde_json::from_slice(&body).unwrap_or_default();
                    let session_id = match request.get("method").and_then(Value::as_str) {
                        Some("tools/list") => "tools-session",
                        Some("resources/list") => "resources-session",
                        _ => "test-session",
                    };
                    let response_body = mcp_http_response_body(&request);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMCP-Session-Id: {}\r\nContent-Length: {}\r\n\r\n{}",
                        session_id,
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (format!("http://{addr}"), task)
    }

    async fn stop_streamable_http_mcp_server(task: tokio::task::JoinHandle<()>) {
        task.abort();
        let _ = task.await;
    }

    fn mcp_http_response_body(request: &Value) -> String {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let result = match request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "initialize" => serde_json::json!({
                "protocolVersion": STREAMABLE_HTTP_PROTOCOL_VERSION,
                "capabilities": { "tools": {}, "resources": {} }
            }),
            "tools/list" => serde_json::json!({
                "tools": [{
                    "name": "ping",
                    "description": "Ping test tool",
                    "inputSchema": { "type": "object", "properties": {} }
                }]
            }),
            "resources/list" => serde_json::json!({
                "resources": [{
                    "uri": "test://resource",
                    "name": "resource",
                    "description": "Test resource",
                    "mimeType": "text/plain"
                }]
            }),
            _ => serde_json::json!({}),
        };
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
