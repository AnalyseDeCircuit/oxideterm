use super::*;
use crate::workspace::new_connection::{MoshConnectionOptions, SshTerminalConnectionOptions};
use crate::workspace::root::init::terminal_preference_overrides;
use oxideterm_connections::SshChannelStrategy;
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopProtocol,
    RemoteDesktopSecret,
};
use oxideterm_session_adapter::managed_key_resolver_from_store;
use oxideterm_ssh_launch::{RemoteDesktopLaunchProtocol, TemporaryRemoteDesktopLaunch};

const SSH_ROOT_NODE_ID_PREFIX: &str = "ssh";

fn next_available_ssh_root_node_id(
    next_sequence: &mut u64,
    mut node_exists: impl FnMut(&NodeId) -> bool,
) -> NodeId {
    loop {
        let sequence = *next_sequence;
        // Restored session trees can already own numeric ids while the workspace counter
        // starts from one, so allocation must skip every live logical node owner.
        *next_sequence = sequence.wrapping_add(1).max(1);
        let node_id = NodeId::new(format!("{SSH_ROOT_NODE_ID_PREFIX}-{sequence}"));
        if !node_exists(&node_id) {
            return node_id;
        }
    }
}

fn attach_saved_owner_to_reused_ssh_node(
    node: &mut WorkspaceSshNode,
    saved_connection_id: &str,
) -> bool {
    // Saved SSH credentials are node-owned. Attach an owner only when the user
    // reached this node through an explicit saved-connection action.
    let saved_connection_id = saved_connection_id.trim();
    if saved_connection_id.is_empty() || node.saved_connection_id.is_some() {
        return false;
    }
    node.saved_connection_id = Some(saved_connection_id.to_string());
    true
}

fn should_use_dedicated_terminal_connection(
    allow_dedicated_connection: bool,
    saved_policy: Option<bool>,
    manual_node_policy: bool,
    channel_strategy: SshChannelStrategy,
) -> bool {
    // Initial tabs and reconnect remounts must consume the node connection that
    // was just authenticated. Only explicit additional terminals may isolate.
    allow_dedicated_connection
        && (channel_strategy.requires_dedicated_consumers()
            || saved_policy.unwrap_or(manual_node_policy))
}

type SshRouteEndpoint<'a> = (&'a str, u16, &'a str);

fn saved_node_route_matches_endpoints(
    node_router: &NodeRouter,
    node_id: &NodeId,
    proxy_hops: &[SshRouteEndpoint<'_>],
    target: SshRouteEndpoint<'_>,
) -> bool {
    let Ok(path) = node_router.path_to_node(node_id) else {
        return false;
    };
    if path.len() != proxy_hops.len() + 1 {
        return false;
    }

    // A saved-connection index is reusable only while every routed endpoint
    // still matches the current saved profile. Authentication and host-key
    // overrides may change without changing which remote node the path owns.
    path.iter()
        .zip(proxy_hops.iter().copied().chain(std::iter::once(target)))
        .all(
            |(node_id, (expected_host, expected_port, expected_username))| {
                let Some(actual) = node_router.node_metadata(node_id) else {
                    return false;
                };
                actual.host == expected_host
                    && actual.port == expected_port
                    && actual.username == expected_username
            },
        )
}

fn saved_node_route_matches_config(
    node_router: &NodeRouter,
    node_id: &NodeId,
    requested_config: &SshConfig,
) -> bool {
    let proxy_hops = requested_config
        .proxy_chain
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|hop| (hop.host.as_str(), hop.port, hop.username.as_str()))
        .collect::<Vec<_>>();
    saved_node_route_matches_endpoints(
        node_router,
        node_id,
        &proxy_hops,
        (
            requested_config.host.as_str(),
            requested_config.port,
            requested_config.username.as_str(),
        ),
    )
}

fn saved_connection_proxy_route<'a>(
    store: &'a oxideterm_connections::ConnectionStore,
    connection: &'a oxideterm_connections::SavedConnection,
) -> Option<Vec<SshRouteEndpoint<'a>>> {
    if !connection.proxy_chain.is_empty() {
        return Some(
            connection
                .proxy_chain
                .iter()
                .map(|hop| (hop.host.as_str(), hop.port, hop.username.as_str()))
                .collect(),
        );
    }
    let Some(jump_id) = connection.options.jump_host.as_deref() else {
        return Some(Vec::new());
    };
    if jump_id == connection.id {
        return None;
    }
    let jump = store.get(jump_id)?;
    Some(vec![(
        jump.host.as_str(),
        jump.port,
        jump.username.as_str(),
    )])
}

fn saved_node_owner_matches_connection(
    node_router: &NodeRouter,
    node_id: &NodeId,
    saved_connection_id: &str,
) -> bool {
    node_router
        .node_metadata(node_id)
        .is_some_and(|snapshot| snapshot.origin.saved_connection_id() == Some(saved_connection_id))
}

fn indexed_saved_node_matches_connection(
    node_router: &NodeRouter,
    node_id: &NodeId,
    node: &WorkspaceSshNode,
    requested_config: &SshConfig,
    saved_connection_id: &str,
) -> bool {
    saved_node_route_matches_config(node_router, node_id, requested_config)
        && saved_node_owner_matches_connection(node_router, node_id, saved_connection_id)
        && node.saved_connection_id.as_deref() == Some(saved_connection_id)
        && node.endpoint.host == requested_config.host
        && node.endpoint.port == requested_config.port
        && node.endpoint.username == requested_config.username
}

fn reusable_indexed_saved_node_for_config(
    saved_ssh_nodes: &HashMap<String, NodeId>,
    ssh_nodes: &HashMap<NodeId, WorkspaceSshNode>,
    node_router: &NodeRouter,
    saved_connection_id: &str,
    requested_config: &SshConfig,
) -> Option<NodeId> {
    let node_id = saved_ssh_nodes.get(saved_connection_id)?;
    let node = ssh_nodes.get(node_id)?;
    indexed_saved_node_matches_connection(
        node_router,
        node_id,
        node,
        requested_config,
        saved_connection_id,
    )
    .then(|| node_id.clone())
}

fn indexed_saved_node_matches_saved_connection(
    node_router: &NodeRouter,
    store: &oxideterm_connections::ConnectionStore,
    node_id: &NodeId,
    node: &WorkspaceSshNode,
    connection: &oxideterm_connections::SavedConnection,
    saved_connection_id: &str,
) -> bool {
    if connection.id != saved_connection_id {
        return false;
    }
    let Some(proxy_hops) = saved_connection_proxy_route(store, connection) else {
        return false;
    };
    saved_node_route_matches_endpoints(
        node_router,
        node_id,
        &proxy_hops,
        (
            connection.host.as_str(),
            connection.port,
            connection.username.as_str(),
        ),
    ) && saved_node_owner_matches_connection(node_router, node_id, saved_connection_id)
        && node.saved_connection_id.as_deref() == Some(saved_connection_id)
        && node.endpoint.host == connection.host
        && node.endpoint.port == connection.port
        && node.endpoint.username == connection.username
}

fn reusable_direct_root_node_for_saved_config(
    node_router: &NodeRouter,
    config: &SshConfig,
    saved_connection_id: &str,
) -> Option<NodeId> {
    // Physical pooling stays registry-owned; one saved profile must not borrow another profile's
    // logical node and terminal consumers merely because their transport endpoints are equal.
    node_router.flatten_tree().into_iter().find_map(|node| {
        let node_id = NodeId::new(node.id);
        let endpoint_matches = node.depth == 0
            && node.host == config.host
            && node.port == config.port
            && node.username == config.username;
        let owner_matches = node_router.node_metadata(&node_id).is_some_and(|snapshot| {
            snapshot
                .origin
                .saved_connection_id()
                .is_none_or(|owner| owner == saved_connection_id)
        });
        (endpoint_matches && owner_matches).then_some(node_id)
    })
}

impl WorkspaceApp {
    pub(crate) fn open_native_connection_launch(
        &mut self,
        launch: NativeConnectionLaunch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        match launch {
            NativeConnectionLaunch::SavedConnection(launch) => {
                self.open_saved_connection(&launch.saved_connection_id, window, cx);
                Ok(())
            }
            NativeConnectionLaunch::Ssh(launch) => self.open_temporary_ssh_launch(launch, cx),
            NativeConnectionLaunch::Telnet(launch) => {
                self.open_temporary_telnet_launch(launch, window, cx)
            }
            NativeConnectionLaunch::Mosh(launch) => self.open_temporary_mosh_launch(launch, cx),
            NativeConnectionLaunch::RemoteDesktop(launch) => {
                self.open_temporary_remote_desktop_launch(launch, window, cx);
                Ok(())
            }
        }
    }

    fn open_temporary_remote_desktop_launch(
        &mut self,
        launch: TemporaryRemoteDesktopLaunch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let protocol = match launch.protocol {
            RemoteDesktopLaunchProtocol::Rdp => RemoteDesktopProtocol::Rdp,
            RemoteDesktopLaunchProtocol::Vnc => RemoteDesktopProtocol::Vnc,
        };
        let endpoint = RemoteDesktopEndpoint::new(launch.host, launch.port);
        let profile = RemoteDesktopConnectionProfile {
            id: format!("native-remote-desktop-{}", uuid::Uuid::new_v4()),
            label: format!(
                "{}://{}",
                launch.protocol.scheme(),
                endpoint.format_authority()
            ),
            protocol,
            endpoint,
            transport_endpoint: None,
            username: launch.username,
            domain: launch.domain,
            credential_ref: None,
            read_only: false,
            session_options: Default::default(),
        };
        // The remote-desktop tab becomes the sole runtime owner of the URI
        // password; it is never copied into the ephemeral profile or a store.
        let password = launch.password.map(RemoteDesktopSecret::from);
        self.open_remote_desktop_connection_tab(profile, password, window, cx);
    }

    pub(in crate::workspace) fn create_local_terminal_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let terminal_config = self.local_terminal_config();
        let title = self.local_terminal_tab_title();
        self.create_local_terminal_tab_with_config(terminal_config, title, window, cx)
    }

    pub(in crate::workspace) fn create_local_terminal_tab_with_config(
        &mut self,
        terminal_config: LocalPtyConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.create_local_terminal_tab_with_owned_session(terminal_config, title, window, cx)
            .map(|_| ())
    }

    pub(in crate::workspace) fn create_local_terminal_tab_with_owned_session(
        &mut self,
        terminal_config: LocalPtyConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(TerminalSessionId, SharedTerminalSession)> {
        // Shell Launcher selections must use the same tab and pane lifecycle as
        // the default-shell shortcut; only the PTY configuration differs.
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let preference_overrides =
            self.terminal_preference_overrides_for_local_shell(terminal_config.shell.as_ref());
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        preference_overrides.apply_to(&mut preferences);
        let pane = cx.new(|cx| {
            TerminalPane::new_local_with_config_and_preferences(
                terminal_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize terminal pane")
            .with_preference_overrides(preference_overrides)
        });
        let shared_session = pane.read(cx).shared_session();

        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::LocalTerminal,
                title,
                title_source: TabTitleSource::Static,
                root_pane: Some(PaneNode::leaf(pane_id, session_id)),
                active_pane_id: Some(pane_id),
            },
            cx,
        );
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        cx.notify();
        Ok((session_id, shared_session))
    }

    pub(in crate::workspace) fn create_telnet_terminal_tab(
        &mut self,
        config: TelnetSessionConfig,
        terminal_options: ConnectionTerminalOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let title = format!("Telnet {}", config.endpoint_label());
        self.create_telnet_terminal_tab_with_title(config, terminal_options, title, window, cx)
    }

    pub(in crate::workspace) fn create_telnet_terminal_tab_with_title(
        &mut self,
        config: TelnetSessionConfig,
        terminal_options: ConnectionTerminalOptions,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        self.create_telnet_terminal_tab_with_login(
            config,
            None,
            terminal_options,
            title,
            window,
            cx,
        )
    }

    fn create_telnet_terminal_tab_with_login(
        &mut self,
        config: TelnetSessionConfig,
        login: Option<oxideterm_terminal::TelnetLoginCredentials>,
        terminal_options: ConnectionTerminalOptions,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        self.create_telnet_terminal_tab_for_connection(
            config,
            login,
            terminal_options,
            title,
            None,
            window,
            cx,
        )
    }

    pub(in crate::workspace) fn create_telnet_terminal_tab_for_connection(
        &mut self,
        config: TelnetSessionConfig,
        login: Option<oxideterm_terminal::TelnetLoginCredentials>,
        terminal_options: ConnectionTerminalOptions,
        title: String,
        connection_attempt_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let reconnect_terminal_options = terminal_options.clone();
        let reconnect_config = config.clone();
        let connection_attempt_id = connection_attempt_id.unwrap_or_else(|| {
            self.standalone_connections.insert_pending(
                standalone_connections::StandaloneConnectionKind::Telnet,
                title.clone(),
                standalone_connections::StandaloneConnectionLaunch::Telnet {
                    config: reconnect_config,
                    terminal_options: reconnect_terminal_options,
                },
            )
        });
        let mut preference_overrides = terminal_preference_overrides(
            terminal_options,
            &self.settings_store.settings().terminal,
        );
        preference_overrides.session_log_context = Some(TerminalSessionLogContext {
            session: title.clone(),
            host: config.host.clone(),
            username: String::new(),
            protocol: "telnet".to_string(),
        });
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        preference_overrides.apply_to(&mut preferences);
        let pane_config = config;
        let pane = cx.new(|cx| {
            TerminalPane::new_telnet_with_login_preferences(
                pane_config,
                login,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize Telnet terminal pane")
            .with_preference_overrides(preference_overrides)
        });

        // Telnet is a local transport in the plugin API: it owns no SSH node,
        // but it still participates in the normal tab/pane/session registry.
        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::LocalTerminal,
                title: title.clone(),
                title_source: TabTitleSource::Static,
                root_pane: Some(PaneNode::leaf(pane_id, session_id)),
                active_pane_id: Some(pane_id),
            },
            cx,
        );
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        let surface = standalone_connections::StandaloneConnectionSurface::Terminal(session_id);
        self.standalone_connections
            .bind_surface_for_attempt(&connection_attempt_id, surface);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        cx.notify();
        Ok(session_id)
    }

    pub(in crate::workspace) fn create_serial_terminal_tab(
        &mut self,
        config: SerialSessionConfig,
        terminal_options: ConnectionTerminalOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let title = format!("Serial {}", config.port_path);
        self.create_serial_terminal_tab_with_title(config, terminal_options, title, window, cx)
    }

    pub(in crate::workspace) fn create_serial_terminal_tab_with_title(
        &mut self,
        config: SerialSessionConfig,
        terminal_options: ConnectionTerminalOptions,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        self.create_serial_terminal_tab_for_connection(
            config,
            terminal_options,
            title,
            None,
            window,
            cx,
        )
    }

    pub(in crate::workspace) fn create_serial_terminal_tab_for_connection(
        &mut self,
        config: SerialSessionConfig,
        terminal_options: ConnectionTerminalOptions,
        title: String,
        connection_attempt_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let reconnect_config = config.clone();
        let reconnect_terminal_options = terminal_options.clone();
        let connection_attempt_id = connection_attempt_id.unwrap_or_else(|| {
            self.standalone_connections.insert_pending(
                standalone_connections::StandaloneConnectionKind::Serial,
                title.clone(),
                standalone_connections::StandaloneConnectionLaunch::Serial {
                    config: reconnect_config,
                    terminal_options: reconnect_terminal_options,
                },
            )
        });
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let mut preference_overrides = terminal_preference_overrides(
            terminal_options,
            &self.settings_store.settings().terminal,
        );
        preference_overrides.session_log_context = Some(TerminalSessionLogContext {
            session: title.clone(),
            host: config.port_path.clone(),
            username: String::new(),
            protocol: "serial".to_string(),
        });
        preference_overrides.apply_to(&mut preferences);
        let pane_config = config.clone();
        let serial_session = match TerminalPane::open_serial_session_with_preferences(
            config.clone(),
            &preferences,
        ) {
            Ok(session) => session,
            Err(error) => {
                self.standalone_connections
                    .mark_attempt_error(&connection_attempt_id);
                cx.notify();
                return Err(error);
            }
        };
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(serial_session, preferences, window, cx)
                .expect("failed to initialize pre-opened Serial terminal pane")
                .with_serial_session_config(pane_config)
                .with_preference_overrides(preference_overrides)
        });

        // Serial owns no SSH node and must not expose SFTP, forwarding, or ProxyJump.
        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.serial_terminal_configs
            .insert(session_id, config.clone());
        self.refresh_native_plugin_terminal_hooks(cx);
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::LocalTerminal,
                title: title.clone(),
                title_source: TabTitleSource::Static,
                root_pane: Some(PaneNode::leaf(pane_id, session_id)),
                active_pane_id: Some(pane_id),
            },
            cx,
        );
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        let surface = standalone_connections::StandaloneConnectionSurface::Terminal(session_id);
        self.standalone_connections
            .bind_surface_for_attempt(&connection_attempt_id, surface);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        cx.notify();
        Ok(session_id)
    }

    pub(in crate::workspace) fn create_mosh_terminal_tab_for_connection(
        &mut self,
        mut config: MoshTerminalConfig,
        terminal_options: ConnectionTerminalOptions,
        title: String,
        connection_attempt_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        // The bootstrap consumer identifier is local runtime metadata, not a remote session name.
        config.bootstrap.session_id = format!("mosh-{}", session_id.0);
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::MoshTerminal, cx);
        let mut preference_overrides = terminal_preference_overrides(
            terminal_options,
            &self.settings_store.settings().terminal,
        );
        preference_overrides.session_log_context = Some(TerminalSessionLogContext {
            session: title.clone(),
            host: String::new(),
            username: String::new(),
            protocol: "mosh".to_string(),
        });
        preference_overrides.apply_to(&mut preferences);
        let pane = cx.new(|cx| {
            TerminalPane::new_mosh_with_preferences(config, preferences, window, cx)
                .expect("failed to initialize Mosh terminal pane")
                .with_preference_overrides(preference_overrides)
        });

        // Mosh owns one UDP terminal and deliberately has no SSH node capabilities.
        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::MoshTerminal,
                title: title.clone(),
                title_source: TabTitleSource::Static,
                root_pane: Some(PaneNode::leaf(pane_id, session_id)),
                active_pane_id: Some(pane_id),
            },
            cx,
        );
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        let surface = standalone_connections::StandaloneConnectionSurface::Terminal(session_id);
        self.standalone_connections
            .bind_surface_for_attempt(&connection_attempt_id, surface);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        cx.notify();
        Ok(session_id)
    }

    pub(in crate::workspace) fn open_or_create_saved_ssh_terminal_tab(
        &mut self,
        saved_connection_id: String,
        config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let (
            saved_terminal_options,
            saved_dedicated_new_terminal_connection,
            saved_ssh_channel_strategy,
        ) = self
            .connection_store
            .get(&saved_connection_id)
            .map(|connection| {
                (
                    connection.options.terminal.clone(),
                    connection.options.dedicated_new_terminal_connection,
                    connection.options.ssh_channel_strategy,
                )
            })
            .unwrap_or_default();
        let indexed_node_id = self.saved_ssh_nodes.get(&saved_connection_id).cloned();
        if let Some(node_id) = reusable_indexed_saved_node_for_config(
            &self.saved_ssh_nodes,
            &self.ssh_nodes,
            &self.node_router,
            &saved_connection_id,
            &config,
        ) {
            self.associate_existing_node_with_saved_connection(&node_id, &saved_connection_id);
            if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                node.terminal_options = saved_terminal_options.clone();
                node.dedicated_new_terminal_connection = saved_dedicated_new_terminal_connection;
                node.ssh_channel_strategy = saved_ssh_channel_strategy;
            }
            if let Some(session_id) = self
                .workspace_runtime
                .read(cx)
                .ssh_terminal_session_ids_for_node(&node_id)
                .first()
                .copied()
                && self.focus_terminal_session(session_id, window, cx)
            {
                let _ = self.connection_store.mark_used(&saved_connection_id);
                return Ok(());
            }
            if self.ssh_nodes.contains_key(&node_id) {
                let node_config = self
                    .config_with_host_key_acceptance_for_node(&node_id, &config)
                    .or_else(|| {
                        self.node_router
                            .node_runtime_snapshot(&node_id)
                            .map(|snapshot| snapshot.config)
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!("SSH node {} has no runtime config", node_id.0)
                    })?;
                // Tauri passes the saved connection's current post-connect command
                // to createTerminalForNode even when the live node already exists.
                let post_connect_command = config.post_connect_command.clone();
                self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    node_id,
                    post_connect_command,
                    node_config,
                    title,
                    Some(saved_connection_id.clone()),
                    Some(saved_connection_id.clone()),
                    None,
                    window,
                    cx,
                )?;
                return Ok(());
            }
        }
        if indexed_node_id.is_some() {
            // Keep an already-running historical node alive for its existing
            // consumers, but never let a stale saved-profile index route a new tab.
            self.saved_ssh_nodes.remove(&saved_connection_id);
        }

        if config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            // Tauri does not represent a saved proxy chain as one target node
            // with an embedded proxy_chain. It expands each hop into the
            // SessionTree and then connects the target through its ancestors.
            let expansion =
                self.expand_saved_connection_tree(&saved_connection_id, config, title.clone())?;
            let target_config = self
                .node_router
                .node_runtime_snapshot(&expansion.target_node_id)
                .map(|snapshot| snapshot.config)
                .ok_or_else(|| anyhow::anyhow!("target node was not materialized"))?;
            let target_node_id = expansion.target_node_id;
            if let Some(node) = self.ssh_nodes.get_mut(&target_node_id) {
                node.terminal_options = saved_terminal_options;
                node.dedicated_new_terminal_connection = saved_dedicated_new_terminal_connection;
                node.ssh_channel_strategy = saved_ssh_channel_strategy;
            }
            let post_connect_command = target_config.post_connect_command.clone();
            self.queue_ssh_terminal_tab_for_node_with_mark_used(
                target_node_id,
                post_connect_command,
                target_config,
                title,
                Some(saved_connection_id.clone()),
                Some(saved_connection_id.clone()),
                None,
                window,
                cx,
            )?;
            return Ok(());
        } else {
            if let Some(existing_node_id) = reusable_direct_root_node_for_saved_config(
                &self.node_router,
                &config,
                &saved_connection_id,
            ) {
                self.ensure_workspace_ssh_node_from_runtime(&existing_node_id);
                self.associate_existing_node_with_saved_connection(
                    &existing_node_id,
                    &saved_connection_id,
                );
                if let Some(node) = self.ssh_nodes.get_mut(&existing_node_id) {
                    node.terminal_options = saved_terminal_options.clone();
                    node.dedicated_new_terminal_connection =
                        saved_dedicated_new_terminal_connection;
                    node.ssh_channel_strategy = saved_ssh_channel_strategy;
                }
                if let Some(session_id) = self
                    .workspace_runtime
                    .read(cx)
                    .ssh_terminal_session_ids_for_node(&existing_node_id)
                    .first()
                    .copied()
                    && self.focus_terminal_session(session_id, window, cx)
                {
                    let _ = self.connection_store.mark_used(&saved_connection_id);
                    return Ok(());
                }
                if let Some((node_title, node_saved_connection_id)) = self
                    .ssh_nodes
                    .get(&existing_node_id)
                    .map(|node| (node.title.clone(), node.saved_connection_id.clone()))
                {
                    // This branch reuses a root node only because the user
                    // explicitly opened this saved connection. The association
                    // above gives every terminal on that reused node one owner
                    // without letting sudo helper code infer one from host text.
                    let node_config = self
                        .config_with_host_key_acceptance_for_node(&existing_node_id, &config)
                        .or_else(|| {
                            self.node_router
                                .node_runtime_snapshot(&existing_node_id)
                                .map(|snapshot| snapshot.config)
                        })
                        .ok_or_else(|| {
                            anyhow::anyhow!("SSH node {} has no runtime config", existing_node_id.0)
                        })?;
                    // Tauri reuses the existing direct root node but still
                    // applies the saved connection's current terminal command.
                    let post_connect_command = config.post_connect_command.clone();
                    self.queue_ssh_terminal_tab_for_node_with_mark_used(
                        existing_node_id,
                        post_connect_command,
                        node_config,
                        node_title,
                        node_saved_connection_id,
                        Some(saved_connection_id.clone()),
                        None,
                        window,
                        cx,
                    )?;
                    return Ok(());
                }
            }
            let node_id = self.materialize_ssh_root_node(
                config.clone(),
                title.clone(),
                Some(saved_connection_id.clone()),
            );
            if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                node.terminal_options = saved_terminal_options.clone();
                node.dedicated_new_terminal_connection = saved_dedicated_new_terminal_connection;
                node.ssh_channel_strategy = saved_ssh_channel_strategy;
            }
            let cleanup_node_id = node_id.clone();
            let post_connect_command = config.post_connect_command.clone();
            let result = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                node_id,
                post_connect_command,
                config,
                title,
                Some(saved_connection_id.clone()),
                Some(saved_connection_id.clone()),
                None,
                window,
                cx,
            );
            if result.is_ok() {
                self.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.mark_pending_ssh_terminal_open_cleanup(
                        &cleanup_node_id,
                        cleanup_node_id.clone(),
                    );
                });
            }
            result
        }
    }

    fn associate_existing_node_with_saved_connection(
        &mut self,
        node_id: &NodeId,
        saved_connection_id: &str,
    ) {
        let saved_connection_id = saved_connection_id.trim();
        if saved_connection_id.is_empty() {
            return;
        }
        let attached = self
            .ssh_nodes
            .get_mut(node_id)
            .is_some_and(|node| attach_saved_owner_to_reused_ssh_node(node, saved_connection_id));
        let owned_by_saved_connection = self
            .ssh_nodes
            .get(node_id)
            .is_some_and(|node| node.saved_connection_id.as_deref() == Some(saved_connection_id));
        if !attached && !owned_by_saved_connection {
            return;
        }

        let saved_connection_id = saved_connection_id.trim().to_string();
        self.saved_ssh_nodes
            .insert(saved_connection_id.clone(), node_id.clone());
        let runtime_origin_needs_owner =
            self.node_router
                .node_metadata(node_id)
                .is_some_and(|snapshot| {
                    snapshot.origin.saved_connection_id() != Some(saved_connection_id.as_str())
                });
        if attached || runtime_origin_needs_owner {
            // This is an explicit saved-connection open that reaches an
            // existing node. Promote runtime origin so persisted node ownership
            // and SSH privilege scope agree after restart.
            let _ = self.node_router.update_node_origin(
                node_id,
                NodeOrigin::Restored {
                    saved_connection_id,
                },
            );
        }
        if attached || runtime_origin_needs_owner {
            self.persist_session_tree_snapshot();
        }
    }

    fn config_with_host_key_acceptance_for_node(
        &mut self,
        node_id: &NodeId,
        accepted_config: &SshConfig,
    ) -> Option<SshConfig> {
        let trust_host_key = accepted_config.trust_host_key?;
        let expected_host_key_fingerprint =
            accepted_config.expected_host_key_fingerprint.clone()?;
        let runtime_snapshot = self.node_router.node_runtime_snapshot(node_id)?;
        let mut config = runtime_snapshot.config;
        // Tauri passes accepted host-key data as connectNode step options. A
        // reused native node connects from its runtime-owned config, so update
        // that owner before starting the worker.
        config.strict_host_key_checking = true;
        config.trust_host_key = Some(trust_host_key);
        config.expected_host_key_fingerprint = Some(expected_host_key_fingerprint);
        let action_config = config.clone();
        self.node_router
            .upsert_node_with_origin(node_id.clone(), config, runtime_snapshot.origin);
        Some(action_config)
    }

    pub(in crate::workspace) fn try_reuse_active_saved_connection_terminal(
        &mut self,
        saved_connection_id: &str,
        connection: &oxideterm_connections::SavedConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session_id) = self.ssh_nodes.iter().find_map(|(node_id, node)| {
            // Missing credentials may focus only the terminal already owned by
            // this exact saved route; endpoint equality cannot establish ownership.
            let matching_saved_node = indexed_saved_node_matches_saved_connection(
                &self.node_router,
                &self.connection_store,
                node_id,
                node,
                connection,
                saved_connection_id,
            );
            // `then_some` evaluates its argument eagerly. Use `first()` so a
            // ready node without attached terminals can be skipped instead of
            // indexing an empty terminal list during startup/event handling.
            let runtime_ready = self
                .node_router
                .node_state(node_id)
                .is_ok_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready);
            let session_id = self
                .workspace_runtime
                .read(cx)
                .ssh_terminal_session_ids_for_node(node_id)
                .first()
                .copied();
            (matching_saved_node && runtime_ready)
                .then_some(session_id)
                .flatten()
        }) else {
            return false;
        };

        if !self.focus_terminal_session(session_id, window, cx) {
            return false;
        }
        let _ = self.connection_store.mark_used(saved_connection_id);
        true
    }

    pub(in crate::workspace) fn indexed_saved_ssh_node_for_connection(
        &self,
        saved_connection_id: &str,
        connection: &oxideterm_connections::SavedConnection,
    ) -> Option<NodeId> {
        let node_id = self.saved_ssh_nodes.get(saved_connection_id)?;
        let node = self.ssh_nodes.get(node_id)?;
        indexed_saved_node_matches_saved_connection(
            &self.node_router,
            &self.connection_store,
            node_id,
            node,
            connection,
            saved_connection_id,
        )
        .then(|| node_id.clone())
    }

    pub(in crate::workspace) fn materialize_ssh_root_node(
        &mut self,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
    ) -> NodeId {
        let indexed_node_id = saved_connection_id
            .as_ref()
            .and_then(|saved_connection_id| self.saved_ssh_nodes.get(saved_connection_id))
            .cloned();
        if let Some(saved_connection_id) = saved_connection_id.as_ref()
            && let Some(node_id) = reusable_indexed_saved_node_for_config(
                &self.saved_ssh_nodes,
                &self.ssh_nodes,
                &self.node_router,
                saved_connection_id,
                &config,
            )
        {
            self.associate_existing_node_with_saved_connection(&node_id, saved_connection_id);
            return node_id;
        }
        if indexed_node_id.is_some()
            && let Some(saved_connection_id) = saved_connection_id.as_ref()
        {
            // Every direct caller, including gateway and public API paths, must
            // invalidate stale routing before materializing a replacement node.
            self.saved_ssh_nodes.remove(saved_connection_id);
        }

        let node_id = next_available_ssh_root_node_id(&mut self.next_ssh_node_id, |node_id| {
            self.node_router.contains_node(node_id) || self.ssh_nodes.contains_key(node_id)
        });
        let origin = saved_connection_id
            .as_ref()
            .map(|id| NodeOrigin::Restored {
                saved_connection_id: id.clone(),
            })
            .unwrap_or(NodeOrigin::Direct);
        let ui_node = WorkspaceSshNode::new(
            saved_connection_id.clone(),
            &config,
            title,
            Vec::new(),
            NodeReadiness::Disconnected,
        );
        self.node_router
            .upsert_node_with_origin(node_id.clone(), config, origin);
        self.ssh_nodes.insert(node_id.clone(), ui_node);
        if let Some(saved_connection_id) = saved_connection_id {
            self.saved_ssh_nodes
                .insert(saved_connection_id, node_id.clone());
        }
        self.persist_session_tree_snapshot();
        node_id
    }

    pub(crate) fn open_temporary_ssh_launch(
        &mut self,
        launch: TemporarySshLaunch,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let title = launch.title();
        let auth = match launch.password {
            Some(password) => AuthMethod::password_secret(password),
            None => AuthMethod::Agent,
        };
        let config = SshConfig {
            host: launch.host,
            port: launch.port,
            username: launch.username,
            auth,
            strict_host_key_checking: true,
            ..SshConfig::default()
        };
        let node_id = self.materialize_ssh_root_node(config, title.clone(), None);
        let queue_outcome = self.workspace_runtime.update(cx, |runtime, runtime_cx| {
            runtime.queue_ssh_terminal_open(
                runtime_entity::PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command: None,
                    mark_used_connection_id: None,
                    save_after_open: None,
                    cleanup_node_id: Some(node_id.clone()),
                    title,
                },
                runtime_cx,
            )
        });
        if queue_outcome == runtime_entity::QueueSshTerminalOpenOutcome::WorkspaceShuttingDown {
            return Err(anyhow::anyhow!("workspace runtime is shutting down"));
        }
        // The temporary launch now shares the same node-owned transport attempt
        // and reliable completion delivery as every other first terminal.
        self.ensure_node_connection_started(&node_id, cx);
        cx.notify();
        Ok(())
    }

    fn open_temporary_telnet_launch(
        &mut self,
        launch: TemporaryTelnetLaunch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let title = launch.title();
        let config = TelnetSessionConfig {
            host: launch.host,
            port: launch.port,
        };
        let login = launch.username.map(|username| {
            // URI user information is consumed by the Telnet worker and never enters the store.
            oxideterm_terminal::TelnetLoginCredentials {
                username,
                password: launch.password,
            }
        });
        self.create_telnet_terminal_tab_with_login(
            config,
            login,
            ConnectionTerminalOptions::default(),
            title,
            window,
            cx,
        )?;
        Ok(())
    }

    fn open_temporary_mosh_launch(
        &mut self,
        launch: TemporaryMoshLaunch,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let title = launch.title();
        let auth = match launch.password {
            Some(password) => AuthMethod::password_secret(password),
            None => AuthMethod::Agent,
        };
        let config = SshConfig {
            host: launch.host,
            port: launch.ssh_port,
            username: launch.username,
            auth,
            strict_host_key_checking: true,
            ..SshConfig::default()
        };
        self.start_ssh_preflight(
            config,
            title,
            SshConnectionIntent::Mosh(MoshConnectionOptions {
                saved_profile_id: None,
                server_executable: oxideterm_mosh::DEFAULT_MOSH_SERVER_EXECUTABLE.to_string(),
                udp_host_override: None,
                udp_port: SavedMoshUdpPortSelection::Automatic,
                ip_family: SavedMoshIpFamily::Auto,
                prediction: MoshPredictionMode::Adaptive,
                locale: None,
                terminal: ConnectionTerminalOptions::default(),
                public_mcp_open_token: None,
                runtime_connection_attempt_id: None,
            }),
            cx,
        );
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn expand_saved_connection_tree(
        &mut self,
        saved_connection_id: &str,
        mut config: SshConfig,
        target_title: String,
    ) -> Result<NodeTreeExpansion> {
        let proxy_chain = config.proxy_chain.take().unwrap_or_default();
        let connect_timeout_seconds = config.timeout_secs;
        // Consume the detached chain so runtime authentication material is not cloned.
        let hops = proxy_chain
            .into_iter()
            .map(|hop| ssh_config_from_proxy_hop(hop, connect_timeout_seconds))
            .collect::<Vec<_>>();
        let expansion = self
            .node_router
            .expand_manual_preset(saved_connection_id, hops, config)?;
        self.register_expanded_tree_nodes(saved_connection_id, &expansion, target_title, true);
        self.persist_session_tree_snapshot();
        Ok(expansion)
    }

    pub(in crate::workspace) fn expand_saved_connection_tree_under_parent(
        &mut self,
        parent_node_id: NodeId,
        saved_connection_id: &str,
        mut config: SshConfig,
        target_title: String,
    ) -> Result<NodeTreeExpansion> {
        let proxy_chain = config.proxy_chain.take().unwrap_or_default();
        let connect_timeout_seconds = config.timeout_secs;
        // Consume the detached chain so runtime authentication material is not cloned.
        let hops = proxy_chain
            .into_iter()
            .map(|hop| ssh_config_from_proxy_hop(hop, connect_timeout_seconds))
            .collect::<Vec<_>>();
        let expansion = self.node_router.expand_manual_preset_under_parent(
            parent_node_id.clone(),
            saved_connection_id,
            hops,
            config,
        )?;
        self.register_expanded_tree_nodes(saved_connection_id, &expansion, target_title, false);
        self.expanded_ssh_nodes.insert(parent_node_id);
        for node_id in &expansion.path_node_ids {
            self.expanded_ssh_nodes.insert(node_id.clone());
        }
        // A saved next hop is scoped to its current parent node. Keep the
        // per-node saved id, but do not replace the root saved-connection
        // reuse index with a context-specific child path.
        self.persist_session_tree_snapshot();
        Ok(expansion)
    }

    fn register_expanded_tree_nodes(
        &mut self,
        saved_connection_id: &str,
        expansion: &NodeTreeExpansion,
        target_title: String,
        update_saved_node_index: bool,
    ) {
        for node_id in &expansion.path_node_ids {
            let Some(snapshot) = self.node_router.node_metadata(node_id) else {
                continue;
            };
            let title = if node_id == &expansion.target_node_id {
                target_title.clone()
            } else {
                format!("{}@{}", snapshot.username, snapshot.host)
            };
            self.ssh_nodes.insert(
                node_id.clone(),
                WorkspaceSshNode {
                    saved_connection_id: snapshot.origin.saved_connection_id().map(str::to_string),
                    endpoint: WorkspaceSshNodeEndpoint {
                        host: snapshot.host,
                        port: snapshot.port,
                        username: snapshot.username,
                    },
                    title,
                    terminal_options: ConnectionTerminalOptions::default(),
                    dedicated_new_terminal_connection: false,
                    ssh_channel_strategy: SshChannelStrategy::default(),
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Disconnected,
                },
            );
        }
        if update_saved_node_index {
            self.saved_ssh_nodes.insert(
                saved_connection_id.to_string(),
                expansion.target_node_id.clone(),
            );
        }
    }

    pub(in crate::workspace) fn create_ssh_terminal_pane_for_existing_node(
        &mut self,
        node_id: &NodeId,
        post_connect_command: Option<String>,
        allow_dedicated_connection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(PaneId, TerminalSessionId)> {
        let (
            host,
            port,
            username,
            saved_connection_id,
            node_dedicated_new_terminal_connection,
            node_ssh_channel_strategy,
        ) = self
            .ssh_nodes
            .get(node_id)
            .map(|node| {
                (
                    node.endpoint.host.clone(),
                    node.endpoint.port,
                    node.endpoint.username.clone(),
                    node.saved_connection_id.clone(),
                    node.dedicated_new_terminal_connection,
                    node.ssh_channel_strategy,
                )
            })
            .ok_or_else(|| anyhow::anyhow!("SSH node {} not found", node_id.0))?;
        let saved_connection_policy = saved_connection_id
            .as_deref()
            .and_then(|id| self.connection_store.get(id))
            .map(|connection| {
                (
                    connection.options.dedicated_new_terminal_connection,
                    connection.options.ssh_channel_strategy,
                )
            });
        let dedicated_new_terminal_connection = should_use_dedicated_terminal_connection(
            allow_dedicated_connection,
            saved_connection_policy.map(|policy| policy.0),
            node_dedicated_new_terminal_connection,
            saved_connection_policy
                .map(|policy| policy.1)
                .unwrap_or(node_ssh_channel_strategy),
        );
        let connection_id = self
            .node_router
            .connection_id_for_node(node_id)
            .ok_or_else(|| anyhow::anyhow!("SSH node {} is not connected", node_id.0))?;
        if !self.node_router.contains_node(node_id) {
            return Err(anyhow::anyhow!(
                "SSH node {} has no runtime owner",
                node_id.0
            ));
        }
        let node_x11_forwarding = self
            .node_router
            .node_x11_forwarding(node_id)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;

        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let preference_overrides = self.terminal_preference_overrides_for_ssh_node(node_id);
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::SshTerminal, cx);
        // Node-scoped history survives terminal pane replacement but never crosses SSH nodes.
        preferences.command_history = self
            .ssh_terminal_command_histories
            .entry(node_id.clone())
            .or_default()
            .clone();
        preference_overrides.apply_to(&mut preferences);
        let consumer = ConnectionConsumer::Terminal(session_id.0.to_string());
        let session_config = if dedicated_new_terminal_connection {
            // This zeroizing, secret-bearing snapshot moves directly into the
            // terminal task and is never copied into WorkspaceSshNode UI state.
            let runtime_snapshot = self
                .node_router
                .node_runtime_snapshot(node_id)
                .ok_or_else(|| anyhow::anyhow!("SSH node {} has no runtime config", node_id.0))?;
            let parent_connection_id = runtime_snapshot
                .parent_id
                .map(|parent_id| {
                    self.node_router
                        .connection_id_for_node(&parent_id)
                        .ok_or_else(|| {
                            anyhow::anyhow!("SSH parent node {} is not connected", parent_id.0)
                        })
                })
                .transpose()?;
            let prompt_handler = self.workspace_runtime.read(cx).native_ssh_prompt_handler();
            SshSessionConfig::for_dedicated_connection(
                runtime_snapshot.config,
                parent_connection_id,
            )
            .with_prompt_handler(prompt_handler)
            .with_managed_key_resolver(managed_key_resolver_from_store(&self.connection_store))
            .with_registry(self.ssh_registry.clone(), consumer)
        } else {
            // The default path adds a channel to the node-owned transport and
            // never clones its authentication configuration.
            SshSessionConfig::for_existing_connection(connection_id, host, port, username)
                .with_x11_forwarding_override(node_x11_forwarding)
                .with_registry(self.ssh_registry.clone(), consumer)
        }
        // Opening another terminal never replays a post-connect command unless
        // the caller explicitly supplies one.
        .with_post_connect_command(post_connect_command)
        // Both policies keep remounted tabs on the deferred PTY boundary
        // so authentication cannot briefly start at a fallback size.
        .with_deferred_pty(true)
        .with_runtime_handle(self.forwarding_runtime.handle().clone())
        .with_trzsz_policy(preferences.trzsz_policy.clone());
        self.register_existing_ssh_terminal_session(node_id, session_id, cx)?;
        let shared_session = TerminalPane::ssh_shared_session(session_config, &preferences);
        self.register_terminal_endpoint_session(node_id, session_id, shared_session.clone(), cx);
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(shared_session, preferences, window, cx)
                .expect("failed to remount ssh terminal pane")
                .with_preference_overrides(preference_overrides)
        });
        self.register_terminal_pane(pane_id, session_id, pane, window, cx);
        if let Some(saved_connection_id) = saved_connection_id {
            self.register_terminal_saved_connection(
                session_id,
                oxideterm_terminal_triggers::SavedConnectionKind::Ssh,
                saved_connection_id,
                cx,
            );
        }
        self.refresh_native_plugin_terminal_hooks(cx);
        self.persist_session_tree_snapshot();
        Ok((pane_id, session_id))
    }

    pub(in crate::workspace) fn create_ssh_terminal_tab_for_existing_node(
        &mut self,
        node_id: &NodeId,
        post_connect_command: Option<String>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        self.create_ssh_terminal_tab_for_existing_node_with_policy(
            node_id,
            post_connect_command,
            title,
            true,
            window,
            cx,
        )
    }

    fn create_initial_ssh_terminal_tab_for_existing_node(
        &mut self,
        node_id: &NodeId,
        post_connect_command: Option<String>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        self.create_ssh_terminal_tab_for_existing_node_with_policy(
            node_id,
            post_connect_command,
            title,
            false,
            window,
            cx,
        )
    }

    fn create_ssh_terminal_tab_for_existing_node_with_policy(
        &mut self,
        node_id: &NodeId,
        post_connect_command: Option<String>,
        title: String,
        allow_dedicated_connection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id(cx);
        let (pane_id, session_id) = self.create_ssh_terminal_pane_for_existing_node(
            node_id,
            post_connect_command,
            allow_dedicated_connection,
            window,
            cx,
        )?;
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::SshTerminal,
                title,
                title_source: TabTitleSource::Static,
                root_pane: Some(PaneNode::leaf(pane_id, session_id)),
                active_pane_id: Some(pane_id),
            },
            cx,
        );
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        if self.sidebar_collapsed {
            self.set_sidebar_collapsed_with_motion(false, cx);
        }
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window, cx);
        self.persist_session_tree_snapshot();
        // The node remains the route owner even when the new tab has a
        // dedicated physical connection.
        self.start_remote_shell_integration_terminal_gate(node_id.clone(), false, cx);
        cx.notify();
        Ok(session_id)
    }

    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_existing_node(
        &mut self,
        node_id: NodeId,
        post_connect_command: Option<String>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if !self.node_is_ready_for_terminal(&node_id) {
            return Err(anyhow::anyhow!("SSH node {} is not ready", node_id.0));
        }
        self.create_ssh_terminal_tab_for_existing_node(
            &node_id,
            post_connect_command,
            title,
            window,
            cx,
        )?;
        Ok(())
    }

    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_node(
        &mut self,
        node_id: NodeId,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            None,
            config,
            title,
            saved_connection_id,
            None,
            None,
            window,
            cx,
        )
    }

    fn save_connection_after_terminal_open(
        &mut self,
        request: SaveConnectionRequest,
        cx: &mut Context<Self>,
    ) {
        // Tauri opens the terminal first and treats save failures as a toast,
        // not as a failed SSH connection attempt.
        if let Err(error) = self.connection_store.upsert(request) {
            self.push_command_palette_toast(
                self.i18n.t("modals.new_connection.save_failed"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            cx.notify();
            return;
        }
        self.queue_cloud_sync_dirty_refresh(cx);
    }

    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_node_with_mark_used(
        &mut self,
        node_id: NodeId,
        post_connect_command: Option<String>,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
        mark_used_connection_id: Option<String>,
        mut save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.ssh_nodes.entry(node_id.clone()).or_insert_with(|| {
            WorkspaceSshNode::new(
                saved_connection_id.clone(),
                &config,
                title.clone(),
                Vec::new(),
                NodeReadiness::Disconnected,
            )
        });
        if let Some(saved_connection_id) = saved_connection_id.as_deref() {
            self.associate_existing_node_with_saved_connection(&node_id, saved_connection_id);
        }
        if self.node_is_ready_for_terminal(&node_id) {
            self.create_initial_ssh_terminal_tab_for_existing_node(
                &node_id,
                post_connect_command,
                title,
                window,
                cx,
            )?;
            if let Some(request) = save_after_open {
                self.save_connection_after_terminal_open(request, cx);
            }
            if let Some(connection_id) = mark_used_connection_id.as_deref() {
                let _ = self.connection_store.mark_used(connection_id);
            }
            return Ok(());
        }
        let target_has_parent = self
            .node_router
            .node_metadata(&node_id)
            .and_then(|snapshot| snapshot.parent_id)
            .is_some();
        if target_has_parent && self.node_router.connection_id_for_node(&node_id).is_none() {
            let intent = mark_used_connection_id
                .clone()
                .or_else(|| saved_connection_id.clone())
                .map(SshConnectionIntent::ConnectSaved)
                .unwrap_or_else(|| {
                    let terminal_options = self
                        .ssh_nodes
                        .get(&node_id)
                        .map(|node| node.terminal_options.clone())
                        .unwrap_or_default();
                    SshConnectionIntent::Connect(SshTerminalConnectionOptions {
                        terminal: terminal_options,
                        dedicated_new_terminal_connection: self
                            .ssh_nodes
                            .get(&node_id)
                            .is_some_and(|node| node.dedicated_new_terminal_connection),
                        ssh_channel_strategy: self
                            .ssh_nodes
                            .get(&node_id)
                            .map(|node| node.ssh_channel_strategy)
                            .unwrap_or_default(),
                    })
                });
            if self.start_existing_session_tree_connect(
                node_id.clone(),
                title.clone(),
                intent,
                &mut save_after_open,
                window,
                cx,
            ) {
                cx.notify();
                return Ok(());
            }
        }
        let queue_outcome = self.workspace_runtime.update(cx, |runtime, runtime_cx| {
            runtime.queue_ssh_terminal_open(
                runtime_entity::PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command,
                    mark_used_connection_id,
                    save_after_open,
                    cleanup_node_id: None,
                    title,
                },
                runtime_cx,
            )
        });
        if queue_outcome == runtime_entity::QueueSshTerminalOpenOutcome::WorkspaceShuttingDown {
            return Err(anyhow::anyhow!("workspace runtime is shutting down"));
        }
        if queue_outcome == runtime_entity::QueueSshTerminalOpenOutcome::Ready {
            cx.notify();
            return Ok(());
        }
        self.ensure_node_connection_started(&node_id, cx);
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn open_ready_ssh_terminal_requests(
        &mut self,
        requests: Vec<runtime_entity::PendingSshTerminalOpen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut opened = false;
        for request in requests {
            if self
                .create_initial_ssh_terminal_tab_for_existing_node(
                    &request.node_id,
                    request.post_connect_command,
                    request.title,
                    window,
                    cx,
                )
                .is_ok()
            {
                let mark_used_connection_id = request.mark_used_connection_id.clone();
                if let Some(save_request) = request.save_after_open {
                    self.save_connection_after_terminal_open(save_request, cx);
                }
                if let Some(connection_id) = mark_used_connection_id.as_deref() {
                    let _ = self.connection_store.mark_used(connection_id);
                }
                opened = true;
            }
        }
        opened
    }

    pub(in crate::workspace) fn node_is_ready_for_terminal(&self, node_id: &NodeId) -> bool {
        self.node_router
            .node_state(node_id)
            .is_ok_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready)
    }

    fn register_terminal_endpoint_session(
        &mut self,
        node_id: &NodeId,
        session_id: TerminalSessionId,
        session: SharedTerminalSession,
        cx: &mut Context<Self>,
    ) {
        let endpoint = TerminalEndpoint {
            // Native GPUI does not need a loopback WebSocket, but the owner
            // boundary mirrors Tauri: NodeRouter exposes a stable terminal
            // endpoint and GPUI panes consume the session by id instead of
            // being the authoritative terminal owner.
            ws_port: 0,
            ws_token: zeroize::Zeroizing::new(format!("native-terminal-{}", session_id.0)),
            session_id: session_id.0.to_string(),
        };
        let retained = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.retain_terminal_endpoint_session(session_id, session)
        });
        debug_assert!(
            retained,
            "registered SSH terminal must retain its endpoint session"
        );
        // Register every endpoint. NodeRouter keeps the first endpoint primary
        // and can elect another live endpoint when that terminal closes.
        self.workspace_runtime
            .read(cx)
            .bind_ssh_terminal_endpoint(node_id, endpoint);
        self.persist_session_tree_snapshot();
    }

    pub(in crate::workspace) fn open_settings_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::Settings)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Settings,
                    title: self.i18n.t("settings_view.title"),
                    title_source: TabTitleSource::I18nKey("settings_view.title"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Settings;
        self.needs_active_pane_focus = false;
        // Opening Settings must preserve the user's current primary-sidebar state.
        self.reveal_active_tab(window, cx);
        if self.settings_workspace.read(cx).route_snapshot().active_tab == SettingsTab::General {
            self.refresh_cli_companion_status(cx);
            #[cfg(not(target_os = "macos"))]
            self.refresh_launch_at_login_status(cx);
        }
        cx.notify();
    }
}

fn ssh_config_from_proxy_hop(hop: ProxyHopConfig, connect_timeout_seconds: u64) -> SshConfig {
    let ProxyHopConfig {
        host,
        port,
        username,
        auth,
        agent_forwarding,
        identity_agent,
        agent_forwarding_socket,
        legacy_ssh_compatibility,
        ssh_algorithms,
        strict_host_key_checking,
        trust_host_key,
        expected_host_key_fingerprint,
    } = hop;
    SshConfig {
        host,
        port,
        username,
        auth,
        timeout_secs: connect_timeout_seconds,
        proxy_chain: None,
        agent_forwarding,
        identity_agent,
        agent_forwarding_socket,
        legacy_ssh_compatibility,
        ssh_algorithms,
        strict_host_key_checking,
        trust_host_key,
        expected_host_key_fingerprint,
        ..SshConfig::default()
    }
}

#[cfg(test)]
mod create_tests {
    use super::*;

    #[test]
    fn ssh_root_node_allocator_skips_restored_node_ids() {
        let occupied_node_ids = HashSet::from([
            NodeId::new("ssh-1"),
            NodeId::new("ssh-2"),
            NodeId::new("direct-restored"),
        ]);
        let mut next_sequence = 1;

        let node_id = next_available_ssh_root_node_id(&mut next_sequence, |node_id| {
            occupied_node_ids.contains(node_id)
        });

        assert_eq!(node_id, NodeId::new("ssh-3"));
        assert_eq!(next_sequence, 4);
    }

    fn saved_connection_metadata(id: &str) -> oxideterm_connections::SavedConnection {
        // This fixture deliberately has no runtime credential so reuse can be
        // verified against persisted route identity alone.
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": "Saved connection",
            "host": "shared.example.com",
            "port": 22,
            "username": "ops",
            "auth": { "type": "password" },
            "created_at": "2026-01-01T00:00:00Z"
        }))
        .expect("valid saved connection fixture")
    }

    #[test]
    fn proxy_hop_conversion_moves_auth_and_preserves_transport_options() {
        let connect_timeout_seconds = 180;
        let config = ssh_config_from_proxy_hop(
            ProxyHopConfig {
                host: "jump.example.com".to_string(),
                port: 2202,
                username: "operator".to_string(),
                auth: AuthMethod::password("runtime-secret"),
                agent_forwarding: true,
                identity_agent: Some("/tmp/identity-agent.sock".to_string()),
                agent_forwarding_socket: Some("/tmp/forward-agent.sock".to_string()),
                legacy_ssh_compatibility: true,
                ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
                strict_host_key_checking: true,
                trust_host_key: Some(false),
                expected_host_key_fingerprint: Some("SHA256:test".to_string()),
            },
            connect_timeout_seconds,
        );

        assert_eq!(config.host, "jump.example.com");
        assert_eq!(config.port, 2202);
        assert_eq!(config.username, "operator");
        assert_eq!(config.timeout_secs, connect_timeout_seconds);
        assert!(config.agent_forwarding);
        assert_eq!(
            config.identity_agent.as_deref(),
            Some("/tmp/identity-agent.sock")
        );
        assert_eq!(
            config.agent_forwarding_socket.as_deref(),
            Some("/tmp/forward-agent.sock")
        );
        assert!(config.legacy_ssh_compatibility);
        assert!(config.strict_host_key_checking);
        assert_eq!(config.trust_host_key, Some(false));
        assert_eq!(
            config.expected_host_key_fingerprint.as_deref(),
            Some("SHA256:test")
        );
        match config.auth {
            AuthMethod::Password { password } => {
                assert_eq!(password.as_str(), "runtime-secret");
            }
            _ => panic!("proxy hop password authentication was not preserved"),
        }
    }

    #[test]
    fn dedicated_policy_applies_only_to_explicit_additional_terminals() {
        assert!(!should_use_dedicated_terminal_connection(
            false,
            Some(true),
            true,
            SshChannelStrategy::DedicatedPerConsumer,
        ));
        assert!(should_use_dedicated_terminal_connection(
            true,
            Some(true),
            false,
            SshChannelStrategy::Multiplexed,
        ));
        assert!(should_use_dedicated_terminal_connection(
            true,
            None,
            true,
            SshChannelStrategy::Multiplexed,
        ));
        assert!(!should_use_dedicated_terminal_connection(
            true,
            Some(false),
            true,
            SshChannelStrategy::Multiplexed,
        ));
        assert!(should_use_dedicated_terminal_connection(
            true,
            Some(false),
            false,
            SshChannelStrategy::DedicatedPerConsumer,
        ));
    }

    #[test]
    fn reused_ssh_node_without_owner_accepts_explicit_saved_owner() {
        let mut node = WorkspaceSshNode::new(
            None,
            &SshConfig::default(),
            "lipsc@100.118.61.75".to_string(),
            vec![TerminalSessionId(1)],
            NodeReadiness::Ready,
        );

        assert!(attach_saved_owner_to_reused_ssh_node(&mut node, "home-100"));
        assert_eq!(node.saved_connection_id.as_deref(), Some("home-100"));
    }

    #[test]
    fn reused_ssh_node_keeps_existing_saved_owner() {
        let mut node = WorkspaceSshNode::new(
            Some("existing-owner".to_string()),
            &SshConfig::default(),
            "Production".to_string(),
            vec![TerminalSessionId(1)],
            NodeReadiness::Ready,
        );

        assert!(!attach_saved_owner_to_reused_ssh_node(
            &mut node,
            "other-owner"
        ));
        assert_eq!(node.saved_connection_id.as_deref(), Some("existing-owner"));
    }

    #[test]
    fn saved_node_route_rejects_a_stale_direct_target() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        router.upsert_node(
            node_id.clone(),
            SshConfig::password("old.example.com", 22, "ops", "old-secret"),
        );
        let requested = SshConfig::password("new.example.com", 22, "ops", "new-secret");

        assert!(!saved_node_route_matches_config(
            &router, &node_id, &requested
        ));
    }

    #[test]
    fn saved_node_route_accepts_the_same_direct_target_with_new_auth() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        router.upsert_node(
            node_id.clone(),
            SshConfig::password("target.example.com", 22, "ops", "old-secret"),
        );
        let requested = SshConfig::password("target.example.com", 22, "ops", "new-secret");

        assert!(saved_node_route_matches_config(
            &router, &node_id, &requested
        ));
    }

    #[test]
    fn saved_node_route_rejects_a_different_proxy_chain() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let expansion = router
            .expand_manual_preset(
                "saved-a",
                vec![SshConfig::password("old-jump.example.com", 22, "ops", "pw")],
                SshConfig::password("target.example.com", 22, "ops", "pw"),
            )
            .unwrap();
        let requested = SshConfig {
            proxy_chain: Some(vec![ProxyHopConfig {
                host: "new-jump.example.com".to_string(),
                port: 22,
                username: "ops".to_string(),
                auth: AuthMethod::password("pw"),
                agent_forwarding: false,
                identity_agent: None,
                agent_forwarding_socket: None,
                legacy_ssh_compatibility: false,
                ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            }]),
            ..SshConfig::password("target.example.com", 22, "ops", "pw")
        };

        assert!(!saved_node_route_matches_config(
            &router,
            &expansion.target_node_id,
            &requested,
        ));
    }

    #[test]
    fn indexed_saved_node_requires_the_requested_saved_owner() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        let requested = SshConfig::password("shared.example.com", 22, "ops", "pw");
        router.upsert_node_with_origin(
            node_id.clone(),
            SshConfig::password("shared.example.com", 22, "ops", "pw"),
            NodeOrigin::Restored {
                saved_connection_id: "saved-a".to_string(),
            },
        );
        let mut node = WorkspaceSshNode::new(
            Some("saved-a".to_string()),
            &requested,
            "Saved A".to_string(),
            Vec::new(),
            NodeReadiness::Ready,
        );

        assert!(indexed_saved_node_matches_connection(
            &router, &node_id, &node, &requested, "saved-a"
        ));
        assert!(!indexed_saved_node_matches_connection(
            &router, &node_id, &node, &requested, "saved-b"
        ));

        router
            .update_node_origin(
                &node_id,
                NodeOrigin::Restored {
                    saved_connection_id: "saved-b".to_string(),
                },
            )
            .unwrap();
        assert!(!indexed_saved_node_matches_connection(
            &router, &node_id, &node, &requested, "saved-b"
        ));
        node.saved_connection_id = Some("saved-b".to_string());
        assert!(indexed_saved_node_matches_connection(
            &router, &node_id, &node, &requested, "saved-b"
        ));
    }

    #[test]
    fn indexed_saved_lookup_rejects_a_foreign_or_stale_mapping() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        let requested = SshConfig::password("shared.example.com", 22, "ops", "runtime-secret");
        router.upsert_node_with_origin(
            node_id.clone(),
            SshConfig::password("shared.example.com", 22, "ops", "runtime-secret"),
            NodeOrigin::Restored {
                saved_connection_id: "saved-a".to_string(),
            },
        );
        let node = WorkspaceSshNode::new(
            Some("saved-a".to_string()),
            &requested,
            "Saved A".to_string(),
            Vec::new(),
            NodeReadiness::Ready,
        );
        let ssh_nodes = HashMap::from([(node_id.clone(), node)]);
        let saved_ssh_nodes = HashMap::from([
            ("saved-a".to_string(), node_id.clone()),
            ("saved-b".to_string(), node_id.clone()),
        ]);

        assert_eq!(
            reusable_indexed_saved_node_for_config(
                &saved_ssh_nodes,
                &ssh_nodes,
                &router,
                "saved-a",
                &requested,
            ),
            Some(node_id)
        );
        assert_eq!(
            reusable_indexed_saved_node_for_config(
                &saved_ssh_nodes,
                &ssh_nodes,
                &router,
                "saved-b",
                &requested,
            ),
            None
        );

        let stale_config = SshConfig::password("changed.example.com", 22, "ops", "new-secret");
        assert_eq!(
            reusable_indexed_saved_node_for_config(
                &saved_ssh_nodes,
                &ssh_nodes,
                &router,
                "saved-a",
                &stale_config,
            ),
            None
        );
    }

    #[test]
    fn metadata_only_saved_reuse_requires_owner_and_current_route() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        router.upsert_node_with_origin(
            node_id.clone(),
            SshConfig::password("shared.example.com", 22, "ops", "runtime-secret"),
            NodeOrigin::Restored {
                saved_connection_id: "saved-a".to_string(),
            },
        );
        let requested = SshConfig::password("shared.example.com", 22, "ops", "runtime-secret");
        let node = WorkspaceSshNode::new(
            Some("saved-a".to_string()),
            &requested,
            "Saved A".to_string(),
            Vec::new(),
            NodeReadiness::Ready,
        );
        let store_path = std::env::temp_dir().join(format!(
            "oxideterm-saved-reuse-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = oxideterm_connections::ConnectionStore::load_read_only(store_path)
            .expect("empty read-only connection store");
        let mut connection = saved_connection_metadata("saved-a");

        assert!(indexed_saved_node_matches_saved_connection(
            &router,
            &store,
            &node_id,
            &node,
            &connection,
            "saved-a",
        ));
        assert!(!indexed_saved_node_matches_saved_connection(
            &router,
            &store,
            &node_id,
            &node,
            &connection,
            "saved-b",
        ));

        connection
            .proxy_chain
            .push(oxideterm_connections::SavedProxyHop {
                host: "jump.example.com".to_string(),
                port: 22,
                username: "ops".to_string(),
                auth: oxideterm_connections::SavedAuth::Agent,
                agent_forwarding: false,
                identity_agent: None,
                agent_forwarding_socket: None,
                legacy_ssh_compatibility: false,
                ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            });
        assert!(!indexed_saved_node_matches_saved_connection(
            &router,
            &store,
            &node_id,
            &node,
            &connection,
            "saved-a",
        ));
    }

    #[test]
    fn direct_root_reuse_rejects_another_saved_profiles_logical_node() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        let requested = SshConfig::password("shared.example.com", 22, "ops", "pw");
        router.upsert_node_with_origin(
            node_id.clone(),
            SshConfig::password("shared.example.com", 22, "ops", "pw"),
            NodeOrigin::Restored {
                saved_connection_id: "saved-a".to_string(),
            },
        );

        assert_eq!(
            reusable_direct_root_node_for_saved_config(&router, &requested, "saved-a"),
            Some(node_id)
        );
        assert_eq!(
            reusable_direct_root_node_for_saved_config(&router, &requested, "saved-b"),
            None
        );
    }

    #[test]
    fn direct_root_reuse_can_attach_an_unowned_logical_node() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("direct-node");
        let requested = SshConfig::password("shared.example.com", 22, "ops", "pw");
        router.upsert_node_with_origin(
            node_id.clone(),
            SshConfig::password("shared.example.com", 22, "ops", "pw"),
            NodeOrigin::Direct,
        );

        assert_eq!(
            reusable_direct_root_node_for_saved_config(&router, &requested, "saved-a"),
            Some(node_id)
        );
    }
}
