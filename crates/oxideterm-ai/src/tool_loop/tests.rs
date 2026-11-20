mod agent_loop_tests {
    use super::*;
    use crate::agent::AgentModel;
    use serde_json::{Value, json};
    use std::{sync::Arc, time::Duration};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn services(path: &std::path::Path) -> AiModelBackendServices {
        let keys = crate::AiProviderKeyStore::new();
        AiModelBackendServices {
            rag_store: Arc::new(crate::RagStore::new(path).unwrap()),
            ai_mcp_registry: crate::McpRegistry::new(keys.clone()),
            ai_key_store: keys,
            ai_providers: vec![],
            ai_embedding_config: None,
            agent_model_limits: HashMap::new(),
        }
    }

    async fn model_server(
        responses: Vec<(u16, Value)>,
    ) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, response) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    headers.push(socket.read_u8().await.unwrap());
                }
                let headers = String::from_utf8(headers).unwrap();
                assert_eq!(headers.lines().next(), Some("POST /v1/responses HTTP/1.1"));
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse().unwrap())
                    })
                    .unwrap();
                let mut body = vec![0; length];
                socket.read_exact(&mut body).await.unwrap();
                requests.push(serde_json::from_slice(&body).unwrap());
                let body = response.to_string();
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nRetry-After: 0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (url, task)
    }

    fn call(id: &str, name: &str, args: Value) -> Value {
        json!({"type":"function_call","id":format!("fc_{id}"),"call_id":id,"name":name,"arguments":args.to_string()})
    }

    fn completed(output: Value) -> Value {
        json!({"status":"completed","output":output})
    }

    fn executed(id: String, name: String, summary: &str) -> AiExecutedToolResult {
        AiExecutedToolResult {
            images: Vec::new(),
            tool_call_id: id,
            tool_name: name.clone(),
            success: true,
            output: summary.into(),
            error: None,
            duration_ms: 1,
            envelope: json!({"ok":true,"summary":summary,"output":summary,"meta":{"toolName":name}}),
        }
    }

    #[tokio::test]
    async fn ui_tool_delivery_preserves_audit_request_identity() {
        use oxideterm_audit::{AuditContext, AuditKeyProvider, AuditService, AuditSource};
        use zeroize::Zeroizing;

        struct Keys;
        impl AuditKeyProvider for Keys {
            fn load(&self, _: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
                Ok(Zeroizing::new(vec![31; 32]))
            }
            fn create(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, oxideterm_audit::AuditError> {
                self.load(id)
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let audit =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let mut context = AuditContext::new(audit.client(), AuditSource::Ai);
        context.parent_id = Some("originating-tool-operation".into());
        context.agent_id = Some(Zeroizing::new("request-agent".into()));
        let (sender, receiver) = AiStreamDeliverySender::channel();
        let host = std::thread::spawn(move || {
            let delivery = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
            let AiStreamDeliveryEvent::ToolExecutionRequested {
                audit_context,
                tool_call_id,
                name,
                sender,
                ..
            } = delivery.event
            else {
                panic!("expected a UI tool request");
            };
            let delivered =
                audit_context.expect("request lost its audit context at the UI boundary");
            assert_eq!(delivered.source, AuditSource::Ai);
            assert_eq!(
                delivered.parent_id.as_deref(),
                Some("originating-tool-operation")
            );
            assert_eq!(
                delivered.agent_id.as_deref().map(String::as_str),
                Some("request-agent")
            );
            sender
                .send(executed(tool_call_id, name, "accepted"))
                .unwrap();
        });
        let result = context
            .scope(execute_ai_tool_uncoordinated(
                &services(directory.path()),
                &sender,
                1,
                &ToolSessionId::new(),
                "conversation",
                "assistant",
                "background-create".into(),
                "create_background_task".into(),
                json!({}),
                false,
                false,
                vec![],
                None,
            ))
            .await;
        host.join().unwrap();
        assert_eq!(result.tool_call_id, "background-create");
        assert_eq!(result.output, "accepted");
    }

    #[tokio::test]
    async fn user_question_resumes_the_same_responses_call_with_the_answer() {
        let (url, server) = model_server(vec![
            (200, completed(json!([call("mixed-question", "ask_user", json!({"question":"Which environment?"})),
                call("premature-command","run_command",json!({"command":"exit 0"}))]))),
            (200, completed(json!([call("question-wire", "ask_user", json!({"question":"Which environment?","options":["Staging","Production"]}))]))),
            (200, completed(json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Using staging."}]}]))),
        ]).await;
        let mut config = loop_test_chat_config();
        config.api_protocol = crate::AiApiProtocol::Responses;
        config.base_url = url;
        config.tool_policy.enabled = true;
        config.tools = crate::agent::agent_tool_definitions(false)
            .into_iter()
            .filter(|tool| tool.name == "ask_user")
            .collect();
        config.tools.extend(
            crate::orchestrator_tool_definitions()
                .into_iter()
                .filter(|tool| tool.name == "run_command"),
        );
        let runtime = crate::agent::AgentRuntime::new(1);
        let run = runtime.create_group(
            "conversation".into(),
            AgentModel {
                provider_id: "provider".into(),
                model: config.model.clone(),
            },
            crate::agent::AgentScope::default(),
            8,
        );
        let execution = AgentExecution::new(
            runtime,
            run,
            crate::agent::AgentResourceCoordinator::default(),
        )
        .unwrap();
        let (sender, receiver) = AiStreamDeliverySender::channel();
        let host = std::thread::spawn(move || {
            let mut asked = false;
            let mut final_text = String::new();
            while let Ok(delivery) = receiver.recv_timeout(Duration::from_secs(5)) {
                match delivery.event {
                    AiStreamDeliveryEvent::HistoryBarrier(sender) => {
                        let _ = sender.send(true);
                    }
                    AiStreamDeliveryEvent::RuntimeContextRequested { sender, .. } => {
                        sender.send(Some("Test runtime".into())).unwrap();
                    }
                    AiStreamDeliveryEvent::UserQuestionRequested { call, sender, .. } => {
                        assert!(!asked, "a pending question must not poll or repeat");
                        asked = true;
                        assert_eq!(
                            serde_json::from_str::<Value>(&call.arguments).unwrap()["question"],
                            "Which environment?"
                        );
                        sender
                            .send(zeroize::Zeroizing::new("Staging, read only".into()))
                            .unwrap();
                    }
                    AiStreamDeliveryEvent::ToolExecutionRequested { .. } => {
                        panic!("question reached a side-effect executor")
                    }
                    AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(text)) => {
                        final_text.push_str(&text)
                    }
                    _ => {}
                }
            }
            assert!(asked);
            assert_eq!(final_text, "Using staging.");
        });
        let temp = tempfile::tempdir().unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            run_ai_chat_tool_loop(
                config,
                vec![agent_chat_message(
                    AiChatRole::User,
                    "Inspect the deployment".into(),
                )],
                AiModelRuntimeState {
                    context_window: 128000,
                },
                services(temp.path()),
                1,
                1,
                ToolSessionId::new(),
                "conversation".into(),
                "assistant".into(),
                sender,
                Some(execution),
            ),
        )
        .await
        .unwrap();
        host.join().unwrap();
        let requests = server.await.unwrap();
        let output = requests[2]["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| {
                item["type"] == "function_call_output" && item["call_id"] == "question-wire"
            })
            .unwrap();
        assert_eq!(output["call_id"], "question-wire");
        let answer: Value = serde_json::from_str(output["output"].as_str().unwrap()).unwrap();
        let response: Value = serde_json::from_str(answer["output"].as_str().unwrap()).unwrap();
        assert_eq!(response["answer"], "Staging, read only");
    }

    #[tokio::test]
    async fn user_question_wait_is_cancelled_by_steering_or_stopping() {
        for steer in [true, false] {
            let runtime = crate::agent::AgentRuntime::new(1);
            let run = runtime.create_group(
                "conversation".into(),
                AgentModel {
                    provider_id: "provider".into(),
                    model: "model".into(),
                },
                crate::agent::AgentScope::default(),
                8,
            );
            let dispatch = runtime.dispatch(&run).unwrap();
            let agent = AgentExecution::new(
                runtime.clone(),
                run.clone(),
                crate::agent::AgentResourceCoordinator::default(),
            )
            .unwrap();
            let (ui, receiver) = AiStreamDeliverySender::channel();
            let (question_tx, question_rx) = tokio::sync::oneshot::channel();
            let host = std::thread::spawn(move || {
                while let Ok(delivery) = receiver.recv_timeout(Duration::from_secs(5)) {
                    if let AiStreamDeliveryEvent::UserQuestionRequested { sender, .. } =
                        delivery.event
                    {
                        question_tx.send(sender).unwrap();
                        break;
                    }
                }
            });
            let task = tokio::spawn(async move {
                let mut execution = Some(agent);
                execute_ai_agent_coordination(
                    &mut execution,
                    &ui,
                    1,
                    &ToolSessionId::new(),
                    "conversation",
                    "assistant",
                    &AiToolCall {
                        id: "question".into(),
                        name: "ask_user".into(),
                        arguments: json!({"question":"Which host?"}).to_string(),
                    },
                    Some(&dispatch),
                )
                .await
            });
            let answer_sender = question_rx.await.unwrap();
            assert_eq!(
                runtime.snapshot(&run).unwrap().state,
                AgentState::AwaitingUser
            );
            assert!(!task.is_finished());
            if steer {
                runtime
                    .send(
                        &run,
                        &run,
                        AgentMessageKind::UserSupplement,
                        AgentText::new("Cancel this approach"),
                    )
                    .unwrap();
            } else {
                runtime.cancel_group(&run).unwrap();
            }
            let result = tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                result
                    .envelope
                    .pointer("/error/code")
                    .and_then(Value::as_str),
                Some(if steer {
                    "agent_direction_changed"
                } else {
                    "operation_cancelled"
                })
            );
            assert!(
                answer_sender
                    .send(zeroize::Zeroizing::new("Late answer".into()))
                    .is_err()
            );
            host.join().unwrap();
        }
    }

    #[tokio::test]
    async fn agent_wait_preserves_user_direction_as_user_history() {
        let runtime = crate::agent::AgentRuntime::new(2);
        let run = runtime.create_group(
            "conversation".into(),
            crate::agent::AgentModel {
                provider_id: "provider".into(),
                model: "model".into(),
            },
            crate::agent::AgentScope::default(),
            8,
        );
        let mut execution = Some(
            AgentExecution::new(
                runtime.clone(),
                run.clone(),
                crate::agent::AgentResourceCoordinator::default(),
            )
            .unwrap(),
        );
        runtime
            .send(
                &run,
                &run,
                AgentMessageKind::UserSupplement,
                AgentText::new("Only inspect; do not modify files"),
            )
            .unwrap();
        let (sender, _receiver) = AiStreamDeliverySender::channel();
        let result = execute_ai_agent_coordination(
            &mut execution,
            &sender,
            1,
            &ToolSessionId::new(),
            "conversation",
            "assistant",
            &AiToolCall {
                id: "wait".into(),
                name: "wait_agents".into(),
                arguments: "{}".into(),
            },
            None,
        )
        .await;
        assert!(result.success, "{}", result.output);
        let mut history = Vec::new();
        append_agent_mailbox(&mut history, execution.as_mut().unwrap());
        assert_eq!(
            history
                .iter()
                .map(|message| (message.role, message.content.as_str()))
                .collect::<Vec<_>>(),
            vec![(AiChatRole::User, "Only inspect; do not modify files")]
        );
        append_agent_mailbox(&mut history, execution.as_mut().unwrap());
        assert_eq!(
            history
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["Only inspect; do not modify files"]
        );
    }

    #[tokio::test]
    async fn steering_revokes_pending_approval_before_a_command_can_dispatch() {
        let runtime = crate::agent::AgentRuntime::new(2);
        let run = runtime.create_group(
            "conversation".into(),
            crate::agent::AgentModel {
                provider_id: "provider".into(),
                model: "model".into(),
            },
            crate::agent::AgentScope::default(),
            8,
        );
        let dispatch = runtime.dispatch(&run).unwrap();
        let (sender, receiver) = AiStreamDeliverySender::channel();
        let runtime_sender = runtime.clone();
        let target = run.clone();
        let host = std::thread::spawn(move || {
            loop {
                match receiver.recv_timeout(Duration::from_secs(5)).unwrap().event {
                    AiStreamDeliveryEvent::ToolPreflightRequested { sender, .. } => {
                        sender.send(None).unwrap();
                    }
                    AiStreamDeliveryEvent::ToolApprovalRequested { sender, .. } => {
                        runtime_sender
                            .send(
                                &target,
                                &target,
                                AgentMessageKind::UserSupplement,
                                AgentText::new("Only inspect; do not change files"),
                            )
                            .unwrap();
                        let _ = sender.send(true);
                    }
                    AiStreamDeliveryEvent::ToolExecutionRequested { .. } => {
                        panic!("old command dispatched after steering")
                    }
                    AiStreamDeliveryEvent::ToolStatus { status, .. } if status == "rejected" => {
                        break;
                    }
                    _ => {}
                }
            }
        });
        let mut config = loop_test_chat_config();
        config.tool_policy.enabled = true;
        let call=AiToolCall { id:"write".into(),name:"run_command".into(),arguments:serde_json::json!({"command":"touch /tmp/result","handle_id":"rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}).to_string() };
        let temp = tempfile::tempdir().unwrap();
        let result = execute_ai_round_call(
            call,
            &config,
            &services(temp.path()),
            &std::collections::HashSet::from(["run_command".into()]),
            &sender,
            1,
            &ToolSessionId::new(),
            "conversation",
            "assistant",
            &mut None,
            true,
            Some(&dispatch),
        )
        .await
        .unwrap();
        assert_eq!(
            result
                .envelope
                .pointer("/error/code")
                .and_then(serde_json::Value::as_str),
            Some("agent_direction_changed")
        );
        host.join().unwrap();
    }

    #[tokio::test]
    async fn parallel_reads_preserve_write_order_and_retry_never_reexecutes_tools() {
        let handle = "rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let first = completed(json!([
            call(
                "wire-a",
                "read_resource",
                json!({"resource":"file","path":"/a","handle_id":handle})
            ),
            call(
                "wire-b",
                "read_resource",
                json!({"resource":"file","path":"/b","handle_id":handle})
            ),
            call(
                "wire-write",
                "run_command",
                json!({"command":"touch /tmp/result","handle_id":handle})
            )
        ]));
        let last = completed(
            json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Finished"}]}]),
        );
        let (url, server) = model_server(vec![(200, first), (503, json!({})), (200, last)]).await;
        let mut config = loop_test_chat_config();
        config.api_protocol = crate::AiApiProtocol::Responses;
        config.base_url = url;
        config.tool_policy.enabled = true;
        config.tools = crate::orchestrator_tool_definitions()
            .into_iter()
            .filter(|tool| matches!(tool.name.as_str(), "read_resource" | "run_command"))
            .collect();
        let temp = tempfile::tempdir().unwrap();
        let (sender, receiver) = AiStreamDeliverySender::channel();
        let host = std::thread::spawn(move || {
            let mut pending = Vec::new();
            let mut reads_completed = 0;
            let mut writes = 0;
            let mut checkpoint = None;
            let mut answer = String::new();
            loop {
                let event = receiver.recv_timeout(Duration::from_secs(5)).unwrap().event;
                match event {
                    AiStreamDeliveryEvent::RuntimeContextRequested { sender, .. } => {
                        sender.send(Some("Test runtime".into())).unwrap();
                    }
                    AiStreamDeliveryEvent::ToolPreflightRequested { sender, .. } => {
                        sender.send(None).unwrap();
                    }
                    AiStreamDeliveryEvent::ToolExecutionRequested {
                        sender,
                        tool_call_id,
                        name,
                        args,
                        ..
                    } if name == "read_resource" => {
                        pending.push((
                            sender,
                            tool_call_id,
                            name,
                            args["path"].as_str().unwrap().to_owned(),
                        ));
                        // Neither request can complete until both have reached the real loop's executor boundary.
                        if pending.len() == 2 {
                            // Preflight awaits may reorder arrivals; complete in reverse model order deliberately.
                            pending.sort_by(|left, right| left.3.cmp(&right.3));
                            assert_eq!(
                                pending
                                    .iter()
                                    .map(|item| item.3.as_str())
                                    .collect::<Vec<_>>(),
                                vec!["/a", "/b"]
                            );
                            for (sender, id, name, path) in pending.drain(..).rev() {
                                sender
                                    .send(executed(id, name, &format!("Read {path}")))
                                    .unwrap();
                                reads_completed += 1;
                            }
                        }
                    }
                    AiStreamDeliveryEvent::ToolApprovalRequested { sender, .. } => {
                        assert_eq!(reads_completed, 2);
                        sender.send(true).unwrap();
                    }
                    AiStreamDeliveryEvent::ToolExecutionRequested {
                        sender,
                        tool_call_id,
                        name,
                        ..
                    } => {
                        assert_eq!(name, "run_command");
                        assert_eq!(reads_completed, 2);
                        writes += 1;
                        sender
                            .send(executed(tool_call_id, name, "Created result"))
                            .unwrap();
                    }
                    AiStreamDeliveryEvent::HistoryBarrier(sender) => {
                        let _ = sender.send(true);
                    }
                    AiStreamDeliveryEvent::Checkpoint(value) => checkpoint = Some(value),
                    AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(text)) => {
                        answer.push_str(&text)
                    }
                    AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error)) => {
                        panic!("unexpected error {error}")
                    }
                    AiStreamDeliveryEvent::Stream(AiStreamEvent::Done) => break,
                    _ => {}
                }
            }
            let checkpoint = checkpoint.unwrap();
            assert_eq!(
                checkpoint
                    .actions
                    .iter()
                    .map(|action| action.summary.as_str())
                    .collect::<Vec<_>>(),
                vec!["Read /a", "Read /b", "Created result"]
            );
            assert!(!checkpoint.needs_continuation);
            assert_eq!(writes, 1);
            assert_eq!(answer, "Finished");
        });
        let history = vec![agent_chat_message(
            AiChatRole::User,
            "Read both files, then create the result".into(),
        )];
        tokio::time::timeout(
            Duration::from_secs(10),
            run_ai_chat_tool_loop(
                config,
                history,
                AiModelRuntimeState {
                    context_window: 128000,
                },
                services(temp.path()),
                1,
                1,
                ToolSessionId::new(),
                "conversation".into(),
                "assistant".into(),
                sender,
                None,
            ),
        )
        .await
        .unwrap();
        host.join().unwrap();
        let requests = server.await.unwrap();
        let outputs=requests[1]["input"].as_array().unwrap().iter().filter(|item|item["type"]=="function_call_output")
            .map(|item|(item["call_id"].as_str().unwrap(),serde_json::from_str::<Value>(item["output"].as_str().unwrap()).unwrap()["summary"].as_str().unwrap().to_owned())).collect::<Vec<_>>();
        assert_eq!(
            outputs,
            vec![
                ("wire-a", "Read /a".into()),
                ("wire-b", "Read /b".into()),
                ("wire-write", "Created result".into())
            ]
        );
        assert_eq!(requests[1], requests[2]);
    }
    #[tokio::test]
    async fn running_compaction_keeps_middle_evidence_and_does_not_replace_history_on_failure() {
        for success in [true, false] {
            let mut response = completed(
                json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Verified: port 9000 is occupied. Pending: choose another port; do not restart the database."}]}]),
            );
            if !success {
                response["status"] = json!("incomplete");
                response["incomplete_details"] = json!({"reason":"max_output_tokens"});
            }
            let (url, server) = model_server(vec![(200, response)]).await;
            let mut config = loop_test_chat_config();
            config.api_protocol = crate::AiApiProtocol::Responses;
            config.base_url = url;
            let mut task = agent_chat_message(
                AiChatRole::User,
                "Fix deployment without restarting the database".into(),
            );
            task.id = "task".into();
            let mut old = agent_chat_message(
                AiChatRole::Assistant,
                format!(
                    "{}\nMIDDLE EVIDENCE: port 9000 is occupied\n{}",
                    "x".repeat(8000),
                    "z".repeat(8000)
                ),
            );
            old.id = "old".into();
            let mut live = agent_chat_message(AiChatRole::Assistant, String::new());
            live.id = "live".into();
            live.tool_calls = vec![json!({"id":"call","name":"read_resource","arguments":"{}"})];
            let mut result =
                agent_chat_message(AiChatRole::Tool, "Read current configuration".into());
            result.id = "result".into();
            result.tool_call_id = Some("call".into());
            let mut history = vec![task, old, live, result];
            let mut checkpoint = crate::agent::AgentCheckpoint::new("Fix deployment");
            checkpoint.working_notes = AgentText::new("previous notes");
            let outcome =
                compact_running_ai_task(&mut history, &config, &mut checkpoint, "task", 8000, None)
                    .await;
            if success {
                outcome.unwrap();
                assert_eq!(
                    history
                        .iter()
                        .map(|message| message.id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["agent-checkpoint", "task", "live", "result"]
                );
                assert!(history[0].content.contains("port 9000 is occupied"));
                assert_eq!(
                    history[1].content,
                    "Fix deployment without restarting the database"
                );
                assert_eq!(history[3].tool_call_id.as_deref(), Some("call"));
            } else {
                assert_eq!(outcome, Err("agent_compaction_failed".into()));
                assert_eq!(
                    history
                        .iter()
                        .map(|message| message.id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["task", "old", "live", "result"]
                );
                assert_eq!(checkpoint.working_notes.as_str(), "previous notes");
            }
            let requests = server.await.unwrap();
            assert!(
                requests[0]["input"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("MIDDLE EVIDENCE: port 9000 is occupied")
            );
            assert_eq!(requests[0].get("tools"), None);
            assert_eq!(requests[0]["max_output_tokens"], 250);
        }
    }

    #[tokio::test]
    async fn cancelling_the_loop_drops_all_pending_read_replies() {
        let first = completed(json!([
            call(
                "a",
                "read_resource",
                json!({"resource":"file","path":"/a","handle_id":"rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})
            ),
            call(
                "b",
                "read_resource",
                json!({"resource":"file","path":"/b","handle_id":"rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})
            )
        ]));
        let (url, server) = model_server(vec![(200, first)]).await;
        let mut config = loop_test_chat_config();
        config.api_protocol = crate::AiApiProtocol::Responses;
        config.base_url = url;
        config.tool_policy.enabled = true;
        config.tools = crate::orchestrator_tool_definitions()
            .into_iter()
            .filter(|tool| tool.name == "read_resource")
            .collect();
        let temp = tempfile::tempdir().unwrap();
        let backend = services(temp.path());
        let (sender, receiver) = AiStreamDeliverySender::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (released, release) = std::sync::mpsc::channel();
        let host = std::thread::spawn(move || {
            let mut pending = Vec::new();
            let mut checkpoint = None;
            while pending.len() < 2 {
                match receiver.recv_timeout(Duration::from_secs(5)).unwrap().event {
                    AiStreamDeliveryEvent::RuntimeContextRequested { sender, .. } => {
                        sender.send(Some("runtime".into())).unwrap();
                    }
                    AiStreamDeliveryEvent::HistoryBarrier(sender) => {
                        let _ = sender.send(true);
                    }
                    AiStreamDeliveryEvent::Checkpoint(value) => checkpoint = Some(value),
                    AiStreamDeliveryEvent::ToolPreflightRequested { sender, .. } => {
                        sender.send(None).unwrap();
                    }
                    AiStreamDeliveryEvent::ToolExecutionRequested { sender, name, .. } => {
                        assert_eq!(name, "read_resource");
                        pending.push(sender);
                    }
                    AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(error)) => {
                        panic!("unexpected {error}")
                    }
                    _ => {}
                }
            }
            let checkpoint = checkpoint.expect("checkpoint precedes tool dispatch");
            assert_eq!(
                checkpoint
                    .actions
                    .iter()
                    .map(|action| (action.tool.as_str(), action.error_code.as_deref()))
                    .collect::<Vec<_>>(),
                vec![
                    ("read_resource", Some("outcome_unknown")),
                    ("read_resource", Some("outcome_unknown"))
                ]
            );
            assert!(checkpoint.needs_continuation);
            started.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(pending.iter().all(tokio::sync::oneshot::Sender::is_closed));
        });
        let task = tokio::spawn(run_ai_chat_tool_loop(
            config,
            vec![agent_chat_message(
                AiChatRole::User,
                "Read both files".into(),
            )],
            AiModelRuntimeState {
                context_window: 128000,
            },
            backend,
            1,
            1,
            ToolSessionId::new(),
            "conversation".into(),
            "assistant".into(),
            sender,
            None,
        ));
        tokio::time::timeout(Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        released.send(()).unwrap();
        host.join().unwrap();
        assert_eq!(server.await.unwrap().len(), 1);
    }
}

#[cfg(test)]
fn loop_test_chat_config() -> crate::AiChatStreamConfig {
    crate::AiChatStreamConfig {
        api_protocol: crate::AiApiProtocol::default(),
        image_input: Default::default(),
        omit_history_images: false,
        execution_backend: Default::default(),
        provider_id: Some("provider".into()),
        acp_agent_id: None,
        acp_session_id: None,
        acp_config_selection: None,
        provider_type: "openai".into(),
        base_url: String::new(),
        model: "queued-model".into(),
        api_key: None,
        max_response_tokens: None,
        reasoning_effort: None,
        safety_mode: Default::default(),
        profile_id: None,
        memory_context: None,
        memory_entry_ids: Vec::new(),
        prompt_cache_key: None,
        tool_policy: Default::default(),
        tools: Vec::new(),
        tool_choice: crate::AiToolChoice::Auto,
    }
}
