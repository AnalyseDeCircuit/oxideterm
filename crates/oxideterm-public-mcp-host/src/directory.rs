use super::*;
impl PublicMcpHost {
    pub fn connection_id(
        &mut self,
        client_ref: &ClientRef,
        connection_ref: &ConnectionRef,
        store: &ConnectionStore,
    ) -> Option<String> {
        self.sync_connection_refs(client_ref, store);
        self.connection_ids
            .get(connection_ref)
            .filter(|(owner, _)| owner == client_ref)
            .and_then(|(_, connection_key)| {
                connection_key
                    .strip_prefix(CONNECTION_KEY_SSH_PREFIX)
                    .map(ToOwned::to_owned)
            })
    }

    pub fn connection_key(
        &mut self,
        client_ref: &ClientRef,
        connection_ref: &ConnectionRef,
        store: &ConnectionStore,
    ) -> Option<String> {
        self.sync_connection_refs(client_ref, store);
        self.connection_ids
            .get(connection_ref)
            .filter(|(owner, _)| owner == client_ref)
            .map(|(_, connection_key)| connection_key.clone())
    }

    fn sync_connection_refs(&mut self, client_ref: &ClientRef, store: &ConnectionStore) {
        let mut valid_internal_keys = HashSet::new();
        for info in store.connection_infos() {
            valid_internal_keys.insert(format!("{CONNECTION_KEY_SSH_PREFIX}{}", info.id));
        }
        for profile in store.serial_profiles() {
            valid_internal_keys.insert(format!("{CONNECTION_KEY_SERIAL_PREFIX}{}", profile.id));
        }
        for profile in store.telnet_profiles() {
            valid_internal_keys.insert(format!("{CONNECTION_KEY_TELNET_PREFIX}{}", profile.id));
        }
        for profile in store.mosh_profiles() {
            valid_internal_keys.insert(format!("{CONNECTION_KEY_MOSH_PREFIX}{}", profile.id));
        }
        for profile in store.remote_desktop_profiles() {
            valid_internal_keys.insert(format!("{CONNECTION_KEY_DESKTOP_PREFIX}{}", profile.id));
        }

        // Keep stable refs for live profiles while dropping handles whose saved
        // records were removed through the UI, import, sync, or another client.
        let stale_refs = self
            .connection_refs
            .extract_if(|(owner, internal_key), _| {
                owner == client_ref && !valid_internal_keys.contains(internal_key)
            })
            .map(|(_, connection_ref)| connection_ref)
            .collect::<HashSet<_>>();
        self.connection_ids
            .retain(|connection_ref, _| !stale_refs.contains(connection_ref));
        for internal_key in valid_internal_keys {
            self.ensure_connection_ref(client_ref, internal_key);
        }
    }

    pub fn ensure_connection_ref(
        &mut self,
        client_ref: &ClientRef,
        internal_key: String,
    ) -> ConnectionRef {
        let connection_ref = self
            .connection_refs
            .entry((client_ref.clone(), internal_key.clone()))
            .or_default()
            .clone();
        self.connection_ids
            .entry(connection_ref.clone())
            .or_insert_with(|| (client_ref.clone(), internal_key));
        connection_ref
    }

    pub fn remove_client_connection_refs(&mut self, client_ref: &ClientRef) {
        let removed_refs = self
            .connection_refs
            .extract_if(|(owner, _), _| owner == client_ref)
            .map(|(_, connection_ref)| connection_ref)
            .collect::<HashSet<_>>();
        self.connection_ids
            .retain(|connection_ref, _| !removed_refs.contains(connection_ref));
    }

    pub fn remove_client_quick_command_refs(&mut self, client_ref: &ClientRef) {
        let mut directory = self.quick_commands.lock();
        let removed_refs = directory
            .quick_command_refs
            .extract_if(|(owner, _), _| owner == client_ref)
            .map(|(_, quickcommand_ref)| quickcommand_ref)
            .collect::<HashSet<_>>();
        directory
            .quick_command_ids
            .retain(|quickcommand_ref, _| !removed_refs.contains(quickcommand_ref));
    }

    pub fn target_label(
        &self,
        client_ref: &ClientRef,
        target: &str,
        store: &ConnectionStore,
        node_router: &NodeRouter,
        plugin_registry: &plugin_host::NativePluginRegistry,
        forwarding_service: &dyn PublicMcpForwardPort,
    ) -> String {
        if let Some(quickcommand_ref) = target
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<QuickCommandRef>().ok())
            && let Some((owner, command_id)) = self
                .quick_commands
                .lock()
                .quick_command_ids
                .get(&quickcommand_ref)
                .cloned()
            && &owner == client_ref
            && let Ok(snapshot) = oxideterm_quick_commands::load_snapshot(&self.settings_path)
            && let Some(command) = snapshot
                .commands
                .into_iter()
                .find(|command| command.id == command_id)
        {
            // The frozen expanded command, when present, is rendered by ApprovalReview.
            return command.name;
        }
        let connection_target = target.split_whitespace().next().unwrap_or(target);
        if let Ok(connection_ref) = connection_target.parse::<ConnectionRef>()
            && let Some((owner, connection_key)) = self.connection_ids.get(&connection_ref)
            && owner == client_ref
        {
            if let Some(connection_id) = connection_key.strip_prefix(CONNECTION_KEY_SSH_PREFIX)
                && let Some(connection) = store.get(connection_id)
            {
                return format!(
                    "{} ({}@{}:{})",
                    connection.name, connection.username, connection.host, connection.port
                );
            }
            if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_SERIAL_PREFIX)
                && let Some(profile) = store
                    .serial_profiles()
                    .iter()
                    .find(|profile| profile.id == profile_id)
            {
                return format!(
                    "{} ({} @ {})",
                    profile.name, profile.port_path, profile.baud_rate
                );
            }
            if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_TELNET_PREFIX)
                && let Some(profile) = store
                    .telnet_profiles()
                    .iter()
                    .find(|profile| profile.id == profile_id)
            {
                return format!("{} ({}:{})", profile.name, profile.host, profile.port);
            }
            if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX)
                && let Some(profile) = store
                    .mosh_profiles()
                    .iter()
                    .find(|profile| profile.id == profile_id)
            {
                return format!(
                    "{} ({}@{}:{})",
                    profile.name, profile.username, profile.host, profile.ssh_port
                );
            }
            if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX)
                && let Some(profile) = store
                    .remote_desktop_profiles()
                    .iter()
                    .find(|profile| profile.id == profile_id)
            {
                return format!(
                    "{} ({}://{}:{})",
                    profile.name,
                    profile.protocol.provider_id(),
                    profile.host,
                    profile.port
                );
            }
        }
        if let Ok(terminal_ref) = connection_target.parse::<TerminalRef>()
            && let Some(record) = self.terminal(client_ref, &terminal_ref)
            && &record.client_ref == client_ref
        {
            let action = target
                .strip_prefix(connection_target)
                .unwrap_or_default()
                .trim();
            return format!("{} ({}) {}", record.title, record.transport, action)
                .trim()
                .to_owned();
        }
        if let Ok(desktop_ref) = connection_target.parse::<DesktopRef>()
            && let Some(record) = self.desktop(client_ref, &desktop_ref)
            && &record.client_ref == client_ref
        {
            let action = target
                .strip_prefix(connection_target)
                .unwrap_or_default()
                .trim();
            return format!("{} {}", record.title, action).trim().to_owned();
        }
        let (addon_target, addon_action) = target.split_once(' ').unwrap_or((target, ""));
        if let Ok(addon_ref) = addon_target.parse::<AddonRef>()
            && let Some((owner, plugin_id)) = self.addon_ids.get(&addon_ref)
            && owner == client_ref
            && let Some(plugin) = plugin_registry
                .plugins()
                .iter()
                .find(|plugin| &plugin.manifest.id == plugin_id)
        {
            return format!(
                "{} ({}) {}",
                plugin.manifest.name, plugin.manifest.id, addon_action
            )
            .trim()
            .to_owned();
        }
        let (forward_target, forward_action) = target.split_once(' ').unwrap_or((target, ""));
        if let Ok(forward_ref) = forward_target.parse::<ForwardRef>()
            && let Some(record) = self
                .runtime_handles
                .lock()
                .forwards
                .get(&forward_ref)
                .filter(|record| record.client_ref == *client_ref)
                .cloned()
            && let Some(rule) = forwarding_service
                .public_mcp_rules_for_node(&record.node_id)
                .into_iter()
                .find(|rule| rule.id == record.forward_id)
        {
            let destination = match rule.forward_type {
                oxideterm_forwarding::ForwardType::Dynamic => "SOCKS".to_owned(),
                oxideterm_forwarding::ForwardType::Local
                | oxideterm_forwarding::ForwardType::Remote => {
                    format!("{}:{}", rule.target_host, rule.target_port)
                }
            };
            return format!(
                "{}:{} → {} {}",
                rule.bind_address, rule.bind_port, destination, forward_action
            )
            .trim()
            .to_owned();
        }
        let (file_target, file_action) = target.split_once(' ').unwrap_or((target, ""));
        if let Ok(file_session_ref) = file_target.parse::<FileSessionRef>()
            && let Some(record) = self
                .runtime_handles
                .lock()
                .file_sessions
                .get(&file_session_ref)
                .filter(|record| record.client_ref == *client_ref)
                .cloned()
        {
            return format!(
                "SFTP {} {}",
                record.root.as_deref().unwrap_or("opening"),
                file_action
            )
            .trim()
            .to_owned();
        }
        if let Ok(workspace_ref) = file_target.parse::<WorkspaceRef>()
            && let Some(record) = self
                .runtime_handles
                .lock()
                .workspaces
                .get(&workspace_ref)
                .filter(|record| record.client_ref == *client_ref)
                .cloned()
        {
            return format!("IDE {} {}", record.root, file_action)
                .trim()
                .to_owned();
        }
        let node_target = target.split_whitespace().next().unwrap_or(target);
        if let Ok(node_ref) = node_target.parse::<NodeRef>()
            && let Some(lease) = node_lease_for_client(&self.runtime_handles, client_ref, &node_ref)
            && let Some(metadata) = node_router.node_metadata(&lease.node_id)
        {
            return format!("{}@{}:{}", metadata.username, metadata.host, metadata.port);
        }
        target.to_owned()
    }
}

// This snapshot fixes references before dispatch. A delayed read cannot change the
// canonical directory after a profile edit or revive mappings after client revocation.
struct ConnectionDirectorySnapshot {
    refs: HashMap<String, ConnectionRef>,
}

impl PublicMcpHost {
    pub fn read_connection_directory(
        &mut self,
        background: &PublicMcpBackground,
        store: &ConnectionStore,
        request: DomainRequest,
    ) {
        let snapshot_or_key = match &request.call {
            PublicToolCall::BrowseConnections(_) => {
                self.sync_connection_refs(&request.client_ref, store);
                Ok(ConnectionDirectorySnapshot {
                    refs: self
                        .connection_refs
                        .iter()
                        .filter(|((client, _), _)| client == &request.client_ref)
                        .map(|((_, key), reference)| (key.clone(), reference.clone()))
                        .collect(),
                })
            }
            PublicToolCall::DescribeConnection(args) => {
                let Some(key) =
                    self.connection_key(&request.client_ref, &args.connection_ref, store)
                else {
                    request.finish(ToolEnvelope::failed("The connection handle is unavailable"));
                    return;
                };
                Err(key)
            }
            _ => unreachable!(),
        };
        let store = store.clone();
        let admission = background.admission.clone();
        background.forwarding_runtime.spawn_blocking(move || {
            if request.is_cancelled() {
                return;
            }
            let result = match (&request.call, snapshot_or_key) {
                (PublicToolCall::BrowseConnections(args), Ok(snapshot)) => {
                    Ok(snapshot.browse(&store, args))
                }
                (PublicToolCall::DescribeConnection(args), Err(key)) => {
                    describe_connection(&store, &args.connection_ref, &key)
                }
                _ => unreachable!(),
            };
            let suspended = admission.lock();
            if request.is_cancelled() {
                return;
            }
            if *suspended {
                request.finish(ToolEnvelope::failed("The OxideTerm workspace is locked"));
                return;
            }
            match result {
                Ok(data) => finish_serialized(request, data),
                Err(error) => request.finish(ToolEnvelope::failed(error)),
            }
        });
    }
}

impl ConnectionDirectorySnapshot {
    fn connection_directory_entry(&self, info: ConnectionInfo) -> PublicConnectionDirectoryEntry {
        let internal_key = format!("{CONNECTION_KEY_SSH_PREFIX}{}", info.id);
        let connection_ref = self.refs[&internal_key].clone();
        PublicConnectionDirectoryEntry {
            connection_ref,
            name: info.name,
            group: info.group,
            connection_type: CONNECTION_TYPE_SSH,
            tags: info.tags,
            last_used_at: info.last_used_at,
        }
    }

    fn profile_directory_entry(
        &self,
        internal_key: String,
        name: String,
        group: Option<String>,
        connection_type: &'static str,
        last_used_at: Option<String>,
    ) -> PublicConnectionDirectoryEntry {
        let connection_ref = self.refs[&internal_key].clone();
        PublicConnectionDirectoryEntry {
            connection_ref,
            name,
            group,
            connection_type,
            tags: Vec::new(),
            last_used_at,
        }
    }

    fn browse(
        &self,
        store: &ConnectionStore,
        args: &oxideterm_public_mcp::calls::BrowseConnectionsArgs,
    ) -> serde_json::Value {
        let query = args
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let requested_types = args
            .connection_types
            .iter()
            .map(|connection_type| connection_type.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let includes = |connection_type: &str| {
            requested_types.is_empty() || requested_types.contains(connection_type)
        };
        let mut connections = Vec::new();
        if includes(CONNECTION_TYPE_SSH) {
            for connection in store
                .connection_infos()
                .into_iter()
                .filter(|connection| connection_directory_matches_query(connection, &query))
            {
                connections.push(self.connection_directory_entry(connection));
            }
        }
        if includes(CONNECTION_TYPE_SERIAL) {
            for profile in store.serial_profiles().to_vec() {
                if public_profile_matches_query(
                    &profile.name,
                    profile.group.as_deref(),
                    &[&profile.port_path, &profile.baud_rate.to_string()],
                    &query,
                ) {
                    connections.push(self.profile_directory_entry(
                        format!("{CONNECTION_KEY_SERIAL_PREFIX}{}", profile.id),
                        profile.name,
                        profile.group,
                        CONNECTION_TYPE_SERIAL,
                        profile.last_used_at.map(|time| time.to_rfc3339()),
                    ));
                }
            }
        }
        if includes(CONNECTION_TYPE_TELNET) {
            for profile in store.telnet_profiles().to_vec() {
                if public_profile_matches_query(
                    &profile.name,
                    profile.group.as_deref(),
                    &[&profile.host, &profile.port.to_string()],
                    &query,
                ) {
                    connections.push(self.profile_directory_entry(
                        format!("{CONNECTION_KEY_TELNET_PREFIX}{}", profile.id),
                        profile.name,
                        profile.group,
                        CONNECTION_TYPE_TELNET,
                        profile.last_used_at.map(|time| time.to_rfc3339()),
                    ));
                }
            }
        }
        if includes(CONNECTION_TYPE_MOSH) {
            for profile in store.mosh_profiles().to_vec() {
                if public_profile_matches_query(
                    &profile.name,
                    profile.group.as_deref(),
                    &[
                        &profile.host,
                        &profile.username,
                        &profile.ssh_port.to_string(),
                    ],
                    &query,
                ) {
                    connections.push(self.profile_directory_entry(
                        format!("{CONNECTION_KEY_MOSH_PREFIX}{}", profile.id),
                        profile.name,
                        profile.group,
                        CONNECTION_TYPE_MOSH,
                        profile.last_used_at.map(|time| time.to_rfc3339()),
                    ));
                }
            }
        }
        for profile in store.remote_desktop_profiles().to_vec() {
            let connection_type = profile.protocol.provider_id();
            if includes(connection_type)
                && public_profile_matches_query(
                    &profile.name,
                    profile.group.as_deref(),
                    &[&profile.host, &profile.port.to_string()],
                    &query,
                )
            {
                connections.push(self.profile_directory_entry(
                    format!("{CONNECTION_KEY_DESKTOP_PREFIX}{}", profile.id),
                    profile.name,
                    profile.group,
                    connection_type,
                    profile.last_used_at.map(|time| time.to_rfc3339()),
                ));
            }
        }
        json!({ "connections": connections })
    }
}

fn describe_connection(
    store: &ConnectionStore,
    connection_ref: &ConnectionRef,
    connection_key: &str,
) -> Result<serde_json::Value, &'static str> {
    if let Some(connection_id) = connection_key.strip_prefix(CONNECTION_KEY_SSH_PREFIX)
        && let Some(connection) = store.get(connection_id)
    {
        let projection = connections::ssh_connection_projection(
            connection_ref,
            connections::connection_revision(store, connection_key)
                .unwrap_or_else(|| "unavailable".to_owned()),
            connection,
        );
        return Ok(json!({ "connection": projection }));
    }
    if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_SERIAL_PREFIX)
        && let Some(profile) = store
            .serial_profiles()
            .iter()
            .find(|profile| profile.id == profile_id)
    {
        return Ok(json!({
            "connection": {
                "connection_ref": connection_ref,
                "revision": connections::connection_revision(store, connection_key),
                "type": CONNECTION_TYPE_SERIAL,
                "name": profile.name,
                "group": profile.group,
                "notes": profile.notes,
                "port_path": profile.port_path,
                "baud_rate": profile.baud_rate,
                "data_bits": profile.data_bits,
                "stop_bits": profile.stop_bits,
                "parity": profile.parity,
                "flow_control": profile.flow_control,
                "connect_on_open": profile.connect_on_open,
                "color": profile.color,
                "icon_background_color": profile.icon_background_color,
                "icon": profile.icon,
                "last_used_at": profile.last_used_at.map(|time| time.to_rfc3339()),
            }
        }));
    }
    if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_TELNET_PREFIX)
        && let Some(profile) = store
            .telnet_profiles()
            .iter()
            .find(|profile| profile.id == profile_id)
    {
        return Ok(json!({
            "connection": {
                "connection_ref": connection_ref,
                "revision": connections::connection_revision(store, connection_key),
                "type": CONNECTION_TYPE_TELNET,
                "name": profile.name,
                "group": profile.group,
                "notes": profile.notes,
                "host": profile.host,
                "port": profile.port,
                "terminal": connections::terminal_options_projection(&profile.terminal),
                "connect_on_open": profile.connect_on_open,
                "color": profile.color,
                "icon_background_color": profile.icon_background_color,
                "icon": profile.icon,
                "last_used_at": profile.last_used_at.map(|time| time.to_rfc3339()),
            }
        }));
    }
    if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX)
        && let Some(profile) = store
            .mosh_profiles()
            .iter()
            .find(|profile| profile.id == profile_id)
    {
        return Ok(json!({
            "connection": {
                "connection_ref": connection_ref,
                "revision": connections::connection_revision(store, connection_key),
                "type": CONNECTION_TYPE_MOSH,
                "name": profile.name,
                "group": profile.group,
                "notes": profile.notes,
                "host": profile.host,
                "ssh_port": profile.ssh_port,
                "username": profile.username,
                "auth": connections::auth_projection(&profile.auth),
                "server_executable": profile.server_executable,
                "udp_host_override": profile.udp_host_override,
                "udp_port": profile.udp_port,
                "ip_family": profile.ip_family,
                "prediction": profile.prediction,
                "locale": profile.locale,
                "identity_agent": profile.identity_agent,
                "legacy_ssh_compatibility": profile.legacy_ssh_compatibility,
                "color": profile.color,
                "icon_background_color": profile.icon_background_color,
                "icon": profile.icon,
                "last_used_at": profile.last_used_at.map(|time| time.to_rfc3339()),
            }
        }));
    }
    if let Some(profile_id) = connection_key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX)
        && let Some(profile) = store
            .remote_desktop_profiles()
            .iter()
            .find(|profile| profile.id == profile_id)
    {
        return Ok(json!({
            "connection": {
                "connection_ref": connection_ref,
                "revision": connections::connection_revision(store, connection_key),
                "type": profile.protocol.provider_id(),
                "name": profile.name,
                "group": profile.group,
                "notes": profile.notes,
                "host": profile.host,
                "port": profile.port,
                "username": profile.username,
                "domain": profile.domain,
                "credential_configured": profile.credential_ref.is_some(),
                "read_only": profile.read_only,
                "options": connections::remote_desktop_options_projection(&profile.session_options),
                "color": profile.color,
                "icon_background_color": profile.icon_background_color,
                "icon": profile.icon,
                "last_used_at": profile.last_used_at.map(|time| time.to_rfc3339()),
            }
        }));
    }
    Err("The saved connection no longer exists")
}
