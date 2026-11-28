use super::*;
use crate::workspace::session_manager::tests::isolated_workspace_fixture;
use gpui::TestAppContext;

#[gpui::test]
fn discovery_pty_survives_background_reattachment_and_releases_on_exit(cx: &mut TestAppContext) {
    let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_DISCOVERY_PTY_TEST_DIR") else {
        return;
    };
    cx.executor().allow_parking();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
        WorkspaceWindowShell::new(workspace, window, cx)
    });
    let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
    let (session_id, terminal, exited, runtime) = cx.update(|window, cx| {
        workspace.update(cx, |ws, cx| {
            #[cfg(unix)]
            let shell = oxideterm_terminal::ShellInfo::new("fixture", "Fixture", "/bin/sh");
            #[cfg(windows)]
            let shell = oxideterm_terminal::ShellInfo::new("fixture", "Fixture", "cmd.exe")
                .with_args(vec!["/D".into(), "/Q".into()]);
            let (id, terminal) = ws
                .create_discovery_terminal_tab(
                    LocalPtyConfig {
                        shell: Some(shell),
                        load_profile: false,
                        ..Default::default()
                    },
                    &DiscoverySource {
                        id: "production".into(),
                        name: "Inventory".into(),
                        ..Default::default()
                    },
                    "Database".into(),
                    window,
                    cx,
                )
                .unwrap();
            let exited = CancellationToken::new();
            ws.plugin_entity.update(cx, |plugin, _| {
                plugin.discovery_instances.entries.insert(
                    "open".into(),
                    OpenInstance {
                        plugin: "inventory".into(),
                        generation: 1,
                        provider: "hosts".into(),
                        source: "production".into(),
                        target: "db".into(),
                        cancel: exited.clone(),
                        terminal: Some(terminal.clone()),
                        cleanup: None,
                        owner: Default::default(),
                        host_ssh: None,
                    },
                );
            });

            ws.detach_active_local_terminal_from_palette(window, cx);
            assert!(ws.tab_host.read(cx).terminal_location(id).is_none());
            assert!(terminal.lock().lifecycle().is_running());
            assert!(!exited.is_cancelled());

            ws.attach_detached_local_terminal_session(id, window, cx);
            let pane = ws.tab_host.read(cx).panes()[&ws.active_pane_id(cx).unwrap()].read(cx);
            assert!(Arc::ptr_eq(&pane.shared_session(), &terminal));
            assert_eq!(ws.active_tab(cx).unwrap().title, "Inventory · Database");
            assert_eq!(
                ws.plugin_entity
                    .read(cx)
                    .discovery_terminal_target(&terminal),
                Some(("production".into(), "db".into()))
            );
            assert!(!ws.can_split_active_pane(cx));
            assert!(ws.active_terminal_cwd_scope_and_pane(cx).is_none());
            assert_eq!(
                ws.terminal_command_active_target_label(cx),
                "Inventory · Database"
            );

            let pane_id = ws.active_pane_id(cx).unwrap();
            let target = ws.quick_command_context_for_pane(pane_id, cx).unwrap();
            assert_eq!(target.label, "Inventory · Database");
            let mut command = oxideterm_quick_commands::default_quick_commands()
                .into_iter()
                .find(|command| command.id == "qc-uptime")
                .unwrap();
            command
                .availability
                .protocols
                .push(oxideterm_quick_commands::QuickCommandTargetProtocol::Local);
            assert!(
                !oxideterm_quick_commands::quick_command_available_for_target(&command, &target)
            );
            command.availability.protocols.clear();
            // The detached shell's exit code proves the run action delivered input to this PTY.
            #[cfg(unix)]
            let (body, exit) = (
                "OXIDE_FIXTURE_EXIT=23",
                "exit \"${OXIDE_FIXTURE_EXIT:-1}\"\r",
            );
            #[cfg(windows)]
            let (body, exit) = ("set OXIDE_FIXTURE_EXIT=23", "exit %OXIDE_FIXTURE_EXIT%\r");
            command.command = body.into();
            ws.run_quick_command_model(&command, window, cx);
            assert!(
                ws.terminal
                    .read(cx)
                    .quick_commands
                    .pending_execution
                    .is_none()
            );
            ws.detach_active_local_terminal_from_palette(window, cx);
            terminal.lock().write_input(exit.as_bytes()).unwrap();
            (id, terminal, exited, ws.forwarding_runtime.clone())
        })
    });
    let wait = runtime.spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), exited.cancelled()).await
    });
    cx.foreground_executor()
        .block_test(wait)
        .unwrap()
        .expect("the provider instance must finish when its detached helper exits");
    assert_eq!(
        terminal.lock().lifecycle(),
        oxideterm_terminal::TerminalLifecycle::Exited(Some(23))
    );
    workspace.update(cx, |ws, cx| {
        ws.cleanup_dead_local_terminal_sessions_from_palette(cx);
        assert!(!ws.detached_local_terminals.contains_key(&session_id));
        assert_eq!(
            ws.plugin_entity
                .read(cx)
                .discovery_terminal_target(&terminal),
            None
        );
    });
}

#[gpui::test]
fn discovery_process_failure_preserves_only_handed_off_host_ssh(cx: &mut TestAppContext) {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let entity = cx.new(|cx| {
        PluginWorkspaceEntity::new(runtime, plugin_host::NativePluginRegistry::default(), cx)
    });
    entity.update(cx, |entity, _| {
        let plugin = "inventory";
        let issuer = CancellationToken::new();
        entity
            .discovery_lifetimes
            .insert(plugin.into(), issuer.clone());
        entity.active_runtime_plugin_ids.insert(plugin.into());
        entity.runtime_generations.insert(plugin.into(), 1);
        let mut register = |host_ssh: bool, connected: bool| {
            let origin = oxideterm_discovery::DiscoveryOrigin::new(
                "production".into(),
                "db".into(),
                uuid::Uuid::new_v4(),
            );
            if connected {
                origin.connected();
            }
            entity.discovery_instances.entries.insert(
                origin.instance_id.clone(),
                OpenInstance {
                    plugin: plugin.into(),
                    generation: 1,
                    provider: "hosts".into(),
                    source: "production".into(),
                    target: "db".into(),
                    cancel: origin.invalidation(),
                    terminal: None,
                    cleanup: None,
                    owner: Default::default(),
                    host_ssh: host_ssh.then(|| origin.observe_establishment()),
                },
            );
            origin
        };
        let delivered = register(true, true);
        let pending = register(true, false);
        let provider_owned = register(false, true);
        let revoked = register(true, true);
        let revoke = |origin: &oxideterm_discovery::DiscoveryOrigin| {
            plugin_runtime::PluginOutboundMessage::EmitEvent {
                event: plugin_runtime::PluginEvent {
                    name: "discovery.instanceInvalidated".into(),
                    payload: serde_json::json!({
                        "instanceId": origin.instance_id, "sourceId": "production", "providerId": "hosts"
                    }),
                }
            }
        };
        let action = CancellationToken::new();
        let (consumed, _) = tokio::sync::oneshot::channel();
        let intent = entity.apply_runtime_delivery(NativePluginRuntimeDelivery::Push {
            plugin_id: plugin.into(),
            generation: 1,
            source_action_authority: Some(action.clone()),
            consumed,
            result: Ok(plugin_runtime::NativePluginRuntimePush {
                messages: vec![revoke(&revoked)],
                effects: vec![plugin_runtime::PluginOutboundEffect::HostCall {
                    request_id: "tab".into(),
                    namespace: "ui".into(),
                    method: "openTab".into(),
                    args: serde_json::json!({"tabId":"credentials"}),
                }],
            }),
        });
        entity.runtime_intents.push_back(intent);
        action.cancel();
        assert!(!revoked.usable());
        let mut intents = entity.take_runtime_intents();
        assert!(matches!(
            intents.pop_front(),
            Some(PluginRuntimeIntent::DiscoveryInstanceInvalidated(id)) if id == revoked.instance_id
        ));
        assert!(matches!(
            intents.pop_front(),
            Some(PluginRuntimeIntent::ApplyEffects { effects, .. }) if effects.is_empty()
        ));
        entity.apply_runtime_push(
            plugin.into(), 1, Some(action),
            Err(plugin_runtime::PluginError::runtime("exit", "process exited")),
        );
        assert!(issuer.is_cancelled());
        assert!(!pending.usable());
        assert!(!provider_owned.usable());
        assert!(delivered.allows_target_operations());

        entity.runtime_generations.insert(plugin.into(), 2);
        entity.apply_plugin_message(plugin, Some(2), &revoke(&delivered)).unwrap();
        assert!(delivered.allows_target_operations());
        entity.start_runtime_deactivation(plugin.into(), RuntimeStop::Disabled);
        assert!(!delivered.usable());
    });
}

#[cfg(unix)]
#[test]
fn discovery_queue_preserves_other_sources_after_cancellation_and_provider_errors() {
    use oxideterm_plugin_protocol::PluginPermissionSet;
    use oxideterm_plugin_registry::NativePluginManifest;
    use std::{fs, os::unix::fs::PermissionsExt};

    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("plugin");
    fs::write(&entry, r#"#!/bin/sh
while IFS= read -r frame; do
  id=$(printf '%s\n' "$frame" | sed -n 's/.*"requestId":"\([^"]*\)".*/\1/p')
  case "$frame" in
    *'"command":"discovery.sourceAction"'*) printf '%s\n' "$frame" | sed -n 's/.*"actionId":"\([^"]*\)".*/\1/p' >> actions ;;
    *'"command":"discovery.list"'*)
      case "$frame" in
        *'"sourceId":"busy"'*) code=rateLimited ;;
        *'"sourceId":"collision"'*) code=invalidDiscoveryMessage ;;
        *) printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"ok","value":{"targets":[{"id":"vm","label":"Available"}]}}}}\n' "$id" "$id"; continue ;;
      esac
      printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"error","error":{"code":"%s","message":"Try later","recoverable":true}}}}\n' "$id" "$id" "$code"
      continue ;;
  esac
  printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"ok","value":{}}}}\n' "$id" "$id"
done
"#).unwrap();
    fs::set_permissions(&entry, fs::Permissions::from_mode(0o755)).unwrap();
    let manifest: NativePluginManifest = serde_json::from_value(serde_json::json!({
        "id":"example.assets","name":"Inventory","version":"1.0.0",
        "contributes":{"discoveryProviders":[{"id":"assets","title":"Assets","icon":"assets",
            "protocolVersion":1,"form":{},"launchKinds":["ssh"],"sourceActions":[{"id":"obsolete","title":"Old"},{"id":"current","title":"New"}]}]}
    }))
    .unwrap();
    let plugin = manifest.id.clone();
    let definition = manifest.contributes.as_ref().unwrap().discovery_providers[0].clone();
    let source = DiscoverySource {
        id: "source".into(),
        provider_id: definition.id.clone(),
        plugin_id: Some(plugin.clone()),
        ..Default::default()
    };
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    runtime.block_on(async {
        let mut host = plugin_runtime::NativePluginRuntimeHost::default();
        let activation = host
            .activate_process_plugin(
                manifest,
                dir.path().to_owned(),
                "plugin".into(),
                PluginPermissionSet {
                    capabilities: vec!["ui.write".into()],
                    ..Default::default()
                },
                Duration::from_secs(10),
            )
            .await
            .unwrap();
        let (delivery, _receiver) = delivery::ActiveDeliverySender::channel();
        let process = DiscoveryProcess {
            generation: activation.generation,
            live: CancellationToken::new(),
            plugin,
            definition,
            runtime: runtime.clone(),
            host: Arc::new(tokio::sync::Mutex::new(host)),
            delivery,
        };
        let busy = process.host.lock().await;
        let first = CancellationToken::new();
        let queued = process.source_action(&source, "obsolete", first.clone());
        tokio::pin!(queued);
        assert!(futures_util::poll!(queued.as_mut()).is_pending());
        tokio::task::yield_now().await;
        first.cancel();
        drop(busy);
        // Keep the old waiter alive: the queue must check the source authority before dispatch.
        process
            .source_action(&source, "current", CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(queued.await, Err(DiscoveryFailure::Unavailable));
        assert_eq!(
            fs::read_to_string(dir.path().join("actions")).unwrap(),
            "current\n"
        );
        let cancel = CancellationToken::new();
        for (source_id, code) in [("busy", "rateLimited"), ("collision", "invalidDiscoveryMessage")] {
            let failing = DiscoverySource { id: source_id.into(), ..source.clone() };
            assert!(matches!(process.list(&failing, "first", None, &cancel).await,
                Err(DiscoveryFailure::Provider(error)) if error.code == code && error.message == "Try later" && error.recoverable));
        }
        let page = process.list(&source, "next", None, &cancel).await.unwrap();
        assert!(matches!(&page.targets[..], [target] if target.id == "vm" && target.label == "Available"));
        process
            .host
            .lock()
            .await
            .deactivate_plugin(&process.plugin)
            .await
            .unwrap();
    });
}
