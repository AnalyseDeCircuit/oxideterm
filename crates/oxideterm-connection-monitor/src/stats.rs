// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::SystemTime;

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

/// Tauri-compatible connection state for `ssh_list_connection_summaries`.
///
/// The SSH registry owns the transport state. This crate keeps the UI-facing
/// projection separate so GPUI renders the same summary contract as Tauri
/// without deriving state from tabs, terminal panes, or visual selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPoolEntryState {
    Connecting,
    Active,
    Idle,
    LinkDown,
    Reconnecting,
    Disconnecting,
    Disconnected,
    Error(String),
}

impl ConnectionPoolEntryState {
    pub fn is_displayed_in_pool(&self) -> bool {
        !matches!(self, Self::Disconnected)
    }

    pub fn is_counted_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolConnectionSummarySnapshot {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub state: ConnectionPoolEntryState,
    pub ref_count: u64,
    pub keep_alive: bool,
    pub created_at: SystemTime,
    pub last_active_at: SystemTime,
    pub terminal_count: usize,
    pub has_sftp_session: bool,
    pub forward_count: usize,
    pub parent_connection_id: Option<String>,
}

/// UI-facing row/card payload for the Tauri `ConnectionsPanel`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionPoolEntrySummary {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub state: ConnectionPoolEntryState,
    pub ref_count: u64,
    pub keep_alive: bool,
    pub created_at: SystemTime,
    pub last_active_at: SystemTime,
    pub terminal_count: usize,
    pub has_sftp_session: bool,
    pub forward_count: usize,
    pub parent_connection_id: Option<String>,
}

impl ConnectionPoolEntrySummary {
    pub fn from_snapshot(snapshot: PoolConnectionSummarySnapshot) -> Self {
        Self {
            id: snapshot.id,
            host: snapshot.host,
            port: snapshot.port,
            username: snapshot.username,
            state: snapshot.state,
            ref_count: snapshot.ref_count,
            keep_alive: snapshot.keep_alive,
            created_at: snapshot.created_at,
            last_active_at: snapshot.last_active_at,
            terminal_count: snapshot.terminal_count,
            has_sftp_session: snapshot.has_sftp_session,
            forward_count: snapshot.forward_count,
            parent_connection_id: snapshot.parent_connection_id,
        }
    }

    pub fn is_displayed_in_pool(&self) -> bool {
        self.state.is_displayed_in_pool()
    }
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

    #[test]
    fn connection_summary_keeps_tauri_pool_fields() {
        let created_at = SystemTime::UNIX_EPOCH;
        let last_active_at = SystemTime::UNIX_EPOCH;

        let summary = ConnectionPoolEntrySummary::from_snapshot(PoolConnectionSummarySnapshot {
            id: "conn-1".into(),
            host: "example.com".into(),
            port: 22,
            username: "alice".into(),
            state: ConnectionPoolEntryState::Idle,
            ref_count: 3,
            keep_alive: true,
            created_at,
            last_active_at,
            terminal_count: 2,
            has_sftp_session: true,
            forward_count: 1,
            parent_connection_id: Some("jump".into()),
        });

        assert_eq!(summary.id, "conn-1");
        assert_eq!(summary.host, "example.com");
        assert_eq!(summary.port, 22);
        assert_eq!(summary.username, "alice");
        assert_eq!(summary.state, ConnectionPoolEntryState::Idle);
        assert_eq!(summary.ref_count, 3);
        assert!(summary.keep_alive);
        assert_eq!(summary.created_at, created_at);
        assert_eq!(summary.last_active_at, last_active_at);
        assert_eq!(summary.terminal_count, 2);
        assert!(summary.has_sftp_session);
        assert_eq!(summary.forward_count, 1);
        assert_eq!(summary.parent_connection_id.as_deref(), Some("jump"));
    }

    #[test]
    fn disconnected_summary_is_hidden_and_not_active() {
        assert!(!ConnectionPoolEntryState::Disconnected.is_displayed_in_pool());
        assert!(!ConnectionPoolEntryState::Disconnected.is_counted_active());
        assert!(!ConnectionPoolEntryState::Reconnecting.is_counted_active());
        assert!(!ConnectionPoolEntryState::LinkDown.is_counted_active());
        assert!(ConnectionPoolEntryState::Active.is_counted_active());
    }
}
