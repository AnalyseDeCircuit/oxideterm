// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

const DEFAULT_FORWARD_HOST: &str = "localhost";

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
    Error,
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

fn normalize_forward_host(host: impl Into<String>) -> String {
    let host = host.into();
    let trimmed = host.trim();
    if trimmed.is_empty() {
        DEFAULT_FORWARD_HOST.to_string()
    } else {
        trimmed.to_string()
    }
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
            bind_address: normalize_forward_host(bind_address),
            bind_port,
            target_host: normalize_forward_host(target_host),
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
            bind_address: normalize_forward_host(bind_address),
            bind_port,
            target_host: normalize_forward_host(target_host),
            target_port,
            status: ForwardStatus::Starting,
            description: String::new(),
        }
    }

    pub fn dynamic(bind_address: impl Into<String>, bind_port: u16) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            forward_type: ForwardType::Dynamic,
            bind_address: normalize_forward_host(bind_address),
            bind_port,
            target_host: String::new(),
            target_port: 0,
            status: ForwardStatus::Starting,
            description: "SOCKS5 Proxy".to_string(),
        }
    }

    pub fn apply_update(&mut self, update: ForwardUpdate) {
        if let Some(forward_type) = update.forward_type {
            self.forward_type = forward_type;
        }
        if let Some(bind_address) = update.bind_address {
            self.bind_address = normalize_forward_host(bind_address);
        }
        if let Some(bind_port) = update.bind_port {
            self.bind_port = bind_port;
        }
        if let Some(target_host) = update.target_host {
            self.target_host = normalize_forward_host(target_host);
        }
        if let Some(target_port) = update.target_port {
            self.target_port = target_port;
        }
        if let Some(description) = update.description {
            self.description = description;
        }
    }

    pub(crate) fn normalize_hosts_for_runtime(&mut self) {
        self.bind_address = normalize_forward_host(std::mem::take(&mut self.bind_address));
        if self.forward_type != ForwardType::Dynamic {
            // SOCKS dynamic forwards pick the target per connection, so only
            // non-dynamic rules inherit Tauri's blank target host fallback.
            self.target_host = normalize_forward_host(std::mem::take(&mut self.target_host));
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
        assert_eq!(rule.target_host, "");
        assert_eq!(rule.target_port, 0);
        assert_eq!(rule.status, ForwardStatus::Starting);
        assert_eq!(rule.description, "SOCKS5 Proxy");
    }

    #[test]
    fn blank_forward_hosts_default_to_localhost() {
        let local = ForwardRule::local("  ", 8080, "\t", 3000);
        assert_eq!(local.bind_address, "localhost");
        assert_eq!(local.target_host, "localhost");

        let remote = ForwardRule::remote("", 8080, " service.internal ", 3000);
        assert_eq!(remote.bind_address, "localhost");
        assert_eq!(remote.target_host, "service.internal");

        let dynamic = ForwardRule::dynamic(" \n ", 1080);
        assert_eq!(dynamic.bind_address, "localhost");
        assert_eq!(dynamic.target_host, "");
    }

    #[test]
    fn forward_updates_default_blank_hosts() {
        let mut rule = ForwardRule::local("127.0.0.1", 8080, "example.test", 3000);
        rule.apply_update(ForwardUpdate {
            bind_address: Some(" ".to_string()),
            target_host: Some("\n".to_string()),
            ..ForwardUpdate::default()
        });

        assert_eq!(rule.bind_address, "localhost");
        assert_eq!(rule.target_host, "localhost");
    }

    #[test]
    fn runtime_normalization_handles_struct_literal_rules() {
        let mut rule = ForwardRule {
            id: "forward-1".to_string(),
            forward_type: ForwardType::Local,
            bind_address: String::new(),
            bind_port: 8080,
            target_host: "  ".to_string(),
            target_port: 3000,
            status: ForwardStatus::Starting,
            description: String::new(),
        };

        rule.normalize_hosts_for_runtime();

        assert_eq!(rule.bind_address, "localhost");
        assert_eq!(rule.target_host, "localhost");
    }
}
