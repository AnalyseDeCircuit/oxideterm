// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use dashmap::DashMap;
use oxideterm_ssh::{RemoteForwardHandler, RemoteForwardedTcpIp, SshConnectionHandle};
use tokio::{net::TcpStream, sync::watch};

use crate::{
    BridgeStatsRecorder, DEFAULT_FORWARD_IDLE_TIMEOUT, ForwardRule, ForwardStats, ForwardStatus,
    ForwardingError, bridge::bridge_tcp_to_ssh_stream_with_existing_connection,
};

const FORWARD_STOP_GRACE_PERIOD: Duration = Duration::from_secs(5);

pub(crate) struct RemoteForward {
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    router: Arc<RemoteForwardRouter>,
    ssh_connection: SshConnectionHandle,
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
        rule.bind_port = actual_port;
        rule.status = ForwardStatus::Active;
        router.register(
            rule.bind_address.clone(),
            rule.bind_port,
            rule.target_host.clone(),
            rule.target_port,
            stats.clone(),
        );

        Ok(Self {
            rule,
            stats,
            router,
            ssh_connection,
        })
    }

    pub(crate) fn rule(&self) -> ForwardRule {
        self.rule.clone()
    }

    pub(crate) fn stats(&self) -> ForwardStats {
        self.stats.snapshot()
    }

    pub(crate) async fn stop(self) -> ForwardRule {
        if let Err(error) = self
            .ssh_connection
            .cancel_remote_tcpip_forward(&self.rule.bind_address, self.rule.bind_port)
            .await
        {
            tracing::warn!(
                "failed to cancel remote forward {}:{}: {error}",
                self.rule.bind_address,
                self.rule.bind_port
            );
        }
        self.router
            .unregister(&self.rule.bind_address, self.rule.bind_port);
        let _ = self
            .stats
            .active_connections()
            .wait_zero(FORWARD_STOP_GRACE_PERIOD)
            .await;

        let mut stopped = self.rule;
        stopped.status = ForwardStatus::Stopped;
        stopped
    }
}

#[derive(Clone, Debug)]
struct RemoteForwardTarget {
    local_host: String,
    local_port: u16,
    stats: BridgeStatsRecorder,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RemoteForwardRouter {
    targets: Arc<DashMap<RemoteForwardKey, RemoteForwardTarget>>,
}

impl RemoteForwardRouter {
    fn register(
        &self,
        remote_address: String,
        remote_port: u16,
        local_host: String,
        local_port: u16,
        stats: BridgeStatsRecorder,
    ) {
        self.targets.insert(
            RemoteForwardKey {
                address: remote_address,
                port: remote_port,
            },
            RemoteForwardTarget {
                local_host,
                local_port,
                stats,
            },
        );
    }

    fn unregister(&self, remote_address: &str, remote_port: u16) {
        self.targets.remove(&RemoteForwardKey {
            address: remote_address.to_string(),
            port: remote_port,
        });
    }

    async fn handle(&self, event: RemoteForwardedTcpIp) {
        let key = RemoteForwardKey {
            address: event.connected_address.clone(),
            port: event.connected_port,
        };
        let Some(target) = self.targets.get(&key).map(|target| target.clone()) else {
            tracing::warn!(
                "no registered remote forward for {}:{}",
                event.connected_address,
                event.connected_port
            );
            return;
        };

        let _connection_guard = target.stats.start_connection();
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let Ok(local_stream) = TcpStream::connect(&local_addr).await else {
            tracing::warn!("failed to connect remote forward target {local_addr}");
            return;
        };
        if let Err(error) = local_stream.set_nodelay(true) {
            tracing::debug!("failed to set TCP_NODELAY for remote forward target: {error}");
        }

        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        if let Err(error) = bridge_tcp_to_ssh_stream_with_existing_connection(
            local_stream,
            event.stream,
            target.stats,
            DEFAULT_FORWARD_IDLE_TIMEOUT,
            shutdown_rx,
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
    use super::*;

    #[test]
    fn remote_router_keeps_other_ports_for_same_address() {
        let router = RemoteForwardRouter::default();
        router.register(
            "0.0.0.0".to_string(),
            9000,
            "localhost".to_string(),
            3000,
            BridgeStatsRecorder::default(),
        );
        router.register(
            "0.0.0.0".to_string(),
            9001,
            "localhost".to_string(),
            3001,
            BridgeStatsRecorder::default(),
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
}
