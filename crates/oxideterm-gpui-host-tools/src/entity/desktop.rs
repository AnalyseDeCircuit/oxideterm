use super::*;

pub(super) struct DesktopHost {
    pub(super) profiler_update_tx: tokio::sync::mpsc::Sender<()>,
    pub(super) registry: SshConnectionRegistry,
    ssh_consumer_context: Option<HostToolsSshConsumerContext>,
}
#[derive(Clone)]
struct HostToolsSshConsumerContext {
    node_router: NodeRouter,
    prompt_handler: Arc<dyn SshPromptHandler>,
    managed_key_resolver: ManagedKeyResolver,
}

impl DesktopHost {
    pub(super) fn new(
        registry: SshConnectionRegistry,
        profiler_update_tx: tokio::sync::mpsc::Sender<()>,
    ) -> Self {
        Self {
            profiler_update_tx,
            registry,
            ssh_consumer_context: None,
        }
    }
    pub(super) fn set_consumer_context(
        &mut self,
        node_router: NodeRouter,
        prompt_handler: Arc<dyn SshPromptHandler>,
        managed_key_resolver: ManagedKeyResolver,
    ) {
        self.ssh_consumer_context = Some(HostToolsSshConsumerContext {
            node_router,
            prompt_handler,
            managed_key_resolver,
        });
    }
    fn remote_sampler(
        &self,
        handle: oxideterm_ssh::SshConnectionHandle,
    ) -> Arc<dyn ResourceSampler> {
        if let Some(context) = self.ssh_consumer_context.as_ref()
            && let Some(node_id) = context
                .node_router
                .node_id_for_connection(handle.connection_id())
            && context
                .node_router
                .node_runtime_snapshot(&node_id)
                .is_some_and(|snapshot| {
                    snapshot
                        .config
                        .ssh_channel_strategy
                        .requires_dedicated_consumers()
                })
        {
            return Arc::new(oxideterm_ssh::DedicatedNodeResourceSampler::new(
                context.node_router.clone(),
                node_id,
                context.prompt_handler.clone(),
                context.managed_key_resolver.clone(),
            ));
        }
        Arc::new(handle)
    }

    pub(super) fn monitor_connections(
        &self,
        summaries: &[ConnectionPoolEntrySummary],
        messages: Option<&HostToolsMessages>,
    ) -> Vec<MonitorConnectionOption> {
        let mut connections = if !summaries.is_empty() {
            summaries
                .iter()
                .filter(|summary| summary.is_displayed_in_pool())
                .map(MonitorConnectionOption::from_pool_summary)
                .collect::<Vec<_>>()
        } else {
            let mut connections = self
                .registry
                .list()
                .into_iter()
                .map(MonitorConnectionOption::from_connection_info)
                .collect::<Vec<_>>();
            connections.sort_by(|left, right| {
                monitor_connection_label(left).cmp(&monitor_connection_label(right))
            });
            connections
        };
        // The local target stays last so an existing SSH selection keeps its
        // position. It appears once the localized label has been delivered.
        if let Some(label) = messages.map(|messages| messages.local_connection_label.clone()) {
            connections.push(MonitorConnectionOption::local(label));
        }
        connections
    }
    pub(super) fn connection_os_type(&self, connection_id: &str) -> Option<String> {
        if connection_id == LOCAL_HOST_CONNECTION_ID {
            return Some(local_host_os_type().to_string());
        }
        self.registry.get(connection_id).map(|handle| {
            handle
                .remote_env()
                .map(|environment| environment.os_type)
                .unwrap_or_else(|| "Unknown".to_string())
        })
    }
    pub(super) fn host_command_target(&self, connection_id: &str) -> Option<HostCaptureTarget> {
        if connection_id == LOCAL_HOST_CONNECTION_ID {
            return Some(Arc::new(HostCommandTarget::Local));
        }
        self.registry
            .get(connection_id)
            .map(|h| Arc::new(HostCommandTarget::Ssh(h)) as HostCaptureTarget)
    }
    pub(super) fn remote_env_detection_running(&self, connection_id: &str) -> bool {
        self.registry
            .get(connection_id)
            .is_some_and(|handle| handle.remote_env_detection_running())
    }
}

impl HostToolsEntity {
    pub(super) fn desktop_sampler(
        &self,
        host: &DesktopHost,
        id: &str,
    ) -> Option<(Arc<dyn ResourceSampler>, String)> {
        if id == LOCAL_HOST_CONNECTION_ID {
            return Some((
                Arc::new(LocalResourceSampler),
                local_host_os_type().to_owned(),
            ));
        }
        let handle = host.registry.get(id)?;
        // Environment detection owns the dialect used by both samplers.
        let os = handle.remote_env()?.os_type;
        #[cfg(test)]
        if let Some(sampler) = self.test_resource_sampler.as_ref() {
            return Some((sampler.clone(), os));
        }
        Some((host.remote_sampler(handle), os))
    }
}
