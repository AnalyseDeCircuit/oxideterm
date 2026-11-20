use super::*;
use gpui::TestAppContext;
use oxideterm_connection_monitor::multiplexer::{MuxKind, MuxNode, MuxStatus};
use oxideterm_connection_monitor::{
    GPU_END_MARKER, ProfilerState, ResourceSampleShell, ResourceSamplerFuture,
};
use oxideterm_ssh::{AuthMethod, ConnectionConsumer, RemoteEnvInfo, SshCommandOutput, SshConfig};
use std::sync::atomic::{AtomicUsize, Ordering};

#[gpui::test]
fn filesystem_panel_routes_search_edits_only_while_focused(cx: &mut TestAppContext) {
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    entity.update(cx, |entity, _cx| {
        let input = HostToolsTextInput::FilesystemSearch;
        entity.ui.focus_input(input);
        assert!(entity.replace_text_input(input, None, "/卷"));
        assert_eq!(entity.filesystem_search_value(), Some("/卷"));

        entity.ui.focus_input(HostToolsTextInput::PortSearch);
        assert!(!entity.replace_text_input(input, None, "stray commit"));
        assert_eq!(entity.filesystem_search_value(), None);
        entity.ui.focus_input(input);
        assert_eq!(entity.filesystem_search_value(), Some("/卷"));
    });
}

struct VisibilityCountingSampler {
    shell_open_count: Arc<AtomicUsize>,
    shell_close_count: Arc<AtomicUsize>,
}

impl ResourceSampler for VisibilityCountingSampler {
    fn open_shell<'a>(
        &'a self,
        _init_command: &'a str,
        _timeout: Duration,
    ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
        let shell_open_count = self.shell_open_count.clone();
        let shell_close_count = self.shell_close_count.clone();
        Box::pin(async move {
            shell_open_count.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(VisibilityCountingShell { shell_close_count })
                as Box<dyn ResourceSampleShell>)
        })
    }
}

struct VisibilityCountingShell {
    shell_close_count: Arc<AtomicUsize>,
}

impl ResourceSampleShell for VisibilityCountingShell {
    fn sample_until<'a>(
        &'a mut self,
        _command: &'a str,
        end_marker: &'a str,
        _timeout: Duration,
        _max_output_size: usize,
    ) -> ResourceSamplerFuture<'a, Result<String, oxideterm_connection_monitor::ResourceSampleError>>
    {
        Box::pin(async move {
            if end_marker == GPU_END_MARKER {
                return Ok(concat!(
                    "===NVIDIA_STATUS===\navailable\n",
                    "===NVIDIA_GPUS===\n",
                    "0, GPU-a, 00000000:01:00.0, NVIDIA L40S, 555.42, P0, 10, 2, 512, 46068, 41, 50, 350, N/A\n",
                    "===NVIDIA_GPU_QUERY_EXIT===\n0\n",
                    "===NVIDIA_PROCESSES===\n",
                    "===NVIDIA_GPU_END==="
                )
                .to_string());
            }
            Ok("===END===\n".to_string())
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.shell_close_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

fn wait_for_counter(counter: &AtomicUsize, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while counter.load(Ordering::SeqCst) < expected {
        assert!(
            Instant::now() < deadline,
            "counter did not reach {expected} before the deadline"
        );
        std::thread::yield_now();
    }
}

#[gpui::test]
fn visibility_stops_page_samplers_but_keeps_reliable_actions_and_node_owner(
    cx: &mut TestAppContext,
) {
    // Real Tokio samplers wake delivery tasks from outside the GPUI test scheduler.
    cx.executor().allow_parking();
    let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
    let registry = SshConnectionRegistry::default();
    let node_consumer = ConnectionConsumer::NodeRouter("node-visibility".to_string());
    let handle = registry.acquire(
        SshConfig {
            host: "host.example".to_string(),
            username: "alice".to_string(),
            auth: AuthMethod::Agent,
            ..SshConfig::default()
        },
        node_consumer.clone(),
    );
    assert!(handle.set_remote_env(RemoteEnvInfo {
        os_type: "Linux".to_string(),
        os_version: None,
        kernel: None,
        arch: None,
        shell: Some("/bin/sh".to_string()),
        home: None,
        zdotdir: None,
        xdg_config_home: None,
        detected_at: 1,
    }));
    let connection_id = handle.connection_id().to_string();
    let shell_open_count = Arc::new(AtomicUsize::new(0));
    let shell_close_count = Arc::new(AtomicUsize::new(0));
    let test_sampler: Arc<dyn ResourceSampler> = Arc::new(VisibilityCountingSampler {
        shell_open_count: shell_open_count.clone(),
        shell_close_count: shell_close_count.clone(),
    });
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity =
        cx.new(|cx| HostToolsEntity::new(profiler_update_tx, profiler_update_rx, registry, cx));
    let mut events = cx.events(&entity);
    let monitoring = oxideterm_settings::HostToolsSettings::default();
    let sampling_config = oxideterm_connection_monitor::ResourceSamplingConfig::default();

    entity.update(cx, |entity, cx| {
        assert!(entity.lifecycle_refresh_task.is_some());
        entity.lifecycle_runtime = Some(runtime.handle().clone());
        entity.last_pool_refresh = None;
        entity.visibility = HostToolsVisibility::Hidden;
        entity.refresh_lifecycle_tick(cx);
        assert!(entity.last_pool_refresh.is_none());
        entity.visibility = HostToolsVisibility::VisibleMainTab;
        entity.refresh_lifecycle_tick(cx);
        assert!(entity.last_pool_refresh.is_some());
        entity.test_resource_sampler = Some(test_sampler);
        entity.test_snapshot_dispatches = Some(Vec::new());
        entity.messages = Some(HostToolsMessages {
            service_connection_missing: "Service connection missing".to_string(),
            service_action_failed: "Service action failed".to_string(),
            log_unknown_error: "Log capture failed".to_string(),
            port_unknown_error: "Port capture failed".to_string(),
            filesystem_unknown_error: "Filesystem capture failed".to_string(),
            package_unknown_error: "Package capture failed".to_string(),
            schedule_unknown_error: "Schedule capture failed".to_string(),
            tmux_unknown_error: "tmux capture failed".to_string(),
            tmux_unavailable: "tmux unavailable".to_string(),
            local_connection_label: "This machine".to_string(),
            ..HostToolsMessages::default()
        });
        entity.select_connection(connection_id.clone(), None, cx);
        entity.active_tool = ContextSidebarTool::Gpu;
        entity.update_lifecycle(
            HostToolsVisibility::VisibleSidebar,
            monitoring.clone(),
            sampling_config,
            runtime.handle().clone(),
            true,
            cx,
        );
    });

    // Visible GPU owns one profiler shell and one page-scoped GPU shell.
    wait_for_counter(&shell_open_count, 2);
    entity.read_with(cx, |entity, _cx| {
        assert_eq!(
            entity.profiler_registry.state(&connection_id),
            Some(ProfilerState::Running)
        );
        assert!(entity.gpu_sampling_is_running(&connection_id));
    });

    let action_request = HostServiceActionRequest {
        connection_id: connection_id.clone(),
        service_id: "ssh.service".to_string(),
        description: "SSH".to_string(),
        action: ServiceActionKind::Restart,
    };
    let reliable_sender = entity.update(cx, |entity, cx| {
        entity.host_services.action_running = Some(action_request.clone());
        entity.update_lifecycle(
            HostToolsVisibility::Hidden,
            monitoring.clone(),
            sampling_config,
            runtime.handle().clone(),
            false,
            cx,
        );
        entity.active_tool = ContextSidebarTool::Services;
        entity.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
        entity.reliable_delivery_tx.clone()
    });

    wait_for_counter(&shell_close_count, 2);
    let hidden_shell_open_count = shell_open_count.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(
        shell_open_count.load(Ordering::SeqCst),
        hidden_shell_open_count
    );
    entity.read_with(cx, |entity, _cx| {
        assert_eq!(entity.profiler_registry.state(&connection_id), None);
        assert!(!entity.gpu_sampling_is_running(&connection_id));
        assert!(
            entity
                .test_snapshot_dispatches
                .as_ref()
                .expect("snapshot test hook")
                .is_empty()
        );
    });

    reliable_sender
        .send(super::delivery::HostToolsReliableDelivery::ServiceAction(
            HostServiceActionDelivery {
                request: action_request,
                result: Ok(true),
            },
        ))
        .expect("deliver hidden-page action result");
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_services.action_running.is_none());
    });
    assert_eq!(
        events.try_recv().expect("hidden action completion notice"),
        HostToolsEvent::ShowNotice(HostToolsNotice::ServiceActionFinished {
            description: "SSH".to_string(),
            succeeded: true,
        })
    );
    assert!(events.try_recv().is_err());
    assert_eq!(handle.info().ref_count, 1);
    assert_eq!(handle.info().consumers, vec![node_consumer.clone()]);

    entity.update(cx, |entity, cx| {
        entity.update_lifecycle(
            HostToolsVisibility::VisibleSidebar,
            monitoring,
            sampling_config,
            runtime.handle().clone(),
            false,
            cx,
        );
        entity.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
    });

    // Re-showing Services restarts only the shared profiler plus Services.
    wait_for_counter(&shell_open_count, hidden_shell_open_count + 1);
    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(
        shell_open_count.load(Ordering::SeqCst),
        hidden_shell_open_count + 1
    );
    entity.read_with(cx, |entity, _cx| {
        assert_eq!(
            entity.profiler_registry.state(&connection_id),
            Some(ProfilerState::Running)
        );
        assert!(!entity.gpu_sampling_is_running(&connection_id));
        assert_eq!(
            entity
                .test_snapshot_dispatches
                .as_ref()
                .expect("snapshot test hook"),
            &[ContextSidebarTool::Services]
        );
    });
    assert_eq!(handle.info().ref_count, 1);
    assert_eq!(handle.info().consumers, vec![node_consumer]);
}

#[gpui::test]
fn process_action_state_and_delivery_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let display_secret = Arc::new(zeroize::Zeroizing::new(
        "worker --token should-not-reach-delivery".to_string(),
    ));
    let request = HostProcessActionRequest {
        connection_id: "connection-1".to_string(),
        pid: "42".to_string(),
        display_command: display_secret.clone(),
        action: ProcessActionKind::Term,
    };

    let (delivery_tx, delivery) = entity.update(cx, |entity, cx| {
        let invalid_request = HostProcessActionRequest {
            connection_id: "connection-1".to_string(),
            pid: "42".to_string(),
            display_command: display_secret.clone(),
            action: ProcessActionKind::Renice { nice: 20 },
        };
        assert_eq!(
            entity.open_process_action_confirm(invalid_request, cx),
            Some(HostToolsNotice::ProcessInvalidNice)
        );
        assert!(
            entity
                .open_process_action_confirm(request.clone(), cx)
                .is_none()
        );
        let (visible_request, _) = entity.process_confirm_view().unwrap();
        assert!(Arc::ptr_eq(
            &visible_request.display_command,
            &display_secret
        ));
        entity.dismiss_process_confirm(cx);
        assert!(entity.process_confirm_view().is_none());

        let running = HostProcessActionRun {
            connection_id: request.connection_id.clone(),
            pid: request.pid.clone(),
            action: request.action.clone(),
        };
        entity.host_process_actions.running = Some(running.clone());
        (
            entity.reliable_delivery_tx.clone(),
            HostProcessActionDelivery {
                request: running,
                // The delivery type cannot carry captured command output.
                result: Ok(false),
            },
        )
    });

    delivery_tx
        .send(super::delivery::HostToolsReliableDelivery::ProcessAction(
            delivery,
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_process_actions.running.is_none());
    });
    let notice = events.try_recv().unwrap();
    assert_eq!(
        notice,
        HostToolsEvent::ShowNotice(HostToolsNotice::ProcessActionFinished {
            pid: "42".to_string(),
            succeeded: false,
        })
    );
    assert!(!format!("{notice:?}").contains(display_secret.as_str()));
    assert!(events.try_recv().is_err());
}

#[gpui::test]
fn docker_actions_and_logs_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let logs_request = HostDockerLogsRequest {
        connection_id: "connection-1".to_string(),
        container_id: "abc123".to_string(),
        container_name: "web".to_string(),
        failure_fallback: "Docker logs failed".to_string(),
        empty_fallback: "No Docker logs".to_string(),
    };
    let logs_delivery_tx = entity.update(cx, |entity, _cx| {
        entity.host_docker_operations.logs_dialog = Some(HostDockerLogsDialog {
            request: logs_request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        entity.reliable_delivery_tx.clone()
    });
    let secret_marker = "Bearer should-not-reach-docker-dialog";

    logs_delivery_tx
        .send(super::delivery::HostToolsReliableDelivery::DockerLogs(
            HostDockerLogsDelivery {
                request: logs_request,
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(1),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        let dialog = entity.host_docker_operations.logs_dialog.as_ref().unwrap();
        assert!(!dialog.loading);
        assert!(dialog.output.is_none());
        assert_eq!(dialog.error.as_deref(), Some("Docker logs failed"));
        assert!(!dialog.error.as_deref().unwrap().contains(secret_marker));
    });
    assert!(events.try_recv().is_err());

    let action_request = HostDockerActionRequest {
        connection_id: "connection-1".to_string(),
        container_id: "abc123".to_string(),
        container_name: "web".to_string(),
        action: DockerActionKind::Restart,
    };
    let (action_delivery_tx, action_delivery) = entity.update(cx, |entity, cx| {
        assert!(
            entity
                .open_docker_action_confirm(action_request.clone(), cx)
                .is_none()
        );
        assert!(entity.docker_confirm_view().is_some());
        entity.dismiss_docker_confirm(cx);
        assert!(entity.docker_confirm_view().is_none());
        entity.host_docker_operations.action_running = Some(action_request.clone());
        (
            entity.reliable_delivery_tx.clone(),
            HostDockerActionDelivery {
                request: action_request,
                // Captured Docker output cannot enter this delivery type.
                result: Ok(false),
            },
        )
    });

    action_delivery_tx
        .send(super::delivery::HostToolsReliableDelivery::DockerAction(
            action_delivery,
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_docker_operations.action_running.is_none());
    });
    assert_eq!(
        events.try_recv().unwrap(),
        HostToolsEvent::ShowNotice(HostToolsNotice::DockerActionFinished {
            container_name: "web".to_string(),
            succeeded: false,
        })
    );
    assert!(events.try_recv().is_err());
}

#[gpui::test]
fn service_snapshot_actions_and_logs_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let snapshot_request = HostServiceSnapshotRequest {
        connection_id: "connection-1".to_string(),
        connection_fallback: "Service connection missing".to_string(),
        failure_fallback: "Service capture failed".to_string(),
    };
    let logs_request = HostServiceLogsRequest {
        connection_id: "connection-1".to_string(),
        service_id: "ssh.service".to_string(),
        description: "SSH".to_string(),
        failure_fallback: "Service logs failed".to_string(),
        empty_fallback: "No service logs".to_string(),
    };
    let sender = entity.update(cx, |entity, _cx| {
        entity.host_services.snapshot_running = Some(snapshot_request.clone());
        entity.host_services.snapshot_in_flight = true;
        entity.host_services.logs_dialog = Some(HostServiceLogsDialog {
            request: logs_request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        entity.reliable_delivery_tx.clone()
    });
    let secret_marker = "Bearer should-not-reach-service-state";

    sender
        .send(super::delivery::HostToolsReliableDelivery::ServiceSnapshot(
            HostServiceSnapshotDelivery {
                request: snapshot_request,
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(1),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    sender
        .send(super::delivery::HostToolsReliableDelivery::ServiceLogs(
            HostServiceLogsDelivery {
                request: logs_request,
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(1),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(!entity.host_services.snapshot_in_flight);
        let snapshot = entity.host_services.snapshot.as_ref().unwrap();
        assert_eq!(
            snapshot.status,
            ResourceServiceStatus::Error {
                message: "Service capture failed".to_string(),
            }
        );
        assert!(!format!("{:?}", snapshot.status).contains(secret_marker));
        let dialog = entity.host_services.logs_dialog.as_ref().unwrap();
        assert!(!dialog.loading);
        assert!(dialog.output.is_none());
        assert_eq!(dialog.error.as_deref(), Some("Service logs failed"));
        assert!(!dialog.error.as_deref().unwrap().contains(secret_marker));
    });
    assert!(events.try_recv().is_err());

    let action_request = HostServiceActionRequest {
        connection_id: "connection-1".to_string(),
        service_id: "ssh.service".to_string(),
        description: "SSH".to_string(),
        action: ServiceActionKind::Restart,
    };
    let sender = entity.update(cx, |entity, cx| {
        entity.visibility = HostToolsVisibility::VisibleSidebar;
        entity.lifecycle_runtime = Some(runtime.handle().clone());
        entity.messages = Some(HostToolsMessages {
            service_connection_missing: "Service connection missing".to_string(),
            service_action_failed: "Service capture failed".to_string(),
            log_unknown_error: "Log capture failed".to_string(),
            port_unknown_error: "Port capture failed".to_string(),
            filesystem_unknown_error: "Filesystem capture failed".to_string(),
            package_unknown_error: "Package capture failed".to_string(),
            schedule_unknown_error: "Schedule capture failed".to_string(),
            tmux_unknown_error: "tmux capture failed".to_string(),
            tmux_unavailable: "tmux unavailable".to_string(),
            local_connection_label: "This machine".to_string(),
            ..HostToolsMessages::default()
        });
        entity.select_tool(ContextSidebarTool::Services, cx);
        assert!(
            entity
                .open_service_action_confirm(action_request.clone(), cx)
                .is_none()
        );
        assert!(entity.service_confirm_view().is_some());
        entity.dismiss_service_confirm(cx);
        assert!(entity.service_confirm_view().is_none());
        entity.host_services.action_running = Some(action_request.clone());
        entity.reliable_delivery_tx.clone()
    });

    sender
        .send(super::delivery::HostToolsReliableDelivery::ServiceAction(
            HostServiceActionDelivery {
                request: action_request,
                // Worker output is intentionally reduced to this boolean.
                result: Ok(false),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_services.action_running.is_none());
        assert_eq!(
            entity.host_services.snapshot.as_ref().unwrap().status,
            ResourceServiceStatus::Error {
                message: "Service connection missing".to_string(),
            }
        );
    });
    assert_eq!(
        events.try_recv().unwrap(),
        HostToolsEvent::ShowNotice(HostToolsNotice::ServiceActionFinished {
            description: "SSH".to_string(),
            succeeded: false,
        })
    );
    assert!(events.try_recv().is_err());
}

#[gpui::test]
fn tmux_snapshot_action_and_confirm_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
    let runtime_handle = runtime.handle().clone();
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let snapshot_request = HostTmuxSnapshotRequest {
        connection_id: "connection-1".to_string(),
        feedback: HostSnapshotFeedback::Silent,
        search_query: String::new(),
        failure_fallback: "tmux capture failed".to_string(),
        unavailable_fallback: "tmux unavailable".to_string(),
    };
    let sender = entity.update(cx, |entity, _cx| {
        entity.host_tmux.snapshot_running = Some(snapshot_request.clone());
        entity.host_tmux.snapshot_in_flight = true;
        entity.reliable_delivery_tx.clone()
    });
    let secret_marker = "Authorization: should-not-reach-tmux-state";

    sender
        .send(super::delivery::HostToolsReliableDelivery::TmuxSnapshot(
            HostTmuxSnapshotDelivery {
                request: snapshot_request,
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(1),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(!entity.host_tmux.snapshot_in_flight);
        assert_eq!(
            entity.host_tmux.snapshot.as_ref().unwrap().status,
            ResourceTmuxStatus::Error {
                message: "tmux capture failed".to_string(),
            }
        );
        assert!(
            !format!("{:?}", entity.host_tmux.snapshot.as_ref().unwrap().status)
                .contains(secret_marker)
        );
    });
    assert!(events.try_recv().is_err());

    let confirm_request = HostTmuxActionRequest {
        connection_id: "connection-1".to_string(),
        session_id: "$1".to_string(),
        session_name: "work".to_string(),
        target_label: "$1".to_string(),
        action: HostTmuxDestructiveAction::KillSession {
            target: "$1".to_string(),
        },
    };
    let action_request = HostTmuxActionRun {
        connection_id: "connection-1".to_string(),
        session_id: "$1".to_string(),
        session_name: "work".to_string(),
        target_label: "$1".to_string(),
    };
    let sender = entity.update(cx, |entity, cx| {
        assert!(
            entity
                .open_tmux_action_confirm(confirm_request, cx)
                .is_none()
        );
        assert!(entity.tmux_confirm_view().is_some());
        entity.dismiss_tmux_confirm(cx);
        assert!(entity.tmux_confirm_view().is_none());
        let empty_dialog = HostTmuxInputDialog {
            connection_id: "connection-1".to_string(),
            session_id: "$1".to_string(),
            session_name: "work".to_string(),
            target_label: "%1".to_string(),
            value: zeroize::Zeroizing::new("   ".to_string()),
            kind: HostTmuxInputDialogKind::SendPaneCommand {
                target: "%1".to_string(),
            },
        };
        entity.open_tmux_input_dialog(empty_dialog, cx);
        assert_eq!(
            entity.submit_tmux_input(runtime_handle, cx),
            vec![HostToolsNotice::TmuxInputRequired]
        );
        entity.host_tmux.action_running = Some(action_request.clone());
        entity.reliable_delivery_tx.clone()
    });

    sender
        .send(super::delivery::HostToolsReliableDelivery::TmuxAction(
            HostTmuxActionDelivery {
                request: action_request,
                // Worker output is intentionally reduced to this boolean.
                result: Ok(false),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_tmux.action_running.is_none());
    });
    assert_eq!(
        events.try_recv().unwrap(),
        HostToolsEvent::ShowNotice(HostToolsNotice::TmuxActionFinished {
            target_label: "$1".to_string(),
            succeeded: false,
        })
    );
    assert!(events.try_recv().is_err());
}

#[gpui::test]
fn mux_capture_failures_are_redacted_and_stale_deliveries_are_ignored(cx: &mut TestAppContext) {
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let secret_marker = "Authorization: should-not-reach-mux-state";
    let current = MuxSnapshotRequest {
        provider: Multiplexer::Herdr,
        connection_id: "connection-1".to_string(),
        feedback: HostSnapshotFeedback::Silent,
        generation: 3,
    };
    let sender = entity.update(cx, |entity, _cx| {
        entity.host_tmux.mux.provider = Some(Multiplexer::Herdr);
        entity.host_tmux.mux.host = "connection-1".to_string();
        entity.host_tmux.mux.snapshot_running = Some(current.clone());
        entity.reliable_delivery_tx.clone()
    });

    sender
        .send(super::delivery::HostToolsReliableDelivery::MuxSnapshot(
            MuxSnapshotDelivery {
                request: current.clone(),
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(1),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_tmux.mux.snapshot_running.is_none());
        // A capture that contradicts the probe is reported with the panel's
        // own failure text, so nothing from the remote stream rides along.
        assert_eq!(
            entity.host_tmux.mux.snapshot.as_ref().unwrap().status,
            MuxStatus::Error(None)
        );
    });
    assert!(events.try_recv().is_err());

    // A capture that started before the panel moved on answers for a list
    // nobody waits for: same provider, same connection, same feedback, only
    // an older generation. Applying it would clear the in-flight marker of
    // the current capture and leave the replaced inventory on screen.
    let superseded = MuxSnapshotRequest {
        generation: 1,
        ..current.clone()
    };
    entity.update(cx, |entity, _cx| {
        entity.host_tmux.mux.snapshot_running = Some(current.clone());
    });
    sender
        .send(super::delivery::HostToolsReliableDelivery::MuxSnapshot(
            MuxSnapshotDelivery {
                request: superseded,
                result: Ok(SshCommandOutput {
                    stdout: "dev".to_string(),
                    stderr: String::new(),
                    exit_code: Some(0),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();
    entity.read_with(cx, |entity, _cx| {
        assert!(
            entity.host_tmux.mux.snapshot_running.is_some(),
            "a superseded answer must not end the current capture"
        );
        assert_eq!(
            entity
                .host_tmux
                .mux
                .snapshot_running
                .as_ref()
                .map(|request| request.generation),
            Some(current.generation),
            "the current request keeps its slot"
        );
    });
    assert!(events.try_recv().is_err());

    // A session subtree follows the same contract: a truncated capture is
    // a failure, never an empty tree, and it keeps no remote text.
    let detail_request = MuxDetailRequest {
        provider: Multiplexer::Herdr,
        connection_id: "connection-1".to_string(),
        session: "default".to_string(),
        generation: 2,
    };
    entity.update(cx, |entity, _cx| {
        entity.host_tmux.mux.detail_running = Some(detail_request.clone());
    });
    sender
        .send(super::delivery::HostToolsReliableDelivery::MuxDetail(
            MuxDetailDelivery {
                request: detail_request.clone(),
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(0),
                    truncated: true,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_tmux.mux.detail_running.is_none());
        assert!(!entity.host_tmux.mux.details.contains_key("default"));
        assert!(
            entity.host_tmux.mux.detail_failed.contains("default"),
            "a truncated subtree is a failure, not an empty session"
        );
    });
    assert!(events.try_recv().is_err());

    // A delivery from a superseded action must not clear the running slot
    // or report completion for the action the user is waiting on. The two
    // runs share connection, provider, session and label: only the
    // generation tells them apart.
    let active = MuxActionRun {
        provider: Multiplexer::Herdr,
        connection_id: "connection-1".to_string(),
        session: "default".to_string(),
        target_label: "wA:pC".to_string(),
        generation: 9,
    };
    let stale = MuxActionRun {
        generation: 4,
        ..active.clone()
    };
    entity.update(cx, |entity, _cx| {
        entity.host_tmux.mux.action_running = Some(active.clone());
    });
    sender
        .send(super::delivery::HostToolsReliableDelivery::MuxAction(
            MuxActionDelivery {
                request: stale,
                result: Ok(true),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert_eq!(
            entity
                .host_tmux
                .mux
                .action_running
                .as_ref()
                .map(|run| run.generation),
            Some(active.generation),
            "the older run's completion is not the current action's result"
        );
    });
    assert!(events.try_recv().is_err());
}

/// The dialog is the user's typed text: an answer the model will not
/// accept must come back with the text and the reason still in place, and
/// a host that moved under the dialog must not receive the command.
#[gpui::test]
fn mux_input_dialog_answers_its_own_submission(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let target = MuxNode {
        kind: MuxKind::Pane,
        id: "wA:tB:p1".to_string(),
        label: "omp".to_string(),
        children: Vec::new(),
        focused: false,
        status: None,
        agent: None,
        title: None,
        cwd: None,
        orphan_agent: false,
        restore_failed: false,
        tab_count: 0,
        pane_count: 0,
    };
    let dialog = |value: &str, connection_id: &str| MuxInputDialog {
        kind: MuxInputDialogKind::RenamePane {
            target: target.clone(),
        },
        session: "default".to_string(),
        provider: Multiplexer::Herdr,
        connection_id: connection_id.to_string(),
        value: zeroize::Zeroizing::new(value.to_string()),
        error: None,
    };
    entity.update(cx, |entity, _cx| {
        entity.lifecycle_runtime = Some(runtime.handle().clone());
        entity.host_tmux.mux.provider = Some(Multiplexer::Herdr);
        entity.host_tmux.mux.host = "connection-1".to_string();
    });

    // Blank: the dialog stays with its reason.
    entity.update(cx, |entity, cx| {
        entity.ui.host_mux_input_dialog = Some(dialog("   ", "connection-1"));
        entity.submit_mux_input_from_view(cx);
    });
    entity.read_with(cx, |entity, _cx| {
        let stored = entity
            .ui
            .host_mux_input_dialog
            .as_ref()
            .expect("dialog stays");
        assert_eq!(
            stored.error,
            Some("sidebar.multiplexer.inputs.error_required")
        );
        assert!(entity.host_tmux.mux.action_running.is_none());
    });
    assert!(events.try_recv().is_err());

    // Refused by the model's payload grammar: the typed text is kept.
    entity.update(cx, |entity, cx| {
        entity.ui.host_mux_input_dialog = Some(dialog("two\nlines", "connection-1"));
        entity.submit_mux_input_from_view(cx);
    });
    entity.read_with(cx, |entity, _cx| {
        let stored = entity
            .ui
            .host_mux_input_dialog
            .as_ref()
            .expect("dialog stays");
        assert_eq!(stored.value.as_str(), "two\nlines");
        assert_eq!(
            stored.error,
            Some("sidebar.multiplexer.inputs.error_rejected")
        );
        assert!(
            entity.host_tmux.mux.action_running.is_none(),
            "a refused payload never reserves the mutating slot"
        );
    });
    assert!(events.try_recv().is_err());

    // The panel moved to another connection while the dialog was open.
    entity.update(cx, |entity, _cx| {
        entity.host_tmux.mux.host = "connection-2".to_string();
    });
    entity.update(cx, |entity, cx| {
        entity.ui.host_mux_input_dialog = Some(dialog("web", "connection-1"));
        entity.submit_mux_input_from_view(cx);
    });
    entity.read_with(cx, |entity, _cx| {
        assert!(
            entity.ui.host_mux_input_dialog.is_none(),
            "text bound to the old host is dropped, not run here"
        );
        assert!(entity.host_tmux.mux.action_running.is_none());
    });
    assert!(matches!(
        events.try_recv(),
        Ok(HostToolsEvent::ShowNotice(
            HostToolsNotice::MuxConnectionMissing
        ))
    ));

    // A valid payload still cannot start a mutation without its connection.
    entity.update(cx, |entity, cx| {
        entity.ui.host_mux_input_dialog = Some(dialog("web", "connection-2"));
        entity.submit_mux_input_from_view(cx);
    });
    entity.read_with(cx, |entity, _cx| {
        assert!(entity.ui.host_mux_input_dialog.is_none());
        assert!(entity.host_tmux.mux.action_running.is_none());
    });
    assert!(matches!(
        events.try_recv(),
        Ok(HostToolsEvent::ShowNotice(
            HostToolsNotice::MuxConnectionMissing
        ))
    ));
}

/// A pane read belongs to the viewer that asked for it: closing the viewer
/// stops the fetch, and an answer for a replaced capture cannot overwrite
/// the buffer the user is reading.
#[gpui::test]
fn mux_read_delivery_requires_its_own_capture(cx: &mut TestAppContext) {
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let current = MuxReadRequest {
        provider: Multiplexer::Herdr,
        connection_id: "connection-1".to_string(),
        session: "default".to_string(),
        target_id: "wA:tB:p1".to_string(),
        target_label: "omp".to_string(),
        generation: 7,
    };
    let sender = entity.update(cx, |entity, _cx| {
        entity.host_tmux.mux.read = Some(MuxReadDialog {
            request: current.clone(),
            output: None,
            failed: false,
            loading: true,
        });
        entity.reliable_delivery_tx.clone()
    });
    let secret_marker = "password: should-not-reach-replaced-read";

    sender
        .send(super::delivery::HostToolsReliableDelivery::MuxRead(
            MuxReadDelivery {
                request: MuxReadRequest {
                    generation: 6,
                    ..current.clone()
                },
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: String::new(),
                    exit_code: Some(0),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        let dialog = entity
            .host_tmux
            .mux
            .read
            .as_ref()
            .expect("viewer stays open");
        assert!(
            dialog.loading,
            "the replaced capture is still the one the viewer waits for"
        );
        assert!(dialog.output.is_none());
        assert!(!dialog.failed);
    });
    assert!(events.try_recv().is_err());
}

#[gpui::test]
fn log_snapshot_delivery_is_entity_owned_and_redacted(cx: &mut TestAppContext) {
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let request = HostLogSnapshotRequest {
        connection_id: "connection-1".to_string(),
        preset: LogPreset::All,
        limit: HOST_LOG_SNAPSHOT_LIMIT,
        feedback: HostSnapshotFeedback::Toast,
        failure_fallback: "Log capture failed".to_string(),
    };
    let sender = entity.update(cx, |entity, _cx| {
        entity.host_logs.running = Some(request.clone());
        entity.host_logs.snapshot_in_flight = true;
        entity.reliable_delivery_tx.clone()
    });
    let secret_marker = "Bearer should-not-reach-ui";

    sender
        .send(super::delivery::HostToolsReliableDelivery::LogSnapshot(
            HostLogSnapshotDelivery {
                request,
                result: Ok(SshCommandOutput {
                    stdout: format!(
                        "===HOST_LOGS===\n__OXIDE_LOG_ERROR__{secret_marker}\n===HOST_LOGS_END===\n"
                    ),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(0),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(!entity.host_logs.snapshot_in_flight);
        let snapshot = entity.host_logs.snapshot.as_ref().unwrap();
        assert_eq!(
            snapshot.status,
            ResourceLogStatus::Error {
                message: "Log capture failed".to_string(),
            }
        );
        assert!(!format!("{:?}", snapshot.status).contains(secret_marker));
    });
    let notice = events.try_recv().unwrap();
    assert_eq!(
        notice,
        HostToolsEvent::ShowNotice(HostToolsNotice::LogSnapshotFailed)
    );
    assert!(!format!("{notice:?}").contains(secret_marker));
}

#[gpui::test]
fn schedule_logs_and_actions_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity = cx.new(|cx| {
        HostToolsEntity::new(
            profiler_update_tx,
            profiler_update_rx,
            SshConnectionRegistry::default(),
            cx,
        )
    });
    let mut events = cx.events(&entity);
    let logs_request = HostScheduleLogsRequest {
        connection_id: "connection-1".to_string(),
        task_id: "backup.timer".to_string(),
        task_name: "Backup".to_string(),
        task_source: "systemd".to_string(),
        task_unit: "backup.service".to_string(),
        failure_fallback: "Schedule logs failed".to_string(),
        empty_fallback: "No schedule logs".to_string(),
    };
    let sender = entity.update(cx, |entity, _cx| {
        entity.host_schedules.logs_dialog = Some(HostScheduleLogsDialog {
            request: logs_request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        entity.reliable_delivery_tx.clone()
    });
    let secret_marker = "Authorization: should-not-reach-schedule-dialog";

    sender
        .send(super::delivery::HostToolsReliableDelivery::ScheduleLogs(
            HostScheduleLogsDelivery {
                request: logs_request,
                result: Ok(SshCommandOutput {
                    stdout: secret_marker.to_string(),
                    stderr: secret_marker.to_string(),
                    exit_code: Some(1),
                    truncated: false,
                }),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        let dialog = entity.host_schedules.logs_dialog.as_ref().unwrap();
        assert!(!dialog.loading);
        assert!(dialog.output.is_none());
        assert_eq!(dialog.error.as_deref(), Some("Schedule logs failed"));
        // Failed command output must be reduced before it reaches renderable state.
        assert!(!dialog.error.as_deref().unwrap().contains(secret_marker));
    });
    assert!(events.try_recv().is_err());

    let action_request = HostScheduleActionRequest {
        connection_id: "connection-1".to_string(),
        task_id: "backup.timer".to_string(),
        task_name: "Backup".to_string(),
        unit: "backup.service".to_string(),
        action: ScheduledTaskActionKind::RunNow {
            id: "backup.timer".to_string(),
            unit: "backup.service".to_string(),
        },
    };
    let sender = entity.update(cx, |entity, cx| {
        assert!(
            entity
                .open_schedule_action_confirm(action_request.clone(), cx)
                .is_none()
        );
        assert!(entity.schedule_confirm_view().is_some());
        entity.dismiss_schedule_confirm(cx);
        assert!(entity.schedule_confirm_view().is_none());
        entity.host_schedules.action_running = Some(action_request.clone());
        entity.reliable_delivery_tx.clone()
    });

    sender
        .send(super::delivery::HostToolsReliableDelivery::ScheduleAction(
            HostScheduleActionDelivery {
                request: action_request,
                // Worker output is intentionally reduced to this boolean.
                result: Ok(false),
            },
        ))
        .unwrap();
    cx.run_until_parked();

    entity.read_with(cx, |entity, _cx| {
        assert!(entity.host_schedules.action_running.is_none());
    });
    assert_eq!(
        events.try_recv().unwrap(),
        HostToolsEvent::ShowNotice(HostToolsNotice::ScheduleActionFinished {
            kind: ScheduleActionNoticeKind::RunNow,
            task_name: "Backup".to_string(),
            succeeded: false,
        })
    );
    assert!(events.try_recv().is_err());
}

#[gpui::test]
fn connection_selector_lists_and_selects_local_after_ssh(cx: &mut TestAppContext) {
    let registry = SshConnectionRegistry::default();
    let handle = registry.acquire(
        SshConfig {
            host: "host.example".to_string(),
            username: "alice".to_string(),
            auth: AuthMethod::Agent,
            ..SshConfig::default()
        },
        ConnectionConsumer::NodeRouter("node-local-target".to_string()),
    );
    let connection_id = handle.connection_id().to_string();
    let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::channel(1);
    let entity =
        cx.new(|cx| HostToolsEntity::new(profiler_update_tx, profiler_update_rx, registry, cx));
    entity.update(cx, |entity, cx| {
        entity.messages = Some(HostToolsMessages {
            service_connection_missing: "Service connection missing".to_string(),
            service_action_failed: "Service action failed".to_string(),
            log_unknown_error: "Log capture failed".to_string(),
            port_unknown_error: "Port capture failed".to_string(),
            filesystem_unknown_error: "Filesystem capture failed".to_string(),
            package_unknown_error: "Package capture failed".to_string(),
            schedule_unknown_error: "Schedule capture failed".to_string(),
            tmux_unknown_error: "tmux capture failed".to_string(),
            tmux_unavailable: "tmux unavailable".to_string(),
            local_connection_label: "This machine".to_string(),
            ..HostToolsMessages::default()
        });
        let connections = entity.monitor_connections();
        assert_eq!(connections.len(), 2);
        assert_eq!(connections[0].connection_id, connection_id);
        assert_eq!(connections[1].connection_id, LOCAL_HOST_CONNECTION_ID);
        assert_eq!(monitor_connection_label(&connections[1]), "This machine");

        for key in ["tab", "enter", "down", "escape"] {
            assert!(
                entity.handle_connection_selector_key(&gpui::Keystroke::parse(key).unwrap(), cx),
                "selector did not handle {key}"
            );
        }
        assert!(!entity.selector_open());
        assert_ne!(
            entity.selected_connection_id.as_deref(),
            Some(LOCAL_HOST_CONNECTION_ID)
        );
        for key in ["enter", "down", "enter"] {
            assert!(
                entity.handle_connection_selector_key(&gpui::Keystroke::parse(key).unwrap(), cx),
                "selector did not handle {key}"
            );
        }
        assert_eq!(
            entity.selected_connection_id.as_deref(),
            Some(LOCAL_HOST_CONNECTION_ID)
        );
        assert!(!entity.selector_open());
    });
}

fn redis_endpoint(target: &str) -> oxideterm_connection_monitor::ResourceRedisEndpoint {
    oxideterm_connection_monitor::ResourceRedisEndpoint {
        kind: oxideterm_connection_monitor::ResourceRedisEndpointKind::Host,
        target: target.to_string(),
    }
}

fn redis_keys_request(connection_id: &str, target: &str) -> HostRedisKeysRequest {
    HostRedisKeysRequest {
        connection_id: connection_id.to_string(),
        endpoint: redis_endpoint(target),
        password: zeroize::Zeroizing::new(String::new()),
        pattern: None,
        failure_fallback: "failed".to_string(),
    }
}

fn redis_value_request(
    connection_id: &str,
    target: &str,
    database: u32,
    key: &str,
    purpose: HostRedisValuePurpose,
) -> HostRedisValueRequest {
    HostRedisValueRequest {
        connection_id: connection_id.to_string(),
        endpoint: redis_endpoint(target),
        database,
        key: key.to_string(),
        password: zeroize::Zeroizing::new(String::new()),
        purpose,
        failure_fallback: "failed".to_string(),
    }
}

fn redis_value_dialog(
    connection_id: &str,
    target: &str,
    database: u32,
    key: &str,
) -> HostRedisValueDialog {
    HostRedisValueDialog {
        connection_id: connection_id.to_string(),
        endpoint: redis_endpoint(target),
        database,
        key: key.to_string(),
        value: None,
        loading: true,
        error: None,
        json_view: false,
    }
}

#[test]
fn redis_keys_request_is_stale_once_the_selection_moved() {
    let request = redis_keys_request("conn-a", "127.0.0.1:6379");
    assert!(!request.is_stale_for(Some("conn-a"), Some(&redis_endpoint("127.0.0.1:6379"))));
    // Another host, another endpoint, and no selection all invalidate a queued
    // read that was made for the old target.
    assert!(request.is_stale_for(Some("conn-b"), Some(&redis_endpoint("127.0.0.1:6379"))));
    assert!(request.is_stale_for(Some("conn-a"), Some(&redis_endpoint("127.0.0.1:6380"))));
    assert!(request.is_stale_for(None, None));
}

#[test]
fn redis_value_dialog_only_accepts_the_delivery_for_its_own_target() {
    let dialog = redis_value_dialog("conn-a", "127.0.0.1:6379", 0, "session:1");
    assert!(dialog.matches_request(&redis_value_request(
        "conn-a",
        "127.0.0.1:6379",
        0,
        "session:1",
        HostRedisValuePurpose::Open,
    )));
    // A key name is not unique across hosts, endpoints, or databases.
    for request in [
        redis_value_request(
            "conn-b",
            "127.0.0.1:6379",
            0,
            "session:1",
            HostRedisValuePurpose::Open,
        ),
        redis_value_request(
            "conn-a",
            "127.0.0.1:6380",
            0,
            "session:1",
            HostRedisValuePurpose::Open,
        ),
        redis_value_request(
            "conn-a",
            "127.0.0.1:6379",
            1,
            "session:1",
            HostRedisValuePurpose::Open,
        ),
        redis_value_request(
            "conn-a",
            "127.0.0.1:6379",
            0,
            "session:2",
            HostRedisValuePurpose::Open,
        ),
    ] {
        assert!(!dialog.matches_request(&request));
    }
    // Purpose is not part of the target identity: the receiver decides what a
    // copy or verify read may do with the dialog.
    for purpose in [
        HostRedisValuePurpose::Copy,
        HostRedisValuePurpose::VerifyWrite,
    ] {
        assert!(dialog.matches_request(&redis_value_request(
            "conn-a",
            "127.0.0.1:6379",
            0,
            "session:1",
            purpose,
        )));
    }

    // The mutation identity is the target too: the same key on another
    // endpoint is not the dialog's key.
    let mutation = |target: &str| HostRedisMutationRequest {
        connection_id: "conn-a".to_string(),
        endpoint: redis_endpoint(target),
        database: 0,
        kind: HostRedisMutationKind::Expire,
        key: "session:1".to_string(),
        keys: Vec::new(),
        new_key: None,
        ttl_seconds: Some(60),
        password: zeroize::Zeroizing::new(String::new()),
        failure_fallback: "failed".to_string(),
    };
    assert!(dialog.matches_mutation(&mutation("127.0.0.1:6379")));
    assert!(!dialog.matches_mutation(&mutation("127.0.0.1:6380")));
}

fn secret_redis_value(body: &str) -> oxideterm_connection_monitor::ResourceRedisValue {
    oxideterm_connection_monitor::ResourceRedisValue {
        status: oxideterm_connection_monitor::ResourceRedisStatus::Available,
        kind: oxideterm_connection_monitor::ResourceRedisValueKind::Hash,
        raw_type: Some("Timeseries".to_string()),
        ttl: oxideterm_connection_monitor::ResourceRedisTtl::Never,
        memory_usage: Some(64),
        length: Some(2),
        body: body.to_string(),
        elements: vec![b"field".to_vec(), body.as_bytes().to_vec()],
    }
}

#[test]
fn redis_value_wipe_clears_every_secret_bearing_projection() {
    let mut value = secret_redis_value("secret");
    zeroize_redis_value(&mut value);
    assert!(value.body.is_empty());
    assert!(value.elements.is_empty());
    assert_eq!(value.raw_type.as_deref(), Some(""));
    // Metadata that is not secret stays readable for rendering.
    assert_eq!(value.length, Some(2));
    assert_eq!(value.memory_usage, Some(64));
}

#[test]
fn redis_value_dialog_replaces_and_clears_through_the_wiping_path() {
    let mut dialog = redis_value_dialog("conn-a", "127.0.0.1:6379", 0, "session:1");
    dialog.set_value(Some(secret_redis_value("first")));
    assert_eq!(
        dialog.value.as_ref().map(|value| value.body.as_str()),
        Some("first")
    );
    dialog.set_value(Some(secret_redis_value("second")));
    assert_eq!(
        dialog.value.as_ref().map(|value| value.body.as_str()),
        Some("second")
    );
    dialog.clear_value();
    assert!(dialog.value.is_none());
    dialog.set_value(None);
    assert!(dialog.value.is_none());
}

#[test]
fn invalidating_the_container_text_dialog_retires_its_running_capture() {
    let mut state = HostContainerState::new();
    let first = state.begin_container_text_dialog();
    let second = state.begin_container_text_dialog();
    assert_ne!(first, second);

    state.text_dialog_running_generation = Some(second);
    // No dialog was open, so nothing gets reported as closed.
    assert!(!state.invalidate_container_text_dialog());
    assert_ne!(state.text_dialog_generation, second);
    assert!(state.text_dialog_running_generation.is_none());
    assert!(state.text_dialog_running.is_none());
}
