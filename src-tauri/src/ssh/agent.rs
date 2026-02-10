//! SSH Agent Client — Cross-platform SSH Agent authentication
//!
//! Provides real SSH Agent integration via russh's [`AgentClient`], supporting
//! challenge-response authentication by delegating signing to the system agent.
//!
//! # Architecture
//!
//! ```text
//!   ┌─────────────────────────────────────────────────────────┐
//!   │  SshAgentClient                                        │
//!   │  ├── connect()          (platform dispatch)            │
//!   │  │    ├── Unix:  AgentClient::connect_env()            │
//!   │  │    └── Win:   AgentClient::connect_named_pipe()     │
//!   │  │         └── .dynamic()  (type-erase stream)         │
//!   │  └── authenticate()     (key iteration + signing)      │
//!   │       └── AgentSigner   (Send-safe Signer wrapper)     │
//!   └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Platform Support
//! - **Unix/Linux/macOS**: `SSH_AUTH_SOCK` Unix domain socket
//! - **Windows**: `\\\\.\\pipe\\openssh-ssh-agent` named pipe (OpenSSH for Windows)
//!
//! # Authentication Flow
//! 1. Connect to the system SSH Agent socket/pipe
//! 2. Request identity list from agent ([`AgentClient::request_identities`])
//! 3. For each key, attempt [`Handle::authenticate_publickey_with`] with [`AgentSigner`]
//! 4. Server sends `Reply::SignRequest { key, data }` → russh calls
//!    [`Signer::auth_publickey_sign`] → agent signs → response completes auth
//!
//! # The `AgentSigner` Workaround (Send + RPITIT)
//!
//! russh 0.54's built-in `impl Signer for AgentClient` returns `impl Future + Send`
//! via RPITIT. Inside [`Handle::authenticate_publickey_with`], the call
//! `signer.auth_publickey_sign(&key, ...)` captures `&key` where `key` is a local
//! `PublicKey` from `Reply::SignRequest`. The Rust compiler cannot prove `Send` for
//! this borrow's lifetime through RPITIT (related: rust-lang/rust#100013).
//!
//! `AgentSigner` solves this by cloning `&PublicKey` → owned before the async block,
//! eliminating the problematic cross-`.await` reference. This pattern is stable across
//! russh versions because it depends only on the `Signer` trait shape, not internals.

use std::future::Future;

use russh::client::Handle;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::ssh_key;
use russh::{AgentAuthError, CryptoVec, Signer};
use tracing::{debug, info, warn};

use crate::ssh::error::SshError;

/// Send-safe wrapper around [`AgentClient`] implementing the [`Signer`] trait.
///
/// # Why this exists
///
/// russh's built-in `impl Signer for AgentClient` uses RPITIT (`impl Future + Send`).
/// Inside [`Handle::authenticate_publickey_with`], the generated state machine does:
///
/// ```ignore
/// Some(Reply::SignRequest { key, data }) => {
///     //  ↓ `key` is a local `PublicKey` from the `Reply` variant
///     let result = signer.auth_publickey_sign(&key, hash_alg, data).await;
///     //                                      ^^^^ borrow of local across .await
/// }
/// ```
///
/// The compiler cannot prove `Send` for `&key`'s specific lifetime through RPITIT
/// (related: rust-lang/rust#100013). Tauri's `#[tauri::command]` macro requires the
/// entire future to be `Send`, causing a hard compile error.
///
/// # Solution
///
/// Clone `&PublicKey` → owned **before** the async block. The future then captures
/// only owned values, making it trivially `Send`. The clone is cheap (~64 bytes for
/// Ed25519 keys).
///
/// # Stability
///
/// This wrapper depends only on the [`Signer`] trait shape and [`AgentClient::sign_request`],
/// both of which are stable public API. It will survive minor russh version bumps without
/// changes.
struct AgentSigner<'a> {
    agent: &'a mut AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>,
}

impl Signer for AgentSigner<'_> {
    type Error = AgentAuthError;

    fn auth_publickey_sign(
        &mut self,
        key: &ssh_key::PublicKey,
        hash_alg: Option<ssh_key::HashAlg>,
        to_sign: CryptoVec,
    ) -> impl Future<Output = Result<CryptoVec, Self::Error>> + Send {
        let key_owned = key.clone();
        async move {
            self.agent
                .sign_request(&key_owned, hash_alg, to_sign)
                .await
                .map_err(Into::into)
        }
    }
}

/// SSH Agent client wrapper
///
/// Wraps russh's `AgentClient` with a type-erased stream for cross-platform support.
pub struct SshAgentClient {
    agent: AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>,
}

impl SshAgentClient {
    /// Connect to the system SSH Agent
    ///
    /// On Unix, reads `SSH_AUTH_SOCK` and connects to the Unix domain socket.
    /// On Windows, connects to the OpenSSH named pipe `\\.\pipe\openssh-ssh-agent`.
    pub async fn connect() -> Result<Self, SshError> {
        info!("Connecting to system SSH Agent");

        #[cfg(unix)]
        {
            let agent = AgentClient::connect_env().await.map_err(|e| {
                SshError::AgentNotAvailable(format!(
                    "Failed to connect to SSH Agent: {}. \
                     Make sure SSH_AUTH_SOCK is set and ssh-agent is running.",
                    e
                ))
            })?;
            info!("Connected to SSH Agent via SSH_AUTH_SOCK");
            Ok(Self {
                agent: agent.dynamic(),
            })
        }

        #[cfg(windows)]
        {
            let agent = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
                .await
                .map_err(|e| {
                    SshError::AgentNotAvailable(format!(
                        "Failed to connect to SSH Agent via named pipe: {}. \
                         Make sure the OpenSSH Authentication Agent service is running.",
                        e
                    ))
                })?;
            info!("Connected to SSH Agent via named pipe");
            Ok(Self {
                agent: agent.dynamic(),
            })
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(SshError::AgentNotAvailable(
                "SSH Agent is not supported on this platform".to_string(),
            ))
        }
    }

    /// Authenticate with the SSH server using agent-held keys
    ///
    /// Iterates through all keys registered in the agent and tries each one
    /// against the server via `authenticate_publickey_with`. The agent handles
    /// the actual signing of the server challenge.
    pub async fn authenticate(
        &mut self,
        handle: &mut Handle<crate::ssh::ClientHandler>,
        username: String,
    ) -> Result<(), SshError> {
        // 1. List keys held by the agent
        let keys = self
            .agent
            .request_identities()
            .await
            .map_err(|e| SshError::AgentError(format!("Failed to list agent keys: {}", e)))?;

        if keys.is_empty() {
            return Err(SshError::AgentError(
                "SSH Agent has no keys loaded. Add keys with: ssh-add".to_string(),
            ));
        }

        info!(
            "SSH Agent reports {} key(s), attempting authentication",
            keys.len()
        );

        // 2. Try each key until one succeeds
        let mut last_error: Option<String> = None;
        for key in &keys {
            debug!("Trying agent key: {} ({})", key.algorithm(), key.comment());

            match handle
                .authenticate_publickey_with(
                    &username,
                    key.clone(),
                    None,
                    &mut AgentSigner {
                        agent: &mut self.agent,
                    },
                )
                .await
            {
                Ok(result) if result.success() => {
                    info!(
                        "SSH Agent authentication succeeded with key: {}",
                        key.comment()
                    );
                    return Ok(());
                }
                Ok(_failure) => {
                    debug!("Key rejected by server: {}", key.comment());
                }
                Err(e) => {
                    warn!("Agent signing error for key {}: {}", key.comment(), e);
                    last_error = Some(format!("{}", e));
                }
            }
        }

        // All keys exhausted
        Err(SshError::AgentError(format!(
            "No agent key was accepted by the server (tried {} key(s)){}",
            keys.len(),
            last_error
                .map(|e| format!(". Last error: {}", e))
                .unwrap_or_default()
        )))
    }
}

/// Check if SSH Agent is available on the system
///
/// Returns `true` if the agent socket/pipe appears to be accessible.
/// This is a quick pre-check; actual connection may still fail.
pub fn is_agent_available() -> bool {
    #[cfg(unix)]
    {
        std::env::var("SSH_AUTH_SOCK").is_ok()
    }

    #[cfg(windows)]
    {
        // OpenSSH for Windows agent uses a named pipe that is always "present"
        // as long as the service is installed; actual availability checked on connect.
        true
    }

    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_availability_check() {
        let available = is_agent_available();
        println!("Agent appears available: {}", available);
    }

    #[tokio::test]
    async fn test_agent_connect_requires_agent() {
        // If SSH_AUTH_SOCK is not set, connect should fail with AgentNotAvailable
        match SshAgentClient::connect().await {
            Ok(_) => println!("Agent connected (agent is running)"),
            Err(SshError::AgentNotAvailable(msg)) => {
                println!("Expected in CI: {}", msg);
            }
            Err(e) => panic!("Unexpected error type: {:?}", e),
        }
    }
}
