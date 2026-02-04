//! SSH Config Parser (Enhanced for HPC)
//!
//! Parses ~/.ssh/config to import existing SSH hosts.
//! Supports:
//! - Basic: Host, HostName, User, Port, IdentityFile
//! - ProxyJump: Multi-hop jump hosts
//! - Port Forwarding: LocalForward, RemoteForward, DynamicForward

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

/// Port forwarding rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForwardRule {
    /// Local bind address (default: localhost)
    pub bind_address: String,
    /// Local port
    pub local_port: u16,
    /// Remote host
    pub remote_host: String,
    /// Remote port
    pub remote_port: u16,
}

impl PortForwardRule {
    /// Parse from SSH config format: "[bind_address:]port host:hostport"
    pub fn parse(value: &str) -> Option<Self> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }

        // Parse local part: [bind_address:]port
        let (bind_address, local_port) = if parts[0].contains(':') {
            let local_parts: Vec<&str> = parts[0].rsplitn(2, ':').collect();
            if local_parts.len() == 2 {
                (local_parts[1].to_string(), local_parts[0].parse().ok()?)
            } else {
                return None;
            }
        } else {
            ("localhost".to_string(), parts[0].parse().ok()?)
        };

        // Parse remote part: host:hostport
        let remote_parts: Vec<&str> = parts[1].rsplitn(2, ':').collect();
        if remote_parts.len() != 2 {
            return None;
        }

        Some(PortForwardRule {
            bind_address,
            local_port,
            remote_host: remote_parts[1].to_string(),
            remote_port: remote_parts[0].parse().ok()?,
        })
    }
}

/// Proxy jump host (for ProxyJump directive)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyJumpHost {
    /// Username (optional, inherits from main config if not specified)
    pub user: Option<String>,
    /// Hostname
    pub host: String,
    /// Port (default: 22)
    pub port: u16,
}

impl ProxyJumpHost {
    /// Parse from SSH config format: "[user@]host[:port]"
    pub fn parse(value: &str) -> Option<Self> {
        let (user, host_port) = if value.contains('@') {
            let parts: Vec<&str> = value.splitn(2, '@').collect();
            (Some(parts[0].to_string()), parts[1])
        } else {
            (None, value)
        };

        let (host, port) = if host_port.contains(':') {
            let parts: Vec<&str> = host_port.rsplitn(2, ':').collect();
            (parts[1].to_string(), parts[0].parse().unwrap_or(22))
        } else {
            (host_port.to_string(), 22)
        };

        Some(ProxyJumpHost { user, host, port })
    }
}

/// A parsed SSH config host entry (Enhanced)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SshConfigHost {
    /// Host alias (the pattern after "Host")
    pub alias: String,
    /// Actual hostname (HostName directive)
    pub hostname: Option<String>,
    /// Username (User directive)
    pub user: Option<String>,
    /// Port number (Port directive)
    pub port: Option<u16>,
    /// Identity file path (IdentityFile directive)
    pub identity_file: Option<String>,

    /// Certificate file path (CertificateFile directive)
    pub certificate_file: Option<String>,

    /// ProxyJump chain (parsed from ProxyJump directive)
    #[serde(default)]
    pub proxy_jump: Vec<ProxyJumpHost>,

    /// ProxyCommand (alternative to ProxyJump)
    pub proxy_command: Option<String>,

    /// Local port forwards
    #[serde(default)]
    pub local_forwards: Vec<PortForwardRule>,

    /// Remote port forwards
    #[serde(default)]
    pub remote_forwards: Vec<PortForwardRule>,

    /// Dynamic forward port (SOCKS proxy)
    pub dynamic_forward: Option<u16>,

    /// Other directives we don't directly use
    #[serde(default)]
    pub other: HashMap<String, String>,
}

impl SshConfigHost {
    /// Get the effective hostname (hostname or alias)
    pub fn effective_hostname(&self) -> &str {
        self.hostname.as_deref().unwrap_or(&self.alias)
    }

    /// Get effective port (port or 22)
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(22)
    }

    /// Check if this is a wildcard pattern
    pub fn is_wildcard(&self) -> bool {
        self.alias.contains('*') || self.alias.contains('?')
    }

    /// Check if this host requires a proxy jump
    pub fn has_proxy_jump(&self) -> bool {
        !self.proxy_jump.is_empty()
    }

    /// Check if this host has any port forwards configured
    pub fn has_port_forwards(&self) -> bool {
        !self.local_forwards.is_empty()
            || !self.remote_forwards.is_empty()
            || self.dynamic_forward.is_some()
    }

    /// Get proxy jump chain description (for UI display)
    pub fn proxy_jump_description(&self) -> Option<String> {
        if self.proxy_jump.is_empty() {
            return None;
        }

        let hops: Vec<String> = self
            .proxy_jump
            .iter()
            .map(|hop| {
                if let Some(ref user) = hop.user {
                    format!("{}@{}:{}", user, hop.host, hop.port)
                } else {
                    format!("{}:{}", hop.host, hop.port)
                }
            })
            .collect();

        Some(hops.join(" → "))
    }
}

/// SSH config parser errors
#[derive(Debug, thiserror::Error)]
pub enum SshConfigError {
    #[error("Failed to determine home directory")]
    NoHomeDir,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
}

/// Get default SSH config path
pub fn default_ssh_config_path() -> Result<PathBuf, SshConfigError> {
    dirs::home_dir()
        .map(|home| home.join(".ssh").join("config"))
        .ok_or(SshConfigError::NoHomeDir)
}

/// Parse SSH config file
pub async fn parse_ssh_config(path: Option<PathBuf>) -> Result<Vec<SshConfigHost>, SshConfigError> {
    let path = match path {
        Some(p) => p,
        None => default_ssh_config_path()?,
    };

    let content = match fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(e) => return Err(SshConfigError::Io(e)),
    };

    parse_ssh_config_content(&content)
}

/// Parse SSH config content string
pub fn parse_ssh_config_content(content: &str) -> Result<Vec<SshConfigHost>, SshConfigError> {
    let mut hosts = Vec::new();
    let mut current_host: Option<SshConfigHost> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse "Key Value" or "Key=Value"
        let (key, value) = if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            let value = line[eq_pos + 1..].trim();
            (key, value)
        } else {
            let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
            if parts.len() < 2 {
                continue; // Skip malformed lines
            }
            (parts[0], parts[1].trim())
        };

        let key_lower = key.to_lowercase();

        if key_lower == "host" {
            // Save previous host if exists
            if let Some(host) = current_host.take() {
                if !host.is_wildcard() {
                    hosts.push(host);
                }
            }

            // Handle multiple hosts on same line (e.g., "Host foo bar")
            for alias in value.split_whitespace() {
                // For now, we only take the first non-wildcard host
                if !alias.contains('*') && !alias.contains('?') {
                    current_host = Some(SshConfigHost {
                        alias: alias.to_string(),
                        ..Default::default()
                    });
                    break;
                }
            }
        } else if let Some(ref mut host) = current_host {
            match key_lower.as_str() {
                "hostname" => host.hostname = Some(value.to_string()),
                "user" => host.user = Some(value.to_string()),
                "port" => {
                    host.port = value.parse().ok();
                }
                "identityfile" => {
                    // Expand ~ to home directory
                    let expanded = if let Some(stripped) = value.strip_prefix("~/") {
                        if let Some(home) = dirs::home_dir() {
                            home.join(stripped).to_string_lossy().into_owned()
                        } else {
                            value.to_string()
                        }
                    } else {
                        value.to_string()
                    };
                    host.identity_file = Some(expanded);
                }
                "certificatefile" => {
                    // Expand ~ to home directory (same logic as IdentityFile)
                    let expanded = if let Some(stripped) = value.strip_prefix("~/") {
                        if let Some(home) = dirs::home_dir() {
                            home.join(stripped).to_string_lossy().into_owned()
                        } else {
                            value.to_string()
                        }
                    } else {
                        value.to_string()
                    };
                    host.certificate_file = Some(expanded);
                }
                // ProxyJump: can be comma-separated for multi-hop
                "proxyjump" => {
                    if value.to_lowercase() != "none" {
                        for jump in value.split(',') {
                            if let Some(proxy_host) = ProxyJumpHost::parse(jump.trim()) {
                                host.proxy_jump.push(proxy_host);
                            }
                        }
                    }
                }
                // ProxyCommand (alternative to ProxyJump)
                "proxycommand" => {
                    if value.to_lowercase() != "none" {
                        host.proxy_command = Some(value.to_string());
                    }
                }
                // LocalForward: [bind_address:]port host:hostport
                "localforward" => {
                    if let Some(rule) = PortForwardRule::parse(value) {
                        host.local_forwards.push(rule);
                    }
                }
                // RemoteForward: [bind_address:]port host:hostport
                "remoteforward" => {
                    if let Some(rule) = PortForwardRule::parse(value) {
                        host.remote_forwards.push(rule);
                    }
                }
                // DynamicForward: [bind_address:]port
                "dynamicforward" => {
                    let port_str = if value.contains(':') {
                        value.rsplit(':').next().unwrap_or(value)
                    } else {
                        value
                    };
                    host.dynamic_forward = port_str.parse().ok();
                }
                _ => {} // Ignore other directives
            }
        }
    }

    // Don't forget the last host
    if let Some(host) = current_host {
        if !host.is_wildcard() {
            hosts.push(host);
        }
    }

    Ok(hosts)
}

/// Filter hosts suitable for import (non-wildcard, has hostname or is valid)
pub fn filter_importable_hosts(hosts: Vec<SshConfigHost>) -> Vec<SshConfigHost> {
    hosts.into_iter().filter(|h| !h.is_wildcard()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let content = r#"
# Comment
Host myserver
    HostName example.com
    User admin
    Port 2222
    IdentityFile ~/.ssh/id_rsa

Host otherserver
    HostName other.com
    User root
"#;

        let hosts = parse_ssh_config_content(content).unwrap();
        assert_eq!(hosts.len(), 2);

        assert_eq!(hosts[0].alias, "myserver");
        assert_eq!(hosts[0].hostname, Some("example.com".to_string()));
        assert_eq!(hosts[0].user, Some("admin".to_string()));
        assert_eq!(hosts[0].port, Some(2222));
        assert!(hosts[0].identity_file.is_some());

        assert_eq!(hosts[1].alias, "otherserver");
        assert_eq!(hosts[1].effective_port(), 22);
    }

    #[test]
    fn test_skip_wildcards() {
        let content = r#"
Host *
    ServerAliveInterval 60
    
Host dev-*
    User developer
    
Host prod
    HostName prod.example.com
"#;

        let hosts = parse_ssh_config_content(content).unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "prod");
    }

    #[test]
    fn test_effective_values() {
        let host = SshConfigHost {
            alias: "myhost".to_string(),
            hostname: None,
            port: None,
            ..Default::default()
        };

        assert_eq!(host.effective_hostname(), "myhost");
        assert_eq!(host.effective_port(), 22);
    }

    #[test]
    fn test_parse_proxy_jump() {
        let content = r#"
Host hpc
    HostName login.hpc.edu.cn
    User zhangsan
    ProxyJump bastion

Host bastion
    HostName jump.school.edu.cn
    User zhangsan
    IdentityFile ~/.ssh/id_ed25519
"#;

        let hosts = parse_ssh_config_content(content).unwrap();
        assert_eq!(hosts.len(), 2);

        // HPC host with ProxyJump
        assert_eq!(hosts[0].alias, "hpc");
        assert!(hosts[0].has_proxy_jump());
        assert_eq!(hosts[0].proxy_jump.len(), 1);
        assert_eq!(hosts[0].proxy_jump[0].host, "bastion");
        assert_eq!(hosts[0].proxy_jump[0].port, 22);

        // Bastion host (no proxy)
        assert_eq!(hosts[1].alias, "bastion");
        assert!(!hosts[1].has_proxy_jump());
    }

    #[test]
    fn test_parse_multi_hop_proxy() {
        let content = r#"
Host compute
    HostName node001.internal
    User admin
    ProxyJump bastion,hpc
"#;

        let hosts = parse_ssh_config_content(content).unwrap();
        assert_eq!(hosts[0].proxy_jump.len(), 2);
        assert_eq!(hosts[0].proxy_jump[0].host, "bastion");
        assert_eq!(hosts[0].proxy_jump[1].host, "hpc");
    }

    #[test]
    fn test_parse_proxy_jump_with_user_port() {
        let content = r#"
Host target
    HostName target.example.com
    ProxyJump admin@jump.example.com:2222
"#;

        let hosts = parse_ssh_config_content(content).unwrap();
        let proxy = &hosts[0].proxy_jump[0];
        assert_eq!(proxy.user, Some("admin".to_string()));
        assert_eq!(proxy.host, "jump.example.com");
        assert_eq!(proxy.port, 2222);
    }

    #[test]
    fn test_parse_port_forwards() {
        let content = r#"
Host hpc
    HostName hpc.edu.cn
    LocalForward 8888 localhost:8888
    LocalForward 127.0.0.1:6006 localhost:6006
    RemoteForward 3000 localhost:3000
    DynamicForward 1080
"#;

        let hosts = parse_ssh_config_content(content).unwrap();
        let host = &hosts[0];

        assert_eq!(host.local_forwards.len(), 2);
        assert_eq!(host.local_forwards[0].local_port, 8888);
        assert_eq!(host.local_forwards[0].remote_port, 8888);
        assert_eq!(host.local_forwards[1].bind_address, "127.0.0.1");

        assert_eq!(host.remote_forwards.len(), 1);
        assert_eq!(host.remote_forwards[0].remote_port, 3000);

        assert_eq!(host.dynamic_forward, Some(1080));
    }

    #[test]
    fn test_proxy_jump_description() {
        let host = SshConfigHost {
            alias: "target".to_string(),
            proxy_jump: vec![
                ProxyJumpHost {
                    user: Some("admin".to_string()),
                    host: "jump1".to_string(),
                    port: 22,
                },
                ProxyJumpHost {
                    user: None,
                    host: "jump2".to_string(),
                    port: 2222,
                },
            ],
            ..Default::default()
        };

        let desc = host.proxy_jump_description().unwrap();
        assert_eq!(desc, "admin@jump1:22 → jump2:2222");
    }
}
