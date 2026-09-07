use super::*;
use crate::oxide_file::EncryptedPortableSecret;
use std::collections::BTreeSet;

mod collect;
mod restore;
pub use restore::{
    PreparedProfileCredentials, ProfileCredentialRestoreSummary, is_profile_credential,
};
#[cfg(test)]
mod tests;

pub const PROFILE_CREDENTIAL_KIND: &str = "profile_credential";
pub const CLEARED_PROFILE_CREDENTIAL_KIND: &str = "cleared_profile_credential";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum CredentialOwner {
    Connection(String),
    StandaloneSftp(String),
    Mosh(String),
    RemoteDesktop(String),
    GlobalProxy,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSlot {
    Primary,
    Hop(usize),
    UpstreamProxy,
    SecondaryPrimary,
    SecondaryHop(usize),
    SecondaryProxy,
}

/// Portable identity excludes device-local references and all secret values.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct CredentialTarget {
    pub owner: CredentialOwner,
    pub slot: CredentialSlot,
    pub identity: String,
}

#[derive(Clone, Debug, Default)]
pub struct CredentialSyncSelection {
    pub connection_ids: BTreeSet<String>,
    pub sftp_ids: BTreeSet<String>,
    pub mosh_ids: BTreeSet<String>,
    pub remote_desktop_ids: BTreeSet<String>,
    pub global_proxy: bool,
}

impl CredentialSyncSelection {
    pub fn contains(&self, owner: &CredentialOwner) -> bool {
        match owner {
            CredentialOwner::Connection(id) => self.connection_ids.contains(id),
            CredentialOwner::StandaloneSftp(id) => self.sftp_ids.contains(id),
            CredentialOwner::Mosh(id) => self.mosh_ids.contains(id),
            CredentialOwner::RemoteDesktop(id) => self.remote_desktop_ids.contains(id),
            CredentialOwner::GlobalProxy => self.global_proxy,
        }
    }
}

struct CredentialBinding<'a> {
    target: CredentialTarget,
    reference: Option<&'a str>,
    plaintext: Option<&'a SecretString>,
}

impl ConnectionStore {
    pub fn profile_credential_count(
        &self,
        selection: &CredentialSyncSelection,
        global_proxy: Option<&SavedUpstreamProxyConfig>,
    ) -> usize {
        self.credential_bindings(global_proxy)
            .iter()
            .filter(|binding| {
                selection.contains(&binding.target.owner)
                    && (binding.reference.is_some()
                        || binding.plaintext.is_some()
                        || self.data.cleared_credentials.contains(&binding.target)
                        || (binding.target.owner == CredentialOwner::GlobalProxy
                            && self.data.global_proxy_credential_cleared))
            })
            .count()
    }

    /// Metadata-only revision: never publish a password digest that could be guessed offline.
    pub fn profile_credentials_revision(&self) -> Result<String> {
        sha256_hex(&(
            self.data
                .connections
                .iter()
                .map(|p| (&p.id, p.updated_at))
                .collect::<Vec<_>>(),
            self.data
                .standalone_sftp_profiles
                .iter()
                .map(|p| (&p.id, p.updated_at))
                .collect::<Vec<_>>(),
            self.data
                .mosh_profiles
                .iter()
                .map(|p| (&p.id, p.updated_at))
                .collect::<Vec<_>>(),
            self.data
                .remote_desktop_profiles
                .iter()
                .map(|p| (&p.id, p.updated_at))
                .collect::<Vec<_>>(),
            &self.data.cleared_credentials,
            &self.data.global_proxy_credential_revision,
        ))
    }

    pub(super) fn stored_credential_targets(
        &self,
        owner: &CredentialOwner,
    ) -> Vec<CredentialTarget> {
        self.credential_bindings(None)
            .into_iter()
            .filter(|binding| {
                &binding.target.owner == owner
                    && (binding.reference.is_some() || binding.plaintext.is_some())
            })
            .map(|binding| binding.target)
            .collect()
    }

    pub(super) fn record_cleared_credentials(&mut self, previous: Vec<CredentialTarget>) {
        let current = self
            .credential_bindings(None)
            .into_iter()
            .map(|binding| {
                (
                    binding.target,
                    binding.reference.is_some() || binding.plaintext.is_some(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        self.data.cleared_credentials.retain(|target| {
            matches!(target.owner, CredentialOwner::GlobalProxy)
                || current.get(target) == Some(&false)
        });
        for target in previous {
            if current.get(&target) == Some(&false)
                && !self.data.cleared_credentials.contains(&target)
            {
                self.data.cleared_credentials.push(target);
            }
        }
    }
}
