// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Workspace-side plumbing for plugin BLE notification routes.
//!
//! The plugin host API accepts subscriptions on tokio worker threads while
//! event dispatch needs the GPUI workspace, so accepted routes travel through
//! an unbounded channel into the dispatcher task below. The routes table is
//! the cancellation authority: offNotification, re-subscription, and plugin
//! deactivation all cancel a route token, and each pump removes its own entry
//! and tears down its CCCD subscription when it exits — including the session
//! close path, where the worker's drop closes the notification queue.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui::{Context, Task, WeakEntity};
use oxideterm_plugin_host_api::ble::{
    BleNotificationBridge, BleNotificationRouteRegistration, NativePluginBleSessionHandles,
    ble_notification_event_value,
};
use oxideterm_plugin_registry::NATIVE_PLUGIN_BLE_NOTIFICATION_EVENT;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::WorkspaceApp;

type BleRouteKey = (String, String, Uuid);

/// Shared table of live notification routes keyed by
/// `(plugin_id, session_id, characteristic)`.
type BleRouteTable = Arc<Mutex<HashMap<BleRouteKey, Arc<CancellationToken>>>>;

/// Workspace-owned registry of BLE GATT command surfaces. One row per live
/// BLE terminal session; cleanup on pane close is hygiene — the session
/// worker's drop already ends pumps and CCCD state on its own. The map is
/// shared behind a mutex so host-call resolvers stay valid across sessions
/// opened after the resolver was built.
pub(in crate::workspace) fn ble_session_handles_for_plugins(
    handles: &Mutex<
        HashMap<oxideterm_workspace::TerminalSessionId, oxideterm_terminal::BleGattHandle>,
    >,
) -> NativePluginBleSessionHandles {
    handles
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|(session_id, handle)| (session_id.0.to_string(), handle.clone()))
        .collect()
}

#[derive(Clone)]
pub(in crate::workspace) struct BleNotificationRouter {
    routes: BleRouteTable,
    registrations_tx: tokio::sync::mpsc::UnboundedSender<BleNotificationRouteRegistration>,
}

impl BleNotificationRouter {
    pub(in crate::workspace) fn new() -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<BleNotificationRouteRegistration>,
    ) {
        let (registrations_tx, registrations_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                routes: Arc::new(Mutex::new(HashMap::new())),
                registrations_tx,
            },
            registrations_rx,
        )
    }

    /// Bridge handed to the plugin host API. `register` may run on any tokio
    /// worker, so it only touches the route table and the channel; the
    /// dispatcher performs everything that needs the workspace.
    pub(in crate::workspace) fn bridge(&self) -> BleNotificationBridge {
        let routes = self.routes.clone();
        let cancel_routes = self.routes.clone();
        let registrations_tx = self.registrations_tx.clone();
        BleNotificationBridge::new(
            Arc::new(move |registration: BleNotificationRouteRegistration| {
                let key = (
                    registration.plugin_id.clone(),
                    registration.session_id.clone(),
                    registration.characteristic,
                );
                // A poisoned lock still holds a consistent map, so recover it
                // instead of panicking on a plugin worker thread.
                routes
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(key, registration.cancel_token.clone());
                // A closed channel means the dispatcher is gone (workspace
                // teardown); dropping the registration then drops its queue,
                // which ends the route exactly like cancellation.
                let _ = registrations_tx.send(registration);
            }),
            Arc::new(move |plugin_id, session_id, characteristic| {
                let key = (
                    plugin_id.to_string(),
                    session_id.to_string(),
                    *characteristic,
                );
                cancel_routes
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&key)
                    .map(|token| token.cancel())
                    .is_some()
            }),
        )
    }

    fn cancel_all_for_plugin(&self, plugin_id: &str) {
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let stale: Vec<BleRouteKey> = routes
            .keys()
            .filter(|(owner, _, _)| owner == plugin_id)
            .cloned()
            .collect();
        for key in stale {
            if let Some(token) = routes.remove(&key) {
                token.cancel();
            }
        }
    }
}

/// Spawns the dispatcher task that owns route registration and plugin
/// deactivation cleanup. Each accepted route gets a pump task; pumps end on
/// cancellation, session-worker drop, or workspace teardown — whichever comes
/// first — and always run their own CCCD unsubscribe.
pub(in crate::workspace) fn spawn_ble_notification_dispatcher(
    router: BleNotificationRouter,
    mut registrations: tokio::sync::mpsc::UnboundedReceiver<BleNotificationRouteRegistration>,
    deactivations: tokio::sync::mpsc::UnboundedReceiver<String>,
    cx: &mut Context<WorkspaceApp>,
) -> Task<()> {
    cx.spawn(async move |workspace, cx| {
        let mut deactivations = deactivations;
        // The entity drops its sender only at teardown; once closed there is
        // nothing left to revoke, so the branch is disabled for good.
        let mut deactivations_open = true;
        loop {
            tokio::select! {
                maybe_registration = registrations.recv() => {
                    let Some(registration) = maybe_registration else {
                        break;
                    };
                    let routes = router.routes.clone();
                    // Pumps dispatch events through the workspace, so they are
                    // spawned from inside an entity update. A failed update
                    // means the workspace is gone and the loop ends.
                    if workspace
                        .update(cx, |_, cx| {
                            cx.spawn(async move |workspace, cx| {
                                run_ble_notification_pump(workspace, cx, routes, registration)
                                    .await;
                            })
                            .detach();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                maybe_plugin = deactivations.recv(), if deactivations_open => {
                    match maybe_plugin {
                        Some(plugin_id) => router.cancel_all_for_plugin(&plugin_id),
                        None => deactivations_open = false,
                    }
                }
            }
        }
    })
}

async fn run_ble_notification_pump(
    workspace: WeakEntity<WorkspaceApp>,
    cx: &mut gpui::AsyncApp,
    routes: BleRouteTable,
    registration: BleNotificationRouteRegistration,
) {
    let BleNotificationRouteRegistration {
        plugin_id,
        session_id,
        characteristic,
        route_id,
        receiver: mut notifications,
        handle,
        cancel_token,
    } = registration;
    let key = (plugin_id.clone(), session_id.clone(), characteristic);
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => break,
            notification = notifications.recv() => {
                let Some(payload) = notification else {
                    break;
                };
                // GATT payload bytes stay inside the event frame; nothing is
                // logged, audited, or echoed in error text.
                let event_payload =
                    ble_notification_event_value(&session_id, characteristic, &payload);
                if workspace
                    .update(cx, |workspace, cx| {
                        workspace.dispatch_native_plugin_event(
                            plugin_id.clone(),
                            NATIVE_PLUGIN_BLE_NOTIFICATION_EVENT,
                            event_payload,
                            cx,
                        );
                    })
                    .is_err()
                {
                    break;
                }
            }
        }
    }
    // Remove the entry only while it still holds this pump's token: a
    // replacement route may already have registered a new token under the
    // same key between cancellation and this cleanup.
    {
        let mut table = routes.lock().unwrap_or_else(|error| error.into_inner());
        if table
            .get(&key)
            .is_some_and(|token| Arc::ptr_eq(token, &cancel_token))
        {
            table.remove(&key);
        }
    }
    // Drop the route-table guard before awaiting. The unsubscribe is scoped to
    // this route's id, so it cannot collide with a replacement route's
    // subscribe, and holding the lock across the await would let a stalled
    // session worker block every later register/cancel.
    // The session worker may already be gone; either outcome ends this
    // route's CCCD responsibility.
    let _ = handle.notify_unsubscribe(characteristic, route_id).await;
}

#[cfg(test)]
mod router_tests {
    use super::*;

    fn characteristic(tag: u8) -> Uuid {
        Uuid::from_bytes([tag; 16])
    }

    fn seed_route(
        router: &BleNotificationRouter,
        plugin: &str,
        session: &str,
        uuid: Uuid,
    ) -> Arc<CancellationToken> {
        let token = Arc::new(CancellationToken::new());
        router
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                (plugin.to_string(), session.to_string(), uuid),
                token.clone(),
            );
        token
    }

    #[test]
    fn cancel_removes_the_route_and_cancels_only_that_token() {
        let (router, mut registrations) = BleNotificationRouter::new();
        let token_a = seed_route(&router, "p", "s1", characteristic(1));
        let token_b = seed_route(&router, "p", "s1", characteristic(2));
        let bridge = router.bridge();

        assert!(bridge.cancel("p", "s1", &characteristic(1)));
        assert!(token_a.is_cancelled());
        assert!(!token_b.is_cancelled());
        // The route is gone: a second cancel reports nothing was removed.
        assert!(!bridge.cancel("p", "s1", &characteristic(1)));
        // A different plugin never cancels someone else's route.
        assert!(!bridge.cancel("other", "s1", &characteristic(2)));
        // cancel() must not enqueue registrations for the dispatcher.
        assert!(registrations.try_recv().is_err());
    }

    #[test]
    fn deactivation_cancels_every_route_of_that_plugin_only() {
        let (router, _registrations) = BleNotificationRouter::new();
        let token_p1 = seed_route(&router, "p", "s1", characteristic(1));
        let token_p2 = seed_route(&router, "p", "s2", characteristic(2));
        let token_q = seed_route(&router, "q", "s1", characteristic(3));

        router.cancel_all_for_plugin("p");
        assert!(token_p1.is_cancelled());
        assert!(token_p2.is_cancelled());
        assert!(!token_q.is_cancelled());
        // Only p's rows left the table: q's route still cancels normally.
        let bridge = router.bridge();
        assert!(bridge.cancel("q", "s1", &characteristic(3)));
    }
}
