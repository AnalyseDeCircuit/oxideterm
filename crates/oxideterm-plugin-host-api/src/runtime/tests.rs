//! Runtime host tests cover the behavior moved out of the GPUI app crate.

use super::*;
use oxideterm_plugin_registry::{
    NativePluginRegistry, NativePluginRuntime, NativePluginRuntimeKind, native_plugins_dir,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
const PROCESS_RUNTIME_TEST_TIMEOUT_MS: u64 = 10_000;

#[cfg(unix)]
fn process_runtime_test_timeout() -> Duration {
    // Full workspace test runs can have many async/process-heavy tests active at
    // once, so process plugin protocol tests use a release-gate timeout that is
    // generous enough for scheduler load while still catching real deadlocks.
    Duration::from_millis(PROCESS_RUNTIME_TEST_TIMEOUT_MS)
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!("oxideterm-{name}-{millis}"))
}

fn sample_manifest() -> NativePluginManifest {
    NativePluginManifest {
        id: "com.example.runtime".to_string(),
        name: "Runtime".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        author: None,
        license: None,
        license_url: None,
        tags: None,
        main: None,
        engines: None,
        manifest_version: None,
        format: None,
        assets: None,
        styles: None,
        shared_dependencies: None,
        repository: None,
        checksum: None,
        contributes: None,
        locales: None,
        runtime: None,
        permissions: oxideterm_plugin_registry::NativePluginPermissions::default(),
    }
}

#[test]
fn process_decoder_classifies_sync_password_frames_before_typed_queues() {
    let frame = decode_process_output_frame(
        r#"{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"sync-1","namespace":"sync","method":"importOxide","args":{"password":"sensitive-value"}}}"#,
    )
    .unwrap();

    let PluginProcessFrame::Outbound(message) = frame else {
        panic!("sync host call should decode as an outbound message");
    };
    assert!(message.host_call_sensitivity().is_sensitive());
    assert!(!format!("{message:?}").contains("sensitive-value"));
}

#[test]
fn process_decoder_rejects_invalid_sensitive_frames_without_disclosure() {
    for (scenario, frame, code, recoverable) in [
        (
            "missing request ID",
            r#"{"protocolVersion":1,"payload":{"type":"callHostApi","namespace":"sync","method":"previewImport","args":{"password":"sensitive-value"}}}"#,
            "process_outbound_decode_failed",
            None,
        ),
        (
            "unknown protocol version",
            r#"{"protocolVersion":2,"payload":{"type":"callHostApi","requestId":"sync-1","namespace":"sync","method":"exportOxide","args":{"password":"sensitive-value"}}}"#,
            "unsupported_protocol_version",
            Some(false),
        ),
    ] {
        let error = decode_process_output_frame(frame).unwrap_err();
        assert_eq!(error.code, code, "{scenario}");
        if let Some(expected) = recoverable {
            assert_eq!(error.recoverable, expected, "{scenario}");
        }
        assert!(!error.message.contains("sensitive-value"), "{scenario}");
    }
}

#[test]
fn runtime_activate_request_uses_versioned_wire_fields() {
    let request = PluginRequest {
        request_id: "activate-1".to_string(),
        kind: PluginRequestKind::Activate {
            manifest: sample_manifest(),
            permissions: PluginPermissionSet {
                capabilities: vec!["plugin.invoke".to_string()],
                allowed_host_apis: vec!["ui.registerCommand".to_string()],
            },
        },
        timeout_ms: Some(5_000),
    };
    let envelope = PluginProtocolEnvelope::new(Some(request.request_id.clone()), request);
    let encoded = serde_json::to_value(envelope).unwrap();
    assert_eq!(encoded["protocolVersion"], 1);
    assert_eq!(encoded["requestId"], "activate-1");
    assert_eq!(encoded["payload"]["requestId"], "activate-1");
    assert_eq!(encoded["payload"]["timeoutMs"], 5_000);
    assert_eq!(encoded["payload"]["kind"]["type"], "activate");
    assert_eq!(
        encoded["payload"]["kind"]["manifest"]["id"],
        "com.example.runtime"
    );
    assert_eq!(
        encoded["payload"]["kind"]["permissions"],
        serde_json::json!({
            "capabilities": ["plugin.invoke"],
            "allowedHostApis": ["ui.registerCommand"],
        })
    );
}

#[test]
fn process_runtime_entry_is_confined_to_plugin_directory() {
    let temp_dir = unique_temp_dir("plugin-process-entry");
    let plugin_dir = temp_dir.join("plugin");
    let bin_dir = plugin_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join("plugin"), b"#!/bin/sh\n").unwrap();

    let resolved = resolve_process_runtime_entry(&plugin_dir, "bin/plugin").unwrap();
    assert!(resolved.starts_with(fs::canonicalize(&plugin_dir).unwrap()));
    let error = resolve_process_runtime_entry(&plugin_dir, "../outside").unwrap_err();
    assert_eq!(error.code, "invalid_process_entry");

    #[cfg(unix)]
    {
        let outside_dir = temp_dir.join("outside");
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(outside_dir.join("runner"), b"#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(outside_dir.join("runner"), plugin_dir.join("runner")).unwrap();

        let error = resolve_process_runtime_entry(&plugin_dir, "runner").unwrap_err();
        assert_eq!(error.code, "process_entry_escapes_plugin_dir");
    }
}

#[cfg(feature = "wasm-runtime")]
#[tokio::test]
async fn wasm_runtime_dispatches_command_and_event_over_memory_abi() {
    let temp_dir = unique_temp_dir("plugin-wasm-dispatch");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), wasm_dispatch_module()).unwrap();

    let mut host = NativePluginRuntimeHost::default();
    let manifest = sample_manifest();
    let activation = host
        .activate_wasm_plugin(
            manifest,
            plugin_dir,
            "plugin.wasm".to_string(),
            PluginPermissionSet::default(),
            Duration::from_millis(250),
        )
        .await
        .unwrap();
    assert!(matches!(
        activation.messages.as_slice(),
        [PluginOutboundMessage::Log { level: PluginRuntimeLogLevel::Info, message }]
            if message == "wasm activated"
    ));

    let command = host
        .dispatch_command(
            "com.example.runtime",
            "demo.run".to_string(),
            serde_json::json!({}),
            Duration::from_millis(250),
        )
        .await
        .unwrap();
    assert_eq!(
        command.response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "handled": true })
        }
    );

    let event = host
        .dispatch_event(
            "com.example.runtime",
            PluginEvent {
                name: "demo.event".to_string(),
                payload: serde_json::json!({}),
            },
            Duration::from_millis(250),
        )
        .await
        .unwrap();
    assert_eq!(
        event.response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "eventHandled": true })
        }
    );
}

#[cfg(feature = "wasm-runtime")]
fn wasm_dispatch_module() -> Vec<u8> {
    let command_response = r#"{"requestId":"command:com.example.runtime:demo.run","result":{"status":"ok","value":{"handled":true}}}"#;
    let event_response = r#"{"requestId":"event:demo.event","result":{"status":"ok","value":{"eventHandled":true}}}"#;
    let drain_response = r#"[{"type":"log","level":"info","message":"wasm activated"}]"#;
    let command_data = wat_data_string(command_response);
    let event_data = wat_data_string(event_response);
    let drain_data = wat_data_string(drain_response);
    let wat = format!(
        r#"
            (module
              (memory (export "memory") 1)
              (global $heap (mut i32) (i32.const 4096))
              (func (export "_start"))
              (func (export "oxideterm_plugin_alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
              (data (i32.const 1024) "{command_data}")
              (data (i32.const 2048) "{event_data}")
              (data (i32.const 3072) "{drain_data}")
              (func (export "oxideterm_plugin_command") (param i32 i32) (result i64)
                i64.const 1024
                i64.const 32
                i64.shl
                i64.const {command_len}
                i64.or)
              (func (export "oxideterm_plugin_event") (param i32 i32) (result i64)
                i64.const 2048
                i64.const 32
                i64.shl
                i64.const {event_len}
                i64.or)
              (func (export "oxideterm_plugin_drain_outbound") (result i64)
                i64.const 3072
                i64.const 32
                i64.shl
                i64.const {drain_len}
                i64.or))
            "#,
        command_len = command_response.len(),
        event_len = event_response.len(),
        drain_len = drain_response.len(),
    );
    wat::parse_str(wat).unwrap()
}

#[cfg(feature = "wasm-runtime")]
fn wat_data_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(not(feature = "wasm-runtime"))]
#[tokio::test]
async fn wasm_runtime_activation_reports_build_support_unavailable() {
    let mut host = NativePluginRuntimeHost::default();
    let error = host
        .activate_wasm_plugin(
            sample_manifest(),
            PathBuf::from("/tmp/plugin"),
            "plugin.wasm".to_string(),
            PluginPermissionSet::default(),
            Duration::from_millis(250),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code, WASM_RUNTIME_UNAVAILABLE_CODE);
    assert!(error.message.contains("not available"));
}

#[cfg(unix)]
fn write_process_plugin(plugin_dir: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = plugin_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let entry = bin_dir.join("plugin");
    fs::write(&entry, body).unwrap();
    let mut permissions = fs::metadata(&entry).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(entry, permissions).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_delivers_idle_pushes_and_rejects_commands_after_deactivation() {
    let plugin_dir = unique_temp_dir("plugin-idle-push");
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":null}}}'
while [ ! -f release ]; do sleep 0.01; done
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"live","pluginId":"com.example.runtime","kind":"tab","metadata":{"tabId":"messages","schema":{"kind":"form","controls":[{"kind":"table","id":"messages","rows":[{"topic":"devices/one","payload":"ready"}]}]}}}}}'
read request
printf '%s\n' '{"protocolVersion":1,"requestId":"command:com.example.runtime:publish","payload":{"requestId":"command:com.example.runtime:publish","result":{"status":"ok","value":{"sent":"devices/two"}}}}'
read request
"#,
    );
    let mut host = NativePluginRuntimeHost::default();
    let wake = host.output_wake();
    host.activate_process_plugin(
        sample_manifest(),
        plugin_dir.clone(),
        "bin/plugin".into(),
        PluginPermissionSet {
            capabilities: vec!["ui.write".into()],
            allowed_host_apis: vec![],
        },
        process_runtime_test_timeout(),
    )
    .await
    .unwrap();
    fs::write(plugin_dir.join("release"), b"").unwrap();
    let push = time::timeout(process_runtime_test_timeout(), async {
        loop {
            wake.notified().await;
            let mut pushes = host.receive_pending_output().await;
            if !pushes.is_empty() {
                break pushes.remove(0);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(push.0, "com.example.runtime");
    let push = push.2.unwrap();
    let PluginOutboundMessage::RegisterContribution { registration } = &push.messages[0] else {
        panic!("expected a live view update")
    };
    assert_eq!(
        registration.metadata["schema"]["controls"][0]["rows"],
        serde_json::json!([{"topic":"devices/one","payload":"ready"}])
    );
    assert_eq!(
        push.effects,
        vec![PluginOutboundEffect::RegistrationChanged]
    );
    let reply = host
        .dispatch_command(
            "com.example.runtime",
            "publish".into(),
            Value::Null,
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();
    assert_eq!(
        reply.response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({"sent":"devices/two"})
        }
    );
    host.deactivate_plugin("com.example.runtime").await.unwrap();
    assert_eq!(
        host.dispatch_command(
            "com.example.runtime",
            "publish".into(),
            Value::Null,
            process_runtime_test_timeout()
        )
        .await
        .unwrap_err()
        .code,
        "plugin_runtime_not_active"
    );
    fs::remove_dir_all(plugin_dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_rejects_unapproved_idle_contributions() {
    let plugin_dir = unique_temp_dir("plugin-idle-denied");
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":null}}}'
while [ ! -f release ]; do sleep 0.01; done
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"live","pluginId":"com.example.runtime","kind":"tab","metadata":{"tabId":"messages"}}}}'
read request
"#,
    );
    let mut host = NativePluginRuntimeHost::default();
    let wake = host.output_wake();
    host.activate_process_plugin(
        sample_manifest(),
        plugin_dir.clone(),
        "bin/plugin".into(),
        PluginPermissionSet::default(),
        process_runtime_test_timeout(),
    )
    .await
    .unwrap();
    fs::write(plugin_dir.join("release"), b"").unwrap();
    let result = time::timeout(process_runtime_test_timeout(), async {
        loop {
            wake.notified().await;
            let mut pushes = host.receive_pending_output().await;
            if !pushes.is_empty() {
                break pushes.remove(0).2;
            }
        }
    })
    .await
    .unwrap();
    let Err(error) = result else {
        panic!("unapproved UI push must be rejected")
    };
    assert_eq!(error.code, "plugin_capability_not_allowed");
    assert!(host.process_runtimes["com.example.runtime"].child.is_none());
    fs::remove_dir_all(plugin_dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn process_runtime_collects_activate_time_outbound_frames() {
    let temp_dir = unique_temp_dir("plugin-process-outbound");
    let plugin_dir = temp_dir.join("plugin");
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"cmd-1","pluginId":"com.example.runtime","kind":"command","metadata":{"id":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"log","level":"info","message":"registered command"}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        process_runtime_test_timeout(),
    );
    let response = runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: PROCESS_RUNTIME_TEST_TIMEOUT_MS,
        })
        .await
        .unwrap();

    assert!(matches!(response.result, PluginResponseResult::Ok { .. }));
    assert_eq!(
        runtime.health().await.unwrap().state,
        PluginRuntimeLifecycleState::Active
    );
    assert_eq!(runtime.supervisor.registration_count(), 1);
    assert_eq!(runtime.supervisor.log_count(), 1);
    let messages = runtime.drain_outbound_messages();
    assert_eq!(messages.len(), 2);
    assert!(matches!(
        messages[0],
        PluginOutboundMessage::RegisterContribution { .. }
    ));
    let effects = runtime.drain_outbound_effects();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0], PluginOutboundEffect::RegistrationChanged);
    assert!(runtime.drain_outbound_messages().is_empty());
    assert!(runtime.drain_outbound_effects().is_empty());
    runtime.kill().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn process_runtime_exposes_host_call_effects_for_workspace_dispatch() {
    let temp_dir = unique_temp_dir("plugin-process-host-call");
    let plugin_dir = temp_dir.join("plugin");
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-1","namespace":"ui","method":"showToast","args":{"title":"Plugin ready","variant":"success"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        process_runtime_test_timeout(),
    );
    runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet {
                capabilities: Vec::new(),
                allowed_host_apis: vec!["ui.showToast".to_string()],
            },
            timeout_ms: PROCESS_RUNTIME_TEST_TIMEOUT_MS,
        })
        .await
        .unwrap();

    let effects = runtime.drain_outbound_effects();
    assert_eq!(
        effects[0],
        PluginOutboundEffect::HostCall {
            request_id: "host-1".to_string(),
            namespace: "ui".to_string(),
            method: "showToast".to_string(),
            args: serde_json::json!({
                "title": "Plugin ready",
                "variant": "success",
            }),
        }
    );
    runtime.kill().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_activates_process_plugin_applies_registry_and_cleans_on_deactivate() {
    let temp_dir = unique_temp_dir("plugin-runtime-host-process");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("runtime");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = sample_manifest();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Process,
        entry: "bin/plugin".to_string(),
    });
    fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"cmd-1","pluginId":"com.example.runtime","kind":"command","metadata":{"id":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-1","namespace":"ui","method":"showToast","args":{"title":"Plugin ready","variant":"success"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
"#,
    );

    let mut registry = NativePluginRegistry::discover(&settings_path);
    let mut host = NativePluginRuntimeHost::default();
    let activation = host
        .activate_process_plugin(
            manifest,
            plugin_dir,
            "bin/plugin".to_string(),
            PluginPermissionSet {
                capabilities: Vec::new(),
                allowed_host_apis: vec!["ui.showToast".to_string()],
            },
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();
    for message in &activation.messages {
        registry
            .apply_runtime_outbound_message(&activation.plugin_id, message)
            .unwrap();
    }

    assert!(matches!(
        activation.response.result,
        PluginResponseResult::Ok { .. }
    ));
    assert_eq!(registry.contributions().runtime_commands.len(), 1);
    assert_eq!(
        registry.contributions().runtime_commands[0].command,
        "demo.run"
    );
    assert!(activation.effects.iter().any(|effect| matches!(
        effect,
        PluginOutboundEffect::HostCall { method, .. } if method == "showToast"
    )));

    host.deactivate_plugin("com.example.runtime").await.unwrap();
    registry.cleanup_runtime_plugin_contributions("com.example.runtime");
    assert!(registry.contributions().runtime_commands.is_empty());
}

#[tokio::test]
async fn runtime_host_deactivation_always_releases_permission_snapshots() {
    let mut host = NativePluginRuntimeHost::default();
    host.process_permissions.insert(
        "com.example.runtime".to_string(),
        PluginPermissionSet {
            capabilities: Vec::new(),
            allowed_host_apis: vec!["ui.showToast".to_string()],
        },
    );

    host.deactivate_plugin("com.example.runtime").await.unwrap();

    assert!(!host.process_permissions.contains_key("com.example.runtime"));
}

#[cfg(unix)]
#[tokio::test]
async fn discovery_dispatches_declared_actions_and_pages_with_their_own_config() {
    use oxideterm_plugin_protocol::discovery::{DiscoveryReply, DiscoveryRequest};
    let dir = unique_temp_dir("process-discovery-pages");
    write_process_plugin(
        &dir,
        r#"#!/bin/sh
read -r activate
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{}}}}'
previous=''
while IFS= read -r frame; do
  id=$(printf '%s\n' "$frame" | sed -n 's/.*"requestId":"\([^"]*\)".*/\1/p')
  case "$frame" in
    *'"command":"discovery.sourceAction"'*)
      case "$frame" in *'"actionId":"inspect"'*) ;; *) exit 1 ;; esac
      case "$frame" in *'"sourceId":"source-2"'*) ;; *) exit 1 ;; esac
      case "$frame" in *'"root/site":"test"'*) ;; *) exit 1 ;; esac
      case "$frame" in *'"root/login":"operator"'*) ;; *) exit 1 ;; esac
      case "$frame" in *'private-extra'*|*'"refreshId"'*) exit 1 ;; esac
      printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"error","error":{"code":"permissionDenied","message":"Choose an inventory role","recoverable":true}}}}\n' "$id" "$id"
      continue ;;
    *'"command":"discovery.list"'*) ;;
    *) exit 1 ;;
  esac
  case "$frame" in *'"root/site":"prod"'*) ;; *) exit 1 ;; esac
  case "$frame" in *'"sourceId":"source-1"'*) ;; *) exit 1 ;; esac
  case "$frame" in *'login-only'*|*'private-extra'*) exit 1 ;; esac
  if [ -z "$previous" ]; then
    value='{"targets":[{"id":"one","label":"First"}],"nextCursor":"remaining"}'
  else
    [ "$id" != "$previous" ] || exit 1
    case "$frame" in *'"cursor":"remaining"'*) ;; *) exit 1 ;; esac
    value='{"targets":[{"id":"two","label":"Second"}]}'
  fi
  printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"ok","value":%s}}}\n' "$id" "$id" "$value"
  previous=$id
done
"#,
    );
    let mut manifest = sample_manifest();
    manifest.contributes = Some(serde_json::from_value(serde_json::json!({
        "discoveryProviders":[{"id":"inventory","title":"Inventory","icon":"server","protocolVersion":1,"launchKinds":["ssh"],
            "sourceActions":[{"id":"inspect","title":"@inspect"}],
            "form":{"controls":[{"id":"site","kind":"text","use":"catalog"},{"id":"login","kind":"text","use":"open"}]}}]
    })).unwrap());
    let mut host = NativePluginRuntimeHost::default();
    let timeout = process_runtime_test_timeout();
    let activation = host
        .activate_process_plugin(
            manifest,
            dir.clone(),
            "bin/plugin".into(),
            PluginPermissionSet {
                capabilities: vec!["ui.write".into()],
                ..Default::default()
            },
            timeout,
        )
        .await
        .unwrap();
    // Undeclared actions must be refused before contacting the shared process.
    let (refused, _) = host
        .dispatch_discovery(
            "com.example.runtime",
            activation.generation,
            "source-2",
            "inventory",
            DiscoveryRequest::SourceAction {
                action_id: "unregistered".into(),
                config: serde_json::json!({}),
            },
            timeout,
        )
        .await
        .unwrap();
    assert!(
        matches!(refused, Err(DiscoveryCommandError::Rejected(error)) if error.code == "unsupported")
    );
    let (result, _) = host
        .dispatch_discovery(
            "com.example.runtime",
            activation.generation,
            "source-2",
            "inventory",
            DiscoveryRequest::SourceAction {
                action_id: "inspect".into(),
                config: serde_json::json!({"root/site":"test","root/login":"operator","unknown":"private-extra"}),
            },
            timeout,
        )
        .await
        .unwrap();
    assert!(matches!(result, Err(DiscoveryCommandError::Provider(error))
        if error.code == "permissionDenied" && error.message == "Choose an inventory role" && error.recoverable));
    let mut cursor = None;
    for (id, label, next) in [("one", "First", Some("remaining")), ("two", "Second", None)] {
        let (reply, _) = host
            .dispatch_discovery(
                "com.example.runtime",
                activation.generation,
                "source-1",
                "inventory",
                DiscoveryRequest::List {
                    refresh_id: "round".into(),
                    cursor,
                    config: serde_json::json!({"root/site":"prod","root/login":"login-only","unknown":"private-extra"}),
                },
                timeout,
            )
            .await
            .unwrap();
        let DiscoveryReply::List(page) = reply.unwrap() else {
            panic!("expected a discovery page")
        };
        assert_eq!(
            page.targets
                .iter()
                .map(|target| (target.id.as_str(), target.label.as_str()))
                .collect::<Vec<_>>(),
            [(id, label)]
        );
        assert_eq!(page.next_cursor.as_deref(), next);
        cursor = page.next_cursor;
    }
    host.deactivate_plugin("com.example.runtime").await.unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn discovery_revocation_precedes_blocked_reply_and_shutdown_closes_provider_resource() {
    use oxideterm_plugin_protocol::discovery::{
        DiscoveryOpen, DiscoveryOwner, DiscoveryReply, DiscoveryRequest,
    };
    let dir = unique_temp_dir("discovery-revocation");
    write_process_plugin(
        &dir,
        r#"#!/bin/sh
mkfifo receipt
while IFS= read -r frame; do
  id=$(printf '%s\n' "$frame" | sed -n 's/.*"requestId":"\([^"]*\)".*/\1/p')
  case "$frame" in
    *'"type":"activate"'*) value='{}' ;;
    *'"command":"discovery.prepareOpen"'*)
      value='{"kind":"ready","owner":"provider","launch":{"kind":"ssh","endpoint":{"host":"vm.example","port":22}}}'
      case "$frame" in
        *'"instanceId":"invalid"'*) value='{"kind":"ready","owner":"provider","launch":{"kind":"ssh"}}' ;;
        *) opened_id=$id ;;
      esac
      ;;
    *'"command":"discovery.list"'*)
      printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"ok","value":{"kind":"ready","owner":"provider","launch":{"kind":"ssh","endpoint":{"host":"vm.example","port":22}}}}}}\n' "$opened_id" "$opened_id"
      printf '%s\n' '{"protocolVersion":1,"payload":{"type":"emitEvent","event":{"name":"discovery.instanceInvalidated","payload":{"sourceId":"s","providerId":"p","instanceId":"opened"}}}}'
      read -r acknowledged < receipt
      value="{\"targets\":[{\"id\":\"observed\",\"label\":\"$acknowledged\"}]}"
      ;;
    *'"command":"discovery.close"'*)
      case "$frame" in
        *'"instanceId":"opened"'*) printf 'opened' > released ;;
        *'"instanceId":"invalid"'*) printf 'invalid' > rejected-resource ;;
        *) exit 1 ;;
      esac
      value='{}' ;;
    *) exit 1 ;;
  esac
  printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"ok","value":%s}}}\n' "$id" "$id" "$value"
done
"#,
    );
    let mut manifest = sample_manifest();
    manifest.contributes=Some(serde_json::from_value(serde_json::json!({"discoveryProviders":[{"id":"p","title":"Inventory","icon":"server","protocolVersion":1,"launchKinds":["ssh"],"form":{"controls":[{"kind":"text","id":"site"}]}}]})).unwrap());
    let timeout = process_runtime_test_timeout();
    let mut host = NativePluginRuntimeHost::default();
    let generation = host
        .activate_process_plugin(
            manifest,
            dir.clone(),
            "bin/plugin".into(),
            PluginPermissionSet {
                capabilities: vec!["ui.write".into()],
                ..Default::default()
            },
            timeout,
        )
        .await
        .unwrap()
        .generation;
    let receipt = dir.join("receipt");
    host.set_discovery_invalidation_sink(
        "com.example.runtime",
        generation,
        Arc::new(move |event| {
            let identity = event.payload["instanceId"].as_str().unwrap();
            fs::write(&receipt, format!("{identity}\n")).unwrap();
        }),
    );
    let open = |instance: &str| DiscoveryRequest::PrepareOpen {
        instance_id: instance.into(),
        target_id: "vm".into(),
        config: serde_json::json!({}),
        continuation: None,
        option_id: None,
    };
    let (response, _) = host
        .dispatch_discovery(
            "com.example.runtime",
            generation,
            "s",
            "p",
            open("opened"),
            timeout,
        )
        .await
        .unwrap();
    assert!(matches!(
        response,
        Ok(DiscoveryReply::Open(DiscoveryOpen::Ready {
            owner: DiscoveryOwner::Provider,
            ..
        }))
    ));
    let (rejected, _) = host
        .dispatch_discovery(
            "com.example.runtime",
            generation,
            "s",
            "p",
            open("invalid"),
            timeout,
        )
        .await
        .unwrap();
    assert!(
        matches!(rejected, Err(DiscoveryCommandError::Rejected(error)) if error.code == "invalidDiscoveryMessage")
    );
    let (cleaned, _) = host
        .dispatch_discovery(
            "com.example.runtime",
            generation,
            "s",
            "p",
            DiscoveryRequest::CancelOpen {
                instance_id: "invalid".into(),
            },
            timeout,
        )
        .await
        .unwrap();
    assert!(matches!(cleaned, Ok(DiscoveryReply::Done)));
    assert_eq!(
        fs::read_to_string(dir.join("rejected-resource")).unwrap(),
        "invalid"
    );
    let list = DiscoveryRequest::List {
        refresh_id: "round".into(),
        cursor: None,
        config: serde_json::json!({}),
    };
    let (reply, _) = host
        .dispatch_discovery("com.example.runtime", generation, "s", "p", list, timeout)
        .await
        .unwrap();
    assert!(matches!(reply,Ok(DiscoveryReply::List(page)) if page.targets[0].label=="opened"));
    host.deactivate_plugin("com.example.runtime").await.unwrap();
    assert_eq!(fs::read_to_string(dir.join("released")).unwrap(), "opened");
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn discovery_slow_provider_does_not_delay_another_process() {
    use oxideterm_plugin_protocol::discovery::{DiscoveryReply, DiscoveryRequest};
    let directory = unique_temp_dir("independent-discovery-processes");
    write_process_plugin(
        &directory,
        r#"#!/bin/sh
while IFS= read -r frame; do
  id=$(printf '%s\n' "$frame" | sed -n 's/.*"requestId":"\([^"]*\)".*/\1/p')
  case "$frame" in
    *'"sourceId":"slow"'*)
      printf '%s\n' '{"protocolVersion":1,"payload":{"type":"emitEvent","event":{"name":"discovery.instanceInvalidated","payload":{"sourceId":"slow","providerId":"inventory","instanceId":"in-flight"}}}}'
      while [ ! -f release ]; do sleep 0.01; done
      value='{"targets":[{"id":"first","label":"First"}]}' ;;
    *'"sourceId":"fast"'*) value='{"targets":[{"id":"second","label":"Second"}]}' ;;
    *) value='{}' ;;
  esac
  printf '{"protocolVersion":1,"requestId":"%s","payload":{"requestId":"%s","result":{"status":"ok","value":%s}}}\n' "$id" "$id" "$value"
done
"#,
    );
    let mut hosts = NativePluginRuntimeHosts::default();
    let timeout = process_runtime_test_timeout();
    let mut generations = Vec::new();
    for id in ["example.slow", "example.fast"] {
        let mut manifest = sample_manifest();
        manifest.id = id.into();
        manifest.contributes = Some(serde_json::from_value(serde_json::json!({
            "discoveryProviders":[{"id":"inventory","title":"Inventory","icon":"server","protocolVersion":1,"launchKinds":["ssh"],"form":{}}]
        })).unwrap());
        hosts.prepare_process(&manifest);
        let host = hosts.for_plugin(id);
        let activation = host
            .lock()
            .await
            .activate_process_plugin(
                manifest,
                directory.clone(),
                "bin/plugin".into(),
                PluginPermissionSet {
                    capabilities: vec!["ui.write".into()],
                    ..Default::default()
                },
                timeout,
            )
            .await
            .unwrap();
        generations.push(activation.generation);
    }
    let (entered, mut waiting) = tokio::sync::mpsc::unbounded_channel();
    hosts
        .for_plugin("example.slow")
        .lock()
        .await
        .set_discovery_invalidation_sink(
            "example.slow",
            generations[0],
            Arc::new(move |event| {
                entered.send(event.payload["instanceId"].clone()).unwrap();
            }),
        );
    let enumerate = |id: &'static str, source: &'static str, generation| {
        let host = hosts.for_plugin(id);
        async move {
            let (reply, _) = host
                .lock()
                .await
                .dispatch_discovery(
                    id,
                    generation,
                    source,
                    "inventory",
                    DiscoveryRequest::List {
                        refresh_id: "round".into(),
                        config: serde_json::json!({}),
                        cursor: None,
                    },
                    timeout,
                )
                .await
                .unwrap();
            let DiscoveryReply::List(page) = reply.unwrap() else {
                panic!("expected a catalog page")
            };
            page.targets
                .into_iter()
                .map(|target| target.id)
                .collect::<Vec<_>>()
        }
    };
    let (slow, fast) = time::timeout(timeout, async {
        tokio::join!(enumerate("example.slow", "slow", generations[0]), async {
            assert_eq!(waiting.recv().await, Some(serde_json::json!("in-flight")));
            let fast = enumerate("example.fast", "fast", generations[1]).await;
            fs::write(directory.join("release"), []).unwrap();
            fast
        })
    })
    .await
    .expect("the second process must finish while the first is blocked");
    assert_eq!(slow, ["first"]);
    assert_eq!(fast, ["second"]);
    for id in ["example.slow", "example.fast"] {
        hosts
            .for_plugin(id)
            .lock()
            .await
            .deactivate_plugin(id)
            .await
            .unwrap();
    }
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_dispatches_registered_command_over_process_rpc() {
    let temp_dir = unique_temp_dir("plugin-runtime-host-dispatch-command");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"cmd-1","pluginId":"com.example.runtime","kind":"command","metadata":{"id":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-2","namespace":"ui","method":"showToast","args":{"title":"Command ran","variant":"success"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"command:com.example.runtime:demo.run","payload":{"requestId":"command:com.example.runtime:demo.run","result":{"status":"ok","value":{"handled":true}}}}'
"#,
    );

    let mut host = NativePluginRuntimeHost::default();
    host.activate_process_plugin(
        sample_manifest(),
        plugin_dir,
        "bin/plugin".to_string(),
        PluginPermissionSet {
            capabilities: Vec::new(),
            allowed_host_apis: vec!["ui.showToast".to_string()],
        },
        process_runtime_test_timeout(),
    )
    .await
    .unwrap();

    let dispatch = host
        .dispatch_command(
            "com.example.runtime",
            "demo.run".to_string(),
            Value::Null,
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();

    assert_eq!(dispatch.command, "demo.run");
    assert!(matches!(
        dispatch.response.result,
        PluginResponseResult::Ok { .. }
    ));
    assert!(dispatch.effects.iter().any(|effect| matches!(
        effect,
        PluginOutboundEffect::HostCall { method, .. } if method == "showToast"
    )));
    host.deactivate_plugin("com.example.runtime").await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_dispatches_subscription_event_over_process_rpc() {
    let temp_dir = unique_temp_dir("plugin-runtime-host-dispatch-event");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"theme-sub-1","pluginId":"com.example.runtime","kind":"event-subscription","metadata":{"event":"app.themeChanged"}}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read event_request
case "$event_request" in
  *'"name":"app.themeChanged"'*) result='{"status":"ok","value":{"received":true}}' ;;
  *) result='{"status":"error","error":{"code":"bad_event","message":"missing event","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"event:app.themeChanged\",\"payload\":{\"requestId\":\"event:app.themeChanged\",\"result\":$result}}"
"#,
    );

    let mut host = NativePluginRuntimeHost::default();
    let activation = host
        .activate_process_plugin(
            sample_manifest(),
            plugin_dir,
            "bin/plugin".to_string(),
            PluginPermissionSet::default(),
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();
    assert!(activation.messages.iter().any(|message| {
        matches!(
            message,
            PluginOutboundMessage::RegisterContribution { registration }
                if registration.kind == PluginRegistrationKind::EventSubscription
        )
    }));

    let dispatch = host
        .dispatch_event(
            "com.example.runtime",
            PluginEvent {
                name: "app.themeChanged".to_string(),
                payload: serde_json::json!({
                    "theme": {
                        "name": "azurite",
                        "isDark": true,
                    }
                }),
            },
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();

    assert_eq!(dispatch.event.name, "app.themeChanged");
    assert_eq!(
        dispatch.response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "received": true })
        }
    );
    host.deactivate_plugin("com.example.runtime").await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn process_runtime_does_not_retain_secret_host_call_copies() {
    let temp_dir = unique_temp_dir("plugin-process-secret-host-call");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-secret-set","namespace":"secrets","method":"set","args":{"key":"token","value":"plugin-sensitive-value"}}}'
read host_response
case "$host_response" in
  *'"host-sensitive-value"'*) result='{"status":"ok","value":{"received":true}}' ;;
  *) result='{"status":"error","error":{"code":"bad_host_response","message":"missing host value","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"command:demo.secret\",\"payload\":{\"requestId\":\"command:demo.secret\",\"result\":$result}}"
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        process_runtime_test_timeout(),
    );
    runtime.set_host_call_handler(Box::new(|call| {
        assert_eq!(call.namespace, "secrets");
        assert_eq!(call.method, "set");
        assert_eq!(call.args["value"], "plugin-sensitive-value");
        Some(PluginResponse::sensitive_ok(
            call.request_id,
            serde_json::json!("host-sensitive-value"),
        ))
    }));
    runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: PROCESS_RUNTIME_TEST_TIMEOUT_MS,
        })
        .await
        .unwrap();

    let response = runtime
        .call(PluginRequest {
            request_id: "command:demo.secret".to_string(),
            kind: PluginRequestKind::DispatchCommand {
                command: "demo.secret".to_string(),
                args: Value::Null,
            },
            timeout_ms: Some(2_000),
        })
        .await
        .unwrap();

    assert_eq!(
        response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "received": true }),
        }
    );
    // Secret frames are consumed synchronously and must never reach retained
    // audit queues that outlive the request/response exchange.
    assert!(runtime.drain_outbound_messages().is_empty());
    assert!(runtime.drain_outbound_effects().is_empty());
    runtime.kill().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn process_runtime_does_not_retain_sync_password_host_call_copies() {
    let temp_dir = unique_temp_dir("plugin-process-sync-password-host-call");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-sync-import","namespace":"sync","method":"importOxide","args":{"password":"plugin-sensitive-value","fileData":[]}}}'
read host_response
printf '%s\n' '{"protocolVersion":1,"requestId":"command:demo.sync","payload":{"requestId":"command:demo.sync","result":{"status":"ok","value":{"received":true}}}}'
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        process_runtime_test_timeout(),
    );
    runtime.set_host_call_handler(Box::new(|mut call| {
        assert_eq!(call.namespace, "sync");
        assert_eq!(call.method, "importOxide");
        assert_eq!(call.args["password"], "plugin-sensitive-value");
        let request_id = std::mem::take(&mut call.request_id);
        call.zeroize_args();
        Some(PluginResponse::ok(request_id, Value::Null))
    }));
    runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: PROCESS_RUNTIME_TEST_TIMEOUT_MS,
        })
        .await
        .unwrap();

    runtime
        .call(PluginRequest {
            request_id: "command:demo.sync".to_string(),
            kind: PluginRequestKind::DispatchCommand {
                command: "demo.sync".to_string(),
                args: Value::Null,
            },
            timeout_ms: Some(2_000),
        })
        .await
        .unwrap();

    assert!(runtime.drain_outbound_messages().is_empty());
    assert!(runtime.drain_outbound_effects().is_empty());
    runtime.kill().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_installs_returnable_host_call_resolver_for_commands() {
    let temp_dir = unique_temp_dir("plugin-runtime-host-returnable-host-call");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-storage-get","namespace":"storage","method":"get","args":{"key":"recent"}}}'
read host_response
case "$host_response" in
  *'"stored"'*) result='{"status":"ok","value":{"read":true}}' ;;
  *) result='{"status":"error","error":{"code":"bad_host_response","message":"missing host value","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"command:com.example.runtime:demo.read\",\"payload\":{\"requestId\":\"command:com.example.runtime:demo.read\",\"result\":$result}}"
"#,
    );

    let mut host = NativePluginRuntimeHost::default();
    host.set_host_api_resolver(Arc::new(|plugin_id, _permissions, call| {
        assert_eq!(plugin_id, "com.example.runtime");
        assert_eq!(call.namespace, "storage");
        assert_eq!(call.method, "get");
        Some(PluginResponse::ok(
            call.request_id,
            serde_json::json!("stored"),
        ))
    }));
    host.activate_process_plugin(
        sample_manifest(),
        plugin_dir,
        "bin/plugin".to_string(),
        PluginPermissionSet {
            capabilities: Vec::new(),
            allowed_host_apis: vec!["storage.get".to_string()],
        },
        process_runtime_test_timeout(),
    )
    .await
    .unwrap();

    let dispatch = host
        .dispatch_command(
            "com.example.runtime",
            "demo.read".to_string(),
            Value::Null,
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();

    assert_eq!(
        dispatch.response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "read": true })
        }
    );
    assert!(dispatch.effects.iter().any(|effect| {
        matches!(
            effect,
            PluginOutboundEffect::HostCall {
                namespace,
                method,
                ..
            } if namespace == "storage" && method == "get"
        )
    }));
    host.deactivate_plugin("com.example.runtime").await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_accepts_keybinding_registration_and_dispatches_its_command() {
    let temp_dir = unique_temp_dir("plugin-runtime-host-keybinding-command");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("runtime");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = sample_manifest();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Process,
        entry: "bin/plugin".to_string(),
    });
    fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"registerContribution","registration":{"registrationId":"key-1","pluginId":"com.example.runtime","kind":"keybinding","metadata":{"keybinding":"Cmd+Shift+R","command":"demo.run","label":"Run Demo"}}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"requestId":"command:com.example.runtime:demo.run","payload":{"requestId":"command:com.example.runtime:demo.run","result":{"status":"ok","value":{"handled":true}}}}'
"#,
    );

    let mut registry = NativePluginRegistry::discover(&settings_path);
    let mut host = NativePluginRuntimeHost::default();
    let activation = host
        .activate_process_plugin(
            manifest,
            plugin_dir,
            "bin/plugin".to_string(),
            PluginPermissionSet::default(),
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();
    for message in &activation.messages {
        registry
            .apply_runtime_outbound_message(&activation.plugin_id, message)
            .unwrap();
    }

    assert_eq!(registry.contributions().runtime_keybindings.len(), 1);
    assert_eq!(
        registry.contributions().runtime_keybindings[0].keybinding,
        "Cmd+Shift+R"
    );
    let dispatch = host
        .dispatch_command(
            "com.example.runtime",
            registry.contributions().runtime_keybindings[0]
                .command
                .clone(),
            Value::Null,
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();
    assert!(matches!(
        dispatch.response.result,
        PluginResponseResult::Ok { .. }
    ));
    host.deactivate_plugin("com.example.runtime").await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_host_returns_permission_error_without_retaining_sensitive_call() {
    let temp_dir = unique_temp_dir("plugin-runtime-host-denied-host-call");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-1","namespace":"secrets","method":"get","args":{"key":"token"}}}'
read host_response
case "$host_response" in
  *'"host_api_not_allowed"'*) result='{"status":"ok","value":{"denied":true}}' ;;
  *) result='{"status":"error","error":{"code":"missing_permission_error","message":"permission error was not returned","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"activate:com.example.runtime\",\"payload\":{\"requestId\":\"activate:com.example.runtime\",\"result\":$result}}"
"#,
    );

    let mut host = NativePluginRuntimeHost::default();
    host.set_host_api_resolver(Arc::new(|_, _, _| {
        panic!("permission denial must not reach the host API resolver")
    }));
    let activation = host
        .activate_process_plugin(
            sample_manifest(),
            plugin_dir,
            "bin/plugin".to_string(),
            PluginPermissionSet {
                capabilities: Vec::new(),
                allowed_host_apis: vec!["ui.showToast".to_string()],
            },
            process_runtime_test_timeout(),
        )
        .await
        .unwrap();

    assert!(matches!(
        activation.response.result,
        PluginResponseResult::Ok { .. }
    ));
    assert!(activation.messages.is_empty());
    assert!(activation.effects.is_empty());
    let health = host.deactivate_plugin("com.example.runtime").await.unwrap();
    assert!(matches!(health.result, PluginResponseResult::Ok { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn process_runtime_rejects_unknown_response_protocol_version() {
    let temp_dir = unique_temp_dir("plugin-process-bad-version");
    let plugin_dir = temp_dir.join("plugin");
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":2,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{}}}}'
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        process_runtime_test_timeout(),
    );
    let error = runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: PROCESS_RUNTIME_TEST_TIMEOUT_MS,
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, "unsupported_protocol_version");
    assert_eq!(runtime.child.is_none(), true);
    assert_eq!(
        runtime.supervisor.state(),
        PluginRuntimeLifecycleState::Error
    );
}

#[cfg(unix)]
#[tokio::test]
async fn process_runtime_cleans_up_when_activate_process_exits() {
    let temp_dir = unique_temp_dir("plugin-process-exits");
    let plugin_dir = temp_dir.join("plugin");
    // Consume activation so this tests response EOF, not a race with stdin writes.
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
exit 0
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        process_runtime_test_timeout(),
    );
    let error = runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: PROCESS_RUNTIME_TEST_TIMEOUT_MS,
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, "process_exited");
    assert!(runtime.child.is_none());
    assert_eq!(
        runtime.supervisor.state(),
        PluginRuntimeLifecycleState::Error
    );
}

#[cfg(unix)]
#[tokio::test]
async fn process_runtime_activate_timeout_moves_runtime_to_error_state() {
    let temp_dir = unique_temp_dir("plugin-process-timeout");
    let plugin_dir = temp_dir.join("plugin");
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
sleep 2
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        Duration::from_millis(50),
    );
    let error = runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: 50,
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, "process_response_timeout");
    assert!(runtime.child.is_none());
    assert_eq!(
        runtime.supervisor.state(),
        PluginRuntimeLifecycleState::Error
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_process_batch_entry_completes_activation_handshake() {
    let plugin_dir = unique_temp_dir("batch plugin & handshake");
    fs::create_dir_all(&plugin_dir).unwrap();
    let entry = "plugin.cmd";
    fs::write(
        plugin_dir.join(entry),
        concat!(
            "@echo off\r\n",
            "set \"request=\"\r\n",
            "set /p request=\r\n",
            "if not defined request exit /b 1\r\n",
            "echo {\"protocolVersion\":1,\"requestId\":\"batch-activate\",\"payload\":{\"requestId\":\"batch-activate\",\"result\":{\"status\":\"ok\",\"value\":{\"entry\":\"plugin.cmd\"}}}}\r\n",
        ),
    )
    .unwrap();

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        entry,
        Duration::from_secs(10),
    );
    let request = || PluginActivateRequest {
        request_id: "batch-activate".to_string(),
        manifest: sample_manifest(),
        permissions: PluginPermissionSet::default(),
        timeout_ms: 10_000,
    };

    let response = runtime.activate(request()).await.unwrap();
    let PluginResponseResult::Ok { value } = response.result else {
        panic!("batch entry did not complete the activation handshake");
    };
    assert_eq!(value, serde_json::json!({ "entry": "plugin.cmd" }));
    assert_eq!(
        runtime.health().await.unwrap().state,
        PluginRuntimeLifecycleState::Active
    );
    runtime.kill().await.unwrap();

    // A sibling executable must not silently take precedence over the manifest entry.
    fs::write(plugin_dir.join("plugin.cmd.exe"), b"not the manifest entry").unwrap();
    let error = runtime.activate(request()).await.unwrap_err();
    assert_eq!(error.code, "process_spawn_failed");
    assert_eq!(
        error.message,
        "A sibling .exe shadows the plugin batch entry"
    );
    fs::remove_dir_all(plugin_dir).unwrap();
}
