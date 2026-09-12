use super::*;
use crate::streaming::{ResponsesStream, responses_body};
use serde_json::json;

fn config() -> AiChatStreamConfig {
    let mut config = test_stream_config("openai");
    config.api_protocol = AiApiProtocol::Responses;
    config
}

fn completed(output: Value) -> Value {
    json!({"type":"response.completed", "response":{"status":"completed","output":output,"usage":{"input_tokens":12,"output_tokens":9}}})
}

fn function(id: &str, name: &str, arguments: &str) -> Value {
    json!({"type":"function_call","id":format!("fc_{id}"),"call_id":id,"name":name,"arguments":arguments,"status":"completed"})
}

#[test]
fn responses_stream_interleaves_items_without_duplicate_text_or_early_execution() {
    let mut parser = ResponsesStream::default();
    let mut events = Vec::new();
    for event in [
        json!({"type":"response.output_item.added","output_index":1,"item":function("wire-a","first","")}),
        json!({"type":"response.output_item.added","output_index":2,"item":function("wire-b","second","")}),
        json!({"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"思考"}),
        json!({"type":"response.function_call_arguments.delta","output_index":2,"delta":"{\"n\":"}),
        json!({"type":"response.function_call_arguments.delta","output_index":1,"delta":"{}"}),
        json!({"type":"response.function_call_arguments.delta","output_index":2,"delta":"2}"}),
        json!({"type":"response.output_text.delta","output_index":3,"content_index":0,"delta":"你好"}),
        json!({"type":"response.output_item.done","output_index":1,"item":function("wire-a","first","{}")}),
    ] {
        events.extend(parser.event(event, "responses:scope").unwrap());
    }
    assert_eq!(
        events,
        vec![
            AiStreamEvent::Thinking("思考".into()),
            AiStreamEvent::ToolCall {
                id: "wire-b".into(),
                name: "second".into(),
                arguments: "{\"n\":".into()
            },
            AiStreamEvent::ToolCall {
                id: "wire-a".into(),
                name: "first".into(),
                arguments: "{}".into()
            },
            AiStreamEvent::ToolCall {
                id: "wire-b".into(),
                name: "second".into(),
                arguments: "{\"n\":2}".into()
            },
            AiStreamEvent::Content("你好".into()),
        ]
    );
    let output = json!([
        {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"思考"}],"encrypted_content":"opaque-content"},
        function("wire-a","first","{}"),function("wire-b","second","{\"n\":2}"),
        {"type":"message","id":"msg_1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"你好世界","annotations":[]}]}
    ]);
    let final_event = completed(output.clone());
    assert_eq!(
        parser
            .event(final_event.clone(), "responses:scope")
            .unwrap(),
        vec![
            AiStreamEvent::ToolCallComplete {
                id: "wire-a".into(),
                name: "first".into(),
                arguments: "{}".into()
            },
            AiStreamEvent::ToolCallComplete {
                id: "wire-b".into(),
                name: "second".into(),
                arguments: "{\"n\":2}".into()
            },
            AiStreamEvent::Content("世界".into()),
            AiStreamEvent::ProviderResponsePart {
                provider_type: "responses:scope".into(),
                part: json!({"output":output})
            },
            AiStreamEvent::Usage {
                input_tokens: Some(12),
                output_tokens: Some(9)
            },
            AiStreamEvent::Done,
        ]
    );
    assert_eq!(
        parser.event(final_event, "responses:scope").unwrap(),
        vec![]
    );
}

#[test]
fn responses_replay_restores_wire_ids_and_survives_persistence_and_scope_changes() {
    for provider_type in ["openai", "xai"] {
        let mut config = config();
        config.provider_type = provider_type.into();
        if provider_type == "xai" {
            config.model = "grok-4.6".into();
        }
        let scope = config.response_state_key();
        let output = json!([
            {"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"gAAAA-opaque"},
            function("wire-a","first","{}")
        ]);
        let ids = HashMap::from([("wire-a".into(), "local-unique".into())]);
        let mut result = chat_message("result", AiChatRole::Tool, "done");
        result.tool_call_id = Some("local-unique".into());
        let mut assistant = chat_message("assistant", AiChatRole::Assistant, "All done");
        let live = responses_round_state(&[json!({"output":output})], &ids, &[]).unwrap();
        set_ai_provider_parts(&mut assistant, &scope, vec![live]);
        assert_eq!(
            responses_body(&config, &[assistant.clone(), result.clone()])["input"],
            json!([
                {"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"gAAAA-opaque"},
                function("wire-a","first","{}"),
                {"type":"function_call_output","call_id":"wire-a","output":"done"}
            ])
        );
        assistant.turn = None;
        append_responses_round(
            &mut assistant,
            &scope,
            responses_round_state(&[json!({"output":output})], &ids, &[result]).unwrap(),
        );
        append_responses_round(
            &mut assistant,
            &scope,
            json!({"output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"All done","annotations":[]}]}],"results":[],"callIds":{}}),
        );
        let mut branch = chat_message("earlier-branch", AiChatRole::Assistant, "Earlier branch");
        let branch_output = json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Earlier branch","annotations":[]}]}]);
        append_responses_round(
            &mut branch,
            &scope,
            json!({"output":branch_output,"results":[],"callIds":{}}),
        );
        assistant.branches = Some(AiMessageBranches {
            total: 2,
            active_index: 1,
            tails: HashMap::from([(0, vec![branch])]),
        });
        let dir = tempfile::tempdir().unwrap();
        let store = AiChatPersistenceStore::new(dir.path().join("chat.redb"));
        let mut state = AiChatState::default();
        let id = state.create_conversation("conversation".into(), None, 1, None);
        state.add_message(&id, assistant);
        store.save_state(state).unwrap();
        let mut history = store.load_conversation(&id).unwrap().unwrap().messages;
        let branch = &history[0].branches.as_ref().unwrap().tails[&0];
        assert_eq!(responses_body(&config, branch)["input"], branch_output);
        normalize_ai_stream_history_for_provider(&mut history);
        history.push(chat_message("next", AiChatRole::User, "continue"));
        assert_eq!(
            responses_body(&config, &history)["input"],
            json!([
                {"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"gAAAA-opaque"},
                function("wire-a","first","{}"),
                {"type":"function_call_output","call_id":"wire-a","output":"done"},
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"All done","annotations":[]}]},
                {"role":"user","content":"continue"}
            ])
        );
        for field in ["provider", "model", "endpoint"] {
            let mut changed = config.clone();
            match field {
                "provider" => changed.provider_id = Some("other".into()),
                "model" => changed.model = "other".into(),
                _ => changed.base_url = "https://other.test/v1".into(),
            }
            let mut projected = history.clone();
            scope_responses_history(&mut projected, &changed);
            assert_eq!(
                ai_prompt_token_breakdown(&projected, &[], "openai", 0).tool_results,
                0,
                "{field}"
            );
            assert_eq!(
                responses_body(&changed, &projected)["input"],
                json!([
                    {"role":"assistant","content":"All done"},{"role":"user","content":"continue"}
                ]),
                "{field}"
            );
        }
        let mut legacy = config.clone();
        legacy.api_protocol = AiApiProtocol::ChatCompletions;
        scope_responses_history(&mut history, &legacy);
        assert_eq!(
            ai_prompt_token_breakdown(&history, &[], "openai", 0).tool_results,
            0
        );
        assert_eq!(
            Value::Array(openai_chat_messages(&legacy, &history)),
            json!([
                {"role":"assistant","content":"All done"},{"role":"user","content":"continue"}
            ])
        );
    }
}

#[test]
fn responses_request_maps_tools_limits_and_legacy_provider_default() {
    let mut config = config();
    config.max_response_tokens = Some(2048);
    config.reasoning_effort = Some("high".into());
    config.tools = vec![AiToolDefinition {
        name: "lookup".into(),
        description: "Find entry".into(),
        parameters: json!({"type":"object","properties":{"q":{"type":"string"}}}),
    }];
    config.tool_choice = AiToolChoice::Named("lookup".into());
    assert_eq!(
        responses_body(
            &config,
            &[
                chat_message("sys", AiChatRole::System, "instructions"),
                chat_message("user", AiChatRole::User, "hello")
            ]
        ),
        json!({
            "model":"model","input":[{"role":"system","content":"instructions"},{"role":"user","content":"hello"}],"stream":true,"store":false,
            "include":["reasoning.encrypted_content"],"max_output_tokens":2048,"reasoning":{"effort":"high","summary":"auto"},
            "tools":[{"type":"function","name":"lookup","description":"Find entry","parameters":{"type":"object","properties":{"q":{"type":"string"}}},"strict":false}],
            "tool_choice":{"type":"function","name":"lookup"}
        })
    );
    config.provider_type = "openai_compatible".into();
    config.model = "gpt-5-pro".into();
    assert_eq!(responses_body(&config, &[])["reasoning"], json!({"effort":"high","summary":"auto"}));
    config.model = "gpt-4o".into();
    config.reasoning_effort = Some("auto".into());
    assert_eq!(responses_body(&config, &[]).get("reasoning"), None);
    let legacy = json!({"id":"provider","type":"openai","baseUrl":"https://example.test"});
    assert_eq!(
        provider_views(&[legacy.clone()])[0].api_protocol,
        AiApiProtocol::ChatCompletions
    );
    let mut current = legacy;
    current["apiProtocol"] = json!("responses");
    assert_eq!(
        provider_views(&[current])[0].api_protocol,
        AiApiProtocol::Responses
    );
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, Value) {
    use tokio::io::AsyncReadExt;
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        headers.push(stream.read_u8().await.unwrap());
    }
    let headers = String::from_utf8(headers).unwrap();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|s| s.trim().parse().unwrap())
        })
        .unwrap();
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    (headers, serde_json::from_slice(&body).unwrap())
}

async fn mock_response(
    data: String,
    content_type: &'static str,
) -> (String, tokio::task::JoinHandle<(String, Value)>) {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_request(&mut stream).await;
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        // HTTP chunk boundaries split both SSE fields and multibyte text.
        for chunk in data.as_bytes().chunks(7) {
            if stream
                .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                .await
                .is_err()
            {
                break;
            }
            if stream.write_all(chunk).await.is_err() {
                break;
            }
            if stream.write_all(b"\r\n").await.is_err() {
                break;
            }
        }
        let _ = stream.write_all(b"0\r\n\r\n").await;
        request
    });
    (url, task)
}

#[tokio::test]
async fn responses_http_stream_and_json_use_selected_endpoint_and_preserve_refusal() {
    let output = json!([{"type":"message","role":"assistant","status":"completed","content":[{"type":"refusal","refusal":"无法执行"}]}]);
    for content_type in ["text/event-stream", "application/json"] {
        let event = completed(output.clone());
        let data = if content_type == "application/json" {
            event["response"].to_string()
        } else {
            format!(
                "event: response.created\ndata: {{\"type\":\"response.created\"}}\n\nevent: response.completed\ndata: {event}\n\n"
            )
        };
        let (url, server) = mock_response(data, content_type).await;
        let mut config = config();
        config.base_url = url;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        stream_chat_completion(
            config,
            vec![chat_message("u", AiChatRole::User, "hello")],
            tx,
        )
        .await;
        let mut visible = Vec::new();
        while let Some(event) = rx.recv().await {
            if !matches!(event, AiStreamEvent::ProviderResponsePart { .. }) {
                visible.push(event);
            }
        }
        assert_eq!(
            visible,
            vec![
                AiStreamEvent::Content("无法执行".into()),
                AiStreamEvent::Usage {
                    input_tokens: Some(12),
                    output_tokens: Some(9)
                },
                AiStreamEvent::Done
            ]
        );
        let (headers, body) = server.await.unwrap();
        assert_eq!(headers.lines().next(), Some("POST /v1/responses HTTP/1.1"));
        assert_eq!(
            body,
            json!({"model":"model","input":[{"role":"user","content":"hello"}],"stream":true,"store":false,"include":["reasoning.encrypted_content"]})
        );
    }
}

#[tokio::test]
async fn responses_failure_never_releases_tools_or_marks_partial_output_complete() {
    for terminal in [
        json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"}}}).to_string(),
        json!({"type":"response.failed","response":{"error":{"message":"secret-password=do-not-display"}}}).to_string(),
        "{invalid".into(),
        String::new(),
    ] {
        let mut data=format!("data: {}\n\ndata: {}\n\n",json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"partial"}),json!({"type":"response.output_item.done","output_index":1,"item":function("wire-a","execute","{}")}));
        if !terminal.is_empty(){data.push_str(&format!("data: {terminal}\n\n"));}
        let (url,server)=mock_response(data,"text/event-stream").await;
        let mut config=config();config.base_url=url;
        let (tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
        stream_chat_completion(config,vec![],tx).await;
        assert_eq!(rx.recv().await,Some(AiStreamEvent::Content("partial".into())));
        let error=rx.recv().await.unwrap();
        assert!(matches!(&error,AiStreamEvent::Error(message) if !message.contains("do-not-display")),"{error:?}");
        assert_eq!(rx.recv().await,None,"no complete calls or Done after failure");
        server.await.unwrap();
    }
}

#[tokio::test]
async fn responses_cancel_drops_http_stream_before_a_followup_request() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = config();
    config.base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_request(&mut stream).await;
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"delta\":\"started\"}\n\n").await.unwrap();
        let mut byte = [0];
        assert_eq!(
            stream.read(&mut byte).await.unwrap(),
            0,
            "cancel must close the active request"
        );
        let (mut next, _) = listener.accept().await.unwrap();
        let (_, body) = read_request(&mut next).await;
        assert_eq!(
            body["input"],
            json!([{"role":"user","content":"follow up"}])
        );
        let response = completed(
            json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"fresh","annotations":[]}]}]),
        );
        let data = format!("data: {response}\n\n");
        next.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{data}",data.len()).as_bytes()).await.unwrap();
    });
    let mut request = crate::agent::AgentModelRequest::start(config.clone(), vec![]);
    assert_eq!(
        request.next_event().await,
        Some(AiStreamEvent::Content("started".into()))
    );
    drop(request);
    let mut next = crate::agent::AgentModelRequest::start(
        config,
        vec![chat_message("u", AiChatRole::User, "follow up")],
    );
    let work = async {
        assert_eq!(
            next.next_event().await,
            Some(AiStreamEvent::Content("fresh".into()))
        );
        while let Some(event) = next.next_event().await {
            assert!(!matches!(event, AiStreamEvent::Error(_)));
        }
        server.await.unwrap();
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), work)
        .await
        .unwrap();
}

#[test]
fn responses_history_trimming_keeps_a_complete_live_tool_round() {
    let mut assistant = chat_message("round", AiChatRole::Assistant, &"large".repeat(100));
    assistant.tool_calls = vec![
        json!({"id":"a","name":"first","arguments":"{}"}),
        json!({"id":"b","name":"second","arguments":"{}"}),
    ];
    let mut a = chat_message("result-a", AiChatRole::Tool, "first result");
    a.tool_call_id = Some("a".into());
    let mut b = chat_message("result-b", AiChatRole::Tool, "second result");
    b.tool_call_id = Some("b".into());
    let mut history = vec![
        chat_message("old", AiChatRole::User, "old request"),
        assistant,
        a,
        b,
    ];
    trim_ai_stream_history_to_request_budget(&mut history, &[], "openai", 10, 100);
    assert_eq!(
        history
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["round", "result-a", "result-b"]
    );
    assert_eq!(
        responses_body(&config(), &history)["input"],
        json!([
            {"role":"assistant","content":"large".repeat(100)},
            {"type":"function_call","call_id":"a","name":"first","arguments":"{}"},
            {"type":"function_call","call_id":"b","name":"second","arguments":"{}"},
            {"type":"function_call_output","call_id":"a","output":"first result"},
            {"type":"function_call_output","call_id":"b","output":"second result"}
        ])
    );
}

#[tokio::test]
async fn responses_incomplete_json_keeps_text_without_executing_finished_calls() {
    let response = json!({"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[
        {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Partial answer"}]},
        function("wire-a","execute","{}")
    ]});
    let (url, server) = mock_response(response.to_string(), "application/json").await;
    let mut config = config();
    config.base_url = url;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    stream_chat_completion(config, vec![], tx).await;
    assert_eq!(
        rx.recv().await,
        Some(AiStreamEvent::Content("Partial answer".into()))
    );
    assert_eq!(
        rx.recv().await,
        Some(AiStreamEvent::Error("responses_incomplete_limit".into()))
    );
    assert_eq!(rx.recv().await, None);
    server.await.unwrap();
}

#[test]
fn responses_durable_round_removes_execution_payloads_but_retains_opaque_reasoning() {
    let mut assistant = chat_message("a", AiChatRole::Assistant, "completed");
    append_responses_round(
        &mut assistant,
        "responses:scope",
        json!({
            "output":[
                {"type":"reasoning","summary":[],"encrypted_content":"gAAAA-opaque"},
                function("wire-a","run_command",r#"{"command":"echo API_KEY=supersecret123456","node_id":"node-a","path":"/tmp"}"#)
            ],
            "callIds":{"local":"wire-a"},
            "results":[{"type":"function_call_output","call_id":"wire-a","output":json!({"data":{"exitCode":0,"nodeId":"node-a"},"password":"supersecret123456"}).to_string()}]
        }),
    );
    let round = &ai_provider_parts(&assistant, "responses:scope").unwrap()[0];
    assert_eq!(
        round["output"][0],
        json!({"type":"reasoning","summary":[],"encrypted_content":"gAAAA-opaque"})
    );
    assert_eq!(
        serde_json::from_str::<Value>(round["output"][1]["arguments"].as_str().unwrap()).unwrap(),
        json!({"path":"/tmp"})
    );
    let output: Value =
        serde_json::from_str(round["results"][0]["output"].as_str().unwrap()).unwrap();
    assert_eq!(output["data"], json!({"exitCode":0}));
    assert!(!round.to_string().contains("supersecret123456"));
}

#[tokio::test]
async fn xai_stream_uses_responses_and_preserves_reasoning_and_function_calls() {
    let output = json!([
        {"type":"reasoning","id":"reasoning","summary":[{"type":"summary_text","text":"检查配置"}],"encrypted_content":"opaque-xai-state"},
        function("xai-call", "inspect_config", "{}")
    ]);
    let data = format!(
        "data: {}\n\ndata: {}\n\n",
        json!({"type":"response.reasoning_text.delta","output_index":0,"content_index":0,"delta":"检查配置"}),
        completed(output.clone())
    );
    let (url, server) = mock_response(data, "text/event-stream").await;
    let mut config = config();
    config.provider_type = "xai".into();
    config.model = "grok-4.6".into();
    config.base_url = url;
    config.api_key = Some(SharedAiProviderKey::new(zeroize::Zeroizing::new(
        "fixture-key".into(),
    )));
    config.reasoning_effort = Some("xhigh".into());
    config.tools = vec![AiToolDefinition {
        name: "inspect_config".into(),
        description: "Inspect configuration".into(),
        parameters: json!({"type":"object","properties":{}}),
    }];
    config.tool_choice = AiToolChoice::Named("inspect_config".into());
    let scope = config.response_state_key();
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    stream_chat_completion(
        config,
        vec![chat_message(
            "user",
            AiChatRole::User,
            "Inspect configuration",
        )],
        sender,
    )
    .await;
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    assert_eq!(
        events,
        vec![
            AiStreamEvent::Thinking("检查配置".into()),
            AiStreamEvent::ToolCallComplete {
                id: "xai-call".into(),
                name: "inspect_config".into(),
                arguments: "{}".into()
            },
            AiStreamEvent::ProviderResponsePart {
                provider_type: scope,
                part: json!({"output":output})
            },
            AiStreamEvent::Usage {
                input_tokens: Some(12),
                output_tokens: Some(9)
            },
            AiStreamEvent::Done,
        ]
    );
    let (headers, body) = server.await.unwrap();
    assert!(headers.starts_with("POST /v1/responses HTTP/1.1"));
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-key")
    );
    assert_eq!(body["reasoning"], json!({"effort":"xhigh"}));
    assert_eq!(body["store"], false);
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
    assert_eq!(
        body["tool_choice"],
        json!({"type":"function","name":"inspect_config"})
    );
    assert_eq!(body["tools"][0]["strict"], false);
}

#[test]
fn xai_reasoning_settings_only_emit_documented_effort_levels() {
    let mut config = config();
    config.provider_type = "xai".into();
    for (model, requested, expected) in [
        ("grok-4.6", "auto", None),
        ("grok-4.6", "none", None),
        ("grok-4.6", "xhigh", Some("xhigh")),
        ("grok-4.5", "high", Some("high")),
        ("grok-4.5", "xhigh", None),
        ("grok-4", "high", None),
    ] {
        config.model = model.into();
        config.reasoning_effort = Some(requested.into());
        assert_eq!(
            responses_body(&config, &[]).get("reasoning").cloned(),
            expected.map(|effort| json!({"effort":effort})),
            "{model}/{requested}"
        );
    }
}
