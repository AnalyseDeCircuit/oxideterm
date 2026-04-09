// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Agent Deployer — uploads and starts the agent on a remote host.
//!
//! Workflow:
//! 1. Detect remote architecture via `uname -m`
//! 2. Check if agent is already deployed (version match)
//! 3. Upload the correct binary via SFTP
//! 4. chmod +x
//! 5. Start agent via SSH exec channel
//! 6. Handshake: wait for `sys/info` response
//!
//! Agent binaries are bundled in the app resources as:
//! - `agents/oxideterm-agent-x86_64-linux-musl`
//! - `agents/oxideterm-agent-aarch64-linux-musl`

use std::path::PathBuf;

use tauri::Manager;
use tracing::{debug, info, warn};

use super::protocol::{AgentStatus, SysInfoResult};
use super::transport::AgentTransport;
use crate::sftp::session::SftpSession;
use crate::ssh::HandleController;

/// Remote path where the agent binary is stored.
const AGENT_REMOTE_DIR: &str = ".oxideterm";
const AGENT_BINARY_NAME: &str = "oxideterm-agent";
const LEGACY_AGENT_COMPATIBILITY_VERSION: u32 = 1;

/// Deployer for the OxideTerm agent.
pub struct AgentDeployer;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteAgentVersionInfo {
    version: String,
    compatibility_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RemoteAgentInstallState {
    Missing,
    Current,
    Incompatible(RemoteAgentVersionInfo),
}

impl AgentDeployer {
    fn remote_path() -> String {
        format!("~/{}/{}", AGENT_REMOTE_DIR, AGENT_BINARY_NAME)
    }

    fn expected_compatibility_version() -> u32 {
        include_str!("../../../agent/COMPATIBILITY_VERSION")
            .trim()
            .parse()
            .expect("agent compatibility version must be a valid u32")
    }

    /// Deploy and start the agent on a remote host.
    ///
    /// Returns a connected `AgentTransport` if successful.
    ///
    /// For unsupported architectures, this method will:
    /// 1. Check if a manually uploaded agent binary exists at the remote path
    /// 2. If it exists, proceed to start + handshake (allowing user-provided binaries)
    /// 3. If not, return `DeployError::ManualUploadRequired` with the expected path
    pub async fn deploy_and_start(
        controller: &HandleController,
        sftp: &SftpSession,
        app_handle: &tauri::AppHandle,
    ) -> Result<(AgentTransport, SysInfoResult), DeployError> {
        // Step 1: Detect remote architecture
        let arch = Self::detect_arch(controller).await?;
        info!("[agent-deploy] Remote architecture: {}", arch);

        // Step 2: Determine remote path
        let remote_dir = format!("~/{}", AGENT_REMOTE_DIR);
        let remote_path = Self::remote_path();

        // Step 3: Try to resolve the local binary for this architecture
        let local_binary_result = Self::resolve_binary(&arch, app_handle);

        match local_binary_result {
            Ok(local_binary) => {
                // Supported architecture — proceed with auto-deploy
                info!("[agent-deploy] Using binary: {}", local_binary.display());

                // Step 4: Check if agent is already deployed (check version)
                let needs_upload = !matches!(
                    Self::probe_remote_install(controller, &remote_path).await,
                    RemoteAgentInstallState::Current
                );

                if needs_upload {
                    // Step 5: Upload binary
                    info!("[agent-deploy] Uploading agent binary...");

                    // Ensure remote directory exists
                    Self::exec_simple(controller, &format!("mkdir -p {}", remote_dir)).await?;

                    // Read the local binary
                    let binary_data = tokio::fs::read(&local_binary)
                        .await
                        .map_err(|e| DeployError::LocalIo(e.to_string()))?;

                    info!("[agent-deploy] Binary size: {} bytes", binary_data.len());

                    // Upload via SFTP
                    sftp.write_content(&remote_path, &binary_data)
                        .await
                        .map_err(|e| DeployError::Upload(e.to_string()))?;

                    // chmod +x
                    Self::exec_simple(controller, &format!("chmod +x {}", remote_path)).await?;

                    info!("[agent-deploy] Upload complete");
                } else {
                    info!("[agent-deploy] Agent already deployed, skipping upload");
                }
            }
            Err(DeployError::UnsupportedArch(ref unsupported_arch)) => {
                // Unsupported architecture — check if user manually uploaded a binary
                info!(
                    "[agent-deploy] Unsupported architecture '{}', checking for manual upload at {}",
                    unsupported_arch, remote_path
                );

                match Self::probe_remote_install(controller, &remote_path).await {
                    RemoteAgentInstallState::Missing => {
                        info!(
                            "[agent-deploy] No agent binary found for arch '{}', manual upload required",
                            arch
                        );
                        return Err(DeployError::ManualUploadRequired {
                            arch: arch.clone(),
                            remote_path: remote_path.clone(),
                        });
                    }
                    RemoteAgentInstallState::Incompatible(version_info) => {
                        info!(
                            "[agent-deploy] Incompatible agent found for unsupported arch '{}': '{}' (compat {} -> {})",
                            arch,
                            version_info.version,
                            version_info.compatibility_version,
                            Self::expected_compatibility_version()
                        );
                        return Err(DeployError::ManualUpdateRequired {
                            arch: arch.clone(),
                            remote_path: remote_path.clone(),
                            current_agent_version: version_info.version,
                            current_compatibility_version: version_info.compatibility_version,
                            expected_compatibility_version: Self::expected_compatibility_version(),
                        });
                    }
                    RemoteAgentInstallState::Current => {
                        info!(
                            "[agent-deploy] Found manually uploaded agent for unsupported arch '{}'",
                            arch
                        );
                    }
                }
            }
            Err(e) => return Err(e),
        }

        // Step 6: Start the agent
        let transport = Self::start_agent(controller, &remote_path).await?;

        // Step 7: Handshake — verify via sys/info
        let info = Self::handshake(&transport).await?;
        info!(
            "[agent-deploy] Agent ready: v{} {} (pid {})",
            info.version, info.arch, info.pid
        );

        Ok((transport, info))
    }

    /// Detect remote OS architecture via `uname -m`.
    async fn detect_arch(controller: &HandleController) -> Result<String, DeployError> {
        let result = crate::commands::ide::exec_command_inner(
            controller.clone(),
            "uname -m".to_string(),
            None,
            Some(10),
        )
        .await
        .map_err(|e| DeployError::ArchDetection(e))?;

        let arch = result.stdout.trim().to_string();
        if arch.is_empty() {
            return Err(DeployError::ArchDetection(
                "uname -m returned empty output".to_string(),
            ));
        }

        Ok(arch)
    }

    /// Map `uname -m` output to our binary suffix.
    fn arch_to_target(arch: &str) -> Result<&'static str, DeployError> {
        match arch {
            "x86_64" | "amd64" => Ok("x86_64-linux-musl"),
            "aarch64" | "arm64" => Ok("aarch64-linux-musl"),
            other => Err(DeployError::UnsupportedArch(other.to_string())),
        }
    }

    /// Resolve the path to the bundled agent binary for the given arch.
    fn resolve_binary(arch: &str, app_handle: &tauri::AppHandle) -> Result<PathBuf, DeployError> {
        let target = Self::arch_to_target(arch)?;
        let binary_name = format!("agents/oxideterm-agent-{}", target);

        // Try to resolve from Tauri resources
        let resource_path = app_handle
            .path()
            .resolve(&binary_name, tauri::path::BaseDirectory::Resource)
            .map_err(|e| {
                DeployError::BinaryNotFound(format!("Resource '{}' not found: {}", binary_name, e))
            })?;

        if !resource_path.exists() {
            return Err(DeployError::BinaryNotFound(format!(
                "Binary not found at {:?}",
                resource_path
            )));
        }

        Ok(resource_path)
    }

    fn parse_remote_version_output(output: &str) -> RemoteAgentInstallState {
        let trimmed = output.trim();

        if trimmed.contains("NOT_FOUND") || trimmed.is_empty() {
            RemoteAgentInstallState::Missing
        } else {
            let mut parts = trimmed.split_whitespace();
            let _binary_name = parts.next();
            let version = parts.next().unwrap_or(trimmed).to_string();
            let mut compatibility_version = LEGACY_AGENT_COMPATIBILITY_VERSION;

            while let Some(part) = parts.next() {
                if part == "compat" {
                    if let Some(raw_version) = parts.next() {
                        if let Ok(parsed) = raw_version.parse::<u32>() {
                            compatibility_version = parsed;
                        }
                    }
                    break;
                }
            }

            if compatibility_version == Self::expected_compatibility_version() {
                RemoteAgentInstallState::Current
            } else {
                RemoteAgentInstallState::Incompatible(RemoteAgentVersionInfo {
                    version,
                    compatibility_version,
                })
            }
        }
    }

    async fn probe_remote_install(
        controller: &HandleController,
        remote_path: &str,
    ) -> RemoteAgentInstallState {
        // Try running the agent with --version flag
        let result = crate::commands::ide::exec_command_inner(
            controller.clone(),
            format!("{} --version 2>/dev/null || echo 'NOT_FOUND'", remote_path),
            None,
            Some(5),
        )
        .await;

        match result {
            Ok(r) => {
                let output = r.stdout.trim();
                match Self::parse_remote_version_output(output) {
                    RemoteAgentInstallState::Missing => {
                        debug!("[agent-deploy] Agent not found at {}", remote_path);
                        RemoteAgentInstallState::Missing
                    }
                    RemoteAgentInstallState::Current => {
                        debug!("[agent-deploy] Version match: {}", output);
                        RemoteAgentInstallState::Current
                    }
                    RemoteAgentInstallState::Incompatible(version_info) => {
                        debug!(
                            "[agent-deploy] Compatibility mismatch: got compat {}, want {}",
                            version_info.compatibility_version,
                            Self::expected_compatibility_version()
                        );
                        RemoteAgentInstallState::Incompatible(version_info)
                    }
                }
            }
            Err(_) => {
                debug!("[agent-deploy] Failed to check version, will upload");
                RemoteAgentInstallState::Missing
            }
        }
    }

    /// Inspect remote agent install state for frontend status display.
    pub async fn inspect_remote_status(
        controller: &HandleController,
    ) -> Result<AgentStatus, DeployError> {
        let arch = Self::detect_arch(controller).await?;
        let remote_path = Self::remote_path();

        match Self::arch_to_target(&arch) {
            Ok(_) => Ok(AgentStatus::NotDeployed),
            Err(DeployError::UnsupportedArch(_)) => {
                match Self::probe_remote_install(controller, &remote_path).await {
                    RemoteAgentInstallState::Missing => {
                        Ok(AgentStatus::ManualUploadRequired { arch, remote_path })
                    }
                    RemoteAgentInstallState::Current => Ok(AgentStatus::NotDeployed),
                    RemoteAgentInstallState::Incompatible(version_info) => {
                        Ok(AgentStatus::ManualUpdateRequired {
                            arch,
                            remote_path,
                            current_agent_version: version_info.version,
                            current_compatibility_version: version_info.compatibility_version,
                            expected_compatibility_version: Self::expected_compatibility_version(),
                        })
                    }
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Start the agent process via SSH exec channel.
    async fn start_agent(
        controller: &HandleController,
        remote_path: &str,
    ) -> Result<AgentTransport, DeployError> {
        let channel = controller
            .open_session_channel()
            .await
            .map_err(|e| DeployError::StartFailed(format!("Channel open failed: {}", e)))?;

        let agent_command = remote_path.to_string();

        let transport = AgentTransport::new(channel, &agent_command)
            .await
            .map_err(|e| DeployError::StartFailed(e.to_string()))?;

        Ok(transport)
    }

    /// Perform handshake — call sys/info and verify the agent is ready.
    async fn handshake(transport: &AgentTransport) -> Result<SysInfoResult, DeployError> {
        // First, a simple ping
        let pong = transport
            .call_with_timeout("sys/ping", serde_json::json!({}), 10)
            .await
            .map_err(|e| DeployError::Handshake(format!("Ping failed: {}", e)))?;

        debug!("[agent-deploy] Ping response: {:?}", pong);

        // Then get full info
        let info_value = transport
            .call_with_timeout("sys/info", serde_json::json!({}), 10)
            .await
            .map_err(|e| DeployError::Handshake(format!("sys/info failed: {}", e)))?;

        let info: SysInfoResult = serde_json::from_value(info_value)
            .map_err(|e| DeployError::Handshake(format!("Invalid sys/info response: {}", e)))?;

        if info.compatibility_version != Self::expected_compatibility_version() {
            return Err(DeployError::Handshake(format!(
                "Agent compatibility mismatch: got {}, expected {}",
                info.compatibility_version,
                Self::expected_compatibility_version()
            )));
        }

        Ok(info)
    }

    /// Execute a simple command and return stdout.
    async fn exec_simple(
        controller: &HandleController,
        command: &str,
    ) -> Result<String, DeployError> {
        let result = crate::commands::ide::exec_command_inner(
            controller.clone(),
            command.to_string(),
            None,
            Some(30),
        )
        .await
        .map_err(|e| DeployError::ExecFailed(e))?;

        // Only warn on explicit non-zero exit codes.
        // When exit_code is None (SSH channel didn't send ExitStatus before Close),
        // treat as success if stderr is empty — this is common for simple commands.
        match result.exit_code {
            Some(code) if code != 0 => {
                warn!(
                    "[agent-deploy] Command '{}' failed (exit {}): {}",
                    command, code, result.stderr
                );
            }
            None if !result.stderr.trim().is_empty() => {
                warn!(
                    "[agent-deploy] Command '{}' produced stderr: {}",
                    command, result.stderr
                );
            }
            _ => {}
        }

        Ok(result.stdout)
    }
}

/// Deployment errors.
#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error("Architecture detection failed: {0}")]
    ArchDetection(String),

    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),

    #[error("Manual upload required for arch '{arch}': upload agent binary to {remote_path}")]
    ManualUploadRequired { arch: String, remote_path: String },

    #[error(
        "Manual update required for arch '{arch}': replace {remote_path} (agent {current_agent_version}, compat {current_compatibility_version} -> {expected_compatibility_version})"
    )]
    ManualUpdateRequired {
        arch: String,
        remote_path: String,
        current_agent_version: String,
        current_compatibility_version: u32,
        expected_compatibility_version: u32,
    },

    #[error("Agent binary not found: {0}")]
    BinaryNotFound(String),

    #[error("Local I/O error: {0}")]
    LocalIo(String),

    #[error("Upload failed: {0}")]
    Upload(String),

    #[error("Command execution failed: {0}")]
    ExecFailed(String),

    #[error("Agent start failed: {0}")]
    StartFailed(String),

    #[error("Handshake failed: {0}")]
    Handshake(String),
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::NotDeployed => write!(f, "Not deployed"),
            AgentStatus::Deploying => write!(f, "Deploying..."),
            AgentStatus::Ready { version, arch, pid } => {
                write!(f, "Ready v{} {} (pid {})", version, arch, pid)
            }
            AgentStatus::Failed { reason } => write!(f, "Failed: {}", reason),
            AgentStatus::UnsupportedArch { arch } => write!(f, "Unsupported arch: {}", arch),
            AgentStatus::ManualUploadRequired { arch, remote_path } => {
                write!(f, "Manual upload required for {}: {}", arch, remote_path)
            }
            AgentStatus::ManualUpdateRequired {
                arch,
                remote_path,
                current_agent_version,
                current_compatibility_version,
                expected_compatibility_version,
            } => write!(
                f,
                "Manual update required for {}: {} (agent {}, compat {} -> {})",
                arch,
                remote_path,
                current_agent_version,
                current_compatibility_version,
                expected_compatibility_version
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentDeployer, RemoteAgentInstallState, RemoteAgentVersionInfo};

    #[test]
    fn parses_missing_remote_agent() {
        assert_eq!(
            AgentDeployer::parse_remote_version_output("NOT_FOUND"),
            RemoteAgentInstallState::Missing
        );
        assert_eq!(
            AgentDeployer::parse_remote_version_output("   "),
            RemoteAgentInstallState::Missing
        );
    }

    #[test]
    fn parses_current_remote_agent() {
        let current_output = format!(
            "oxideterm-agent 0.12.1 compat {}",
            AgentDeployer::expected_compatibility_version()
        );
        assert_eq!(
            AgentDeployer::parse_remote_version_output(&current_output),
            RemoteAgentInstallState::Current
        );

        assert_eq!(
            AgentDeployer::parse_remote_version_output("oxideterm-agent 0.12.1"),
            RemoteAgentInstallState::Current
        );
    }

    #[test]
    fn parses_outdated_remote_agent() {
        assert_eq!(
            AgentDeployer::parse_remote_version_output("oxideterm-agent 0.12.1 compat 2"),
            RemoteAgentInstallState::Incompatible(RemoteAgentVersionInfo {
                version: "0.12.1".to_string(),
                compatibility_version: 2,
            })
        );
    }
}
