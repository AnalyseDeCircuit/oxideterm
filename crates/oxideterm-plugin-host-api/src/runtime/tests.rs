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
    }
}

#[test]
fn protocol_envelope_rejects_unknown_version() {
    let envelope = PluginProtocolEnvelope {
        protocol_version: NATIVE_PLUGIN_PROTOCOL_VERSION + 1,
        request_id: Some("req-1".to_string()),
        payload: PluginEvent {
            name: "demo".to_string(),
            payload: Value::Null,
        },
    };

    let error = envelope.validate_version().unwrap_err();
    assert_eq!(error.code, "unsupported_protocol_version");
    assert!(!error.recoverable);
}

#[test]
fn runtime_request_round_trips_as_versioned_json() {
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
    let encoded = serde_json::to_string(&envelope).unwrap();
    let decoded: PluginProtocolEnvelope<PluginRequest> = serde_json::from_str(&encoded).unwrap();

    decoded.validate_version().unwrap();
    assert_eq!(decoded.request_id.as_deref(), Some("activate-1"));
    assert!(matches!(
        decoded.payload.kind,
        PluginRequestKind::Activate { .. }
    ));
}

#[test]
fn response_helpers_and_supervisor_lifecycle_state_are_covered() {
    let ok = PluginResponse::ok("req-ok", serde_json::json!({ "done": true }));
    assert!(matches!(ok.result, PluginResponseResult::Ok { .. }));
    let error = PluginResponse::error("req-error", PluginError::runtime("boom", "failed"));
    assert!(matches!(error.result, PluginResponseResult::Error { .. }));

    let mut supervisor =
        PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_millis(250));
    assert_eq!(supervisor.lifecycle_timeout(), Duration::from_millis(250));
    supervisor.start_activation();
    assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Activating);
    supervisor.mark_active();
    assert!(supervisor.health().healthy);
    supervisor.record_log(PluginRuntimeLogLevel::Info, "activated");
    assert_eq!(supervisor.log_count(), 1);
    supervisor.start_deactivation();
    assert_eq!(
        supervisor.state(),
        PluginRuntimeLifecycleState::Deactivating
    );
    supervisor.kill();
    assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Killed);
}

#[test]
fn process_runtime_entry_resolves_inside_plugin_dir() {
    let temp_dir = unique_temp_dir("plugin-process-entry");
    let plugin_dir = temp_dir.join("plugin");
    let bin_dir = plugin_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join("plugin"), b"#!/bin/sh\n").unwrap();

    let resolved = resolve_process_runtime_entry(&plugin_dir, "bin/plugin").unwrap();
    assert!(resolved.starts_with(fs::canonicalize(&plugin_dir).unwrap()));
}

#[test]
fn process_runtime_entry_rejects_path_traversal() {
    let temp_dir = unique_temp_dir("plugin-process-traversal");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    let error = resolve_process_runtime_entry(&plugin_dir, "../outside").unwrap_err();
    assert_eq!(error.code, "invalid_process_entry");
}

#[cfg(unix)]
#[test]
fn process_runtime_entry_rejects_symlink_escape() {
    let temp_dir = unique_temp_dir("plugin-process-symlink");
    let plugin_dir = temp_dir.join("plugin");
    let outside_dir = temp_dir.join("outside");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::create_dir_all(&outside_dir).unwrap();
    fs::write(outside_dir.join("runner"), b"#!/bin/sh\n").unwrap();
    std::os::unix::fs::symlink(outside_dir.join("runner"), plugin_dir.join("runner")).unwrap();

    let error = resolve_process_runtime_entry(&plugin_dir, "runner").unwrap_err();
    assert_eq!(error.code, "process_entry_escapes_plugin_dir");
}

#[cfg(feature = "wasm-runtime")]
#[test]
fn wasm_runtime_entry_validates_plugin_path_and_magic() {
    let temp_dir = unique_temp_dir("plugin-wasm-entry");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();
    fs::write(plugin_dir.join("not-wasm.bin"), b"nope").unwrap();

    let resolved = resolve_wasm_runtime_entry(&plugin_dir, "plugin.wasm").unwrap();
    assert!(resolved.starts_with(fs::canonicalize(&plugin_dir).unwrap()));
    let error = resolve_wasm_runtime_entry(&plugin_dir, "not-wasm.bin").unwrap_err();
    assert_eq!(error.code, "wasm_entry_invalid_magic");
    let traversal = resolve_wasm_runtime_entry(&plugin_dir, "../plugin.wasm").unwrap_err();
    assert_eq!(traversal.code, "invalid_wasm_entry");
}

#[cfg(feature = "wasm-runtime")]
#[tokio::test]
async fn wasm_runtime_activation_executes_wasi_preview1_start() {
    let temp_dir = unique_temp_dir("plugin-wasm-activate");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), wasm_noop_start_module()).unwrap();

    let mut runtime = NativeWasmPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "plugin.wasm",
        Duration::from_millis(50),
    );
    let response = runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: 50,
        })
        .await
        .unwrap();

    assert_eq!(
        response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({
                "state": "active",
                "runtime": "wasm",
                "wasi": "preview1",
            })
        }
    );
    assert_eq!(
        runtime.health().await.unwrap().state,
        PluginRuntimeLifecycleState::Active
    );
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
fn wasm_noop_start_module() -> Vec<u8> {
    wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (global $heap (mut i32) (i32.const 2048))
              (func (export "_start"))
              (func (export "oxideterm_plugin_alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr))
            "#,
    )
    .unwrap()
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
async fn wasm_runtime_activation_reports_optional_runtime_missing() {
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

    assert_eq!(error.code, "wasm_runtime_not_installed");
    assert!(error.message.contains("standard build"));
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
async fn process_runtime_activate_uses_json_lines_protocol() {
    let temp_dir = unique_temp_dir("plugin-process-activate");
    let plugin_dir = temp_dir.join("plugin");
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
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
    runtime.kill().await.unwrap();
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
async fn process_runtime_replies_to_returnable_host_call_before_final_response() {
    let temp_dir = unique_temp_dir("plugin-process-returnable-host-call");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read activate
printf '%s\n' '{"protocolVersion":1,"requestId":"activate-test","payload":{"requestId":"activate-test","result":{"status":"ok","value":{"activated":true}}}}'
read dispatch
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-storage-get","namespace":"storage","method":"get","args":{"key":"recent"}}}'
read host_response
case "$host_response" in
  *'"value":"stored"'*) result='{"status":"ok","value":{"read":true}}' ;;
  *) result='{"status":"error","error":{"code":"bad_host_response","message":"missing host value","recoverable":false}}' ;;
esac
printf '%s\n' "{\"protocolVersion\":1,\"requestId\":\"command:demo.read\",\"payload\":{\"requestId\":\"command:demo.read\",\"result\":$result}}"
"#,
    );

    let mut runtime = NativeProcessPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "bin/plugin",
        process_runtime_test_timeout(),
    );
    runtime.set_host_call_handler(Box::new(|call| {
        assert_eq!(call.namespace, "storage");
        assert_eq!(call.method, "get");
        Some(PluginResponse::ok(
            call.request_id,
            serde_json::json!({ "value": "stored" }),
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
            request_id: "command:demo.read".to_string(),
            kind: PluginRequestKind::DispatchCommand {
                command: "demo.read".to_string(),
                args: Value::Null,
            },
            timeout_ms: Some(2_000),
        })
        .await
        .unwrap();

    assert_eq!(
        response.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "read": true })
        }
    );
    assert!(runtime.drain_outbound_effects().iter().any(|effect| {
        matches!(
            effect,
            PluginOutboundEffect::HostCall {
                namespace,
                method,
                ..
            } if namespace == "storage" && method == "get"
        )
    }));
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
async fn runtime_host_rejects_unauthorized_host_call_effects() {
    let temp_dir = unique_temp_dir("plugin-runtime-host-denied-host-call");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
read request
printf '%s\n' '{"protocolVersion":1,"payload":{"type":"callHostApi","requestId":"host-1","namespace":"secrets","method":"get","args":{"key":"token"}}}'
printf '%s\n' '{"protocolVersion":1,"requestId":"activate:com.example.runtime","payload":{"requestId":"activate:com.example.runtime","result":{"status":"ok","value":{"activated":true}}}}'
"#,
    );

    let mut host = NativePluginRuntimeHost::default();
    let error = host
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
        .unwrap_err();

    assert_eq!(error.code, "host_api_not_allowed");
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
    write_process_plugin(
        &plugin_dir,
        r#"#!/bin/sh
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

#[test]
fn supervisor_auto_disables_and_cleans_registrations_after_repeated_errors() {
    let mut supervisor =
        PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
    supervisor.mark_active();
    supervisor
        .record_registration(PluginRegistration {
            registration_id: "command-1".to_string(),
            plugin_id: "com.example.runtime".to_string(),
            kind: PluginRegistrationKind::Command,
            metadata: serde_json::json!({ "command": "demo.run" }),
        })
        .unwrap();

    supervisor.record_error(PluginError::runtime("crash", "first"));
    supervisor.record_error(PluginError::runtime("crash", "second"));
    assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Error);
    assert_eq!(supervisor.registration_count(), 1);

    supervisor.record_error(PluginError::runtime("crash", "third"));
    assert_eq!(
        supervisor.state(),
        PluginRuntimeLifecycleState::AutoDisabled
    );
    assert_eq!(supervisor.registration_count(), 0);
}

#[test]
fn supervisor_rejects_foreign_plugin_registration() {
    let mut supervisor =
        PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
    let result = supervisor.record_registration(PluginRegistration {
        registration_id: "status-1".to_string(),
        plugin_id: "com.example.other".to_string(),
        kind: PluginRegistrationKind::StatusBar,
        metadata: Value::Null,
    });

    assert!(result.is_err());
    assert_eq!(supervisor.registration_count(), 0);
}

#[test]
fn supervisor_applies_register_dispose_log_and_error_outbound_messages() {
    let mut supervisor =
        PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
    let registration = PluginRegistration {
        registration_id: "status-1".to_string(),
        plugin_id: "com.example.runtime".to_string(),
        kind: PluginRegistrationKind::StatusBar,
        metadata: serde_json::json!({ "text": "ready" }),
    };

    let effect = supervisor
        .handle_outbound_message(PluginOutboundMessage::RegisterContribution {
            registration: registration.clone(),
        })
        .unwrap();
    assert_eq!(effect, PluginOutboundEffect::RegistrationChanged);
    assert_eq!(supervisor.registration_count(), 1);

    let effect = supervisor
        .handle_outbound_message(PluginOutboundMessage::Log {
            level: PluginRuntimeLogLevel::Info,
            message: "registered".to_string(),
        })
        .unwrap();
    assert_eq!(effect, PluginOutboundEffect::None);
    assert_eq!(supervisor.log_count(), 1);

    let effect = supervisor
        .handle_outbound_message(PluginOutboundMessage::DisposeContribution {
            registration_id: registration.registration_id,
        })
        .unwrap();
    assert_eq!(effect, PluginOutboundEffect::RegistrationChanged);
    assert_eq!(supervisor.registration_count(), 0);

    supervisor
        .handle_outbound_message(PluginOutboundMessage::RuntimeError {
            error: PluginError::runtime("crash", "failed"),
        })
        .unwrap();
    assert_eq!(supervisor.state(), PluginRuntimeLifecycleState::Error);
}

#[test]
fn supervisor_rejects_foreign_registration_from_outbound_message() {
    let mut supervisor =
        PluginRuntimeSupervisorState::new("com.example.runtime", Duration::from_secs(5));
    let error = supervisor
        .handle_outbound_message(PluginOutboundMessage::RegisterContribution {
            registration: PluginRegistration {
                registration_id: "command-1".to_string(),
                plugin_id: "com.example.other".to_string(),
                kind: PluginRegistrationKind::Command,
                metadata: Value::Null,
            },
        })
        .unwrap_err();

    assert_eq!(error.code, "invalid_registration");
    assert_eq!(supervisor.registration_count(), 0);
}
