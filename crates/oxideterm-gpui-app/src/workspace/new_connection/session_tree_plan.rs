// Tauri connects expanded proxy chains as a resumable, root-to-target plan:
// preflight the current node, connect it, then advance to the next step.
#![allow(dead_code)]

use oxideterm_ssh::{HostKeyStatus, NodeId, NodeTreeExpansion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct NativeSessionTreeConnectEndpoint {
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: u16,
}

impl NativeSessionTreeConnectEndpoint {
    pub(in crate::workspace) fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct NativeSessionTreeConnectStep {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: u16,
    pub(in crate::workspace) trust_host_key: Option<bool>,
    pub(in crate::workspace) expected_host_key_fingerprint: Option<String>,
    pub(in crate::workspace) preflight_verified: bool,
}

impl NativeSessionTreeConnectStep {
    pub(in crate::workspace) fn has_accepted_host_key(&self) -> bool {
        self.trust_host_key.is_some() && self.expected_host_key_fingerprint.is_some()
    }

    pub(in crate::workspace) fn can_connect_without_preflight(&self) -> bool {
        // Tauri only skips preflight on a resumed host-key challenge when both
        // trustHostKey and expectedHostKeyFingerprint are present. A freshly
        // verified preflight is native-only state used to continue the same
        // connect loop without adding fake fingerprint data to the plan.
        self.preflight_verified || self.has_accepted_host_key()
    }

    pub(in crate::workspace) fn with_accepted_host_key(
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct NativeSessionTreeConnectPlan {
    pub(in crate::workspace) target_node_id: NodeId,
    pub(in crate::workspace) cleanup_node_id: Option<NodeId>,
    pub(in crate::workspace) steps: Vec<NativeSessionTreeConnectStep>,
    pub(in crate::workspace) current_index: usize,
}

impl NativeSessionTreeConnectPlan {
    pub(in crate::workspace) fn from_expansion(
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

        // Mirrors Tauri's SessionTreeConnectPlan: the plan is resumable and
        // stores the target separately from the step list so terminal creation
        // happens only after the target node is connected.
        Ok(Self {
            target_node_id: expansion.target_node_id.clone(),
            cleanup_node_id,
            steps,
            current_index: 0,
        })
    }

    pub(in crate::workspace) fn next_action(&self) -> NativeSessionTreeConnectAction {
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

    pub(in crate::workspace) fn advance_after_connected_step(&mut self) {
        if self.current_index < self.steps.len() {
            self.current_index += 1;
        }
    }

    pub(in crate::workspace) fn accept_current_host_key(
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

    pub(in crate::workspace) fn mark_current_preflight_verified(&mut self) -> Result<(), String> {
        let Some(step) = self.steps.get_mut(self.current_index) else {
            return Err("proxy connect plan has no current step".to_string());
        };
        step.preflight_verified = true;
        step.trust_host_key = None;
        step.expected_host_key_fingerprint = None;
        Ok(())
    }

    pub(in crate::workspace) fn cleanup_root_node_id(&self) -> Option<NodeId> {
        // Tauri cleanupSessionTreeConnectPlan calls removeNode(cleanupNodeId)
        // exactly; it does not reinterpret cleanup as "the first step".
        self.cleanup_node_id.clone()
    }

    pub(in crate::workspace) fn challenge_for_current_step(
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

#[derive(Clone, Debug, PartialEq)]
pub(in crate::workspace) struct NativeSessionTreeConnectChallenge {
    pub(in crate::workspace) plan: NativeSessionTreeConnectPlan,
    pub(in crate::workspace) status: HostKeyStatus,
    pub(in crate::workspace) step: NativeSessionTreeConnectStep,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct PendingProxyConnectPlan {
    pub(in crate::workspace) plan: NativeSessionTreeConnectPlan,
}

impl PendingProxyConnectPlan {
    pub(in crate::workspace) fn new(plan: NativeSessionTreeConnectPlan) -> Self {
        Self { plan }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NativeSessionTreeConnectAction {
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

    #[test]
    fn session_tree_connect_plan_preserves_root_to_target_order() {
        let plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            vec![
                NativeSessionTreeConnectEndpoint::new("jump-a", 22),
                NativeSessionTreeConnectEndpoint::new("jump-b", 2200),
                NativeSessionTreeConnectEndpoint::new("target.internal", 2222),
            ],
            Some(node_id("target")),
        )
        .expect("valid plan");

        assert_eq!(
            plan.steps
                .iter()
                .map(|step| step.node_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["hop-1", "hop-2", "target"]
        );
        assert_eq!(
            plan.steps
                .iter()
                .map(|step| (step.host.as_str(), step.port))
                .collect::<Vec<_>>(),
            vec![("jump-a", 22), ("jump-b", 2200), ("target.internal", 2222)]
        );
    }

    #[test]
    fn session_tree_connect_plan_keeps_target_and_cleanup_node() {
        let plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            vec![
                NativeSessionTreeConnectEndpoint::new("jump-a", 22),
                NativeSessionTreeConnectEndpoint::new("jump-b", 22),
                NativeSessionTreeConnectEndpoint::new("target.internal", 22),
            ],
            Some(node_id("target")),
        )
        .expect("valid plan");

        assert_eq!(plan.target_node_id, node_id("target"));
        assert_eq!(plan.cleanup_node_id, Some(node_id("target")));
        assert_eq!(
            plan.steps.last().map(|step| &step.node_id),
            Some(&node_id("target"))
        );
        assert_eq!(plan.current_index, 0);
    }

    #[test]
    fn session_tree_connect_plan_cleanup_uses_cleanup_node_not_first_step() {
        let plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            vec![
                NativeSessionTreeConnectEndpoint::new("jump-a", 22),
                NativeSessionTreeConnectEndpoint::new("jump-b", 22),
                NativeSessionTreeConnectEndpoint::new("target.internal", 22),
            ],
            Some(node_id("target")),
        )
        .expect("valid plan");

        assert_eq!(plan.cleanup_root_node_id(), Some(node_id("target")));
        assert_ne!(plan.cleanup_root_node_id(), Some(node_id("hop-1")));
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
    fn session_tree_connect_step_separates_verified_preflight_from_accepted_fingerprint() {
        let step = NativeSessionTreeConnectStep {
            node_id: node_id("hop-1"),
            host: "jump-a".to_string(),
            port: 22,
            trust_host_key: Some(false),
            expected_host_key_fingerprint: None,
            preflight_verified: false,
        };
        assert!(!step.has_accepted_host_key());
        assert!(!step.can_connect_without_preflight());

        assert!(
            step.with_accepted_host_key(false, "SHA256:test")
                .has_accepted_host_key()
        );
    }

    #[test]
    fn session_tree_connect_plan_requests_preflight_before_unaccepted_step() {
        let plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            vec![
                NativeSessionTreeConnectEndpoint::new("jump-a", 22),
                NativeSessionTreeConnectEndpoint::new("jump-b", 22),
                NativeSessionTreeConnectEndpoint::new("target.internal", 22),
            ],
            Some(node_id("target")),
        )
        .expect("valid plan");

        match plan.next_action() {
            NativeSessionTreeConnectAction::Preflight { step } => {
                assert_eq!(step.node_id, node_id("hop-1"));
                assert_eq!(step.host, "jump-a");
            }
            action => panic!("unexpected action: {action:?}"),
        }
    }

    #[test]
    fn session_tree_connect_plan_connects_accepted_step_without_preflight() {
        let mut plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            vec![
                NativeSessionTreeConnectEndpoint::new("jump-a", 22),
                NativeSessionTreeConnectEndpoint::new("jump-b", 22),
                NativeSessionTreeConnectEndpoint::new("target.internal", 22),
            ],
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
            vec![
                NativeSessionTreeConnectEndpoint::new("jump-a", 22),
                NativeSessionTreeConnectEndpoint::new("jump-b", 22),
                NativeSessionTreeConnectEndpoint::new("target.internal", 22),
            ],
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

    #[test]
    fn session_tree_connect_plan_advances_to_complete_after_last_step() {
        let mut plan = NativeSessionTreeConnectPlan::from_expansion(
            &expansion(),
            vec![
                NativeSessionTreeConnectEndpoint::new("jump-a", 22),
                NativeSessionTreeConnectEndpoint::new("jump-b", 22),
                NativeSessionTreeConnectEndpoint::new("target.internal", 22),
            ],
            Some(node_id("target")),
        )
        .expect("valid plan");
        plan.advance_after_connected_step();
        plan.advance_after_connected_step();
        plan.advance_after_connected_step();

        assert_eq!(
            plan.next_action(),
            NativeSessionTreeConnectAction::Complete {
                target_node_id: node_id("target")
            }
        );
    }
}
