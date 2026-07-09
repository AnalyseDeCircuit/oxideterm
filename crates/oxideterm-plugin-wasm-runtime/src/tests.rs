// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use oxideterm_plugin_protocol::{
    PluginActivateRequest, PluginEvent, PluginOutboundMessage, PluginPermissionSet, PluginRequest,
    PluginRequestKind, PluginResponseResult, PluginRuntimeLifecycleState, PluginRuntimeLogLevel,
};

use super::*;
use crate::runtime::wasm_execution_error;

fn unique_temp_dir(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!("oxideterm-wasm-runtime-{name}-{millis}"))
}

fn sample_manifest() -> oxideterm_plugin_manifest::NativePluginManifest {
    oxideterm_plugin_manifest::NativePluginManifest {
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
fn wasm_runtime_entry_validates_plugin_path_and_magic() {
    let temp_dir = unique_temp_dir("entry");
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

#[tokio::test]
async fn wasm_runtime_activation_executes_wasi_preview1_start() {
    let temp_dir = unique_temp_dir("activate");
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

#[tokio::test]
async fn wasm_runtime_dispatches_command_and_event_over_memory_abi() {
    let temp_dir = unique_temp_dir("dispatch");
    let plugin_dir = temp_dir.join("plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), wasm_dispatch_module()).unwrap();

    let mut runtime = NativeWasmPluginRuntime::new(
        "com.example.runtime",
        &plugin_dir,
        "plugin.wasm",
        Duration::from_millis(250),
    );
    runtime
        .activate(PluginActivateRequest {
            request_id: "activate-test".to_string(),
            manifest: sample_manifest(),
            permissions: PluginPermissionSet::default(),
            timeout_ms: 250,
        })
        .await
        .unwrap();
    assert!(matches!(
        runtime.drain_outbound_messages().as_slice(),
        [PluginOutboundMessage::Log { level: PluginRuntimeLogLevel::Info, message }]
            if message == "wasm activated"
    ));

    let command = runtime
        .call(PluginRequest {
            request_id: "command:com.example.runtime:demo.run".to_string(),
            kind: PluginRequestKind::DispatchCommand {
                command: "demo.run".to_string(),
                args: serde_json::json!({}),
            },
            timeout_ms: Some(250),
        })
        .await
        .unwrap();
    assert_eq!(
        command.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "handled": true })
        }
    );

    let event = runtime
        .send_event(PluginEvent {
            name: "demo.event".to_string(),
            payload: serde_json::json!({}),
        })
        .await
        .unwrap();
    assert_eq!(
        event.result,
        PluginResponseResult::Ok {
            value: serde_json::json!({ "eventHandled": true })
        }
    );
}

#[test]
fn wasm_execution_error_preserves_wasi_exit_status() {
    // Wasmtime 46 exposes execution failures through its own error type.
    let error = wasmtime::Error::new(wasmtime_wasi::I32Exit(7));
    let plugin_error = wasm_execution_error(
        "wasm_handler_failed",
        "com.example.runtime",
        "execute native WASM plugin handler",
        error,
    );

    assert_eq!(plugin_error.code, "wasm_exit_status");
    assert!(plugin_error.message.contains("status 7"));
}

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

fn wat_data_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
