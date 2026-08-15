use std::time::{Duration, SystemTime};

use oxideterm_connection_monitor::{
    DockerActionKind, LogPreset, ProcessActionKind, ResourceSamplingConfig,
    ScheduledTaskActionKind, ServiceActionKind, TmuxActionKind, build_docker_action_command,
    build_filesystem_snapshot_command, build_log_snapshot_command, build_package_snapshot_command,
    build_port_snapshot_command, build_process_action_command, build_sample_command_for,
    build_scheduled_task_action_command, build_scheduled_task_snapshot_command,
    build_service_action_command, build_service_snapshot_command, build_tmux_action_command,
    build_tmux_snapshot_command, docker_sample_command, parse_docker_snapshot,
    parse_filesystem_snapshot, parse_log_snapshot, parse_package_snapshot, parse_port_snapshot,
    parse_resource_metrics, parse_scheduled_task_snapshot, parse_service_snapshot,
    parse_tmux_snapshot,
};
use oxideterm_public_mcp::{
    DomainRequest, HostToolLogPreset, HostToolOperation, HostToolResource, PublicToolCall,
    ToolEnvelope,
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{WorkspaceApp, finish_serialized, node_lease_for_client, public_command_error};

const HOST_TOOLS_CAPTURE_TIMEOUT: Duration = Duration::from_secs(20);
const HOST_TOOLS_CAPTURE_OUTPUT_LIMIT: usize = 512 * 1024;

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_host_tools_catalog(&self, request: DomainRequest) {
        let PublicToolCall::HostToolsCatalog(args) = &request.call else {
            return;
        };
        let Some(lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let platform = self
            .node_router
            .resolve_connection_now(&lease.node_id)
            .ok()
            .and_then(|resolved| resolved.handle.remote_env())
            .map_or_else(|| "unknown".to_owned(), |environment| environment.os_type);
        // This catalog is fixed product surface; plugin monitor commands are never merged here.
        finish_serialized(
            request,
            json!({
                "platform": platform,
                "resources": [
                    "system",
                    "processes",
                    "docker",
                    "services",
                    "logs",
                    "tmux",
                    "ports",
                    "filesystems",
                    "packages",
                    "scheduled_tasks",
                ],
                "actions": [
                    "process_stop", "process_continue", "process_renice",
                    "process_terminate", "process_kill",
                    "docker_start", "docker_stop", "docker_restart",
                    "service_start", "service_stop", "service_restart", "service_reload",
                    "service_enable", "service_disable",
                    "tmux_rename_session", "tmux_rename_window", "tmux_kill_session",
                    "tmux_kill_window", "tmux_kill_pane",
                    "scheduled_task_run", "scheduled_task_enable", "scheduled_task_disable",
                ],
            }),
        );
    }

    pub(super) fn handle_public_mcp_host_tools_capture(&self, request: DomainRequest) {
        let PublicToolCall::HostToolsCapture(args) = &request.call else {
            return;
        };
        let Some(lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let resource = args.resource;
        let log_preset = args.log_preset;
        let limit = args.limit as usize;
        let cancellation = request.cancellation_token();
        let router = self.node_router.clone();
        self.forwarding_runtime.spawn(async move {
            let resolved = tokio::select! {
                _ = cancellation.cancelled() => return,
                result = router.resolve_connection(&lease.node_id) => result,
            };
            let resolved = match resolved {
                Ok(resolved) => resolved,
                Err(_) => {
                    request.finish(ToolEnvelope::failed("The SSH node is no longer ready"));
                    return;
                }
            };
            let os_type = resolved
                .handle
                .remote_env()
                .map_or_else(|| "Unknown".to_owned(), |environment| environment.os_type);
            let command = match host_tools_capture_command(resource, log_preset, limit, &os_type) {
                Ok(command) => Zeroizing::new(command),
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            let output = tokio::select! {
                _ = cancellation.cancelled() => return,
                result = resolved.handle.run_secret_command_capture(
                    command.as_str(),
                    HOST_TOOLS_CAPTURE_TIMEOUT,
                    HOST_TOOLS_CAPTURE_OUTPUT_LIMIT,
                ) => result,
            };
            let output = match output {
                Ok(output) => output,
                Err(error) => {
                    request.finish(ToolEnvelope::failed(public_command_error(error)));
                    return;
                }
            };
            let stdout = String::from_utf8_lossy(output.stdout.as_slice());
            let snapshot = match host_tools_capture_value(resource, &stdout, limit) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            finish_serialized(
                request,
                json!({
                    "resource": resource,
                    "snapshot": snapshot,
                    "exit_code": output.exit_code,
                    "truncated": output.truncated,
                }),
            );
        });
    }

    pub(super) fn handle_public_mcp_host_tools_operate(&self, request: DomainRequest) {
        let PublicToolCall::HostToolsOperate(args) = &request.call else {
            return;
        };
        let Some(lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let operation = args.operation.clone();
        let cancellation = request.cancellation_token();
        let router = self.node_router.clone();
        self.forwarding_runtime.spawn(async move {
            let resolved = tokio::select! {
                _ = cancellation.cancelled() => return,
                result = router.resolve_connection(&lease.node_id) => result,
            };
            let resolved = match resolved {
                Ok(resolved) => resolved,
                Err(_) => {
                    request.finish(ToolEnvelope::failed("The SSH node is no longer ready"));
                    return;
                }
            };
            let os_type = resolved
                .handle
                .remote_env()
                .map_or_else(|| "Unknown".to_owned(), |environment| environment.os_type);
            let command = match host_tools_operation_command(&operation, &os_type) {
                Ok(command) => Zeroizing::new(command),
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            };
            let output = tokio::select! {
                _ = cancellation.cancelled() => return,
                result = resolved.handle.run_secret_command_capture(
                    command.as_str(),
                    HOST_TOOLS_CAPTURE_TIMEOUT,
                    HOST_TOOLS_CAPTURE_OUTPUT_LIMIT,
                ) => result,
            };
            match output {
                Ok(output) => finish_serialized(
                    request,
                    json!({
                        "success": output.exit_code == Some(0),
                        "exit_code": output.exit_code,
                        "truncated": output.truncated,
                    }),
                ),
                Err(error) => {
                    request.finish(ToolEnvelope::failed(public_command_error(error)));
                }
            }
        });
    }
}

fn host_tools_capture_command(
    resource: HostToolResource,
    log_preset: HostToolLogPreset,
    limit: usize,
    os_type: &str,
) -> Result<String, String> {
    let command = match resource {
        HostToolResource::System => build_sample_command_for(
            os_type,
            ResourceSamplingConfig {
                system: true,
                gpu: true,
                processes: false,
                docker: false,
            },
        ),
        HostToolResource::Processes => build_sample_command_for(
            os_type,
            ResourceSamplingConfig {
                system: false,
                gpu: false,
                processes: true,
                docker: false,
            },
        ),
        HostToolResource::Docker => docker_sample_command(os_type).to_owned(),
        HostToolResource::Services => build_service_snapshot_command(os_type).command,
        HostToolResource::Logs => {
            build_log_snapshot_command(os_type, mapped_log_preset(log_preset), limit)?.command
        }
        HostToolResource::Tmux => build_tmux_snapshot_command(os_type).command,
        HostToolResource::Ports => build_port_snapshot_command(os_type).command,
        HostToolResource::Filesystems => build_filesystem_snapshot_command(os_type).command,
        HostToolResource::Packages => build_package_snapshot_command(os_type).command,
        HostToolResource::ScheduledTasks => build_scheduled_task_snapshot_command(os_type).command,
    };
    Ok(command)
}

fn host_tools_capture_value(
    resource: HostToolResource,
    output: &str,
    limit: usize,
) -> Result<Value, String> {
    let mut value = match resource {
        HostToolResource::System => {
            let mut metrics = parse_resource_metrics(output, None, unix_time_ms());
            metrics.top_processes.clear();
            metrics.docker.containers.clear();
            metrics.services.services.clear();
            serde_json::to_value(metrics)
        }
        HostToolResource::Processes => {
            let mut metrics = parse_resource_metrics(output, None, unix_time_ms());
            for process in &mut metrics.top_processes {
                // Full command lines can contain credentials; the public snapshot keeps display names.
                process.full_command = None;
            }
            metrics.top_processes.truncate(limit);
            serde_json::to_value(metrics.top_processes)
        }
        HostToolResource::Docker => {
            let mut snapshot = parse_docker_snapshot(output);
            snapshot.containers.truncate(limit);
            serde_json::to_value(snapshot)
        }
        HostToolResource::Services => {
            let mut snapshot = parse_service_snapshot(output);
            snapshot.services.truncate(limit);
            serde_json::to_value(snapshot)
        }
        HostToolResource::Logs => {
            let mut snapshot = parse_log_snapshot(output);
            snapshot.entries.truncate(limit);
            serde_json::to_value(snapshot)
        }
        HostToolResource::Tmux => {
            let mut snapshot = parse_tmux_snapshot(output);
            snapshot.sessions.truncate(limit);
            snapshot.windows.truncate(limit);
            snapshot.panes.truncate(limit);
            serde_json::to_value(snapshot)
        }
        HostToolResource::Ports => {
            let mut snapshot = parse_port_snapshot(output);
            snapshot.entries.truncate(limit);
            serde_json::to_value(snapshot)
        }
        HostToolResource::Filesystems => {
            let mut snapshot = parse_filesystem_snapshot(output);
            snapshot.entries.truncate(limit);
            serde_json::to_value(snapshot)
        }
        HostToolResource::Packages => {
            let mut snapshot = parse_package_snapshot(output);
            snapshot.entries.truncate(limit);
            serde_json::to_value(snapshot)
        }
        HostToolResource::ScheduledTasks => {
            let mut snapshot = parse_scheduled_task_snapshot(output);
            snapshot.entries.truncate(limit);
            serde_json::to_value(snapshot)
        }
    }
    .map_err(|_| "The typed Host Tools snapshot could not be serialized".to_owned())?;
    redact_error_messages(&mut value);
    Ok(value)
}

fn mapped_log_preset(preset: HostToolLogPreset) -> LogPreset {
    match preset {
        HostToolLogPreset::All => LogPreset::All,
        HostToolLogPreset::Errors => LogPreset::Errors,
        HostToolLogPreset::Auth => LogPreset::Auth,
        HostToolLogPreset::Kernel => LogPreset::Kernel,
        HostToolLogPreset::System => LogPreset::System,
    }
}

fn host_tools_operation_command(
    operation: &HostToolOperation,
    os_type: &str,
) -> Result<String, String> {
    let command = match operation {
        HostToolOperation::ProcessStop { pid } => {
            build_process_action_command(os_type, pid, ProcessActionKind::Stop)?.command
        }
        HostToolOperation::ProcessContinue { pid } => {
            build_process_action_command(os_type, pid, ProcessActionKind::Cont)?.command
        }
        HostToolOperation::ProcessRenice { pid, nice } => {
            build_process_action_command(os_type, pid, ProcessActionKind::Renice { nice: *nice })?
                .command
        }
        HostToolOperation::ProcessTerminate { pid } => {
            build_process_action_command(os_type, pid, ProcessActionKind::Term)?.command
        }
        HostToolOperation::ProcessKill { pid } => {
            build_process_action_command(os_type, pid, ProcessActionKind::Kill)?.command
        }
        HostToolOperation::DockerStart { container_id } => {
            build_docker_action_command(os_type, container_id, DockerActionKind::Start)?.command
        }
        HostToolOperation::DockerStop { container_id } => {
            build_docker_action_command(os_type, container_id, DockerActionKind::Stop)?.command
        }
        HostToolOperation::DockerRestart { container_id } => {
            build_docker_action_command(os_type, container_id, DockerActionKind::Restart)?.command
        }
        HostToolOperation::ServiceStart { service_id } => {
            build_service_action_command(os_type, service_id, ServiceActionKind::Start)?.command
        }
        HostToolOperation::ServiceStop { service_id } => {
            build_service_action_command(os_type, service_id, ServiceActionKind::Stop)?.command
        }
        HostToolOperation::ServiceRestart { service_id } => {
            build_service_action_command(os_type, service_id, ServiceActionKind::Restart)?.command
        }
        HostToolOperation::ServiceReload { service_id } => {
            build_service_action_command(os_type, service_id, ServiceActionKind::Reload)?.command
        }
        HostToolOperation::ServiceEnable { service_id } => {
            build_service_action_command(os_type, service_id, ServiceActionKind::Enable)?.command
        }
        HostToolOperation::ServiceDisable { service_id } => {
            build_service_action_command(os_type, service_id, ServiceActionKind::Disable)?.command
        }
        HostToolOperation::TmuxRenameSession { target, name } => {
            build_tmux_action_command(
                os_type,
                TmuxActionKind::RenameSession {
                    target: target.clone(),
                    name: name.clone(),
                },
            )?
            .command
        }
        HostToolOperation::TmuxRenameWindow { target, name } => {
            build_tmux_action_command(
                os_type,
                TmuxActionKind::RenameWindow {
                    target: target.clone(),
                    name: name.clone(),
                },
            )?
            .command
        }
        HostToolOperation::TmuxKillSession { target } => {
            build_tmux_action_command(
                os_type,
                TmuxActionKind::KillSession {
                    target: target.clone(),
                },
            )?
            .command
        }
        HostToolOperation::TmuxKillWindow { target } => {
            build_tmux_action_command(
                os_type,
                TmuxActionKind::KillWindow {
                    target: target.clone(),
                },
            )?
            .command
        }
        HostToolOperation::TmuxKillPane { target } => {
            build_tmux_action_command(
                os_type,
                TmuxActionKind::KillPane {
                    target: target.clone(),
                },
            )?
            .command
        }
        HostToolOperation::ScheduledTaskRun { id, unit } => {
            build_scheduled_task_action_command(
                os_type,
                ScheduledTaskActionKind::RunNow {
                    id: id.clone(),
                    unit: unit.clone(),
                },
            )?
            .command
        }
        HostToolOperation::ScheduledTaskEnable { id, source } => {
            build_scheduled_task_action_command(
                os_type,
                ScheduledTaskActionKind::Enable {
                    id: id.clone(),
                    source: source.clone(),
                },
            )?
            .command
        }
        HostToolOperation::ScheduledTaskDisable { id, source } => {
            build_scheduled_task_action_command(
                os_type,
                ScheduledTaskActionKind::Disable {
                    id: id.clone(),
                    source: source.clone(),
                },
            )?
            .command
        }
    };
    Ok(command)
}

fn redact_error_messages(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(redact_error_messages),
        Value::Object(fields) => {
            if let Some(Value::Object(error)) = fields.get_mut("error")
                && error.contains_key("message")
            {
                error.insert("message".to_owned(), Value::String("<redacted>".to_owned()));
            }
            fields.values_mut().for_each(redact_error_messages);
        }
        _ => {}
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}
