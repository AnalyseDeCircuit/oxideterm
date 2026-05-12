// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use oxideterm_ssh::SshConnectionHandle;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinHandle,
};

use crate::{
    BridgeStatsRecorder, DEFAULT_FORWARD_IDLE_TIMEOUT, ForwardRule, ForwardStats, ForwardStatus,
    ForwardingError, bridge::bridge_tcp_to_ssh_stream, tauri_local_bind_error,
};

const FORWARD_STOP_GRACE_PERIOD: Duration = Duration::from_secs(5);

pub(crate) struct LocalForward {
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl LocalForward {
    pub(crate) async fn start(
        mut rule: ForwardRule,
        ssh_connection: SshConnectionHandle,
    ) -> Result<Self, ForwardingError> {
        validate_local_rule(&rule)?;
        let listener = TcpListener::bind((rule.bind_address.as_str(), rule.bind_port))
            .await
            .map_err(|error| tauri_local_bind_error(&rule.bind_address, rule.bind_port, error))?;
        let bound_addr = listener.local_addr()?;
        rule.bind_address = bound_addr.ip().to_string();
        rule.bind_port = bound_addr.port();
        rule.status = ForwardStatus::Active;

        let stats = BridgeStatsRecorder::default();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task_rule = rule.clone();
        let task_stats = stats.clone();
        let task = tokio::spawn(async move {
            accept_local_connections(listener, ssh_connection, task_rule, task_stats, shutdown_rx)
                .await;
        });

        Ok(Self {
            rule,
            stats,
            shutdown_tx,
            task,
        })
    }

    pub(crate) fn rule(&self) -> ForwardRule {
        self.rule.clone()
    }

    pub(crate) fn stats(&self) -> ForwardStats {
        self.stats.snapshot()
    }

    pub(crate) async fn stop(self) -> ForwardRule {
        let _ = self.shutdown_tx.send(true);
        let _ = self
            .stats
            .active_connections()
            .wait_zero(FORWARD_STOP_GRACE_PERIOD)
            .await;
        self.task.abort();
        let mut stopped = self.rule;
        stopped.status = ForwardStatus::Stopped;
        stopped
    }
}

async fn accept_local_connections(
    listener: TcpListener,
    ssh_connection: SshConnectionHandle,
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, origin_addr) = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        tracing::warn!("local forward {} accept failed: {error}", rule.id);
                        continue;
                    }
                };
                if let Err(error) = stream.set_nodelay(true) {
                    tracing::debug!("local forward {} failed to set TCP_NODELAY: {error}", rule.id);
                }
                let connection = ssh_connection.clone();
                let connection_rule = rule.clone();
                let connection_stats = stats.clone();
                let connection_shutdown = shutdown_rx.clone();
                tokio::spawn(async move {
                    if let Err(error) = bridge_local_connection(
                        stream,
                        connection,
                        connection_rule,
                        connection_stats,
                        connection_shutdown,
                        origin_addr.ip().to_string(),
                        origin_addr.port(),
                    )
                    .await
                    {
                        tracing::warn!("local forward connection failed: {error}");
                    }
                });
            }
        }
    }
}

async fn bridge_local_connection(
    stream: TcpStream,
    ssh_connection: SshConnectionHandle,
    rule: ForwardRule,
    stats: BridgeStatsRecorder,
    shutdown_rx: watch::Receiver<bool>,
    _origin_host: String,
    _origin_port: u16,
) -> Result<(), ForwardingError> {
    let ssh_stream = ssh_connection
        .open_direct_tcpip(&rule.target_host, rule.target_port, "127.0.0.1", 0)
        .await?;

    bridge_tcp_to_ssh_stream(
        stream,
        ssh_stream,
        stats,
        DEFAULT_FORWARD_IDLE_TIMEOUT,
        shutdown_rx,
        format!(
            "local forward {} {}:{} -> {}:{}",
            rule.id, rule.bind_address, rule.bind_port, rule.target_host, rule.target_port
        ),
    )
    .await
}

fn validate_local_rule(rule: &ForwardRule) -> Result<(), ForwardingError> {
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
