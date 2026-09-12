mod agent_loop_tests {
    use super::*;
    use serde_json::{Value, json};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn services(path: &std::path::Path) -> AiModelBackendServices {
        let keys = oxideterm_ai::AiProviderKeyStore::new();
        AiModelBackendServices {
            rag_store: Arc::new(oxideterm_ai::RagStore::new(path).unwrap()),
            ai_mcp_registry: oxideterm_ai::McpRegistry::new(keys.clone()),
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
    async fn local_command_control_tracks_process_completion_instead_of_tool_success() {
        use oxideterm_ai::agent::{AgentModel, AgentResourceCoordinator, AgentRuntime, AgentScope, AgentToolLease};
        let runtime = AgentRuntime::new(1);
        let run = runtime.create_group("local-command".into(), AgentModel {
            provider_id: "provider".into(), model: "model".into(),
        }, AgentScope::default(), 8);
        let resources = AgentResourceCoordinator::default();
        let key = resources.workspace_resource();
        for early_response in [None, Some(true), Some(false)] {
            let lease = resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
            let lease = AgentToolLease::new(resources.clone(), lease);
            lease.dispatched();
            let (release, wait) = tokio::sync::oneshot::channel();
            let process = tokio::spawn(async move {
                wait.await.unwrap();
                run_local_ai_command("exit 7", None, false, None).await
            });
            let mut task = AiOwnedCommandTask::new(process, vec![lease.clone()]);
            if let Some(success) = early_response { lease.finish_response(success); }
            assert!(resources.owned_by(&key, &run).is_some(), "early response must not release the process");
            release.send(()).unwrap();
            let action = (&mut task.process).await.unwrap();
            assert!(!action.ok);
            assert_eq!(action.data["exitCode"], 7);
            task.finish(&action);
            lease.finish_response(action.ok);
            drop(task);
            let next = tokio::time::timeout(Duration::from_secs(1), resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap())).await.unwrap().unwrap();
            resources.complete(&next);
        }

        let lease = resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
        let lease = AgentToolLease::new(resources.clone(), lease);
        lease.dispatched();
        let task = AiOwnedCommandTask::new(tokio::spawn(std::future::pending()), vec![lease]);
        drop(task);
        assert!(matches!(resources.acquire(key, run.clone(), runtime.cancellation(&run).unwrap()).await,
            Err(oxideterm_ai::agent::AgentError::ResourceUnresolved)));
    }

    #[tokio::test]
    async fn user_question_resumes_the_same_responses_call_with_the_answer() {
        let (url, server) = model_server(vec![
            (200, completed(json!([call("mixed-question", "ask_user", json!({"question":"Which environment?"})),
                call("premature-command","run_command",json!({"command":"exit 0"}))]))),
            (200, completed(json!([call("question-wire", "ask_user", json!({"question":"Which environment?","options":["Staging","Production"]}))]))),
            (200, completed(json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Using staging."}]}]))),
        ]).await;
        let mut config = crate::workspace::ai_state::entity_tests::queued_turn("task").config;
        config.api_protocol = oxideterm_ai::AiApiProtocol::Responses;
        config.base_url = url;
        config.tool_policy.enabled = true;
        config.tools = oxideterm_ai::agent::agent_tool_definitions(false)
            .into_iter()
            .filter(|tool| tool.name == "ask_user")
            .collect();
        config.tools.extend(
            oxideterm_ai::orchestrator_tool_definitions()
                .into_iter()
                .filter(|tool| tool.name == "run_command"),
        );
        let runtime = oxideterm_ai::agent::AgentRuntime::new(1);
        let run = runtime.create_group(
            "conversation".into(),
            AgentModel {
                provider_id: "provider".into(),
                model: config.model.clone(),
            },
            oxideterm_ai::agent::AgentScope::default(),
            8,
        );
        let execution = AgentExecution::new(
            runtime,
            run,
            oxideterm_ai::agent::AgentResourceCoordinator::default(),
        )
        .unwrap();
        let (sender, receiver) = AiStreamDeliverySender::channel();
        let host = std::thread::spawn(move || {
            let mut asked = false;
            let mut final_text = String::new();
            while let Ok(delivery) = receiver.recv_timeout(Duration::from_secs(5)) {
                match delivery.event {
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
            .find(|item| item["type"] == "function_call_output" && item["call_id"] == "question-wire")
            .unwrap();
        assert_eq!(output["call_id"], "question-wire");
        let answer: Value = serde_json::from_str(output["output"].as_str().unwrap()).unwrap();
        let response: Value = serde_json::from_str(answer["output"].as_str().unwrap()).unwrap();
        assert_eq!(response["answer"], "Staging, read only");
    }

    #[tokio::test]
    async fn user_question_wait_is_cancelled_by_steering_or_stopping() {
        for steer in [true, false] {
            let runtime = oxideterm_ai::agent::AgentRuntime::new(1);
            let run = runtime.create_group(
                "conversation".into(),
                AgentModel {
                    provider_id: "provider".into(),
                    model: "model".into(),
                },
                oxideterm_ai::agent::AgentScope::default(),
                8,
            );
            let dispatch = runtime.dispatch(&run).unwrap();
            let agent = AgentExecution::new(
                runtime.clone(),
                run.clone(),
                oxideterm_ai::agent::AgentResourceCoordinator::default(),
            )
            .unwrap();
            let (ui, receiver) = AiStreamDeliverySender::channel();
            let (question_tx, question_rx) = tokio::sync::oneshot::channel();
            let host = std::thread::spawn(move || {
                while let Ok(delivery) = receiver.recv_timeout(Duration::from_secs(5)) {
                    if let AiStreamDeliveryEvent::UserQuestionRequested { sender, .. } = delivery.event
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

    #[test]
    fn command_observation_deadline_is_bounded_and_can_be_renewed() {
        let start = std::time::Instant::now();
        let wait = AiTerminalCommandWait::new(start);
        for seconds in [31, 90, 1799] {
            assert!(!wait.expired(start + Duration::from_secs(seconds)));
        }
        assert!(wait.expired(start + Duration::from_secs(1800)));
        let renewed = AiTerminalCommandWait::new(start + Duration::from_secs(1800));
        assert!(!renewed.expired(start + Duration::from_secs(1801)));
        assert!(renewed.expired(start + Duration::from_secs(3600)));
        let capped = AiTerminalCommandWait::with_timeout(start, Duration::from_secs(7200));
        assert!(capped.expired(start + Duration::from_secs(1800)));
    }

    #[tokio::test]
    async fn agent_wait_preserves_user_direction_as_user_history() {
        let runtime = oxideterm_ai::agent::AgentRuntime::new(2);
        let run = runtime.create_group(
            "conversation".into(),
            oxideterm_ai::agent::AgentModel {
                provider_id: "provider".into(),
                model: "model".into(),
            },
            oxideterm_ai::agent::AgentScope::default(),
            8,
        );
        let mut execution = Some(
            AgentExecution::new(
                runtime.clone(),
                run.clone(),
                oxideterm_ai::agent::AgentResourceCoordinator::default(),
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
                .map(|message| (message.role.clone(), message.content.as_str()))
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
        let runtime = oxideterm_ai::agent::AgentRuntime::new(2);
        let run = runtime.create_group(
            "conversation".into(),
            oxideterm_ai::agent::AgentModel {
                provider_id: "provider".into(),
                model: "model".into(),
            },
            oxideterm_ai::agent::AgentScope::default(),
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
        let mut config = crate::workspace::ai_state::entity_tests::queued_turn("task").config;
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
        let mut config = crate::workspace::ai_state::entity_tests::queued_turn("task").config;
        config.api_protocol = oxideterm_ai::AiApiProtocol::Responses;
        config.base_url = url;
        config.tool_policy.enabled = true;
        config.tools = oxideterm_ai::orchestrator_tool_definitions()
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
            let mut config = crate::workspace::ai_state::entity_tests::queued_turn("task").config;
            config.api_protocol = oxideterm_ai::AiApiProtocol::Responses;
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
            let mut checkpoint = oxideterm_ai::agent::AgentCheckpoint::new("Fix deployment");
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

    #[test]
    fn summary_fragments_preserve_unicode_and_respect_request_budget() {
        let evidence = "配置端口9000，保留数据库。🦀\n".repeat(300);
        let mut remaining = evidence.as_str();
        let mut reconstructed = String::new();
        while !remaining.is_empty() {
            let end = summary_chunk_end(remaining, 97);
            assert!(end > 0, "a fragment must make progress");
            let fragment = &remaining[..end];
            assert!(ai_estimated_tokens(fragment) <= 97);
            reconstructed.push_str(fragment);
            remaining = &remaining[end..];
        }
        assert_eq!(reconstructed, evidence);
        assert_eq!(summary_chunk_end("配置", 0), 0);
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
        let mut config = crate::workspace::ai_state::entity_tests::queued_turn("task").config;
        config.api_protocol = oxideterm_ai::AiApiProtocol::Responses;
        config.base_url = url;
        config.tool_policy.enabled = true;
        config.tools = oxideterm_ai::orchestrator_tool_definitions()
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

    #[tokio::test]
    #[ignore = "manual comparison of serial and concurrent I/O dispatch"]
    async fn benchmark_read_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let backend = services(temp.path());
        let mut config = crate::workspace::ai_state::entity_tests::queued_turn("benchmark").config;
        config.tool_policy.enabled = true;
        let available = std::collections::HashSet::from(["read_resource".to_string()]);
        let calls=(0..4).map(|index|AiToolCall{id:format!("call-{index}"),name:"read_resource".into(),arguments:json!({"resource":"file","path":format!("/file-{index}"),"handle_id":"rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}).to_string()}).collect::<Vec<_>>();
        for parallel in [false, true] {
            let mut samples = Vec::new();
            for _ in 0..3 {
                let (sender, receiver) = AiStreamDeliverySender::channel();
                let host = std::thread::spawn(move || {
                    let mut workers = Vec::new();
                    while workers.len() < 4 {
                        match receiver.recv_timeout(Duration::from_secs(5)).unwrap().event {
                            AiStreamDeliveryEvent::ToolPreflightRequested { sender, .. } => {
                                sender.send(None).unwrap();
                            }
                            AiStreamDeliveryEvent::ToolExecutionRequested {
                                sender,
                                tool_call_id,
                                name,
                                ..
                            } => {
                                workers.push(std::thread::spawn(move || {
                                    std::thread::sleep(Duration::from_millis(25));
                                    sender
                                        .send(executed(tool_call_id, name, "read complete"))
                                        .unwrap();
                                }));
                            }
                            _ => {}
                        }
                    }
                    for worker in workers {
                        worker.join().unwrap();
                    }
                    // Keep the status receiver alive until all result senders have finished.
                    while receiver.recv_timeout(Duration::from_millis(10)).is_ok() {}
                });
                let session = ToolSessionId::new();
                let started = std::time::Instant::now();
                let results = if parallel {
                    futures_util::future::join_all(calls.iter().cloned().map(|call| {
                        let config = &config;
                        let backend = &backend;
                        let available = &available;
                        let sender = &sender;
                        let session = &session;
                        async move {
                            execute_ai_round_call(
                                call,
                                config,
                                backend,
                                available,
                                sender,
                                1,
                                session,
                                "conversation",
                                "assistant",
                                &mut None,
                                false,
                                None,
                            )
                            .await
                        }
                    }))
                    .await
                } else {
                    let mut results = Vec::new();
                    for call in &calls {
                        results.push(
                            execute_ai_round_call(
                                call.clone(),
                                &config,
                                &backend,
                                &available,
                                &sender,
                                1,
                                &session,
                                "conversation",
                                "assistant",
                                &mut None,
                                false,
                                None,
                            )
                            .await,
                        );
                    }
                    results
                };
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(
                    results
                        .into_iter()
                        .map(|result| result.unwrap().output)
                        .collect::<Vec<_>>(),
                    vec!["read complete"; 4]
                );
                drop(sender);
                host.join().unwrap();
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "{}: {:.3} ms median for four 25 ms reads",
                if parallel { "parallel" } else { "serial" },
                samples[1]
            );
        }
    }
}
