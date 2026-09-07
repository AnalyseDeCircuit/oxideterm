use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileCredentialRestoreSummary {
    pub restored: usize,
    pub cleared: usize,
    pub skipped: usize,
}

/// New slots are staged separately; old slots remain usable until metadata commits.
#[derive(Debug)]
#[must_use]
pub struct PreparedProfileCredentials {
    created: Vec<String>,
    stale: Vec<String>,
    pub summary: ProfileCredentialRestoreSummary,
}

impl PreparedProfileCredentials {
    pub fn pending_cleanup_count(&self) -> usize {
        self.stale.len()
    }
}

pub fn is_profile_credential(secret: &EncryptedPortableSecret) -> bool {
    matches!(
        secret.kind.as_str(),
        PROFILE_CREDENTIAL_KIND | CLEARED_PROFILE_CREDENTIAL_KIND
    )
}

fn set_auth_reference(auth: &mut SavedAuth, reference: Option<String>) -> Result<()> {
    match auth {
        SavedAuth::Password {
            keychain_id,
            plaintext_password,
        } => {
            *keychain_id = reference;
            *plaintext_password = None;
        }
        SavedAuth::Key {
            passphrase_keychain_id,
            plaintext_passphrase,
            has_passphrase,
            ..
        }
        | SavedAuth::Certificate {
            passphrase_keychain_id,
            plaintext_passphrase,
            has_passphrase,
            ..
        } => {
            *has_passphrase = reference.is_some();
            *passphrase_keychain_id = reference;
            *plaintext_passphrase = None;
        }
        SavedAuth::ManagedKey {
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        } => {
            *passphrase_keychain_id = reference;
            *plaintext_passphrase = None;
        }
        SavedAuth::KerberosPreferred { fallback, .. } => {
            return set_auth_reference(fallback, reference);
        }
        _ => bail!("Credential does not match the selected authentication method"),
    }
    Ok(())
}

fn set_proxy_reference(
    proxy: &mut SavedUpstreamProxyConfig,
    reference: Option<String>,
) -> Result<()> {
    let SavedUpstreamProxyAuth::Password {
        keychain_id,
        plaintext_password,
        ..
    } = &mut proxy.auth
    else {
        bail!("Proxy does not use password authentication");
    };
    *keychain_id = reference;
    *plaintext_password = None;
    Ok(())
}

fn set_policy_reference(
    policy: &mut SavedUpstreamProxyPolicy,
    reference: Option<String>,
) -> Result<()> {
    let SavedUpstreamProxyPolicy::Custom { proxy } = policy else {
        bail!("Proxy credential target is unavailable");
    };
    set_proxy_reference(proxy, reference)
}

fn update_reference(
    data: &mut ConnectionStoreData,
    target: &CredentialTarget,
    reference: Option<String>,
    global: &mut Option<SavedUpstreamProxyConfig>,
) -> Result<()> {
    match &target.owner {
        CredentialOwner::Connection(id) => {
            let p = data
                .connections
                .iter_mut()
                .find(|p| &p.id == id)
                .context("Connection is unavailable")?;
            set_policy_reference(&mut p.upstream_proxy, reference)?;
        }
        CredentialOwner::Mosh(id) => {
            let p = data
                .mosh_profiles
                .iter_mut()
                .find(|p| &p.id == id)
                .context("Mosh profile is unavailable")?;
            let auth = match target.slot {
                CredentialSlot::Primary => &mut p.auth,
                CredentialSlot::Hop(i) => {
                    &mut p
                        .proxy_chain
                        .get_mut(i)
                        .context("Mosh hop is unavailable")?
                        .auth
                }
                _ => bail!("Invalid Mosh credential slot"),
            };
            set_auth_reference(auth, reference)?;
        }
        CredentialOwner::StandaloneSftp(id) => {
            let p = data
                .standalone_sftp_profiles
                .iter_mut()
                .find(|p| &p.id == id)
                .context("SFTP profile is unavailable")?;
            match target.slot {
                CredentialSlot::Primary => set_auth_reference(&mut p.auth, reference)?,
                CredentialSlot::Hop(i) => set_auth_reference(
                    &mut p
                        .proxy_chain
                        .get_mut(i)
                        .context("SFTP hop is unavailable")?
                        .auth,
                    reference,
                )?,
                CredentialSlot::UpstreamProxy => {
                    set_policy_reference(&mut p.upstream_proxy, reference)?
                }
                ref slot => {
                    let endpoint = p
                        .secondary_endpoint
                        .as_mut()
                        .context("Secondary SFTP endpoint is unavailable")?;
                    match slot {
                        CredentialSlot::SecondaryPrimary => {
                            set_auth_reference(&mut endpoint.auth, reference)?
                        }
                        CredentialSlot::SecondaryHop(i) => set_auth_reference(
                            &mut endpoint
                                .proxy_chain
                                .get_mut(*i)
                                .context("Secondary SFTP hop is unavailable")?
                                .auth,
                            reference,
                        )?,
                        CredentialSlot::SecondaryProxy => {
                            set_policy_reference(&mut endpoint.upstream_proxy, reference)?
                        }
                        _ => bail!("Invalid SFTP credential slot"),
                    }
                }
            }
        }
        CredentialOwner::RemoteDesktop(id) => {
            data.remote_desktop_profiles
                .iter_mut()
                .find(|p| &p.id == id)
                .context("Remote desktop profile is unavailable")?
                .credential_ref = reference;
        }
        CredentialOwner::GlobalProxy => {
            data.synced_global_proxy_reference = reference.clone();
            data.global_proxy_credential_cleared = reference.is_none();
            set_proxy_reference(
                global.as_mut().context("Global proxy is unavailable")?,
                reference,
            )?;
        }
    }
    Ok(())
}

impl ConnectionStore {
    pub fn prepare_profile_credentials(
        &mut self,
        secrets: &[EncryptedPortableSecret],
        selection: &CredentialSyncSelection,
        global_proxy: &mut Option<SavedUpstreamProxyConfig>,
    ) -> Result<PreparedProfileCredentials> {
        let available = self
            .credential_bindings(global_proxy.as_ref())
            .into_iter()
            .map(|binding| (binding.target, binding.reference.map(str::to_string)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut seen = BTreeSet::new();
        let mut selected = Vec::new();
        let mut prepared = PreparedProfileCredentials {
            created: Vec::new(),
            stale: Vec::new(),
            summary: ProfileCredentialRestoreSummary::default(),
        };
        for secret in secrets
            .iter()
            .filter(|secret| is_profile_credential(secret))
        {
            let target: CredentialTarget = serde_json::from_str(&secret.id)
                .map_err(|_| anyhow::anyhow!("Invalid portable credential target"))?;
            if !seen.insert(target.clone()) {
                bail!("Duplicate portable credential target");
            }
            if !selection.contains(&target.owner) || !available.contains_key(&target) {
                prepared.summary.skipped += 1;
                continue;
            }
            if secret.kind == CLEARED_PROFILE_CREDENTIAL_KIND && !secret.secret.is_empty() {
                bail!("Credential deletion must not contain a value");
            }
            selected.push((target, secret));
        }
        let mut next_data = self.data.clone();
        let mut next_global = global_proxy.clone();
        let result = (|| -> Result<()> {
            for (target, secret) in selected {
                let clear = secret.kind == CLEARED_PROFILE_CREDENTIAL_KIND;
                let reference = if clear {
                    None
                } else {
                    // Never overwrite a slot referenced by the pre-import store.
                    let prefix = if target.owner == CredentialOwner::GlobalProxy {
                        "oxide_global_proxy"
                    } else {
                        "oxide_sync_credential"
                    };
                    let reference = format!("{prefix}_{}", Uuid::new_v4());
                    self.keychain
                        .store(&reference, &SecretString::from(secret.secret.as_str()))?;
                    prepared.created.push(reference.clone());
                    Some(reference)
                };
                if let Some(old) = available
                    .get(&target)
                    .and_then(|reference| reference.clone())
                {
                    prepared.stale.push(old);
                }
                update_reference(&mut next_data, &target, reference, &mut next_global)?;
                next_data
                    .cleared_credentials
                    .retain(|entry| entry != &target);
                if clear {
                    next_data.cleared_credentials.push(target);
                    prepared.summary.cleared += 1;
                } else {
                    prepared.summary.restored += 1;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.rollback_profile_credentials(prepared)?;
            return Err(error);
        }
        self.data = next_data;
        *global_proxy = next_global;
        Ok(prepared)
    }

    pub fn rollback_profile_credentials(&self, prepared: PreparedProfileCredentials) -> Result<()> {
        let mut failed = false;
        for reference in prepared.created {
            failed |= self.keychain.delete(&reference).is_err();
        }
        if failed {
            bail!("Failed to remove staged credential slots");
        }
        Ok(())
    }

    pub fn commit_profile_credentials(
        &mut self,
        prepared: &mut PreparedProfileCredentials,
    ) -> Result<()> {
        for reference in &prepared.stale {
            // Retain a shared slot if another profile still owns it.
            if !self
                .credential_bindings(None)
                .iter()
                .any(|binding| binding.reference == Some(reference.as_str()))
            {
                self.delete_or_queue_connection_keychain_entry(reference.clone())?;
            }
        }
        if !self.data.pending_keychain_cleanup.is_empty() {
            self.save()?;
        }
        prepared.stale.clear();
        Ok(())
    }
}
