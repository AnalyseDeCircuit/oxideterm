use oxideterm_forwarding::{ForwardRule, ForwardStats, ForwardUpdate, PortDetectionSnapshot};
use oxideterm_ssh::{ConnectionConsumerLease, NodeId};

pub struct PublicMcpForwardMutation {
    pub rule: ForwardRule,
    pub persisted: bool,
}

/// Holds the last listener's transport through the grant check and any rollback.
pub struct PublicMcpForwardStopHandoff {
    pub revision: u64,
    pub connection: ConnectionConsumerLease,
}

/// Node-owned forwarding stays in the runtime service. This port never looks up panes.
#[async_trait::async_trait]
pub trait PublicMcpForwardPort: Send + Sync {
    fn public_mcp_rules_for_node(&self, node_id: &NodeId) -> Vec<ForwardRule>;
    fn public_mcp_forward_is_persisted(&self, forward_id: &str) -> bool;
    fn public_mcp_forward_owner_connection_id(&self, forward_id: &str) -> Option<String>;
    async fn public_mcp_open_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        rule: ForwardRule,
        check_health: bool,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String>;
    async fn public_mcp_change_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        expected_rule: &ForwardRule,
        update: ForwardUpdate,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String>;
    async fn public_mcp_stop_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
    ) -> Result<(ForwardRule, PublicMcpForwardStopHandoff), String>;
    async fn public_mcp_restart_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String>;
    async fn public_mcp_remove_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        remove_saved: bool,
    ) -> Result<bool, String>;
    fn public_mcp_forward_stats(
        &self,
        node_id: &NodeId,
        forward_id: &str,
    ) -> Result<ForwardStats, String>;
    async fn public_mcp_discover_ports(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
    ) -> Result<PortDetectionSnapshot, String>;
    async fn public_mcp_revoke_forward(&self, node_id: &NodeId, forward_id: &str, persisted: bool);
    async fn public_mcp_restore_forward_after_revocation(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        original_rule: &ForwardRule,
        revoked_rule: &ForwardRule,
        stop_handoff: Option<&PublicMcpForwardStopHandoff>,
        created_by_client: bool,
        persisted: bool,
    ) -> bool;
}

#[derive(Clone, Copy)]
pub enum PublicMcpHostEffect {
    SavedForwardsChanged,
}
