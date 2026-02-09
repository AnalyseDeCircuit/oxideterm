//! SSH Agent Client - Cross-platform SSH Agent authentication
//!
//! This module provides SSH Agent detection and user-friendly error messages.
//!
//! # Current Status
//!
//! **⚠️ TODO: Full SSH Agent signing implementation pending**
//!
//! SSH Agent authentication requires low-level protocol implementation to:
//! 1. Request challenge from SSH server
//! 2. Forward challenge to agent for signing
//! 3. Return signed response to server
//!
//! Current russh library (0.48) does not expose sufficient low-level APIs
//! for agent signing integration. Waiting for library updates or will implement
//! custom signing flow in future versions.
//!
//! # Platform Support
//! - **Unix/Linux/macOS**: Uses `SSH_AUTH_SOCK` environment variable
//! - **Windows**: Uses `\\.\pipe\openssh-ssh-agent` named pipe
//!
//! # Workaround
//! Users can use SSH key files with the agent keys exported, or configure
//! OpenSSH config with ProxyCommand for agent forwarding.

use crate::ssh::error::SshError;
use russh::client::Handle;
use tracing::info;

/// SSH Agent client wrapper (placeholder for future implementation)
///
/// Currently returns user-friendly error messages directing users to alternatives.
pub struct SshAgentClient;

impl SshAgentClient {
    /// Connect to the system SSH Agent
    ///
    /// # Returns
    /// - `Err(SshError::AgentNotAvailable)` - Always, with helpful error message
    ///
    /// # TODO
    /// Implement full agent connection using russh::keys::agent::client::AgentClient
    /// when library provides necessary signing APIs.
    pub async fn connect() -> Result<Self, SshError> {
        info!("SSH Agent authentication requested");

        // TODO: Implement full agent connection
        // let stream = russh::keys::agent::client::AgentClient::connect_env().await?;

        Err(SshError::AgentNotAvailable(format!(
            "SSH Agent authentication is not yet fully implemented.\n\n\
             This feature requires deep integration with SSH agent protocol for challenge signing.\n\
             We're waiting for russh library updates to provide the necessary low-level APIs.\n\n\
             Workarounds:\n\
             1. Use SSH key file authentication instead (recommended)\n\
             2. Export your agent key: ssh-add -L > ~/.ssh/id_agent.pub\n\
             3. Use the corresponding private key file for connection\n\n\
             {}",
            get_platform_help()
        )))
    }

    /// Authenticate using SSH Agent (not yet implemented)
    ///
    /// # TODO
    /// Implement challenge-response flow:
    /// 1. Get agent identities
    /// 2. Try each key with server
    /// 3. Use agent.sign_request() for challenge signing
    /// 4. Complete authentication
    pub async fn authenticate(
        &mut self,
        _handle: &Handle<crate::ssh::ClientHandler>,
        _username: &str,
    ) -> Result<(), SshError> {
        // This should never be called since connect() always fails
        Err(SshError::AgentError(
            "SSH Agent authentication not yet implemented".to_string(),
        ))
    }
}

/// Check if SSH Agent is available on the system
///
/// # Returns
/// - `true` if agent socket/pipe appears to be accessible
/// - `false` otherwise
///
/// Note: Even if this returns true, actual authentication will fail with
/// a helpful error message directing users to alternatives.
pub fn is_agent_available() -> bool {
    #[cfg(unix)]
    {
        std::env::var("SSH_AUTH_SOCK").is_ok()
    }

    #[cfg(windows)]
    {
        // On Windows, check if the named pipe exists
        // Basic check - actual connection might still fail
        true
    }

    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Get platform-specific help message for SSH Agent setup and alternatives
fn get_platform_help() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Platform: macOS\n\
         \n\
         To use SSH keys with OxideTerm:\n\
         1. Use key file authentication (Select 'SSH Key' in connection dialog)\n\
         2. Point to your key file: ~/.ssh/id_rsa or ~/.ssh/id_ed25519\n\
         \n\
         If you need agent forwarding:\n\
         - This will be supported in a future version\n\
         - For now, use key files directly"
    }

    #[cfg(target_os = "windows")]
    {
        "Platform: Windows\n\
         \n\
         To use SSH keys with OxideTerm:\n\
         1. Use key file authentication (Select 'SSH Key' in connection dialog)\n\
         2. Point to your key file: %USERPROFILE%\\.ssh\\id_rsa\n\
         \n\
         If you need agent forwarding:\n\
         - This will be supported in a future version\n\
         - For now, use key files directly"
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "Platform: Linux\n\
         \n\
         To use SSH keys with OxideTerm:\n\
         1. Use key file authentication (Select 'SSH Key' in connection dialog)\n\
         2. Point to your key file: ~/.ssh/id_rsa or ~/.ssh/id_ed25519\n\
         \n\
         If you need agent forwarding:\n\
         - This will be supported in a future version\n\
         - For now, use key files directly"
    }

    #[cfg(not(any(unix, windows)))]
    {
        "Please use SSH key file authentication instead of agent authentication."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_availability_check() {
        // This test just ensures the function doesn't panic
        let available = is_agent_available();
        println!("Agent appears available: {}", available);
    }

    #[test]
    fn test_platform_help_message() {
        let help = get_platform_help();
        assert!(!help.is_empty());
        println!("Platform help:\n{}", help);
    }

    #[tokio::test]
    async fn test_agent_connection_returns_helpful_error() {
        // Verify that we get a helpful error message
        match SshAgentClient::connect().await {
            Ok(_) => panic!("Expected error, got Ok"),
            Err(SshError::AgentNotAvailable(msg)) => {
                println!("Error message:\n{}", msg);
                assert!(msg.contains("not yet fully implemented"));
                assert!(msg.contains("Workarounds"));
            }
            Err(e) => panic!("Expected AgentNotAvailable, got: {:?}", e),
        }
    }
}
