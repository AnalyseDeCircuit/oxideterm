// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Plugin-facing BLE GATT data plane.
//!
//! The session worker owns the radio; this module only validates plugin
//! requests, drives the shared `BleGattHandle` command channel, and assembles
//! responses. GATT payloads are sensitive: they never enter logs, audit, or
//! error text, so every message below stays at identity/length granularity.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use oxideterm_terminal::{BleGattError, BleGattHandle, BleGattServiceInfo};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use oxideterm_plugin_protocol as plugin_runtime;

use crate::capabilities::{NATIVE_PLUGIN_CAPABILITY_BLE_READ, NATIVE_PLUGIN_CAPABILITY_BLE_WRITE};

/// Default wait before a plugin GATT call gives up. Abandoned calls never
/// touch the link: GATT has no cancel primitive, so the host stops waiting
/// instead of tearing the connection down.
const BLE_DEFAULT_TIMEOUT_MS: u64 = 5000;
/// Hard ceiling so a runaway plugin value cannot pin a host-call worker.
const BLE_MAX_TIMEOUT_MS: u64 = 60_000;
/// Bounded per-route notification queue; the session worker drops newest on a
/// full queue so a slow plugin consumer cannot stall the radio path.
const BLE_NOTIFICATION_QUEUE_CAPACITY: usize = 16;

/// Session-id keyed GATT command surfaces resolved by the workspace before
/// the synchronous host-call hook runs.
pub type NativePluginBleSessionHandles = HashMap<String, BleGattHandle>;

/// Pure-data projection of one session's published state. Directly
/// constructible so response assembly stays testable without a live session.
/// The sessions projection reads identity from here; describe emits only the
/// GATT half.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BleDescribeSnapshot {
    pub device_name: Option<String>,
    pub device_address: String,
    pub service_uuid: uuid::Uuid,
    pub services: Vec<BleGattServiceInfo>,
    pub negotiated_mtu: Option<u16>,
    pub connected: bool,
}

impl BleDescribeSnapshot {
    pub fn from_handle(handle: &BleGattHandle) -> Self {
        let state = handle.shared_state();
        Self {
            device_name: state.device_name(),
            device_address: state.device_address(),
            service_uuid: state.default_service_uuid(),
            services: state.services(),
            negotiated_mtu: state.negotiated_mtu(),
            connected: state.connected(),
        }
    }
}

/// One accepted notification route handed to the workspace bridge. The pump
/// task owns the receiver and must pass `route_id` back to
/// `notify_unsubscribe` when the route ends, because subscription tables live
/// in the session worker and only the pump knows when its consumer stops.
pub struct BleNotificationRouteRegistration {
    pub plugin_id: String,
    pub session_id: String,
    pub characteristic: uuid::Uuid,
    pub route_id: u64,
    pub receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
    pub handle: BleGattHandle,
    pub cancel_token: Arc<CancellationToken>,
}

/// Workspace-side sink for notification routes: the host API accepts a
/// subscription and the bridge spawns the pump that streams payloads to the
/// plugin and performs CCCD teardown when the route ends.
#[derive(Clone)]
pub struct BleNotificationBridge {
    register: Arc<dyn Fn(BleNotificationRouteRegistration) + Send + Sync>,
    cancel: Arc<dyn Fn(&str, &str, &uuid::Uuid) -> bool + Send + Sync>,
}

impl BleNotificationBridge {
    pub fn new(
        register: Arc<dyn Fn(BleNotificationRouteRegistration) + Send + Sync>,
        cancel: Arc<dyn Fn(&str, &str, &uuid::Uuid) -> bool + Send + Sync>,
    ) -> Self {
        Self { register, cancel }
    }

    /// Bridge with no attached pump; used by tests.
    pub fn noop() -> Self {
        Self::new(Arc::new(|_| {}), Arc::new(|_, _, _| false))
    }

    pub fn register(&self, registration: BleNotificationRouteRegistration) {
        (self.register)(registration);
    }

    /// Returns whether an active route was actually removed.
    pub fn cancel(&self, plugin_id: &str, session_id: &str, characteristic: &uuid::Uuid) -> bool {
        (self.cancel)(plugin_id, session_id, characteristic)
    }
}

pub fn native_plugin_ble_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    permissions: &plugin_runtime::PluginPermissionSet,
    handles: &NativePluginBleSessionHandles,
    runtime: &Arc<tokio::runtime::Runtime>,
    bridge: &BleNotificationBridge,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    if let Err(error) = native_plugin_ble_check_capability(&call.method, permissions) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("plugin_ble_capability_denied", error),
        );
    }
    let plugin_id = plugin_id.to_string();
    let method = call.method.clone();
    let args = call.args;
    let handles = handles.clone();
    let bridge = bridge.clone();
    // Shared with the spawned task so a hook timeout can mark the call
    // abandoned; the timeout branch also needs the request identity to
    // release a route registered in the final race window.
    let abandoned = Arc::new(AtomicBool::new(false));
    let timeout_guard = (
        plugin_id.clone(),
        method.clone(),
        args.clone(),
        bridge.clone(),
    );
    let (response_tx, response_rx) = mpsc::channel();

    // The stdio host-call hook is synchronous, while GATT operations live on
    // the shared async runtime. Spawn there and block only this plugin
    // host-call worker until the response arrives, mirroring the SFTP shape.
    //
    // Blocking a runtime worker right after `spawn` strands the spawned task
    // in that worker's LIFO slot: the shared forwarding pool is tiny, so the
    // parked peer never migrates it and `recv` would hang forever while the
    // caller holds the runtime lock. `block_in_place` hands the LIFO task to
    // other workers, and `recv_timeout` bounds the wait either way.
    let audit_context = oxideterm_audit::AuditContext::current_request();
    let hook_timeout = ble_hook_response_timeout(&method, &args);
    let wait = || {
        let abandoned = abandoned.clone();
        runtime.spawn(oxideterm_audit::AuditContext::scope_optional(
            audit_context,
            async move {
                let result = native_plugin_ble_result(
                    &plugin_id, &method, &args, &handles, &bridge, &abandoned,
                )
                .await;
                let _ = response_tx.send(result);
            },
        ));
        response_rx.recv_timeout(hook_timeout)
    };
    let received = if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(wait)
    } else {
        wait()
    };

    match received {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(request_id, error),
        Err(recv_error) => {
            // The task may still finish after the hook timeout. Mark the call
            // abandoned so an in-flight onNotification undoes its own
            // subscription, and release a route that registered in the final
            // race window so no orphan pump holds the CCCD reference for the
            // rest of the session.
            abandoned.store(true, Ordering::SeqCst);
            let (plugin_id, method, args, bridge) = timeout_guard;
            if method == "onNotification"
                && let Ok(request) = native_plugin_ble_notification_args(&args)
            {
                bridge.cancel(
                    &plugin_id,
                    &request.session_id,
                    &request.characteristic_uuid,
                );
            }
            // Timeout and a dropped sender (worker panic) need different
            // wording: only the latter means the worker closed.
            let message = match recv_error {
                mpsc::RecvTimeoutError::Timeout => {
                    "Native plugin BLE worker did not respond within the hook timeout"
                }
                mpsc::RecvTimeoutError::Disconnected => {
                    "Native plugin BLE worker closed before returning a response"
                }
            };
            plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime("plugin_ble_unavailable", message),
            )
        }
    }
}

pub fn native_plugin_ble_check_capability(
    method: &str,
    permissions: &plugin_runtime::PluginPermissionSet,
) -> Result<(), String> {
    let required = match method {
        // Baseline metadata reads; the catalog marks both capability-free.
        "describe" | "sessions" => return Ok(()),
        "read" | "onNotification" | "offNotification" => NATIVE_PLUGIN_CAPABILITY_BLE_READ,
        "write" => NATIVE_PLUGIN_CAPABILITY_BLE_WRITE,
        // Unknown methods fail in the dispatch below, not in the gate.
        _ => return Ok(()),
    };
    if permissions
        .capabilities
        .iter()
        .any(|capability| capability == required)
    {
        return Ok(());
    }
    Err(format!(
        "Native plugin BLE host call \"{method}\" requires capability \"{required}\""
    ))
}

async fn native_plugin_ble_result(
    plugin_id: &str,
    method: &str,
    args: &Value,
    handles: &NativePluginBleSessionHandles,
    bridge: &BleNotificationBridge,
    abandoned: &AtomicBool,
) -> Result<Value, plugin_runtime::PluginError> {
    match method {
        // describe reads only the shared state snapshot: no worker round trip,
        // no radio traffic, and valid even while the link is down.
        "describe" => {
            let session_id =
                native_plugin_ble_session_id_arg(args).map_err(native_plugin_ble_invalid_args)?;
            let handle = native_plugin_ble_handle_for(&session_id, handles)?;
            Ok(ble_describe_json(&BleDescribeSnapshot::from_handle(
                &handle,
            )))
        }
        // Live session discovery: plugin surfaces (tabs, sidebars) hold no
        // focused terminal, so they resolve the BLE sessionId here first. The
        // projection stays at device-identity granularity.
        "sessions" => Ok(ble_sessions_json(handles.iter().map(
            |(session_id, handle)| {
                (
                    session_id.as_str(),
                    BleDescribeSnapshot::from_handle(handle),
                )
            },
        ))),
        "read" => {
            let request = native_plugin_ble_read_args(args)?;
            let handle = native_plugin_ble_connected_handle(&request.session_id, handles)?;
            let data = tokio::time::timeout(
                Duration::from_millis(request.timeout_ms),
                handle.read(request.service_uuid, request.characteristic_uuid),
            )
            .await
            .map_err(|_| ble_runtime_error(BleGattError::Timeout))?
            .map_err(ble_runtime_error)?;
            ble_read_json(&data, request.encoding, request.max_length)
        }
        "write" => {
            let request = native_plugin_ble_write_args(args)?;
            let handle = native_plugin_ble_connected_handle(&request.session_id, handles)?;
            tokio::time::timeout(
                Duration::from_millis(request.timeout_ms),
                handle.write(
                    request.service_uuid,
                    request.characteristic_uuid,
                    request.data,
                    request.write_without_response,
                ),
            )
            .await
            .map_err(|_| ble_runtime_error(BleGattError::Timeout))?
            .map_err(ble_runtime_error)?;
            Ok(Value::Null)
        }
        "onNotification" => {
            let request = native_plugin_ble_notification_args(args)?;
            let handle = native_plugin_ble_connected_handle(&request.session_id, handles)?;
            // Re-subscribing replaces the active route for the same
            // characteristic; the previous pump performs its own CCCD teardown
            // once it observes cancellation.
            bridge.cancel(plugin_id, &request.session_id, &request.characteristic_uuid);
            let (sender, receiver) = tokio::sync::mpsc::channel(BLE_NOTIFICATION_QUEUE_CAPACITY);
            let route_id = handle
                .notify_subscribe(request.service_uuid, request.characteristic_uuid, sender)
                .await
                .map_err(ble_runtime_error)?;
            if abandoned.load(Ordering::SeqCst) {
                // The host already answered a hook timeout; the plugin never
                // learns this route exists, so undo the subscription instead
                // of registering a pump nobody can release.
                let _ = handle
                    .notify_unsubscribe(request.characteristic_uuid, route_id)
                    .await;
                return Ok(Value::Null);
            }
            bridge.register(BleNotificationRouteRegistration {
                plugin_id: plugin_id.to_string(),
                session_id: request.session_id.clone(),
                characteristic: request.characteristic_uuid,
                route_id,
                receiver,
                handle,
                cancel_token: Arc::new(CancellationToken::new()),
            });
            if abandoned.load(Ordering::SeqCst) {
                // A hook timeout between the check above and this register ran
                // its cancel against an empty table, so undo the route this
                // call just added; the pump performs its own CCCD teardown on
                // cancel. Without this re-check the plugin has already been
                // answered with an error while an orphan pump keeps dispatching.
                bridge.cancel(plugin_id, &request.session_id, &request.characteristic_uuid);
            }
            Ok(Value::Null)
        }
        "offNotification" => {
            let request = native_plugin_ble_notification_args(args)?;
            // Idempotent by contract: unknown sessions and stale routes are
            // still successful removals so plugin cleanup paths cannot break.
            bridge.cancel(plugin_id, &request.session_id, &request.characteristic_uuid);
            Ok(Value::Null)
        }
        method => Err(native_plugin_ble_invalid_args(format!(
            "Unsupported BLE host call: {method}"
        ))),
    }
}

fn native_plugin_ble_handle_for(
    session_id: &str,
    handles: &NativePluginBleSessionHandles,
) -> Result<BleGattHandle, plugin_runtime::PluginError> {
    handles.get(session_id).cloned().ok_or_else(|| {
        plugin_runtime::PluginError::runtime(
            BleGattError::SessionNotFound.code(),
            ble_gatt_error_message(BleGattError::SessionNotFound),
        )
    })
}

/// Fails fast on a downed link so plugin commands cannot queue behind a
/// reconnect with a mismatched timeout.
fn native_plugin_ble_connected_handle(
    session_id: &str,
    handles: &NativePluginBleSessionHandles,
) -> Result<BleGattHandle, plugin_runtime::PluginError> {
    let handle = native_plugin_ble_handle_for(session_id, handles)?;
    if !handle.shared_state().connected() {
        return Err(ble_runtime_error(BleGattError::NotConnected));
    }
    Ok(handle)
}

#[derive(Debug)]
struct BleReadRequest {
    session_id: String,
    service_uuid: Option<uuid::Uuid>,
    characteristic_uuid: uuid::Uuid,
    encoding: BleDataEncoding,
    timeout_ms: u64,
    max_length: Option<usize>,
}

fn native_plugin_ble_read_args(
    args: &Value,
) -> Result<BleReadRequest, plugin_runtime::PluginError> {
    Ok(BleReadRequest {
        session_id: native_plugin_ble_session_id_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        service_uuid: native_plugin_ble_optional_service_uuid_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        characteristic_uuid: native_plugin_ble_uuid_arg(args, "characteristicUuid")
            .map_err(native_plugin_ble_invalid_args)?,
        encoding: BleDataEncoding::parse_arg(args, "encoding")
            .map_err(native_plugin_ble_invalid_args)?,
        timeout_ms: native_plugin_ble_timeout_ms_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        max_length: native_plugin_ble_max_length_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
    })
}

#[derive(Debug)]
struct BleWriteRequest {
    session_id: String,
    service_uuid: Option<uuid::Uuid>,
    characteristic_uuid: uuid::Uuid,
    data: Vec<u8>,
    write_without_response: Option<bool>,
    timeout_ms: u64,
}

fn native_plugin_ble_write_args(
    args: &Value,
) -> Result<BleWriteRequest, plugin_runtime::PluginError> {
    let data_encoding =
        BleDataEncoding::parse_arg(args, "dataEncoding").map_err(native_plugin_ble_invalid_args)?;
    let raw_data = args
        .get("data")
        .and_then(Value::as_str)
        .filter(|data| !data.is_empty())
        .ok_or_else(|| {
            native_plugin_ble_invalid_args("ble write requires args.data".to_string())
        })?;
    // A payload that fails its declared encoding is a data defect, not an
    // argument-shape defect, so it maps to the runtime encoding code.
    let data = data_encoding
        .decode(raw_data)
        .map_err(ble_invalid_encoding_error)?;
    Ok(BleWriteRequest {
        session_id: native_plugin_ble_session_id_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        service_uuid: native_plugin_ble_optional_service_uuid_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        characteristic_uuid: native_plugin_ble_uuid_arg(args, "characteristicUuid")
            .map_err(native_plugin_ble_invalid_args)?,
        data,
        write_without_response: native_plugin_ble_write_type_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        timeout_ms: native_plugin_ble_timeout_ms_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
    })
}

struct BleNotificationRequest {
    session_id: String,
    service_uuid: Option<uuid::Uuid>,
    characteristic_uuid: uuid::Uuid,
}

fn native_plugin_ble_notification_args(
    args: &Value,
) -> Result<BleNotificationRequest, plugin_runtime::PluginError> {
    Ok(BleNotificationRequest {
        session_id: native_plugin_ble_session_id_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        service_uuid: native_plugin_ble_optional_service_uuid_arg(args)
            .map_err(native_plugin_ble_invalid_args)?,
        characteristic_uuid: native_plugin_ble_uuid_arg(args, "characteristicUuid")
            .map_err(native_plugin_ble_invalid_args)?,
    })
}

fn native_plugin_ble_session_id_arg(args: &Value) -> Result<String, String> {
    // BLE owns no SSH node: args.sessionId carries the terminal session id the
    // workspace registry keys handles by.
    let Some(session_id) = args.get("sessionId") else {
        return Err("ble host call requires args.sessionId".to_string());
    };
    let Some(session_id) = session_id
        .as_str()
        .filter(|session_id| !session_id.is_empty())
    else {
        return Err("ble args.sessionId must be a session id string".to_string());
    };
    Ok(session_id.to_string())
}

fn native_plugin_ble_uuid_arg(args: &Value, field: &str) -> Result<uuid::Uuid, String> {
    let Some(raw) = args.get(field) else {
        return Err(format!("ble host call requires args.{field}"));
    };
    let Some(raw) = raw.as_str().filter(|raw| !raw.is_empty()) else {
        return Err(format!("ble args.{field} must be a UUID string"));
    };
    uuid::Uuid::parse_str(raw).map_err(|_| format!("ble args.{field} is not a valid UUID"))
}

fn native_plugin_ble_optional_service_uuid_arg(args: &Value) -> Result<Option<uuid::Uuid>, String> {
    match args.get("serviceUuid") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => native_plugin_ble_uuid_arg(args, "serviceUuid").map(Some),
    }
}

fn ble_hook_response_timeout(method: &str, args: &Value) -> Duration {
    // The async operation owns the requested deadline; allow its error to
    // reach the synchronous caller before the worker-stall guard expires.
    let operation_ms = if matches!(method, "read" | "write") {
        native_plugin_ble_timeout_ms_arg(args).unwrap_or(BLE_DEFAULT_TIMEOUT_MS)
    } else {
        BLE_DEFAULT_TIMEOUT_MS
    };
    Duration::from_millis(operation_ms.saturating_add(5000).max(30_000))
}

fn native_plugin_ble_timeout_ms_arg(args: &Value) -> Result<u64, String> {
    match args.get("timeoutMs") {
        None | Some(Value::Null) => Ok(BLE_DEFAULT_TIMEOUT_MS),
        Some(value) => value
            .as_u64()
            .filter(|timeout_ms| *timeout_ms > 0)
            .map(|timeout_ms| timeout_ms.min(BLE_MAX_TIMEOUT_MS))
            .ok_or_else(|| "ble args.timeoutMs must be a positive integer".to_string()),
    }
}

fn native_plugin_ble_max_length_arg(args: &Value) -> Result<Option<usize>, String> {
    match args.get("maxLength") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let max_length = value
                .as_u64()
                .filter(|max_length| *max_length > 0)
                .ok_or_else(|| "ble args.maxLength must be a positive integer".to_string())?;
            usize::try_from(max_length)
                .map(Some)
                .map_err(|_| "ble args.maxLength must be a positive integer".to_string())
        }
    }
}

fn native_plugin_ble_write_type_arg(args: &Value) -> Result<Option<bool>, String> {
    let Some(raw) = args
        .get("writeType")
        .and_then(Value::as_str)
        .filter(|raw| !raw.is_empty())
    else {
        // Omitted follows the profile snapshot held in the session state.
        return Ok(None);
    };
    match raw {
        "withResponse" => Ok(Some(false)),
        "withoutResponse" => Ok(Some(true)),
        _ => Err("ble args.writeType must be \"withResponse\" or \"withoutResponse\"".to_string()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BleDataEncoding {
    Hex,
    Base64,
    Utf8,
}

impl BleDataEncoding {
    /// Parses a plugin-supplied encoding field; omitted or empty selects hex,
    /// the compact default for register-style payloads.
    fn parse_arg(args: &Value, field: &str) -> Result<Self, String> {
        let Some(raw) = args
            .get(field)
            .and_then(Value::as_str)
            .filter(|raw| !raw.is_empty())
        else {
            return Ok(Self::Hex);
        };
        match raw {
            "hex" => Ok(Self::Hex),
            "base64" => Ok(Self::Base64),
            "utf8" => Ok(Self::Utf8),
            _ => Err(format!(
                "ble args.{field} must be one of \"hex\", \"base64\", \"utf8\""
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Base64 => "base64",
            Self::Utf8 => "utf8",
        }
    }

    fn encode(self, data: &[u8]) -> Result<String, String> {
        match self {
            Self::Hex => Ok(ble_encode_hex(data)),
            Self::Base64 => Ok(STANDARD.encode(data)),
            Self::Utf8 => String::from_utf8(data.to_vec()).map_err(|_| {
                "BLE payload is not valid UTF-8 for args.encoding \"utf8\"".to_string()
            }),
        }
    }

    fn decode(self, raw: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Hex => ble_decode_hex(raw),
            Self::Base64 => STANDARD
                .decode(raw)
                .map_err(|_| "ble args.data is not valid base64".to_string()),
            Self::Utf8 => Ok(raw.as_bytes().to_vec()),
        }
    }
}

fn ble_encode_hex(data: &[u8]) -> String {
    let mut encoded = String::with_capacity(data.len() * 2);
    for byte in data {
        // The write into a pre-sized string cannot fail.
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn ble_decode_hex(raw: &str) -> Result<Vec<u8>, String> {
    // Strict digit check first: it rejects sign prefixes that
    // from_str_radix would otherwise accept.
    if !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) || raw.len() % 2 != 0 {
        return Err("ble args.data is not valid hex".to_string());
    }
    (0..raw.len() / 2)
        .map(|index| {
            u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16)
                .map_err(|_| "ble args.data is not valid hex".to_string())
        })
        .collect()
}

/// Projects live BLE sessions for plugin-side node discovery. Entries carry
/// device identity only; GATT payloads stay behind the read/write calls.
fn ble_sessions_json<'a>(
    sessions: impl IntoIterator<Item = (&'a str, BleDescribeSnapshot)>,
) -> Value {
    let mut sessions = sessions
        .into_iter()
        .map(|(session_id, snapshot)| {
            json!({
                "sessionId": session_id,
                "deviceName": snapshot.device_name,
                "deviceAddress": snapshot.device_address,
                "serviceUuid": snapshot.service_uuid.to_string(),
                "connected": snapshot.connected,
            })
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        left.get("sessionId")
            .and_then(Value::as_str)
            .cmp(&right.get("sessionId").and_then(Value::as_str))
    });
    json!({ "sessions": sessions })
}

fn ble_describe_json(snapshot: &BleDescribeSnapshot) -> Value {
    json!({
        "serviceUuid": snapshot.service_uuid.to_string(),
        "negotiatedMtu": snapshot.negotiated_mtu,
        "connected": snapshot.connected,
        "services": snapshot
            .services
            .iter()
            .map(|service| {
                json!({
                    "uuid": service.uuid.to_string(),
                    "characteristics": service
                        .characteristics
                        .iter()
                        .map(|characteristic| json!({
                            "uuid": characteristic.uuid.to_string(),
                            // Bitmask over the exported BLE_GATT_PROPERTY_* flags.
                            "properties": characteristic.properties,
                        }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    })
}

fn ble_read_json(
    data: &[u8],
    encoding: BleDataEncoding,
    max_length: Option<usize>,
) -> Result<Value, plugin_runtime::PluginError> {
    if let Some(max_length) = max_length
        && data.len() > max_length
    {
        // Lengths are identity-level metadata, so naming them is safe here.
        return Err(plugin_runtime::PluginError::runtime(
            "ble_read_too_large",
            format!(
                "BLE read returned {len} bytes, exceeding args.maxLength {max_length}",
                len = data.len()
            ),
        ));
    }
    let encoded = encoding.encode(data).map_err(ble_invalid_encoding_error)?;
    Ok(json!({
        "data": encoded,
        "encoding": encoding.name(),
        "length": data.len(),
    }))
}

/// Wire shape for `ble.notification` events delivered to plugins. Assembled
/// here so the host app cannot drift from the documented contract; hex is
/// fixed because plugins cannot choose an encoding per frame.
pub fn ble_notification_event_value(
    session_id: &str,
    characteristic: uuid::Uuid,
    data: &[u8],
) -> Value {
    json!({
        "sessionId": session_id,
        "characteristicUuid": characteristic.to_string(),
        "data": ble_encode_hex(data),
        "encoding": BleDataEncoding::Hex.name(),
        "length": data.len(),
    })
}

fn native_plugin_ble_invalid_args(message: String) -> plugin_runtime::PluginError {
    plugin_runtime::PluginError::protocol("plugin_ble_invalid_args", message)
}

fn ble_invalid_encoding_error(message: impl Into<String>) -> plugin_runtime::PluginError {
    plugin_runtime::PluginError::runtime(BleGattError::InvalidEncoding.code(), message)
}

fn ble_runtime_error(error: BleGattError) -> plugin_runtime::PluginError {
    plugin_runtime::PluginError::runtime(error.code(), ble_gatt_error_message(error))
}

// Messages stay identity-level: GATT payload bytes must never reach error
// text, logs, or diagnostics.
fn ble_gatt_error_message(error: BleGattError) -> String {
    match error {
        BleGattError::SessionNotFound => "no live BLE session matches args.sessionId".to_string(),
        BleGattError::NotConnected => "the BLE session is not connected".to_string(),
        BleGattError::UnsupportedCharacteristic => {
            "the characteristic is not published by this BLE session".to_string()
        }
        BleGattError::WriteTooLarge => {
            "the write payload exceeds the negotiated ATT write budget".to_string()
        }
        BleGattError::InvalidEncoding => {
            "the payload does not match the declared encoding".to_string()
        }
        BleGattError::Timeout => "the GATT operation timed out".to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hook_guard_allows_the_full_requested_operation_timeout() {
        for method in ["read", "write"] {
            for (requested, expected) in [
                (5000, 30_000),
                (45_000, 50_000),
                (60_000, 65_000),
                (600_000, 65_000),
            ] {
                assert_eq!(
                    super::ble_hook_response_timeout(
                        method,
                        &serde_json::json!({"timeoutMs": requested})
                    ),
                    std::time::Duration::from_millis(expected)
                );
            }
        }
    }
    use super::*;
    use crate::catalog::{AccessTier, HOST_API_CATALOG, allowed_host_apis_for_capabilities};
    use oxideterm_terminal::{
        BLE_GATT_PROPERTY_NOTIFY, BLE_GATT_PROPERTY_READ, BLE_GATT_PROPERTY_WRITE,
        BleGattCharacteristicInfo,
    };

    const VALID_SERVICE: &str = "00000000-0000-0000-0000-000000001000";
    const VALID_CHARACTERISTIC: &str = "00000000-0000-0000-0000-000000003001";

    fn ble_call(method: &str, args: Value) -> plugin_runtime::PluginHostCall {
        plugin_runtime::PluginHostCall {
            request_id: "req-1".to_string(),
            namespace: "ble".to_string(),
            method: method.to_string(),
            args,
        }
    }

    fn permissions(capabilities: &[&str]) -> plugin_runtime::PluginPermissionSet {
        plugin_runtime::PluginPermissionSet {
            capabilities: capabilities
                .iter()
                .map(|capability| capability.to_string())
                .collect(),
            allowed_host_apis: Vec::new(),
        }
    }

    fn response_error_code(response: &plugin_runtime::PluginResponse) -> String {
        match &response.result {
            plugin_runtime::PluginResponseResult::Error { error } => error.code.clone(),
            other => panic!("expected an error response, got {other:?}"),
        }
    }

    #[test]
    fn ble_capability_gate_matches_catalog_tiers() {
        let none = permissions(&[]);
        // describe and sessions are baseline and need no capability.
        assert!(native_plugin_ble_check_capability("describe", &none).is_ok());
        assert!(native_plugin_ble_check_capability("sessions", &none).is_ok());
        for method in ["read", "onNotification", "offNotification"] {
            assert!(native_plugin_ble_check_capability(method, &none).is_err());
            assert!(
                native_plugin_ble_check_capability(
                    method,
                    &permissions(&[NATIVE_PLUGIN_CAPABILITY_BLE_READ])
                )
                .is_ok()
            );
        }
        assert!(native_plugin_ble_check_capability("write", &none).is_err());
        // ble.read alone must not unlock the mutating write path.
        assert!(
            native_plugin_ble_check_capability(
                "write",
                &permissions(&[NATIVE_PLUGIN_CAPABILITY_BLE_READ])
            )
            .is_err()
        );
        assert!(
            native_plugin_ble_check_capability(
                "write",
                &permissions(&[NATIVE_PLUGIN_CAPABILITY_BLE_WRITE])
            )
            .is_ok()
        );
    }

    #[test]
    fn ble_catalog_entries_map_to_planned_tiers_and_capabilities() {
        for (method, tier, capability) in [
            ("describe", AccessTier::BaselineRead, None),
            ("sessions", AccessTier::BaselineRead, None),
            (
                "read",
                AccessTier::SensitiveRead,
                Some(NATIVE_PLUGIN_CAPABILITY_BLE_READ),
            ),
            (
                "write",
                AccessTier::Mutating,
                Some(NATIVE_PLUGIN_CAPABILITY_BLE_WRITE),
            ),
            (
                "onNotification",
                AccessTier::SensitiveRead,
                Some(NATIVE_PLUGIN_CAPABILITY_BLE_READ),
            ),
            (
                "offNotification",
                AccessTier::SensitiveRead,
                Some(NATIVE_PLUGIN_CAPABILITY_BLE_READ),
            ),
        ] {
            let descriptor = HOST_API_CATALOG
                .iter()
                .find(|descriptor| descriptor.namespace == "ble" && descriptor.method == method)
                .unwrap_or_else(|| panic!("ble.{method} must be cataloged"));
            assert_eq!(descriptor.access_tier, tier);
            assert_eq!(descriptor.capability, capability);
        }

        let baseline = allowed_host_apis_for_capabilities(std::iter::empty::<&str>());
        assert!(baseline.contains(&"ble.describe".to_string()));
        assert!(baseline.contains(&"ble.sessions".to_string()));
        assert!(!baseline.contains(&"ble.read".to_string()));
        assert!(!baseline.contains(&"ble.write".to_string()));

        let reader = allowed_host_apis_for_capabilities([NATIVE_PLUGIN_CAPABILITY_BLE_READ]);
        assert!(reader.contains(&"ble.read".to_string()));
        assert!(reader.contains(&"ble.onNotification".to_string()));
        assert!(reader.contains(&"ble.offNotification".to_string()));
        assert!(!reader.contains(&"ble.write".to_string()));
    }

    #[test]
    fn ble_sessions_json_lists_registry_identity_sorted_by_session_id() {
        let service = uuid::Uuid::parse_str(VALID_SERVICE).unwrap();
        let named = BleDescribeSnapshot {
            device_name: Some("Sensor Board".to_string()),
            device_address: "12:34:56:78:9A:BC".to_string(),
            service_uuid: service,
            negotiated_mtu: None,
            connected: true,
            services: Vec::new(),
        };
        let unnamed = BleDescribeSnapshot {
            device_name: None,
            device_address: "AA:BB:CC:DD:EE:FF".to_string(),
            service_uuid: service,
            negotiated_mtu: None,
            connected: false,
            services: Vec::new(),
        };
        // Insert out of order so the projection must sort by session id.
        let value = ble_sessions_json([("session-b", unnamed), ("session-a", named)]);
        let sessions = value.get("sessions").and_then(Value::as_array).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0]["sessionId"], json!("session-a"));
        assert_eq!(sessions[0]["deviceName"], json!("Sensor Board"));
        assert_eq!(sessions[0]["deviceAddress"], json!("12:34:56:78:9A:BC"));
        assert_eq!(sessions[0]["serviceUuid"], json!(VALID_SERVICE));
        assert_eq!(sessions[0]["connected"], json!(true));
        assert_eq!(sessions[1]["sessionId"], json!("session-b"));
        // A device without an advertised name projects null, not an empty label.
        assert_eq!(sessions[1]["deviceName"], Value::Null);
        assert_eq!(sessions[1]["deviceAddress"], json!("AA:BB:CC:DD:EE:FF"));
        assert_eq!(sessions[1]["connected"], json!(false));
    }

    #[test]
    fn ble_describe_json_reports_the_full_gatt_table() {
        let service = uuid::Uuid::parse_str(VALID_SERVICE).unwrap();
        let notify_characteristic = uuid::Uuid::parse_str(VALID_CHARACTERISTIC).unwrap();
        let write_characteristic =
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000003002").unwrap();
        let snapshot = BleDescribeSnapshot {
            device_name: Some("Sensor Board".to_string()),
            device_address: "12:34:56:78:9A:BC".to_string(),
            service_uuid: service,
            negotiated_mtu: Some(247),
            connected: true,
            services: vec![BleGattServiceInfo {
                uuid: service,
                characteristics: vec![
                    BleGattCharacteristicInfo {
                        uuid: notify_characteristic,
                        properties: BLE_GATT_PROPERTY_READ | BLE_GATT_PROPERTY_NOTIFY,
                    },
                    BleGattCharacteristicInfo {
                        uuid: write_characteristic,
                        properties: BLE_GATT_PROPERTY_WRITE,
                    },
                ],
            }],
        };

        let value = ble_describe_json(&snapshot);

        assert_eq!(value["serviceUuid"], VALID_SERVICE);
        assert_eq!(value["negotiatedMtu"], 247);
        assert_eq!(value["connected"], true);
        let characteristics = &value["services"][0]["characteristics"];
        assert_eq!(characteristics[0]["uuid"], VALID_CHARACTERISTIC);
        assert_eq!(
            characteristics[0]["properties"],
            BLE_GATT_PROPERTY_READ | BLE_GATT_PROPERTY_NOTIFY
        );
        assert_eq!(characteristics[1]["properties"], BLE_GATT_PROPERTY_WRITE);

        // A never-negotiated session renders nulls instead of zero sentinels;
        // identity fields are carried but describe leaves them to sessions.
        // The snapshot holds real identity values so a leaked identity field
        // would render non-null and fail these assertions.
        let idle = ble_describe_json(&BleDescribeSnapshot {
            device_name: Some("Idle Board".to_string()),
            device_address: "12:34:56:78:9A:BC".to_string(),
            service_uuid: service,
            services: Vec::new(),
            negotiated_mtu: None,
            connected: false,
        });
        assert_eq!(idle["deviceName"], Value::Null);
        assert_eq!(idle["deviceAddress"], Value::Null);
        assert_eq!(idle["negotiatedMtu"], Value::Null);
        assert_eq!(idle["services"], serde_json::json!([]));
    }

    #[test]
    fn ble_data_encoding_uses_independent_wire_expectations() {
        let bytes = [0x01u8, 0xa0, 0xff];
        assert_eq!(BleDataEncoding::Hex.encode(&bytes).unwrap(), "01a0ff");
        assert_eq!(BleDataEncoding::Hex.decode("01A0ff").unwrap(), bytes);
        assert_eq!(
            BleDataEncoding::Base64.encode(b"hello").unwrap(),
            "aGVsbG8="
        );
        assert_eq!(
            BleDataEncoding::Base64.decode("aGVsbG8=").unwrap(),
            b"hello".to_vec()
        );
        assert_eq!(
            BleDataEncoding::Utf8.encode("héllo".as_bytes()).unwrap(),
            "héllo"
        );
        assert_eq!(
            BleDataEncoding::Utf8.decode("héllo").unwrap(),
            "héllo".as_bytes().to_vec()
        );

        // Rejects non-hex digits, sign prefixes that from_str_radix would
        // accept, and odd-length input.
        assert!(BleDataEncoding::Hex.decode("0 1a0").is_err());
        assert!(BleDataEncoding::Hex.decode("+1a0").is_err());
        assert!(BleDataEncoding::Hex.decode("01a").is_err());
        assert!(BleDataEncoding::Base64.decode("!!!").is_err());
    }

    #[test]
    fn ble_notification_event_value_matches_read_payload_contract() {
        let value = ble_notification_event_value(
            "ble-session-7",
            uuid::uuid!("0000ffe1-0000-1000-8000-00805f9b34fb"),
            &[0x01, 0xa0, 0xff],
        );
        assert_eq!(value["sessionId"], "ble-session-7");
        assert_eq!(
            value["characteristicUuid"],
            "0000ffe1-0000-1000-8000-00805f9b34fb"
        );
        assert_eq!(value["data"], "01a0ff");
        assert_eq!(value["encoding"], "hex");
        assert_eq!(value["length"], 3);
    }

    #[test]
    fn ble_read_json_enforces_max_length_and_encodes_declared() {
        let bytes = [0x01u8, 0xa0, 0xff];
        let value = ble_read_json(&bytes, BleDataEncoding::Hex, Some(3)).unwrap();
        assert_eq!(value["data"], "01a0ff");
        assert_eq!(value["encoding"], "hex");
        assert_eq!(value["length"], 3);

        assert_eq!(
            ble_read_json(&bytes, BleDataEncoding::Hex, Some(2))
                .unwrap_err()
                .code,
            "ble_read_too_large"
        );

        // A payload that violates its declared encoding is a runtime failure.
        assert_eq!(
            ble_read_json(&[0xff], BleDataEncoding::Utf8, None)
                .unwrap_err()
                .code,
            "ble_invalid_encoding"
        );
    }

    #[test]
    fn ble_request_parsers_accept_documented_shapes_and_reject_malformed_values() {
        let request = native_plugin_ble_read_args(&serde_json::json!({
            "sessionId": "ble-session-1",
            "serviceUuid": VALID_SERVICE,
            "characteristicUuid": VALID_CHARACTERISTIC,
            "encoding": "base64",
            "timeoutMs": 1234,
            "maxLength": 64,
        }))
        .unwrap();
        assert_eq!(request.session_id, "ble-session-1");
        assert_eq!(
            request.service_uuid,
            Some(uuid::Uuid::parse_str(VALID_SERVICE).unwrap())
        );
        assert_eq!(
            request.characteristic_uuid,
            uuid::Uuid::parse_str(VALID_CHARACTERISTIC).unwrap()
        );
        assert_eq!(request.encoding, BleDataEncoding::Base64);
        assert_eq!(request.timeout_ms, 1234);
        assert_eq!(request.max_length, Some(64));

        // Omitted opts fall back to documented defaults.
        let request = native_plugin_ble_read_args(&serde_json::json!({
            "sessionId": "ble-session-1",
            "characteristicUuid": VALID_CHARACTERISTIC,
        }))
        .unwrap();
        assert_eq!(request.service_uuid, None);
        assert_eq!(request.encoding, BleDataEncoding::Hex);
        assert_eq!(request.timeout_ms, BLE_DEFAULT_TIMEOUT_MS);
        assert_eq!(request.max_length, None);

        // Runaway timeout values clamp at the host boundary.
        let request = native_plugin_ble_read_args(&serde_json::json!({
            "sessionId": "ble-session-1",
            "characteristicUuid": VALID_CHARACTERISTIC,
            "timeoutMs": 600_000,
        }))
        .unwrap();
        assert_eq!(request.timeout_ms, BLE_MAX_TIMEOUT_MS);

        for (fragment, args) in [
            ("sessionId", serde_json::json!({})),
            ("sessionId", serde_json::json!({ "sessionId": 7 })),
            (
                "characteristicUuid",
                serde_json::json!({ "sessionId": "s" }),
            ),
            (
                "UUID",
                serde_json::json!({ "sessionId": "s", "characteristicUuid": "not-a-uuid" }),
            ),
            (
                "serviceUuid",
                serde_json::json!({
                    "sessionId": "s",
                    "characteristicUuid": VALID_CHARACTERISTIC,
                    "serviceUuid": "zz",
                }),
            ),
            (
                "encoding",
                serde_json::json!({
                    "sessionId": "s",
                    "characteristicUuid": VALID_CHARACTERISTIC,
                    "encoding": "raw",
                }),
            ),
            (
                "timeoutMs",
                serde_json::json!({
                    "sessionId": "s",
                    "characteristicUuid": VALID_CHARACTERISTIC,
                    "timeoutMs": 0,
                }),
            ),
            (
                "maxLength",
                serde_json::json!({
                    "sessionId": "s",
                    "characteristicUuid": VALID_CHARACTERISTIC,
                    "maxLength": 0,
                }),
            ),
        ] {
            let error = native_plugin_ble_read_args(&args).unwrap_err();
            assert_eq!(error.code, "plugin_ble_invalid_args");
            assert!(
                error.message.contains(fragment),
                "expected {fragment} context in \"{}\"",
                error.message
            );
        }
    }

    #[test]
    fn ble_write_request_decodes_payload_by_declared_encoding() {
        let request = native_plugin_ble_write_args(&serde_json::json!({
            "sessionId": "ble-session-1",
            "characteristicUuid": VALID_CHARACTERISTIC,
            "data": "01A0ff",
            "writeType": "withoutResponse",
        }))
        .unwrap();
        assert_eq!(request.data, vec![0x01, 0xa0, 0xff]);
        assert_eq!(request.write_without_response, Some(true));

        // utf8 passes the raw string bytes through; writeType omitted follows
        // the profile snapshot held in the session state.
        let request = native_plugin_ble_write_args(&serde_json::json!({
            "sessionId": "ble-session-1",
            "characteristicUuid": VALID_CHARACTERISTIC,
            "dataEncoding": "utf8",
            "data": "héllo",
        }))
        .unwrap();
        assert_eq!(request.data, "héllo".as_bytes().to_vec());
        assert_eq!(request.write_without_response, None);

        // A payload that does not match its declared encoding is a runtime
        // encoding failure, distinct from argument-shape errors.
        assert_eq!(
            native_plugin_ble_write_args(&serde_json::json!({
                "sessionId": "ble-session-1",
                "characteristicUuid": VALID_CHARACTERISTIC,
                "data": "zz",
            }))
            .unwrap_err()
            .code,
            "ble_invalid_encoding"
        );

        // Missing, empty, and mistyped payloads are argument-shape errors.
        for args in [
            serde_json::json!({ "sessionId": "s", "characteristicUuid": VALID_CHARACTERISTIC }),
            serde_json::json!({ "sessionId": "s", "characteristicUuid": VALID_CHARACTERISTIC, "data": "" }),
            serde_json::json!({ "sessionId": "s", "characteristicUuid": VALID_CHARACTERISTIC, "data": 7 }),
            serde_json::json!({
                "sessionId": "s",
                "characteristicUuid": VALID_CHARACTERISTIC,
                "data": "00",
                "writeType": "streamed",
            }),
        ] {
            assert_eq!(
                native_plugin_ble_write_args(&args).unwrap_err().code,
                "plugin_ble_invalid_args"
            );
        }
    }

    #[test]
    fn ble_response_shell_maps_session_and_capability_failures() {
        let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let handles = NativePluginBleSessionHandles::new();
        let bridge = BleNotificationBridge::noop();
        let reader = permissions(&[NATIVE_PLUGIN_CAPABILITY_BLE_READ]);
        let writer = permissions(&[NATIVE_PLUGIN_CAPABILITY_BLE_WRITE]);

        // Capability denial precedes any session work and keeps the request id.
        let response = native_plugin_ble_response(
            "plugin-a",
            ble_call(
                "write",
                serde_json::json!({ "sessionId": "s", "characteristicUuid": VALID_CHARACTERISTIC, "data": "00" }),
            ),
            &reader,
            &handles,
            &runtime,
            &bridge,
        );
        assert_eq!(response.request_id, "req-1");
        assert_eq!(
            response_error_code(&response),
            "plugin_ble_capability_denied"
        );

        // Unknown methods are protocol errors, not runtime failures.
        let response = native_plugin_ble_response(
            "plugin-a",
            ble_call("ping", serde_json::json!({})),
            &reader,
            &handles,
            &runtime,
            &bridge,
        );
        assert_eq!(response_error_code(&response), "plugin_ble_invalid_args");

        // Malformed args fail before session resolution.
        let response = native_plugin_ble_response(
            "plugin-a",
            ble_call("read", serde_json::json!({ "sessionId": "s" })),
            &reader,
            &handles,
            &runtime,
            &bridge,
        );
        assert_eq!(response_error_code(&response), "plugin_ble_invalid_args");

        // A session id missing from the registry resolves to the runtime
        // session-not-found code for every session-scoped method.
        for method in ["describe", "read", "write", "onNotification"] {
            let args = if method == "write" {
                serde_json::json!({ "sessionId": "gone", "characteristicUuid": VALID_CHARACTERISTIC, "data": "00" })
            } else {
                serde_json::json!({ "sessionId": "gone", "characteristicUuid": VALID_CHARACTERISTIC })
            };
            let granted = if method == "write" { &writer } else { &reader };
            let response = native_plugin_ble_response(
                "plugin-a",
                ble_call(method, args),
                granted,
                &handles,
                &runtime,
                &bridge,
            );
            assert_eq!(
                response_error_code(&response),
                "ble_session_not_found",
                "{method}"
            );
        }

        // offNotification is idempotent for unknown sessions and stale routes.
        let response = native_plugin_ble_response(
            "plugin-a",
            ble_call(
                "offNotification",
                serde_json::json!({ "sessionId": "gone", "characteristicUuid": VALID_CHARACTERISTIC }),
            ),
            &reader,
            &handles,
            &runtime,
            &bridge,
        );
        assert!(matches!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok { .. }
        ));
    }
}
