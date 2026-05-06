// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardType {
    Local,
    Remote,
    Dynamic,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardStatus {
    Starting,
    Active,
    Stopped,
    Error(String),
    Suspended,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRule {
    pub id: String,
    pub forward_type: ForwardType,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub status: ForwardStatus,
    pub description: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardStats {
    pub connection_count: u64,
    pub active_connections: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ForwardUpdate {
    pub forward_type: Option<ForwardType>,
    pub bind_address: Option<String>,
    pub bind_port: Option<u16>,
    pub target_host: Option<String>,
    pub target_port: Option<u16>,
    pub description: Option<String>,
}

impl ForwardRule {
    pub fn local(
        bind_address: impl Into<String>,
        bind_port: u16,
        target_host: impl Into<String>,
        target_port: u16,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            forward_type: ForwardType::Local,
            bind_address: bind_address.into(),
            bind_port,
            target_host: target_host.into(),
            target_port,
            status: ForwardStatus::Starting,
            description: String::new(),
        }
    }

    pub fn remote(
        bind_address: impl Into<String>,
        bind_port: u16,
        target_host: impl Into<String>,
        target_port: u16,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            forward_type: ForwardType::Remote,
            bind_address: bind_address.into(),
            bind_port,
            target_host: target_host.into(),
            target_port,
            status: ForwardStatus::Starting,
            description: String::new(),
        }
    }

    pub fn dynamic(bind_address: impl Into<String>, bind_port: u16) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            forward_type: ForwardType::Dynamic,
            bind_address: bind_address.into(),
            bind_port,
            target_host: "0.0.0.0".to_string(),
            target_port: 0,
            status: ForwardStatus::Starting,
            description: String::new(),
        }
    }

    pub fn apply_update(&mut self, update: ForwardUpdate) {
        if let Some(forward_type) = update.forward_type {
            self.forward_type = forward_type;
        }
        if let Some(bind_address) = update.bind_address {
            self.bind_address = bind_address;
        }
        if let Some(bind_port) = update.bind_port {
            self.bind_port = bind_port;
        }
        if let Some(target_host) = update.target_host {
            self.target_host = target_host;
        }
        if let Some(target_port) = update.target_port {
            self.target_port = target_port;
        }
        if let Some(description) = update.description {
            self.description = description;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_rule_matches_tauri_target_defaults() {
        let rule = ForwardRule::dynamic("localhost", 8080);

        assert_eq!(rule.forward_type, ForwardType::Dynamic);
        assert_eq!(rule.target_host, "0.0.0.0");
        assert_eq!(rule.target_port, 0);
        assert_eq!(rule.status, ForwardStatus::Starting);
    }
}
