#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FtpSecurity {
    Plain,
    #[default]
    ExplicitTls,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtpProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub security: FtpSecurity,
    pub initial_path: String,
    pub connect_timeout_seconds: u64,
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    pub password_keychain_id: Option<String>,
    pub group: Option<String>,
    pub notes: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl FtpProfile {
    pub fn new(name: String, host: String, username: String, security: FtpSecurity) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            host,
            port: 21,
            username,
            security,
            initial_path: String::new(),
            connect_timeout_seconds: 30,
            upstream_proxy: SavedUpstreamProxyPolicy::Direct,
            password_keychain_id: None,
            group: None,
            notes: None,
            icon: None,
            color: None,
            icon_background_color: None,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.host.trim().is_empty()
            || self.port == 0
            || self.connect_timeout_seconds == 0
            || self.username.is_empty()
            || self.username.contains(['\r', '\n', '\0'])
            || self.host.contains(['\r', '\n', '\0'])
            || self.initial_path.contains(['\r', '\n', '\0'])
        {
            bail!("Invalid FTP connection fields");
        }
        normalize_optional_group_name(self.group.as_deref())?;
        if let SavedUpstreamProxyPolicy::Custom { proxy } = &self.upstream_proxy {
            if proxy.host.trim().is_empty() || proxy.port == 0 {
                bail!("Invalid FTP upstream proxy");
            }
        }
        Ok(())
    }
}

pub struct SaveFtpProfileRequest {
    pub profile: FtpProfile,
    pub password: Option<SecretString>,
    pub clear_password: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtpProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<FtpProfile>,
    #[serde(default)]
    pub tombstones: Vec<DeletedConnectionTombstone>,
}

impl ConnectionStore {
    pub fn ftp_profiles(&self) -> &[FtpProfile] {
        &self.data.ftp_profiles
    }

    pub fn get_ftp_profile(&self, id: &str) -> Option<&FtpProfile> {
        self.data.ftp_profiles.iter().find(|p| p.id == id)
    }

    pub fn mark_ftp_profile_used(&mut self, id: &str) -> Result<bool> {
        let Some(profile) = self.data.ftp_profiles.iter_mut().find(|p| p.id == id) else {
            return Ok(false);
        };
        let now = Utc::now();
        profile.last_used_at = Some(now);
        profile.updated_at = now;
        self.save()?;
        Ok(true)
    }

    pub fn upsert_ftp_profile(&mut self, request: SaveFtpProfileRequest) -> Result<FtpProfile> {
        let mut profile = request.profile;
        profile.validate()?;
        if request
            .password
            .as_ref()
            .is_some_and(|password| password.expose_secret().contains(['\r', '\n', '\0']))
        {
            bail!("Invalid FTP password characters");
        }
        if request.clear_password && request.password.is_some() {
            bail!("Cannot replace and clear FTP password together");
        }
        let previous_credentials =
            self.stored_credential_targets(&CredentialOwner::Ftp(profile.id.clone()));
        let existing = self.get_ftp_profile(&profile.id).cloned();
        profile.host = profile.host.trim().to_owned();
        profile.name = profile.name.trim().to_owned();
        if profile.name.is_empty() {
            profile.name = format!("{}@{}", profile.username, profile.host);
        }
        profile.group = normalize_optional_group_name(profile.group.as_deref())?;
        profile.notes = normalize_optional_text(profile.notes);
        profile.updated_at = Utc::now();
        profile.created_at = existing
            .as_ref()
            .map_or(profile.updated_at, |old| old.created_at);
        let old_reference = existing
            .as_ref()
            .and_then(|old| old.password_keychain_id.clone());
        profile.password_keychain_id = if request.clear_password {
            None
        } else {
            old_reference.clone()
        };
        profile.upstream_proxy = self.materialize_upstream_proxy_policy(
            profile.upstream_proxy,
            existing.as_ref().map(|old| &old.upstream_proxy),
        )?;
        if let Some(password) = request.password {
            let reference = profile
                .password_keychain_id
                .get_or_insert_with(|| format!("ftp-{}", profile.id));
            self.keychain.store(reference, &password)?;
        }
        if let Some(old) = self
            .data
            .ftp_profiles
            .iter_mut()
            .find(|p| p.id == profile.id)
        {
            *old = profile.clone();
        } else {
            self.data.ftp_profiles.push(profile.clone());
        }
        self.data.ftp_tombstones.retain(|t| t.id != profile.id);
        self.record_cleared_credentials(previous_credentials);
        if let Some(group) = &profile.group {
            self.ensure_group(group.clone())?;
        }
        self.save()?;
        if profile.password_keychain_id != old_reference {
            if let Some(reference) = old_reference {
                self.delete_or_queue_connection_keychain_entry(reference)?;
            }
        }
        if let Some(old) = existing {
            let retained = collect_keychain_ids_for_upstream_proxy(&profile.upstream_proxy);
            for id in collect_keychain_ids_for_upstream_proxy(&old.upstream_proxy)
                .into_iter()
                .filter(|id| !retained.contains(id))
            {
                self.delete_or_queue_connection_keychain_entry(id)?;
            }
        }
        Ok(profile)
    }

    pub fn get_ftp_password(&self, id: &str) -> Result<Option<SecretString>> {
        self.get_ftp_profile(id)
            .and_then(|p| p.password_keychain_id.as_deref())
            .map(|id| self.keychain.get(id))
            .transpose()
    }

    pub fn delete_ftp_profile(&mut self, id: &str) -> Result<bool> {
        let Some(profile) = self.get_ftp_profile(id).cloned() else {
            return Ok(false);
        };
        self.data.ftp_profiles.retain(|p| p.id != id);
        self.data.ftp_tombstones = active_connection_tombstones(&self.data.ftp_tombstones);
        self.data.ftp_tombstones.retain(|t| t.id != id);
        self.data.ftp_tombstones.push(DeletedConnectionTombstone {
            id: id.to_owned(),
            deleted_at: Utc::now(),
        });
        self.save()?;
        let mut references = collect_keychain_ids_for_upstream_proxy(&profile.upstream_proxy);
        references.extend(profile.password_keychain_id);
        for reference in references {
            self.delete_or_queue_connection_keychain_entry(reference)?;
        }
        Ok(true)
    }

    pub fn export_ftp_profiles_snapshot(&self) -> Result<FtpProfilesSyncSnapshot> {
        build_ftp_profiles_sync_snapshot(&self.data)
    }

    pub fn apply_ftp_profiles_snapshot(
        &mut self,
        snapshot: FtpProfilesSyncSnapshot,
    ) -> Result<usize> {
        for profile in &snapshot.records {
            profile.validate()?;
        }
        let previous_references = self.ftp_credential_references();
        let mut applied = 0;
        for tombstone in snapshot.tombstones {
            if let Some(local) = self
                .data
                .ftp_tombstones
                .iter_mut()
                .find(|t| t.id == tombstone.id)
            {
                local.deleted_at = local.deleted_at.max(tombstone.deleted_at);
            } else {
                self.data.ftp_tombstones.push(tombstone);
            }
        }
        self.data.ftp_profiles.retain(|p| {
            !self
                .data
                .ftp_tombstones
                .iter()
                .any(|t| t.id == p.id && t.deleted_at >= p.updated_at)
        });
        for mut incoming in snapshot.records {
            if self
                .data
                .ftp_tombstones
                .iter()
                .any(|t| t.id == incoming.id && t.deleted_at >= incoming.updated_at)
            {
                continue;
            }
            incoming.password_keychain_id = None;
            incoming.upstream_proxy = portable_upstream_proxy(&incoming.upstream_proxy);
            if let Some(local) = self
                .data
                .ftp_profiles
                .iter_mut()
                .find(|p| p.id == incoming.id)
            {
                if incoming.updated_at < local.updated_at {
                    continue;
                }
                if incoming.host == local.host
                    && incoming.port == local.port
                    && incoming.username == local.username
                    && incoming.security == local.security
                {
                    incoming.password_keychain_id = local.password_keychain_id.clone();
                }
                preserve_standalone_sftp_upstream_proxy_secret(
                    &mut incoming.upstream_proxy,
                    &local.upstream_proxy,
                );
                *local = incoming;
            } else {
                self.data.ftp_profiles.push(incoming);
            }
            applied += 1;
        }
        self.save()?;
        let retained = self.ftp_credential_references();
        for reference in previous_references
            .into_iter()
            .filter(|reference| !retained.contains(reference))
        {
            self.delete_or_queue_connection_keychain_entry(reference)?;
        }
        Ok(applied)
    }

    fn ftp_credential_references(&self) -> HashSet<String> {
        self.data
            .ftp_profiles
            .iter()
            .flat_map(|profile| {
                let mut references =
                    collect_keychain_ids_for_upstream_proxy(&profile.upstream_proxy);
                references.extend(profile.password_keychain_id.clone());
                references
            })
            .collect()
    }
}

fn build_ftp_profiles_sync_snapshot(data: &ConnectionStoreData) -> Result<FtpProfilesSyncSnapshot> {
    let mut records = data.ftp_profiles.clone();
    for p in &mut records {
        make_ftp_profile_portable(p);
    }
    records.sort_by(|a, b| a.id.cmp(&b.id));
    let mut tombstones = data.ftp_tombstones.clone();
    tombstones.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(FtpProfilesSyncSnapshot {
        revision: sha256_hex(&(&records, &tombstones))?,
        exported_at: Utc::now().to_rfc3339(),
        records,
        tombstones,
    })
}

pub(crate) fn make_ftp_profile_portable(profile: &mut FtpProfile) {
    profile.password_keychain_id = None;
    profile.upstream_proxy = portable_upstream_proxy(&profile.upstream_proxy);
}

#[cfg(test)]
mod ftp_tests {
    use super::*;

    #[test]
    fn ftp_snapshot_preserves_local_credentials_and_applies_deletion() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        let mut store = ConnectionStore::load(path.clone()).unwrap();
        let profile = store
            .upsert_ftp_profile(SaveFtpProfileRequest {
                profile: FtpProfile::new(
                    "Files".into(),
                    "files.example".into(),
                    "me".into(),
                    FtpSecurity::ExplicitTls,
                ),
                password: Some(SecretString::from("ftp-test-secret")),
                clear_password: false,
            })
            .unwrap();
        let mut snapshot = store.export_ftp_profiles_snapshot().unwrap();
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("ftp-test-secret"));
        assert!(!serialized.contains(profile.password_keychain_id.as_deref().unwrap()));
        snapshot.records[0].name = "Renamed".into();
        snapshot.records[0].updated_at = Utc::now() + Duration::seconds(1);
        store.apply_ftp_profiles_snapshot(snapshot.clone()).unwrap();
        assert_eq!(
            store.get_ftp_password(&profile.id).unwrap().unwrap(),
            "ftp-test-secret"
        );
        assert_eq!(
            ConnectionStore::load(path)
                .unwrap()
                .get_ftp_profile(&profile.id)
                .unwrap()
                .name,
            "Renamed"
        );
        snapshot.tombstones.push(DeletedConnectionTombstone {
            id: profile.id.clone(),
            deleted_at: Utc::now() + Duration::seconds(2),
        });
        store.apply_ftp_profiles_snapshot(snapshot).unwrap();
        assert!(store.get_ftp_profile(&profile.id).is_none());
        let tombstones = store.export_ftp_profiles_snapshot().unwrap().tombstones;
        assert_eq!(
            tombstones.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            [profile.id.as_str()]
        );
    }
}
