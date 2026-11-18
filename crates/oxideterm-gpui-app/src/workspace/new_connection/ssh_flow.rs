use std::collections::HashMap;

use gpui::{App, Context, Window};
use oxideterm_connections::{
    MoshIpFamily as SavedMoshIpFamily, MoshPredictionMode, MoshUdpPortSelection as SavedMoshUdpPortSelection,
    SaveConnectionRequest,
};
use oxideterm_mosh::{MoshBootstrapConfig, MoshBootstrapContext};
use oxideterm_remote_desktop::RemoteDesktopSecret;
use oxideterm_session_adapter::{managed_key_resolver_from_store, ssh_config_from_saved_connection};
use oxideterm_ssh::{
    HostKeyStatus, NativeSessionTreeConnectEndpoint, NativeSessionTreeConnectPlan,
    NodeId, NodeReadiness, SshConfig,
};
use oxideterm_terminal::MoshTerminalConfig;

use super::{
    ConnectionFormState, NativeProxyConnectRun,
    form_state::{
        NewConnectionProxyHop, NewConnectionSubmitAction,
        NewConnectionTransport, SavedConnectionPromptAction,
    },
    ssh_connection_display_title,
};
use crate::workspace::{WorkspaceApp, WorkspaceSshNode, delivery::ActiveDeliverySender};
use oxideterm_gpui_connections::{
    conversion::{prepare_proxy_chain_test_config, prepare_tree_connect_config, x11_forward_policy},
    submission::SavedConnectionRuntimeHandoff,
};

pub(in crate::workspace) use oxideterm_gpui_connections::{
    launch::{
        MoshConnectionOptions, NativeSshPromptHandler, SshConnectionIntent,
        SshConnectionWorkerResult, SshTerminalConnectionOptions,
    },
    profiles::mosh_options_from_profile,
};

mod connect;
mod save;

fn sync_saved_connection_node_title_for_nodes(
    ssh_nodes: &mut HashMap<NodeId, WorkspaceSshNode>,
    saved_connection_id: &str,
    title: &str,
) -> bool {
    let title = ssh_connection_display_title(title);
    let mut changed = false;
    for node in ssh_nodes.values_mut() {
        if node.saved_connection_id.as_deref() != Some(saved_connection_id) {
            continue;
        }
        if node.title == title {
            continue;
        }
        // Only mirror saved display metadata. The live runtime config keeps
        // describing the already-created SSH node until the user reconnects.
        node.title.clone_from(&title);
        changed = true;
    }
    changed
}

impl WorkspaceApp {
    /// Borrows the entity-owned form state without copying secret drafts.
    pub(in crate::workspace) fn connection_form_state<'a>(
        &self,
        cx: &'a App,
    ) -> &'a super::ConnectionFormState {
        &self.connections.flow.read(cx).form
    }

    /// Applies one UI transition inside the form entity and schedules its repaint.
    pub(in crate::workspace) fn update_connection_form_state<R>(
        &self,
        cx: &mut App,
        update: impl FnOnce(&mut super::ConnectionFormState) -> R,
    ) -> R {
        self.connections.flow.update(cx, |connection_flow, cx| {
            let result = update(&mut connection_flow.form);
            cx.notify();
            result
        })
    }

    pub(in crate::workspace) fn open_new_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_connection_form(NewConnectionTransport::Ssh, None, window, cx);
    }

    pub(in crate::workspace) fn open_serial_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_connection_form(NewConnectionTransport::Serial, None, window, cx);
        self.refresh_serial_ports(cx);
    }

    pub(in crate::workspace) fn open_telnet_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_connection_form(NewConnectionTransport::Telnet, None, window, cx);
    }

    pub(in crate::workspace) fn open_drill_down_form(&mut self, parent: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.node_router.node_metadata(&parent).is_some_and(|node| node.readiness == NodeReadiness::Ready) {
            let message = format!("{}: {}", self.i18n.t("sessions.tree.actions.drill_in"), self.i18n.t("ssh.drill_down.parent_not_ready"));
            self.set_connection_status(Some(message), cx);
            return;
        }
        self.open_connection_form(NewConnectionTransport::Ssh, Some(parent), window, cx);
    }

    fn open_connection_form(
        &mut self, transport: NewConnectionTransport, parent: Option<NodeId>, window: &mut Window, cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        self.ssh_tunnels.form = None;
        self.connections.flow.update(cx, |flow, cx| {
            flow.begin_connection_form(transport, parent, self.settings_store.settings(), &self.i18n, cx);
        });
        self.focus_connection_form(window, cx);
    }

    pub(in crate::workspace) fn connect_saved_connection_as_next_hop(
        &mut self,
        parent_node_id: NodeId,
        saved_connection_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent_ready = self
            .node_router
            .node_metadata(&parent_node_id)
            .is_some_and(|snapshot| snapshot.readiness == NodeReadiness::Ready);
        if !parent_ready {
            self.report_saved_next_hop_error("sessions.saved_next_hop.parent_not_ready", cx);
            return;
        }

        let Some(connection) = self.connection_store.get(&saved_connection_id).cloned() else {
            self.report_saved_next_hop_error("sessions.saved_next_hop.not_found", cx);
            return;
        };
        let Some(mut config) = ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
        ) else {
            self.report_saved_next_hop_error("sessions.saved_next_hop.missing_credentials", cx);
            return;
        };
        if let Err(error) = prepare_tree_connect_config(&mut config) {
            self.report_saved_next_hop_message(error, cx);
            return;
        }

        // Saved next-hop reuse still belongs to the native SessionTree path:
        // materialize the saved target under the live parent, then let
        // NodeRouter connect through that ancestry.
        let expansion = match self.expand_saved_connection_tree_under_parent(
            parent_node_id,
            &saved_connection_id,
            config,
            connection.name,
        ) {
            Ok(expansion) => expansion,
            Err(error) => {
                let message = format!(
                    "{}: {error}",
                    self.i18n.t("sessions.saved_next_hop.materialize_failed")
                );
                self.report_saved_next_hop_message(message, cx);
                return;
            }
        };

        let target_node_id = expansion.target_node_id;
        if let Some(node) = self.ssh_nodes.get_mut(&target_node_id) {
            node.readiness = NodeReadiness::Connecting;
        }
        self.active_ssh_node_id = Some(target_node_id.clone());
        self.connections.flow.update(cx, |connection_flow, cx| {
            connection_flow.clear_host_key_challenge(cx);
        });
        self.update_connection_form_state(cx, ConnectionFormState::clear);
        let message = self.i18n.t("ssh.drill_down.connecting");
        self.connections.session_manager.update(cx, |session_manager, cx| {
            session_manager.set_status(Some(message), cx);
        });
        self.ensure_node_connection_started(&target_node_id, cx);
        let _ = self.connection_store.mark_used(&saved_connection_id);
        self.persist_session_tree_snapshot();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_connection_title_sync_updates_only_matching_nodes() {
        let mut nodes = HashMap::from([
            (
                NodeId::new("node-home"),
                WorkspaceSshNode::new(
                    Some("home".to_string()),
                    &SshConfig {
                        host: "100.118.61.75".to_string(),
                        ..SshConfig::default()
                    },
                    "Old Home".to_string(),
                    Vec::new(),
                    NodeReadiness::Ready,
                ),
            ),
            (
                NodeId::new("node-prod"),
                WorkspaceSshNode::new(
                    Some("prod".to_string()),
                    &SshConfig {
                        host: "prod.example.com".to_string(),
                        ..SshConfig::default()
                    },
                    "Production".to_string(),
                    Vec::new(),
                    NodeReadiness::Ready,
                ),
            ),
        ]);

        assert!(sync_saved_connection_node_title_for_nodes(
            &mut nodes,
            "home",
            "Renamed\tHome"
        ));

        let home = nodes.get(&NodeId::new("node-home")).unwrap();
        let prod = nodes.get(&NodeId::new("node-prod")).unwrap();
        assert_eq!(home.title, "Renamed Home");
        assert_eq!(home.endpoint.host, "100.118.61.75");
        assert_eq!(prod.title, "Production");
    }
}
