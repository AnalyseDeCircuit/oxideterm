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
            broadcast::channel(1).0,
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
            broadcast::channel(1).0,
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
            broadcast::channel(1).0,
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
            broadcast::channel(1).0,
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
            broadcast::channel(1).0,
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
            broadcast::channel(1).0,
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

    #[tokio::test]
    async fn dropping_registry_clone_does_not_stop_shared_processes() {
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        let ordinary_clone = registry.clone();

        drop(ordinary_clone);
        // Give cleanup tasks a deterministic opportunity to run on the test runtime.
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }

        assert_eq!(
            registry
                .processes
                .stop_all_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );

        registry.shutdown().await;
        assert_eq!(
            registry
                .processes
                .stop_all_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
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
    fn modern_rpc_errors_never_trigger_legacy_http_fallback() {
        for code in [
            MCP_HEADER_MISMATCH,
            MCP_MISSING_REQUIRED_CLIENT_CAPABILITY,
            MCP_UNSUPPORTED_PROTOCOL_VERSION,
        ] {
            let error = McpError::Rpc {
                code,
                message: "modern error".to_string(),
                data: None,
                status: Some(StatusCode::BAD_REQUEST),
            };
            assert!(is_recognized_modern_http_error(&error));
            assert!(!is_legacy_http_probe_error(&error));
        }
        let method_not_found = McpError::Rpc {
            code: JSON_RPC_METHOD_NOT_FOUND,
            message: "method not found".to_string(),
            data: None,
            status: Some(StatusCode::NOT_FOUND),
        };
        assert!(is_recognized_modern_http_error(&method_not_found));
    }

    #[test]
    fn subscription_notifications_are_correlated_by_request_id() {
        let expected_id = serde_json::json!(7);
        let matching = serde_json::json!({
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/subscriptionId": 7
                }
            }
        });
        let stale = serde_json::json!({
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/subscriptionId": 6
                }
            }
        });
        assert!(notification_matches_subscription(&matching, &expected_id));
        assert!(!notification_matches_subscription(&stale, &expected_id));
    }

    #[test]
    fn subscription_acknowledgment_limits_delivered_notifications() {
        let acknowledgment = serde_json::json!({
            "params": {
                "notifications": {
                    "toolsListChanged": true,
                    "resourceSubscriptions": ["test://allowed"]
                }
            }
        });
        let filter = acknowledged_subscription_filter(&acknowledgment).unwrap();
        assert!(subscription_filter_allows_notification(
            &filter,
            &serde_json::json!({ "method": "notifications/tools/list_changed" }),
        ));
        assert!(!subscription_filter_allows_notification(
            &filter,
            &serde_json::json!({ "method": "notifications/resources/list_changed" }),
        ));
        assert!(subscription_filter_allows_notification(
            &filter,
            &serde_json::json!({
                "method": "notifications/resources/updated",
                "params": { "uri": "test://allowed" }
            }),
        ));
        assert!(!subscription_filter_allows_notification(
            &filter,
            &serde_json::json!({
                "method": "notifications/resources/updated",
                "params": { "uri": "test://other" }
            }),
        ));
    }

    #[test]
    fn protocol_era_does_not_choose_nonstandard_stdio_framing() {
        assert_eq!(
            McpProtocol::modern_preferred().stdio_framing,
            McpStdioFraming::LineDelimited
        );
        assert_eq!(
            McpProtocol::legacy_streamable_http().stdio_framing,
            McpStdioFraming::LineDelimited
        );
        assert_eq!(
            McpProtocol::legacy_content_length_stdio().stdio_framing,
            McpStdioFraming::LegacyContentLength
        );
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
            auth_header_mode: Some(McpAuthHeaderMode::None),
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
                protocol: None,
                tools_cache: None,
                resources_cache: None,
                resource_content_cache: HashMap::new(),
                resource_subscriptions: std::collections::HashSet::new(),
                subscription_abort: None,
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
                protocol: None,
                tools_cache: None,
                resources_cache: None,
                resource_content_cache: HashMap::new(),
                resource_subscriptions: std::collections::HashSet::new(),
                subscription_abort: None,
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

    #[tokio::test]
    async fn mcp_tools_and_resources_follow_config_order() {
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
            .await
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
    async fn modern_http_discovers_without_initialize_and_sends_routing_headers() {
        let (url, requests, task) = spawn_modern_http_mcp_server().await;
        let registry = McpRegistry::new(AiProviderKeyStore::new());
        registry
            .connect_config(http_test_config(
                "modern",
                McpTransport::StreamableHttp,
                &url,
            ))
            .await;

        let snapshot = registry
            .snapshots()
            .into_iter()
            .find(|server| server.config.id == "modern")
            .unwrap();
        assert_eq!(snapshot.status, "connected");
        assert_eq!(snapshot.protocol_era.as_deref(), Some("modern"));
        assert_eq!(
            snapshot.protocol_version.as_deref(),
            Some(MODERN_PROTOCOL_VERSION)
        );
        assert!(snapshot.session_id.is_none());
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let requests = requests.lock().await;
                let tool_list_calls = requests
                    .iter()
                    .filter(|(_, request)| {
                        request.get("method").and_then(Value::as_str) == Some("tools/list")
                    })
                    .count();
                drop(requests);
                if tool_list_calls >= 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let result = registry
            .call_prefixed_tool("mcp::modern::ping", serde_json::json!({ "region": "华东" }))
            .await
            .unwrap();
        assert_eq!(
            result.structured_content,
            Some(serde_json::json!({ "ok": true }))
        );
        let multi_round = registry
            .call_prefixed_tool("mcp::modern::multi", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(
            multi_round
                .content
                .first()
                .and_then(|content| content.text.as_deref()),
            Some("continued")
        );
        let task_result = registry
            .call_prefixed_tool("mcp::modern::task", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(
            task_result
                .content
                .first()
                .and_then(|content| content.text.as_deref()),
            Some("task complete")
        );
        let resource = registry
            .read_resource("modern", "test://resource")
            .await
            .unwrap();
        assert_eq!(
            resource.first().and_then(|content| content.text.as_deref()),
            Some("resource body")
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let requests = requests.lock().await;
                let subscribed = requests.iter().any(|(_, request)| {
                    request
                        .pointer("/params/notifications/resourceSubscriptions/0")
                        .and_then(Value::as_str)
                        == Some("test://resource")
                });
                drop(requests);
                if subscribed {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let requests = requests.lock().await;
        assert!(requests.iter().all(
            |(_, request)| request.get("method").and_then(Value::as_str) != Some("initialize")
        ));
        let (headers, tool_request) = requests
            .iter()
            .find(|(_, request)| {
                request.get("method").and_then(Value::as_str) == Some("tools/call")
            })
            .unwrap();
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("mcp-method: tools/call")
        );
        assert!(headers.to_ascii_lowercase().contains("mcp-name: ping"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("mcp-param-region: =?base64?")
        );
        assert_eq!(
            tool_request
                .pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion")
                .and_then(Value::as_str),
            Some(MODERN_PROTOCOL_VERSION)
        );
        let retry = requests
            .iter()
            .find(|(_, request)| {
                request
                    .pointer("/params/requestState")
                    .and_then(Value::as_str)
                    == Some("state-1")
            })
            .map(|(_, request)| request)
            .unwrap();
        assert!(retry.pointer("/params/inputResponses").is_none());
        assert!(requests.iter().any(|(_, request)| {
            request.get("method").and_then(Value::as_str) == Some("tasks/get")
        }));

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
                protocol: Some(McpProtocol::legacy_streamable_http()),
                tools_cache: None,
                resources_cache: None,
                resource_content_cache: HashMap::new(),
                resource_subscriptions: std::collections::HashSet::new(),
                subscription_abort: None,
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
            &McpProtocol::legacy_streamable_http(),
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
    fn mcp_snapshot_redacts_sensitive_args_like_tauri() {
        let redacted = redact_sensitive_args(&[
            "--api-key".to_string(),
            "secret-1".to_string(),
            "--api_key=secret-2".to_string(),
            "--authorization".to_string(),
            "secret-3".to_string(),
            "--bearer=secret-4".to_string(),
            "plain".to_string(),
        ]);

        assert_eq!(
            redacted,
            vec![
                "--api-key",
                "[redacted]",
                "--api_key=[redacted]",
                "--authorization",
                "[redacted]",
                "--bearer=[redacted]",
                "plain",
            ]
        );
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
            structured_content: None,
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
            structured_content: None,
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
            auth_header_mode: Some(McpAuthHeaderMode::None),
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
                extensions: None,
            }),
            tools: vec![McpToolSchema {
                name: tool_name.to_string(),
                description: None,
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: None,
            }],
            resources: Vec::new(),
            runtime_id: None,
            endpoint_url: Some("http://127.0.0.1".to_string()),
            resolved_transport: Some(McpEffectiveTransport::StreamableHttp),
            session_id: None,
            protocol: Some(McpProtocol::legacy_streamable_http()),
            tools_cache: None,
            resources_cache: None,
            resource_content_cache: HashMap::new(),
            resource_subscriptions: std::collections::HashSet::new(),
            subscription_abort: None,
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
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    if force_legacy && request_line.starts_with("POST / ") {
                        let response = "HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    let request: Value = serde_json::from_slice(&body).unwrap_or_default();
                    if request.get("method").and_then(Value::as_str) == Some("server/discover") {
                        let body = "legacy server";
                        let response = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    let session_id = match request.get("method").and_then(Value::as_str) {
                        Some("tools/list") => "tools-session",
                        Some("resources/list") => "resources-session",
                        _ => "test-session",
                    };
                    let response_body = mcp_http_response_body(&request, force_legacy);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nMCP-Session-Id: {}\r\nContent-Length: {}\r\n\r\n{}",
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

    fn mcp_http_response_body(request: &Value, legacy_sse: bool) -> String {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let result = match request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "initialize" => serde_json::json!({
                "protocolVersion": if legacy_sse {
                    LEGACY_SSE_PROTOCOL_VERSION
                } else {
                    LEGACY_STREAMABLE_HTTP_PROTOCOL_VERSION
                },
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

    async fn spawn_modern_http_mcp_server() -> (
        String,
        Arc<Mutex<Vec<(String, Value)>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let captured = captured.clone();
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
                    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
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
                    let request: Value = serde_json::from_slice(&body).unwrap();
                    captured
                        .lock()
                        .await
                        .push((headers.clone(), request.clone()));
                    if request.get("method").and_then(Value::as_str) == Some("subscriptions/listen")
                    {
                        let subscription_id = request.get("id").cloned().unwrap_or(Value::Null);
                        let acknowledged_notifications = request
                            .pointer("/params/notifications")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({}));
                        let acknowledged = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/subscriptions/acknowledged",
                            "params": {
                                "_meta": {
                                    "io.modelcontextprotocol/subscriptionId": subscription_id
                                },
                                "notifications": acknowledged_notifications
                            }
                        });
                        let changed = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/tools/list_changed",
                            "params": {
                                "_meta": {
                                    "io.modelcontextprotocol/subscriptionId": subscription_id
                                }
                            }
                        });
                        let complete = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": subscription_id,
                            "result": { "resultType": "complete" }
                        });
                        let response_body = format!(
                            "data: {acknowledged}\n\ndata: {changed}\n\ndata: {complete}\n\n"
                        );
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                            response_body.len(),
                            response_body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        return;
                    }
                    let result = match request
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                    {
                        "server/discover" => serde_json::json!({
                            "resultType": "complete",
                            "supportedVersions": [MODERN_PROTOCOL_VERSION],
                            "capabilities": {
                                "tools": { "listChanged": true },
                                "resources": { "subscribe": true },
                                "extensions": {
                                    MCP_TASKS_EXTENSION: {}
                                }
                            },
                            "ttlMs": 60_000,
                            "cacheScope": "private"
                        }),
                        "tools/list" => serde_json::json!({
                            "resultType": "complete",
                            "tools": [{
                                "name": "ping",
                                "description": "Ping test tool",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "region": {
                                            "type": "string",
                                            "x-mcp-header": "Region"
                                        }
                                    }
                                }
                            }, {
                                "name": "multi",
                                "inputSchema": { "type": "object", "properties": {} }
                            }, {
                                "name": "task",
                                "inputSchema": { "type": "object", "properties": {} }
                            }],
                            "ttlMs": 60_000,
                            "cacheScope": "private"
                        }),
                        "resources/list" => serde_json::json!({
                            "resultType": "complete",
                            "resources": [{
                                "uri": "test://resource",
                                "name": "resource",
                                "mimeType": "text/plain"
                            }],
                            "ttlMs": 60_000,
                            "cacheScope": "private"
                        }),
                        "resources/read" => serde_json::json!({
                            "resultType": "complete",
                            "contents": [{
                                "uri": "test://resource",
                                "mimeType": "text/plain",
                                "text": "resource body"
                            }],
                            "ttlMs": 60_000,
                            "cacheScope": "private"
                        }),
                        "tools/call" => {
                            let name = request
                                .pointer("/params/name")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            match name {
                                "multi" if request.pointer("/params/requestState").is_none() => {
                                    serde_json::json!({
                                        "resultType": "input_required",
                                        "requestState": "state-1"
                                    })
                                }
                                "multi" => serde_json::json!({
                                    "resultType": "complete",
                                    "content": [{ "type": "text", "text": "continued" }]
                                }),
                                "task" => serde_json::json!({
                                    "resultType": "task",
                                    "taskId": "task-1",
                                    "status": "working",
                                    "ttlMs": 60_000,
                                    "pollIntervalMs": 1
                                }),
                                _ => serde_json::json!({
                                    "resultType": "complete",
                                    "content": [{ "type": "text", "text": "pong" }],
                                    "structuredContent": { "ok": true }
                                }),
                            }
                        }
                        "tasks/get" => serde_json::json!({
                            "resultType": "complete",
                            "taskId": "task-1",
                            "status": "completed",
                            "ttlMs": 60_000,
                            "result": {
                                "resultType": "complete",
                                "content": [{ "type": "text", "text": "task complete" }]
                            }
                        }),
                        _ => serde_json::json!({ "resultType": "complete" }),
                    };
                    let response_body = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request.get("id").cloned().unwrap_or(Value::Null),
                        "result": result
                    })
                    .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (format!("http://{addr}"), requests, task)
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
