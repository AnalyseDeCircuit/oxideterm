use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{ConnectionStore, ConnectionStoreData};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedSshIdentity([u8; 32]);

impl ConnectionStoreData {
    // A credential replacement can change identity without changing its keychain reference.
    pub(super) fn rotate_ssh_identity(&mut self, id: &str) {
        self.ssh_identity_generations
            .insert(id.to_owned(), Uuid::new_v4());
    }
}

impl ConnectionStore {
    pub fn saved_ssh_identity(&self, id: &str) -> Option<SavedSshIdentity> {
        let connection = self.get(id)?;
        let mut digest = Sha256::new();
        // An embedded route takes precedence over a referenced jump host.
        let jump = connection
            .options
            .jump_host
            .as_deref()
            .filter(|_| connection.proxy_chain.is_empty())
            .and_then(|id| self.get(id));
        for record in std::iter::once(connection).chain(jump) {
            serde_json::to_writer(
                &mut digest,
                &(
                    &record.id,
                    record.created_at,
                    &record.host,
                    record.port,
                    &record.username,
                    &record.auth,
                    &record.options.identity_agent,
                    &record.options.totp_credential_id,
                    &record.proxy_chain,
                    (
                        &record.proxy_command,
                        record.proxy_command.as_ref().map(|_| &record.name),
                    ),
                    &record.upstream_proxy,
                    &record.options.jump_host,
                    self.data.ssh_identity_generations.get(&record.id),
                ),
            )
            .ok()?;
        }
        Some(SavedSshIdentity(digest.finalize().into()))
    }

    pub fn upsert_confirmed_profile_edit(
        &mut self,
        request: super::SaveConnectionRequest,
        expected: Option<SavedSshIdentity>,
    ) -> Result<super::ConnectionInfo> {
        let gate = self.revision.gate.clone();
        let generation = gate.lock();
        self.refresh_for_write(generation.get())?;
        if let Some(expected) = expected {
            ensure!(
                request
                    .id
                    .as_deref()
                    .and_then(|id| self.saved_ssh_identity(id))
                    == Some(expected),
                "Connection identity changed during editing"
            );
        }
        self.upsert(request)
    }

    pub fn with_confirmed_ssh_identity<T>(
        &mut self,
        id: &str,
        identity: SavedSshIdentity,
        write: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let gate = self.revision.gate.clone();
        let generation = gate.lock();
        self.refresh_for_write(generation.get())?;
        ensure!(
            self.saved_ssh_identity(id) == Some(identity),
            "Connection identity changed before credential confirmation"
        );
        write(self)
    }
}
