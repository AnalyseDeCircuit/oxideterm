use super::*;
use crate::RuntimeOwnerKey;
use std::{
    collections::{BTreeSet, HashSet},
    time::Duration,
};

fn group(runtime: &AgentRuntime) -> AgentRunRef {
    runtime.create_group(
        "conversation".into(),
        AgentModel {
            provider_id: "provider".into(),
            model: "model".into(),
        },
        AgentScope {
            targets: HashSet::from([RuntimeOwnerKey::new()]),
            tools: BTreeSet::from(["observe_terminal".into()]),
        },
        2,
    )
}
fn child(runtime: &AgentRuntime, parent: &AgentRunRef) -> AgentRunRef {
    runtime
        .delegate(
            parent,
            AgentText::new("Inspect"),
            AgentText::new("Inspect the terminal"),
            runtime.snapshot(parent).unwrap().scope,
            None,
        )
        .unwrap()
}
fn result() -> AgentResult {
    AgentResult {
        summary: AgentText::new("Done"),
        evidence: Vec::new(),
        actions: Vec::new(),
        unfinished: Vec::new(),
        error_code: None,
    }
}

#[test]
fn usage_snapshots_replace_cumulative_updates_and_preserve_unknown_requests() {
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let first = runtime.begin_request(&parent).unwrap();
    runtime
        .record_usage(&parent, first, Some(20), Some(1))
        .unwrap();
    runtime.record_usage(&parent, first, None, Some(8)).unwrap();
    assert_eq!(
        runtime.snapshot(&parent).unwrap().usage,
        AgentUsage {
            requests: 1,
            input_tokens: Some(20),
            output_tokens: Some(8)
        }
    );
    runtime.begin_request(&parent).unwrap();
    assert_eq!(
        runtime.snapshot(&parent).unwrap().usage,
        AgentUsage {
            requests: 2,
            input_tokens: None,
            output_tokens: None
        }
    );
}

#[test]
fn completion_cannot_discard_an_accepted_supplement_or_unread_child_result() {
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let child = child(&runtime, &parent);
    runtime
        .send(
            &parent,
            &parent,
            AgentMessageKind::UserSupplement,
            AgentText::new("Also check disk space"),
        )
        .unwrap();
    runtime.complete(&child, result()).unwrap();
    assert_eq!(
        runtime.prepare_completion(&parent, 0),
        Err(AgentError::PendingMessages)
    );
    assert_eq!(runtime.drain_messages(&parent).unwrap().len(), 1);
    assert_eq!(
        runtime.prepare_completion(&parent, 0),
        Err(AgentError::PendingMessages)
    );
    let cursor = runtime
        .updates_since(&parent, 0)
        .unwrap()
        .last()
        .unwrap()
        .sequence;
    runtime.prepare_completion(&parent, cursor).unwrap();
    assert!(
        runtime
            .send(
                &parent,
                &parent,
                AgentMessageKind::UserSupplement,
                AgentText::new("late instruction")
            )
            .is_err()
    );
    assert!(!runtime.accepts_messages(&parent));
    runtime.complete(&parent, result()).unwrap();
    runtime.finish_group(&parent).unwrap();
}

#[test]
fn persisted_agents_are_descriptive_and_deleted_with_their_conversation() {
    let directory = tempfile::tempdir().unwrap();
    let store = crate::AiChatPersistenceStore::new(directory.path().join("agents.redb"));
    let mut state = crate::AiChatState::default();
    state.create_conversation("conversation".into(), Some("Parent".into()), 1, None);
    store.save_state(state.clone()).unwrap();
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let child = child(&runtime, &parent);
    let record = AgentRecord {
        parent_usage: AgentUsage::default(),
        created_at_ms: 2,
        snapshot: runtime.snapshot(&child).unwrap(),
        parent_message_id: "reply".into(),
        target_labels: vec![AgentText::new("server")],
        messages: vec![serde_json::from_value(serde_json::json!({
            "id": "child-answer", "role": "assistant", "content": "Visible answer", "timestamp_ms": 2,
            "thinking_content": "private reasoning",
            "turn": { "parts": [
                { "type": "thinking", "text": "private reasoning" },
                { "type": "text", "text": "Visible answer" }
            ] }
        })).unwrap()],
        communication: Vec::new(),
        revision: 1,
    };
    store.save_agent_records(vec![record.clone()]).unwrap();
    let summaries = store.load_agent_summaries("conversation").unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].snapshot.state, AgentState::Interrupted);
    assert!(summaries[0].snapshot.scope.targets.is_empty());
    assert_eq!(state.conversations.len(), 1);
    let loaded = store
        .load_agent_record("conversation", &child.run_id)
        .unwrap()
        .unwrap();
    let message = &loaded.messages[0];
    assert!(message.thinking_content.is_none());
    assert_eq!(
        message.turn.as_ref().unwrap()["parts"],
        serde_json::json!([
            { "type": "text", "text": "Visible answer" }
        ])
    );
    state.delete_conversation("conversation");
    store.save_state(state).unwrap();
    store.save_agent_records(vec![record]).unwrap();
    assert!(
        store
            .load_agent_record("conversation", &child.run_id)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .load_agent_summaries("conversation")
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn waiting_releases_capacity_and_model_stays_locked_until_followup() {
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let second = child(&runtime, &parent);
    let resources = AgentResourceCoordinator::default();
    let mut execution =
        AgentExecution::new(runtime.clone(), first.clone(), resources.clone()).unwrap();
    execution.ready().await.unwrap();
    runtime.lock_model(&first).unwrap();
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<()>();
    let waiting =
        tokio::spawn(async move { execution.wait(AgentState::AwaitingParent, reply_rx).await });
    let mut other = AgentExecution::new(runtime.clone(), second.clone(), resources).unwrap();
    tokio::time::timeout(Duration::from_secs(1), other.ready())
        .await
        .unwrap()
        .unwrap();
    runtime.set_state(&first, AgentState::Queued).unwrap();
    let alternate = AgentModel {
        provider_id: "configured".into(),
        model: "other".into(),
    };
    assert_eq!(
        runtime.change_queued_model(&first, alternate.clone()),
        Err(AgentError::InvalidState)
    );
    runtime.stop(&parent, &first).unwrap();
    assert!(waiting.await.unwrap().is_err());
    drop(reply_tx);
    let resumed = runtime.resume(&parent, &first).unwrap();
    runtime
        .change_queued_model(&resumed, alternate.clone())
        .unwrap();
    assert_eq!(runtime.lock_model(&resumed).unwrap(), alternate);
    assert_eq!(
        runtime.snapshot(&second).unwrap().state,
        AgentState::Running
    );
}

#[tokio::test]
async fn terminal_response_and_interactive_input_do_not_release_running_command() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let resources = AgentResourceCoordinator::default();
    let key = RuntimeOwnerKey::new();
    let lease = resources
        .acquire(
            key.clone(),
            first.clone(),
            runtime.cancellation(&first).unwrap(),
        )
        .await
        .unwrap();
    let command = AgentToolLease::new(resources.clone(), lease.clone());
    command.dispatched();
    command.monitor_command();
    command.finish_response(true);
    drop(AgentToolResponse(vec![command.clone()]));
    assert!(resources.owns(&lease));
    drop(AgentToolLease::borrow_command(
        resources.clone(),
        lease.clone(),
    ));
    assert!(resources.owns(&lease));
    let input = AgentToolLease::borrow_command(
        resources.clone(),
        resources.owned_by(&key, &first).unwrap(),
    );
    input.dispatched();
    input.finish_response(true);
    drop(AgentToolResponse(vec![input]));
    assert!(resources.owns(&lease));
    command.command_finished();
    assert!(!resources.has_owner(&key));
    assert!(!resources.is_blocked(&key));
}

#[tokio::test]
async fn dropped_mutation_blocks_reuse_but_undispatched_work_does_not() {
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let resources = AgentResourceCoordinator::default();
    let key = RuntimeOwnerKey::new();
    let lease = resources
        .acquire(
            key.clone(),
            parent.clone(),
            runtime.cancellation(&parent).unwrap(),
        )
        .await
        .unwrap();
    drop(AgentToolLease::new(resources.clone(), lease));
    let lease = resources
        .acquire(
            key.clone(),
            parent.clone(),
            runtime.cancellation(&parent).unwrap(),
        )
        .await
        .unwrap();
    let tool = AgentToolLease::new(resources.clone(), lease);
    tool.dispatched();
    drop(AgentToolResponse(vec![tool]));
    assert!(matches!(
        resources
            .acquire(key, parent.clone(), runtime.cancellation(&parent).unwrap())
            .await,
        Err(AgentError::ResourceUnresolved)
    ));
}

#[tokio::test]
async fn mailbox_isolated_and_resume_rejects_late_results() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let second = child(&runtime, &parent);
    assert_eq!(
        runtime.send(
            &first,
            &second,
            AgentMessageKind::Question,
            AgentText::new("cross talk")
        ),
        Err(AgentError::ParentOnly)
    );
    assert_eq!(
        runtime.delegate(
            &first,
            AgentText::default(),
            AgentText::default(),
            AgentScope::default(),
            None
        ),
        Err(AgentError::ParentOnly)
    );
    runtime
        .send(
            &first,
            &parent,
            AgentMessageKind::Question,
            AgentText::new("Which process?"),
        )
        .unwrap();
    let received = runtime.wait_messages(&parent).await.unwrap();
    assert_eq!(received.len(), 1);
    assert!(received[0].consumed);
    assert!(runtime.drain_messages(&parent).unwrap().is_empty());
    runtime.complete(&first, result()).unwrap();
    let resumed = runtime.resume(&parent, &first).unwrap();
    assert_ne!(resumed.run_id, first.run_id);
    assert_eq!(
        runtime.complete(&first, result()),
        Err(AgentError::StaleRun)
    );
    assert_eq!(runtime.finish_group(&parent), Err(AgentError::InvalidState));
}

#[tokio::test]
async fn concurrency_and_budget_are_shared_and_waiters_cancel() {
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let second = child(&runtime, &parent);
    let permit = runtime.acquire(&first).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), runtime.acquire(&second))
            .await
            .is_err()
    );
    runtime.take_round(&parent).unwrap();
    runtime.take_round(&first).unwrap();
    assert_eq!(
        runtime.take_round(&second),
        Err(AgentError::BudgetExhausted)
    );
    runtime.stop(&parent, &second).unwrap();
    assert!(matches!(
        runtime.acquire(&second).await,
        Err(AgentError::Cancelled)
    ));
    assert!(matches!(
        runtime.wait_messages(&second).await,
        Err(AgentError::Cancelled)
    ));
    drop(permit);
    assert_eq!(runtime.snapshot(&first).unwrap().state, AgentState::Running);
}

#[tokio::test]
async fn resource_timeout_retains_ownership_and_takeover_rejects_late_completion() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let second = child(&runtime, &parent);
    let resources = AgentResourceCoordinator::default();
    let resource = runtime
        .snapshot(&parent)
        .unwrap()
        .scope
        .targets
        .into_iter()
        .next()
        .unwrap();
    let lease = resources
        .acquire(
            resource.clone(),
            first.clone(),
            runtime.cancellation(&first).unwrap(),
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(10),
            resources.acquire(
                resource.clone(),
                second.clone(),
                runtime.cancellation(&second).unwrap()
            )
        )
        .await
        .is_err()
    );
    assert!(resources.owns(&lease));
    assert_eq!(resources.invalidate(&resource), Some(first));
    assert!(matches!(
        resources
            .acquire(
                resource.clone(),
                second.clone(),
                runtime.cancellation(&second).unwrap()
            )
            .await,
        Err(AgentError::ResourceUnresolved)
    ));
    resources.allow_new_requests(&resource);
    let next = resources
        .acquire(
            resource.clone(),
            second.clone(),
            runtime.cancellation(&second).unwrap(),
        )
        .await
        .unwrap();
    assert!(!resources.complete(&lease));
    assert!(resources.owns(&next));
    assert!(resources.complete(&next));
}

#[test]
fn delegated_scope_and_secret_boundaries() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let mut scope = runtime.snapshot(&parent).unwrap().scope;
    scope.targets.insert(RuntimeOwnerKey::new());
    assert_eq!(
        runtime.delegate(
            &parent,
            AgentText::default(),
            AgentText::default(),
            scope,
            None
        ),
        Err(AgentError::ScopeDenied)
    );
    let text = AgentText::new("Authorization: Bearer private-token-value");
    assert!(!format!("{text:?}").contains("private-token-value"));
    assert!(
        !serde_json::to_string(&text)
            .unwrap()
            .contains("private-token-value")
    );
    let first = child(&runtime, &parent);
    let saved = runtime.snapshot(&first).unwrap();
    let serialized = serde_json::to_string(&saved).unwrap();
    for target in &saved.scope.targets {
        assert!(!serialized.contains(target.as_str()));
    }
    let restored: AgentSnapshot = serde_json::from_str(&serialized).unwrap();
    assert_eq!(restored.state, AgentState::Interrupted);
    assert!(restored.scope.targets.is_empty());
    assert!(restored.scope.tools.is_empty());
}

#[tokio::test]
async fn parent_receives_all_completions_even_when_message_mailbox_is_full() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let second = child(&runtime, &parent);
    for _ in 0..super::runtime::MAX_MAILBOX_MESSAGES {
        runtime
            .send(
                &first,
                &parent,
                AgentMessageKind::Question,
                AgentText::new("Need a target"),
            )
            .unwrap();
    }
    assert_eq!(
        runtime.send(
            &first,
            &parent,
            AgentMessageKind::Question,
            AgentText::new("Overflow")
        ),
        Err(AgentError::MailboxFull)
    );
    let cursor = runtime
        .wait_updates(&parent, 0)
        .await
        .unwrap()
        .last()
        .unwrap()
        .sequence;
    runtime.complete(&first, result()).unwrap();
    runtime.complete(&second, result()).unwrap();
    let updates = runtime.wait_updates(&parent, cursor).await.unwrap();
    assert_eq!(
        updates
            .iter()
            .map(|update| &update.source)
            .collect::<Vec<_>>(),
        vec![&first, &second]
    );
    assert!(
        runtime
            .wait_updates(&parent, updates.last().unwrap().sequence)
            .await
            .unwrap()
            .is_empty()
    );
    runtime.finish_group(&parent).unwrap();
    assert_eq!(
        runtime.resume(&parent, &first),
        Err(AgentError::GroupFinished)
    );
}

#[tokio::test]
async fn progress_does_not_return_from_parent_wait_and_question_does() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let waiting = runtime.wait_updates(&parent, 0);
    tokio::pin!(waiting);
    assert!(futures_util::poll!(&mut waiting).is_pending());
    runtime
        .report_progress(&first, AgentText::new("Inspecting"))
        .unwrap();
    assert!(futures_util::poll!(&mut waiting).is_pending());
    runtime
        .send(
            &first,
            &parent,
            AgentMessageKind::Question,
            AgentText::new("Which service?"),
        )
        .unwrap();
    let updates = waiting.await.unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(
        updates[0].message.as_ref().unwrap().kind,
        AgentMessageKind::Question
    );
}

#[tokio::test]
async fn changing_concurrency_and_cancelling_a_queued_future_preserves_progress() {
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let second = child(&runtime, &parent);
    let third = child(&runtime, &parent);
    let permit = runtime.acquire(&first).await.unwrap();
    assert!(matches!(
        runtime.acquire(&first).await,
        Err(AgentError::InvalidState)
    ));
    {
        let queued = runtime.acquire(&second);
        tokio::pin!(queued);
        assert!(futures_util::poll!(&mut queued).is_pending());
    }
    let queued = runtime.acquire(&third);
    tokio::pin!(queued);
    assert!(futures_util::poll!(&mut queued).is_pending());
    runtime.set_concurrency(2);
    let third_permit = queued.await.unwrap();
    runtime.set_concurrency(1);
    drop(permit);
    let second_queued = runtime.acquire(&second);
    tokio::pin!(second_queued);
    assert!(futures_util::poll!(&mut second_queued).is_pending());
    drop(third_permit);
    let _second_permit = second_queued.await.unwrap();
}

#[tokio::test]
async fn takeover_rejects_previously_queued_commands_even_after_hand_back() {
    let runtime = AgentRuntime::new(1);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let second = child(&runtime, &parent);
    let resources = AgentResourceCoordinator::default();
    let resource = runtime
        .snapshot(&parent)
        .unwrap()
        .scope
        .targets
        .into_iter()
        .next()
        .unwrap();
    let _lease = resources
        .acquire(
            resource.clone(),
            first.clone(),
            runtime.cancellation(&first).unwrap(),
        )
        .await
        .unwrap();
    let queued = resources.acquire(
        resource.clone(),
        second.clone(),
        runtime.cancellation(&second).unwrap(),
    );
    tokio::pin!(queued);
    assert!(futures_util::poll!(&mut queued).is_pending());
    resources.invalidate(&resource);
    resources.allow_new_requests(&resource);
    assert_eq!(queued.await, Err(AgentError::ResourceUnresolved));
}

#[tokio::test]
async fn cancellation_is_scoped_and_disallows_more_work() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let first = child(&runtime, &parent);
    let unrelated_parent = runtime.create_group(
        "other-conversation".into(),
        runtime.snapshot(&parent).unwrap().model,
        AgentScope::default(),
        2,
    );
    assert_eq!(
        runtime.complete(&parent, result()),
        Err(AgentError::InvalidState)
    );
    runtime.cancel_group(&parent).unwrap();
    assert_eq!(runtime.take_round(&first), Err(AgentError::Cancelled));
    assert_eq!(
        runtime.take_final_summary(&parent),
        Err(AgentError::Cancelled)
    );
    assert_eq!(
        runtime.delegate(
            &parent,
            AgentText::default(),
            AgentText::default(),
            AgentScope::default(),
            None
        ),
        Err(AgentError::Cancelled)
    );
    assert!(runtime.wait_messages(&first).await.is_err());
    runtime.take_round(&unrelated_parent).unwrap();
    assert_eq!(
        runtime.snapshot(&unrelated_parent).unwrap().state,
        AgentState::Running
    );
    runtime.remove_conversation("conversation");
    assert!(runtime.snapshots("conversation").is_empty());
    assert_eq!(runtime.snapshots("other-conversation").len(), 1);
}

fn progress_message(id: &str, role: &str, content: &str) -> crate::AiChatMessage {
    serde_json::from_value(serde_json::json!({"id":id,"role":role,"content":content,"timestamp_ms":0})).unwrap()
}

fn progress_result(call: &crate::AiToolCall, success: bool, output: &str) -> crate::AiExecutedToolResult {
    crate::AiExecutedToolResult {
        tool_call_id:call.id.clone(), tool_name:call.name.clone(), success, output:output.into(),
        error:(!success).then(||output.into()), duration_ms:1,
        envelope:serde_json::json!({"ok":success,"summary":output,"error":if success {serde_json::Value::Null} else {serde_json::json!({"code":"failed","message":output})}}),
    }
}

#[test]
fn progress_guard_distinguishes_failures_unchanged_reads_and_legitimate_polling() {
    let mut guard=ProgressGuard::default();
    let call=crate::AiToolCall{id:"c".into(),name:"run_command".into(),arguments:r#"{"command":"missing"}"#.into()};
    let failure=progress_result(&call,false,"not found");
    assert_eq!((0..5).map(|_|guard.observe(&call,&failure)).collect::<Vec<_>>(),vec![false,false,true,false,false]);
    assert!(guard.stalled());
    guard.reset();
    let read=crate::AiToolCall{name:"read_resource".into(),arguments:r#"{"resource":"file","handle_id":"rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","path":"/tmp/a"}"#.into(),..call.clone()};
    let unchanged=progress_result(&read,true,"same contents");
    assert_eq!((0..5).map(|_|guard.observe(&read,&unchanged)).collect::<Vec<_>>(),vec![false,false,true,false,false]);
    assert!(!guard.stalled());
    guard.observe(&call,&progress_result(&call,true,"file updated"));
    assert!(!guard.observe(&read,&unchanged));
    let poll=crate::AiToolCall{name:"observe_terminal".into(),arguments:r#"{"handle_id":"rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.into(),..call.clone()};
    assert_eq!((0..8).map(|_|guard.observe(&poll,&progress_result(&poll,true,"running"))).collect::<Vec<_>>(),vec![false;8]);
    assert!(parallel_read_call(&read));
    assert!(parallel_read_call(&poll));
    assert!(!parallel_read_call(&call));
    assert!(!parallel_read_call(&crate::AiToolCall{name:"mcp_custom_read_file".into(),..call}));
}

#[test]
fn checkpoint_retains_user_directions_and_complete_wire_rounds_across_compaction() {
    let mut checkpoint=AgentCheckpoint::new("fix deployment");
    checkpoint.working_notes=AgentText::new("The middle of the log identified port 9000 as occupied. Stop the conflicting process before retrying deployment.");
    let call=crate::AiToolCall{id:"call".into(),name:"run_command".into(),arguments:"{}".into()};
    checkpoint.pending(&call);
    let mut interrupted = progress_message("interrupted", "assistant", "");
    set_message_checkpoint(&mut interrupted, &checkpoint);
    let uncertain = recoverable_checkpoint(&interrupted).unwrap();
    assert_eq!(uncertain.actions[0].error_code.as_deref(), Some("outcome_unknown"));
    assert!(!uncertain.actions[0].success);
    checkpoint.record(&call,&progress_result(&call,true,"Configuration saved"));
    assert_eq!(checkpoint.actions.iter().map(|action| (action.call_id.as_str(), action.success, action.error_code.as_deref())).collect::<Vec<_>>(), vec![("call", true, None)]);
    let mut assistant=progress_message("live-round","assistant","");
    assistant.tool_calls=vec![serde_json::json!({"id":"call","name":"run_command","arguments":"{}"})];
    let native=serde_json::json!({"output":[{"type":"function_call","call_id":"wire","name":"run_command","arguments":"{}"}],"callIds":{"call":"wire"},"results":[]});
    crate::set_ai_provider_parts(&mut assistant,"responses:scope",vec![native.clone()]);
    let mut result=progress_message("result","tool","saved");result.tool_call_id=Some("call".into());
    let mut history=vec![progress_message("base","system","system rules"),progress_message("old","user","old task"),progress_message("old-result","assistant","old answer"),progress_message("task","user","fix deployment"),assistant,result,progress_message("steer","user","Do not restart the database")];
    compact_agent_history(&mut history,&checkpoint,"task");
    assert_eq!(history.iter().map(|message|message.id.as_str()).collect::<Vec<_>>(),vec!["agent-checkpoint","base","task","live-round","result","steer"]);
    assert!(history[0].content.contains("port 9000"));
    assert_eq!(crate::ai_provider_parts(&history[3],"responses:scope"),Some([native].as_slice()));
    assert_eq!(history[4].tool_call_id.as_deref(),Some("call"));
    assert_eq!(history[5].content,"Do not restart the database");
    let mut message=progress_message("response","assistant","partial answer");
    set_message_checkpoint(&mut message,&checkpoint);
    let restored:crate::AiChatMessage=serde_json::from_value(serde_json::to_value(message).unwrap()).unwrap();
    let resume=recoverable_checkpoint(&restored).unwrap();
    assert_eq!(resume.actions[0].call_id,"call");
    assert_eq!(resume.actions[0].summary.as_str(),"Configuration saved");
    checkpoint.needs_continuation=false;
    let mut done=restored;set_message_checkpoint(&mut done,&checkpoint);
    assert!(recoverable_checkpoint(&done).is_none());
}

#[tokio::test]
async fn direction_change_revokes_old_dispatch_until_new_instructions_are_consumed() {
    let runtime = AgentRuntime::new(2);
    let parent = group(&runtime);
    let child = child(&runtime, &parent);
    let old_parent = runtime.dispatch(&parent).unwrap();
    let old_child = runtime.dispatch(&child).unwrap();
    runtime.send(&parent, &parent, AgentMessageKind::UserSupplement, AgentText::new("Only inspect")).unwrap();
    old_parent.invalidated().await;
    assert_eq!(old_parent.check(), Err(AgentError::DirectionChanged));
    assert_eq!(old_child.check(), Err(AgentError::DirectionChanged));
    runtime.drain_messages(&parent).unwrap();
    runtime.dispatch(&parent).unwrap().check().unwrap();
    assert_eq!(old_parent.check(), Err(AgentError::DirectionChanged));
    assert!(matches!(runtime.dispatch(&child), Err(AgentError::DirectionChanged)));
    runtime.send(&parent, &child, AgentMessageKind::FollowUp, AgentText::new("Read only; do not edit")).unwrap();
    runtime.drain_messages(&child).unwrap();
    runtime.dispatch(&child).unwrap().check().unwrap();
    assert_eq!(old_child.check(), Err(AgentError::DirectionChanged));
}

#[test]
fn dependency_failures_block_mutations_without_blocking_independent_reads() {
    let read = |id: &str, path: &str| crate::AiToolCall { id:id.into(), name:"read_resource".into(),
        arguments:serde_json::json!({"resource":"file","handle_id":"rt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","path":path}).to_string() };
    let a=read("a","/a"); let b=read("b","/b");
    let write=crate::AiToolCall{id:"write".into(),name:"run_command".into(),arguments:r#"{"command":"touch /result"}"#.into()};
    let after=read("after","/result");
    let mut deps=ToolDependencies::new(&[a.clone(),b.clone(),write.clone(),after.clone()]);
    deps.record(&a,false);
    assert!(!deps.failed_dependency(&b));
    deps.record(&b,true);
    assert!(deps.failed_dependency(&write));
    deps.record(&write,false);
    assert!(deps.failed_dependency(&after));
    let mut failed=progress_result(&a,false,"temporary failure");
    failed.envelope["error"]["code"]=serde_json::json!("network_timeout");
    let description=ToolExecutionDescription::for_call(&a);
    assert_eq!(description.retry_delay(&failed,0),Some(std::time::Duration::from_secs(1)));
    assert_eq!(description.retry_delay(&failed,2),None);
    assert_eq!(ToolExecutionDescription::for_call(&write).retry_delay(&failed,0),None);
    failed.envelope["error"]["code"]=serde_json::json!("local_command_failed");
    assert_eq!(result_recovery(&failed),Some(Recovery::OutcomeUnknown));
    assert_eq!(description.retry_delay(&failed,0),None);
}

#[test]
fn owned_resources_report_cleanup_and_restore_running_work_as_unknown() {
    let runtime=AgentRuntime::new(2);let parent=group(&runtime);
    let observation=runtime.register_resource(&parent,OwnedResourceKind::Observation,AgentText::new("command observation")).unwrap();
    let remote=runtime.register_resource(&parent,OwnedResourceKind::TerminalCommand,AgentText::new("remote command")).unwrap();
    runtime.cancel_group(&parent).unwrap();
    drop(observation);
    let snapshot=runtime.snapshot(&parent).unwrap();
    assert_eq!(snapshot.resources.iter().map(|resource|resource.state).collect::<Vec<_>>(),vec![OwnedResourceState::Stopped,OwnedResourceState::Running]);
    let restored:AgentSnapshot=serde_json::from_value(serde_json::to_value(snapshot).unwrap()).unwrap();
    assert_eq!(restored.resources[1].state,OwnedResourceState::OutcomeUnknown);
    remote.finish(OwnedResourceState::Completed);drop(remote);
    assert_eq!(runtime.snapshot(&parent).unwrap().resources[1].state,OwnedResourceState::Completed);
}
