// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::ConnectionTopologyStatus;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopologyViewStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed,
    Pending,
}

impl TopologyViewStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TopologyViewStatus::Connected => "connected",
            TopologyViewStatus::Connecting => "connecting",
            TopologyViewStatus::Disconnected => "disconnected",
            TopologyViewStatus::Failed => "failed",
            TopologyViewStatus::Pending => "pending",
        }
    }

    pub fn is_connected(self) -> bool {
        matches!(self, TopologyViewStatus::Connected)
    }

    pub fn is_down(self) -> bool {
        matches!(
            self,
            TopologyViewStatus::Disconnected | TopologyViewStatus::Failed
        )
    }

    pub fn is_connecting(self) -> bool {
        matches!(self, TopologyViewStatus::Connecting)
    }
}

pub fn matrix_visible(status: ConnectionTopologyStatus) -> bool {
    matches!(
        status,
        ConnectionTopologyStatus::Active
            | ConnectionTopologyStatus::Idle
            | ConnectionTopologyStatus::Connecting
            | ConnectionTopologyStatus::Reconnecting
    )
}

pub fn matrix_view_status(status: ConnectionTopologyStatus) -> TopologyViewStatus {
    match status {
        ConnectionTopologyStatus::Active | ConnectionTopologyStatus::Idle => {
            TopologyViewStatus::Connected
        }
        ConnectionTopologyStatus::Connecting | ConnectionTopologyStatus::Reconnecting => {
            TopologyViewStatus::Connecting
        }
        ConnectionTopologyStatus::LinkDown | ConnectionTopologyStatus::Error => {
            TopologyViewStatus::Failed
        }
        ConnectionTopologyStatus::Disconnected | ConnectionTopologyStatus::Disconnecting => {
            TopologyViewStatus::Disconnected
        }
        ConnectionTopologyStatus::Unknown => TopologyViewStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_visibility_matches_tauri_connected_connecting_filter() {
        assert!(matrix_visible(ConnectionTopologyStatus::Active));
        assert!(matrix_visible(ConnectionTopologyStatus::Idle));
        assert!(matrix_visible(ConnectionTopologyStatus::Connecting));
        assert!(matrix_visible(ConnectionTopologyStatus::Reconnecting));
        assert!(!matrix_visible(ConnectionTopologyStatus::LinkDown));
        assert!(!matrix_visible(ConnectionTopologyStatus::Disconnected));
        assert!(!matrix_visible(ConnectionTopologyStatus::Error));
    }
}
