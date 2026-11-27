use super::*;
use oxideterm_discovery::{DiscoveryFailure, DiscoveryProvider, DiscoverySource};
use oxideterm_plugin_protocol::discovery::*;
use oxideterm_plugin_registry::NativePluginDiscoveryProviderDef;
use tokio_util::sync::CancellationToken;

mod open;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct DiscoveryInstances {
    entries: HashMap<String, OpenInstance>,
}

struct OpenInstance {
    plugin: String,
    generation: u64,
    target: String,
    provider: String,
    source: String,
    cancel: CancellationToken,
    terminal: Option<SharedTerminalSession>,
    cleanup: Option<(InstanceCleanup, oxideterm_discovery::DiscoveryLease)>,
    owner: std::sync::Weak<()>,
    /// Only an established SSH transport delivered to the host survives provider exit.
    host_ssh: Option<std::sync::Weak<std::sync::atomic::AtomicBool>>,
}

struct InstanceCleanup {
    _lifetime: Arc<()>,
    process: DiscoveryProcess,
    source_id: String,
    instance_id: String,
    stage: CleanupStage,
    tracking: Option<oxideterm_discovery::DiscoveryPendingOpen>,
}

#[derive(Clone, Copy)]
enum CleanupStage {
    None,
    Choosing,
    Provider,
}

impl Drop for InstanceCleanup {
    fn drop(&mut self) {
        let request = match self.stage {
            CleanupStage::None => return,
            CleanupStage::Choosing => DiscoveryRequest::CancelOpen {
                instance_id: self.instance_id.clone(),
            },
            CleanupStage::Provider => DiscoveryRequest::Close {
                instance_id: self.instance_id.clone(),
            },
        };
        let process = self.process.clone();
        let source = self.source_id.clone();
        let tracking = self.tracking.take();
        self.process.runtime.spawn(async move {
            let _tracking = tracking;
            let _ = process
                .call(&source, request, NATIVE_PLUGIN_LIFECYCLE_TIMEOUT)
                .await;
        });
    }
}

impl DiscoveryInstances {
    fn invalidate(&mut self, id: &str) {
        if let Some(entry) = self.entries.get_mut(id)
            && let Some(terminal) = &entry.terminal
        {
            entry.cancel.cancel();
            terminal.lock().shutdown();
            // Keep remote provenance and the reopen target until the pane actually closes.
            entry.cleanup.take();
        } else {
            self.remove(id);
        }
    }
    fn remove(&mut self, instance: &str) {
        if let Some(entry) = self.entries.remove(instance) {
            entry.cancel.cancel();
            if let Some(terminal) = entry.terminal {
                terminal.lock().shutdown();
            }
        }
    }

    pub(super) fn retire_plugin(&mut self, plugin: &str, stop: RuntimeStop) -> Vec<String> {
        let ids = self
            .entries
            .iter()
            .filter(|(_, e)| {
                e.plugin == plugin
                    && !(stop == RuntimeStop::Failure
                        && e.host_ssh
                            .as_ref()
                            .and_then(std::sync::Weak::upgrade)
                            .is_some_and(|established| {
                                established.load(std::sync::atomic::Ordering::Acquire)
                            }))
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in &ids {
            self.invalidate(id);
        }
        ids
    }

    pub(super) fn invalidate_event<'a>(
        &mut self,
        plugin: &str,
        generation: Option<u64>,
        payload: &'a serde_json::Value,
    ) -> Option<&'a str> {
        let id = payload["instanceId"].as_str()?;
        let e = self.entries.get(id)?;
        if e.plugin != plugin
            || generation != Some(e.generation)
            || Some(e.source.as_str()) != payload["sourceId"].as_str()
            || Some(e.provider.as_str()) != payload["providerId"].as_str()
            || e.cancel.is_cancelled()
        {
            return None;
        }
        self.invalidate(id);
        Some(id)
    }
}

impl Drop for DiscoveryInstances {
    fn drop(&mut self) {
        for (_, entry) in self.entries.drain() {
            entry.cancel.cancel();
            if let Some(terminal) = entry.terminal {
                terminal.lock().shutdown();
            }
        }
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct DiscoveryProcess {
    plugin: String,
    live: CancellationToken,
    generation: u64,
    definition: NativePluginDiscoveryProviderDef,
    runtime: Arc<tokio::runtime::Runtime>,
    host: Arc<tokio::sync::Mutex<plugin_runtime::NativePluginRuntimeHost>>,
    delivery: delivery::ActiveDeliverySender<NativePluginRuntimeDelivery>,
}

impl PluginWorkspaceEntity {
    pub(in crate::workspace) fn take_discovery_catalog_changed(&mut self) -> bool {
        std::mem::take(&mut self.discovery_catalog_changed)
    }
    pub(in crate::workspace) fn discovery_process(
        &self,
        plugin_id: &str,
        provider_id: &str,
    ) -> Option<DiscoveryProcess> {
        let generation = *self.runtime_generations.get(plugin_id)?;
        let (_, definition) = self
            .registry()
            .discovery_providers()
            .find(|(p, d)| *p == plugin_id && d.id == provider_id)?;
        Some(DiscoveryProcess {
            plugin: plugin_id.into(),
            live: self.discovery_lifetimes.get(plugin_id)?.clone(),
            generation,
            definition: definition.clone(),
            runtime: self.task_runtime.clone(),
            host: self.runtime_hosts.for_plugin(plugin_id),
            delivery: self.runtime_delivery_tx.clone(),
        })
    }
    pub(in crate::workspace) fn discovery_terminal(
        &self,
        terminal: &SharedTerminalSession,
    ) -> bool {
        self.discovery_terminal_instance(terminal).is_some()
    }

    fn discovery_terminal_instance(
        &self,
        terminal: &SharedTerminalSession,
    ) -> Option<&OpenInstance> {
        self.discovery_instances.entries.values().find(|entry| {
            entry
                .terminal
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, terminal))
        })
    }

    pub(in crate::workspace) fn discovery_terminal_is_live(
        &self,
        terminal: &SharedTerminalSession,
    ) -> bool {
        self.discovery_terminal_instance(terminal)
            .is_some_and(|entry| {
                !entry.cancel.is_cancelled() && terminal.lock().lifecycle().is_running()
            })
    }
    pub(in crate::workspace) fn finish_discovery_terminal(
        &mut self,
        terminal: &SharedTerminalSession,
    ) -> bool {
        if let Some(entry) = self.discovery_instances.entries.values_mut().find(|e| {
            e.terminal
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, terminal))
        }) {
            entry.cancel.cancel();
            entry.cleanup.take();
            return true;
        }
        false
    }

    pub(in crate::workspace) fn release_discovery_terminal(
        &mut self,
        terminal: &SharedTerminalSession,
    ) {
        if let Some(id) = self
            .discovery_instances
            .entries
            .iter()
            .find(|(_, e)| {
                e.terminal
                    .as_ref()
                    .is_some_and(|t| Arc::ptr_eq(t, terminal))
            })
            .map(|(id, _)| id.clone())
        {
            self.discovery_instances.remove(&id);
        }
    }

    pub(in crate::workspace) fn discovery_terminal_target(
        &self,
        terminal: &SharedTerminalSession,
    ) -> Option<(String, String)> {
        let entry = self.discovery_terminal_instance(terminal)?;
        Some((entry.source.clone(), entry.target.clone()))
    }
}

impl DiscoveryProcess {
    fn bound_to(&self, source: &DiscoverySource) -> bool {
        source.enabled
            && source.plugin_id.as_deref() == Some(self.plugin.as_str())
            && source.provider_id == self.definition.id
    }
    pub(in crate::workspace) async fn call(
        &self,
        source_id: &str,
        request: DiscoveryRequest,
        timeout: Duration,
    ) -> Result<DiscoveryReply, DiscoveryFailure> {
        self.call_cancellable(source_id, request, timeout, None)
            .await
    }

    async fn call_cancellable(
        &self,
        source_id: &str,
        request: DiscoveryRequest,
        timeout: Duration,
        cancel: Option<CancellationToken>,
    ) -> Result<DiscoveryReply, DiscoveryFailure> {
        let handle = self.clone();
        let source = source_id.to_owned();
        let (reply, receiver) = tokio::sync::oneshot::channel();
        // A cancelled waiter cannot drop a half-read JSON Lines exchange. The process owner
        // drains it, then the waiting cancellation can acquire the serialized transport.
        self.runtime.spawn(async move {
            let mut host = handle.host.lock().await;
            if reply.is_closed() || cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return;
            }
            if host.process_generation(&handle.plugin) != Some(handle.generation) {
                return;
            }
            let delivery = handle.delivery.clone();
            let plugin = handle.plugin.clone();
            let generation = handle.generation;
            host.set_discovery_invalidation_sink(
                &handle.plugin,
                generation,
                Arc::new(move |event| {
                    let (consumed, _) = tokio::sync::oneshot::channel();
                    let push = plugin_runtime::NativePluginRuntimePush {
                        messages: vec![plugin_runtime::PluginOutboundMessage::EmitEvent { event }],
                        effects: vec![],
                    };
                    let _ = delivery.send(NativePluginRuntimeDelivery::Push {
                        plugin_id: plugin.clone(),
                        generation,
                        source_action_authority: None,
                        result: Ok(push),
                        consumed,
                    });
                }),
            );
            let cleanup = matches!(
                &request,
                DiscoveryRequest::Close { .. }
                    | DiscoveryRequest::CancelOpen { .. }
                    | DiscoveryRequest::CancelRefresh { .. }
            );
            let opening = match &request {
                DiscoveryRequest::PrepareOpen { instance_id, .. } => Some(instance_id.clone()),
                _ => None,
            };
            let source_action_authority =
                cancel.filter(|_| matches!(&request, DiscoveryRequest::SourceAction { .. }));
            let result = host
                .dispatch_discovery(
                    &handle.plugin,
                    handle.generation,
                    &source,
                    &handle.definition.id,
                    request,
                    timeout,
                )
                .await;
            use plugin_runtime::DiscoveryCommandError::{Provider, Rejected};
            let result = match result {
                Ok((Err(Provider(error) | Rejected(error)), _)) if cleanup => Err(error),
                result => result,
            };
            let (answer, push) = match result {
                Ok((answer, push)) => {
                    let answer = answer.map_err(|error| match error {
                        Provider(error) => DiscoveryFailure::Provider(error),
                        Rejected(error) if error.code == "permissionDenied" => {
                            DiscoveryFailure::PermissionDenied
                        }
                        Rejected(_) => DiscoveryFailure::InvalidResponse,
                    });
                    (answer, Ok(push))
                }
                Err(error) => {
                    let _ = host.deactivate_plugin(&handle.plugin).await;
                    (Err(DiscoveryFailure::Unavailable), Err(error))
                }
            };
            // Hold cleanup across delivery: cancellation can drop a buffered ready
            // before the awaiting workspace ever gets to inspect its owner.
            let guard = opening
                .filter(|_| {
                    matches!(
                        &answer,
                        Ok(DiscoveryReply::Open(DiscoveryOpen::Ready {
                            owner: DiscoveryOwner::Provider,
                            ..
                        }))
                    )
                })
                .map(|instance_id| InstanceCleanup {
                    _lifetime: Arc::new(()),
                    process: handle.clone(),
                    source_id: source.clone(),
                    instance_id,
                    stage: CleanupStage::Provider,
                    tracking: None,
                });
            drop(host);
            if push
                .as_ref()
                .map_or(true, |p| !p.messages.is_empty() || !p.effects.is_empty())
            {
                let (consumed, prior) = tokio::sync::oneshot::channel();
                if handle
                    .delivery
                    .send(NativePluginRuntimeDelivery::Push {
                        plugin_id: handle.plugin.clone(),
                        generation: handle.generation,
                        source_action_authority,
                        result: push,
                        consumed,
                    })
                    .is_err()
                {
                    return;
                }
                let _ = prior.await;
            }
            let _ = reply.send((answer, guard));
        });
        let (answer, mut guard) = receiver.await.map_err(|_| DiscoveryFailure::Unavailable)?;
        if let Some(guard) = &mut guard {
            // The caller now owns this ready reply and its provider cleanup.
            guard.stage = CleanupStage::None;
        }
        answer
    }
}

#[async_trait::async_trait]
impl DiscoveryProvider for DiscoveryProcess {
    fn supports_source_action(&self, action_id: &str) -> bool {
        self.definition
            .source_actions
            .iter()
            .any(|action| action.id == action_id)
    }
    async fn source_action(
        &self,
        source: &DiscoverySource,
        action_id: &str,
        cancel: CancellationToken,
    ) -> Result<(), DiscoveryFailure> {
        if !self.bound_to(source) || !self.supports_source_action(action_id) {
            return Err(DiscoveryFailure::Unavailable);
        }
        let request = DiscoveryRequest::SourceAction {
            action_id: action_id.into(),
            config: source.config.clone(),
        };
        let reply = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(DiscoveryFailure::Unavailable),
            reply = self.call_cancellable(
                &source.id, request, NATIVE_PLUGIN_LIFECYCLE_TIMEOUT, Some(cancel.clone()),
            ) => reply?,
        };
        match reply {
            DiscoveryReply::Done => Ok(()),
            _ => Err(DiscoveryFailure::InvalidResponse),
        }
    }
    fn is_live(&self) -> bool {
        !self.live.is_cancelled()
    }
    fn catalog_config(&self, source: &DiscoverySource) -> serde_json::Value {
        self.definition.config_for(
            &source.id, &source.config,
            oxideterm_plugin_registry::DiscoveryConfigUse::Catalog,
        )
    }
    fn generation(&self) -> u64 {
        self.generation
    }
    async fn list(
        &self,
        source: &DiscoverySource,
        refresh_id: &str,
        cursor: Option<String>,
        cancel: &CancellationToken,
    ) -> Result<oxideterm_discovery::DiscoveryPage, DiscoveryFailure> {
        if !self.bound_to(source) {
            return Err(DiscoveryFailure::Unavailable);
        }
        match self
            .call_cancellable(
                &source.id,
                DiscoveryRequest::List {
                    refresh_id: refresh_id.into(),
                    config: source.config.clone(),
                    cursor,
                },
                NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
                Some(cancel.clone()),
            )
            .await?
        {
            DiscoveryReply::List(page) => Ok(page),
            _ => Err(DiscoveryFailure::InvalidResponse),
        }
    }
    async fn cancel_list(&self, source: &DiscoverySource, refresh_id: &str) {
        let _ = self
            .call(
                &source.id,
                DiscoveryRequest::CancelRefresh {
                    refresh_id: refresh_id.into(),
                },
                NATIVE_PLUGIN_LIFECYCLE_TIMEOUT,
            )
            .await;
    }
}
