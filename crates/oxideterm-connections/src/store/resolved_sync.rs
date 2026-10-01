/// Complete effective configuration after causal merging and local scope filtering.
/// Runtime fields and protected references are preserved by the owning store.
pub struct ResolvedConnectionConfiguration {
    pub connections: SavedConnectionsSyncSnapshot,
    pub serial: Vec<SerialProfile>,
    pub telnet: Vec<TelnetProfile>,
    pub mosh: Vec<MoshProfile>,
    pub sftp: Vec<StandaloneSftpProfile>,
    pub ftp: Vec<FtpProfile>,
    pub remote_desktop: Vec<RemoteDesktopProfile>,
}

impl ConnectionStore {
    /// No timestamp or display-name arbitration belongs here: the caller has
    /// already resolved changes by stable identity. Cleanup stays deferred.
    pub fn prepare_resolved_configuration(
        &mut self,
        mut incoming: ResolvedConnectionConfiguration,
    ) -> Result<PreparedSavedConnectionsSync> {
        let checkpoint = self.create_checkpoint()?;
        let old_references = self.credential_reference_ids();
        let mut next = self.data.clone();
        let now = Utc::now();
        let mut connections = Vec::new();
        let mut ids = HashSet::new();
        for record in incoming.connections.records {
            if record.deleted {
                continue;
            }
            let payload = record
                .payload
                .context("Resolved connection has no configuration")?;
            if record.id != payload.id || !ids.insert(record.id.clone()) {
                bail!("Invalid resolved connection identity");
            }
            let previous = self.get(&record.id);
            let mut connection = build_saved_connection_from_sync_payload(
                &payload,
                record.options.as_ref(),
                now,
                previous,
                true,
            )?;
            connection.last_used_at = previous.and_then(|value| value.last_used_at);
            connections.push(connection);
        }
        let deleted_connection_ids = self
            .data
            .connections
            .iter()
            .filter(|connection| !ids.contains(&connection.id))
            .map(|connection| connection.id.clone())
            .collect::<Vec<_>>();
        let pending_privilege_keychain_ids = self
            .data
            .connections
            .iter()
            .filter(|connection| !ids.contains(&connection.id))
            .flat_map(collect_privilege_keychain_ids)
            .collect();
        next.connection_tombstones
            .retain(|entry| !ids.contains(&entry.id));
        for id in &deleted_connection_ids {
            next.connection_tombstones.retain(|entry| entry.id != *id);
            next.connection_tombstones.push(DeletedConnectionTombstone {
                id: id.clone(),
                deleted_at: now,
            });
        }
        next.connections = connections;

        for profile in &incoming.connections.local_terminal_profiles {
            non_empty(profile.name.trim(), "Local terminal profile name")?;
            normalize_optional_group_name(profile.group.as_deref())?;
        }
        let local_ids = incoming
            .connections
            .local_terminal_profiles
            .iter()
            .map(|p| p.id.clone())
            .collect::<HashSet<_>>();
        next.local_terminal_tombstones
            .retain(|entry| !local_ids.contains(&entry.id));
        for profile in &self.data.local_terminal_profiles {
            if !local_ids.contains(&profile.id) {
                next.local_terminal_tombstones
                    .retain(|entry| entry.id != profile.id);
                next.local_terminal_tombstones
                    .push(DeletedConnectionTombstone {
                        id: profile.id.clone(),
                        deleted_at: now,
                    });
            }
        }
        next.local_terminal_profiles = incoming.connections.local_terminal_profiles;
        for credential in &mut incoming.connections.totp_credentials {
            credential.validate()?;
            credential.secret_reference = self
                .data
                .totp_credentials
                .iter()
                .find(|old| {
                    old.id == credential.id && old.secret_revision == credential.secret_revision
                })
                .map(|old| old.secret_reference.clone())
                .unwrap_or_default();
        }
        next.totp_credentials = incoming.connections.totp_credentials;
        for profile in &incoming.serial {
            profile.validate()?;
        }
        next.serial_profiles = incoming.serial;
        for profile in &mut incoming.telnet {
            profile.validate()?;
            profile.upstream_proxy = portable_upstream_proxy(&profile.upstream_proxy);
            if let Some(old) = self
                .data
                .telnet_profiles
                .iter()
                .find(|old| old.id == profile.id)
            {
                preserve_standalone_sftp_upstream_proxy_secret(
                    &mut profile.upstream_proxy,
                    &old.upstream_proxy,
                );
            }
        }
        next.telnet_profiles = incoming.telnet;
        for profile in &mut incoming.mosh {
            profile.validate()?;
            profile.auth = portable_mosh_auth(&profile.auth);
            for hop in &mut profile.proxy_chain {
                hop.auth = portable_mosh_auth(&hop.auth);
            }
            if let Some(old) = self
                .data
                .mosh_profiles
                .iter()
                .find(|old| old.id == profile.id)
            {
                preserve_local_auth_secret(&mut profile.auth, &old.auth);
                preserve_proxy_chain_local_secrets(&mut profile.proxy_chain, &old.proxy_chain);
            }
        }
        next.mosh_profiles = incoming.mosh;
        for profile in &mut incoming.sftp {
            make_standalone_sftp_profile_portable(profile);
            profile.validate()?;
            if let Some(old) = self
                .data
                .standalone_sftp_profiles
                .iter()
                .find(|old| old.id == profile.id)
            {
                preserve_standalone_sftp_local_secrets(profile, old);
            }
        }
        next.standalone_sftp_profiles = incoming.sftp;
        for profile in &mut incoming.ftp {
            profile.validate()?;
            profile.password_keychain_id = None;
            profile.upstream_proxy = portable_upstream_proxy(&profile.upstream_proxy);
            if let Some(old) = self
                .data
                .ftp_profiles
                .iter()
                .find(|old| old.id == profile.id)
            {
                if old.host == profile.host
                    && old.port == profile.port
                    && old.username == profile.username
                    && old.security == profile.security
                {
                    profile.password_keychain_id = old.password_keychain_id.clone();
                }
                preserve_standalone_sftp_upstream_proxy_secret(
                    &mut profile.upstream_proxy,
                    &old.upstream_proxy,
                );
            }
        }
        let ftp_ids = incoming
            .ftp
            .iter()
            .map(|p| p.id.clone())
            .collect::<HashSet<_>>();
        next.ftp_tombstones
            .retain(|entry| !ftp_ids.contains(&entry.id));
        for profile in &self.data.ftp_profiles {
            if !ftp_ids.contains(&profile.id) {
                next.ftp_tombstones.retain(|entry| entry.id != profile.id);
                next.ftp_tombstones.push(DeletedConnectionTombstone {
                    id: profile.id.clone(),
                    deleted_at: now,
                });
            }
        }
        next.ftp_profiles = incoming.ftp;
        for profile in &mut incoming.remote_desktop {
            profile.validate()?;
            profile.credential_ref = None;
            profile.upstream_proxy = portable_upstream_proxy(&profile.upstream_proxy);
            if let Some(old) = self
                .data
                .remote_desktop_profiles
                .iter()
                .find(|old| old.id == profile.id)
            {
                profile.credential_ref = old.credential_ref.clone();
                preserve_standalone_sftp_upstream_proxy_secret(
                    &mut profile.upstream_proxy,
                    &old.upstream_proxy,
                );
            }
        }
        next.remote_desktop_profiles = incoming.remote_desktop;
        self.data = next;
        self.normalize();
        if let Err(error) = self.save() {
            self.restore_checkpoint(&checkpoint)?;
            return Err(error);
        }
        let retained = self.credential_reference_ids();
        Ok(PreparedSavedConnectionsSync {
            checkpoint,
            outcome: ApplySavedConnectionsSyncOutcome {
                result: ApplySavedConnectionsSyncSnapshotResult {
                    applied: self.data.connections.len(),
                    skipped: 0,
                    conflicts: 0,
                },
                deleted_connection_ids,
            },
            pending_keychain_ids: old_references.difference(&retained).cloned().collect(),
            pending_privilege_keychain_ids,
        })
    }
}
