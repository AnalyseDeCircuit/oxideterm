/// Saved metadata survives plugin removal. Credentials remain in the host secret store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginConnectionProfile {
    pub id: String,
    pub plugin_id: String,
    pub type_id: String,
    pub name: String,
    pub group: Option<String>,
    pub config: std::collections::BTreeMap<String, serde_json::Value>,
    pub credential_refs: std::collections::BTreeMap<String, Option<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl PluginConnectionProfile {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty()
            || self.plugin_id.trim().is_empty()
            || self.type_id.trim().is_empty()
            || self.name.trim().is_empty()
        {
            bail!("Invalid plugin connection identity");
        }
        normalize_optional_group_name(self.group.as_deref())?;
        Ok(())
    }

    pub fn portable(&self) -> Self {
        let mut profile = self.clone();
        profile.last_used_at = None;
        profile
            .credential_refs
            .values_mut()
            .for_each(|reference| *reference = None);
        profile
    }

    pub(crate) fn credential_identity(&self) -> String {
        sha256_hex(&(&self.plugin_id, &self.type_id, &self.config))
            .expect("serializable plugin configuration")
    }

    pub(crate) fn preserve_local_credentials(&mut self, previous: Option<&Self>) {
        let matches =
            previous.filter(|old| old.credential_identity() == self.credential_identity());
        for (field, reference) in &mut self.credential_refs {
            *reference = matches
                .and_then(|old| old.credential_refs.get(field))
                .cloned()
                .flatten();
        }
        self.last_used_at = previous.and_then(|old| old.last_used_at);
    }
}

impl ConnectionStore {
    pub(crate) fn stage_imported_plugin_profile(
        &mut self,
        mut profile: PluginConnectionProfile,
    ) -> Result<()> {
        profile.validate()?;
        let old = self
            .data
            .plugin_profiles
            .iter()
            .find(|p| p.id == profile.id);
        profile.preserve_local_credentials(old);
        if let Some(old) = old {
            profile.created_at = old.created_at;
            self.data.pending_keychain_cleanup.extend(
                old.credential_refs
                    .values()
                    .flatten()
                    .filter(|old| {
                        !profile
                            .credential_refs
                            .values()
                            .flatten()
                            .any(|kept| kept == *old)
                    })
                    .cloned(),
            );
        }
        profile.updated_at = Utc::now();
        self.data.plugin_profiles.retain(|p| p.id != profile.id);
        self.data.plugin_tombstones.retain(|p| p.id != profile.id);
        self.data.plugin_profiles.push(profile);
        self.normalize();
        Ok(())
    }
    pub fn mark_plugin_profile_used(&mut self, id: &str) -> Result<()> {
        let Some(profile) = self
            .data
            .plugin_profiles
            .iter_mut()
            .find(|profile| profile.id == id)
        else {
            return Ok(());
        };
        let previous = profile.last_used_at;
        profile.last_used_at = Some(Utc::now());
        if let Err(error) = self.save() {
            self.data
                .plugin_profiles
                .iter_mut()
                .find(|profile| profile.id == id)
                .unwrap()
                .last_used_at = previous;
            return Err(error);
        }
        Ok(())
    }

    fn apply_plugin_profiles(
        &mut self,
        profiles: Vec<PluginConnectionProfile>,
        tombstones: Vec<DeletedConnectionTombstone>,
        strategy: SavedConnectionsConflictStrategy,
        result: &mut ApplySavedConnectionsSyncSnapshotResult,
    ) -> Result<()> {
        for tombstone in tombstones {
            if self
                .data
                .plugin_profiles
                .iter()
                .any(|p| p.id == tombstone.id && p.updated_at > tombstone.deleted_at)
            {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            if self
                .data
                .plugin_tombstones
                .iter()
                .any(|p| p.id == tombstone.id && p.deleted_at >= tombstone.deleted_at)
            {
                continue;
            }
            self.data.plugin_profiles.retain(|p| p.id != tombstone.id);
            self.data.plugin_tombstones.retain(|p| p.id != tombstone.id);
            self.data.plugin_tombstones.push(tombstone);
            result.applied += 1;
        }
        for mut profile in profiles {
            profile.validate()?;
            if self
                .data
                .plugin_tombstones
                .iter()
                .any(|p| p.id == profile.id && p.deleted_at >= profile.updated_at)
            {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            let old = self
                .data
                .plugin_profiles
                .iter()
                .find(|p| p.id == profile.id);
            if old.is_some_and(|p| {
                p.updated_at > profile.updated_at
                    && strategy != SavedConnectionsConflictStrategy::Replace
            }) {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            if old.is_none()
                && strategy == SavedConnectionsConflictStrategy::Skip
                && self.data.plugin_profiles.iter().any(|p| {
                    p.name == profile.name
                        && p.plugin_id == profile.plugin_id
                        && p.type_id == profile.type_id
                })
            {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            profile.preserve_local_credentials(old);
            self.data.plugin_tombstones.retain(|p| p.id != profile.id);
            self.data.plugin_profiles.retain(|p| p.id != profile.id);
            self.data.plugin_profiles.push(profile);
            result.applied += 1;
        }
        Ok(())
    }

    pub fn plugin_profiles(&self) -> &[PluginConnectionProfile] {
        &self.data.plugin_profiles
    }

    pub fn save_plugin_profile(&mut self, mut profile: PluginConnectionProfile) -> Result<()> {
        profile.validate()?;
        profile.name = profile.name.trim().to_string();
        profile.group = normalize_optional_group_name(profile.group.as_deref())?;
        profile.updated_at = Utc::now();
        let previous = self.data.clone();
        let previous_credentials =
            self.stored_credential_targets(&CredentialOwner::PluginConnection(profile.id.clone()));
        if let Some(old) = self
            .data
            .plugin_profiles
            .iter()
            .find(|old| old.id == profile.id)
        {
            if old.plugin_id != profile.plugin_id || old.type_id != profile.type_id {
                bail!("Cannot change a saved connection provider");
            }
            self.data.pending_keychain_cleanup.extend(
                old.credential_refs
                    .values()
                    .flatten()
                    .filter(|reference| {
                        !profile
                            .credential_refs
                            .values()
                            .flatten()
                            .any(|kept| kept == *reference)
                    })
                    .cloned(),
            );
        }
        self.data
            .plugin_profiles
            .retain(|saved| saved.id != profile.id);
        if let Some(group) = &profile.group {
            self.ensure_group(group.clone())?;
        }
        self.data.plugin_profiles.push(profile);
        self.data.plugin_tombstones.retain(|entry| {
            !self
                .data
                .plugin_profiles
                .iter()
                .any(|profile| profile.id == entry.id)
        });
        self.record_cleared_credentials(previous_credentials);
        if let Err(error) = self.save() {
            self.data = previous;
            return Err(error);
        }
        let _ = self.retry_persisted_keychain_cleanup();
        Ok(())
    }

    pub fn delete_plugin_profile(&mut self, id: &str) -> Result<bool> {
        if !self
            .data
            .plugin_profiles
            .iter()
            .any(|profile| profile.id == id)
        {
            return Ok(false);
        }
        let previous = self.data.clone();
        let credentials = self
            .data
            .plugin_profiles
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .credential_refs
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        self.data.pending_keychain_cleanup.extend(credentials);
        self.data.plugin_profiles.retain(|profile| profile.id != id);
        self.data.plugin_tombstones.retain(|entry| entry.id != id);
        self.data
            .plugin_tombstones
            .push(DeletedConnectionTombstone {
                id: id.into(),
                deleted_at: Utc::now(),
            });
        self.data
            .cleared_credentials
            .retain(|target| target.owner != CredentialOwner::PluginConnection(id.into()));
        if let Err(error) = self.save() {
            self.data = previous;
            return Err(error);
        }
        let _ = self.retry_persisted_keychain_cleanup();
        Ok(true)
    }

    pub fn save_plugin_profile_credentials(
        &mut self,
        mut profile: PluginConnectionProfile,
        secrets: std::collections::BTreeMap<String, SecretString>,
    ) -> Result<()> {
        let mut created: Vec<String> = Vec::new();
        let old = self.data.clone();
        for (field, secret) in secrets {
            profile.config.remove(&field);
            if secret.expose_secret().is_empty() {
                continue;
            }
            let reference = format!("plugin-connection-{}", Uuid::new_v4());
            if let Err(error) = self.keychain.store(&reference, &secret) {
                for reference in created {
                    let _ = self.keychain.delete(&reference);
                }
                return Err(error);
            }
            created.push(reference.clone());
            profile.credential_refs.insert(field, Some(reference));
        }
        if let Err(error) = self.save_plugin_profile(profile) {
            self.data = old;
            for reference in created {
                let _ = self.keychain.delete(&reference);
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn plugin_connection_credentials(
        &self,
        id: &str,
    ) -> Result<std::collections::BTreeMap<String, SecretString>> {
        self.plugin_connection_credentials_for_fields(id, |_| true)
    }

    /// Read only declared password fields; obsolete references must not block opening a profile.
    pub fn plugin_connection_credentials_for_fields(
        &self,
        id: &str,
        accepts: impl Fn(&str) -> bool,
    ) -> Result<std::collections::BTreeMap<String, SecretString>> {
        let profile = self
            .plugin_profiles()
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("Plugin connection no longer exists"))?;
        profile
            .credential_refs
            .iter()
            .filter(|(field, _)| accepts(field))
            .filter_map(|(field, reference)| reference.as_ref().map(|reference| (field, reference)))
            .map(|(field, reference)| {
                self.keychain
                    .get_preserving_multiline(reference)
                    .map(|secret| (field.clone(), secret))
            })
            .collect()
    }
}
