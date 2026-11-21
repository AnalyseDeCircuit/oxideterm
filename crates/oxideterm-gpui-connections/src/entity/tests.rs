use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui::{AppContext, TestAppContext};
use oxideterm_ssh::{
    HostKeyStatus, KeyboardInteractivePrompt, KeyboardInteractivePromptRequest,
    NativeSessionTreeConnectAction, NativeSessionTreeConnectPlan, NativeSessionTreeConnectStep,
    NodeId, SshConfig, SshPromptError,
};
use tokio::sync::oneshot;

use super::{ConnectionFlowEntity, ConnectionFlowEvent, NativeProxyConnectRun};
use crate::{
    HostKeyChallenge, NewConnectionField, NewConnectionForm, NewConnectionProxyHop,
    SavedConnectionPromptAction, SshConnectionIntent, SshConnectionWorkerResult,
};

fn unknown_host_key_challenge() -> HostKeyChallenge {
    HostKeyChallenge {
        presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
        config: SshConfig::default(),
        title: "Test".to_string(),
        status: HostKeyStatus::Unknown {
            fingerprint: "SHA256:test".to_string(),
            key_type: "ssh-ed25519".to_string(),
        },
        intent: SshConnectionIntent::Test,
        session_tree_challenge: false,
        host: "example.test".to_string(),
        port: 22,
    }
}

fn keyboard_interactive_request(flow_id: &str) -> KeyboardInteractivePromptRequest {
    KeyboardInteractivePromptRequest {
        flow_id: flow_id.to_string(),
        name: "Authentication".to_string(),
        instructions: String::new(),
        prompts: vec![KeyboardInteractivePrompt {
            prompt: "Password".to_string(),
            echo: false,
        }],
        chained: false,
    }
}

fn proxy_connect_run() -> NativeProxyConnectRun {
    NativeProxyConnectRun {
        parent_lease: None,
        generation: 0,
        plan: NativeSessionTreeConnectPlan {
            target_node_id: NodeId::new("target"),
            cleanup_node_id: Some(NodeId::new("target")),
            steps: vec![NativeSessionTreeConnectStep {
                node_id: NodeId::new("target"),
                host: "target.example.test".to_string(),
                port: 22,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
                host_key_replacement: None,
                preflight_verified: false,
            }],
            current_index: 0,
        },
        title: "Target".to_string(),
        intent: SshConnectionIntent::Connect(Default::default()),
        save_after_open: None,
        upstream_proxy: None,
    }
}

#[gpui::test]
fn ssh_worker_delivery_and_release_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let worker_results_ready = Arc::new(AtomicBool::new(false));
    let event_flag = worker_results_ready.clone();
    let _subscription = entity.update(cx, |_, cx| {
        cx.subscribe(&entity, move |_, _, event: &ConnectionFlowEvent, _cx| {
            if *event == ConnectionFlowEvent::WorkerResultsReady {
                event_flag.store(true, Ordering::Release);
            }
        })
    });
    let sender = cx.read(|cx| entity.read(cx).ssh_worker_sender());
    let wake = sender.wake();

    sender
        .send(SshConnectionWorkerResult::Test { result: Ok(()) })
        .expect("SSH worker delivery");
    cx.run_until_parked();

    entity.update(cx, |entity, _cx| {
        assert!(matches!(
            entity.take_worker_results().front(),
            Some(SshConnectionWorkerResult::Test { result: Ok(()) })
        ));
    });
    assert!(worker_results_ready.load(Ordering::Acquire));

    drop(entity);
    cx.update(|_cx| {});
    assert!(wake.is_stopped());
}

#[gpui::test]
fn form_close_releases_drafts_and_moves_proxy_run_to_cleanup(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);

    entity.update(cx, |entity, cx| {
        let mut form = NewConnectionForm::default();
        form.password = "secret".to_string();
        entity.form.replace_with_new_form(form);
        entity.form.editing_saved_connection_id = Some("saved-id".to_string());
        entity.form.saved_connection_prompt_action = Some(SavedConnectionPromptAction::Connect);
        assert!(
            entity
                .start_proxy_connect_run(proxy_connect_run(), cx)
                .is_ok()
        );
        assert!(entity.has_active_proxy_connect_run());

        assert!(entity.begin_connection_form_exit(Duration::ZERO, cx));
        assert!(!entity.has_active_proxy_connect_run());
        assert!(entity.form.form.is_none());
        assert!(entity.form.editing_saved_connection_id.is_none());
        assert!(entity.form.saved_connection_prompt_action.is_none());
        let cancelled = entity.take_cancelled_proxy_connect_runs();
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0].generation, 1);
    });
}

#[gpui::test]
fn proxy_run_transitions_and_host_key_state_stay_inside_entity(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let target_node_id = NodeId::new("target");

    entity.update(cx, |entity, cx| {
        assert!(
            entity
                .start_proxy_connect_run(proxy_connect_run(), cx)
                .is_ok()
        );
        assert!(matches!(
            entity.proxy_connect_next_action(),
            Some(NativeSessionTreeConnectAction::Preflight { .. })
        ));

        let context = entity
            .take_proxy_connect_preflight_context(cx)
            .expect("preflight context");
        assert_eq!(context.generation, 1);
        assert_eq!(context.step_index, 0);
        assert!(entity.restore_proxy_connect_preflight_context(context, cx));

        let replacement = oxideterm_ssh::HostKeyReplacement {
            host: "target.example.test".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            previous_fingerprint: "SHA256:old".to_string(),
            fingerprint: "SHA256:test".to_string(),
        };
        assert!(entity.open_active_proxy_host_key_challenge(
            HostKeyStatus::Changed {
                expected_fingerprint: "SHA256:old".to_string(),
                actual_fingerprint: "SHA256:test".to_string(),
                key_type: "ssh-ed25519".to_string(),
            },
            cx,
        ));
        let challenge = entity
            .take_host_key_challenge(cx)
            .expect("proxy host-key challenge");
        assert!(challenge.session_tree_challenge);

        entity
            .accept_active_proxy_connect_host_key(
                true,
                "SHA256:test".to_string(),
                Some(replacement.clone()),
                cx,
            )
            .expect("accepted host key");
        assert!(matches!(
            entity.proxy_connect_next_action(),
            Some(NativeSessionTreeConnectAction::Connect { step })
                if step.host_key_replacement == Some(replacement)
                    && step.expected_host_key_fingerprint.as_deref() == Some("SHA256:test")
        ));
        assert!(entity.advance_active_proxy_connect_after_node_connected(&target_node_id, cx));
        assert!(matches!(
            entity.proxy_connect_next_action(),
            Some(NativeSessionTreeConnectAction::Complete { .. })
        ));
    });
}

#[gpui::test]
fn host_key_state_and_exit_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);

    entity.update(cx, |entity, cx| {
        entity.open_host_key_challenge(unknown_host_key_challenge(), cx);
        let snapshot = entity
            .host_key_dialog_snapshot()
            .expect("host-key render snapshot");
        assert!(snapshot.visible);
        assert_eq!(snapshot.host, "example.test");
        let challenge = entity
            .take_host_key_challenge(cx)
            .expect("owned host-key challenge");
        assert!(!entity.has_host_key_challenge());
        entity.open_host_key_challenge(challenge, cx);
        assert!(entity.has_host_key_challenge());
        assert!(entity.begin_host_key_challenge_exit(Duration::ZERO, cx));
        assert!(!entity.has_host_key_challenge());
    });
}

#[gpui::test]
fn keyboard_interactive_responses_move_once_to_the_prompt_waiter(cx: &mut TestAppContext) {
    use std::{cell::Cell, rc::Rc};

    let entity = cx.new(ConnectionFlowEntity::new);
    let (response_tx, mut response_rx) = oneshot::channel();

    entity.update(cx, |entity, cx| {
        assert!(entity.open_keyboard_interactive_challenge(
            keyboard_interactive_request("flow-a"),
            response_tx,
            cx,
        ));
    });

    let form_notified = Rc::new(Cell::new(false));
    let auth_notified = Rc::new(Cell::new(false));
    let (_form_observation, _auth_subscription) = cx.update(|cx| {
        let observed_form = form_notified.clone();
        let observed_auth = auth_notified.clone();
        (
            cx.observe(&entity, move |_, _| observed_form.set(true)),
            cx.subscribe(&entity, move |_, _: &super::AuthenticationChanged, _| {
                observed_auth.set(true);
            }),
        )
    });
    entity.update(cx, |entity, cx| {
        assert!(entity.replace_keyboard_interactive_response(0, None, "secret", cx));
    });
    assert!(
        auth_notified.get(),
        "prompt text must repaint the auth surface"
    );
    assert!(
        !form_notified.get(),
        "typing must not notify form observers"
    );

    entity.update(cx, |entity, cx| {
        assert!(matches!(
            entity.submit_keyboard_interactive_challenge(cx),
            super::KeyboardInteractiveSubmitResult::Submitted
        ));
        assert!(!entity.has_keyboard_interactive_challenge());
    });

    let responses = response_rx
        .try_recv()
        .expect("prompt response delivery")
        .expect("submitted responses");
    assert_eq!(responses.as_slice(), ["secret"]);
}

#[gpui::test]
fn security_key_touch_wait_cannot_submit_and_releases_the_dialog_with_its_waiter(
    cx: &mut TestAppContext,
) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let (response_tx, response_rx) = oneshot::channel();
    entity.update(cx, |entity, cx| {
        let mut request = keyboard_interactive_request("fido-touch");
        request.prompts.clear();
        assert!(entity.open_keyboard_interactive_challenge(request, response_tx, cx));
        entity.wait_for_security_key_touch("fido-touch", cx);
        assert!(matches!(
            entity.handle_keyboard_interactive_key("enter", false, false, cx),
            super::KeyboardInteractiveKeyAction::Handled
        ));
        assert!(matches!(
            entity.submit_keyboard_interactive_challenge(cx),
            super::KeyboardInteractiveSubmitResult::Blocked
        ));
        assert!(entity.has_keyboard_interactive_challenge());
    });
    drop(response_rx);
    entity.update(cx, |entity, cx| {
        entity.clear_inactive_keyboard_interactive_challenge(cx);
        assert!(!entity.has_keyboard_interactive_challenge());
    });
}

#[gpui::test]
fn password_prompt_requires_opt_in_before_transferring_save_consent(cx: &mut TestAppContext) {
    for remember in [false, true] {
        let entity = cx.new(ConnectionFlowEntity::new);
        let (response_tx, mut response_rx) = oneshot::channel();
        entity.update(cx, |entity, cx| {
            entity.open_keyboard_interactive_challenge(
                keyboard_interactive_request("password"),
                crate::prompts::NativeSshPromptSender::Password {
                    response_tx,
                    on_authenticated: Some(Box::new(|_| {})),
                },
                cx,
            );
            assert!(
                !entity
                    .keyboard_interactive_challenge()
                    .unwrap()
                    .remember_password
            );
            if remember {
                entity.toggle_prompt_password_remember(cx);
            }
            entity.replace_keyboard_interactive_response(0, None, "entered-password", cx);
            entity.submit_keyboard_interactive_challenge(cx);
        });
        let response = response_rx.try_recv().unwrap().unwrap();
        assert_eq!(response.password.as_str(), "entered-password");
        assert_eq!(response.on_authenticated.is_some(), remember);
    }
}

#[gpui::test]
fn competing_keyboard_interactive_flow_is_cancelled_without_replacing_owner(
    cx: &mut TestAppContext,
) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let (first_tx, _first_rx) = oneshot::channel();
    let (second_tx, mut second_rx) = oneshot::channel();

    entity.update(cx, |entity, cx| {
        assert!(entity.open_keyboard_interactive_challenge(
            keyboard_interactive_request("flow-a"),
            first_tx,
            cx,
        ));
        assert!(!entity.open_keyboard_interactive_challenge(
            keyboard_interactive_request("flow-b"),
            second_tx,
            cx,
        ));
        assert!(entity.has_keyboard_interactive_challenge());
    });

    assert!(matches!(
        second_rx.try_recv(),
        Ok(Err(SshPromptError::Cancelled))
    ));
}

#[gpui::test]
fn abandoned_keyboard_interactive_request_does_not_block_next_prompt(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let (first_tx, first_rx) = oneshot::channel();
    let (next_tx, mut next_rx) = oneshot::channel();

    entity.update(cx, |entity, cx| {
        assert!(entity.open_keyboard_interactive_challenge(
            keyboard_interactive_request("expired-flow"),
            first_tx,
            cx,
        ));
    });
    // The transport drops its waiter when authentication times out or is cancelled.
    drop(first_rx);
    entity.update(cx, |entity, cx| {
        assert!(entity.open_keyboard_interactive_challenge(
            keyboard_interactive_request("next-flow"),
            next_tx,
            cx,
        ));
        assert!(entity.replace_keyboard_interactive_response(0, None, "123456", cx));
        assert!(matches!(
            entity.submit_keyboard_interactive_challenge(cx),
            super::KeyboardInteractiveSubmitResult::Submitted
        ));
    });
    assert_eq!(next_rx.try_recv().unwrap().unwrap().as_slice(), ["123456"]);
}

#[gpui::test]
fn expired_keyboard_interactive_prompt_rejects_waiter_and_releases_input(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let (response_tx, mut response_rx) = oneshot::channel();
    entity.update(cx, |entity, cx| {
        assert!(entity.open_keyboard_interactive_challenge(
            keyboard_interactive_request("expired-flow"),
            response_tx,
            cx,
        ));
        assert!(entity.replace_keyboard_interactive_response(0, None, "123456", cx));
        entity
            .keyboard_interactive_challenge
            .as_mut()
            .unwrap()
            .expires_at = std::time::Instant::now();
        entity.clear_inactive_keyboard_interactive_challenge(cx);
        assert!(!entity.has_keyboard_interactive_challenge());
        assert!(entity.focused_keyboard_interactive_prompt().is_none());
    });
    assert!(matches!(
        response_rx.try_recv(),
        Ok(Err(SshPromptError::Timeout))
    ));
}

#[gpui::test]
fn cancelling_keyboard_interactive_challenge_rejects_waiter_and_drops_answers(
    cx: &mut TestAppContext,
) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let (response_tx, mut response_rx) = oneshot::channel();

    entity.update(cx, |entity, cx| {
        assert!(entity.open_keyboard_interactive_challenge(
            keyboard_interactive_request("flow-a"),
            response_tx,
            cx,
        ));
        assert!(entity.replace_keyboard_interactive_response(0, None, "secret", cx));
        assert!(entity.cancel_keyboard_interactive_challenge(Duration::ZERO, cx));
        assert!(!entity.has_keyboard_interactive_challenge());
    });

    assert!(matches!(
        response_rx.try_recv(),
        Ok(Err(SshPromptError::Cancelled))
    ));
}

#[gpui::test]
fn closing_form_cancels_entity_owned_picker_task(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);
    let (_path_tx, path_rx) = oneshot::channel::<Option<String>>();

    entity.update(cx, |entity, cx| {
        entity
            .form
            .replace_with_new_form(NewConnectionForm::default());
        assert!(entity.start_path_picker(
            NewConnectionField::KeyPath,
            async move { path_rx.await.ok().flatten() },
            cx,
        ));
        assert!(entity.path_picker_task.is_some());

        assert!(entity.begin_connection_form_exit(Duration::ZERO, cx));
        assert!(entity.path_picker_task.is_none());
        assert!(entity.form.form.is_none());
    });
}

#[gpui::test]
fn jump_server_exit_commits_once_inside_the_connection_entity(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);

    entity.update(cx, |entity, cx| {
        let mut form = NewConnectionForm::default();
        let mut jump_server = NewConnectionProxyHop::new();
        jump_server.host = "jump.example.test".to_string();
        jump_server.username = "alice".to_string();
        form.jump_server_form = Some(jump_server);
        entity.form.replace_with_new_form(form);

        assert!(entity.begin_jump_server_form_exit(true, Duration::ZERO, cx));
        let form = entity.form.form.as_ref().expect("retained connection form");
        assert!(form.jump_server_form.is_none());
        assert_eq!(form.proxy_hops.len(), 1);
        assert_eq!(form.proxy_hops[0].host, "jump.example.test");
    });
}

#[gpui::test]
fn cancelling_jump_server_edit_restores_the_original_hop(cx: &mut TestAppContext) {
    let entity = cx.new(ConnectionFlowEntity::new);

    entity.update(cx, |entity, cx| {
        let mut form = NewConnectionForm::default();
        let mut jump_server = NewConnectionProxyHop::new();
        jump_server.host = "jump.example.test".to_string();
        jump_server.username = "alice".to_string();
        form.jump_server_form = Some(jump_server);
        form.jump_server_edit_index = Some(0);
        entity.form.replace_with_new_form(form);

        assert!(entity.begin_jump_server_form_exit(false, Duration::ZERO, cx));
        let form = entity.form.form.as_ref().expect("retained connection form");
        assert!(form.jump_server_form.is_none());
        assert_eq!(form.proxy_hops.len(), 1);
        assert_eq!(form.proxy_hops[0].host, "jump.example.test");
    });
}
