use super::*;

impl HostToolsEntity {
    pub fn execute_ai_action(
        &mut self,
        connection_id: String,
        resource: &str,
        action: &str,
        entity_id: &str,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let runtime = self
            .lifecycle_runtime
            .clone()
            .ok_or_else(|| "The Host Tools runtime is unavailable.".to_string())?;
        let notices = match resource {
            "process" => {
                let signal = value.unwrap_or("term").trim().to_ascii_lowercase();
                let action = match (action, signal.as_str()) {
                    ("signal", "term") => ProcessActionKind::Term,
                    ("signal", "kill") => ProcessActionKind::Kill,
                    ("signal", "stop") => ProcessActionKind::Stop,
                    ("signal", "cont") => ProcessActionKind::Cont,
                    _ => return Err("The requested process signal is unsupported.".to_string()),
                };
                self.start_process_action(
                    HostProcessActionRun {
                        connection_id,
                        pid: entity_id.to_string(),
                        action,
                    },
                    runtime,
                    cx,
                )
            }
            "docker" => {
                let action = match action {
                    "start" => DockerActionKind::Start,
                    "stop" => DockerActionKind::Stop,
                    "restart" => DockerActionKind::Restart,
                    _ => return Err("The requested Docker action is unsupported.".to_string()),
                };
                self.start_docker_action(
                    HostDockerActionRequest {
                        connection_id,
                        container_id: entity_id.to_string(),
                        container_name: entity_id.to_string(),
                        action,
                    },
                    runtime,
                    cx,
                )
            }
            "service" => {
                let action = match action {
                    "start" => ServiceActionKind::Start,
                    "stop" => ServiceActionKind::Stop,
                    "restart" => ServiceActionKind::Restart,
                    "enable" => ServiceActionKind::Enable,
                    "disable" => ServiceActionKind::Disable,
                    _ => return Err("The requested service action is unsupported.".to_string()),
                };
                self.start_service_action(
                    HostServiceActionRequest {
                        connection_id,
                        service_id: entity_id.to_string(),
                        description: entity_id.to_string(),
                        action,
                    },
                    runtime,
                    cx,
                )
            }
            "tmux" => {
                let request = HostTmuxActionRun {
                    connection_id: connection_id.clone(),
                    session_id: entity_id.to_string(),
                    session_name: entity_id.to_string(),
                    target_label: entity_id.to_string(),
                };
                match action {
                    "kill_session" | "kill_window" | "kill_pane" => {
                        let destructive_action = match action {
                            "kill_session" => HostTmuxDestructiveAction::KillSession {
                                target: entity_id.to_string(),
                            },
                            "kill_window" => HostTmuxDestructiveAction::KillWindow {
                                target: entity_id.to_string(),
                            },
                            _ => HostTmuxDestructiveAction::KillPane {
                                target: entity_id.to_string(),
                            },
                        };
                        self.start_tmux_action(
                            HostTmuxActionRequest {
                                connection_id,
                                session_id: entity_id.to_string(),
                                session_name: entity_id.to_string(),
                                target_label: entity_id.to_string(),
                                action: destructive_action,
                            },
                            runtime,
                            cx,
                        )
                    }
                    "rename_session" | "rename_window" | "send_pane_command" => {
                        let value = value
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                "The requested tmux action requires a value.".to_string()
                            })?;
                        let os_type = self.connection_os_type(&connection_id).ok_or_else(|| {
                            "The Host Tools connection is unavailable.".to_string()
                        })?;
                        let command = match action {
                            "rename_session" => {
                                build_tmux_rename_session_command(&os_type, entity_id, value)
                            }
                            "rename_window" => {
                                build_tmux_rename_window_command(&os_type, entity_id, value)
                            }
                            _ => build_tmux_send_pane_command(&os_type, entity_id, value),
                        }
                        .map_err(|_| "The requested tmux action is invalid.".to_string())?;
                        self.start_tmux_action_command(command, request, runtime, cx)
                    }
                    _ => return Err("The requested tmux action is unsupported.".to_string()),
                }
            }
            "schedule" => {
                let task = self
                    .host_schedules
                    .snapshot
                    .as_ref()
                    .filter(|_| {
                        self.host_schedules.snapshot_connection_id.as_deref()
                            == Some(connection_id.as_str())
                    })
                    .and_then(|snapshot| snapshot.entries.iter().find(|task| task.id == entity_id))
                    .cloned()
                    .ok_or_else(|| {
                        "Refresh scheduled tasks before controlling this task.".to_string()
                    })?;
                let scheduled_action = match action {
                    "run" => ScheduledTaskActionKind::RunNow {
                        id: task.id.clone(),
                        unit: task.unit.clone(),
                    },
                    "enable" => ScheduledTaskActionKind::Enable {
                        id: task.id.clone(),
                        source: task.source.clone(),
                    },
                    "disable" => ScheduledTaskActionKind::Disable {
                        id: task.id.clone(),
                        source: task.source.clone(),
                    },
                    _ => {
                        return Err(
                            "The requested scheduled task action is unsupported.".to_string()
                        );
                    }
                };
                self.start_schedule_action(
                    HostScheduleActionRequest {
                        connection_id,
                        task_id: task.id,
                        task_name: task.name,
                        unit: task.unit,
                        action: scheduled_action,
                    },
                    runtime,
                    cx,
                )
            }
            _ => return Err("The requested Host Tools resource is unsupported.".to_string()),
        };
        for notice in notices {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
        Ok(())
    }

    pub fn ai_snapshot(&self, connection_id: &str, resource: &str) -> serde_json::Value {
        let latest_metrics = self.profiler_registry.latest(connection_id);
        match resource {
            "overview" => latest_metrics
                .map(|metrics| serde_json::to_value(metrics).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null),
            "processes" => latest_metrics
                .map(|metrics| serde_json::to_value(metrics.top_processes).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null),
            "docker" => latest_metrics
                .map(|metrics| serde_json::to_value(metrics.docker).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null),
            "services" => self
                .host_services
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_services.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "tmux" => self
                .host_tmux
                .snapshot
                .as_ref()
                .filter(|_| self.host_tmux.snapshot_connection_id.as_deref() == Some(connection_id))
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "ports" => self
                .host_ports
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_ports.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "filesystems" => self
                .host_filesystems
                .snapshot_for(connection_id)
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "packages" => self
                .host_packages
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_packages.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "schedules" => self
                .host_schedules
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_schedules.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "logs" => self
                .log_snapshot_for(connection_id)
                .map(|snapshot| {
                    // AI receives bounded log metadata, never raw log messages.
                    let mut levels = std::collections::BTreeSet::new();
                    let mut sources = std::collections::BTreeSet::new();
                    let mut units = std::collections::BTreeSet::new();
                    for entry in &snapshot.entries {
                        if !entry.level.trim().is_empty() {
                            levels.insert(entry.level.clone());
                        }
                        if !entry.source.trim().is_empty() {
                            sources.insert(entry.source.clone());
                        }
                        if !entry.unit.trim().is_empty() {
                            units.insert(entry.unit.clone());
                        }
                    }
                    serde_json::json!({
                        "status": snapshot.status,
                        "entryCount": snapshot.entries.len(),
                        "levels": levels.into_iter().take(32).collect::<Vec<_>>(),
                        "sources": sources.into_iter().take(32).collect::<Vec<_>>(),
                        "units": units.into_iter().take(32).collect::<Vec<_>>(),
                    })
                })
                .unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Null,
        }
    }

    pub fn request_ai_snapshot_refresh(
        &mut self,
        connection_id: String,
        resource: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return false;
        };
        let Some(messages) = self.messages.clone() else {
            return false;
        };
        match resource {
            "overview" | "processes" | "docker" => {
                if !self.visibility.sidebar_is_visible() {
                    // Resource snapshots are scoped to the mounted Host Tools sidebar.
                    return false;
                }
                self.start_profiler(connection_id, self.sampling_config, runtime, cx);
            }
            "services" => {
                self.request_service_snapshot(
                    connection_id,
                    runtime,
                    messages.service_connection_missing,
                    messages.service_action_failed,
                    cx,
                );
            }
            "tmux" => {
                for notice in self.request_tmux_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    String::new(),
                    messages.tmux_unknown_error,
                    messages.tmux_unavailable,
                    runtime,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "ports" => {
                for notice in self.request_port_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.port_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "filesystems" => {
                for notice in self.request_filesystem_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.filesystem_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "packages" => {
                for notice in self.request_package_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.package_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "schedules" => {
                for notice in self.request_schedule_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.schedule_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "logs" => {
                for notice in self.request_log_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    self.monitoring.logs_enabled,
                    runtime,
                    messages.log_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            _ => return false,
        }
        true
    }
}
