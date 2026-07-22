// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Typed Host Tools operations over the NodeRouter-owned SSH connection.

use std::{sync::mpsc, time::Duration};

use std::collections::HashMap;

use oxideterm_connection_monitor::{
    DockerActionKind, LogPreset, ProcessActionKind, ProfilerRegistry, ScheduledTaskActionKind,
    ServiceActionKind, TmuxActionKind, build_docker_action_command,
    build_filesystem_snapshot_command, build_log_snapshot_command, build_package_snapshot_command,
    build_port_snapshot_command, build_process_action_command, build_scheduled_task_action_command,
    build_scheduled_task_snapshot_command, build_service_action_command, build_tmux_action_command,
    build_tmux_snapshot_command, docker_sample_command, parse_docker_snapshot,
    parse_filesystem_snapshot, parse_log_snapshot, parse_package_snapshot, parse_port_snapshot,
    parse_scheduled_task_snapshot, parse_service_snapshot, parse_tmux_snapshot,
    service_sample_command,
};
use oxideterm_plugin_protocol as plugin_runtime;
#[cfg(test)]
use oxideterm_plugin_registry::NativePluginHostMonitorContribution;
use oxideterm_plugin_registry::{
    NativePluginContributionStore, NativePluginHostMonitorDef, NativePluginHostMonitorOutputDef,
    NativePluginHostMonitorOutputFormat,
};
use oxideterm_ssh::{NodeId, NodeRouter, SshCommandOutput};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::capabilities::{
    NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_CUSTOM_EXECUTE,
    NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_DESTRUCTIVE, NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_READ,
    NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_WRITE,
};

const HOST_TOOLS_COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
const HOST_TOOLS_RESPONSE_TIMEOUT: Duration = Duration::from_secs(35);
const HOST_TOOLS_MAX_OUTPUT_SIZE: usize = 4 * 1024 * 1024;

/// Builds capability-gated cached Host Tools data without raw process arguments.
pub fn native_plugin_host_tools_snapshot_array(
    registry: &ProfilerRegistry,
    node_connection_ids: &HashMap<String, String>,
) -> Value {
    let mut nodes = node_connection_ids.iter().collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.0.cmp(right.0));
    Value::Array(
        nodes
            .into_iter()
            .filter_map(|(node_id, connection_id)| {
                let metrics = registry.latest(connection_id)?;
                let processes = metrics
                    .top_processes
                    .iter()
                    .map(|process| {
                        json!({
                            "pid": &process.pid,
                            "ppid": &process.ppid,
                            "user": &process.user,
                            "state": &process.state,
                            "cpuPercent": process.cpu_percent,
                            "memoryPercent": process.memory_percent,
                            "rssBytes": process.rss_bytes,
                            "vszBytes": process.vsz_bytes,
                            "elapsed": &process.elapsed,
                            "command": &process.command,
                        })
                    })
                    .collect::<Vec<_>>();
                let mut entry = json!({
                    "nodeId": node_id,
                    "timestampMs": metrics.timestamp_ms,
                    "metrics": {
                        "cpuPercent": metrics.cpu_percent,
                        "memoryUsed": metrics.memory_used,
                        "memoryTotal": metrics.memory_total,
                        "memoryPercent": metrics.memory_percent,
                        "swapUsed": metrics.swap_used,
                        "swapTotal": metrics.swap_total,
                        "swapPercent": metrics.swap_percent,
                        "diskUsed": metrics.disk_used,
                        "diskTotal": metrics.disk_total,
                        "diskPercent": metrics.disk_percent,
                        "loadAvg1": metrics.load_avg_1,
                        "loadAvg5": metrics.load_avg_5,
                        "loadAvg15": metrics.load_avg_15,
                        "cpuCores": metrics.cpu_cores,
                        "netRxBytesPerSec": metrics.net_rx_bytes_per_sec,
                        "netTxBytesPerSec": metrics.net_tx_bytes_per_sec,
                        "sshRttMs": metrics.ssh_rtt_ms,
                    },
                    "processes": processes,
                    "docker": metrics.docker,
                    "services": metrics.services,
                });
                redact_snapshot_error_messages(&mut entry);
                Some(entry)
            })
            .collect(),
    )
}

/// Runs a typed Host Tools call without creating or owning another SSH connection.
pub fn native_plugin_host_tools_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    permissions: &plugin_runtime::PluginPermissionSet,
    contributions: &NativePluginContributionStore,
    router: &NodeRouter,
    runtime: &std::sync::Arc<tokio::runtime::Runtime>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    if call.method == "getExtensions" {
        return plugin_runtime::PluginResponse::ok(
            request_id,
            host_monitor_metadata(contributions, plugin_id),
        );
    }
    if let Err(error) = native_plugin_host_tools_check_capability(&call.method, permissions) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("plugin_host_tools_capability_denied", error),
        );
    }

    let extension = if call.method == "runExtension" {
        let monitor_id = match required_string_arg(&call.args, "monitorId") {
            Ok(monitor_id) => monitor_id,
            Err(error) => {
                return plugin_runtime::PluginResponse::error(
                    request_id,
                    plugin_runtime::PluginError::protocol(
                        "plugin_host_tools_extension_invalid",
                        error,
                    ),
                );
            }
        };
        match contributions.host_monitor(plugin_id, &monitor_id) {
            Some(extension) => Some(extension.definition),
            None => {
                return plugin_runtime::PluginResponse::error(
                    request_id,
                    plugin_runtime::PluginError::protocol(
                        "plugin_host_tools_extension_not_declared",
                        "The requested Host Tools monitor is not declared by this plugin",
                    ),
                );
            }
        }
    } else {
        None
    };

    let method = call.method;
    let args = call.args;
    let router = router.clone();
    let (response_tx, response_rx) = mpsc::channel();
    runtime.spawn(async move {
        let result = native_plugin_host_tools_result(&router, &method, &args, extension).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv_timeout(HOST_TOOLS_RESPONSE_TIMEOUT) {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_host_tools_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_host_tools_unavailable",
                "Host Tools did not return before the protected operation timeout",
            ),
        ),
    }
}

fn native_plugin_host_tools_check_capability(
    method: &str,
    permissions: &plugin_runtime::PluginPermissionSet,
) -> Result<(), String> {
    let required = match method {
        "capture" => NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_READ,
        "execute" => NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_WRITE,
        "terminate" => NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_DESTRUCTIVE,
        "runExtension" => NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_CUSTOM_EXECUTE,
        _ => return Ok(()),
    };
    if permissions
        .capabilities
        .iter()
        .any(|capability| capability == required)
    {
        return Ok(());
    }
    Err(format!(
        "hostTools.{method} requires capability \"{required}\""
    ))
}

async fn native_plugin_host_tools_result(
    router: &NodeRouter,
    method: &str,
    args: &Value,
    extension: Option<NativePluginHostMonitorDef>,
) -> Result<Value, String> {
    let node_id = required_string_arg(args, "nodeId")?;
    let os_type = required_string_arg(args, "osType")?;
    if method == "runExtension" {
        let extension = extension.ok_or_else(|| {
            "The requested Host Tools monitor is not declared by this plugin".to_string()
        })?;
        return run_host_monitor_extension(router, &node_id, &os_type, extension).await;
    }
    let resource = required_string_arg(args, "resource")?;
    let command = match method {
        "capture" => capture_command(&resource, &os_type, args)?,
        "execute" => execute_command(&resource, &os_type, args)?,
        "terminate" => terminate_command(&resource, &os_type, args)?,
        _ => return Err(format!("Unknown hostTools.{method} host API")),
    };
    let output = run_host_tools_command(
        router,
        &node_id,
        command,
        HOST_TOOLS_COMMAND_TIMEOUT,
        HOST_TOOLS_MAX_OUTPUT_SIZE,
    )
    .await?;
    match method {
        "capture" => capture_response(&resource, output),
        "execute" | "terminate" => Ok(json!({
            "success": output.exit_code.unwrap_or_default() == 0,
            "exitCode": output.exit_code,
            "truncated": output.truncated,
        })),
        _ => unreachable!(),
    }
}

fn capture_command(resource: &str, os_type: &str, args: &Value) -> Result<String, String> {
    match resource {
        "docker" => Ok(docker_sample_command(os_type).to_string()),
        "services" => Ok(service_sample_command(os_type).to_string()),
        "logs" => {
            let preset = match args.get("preset").and_then(Value::as_str).unwrap_or("all") {
                "all" => LogPreset::All,
                "errors" => LogPreset::Errors,
                "auth" => LogPreset::Auth,
                "kernel" => LogPreset::Kernel,
                "system" => LogPreset::System,
                value => return Err(format!("Unsupported Host Tools log preset \"{value}\"")),
            };
            build_log_snapshot_command(
                os_type,
                preset,
                args.get("limit").and_then(Value::as_u64).unwrap_or(200) as usize,
            )
            .map(|capture| capture.command)
        }
        "tmux" => Ok(build_tmux_snapshot_command(os_type).command),
        "ports" => Ok(build_port_snapshot_command(os_type).command),
        "filesystems" => Ok(build_filesystem_snapshot_command(os_type).command),
        "packages" => Ok(build_package_snapshot_command(os_type).command),
        "scheduledTasks" => Ok(build_scheduled_task_snapshot_command(os_type).command),
        value => Err(format!(
            "Unsupported Host Tools capture resource \"{value}\""
        )),
    }
}

fn capture_response(resource: &str, output: SshCommandOutput) -> Result<Value, String> {
    let mut snapshot = match resource {
        "docker" => serde_json::to_value(parse_docker_snapshot(&output.stdout)),
        "services" => serde_json::to_value(parse_service_snapshot(&output.stdout)),
        "logs" => serde_json::to_value(parse_log_snapshot(&output.stdout)),
        "tmux" => serde_json::to_value(parse_tmux_snapshot(&output.stdout)),
        "ports" => serde_json::to_value(parse_port_snapshot(&output.stdout)),
        "filesystems" => serde_json::to_value(parse_filesystem_snapshot(&output.stdout)),
        "packages" => serde_json::to_value(parse_package_snapshot(&output.stdout)),
        "scheduledTasks" => serde_json::to_value(parse_scheduled_task_snapshot(&output.stdout)),
        value => {
            return Err(format!(
                "Unsupported Host Tools capture resource \"{value}\""
            ));
        }
    }
    .map_err(|_| "Host Tools could not serialize the typed snapshot".to_string())?;
    redact_snapshot_error_messages(&mut snapshot);
    Ok(json!({
        "resource": resource,
        "snapshot": snapshot,
        "exitCode": output.exit_code,
        "truncated": output.truncated,
    }))
}

fn redact_snapshot_error_messages(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                redact_snapshot_error_messages(value);
            }
        }
        Value::Object(fields) => {
            if let Some(Value::Object(error)) = fields.get_mut("error")
                && error.contains_key("message")
            {
                error.insert(
                    "message".to_string(),
                    Value::String("<redacted>".to_string()),
                );
            }
            for value in fields.values_mut() {
                redact_snapshot_error_messages(value);
            }
        }
        _ => {}
    }
}

fn execute_command(resource: &str, os_type: &str, args: &Value) -> Result<String, String> {
    let action = required_string_arg(args, "action")?;
    match resource {
        "process" => {
            let kind = match action.as_str() {
                "stop" => ProcessActionKind::Stop,
                "continue" => ProcessActionKind::Cont,
                "renice" => ProcessActionKind::Renice {
                    nice: args
                        .get("nice")
                        .and_then(Value::as_i64)
                        .and_then(|value| i32::try_from(value).ok())
                        .ok_or_else(|| {
                            "Host Tools renice requires integer args.nice".to_string()
                        })?,
                },
                _ => {
                    return Err(format!(
                        "Unsupported non-destructive process action \"{action}\""
                    ));
                }
            };
            build_process_action_command(os_type, &required_string_arg(args, "target")?, kind)
                .map(|command| command.command)
        }
        "docker" => {
            let kind = match action.as_str() {
                "start" => DockerActionKind::Start,
                "stop" => DockerActionKind::Stop,
                "restart" => DockerActionKind::Restart,
                _ => return Err(format!("Unsupported Docker action \"{action}\"")),
            };
            build_docker_action_command(os_type, &required_string_arg(args, "target")?, kind)
                .map(|command| command.command)
        }
        "service" => {
            let kind = match action.as_str() {
                "start" => ServiceActionKind::Start,
                "stop" => ServiceActionKind::Stop,
                "restart" => ServiceActionKind::Restart,
                "reload" => ServiceActionKind::Reload,
                "enable" => ServiceActionKind::Enable,
                "disable" => ServiceActionKind::Disable,
                _ => return Err(format!("Unsupported service action \"{action}\"")),
            };
            build_service_action_command(os_type, &required_string_arg(args, "target")?, kind)
                .map(|command| command.command)
        }
        "tmux" => {
            let target = required_string_arg(args, "target")?;
            let kind = match action.as_str() {
                "renameSession" => TmuxActionKind::RenameSession {
                    target,
                    name: required_string_arg(args, "name")?,
                },
                "renameWindow" => TmuxActionKind::RenameWindow {
                    target,
                    name: required_string_arg(args, "name")?,
                },
                "sendPaneCommand" => TmuxActionKind::SendPaneCommand {
                    target,
                    command: required_string_arg(args, "command")?,
                },
                _ => {
                    return Err(format!(
                        "Unsupported non-destructive tmux action \"{action}\""
                    ));
                }
            };
            build_tmux_action_command(os_type, kind).map(|command| command.command)
        }
        "scheduledTask" => {
            let id = required_string_arg(args, "target")?;
            let kind = match action.as_str() {
                "runNow" => ScheduledTaskActionKind::RunNow {
                    id,
                    unit: optional_string_arg(args, "unit"),
                },
                "enable" => ScheduledTaskActionKind::Enable {
                    id,
                    source: required_string_arg(args, "source")?,
                },
                "disable" => ScheduledTaskActionKind::Disable {
                    id,
                    source: required_string_arg(args, "source")?,
                },
                _ => return Err(format!("Unsupported scheduled-task action \"{action}\"")),
            };
            build_scheduled_task_action_command(os_type, kind).map(|command| command.command)
        }
        value => Err(format!(
            "Unsupported Host Tools action resource \"{value}\""
        )),
    }
}

fn terminate_command(resource: &str, os_type: &str, args: &Value) -> Result<String, String> {
    let action = required_string_arg(args, "action")?;
    let target = required_string_arg(args, "target")?;
    match (resource, action.as_str()) {
        ("process", "terminate") => {
            build_process_action_command(os_type, &target, ProcessActionKind::Term)
                .map(|command| command.command)
        }
        ("process", "kill") => {
            build_process_action_command(os_type, &target, ProcessActionKind::Kill)
                .map(|command| command.command)
        }
        ("tmux", "killSession") => {
            build_tmux_action_command(os_type, TmuxActionKind::KillSession { target })
                .map(|command| command.command)
        }
        ("tmux", "killWindow") => {
            build_tmux_action_command(os_type, TmuxActionKind::KillWindow { target })
                .map(|command| command.command)
        }
        ("tmux", "killPane") => {
            build_tmux_action_command(os_type, TmuxActionKind::KillPane { target })
                .map(|command| command.command)
        }
        _ => Err(format!(
            "Unsupported destructive Host Tools action \"{resource}.{action}\""
        )),
    }
}

async fn run_host_tools_command(
    router: &NodeRouter,
    node_id: &str,
    command: String,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<SshCommandOutput, String> {
    let resolved = router
        .resolve_connection_now(&NodeId::new(node_id.to_string()))
        .map_err(|_| "Host Tools requires an active routed node connection".to_string())?;
    // Command builders validate every interpolated identifier. Zeroize the
    // generated shell string after the router-owned request completes.
    let command = Zeroizing::new(command);
    resolved
        .handle
        .run_command_capture(command.as_str(), timeout, max_output_bytes)
        .await
        .map_err(|_| "Host Tools command could not be completed".to_string())
}

fn host_monitor_metadata(contributions: &NativePluginContributionStore, plugin_id: &str) -> Value {
    let monitors = contributions
        .host_monitors_for_plugin(plugin_id)
        .into_iter()
        .map(|entry| {
            let mut platforms = entry
                .definition
                .commands
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            platforms.sort();
            json!({
                "id": entry.definition.id,
                "title": entry.definition.title,
                "description": entry.definition.description,
                "platforms": platforms,
                "outputFormat": entry.definition.output.format,
                "columns": entry.definition.output.columns,
                "timeoutSeconds": entry.definition.timeout_seconds,
                "maxOutputBytes": entry.definition.max_output_bytes,
                "maxRows": entry.definition.output.max_rows,
            })
        })
        .collect::<Vec<_>>();
    Value::Array(monitors)
}

async fn run_host_monitor_extension(
    router: &NodeRouter,
    node_id: &str,
    os_type: &str,
    extension: NativePluginHostMonitorDef,
) -> Result<Value, String> {
    let command = host_monitor_command(&extension, os_type)?.to_string();
    let output = run_host_tools_command(
        router,
        node_id,
        command,
        Duration::from_secs(extension.timeout_seconds),
        extension.max_output_bytes,
    )
    .await?;
    let SshCommandOutput {
        stdout,
        stderr,
        exit_code,
        truncated: command_output_truncated,
    } = output;
    // Arbitrary monitor output may contain credentials. Keep both streams in
    // zeroizing storage even though stderr never crosses the plugin boundary.
    let stdout = Zeroizing::new(stdout);
    let _stderr = Zeroizing::new(stderr);
    let success = exit_code.unwrap_or_default() == 0;
    if !success {
        // Do not expose stdout or stderr from failed arbitrary commands.
        return Ok(json!({
            "monitorId": extension.id,
            "success": false,
            "data": Value::Null,
            "rowCount": 0,
            "exitCode": exit_code,
            "truncated": command_output_truncated,
        }));
    }
    let parsed = parse_host_monitor_output(stdout.as_str(), &extension.output)?;
    Ok(json!({
        "monitorId": extension.id,
        "success": true,
        "data": parsed.data,
        "rowCount": parsed.row_count,
        "exitCode": exit_code,
        "truncated": command_output_truncated || parsed.truncated,
    }))
}

fn host_monitor_command<'a>(
    extension: &'a NativePluginHostMonitorDef,
    os_type: &str,
) -> Result<&'a str, String> {
    let normalized = os_type.trim().to_ascii_lowercase();
    let platform = if normalized.contains("windows") {
        "windows"
    } else if normalized.contains("darwin") || normalized.contains("macos") {
        "macos"
    } else if normalized.contains("bsd") {
        "bsd"
    } else if normalized.contains("linux") {
        "linux"
    } else {
        "default"
    };
    extension
        .commands
        .get(platform)
        .or_else(|| extension.commands.get("default"))
        .map(String::as_str)
        .ok_or_else(|| {
            format!(
                "Host Tools monitor \"{}\" has no command for this host platform",
                extension.id
            )
        })
}

struct ParsedHostMonitorOutput {
    data: Value,
    row_count: usize,
    truncated: bool,
}

fn parse_host_monitor_output(
    stdout: &str,
    output: &NativePluginHostMonitorOutputDef,
) -> Result<ParsedHostMonitorOutput, String> {
    match output.format {
        NativePluginHostMonitorOutputFormat::Json => parse_host_monitor_json(stdout, output),
        NativePluginHostMonitorOutputFormat::JsonLines => {
            parse_host_monitor_json_lines(stdout, output)
        }
        NativePluginHostMonitorOutputFormat::Tsv => parse_host_monitor_tsv(stdout, output),
        NativePluginHostMonitorOutputFormat::TextLines => {
            Ok(parse_host_monitor_text_lines(stdout, output))
        }
    }
}

fn parse_host_monitor_json(
    stdout: &str,
    output: &NativePluginHostMonitorOutputDef,
) -> Result<ParsedHostMonitorOutput, String> {
    let mut data = serde_json::from_str::<Value>(stdout.trim())
        .map_err(|_| "Host Tools monitor returned invalid json output".to_string())?;
    let (row_count, truncated) = if let Some(rows) = data.as_array_mut() {
        let truncated = rows.len() > output.max_rows;
        rows.truncate(output.max_rows);
        (rows.len(), truncated)
    } else {
        (usize::from(!data.is_null()), false)
    };
    Ok(ParsedHostMonitorOutput {
        data,
        row_count,
        truncated,
    })
}

fn parse_host_monitor_json_lines(
    stdout: &str,
    output: &NativePluginHostMonitorOutputDef,
) -> Result<ParsedHostMonitorOutput, String> {
    let mut rows = Vec::new();
    let mut truncated = false;
    for (line_index, line) in stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        if rows.len() == output.max_rows {
            truncated = true;
            break;
        }
        let row = serde_json::from_str::<Value>(line).map_err(|_| {
            format!(
                "Host Tools monitor returned invalid jsonLines output at line {}",
                line_index + 1
            )
        })?;
        rows.push(row);
    }
    let row_count = rows.len();
    Ok(ParsedHostMonitorOutput {
        data: Value::Array(rows),
        row_count,
        truncated,
    })
}

fn parse_host_monitor_tsv(
    stdout: &str,
    output: &NativePluginHostMonitorOutputDef,
) -> Result<ParsedHostMonitorOutput, String> {
    let mut rows = Vec::new();
    let mut truncated = false;
    for (line_index, line) in stdout.lines().filter(|line| !line.is_empty()).enumerate() {
        if rows.len() == output.max_rows {
            truncated = true;
            break;
        }
        let values = line.split('\t').collect::<Vec<_>>();
        if values.len() != output.columns.len() {
            return Err(format!(
                "Host Tools monitor returned a tsv row with the wrong column count at line {}",
                line_index + 1
            ));
        }
        let row = output
            .columns
            .iter()
            .zip(values)
            .map(|(column, value)| (column.clone(), Value::String(value.to_string())))
            .collect();
        rows.push(Value::Object(row));
    }
    let row_count = rows.len();
    Ok(ParsedHostMonitorOutput {
        data: Value::Array(rows),
        row_count,
        truncated,
    })
}

fn parse_host_monitor_text_lines(
    stdout: &str,
    output: &NativePluginHostMonitorOutputDef,
) -> ParsedHostMonitorOutput {
    let lines = stdout.lines().collect::<Vec<_>>();
    let truncated = lines.len() > output.max_rows;
    let rows = lines
        .into_iter()
        .take(output.max_rows)
        .map(|line| Value::String(line.to_string()))
        .collect::<Vec<_>>();
    let row_count = rows.len();
    ParsedHostMonitorOutput {
        data: Value::Array(rows),
        row_count,
        truncated,
    }
}

fn required_string_arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("Host Tools requires non-empty args.{key}"))
}

fn optional_string_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_monitor_definition(
        format: NativePluginHostMonitorOutputFormat,
    ) -> NativePluginHostMonitorDef {
        NativePluginHostMonitorDef {
            id: "workers".to_string(),
            title: "Workers".to_string(),
            description: Some("Worker rows".to_string()),
            commands: HashMap::from([
                ("linux".to_string(), "private sampler command".to_string()),
                ("default".to_string(), "fallback command".to_string()),
            ]),
            output: NativePluginHostMonitorOutputDef {
                format,
                columns: Vec::new(),
                max_rows: 2,
            },
            timeout_seconds: 10,
            max_output_bytes: 256 * 1024,
        }
    }

    #[test]
    fn destructive_process_actions_are_not_accepted_by_execute() {
        let args = json!({ "action": "kill", "target": "42" });
        assert!(execute_command("process", "Linux", &args).is_err());
        assert!(terminate_command("process", "Linux", &args).is_ok());
    }

    #[test]
    fn capture_commands_are_selected_from_closed_resource_names() {
        assert!(capture_command("docker", "Linux", &json!({})).is_ok());
        assert!(capture_command("arbitrary", "Linux", &json!({})).is_err());
    }

    #[test]
    fn typed_snapshot_errors_are_redacted_before_crossing_plugin_boundary() {
        let mut snapshot = json!({
            "status": { "error": { "message": "token=private" } },
            "entries": [{ "message": "credential-like remote output" }],
        });
        redact_snapshot_error_messages(&mut snapshot);
        assert_eq!(snapshot["status"]["error"]["message"], "<redacted>");
        assert_eq!(
            snapshot["entries"][0]["message"],
            "credential-like remote output"
        );
        assert!(!snapshot.to_string().contains("private"));
    }

    #[test]
    fn extension_metadata_is_plugin_scoped_and_hides_commands() {
        let store = NativePluginContributionStore {
            host_monitors: vec![
                NativePluginHostMonitorContribution {
                    plugin_id: "com.example.demo".to_string(),
                    plugin_name: "Demo".to_string(),
                    definition: host_monitor_definition(
                        NativePluginHostMonitorOutputFormat::JsonLines,
                    ),
                },
                NativePluginHostMonitorContribution {
                    plugin_id: "com.example.other".to_string(),
                    plugin_name: "Other".to_string(),
                    definition: host_monitor_definition(NativePluginHostMonitorOutputFormat::Json),
                },
            ],
            ..NativePluginContributionStore::default()
        };

        let metadata = host_monitor_metadata(&store, "com.example.demo");
        assert_eq!(metadata.as_array().unwrap().len(), 1);
        assert_eq!(metadata[0]["id"], "workers");
        assert_eq!(metadata[0]["outputFormat"], "jsonLines");
        assert!(metadata[0].get("commands").is_none());
        assert!(!metadata.to_string().contains("private sampler command"));
    }

    #[test]
    fn custom_monitor_execution_requires_its_dedicated_capability() {
        let denied = native_plugin_host_tools_check_capability(
            "runExtension",
            &plugin_runtime::PluginPermissionSet::default(),
        );
        assert!(denied.is_err());
        let allowed = native_plugin_host_tools_check_capability(
            "runExtension",
            &plugin_runtime::PluginPermissionSet {
                capabilities: vec![NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_CUSTOM_EXECUTE.to_string()],
                allowed_host_apis: Vec::new(),
            },
        );
        assert!(allowed.is_ok());
    }

    #[test]
    fn extension_command_uses_platform_then_default_without_substitution() {
        let extension = host_monitor_definition(NativePluginHostMonitorOutputFormat::Json);
        assert_eq!(
            host_monitor_command(&extension, "Linux").unwrap(),
            "private sampler command"
        );
        assert_eq!(
            host_monitor_command(&extension, "Solaris").unwrap(),
            "fallback command"
        );
    }

    #[test]
    fn extension_output_parsers_enforce_shape_and_row_limits() {
        let json_lines = NativePluginHostMonitorOutputDef {
            format: NativePluginHostMonitorOutputFormat::JsonLines,
            columns: Vec::new(),
            max_rows: 2,
        };
        let parsed =
            parse_host_monitor_output("{\"pid\":1}\n{\"pid\":2}\n{\"pid\":3}", &json_lines)
                .unwrap();
        assert_eq!(parsed.row_count, 2);
        assert!(parsed.truncated);
        assert_eq!(parsed.data.as_array().unwrap().len(), 2);
        assert!(parse_host_monitor_output("{\"pid\":1}\nnot-json", &json_lines).is_err());

        let tsv = NativePluginHostMonitorOutputDef {
            format: NativePluginHostMonitorOutputFormat::Tsv,
            columns: vec!["pid".to_string(), "name".to_string()],
            max_rows: 2,
        };
        let parsed = parse_host_monitor_output("1\tinit\n2\tworker", &tsv).unwrap();
        assert_eq!(parsed.data[1]["name"], "worker");
        assert!(parse_host_monitor_output("1", &tsv).is_err());
    }
}
