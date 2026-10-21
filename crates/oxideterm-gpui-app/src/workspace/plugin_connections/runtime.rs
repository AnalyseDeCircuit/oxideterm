use super::*;

// Storage remains lossless across provider versions; only declared fields reach the process.
fn connection_fields(
    schema: &plugin_host::NativePluginDeclarativeUiSchema,
) -> BTreeMap<String, bool> {
    fn collect(
        controls: &[plugin_host::NativePluginDeclarativeUiControl],
        section: &str,
        fields: &mut BTreeMap<String, bool>,
    ) {
        for control in controls {
            if matches!(
                control.kind.as_str(),
                "text" | "password" | "number" | "checkbox" | "select" | "textarea"
            ) && let Some(id) = &control.id
            {
                fields.insert(format!("{section}/{id}"), control.kind == "password");
            }
            collect(&control.children, section, fields);
        }
    }
    let mut fields = BTreeMap::new();
    collect(&schema.controls, "root", &mut fields);
    for section in &schema.sections {
        collect(&section.controls, &section.id, &mut fields);
    }
    fields
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_plugin_connection(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .plugin_profiles()
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        let definition = self
            .plugin_connection_types(cx)
            .into_iter()
            .find(|(plugin, definition)| {
                plugin == &profile.plugin_id && definition.id == profile.type_id
            })
            .map(|(_, definition)| definition);
        let instance = uuid::Uuid::new_v4().to_string();
        let tab = self.alloc_tab_id(cx);
        self.insert_tab(
            Tab {
                id: tab,
                kind: TabKind::Plugin {
                    plugin_id: profile.plugin_id.clone(),
                    tab_id: instance.clone(),
                },
                title: profile.name.clone(),
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            },
            cx,
        );
        // Each opening owns its provider resource independently of saved profile identity.
        self.plugin_entity.update(cx, |plugins, _| {
            plugins.connections.sessions.insert(
                instance,
                PluginConnectionSession {
                    plugin_id: profile.plugin_id,
                    profile_id: profile.id,
                    definition: definition.clone(),
                    schema: None,
                    started: false,
                    cancel: tokio_util::sync::CancellationToken::new(),
                    status: if definition.is_some() {
                        "connecting"
                    } else {
                        "unavailable"
                    },
                    terminal: None,
                    tab,
                },
            );
        });
        self.set_main_window_active_tab(Some(tab), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.bootstrap_native_plugin_runtime(cx);
        self.start_pending_plugin_connections(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn start_pending_plugin_connections(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let pending = self
            .plugin_entity
            .read(cx)
            .connections
            .sessions
            .iter()
            .filter(|(_, s)| !s.started && s.status == "connecting")
            .map(|(id, s)| (id.clone(), s.plugin_id.clone(), s.profile_id.clone()))
            .collect::<Vec<_>>();
        for (instance, plugin_id, profile_id) in pending {
            let plugin = self
                .plugin_entity
                .read(cx)
                .registry()
                .plugins()
                .iter()
                .find(|p| p.manifest.id == plugin_id)
                .cloned();
            let Some(plugin) = plugin else {
                self.plugin_entity.update(cx, |plugins, _| {
                    if let Some(session) = plugins.connections.sessions.get_mut(&instance) {
                        session.status = "unavailable";
                    }
                });
                continue;
            };
            if plugin.state != plugin_host::NativePluginState::Active {
                if !matches!(
                    plugin.state,
                    plugin_host::NativePluginState::ReadyProcess
                        | plugin_host::NativePluginState::Loading
                ) {
                    self.plugin_entity.update(cx, |plugins, _| {
                        if let Some(session) = plugins.connections.sessions.get_mut(&instance) {
                            session.status = "unavailable";
                        }
                    });
                }
                continue;
            }
            let profile = self
                .connection_store
                .plugin_profiles()
                .iter()
                .find(|p| p.id == profile_id)
                .cloned();
            let definition = profile.as_ref().and_then(|profile| {
                plugin
                    .manifest
                    .contributes
                    .as_ref()?
                    .connections
                    .as_ref()?
                    .iter()
                    .find(|definition| definition.id == profile.type_id)
            });
            let Some(definition) = definition else {
                self.plugin_entity.update(cx, |plugins, _| {
                    if let Some(session) = plugins.connections.sessions.get_mut(&instance) {
                        session.status = "unavailable";
                    }
                });
                continue;
            };
            let fields = connection_fields(&definition.form);
            let credentials = self
                .connection_store
                .plugin_connection_credentials_for_fields(&profile_id, |field| {
                    fields.get(field) == Some(&true)
                });
            let (Some(mut profile), Ok(credentials)) = (profile, credentials) else {
                self.plugin_entity.update(cx, |plugins, _| {
                    if let Some(session) = plugins.connections.sessions.get_mut(&instance) {
                        session.status = "failed";
                    }
                });
                continue;
            };
            // Raw credentials are handed only to an explicitly approved provider.
            if !credentials.is_empty()
                && !plugin
                    .config
                    .approved_capabilities
                    .iter()
                    .any(|c| c == "credentials.raw.read")
            {
                self.plugin_entity.update(cx, |plugins, _| {
                    if let Some(session) = plugins.connections.sessions.get_mut(&instance) {
                        session.status = "failed";
                    }
                });
                continue;
            }
            let secrets: BTreeMap<_, _> = credentials
                .iter()
                .map(|(key, secret)| (key, secret.expose_secret()))
                .collect();
            profile.config.retain(|field, _| {
                fields.get(field) == Some(&false) && !profile.credential_refs.contains_key(field)
            });
            let arguments = serde_json::json!({"instanceId":instance,"profileId":profile.id,"typeId":profile.type_id,"config":profile.config,"credentials":secrets});
            let resolver = self.native_plugin_host_api_resolver(cx);
            self.plugin_entity.update(cx, |plugins, _| {
                if let Some(session) = plugins.connections.sessions.get_mut(&instance) {
                    session.started = true;
                    let cancel = session.cancel.clone();
                    plugins.start_connection_open(plugin_id, instance, arguments, cancel, resolver);
                }
            });
        }
    }

    pub(in crate::workspace) fn disconnect_plugin_connection(
        &mut self,
        instance: &str,
        cx: &mut Context<Self>,
    ) {
        let plugin = self.plugin_entity.update(cx, |plugins, _| {
            let session = plugins.connections.sessions.get_mut(instance)?;
            let was_open = matches!(session.status, "connecting" | "connected");
            session.cancel.cancel();
            session.status = "disconnected";
            session.schema = None;
            if let Some(terminal) = session.terminal.take() {
                terminal.lock().shutdown();
            }
            let plugin = session.plugin_id.clone();
            let started = session.started;
            plugins
                .ui_state_mut()
                .remove_surface(&plugin, "tab", instance);
            (was_open && started).then_some(plugin)
        });
        if let Some(plugin) = plugin {
            self.close_plugin_connection_resource(&plugin, instance, cx);
        }
        cx.notify();
    }

    fn close_plugin_connection_resource(
        &mut self,
        plugin: &str,
        instance: &str,
        cx: &mut Context<Self>,
    ) {
        let resolver = self.native_plugin_host_api_resolver(cx);
        self.plugin_entity.read(cx).start_connection_close(
            plugin.into(),
            instance.into(),
            resolver,
        );
    }

    pub(in crate::workspace) fn close_plugin_connection_tab(
        &mut self,
        tab: TabId,
        cx: &mut Context<Self>,
    ) {
        let instances = self
            .plugin_entity
            .read(cx)
            .connections
            .sessions
            .iter()
            .filter(|(_, session)| session.tab == tab && session.terminal.is_none())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for instance in instances {
            self.disconnect_plugin_connection(&instance, cx);
            self.plugin_entity.update(cx, |plugins, _| {
                plugins.connections.sessions.remove(&instance);
            });
        }
    }

    pub(in crate::workspace) fn close_plugin_connection_terminal(
        &mut self,
        terminal: &SharedTerminalSession,
        cx: &mut Context<Self>,
    ) {
        let instance = self
            .plugin_entity
            .read(cx)
            .connections
            .sessions
            .iter()
            .find(|(_, s)| {
                s.terminal
                    .as_ref()
                    .is_some_and(|owned| Arc::ptr_eq(owned, terminal))
            })
            .map(|(id, _)| id.clone());
        if let Some(instance) = instance {
            self.disconnect_plugin_connection(&instance, cx);
            self.plugin_entity.update(cx, |plugins, _| {
                plugins.connections.sessions.remove(&instance);
            });
        }
    }

    pub(in crate::workspace) fn apply_plugin_connection_opened(
        &mut self,
        plugin: &str,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(instance) = args.get("instanceId").and_then(|v| v.as_str()) else {
            return;
        };
        let Some((status, definition, old_tab, profile_id)) = self
            .plugin_entity
            .read(cx)
            .connections
            .sessions
            .get(instance)
            .filter(|s| s.plugin_id == plugin)
            .map(|s| (s.status, s.definition.clone(), s.tab, s.profile_id.clone()))
        else {
            return;
        };
        if status != "connecting" {
            return;
        }
        let value = &args["value"];
        let title = self
            .tab_by_id(old_tab, cx)
            .map(|tab| tab.title.clone())
            .unwrap_or_default();
        let mut connected = false;
        match value.get("kind").and_then(|v| v.as_str()) {
            Some("workbench") => {
                if let Ok(schema) = serde_json::from_value::<
                    plugin_host::NativePluginDeclarativeUiSchema,
                >(value["schema"].clone())
                    && plugin_host::validate_native_plugin_declarative_ui_schema(&schema).is_ok()
                {
                    self.plugin_entity.update(cx, |plugins, _| {
                        if let Some(session) = plugins.connections.sessions.get_mut(instance) {
                            session.schema.get_or_insert(schema);
                        }
                    });
                    connected = true;
                }
            }
            Some("terminal") => {
                let launch = (|| {
                    let entry = definition.as_ref()?.terminal_entry.as_ref()?;
                    let info = self
                        .plugin_entity
                        .read(cx)
                        .registry()
                        .plugins()
                        .iter()
                        .find(|p| {
                            p.manifest.id == plugin
                                && p.state == plugin_host::NativePluginState::Active
                        })?;
                    let path =
                        plugin_runtime::resolve_process_runtime_entry(&info.install_dir, entry)
                            .ok()?;
                    let arguments: Vec<String> =
                        serde_json::from_value(value["args"].clone()).ok()?;
                    Some((path, arguments, title.clone()))
                })();
                if let Some((path, arguments, title)) = launch {
                    let shell = oxideterm_terminal::ShellInfo::new(plugin, &title, path)
                        .with_args(arguments);
                    let config = LocalPtyConfig {
                        shell: Some(shell),
                        load_profile: false,
                        ..Default::default()
                    };
                    if let Ok((_session_id, terminal)) =
                        self.create_local_terminal_tab_with_owned_session(config, title, window, cx)
                    {
                        if let Some(tab) = self.active_tab_id(cx) {
                            self.plugin_entity.update(cx, |plugins, _| {
                                if let Some(session) =
                                    plugins.connections.sessions.get_mut(instance)
                                {
                                    session.terminal = Some(terminal);
                                    session.tab = tab;
                                }
                            });
                            self.close_tab_by_id(old_tab, window, cx);
                            connected = true;
                        }
                    }
                }
            }
            Some("sshTerminal") => {
                let approved = self
                    .plugin_entity
                    .read(cx)
                    .registry()
                    .plugins()
                    .iter()
                    .any(|p| {
                        p.manifest.id == plugin
                            && p.state == plugin_host::NativePluginState::Active
                            && p.config
                                .approved_capabilities
                                .iter()
                                .any(|c| c == "terminal.write")
                    });
                if approved
                    && let Some((node, command)) = ssh_terminal_command(value)
                    && self.node_is_ready_for_terminal(&node)
                {
                    // The existing node owns SSH; this plugin receives only a terminal consumer.
                    if let Ok(session_id) = self.create_ssh_terminal_tab_for_existing_node(
                        &node,
                        Some(command),
                        title,
                        window,
                        cx,
                    ) && let Some(location) =
                        self.tab_host.read(cx).terminal_location(session_id)
                        && let Some(terminal) = self
                            .tab_host
                            .read(cx)
                            .panes()
                            .get(&location.pane_id)
                            .map(|pane| pane.read(cx).shared_session())
                    {
                        self.plugin_entity.update(cx, |plugins, _| {
                            if let Some(session) = plugins.connections.sessions.get_mut(instance) {
                                session.terminal = Some(terminal);
                                session.tab = location.tab_id;
                            }
                        });
                        self.close_tab_by_id(old_tab, window, cx);
                        connected = true;
                    }
                }
            }
            _ => {}
        }
        self.plugin_entity.update(cx, |plugins, _| {
            if let Some(session) = plugins.connections.sessions.get_mut(instance) {
                session.status = if connected { "connected" } else { "failed" };
            }
        });
        if !connected {
            self.close_plugin_connection_resource(plugin, instance, cx);
        }
        if connected {
            let _ = self.connection_store.mark_plugin_profile_used(&profile_id);
        }
        cx.notify();
    }
}

fn ssh_terminal_command(value: &serde_json::Value) -> Option<(NodeId, String)> {
    if value.get("shell")?.as_str()? != "posix" {
        return None;
    }
    let node = NodeId::new(value.get("nodeId")?.as_str()?.to_string());
    let argv = serde_json::from_value::<Vec<String>>(value.get("argv")?.clone()).ok()?;
    // This is typed into an interactive shell: control characters can edit or submit
    // the command line before shell quoting has any effect.
    if argv.first().is_none_or(String::is_empty)
        || argv.iter().any(|arg| arg.chars().any(char::is_control))
    {
        return None;
    }
    Some((
        node,
        argv.iter()
            .map(|arg| oxideterm_environment::shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" "),
    ))
}

impl PluginConnections {
    pub(in crate::workspace) fn apply_message(
        &mut self,
        plugin: &str,
        message: &plugin_runtime::PluginOutboundMessage,
    ) -> Option<Result<(), String>> {
        let plugin_runtime::PluginOutboundMessage::RegisterContribution { registration } = message
        else {
            return None;
        };
        let instance = registration.metadata.get("connectionId")?.as_str();
        Some((|| {
            let instance = instance.ok_or("Invalid connection identity")?;
            if registration.plugin_id != plugin
                || registration.kind != plugin_runtime::PluginRegistrationKind::Tab
            {
                return Err("Invalid connection view owner".into());
            }
            // Late pushes cannot recreate a consumer after closure.
            let Some(session) = self.sessions.get_mut(instance) else {
                return Ok(());
            };
            if session.plugin_id != plugin {
                return Err("Invalid connection view owner".into());
            }
            if !matches!(session.status, "connecting" | "connected") || session.terminal.is_some() {
                return Ok(());
            }
            let schema = serde_json::from_value(registration.metadata["schema"].clone())
                .map_err(|_| "Invalid connection view")?;
            plugin_host::validate_native_plugin_declarative_ui_schema(&schema)?;
            session.schema = Some(schema);
            Ok(())
        })())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_fields_follow_installed_schema_without_exposing_unknown_slots() {
        let schema = serde_json::from_value(serde_json::json!({
            "controls": [{"kind":"text","id":"host"}, {"kind":"markdown","id":"help"}],
            "sections": [{"id":"auth","controls":[{"kind":"row","children":[
                {"kind":"password","id":"token"}, {"kind":"checkbox","id":"tls"}
            ]}]}]
        }))
        .unwrap();
        assert_eq!(
            connection_fields(&schema),
            [
                ("root/host".into(), false),
                ("auth/token".into(), true),
                ("auth/tls".into(), false)
            ]
            .into()
        );
    }

    #[test]
    fn ssh_connection_arguments_remain_literal_and_cannot_edit_the_command_line() {
        let request = serde_json::json!({"shell":"posix","nodeId":"node-one","argv":["tool","a b","it's; $(touch bad)",""]});
        let (node, command) = ssh_terminal_command(&request).unwrap();
        assert_eq!(node.0, "node-one");
        assert_eq!(command, "'tool' 'a b' 'it'\\''s; $(touch bad)' ''");
        for argument in ["\n", "\r", "\0", "\u{1b}", "\u{f}", "\t"] {
            let mut invalid = request.clone();
            invalid["argv"][1] = serde_json::json!(argument);
            assert!(ssh_terminal_command(&invalid).is_none(), "{argument:?}");
        }
        let mut unsupported = request;
        unsupported["shell"] = serde_json::json!("powershell");
        assert!(ssh_terminal_command(&unsupported).is_none());
    }

    #[test]
    fn connection_views_route_by_instance_and_stop_with_the_provider() {
        let mut connections = PluginConnections::default();
        for (id, plugin) in [
            ("one", "provider"),
            ("two", "provider"),
            ("other", "another-provider"),
        ] {
            connections.sessions.insert(
                id.into(),
                PluginConnectionSession {
                    plugin_id: plugin.into(),
                    profile_id: "same-saved-profile".into(),
                    definition: None,
                    schema: None,
                    started: true,
                    cancel: tokio_util::sync::CancellationToken::new(),
                    status: "connected",
                    terminal: None,
                    tab: TabId(1),
                },
            );
        }
        let update = |plugin: &str, instance: &str, value: &str| {
            plugin_runtime::PluginOutboundMessage::RegisterContribution {
                registration: plugin_runtime::PluginRegistration {
                    registration_id: instance.into(),
                    plugin_id: plugin.into(),
                    kind: plugin_runtime::PluginRegistrationKind::Tab,
                    metadata: serde_json::json!({"connectionId":instance,"schema":{"controls":[{"kind":"text","id":"message","value":value}]}}),
                },
            }
        };
        connections
            .apply_message("provider", &update("provider", "one", "first message"))
            .unwrap()
            .unwrap();
        connections
            .apply_message("provider", &update("provider", "two", "second message"))
            .unwrap()
            .unwrap();
        assert_eq!(
            connections.sessions["one"]
                .schema
                .as_ref()
                .unwrap()
                .controls[0]
                .value,
            Some(serde_json::json!("first message"))
        );
        assert_eq!(
            connections.sessions["two"]
                .schema
                .as_ref()
                .unwrap()
                .controls[0]
                .value,
            Some(serde_json::json!("second message"))
        );
        assert_eq!(
            connections
                .apply_message("provider", &update("provider", "other", "wrong owner"))
                .unwrap(),
            Err("Invalid connection view owner".into())
        );
        assert!(connections.sessions["other"].schema.is_none());
        connections.sessions.remove("one");
        connections
            .apply_message("provider", &update("provider", "one", "late message"))
            .unwrap()
            .unwrap();
        assert!(!connections.sessions.contains_key("one"));
        connections.stop_plugin("provider");
        connections
            .apply_message("provider", &update("provider", "two", "retired message"))
            .unwrap()
            .unwrap();
        assert_eq!(connections.sessions["two"].status, "unavailable");
        assert!(connections.sessions["two"].schema.is_none());
        assert!(connections.sessions["two"].cancel.is_cancelled());
        assert_eq!(connections.sessions["other"].status, "connected");
        assert!(!connections.sessions["other"].cancel.is_cancelled());
    }
}
