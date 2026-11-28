//! Browser Web Bluetooth GATT transport for OxideTerm Web.
//!
//! Uses dynamic `js_sys::Reflect` calls against `navigator.bluetooth` so the
//! crate compiles on stable `wasm32-unknown-unknown` without unstable `web-sys`
//! flags while providing full BLE UART discovery, notifications, chunked writes,
//! and clean GATT teardown on drop.

use futures::{StreamExt, channel::mpsc};
use gpui::{AsyncApp, Task};
use js_sys::{Array, DataView, Function, Promise, Reflect, Uint8Array};
use oxideterm_gpui_connections::{form_model::SerialLineEnding, form_state::WebBleServiceSummary};
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;

const NUS_SERVICE_UUID: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
const NUS_TX_CHAR_UUID: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
const NUS_RX_CHAR_UUID: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";
const HM10_SERVICE_UUID: &str = "0000ffe0-0000-1000-8000-00805f9b34fb";
const HM10_CHAR_UUID: &str = "0000ffe1-0000-1000-8000-00805f9b34fb";
const MICROCHIP_UART_SERVICE_UUID: &str = "49535343-fe7d-4ae5-8fa9-9fafd205e455";
const MICROCHIP_TX_CHAR_UUID: &str = "49535343-8841-43f4-a8d4-ecbe34729bb3";
const MICROCHIP_RX_CHAR_UUID: &str = "49535343-1e4d-4bd9-ba61-23c647249616";
const UBLOX_SPS_SERVICE_UUID: &str = "2456e1b9-26e2-8f83-e744-f34f01e9d701";
const DEVICE_INFO_SERVICE_UUID: &str = "0000180a-0000-1000-8000-00805f9b34fb";
const BATTERY_SERVICE_UUID: &str = "0000180f-0000-1000-8000-00805f9b34fb";

const OPTIONAL_SERVICES: &[&str] = &[
    NUS_SERVICE_UUID,
    HM10_SERVICE_UUID,
    MICROCHIP_UART_SERVICE_UUID,
    UBLOX_SPS_SERVICE_UUID,
    DEVICE_INFO_SERVICE_UUID,
    BATTERY_SERVICE_UUID,
];

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BleSessionConfig {
    pub service_uuid: Option<Uuid>,
    pub input_line_ending: SerialLineEnding,
    pub output_line_ending: SerialLineEnding,
    pub local_echo: bool,
    pub mtu: Option<u16>,
    pub write_without_response: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BleProfile {
    pub name: String,
    pub device_id: Option<String>,
    pub device_name: String,
    pub config: BleSessionConfig,
}

/// A paired Web Bluetooth device handle and its inspected GATT services.
#[derive(Clone)]
pub struct PairedWebBle {
    pub id: String,
    pub name: String,
    device: JsValue,
    gatt: JsValue,
    pub services: Vec<WebBleServiceSummary>,
}

pub enum BleEvent {
    Rx(Vec<u8>),
    WriteFailed,
    Disconnected,
}

pub struct BleConnectionInfo {
    pub device_name: String,
    pub service_label: String,
    pub service_uuid: Uuid,
    pub has_tx: bool,
    pub has_rx: bool,
}

/// Active Web Bluetooth GATT terminal link. Dropping this struct detaches JS
/// listeners and disconnects the GATT server cleanly.
pub struct WebBleLink {
    device: JsValue,
    gatt: JsValue,
    _lease: BleLease,
    _notifications: Option<BleSubscription>,
    write_tx: Option<mpsc::UnboundedSender<zeroize::Zeroizing<Vec<u8>>>>,
    chunk_size: usize,
    input_line_ending: SerialLineEnding,
    local_echo: bool,
    on_disconnect: Option<Closure<dyn FnMut(JsValue)>>,
    _writer: Task<()>,
}

impl Drop for WebBleLink {
    fn drop(&mut self) {
        if let Some(cb) = &self.on_disconnect {
            let _ = js_call(
                &self.device,
                "removeEventListener",
                &[
                    JsValue::from_str("gattserverdisconnected"),
                    cb.as_ref().clone(),
                ],
            );
        }
    }
}

pub fn is_ble_node(node: &str) -> bool {
    node.starts_with("ble:")
}

pub fn known_service_label(uuid: Uuid) -> String {
    let text = uuid.to_string();
    match text.as_str() {
        NUS_SERVICE_UUID => "Nordic UART Service (NUS)".into(),
        HM10_SERVICE_UUID => "HM-10 / Serial UART".into(),
        MICROCHIP_UART_SERVICE_UUID => "Microchip Transparent UART".into(),
        UBLOX_SPS_SERVICE_UUID => "u-blox Serial Port Service".into(),
        DEVICE_INFO_SERVICE_UUID => "Device Information".into(),
        BATTERY_SERVICE_UUID => "Battery Service".into(),
        _ => format!("GATT Service {}", &text[..8]),
    }
}

pub fn available() -> bool {
    web_sys::window()
        .filter(|w| w.is_secure_context())
        .and_then(|w| js_prop(&w.navigator().into(), "bluetooth").ok())
        .and_then(|api| js_prop(&api, "requestDevice").ok())
        .is_some_and(|method| method.is_function())
}

#[wasm_bindgen::prelude::wasm_bindgen(module = "/src/ble/discovery.mjs")]
extern "C" {
    #[wasm_bindgen(js_name = chooseDevice)]
    fn choose_device(bluetooth: &JsValue, services: &Array, expected_id: Option<&str>) -> Promise;
    #[wasm_bindgen(js_name = inspectServices)]
    fn inspect_services(device: &JsValue) -> Promise;
    type BrowserBleLease;
    #[wasm_bindgen(js_name = subscribeRx)]
    fn subscribe_rx(characteristic: &JsValue, callback: &JsValue) -> BrowserBleLease;
    #[wasm_bindgen(js_name = retainGatt)]
    fn retain_gatt(gatt: &JsValue) -> BrowserBleLease;
    #[wasm_bindgen(method, getter, structural)]
    fn ready(this: &BrowserBleLease) -> Promise;
    #[wasm_bindgen(method, structural)]
    fn release(this: &BrowserBleLease);
}

struct BleLease(BrowserBleLease);

struct BleSubscription {
    // Release the JS listener before the wasm callback is destroyed.
    _lease: BleLease,
    _callback: Closure<dyn FnMut(JsValue)>,
}

impl Drop for BleLease {
    fn drop(&mut self) {
        self.0.release();
    }
}

pub fn pair_and_inspect(
    extra_service: Option<Uuid>,
    expected_id: Option<&str>,
) -> impl Future<Output = Result<Option<PairedWebBle>, &'static str>> + use<> {
    let bluetooth = web_sys::window()
        .and_then(|w| js_prop(&w.navigator().into(), "bluetooth").ok())
        .unwrap_or(JsValue::UNDEFINED);
    let services = Array::new();
    for uuid in OPTIONAL_SERVICES {
        services.push(&JsValue::from_str(uuid));
    }
    if let Some(extra) = extra_service {
        let uuid = extra.to_string();
        if !OPTIONAL_SERVICES.contains(&uuid.as_str()) {
            services.push(&uuid.into());
        }
    }
    let selected = JsFuture::from(choose_device(&bluetooth, &services, expected_id));
    async move {
        let device = selected
            .await
            .map_err(|err| match err.as_string().as_deref() {
                Some("unavailable") => "modals.new_connection.ble_web_unavailable",
                Some("denied") => "modals.new_connection.ble_web_denied",
                Some("wrong-device") => "modals.new_connection.ble_web_wrong_device",
                Some("inspect") => "modals.new_connection.ble_inspect_failed",
                _ => "modals.new_connection.ble_scan_failed",
            })?;
        if device.is_null() {
            return Ok(None);
        }
        inspect_device(device).await.map(Some)
    }
}

async fn inspect_device(device: JsValue) -> Result<PairedWebBle, &'static str> {
    let failed = "modals.new_connection.ble_inspect_failed";
    let id = js_string(&device, "id").ok_or(failed)?;
    let name = js_string(&device, "name")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| id.clone());
    let gatt = js_prop(&device, "gatt").map_err(|_| failed)?;
    let summaries =
        JsFuture::from(inspect_services(&device))
            .await
            .map_err(|err| match js_string(&err, "name").as_deref() {
                Some("SecurityError" | "NotAllowedError") => "modals.new_connection.ble_web_denied",
                _ => failed,
            })?;
    let mut services = Vec::new();
    for service in Array::from(&summaries).iter() {
        let uuid = js_string(&service, "uuid")
            .and_then(|s| parse_ble_uuid(&s))
            .ok_or(failed)?;
        services.push(WebBleServiceSummary {
            uuid,
            label: known_service_label(uuid),
            has_write: js_bool(&service, "write"),
            has_write_without_response: js_bool(&service, "writeWithoutResponse"),
            has_notify: js_bool(&service, "notify"),
        });
    }

    // Prioritize full-duplex UART services so the default selection works out of the box.
    services.sort_by_key(|s| {
        let tx = s.has_write || s.has_write_without_response;
        let rx = s.has_notify;
        let known_uart = matches!(
            s.uuid.to_string().as_str(),
            NUS_SERVICE_UUID
                | HM10_SERVICE_UUID
                | MICROCHIP_UART_SERVICE_UUID
                | UBLOX_SPS_SERVICE_UUID
        );
        match (tx && rx, known_uart, tx || rx) {
            (true, true, _) => 0,
            (true, false, _) => 1,
            (false, true, _) => 2,
            (false, false, true) => 3,
            _ => 4,
        }
    });

    Ok(PairedWebBle {
        id,
        name,
        device,
        gatt,
        services,
    })
}

impl WebBleLink {
    /// Connects to the selected GATT service, subscribes to RX notifications,
    /// and starts a serialized write queue for TX characteristics.
    pub async fn open(
        paired: PairedWebBle,
        config: BleSessionConfig,
        events: mpsc::UnboundedSender<BleEvent>,
        cx: &mut AsyncApp,
    ) -> Result<(Self, BleConnectionInfo), String> {
        let lease = BleLease(retain_gatt(&paired.gatt));
        let gatt = JsFuture::from(lease.0.ready()).await.map_err(js_err)?;

        let target_uuid = config.service_uuid.or_else(|| {
            paired
                .services
                .iter()
                .find(|s| (s.has_write || s.has_write_without_response) && s.has_notify)
                .or_else(|| paired.services.first())
                .map(|s| s.uuid)
        });

        let (service, service_uuid) = if let Some(uuid) = target_uuid {
            let uuid_str = uuid.to_string();
            let svc = js_await(js_call(
                &gatt,
                "getPrimaryService",
                &[JsValue::from_str(&uuid_str)],
            )?)
            .await?;
            (svc, uuid)
        } else {
            let list = js_await(js_call(&gatt, "getPrimaryServices", &[])?).await?;
            let arr = Array::from(&list);
            let first = arr.get(0);
            if first.is_undefined() || first.is_null() {
                return Err(
                    "No accessible GATT services found on this device. Ensure it advertises a supported UART service."
                        .into(),
                );
            }
            let uuid = js_string(&first, "uuid")
                .and_then(|u| parse_ble_uuid(&u))
                .ok_or_else(|| "Unable to read primary GATT service UUID".to_string())?;
            (first, uuid)
        };

        let chars_val = js_await(js_call(&service, "getCharacteristics", &[])?).await?;
        let chars = Array::from(&chars_val);

        let mut tx_char: Option<(JsValue, bool, bool)> = None;
        let mut rx_char: Option<JsValue> = None;

        for ch in chars.iter() {
            let uuid_str = js_string(&ch, "uuid").unwrap_or_default().to_lowercase();
            let Ok(props) = js_prop(&ch, "properties") else {
                continue;
            };
            let can_write = js_bool(&props, "write");
            let can_write_nr = js_bool(&props, "writeWithoutResponse");
            let can_notify = js_bool(&props, "notify") || js_bool(&props, "indicate");

            if (can_write || can_write_nr)
                && (tx_char.is_none()
                    || uuid_str == NUS_TX_CHAR_UUID
                    || uuid_str == MICROCHIP_TX_CHAR_UUID
                    || uuid_str == HM10_CHAR_UUID)
            {
                tx_char = Some((ch.clone(), can_write, can_write_nr));
            }
            if can_notify
                && (rx_char.is_none()
                    || uuid_str == NUS_RX_CHAR_UUID
                    || uuid_str == MICROCHIP_RX_CHAR_UUID
                    || uuid_str == HM10_CHAR_UUID)
            {
                rx_char = Some(ch);
            }
        }

        if tx_char.is_none() && rx_char.is_none() {
            return Err(format!(
                "Service {} has neither writable nor notifiable characteristics",
                known_service_label(service_uuid)
            ));
        }

        let output_eol = config.output_line_ending;
        let rx_events = events.clone();
        let on_notify = if let Some(rx) = &rx_char {
            let cb = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
                if let Ok(target) = js_prop(&event, "target")
                    && let Ok(val) = js_prop(&target, "value")
                    && let Ok(view) = val.dyn_into::<DataView>()
                {
                    if let (Ok(offset), Ok(len)) = (
                        u32::try_from(view.byte_offset()),
                        u32::try_from(view.byte_length()),
                    ) {
                        let bytes = Uint8Array::new_with_byte_offset_and_length(
                            &view.buffer(),
                            offset,
                            len,
                        )
                        .to_vec();
                        if !bytes.is_empty() {
                            let _ = rx_events.unbounded_send(BleEvent::Rx(
                                apply_output_line_ending(&bytes, output_eol),
                            ));
                        }
                    }
                }
            });
            let lease = BleLease(subscribe_rx(rx, cb.as_ref()));
            let subscription = BleSubscription {
                _lease: lease,
                _callback: cb,
            };
            JsFuture::from(subscription._lease.0.ready())
                .await
                .map_err(js_err)?;
            Some(subscription)
        } else {
            None
        };

        let writer_events = events.clone();
        let disconnect_cb = Closure::<dyn FnMut(JsValue)>::new(move |_event: JsValue| {
            let _ = events.unbounded_send(BleEvent::Disconnected);
        });
        js_call(
            &paired.device,
            "addEventListener",
            &[
                JsValue::from_str("gattserverdisconnected"),
                disconnect_cb.as_ref().clone(),
            ],
        )?;

        let (write_tx, mut write_rx) = mpsc::unbounded::<zeroize::Zeroizing<Vec<u8>>>();
        let has_tx = tx_char.is_some();
        let has_rx = rx_char.is_some();
        let prefer_without_response = config.write_without_response;

        // Serialize GATT writes to avoid Web Bluetooth's "GATT operation already in progress" rejection.
        let writer = cx.spawn(async move |_| {
            let Some((ch, can_write, can_write_nr)) = tx_char else {
                return;
            };
            let use_without_response = can_write_nr && (prefer_without_response || !can_write);
            while let Some(chunk) = write_rx.next().await {
                if chunk.is_empty() {
                    continue;
                }
                let arr = Uint8Array::from(chunk.as_slice());
                let arg: JsValue = arr.into();
                let call = if use_without_response {
                    js_call(&ch, "writeValueWithoutResponse", &[arg.clone()])
                        .or_else(|_| js_call(&ch, "writeValue", &[arg]))
                } else {
                    js_call(&ch, "writeValueWithResponse", &[arg.clone()])
                        .or_else(|_| js_call(&ch, "writeValue", &[arg]))
                };
                let written = match call {
                    Ok(promise) => js_await(promise).await.is_ok(),
                    Err(_) => false,
                };
                if !written {
                    let _ = writer_events.unbounded_send(BleEvent::WriteFailed);
                    break;
                }
            }
        });

        let chunk_size = usize::from(config.mtu.unwrap_or(247).saturating_sub(3)).clamp(20, 512);
        let info = BleConnectionInfo {
            device_name: paired.name,
            service_label: known_service_label(service_uuid),
            service_uuid,
            has_tx,
            has_rx,
        };
        let link = Self {
            device: paired.device,
            gatt,
            _lease: lease,
            _notifications: on_notify,
            write_tx: has_tx.then_some(write_tx),
            chunk_size,
            input_line_ending: config.input_line_ending,
            local_echo: config.local_echo,
            on_disconnect: Some(disconnect_cb),
            _writer: writer,
        };
        Ok((link, info))
    }

    /// Encodes terminal input according to the configured line ending, splits it
    /// into MTU-safe ATT chunks, and returns optional local-echo bytes.
    pub fn writable(&self) -> bool {
        self.write_tx.as_ref().is_some_and(|tx| !tx.is_closed()) && js_bool(&self.gatt, "connected")
    }

    pub fn write(&self, raw: &[u8]) -> Result<Option<Vec<u8>>, ()> {
        let encoded = zeroize::Zeroizing::new(apply_input_line_ending(raw, self.input_line_ending));
        self.write_raw(&encoded)?;
        Ok(self.local_echo.then(|| format_local_echo(raw)))
    }

    pub fn write_raw(&self, bytes: &[u8]) -> Result<(), ()> {
        let tx = self.write_tx.as_ref().ok_or(())?;
        if !self.writable() || bytes.is_empty() {
            return Err(());
        }
        for chunk in bytes.chunks(self.chunk_size.max(20)) {
            tx.unbounded_send(zeroize::Zeroizing::new(chunk.to_vec()))
                .map_err(|_| ())?;
        }
        Ok(())
    }
}

fn apply_input_line_ending(raw: &[u8], ending: SerialLineEnding) -> Vec<u8> {
    if ending == SerialLineEnding::None {
        return raw.to_vec();
    }
    let replacement: &[u8] = match ending {
        SerialLineEnding::CrLf => b"\r\n",
        SerialLineEnding::Lf => b"\n",
        SerialLineEnding::Cr => b"\r",
        SerialLineEnding::None => unreachable!(),
    };
    let mut out = Vec::with_capacity(raw.len() + 4);
    for &b in raw {
        if b == b'\r' {
            out.extend_from_slice(replacement);
        } else {
            out.push(b);
        }
    }
    out
}

fn apply_output_line_ending(raw: &[u8], ending: SerialLineEnding) -> Vec<u8> {
    match ending {
        SerialLineEnding::None | SerialLineEnding::Lf => {
            // Convert standalone LF to CRLF so the terminal cursor returns to column 0.
            let mut out = Vec::with_capacity(raw.len() + 8);
            let mut prev = 0u8;
            for &b in raw {
                if b == b'\n' && prev != b'\r' {
                    out.push(b'\r');
                }
                out.push(b);
                prev = b;
            }
            out
        }
        SerialLineEnding::Cr => {
            let mut out = Vec::with_capacity(raw.len() + 8);
            for &b in raw {
                out.push(b);
                if b == b'\r' {
                    out.push(b'\n');
                }
            }
            out
        }
        SerialLineEnding::CrLf => raw.to_vec(),
    }
}

fn format_local_echo(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 4);
    for &b in raw {
        if b == b'\r' {
            out.extend_from_slice(b"\r\n");
        } else {
            out.push(b);
        }
    }
    out
}

fn parse_ble_uuid(text: &str) -> Option<Uuid> {
    let trimmed = text.trim();
    if let Ok(uuid) = Uuid::parse_str(trimmed) {
        return Some(uuid);
    }
    let short = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    if short.len() == 4 && short.chars().all(|c| c.is_ascii_hexdigit()) {
        return Uuid::parse_str(&format!("0000{short}-0000-1000-8000-00805f9b34fb")).ok();
    }
    None
}

fn js_prop(target: &JsValue, key: &str) -> Result<JsValue, String> {
    Reflect::get(target, &JsValue::from_str(key)).map_err(js_err)
}

fn js_string(target: &JsValue, key: &str) -> Option<String> {
    js_prop(target, key).ok().and_then(|v| v.as_string())
}

fn js_bool(target: &JsValue, key: &str) -> bool {
    js_prop(target, key)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn js_call(target: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let func_val = js_prop(target, method)?;
    let func = func_val
        .dyn_into::<Function>()
        .map_err(|_| format!("Web Bluetooth method '{method}' is not available"))?;
    let js_args = Array::new();
    for arg in args {
        js_args.push(arg);
    }
    func.apply(target, &js_args).map_err(js_err)
}

async fn js_await(value: JsValue) -> Result<JsValue, String> {
    let promise: Promise = value.into();
    JsFuture::from(promise).await.map_err(js_err)
}

fn js_err(err: JsValue) -> String {
    if let Some(msg) = Reflect::get(&err, &"message".into())
        .ok()
        .and_then(|v| v.as_string())
        && !msg.trim().is_empty()
    {
        return msg;
    }
    if let Some(text) = err.as_string() {
        return text;
    }
    format!("{err:?}")
}
