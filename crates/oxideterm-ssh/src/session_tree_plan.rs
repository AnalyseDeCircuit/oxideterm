// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{HostKeyStatus, NodeId, NodeTreeExpansion};

/// Identifies the network endpoint used by one root-to-target connection step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSessionTreeConnectEndpoint {
    pub host: String,
    pub port: u16,
}

impl NativeSessionTreeConnectEndpoint {
    /// Creates an endpoint without coupling the plan to a transport runtime.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}

/// Stores the resumable host-key state for one node in a connection chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSessionTreeConnectStep {
    pub node_id: NodeId,
    pub host: String,
    pub port: u16,
    pub trust_host_key: Option<bool>,
    pub expected_host_key_fingerprint: Option<String>,
    pub preflight_verified: bool,
}

impl NativeSessionTreeConnectStep {
    /// Matches Tauri's accepted challenge contract, which requires both values.
    pub fn has_accepted_host_key(&self) -> bool {
        self.trust_host_key.is_some() && self.expected_host_key_fingerprint.is_some()
    }

    /// Returns whether the current step may connect without another preflight.
    pub fn can_connect_without_preflight(&self) -> bool {
        // A verified preflight only advances this native execution attempt. It
        // must not fabricate accepted fingerprint data in the persisted plan.
        self.preflight_verified || self.has_accepted_host_key()
    }

    /// Records the exact fingerprint accepted by the user for this step.
    pub fn with_accepted_host_key(
        mut self,
        trust_host_key: bool,
        expected_host_key_fingerprint: impl Into<String>,
    ) -> Self {
        self.trust_host_key = Some(trust_host_key);
        self.expected_host_key_fingerprint = Some(expected_host_key_fingerprint.into());
        self.preflight_verified = false;
        self
    }
}

/// Owns the pure state machine for a resumable root-to-target SSH connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSessionTreeConnectPlan {
    pub target_node_id: NodeId,
    pub cleanup_node_id: Option<NodeId>,
    pub steps: Vec<NativeSessionTreeConnectStep>,
    pub current_index: usize,
}

impl NativeSessionTreeConnectPlan {
    /// Builds a plan while preserving the expansion's root-to-target ordering.
    pub fn from_expansion(
        expansion: &NodeTreeExpansion,
        endpoints: Vec<NativeSessionTreeConnectEndpoint>,
        cleanup_node_id: Option<NodeId>,
    ) -> Result<Self, String> {
        if expansion.path_node_ids.len() != endpoints.len() {
            return Err(format!(
                "proxy connect plan endpoint mismatch: pathNodes={} endpoints={}",
                expansion.path_node_ids.len(),
                endpoints.len()
            ));
        }

        let steps = expansion
            .path_node_ids
            .iter()
            .cloned()
            .zip(endpoints)
            .map(|(node_id, endpoint)| NativeSessionTreeConnectStep {
                node_id,
                host: endpoint.host,
                port: endpoint.port,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
                preflight_verified: false,
            })
            .collect::<Vec<_>>();

        // The target remains separate so the UI creates a terminal only after
        // every required node has connected, matching the Tauri plan contract.
        Ok(Self {
            target_node_id: expansion.target_node_id.clone(),
            cleanup_node_id,
            steps,
            current_index: 0,
        })
    }

    /// Selects the next effect that the GPUI orchestration layer must execute.
    pub fn next_action(&self) -> NativeSessionTreeConnectAction {
        let Some(step) = self.steps.get(self.current_index).cloned() else {
            return NativeSessionTreeConnectAction::Complete {
                target_node_id: self.target_node_id.clone(),
            };
        };

        if step.can_connect_without_preflight() {
            NativeSessionTreeConnectAction::Connect { step }
        } else {
            NativeSessionTreeConnectAction::Preflight { step }
        }
    }

    /// Advances only after the orchestration layer confirms a connected step.
    pub fn advance_after_connected_step(&mut self) {
        if self.current_index < self.steps.len() {
            self.current_index += 1;
        }
    }

    /// Resumes the current step with the fingerprint accepted by the user.
    pub fn accept_current_host_key(
        &mut self,
        trust_host_key: bool,
        expected_host_key_fingerprint: impl Into<String>,
    ) -> Result<(), String> {
        let Some(step) = self.steps.get_mut(self.current_index) else {
            return Err("proxy connect plan has no current step".to_string());
        };
        step.trust_host_key = Some(trust_host_key);
        step.expected_host_key_fingerprint = Some(expected_host_key_fingerprint.into());
        step.preflight_verified = false;
        Ok(())
    }

    /// Marks only the current hop as verified for this execution attempt.
    pub fn mark_current_preflight_verified(&mut self) -> Result<(), String> {
        let Some(step) = self.steps.get_mut(self.current_index) else {
            return Err("proxy connect plan has no current step".to_string());
        };
        step.preflight_verified = true;
        step.trust_host_key = None;
        step.expected_host_key_fingerprint = None;
        Ok(())
    }

    /// Returns Tauri's explicit cleanupNodeId without reinterpreting the path.
    pub fn cleanup_root_node_id(&self) -> Option<NodeId> {
        self.cleanup_node_id.clone()
    }

    /// Captures the current plan and step for a host-key confirmation dialog.
    pub fn challenge_for_current_step(
        &self,
        status: HostKeyStatus,
    ) -> Result<NativeSessionTreeConnectChallenge, String> {
        let Some(step) = self.steps.get(self.current_index).cloned() else {
            return Err("proxy connect plan has no current step".to_string());
        };
        Ok(NativeSessionTreeConnectChallenge {
            plan: self.clone(),
            status,
            step,
        })
    }
}

/// Carries a resumable unknown or changed host-key decision to the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeSessionTreeConnectChallenge {
    pub plan: NativeSessionTreeConnectPlan,
    pub status: HostKeyStatus,
    pub step: NativeSessionTreeConnectStep,
}

/// Describes the next side effect without executing GPUI or transport work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeSessionTreeConnectAction {
    Preflight { step: NativeSessionTreeConnectStep },
    Connect { step: NativeSessionTreeConnectStep },
    Complete { target_node_id: NodeId },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_id(value: &str) -> NodeId {
        NodeId::new(value.to_string())
    }

    fn expansion() -> NodeTreeExpansion {
        NodeTreeExpansion {
            target_node_id: node_id("target"),
            path_node_ids: vec![node_id("hop-1"), node_id("hop-2"), node_id("target")],
            chain_depth: 3,
        }
    }

    fn endpoints() -> Vec<NativeSessionTreeConnectEndpoint> {
        vec![
            NativeSessionTreeConnectEndpoint::new("jump-a", 22),
            NativeSessionTreeConnectEndpoint::new("jump-b", 2200),
            NativeSessionTreeConnectEndpoint::new("target.internal", 2222),
        ]
    }

    #[test]
    fn session_tree_connect_plan_rejects_endpoint_count_mismatch() {
        let error = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            vec![NativeSessionTreeConnectEndpoint::new("jump-a", 22)],
            Some(node_id("target")),
        )
        .expect_err("endpoint count mismatch");

        assert!(error.contains("pathNodes=3 endpoints=1"));
    }

    #[test]
    fn session_tree_connect_plan_preflights_each_hop_in_order_and_completes() {
        let mut plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            endpoints(),
            Some(node_id("target")),
        )
        .expect("valid plan");

        assert_eq!(plan.target_node_id, node_id("target"));
        assert_eq!(plan.cleanup_root_node_id(), Some(node_id("target")));
        plan.steps[0].trust_host_key = Some(false);
        for (id, host, port) in [
            ("hop-1", "jump-a", 22),
            ("hop-2", "jump-b", 2200),
            ("target", "target.internal", 2222),
        ] {
            match plan.next_action() {
                NativeSessionTreeConnectAction::Preflight { step } => {
                    assert_eq!(step.node_id, node_id(id));
                    assert_eq!((step.host.as_str(), step.port), (host, port));
                }
                action => panic!("expected preflight for {id}, got {action:?}"),
            }
            plan.mark_current_preflight_verified().unwrap();
            assert!(matches!(
                plan.next_action(),
                NativeSessionTreeConnectAction::Connect { ref step }
                    if step.node_id == node_id(id)
            ));
            plan.advance_after_connected_step();
        }
        assert_eq!(
            plan.next_action(),
            NativeSessionTreeConnectAction::Complete {
                target_node_id: node_id("target")
            }
        );
    }

    #[test]
    fn session_tree_connect_plan_connects_accepted_step_without_preflight() {
        let mut plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            endpoints(),
            Some(node_id("target")),
        )
        .expect("valid plan");
        plan.accept_current_host_key(false, "SHA256:test")
            .expect("current step accepts host key");

        match plan.next_action() {
            NativeSessionTreeConnectAction::Connect { step } => {
                assert_eq!(step.node_id, node_id("hop-1"));
                assert_eq!(step.trust_host_key, Some(false));
                assert_eq!(
                    step.expected_host_key_fingerprint.as_deref(),
                    Some("SHA256:test")
                );
            }
            action => panic!("unexpected action: {action:?}"),
        }
    }

    #[test]
    fn session_tree_connect_plan_connects_verified_step_without_fake_fingerprint() {
        let mut plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            endpoints(),
            Some(node_id("target")),
        )
        .expect("valid plan");
        plan.mark_current_preflight_verified()
            .expect("current step can be marked verified");

        match plan.next_action() {
            NativeSessionTreeConnectAction::Connect { step } => {
                assert_eq!(step.node_id, node_id("hop-1"));
                assert!(step.preflight_verified);
                assert_eq!(step.trust_host_key, None);
                assert_eq!(step.expected_host_key_fingerprint, None);
            }
            action => panic!("unexpected action: {action:?}"),
        }
    }
}
