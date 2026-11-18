use std::{sync::Arc, time::Duration};

use oxideterm_connections::DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS;
use oxideterm_ssh::{
    ConnectionConsumer, HostKeyStatus, ManagedKeyResolver, NativeSessionTreeConnectStep,
    NodeRouter, SshConfig, SshConnectionRegistry, SshTransportClient, check_host_key_with_route,
    check_host_key_with_upstream_proxy,
};
use oxideterm_workspace::delivery::ActiveDeliverySender;

use crate::{
    entity::ProxyConnectPreflightContext,
    launch::{NativeSshPromptHandler, SshConnectionIntent, SshConnectionWorkerResult},
};

pub fn preflight(
    mut config: SshConfig,
    title: String,
    intent: SshConnectionIntent,
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
    managed_key_resolver: ManagedKeyResolver,
) {
    let host = config.host.clone();
    let port = config.port;
    let connect_timeout_seconds = config.timeout_secs;
    let upstream_proxy = config.upstream_proxy.take();
    let worker_config = config;
    let worker_title = title;
    let routed_preflight = worker_config
        .proxy_chain
        .as_ref()
        .is_some_and(|chain| !chain.is_empty())
        && matches!(
            &intent,
            SshConnectionIntent::StandaloneSftp { .. }
                | SshConnectionIntent::StandaloneSftpSecondary { .. }
                | SshConnectionIntent::TestStandaloneSftp
                | SshConnectionIntent::Mosh(_)
        );
    let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
    std::thread::spawn(move || {
        let (checked_host, checked_port, status) = match tokio::runtime::Runtime::new() {
            Ok(runtime) if routed_preflight => {
                // Routed SFTP and Mosh preflight authenticate through every hop. The
                // bounded zeroizing copy survives only until the accepted launch starts.
                let mut route_config = worker_config.clone();
                route_config.upstream_proxy = upstream_proxy.clone();
                runtime.block_on(
                    SshTransportClient::new(route_config)
                        .with_prompt_handler(prompt_handler)
                        .with_managed_key_resolver(managed_key_resolver)
                        .preflight_route_host_keys(),
                )
            }
            Ok(runtime) => (
                host.clone(),
                port,
                runtime.block_on(check_host_key_with_route(
                    &host,
                    port,
                    connect_timeout_seconds,
                    upstream_proxy.as_ref(),
                    worker_config.proxy_command.as_ref(),
                    worker_config.legacy_ssh_compatibility,
                    &worker_config.ssh_algorithms,
                )),
            ),
            Err(error) => (
                host.clone(),
                port,
                HostKeyStatus::Error {
                    message: format!("failed to initialize SSH runtime: {error}"),
                },
            ),
        };
        let _ = tx.send(SshConnectionWorkerResult::Preflight {
            config: worker_config,
            upstream_proxy,
            title: worker_title,
            intent,
            host: checked_host,
            port: checked_port,
            status,
        });
    });
}

pub fn preflight_tree_step(
    router: NodeRouter,
    step: NativeSessionTreeConnectStep,
    preflight_context: ProxyConnectPreflightContext,
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
) {
    let (connect_timeout_seconds, legacy_compatibility, algorithms) = router
        .node_runtime_snapshot(&step.node_id)
        .map(|snapshot| {
            (
                snapshot.config.timeout_secs,
                snapshot.config.legacy_ssh_compatibility,
                snapshot.config.ssh_algorithms,
            )
        })
        .unwrap_or((
            DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            false,
            Default::default(),
        ));
    let ProxyConnectPreflightContext {
        generation,
        step_index,
        upstream_proxy,
    } = preflight_context;
    std::thread::spawn(move || {
        let status = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime.block_on(async {
                match router
                    .node_runtime_snapshot(&step.node_id)
                    .and_then(|snapshot| snapshot.parent_id)
                {
                    Some(parent_id) => {
                        let consumer =
                            ConnectionConsumer::NodeRouter(format!("{}:preflight", step.node_id.0));
                        match router
                            .acquire_connection_wait(
                                &parent_id,
                                consumer.clone(),
                                Duration::from_secs(connect_timeout_seconds),
                            )
                            .await
                        {
                            Ok(parent) => {
                                let connection_id = parent.connection_id.clone();
                                let status = parent
                                    .handle
                                    .preflight_host_key_via_direct_tcpip(
                                        &step.host,
                                        step.port,
                                        connect_timeout_seconds,
                                        legacy_compatibility,
                                        &algorithms,
                                    )
                                    .await;
                                router.release_consumer(&connection_id, &consumer);
                                status
                            }
                            Err(error) => HostKeyStatus::Error {
                                message: error.to_string(),
                            },
                        }
                    }
                    None => {
                        // The root ProxyJump step uses the same initial TCP outlet
                        // as the eventual SSH connection; child steps keep using
                        // parent direct-tcpip streams.
                        check_host_key_with_upstream_proxy(
                            &step.host,
                            step.port,
                            connect_timeout_seconds,
                            upstream_proxy.as_ref(),
                            legacy_compatibility,
                            &algorithms,
                        )
                        .await
                    }
                }
            }),
            Err(error) => HostKeyStatus::Error {
                message: format!("failed to initialize SSH runtime: {error}"),
            },
        };
        let _ = tx.send(SshConnectionWorkerResult::SessionTreePreflight {
            generation,
            step_index,
            upstream_proxy,
            status,
        });
    });
}

pub fn test_ssh(
    config: SshConfig,
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
    managed_key_resolver: ManagedKeyResolver,
) {
    std::thread::spawn(move || {
        let result = match tokio::runtime::Runtime::new() {
            Ok(runtime) => {
                let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
                runtime
                    .block_on(
                        SshTransportClient::new(config)
                            .with_prompt_handler(prompt_handler)
                            .with_managed_key_resolver(managed_key_resolver)
                            .test_connection(),
                    )
                    .map_err(|error| error.to_string())
            }
            Err(error) => Err(format!("failed to initialize SSH runtime: {error}")),
        };
        let _ = tx.send(SshConnectionWorkerResult::Test { result });
    });
}

async fn open_sftp_endpoint(
    config: SshConfig,
    registry: &SshConnectionRegistry,
    consumer: &ConnectionConsumer,
    prompt: Arc<NativeSshPromptHandler>,
    resolver: ManagedKeyResolver,
) -> Result<oxideterm_ssh::SshConnectionHandle, String> {
    let handle = SshTransportClient::new(config)
        .with_prompt_handler(prompt)
        .with_managed_key_resolver(resolver)
        .connect_dedicated_node_with_registry(registry.clone(), consumer.clone())
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = handle.acquire_sftp().await {
        registry.release(handle.connection_id(), consumer);
        return Err(error.to_string());
    }
    Ok(handle)
}

pub fn connect_standalone_sftp(
    config: SshConfig,
    endpoint_id: String,
    saved_profile_id: Option<String>,
    title: String,
    initial_remote_path: Option<String>,
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
    resolver: ManagedKeyResolver,
    registry: SshConnectionRegistry,
    runtime: &tokio::runtime::Runtime,
) {
    let consumer = ConnectionConsumer::Sftp(format!("standalone-tab:{endpoint_id}"));
    let prompt = Arc::new(NativeSshPromptHandler::new(tx.clone()));
    runtime.spawn(async move {
        let result = open_sftp_endpoint(config, &registry, &consumer, prompt, resolver).await;
        let connection_id = result
            .as_ref()
            .ok()
            .map(|handle| handle.connection_id().to_owned());
        if tx
            .send(SshConnectionWorkerResult::StandaloneSftpConnected {
                endpoint_id,
                saved_profile_id,
                title,
                initial_remote_path,
                consumer: consumer.clone(),
                result,
            })
            .is_err()
            && let Some(id) = connection_id
        {
            // No runtime could adopt this consumer after the window released the flow.
            registry.release(&id, &consumer);
        }
    });
}

pub fn connect_standalone_sftp_pair(
    primary: SshConfig,
    secondary: SshConfig,
    launch: crate::launch::PendingStandaloneSftpPairLaunch,
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
    resolver: ManagedKeyResolver,
    registry: SshConnectionRegistry,
    runtime: &tokio::runtime::Runtime,
) {
    let primary_endpoint_id = launch.saved_profile_id.clone();
    let secondary_endpoint_id = format!("{}:secondary", launch.saved_profile_id);
    let primary_consumer =
        ConnectionConsumer::Sftp(format!("standalone-tab:{primary_endpoint_id}:primary"));
    let secondary_consumer =
        ConnectionConsumer::Sftp(format!("standalone-tab:{primary_endpoint_id}:secondary"));
    let prompt = Arc::new(NativeSshPromptHandler::new(tx.clone()));
    runtime.spawn(async move {
        let result = async {
            let first = open_sftp_endpoint(
                primary,
                &registry,
                &primary_consumer,
                prompt.clone(),
                resolver.clone(),
            )
            .await?;
            match open_sftp_endpoint(secondary, &registry, &secondary_consumer, prompt, resolver)
                .await
            {
                Ok(second) => Ok((first, second)),
                Err(error) => {
                    registry.release(first.connection_id(), &primary_consumer);
                    Err(error)
                }
            }
        }
        .await;
        let ids = result.as_ref().ok().map(|(first, second)| {
            (
                first.connection_id().to_owned(),
                second.connection_id().to_owned(),
            )
        });
        if tx
            .send(SshConnectionWorkerResult::StandaloneSftpPairConnected {
                saved_profile_id: launch.saved_profile_id,
                title: launch.title,
                primary_endpoint_id,
                secondary_endpoint_id,
                primary_initial_remote_path: launch.primary_initial_remote_path,
                secondary_initial_remote_path: launch.secondary_initial_remote_path,
                primary_consumer: primary_consumer.clone(),
                secondary_consumer: secondary_consumer.clone(),
                result,
            })
            .is_err()
            && let Some((first, second)) = ids
        {
            registry.release(&first, &primary_consumer);
            registry.release(&second, &secondary_consumer);
        }
    });
}

pub fn test_standalone_sftp(
    config: SshConfig,
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
    managed_key_resolver: ManagedKeyResolver,
    registry: SshConnectionRegistry,
    runtime: &tokio::runtime::Runtime,
) {
    let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
    let consumer = ConnectionConsumer::Sftp(format!("standalone-test:{}", uuid::Uuid::new_v4()));
    runtime.spawn(async move {
        let client = SshTransportClient::new(config)
            .with_prompt_handler(prompt_handler)
            .with_managed_key_resolver(managed_key_resolver);
        let result = match client
            .connect_dedicated_node_with_registry(registry.clone(), consumer.clone())
            .await
        {
            Ok(handle) => {
                let result = handle
                    .acquire_sftp()
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                registry.release(handle.connection_id(), &consumer);
                result
            }
            Err(error) => Err(error.to_string()),
        };
        let _ = tx.send(SshConnectionWorkerResult::Test { result });
    });
}
