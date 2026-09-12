//! Run the same synthetic histories against legacy and current storage.
use oxideterm_ai::{
    AiChatMessage, AiChatPersistenceStore, AiChatRole, AiChatState, ConversationStore,
    HistoryMutation,
};
use std::time::Instant;

fn peak_rss_bytes() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0;
    }
    let value = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        value
    } else {
        value.saturating_mul(1024)
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--stream") {
        return stream_benchmark();
    }
    let count = args
        .windows(2)
        .find(|args| args[0] == "--count")
        .map(|args| args[1].parse::<usize>())
        .transpose()?;
    if count.is_none() {
        for count in [1_000, 10_000, 100_000] {
            let status = std::process::Command::new(std::env::current_exe()?)
                .args(&args[1..])
                .args(["--count", &count.to_string()])
                .status()?;
            anyhow::ensure!(status.success(), "benchmark subprocess failed");
        }
        return Ok(());
    }
    let count = count.unwrap();
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("chat.redb");
    let v4 = std::env::args().any(|arg| arg == "--v4");
    let mut state = AiChatState::default();
    let id = state.create_conversation("benchmark".into(), None, 1, None);
    let conversation = state.active_conversation_mut().unwrap();
    conversation.messages = (0..count)
        .map(|index| {
            serde_json::from_value::<AiChatMessage>(serde_json::json!({
                "id": format!("message-{index}"), "role": AiChatRole::User,
                "content": format!("{index}: {}", "a".repeat(1024)), "timestamp_ms": index
            }))
            .unwrap()
        })
        .collect();
    conversation.message_count = count;
    conversation.turn_count = count;
    if v4 {
        let store = ConversationStore::open(&path)?;
        let mut conversation = state.conversations.remove(0);
        let messages = std::mem::take(&mut conversation.messages);
        let started = Instant::now();
        store.apply(vec![HistoryMutation::Create {
            conversation,
            revision: 1,
        }])?;
        let mut revision = 1;
        let import_commits =
            store.import_messages(&id, "main", messages.iter().cloned().map(Ok), &mut revision)?;
        let initial_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let descriptions = store.message_page(&id, "main", None, 50)?;
        let descriptions_ms = started.elapsed().as_secs_f64() * 1000.0;
        let cached_after_descriptions = store.cached_bytes();
        let started = Instant::now();
        let last = descriptions.messages.last().unwrap();
        let visible = store.message_view(&id, &last.storage_id, last.revision, None)?;
        let visible_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(visible.message.id, format!("message-{}", count - 1));
        let started = Instant::now();
        let older = store.message_page(&id, "main", descriptions.before.as_ref(), 50)?;
        let older_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            older.messages.last().unwrap().id,
            format!("message-{}", count - 51)
        );
        let started = Instant::now();
        let page = store.page(&id, "main", None, 50)?;
        let load_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            page.messages.first().unwrap().id,
            format!("message-{}", count - 50)
        );
        assert_eq!(
            page.messages.last().unwrap().id,
            format!("message-{}", count - 1)
        );
        let mut message = messages.last().unwrap().clone();
        message.content.push_str(" updated");
        let started = Instant::now();
        store.apply(vec![HistoryMutation::PutMessage {
            conversation_id: id.clone(),
            branch_id: "main".into(),
            message,
            revision: revision + 1,
        }])?;
        println!(
            "{}",
            serde_json::json!({"backend":"v4","messages":count,"loaded_messages":50,
                "initial_ms":initial_ms,"load_ms":load_ms,"update_ms":started.elapsed().as_secs_f64()*1000.0,
                "database_bytes":std::fs::metadata(&path)?.len(),
                "descriptions_ms":descriptions_ms,"visible_body_ms":visible_ms,"older_descriptions_ms":older_ms,
                "cache_after_descriptions":cached_after_descriptions,"cache_bytes":store.cached_bytes(),
                "commits":import_commits+2,"process_peak_rss_bytes":peak_rss_bytes()})
        );
        return Ok(());
    }
    let store = AiChatPersistenceStore::try_new(&path)?;
    let started = Instant::now();
    store.save_state(state.clone())?;
    let initial_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let loaded = store.load_conversation(&id)?.unwrap();
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(loaded.messages.first().unwrap().id, "message-0");
    assert_eq!(
        loaded.messages.last().unwrap().id,
        format!("message-{}", count - 1)
    );
    state
        .active_conversation_mut()
        .unwrap()
        .messages
        .last_mut()
        .unwrap()
        .content
        .push_str(" updated");
    let started = Instant::now();
    store.save_state(state)?;
    println!(
        "{}",
        serde_json::json!({"backend":"legacy", "messages":count,
            "initial_ms":initial_ms,"load_ms":load_ms,"update_ms":started.elapsed().as_secs_f64()*1000.0,
            "database_bytes":std::fs::metadata(&path)?.len(),"commits":2,"process_peak_rss_bytes":peak_rss_bytes()})
    );
    Ok(())
}

fn stream_benchmark() -> anyhow::Result<()> {
    use oxideterm_ai::HistoryStreamSnapshot;
    let dir = tempfile::tempdir()?;
    let store = ConversationStore::open(dir.path().join("stream.redb"))?;
    let mut state = AiChatState::default();
    state.create_conversation("stream".into(), None, 1, None);
    let text = "Large streaming response 中文\n".repeat(40_000);
    let output = "Tool observation output\n".repeat(40_000);
    let mut message: AiChatMessage = serde_json::from_value(serde_json::json!({
        "id":"reply", "role":"assistant", "timestamp_ms":1,"is_streaming":true,"content":text,
        "turn":{"parts":[{"type":"text","text":text}]},
        "tool_calls":(0..8).map(|index| serde_json::json!({"id":format!("tool-{index}"),"name":"inspect","status":"running","elapsedMs":0,"result":{"stdout":output}})).collect::<Vec<_>>()
    }))?;
    store.apply(vec![
        HistoryMutation::Create {
            conversation: state.conversations.remove(0),
            revision: 1,
        },
        HistoryMutation::PutMessage {
            conversation_id: "stream".into(),
            branch_id: "main".into(),
            message: message.clone(),
            revision: 2,
        },
    ])?;
    let mut base = HistoryStreamSnapshot::capture(&message, 2);
    let mut latencies = Vec::new();
    let mut capture_latencies = Vec::new();
    let mut encoded_bytes = 0;
    let mut max_encoded_bytes = 0;
    for revision in 3..23 {
        message.tool_calls[0]["elapsedMs"] = (revision * 250).into();
        let capture_started = Instant::now();
        let source = message.clone();
        capture_latencies.push(capture_started.elapsed().as_secs_f64() * 1000.0);
        let started = Instant::now();
        let (delta, next) = base
            .message_delta(&source, revision)
            .ok_or_else(|| anyhow::anyhow!("stream delta was unavailable"))?;
        let operation = HistoryMutation::StreamText {
            conversation_id: "stream".into(),
            branch_id: "main".into(),
            message_id: "reply".into(),
            delta,
        };
        let bytes = rmp_serde::to_vec(&operation)?.len();
        encoded_bytes += bytes;
        max_encoded_bytes = max_encoded_bytes.max(bytes);
        store.apply(vec![operation])?;
        base = next;
        latencies.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let final_message = store.message_by_id("stream", "main", "reply")?;
    assert_eq!(final_message.tool_calls[0]["elapsedMs"], 5500);
    assert_eq!(final_message.tool_calls[7]["result"]["stdout"], output);
    latencies.sort_by(f64::total_cmp);
    capture_latencies.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"scenario":"large_tool_status", "commits":22,
        "runtime_text_payload_bytes":text.len()*2+output.len()*8,"encoded_update_bytes":encoded_bytes,"max_encoded_update_bytes":max_encoded_bytes,
        "snapshot_clone_p95_ms":capture_latencies[18],"update_p50_ms":latencies[10],"update_p95_ms":latencies[18],"update_max_ms":latencies[19],
        "process_peak_rss_bytes":peak_rss_bytes(),"cache_bytes":store.cached_bytes(),"database_bytes":std::fs::metadata(store.path())?.len()})
    );
    Ok(())
}
