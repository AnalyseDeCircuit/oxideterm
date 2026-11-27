use super::*;
use oxideterm_gpui_connections::SshAuthTab;
use oxideterm_settings::{DiscoverySshAuth, DiscoverySshPreferences};
use oxideterm_connections::ConnectionStore;

fn apply_discovered_ssh_preferences(
    prefs: &DiscoverySshPreferences,
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    passphrase: Option<zeroize::Zeroizing<String>>,
) -> Result<(), &'static str> {
    form.auth_tab = match &prefs.auth {
        DiscoverySshAuth::Password => SshAuthTab::Password,
        DiscoverySshAuth::DefaultKey => SshAuthTab::DefaultKey,
        DiscoverySshAuth::KeyFile(path) => {
            form.key_path.clone_from(path);
            SshAuthTab::SshKey
        }
        DiscoverySshAuth::Certificate { key_path, cert_path } => {
            form.key_path.clone_from(key_path);
            form.cert_path.clone_from(cert_path);
            SshAuthTab::Certificate
        }
        DiscoverySshAuth::ManagedKey(id) => {
            store
                .managed_ssh_key_metadata(id)
                .map_err(|_| "ssh.form.managed_key_required")?;
            form.managed_key_id.clone_from(id);
            SshAuthTab::ManagedKey
        }
        DiscoverySshAuth::Agent => {
            form.identity_agent = prefs.identity_agent.clone().unwrap_or_default();
            SshAuthTab::Agent
        }
        DiscoverySshAuth::TwoFactor => SshAuthTab::TwoFactor,
    };
    if let Some(id) = &prefs.totp_credential_id
        && !store
            .totp_credentials()
            .iter()
            .any(|entry| entry.enabled && entry.id == *id)
    {
        return Err("settings_view.totp.unavailable");
    }
    if let Some(mut passphrase) = passphrase {
        form.passphrase = std::mem::take(&mut *passphrase);
    }
    form.totp_credential_id
        .clone_from(&prefs.totp_credential_id);
    Ok(())
}

fn can_connect_with_preset(
    prefs: &DiscoverySshPreferences,
    login: Option<&str>,
) -> bool {
    prefs.skip_connection_form && login.is_some_and(|name| !name.trim().is_empty())
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_process_discovery_target(
        &mut self,
        mut source: DiscoverySource,
        target_id: String,
        open_request: Arc<()>,
        reconnect: Option<(
            NodeId,
            oxideterm_discovery::DiscoveryOrigin,
            CancellationToken,
        )>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(plugin) = source.plugin_id.as_deref() else {
            return;
        };
        let Some(process) = self
            .plugin_entity
            .read(cx)
            .discovery_process(plugin, &source.provider_id)
        else {
            return self.report_discovery_open_failure(DiscoveryFailure::Unavailable, cx);
        };
        source.apply_ssh_policy(&process.definition);
        let Some(epoch) = self.discovery.epoch(&source.id) else {
            return self.report_discovery_open_failure(DiscoveryFailure::PermissionDenied, cx);
        };
        let Some(target) = self
            .discovery
            .snapshot(&source.id)
            .and_then(|s| s.targets.iter().find(|t| t.id == target_id).cloned())
        else {
            return self.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
        };
        // Selections and the handoff must keep the directory grant that started this request.
        let selected_origin =
            oxideterm_discovery::DiscoveryOrigin::new(source.id.clone(), target_id.clone(), epoch)
                .with_target_revision(&self.discovery, target.target_revision.clone());
        let Some(tracking) = self.discovery.track_open(&source.id, epoch) else {
            return self.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
        };
        let instance_id = selected_origin.instance_id.clone();
        let lifetime = Arc::new(());
        let validity = reconnect
            .as_ref()
            .map(|(_, _, token)| token.child_token())
            .unwrap_or_default();
        self.plugin_entity.update(cx, |entity, _| {
            let entries = &mut entity.discovery_instances.entries;
            entries.retain(|_, e| e.owner.strong_count() != 0 || e.terminal.is_some());
            entries.insert(
                instance_id.clone(),
                OpenInstance {
                    plugin: process.plugin.clone(),
                    generation: process.generation,
                    target: target_id.clone(),
                    provider: source.provider_id.clone(),
                    source: source.id.clone(),
                    cancel: validity.clone(),
                    terminal: None,
                    cleanup: None,
                    owner: Arc::downgrade(&lifetime),
                    host_ssh: None,
                },
            );
        });
        let mut cleanup = InstanceCleanup {
            _lifetime: lifetime,
            process: process.clone(),
            source_id: source.id.clone(),
            instance_id: instance_id.clone(),
            stage: CleanupStage::None,
            tracking: Some(tracking),
        };
        let i18n = self.i18n.clone();
        let discovery = self.discovery.clone();
        let secret_store = self.ai.state.read(cx).key_store().clone();
        let op_token = validity.child_token();
        let operation = op_token.clone().drop_guard();
        let mut seen = HashSet::new();
        let mut selection = None::<(String, String)>;
        cx.spawn_in(window, async move |workspace, cx| {
            let _operation = operation;
            let current = || {
                !validity.is_cancelled()
                    && !process.live.is_cancelled()
                    && selected_origin.can_handshake()
            };
            let invalidated = async {
                tokio::select! {
                    _ = validity.cancelled() => {},
                    _ = process.live.cancelled() => {},
                    _ = selected_origin.binding_changed() => {},
                }
                op_token.cancel();
            };
            tokio::pin!(invalidated);
            let failure = loop {
                if !current() {
                    break DiscoveryFailure::StaleTarget;
                }
                let (continuation, option_id) = selection.take().unzip();
                let request = DiscoveryRequest::PrepareOpen {
                    instance_id: instance_id.clone(),
                    target_id: target_id.clone(),
                    config: source.config.clone(),
                    continuation,
                    option_id,
                };
                cleanup.stage = CleanupStage::Choosing;
                let response = tokio::select! {
                    _ = &mut invalidated => break DiscoveryFailure::StaleTarget,
                    value = process.call_cancellable(&source.id, request, NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                        Some(op_token.clone())) => value,
                };
                let open = match response {
                    Ok(DiscoveryReply::Open(open)) => open,
                    failure => {
                        if !current() {
                            break DiscoveryFailure::StaleTarget;
                        }
                        let reason = failure
                            .err()
                            .unwrap_or(DiscoveryFailure::InvalidResponse);
                        discovery.report_failure(&source.id, epoch, reason.clone());
                        break reason;
                    }
                };
                match open {
                    DiscoveryOpen::Choose {
                        title_key,
                        continuation,
                        options,
                    } => {
                        if !current() {
                            break DiscoveryFailure::StaleTarget;
                        }
                        if !seen.insert(continuation.clone()) {
                            break DiscoveryFailure::InvalidResponse;
                        }
                        let translations = &process.definition.form.translations;
                        let title = translations
                            .get(plugin_ui::native_plugin_locale_tag(i18n.locale()))
                            .or_else(|| translations.get("en"))
                            .and_then(|messages| messages.get(title_key.trim_start_matches('@')))
                            .cloned()
                            .unwrap_or(title_key);
                        let prompt = workspace.update_in(cx, |_, window, cx| {
                            let cancel = i18n.t("common.actions.cancel");
                            let mut answers = options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>();
                            answers.push(&cancel);
                            window.prompt(
                                gpui::PromptLevel::Info,
                                &title,
                                Some(&source.name),
                                &answers,
                                cx,
                            )
                        });
                        let Ok(prompt) = prompt else {
                            return;
                        };
                        let answer = tokio::select! {
                            _ = &mut invalidated => break DiscoveryFailure::StaleTarget,
                            index = prompt => index,
                        };
                        let Some(option) = answer.ok().and_then(|n| options.get(n)) else {
                            return;
                        };
                        selection = Some((continuation, option.id.clone()));
                    }
                    DiscoveryOpen::Ready { owner, launch } => {
                        cleanup.stage = match owner {
                            DiscoveryOwner::Host => CleanupStage::None,
                            DiscoveryOwner::Provider => CleanupStage::Provider,
                        };
                        if !current() {
                            break DiscoveryFailure::StaleTarget;
                        }
                        let mut reconnect_from = None;
                        if let Some((node, previous, _)) = &reconnect {
                            let comparison = match &launch {
                                DiscoveryLaunch::Ssh { session_identity, .. } =>
                                    previous.same_access_identity(&selected_origin.clone().with_session_identity(session_identity.clone())),
                                DiscoveryLaunch::Terminal { .. } => None,
                            };
                            if comparison == Some(true) {
                                reconnect_from = Some((node.0.clone(), previous.instance_id.clone()));
                            } else if comparison != Some(false) || seen.is_empty() {
                                let Ok(prompt) = workspace.update_in(cx, |_, window, cx| {
                                    let cancel = i18n.t("common.actions.cancel");
                                    let open = i18n.t("settings_view.discovery.separate_session");
                                    window.prompt(gpui::PromptLevel::Info, &i18n.t("settings_view.discovery.identity_changed"),
                                        Some(&target.label), &[cancel.as_str(), open.as_str()], cx)
                                }) else { return; };
                                tokio::select! {
                                    _ = &mut invalidated => break DiscoveryFailure::StaleTarget,
                                    answer = prompt => if !matches!(answer, Ok(1)) { return; }
                                }
                            }
                            if !workspace.update_in(cx, |this, _, _| {
                                !this.node_connection_is_active_or_connecting(node)
                                    && this.node_router.runtime_store().discovery_origin(node).as_ref() == Some(previous)
                            }).unwrap_or(false) || !current() {
                                break DiscoveryFailure::StaleTarget;
                            }
                        }
                        let title = format!("{} · {}", source.name, target.label);
                        match launch {
                            DiscoveryLaunch::Terminal { args } => {
                                let _ = workspace.update_in(cx, |this, window, cx| {
                                    if !current() {
                                        return this.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
                                    }
                                    let Some(lease) = this.discovery.acquire(&source.id, epoch) else {
                                        return this.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
                                    };
                                    let path = this
                                        .plugin_entity
                                        .read(cx)
                                        .registry()
                                        .plugins()
                                        .iter()
                                        .find(|p| p.manifest.id == process.plugin)
                                        .and_then(|p| {
                                            plugin_runtime::resolve_process_runtime_entry(
                                                &p.install_dir,
                                                process.definition.terminal_entry.as_deref()?,
                                            )
                                            .ok()
                                        });
                                    let Some(path) = path else {
                                        return this
                                            .report_discovery_open_failure(DiscoveryFailure::Unavailable, cx);
                                    };
                                    let shell =
                                        oxideterm_terminal::ShellInfo::new(&process.plugin, &title, path)
                                            .with_args(args);
                                    let config = LocalPtyConfig {
                                        shell: Some(shell),
                                        load_profile: false,
                                        ..Default::default()
                                    };
                                    let Ok((session_id, terminal)) = this
                                        .create_discovery_terminal_tab(
                                            config, &source, target.label.clone(), window, cx,
                                        )
                                    else {
                                        return this
                                            .report_discovery_open_failure(DiscoveryFailure::Unavailable, cx);
                                    };
                                    if let Some(location) =
                                        this.tab_host.read(cx).terminal_location(session_id)
                                        && let Some(pane) = this
                                            .tab_host
                                            .read(cx)
                                            .terminal_pane_for_target(location.pane_id, session_id)
                                    {
                                        pane.update(cx, |pane, cx| {
                                            pane.set_discovery_reopen_label(
                                                i18n.t("settings_view.discovery.reopen"),
                                                cx,
                                            )
                                        });
                                    }
                                    this.plugin_entity.update(cx, |entity, _| {
                                        if let Some(entry) =
                                            entity.discovery_instances.entries.get_mut(&instance_id)
                                        {
                                            entry.terminal = Some(terminal);
                                            entry.cleanup = Some((cleanup, lease));
                                        } else {
                                            terminal.lock().shutdown();
                                        }
                                    });
                                });
                            }
                            DiscoveryLaunch::Ssh {
                                endpoint,
                                username: login,
                                allow_reuse_on_open,
                                asset_session,
                                session_identity,
                            } => {
                                let (host, port) = (endpoint.host, endpoint.port.get());
                                let passphrase = if let Some(account) = source.ssh_preferences.passphrase_account(&source.id) {
                                    let store = secret_store.clone();
                                    match process.runtime.spawn_blocking(move || store.get_provider_key(&account)).await {
                                        Ok(Ok(Some(value))) => Some(value),
                                        _ => {
                                            let _ = workspace.update_in(cx, |this, _, cx| {
                                                if current() {
                                                    this.report_discovery_open_error(i18n.t("settings_view.totp.unavailable"), cx);
                                                }
                                            });
                                            return;
                                        }
                                    }
                                } else {
                                    None
                                };
                                let use_preset = can_connect_with_preset(
                                    &source.ssh_preferences, login.as_deref(),
                                );
                                let _ = workspace.update_in(cx, |this, window, cx| {
                                    if !current() {
                                        return this.report_discovery_open_failure(DiscoveryFailure::StaleTarget, cx);
                                    }
                                    let mut form = NewConnectionForm::default();
                                    if let Err(key) = apply_discovered_ssh_preferences(&source.ssh_preferences, &mut form, &this.connection_store, passphrase)
                                    {
                                        return this.report_discovery_open_error(i18n.t(key), cx);
                                    }
                                    form.host = host.clone();
                                    form.port = port.to_string();
                                    form.name = title;
                                    form.username = login.clone().unwrap_or_default();
                                    form.save_connection = false;
                                    form.save_password = false;
                                    let resources: Arc<dyn Send + Sync> = match owner {
                                        // A host-owned transport keeps its source reservation without retaining the plugin runtime.
                                        DiscoveryOwner::Host => Arc::new((cleanup._lifetime.clone(), cleanup.tracking.take())),
                                        DiscoveryOwner::Provider => Arc::new(cleanup),
                                    };
                                    let mut origin = selected_origin
                                        .clone()
                                        .with_instance(
                                            instance_id.clone(),
                                            validity.clone(),
                                            resources,
                                        )
                                        .with_directory_reuse(allow_reuse_on_open, process.live.clone())
                                        .with_asset_session(asset_session)
                                        .with_session_identity(session_identity)
                                        .with_open_request(open_request)
                                        .with_endpoint(host, port, login);
                                    origin.reconnect_from = reconnect_from;
                                    if owner == DiscoveryOwner::Host {
                                        this.plugin_entity.update(cx, |entity, _| {
                                            if let Some(instance) = entity.discovery_instances.entries.get_mut(&instance_id) {
                                                instance.host_ssh = Some(origin.observe_establishment());
                                            }
                                        });
                                    }
                                    form.discovery_origin = Some(origin);
                                    if use_preset {
                                        return this.connect_discovery_with_preset(form, window, cx);
                                    }
                                    this.prepare_modal_interaction_boundary(cx);
                                    this.update_connection_form_state(cx, |state| {
                                        state.replace_with_new_form(form)
                                    });
                                    this.show_active_input_caret(cx);
                                    this.needs_active_pane_focus = false;
                                    window.focus(&this.focus_handle, cx);
                                    cx.notify();
                                });
                            }
                        }
                        return;
                    }
                }
            };
            let _ = workspace.update_in(cx, |this, _, cx| this.report_discovery_open_failure(failure, cx));
        }).detach();
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_gpui_connections::{
        configuration::RuntimeSecretHandoff, conversion::build_ssh_config,
    };

    #[test]
    fn discovery_ssh_preset_reaches_native_auth_and_rejects_a_missing_totp() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        std::fs::write(&path, serde_json::json!({"totp_credentials":[{
            "id":"gateway-mfa","name":"Gateway MFA","enabled":true,
            "parameters":{"algorithm":"Sha1","digits":6,"period":30},
            "prompt_pattern":"OTP","updated_at":"2026-01-01T00:00:00Z","secret_revision":"fixture"
        }]}).to_string()).unwrap();
        let store = ConnectionStore::load_read_only(&path).unwrap();
        let mut prefs = DiscoverySshPreferences {
            auth: DiscoverySshAuth::TwoFactor,
            totp_credential_id: Some("gateway-mfa".into()),
            skip_connection_form: true,
            ..Default::default()
        };
        let login = Some("alice@ops@asset");
        assert!(can_connect_with_preset(&prefs, login));
        assert!(!can_connect_with_preset(&DiscoverySshPreferences::default(), login));
        assert!(!can_connect_with_preset(&prefs, None));
        let mut form = NewConnectionForm::default();
        form.host = "gateway.example".into();
        form.username = "alice@ops@asset".into();
        apply_discovered_ssh_preferences(&prefs, &mut form, &store, None).unwrap();
        let (config, _) = build_ssh_config(
            &mut form,
            &store,
            &Default::default(),
            &Default::default(),
            RuntimeSecretHandoff::Move,
        )
        .unwrap();
        assert!(matches!(
            config.auth,
            oxideterm_ssh::AuthMethod::KeyboardInteractive
        ));
        assert_eq!(config.totp.unwrap().credential_id, "gateway-mfa");
        for (auth, certificate) in [
            (DiscoverySshAuth::KeyFile("fixture/key".into()), false),
            (DiscoverySshAuth::Certificate { key_path: "fixture/key".into(), cert_path: "fixture/cert.pub".into() }, true),
        ] {
            prefs.auth = auth;
            apply_discovered_ssh_preferences(&prefs, &mut form, &store, Some(zeroize::Zeroizing::new("fixture-unlock".into()))).unwrap();
            let (config, _) = build_ssh_config(&mut form, &store, &Default::default(), &Default::default(), RuntimeSecretHandoff::Move).unwrap();
            match config.auth {
                oxideterm_ssh::AuthMethod::Key {key_path, passphrase} if !certificate => {
                    assert_eq!(key_path, "fixture/key");
                    assert_eq!(passphrase.unwrap().as_str(), "fixture-unlock");
                }
                oxideterm_ssh::AuthMethod::Certificate { key_path, cert_path, passphrase } if certificate => {
                    assert_eq!((key_path.as_str(), cert_path.as_str()), ("fixture/key", "fixture/cert.pub"));
                    assert_eq!(passphrase.unwrap().as_str(), "fixture-unlock");
                }
                _ => panic!("discovery lost the selected key source"),
            }
            assert_eq!(config.totp.unwrap().credential_id, "gateway-mfa");
        }
        prefs.totp_credential_id = Some("removed-mfa".into());
        assert_eq!(
            apply_discovered_ssh_preferences(&prefs, &mut form, &store, None),
            Err("settings_view.totp.unavailable")
        );
    }
}
