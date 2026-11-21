use crate::workspace::forwards::ForwardingRuntimeService;
use oxideterm_forwarding::{ForwardRule, ForwardStats, ForwardUpdate, PortDetectionSnapshot};
use oxideterm_public_mcp_host::{
    PublicMcpForwardMutation, PublicMcpForwardPort, PublicMcpForwardStopHandoff,
};
use oxideterm_ssh::NodeId;

#[async_trait::async_trait]
impl PublicMcpForwardPort for ForwardingRuntimeService {
    fn public_mcp_rules_for_node(&self, node_id: &NodeId) -> Vec<ForwardRule> {
        ForwardingRuntimeService::public_mcp_rules_for_node(self, node_id)
    }
    fn public_mcp_forward_is_persisted(&self, forward_id: &str) -> bool {
        ForwardingRuntimeService::public_mcp_forward_is_persisted(self, forward_id)
    }
    fn public_mcp_forward_owner_connection_id(&self, forward_id: &str) -> Option<String> {
        ForwardingRuntimeService::public_mcp_forward_owner_connection_id(self, forward_id)
    }
    async fn public_mcp_open_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        rule: ForwardRule,
        check_health: bool,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        ForwardingRuntimeService::public_mcp_open_forward(
            self,
            node_id,
            owner_connection_id,
            rule,
            check_health,
            persist,
        )
        .await
        .map(|mutation| PublicMcpForwardMutation {
            rule: mutation.rule,
            persisted: mutation.persisted,
        })
    }
    async fn public_mcp_change_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        expected_rule: &ForwardRule,
        update: ForwardUpdate,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        ForwardingRuntimeService::public_mcp_change_forward(
            self,
            node_id,
            owner_connection_id,
            forward_id,
            expected_rule,
            update,
            persist,
        )
        .await
        .map(|mutation| PublicMcpForwardMutation {
            rule: mutation.rule,
            persisted: mutation.persisted,
        })
    }
    async fn public_mcp_stop_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
    ) -> Result<(ForwardRule, PublicMcpForwardStopHandoff), String> {
        ForwardingRuntimeService::public_mcp_stop_forward(
            self,
            node_id,
            owner_connection_id,
            forward_id,
        )
        .await
    }
    async fn public_mcp_restart_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        ForwardingRuntimeService::public_mcp_restart_forward(
            self,
            node_id,
            owner_connection_id,
            forward_id,
            persist,
        )
        .await
        .map(|mutation| PublicMcpForwardMutation {
            rule: mutation.rule,
            persisted: mutation.persisted,
        })
    }
    async fn public_mcp_remove_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        remove_saved: bool,
    ) -> Result<bool, String> {
        ForwardingRuntimeService::public_mcp_remove_forward(
            self,
            node_id,
            owner_connection_id,
            forward_id,
            remove_saved,
        )
        .await
    }
    fn public_mcp_forward_stats(
        &self,
        node_id: &NodeId,
        forward_id: &str,
    ) -> Result<ForwardStats, String> {
        ForwardingRuntimeService::public_mcp_forward_stats(self, node_id, forward_id)
    }
    async fn public_mcp_discover_ports(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
    ) -> Result<PortDetectionSnapshot, String> {
        ForwardingRuntimeService::public_mcp_discover_ports(self, node_id, owner_connection_id)
            .await
    }
    async fn public_mcp_revoke_forward(&self, node_id: &NodeId, forward_id: &str, persisted: bool) {
        ForwardingRuntimeService::public_mcp_revoke_forward(self, node_id, forward_id, persisted)
            .await
    }
    async fn public_mcp_restore_forward_after_revocation(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        original_rule: &ForwardRule,
        revoked_rule: &ForwardRule,
        stop_handoff: Option<&PublicMcpForwardStopHandoff>,
        created_by_client: bool,
        persisted: bool,
    ) -> bool {
        ForwardingRuntimeService::public_mcp_restore_forward_after_revocation(
            self,
            node_id,
            owner_connection_id,
            original_rule,
            revoked_rule,
            stop_handoff,
            created_by_client,
            persisted,
        )
        .await
    }
}
