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
        let data = if path.exists() {
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?
        } else {
            ConnectionStoreData::default()
        };
        Ok(Self {
            path,
            data,
            keychain: ConnectionKeychain::default(),
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
        let data = serde_json::to_vec_pretty(&self.data)?;
        fs::write(&self.path, data)
            .with_context(|| format!("failed to write {}", self.path.display()))
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
        let next_keychain_ids = collect_keychain_ids_for_parts(&auth, &proxy_chain);
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
            post_connect_command: request
                .post_connect_command
                .and_then(|command| {
                    let command = command.trim().to_string();
                    (!command.is_empty()).then_some(command)
                }),
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
        let deleted = self
            .remove_connection_with_tombstone_at(id, Utc::now())
            .is_some();
        if deleted {
            self.normalize();
            self.save()?;
            for keychain_id in keychain_ids {
                let _ = self.keychain.delete(&keychain_id);
            }
        }
        Ok(deleted)
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
        if let Some(existing) = existing.as_ref() {
            connection.created_at = existing.created_at;
            connection.last_used_at = existing.last_used_at;
        } else if connection.created_at.timestamp() <= 0 {
            connection.created_at = now;
        }
        connection.updated_at = Some(now);

        let next_keychain_ids =
            collect_keychain_ids_for_parts(&connection.auth, &connection.proxy_chain);
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
        let original_data = self.data.clone();
        let original_keychain = self.snapshot_keychain_entries(&original_data);
        let mut touched_keychain_ids = HashSet::new();
        let mut stale_old_keychain_ids = HashSet::new();
        let mut imported_ids = Vec::new();

        let result = (|| {
            for connection in connections {
                let staged = self.stage_imported_connection(connection)?;
                touched_keychain_ids.extend(staged.touched_keychain_ids);
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
            SavedAuth::Key {
                plaintext_passphrase: Some(passphrase),
                ..
            }
            | SavedAuth::Certificate {
                plaintext_passphrase: Some(passphrase),
                ..
            } => Ok(Some(passphrase.clone())),
            SavedAuth::Key { .. } | SavedAuth::Certificate { .. } => Ok(None),
            _ => bail!("Connection does not use key passphrase auth"),
        }
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

        connection.auth = auth;
        connection.proxy_chain = proxy_chain;
        if let Some(existing) = existing.as_ref() {
            connection.created_at = existing.created_at;
            connection.last_used_at = existing.last_used_at;
        } else if connection.created_at.timestamp() <= 0 {
            connection.created_at = now;
        }
        connection.updated_at = Some(now);

        let next_keychain_ids =
            collect_keychain_ids_for_parts(&connection.auth, &connection.proxy_chain);
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
        self.data.connection_tombstones = active_connection_tombstones(&self.data.connection_tombstones);
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
