// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_connections::{ConnectionStore, SaveConnectionRequest};
use oxideterm_i18n::I18n;
use oxideterm_ssh::{
    NativeSessionTreeConnectAction, NativeSessionTreeConnectEndpoint, NativeSessionTreeConnectPlan,
    NodeId, NodeReadiness, NodeRouter, NodeTreeExpansion, SshConfig, UpstreamProxyConfig,
};

use crate::{
    conversion::proxy_session_tree_endpoints,
    entity::{ConnectionFlowEntity, NativeProxyConnectRun},
    launch::{SshConnectionIntent, SshTerminalConnectionOptions},
    saved_auth::SavedAuthSaveTarget,
    views::ConnectionsWindowRequest as Request,
};

pub const CHAIN_LOCK_BUSY: &str = "CHAIN_LOCK_BUSY: Another connection chain is in progress";

pub struct ProxyExpansion {
    endpoints: Vec<NativeSessionTreeConnectEndpoint>,
    title: String,
    intent: SshConnectionIntent,
    save_after_open: Option<SaveConnectionRequest>,
    upstream_proxy: Option<UpstreamProxyConfig>,
}

pub struct ReadyProxyTerminal {
    pub node_id: NodeId,
    pub config: SshConfig,
    pub title: String,
    pub saved_id: Option<String>,
    pub options: Option<SshTerminalConnectionOptions>,
    pub save_after_open: Option<SaveConnectionRequest>,
    pub open_sftp: bool,
}

impl ProxyExpansion {
    pub fn prepare(
        config: &mut SshConfig,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: Option<SaveConnectionRequest>,
    ) -> Self {
        Self {
            endpoints: proxy_session_tree_endpoints(config),
            title,
            intent,
            save_after_open,
            upstream_proxy: config.upstream_proxy.take(),
        }
    }

    pub fn expansion_id(&self, next_node_id: u64) -> String {
        match &self.intent {
            SshConnectionIntent::ConnectSaved { id, .. } => id.clone(),
            _ => format!("manual-{next_node_id}"),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

impl ConnectionFlowEntity {
    pub fn stage_proxy_expansion(
        &mut self,
        pending: ProxyExpansion,
        expansion: &NodeTreeExpansion,
        cx: &mut Context<Self>,
    ) -> Result<Option<SavedAuthSaveTarget>, String> {
        let ProxyExpansion {
            endpoints,
            title,
            mut intent,
            save_after_open,
            upstream_proxy,
        } = pending;
        let plan = NativeSessionTreeConnectPlan::from_expansion(
            expansion,
            endpoints,
            Some(expansion.target_node_id.clone()),
        )?;
        let consent = match &mut intent {
            SshConnectionIntent::ConnectSaved {
                auth_save_target, ..
            } => auth_save_target.take(),
            _ => None,
        };
        self.start_proxy_connect_run(
            NativeProxyConnectRun {
                generation: 0,
                plan,
                title,
                intent,
                save_after_open,
                upstream_proxy,
            },
            cx,
        )
        .map_err(|_| CHAIN_LOCK_BUSY.to_owned())?;
        Ok(consent)
    }

    pub fn stage_existing_proxy_chain(
        &mut self,
        router: &NodeRouter,
        target_node_id: NodeId,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: &mut Option<SaveConnectionRequest>,
        chain_locked: bool,
        nodes_locked: impl FnOnce(&[NodeId]) -> bool,
        endpoint: impl Fn(&NodeId) -> Option<NativeSessionTreeConnectEndpoint>,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.has_active_proxy_connect_run() || chain_locked {
            return Err(CHAIN_LOCK_BUSY.to_owned());
        }
        let path = router
            .path_to_node(&target_node_id)
            .map_err(|_| format!("Node path not found for {}", target_node_id.0))?;
        if path.len() <= 1 {
            return Ok(false);
        }
        let first_unready = path
            .iter()
            .position(|node| !ready(router, node))
            .unwrap_or(path.len());
        let remaining = &path[first_unready..];
        if remaining.is_empty() {
            return Ok(false);
        }
        if nodes_locked(remaining) {
            return Err("NODE_LOCK_BUSY: Node is already connecting".into());
        }
        let endpoints = remaining
            .iter()
            .map(|node| endpoint(node).ok_or_else(|| format!("SSH node {} not found", node.0)))
            .collect::<Result<Vec<_>, _>>()?;
        let upstream_proxy = router
            .node_runtime_snapshot(&remaining[0])
            .filter(|snapshot| snapshot.parent_id.is_none())
            .and_then(|snapshot| snapshot.config.upstream_proxy);
        let expansion = NodeTreeExpansion {
            target_node_id,
            path_node_ids: remaining.to_vec(),
            chain_depth: path.len() as u32,
        };
        let plan = NativeSessionTreeConnectPlan::from_expansion(&expansion, endpoints, None)?;
        self.start_proxy_connect_run(
            NativeProxyConnectRun {
                generation: 0,
                plan,
                title,
                intent,
                save_after_open: save_after_open.take(),
                upstream_proxy,
            },
            cx,
        )
        .map_err(|_| CHAIN_LOCK_BUSY.to_owned())?;
        Ok(true)
    }

    pub fn next_proxy_chain_request(
        &mut self,
        router: &NodeRouter,
        store: &ConnectionStore,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<Request> {
        loop {
            match self.proxy_connect_next_action()? {
                NativeSessionTreeConnectAction::Preflight { step } => {
                    let message = i18n.t("ssh.form.checking_host_key");
                    let inline = self.set_form_feedback(Some(true), Some(message.clone()), cx);
                    if let Some(context) = self.take_proxy_connect_preflight_context(cx) {
                        crate::workers::preflight_tree_step(
                            router.clone(),
                            step,
                            context,
                            self.ssh_worker_sender(),
                        );
                    }
                    return (!inline).then_some(Request::ConnectionStatus(Some(message)));
                }
                NativeSessionTreeConnectAction::Connect { step }
                    if ready(router, &step.node_id) =>
                {
                    if !self.advance_active_proxy_connect_after_node_connected(&step.node_id, cx) {
                        return None;
                    }
                }
                NativeSessionTreeConnectAction::Connect { step } => {
                    let proxy = self.take_active_proxy_upstream_proxy_for_node(&step.node_id, cx);
                    let changed = crate::preflight::apply_tree_step_preflight(router, &step, proxy);
                    return Some(Request::ConnectProxyNode {
                        node_id: step.node_id,
                        changed,
                    });
                }
                NativeSessionTreeConnectAction::Complete { target_node_id } => {
                    let run = self.take_active_proxy_connect_run(cx)?;
                    self.clear_host_key_challenge(cx);
                    let mut report = None;
                    let terminal = if let Some(config) = router
                        .node_runtime_snapshot(&target_node_id)
                        .map(|state| state.config)
                    {
                        let mut saved_id = None;
                        let (options, open_sftp, save_after_open) = match run.intent {
                            SshConnectionIntent::Connect(options) => {
                                let open_sftp = options.open_sftp_on_connect;
                                (Some(options), open_sftp, run.save_after_open)
                            }
                            SshConnectionIntent::ConnectSaved {
                                id,
                                open_sftp_on_connect,
                                ..
                            } => {
                                let options =
                                    store
                                        .get(&id)
                                        .map(|connection| SshTerminalConnectionOptions {
                                            terminal: connection.options.terminal.clone(),
                                            dedicated_new_terminal_connection: connection
                                                .options
                                                .dedicated_new_terminal_connection,
                                            open_sftp_on_connect,
                                            ssh_channel_strategy: connection
                                                .options
                                                .ssh_channel_strategy,
                                        });
                                saved_id = Some(id);
                                (options, open_sftp_on_connect, None)
                            }
                            _ => {
                                return Some(Request::FinishProxyChain {
                                    plan: run.plan,
                                    terminal: None,
                                    report: None,
                                });
                            }
                        };
                        self.clear_form_after_submit(cx);
                        Some(ReadyProxyTerminal {
                            node_id: target_node_id,
                            config,
                            title: run.title,
                            saved_id,
                            options,
                            save_after_open,
                            open_sftp,
                        })
                    } else {
                        report = Some("target node was not materialized".to_owned());
                        None
                    };
                    return Some(Request::FinishProxyChain {
                        plan: run.plan,
                        terminal,
                        report,
                    });
                }
            }
        }
    }
}

fn ready(router: &NodeRouter, node: &NodeId) -> bool {
    router
        .node_state(node)
        .is_ok_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready)
}
