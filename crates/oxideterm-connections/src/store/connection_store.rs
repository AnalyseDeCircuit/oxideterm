impl ConnectionStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut store = Self::load_without_side_effects(path)?;
        store.normalize();
        if store.migrate_legacy_credentials()? {
            store.save()?;
        }
        Ok(store)
    }

    pub fn load_read_only(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut store = Self::load_without_side_effects(path)?;
        // CLI and inspection callers need normalized data without triggering
        // legacy keychain migration or rewriting the store on disk.
        store.normalize();
        Ok(store)
    }

    fn load_without_side_effects(path: PathBuf) -> Result<Self> {
        let loaded = if path.exists() {
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            decode_connection_store_data(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?
        } else {
            LoadedConnectionStoreData {
                data: ConnectionStoreData::default(),
                format: ConnectionStoreStorageFormat::Missing,
            }
        };
        #[cfg(target_os = "macos")]
        let privilege_keychain = ConnectionKeychain::with_macos_biometrics_reason(
            PRIVILEGE_CREDENTIAL_KEYCHAIN_SERVICE,
            "OxideTerm needs to access your privilege helper credential",
        );
        #[cfg(not(target_os = "macos"))]
        let privilege_keychain =
            ConnectionKeychain::with_service(PRIVILEGE_CREDENTIAL_KEYCHAIN_SERVICE);

        Ok(Self {
            path,
            data: loaded.data,
            storage_format: loaded.format,
            keychain: ConnectionKeychain::default(),
            managed_keychain: ConnectionKeychain::with_service(MANAGED_SSH_KEYCHAIN_SERVICE),
            privilege_keychain,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connections(&self) -> &[SavedConnection] {
        &self.data.connections
    }

    pub fn connection_infos(&self) -> Vec<ConnectionInfo> {
        self.data
            .connections
            .iter()
            .map(ConnectionInfo::from)
            .collect()
    }

    pub fn serial_profiles(&self) -> &[SerialProfile] {
        &self.data.serial_profiles
    }

    pub fn groups(&self) -> &[String] {
        &self.data.groups
    }

    pub fn get(&self, id: &str) -> Option<&SavedConnection> {
        self.data.connections.iter().find(|conn| conn.id == id)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let data = encode_connection_store_data(&self.data, self.storage_format)?;
        fs::write(&self.path, data)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }

    fn data_dir(&self) -> Result<&Path> {
        self.path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invalid connections store path"))
    }

    fn store_managed_ssh_key_secret(
        &self,
        secret_id: &str,
        secret: &SecretString,
    ) -> Result<ManagedSshKeySecretWrite> {
        match self.managed_keychain.store(secret_id, secret) {
            Ok(()) => Ok(ManagedSshKeySecretWrite {
                created_config_key: false,
            }),
            Err(_keychain_error) => {
                let _ = self.managed_keychain.delete(secret_id);
                let (config_key, created_config_key) = get_or_create_config_encryption_key()?;
                write_managed_ssh_key_secret_file(self.data_dir()?, secret_id, secret, &config_key)?;
                Ok(ManagedSshKeySecretWrite { created_config_key })
            }
        }
    }

    fn get_managed_ssh_key_secret(&self, secret_id: &str) -> Result<SecretString> {
        match self.managed_keychain.get(secret_id) {
            Ok(secret) => Ok(secret),
            Err(keychain_error) => {
                let config_key = load_config_encryption_key()?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Managed SSH key secret unavailable from keychain ({keychain_error:#}) and local config key is missing"
                    )
                })?;
                read_managed_ssh_key_secret_file(self.data_dir()?, secret_id, &config_key)
                    .with_context(|| {
                        format!(
                            "managed SSH key secret unavailable from keychain ({keychain_error:#}) or encrypted file"
                        )
                    })
            }
        }
    }

    fn delete_managed_ssh_key_secret(&self, secret_id: &str) -> Result<()> {
        let keychain_result = self.managed_keychain.delete(secret_id);
        let file_result = delete_managed_ssh_key_secret_file(self.data_dir()?, secret_id);

        match (keychain_result, file_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(_keychain_error), Ok(())) => Ok(()),
            (Ok(()), Err(file_error)) => Err(file_error),
            (Err(keychain_error), Err(file_error)) => Err(anyhow::anyhow!(
                "failed to delete managed SSH key secret from keychain ({keychain_error:#}) and encrypted file ({file_error:#})"
            )),
        }
    }

    pub fn upsert(&mut self, request: SaveConnectionRequest) -> Result<ConnectionInfo> {
        let group = normalize_optional_group_name(request.group.as_deref())?;
        let now = Utc::now();
        let id = request.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let old_keychain_ids = self
            .get(&id)
            .map(collect_connection_keychain_ids)
            .unwrap_or_default();
        let existing = self.get(&id).cloned();
        let is_update = existing.is_some();
        let existing_auth = existing.as_ref().map(|conn| conn.auth.clone());
        let mut options = existing
            .as_ref()
            .map(|conn| conn.options.clone())
            .unwrap_or_default();
        // Tauri preserves saved per-connection SSH options on edit and only
        // overwrites the UI-exposed agent-forwarding bit. This keeps imported
        // Tauri config tails such as compression/term_type from being dropped.
        options.agent_forwarding = request.agent_forwarding;
        let auth = self.materialize_auth(request.auth, existing_auth.as_ref())?;
        let proxy_chain = self.materialize_proxy_chain(request.proxy_chain)?;
        let upstream_proxy = self.materialize_upstream_proxy_policy(
            request.upstream_proxy,
            existing.as_ref().map(|conn| &conn.upstream_proxy),
        )?;
        let next_keychain_ids =
            collect_keychain_ids_for_parts(&auth, &proxy_chain, &upstream_proxy);
        let post_connect_command = request.post_connect_command.and_then(|command| {
            let command = command.trim().to_string();
            (!command.is_empty()).then_some(command)
        });
        // Tauri stores this command under options; the top-level field remains
        // readable for old native plaintext stores but is no longer emitted.
        options.post_connect_command = post_connect_command.clone();
        let connection = SavedConnection {
            id: id.clone(),
            version: existing
                .as_ref()
                .map(|conn| conn.version)
                .unwrap_or(CONFIG_VERSION),
            name: non_empty(request.name.trim(), "Connection name")?.to_string(),
            group: group.clone(),
            host: non_empty(request.host.trim(), "Host")?.to_string(),
            port: request.port.max(1),
            username: non_empty(request.username.trim(), "Username")?.to_string(),
            auth,
            proxy_chain,
            upstream_proxy,
            options,
            created_at: self.get(&id).map(|conn| conn.created_at).unwrap_or(now),
            last_used_at: if is_update {
                Some(now)
            } else {
                self.get(&id).and_then(|conn| conn.last_used_at)
            },
            updated_at: Some(now),
            color: request.color,
            tags: request.tags,
            post_connect_command: None,
            privilege_credentials: existing
                .map(|conn| conn.privilege_credentials)
                .unwrap_or_default(),
        };
        if let Some(index) = self.data.connections.iter().position(|conn| conn.id == id) {
            self.data.connections[index] = connection;
        } else {
            self.data.connections.push(connection);
        }
        if let Some(group) = group {
            self.ensure_group(group)?;
        }
        self.normalize();
        self.save()?;
        for keychain_id in old_keychain_ids
            .iter()
            .filter(|keychain_id| !next_keychain_ids.contains(*keychain_id))
        {
            let _ = self.keychain.delete(keychain_id);
        }
        Ok(ConnectionInfo::from(
            self.get(&id).expect("connection saved"),
        ))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let keychain_ids = self
            .get(id)
            .map(collect_connection_keychain_ids)
            .unwrap_or_default();
        let privilege_keychain_ids = self
            .get(id)
            .map(collect_privilege_keychain_ids)
            .unwrap_or_default();
        let deleted = self
            .remove_connection_with_tombstone_at(id, Utc::now())
            .is_some();
        if deleted {
            self.normalize();
            self.save()?;
            for keychain_id in keychain_ids {
                let _ = self.keychain.delete(&keychain_id);
            }
            for keychain_id in privilege_keychain_ids {
                let _ = self.privilege_keychain.delete(&keychain_id);
            }
        }
        Ok(deleted)
    }

    pub fn rename_connection(&mut self, id: &str, name: String) -> Result<bool> {
        let Some(connection) = self
            .data
            .connections
            .iter_mut()
            .find(|connection| connection.id == id)
        else {
            return Ok(false);
        };
        connection.name = non_empty(name.trim(), "Connection name")?.to_string();
        connection.updated_at = Some(Utc::now());
        self.normalize();
        self.save()?;
        Ok(true)
    }

    pub fn ensure_group(&mut self, name: String) -> Result<()> {
        let name = validate_group_name(&name)?;
        if !self.data.groups.contains(&name) {
            self.data.groups.push(name);
            self.normalize();
        }
        Ok(())
    }

    pub fn create_group(&mut self, name: String) -> Result<()> {
        self.ensure_group(name)?;
        self.save()
    }

    pub fn delete_group(&mut self, name: &str) -> Result<()> {
        self.data.groups.retain(|group| group != name);
        for conn in &mut self.data.connections {
            if conn.group.as_deref() == Some(name) {
                conn.group = None;
            }
        }
        self.save()
    }

    pub fn rename_group(&mut self, old_name: &str, new_name: String) -> Result<usize> {
        let new_name = validate_group_name(&new_name)?;
        let mut updated = 0;
        for group in &mut self.data.groups {
            if group == old_name {
                *group = new_name.clone();
                updated += 1;
            }
        }
        for connection in &mut self.data.connections {
            if connection.group.as_deref() == Some(old_name) {
                connection.group = Some(new_name.clone());
                connection.updated_at = Some(Utc::now());
                updated += 1;
            }
        }
        if updated > 0 {
            self.normalize();
            self.save()?;
        }
        Ok(updated)
    }

    pub fn move_to_group(&mut self, ids: &[String], group: Option<&str>) -> Result<usize> {
        let group = normalize_optional_group_name(group)?;
        let id_set = ids.iter().collect::<HashSet<_>>();
        let now = Utc::now();
        let mut updated = 0;
        for conn in &mut self.data.connections {
            if id_set.contains(&conn.id) {
                conn.group = group.clone();
                conn.updated_at = Some(now);
                updated += 1;
            }
        }
        if let Some(group) = group {
            self.ensure_group(group)?;
        }
        self.save()?;
        Ok(updated)
    }

    pub fn duplicate(&mut self, id: &str) -> Result<Option<ConnectionInfo>> {
        let Some(mut duplicate) = self.get(id).cloned() else {
            return Ok(None);
        };
        duplicate.id = Uuid::new_v4().to_string();
        duplicate.name = format!("{} (Copy)", duplicate.name);
        duplicate.created_at = Utc::now();
        duplicate.updated_at = Some(Utc::now());
        duplicate.last_used_at = None;
        duplicate.auth = self.clone_auth_secret(&duplicate.auth)?;
        for hop in &mut duplicate.proxy_chain {
            hop.auth = self.clone_auth_secret(&hop.auth)?;
        }
        // Unlike SSH auth secrets, sudo/su helper credentials must not be
        // duplicated silently because their scope is an explicit per-connection
        // safety choice.
        duplicate.privilege_credentials.clear();
        let duplicate_id = duplicate.id.clone();
        self.data.connections.push(duplicate);
        self.normalize();
        self.save()?;
        Ok(self.get(&duplicate_id).map(ConnectionInfo::from))
    }

    pub fn mark_used(&mut self, id: &str) -> Result<bool> {
        let Some(conn) = self.data.connections.iter_mut().find(|conn| conn.id == id) else {
            return Ok(false);
        };
        conn.touch();
        self.data.recent.retain(|recent_id| recent_id != id);
        self.data.recent.insert(0, id.to_string());
        self.data.recent.truncate(10);
        self.save()?;
        Ok(true)
    }

    pub fn upsert_serial_profile(
        &mut self,
        request: SaveSerialProfileRequest,
    ) -> Result<SerialProfile> {
        let group = normalize_optional_group_name(request.group.as_deref())?;
        let now = Utc::now();
        let id = request.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let mut profile = self
            .data
            .serial_profiles
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .unwrap_or_else(|| {
                let mut profile = SerialProfile::new(request.name.trim(), request.port_path.trim());
                profile.id = id.clone();
                profile
            });

        profile.name = request.name.trim().to_string();
        profile.group = group;
        profile.port_path = request.port_path.trim().to_string();
        profile.baud_rate = request.baud_rate.unwrap_or(115_200);
        profile.data_bits = request.data_bits.unwrap_or(8);
        profile.stop_bits = request.stop_bits.unwrap_or(1);
        profile.parity = request.parity.unwrap_or(SerialParity::None);
        profile.flow_control = request.flow_control.unwrap_or(SerialFlowControl::None);
        profile.connect_on_open = request.connect_on_open.unwrap_or(false);
        if !self
            .data
            .serial_profiles
            .iter()
            .any(|existing| existing.id == id)
        {
            profile.created_at = now;
        }
        profile.updated_at = now;
        profile.validate()?;

        if let Some(existing) = self
            .data
            .serial_profiles
            .iter_mut()
            .find(|existing| existing.id == id)
        {
            *existing = profile.clone();
        } else {
            self.data.serial_profiles.push(profile.clone());
        }
        self.normalize();
        self.save()?;
        Ok(profile)
    }

    pub fn delete_serial_profile(&mut self, id: &str) -> Result<bool> {
        let before = self.data.serial_profiles.len();
        self.data.serial_profiles.retain(|profile| profile.id != id);
        let deleted = self.data.serial_profiles.len() != before;
        if deleted {
            self.save()?;
        }
        Ok(deleted)
    }

    pub fn mark_serial_profile_used(&mut self, id: &str) -> Result<bool> {
        let Some(profile) = self
            .data
            .serial_profiles
            .iter_mut()
            .find(|profile| profile.id == id)
        else {
            return Ok(false);
        };
        let now = Utc::now();
        profile.last_used_at = Some(now);
        profile.updated_at = now;
        self.save()?;
        Ok(true)
    }

    pub fn import_ssh_connection(
        &mut self,
        mut connection: SavedConnection,
    ) -> Result<ConnectionInfo> {
        connection.id = Uuid::new_v4().to_string();
        connection.version = CONFIG_VERSION;
        connection.created_at = Utc::now();
        connection.updated_at = Some(Utc::now());
        connection.auth = self.materialize_auth(connection.auth, None)?;
        connection.proxy_chain = self.materialize_proxy_chain(connection.proxy_chain)?;
        connection.upstream_proxy =
            self.materialize_upstream_proxy_policy(connection.upstream_proxy, None)?;
        // Third-party imports do not carry privilege helper secrets. The user
        // must explicitly create them after import.
        connection.privilege_credentials.clear();
        if let Some(group) = connection.group.clone() {
            self.ensure_group(group)?;
        }
        let id = connection.id.clone();
        self.data.connections.push(connection);
        self.normalize();
        self.save()?;
        Ok(self.get(&id).map(ConnectionInfo::from).expect("imported"))
    }

    pub fn upsert_imported_connection(
        &mut self,
        mut connection: SavedConnection,
    ) -> Result<ConnectionInfo> {
        let group = normalize_optional_group_name(connection.group.as_deref())?;
        let now = Utc::now();
        if connection.id.trim().is_empty() {
            connection.id = Uuid::new_v4().to_string();
        }
        let id = connection.id.clone();
        let existing = self.get(&id).cloned();
        let old_keychain_ids = existing
            .as_ref()
            .map(collect_connection_keychain_ids)
            .unwrap_or_default();
        let existing_auth = existing.as_ref().map(|conn| conn.auth.clone());

        connection.version = CONFIG_VERSION;
        connection.name = non_empty(connection.name.trim(), "Connection name")?.to_string();
        connection.group = group.clone();
        connection.host = non_empty(connection.host.trim(), "Host")?.to_string();
        connection.port = connection.port.max(1);
        connection.username = non_empty(connection.username.trim(), "Username")?.to_string();
        connection.auth = self.materialize_auth(connection.auth, existing_auth.as_ref())?;
        connection.proxy_chain = self.materialize_proxy_chain(connection.proxy_chain)?;
        connection.upstream_proxy = self.materialize_upstream_proxy_policy(
            connection.upstream_proxy,
            existing.as_ref().map(|conn| &conn.upstream_proxy),
        )?;
        if let Some(existing) = existing.as_ref() {
            connection.created_at = existing.created_at;
            connection.last_used_at = existing.last_used_at;
            connection.privilege_credentials = existing.privilege_credentials.clone();
        } else if connection.created_at.timestamp() <= 0 {
            connection.created_at = now;
        }
        connection.updated_at = Some(now);

        let next_keychain_ids = collect_keychain_ids_for_parts(
            &connection.auth,
            &connection.proxy_chain,
            &connection.upstream_proxy,
        );
        if let Some(index) = self
            .data
            .connections
            .iter()
            .position(|candidate| candidate.id == id)
        {
            self.data.connections[index] = connection;
        } else {
            self.data.connections.push(connection);
        }
        if let Some(group) = group {
            self.ensure_group(group)?;
        }
        self.normalize();
        self.save()?;
        for keychain_id in old_keychain_ids
            .iter()
            .filter(|keychain_id| !next_keychain_ids.contains(*keychain_id))
        {
            let _ = self.keychain.delete(keychain_id);
        }
        Ok(ConnectionInfo::from(
            self.get(&id).expect("connection imported"),
        ))
    }

    pub fn upsert_imported_connections_transaction(
        &mut self,
        connections: Vec<SavedConnection>,
    ) -> Result<Vec<ConnectionInfo>> {
        self.upsert_imported_connections_and_managed_keys_transaction(connections, Vec::new())
    }

    pub(crate) fn upsert_imported_connections_and_managed_keys_transaction(
        &mut self,
        connections: Vec<SavedConnection>,
        managed_keys: Vec<ImportedManagedSshKey>,
    ) -> Result<Vec<ConnectionInfo>> {
        let original_data = self.data.clone();
        let original_keychain = self.snapshot_keychain_entries(&original_data);
        let original_privilege_keychain = self.snapshot_privilege_keychain_entries(&original_data);
        let original_managed_keychain = self.snapshot_managed_keychain_entries(&original_data);
        let mut touched_keychain_ids = HashSet::new();
        let mut touched_privilege_keychain_ids = HashSet::new();
        let mut touched_managed_secret_ids = HashSet::new();
        let mut created_managed_secret_config_key = false;
        let mut stale_old_keychain_ids = HashSet::new();
        let mut imported_ids = Vec::new();

        let result = (|| {
            for managed_key in managed_keys {
                touched_managed_secret_ids.insert(managed_key.key.secret_id.clone());
                let secret_write =
                    self.store_managed_ssh_key_secret(&managed_key.key.secret_id, &managed_key.secret)?;
                created_managed_secret_config_key |= secret_write.created_config_key;
                self.data
                    .managed_ssh_keys
                    .retain(|candidate| candidate.id != managed_key.key.id);
                self.data.managed_ssh_keys.push(managed_key.key);
            }
            for connection in connections {
                let staged = self.stage_imported_connection(connection)?;
                touched_keychain_ids.extend(staged.touched_keychain_ids);
                touched_privilege_keychain_ids.extend(staged.touched_privilege_keychain_ids);
                stale_old_keychain_ids.extend(staged.stale_old_keychain_ids);
                imported_ids.push(staged.id);
            }
            self.normalize();
            self.save()?;
            Ok::<(), anyhow::Error>(())
        })();

        if let Err(error) = result {
            self.data = original_data;
            let _ = self.save();
            self.rollback_keychain_entries(&touched_keychain_ids, &original_keychain);
            self.rollback_privilege_keychain_entries(
                &touched_privilege_keychain_ids,
                &original_privilege_keychain,
            );
            self.rollback_managed_keychain_entries(
                &touched_managed_secret_ids,
                &original_managed_keychain,
            );
            if created_managed_secret_config_key {
                rollback_created_config_key();
            }
            return Err(error);
        }

        for keychain_id in &stale_old_keychain_ids {
            let _ = self.keychain.delete(keychain_id);
        }

        Ok(imported_ids
            .iter()
            .filter_map(|id| self.get(id).map(ConnectionInfo::from))
            .collect())
    }

    pub fn get_connection_password(&self, id: &str) -> Result<SecretString> {
        let conn = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;
        self.get_saved_auth_password(&conn.auth)
    }

    pub fn list_privilege_credentials(
        &self,
        connection_id: &str,
    ) -> Result<Vec<SavedPrivilegeCredential>> {
        Ok(self
            .privilege_credentials_for_scope(connection_id)?
            .iter()
            .cloned()
            .map(normalize_saved_privilege_credential_for_display)
            .collect())
    }

    pub fn save_privilege_credential(
        &mut self,
        request: SavePrivilegeCredentialRequest,
    ) -> Result<SavedPrivilegeCredential> {
        let connection_id = non_empty(request.connection_id.trim(), "Connection id")?.to_string();
        let label = non_empty(request.label.trim(), "Credential label")?.to_string();
        let now = Utc::now();
        let credential_id = request
            .credential_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let keychain_id = privilege_keychain_id(&connection_id, &credential_id);

        if let Some(secret) = request.secret.as_ref() {
            // The explicit-click privilege secret is written to the dedicated
            // namespace before metadata is persisted, matching the Tauri
            // boundary where SavedConnection never owns the secret value.
            self.privilege_keychain.store(&keychain_id, secret)?;
        }

        let credentials = self.privilege_credentials_for_scope_mut(&connection_id)?;
        let existing = credentials
            .iter()
            .find(|credential| credential.id == credential_id)
            .cloned();
        let prompt_patterns =
            normalize_privilege_prompt_patterns(request.kind, request.prompt_patterns);
        let keychain_id = if request.secret.is_some() {
            Some(keychain_id)
        } else {
            existing
                .as_ref()
                .and_then(|credential| credential.keychain_id.clone())
        };
        let credential = SavedPrivilegeCredential {
            id: credential_id.clone(),
            connection_id,
            label,
            kind: request.kind,
            username_hint: request
                .username_hint
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            prompt_patterns,
            keychain_id,
            plaintext_secret: None,
            enabled: request.enabled,
            require_click_to_send: request.require_click_to_send,
            created_at: existing
                .as_ref()
                .map(|credential| credential.created_at)
                .unwrap_or(now),
            updated_at: now,
        };
        if let Some(index) = credentials
            .iter()
            .position(|candidate| candidate.id == credential_id)
        {
            credentials[index] = credential.clone();
        } else {
            credentials.push(credential.clone());
        }
        self.touch_privilege_scope(&credential.connection_id);
        self.save()?;
        Ok(credential)
    }

    pub fn delete_privilege_credential(
        &mut self,
        connection_id: &str,
        credential_id: &str,
    ) -> Result<bool> {
        let credentials = self.privilege_credentials_for_scope_mut(connection_id)?;
        let before = credentials.len();
        credentials
            .retain(|credential| credential.id != credential_id);
        let removed = before != credentials.len();
        if removed {
            self.touch_privilege_scope(connection_id);
            let keychain_id = privilege_keychain_id(connection_id, credential_id);
            let _ = self.privilege_keychain.delete(&keychain_id);
            self.save()?;
        }
        Ok(removed)
    }

    pub fn get_privilege_credential_secret(
        &self,
        connection_id: &str,
        credential_id: &str,
    ) -> Result<SecretString> {
        let credential = self
            .privilege_credentials_for_scope(connection_id)?
            .iter()
            .find(|credential| credential.id == credential_id)
            .ok_or_else(|| anyhow::anyhow!("Privilege credential not found"))?;
        if !credential.enabled {
            bail!("Privilege credential is disabled");
        }
        let keychain_id = credential
            .keychain_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Privilege credential secret is not saved"))?;
        // This read method is only for the UI-confirmed fill path. Callers must
        // immediately write to PTY and drop the returned SecretString.
        self.privilege_keychain.get(keychain_id)
    }

    fn privilege_credentials_for_scope(
        &self,
        connection_id: &str,
    ) -> Result<&Vec<SavedPrivilegeCredential>> {
        if connection_id == LOCAL_SHELL_PRIVILEGE_CONNECTION_ID {
            // Local shell credentials are app-scoped, not tied to a saved SSH
            // connection. They still reuse the same metadata shape and
            // keychain-only secret boundary as SSH privilege credentials.
            return Ok(&self.data.local_privilege_credentials);
        }
        self.get(connection_id)
            .map(|connection| &connection.privilege_credentials)
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))
    }

    fn privilege_credentials_for_scope_mut(
        &mut self,
        connection_id: &str,
    ) -> Result<&mut Vec<SavedPrivilegeCredential>> {
        if connection_id == LOCAL_SHELL_PRIVILEGE_CONNECTION_ID {
            // Local shell has no SavedConnection row, so edits land in a store
            // level list while secrets remain in the dedicated privilege
            // keychain service.
            return Ok(&mut self.data.local_privilege_credentials);
        }
        self.data
            .connections
            .iter_mut()
            .find(|connection| connection.id == connection_id)
            .map(|connection| &mut connection.privilege_credentials)
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))
    }

    fn touch_privilege_scope(&mut self, connection_id: &str) {
        if connection_id == LOCAL_SHELL_PRIVILEGE_CONNECTION_ID {
            return;
        }
        if let Some(connection) = self
            .data
            .connections
            .iter_mut()
            .find(|connection| connection.id == connection_id)
        {
            connection.updated_at = Some(Utc::now());
        }
    }

    pub fn get_saved_auth_password(&self, auth: &SavedAuth) -> Result<SecretString> {
        match auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => self.keychain.get(keychain_id),
            SavedAuth::Password {
                plaintext_password: Some(password),
                ..
            } => Ok(password.clone()),
            SavedAuth::Password {
                keychain_id: None, ..
            } => bail!("Password not saved for this connection"),
            _ => bail!("Connection does not use password auth"),
        }
    }

    pub fn get_connection_passphrase(&self, id: &str) -> Result<Option<SecretString>> {
        let conn = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;
        self.get_saved_auth_passphrase(&conn.auth)
    }

    pub fn get_saved_auth_passphrase(&self, auth: &SavedAuth) -> Result<Option<SecretString>> {
        match auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            }
            | SavedAuth::Certificate {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => self.keychain.get(keychain_id).map(Some),
            SavedAuth::ManagedKey {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => self.keychain.get(keychain_id).map(Some),
            SavedAuth::Key {
                plaintext_passphrase: Some(passphrase),
                ..
            }
            | SavedAuth::Certificate {
                plaintext_passphrase: Some(passphrase),
                ..
            }
            | SavedAuth::ManagedKey {
                plaintext_passphrase: Some(passphrase),
                ..
            } => Ok(Some(passphrase.clone())),
            SavedAuth::Key { .. }
            | SavedAuth::Certificate { .. }
            | SavedAuth::ManagedKey { .. } => Ok(None),
            _ => bail!("Connection does not use key passphrase auth"),
        }
    }

    pub fn get_saved_upstream_proxy_password(
        &self,
        auth: &SavedUpstreamProxyAuth,
    ) -> Result<SecretString> {
        match auth {
            SavedUpstreamProxyAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => self.keychain.get(keychain_id),
            SavedUpstreamProxyAuth::Password {
                plaintext_password: Some(password),
                ..
            } => Ok(password.clone()),
            SavedUpstreamProxyAuth::Password {
                keychain_id: None, ..
            } => bail!("Upstream proxy password is not saved"),
            SavedUpstreamProxyAuth::None => bail!("Upstream proxy does not use password auth"),
        }
    }

    pub fn save_global_upstream_proxy_password(&self, password: &SecretString) -> Result<String> {
        // The global proxy has one stable keychain slot, separate from
        // connection-scoped oxide_conn_upstream_proxy_* entries.
        self.keychain
            .store(GLOBAL_UPSTREAM_PROXY_PASSWORD_KEYCHAIN_ID, password)?;
        Ok(GLOBAL_UPSTREAM_PROXY_PASSWORD_KEYCHAIN_ID.to_string())
    }

    pub fn delete_global_upstream_proxy_password(&self) -> Result<()> {
        self.keychain
            .delete(GLOBAL_UPSTREAM_PROXY_PASSWORD_KEYCHAIN_ID)
            .map_err(Into::into)
    }

    pub fn get_global_upstream_proxy_password(&self, keychain_id: &str) -> Result<SecretString> {
        if keychain_id != GLOBAL_UPSTREAM_PROXY_PASSWORD_KEYCHAIN_ID {
            bail!("Invalid global upstream proxy keychain id");
        }
        self.keychain.get(keychain_id)
    }

    pub fn create_managed_ssh_key_from_text(
        &mut self,
        private_key: SecretString,
        name: Option<String>,
        passphrase: Option<SecretString>,
    ) -> Result<ManagedSshKeyInfo> {
        self.create_managed_ssh_key(
            private_key,
            name,
            passphrase,
            ManagedSshKeyOrigin::PastedText,
            "Managed SSH Key",
        )
    }

    pub fn create_managed_ssh_key_from_file(
        &mut self,
        path: impl AsRef<Path>,
        name: Option<String>,
        passphrase: Option<SecretString>,
    ) -> Result<ManagedSshKeyInfo> {
        let path = path.as_ref();
        let fallback_name = fallback_name_from_path(path);
        let private_key = SecretString::from(
            fs::read_to_string(path)
                .with_context(|| format!("failed to read SSH private key file {}", path.display()))?,
        );
        self.create_managed_ssh_key(
            private_key,
            name,
            passphrase,
            ManagedSshKeyOrigin::ImportedFile,
            &fallback_name,
        )
    }

    pub fn managed_ssh_keys(&self) -> Vec<ManagedSshKeyInfo> {
        self.data
            .managed_ssh_keys
            .iter()
            .map(ManagedSshKeyInfo::from)
            .collect()
    }

    pub fn managed_ssh_key_metadata(&self, id: &str) -> Result<ManagedSshKey> {
        self.data
            .managed_ssh_keys
            .iter()
            .find(|key| key.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Managed SSH key not found"))
    }

    pub fn rename_managed_ssh_key(
        &mut self,
        id: &str,
        name: String,
    ) -> Result<ManagedSshKeyInfo> {
        let key = self
            .data
            .managed_ssh_keys
            .iter_mut()
            .find(|key| key.id == id)
            .ok_or_else(|| anyhow::anyhow!("Managed SSH key not found"))?;
        key.name = managed_key_display_name(Some(name), "Managed SSH Key");
        key.updated_at = Utc::now();
        let info = ManagedSshKeyInfo::from(&*key);
        self.save()?;
        Ok(info)
    }

    pub fn managed_ssh_key_usage(&self, id: &str) -> Result<ManagedSshKeyUsage> {
        if !self.data.managed_ssh_keys.iter().any(|key| key.id == id) {
            bail!("Managed SSH key not found");
        }
        Ok(managed_key_usage_from_data(&self.data, id))
    }

    pub fn delete_managed_ssh_key(
        &mut self,
        id: &str,
        force: bool,
    ) -> Result<ManagedSshKeyDeleteResult> {
        let usage = self.managed_ssh_key_usage(id)?;
        if usage.count > 0 && !force {
            bail!(
                "Managed SSH key is used by {} saved connection entries",
                usage.count
            );
        }
        let index = self
            .data
            .managed_ssh_keys
            .iter()
            .position(|key| key.id == id)
            .ok_or_else(|| anyhow::anyhow!("Managed SSH key not found"))?;
        let removed = self.data.managed_ssh_keys.remove(index);
        if let Err(error) = self.save() {
            self.data.managed_ssh_keys.push(removed);
            return Err(error);
        }
        self.delete_managed_ssh_key_secret(&removed.secret_id)?;
        Ok(ManagedSshKeyDeleteResult {
            deleted: true,
            key_id: id.to_string(),
            usage,
        })
    }

    pub fn resolve_managed_ssh_key_private_key(&self, id: &str) -> Result<SecretString> {
        let secret_id = self
            .data
            .managed_ssh_keys
            .iter()
            .find(|key| key.id == id)
            .map(|key| key.secret_id.clone())
            .ok_or_else(|| anyhow::anyhow!("Managed SSH key not found"))?;

        // Secret material leaves the managed backend only at the SSH auth boundary.
        // Callers must decode/use it immediately and must not persist this value.
        self.get_managed_ssh_key_secret(&secret_id)
    }

    fn create_managed_ssh_key(
        &mut self,
        private_key: SecretString,
        name: Option<String>,
        passphrase: Option<SecretString>,
        origin: ManagedSshKeyOrigin,
        fallback_name: &str,
    ) -> Result<ManagedSshKeyInfo> {
        let decoded_key = decode_managed_private_key(&private_key, passphrase.as_ref())?;
        let fingerprint = fingerprint_public_key(decoded_key.public_key());
        let public_key = public_key_line_from_private_key(&decoded_key);
        if let Some(existing) = self
            .data
            .managed_ssh_keys
            .iter()
            .find(|key| key.fingerprint == fingerprint)
        {
            bail!("Managed SSH key already exists: {}", existing.name);
        }

        let id = Uuid::new_v4().to_string();
        let secret_id = format!("managed-key-{id}");
        // Private key material crosses into the managed secret backend here;
        // ConnectionStoreData keeps only metadata and the secret id.
        let secret_write = self.store_managed_ssh_key_secret(&secret_id, &private_key)?;

        let now = Utc::now();
        let key = ManagedSshKey {
            id,
            secret_id,
            name: managed_key_display_name(name, fallback_name),
            fingerprint,
            public_key,
            requires_passphrase: managed_key_requires_passphrase(&private_key, passphrase.as_ref()),
            origin,
            created_at: now,
            updated_at: now,
        };
        let info = ManagedSshKeyInfo::from(&key);
        self.data.managed_ssh_keys.push(key);
        if let Err(error) = self.save() {
            if let Some(removed) = self.data.managed_ssh_keys.pop() {
                let _ = self.delete_managed_ssh_key_secret(&removed.secret_id);
                if secret_write.created_config_key {
                    rollback_created_config_key();
                }
            }
            return Err(error);
        }
        Ok(info)
    }

    fn materialize_auth(
        &self,
        auth: SavedAuth,
        existing_auth: Option<&SavedAuth>,
    ) -> Result<SavedAuth> {
        match auth {
            SavedAuth::Password {
                keychain_id,
                plaintext_password,
            } => {
                if let Some(password) = plaintext_password {
                    let keychain_id = existing_password_keychain_id(existing_auth)
                        .or(keychain_id)
                        .unwrap_or_else(new_password_keychain_id);
                    self.keychain.store(&keychain_id, &password)?;
                    Ok(SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                        plaintext_password: None,
                    })
                } else {
                    Ok(SavedAuth::Password {
                        keychain_id,
                        plaintext_password: None,
                    })
                }
            }
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => {
                let retained_id = matching_key_passphrase_id(existing_auth, &key_path);
                if let Some(passphrase) = plaintext_passphrase {
                    let keychain_id = retained_id
                        .or(passphrase_keychain_id)
                        .unwrap_or_else(new_key_passphrase_keychain_id);
                    self.keychain.store(&keychain_id, &passphrase)?;
                    Ok(SavedAuth::Key {
                        key_path,
                        has_passphrase: true,
                        passphrase_keychain_id: Some(keychain_id),
                        plaintext_passphrase: None,
                    })
                } else if let Some((has_passphrase, passphrase_keychain_id)) =
                    matching_key_passphrase(existing_auth, &key_path)
                {
                    Ok(SavedAuth::Key {
                        key_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                } else {
                    let has_passphrase = has_passphrase || passphrase_keychain_id.is_some();
                    Ok(SavedAuth::Key {
                        key_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                }
            }
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => {
                let retained_id =
                    matching_certificate_passphrase_id(existing_auth, &key_path, &cert_path);
                if let Some(passphrase) = plaintext_passphrase {
                    let keychain_id = retained_id
                        .or(passphrase_keychain_id)
                        .unwrap_or_else(new_key_passphrase_keychain_id);
                    self.keychain.store(&keychain_id, &passphrase)?;
                    Ok(SavedAuth::Certificate {
                        key_path,
                        cert_path,
                        has_passphrase: true,
                        passphrase_keychain_id: Some(keychain_id),
                        plaintext_passphrase: None,
                    })
                } else if let Some((has_passphrase, passphrase_keychain_id)) =
                    matching_certificate_passphrase(existing_auth, &key_path, &cert_path)
                {
                    Ok(SavedAuth::Certificate {
                        key_path,
                        cert_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                } else {
                    let has_passphrase = has_passphrase || passphrase_keychain_id.is_some();
                    Ok(SavedAuth::Certificate {
                        key_path,
                        cert_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                }
            }
            SavedAuth::ManagedKey {
                key_id,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => {
                let retained_id = matching_managed_key_passphrase_id(existing_auth, &key_id);
                if let Some(passphrase) = plaintext_passphrase {
                    let keychain_id = retained_id
                        .or(passphrase_keychain_id)
                        .unwrap_or_else(new_key_passphrase_keychain_id);
                    self.keychain.store(&keychain_id, &passphrase)?;
                    Ok(SavedAuth::ManagedKey {
                        key_id,
                        passphrase_keychain_id: Some(keychain_id),
                        plaintext_passphrase: None,
                    })
                } else if let Some(passphrase_keychain_id) =
                    matching_managed_key_passphrase_id(existing_auth, &key_id)
                {
                    Ok(SavedAuth::ManagedKey {
                        key_id,
                        passphrase_keychain_id: Some(passphrase_keychain_id),
                        plaintext_passphrase: None,
                    })
                } else {
                    Ok(SavedAuth::ManagedKey {
                        key_id,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                }
            }
            auth => Ok(auth),
        }
    }

    fn materialize_proxy_chain(&self, proxy_chain: Vec<SavedProxyHop>) -> Result<Vec<SavedProxyHop>> {
        proxy_chain
            .into_iter()
            .map(|hop| {
                Ok(SavedProxyHop {
                    host: non_empty(hop.host.trim(), "Proxy host")?.to_string(),
                    port: hop.port.max(1),
                    username: non_empty(hop.username.trim(), "Proxy username")?.to_string(),
                    auth: self.materialize_auth(hop.auth, None)?,
                    agent_forwarding: hop.agent_forwarding,
                })
            })
            .collect()
    }

    fn materialize_upstream_proxy_policy(
        &self,
        policy: SavedUpstreamProxyPolicy,
        existing_policy: Option<&SavedUpstreamProxyPolicy>,
    ) -> Result<SavedUpstreamProxyPolicy> {
        match policy {
            SavedUpstreamProxyPolicy::UseGlobal => Ok(SavedUpstreamProxyPolicy::UseGlobal),
            SavedUpstreamProxyPolicy::Direct => Ok(SavedUpstreamProxyPolicy::Direct),
            SavedUpstreamProxyPolicy::Custom { proxy } => {
                let auth = self.materialize_upstream_proxy_auth(proxy.auth, existing_policy)?;
                Ok(SavedUpstreamProxyPolicy::Custom {
                    proxy: SavedUpstreamProxyConfig {
                        protocol: proxy.protocol,
                        host: non_empty(proxy.host.trim(), "Upstream proxy host")?.to_string(),
                        port: proxy.port.max(1),
                        auth,
                        remote_dns: proxy.remote_dns,
                        no_proxy: proxy.no_proxy.trim().to_string(),
                    },
                })
            }
        }
    }

    fn materialize_upstream_proxy_auth(
        &self,
        auth: SavedUpstreamProxyAuth,
        existing_policy: Option<&SavedUpstreamProxyPolicy>,
    ) -> Result<SavedUpstreamProxyAuth> {
        match auth {
            SavedUpstreamProxyAuth::None => Ok(SavedUpstreamProxyAuth::None),
            SavedUpstreamProxyAuth::Password {
                username,
                keychain_id,
                plaintext_password,
            } => {
                let username = non_empty(username.trim(), "Upstream proxy username")?.to_string();
                if let Some(password) = plaintext_password {
                    let keychain_id = existing_upstream_proxy_password_keychain_id(existing_policy)
                        .or(keychain_id)
                        .unwrap_or_else(new_upstream_proxy_password_keychain_id);
                    self.keychain.store(&keychain_id, &password)?;
                    Ok(SavedUpstreamProxyAuth::Password {
                        username,
                        keychain_id: Some(keychain_id),
                        plaintext_password: None,
                    })
                } else {
                    Ok(SavedUpstreamProxyAuth::Password {
                        username,
                        keychain_id,
                        plaintext_password: None,
                    })
                }
            }
        }
    }

    fn stage_imported_connection(
        &mut self,
        mut connection: SavedConnection,
    ) -> Result<StagedImportedConnection> {
        let group = normalize_optional_group_name(connection.group.as_deref())?;
        let now = Utc::now();
        if connection.id.trim().is_empty() {
            connection.id = Uuid::new_v4().to_string();
        }
        let id = connection.id.clone();
        let existing = self.get(&id).cloned();
        let old_keychain_ids = existing
            .as_ref()
            .map(collect_connection_keychain_ids)
            .unwrap_or_default();
        let existing_auth = existing.as_ref().map(|conn| conn.auth.clone());

        connection.version = CONFIG_VERSION;
        connection.name = non_empty(connection.name.trim(), "Connection name")?.to_string();
        connection.group = group.clone();
        connection.host = non_empty(connection.host.trim(), "Host")?.to_string();
        connection.port = connection.port.max(1);
        connection.username = non_empty(connection.username.trim(), "Username")?.to_string();
        for hop in &connection.proxy_chain {
            non_empty(hop.host.trim(), "Proxy host")?;
            non_empty(hop.username.trim(), "Proxy username")?;
        }

        let auth = self.materialize_auth(connection.auth, existing_auth.as_ref())?;
        let mut touched_keychain_ids = collect_keychain_ids_for_auth(&auth);
        let mut proxy_chain = Vec::with_capacity(connection.proxy_chain.len());
        for hop in connection.proxy_chain {
            let hop_auth = self.materialize_auth(hop.auth, None)?;
            touched_keychain_ids.extend(collect_keychain_ids_for_auth(&hop_auth));
            proxy_chain.push(SavedProxyHop {
                host: non_empty(hop.host.trim(), "Proxy host")?.to_string(),
                port: hop.port.max(1),
                username: non_empty(hop.username.trim(), "Proxy username")?.to_string(),
                auth: hop_auth,
                agent_forwarding: hop.agent_forwarding,
            });
        }

        let upstream_proxy = self.materialize_upstream_proxy_policy(
            connection.upstream_proxy,
            existing.as_ref().map(|conn| &conn.upstream_proxy),
        )?;
        touched_keychain_ids.extend(collect_keychain_ids_for_upstream_proxy(&upstream_proxy));
        connection.auth = auth;
        connection.proxy_chain = proxy_chain;
        connection.upstream_proxy = upstream_proxy;
        if let Some(existing) = existing.as_ref() {
            connection.created_at = existing.created_at;
            connection.last_used_at = existing.last_used_at;
            if connection.privilege_credentials.is_empty() {
                // Transaction imports mirror Tauri's merge semantics: ordinary
                // connection imports leave locally configured privilege helpers
                // attached unless an encrypted import explicitly carries them.
                connection.privilege_credentials = existing.privilege_credentials.clone();
            }
        } else if connection.created_at.timestamp() <= 0 {
            connection.created_at = now;
        }
        let (privilege_credentials, touched_privilege_keychain_ids) =
            self.materialize_privilege_credentials(&connection.id, connection.privilege_credentials)?;
        connection.privilege_credentials = privilege_credentials;
        connection.updated_at = Some(now);

        let next_keychain_ids = collect_keychain_ids_for_parts(
            &connection.auth,
            &connection.proxy_chain,
            &connection.upstream_proxy,
        );
        let stale_old_keychain_ids = old_keychain_ids
            .into_iter()
            .filter(|keychain_id| !next_keychain_ids.contains(keychain_id))
            .collect::<Vec<_>>();

        if let Some(index) = self
            .data
            .connections
            .iter()
            .position(|candidate| candidate.id == id)
        {
            self.data.connections[index] = connection;
        } else {
            self.data.connections.push(connection);
        }
        if let Some(group) = group {
            self.ensure_group(group)?;
        }

        Ok(StagedImportedConnection {
            id,
            touched_keychain_ids,
            touched_privilege_keychain_ids,
            stale_old_keychain_ids,
        })
    }

    fn snapshot_keychain_entries(
        &self,
        data: &ConnectionStoreData,
    ) -> HashMap<String, Option<SecretString>> {
        data.connections
            .iter()
            .flat_map(collect_connection_keychain_ids)
            .map(|keychain_id| {
                let value = self.keychain.get(&keychain_id).ok();
                (keychain_id, value)
            })
            .collect()
    }

    fn snapshot_privilege_keychain_entries(
        &self,
        data: &ConnectionStoreData,
    ) -> HashMap<String, Option<SecretString>> {
        data.connections
            .iter()
            .flat_map(collect_privilege_keychain_ids)
            .chain(
                data.local_privilege_credentials
                    .iter()
                    .filter_map(|credential| credential.keychain_id.clone()),
            )
            .map(|keychain_id| {
                let value = self.privilege_keychain.get(&keychain_id).ok();
                (keychain_id, value)
            })
            .collect()
    }

    fn snapshot_managed_keychain_entries(
        &self,
        data: &ConnectionStoreData,
    ) -> HashMap<String, Option<SecretString>> {
        data.managed_ssh_keys
            .iter()
            .map(|key| {
                let value = self.get_managed_ssh_key_secret(&key.secret_id).ok();
                (key.secret_id.clone(), value)
            })
            .collect()
    }

    fn rollback_keychain_entries(
        &self,
        touched_keychain_ids: &HashSet<String>,
        original_keychain: &HashMap<String, Option<SecretString>>,
    ) {
        for keychain_id in touched_keychain_ids {
            match original_keychain.get(keychain_id) {
                Some(Some(secret)) => {
                    let _ = self.keychain.store(keychain_id, secret);
                }
                Some(None) | None => {
                    let _ = self.keychain.delete(keychain_id);
                }
            }
        }
    }

    fn rollback_privilege_keychain_entries(
        &self,
        touched_keychain_ids: &HashSet<String>,
        original_keychain: &HashMap<String, Option<SecretString>>,
    ) {
        for keychain_id in touched_keychain_ids {
            match original_keychain.get(keychain_id) {
                Some(Some(secret)) => {
                    let _ = self.privilege_keychain.store(keychain_id, secret);
                }
                Some(None) | None => {
                    let _ = self.privilege_keychain.delete(keychain_id);
                }
            }
        }
    }

    fn rollback_managed_keychain_entries(
        &self,
        touched_secret_ids: &HashSet<String>,
        original_keychain: &HashMap<String, Option<SecretString>>,
    ) {
        for secret_id in touched_secret_ids {
            match original_keychain.get(secret_id) {
                Some(Some(secret)) => {
                    let _ = self.store_managed_ssh_key_secret(secret_id, secret);
                }
                Some(None) | None => {
                    let _ = self.delete_managed_ssh_key_secret(secret_id);
                }
            }
        }
    }

    fn materialize_privilege_credentials(
        &self,
        connection_id: &str,
        credentials: Vec<SavedPrivilegeCredential>,
    ) -> Result<(Vec<SavedPrivilegeCredential>, Vec<String>)> {
        let mut touched_keychain_ids = Vec::new();
        let mut materialized = Vec::with_capacity(credentials.len());
        for mut credential in credentials {
            if credential.connection_id.trim().is_empty() {
                credential.connection_id = connection_id.to_string();
            }
            if let Some(secret) = credential.plaintext_secret.take() {
                let keychain_id = privilege_keychain_id(&credential.connection_id, &credential.id);
                self.privilege_keychain.store(&keychain_id, &secret)?;
                credential.keychain_id = Some(keychain_id.clone());
                touched_keychain_ids.push(keychain_id);
            }
            materialized.push(credential);
        }
        Ok((materialized, touched_keychain_ids))
    }

    fn clone_auth_secret(&self, auth: &SavedAuth) -> Result<SavedAuth> {
        match auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => {
                let password = self.keychain.get(keychain_id)?;
                let next_keychain_id = new_password_keychain_id();
                self.keychain.store(&next_keychain_id, &password)?;
                Ok(SavedAuth::Password {
                    keychain_id: Some(next_keychain_id),
                    plaintext_password: None,
                })
            }
            SavedAuth::Password {
                keychain_id: None, ..
            } => Ok(SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            }),
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id: Some(passphrase_keychain_id),
                ..
            } => {
                let passphrase = self.keychain.get(passphrase_keychain_id)?;
                let next_keychain_id = new_key_passphrase_keychain_id();
                self.keychain.store(&next_keychain_id, &passphrase)?;
                Ok(SavedAuth::Key {
                    key_path: key_path.clone(),
                    has_passphrase: *has_passphrase,
                    passphrase_keychain_id: Some(next_keychain_id),
                    plaintext_passphrase: None,
                })
            }
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase,
                passphrase_keychain_id: Some(passphrase_keychain_id),
                ..
            } => {
                let passphrase = self.keychain.get(passphrase_keychain_id)?;
                let next_keychain_id = new_key_passphrase_keychain_id();
                self.keychain.store(&next_keychain_id, &passphrase)?;
                Ok(SavedAuth::Certificate {
                    key_path: key_path.clone(),
                    cert_path: cert_path.clone(),
                    has_passphrase: *has_passphrase,
                    passphrase_keychain_id: Some(next_keychain_id),
                    plaintext_passphrase: None,
                })
            }
            auth => Ok(auth.clone()),
        }
    }

    fn migrate_legacy_credentials(&mut self) -> Result<bool> {
        let mut migrated = false;
        for conn in &mut self.data.connections {
            migrated |= migrate_legacy_auth_credentials(&mut conn.auth, &self.keychain)?;
            for hop in &mut conn.proxy_chain {
                migrated |= migrate_legacy_auth_credentials(&mut hop.auth, &self.keychain)?;
            }
        }
        Ok(migrated)
    }

    fn normalize(&mut self) {
        self.data.connection_tombstones =
            active_connection_tombstones(&self.data.connection_tombstones);
        self.data
            .recent
            .retain(|recent_id| self.data.connections.iter().any(|conn| &conn.id == recent_id));
        self.data.recent.dedup();
        self.data
            .groups
            .sort_by(|left, right| left.to_lowercase().cmp(&right.to_lowercase()));
        self.data.groups.dedup();
        let implicit_groups = self
            .data
            .connections
            .iter()
            .filter_map(|conn| conn.group.clone())
            .collect::<Vec<_>>();
        for group in implicit_groups {
            if !self.data.groups.contains(&group) {
                self.data.groups.push(group);
            }
        }
        for conn in &mut self.data.connections {
            if conn.options.post_connect_command.is_none() {
                conn.options.post_connect_command = conn.post_connect_command.take();
            } else {
                conn.post_connect_command = None;
            }
        }
        self.data
            .connections
            .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    }

    fn add_connection(&mut self, connection: SavedConnection) {
        self.data
            .connections
            .retain(|candidate| candidate.id != connection.id);
        self.data
            .connection_tombstones
            .retain(|tombstone| tombstone.id != connection.id);
        self.data.connections.push(connection);
    }

    fn remove_connection_without_tombstone(&mut self, id: &str) -> Option<SavedConnection> {
        let position = self
            .data
            .connections
            .iter()
            .position(|connection| connection.id == id)?;
        self.data.recent.retain(|recent_id| recent_id != id);
        Some(self.data.connections.remove(position))
    }

    fn remove_connection_with_tombstone_at(
        &mut self,
        id: &str,
        deleted_at: DateTime<Utc>,
    ) -> Option<SavedConnection> {
        let removed = self.remove_connection_without_tombstone(id)?;
        self.upsert_connection_tombstone(removed.id.clone(), deleted_at);
        Some(removed)
    }

    fn upsert_connection_tombstone(&mut self, id: String, deleted_at: DateTime<Utc>) -> bool {
        self.data.connection_tombstones =
            active_connection_tombstones(&self.data.connection_tombstones);
        if let Some(existing) = self
            .data
            .connection_tombstones
            .iter_mut()
            .find(|tombstone| tombstone.id == id)
        {
            if existing.deleted_at >= deleted_at {
                return false;
            }
            existing.deleted_at = deleted_at;
            return true;
        }
        self.data
            .connection_tombstones
            .push(DeletedConnectionTombstone { id, deleted_at });
        true
    }
}
