// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

use serde_json::{Value, json};

use super::types::{NativePluginTerminalAction, NativePluginTerminalRequest};
use crate::workspace::{
    TerminalInputInterceptorResult, delivery,
    plugin_host::NativePluginRuntimeTerminalHookContribution, plugin_runtime,
    plugin_runtime::PluginResponseResult,
};

// Terminal hooks fail open when a plugin exceeds the foreground latency budget.
const NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT: Duration = Duration::from_millis(5);

// Upper bound on a terminal host call. Every `NativePluginTerminalAction`
// answers within one UI turn, so the only way the response wait blocks past
// this is a request whose owning window is gone and never drains the queue.
// An unbounded `recv` would pin this host-call worker forever (the BLE bridge
// applies the same `recv_timeout` lesson); the value is far above any real UI
// latency so it only trips on that leak.
const NATIVE_PLUGIN_TERMINAL_CALL_TIMEOUT: Duration = Duration::from_secs(10);

// Terminal hook execution has a strict timeout and fail-open behavior; keeping
// it isolated makes that contract easier to audit than burying it in lifecycle.
pub(super) fn native_plugin_terminal_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    terminal_tx: &delivery::ActiveDeliverySender<NativePluginTerminalRequest>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let action = match native_plugin_terminal_action_from_call(&call) {
        Ok(action) => action,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_terminal_args", error),
            );
        }
    };
    let (response_tx, response_rx) = mpsc::channel();
    if terminal_tx
        .send(NativePluginTerminalRequest {
            plugin_id: plugin_id.to_string(),
            request_id: request_id.clone(),
            action,
            response_tx,
        })
        .is_err()
    {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "terminal_host_unavailable",
                "Native plugin terminal host is unavailable",
            ),
        );
    }
    response_rx
        .recv_timeout(NATIVE_PLUGIN_TERMINAL_CALL_TIMEOUT)
        .unwrap_or_else(|_| {
            plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime(
                    "terminal_response_unavailable",
                    "Native plugin terminal host closed before answering",
                ),
            )
        })
}

pub(super) fn native_plugin_terminal_action_from_call(
    call: &plugin_runtime::PluginHostCall,
) -> Result<NativePluginTerminalAction, String> {
    match call.method.as_str() {
        "writeToActive" => {
            let text = call
                .args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.writeToActive requires args.text".to_string())?;
            Ok(NativePluginTerminalAction::WriteActive {
                text: text.to_string(),
            })
        }
        "writeToNode" => {
            let node_id = call
                .args
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.writeToNode requires args.nodeId".to_string())?;
            let text = call
                .args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.writeToNode requires args.text".to_string())?;
            Ok(NativePluginTerminalAction::WriteNode {
                node_id: node_id.to_string(),
                text: text.to_string(),
            })
        }
        "clearBuffer" => {
            let node_id = call
                .args
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.clearBuffer requires args.nodeId".to_string())?;
            Ok(NativePluginTerminalAction::ClearBuffer {
                node_id: node_id.to_string(),
            })
        }
        "openTelnet" => {
            let host = call
                .args
                .get("host")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .ok_or_else(|| "Telnet host cannot be empty".to_string())?;
            let port = call
                .args
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .filter(|port| *port > 0)
                .unwrap_or(23);
            Ok(NativePluginTerminalAction::OpenTelnet {
                host: host.to_string(),
                port,
            })
        }
        "writeLocal" => {
            // Display-only write: the text renders on the terminal screen but
            // never crosses into the session transport or the session log.
            let text = call
                .args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal.writeLocal requires args.text".to_string())?;
            // Optional args.terminalId selects the owning terminal session (the
            // same id the input interceptor reports and ble.* calls consume);
            // BLE and serial sessions have no SSH node, so a node id could not
            // address them. Without it the write lands on the active terminal.
            let terminal_id = call
                .args
                .get("terminalId")
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(NativePluginTerminalAction::WriteLocal {
                terminal_id,
                text: text.to_string(),
            })
        }
        "notify" => {
            // Display-only feedback channel: the notice overlays the workspace
            // and never crosses into the session transport the way the write
            // actions do.
            let title = call
                .args
                .get("title")
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())
                .ok_or_else(|| "ui.notify requires args.title".to_string())?;
            let description = call
                .args
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);
            let variant = call
                .args
                .get("variant")
                .and_then(Value::as_str)
                .unwrap_or("default")
                .to_string();
            Ok(NativePluginTerminalAction::Notify {
                title: title.to_string(),
                description,
                variant,
            })
        }
        "reportActionState" => {
            let action_id = call
                .args
                .get("actionId")
                .and_then(Value::as_str)
                .ok_or_else(|| "toolbar.reportActionState requires args.actionId".to_string())?;
            let state_id = call
                .args
                .get("stateId")
                .and_then(Value::as_str)
                .ok_or_else(|| "toolbar.reportActionState requires args.stateId".to_string())?;
            let pane_id = call
                .args
                .get("paneId")
                .and_then(Value::as_u64)
                .ok_or_else(|| "toolbar.reportActionState requires args.paneId".to_string())?;
            Ok(NativePluginTerminalAction::ReportToolbarActionState {
                action_id: action_id.to_string(),
                state_id: state_id.to_string(),
                pane_id,
            })
        }
        method => Err(format!("Unsupported terminal host call: {method}")),
    }
}

pub(super) fn native_plugin_apply_input_interceptors(
    bytes: &[u8],
    session_id: Option<&str>,
    hooks: &[NativePluginRuntimeTerminalHookContribution],
    runtime_host: Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>>,
    runtime: Arc<tokio::runtime::Runtime>,
    host_api_resolver: plugin_runtime::NativeHostApiResolver,
) -> TerminalInputInterceptorResult {
    native_plugin_reduce_input_interceptors(bytes, session_id, hooks, |hook, args| {
        // The UI thread waits only for the hook budget. A busy runtime, timeout,
        // transport error, or malformed response falls through to fail-open.
        let dispatch = runtime.block_on(async {
            tokio::time::timeout(NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT, async {
                let mut host = runtime_host.lock().await;
                host.set_host_api_resolver(host_api_resolver.clone());
                host.dispatch_command(
                    &hook.plugin_id,
                    hook.command.clone(),
                    args.clone(),
                    NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT,
                )
                .await
            })
            .await
        });
        let Ok(Ok(dispatch)) = dispatch else {
            return None;
        };
        match dispatch.response.result {
            PluginResponseResult::Ok { value } => Some(value),
            PluginResponseResult::Error { .. } => None,
        }
    })
}

pub(super) fn native_plugin_apply_output_processors(
    bytes: &[u8],
    hooks: &[NativePluginRuntimeTerminalHookContribution],
    runtime_host: Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>>,
    runtime: Arc<tokio::runtime::Runtime>,
    host_api_resolver: plugin_runtime::NativeHostApiResolver,
) -> Vec<u8> {
    native_plugin_reduce_output_processors(bytes, hooks, |hook, args| {
        // Output processors are allowed to transform display bytes, but timeout
        // and error semantics preserve the current byte stream to avoid terminal
        // corruption.
        let dispatch = runtime.block_on(async {
            tokio::time::timeout(NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT, async {
                let mut host = runtime_host.lock().await;
                host.set_host_api_resolver(host_api_resolver.clone());
                host.dispatch_command(
                    &hook.plugin_id,
                    hook.command.clone(),
                    args.clone(),
                    NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT,
                )
                .await
            })
            .await
        });
        let Ok(Ok(dispatch)) = dispatch else {
            return None;
        };
        match dispatch.response.result {
            PluginResponseResult::Ok { value } => Some(value),
            PluginResponseResult::Error { .. } => None,
        }
    })
}

pub(super) fn native_plugin_reduce_input_interceptors<F>(
    bytes: &[u8],
    session_id: Option<&str>,
    hooks: &[NativePluginRuntimeTerminalHookContribution],
    mut dispatch: F,
) -> TerminalInputInterceptorResult
where
    F: FnMut(&NativePluginRuntimeTerminalHookContribution, Value) -> Option<Value>,
{
    let mut current = String::from_utf8_lossy(bytes).to_string();
    let mut echo: Vec<u8> = Vec::new();
    for hook in hooks {
        // terminalId carries the owning session so plugins can attribute the
        // keystrokes to a concrete terminal (it matches the sessionId exposed
        // by ble.sessions and other session projections).
        let args = json!({
            "registrationId": hook.registration_id.clone(),
            "terminalId": session_id,
            "data": current.clone(),
            "text": current.clone(),
            "bytes": current.as_bytes().to_vec(),
        });
        let Some(value) = dispatch(hook, args) else {
            continue;
        };
        if value.is_null() {
            return TerminalInputInterceptorResult::Suppress;
        }
        // {"echo": "..."} accumulates screen-only display bytes. Together with
        // "suppress": true the payload is held back entirely (local echo); on
        // its own the echo still renders while the text keeps flowing.
        if let Some(echo_text) = value.get("echo").and_then(Value::as_str) {
            echo.extend_from_slice(echo_text.as_bytes());
        }
        if value.get("suppress").and_then(Value::as_bool) == Some(true) {
            return if echo.is_empty() {
                TerminalInputInterceptorResult::Suppress
            } else {
                TerminalInputInterceptorResult::LocalEcho { echo, send: None }
            };
        }
        if let Some(next) = native_plugin_terminal_hook_text_value(&value) {
            current = next;
        }
    }

    if echo.is_empty() {
        TerminalInputInterceptorResult::Continue(current.into_bytes())
    } else {
        TerminalInputInterceptorResult::LocalEcho {
            echo,
            send: Some(current.into_bytes()),
        }
    }
}

pub(super) fn native_plugin_reduce_output_processors<F>(
    bytes: &[u8],
    hooks: &[NativePluginRuntimeTerminalHookContribution],
    mut dispatch: F,
) -> Vec<u8>
where
    F: FnMut(&NativePluginRuntimeTerminalHookContribution, Value) -> Option<Value>,
{
    let mut current = bytes.to_vec();
    for hook in hooks {
        let args = json!({
            "registrationId": hook.registration_id.clone(),
            "bytes": current.clone(),
            "data": String::from_utf8_lossy(&current).to_string(),
        });
        let Some(value) = dispatch(hook, args) else {
            continue;
        };
        if let Some(next) = native_plugin_terminal_hook_bytes_value(&value) {
            current = next;
        }
    }
    current
}

pub(super) fn native_plugin_terminal_hook_host_api_resolver()
-> plugin_runtime::NativeHostApiResolver {
    Arc::new(|_plugin_id, _permissions, call| {
        // Terminal hooks run on the input budget; host APIs that bounce through
        // Workspace UI queues would make the timeout unenforceable.
        Some(plugin_runtime::PluginResponse::error(
            call.request_id,
            plugin_runtime::PluginError::runtime(
                "terminal_hook_host_api_unavailable",
                "Host APIs are unavailable while a terminal input hook is running",
            ),
        ))
    })
}

pub(super) fn native_plugin_terminal_hook_text_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    value
        .get("data")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn native_plugin_terminal_hook_bytes_value(value: &Value) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_str() {
        return Some(text.as_bytes().to_vec());
    }
    if let Some(bytes) = value.as_array() {
        return native_plugin_u8_array(bytes);
    }
    if let Some(bytes) = value.get("bytes").and_then(Value::as_array) {
        return native_plugin_u8_array(bytes);
    }
    value
        .get("data")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(|text| text.as_bytes().to_vec())
}

pub(super) fn native_plugin_u8_array(values: &[Value]) -> Option<Vec<u8>> {
    values
        .iter()
        .map(|value| value.as_u64().and_then(|byte| u8::try_from(byte).ok()))
        .collect()
}
