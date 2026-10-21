// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Serialize, Deserialize)]
pub struct PrivilegeCredentialSyncRecord {
    pub metadata: SavedPrivilegeCredential,
    pub secret: Option<SecretString>,
}

impl ConnectionStore {
    pub fn export_privilege_sync_records(
        &self,
        connection_ids: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<PrivilegeCredentialSyncRecord>> {
        self.data
            .connections
            .iter()
            .filter(|connection| connection_ids.contains(&connection.id))
            .flat_map(|connection| connection.privilege_credentials.iter())
            .chain(self.data.ssh_common_privilege_credentials.iter()
                .filter(|_| connection_ids.contains(SSH_COMMON_PRIVILEGE_SCOPE_ID)))
            .map(|credential| {
                let secret = if let Some(reference) = &credential.keychain_id {
                    Some(self.privilege_keychain.get(reference)?)
                } else {
                    credential.plaintext_secret.clone()
                };
                let mut metadata = credential.clone();
                metadata.keychain_id = None;
                metadata.plaintext_secret = None;
                Ok(PrivilegeCredentialSyncRecord { metadata, secret })
            })
            .collect()
    }

    pub fn prepare_privilege_sync(
        &mut self,
        records: Vec<PrivilegeCredentialSyncRecord>,
        owners: &std::collections::BTreeSet<String>,
        mut journal: impl FnMut(&str) -> Result<()>,
    ) -> Result<Vec<String>> {
        let mut prepared = HashMap::<String, Vec<SavedPrivilegeCredential>>::new();
        let mut retained = HashSet::new();
        let mut identities = HashSet::new();
        for record in &records {
            non_empty(&record.metadata.id, "Privilege credential identity")?;
            non_empty(&record.metadata.label, "Privilege credential label")?;
            if !identities.insert((&record.metadata.connection_id, &record.metadata.id)) {
                bail!("Duplicate privilege credential identity");
            }
        }
        for mut record in records {
            if !owners.contains(&record.metadata.connection_id) {
                continue;
            }
            record.metadata.host_patterns =
                normalize_privilege_host_patterns(record.metadata.host_patterns);
            record.metadata.second_factor_patterns = normalize_privilege_second_factor_patterns(
                record.metadata.response_mode,
                record.metadata.second_factor_patterns,
            );
            record.metadata.second_factor_user_hint =
                if record.metadata.response_mode.waits_for_totp() {
                    record
                        .metadata
                        .second_factor_user_hint
                        .as_deref()
                        .map(str::trim)
                        .filter(|hint| !hint.is_empty())
                        .map(ToOwned::to_owned)
                } else {
                    None
                };
            if !record.metadata.response_mode.uses_totp() {
                record.metadata.totp_credential_id = None;
            }
            if record.metadata.connection_id != SSH_COMMON_PRIVILEGE_SCOPE_ID {
                record.metadata.host_patterns.clear();
            } else if !record.metadata.require_click_to_send
                && record.metadata.host_patterns.is_empty()
            {
                // A synced snapshot must not turn a generic shared credential
                // into an automatic sender for every SSH host.
                record.metadata.require_click_to_send = true;
            }
            let old = self.privilege_credentials_for_scope(&record.metadata.connection_id)?
                .iter()
                .find(|credential| credential.id == record.metadata.id);
            let unchanged = if let Some(old) = old {
                let previous = if let Some(reference) = &old.keychain_id {
                    Some(self.privilege_keychain.get(reference)?)
                } else {
                    old.plaintext_secret.clone()
                };
                previous == record.secret
            } else {
                false
            };
            record.metadata.keychain_id =
                if unchanged && old.is_some_and(|old| old.keychain_id.is_some()) {
                    old.and_then(|old| old.keychain_id.clone())
                } else if let Some(secret) = &record.secret {
                    let reference = format!("privilege-sync-{}", Uuid::new_v4());
                    journal(&reference)?;
                    self.privilege_keychain.store(&reference, secret)?;
                    Some(reference)
                } else {
                    None
                };
            record.metadata.plaintext_secret = None;
            if let Some(reference) = &record.metadata.keychain_id {
                retained.insert(reference.clone());
            }
            prepared
                .entry(record.metadata.connection_id.clone())
                .or_default()
                .push(record.metadata);
        }
        let mut stale = Vec::new();
        for owner in owners {
            let credentials = self.privilege_credentials_for_scope_mut(owner)?;
            stale.extend(credentials.iter().filter_map(|credential| credential.keychain_id.as_ref())
                .filter(|reference| !retained.contains(*reference)).cloned());
            *credentials = prepared.remove(owner).unwrap_or_default();
        }
        self.save()?;
        Ok(stale)
    }

    pub fn remove_synced_privilege_slots(&self, slots: &[String]) -> Result<()> {
        for reference in slots {
            if self
                .data
                .connections
                .iter()
                .flat_map(|connection| &connection.privilege_credentials)
                .chain(&self.data.local_privilege_credentials)
                .chain(&self.data.ssh_common_privilege_credentials)
                .any(|credential| credential.keychain_id.as_ref() == Some(reference))
            {
                continue;
            }
            self.privilege_keychain.delete(reference)?;
        }
        Ok(())
    }
}
