// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use fernomade_crypto::SessionKey;
use oxideterm_ssh::{
    ConnectionConsumer, ManagedKeyResolver, SshConfig, SshConnectionRegistry, SshPromptHandler,
    SshSecretCommandOutput, SshTransportClient, SshTransportError,
};

/// Default remote bootstrap executable used when no saved Mosh profile is involved.
pub const DEFAULT_MOSH_SERVER_EXECUTABLE: &str = "mosh-server";
const DEFAULT_BOOTSTRAP_COLUMNS: u16 = 80;
const DEFAULT_BOOTSTRAP_ROWS: u16 = 24;
const DEFAULT_COLOR_COUNT: u16 = 256;
const DEFAULT_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BOOTSTRAP_OUTPUT_BYTES: usize = 64 * 1024;
const MOSH_CONNECT_PREFIX: &[u8] = b"MOSH CONNECT ";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MoshIpFamily {
    #[default]
    Auto,
    Ipv4,
    Ipv6,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MoshUdpPortSelection {
    #[default]
    Automatic,
    Fixed(u16),
    Range {
        start: u16,
        end: u16,
    },
}

impl MoshUdpPortSelection {
    fn server_value(self) -> Result<Option<String>, MoshBootstrapError> {
        match self {
            Self::Automatic => Ok(None),
            Self::Fixed(0) => Err(MoshBootstrapError::InvalidUdpPortSelection),
            Self::Fixed(port) => Ok(Some(port.to_string())),
            Self::Range { start: 0, .. } | Self::Range { end: 0, .. } => {
                Err(MoshBootstrapError::InvalidUdpPortSelection)
            }
            Self::Range { start, end } if start > end => {
                Err(MoshBootstrapError::InvalidUdpPortSelection)
            }
            Self::Range { start, end } => Ok(Some(format!("{start}:{end}"))),
        }
    }
}

pub struct MoshBootstrapConfig {
    pub session_id: String,
    pub ssh: SshConfig,
    pub server_executable: String,
    pub udp_host_override: Option<String>,
    pub udp_port: MoshUdpPortSelection,
    pub ip_family: MoshIpFamily,
    pub color_count: u16,
    pub locale_assignments: Vec<(String, String)>,
    pub timeout: Duration,
    pub terminal_columns: u16,
    pub terminal_rows: u16,
}

impl MoshBootstrapConfig {
    #[must_use]
    pub fn new(session_id: impl Into<String>, ssh: SshConfig) -> Self {
        Self {
            session_id: session_id.into(),
            ssh,
            server_executable: DEFAULT_MOSH_SERVER_EXECUTABLE.to_string(),
            udp_host_override: None,
            udp_port: MoshUdpPortSelection::Automatic,
            ip_family: MoshIpFamily::Auto,
            color_count: DEFAULT_COLOR_COUNT,
            locale_assignments: Vec::new(),
            timeout: DEFAULT_BOOTSTRAP_TIMEOUT,
            terminal_columns: DEFAULT_BOOTSTRAP_COLUMNS,
            terminal_rows: DEFAULT_BOOTSTRAP_ROWS,
        }
    }
}

impl fmt::Debug for MoshBootstrapConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MoshBootstrapConfig")
            .field("session_id", &self.session_id)
            .field("ssh", &self.ssh)
            .field("server_executable", &self.server_executable)
            .field("udp_host_override", &self.udp_host_override)
            .field("udp_port", &self.udp_port)
            .field("ip_family", &self.ip_family)
            .field("color_count", &self.color_count)
            .field("locale_assignment_count", &self.locale_assignments.len())
            .field("timeout", &self.timeout)
            .field("terminal_columns", &self.terminal_columns)
            .field("terminal_rows", &self.terminal_rows)
            .finish()
    }
}

#[derive(Clone)]
pub struct MoshBootstrapContext {
    pub registry: SshConnectionRegistry,
    pub prompt_handler: Option<Arc<dyn SshPromptHandler>>,
    pub managed_key_resolver: Option<ManagedKeyResolver>,
}

pub struct MoshBootstrapResult {
    pub remote_host: String,
    pub remote_port: u16,
    pub ip_family: MoshIpFamily,
    pub key: SessionKey,
}

impl fmt::Debug for MoshBootstrapResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MoshBootstrapResult")
            .field("remote_host", &self.remote_host)
            .field("remote_port", &self.remote_port)
            .field("ip_family", &self.ip_family)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MoshBootstrapError {
    #[error("Mosh session identifier is empty")]
    EmptySessionId,
    #[error("Mosh server executable is empty")]
    EmptyServerExecutable,
    #[error("Mosh color count must be greater than zero")]
    InvalidColorCount,
    #[error("Mosh bootstrap terminal size must be greater than zero")]
    InvalidTerminalSize,
    #[error("Mosh UDP port or range is invalid")]
    InvalidUdpPortSelection,
    #[error("Mosh locale assignment is invalid")]
    InvalidLocaleAssignment,
    #[error("Mosh bootstrap failed over SSH: {0}")]
    Ssh(#[from] SshTransportError),
    #[error("Mosh bootstrap output exceeded the safety limit")]
    OutputTruncated,
    #[error("mosh-server exited without a successful connection response")]
    ServerFailed,
    #[error("mosh-server did not return one valid connection response")]
    InvalidServerResponse,
    #[error("mosh-server returned an invalid session key")]
    InvalidSessionKey,
}

/// Starts mosh-server over an isolated SSH transport and consumes its key.
pub async fn bootstrap_mosh(
    config: MoshBootstrapConfig,
    context: MoshBootstrapContext,
) -> Result<MoshBootstrapResult, MoshBootstrapError> {
    validate_config(&config)?;
    let command = build_server_command(&config)?;
    let consumer = ConnectionConsumer::MoshBootstrap(config.session_id.clone());
    let mut client = SshTransportClient::new(config.ssh.clone());
    if let Some(prompt_handler) = context.prompt_handler {
        client = client.with_prompt_handler(prompt_handler);
    }
    if let Some(managed_key_resolver) = context.managed_key_resolver {
        client = client.with_managed_key_resolver(managed_key_resolver);
    }

    let connection = client
        .connect_dedicated_node_with_registry(context.registry.clone(), consumer.clone())
        .await?;
    let connection_id = connection.connection_id().to_string();
    let release_guard = BootstrapConsumerGuard::new(context.registry, connection_id, consumer);
    let output = connection
        .run_secret_pty_command_capture(
            &command,
            u32::from(config.terminal_columns),
            u32::from(config.terminal_rows),
            config.timeout,
            MAX_BOOTSTRAP_OUTPUT_BYTES,
        )
        .await?;
    drop(release_guard);

    let (remote_port, key) = parse_server_output(&output)?;
    let remote_host = config
        .udp_host_override
        .filter(|host| !host.trim().is_empty())
        .unwrap_or(config.ssh.host);
    Ok(MoshBootstrapResult {
        remote_host,
        remote_port,
        ip_family: config.ip_family,
        key,
    })
}

fn validate_config(config: &MoshBootstrapConfig) -> Result<(), MoshBootstrapError> {
    if config.session_id.trim().is_empty() {
        return Err(MoshBootstrapError::EmptySessionId);
    }
    if config.server_executable.trim().is_empty() {
        return Err(MoshBootstrapError::EmptyServerExecutable);
    }
    if config.color_count == 0 {
        return Err(MoshBootstrapError::InvalidColorCount);
    }
    if config.terminal_columns == 0 || config.terminal_rows == 0 {
        return Err(MoshBootstrapError::InvalidTerminalSize);
    }
    let _ = config.udp_port.server_value()?;
    if config
        .locale_assignments
        .iter()
        .any(|(name, value)| !valid_locale_name(name) || value.contains(['\0', '\r', '\n']))
    {
        return Err(MoshBootstrapError::InvalidLocaleAssignment);
    }
    Ok(())
}

fn valid_locale_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn build_server_command(config: &MoshBootstrapConfig) -> Result<String, MoshBootstrapError> {
    let mut arguments = vec![
        config.server_executable.clone(),
        "new".to_string(),
        "-c".to_string(),
        config.color_count.to_string(),
        "-s".to_string(),
    ];
    if let Some(port) = config.udp_port.server_value()? {
        arguments.push("-p".to_string());
        arguments.push(port);
    }
    for (name, value) in &config.locale_assignments {
        arguments.push("-l".to_string());
        arguments.push(format!("{name}={value}"));
    }
    Ok(arguments
        .iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" "))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn parse_server_output(
    output: &SshSecretCommandOutput,
) -> Result<(u16, SessionKey), MoshBootstrapError> {
    if output.truncated {
        return Err(MoshBootstrapError::OutputTruncated);
    }
    if output.exit_code.is_some_and(|code| code != 0) {
        return Err(MoshBootstrapError::ServerFailed);
    }

    let lines = output
        .stdout
        .split(|byte| *byte == b'\n')
        .chain(output.stderr.split(|byte| *byte == b'\n'));
    let mut parsed_connect = None;
    for line in lines {
        let normalized = line.strip_suffix(b"\r").unwrap_or(line);
        if !normalized.starts_with(MOSH_CONNECT_PREFIX) {
            continue;
        }
        let parsed =
            parse_connect_line(normalized).ok_or(MoshBootstrapError::InvalidServerResponse)?;
        if parsed_connect.replace(parsed).is_some() {
            return Err(MoshBootstrapError::InvalidServerResponse);
        }
    }
    let Some((port, key_text)) = parsed_connect else {
        return Err(MoshBootstrapError::InvalidServerResponse);
    };
    let key_text =
        std::str::from_utf8(key_text).map_err(|_| MoshBootstrapError::InvalidSessionKey)?;
    let key = SessionKey::decode(key_text).map_err(|_| MoshBootstrapError::InvalidSessionKey)?;
    Ok((port, key))
}

fn parse_connect_line(line: &[u8]) -> Option<(u16, &[u8])> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let payload = line.strip_prefix(MOSH_CONNECT_PREFIX)?;
    let mut fields = payload.split(|byte| *byte == b' ');
    let port = std::str::from_utf8(fields.next()?).ok()?.parse().ok()?;
    if port == 0 {
        return None;
    }
    let key = fields.next()?;
    if key.is_empty() || fields.next().is_some() {
        return None;
    }
    Some((port, key))
}

struct BootstrapConsumerGuard {
    registry: SshConnectionRegistry,
    connection_id: String,
    consumer: ConnectionConsumer,
}

impl BootstrapConsumerGuard {
    fn new(
        registry: SshConnectionRegistry,
        connection_id: String,
        consumer: ConnectionConsumer,
    ) -> Self {
        Self {
            registry,
            connection_id,
            consumer,
        }
    }
}

impl Drop for BootstrapConsumerGuard {
    fn drop(&mut self) {
        // Dedicated registry entries retire as soon as this only consumer leaves.
        self.registry.release(&self.connection_id, &self.consumer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    const SYNTHETIC_KEY: &str = "AQIDBAUGBwgJCgsMDQ4PEA";

    fn output(stdout: &[u8], stderr: &[u8]) -> SshSecretCommandOutput {
        SshSecretCommandOutput {
            stdout: Zeroizing::new(stdout.to_vec()),
            stderr: Zeroizing::new(stderr.to_vec()),
            exit_code: Some(0),
            truncated: false,
        }
    }

    #[test]
    fn command_quotes_server_and_locale_values() {
        let mut config = MoshBootstrapConfig::new("session", SshConfig::default());
        config.server_executable = "/opt/Mosh Server/bin/mosh-server".to_string();
        config.udp_port = MoshUdpPortSelection::Range {
            start: 60_000,
            end: 60_010,
        };
        config.locale_assignments = vec![("LANG".to_string(), "en_US.UTF-8'x".to_string())];

        assert_eq!(
            build_server_command(&config).expect("command must build"),
            "'/opt/Mosh Server/bin/mosh-server' 'new' '-c' '256' '-s' '-p' '60000:60010' '-l' 'LANG=en_US.UTF-8'\\''x'"
        );
    }

    #[test]
    fn bootstrap_requires_a_non_zero_pty_size() {
        let mut config = MoshBootstrapConfig::new("session", SshConfig::default());
        assert_eq!(
            (config.terminal_columns, config.terminal_rows),
            (DEFAULT_BOOTSTRAP_COLUMNS, DEFAULT_BOOTSTRAP_ROWS)
        );

        config.terminal_rows = 0;
        assert!(matches!(
            validate_config(&config),
            Err(MoshBootstrapError::InvalidTerminalSize)
        ));
    }

    #[test]
    fn parses_single_connect_line_from_noisy_output() {
        let output = output(
            format!("banner\r\nMOSH CONNECT 60001 {SYNTHETIC_KEY}\r\n").as_bytes(),
            b"warning\n",
        );

        let (port, key) = parse_server_output(&output).expect("response must parse");
        assert_eq!(port, 60_001);
        assert_eq!(format!("{key:?}"), "SessionKey([REDACTED])");
    }

    #[test]
    fn rejects_duplicate_or_malformed_connect_lines() {
        let duplicate = output(
            format!("MOSH CONNECT 60001 {SYNTHETIC_KEY}\nMOSH CONNECT 60002 {SYNTHETIC_KEY}\n")
                .as_bytes(),
            b"",
        );
        assert!(matches!(
            parse_server_output(&duplicate),
            Err(MoshBootstrapError::InvalidServerResponse)
        ));

        let malformed = output(b"MOSH CONNECT 0 not-a-key\n", b"");
        assert!(matches!(
            parse_server_output(&malformed),
            Err(MoshBootstrapError::InvalidServerResponse)
        ));

        let malformed_then_valid = output(
            format!("MOSH CONNECT nope broken\nMOSH CONNECT 60001 {SYNTHETIC_KEY}\n").as_bytes(),
            b"",
        );
        assert!(matches!(
            parse_server_output(&malformed_then_valid),
            Err(MoshBootstrapError::InvalidServerResponse)
        ));
    }

    #[test]
    fn bootstrap_debug_redacts_session_key() {
        let result = MoshBootstrapResult {
            remote_host: "example.test".to_string(),
            remote_port: 60_001,
            ip_family: MoshIpFamily::Auto,
            key: SessionKey::decode(SYNTHETIC_KEY).expect("key must decode"),
        };
        let debug = format!("{result:?}");
        assert!(!debug.contains(SYNTHETIC_KEY));
        assert!(debug.contains("[REDACTED]"));
    }
}
