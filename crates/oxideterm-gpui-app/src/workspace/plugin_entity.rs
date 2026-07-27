// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use zeroize::Zeroizing;

pub(in crate::workspace) enum PluginWorkspaceEvent {
    ManagerDeliveryReady,
}

/// Owns plugin workers and reliable delivery independently from plugin page visibility.
pub(in crate::workspace) struct PluginWorkspaceEntity {
    task_runtime: Arc<tokio::runtime::Runtime>,
    manager_operation_in_flight: bool,
    manager_delivery_tx:
        delivery::ActiveDeliverySender<plugin_manager::NativePluginManagerDelivery>,
    manager_delivery_rx: std::sync::mpsc::Receiver<plugin_manager::NativePluginManagerDelivery>,
    manager_deliveries: VecDeque<plugin_manager::NativePluginManagerDelivery>,
}

impl PluginWorkspaceEntity {
    pub(in crate::workspace) fn new(
        task_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (manager_delivery_tx, manager_delivery_rx) = delivery::ActiveDeliverySender::channel();
        let entity = Self {
            task_runtime,
            manager_operation_in_flight: false,
            manager_delivery_tx,
            manager_delivery_rx,
            manager_deliveries: VecDeque::new(),
        };
        entity.schedule_manager_delivery(cx);
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
}
