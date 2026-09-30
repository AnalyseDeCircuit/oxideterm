use super::*;
use crate::totp::{
    TotpCredential, TotpError, TotpGenerator, TotpParameters, validate_totp_pattern,
};

#[derive(Clone)]
pub struct TotpResolver {
    path: PathBuf,
    keychain: ConnectionKeychain,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::totp::DEFAULT_TOTP_PROMPT;

    const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn credential(store: &mut ConnectionStore) -> String {
        store
            .save_totp_credential(
                None,
                "JumpServer",
                Some(&SecretString::from(SEED)),
                TotpParameters::default(),
                DEFAULT_TOTP_PROMPT,
                true,
            )
            .unwrap()
    }

    fn connection(store: &mut ConnectionStore, credential_id: &str, name: &str) -> String {
        let mut connection: SavedConnection = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4().to_string(), "name": name, "host": "server.test",
            "username": "user", "auth": {"type": "keyboard_interactive"},
            "created_at": "2026-09-30T00:00:00Z"
        }))
        .unwrap();
        connection.options.totp_credential_id = Some(credential_id.into());
        let id = connection.id.clone();
        store.add_connection(connection);
        store.save().unwrap();
        id
    }

    #[test]
    fn totp_store_protects_seed_and_live_binding_observes_edits_and_revocation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = ConnectionStore::load(directory.path().join("connections.json")).unwrap();
        let uri = format!("otpauth://totp/Test?secret={SEED}&digits=8");
        let id = store
            .save_totp_credential(
                None,
                "Shared MFA",
                Some(&SecretString::from(uri.clone())),
                TotpParameters::default(),
                DEFAULT_TOTP_PROMPT,
                true,
            )
            .unwrap();
        let binding = store.totp_binding(Some(&id)).unwrap();
        assert_eq!(
            binding.resolve().unwrap().1.generate(59).as_str(),
            "94287082"
        );
        let metadata = serde_json::to_string(&store.data).unwrap();
        assert!(!metadata.contains(SEED));
        assert!(!metadata.contains(&uri));
        let old_reference = store.totp_credentials()[0].secret_reference.clone();
        // An imported URI must not override later edits to the stored parameters.
        store
            .save_totp_credential(
                Some(&id),
                "Shared MFA",
                None,
                TotpParameters::default(),
                DEFAULT_TOTP_PROMPT,
                true,
            )
            .unwrap();
        assert_eq!(binding.resolve().unwrap().1.generate(59).as_str(), "287082");
        assert_eq!(store.totp_credentials()[0].secret_reference, old_reference);
        let owner = connection(&mut store, &id, "Target");
        assert_eq!(store.delete_totp_credential(&id), Err(TotpError::InUse));
        store
            .save_totp_credential(
                Some(&id),
                "Shared MFA",
                None,
                TotpParameters::default(),
                DEFAULT_TOTP_PROMPT,
                false,
            )
            .unwrap();
        assert!(matches!(binding.resolve(), Err(TotpError::Unavailable)));
        store.data.connections.retain(|p| p.id != owner);
        store.delete_totp_credential(&id).unwrap();
        assert!(matches!(binding.metadata(), Err(TotpError::Unavailable)));
    }

    #[test]
    fn totp_failed_save_preserves_previous_metadata_and_seed() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = ConnectionStore::load(directory.path().join("connections.json")).unwrap();
        let id = credential(&mut store);
        let binding = store.totp_binding(Some(&id)).unwrap();
        let previous = store.totp_credentials()[0].clone();
        let path = store.path.clone();
        store.path = directory.path().to_path_buf();
        assert_eq!(
            store.save_totp_credential(
                Some(&id),
                "Changed",
                Some(&SecretString::from("JBSWY3DPEHPK3PXP")),
                TotpParameters::default(),
                DEFAULT_TOTP_PROMPT,
                true
            ),
            Err(TotpError::SaveFailed)
        );
        store.path = path;
        assert_eq!(store.totp_credentials(), &[previous]);
        assert_eq!(binding.resolve().unwrap().1.generate(59).as_str(), "287082");
    }

    #[test]
    fn totp_shared_credential_sync_preserves_selection_and_seed_revision() {
        let directory = tempfile::tempdir().unwrap();
        let mut source = ConnectionStore::load(directory.path().join("source.json")).unwrap();
        let id = credential(&mut source);
        let owner = connection(&mut source, &id, "One");
        connection(&mut source, &id, "Two");
        let selection = CredentialSyncSelection {
            connection_ids: [owner].into_iter().collect(),
            ..Default::default()
        };
        let snapshot = source.export_saved_connections_snapshot().unwrap();
        let metadata = serde_json::to_string(&snapshot).unwrap();
        assert!(!metadata.contains(SEED));
        assert!(!metadata.contains(&source.totp_credentials()[0].secret_reference));
        let secrets = source.export_profile_credentials(&selection, None).unwrap();
        assert_eq!(
            secrets
                .iter()
                .map(|s| serde_json::from_str::<CredentialTarget>(&s.id)
                    .unwrap()
                    .owner)
                .collect::<Vec<_>>(),
            [CredentialOwner::Totp(id.clone())]
        );
        let mut target = ConnectionStore::load(directory.path().join("target.json")).unwrap();
        target
            .apply_saved_connections_snapshot(snapshot, SavedConnectionsConflictStrategy::Replace)
            .unwrap();
        let binding = target.totp_binding(Some(&id)).unwrap();
        assert!(matches!(binding.resolve(), Err(TotpError::Unavailable)));
        let mut skipped = target
            .prepare_profile_credentials(&secrets, &Default::default(), &mut None)
            .unwrap();
        target.commit_profile_credentials(&mut skipped).unwrap();
        assert!(matches!(binding.resolve(), Err(TotpError::Unavailable)));
        let mut prepared = target
            .prepare_profile_credentials(&secrets, &selection, &mut None)
            .unwrap();
        target.save().unwrap();
        target.commit_profile_credentials(&mut prepared).unwrap();
        assert_eq!(binding.resolve().unwrap().1.generate(59).as_str(), "287082");
        assert_eq!(
            target
                .connections()
                .iter()
                .map(|p| p.options.totp_credential_id.as_deref())
                .collect::<Vec<_>>(),
            [Some(id.as_str()), Some(id.as_str())]
        );
        source
            .save_totp_credential(
                Some(&id),
                "Rotated",
                Some(&SecretString::from("JBSWY3DPEHPK3PXP")),
                TotpParameters::default(),
                DEFAULT_TOTP_PROMPT,
                true,
            )
            .unwrap();
        target
            .apply_saved_connections_snapshot(
                source.export_saved_connections_snapshot().unwrap(),
                SavedConnectionsConflictStrategy::Replace,
            )
            .unwrap();
        assert!(
            matches!(binding.resolve(), Err(TotpError::Unavailable)),
            "metadata-only rotation cannot reuse the old seed"
        );
        let stale = target
            .prepare_profile_credentials(&secrets, &selection, &mut None)
            .unwrap();
        assert_eq!(stale.summary.skipped, 1);
        target.rollback_profile_credentials(stale).unwrap();
    }

    #[test]
    fn totp_encrypted_archive_restores_shared_binding_only_with_secret_opt_in() {
        use crate::oxide_file::*;
        let directory = tempfile::tempdir().unwrap();
        let mut source = ConnectionStore::load(directory.path().join("source.json")).unwrap();
        let id = credential(&mut source);
        let owners = vec![
            connection(&mut source, &id, "One"),
            connection(&mut source, &id, "Two"),
        ];
        let bytes = export_connections_to_oxide(
            &source,
            &owners,
            "archive-password",
            OxideExportOptions {
                include_passwords: true,
                ..Default::default()
            },
        )
        .unwrap();
        for import_secrets in [false, true] {
            let mut target = ConnectionStore::load(
                directory
                    .path()
                    .join(format!("target-{import_secrets}.json")),
            )
            .unwrap();
            apply_oxide_import_with_options(
                &mut target,
                &bytes,
                "archive-password",
                OxideImportOptions {
                    import_portable_secrets: import_secrets,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                target
                    .totp_credentials()
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>(),
                [id.as_str()]
            );
            assert_eq!(
                target
                    .connections()
                    .iter()
                    .map(|p| p.options.totp_credential_id.as_deref())
                    .collect::<Vec<_>>(),
                [Some(id.as_str()), Some(id.as_str())]
            );
            let resolved = target.totp_resolver().resolve(&id);
            if import_secrets {
                assert_eq!(resolved.unwrap().1.generate(59).as_str(), "287082");
            } else {
                assert!(matches!(resolved, Err(TotpError::Unavailable)));
            }
        }
    }
}

#[derive(Clone)]
pub struct TotpBinding {
    pub credential_id: String,
    resolver: TotpResolver,
}

impl std::fmt::Debug for TotpBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TotpBinding")
            .field("credential_id", &self.credential_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for TotpBinding {
    fn eq(&self, other: &Self) -> bool {
        self.credential_id == other.credential_id && self.resolver.path == other.resolver.path
    }
}
impl Eq for TotpBinding {}

impl TotpBinding {
    pub fn metadata(&self) -> Result<TotpCredential, TotpError> {
        self.resolver.metadata(&self.credential_id)
    }
    pub fn resolve(&self) -> Result<(TotpCredential, TotpGenerator), TotpError> {
        self.resolver.resolve(&self.credential_id)
    }
}

impl TotpResolver {
    fn metadata(&self, id: &str) -> Result<TotpCredential, TotpError> {
        let bytes = fs::read(&self.path).map_err(|_| TotpError::Unavailable)?;
        let data = decode_connection_store_data(&bytes)
            .map_err(|_| TotpError::Unavailable)?
            .data;
        data.totp_credentials
            .into_iter()
            .find(|entry| entry.id == id && entry.enabled)
            .ok_or(TotpError::Unavailable)
    }
    pub fn resolve(&self, id: &str) -> Result<(TotpCredential, TotpGenerator), TotpError> {
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Security,
            "credential_read",
            Some(id),
            Some("totp"),
        );
        let result = (|| {
            // Read current metadata on demand so edits and revocation apply to reconnects,
            // without retaining a seed in the long-lived connection registry.
            let credential = self.metadata(id)?;
            let secret = self
                .keychain
                .get(&credential.secret_reference)
                .map_err(|_| TotpError::Unavailable)?;
            let generator = TotpGenerator::parse(secret.expose_secret(), credential.parameters)?;
            Ok((credential, generator))
        })();
        audit.result(&result);
        result
    }
}

impl ConnectionStore {
    pub fn totp_binding(&self, credential_id: Option<&str>) -> Option<TotpBinding> {
        credential_id.map(|id| TotpBinding {
            credential_id: id.into(),
            resolver: self.totp_resolver(),
        })
    }
    pub fn totp_credentials(&self) -> &[TotpCredential] {
        &self.data.totp_credentials
    }

    pub fn totp_resolver(&self) -> TotpResolver {
        TotpResolver {
            path: self.path.clone(),
            keychain: self.keychain.clone(),
        }
    }

    pub(crate) fn export_totp_secret(
        &self,
        credential: &TotpCredential,
    ) -> Result<Option<crate::oxide_file::EncryptedPortableSecret>> {
        if credential.secret_reference.is_empty() {
            return Ok(None);
        }
        Ok(Some(crate::oxide_file::EncryptedPortableSecret {
            kind: PROFILE_CREDENTIAL_KIND.into(),
            id: serde_json::to_string(&CredentialTarget {
                owner: CredentialOwner::Totp(credential.id.clone()),
                slot: CredentialSlot::Primary,
                identity: credential.secret_revision.clone(),
            })?,
            secret: self
                .keychain
                .get(&credential.secret_reference)
                .context("Could not read TOTP secret from protected storage")?
                .into_zeroizing(),
        }))
    }

    pub fn save_totp_credential(
        &mut self,
        id: Option<&str>,
        name: &str,
        input: Option<&SecretString>,
        parameters: TotpParameters,
        prompt_pattern: &str,
        enabled: bool,
    ) -> Result<String, TotpError> {
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Configuration,
            "credential_set",
            id,
            Some("totp"),
        );
        let result = (|| {
            let name = name.trim();
            if name.is_empty() || name.len() > 256 {
                return Err(TotpError::InvalidParameters);
            }
            validate_totp_pattern(prompt_pattern)?;
            let previous = id
                .and_then(|id| {
                    self.data
                        .totp_credentials
                        .iter()
                        .find(|entry| entry.id == id)
                })
                .cloned();
            if id.is_some() && previous.is_none() {
                return Err(TotpError::Unavailable);
            }
            let id = id
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let generator = input
                .map(|secret| TotpGenerator::parse(secret.expose_secret(), parameters))
                .transpose()?;
            let reference = if generator.is_some() {
                format!("totp:{}", Uuid::new_v4())
            } else {
                previous
                    .as_ref()
                    .ok_or(TotpError::InvalidSecret)?
                    .secret_reference
                    .clone()
            };
            let secret_revision = previous
                .as_ref()
                .filter(|_| input.is_none())
                .map(|p| p.secret_revision.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let credential = TotpCredential {
                id: id.clone(),
                name: name.into(),
                parameters: generator
                    .as_ref()
                    .map_or(parameters, TotpGenerator::parameters),
                prompt_pattern: prompt_pattern.into(),
                enabled,
                updated_at: Utc::now(),
                secret_revision,
                secret_reference: reference.clone(),
            };
            credential.validate()?;
            if let Some(generator) = &generator {
                let secret = SecretString::from(generator.encoded_secret());
                self.keychain
                    .store(&reference, &secret)
                    .map_err(|_| TotpError::SaveFailed)?;
            }
            let mut candidate = self.clone();
            candidate
                .data
                .totp_credentials
                .retain(|entry| entry.id != id);
            candidate.data.totp_credentials.push(credential);
            if let Some(previous) = &previous
                && input.is_some()
                && !previous.secret_reference.is_empty()
            {
                candidate
                    .data
                    .pending_keychain_cleanup
                    .push(previous.secret_reference.clone());
            }
            if candidate.save().is_err() {
                if input.is_some() {
                    let _ = self.keychain.delete(&reference);
                }
                return Err(TotpError::SaveFailed);
            }
            self.data = candidate.data;
            let _ = self.retry_persisted_keychain_cleanup();
            Ok(id)
        })();
        audit.result(&result);
        result
    }

    pub fn delete_totp_credential(&mut self, id: &str) -> Result<(), TotpError> {
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::Configuration,
            "credential_delete",
            Some(id),
            Some("totp"),
        );
        let result = (|| {
            let all = CredentialSyncSelection {
                connection_ids: self.data.connections.iter().map(|p| p.id.clone()).collect(),
                mosh_ids: self
                    .data
                    .mosh_profiles
                    .iter()
                    .map(|p| p.id.clone())
                    .collect(),
                sftp_ids: self
                    .data
                    .standalone_sftp_profiles
                    .iter()
                    .map(|p| p.id.clone())
                    .collect(),
                ..Default::default()
            };
            if self.totp_selected(id, &all) {
                return Err(TotpError::InUse);
            }
            let previous = self
                .data
                .totp_credentials
                .iter()
                .find(|entry| entry.id == id)
                .ok_or(TotpError::Unavailable)?;
            let mut candidate = self.clone();
            candidate
                .data
                .pending_keychain_cleanup
                .push(previous.secret_reference.clone());
            candidate
                .data
                .totp_credentials
                .retain(|entry| entry.id != id);
            candidate.save().map_err(|_| TotpError::SaveFailed)?;
            self.data = candidate.data;
            let _ = self.retry_persisted_keychain_cleanup();
            Ok(())
        })();
        audit.result(&result);
        result
    }

    pub(crate) fn totp_selected(&self, id: &str, selection: &CredentialSyncSelection) -> bool {
        let hops_use = |hops: &[SavedProxyHop]| {
            hops.iter()
                .any(|h| h.totp_credential_id.as_deref() == Some(id))
        };
        self.data.connections.iter().any(|p| {
            selection.connection_ids.contains(&p.id)
                && (p.options.totp_credential_id.as_deref() == Some(id)
                    || hops_use(&p.proxy_chain)
                    || p.options
                        .jump_host
                        .as_deref()
                        .and_then(|jump| self.get(jump))
                        .is_some_and(|jump| jump.options.totp_credential_id.as_deref() == Some(id)))
        }) || self
            .data
            .mosh_profiles
            .iter()
            .any(|p| selection.mosh_ids.contains(&p.id) && hops_use(&p.proxy_chain))
            || self.data.standalone_sftp_profiles.iter().any(|p| {
                selection.sftp_ids.contains(&p.id)
                    && (hops_use(&p.proxy_chain)
                        || p.secondary_endpoint
                            .as_ref()
                            .is_some_and(|e| hops_use(&e.proxy_chain)))
            })
    }

    pub(crate) fn merge_totp_credentials(
        &mut self,
        credentials: Vec<TotpCredential>,
        replace: bool,
    ) -> Result<usize> {
        let mut seen = std::collections::HashSet::new();
        for credential in &credentials {
            credential.validate()?;
            if !seen.insert(&credential.id) {
                anyhow::bail!("Duplicate TOTP credential");
            }
        }
        let mut changed = 0;
        for mut credential in credentials {
            credential.secret_reference.clear();
            if let Some(existing) = self
                .data
                .totp_credentials
                .iter_mut()
                .find(|p| p.id == credential.id)
            {
                if !replace && existing.updated_at >= credential.updated_at {
                    continue;
                }
                if existing.secret_revision == credential.secret_revision {
                    credential.secret_reference = existing.secret_reference.clone();
                } else if !existing.secret_reference.is_empty() {
                    self.data
                        .pending_keychain_cleanup
                        .push(existing.secret_reference.clone());
                }
                *existing = credential;
            } else {
                self.data.totp_credentials.push(credential);
            }
            changed += 1;
        }
        Ok(changed)
    }
}
