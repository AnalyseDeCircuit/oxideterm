// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Serialize, Deserialize)]
pub struct ManagedSshKeySyncRecord {
    pub metadata: ManagedSshKey,
    pub private_key: SecretString,
}

impl ManagedSshKeySyncRecord {
    pub fn validate_certificate(&self) -> Result<()> {
        if let Some(certificate) = &self.metadata.certificate {
            let certificate = russh::keys::ssh_key::Certificate::from_openssh(certificate)
                .map_err(|_| anyhow::anyhow!("Invalid synchronized SSH certificate"))?;
            let key = PrivateKey::from_openssh(self.private_key.expose_secret())
                .or_else(|_| russh::keys::decode_secret_key(self.private_key.expose_secret(), None))
                .map_err(|_| anyhow::anyhow!("Invalid synchronized SSH private key"))?;
            if certificate.public_key() != key.public_key().key_data() {
                bail!("Synchronized SSH certificate does not match its private key");
            }
        }
        Ok(())
    }
    pub fn from_private_key(id: String, name: String, private_key: SecretString) -> Result<Self> {
        let key = PrivateKey::from_openssh(private_key.expose_secret())
            .or_else(|_| russh::keys::decode_secret_key(private_key.expose_secret(), None))
            .map_err(|_| anyhow::anyhow!("Invalid synchronized SSH private key"))?;
        let now = Utc::now();
        Ok(Self {
            metadata: ManagedSshKey {
                certificate: None,
                id,
                name,
                secret_id: String::new(),
                fingerprint: fingerprint_public_key(key.public_key()),
                public_key: public_key_line_from_private_key(&key),
                requires_passphrase: key.is_encrypted(),
                origin: ManagedSshKeyOrigin::OxideImport,
                created_at: now,
                updated_at: now,
            },
            private_key,
        })
    }
}

impl ConnectionStore {
    pub fn snapshot_archived_connections(
        &self,
        records: Vec<SavedConnection>,
    ) -> Result<(
        SavedConnectionsSyncSnapshot,
        Vec<crate::oxide_file::EncryptedPortableSecret>,
    )> {
        let mut staged = self.clone();
        staged.data = ConnectionStoreData::default();
        staged.data.connections = records;
        let selection = CredentialSyncSelection {
            connection_ids: staged
                .data
                .connections
                .iter()
                .map(|record| record.id.clone())
                .collect(),
            ..Default::default()
        };
        Ok((
            build_saved_connections_sync_snapshot(&staged.data)?,
            staged.export_sync_credentials(&selection, None)?,
        ))
    }
    pub fn export_managed_key_for_sync(&self, id: &str) -> Result<ManagedSshKeySyncRecord> {
        let mut metadata = self.managed_ssh_key_metadata(id)?;
        let private_key = self.resolve_managed_ssh_key_private_key(id)?;
        metadata.secret_id.clear();
        Ok(ManagedSshKeySyncRecord {
            metadata,
            private_key,
        })
    }

    /// The caller journals each new protected slot before creation and retains
    /// returned old slots until the surrounding configuration commit is durable.
    pub fn prepare_managed_key_sync(
        &mut self,
        mut incoming: ManagedSshKeySyncRecord,
        mut record_creation: impl FnMut(&str) -> Result<()>,
    ) -> Result<Option<String>> {
        non_empty(&incoming.metadata.id, "Managed key identity")?;
        let key = PrivateKey::from_openssh(incoming.private_key.expose_secret())
            .or_else(|_| russh::keys::decode_secret_key(incoming.private_key.expose_secret(), None))
            .map_err(|_| anyhow::anyhow!("Invalid synchronized SSH private key"))?;
        if fingerprint_public_key(key.public_key()) != incoming.metadata.fingerprint {
            bail!("Synchronized SSH key fingerprint does not match its contents");
        }
        incoming.metadata.public_key = public_key_line_from_private_key(&key);
        if let Some(certificate) = &incoming.metadata.certificate {
            let certificate = russh::keys::ssh_key::Certificate::from_openssh(certificate)
                .map_err(|_| anyhow::anyhow!("Invalid synchronized SSH certificate"))?;
            if certificate.public_key() != key.public_key().key_data() {
                bail!("Synchronized SSH certificate does not match its private key");
            }
        }
        incoming.metadata.requires_passphrase = key.is_encrypted();
        let previous = self
            .data
            .managed_ssh_keys
            .iter()
            .find(|key| key.id == incoming.metadata.id)
            .cloned();
        let unchanged = previous
            .as_ref()
            .map(|old| {
                self.get_managed_ssh_key_secret(&old.secret_id)
                    .map(|secret| secret == incoming.private_key)
            })
            .transpose()?
            .unwrap_or(false);
        let stale = if unchanged {
            incoming.metadata.secret_id = previous.as_ref().unwrap().secret_id.clone();
            None
        } else {
            let reference = format!("managed-key-sync-{}", Uuid::new_v4());
            record_creation(&reference)?;
            self.store_managed_ssh_key_secret(&reference, &incoming.private_key)?;
            incoming.metadata.secret_id = reference;
            previous.map(|old| old.secret_id)
        };
        self.data
            .managed_ssh_keys
            .retain(|key| key.id != incoming.metadata.id);
        self.data.managed_ssh_keys.push(incoming.metadata);
        Ok(stale)
    }

    pub fn remove_synced_managed_key_slots(&self, slots: &[String]) -> Result<()> {
        for reference in slots {
            if self
                .data
                .managed_ssh_keys
                .iter()
                .any(|key| &key.secret_id == reference)
            {
                continue;
            }
            if !reference.starts_with("managed-key-") {
                bail!("Invalid managed key slot");
            }
            self.delete_managed_ssh_key_secret(reference)?;
        }
        Ok(())
    }
}
