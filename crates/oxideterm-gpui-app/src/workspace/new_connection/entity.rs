// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use gpui::{Context, Timer};
use oxideterm_ssh::HostKeyStatus;

use super::HostKeyChallenge;

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
}

impl ConnectionFlowEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            host_key_challenge: None,
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
        self.host_key_challenge = Some(challenge);
        cx.notify();
    }

    pub(in crate::workspace) fn clear_host_key_challenge(&mut self, cx: &mut Context<Self>) {
        if self.host_key_challenge.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn take_host_key_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<HostKeyChallenge> {
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
        if delay.is_zero() {
            self.finish_host_key_challenge_exit(generation, cx);
            return true;
        }
        cx.spawn(async move |connection_flow, cx| {
            Timer::after(delay).await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.finish_host_key_challenge_exit(generation, cx);
            });
        })
        .detach();
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
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{AppContext, TestAppContext};
    use oxideterm_ssh::{HostKeyStatus, SshConfig};

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
}
