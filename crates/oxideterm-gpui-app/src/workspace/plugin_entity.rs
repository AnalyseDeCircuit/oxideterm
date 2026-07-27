// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::time::Instant;
use zeroize::Zeroizing;

use super::plugin_lifecycle::{
    NativePluginConfirmDialog, NativePluginConfirmRequest, NativePluginProductUiEffect,
    NativePluginSyncRequest, NativePluginTerminalRequest,
};
#[cfg(test)]
use super::plugin_lifecycle::{NativePluginSyncAction, NativePluginTerminalAction};

pub(in crate::workspace) enum PluginWorkspaceEvent {
    ManagerDeliveryReady,
    RuntimeRequestsReady,
}

/// Producer endpoints shared by one native plugin host resolver.
pub(in crate::workspace) struct PluginRuntimeRequestSenders {
    pub(in crate::workspace) confirm: delivery::ActiveDeliverySender<NativePluginConfirmRequest>,
    pub(in crate::workspace) terminal: delivery::ActiveDeliverySender<NativePluginTerminalRequest>,
    pub(in crate::workspace) sync: delivery::ActiveDeliverySender<NativePluginSyncRequest>,
}

/// Owns plugin workers and reliable delivery independently from plugin page visibility.
pub(in crate::workspace) struct PluginWorkspaceEntity {
    task_runtime: Arc<tokio::runtime::Runtime>,
    manager_operation_in_flight: bool,
    manager_delivery_tx:
        delivery::ActiveDeliverySender<plugin_manager::NativePluginManagerDelivery>,
    manager_delivery_rx: std::sync::mpsc::Receiver<plugin_manager::NativePluginManagerDelivery>,
    manager_deliveries: VecDeque<plugin_manager::NativePluginManagerDelivery>,
    runtime_request_wake: delivery::ActiveDeliveryWake,
    confirm_tx: delivery::ActiveDeliverySender<NativePluginConfirmRequest>,
    confirm_rx: std::sync::mpsc::Receiver<NativePluginConfirmRequest>,
    confirm: Option<NativePluginConfirmDialog>,
    confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    terminal_tx: delivery::ActiveDeliverySender<NativePluginTerminalRequest>,
    terminal_rx: std::sync::mpsc::Receiver<NativePluginTerminalRequest>,
    sync_tx: delivery::ActiveDeliverySender<NativePluginSyncRequest>,
    sync_rx: std::sync::mpsc::Receiver<NativePluginSyncRequest>,
    product_ui_effects: VecDeque<NativePluginProductUiEffect>,
}

impl PluginWorkspaceEntity {
    pub(in crate::workspace) fn new(
        task_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (manager_delivery_tx, manager_delivery_rx) = delivery::ActiveDeliverySender::channel();
        let runtime_request_wake = delivery::ActiveDeliveryWake::default();
        let (confirm_tx, confirm_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_request_wake.clone());
        let (terminal_tx, terminal_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_request_wake.clone());
        let (sync_tx, sync_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_request_wake.clone());
        let entity = Self {
            task_runtime,
            manager_operation_in_flight: false,
            manager_delivery_tx,
            manager_delivery_rx,
            manager_deliveries: VecDeque::new(),
            runtime_request_wake,
            confirm_tx,
            confirm_rx,
            confirm: None,
            confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            terminal_tx,
            terminal_rx,
            sync_tx,
            sync_rx,
            product_ui_effects: VecDeque::new(),
        };
        entity.schedule_manager_delivery(cx);
        entity.schedule_runtime_request_delivery(cx);
        entity
    }

    pub(in crate::workspace) fn manager_operation_in_flight(&self) -> bool {
        self.manager_operation_in_flight
    }

    pub(in crate::workspace) fn start_package_install(
        &mut self,
        settings_path: PathBuf,
        download_url: Zeroizing<String>,
        checksum: Option<String>,
        overwrite: bool,
    ) -> bool {
        if self.manager_operation_in_flight {
            return false;
        }
        self.manager_operation_in_flight = true;
        let delivery_tx = self.manager_delivery_tx.clone();
        self.task_runtime.spawn(async move {
            let result = plugin_host::NativePluginRegistry::install_plugin_package_from_url(
                &settings_path,
                download_url.trim(),
                checksum.as_deref(),
                overwrite,
            )
            .await;
            let outcome = match result {
                Ok(result) => plugin_manager::NativePluginInstallOutcome::Installed(result),
                Err(error) => {
                    let error = Zeroizing::new(error);
                    match plugin_host::native_plugin_conflict_id(&error) {
                        Some(plugin_id) => {
                            plugin_manager::NativePluginInstallOutcome::Conflict { plugin_id }
                        }
                        None => plugin_manager::NativePluginInstallOutcome::Failed,
                    }
                }
            };
            // Move the original request values into the result; no duplicate
            // URL or checksum is retained while the package worker runs.
            let _ = delivery_tx.send(plugin_manager::NativePluginManagerDelivery::Install {
                download_url,
                checksum,
                outcome,
            });
        });
        true
    }

    pub(in crate::workspace) fn start_update_check(
        &mut self,
        registry_url: Zeroizing<String>,
        installed: Vec<plugin_host::NativePluginInstalledInfo>,
    ) -> bool {
        if self.manager_operation_in_flight {
            return false;
        }
        self.manager_operation_in_flight = true;
        let delivery_tx = self.manager_delivery_tx.clone();
        self.task_runtime.spawn(async move {
            let result =
                match plugin_host::NativePluginRegistry::fetch_plugin_registry(registry_url.trim())
                    .await
                {
                    Ok(index) => Some(plugin_host::NativePluginRegistry::check_plugin_updates(
                        index, &installed,
                    )),
                    Err(error) => {
                        // Registry errors may echo credential-bearing URLs.
                        drop(Zeroizing::new(error));
                        None
                    }
                };
            let _ = delivery_tx.send(plugin_manager::NativePluginManagerDelivery::CheckUpdates(
                result,
            ));
        });
        true
    }

    pub(in crate::workspace) fn start_wasm_runtime_install(
        &mut self,
        settings_path: PathBuf,
    ) -> bool {
        if self.manager_operation_in_flight {
            return false;
        }
        self.manager_operation_in_flight = true;
        let delivery_tx = self.manager_delivery_tx.clone();
        self.task_runtime.spawn(async move {
            let result =
                oxideterm_plugin_runtime_install::install_wasm_runtime_sidecar(&settings_path)
                    .await;
            let result = result.map_err(Zeroizing::new).ok();
            let _ = delivery_tx
                .send(plugin_manager::NativePluginManagerDelivery::InstallWasmRuntime(result));
        });
        true
    }

    pub(in crate::workspace) fn take_manager_deliveries(
        &mut self,
    ) -> VecDeque<plugin_manager::NativePluginManagerDelivery> {
        std::mem::take(&mut self.manager_deliveries)
    }

    pub(in crate::workspace) fn runtime_request_senders(&self) -> PluginRuntimeRequestSenders {
        // These are lightweight channel endpoints; request payloads stay unique
        // and are moved through the channels without being cloned.
        PluginRuntimeRequestSenders {
            confirm: self.confirm_tx.clone(),
            terminal: self.terminal_tx.clone(),
            sync: self.sync_tx.clone(),
        }
    }

    pub(in crate::workspace) fn promote_confirm_request(&mut self) -> bool {
        if self.confirm.is_some() {
            return false;
        }
        let Ok(request) = self.confirm_rx.try_recv() else {
            return false;
        };
        self.confirm = Some(request.into());
        self.confirm_presence.reopen();
        true
    }

    pub(in crate::workspace) fn confirm_dialog(&self) -> Option<&NativePluginConfirmDialog> {
        self.confirm.as_ref()
    }

    pub(in crate::workspace) fn confirm_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.confirm_presence.phase()
    }

    pub(in crate::workspace) fn begin_confirm_exit(&mut self, confirmed: bool) -> Option<u64> {
        let dialog = self.confirm.as_ref()?;
        let generation = self.confirm_presence.begin_exit()?;
        // Resolve exactly once while retaining the dialog for its exit frame.
        dialog.respond(confirmed);
        Some(generation)
    }

    pub(in crate::workspace) fn finish_confirm_exit(&mut self, generation: u64) -> bool {
        if !self.confirm_presence.finish_exit(generation) {
            return false;
        }
        self.confirm = None;
        self.promote_confirm_request()
    }

    pub(in crate::workspace) fn take_terminal_requests(
        &mut self,
    ) -> delivery::ChannelDrain<NativePluginTerminalRequest> {
        delivery::drain_channel(&self.terminal_rx, delivery::USER_ACTION_DELIVERY_BUDGET)
    }

    pub(in crate::workspace) fn take_sync_requests(
        &mut self,
    ) -> delivery::ChannelDrain<NativePluginSyncRequest> {
        delivery::drain_channel(&self.sync_rx, delivery::USER_ACTION_DELIVERY_BUDGET)
    }

    pub(in crate::workspace) fn enqueue_product_ui_effect(
        &mut self,
        effect: NativePluginProductUiEffect,
    ) {
        self.product_ui_effects.push_back(effect);
        self.runtime_request_wake.mark();
    }

    pub(in crate::workspace) fn take_product_ui_effects(
        &mut self,
    ) -> (VecDeque<NativePluginProductUiEffect>, bool) {
        let started_at = Instant::now();
        let mut effects = VecDeque::new();
        while delivery::USER_ACTION_DELIVERY_BUDGET.allows_next(effects.len(), started_at.elapsed())
        {
            let Some(effect) = self.product_ui_effects.pop_front() else {
                break;
            };
            effects.push_back(effect);
        }
        (effects, !self.product_ui_effects.is_empty())
    }

    pub(in crate::workspace) fn mark_runtime_requests_ready(&self) {
        self.runtime_request_wake.mark();
    }

    fn schedule_manager_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.manager_delivery_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Releasing the window stops only its waiter; the workspace runtime
            // still owns any package operation until that task finishes.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_manager_deliveries(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_runtime_request_delivery(&self, cx: &mut Context<Self>) {
        let request_wake = self.runtime_request_wake.clone();
        let release_wake = request_wake.clone();
        cx.on_release(move |_, _| {
            // The entity owns this waiter for the full workspace lifetime.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |_entity, cx| {
            loop {
                request_wake.wait().await;
                let should_deliver = request_wake.take();
                let stopped = request_wake.is_stopped();
                if should_deliver {
                    let _ = _entity.update(cx, |_entity, cx| {
                        cx.emit(PluginWorkspaceEvent::RuntimeRequestsReady);
                    });
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_manager_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let drain = delivery::drain_channel(
            &self.manager_delivery_rx,
            delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        if !drain.items.is_empty() {
            self.manager_operation_in_flight = false;
            self.manager_deliveries.extend(drain.items);
            cx.emit(PluginWorkspaceEvent::ManagerDeliveryReady);
            cx.notify();
        }
        drain.outcome.backlog_remaining
    }
}

impl gpui::EventEmitter<PluginWorkspaceEvent> for PluginWorkspaceEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn manager_operation_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| PluginWorkspaceEntity::new(runtime, cx));
        let delivery_tx = entity.update(cx, |entity, _cx| {
            entity.manager_operation_in_flight = true;
            entity.manager_delivery_tx.clone()
        });
        delivery_tx
            .send(plugin_manager::NativePluginManagerDelivery::CheckUpdates(
                Some(Vec::new()),
            ))
            .expect("manager delivery");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(!entity.manager_operation_in_flight());
            assert!(matches!(
                entity.take_manager_deliveries().pop_front(),
                Some(plugin_manager::NativePluginManagerDelivery::CheckUpdates(Some(
                    updates
                ))) if updates.is_empty()
            ));
        });
    }

    #[gpui::test]
    fn runtime_requests_and_confirm_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| PluginWorkspaceEntity::new(runtime, cx));
        let senders = entity.read_with(cx, |entity, _cx| entity.runtime_request_senders());
        let (first_confirm_tx, first_confirm_rx) = std::sync::mpsc::channel();
        let (second_confirm_tx, _second_confirm_rx) = std::sync::mpsc::channel();
        let (terminal_response_tx, _terminal_response_rx) = std::sync::mpsc::channel();
        let (sync_response_tx, _sync_response_rx) = std::sync::mpsc::channel();

        senders
            .confirm
            .send(NativePluginConfirmRequest {
                plugin_id: "plugin.test".to_string(),
                request_id: "confirm-first".to_string(),
                title: "First".to_string(),
                description: "First request".to_string(),
                response_tx: first_confirm_tx,
            })
            .expect("first confirm request");
        senders
            .confirm
            .send(NativePluginConfirmRequest {
                plugin_id: "plugin.test".to_string(),
                request_id: "confirm-second".to_string(),
                title: "Second".to_string(),
                description: "Second request".to_string(),
                response_tx: second_confirm_tx,
            })
            .expect("second confirm request");
        senders
            .terminal
            .send(NativePluginTerminalRequest {
                request_id: "terminal-clear".to_string(),
                action: NativePluginTerminalAction::ClearBuffer {
                    node_id: "node-test".to_string(),
                },
                response_tx: terminal_response_tx,
            })
            .expect("terminal request");
        senders
            .sync
            .send(NativePluginSyncRequest {
                request_id: "sync-progress".to_string(),
                action: NativePluginSyncAction::ReportProgress {
                    plugin_id: "plugin.test".to_string(),
                    registration_id: "progress-test".to_string(),
                    value: serde_json::json!({"current": 1}),
                },
                response_tx: sync_response_tx,
            })
            .expect("sync request");
        entity.update(cx, |entity, _cx| {
            entity.enqueue_product_ui_effect(NativePluginProductUiEffect {
                plugin_id: "plugin.test".to_string(),
                namespace: "connections".to_string(),
                method: "connect".to_string(),
                args: serde_json::json!({"connectionId": "connection-test"}),
            });
        });

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(entity.promote_confirm_request());
            assert!(entity.confirm_dialog().is_some());
            let generation = entity
                .begin_confirm_exit(true)
                .expect("visible confirm generation");
            assert!(entity.finish_confirm_exit(generation));
            assert!(entity.confirm_dialog().is_some());

            let terminal_requests = entity.take_terminal_requests();
            assert_eq!(terminal_requests.items.len(), 1);
            assert!(!terminal_requests.outcome.backlog_remaining);

            let sync_requests = entity.take_sync_requests();
            assert_eq!(sync_requests.items.len(), 1);
            assert!(!sync_requests.outcome.backlog_remaining);

            let (product_effects, product_backlog) = entity.take_product_ui_effects();
            assert_eq!(product_effects.len(), 1);
            assert!(!product_backlog);
        });
        assert_eq!(first_confirm_rx.try_recv(), Ok(true));
    }

    #[gpui::test]
    fn entity_release_stops_plugin_delivery_waiters(cx: &mut TestAppContext) {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("plugin entity test runtime"),
        );
        let entity = cx.new(|cx| PluginWorkspaceEntity::new(runtime, cx));
        let (manager_wake, runtime_request_wake) = cx.read(|cx| {
            let entity = entity.read(cx);
            (
                entity.manager_delivery_tx.wake(),
                entity.runtime_request_wake.clone(),
            )
        });

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        // Releasing the Entity ends UI delivery without cancelling backend work.
        assert!(manager_wake.is_stopped());
        assert!(runtime_request_wake.is_stopped());
    }
}
