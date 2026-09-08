use super::*;

fn auth_binding<'a>(
    owner: CredentialOwner,
    slot: CredentialSlot,
    host: &str,
    port: u16,
    username: &str,
    auth: &'a SavedAuth,
) -> Option<CredentialBinding<'a>> {
    let (reference, plaintext) = match auth.conventional_fallback() {
        SavedAuth::Password {
            keychain_id,
            plaintext_password,
        } => (keychain_id.as_deref(), plaintext_password.as_ref()),
        SavedAuth::Key {
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        }
        | SavedAuth::Certificate {
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        }
        | SavedAuth::ManagedKey {
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        } => (
            passphrase_keychain_id.as_deref(),
            plaintext_passphrase.as_ref(),
        ),
        _ => return None,
    };
    Some(CredentialBinding {
        target: CredentialTarget {
            owner,
            slot,
            identity: sha256_hex(&(host, port, username, portable_mosh_auth(auth)))
                .expect("serializable credential identity"),
        },
        reference,
        plaintext,
    })
}

fn proxy_binding(
    owner: CredentialOwner,
    slot: CredentialSlot,
    proxy: &SavedUpstreamProxyConfig,
) -> Option<CredentialBinding<'_>> {
    let SavedUpstreamProxyAuth::Password {
        username,
        keychain_id,
        plaintext_password,
    } = &proxy.auth
    else {
        return None;
    };
    Some(CredentialBinding {
        target: CredentialTarget {
            owner,
            slot,
            identity: sha256_hex(&(proxy.protocol, &proxy.host, proxy.port, username))
                .expect("serializable proxy identity"),
        },
        reference: keychain_id.as_deref(),
        plaintext: plaintext_password.as_ref(),
    })
}

impl ConnectionStore {
    pub(super) fn credential_bindings<'a>(
        &'a self,
        global_proxy: Option<&'a SavedUpstreamProxyConfig>,
    ) -> Vec<CredentialBinding<'a>> {
        let mut bindings = Vec::new();
        for profile in &self.data.connections {
            let owner = CredentialOwner::Connection(profile.id.clone());
            if let SavedUpstreamProxyPolicy::Custom { proxy } = &profile.upstream_proxy {
                bindings.extend(proxy_binding(owner, CredentialSlot::UpstreamProxy, proxy));
            }
        }
        for profile in &self.data.mosh_profiles {
            let owner = CredentialOwner::Mosh(profile.id.clone());
            bindings.extend(auth_binding(
                owner.clone(),
                CredentialSlot::Primary,
                &profile.host,
                profile.ssh_port,
                &profile.username,
                &profile.auth,
            ));
            for (index, hop) in profile.proxy_chain.iter().enumerate() {
                bindings.extend(auth_binding(
                    owner.clone(),
                    CredentialSlot::Hop(index),
                    &hop.host,
                    hop.port,
                    &hop.username,
                    &hop.auth,
                ));
            }
        }
        for profile in &self.data.standalone_sftp_profiles {
            let owner = CredentialOwner::StandaloneSftp(profile.id.clone());
            bindings.extend(auth_binding(
                owner.clone(),
                CredentialSlot::Primary,
                &profile.host,
                profile.port,
                &profile.username,
                &profile.auth,
            ));
            for (index, hop) in profile.proxy_chain.iter().enumerate() {
                bindings.extend(auth_binding(
                    owner.clone(),
                    CredentialSlot::Hop(index),
                    &hop.host,
                    hop.port,
                    &hop.username,
                    &hop.auth,
                ));
            }
            if let SavedUpstreamProxyPolicy::Custom { proxy } = &profile.upstream_proxy {
                bindings.extend(proxy_binding(
                    owner.clone(),
                    CredentialSlot::UpstreamProxy,
                    proxy,
                ));
            }
            if let Some(endpoint) = &profile.secondary_endpoint {
                bindings.extend(auth_binding(
                    owner.clone(),
                    CredentialSlot::SecondaryPrimary,
                    &endpoint.host,
                    endpoint.port,
                    &endpoint.username,
                    &endpoint.auth,
                ));
                for (index, hop) in endpoint.proxy_chain.iter().enumerate() {
                    bindings.extend(auth_binding(
                        owner.clone(),
                        CredentialSlot::SecondaryHop(index),
                        &hop.host,
                        hop.port,
                        &hop.username,
                        &hop.auth,
                    ));
                }
                if let SavedUpstreamProxyPolicy::Custom { proxy } = &endpoint.upstream_proxy {
                    bindings.extend(proxy_binding(
                        owner.clone(),
                        CredentialSlot::SecondaryProxy,
                        proxy,
                    ));
                }
            }
        }
        for profile in &self.data.remote_desktop_profiles {
            if let SavedUpstreamProxyPolicy::Custom { proxy } = &profile.upstream_proxy {
                bindings.extend(proxy_binding(
                    CredentialOwner::RemoteDesktop(profile.id.clone()),
                    CredentialSlot::UpstreamProxy,
                    proxy,
                ));
            }
            bindings.push(CredentialBinding {
                target: CredentialTarget {
                    owner: CredentialOwner::RemoteDesktop(profile.id.clone()),
                    slot: CredentialSlot::Primary,
                    identity: sha256_hex(&(
                        profile.protocol,
                        &profile.host,
                        profile.port,
                        &profile.username,
                        &profile.domain,
                    ))
                    .expect("serializable desktop identity"),
                },
                reference: profile.credential_ref.as_deref(),
                plaintext: None,
            });
        }
        if let Some(proxy) = global_proxy {
            bindings.extend(proxy_binding(
                CredentialOwner::GlobalProxy,
                CredentialSlot::UpstreamProxy,
                proxy,
            ));
        }
        bindings
    }

    pub fn export_profile_credentials(
        &self,
        selection: &CredentialSyncSelection,
        global_proxy: Option<&SavedUpstreamProxyConfig>,
    ) -> Result<Vec<EncryptedPortableSecret>> {
        let mut result = Vec::new();
        for binding in self.credential_bindings(global_proxy) {
            if !selection.contains(&binding.target.owner) {
                continue;
            }
            let secret = if let Some(reference) = binding.reference {
                Some(
                    self.keychain
                        .get(reference)
                        .context("failed to read selected credential from protected storage")?,
                )
            } else {
                binding.plaintext.cloned()
            };
            if let Some(secret) = secret {
                result.push(EncryptedPortableSecret {
                    kind: PROFILE_CREDENTIAL_KIND.into(),
                    id: serde_json::to_string(&binding.target)?,
                    secret: secret.into_zeroizing(),
                });
            } else if self.data.cleared_credentials.contains(&binding.target)
                || (binding.target.owner == CredentialOwner::GlobalProxy
                    && self.data.global_proxy_credential_cleared)
            {
                result.push(EncryptedPortableSecret {
                    kind: CLEARED_PROFILE_CREDENTIAL_KIND.into(),
                    id: serde_json::to_string(&binding.target)?,
                    secret: zeroize::Zeroizing::new(String::new()),
                });
            }
        }
        Ok(result)
    }
}
