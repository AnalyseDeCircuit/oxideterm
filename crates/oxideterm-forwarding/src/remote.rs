// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use dashmap::DashMap;
use oxideterm_ssh::{RemoteForwardHandler, RemoteForwardedTcpIp, SshConnectionHandle};
use tokio::{net::TcpStream, sync::watch};

use crate::{
    BridgeStatsRecorder, DEFAULT_FORWARD_IDLE_TIMEOUT, ForwardRule, ForwardStats, ForwardStatus,
    ForwardingError,
    bridge::{bridge_tcp_to_ssh_stream_with_existing_connection, wait_for_shutdown},
};

const FORWARD_STOP_GRACE_PERIOD: Duration = Duration::from_secs(5);
const REMOTE_TARGET_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct RemoteForward {
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    router: Arc<RemoteForwardRouter>,
    ssh_connection: SshConnectionHandle,
    shutdown_tx: watch::Sender<bool>,
}

impl RemoteForward {
    pub(crate) async fn start(
        mut rule: ForwardRule,
        ssh_connection: SshConnectionHandle,
        router: Arc<RemoteForwardRouter>,
    ) -> Result<Self, ForwardingError> {
        validate_remote_rule(&rule)?;
        ssh_connection.set_remote_forward_handler(router.clone())?;

        let actual_port = ssh_connection
            .request_remote_tcpip_forward(&rule.bind_address, rule.bind_port)
            .await?;

        let stats = BridgeStatsRecorder::default();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        rule.bind_port = actual_port;
        rule.status = ForwardStatus::Active;
        router.register(
            RemoteForwardKey {
                address: rule.bind_address.clone(),
                port: rule.bind_port,
            },
            RemoteForwardTarget {
                connection_id: ssh_connection.connection_id().to_string(),
                local_host: rule.target_host.clone(),
                local_port: rule.target_port,
                stats: stats.clone(),
                shutdown_rx,
            },
        );

        Ok(Self {
            rule,
            stats,
            router,
            ssh_connection,
            shutdown_tx,
        })
    }

    pub(crate) fn rule(&self) -> ForwardRule {
        self.rule.clone()
    }

    pub(crate) fn stats(&self) -> ForwardStats {
        self.stats.snapshot()
    }

    pub(crate) async fn cancel_on_server(&self) -> Result<(), ForwardingError> {
        self.ssh_connection
            .cancel_remote_tcpip_forward(&self.rule.bind_address, self.rule.bind_port)
            .await
            .map_err(ForwardingError::from)
    }

    pub(crate) async fn finish_stop(self) -> ForwardRule {
        let _ = self.shutdown_tx.send(true);
        self.router
            .unregister(&self.rule.bind_address, self.rule.bind_port);
        let _ = self
            .stats
            .active_connections()
            .wait_zero(FORWARD_STOP_GRACE_PERIOD)
            .await;

        let mut stopped = self.rule.clone();
        stopped.status = ForwardStatus::Stopped;
        stopped
    }

    pub(crate) async fn stop_best_effort(self) -> ForwardRule {
        // Stop local routing first so a slow server cancellation cannot admit
        // new forwarded channels during bounded session shutdown.
        let _ = self.shutdown_tx.send(true);
        self.router
            .unregister(&self.rule.bind_address, self.rule.bind_port);
        match tokio::time::timeout(FORWARD_STOP_GRACE_PERIOD, self.cancel_on_server()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(
                    "failed to cancel remote forward {}:{}: {error}",
                    self.rule.bind_address,
                    self.rule.bind_port
                );
            }
            Err(_) => {
                tracing::warn!(
                    "timed out cancelling remote forward {}:{}",
                    self.rule.bind_address,
                    self.rule.bind_port
                );
            }
        }
        self.finish_stop().await
    }
}

impl Drop for RemoteForward {
    fn drop(&mut self) {
        // Future cancellation must still unregister the synchronous router target.
        let _ = self.shutdown_tx.send(true);
        self.router
            .unregister(&self.rule.bind_address, self.rule.bind_port);
    }
}

#[derive(Clone, Debug)]
struct RemoteForwardTarget {
    connection_id: String,
    local_host: String,
    local_port: u16,
    stats: BridgeStatsRecorder,
    shutdown_rx: watch::Receiver<bool>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RemoteForwardRouter {
    targets: Arc<DashMap<RemoteForwardKey, RemoteForwardTarget>>,
}

impl RemoteForwardRouter {
    fn register(&self, key: RemoteForwardKey, target: RemoteForwardTarget) {
        self.targets.insert(key, target);
    }

    fn unregister(&self, remote_address: &str, remote_port: u16) {
        self.targets.remove(&RemoteForwardKey {
            address: remote_address.to_string(),
            port: remote_port,
        });
    }

    fn target_for(
        &self,
        remote_address: &str,
        remote_port: u16,
        connection_id: &str,
    ) -> Option<RemoteForwardTarget> {
        let key = RemoteForwardKey {
            address: remote_address.to_string(),
            port: remote_port,
        };
        self.targets
            .get(&key)
            .map(|target| target.clone())
            .filter(|target| target.connection_id == connection_id)
    }

    async fn handle(&self, event: RemoteForwardedTcpIp) {
        let Some(target) = self.target_for(
            &event.connected_address,
            event.connected_port,
            &event.connection_id,
        ) else {
            tracing::warn!(
                "no registered remote forward for {}:{} on connection {}",
                event.connected_address,
                event.connected_port,
                event.connection_id
            );
            return;
        };

        let _connection_guard = target.stats.start_connection();
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let connect_result = tokio::select! {
            biased;
            _ = wait_for_shutdown(target.shutdown_rx.clone()) => return,
            result = tokio::time::timeout(
                REMOTE_TARGET_CONNECT_TIMEOUT,
                TcpStream::connect(&local_addr),
            ) => result,
        };
        let local_stream = match connect_result {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => {
                tracing::warn!("failed to connect remote forward target {local_addr}: {error}");
                return;
            }
            Err(_) => {
                tracing::warn!("timed out connecting remote forward target {local_addr}");
                return;
            }
        };
        if let Err(error) = local_stream.set_nodelay(true) {
            tracing::debug!("failed to set TCP_NODELAY for remote forward target: {error}");
        }

        if let Err(error) = bridge_tcp_to_ssh_stream_with_existing_connection(
            local_stream,
            event.stream,
            target.stats,
            DEFAULT_FORWARD_IDLE_TIMEOUT,
            target.shutdown_rx,
            format!(
                "remote forward {}:{} from {}:{} -> {}",
                event.connected_address,
                event.connected_port,
                event.originator_address,
                event.originator_port,
                local_addr
            ),
        )
        .await
        {
            tracing::warn!("remote forward bridge failed: {error}");
        }
    }
}

impl RemoteForwardHandler for RemoteForwardRouter {
    fn handle_remote_forward(
        &self,
        event: RemoteForwardedTcpIp,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let router = self.clone();
        Box::pin(async move {
            router.handle(event).await;
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RemoteForwardKey {
    address: String,
    port: u16,
}

fn validate_remote_rule(rule: &ForwardRule) -> Result<(), ForwardingError> {
    if rule.bind_address.trim().is_empty() {
        return Err(ForwardingError::InvalidRule(
            "bind address is required".to_string(),
        ));
    }
    if rule.target_host.trim().is_empty() {
        return Err(ForwardingError::InvalidRule(
            "target host is required".to_string(),
        ));
    }
    if rule.target_port == 0 {
        return Err(ForwardingError::InvalidRule(
            "target port is required".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use oxideterm_ssh::{ConnectionConsumer, SshConfig, SshConnectionRegistry};

    use super::*;

    #[test]
    fn remote_router_keeps_other_ports_for_same_address() {
        let router = RemoteForwardRouter::default();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        router.register(
            RemoteForwardKey {
                address: "0.0.0.0".to_string(),
                port: 9000,
            },
            RemoteForwardTarget {
                connection_id: "connection-a".to_string(),
                local_host: "localhost".to_string(),
                local_port: 3000,
                stats: BridgeStatsRecorder::default(),
                shutdown_rx: shutdown_rx.clone(),
            },
        );
        router.register(
            RemoteForwardKey {
                address: "0.0.0.0".to_string(),
                port: 9001,
            },
            RemoteForwardTarget {
                connection_id: "connection-a".to_string(),
                local_host: "localhost".to_string(),
                local_port: 3001,
                stats: BridgeStatsRecorder::default(),
                shutdown_rx,
            },
        );

        router.unregister("0.0.0.0", 9000);

        assert!(router.targets.contains_key(&RemoteForwardKey {
            address: "0.0.0.0".to_string(),
            port: 9001,
        }));
        assert!(!router.targets.contains_key(&RemoteForwardKey {
            address: "0.0.0.0".to_string(),
            port: 9000,
        }));
    }

    #[test]
    fn remote_router_ignores_forwarded_tcpip_from_stale_connection() {
        let router = RemoteForwardRouter::default();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        router.register(
            RemoteForwardKey {
                address: "0.0.0.0".to_string(),
                port: 9000,
            },
            RemoteForwardTarget {
                connection_id: "new-connection".to_string(),
                local_host: "localhost".to_string(),
                local_port: 3000,
                stats: BridgeStatsRecorder::default(),
                shutdown_rx,
            },
        );

        assert!(
            router
                .target_for("0.0.0.0", 9000, "old-connection")
                .is_none()
        );
        assert!(
            router
                .target_for("0.0.0.0", 9000, "new-connection")
                .is_some()
        );
    }

    #[test]
    fn dropping_remote_forward_unregisters_router_target() {
        let router = Arc::new(RemoteForwardRouter::default());
        let ssh_connection = SshConnectionRegistry::default().acquire(
            SshConfig::password("remote.example", 22, "tester", "pw"),
            ConnectionConsumer::PortForward("node:a".into()),
        );
        let mut rule = ForwardRule::remote("0.0.0.0", 9000, "localhost", 3000);
        rule.status = ForwardStatus::Active;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        router.register(
            RemoteForwardKey {
                address: rule.bind_address.clone(),
                port: rule.bind_port,
            },
            RemoteForwardTarget {
                connection_id: ssh_connection.connection_id().to_string(),
                local_host: rule.target_host.clone(),
                local_port: rule.target_port,
                stats: BridgeStatsRecorder::default(),
                shutdown_rx,
            },
        );
        let forward = RemoteForward {
            rule,
            stats: BridgeStatsRecorder::default(),
            router: router.clone(),
            ssh_connection,
            shutdown_tx,
        };

        drop(forward);

        assert!(router.targets.is_empty());
    }
}
