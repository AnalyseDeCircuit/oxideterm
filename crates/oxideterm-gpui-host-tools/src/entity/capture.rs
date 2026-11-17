use super::*;

impl HostToolsEntity {
    pub(crate) fn spawn_log_snapshot_capture(
        &self,
        command: String,
        request: HostLogSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = request.feedback.should_toast().then(|| {
            self.host_tool_audit(
                &request.connection_id,
                "query_host_logs",
                Some(&format!(
                    "preset={:?} limit={}",
                    request.preset, request.limit
                )),
            )
        });
        let Some(target) = self.host_command_target(&request.connection_id) else {
            if let Some(audit) = audit {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Only the generated log command crosses the task boundary. The
        // resolved target keeps credentials and node lifetime private.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            if let Some(audit) = audit {
                finish_host_query_audit(audit, &result);
            }
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::LogSnapshot(
                HostLogSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_port_snapshot_capture(
        &self,
        command: String,
        request: HostPortSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = request
            .feedback
            .should_toast()
            .then(|| self.host_tool_audit(&request.connection_id, "query_host_ports", None));
        let Some(target) = self.host_command_target(&request.connection_id) else {
            if let Some(audit) = audit {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // The resolved target keeps authentication and node ownership inside
        // its runtime while the generated diagnostic command runs.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            if let Some(audit) = audit {
                finish_host_query_audit(audit, &result);
            }
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::PortSnapshot(
                HostPortSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_filesystem_snapshot_capture(
        &self,
        command: String,
        request: FilesystemSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = request
            .notify
            .then(|| self.host_tool_audit(&request.connection_id, "query_host_filesystems", None));
        let Some(target) = self.host_command_target(&request.connection_id) else {
            if let Some(audit) = audit {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Filesystem capture uses the registry-owned transport without moving
        // credentials or node lifecycle control into the page task.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            if let Some(audit) = audit {
                finish_host_query_audit(audit, &result);
            }
            let _ = delivery_tx.send(
                super::delivery::HostToolsReliableDelivery::FilesystemSnapshot(
                    HostFilesystemSnapshotDelivery { request, result },
                ),
            );
        });
        true
    }

    pub(crate) fn spawn_package_snapshot_capture(
        &self,
        command: String,
        request: HostPackageSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = request
            .feedback
            .should_toast()
            .then(|| self.host_tool_audit(&request.connection_id, "query_host_packages", None));
        let Some(target) = self.host_command_target(&request.connection_id) else {
            if let Some(audit) = audit {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Package inventory uses the registry-owned transport and cannot
        // release the shared node or inspect its authentication material.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            if let Some(audit) = audit {
                finish_host_query_audit(audit, &result);
            }
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::PackageSnapshot(
                HostPackageSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_schedule_snapshot_capture(
        &self,
        command: String,
        request: HostScheduleSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = request
            .feedback
            .should_toast()
            .then(|| self.host_tool_audit(&request.connection_id, "query_host_schedules", None));
        let Some(target) = self.host_command_target(&request.connection_id) else {
            if let Some(audit) = audit {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Scheduled-task inventory runs on the registry-owned transport and
        // cannot acquire node release or authentication capabilities.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            if let Some(audit) = audit {
                finish_host_query_audit(audit, &result);
            }
            let _ = delivery_tx.send(
                super::delivery::HostToolsReliableDelivery::ScheduleSnapshot(
                    HostScheduleSnapshotDelivery { request, result },
                ),
            );
        });
        true
    }

    pub(crate) fn spawn_schedule_logs_capture(
        &self,
        command: String,
        request: HostScheduleLogsRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "query_scheduled_task_logs",
            Some(&format!("task_id={}", request.task_id)),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw log output stays inside the Entity delivery and dialog state.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            finish_host_query_audit(audit, &result);
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ScheduleLogs(
                HostScheduleLogsDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_schedule_action(
        &self,
        command: String,
        request: HostScheduleActionRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "scheduled_task_action",
            Some(&format!(
                "task={} action={:?}",
                request.task_id, request.action
            )),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Action output is reduced to a success bit before it enters delivery.
        runtime.spawn(async move {
            let mut audit_exit_code = None;
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    audit_exit_code = output.exit_code;
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    // Captured action text may contain credentials and has no UI consumer.
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let audit_outcome = match (&result, audit_exit_code) {
                (Err(_), _) => oxideterm_audit::AuditOutcome::Failed,
                (_, Some(0)) => oxideterm_audit::AuditOutcome::Succeeded,
                (_, Some(_)) => oxideterm_audit::AuditOutcome::Failed,
                (_, None) => oxideterm_audit::AuditOutcome::Unknown,
            };
            audit.finish(
                audit_outcome,
                if audit_exit_code.is_some() {
                    oxideterm_audit::AuditEvidence::ExitCode
                } else {
                    oxideterm_audit::AuditEvidence::Protocol
                },
                audit_exit_code,
                None,
            );
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ScheduleAction(
                HostScheduleActionDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_process_action(
        &self,
        command: String,
        request: HostProcessActionRun,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "process_action",
            Some(&format!("pid={} action={:?}", request.pid, request.action)),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Remote process output has no UI consumer. Reduce it in the worker
        // and clear both buffers before crossing the Entity delivery boundary.
        runtime.spawn(async move {
            let mut audit_exit_code = None;
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    audit_exit_code = output.exit_code;
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let audit_outcome = match (&result, audit_exit_code) {
                (Err(_), _) => oxideterm_audit::AuditOutcome::Failed,
                (_, Some(0)) => oxideterm_audit::AuditOutcome::Succeeded,
                (_, Some(_)) => oxideterm_audit::AuditOutcome::Failed,
                (_, None) => oxideterm_audit::AuditOutcome::Unknown,
            };
            audit.finish(
                audit_outcome,
                if audit_exit_code.is_some() {
                    oxideterm_audit::AuditEvidence::ExitCode
                } else {
                    oxideterm_audit::AuditEvidence::Protocol
                },
                audit_exit_code,
                None,
            );
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ProcessAction(
                HostProcessActionDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_docker_action(
        &self,
        command: String,
        request: HostDockerActionRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "container_action",
            Some(&format!(
                "container={} action={:?}",
                request.container_id, request.action
            )),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Docker action output is not product data. Clear it before delivery.
        runtime.spawn(async move {
            let mut audit_exit_code = None;
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    audit_exit_code = output.exit_code;
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let audit_outcome = match (&result, audit_exit_code) {
                (Err(_), _) => oxideterm_audit::AuditOutcome::Failed,
                (_, Some(0)) => oxideterm_audit::AuditOutcome::Succeeded,
                (_, Some(_)) => oxideterm_audit::AuditOutcome::Failed,
                (_, None) => oxideterm_audit::AuditOutcome::Unknown,
            };
            audit.finish(
                audit_outcome,
                if audit_exit_code.is_some() {
                    oxideterm_audit::AuditEvidence::ExitCode
                } else {
                    oxideterm_audit::AuditEvidence::Protocol
                },
                audit_exit_code,
                None,
            );
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::DockerAction(
                HostDockerActionDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_docker_logs_capture(
        &self,
        command: String,
        request: HostDockerLogsRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "query_container_logs",
            Some(&format!("container_id={}", request.container_id)),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw logs remain inside the Entity delivery and dialog state.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            finish_host_query_audit(audit, &result);
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::DockerLogs(
                HostDockerLogsDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_service_snapshot_capture(
        &self,
        command: String,
        request: HostServiceSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(target) = self.host_command_target(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Inventory output stays inside the Entity and is parsed before render.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ServiceSnapshot(
                HostServiceSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_service_action(
        &self,
        command: String,
        request: HostServiceActionRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "service_action",
            Some(&format!(
                "service={} action={:?}",
                request.service_id, request.action
            )),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Service action output has no UI consumer and is cleared in the worker.
        runtime.spawn(async move {
            let mut audit_exit_code = None;
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    audit_exit_code = output.exit_code;
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let audit_outcome = match (&result, audit_exit_code) {
                (Err(_), _) => oxideterm_audit::AuditOutcome::Failed,
                (_, Some(0)) => oxideterm_audit::AuditOutcome::Succeeded,
                (_, Some(_)) => oxideterm_audit::AuditOutcome::Failed,
                (_, None) => oxideterm_audit::AuditOutcome::Unknown,
            };
            audit.finish(
                audit_outcome,
                if audit_exit_code.is_some() {
                    oxideterm_audit::AuditEvidence::ExitCode
                } else {
                    oxideterm_audit::AuditEvidence::Protocol
                },
                audit_exit_code,
                None,
            );
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ServiceAction(
                HostServiceActionDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_service_logs_capture(
        &self,
        command: String,
        request: HostServiceLogsRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "query_service_logs",
            Some(&format!("service_id={}", request.service_id)),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw service logs never cross the typed workspace event boundary.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            finish_host_query_audit(audit, &result);
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ServiceLogs(
                HostServiceLogsDelivery { request, result },
            ));
        });
        true
    }

    /// Starts one bounded session-list capture and returns its handle, which
    /// the panel aborts when the request is superseded or its host changes.
    /// Output stays inside this delivery boundary and is matched against the
    /// request identity before anything is rendered.
    pub(crate) fn spawn_mux_snapshot_capture(
        &self,
        command: String,
        request: MuxSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let provider = request.provider.name();
        let audit = request.feedback.should_toast().then(|| {
            self.host_tool_audit(&request.connection_id, "query_multiplexer", Some(provider))
        });
        let Some(target) = self.host_command_target(&request.connection_id) else {
            if let Some(audit) = audit {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
            return None;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        Some(runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            if let Some(audit) = audit {
                finish_host_query_audit(audit, &result);
            }
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::MuxSnapshot(
                MuxSnapshotDelivery { request, result },
            ));
        }))
    }

    pub(crate) fn spawn_mux_detail_capture(
        &self,
        command: String,
        request: MuxDetailRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "query_multiplexer",
            Some(&format!(
                "provider={} session={}",
                request.provider.name(),
                request.session
            )),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return None;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // One session's subtree travels the same bounded channel as its list.
        Some(runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            finish_host_query_audit(audit, &result);
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::MuxDetail(
                MuxDetailDelivery { request, result },
            ));
        }))
    }

    /// Audits the provider and the target label only; user-authored labels and
    /// run commands never enter the audit record.
    pub(crate) fn spawn_mux_action(
        &self,
        provider: Multiplexer,
        command: zeroize::Zeroizing<String>,
        request: MuxActionRun,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "manage_multiplexer",
            Some(provider.name()),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return None;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        Some(runtime.spawn(async move {
            let mut audit_exit_code = None;
            let result = target
                .run_command_capture(command.as_str(), timeout, max_output_size)
                .await
                .map(|mut output| {
                    audit_exit_code = output.exit_code;
                    // A provider may report failure inside its envelope while
                    // still exiting zero, so the envelope decides; a truncated
                    // capture holds only part of it and decides nothing.
                    let succeeded = provider.action_succeeded(
                        &output.stdout,
                        output.exit_code,
                        output.truncated,
                    );
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let audit_outcome = match &result {
                Ok(true) => oxideterm_audit::AuditOutcome::Succeeded,
                Ok(false) | Err(_) => oxideterm_audit::AuditOutcome::Failed,
            };
            audit.finish(
                audit_outcome,
                if audit_exit_code.is_some() {
                    oxideterm_audit::AuditEvidence::ExitCode
                } else {
                    oxideterm_audit::AuditEvidence::Protocol
                },
                audit_exit_code,
                None,
            );
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::MuxAction(
                MuxActionDelivery { request, result },
            ));
        }))
    }

    pub(crate) fn spawn_mux_read(
        &self,
        command: String,
        request: MuxReadRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "query_multiplexer",
            Some(&format!(
                "provider={} target={}",
                request.provider.name(),
                request.target_id
            )),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return None;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw pane output remains inside the Entity delivery and dialog state.
        Some(runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            finish_host_query_audit(audit, &result);
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::MuxRead(
                MuxReadDelivery { request, result },
            ));
        }))
    }

    pub(crate) fn spawn_tmux_snapshot_capture(
        &self,
        command: String,
        request: HostTmuxSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = request
            .feedback
            .should_toast()
            .then(|| self.host_tool_audit(&request.connection_id, "query_host_tmux", None));
        let Some(target) = self.host_command_target(&request.connection_id) else {
            if let Some(audit) = audit {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw tmux inventory stays inside the Entity delivery boundary.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            if let Some(audit) = audit {
                finish_host_query_audit(audit, &result);
            }
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::TmuxSnapshot(
                HostTmuxSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_tmux_action(
        &self,
        command: zeroize::Zeroizing<String>,
        request: HostTmuxActionRun,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "tmux_action",
            Some(&format!("session={}", request.session_id)),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // The generated command is moved once into the worker. Captured output
        // is cleared before only a success bit crosses delivery.
        runtime.spawn(async move {
            let mut audit_exit_code = None;
            let result = target
                .run_command_capture(command.as_str(), timeout, max_output_size)
                .await
                .map(|mut output| {
                    audit_exit_code = output.exit_code;
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let audit_outcome = match (&result, audit_exit_code) {
                (Err(_), _) => oxideterm_audit::AuditOutcome::Failed,
                (_, Some(0)) => oxideterm_audit::AuditOutcome::Succeeded,
                (_, Some(_)) => oxideterm_audit::AuditOutcome::Failed,
                (_, None) => oxideterm_audit::AuditOutcome::Unknown,
            };
            audit.finish(
                audit_outcome,
                if audit_exit_code.is_some() {
                    oxideterm_audit::AuditEvidence::ExitCode
                } else {
                    oxideterm_audit::AuditEvidence::Protocol
                },
                audit_exit_code,
                None,
            );
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::TmuxAction(
                HostTmuxActionDelivery { request, result },
            ));
        });
        true
    }
}
