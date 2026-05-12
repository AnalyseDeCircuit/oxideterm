// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionMonitorConsumerKind {
    Terminal,
    Sftp,
    PortForward,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolConnectionMonitorSnapshot {
    pub is_active: bool,
    pub is_idle: bool,
    pub is_reconnecting: bool,
    pub is_link_down: bool,
    pub ref_count: u64,
    pub has_sftp_session: bool,
    pub consumers: Vec<ConnectionMonitorConsumerKind>,
}

/// Tauri-compatible `ssh_get_pool_stats` payload.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPoolMonitorStats {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub reconnecting_connections: usize,
    pub link_down_connections: usize,
    pub total_terminals: usize,
    pub total_sftp_sessions: usize,
    pub total_forwards: usize,
    pub total_ref_count: u32,
    pub pool_capacity: usize,
    pub idle_timeout_secs: u64,
}

impl ConnectionPoolMonitorStats {
    pub fn from_snapshots(
        snapshots: impl IntoIterator<Item = PoolConnectionMonitorSnapshot>,
        pool_capacity: usize,
        idle_timeout_secs: u64,
    ) -> Self {
        let mut stats = Self {
            pool_capacity,
            idle_timeout_secs,
            ..Self::default()
        };

        for snapshot in snapshots {
            stats.total_connections += 1;
            if snapshot.is_active {
                stats.active_connections += 1;
            }
            if snapshot.is_idle {
                stats.idle_connections += 1;
            }
            if snapshot.is_reconnecting {
                stats.reconnecting_connections += 1;
            }
            if snapshot.is_link_down {
                stats.link_down_connections += 1;
            }

            for consumer in snapshot.consumers {
                match consumer {
                    ConnectionMonitorConsumerKind::Terminal => stats.total_terminals += 1,
                    ConnectionMonitorConsumerKind::PortForward => stats.total_forwards += 1,
                    ConnectionMonitorConsumerKind::Sftp | ConnectionMonitorConsumerKind::Other => {}
                }
            }

            if snapshot.has_sftp_session {
                stats.total_sftp_sessions += 1;
            }
            stats.total_ref_count = stats
                .total_ref_count
                .saturating_add(snapshot.ref_count.min(u32::MAX as u64) as u32);
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tauri_shaped_pool_stats_from_snapshots() {
        let stats = ConnectionPoolMonitorStats::from_snapshots(
            [
                PoolConnectionMonitorSnapshot {
                    is_active: true,
                    is_idle: false,
                    is_reconnecting: false,
                    is_link_down: false,
                    ref_count: 3,
                    has_sftp_session: true,
                    consumers: vec![
                        ConnectionMonitorConsumerKind::Terminal,
                        ConnectionMonitorConsumerKind::Terminal,
                        ConnectionMonitorConsumerKind::Sftp,
                        ConnectionMonitorConsumerKind::PortForward,
                    ],
                },
                PoolConnectionMonitorSnapshot {
                    is_active: false,
                    is_idle: true,
                    is_reconnecting: false,
                    is_link_down: false,
                    ref_count: 1,
                    has_sftp_session: false,
                    consumers: Vec::new(),
                },
                PoolConnectionMonitorSnapshot {
                    is_active: false,
                    is_idle: false,
                    is_reconnecting: true,
                    is_link_down: false,
                    ref_count: u64::MAX,
                    has_sftp_session: false,
                    consumers: vec![ConnectionMonitorConsumerKind::PortForward],
                },
            ],
            9,
            120,
        );

        assert_eq!(stats.total_connections, 3);
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.idle_connections, 1);
        assert_eq!(stats.reconnecting_connections, 1);
        assert_eq!(stats.link_down_connections, 0);
        assert_eq!(stats.total_terminals, 2);
        assert_eq!(stats.total_sftp_sessions, 1);
        assert_eq!(stats.total_forwards, 2);
        assert_eq!(stats.total_ref_count, u32::MAX);
        assert_eq!(stats.pool_capacity, 9);
        assert_eq!(stats.idle_timeout_secs, 120);
    }
}
