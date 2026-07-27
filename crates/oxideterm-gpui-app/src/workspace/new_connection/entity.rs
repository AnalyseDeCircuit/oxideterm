// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{ops::Range, time::Duration};

use gpui::{Context, Task, Timer};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_ssh::{
    HostKeyStatus, KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, SshPromptError,
};
use tokio::sync::oneshot;

use super::{HostKeyChallenge, KeyboardInteractiveChallenge};

/// Contains only non-secret values needed to render the host-key dialog.
pub(in crate::workspace) struct HostKeyDialogSnapshot {
    pub(in crate::workspace) visible: bool,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: u16,
    pub(in crate::workspace) status: HostKeyStatus,
}

/// Owns connection-flow state that must survive independently of root rendering.
pub(in crate::workspace) struct ConnectionFlowEntity {
    host_key_challenge: Option<HostKeyChallenge>,
    host_key_exit_task: Option<Task<()>>,
    keyboard_interactive_challenge: Option<KeyboardInteractiveChallenge>,
    keyboard_interactive_timer_generation: u64,
    keyboard_interactive_timer_task: Option<Task<()>>,
    keyboard_interactive_exit_task: Option<Task<()>>,
}

pub(in crate::workspace) enum KeyboardInteractiveKeyAction {
    NotHandled,
    Handled,
    Paste,
    Submit,
    Cancel,
}

pub(in crate::workspace) enum KeyboardInteractiveSubmitResult {
    Missing,
    Blocked,
    Submitted,
}

impl ConnectionFlowEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            host_key_challenge: None,
            host_key_exit_task: None,
            keyboard_interactive_challenge: None,
            keyboard_interactive_timer_generation: 0,
            keyboard_interactive_timer_task: None,
            keyboard_interactive_exit_task: None,
        }
    }

    pub(in crate::workspace) fn has_host_key_challenge(&self) -> bool {
        self.host_key_challenge.is_some()
    }

    pub(in crate::workspace) fn host_key_dialog_snapshot(&self) -> Option<HostKeyDialogSnapshot> {
        let challenge = self.host_key_challenge.as_ref()?;
        Some(HostKeyDialogSnapshot {
            visible: challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible,
            host: challenge.host.clone(),
            port: challenge.port,
            status: challenge.status.clone(),
        })
    }

    pub(in crate::workspace) fn open_host_key_challenge(
        &mut self,
        challenge: HostKeyChallenge,
        cx: &mut Context<Self>,
    ) {
        // Replacing a challenge drops the previous config without duplicating its auth material.
        self.host_key_exit_task = None;
        self.host_key_challenge = Some(challenge);
        cx.notify();
    }

    pub(in crate::workspace) fn clear_host_key_challenge(&mut self, cx: &mut Context<Self>) {
        self.host_key_exit_task = None;
        if self.host_key_challenge.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn take_host_key_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<HostKeyChallenge> {
        self.host_key_exit_task = None;
        let challenge = self.host_key_challenge.take();
        if challenge.is_some() {
            cx.notify();
        }
        challenge
    }

    pub(in crate::workspace) fn restore_host_key_challenge(
        &mut self,
        challenge: HostKeyChallenge,
        cx: &mut Context<Self>,
    ) {
        self.host_key_exit_task = None;
        self.host_key_challenge = Some(challenge);
        cx.notify();
    }

    pub(in crate::workspace) fn begin_host_key_challenge_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.host_key_challenge.as_mut() else {
            return false;
        };
        let Some(generation) = challenge.presence.begin_exit() else {
            return false;
        };
        self.host_key_exit_task = None;
        if delay.is_zero() {
            self.finish_host_key_challenge_exit(generation, cx);
            return true;
        }
        self.host_key_exit_task = Some(cx.spawn(async move |connection_flow, cx| {
            Timer::after(delay).await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.finish_host_key_challenge_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_host_key_challenge_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self
            .host_key_challenge
            .as_ref()
            .is_some_and(|challenge| challenge.presence.finish_exit(generation))
        {
            return false;
        }
        self.host_key_challenge = None;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn has_keyboard_interactive_challenge(&self) -> bool {
        self.keyboard_interactive_challenge.is_some()
    }

    pub(in crate::workspace) fn keyboard_interactive_challenge(
        &self,
    ) -> Option<&KeyboardInteractiveChallenge> {
        self.keyboard_interactive_challenge.as_ref()
    }

    pub(in crate::workspace) fn focused_keyboard_interactive_prompt(&self) -> Option<usize> {
        self.keyboard_interactive_challenge
            .as_ref()
            .map(|challenge| challenge.focused_prompt)
    }

    pub(in crate::workspace) fn keyboard_interactive_response(
        &self,
        index: usize,
    ) -> Option<String> {
        // GPUI's platform input API requires an owned value at this boundary.
        self.keyboard_interactive_challenge
            .as_ref()?
            .responses
            .get(index)
            .cloned()
    }

    pub(in crate::workspace) fn open_keyboard_interactive_challenge(
        &mut self,
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(existing) = self.keyboard_interactive_challenge.as_ref()
            && existing.request.flow_id != request.flow_id
        {
            // Keep the active auth flow as the only owner of the protected dialog.
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
            return false;
        }
        self.keyboard_interactive_timer_task = None;
        self.keyboard_interactive_exit_task = None;
        if let Some(mut existing) = self.keyboard_interactive_challenge.take()
            && let Some(existing_tx) = existing.response_tx.take()
        {
            // Reject the replaced oneshot so no transport waits for stale input.
            let _ = existing_tx.send(Err(SshPromptError::Cancelled));
        }
        self.keyboard_interactive_challenge =
            Some(KeyboardInteractiveChallenge::new(request, response_tx));
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.schedule_keyboard_interactive_timer(self.keyboard_interactive_timer_generation, cx);
        cx.notify();
        true
    }

    fn schedule_keyboard_interactive_timer(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.keyboard_interactive_timer_task = Some(cx.spawn(async move |connection_flow, cx| {
            loop {
                Timer::after(Duration::from_secs(1)).await;
                let keep_ticking = connection_flow
                    .update(cx, |connection_flow, cx| {
                        let Some(challenge) =
                            connection_flow.keyboard_interactive_challenge.as_ref()
                        else {
                            return false;
                        };
                        if connection_flow.keyboard_interactive_timer_generation != generation {
                            return false;
                        }
                        cx.notify();
                        !challenge.timed_out()
                    })
                    .unwrap_or(false);
                if !keep_ticking {
                    break;
                }
            }
        }));
    }

    pub(in crate::workspace) fn handle_keyboard_interactive_key(
        &mut self,
        key: &str,
        shift: bool,
        uses_text_edit_modifier: bool,
        cx: &mut Context<Self>,
    ) -> KeyboardInteractiveKeyAction {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return KeyboardInteractiveKeyAction::NotHandled;
        };
        if challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
            return KeyboardInteractiveKeyAction::Handled;
        }
        if uses_text_edit_modifier {
            return if key == "v" {
                KeyboardInteractiveKeyAction::Paste
            } else {
                KeyboardInteractiveKeyAction::Handled
            };
        }

        match key {
            "escape" => KeyboardInteractiveKeyAction::Cancel,
            "enter" if !challenge.timed_out() && challenge.all_responses_filled() => {
                KeyboardInteractiveKeyAction::Submit
            }
            "tab" => {
                if !challenge.responses.is_empty() {
                    if shift {
                        challenge.focused_prompt = challenge
                            .focused_prompt
                            .saturating_sub(1)
                            .min(challenge.responses.len() - 1);
                    } else {
                        challenge.focused_prompt =
                            (challenge.focused_prompt + 1).min(challenge.responses.len() - 1);
                    }
                }
                cx.notify();
                KeyboardInteractiveKeyAction::Handled
            }
            "backspace" => {
                if !challenge.timed_out()
                    && let Some(response) = challenge.responses.get_mut(challenge.focused_prompt)
                {
                    response.pop();
                    cx.notify();
                }
                KeyboardInteractiveKeyAction::Handled
            }
            _ => KeyboardInteractiveKeyAction::Handled,
        }
    }

    pub(in crate::workspace) fn focus_keyboard_interactive_prompt(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return;
        };
        challenge.focused_prompt = index.min(challenge.responses.len().saturating_sub(1));
        cx.notify();
    }

    pub(in crate::workspace) fn paste_keyboard_interactive_response(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return false;
        };
        if challenge.timed_out() {
            return false;
        }
        let Some(response) = challenge.responses.get_mut(challenge.focused_prompt) else {
            return false;
        };
        response.push_str(text);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn replace_keyboard_interactive_response(
        &mut self,
        index: usize,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return false;
        };
        if challenge.timed_out() {
            return false;
        }
        let Some(response) = challenge.responses.get_mut(index) else {
            return false;
        };
        replace_utf16(response, replacement_range, text);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn submit_keyboard_interactive_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) -> KeyboardInteractiveSubmitResult {
        let Some(mut challenge) = self.keyboard_interactive_challenge.take() else {
            return KeyboardInteractiveSubmitResult::Missing;
        };
        if challenge.timed_out() || !challenge.all_responses_filled() {
            self.keyboard_interactive_challenge = Some(challenge);
            cx.notify();
            return KeyboardInteractiveSubmitResult::Blocked;
        }
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.keyboard_interactive_timer_task = None;
        if let Some(response_tx) = challenge.response_tx.take() {
            // Move the Zeroizing response owner directly to the SSH prompt waiter.
            let _ = response_tx.send(Ok(challenge.responses));
        }
        cx.notify();
        KeyboardInteractiveSubmitResult::Submitted
    }

    pub(in crate::workspace) fn cancel_keyboard_interactive_challenge(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return false;
        };
        let Some(generation) = challenge.presence.begin_exit() else {
            return false;
        };
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.keyboard_interactive_timer_task = None;
        self.keyboard_interactive_exit_task = None;
        if let Some(response_tx) = challenge.response_tx.take() {
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
        }
        if delay.is_zero() {
            self.finish_keyboard_interactive_exit(generation, cx);
            return true;
        }
        self.keyboard_interactive_exit_task = Some(cx.spawn(async move |connection_flow, cx| {
            Timer::after(delay).await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.finish_keyboard_interactive_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_keyboard_interactive_exit(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .keyboard_interactive_challenge
            .as_ref()
            .is_some_and(|challenge| challenge.presence.finish_exit(generation))
        {
            return false;
        }
        // Dropping the retained Zeroizing payload scrubs every secret answer.
        self.keyboard_interactive_challenge = None;
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{AppContext, TestAppContext};
    use oxideterm_ssh::{
        HostKeyStatus, KeyboardInteractivePrompt, KeyboardInteractivePromptRequest, SshConfig,
        SshPromptError,
    };
    use tokio::sync::oneshot;

    use super::ConnectionFlowEntity;
    use crate::workspace::new_connection::{HostKeyChallenge, SshConnectionIntent};

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
            session_tree_challenge: None,
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

    #[gpui::test]
    fn host_key_state_and_exit_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(|_| ConnectionFlowEntity::new());

        entity.update(cx, |entity, cx| {
            entity.open_host_key_challenge(unknown_host_key_challenge(), cx);
            let snapshot = entity
                .host_key_dialog_snapshot()
                .expect("host-key render snapshot");
            assert!(snapshot.visible);
            assert_eq!(snapshot.host, "example.test");
            assert!(entity.begin_host_key_challenge_exit(Duration::ZERO, cx));
            assert!(!entity.has_host_key_challenge());
        });
    }

    #[gpui::test]
    fn taking_and_restoring_host_key_challenge_preserves_single_ownership(cx: &mut TestAppContext) {
        let entity = cx.new(|_| ConnectionFlowEntity::new());

        entity.update(cx, |entity, cx| {
            entity.open_host_key_challenge(unknown_host_key_challenge(), cx);
            let challenge = entity
                .take_host_key_challenge(cx)
                .expect("owned host-key challenge");
            assert!(!entity.has_host_key_challenge());
            entity.restore_host_key_challenge(challenge, cx);
            assert!(entity.has_host_key_challenge());
        });
    }

    #[gpui::test]
    fn keyboard_interactive_responses_move_once_to_the_prompt_waiter(cx: &mut TestAppContext) {
        let entity = cx.new(|_| ConnectionFlowEntity::new());
        let (response_tx, mut response_rx) = oneshot::channel();

        entity.update(cx, |entity, cx| {
            assert!(entity.open_keyboard_interactive_challenge(
                keyboard_interactive_request("flow-a"),
                response_tx,
                cx,
            ));
            assert!(entity.replace_keyboard_interactive_response(0, None, "secret", cx));
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
    fn competing_keyboard_interactive_flow_is_cancelled_without_replacing_owner(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(|_| ConnectionFlowEntity::new());
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
    fn cancelling_keyboard_interactive_challenge_rejects_waiter_and_drops_answers(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(|_| ConnectionFlowEntity::new());
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
}
