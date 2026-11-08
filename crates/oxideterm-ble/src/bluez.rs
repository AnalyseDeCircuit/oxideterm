//! Linux BlueZ address-directed recovery, spoken directly over D-Bus.
//!
//! btleplug's BlueZ backend can only connect to devices bluetoothd already
//! has a device object for; such objects appear in `GetManagedObjects` only
//! after a scan, a pairing, or another consumer's connect. When bluetoothd
//! reaped the object — the usual case after the peripheral powered off and
//! back on — the cached lookup can never recover it.
//!
//! `Adapter1.ConnectDevice` would create the device object straight from
//! the address, but it is an experimental method that no stock bluetoothd
//! registers (verified on BlueZ 5.85: absent without `--experimental`). The
//! only stock-API way to make bluetoothd (re)learn a device is discovery,
//! so this module runs a discovery window narrowed by the target address
//! (`SetDiscoveryFilter` "Pattern" prefix-matches Name or Address, so other
//! peers are not reported); the poll below still matches the Address
//! property exactly, and no stale cache entry can satisfy it. The window
//! stays open after the object appears — bluetoothd reaps unpaired temporary
//! devices around discovery stop, so the caller holds the window until its
//! own link is up.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dbus::Message;
use dbus::arg::{PropMap, Variant};
use dbus::blocking::{BlockingSender, SyncConnection};

use crate::BleError;

/// D-Bus timeout for the individual method calls in the recovery flow;
/// bluetoothd answers all of them in milliseconds.
const BLUEZ_METHOD_TIMEOUT: Duration = Duration::from_secs(5);
/// Interval between device-object polls during the directed discovery
/// window; LE devices commonly advertise every 100 ms to 1 s, so this
/// reacts within about one advertising interval.
const DISCOVERY_POLL_INTERVAL: Duration = Duration::from_millis(300);
/// Budget held back from the caller's timeout so `StopDiscovery` and its
/// reply still fit inside the outer async deadline.
const DISCOVERY_STOP_MARGIN: Duration = Duration::from_secs(1);

/// Runs a directed discovery for `device_mac` on the controller whose
/// address is `adapter_mac` and returns once bluetoothd has (re)created the
/// device object for it. Only public addresses are accepted, matching the
/// profile identity contract (resolvable private addresses rotate and
/// cannot be stored).
///
/// This does not open a link. The returned handle keeps the discovery window
/// open so bluetoothd cannot reap the temporary device object while the
/// caller resolves and connects through it; the caller closes the window
/// with [`BluezDirectedDiscovery::stop`] once the link has settled. Dropping
/// the handle instead still ends the window: bluetoothd removes a per-sender
/// discovery session when the sender disconnects from the bus. Returns an
/// error when the device does not appear within `timeout` (typically powered
/// off or out of range).
pub async fn ble_start_directed_discovery(
    adapter_mac: &str,
    device_mac: &str,
    timeout: Duration,
    cancelled: &tokio_util::sync::CancellationToken,
) -> Result<BluezDirectedDiscovery, BleError> {
    if cancelled.is_cancelled() {
        return Err(BleError::Cancelled);
    }
    let adapter_mac = adapter_mac.to_owned();
    let device_mac = device_mac.to_owned();
    let shutdown = cancelled.child_token();
    // Dropping this future must also stop the blocking worker, which Tokio
    // cannot abort once it has started running.
    let _cancel_on_drop = shutdown.clone().drop_guard();
    let worker_shutdown = shutdown.clone();
    // The blocking D-Bus round trips and poll loop park their thread until
    // bluetoothd answers, so they must not run on the async runtime's
    // worker threads.
    let mut task = tokio::task::spawn_blocking(move || {
        start_directed_discovery_blocking(&adapter_mac, &device_mac, timeout, &worker_shutdown)
    });
    tokio::select! {
        biased;
        _ = cancelled.cancelled() => {
            shutdown.cancel();
            // Join before returning so a retry cannot overlap our discovery.
            // A pending D-Bus call is bounded by BLUEZ_METHOD_TIMEOUT.
            let _ = task.await;
            Err(BleError::Cancelled)
        }
        _ = tokio::time::sleep(timeout) => {
            shutdown.cancel();
            let _ = task.await;
            Err(BleError::ConnectTimedOut)
        }
        result = &mut task => match result {
            Ok(Ok(discovery)) => Ok(discovery),
            Ok(Err(_)) if cancelled.is_cancelled() => Err(BleError::Cancelled),
            Ok(Err(reason)) => Err(BleError::DirectedDiscovery(reason)),
            Err(error) => Err(BleError::DirectedDiscovery(error.to_string())),
        },
    }
}

/// A directed discovery window held open for the caller's connect.
pub struct BluezDirectedDiscovery {
    connection: SyncConnection,
    adapter_path: dbus::Path<'static>,
    /// False when another consumer owns the active session; stopping the
    /// adapter would then tear down their scan.
    owns_discovery: bool,
}

impl BluezDirectedDiscovery {
    /// Ends the discovery window. No-op when another consumer owns the
    /// session; the caller invokes this once its connect has settled,
    /// whichever way it settled.
    pub async fn stop(self) {
        let BluezDirectedDiscovery {
            connection,
            adapter_path,
            owns_discovery,
        } = self;
        if !owns_discovery {
            return;
        }
        let task = tokio::task::spawn_blocking(move || {
            stop_discovery(&connection, &adapter_path);
        });
        let _ = task.await;
    }
}

fn stop_discovery(connection: &SyncConnection, adapter_path: &dbus::Path<'static>) {
    if let Ok(message) = Message::new_method_call(
        "org.bluez",
        adapter_path,
        "org.bluez.Adapter1",
        "StopDiscovery",
    ) {
        let _ = connection.send_with_reply_and_block(message, BLUEZ_METHOD_TIMEOUT);
    }
}

fn start_directed_discovery_blocking(
    adapter_mac: &str,
    device_mac: &str,
    timeout: Duration,
    cancelled: &tokio_util::sync::CancellationToken,
) -> Result<BluezDirectedDiscovery, String> {
    check_cancelled(cancelled)?;
    let connection = SyncConnection::new_system().map_err(|error| error.to_string())?;
    check_cancelled(cancelled)?;
    let adapter_path = find_adapter_path(&connection, adapter_mac)?;
    check_cancelled(cancelled)?;

    // "Pattern" is the only documented filter that narrows discovery to one
    // peer: bluetoothd prefix-matches it against the advertised Name or
    // Address and does not report (or create objects for) devices that fail
    // the filter. Addresses are compared in their uppercase D-Bus form; the
    // poll below still matches the Address property exactly, so a peer whose
    // name happens to start with this string cannot satisfy it. A rejected
    // filter does not invalidate the recovery. bluetoothd applies the filter
    // per connection, so it cannot disturb another consumer's scan.
    let mut filter: PropMap = HashMap::new();
    filter.insert(
        "Pattern".to_owned(),
        Variant(Box::new(device_mac.to_uppercase()) as Box<dyn dbus::arg::RefArg>),
    );
    if let Ok(message) = Message::new_method_call(
        "org.bluez",
        &adapter_path,
        "org.bluez.Adapter1",
        "SetDiscoveryFilter",
    ) {
        let _ = connection.send_with_reply_and_block(message.append1(filter), BLUEZ_METHOD_TIMEOUT);
    }
    check_cancelled(cancelled)?;

    let message = Message::new_method_call(
        "org.bluez",
        &adapter_path,
        "org.bluez.Adapter1",
        "StartDiscovery",
    )?;
    let owns_discovery = match connection.send_with_reply_and_block(message, BLUEZ_METHOD_TIMEOUT) {
        Ok(reply) => {
            drop(reply);
            true
        }
        // Another consumer is already scanning; its reports populate the
        // same object tree, so polling still works — but its session must
        // not be stopped by us.
        Err(error) if error.name() == Some("org.bluez.Error.InProgress") => false,
        Err(error) => return Err(dbus_error_text(&error)),
    };

    let poll_deadline = Instant::now() + timeout.saturating_sub(DISCOVERY_STOP_MARGIN);
    let mut last_lookup_error: Option<String> = None;
    let result = loop {
        if let Err(error) = check_cancelled(cancelled) {
            break Err(error);
        }
        match find_device_path(&connection, device_mac) {
            Ok(Some(_)) => break Ok(()),
            Ok(None) => {}
            // A rejected object-tree call is usually transient (bluetoothd
            // busy); keep polling to the deadline instead of aborting the
            // recovery, and surface the last failure if nothing appears.
            Err(error) => last_lookup_error = Some(error),
        }
        if Instant::now() >= poll_deadline {
            break Err(match last_lookup_error {
                Some(error) => format!(
                    "device did not appear during directed discovery and the last object-tree lookup failed: {error}"
                ),
                None => format!(
                    "device did not appear during directed discovery (is {device_mac} powered on and advertising?)"
                ),
            });
        }
        std::thread::sleep(DISCOVERY_POLL_INTERVAL);
    };

    match result {
        // Hand the open window back so the object survives until the
        // caller's connect lands and makes the device non-temporary.
        Ok(()) => Ok(BluezDirectedDiscovery {
            connection,
            adapter_path,
            owns_discovery,
        }),
        // The attempt failed: close our window before returning so the
        // radio is released even if nobody consumes the error.
        Err(reason) => {
            // Disconnecting this private D-Bus sender releases its session
            // on cancellation without another blocking StopDiscovery call.
            if owns_discovery && !cancelled.is_cancelled() {
                stop_discovery(&connection, &adapter_path);
            }
            Err(reason)
        }
    }
}

fn check_cancelled(cancelled: &tokio_util::sync::CancellationToken) -> Result<(), String> {
    if cancelled.is_cancelled() {
        Err("Bluetooth discovery cancelled".to_owned())
    } else {
        Ok(())
    }
}

/// Renders a D-Bus error with its error name when available — BlueZ error
/// names like `org.bluez.Error.NotReady` are the useful part for the
/// user-facing reason string.
fn dbus_error_text(error: &dbus::Error) -> String {
    match (error.name(), error.message()) {
        (Some(name), Some(message)) => format!("{name}: {message}"),
        (Some(name), None) => name.to_owned(),
        (None, Some(message)) => message.to_owned(),
        (None, None) => "unknown D-Bus error".to_owned(),
    }
}

/// Finds the adapter object path whose `Adapter1.Address` matches
/// `adapter_mac`, so the stored controller address — not an hci index —
/// decides which radio performs the discovery.
fn find_adapter_path(
    connection: &SyncConnection,
    adapter_mac: &str,
) -> Result<dbus::Path<'static>, String> {
    let message = Message::new_method_call(
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
    )?;
    let reply = connection
        .send_with_reply_and_block(message, BLUEZ_METHOD_TIMEOUT)
        .map_err(|error| dbus_error_text(&error))?;
    let objects: HashMap<dbus::Path<'static>, HashMap<String, PropMap>> =
        reply.read1().map_err(|error| error.to_string())?;
    objects
        .into_iter()
        .find_map(|(path, interfaces)| {
            let properties = interfaces.get("org.bluez.Adapter1")?;
            let address = properties.get("Address")?.0.as_str()?;
            address.eq_ignore_ascii_case(adapter_mac).then_some(path)
        })
        .ok_or_else(|| {
            format!("no org.bluez.Adapter1 with address {adapter_mac} is present on the system bus")
        })
}

/// Returns the object path of the `Device1` peer whose Address matches
/// `device_mac`, if bluetoothd currently knows it.
fn find_device_path(
    connection: &SyncConnection,
    device_mac: &str,
) -> Result<Option<dbus::Path<'static>>, String> {
    let message = Message::new_method_call(
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
    )?;
    let reply = connection
        .send_with_reply_and_block(message, BLUEZ_METHOD_TIMEOUT)
        .map_err(|error| dbus_error_text(&error))?;
    let objects: HashMap<dbus::Path<'static>, HashMap<String, PropMap>> =
        reply.read1().map_err(|error| error.to_string())?;
    Ok(objects.into_iter().find_map(|(path, interfaces)| {
        let properties = interfaces.get("org.bluez.Device1")?;
        let address = properties.get("Address")?.0.as_str()?;
        address.eq_ignore_ascii_case(device_mac).then_some(path)
    }))
}
