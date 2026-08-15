use std::{collections::BTreeSet, fmt, fs, path::PathBuf};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::handles::ClientRef;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolGroup {
    Basic,
    ConnectionDirectory,
    ConnectionRead,
    ConnectionManage,
    CredentialManage,
    NodeSession,
    TerminalSession,
    TerminalObserve,
    TerminalInput,
    RecordingControl,
    RecordingContent,
    DesktopSession,
    DesktopObserve,
    DesktopInput,
    DesktopClipboard,
    CommandObserve,
    CommandExecute,
    AuditRead,
    ArtifactTransfer,
    HostToolsObserve,
    HostToolsOperate,
    QuickCommandRead,
    QuickCommandContentRead,
    QuickCommandManage,
    QuickCommandExecute,
    AddonRead,
    AddonManage,
    ForwardRead,
    ForwardManage,
    FileRead,
    FileWrite,
    WorkspaceRead,
    WorkspaceEdit,
    CloudSync,
}

impl ToolGroup {
    pub const fn selectable() -> &'static [Self] {
        &[
            Self::ConnectionDirectory,
            Self::ConnectionRead,
            Self::ConnectionManage,
            Self::CredentialManage,
            Self::NodeSession,
            Self::TerminalSession,
            Self::TerminalObserve,
            Self::TerminalInput,
            Self::RecordingControl,
            Self::RecordingContent,
            Self::DesktopSession,
            Self::DesktopObserve,
            Self::DesktopInput,
            Self::DesktopClipboard,
            Self::CommandObserve,
            Self::CommandExecute,
            Self::AuditRead,
            Self::ArtifactTransfer,
            Self::HostToolsObserve,
            Self::HostToolsOperate,
            Self::QuickCommandRead,
            Self::QuickCommandContentRead,
            Self::QuickCommandManage,
            Self::QuickCommandExecute,
            Self::AddonRead,
            Self::AddonManage,
            Self::ForwardRead,
            Self::ForwardManage,
            Self::FileRead,
            Self::FileWrite,
            Self::WorkspaceRead,
            Self::WorkspaceEdit,
            Self::CloudSync,
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientApprovalMode {
    #[default]
    Standard,
    Unattended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientProjection {
    pub client_ref: ClientRef,
    pub label: String,
    pub enabled: bool,
    pub approval_mode: ClientApprovalMode,
    pub tool_groups: BTreeSet<ToolGroup>,
}

#[derive(Clone)]
struct ClientRecord {
    projection: ClientProjection,
    credential_digest: [u8; 32],
}

struct ClientRegistryState {
    clients: Vec<ClientRecord>,
}

pub struct ClientRegistry {
    state: RwLock<ClientRegistryState>,
    persistence_path: Option<PathBuf>,
}

pub struct ClientCredential(Zeroizing<String>);

impl ClientCredential {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClientCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClientCredential([REDACTED])")
    }
}

pub struct RegisteredClient {
    pub projection: ClientProjection,
    pub credential: ClientCredential,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientRegistryError {
    #[error("the client does not exist")]
    NotFound,
    #[error("failed to read the MCP client registry: {0}")]
    Read(#[source] std::io::Error),
    #[error("failed to decode the MCP client registry: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("unsupported MCP client registry version {0}")]
    UnsupportedVersion(u32),
    #[error("the MCP client registry contains an invalid credential digest")]
    InvalidDigest,
    #[error("failed to encode the MCP client registry: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("failed to save the MCP client registry: {0}")]
    Write(#[source] std::io::Error),
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedClientRegistry {
    version: u32,
    clients: Vec<PersistedClientRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedClientRecord {
    client_ref: ClientRef,
    label: String,
    enabled: bool,
    #[serde(default)]
    approval_mode: ClientApprovalMode,
    tool_groups: BTreeSet<ToolGroup>,
    credential_digest: String,
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self {
            state: RwLock::new(ClientRegistryState {
                clients: Vec::new(),
            }),
            persistence_path: None,
        }
    }
}

impl ClientRegistry {
    /// Opens local authorization metadata that is intentionally separate from settings and sync.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ClientRegistryError> {
        let path = path.into();
        let clients = if path.exists() {
            let bytes = fs::read(&path).map_err(ClientRegistryError::Read)?;
            let persisted: PersistedClientRegistry =
                serde_json::from_slice(&bytes).map_err(ClientRegistryError::Decode)?;
            if !matches!(persisted.version, 1 | 2) {
                return Err(ClientRegistryError::UnsupportedVersion(persisted.version));
            }
            persisted
                .clients
                .into_iter()
                .map(|record| {
                    let mut tool_groups = record.tool_groups;
                    tool_groups.insert(ToolGroup::Basic);
                    Ok(ClientRecord {
                        projection: ClientProjection {
                            client_ref: record.client_ref,
                            label: record.label,
                            enabled: record.enabled,
                            approval_mode: record.approval_mode,
                            tool_groups,
                        },
                        credential_digest: decode_digest(&record.credential_digest)?,
                    })
                })
                .collect::<Result<Vec<_>, ClientRegistryError>>()?
        } else {
            Vec::new()
        };
        Ok(Self {
            state: RwLock::new(ClientRegistryState { clients }),
            persistence_path: Some(path),
        })
    }

    pub fn register(
        &self,
        label: impl Into<String>,
        approval_mode: ClientApprovalMode,
        tool_groups: impl IntoIterator<Item = ToolGroup>,
    ) -> Result<RegisteredClient, ClientRegistryError> {
        let mut credential_text = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let credential_digest = credential_digest(&credential_text);
        let credential = ClientCredential(Zeroizing::new(std::mem::take(&mut credential_text)));
        credential_text.zeroize();

        let mut tool_groups = tool_groups.into_iter().collect::<BTreeSet<_>>();
        tool_groups.insert(ToolGroup::Basic);
        let projection = ClientProjection {
            client_ref: ClientRef::new(),
            label: label.into(),
            enabled: true,
            approval_mode,
            tool_groups,
        };
        let mut state = self.state.write();
        state.clients.push(ClientRecord {
            projection: projection.clone(),
            credential_digest,
        });
        if let Err(error) = self.persist(&state) {
            state.clients.pop();
            return Err(error);
        }
        Ok(RegisteredClient {
            projection,
            credential,
        })
    }

    pub fn authenticate_bearer(&self, authorization: &str) -> Option<ClientProjection> {
        let token = authorization.strip_prefix("Bearer ")?;
        let candidate = credential_digest(token);
        self.state
            .read()
            .clients
            .iter()
            .find(|record| {
                record.projection.enabled && bool::from(record.credential_digest.ct_eq(&candidate))
            })
            .map(|record| record.projection.clone())
    }

    pub fn get(&self, client_ref: &ClientRef) -> Option<ClientProjection> {
        self.state
            .read()
            .clients
            .iter()
            .find(|record| &record.projection.client_ref == client_ref)
            .map(|record| record.projection.clone())
    }

    pub fn list(&self) -> Vec<ClientProjection> {
        self.state
            .read()
            .clients
            .iter()
            .map(|record| record.projection.clone())
            .collect()
    }

    pub fn set_groups(
        &self,
        client_ref: &ClientRef,
        tool_groups: impl IntoIterator<Item = ToolGroup>,
    ) -> Result<(), ClientRegistryError> {
        let mut state = self.state.write();
        let record = state
            .clients
            .iter_mut()
            .find(|record| &record.projection.client_ref == client_ref)
            .ok_or(ClientRegistryError::NotFound)?;
        let previous = record.projection.tool_groups.clone();
        let mut tool_groups = tool_groups.into_iter().collect::<BTreeSet<_>>();
        tool_groups.insert(ToolGroup::Basic);
        record.projection.tool_groups = tool_groups;
        if let Err(error) = self.persist(&state) {
            let record = state
                .clients
                .iter_mut()
                .find(|record| &record.projection.client_ref == client_ref)
                .expect("client was found before persistence");
            record.projection.tool_groups = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_approval_mode(
        &self,
        client_ref: &ClientRef,
        approval_mode: ClientApprovalMode,
    ) -> Result<(), ClientRegistryError> {
        let mut state = self.state.write();
        let record = state
            .clients
            .iter_mut()
            .find(|record| &record.projection.client_ref == client_ref)
            .ok_or(ClientRegistryError::NotFound)?;
        let previous = record.projection.approval_mode;
        record.projection.approval_mode = approval_mode;
        if let Err(error) = self.persist(&state) {
            let record = state
                .clients
                .iter_mut()
                .find(|record| &record.projection.client_ref == client_ref)
                .expect("client was found before persistence");
            record.projection.approval_mode = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_enabled(
        &self,
        client_ref: &ClientRef,
        enabled: bool,
    ) -> Result<(), ClientRegistryError> {
        let mut state = self.state.write();
        let record = state
            .clients
            .iter_mut()
            .find(|record| &record.projection.client_ref == client_ref)
            .ok_or(ClientRegistryError::NotFound)?;
        let previous = record.projection.enabled;
        record.projection.enabled = enabled;
        if let Err(error) = self.persist(&state) {
            let record = state
                .clients
                .iter_mut()
                .find(|record| &record.projection.client_ref == client_ref)
                .expect("client was found before persistence");
            record.projection.enabled = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn remove(&self, client_ref: &ClientRef) -> Result<(), ClientRegistryError> {
        let mut state = self.state.write();
        let Some(index) = state
            .clients
            .iter()
            .position(|record| &record.projection.client_ref == client_ref)
        else {
            return Err(ClientRegistryError::NotFound);
        };
        let removed = state.clients.remove(index);
        if let Err(error) = self.persist(&state) {
            state.clients.insert(index, removed);
            return Err(error);
        }
        Ok(())
    }

    pub fn rotate_credential(
        &self,
        client_ref: &ClientRef,
    ) -> Result<ClientCredential, ClientRegistryError> {
        let mut credential_text = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let credential_digest = credential_digest(&credential_text);
        let credential = ClientCredential(Zeroizing::new(std::mem::take(&mut credential_text)));
        credential_text.zeroize();

        let mut state = self.state.write();
        let record = state
            .clients
            .iter_mut()
            .find(|record| &record.projection.client_ref == client_ref)
            .ok_or(ClientRegistryError::NotFound)?;
        let previous = record.credential_digest;
        record.credential_digest = credential_digest;
        if let Err(error) = self.persist(&state) {
            let record = state
                .clients
                .iter_mut()
                .find(|record| &record.projection.client_ref == client_ref)
                .expect("client was found before persistence");
            record.credential_digest = previous;
            return Err(error);
        }
        Ok(credential)
    }

    fn persist(&self, state: &ClientRegistryState) -> Result<(), ClientRegistryError> {
        let Some(path) = &self.persistence_path else {
            return Ok(());
        };
        let persisted = PersistedClientRegistry {
            version: 2,
            clients: state
                .clients
                .iter()
                .map(|record| PersistedClientRecord {
                    client_ref: record.projection.client_ref.clone(),
                    label: record.projection.label.clone(),
                    enabled: record.projection.enabled,
                    approval_mode: record.projection.approval_mode,
                    tool_groups: record.projection.tool_groups.clone(),
                    credential_digest: encode_digest(&record.credential_digest),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&persisted).map_err(ClientRegistryError::Encode)?;
        oxideterm_atomic_file::durable_write(path, &bytes).map_err(ClientRegistryError::Write)
    }
}

fn credential_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn encode_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_digest(value: &str) -> Result<[u8; 32], ClientRegistryError> {
    if value.len() != 64 {
        return Err(ClientRegistryError::InvalidDigest);
    }
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| ClientRegistryError::InvalidDigest)?;
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_client_registry_authenticates_without_storing_cleartext_credential() {
        let registry_path = std::env::temp_dir().join(format!(
            "oxideterm-public-mcp-clients-{}.json",
            Uuid::new_v4().simple()
        ));
        let registry = ClientRegistry::open(&registry_path).expect("open empty registry");
        let registered = registry
            .register(
                "integration client",
                ClientApprovalMode::Standard,
                [ToolGroup::Basic],
            )
            .expect("register client");
        let credential = Zeroizing::new(registered.credential.expose().to_owned());
        let persisted = fs::read(&registry_path).expect("read persisted registry");
        let persisted_json: serde_json::Value =
            serde_json::from_slice(&persisted).expect("decode persisted registry");
        assert_eq!(persisted_json["version"], 2);
        assert!(
            !persisted
                .windows(credential.len())
                .any(|window| window == credential.as_bytes())
        );

        let reopened = ClientRegistry::open(&registry_path).expect("reopen persisted registry");
        let authorization = Zeroizing::new(format!("Bearer {}", credential.as_str()));
        let authenticated = reopened
            .authenticate_bearer(&authorization)
            .expect("authenticate persisted digest");
        assert_eq!(authenticated.client_ref, registered.projection.client_ref);

        let legacy_client_ref = ClientRef::new();
        let legacy_token = Zeroizing::new("legacy-high-entropy-test-token".to_owned());
        let legacy_registry = serde_json::json!({
            "version": 1,
            "clients": [{
                "client_ref": legacy_client_ref,
                "label": "legacy client",
                "enabled": true,
                "tool_groups": ["basic"],
                "credential_digest": encode_digest(&credential_digest(&legacy_token)),
            }],
        });
        fs::write(
            &registry_path,
            serde_json::to_vec_pretty(&legacy_registry).expect("encode legacy registry"),
        )
        .expect("write legacy registry");
        let migrated = ClientRegistry::open(&registry_path).expect("open legacy registry");
        let legacy_projection = migrated
            .get(&legacy_client_ref)
            .expect("load legacy client");
        assert_eq!(
            legacy_projection.approval_mode,
            ClientApprovalMode::Standard
        );
        migrated
            .set_enabled(&legacy_client_ref, false)
            .expect("persist migrated registry");
        let migrated_json: serde_json::Value =
            serde_json::from_slice(&fs::read(&registry_path).expect("read migrated registry"))
                .expect("decode migrated registry");
        assert_eq!(migrated_json["version"], 2);

        let _ = fs::remove_file(registry_path);
    }
}
