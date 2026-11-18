use std::collections::HashMap;
use std::hash::Hash;

use oxideterm_settings::PersistedSettings;
use oxideterm_workspace::{ActiveSessionReadiness, TabId, TabKind, TerminalSessionId};

use super::*;
use crate::{AiExecutedToolResult, tool_result_protocol::prepare_ai_tool_output};

const AI_TARGET_DISCOVERY_LIMIT: usize = 128;

pub struct AiOrchestratorRuntimeSnapshot {
    pub targets: Vec<AiOrchestratorTarget>,
    /// Maps internal target identities to the current tool session's opaque handles.
    pub runtime_handles: HashMap<String, crate::RuntimeHandleProjection>,
    pub active_tab: Option<serde_json::Value>,
    pub active_node: Option<serde_json::Value>,
    pub active_session_id: Option<String>,
    pub active_tab_id: Option<String>,
    pub active_node_id: Option<String>,
    pub memory: serde_json::Value,
    pub health_state: serde_json::Value,
    pub transfers_state: serde_json::Value,
    pub model_visible_settings: serde_json::Value,
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn background_result_projection() -> Self {
        // Background MCP execution needs only the shared result formatter. An
        // empty projection prevents raw application identifiers entering the loop.
        Self {
            targets: Vec::new(),
            runtime_handles: HashMap::new(),
            active_tab: None,
            active_node: None,
            active_session_id: None,
            active_tab_id: None,
            active_node_id: None,
            memory: serde_json::Value::Null,
            health_state: serde_json::Value::Null,
            transfers_state: serde_json::Value::Null,
            model_visible_settings: serde_json::Value::Null,
        }
    }
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn connect_target_ready_result(
        &self,
        resource_ref: &crate::StableResourceRef,
        connection_label: Option<&str>,
        tool_call_id: &str,
        tool_name: &str,
        duration_ms: u128,
    ) -> Option<AiExecutedToolResult> {
        let ready_targets = self
            .targets
            .iter()
            .filter(|target| {
                matches!(target.kind.as_str(), "ssh-node" | "terminal-session")
                    && target.state == "connected"
                    && target
                        .refs
                        .get("connectionId")
                        .is_some_and(|id| id == resource_ref.id())
            })
            .cloned()
            .collect::<Vec<_>>();
        let primary = ready_targets
            .iter()
            .find(|target| target.kind == "terminal-session")
            .or_else(|| ready_targets.first())?
            .clone();
        let label = connection_label
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or("saved connection");
        Some(
            self.to_executed_tool_result(
                tool_call_id.to_string(),
                tool_name.to_string(),
                self.ok(
                    format!("Connected {label}."),
                    "A live terminal is ready.",
                    serde_json::json!({ "resourceRef": resource_ref }),
                    "write",
                )
                .with_target(primary)
                .with_targets(ready_targets),
                duration_ms,
            ),
        )
    }

    pub fn connect_target_timeout_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        duration_ms: u128,
    ) -> AiExecutedToolResult {
        let target = match ai_stable_resource_operation("connect_target", args) {
            Ok(AiStableResourceOperation::SavedConnection(resource_ref)) => self
                .targets
                .iter()
                .find(|target| {
                    target.kind == "saved-connection"
                        && target
                            .refs
                            .get("connectionId")
                            .is_some_and(|id| id == resource_ref.id())
                })
                .cloned(),
            _ => None,
        };
        let next_actions = target
            .as_ref()
            .map(|target| {
                vec![serde_json::json!({
                    "action": "select_target",
                    "args": { "query": target.label },
                    "reason": "Re-select the saved connection after credentials are updated."
                })]
            })
            .unwrap_or_default();
        self.to_executed_tool_result(
            tool_call_id.to_string(),
            tool_name.to_string(),
            self.fail(
                "Connection did not complete.",
                "connect_failed",
                "The saved connection flow did not return a live terminal.",
                "write",
            )
            .with_optional_target(target)
            .with_next_actions(next_actions),
            duration_ms,
        )
    }
}

pub struct AiActiveNodeCapture<'a> {
    pub host: &'a str,
    pub username: &'a str,
    pub readiness: ActiveSessionReadiness,
}

pub struct AiRuntimeHealthCapture<I> {
    pub open_tabs: usize,
    pub has_active_tab: bool,
    pub pane_count: usize,
    pub local_session_count: usize,
    pub node_states: I,
    pub recent_events: (usize, usize, usize),
}

fn node_readiness_label(readiness: ActiveSessionReadiness) -> &'static str {
    match readiness {
        ActiveSessionReadiness::Ready => "connected",
        ActiveSessionReadiness::Connecting => "connecting",
        ActiveSessionReadiness::Error => "error",
        ActiveSessionReadiness::Disconnected => "disconnected",
    }
}

pub fn finish_ai_workspace_snapshot(
    mut snapshot: AiOrchestratorRuntimeSnapshot,
    active_tab: Option<(&TabKind, &str)>,
    active_node: Option<AiActiveNodeCapture<'_>>,
    active_session_id: Option<TerminalSessionId>,
    active_tab_id: Option<TabId>,
    active_node_id: Option<&str>,
    settings: &PersistedSettings,
    transfers: serde_json::Value,
    health: AiRuntimeHealthCapture<impl Iterator<Item = ActiveSessionReadiness>>,
) -> AiOrchestratorRuntimeSnapshot {
    snapshot.active_tab = active_tab.map(
        |(kind, title)| serde_json::json!({ "type": ai_tab_kind_label(kind), "title": title }),
    );
    snapshot.active_node = active_node.map(|node| {
        serde_json::json!({
            "host": node.host,
            "username": node.username,
            "status": node_readiness_label(node.readiness),
        })
    });
    snapshot.active_session_id = active_session_id.map(|id| id.0.to_string());
    snapshot.active_tab_id = active_tab_id.map(|id| id.0.to_string());
    snapshot.active_node_id = active_node_id.map(str::to_owned);
    snapshot.memory = ai_memory_settings_json(
        settings.ai.memory.enabled,
        &settings.ai.memory.content,
        &settings.ai.memory.entries,
    );
    snapshot.model_visible_settings = ai_model_visible_settings_projection(settings);

    let mut states = BTreeMap::<_, usize>::new();
    let mut total = 0;
    for state in health.node_states {
        *states.entry(node_readiness_label(state)).or_default() += 1;
        total += 1;
    }
    let (events, warnings, errors) = health.recent_events;
    snapshot.health_state = serde_json::json!({
        "tabs": { "open": health.open_tabs, "hasActiveTab": health.has_active_tab },
        "terminalRegistry": { "entries": health.pane_count },
        "localTerminals": { "count": health.local_session_count },
        "sshNodes": { "total": total, "states": states },
        "transfers": {
            "total": transfers.get("total").and_then(serde_json::Value::as_u64).unwrap_or(0),
            "counts": transfers.get("counts").cloned().unwrap_or_else(|| serde_json::json!({})),
        },
        "recentEvents": { "total": events, "warnings": warnings, "errors": errors },
    });
    snapshot.transfers_state = transfers;
    snapshot
}

pub fn ai_runtime_context_prompt<N: Clone + Eq + Hash>(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    authority: &crate::AiRuntimeAuthority<N>,
    session: &crate::ToolSessionId,
    observed_at_ms: i64,
) -> String {
    let mut stable_resources = if authority.is_agent_scoped_session(session) {
        Vec::new()
    } else {
        ai_app_surface_stable_resources()
    };
    for resource in snapshot
        .targets
        .iter()
        .filter_map(ai_stable_resource_ref_for_target)
    {
        if !stable_resources.contains(&resource) {
            stable_resources.push(resource);
        }
        if stable_resources.len() >= AI_TARGET_DISCOVERY_LIMIT {
            break;
        }
    }
    let projection = crate::RuntimeContextSnapshot {
        protocol_version: 2,
        snapshot_id: format!("snap_{}", uuid::Uuid::new_v4().simple()),
        observed_at_ms,
        registry_epoch: authority.registry_epoch(),
        stable_resources,
        live_handles: authority.current_handle_projections(session),
    };
    serde_json::to_string_pretty(&serde_json::json!({ "runtimeContext": projection }))
        .map(|text| ai_model_safe_runtime_text(&text))
        .unwrap_or_else(|_| "{\"runtimeContext\":{\"protocolVersion\":2}}".to_owned())
}

/// Application surfaces are durable navigation destinations, not live tab identities.
pub fn ai_app_surface_stable_resources() -> Vec<crate::StableResourceRef> {
    const SURFACES: &[(&str, &str)] = &[
        ("settings", "Settings"),
        ("connection_manager", "Connection manager"),
        ("connection_pool", "Connection pool"),
        ("connection_monitor", "Connection monitor"),
        ("sftp", "SFTP"),
        ("ide", "IDE"),
        ("file_manager", "File manager"),
        ("local_terminal", "Local terminal"),
        ("terminal", "Terminal"),
    ];

    SURFACES
        .iter()
        .filter_map(|(id, label)| {
            crate::StableResourceRef::new(
                crate::StableResourceKind::AppSurface,
                (*id).to_string(),
                Some((*label).to_string()),
            )
            .ok()
        })
        .collect()
}

pub fn ai_transfer_path_looks_directory(path: &str) -> bool {
    // Tauri uses /[\\/]$/ so both POSIX and Windows-style trailing separators
    // select directory transfer semantics.
    path.ends_with('/') || path.ends_with('\\')
}

pub fn ai_application_action_result(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    result: Result<serde_json::Value, String>,
    summary: &str,
    risk: &'static str,
) -> AiActionResultLite {
    match result {
        Ok(data) => snapshot.ok(
            summary,
            serde_json::to_string_pretty(&data).unwrap_or_default(),
            data,
            risk,
        ),
        Err(message) => snapshot.fail(summary, "application_tool_failed", message, risk),
    }
}

pub fn ai_target_state<N: Clone + Eq + std::hash::Hash>(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    authority: &crate::AiRuntimeAuthority<N>,
    tool_session_id: &crate::ToolSessionId,
    args: &serde_json::Value,
    saved_connection_exists: impl Fn(&str) -> bool,
) -> AiActionResultLite {
    if args.get("scope").and_then(serde_json::Value::as_str) != Some("target") {
        return snapshot.get_state(args);
    }
    if args.get("handle_id").is_some() {
        return match authority.validate_state_handle(
            tool_session_id,
            args.get("handle_id").and_then(serde_json::Value::as_str),
        ) {
            Ok(handle) => snapshot.ok(
                "Read current target state.",
                serde_json::to_string_pretty(&handle).unwrap_or_default(),
                serde_json::to_value(handle).unwrap_or(serde_json::Value::Null),
                "read",
            ),
            Err(error) => snapshot.fail(
                "Runtime target is unavailable.",
                error.public_code(),
                "Rediscover current targets before retrying.",
                "read",
            ),
        };
    }

    let Some(resource_ref) = args
        .get("resource_ref")
        .cloned()
        .and_then(|value| serde_json::from_value::<crate::StableResourceRef>(value).ok())
    else {
        return snapshot.fail(
            "Target authority is required.",
            "runtime_handle_missing",
            "Provide one current handle or durable resource reference.",
            "read",
        );
    };
    if resource_ref.kind() == crate::StableResourceKind::SavedConnection
        && !saved_connection_exists(resource_ref.id())
    {
        return snapshot.fail(
            "Saved resource no longer exists.",
            "resource_removed",
            "Rediscover saved connections before retrying.",
            "read",
        );
    }
    let state = snapshot
        .targets
        .iter()
        .find(|target| ai_stable_resource_ref_for_target(target).as_ref() == Some(&resource_ref))
        .and_then(|target| snapshot.model_target_json(target))
        .unwrap_or_else(|| {
            serde_json::json!({
                "authority": {
                    "kind": "stable_resource",
                    "resource_ref": resource_ref,
                },
                "state": "available",
            })
        });
    snapshot.ok(
        "Read durable target state.",
        serde_json::to_string_pretty(&state).unwrap_or_default(),
        state,
        "read",
    )
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn list_targets(&self, args: &serde_json::Value) -> AiActionResultLite {
        let view = normalized_ai_target_view(args.get("view").and_then(serde_json::Value::as_str));
        let query = normalized_ai_query(args.get("query").and_then(serde_json::Value::as_str));
        let kind = args
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("all");
        let targets = self
            .targets
            .iter()
            .filter(|target| kind == "all" || target.kind == kind)
            .filter(|target| target_in_ai_view(target, view))
            .filter(|target| target_matches_ai_query(target, &query))
            .take(AI_TARGET_DISCOVERY_LIMIT)
            .cloned()
            .collect::<Vec<_>>();
        let model_targets = targets
            .iter()
            .filter_map(|target| self.model_target_json(target))
            .collect::<Vec<_>>();
        let output = model_targets
            .iter()
            .map(|target| {
                let label = target
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Target");
                let kind = target
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown");
                let state = target
                    .get("state")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("available");
                format!("{label} [{kind}, {state}]")
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.ok(
            format!(
                "Found {} target{}.",
                model_targets.len(),
                if model_targets.len() == 1 { "" } else { "s" }
            ),
            if output.is_empty() {
                "No targets found.".to_string()
            } else {
                output
            },
            serde_json::Value::Array(model_targets),
            "read",
        )
        .with_targets(targets)
    }

    pub fn select_target(&self, args: &serde_json::Value) -> AiActionResultLite {
        let query = args
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let Some(intent) =
            normalized_ai_intent(args.get("intent").and_then(serde_json::Value::as_str))
        else {
            return self
                .fail(
                    "Target intent is required.",
                    "missing_target_intent",
                    "select_target requires intent: connection, command, terminal, settings, file, sftp, app_surface, knowledge, status, local, or unknown.",
                    "read",
                )
                .with_next_actions(vec![serde_json::json!({
                        "action": "list_targets",
                        "args": { "view": "connections", "query": query },
                        "reason": "Inspect the correct target view before selecting."
                    })]);
        };
        if matches!(intent, "command" | "terminal") && is_ai_command_like_query(query) {
            let view = if intent == "command" {
                "live_sessions"
            } else {
                "connections"
            };
            return self
                .fail(
                    "Command text is not a target.",
                    "command_query_not_target",
                    format!("{query:?} looks like a command. Select a live SSH or terminal target first, then call run_command with this command."),
                    "read",
                )
                .with_next_actions(vec![serde_json::json!({
                        "action": "list_targets",
                        "args": { "view": view },
                        "reason": "Choose the execution target before running the command."
                    })]);
        }
        let view = view_for_ai_intent(intent);
        let lowered = normalized_ai_query(Some(query));
        let select_kind =
            normalized_ai_select_target_kind(args.get("kind").and_then(serde_json::Value::as_str));
        let matches = self
            .targets
            .iter()
            .filter(|target| target_in_ai_view(target, view))
            // Tauri validates select_target.kind before filtering; unknown
            // values are ignored instead of producing an empty candidate set.
            .filter(|target| select_kind.is_none_or(|kind| kind == "all" || target.kind == kind))
            .filter(|target| target_matches_ai_query(target, &lowered))
            .take(AI_TARGET_DISCOVERY_LIMIT)
            .cloned()
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => {
                let mut next_actions = vec![serde_json::json!({
                    "action": "list_targets",
                    "args": { "view": view, "query": query },
                    "reason": "Inspect available targets and ask the user to choose."
                })];
                if matches!(intent, "command" | "terminal") {
                    next_actions.push(serde_json::json!({
                        "action": "list_targets",
                        "args": { "view": "connections", "query": query },
                        "reason": "If the named host is saved but not live, connect it before running commands."
                    }));
                }
                self.fail(
                    "No matching target found.",
                    "target_not_found",
                    format!("No target matched \"{query}\"."),
                    "read",
                )
                .with_next_actions(next_actions)
            }
            [target] => {
                let Some(model_target) = self.model_target_json(target) else {
                    return self.fail(
                        "Target is no longer available.",
                        "runtime_owner_closed",
                        "Rediscover the current terminal target before retrying.",
                        "read",
                    );
                };
                self.ok(
                    format!("Selected target: {}", target.label),
                    serde_json::to_string_pretty(&model_target)
                        .unwrap_or_else(|_| target.label.clone()),
                    model_target,
                    "read",
                )
                .with_target(target.clone())
            }
            _ => {
                let mut retry_args = serde_json::Map::from_iter([
                    ("query".to_string(), serde_json::json!(query)),
                    ("intent".to_string(), serde_json::json!(intent)),
                ]);
                if let Some(kind) = args.get("kind").and_then(serde_json::Value::as_str) {
                    retry_args.insert("kind".to_string(), serde_json::json!(kind));
                }
                self.fail(
                    "Multiple targets match. Ask the user to choose one.",
                    "target_disambiguation_required",
                    matches
                        .iter()
                        .enumerate()
                        .map(|(index, target)| {
                            format!(
                                "{}. {} [{}]",
                                index + 1,
                                target.label,
                                target.kind
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    "read",
                )
                .with_targets(matches)
                .with_next_actions(vec![serde_json::json!({
                    "action": "select_target",
                    "args": retry_args,
                    "reason": "Retry with a more specific label or host, or add a target kind filter."
                })])
            }
        }
    }
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn get_state(&self, args: &serde_json::Value) -> AiActionResultLite {
        let scope = args
            .get("scope")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("targets");
        let valid_scope = matches!(
            scope,
            "connections" | "transfers" | "settings" | "targets" | "health" | "active"
        );
        if !valid_scope {
            return self
                .fail(
                    "Unknown state scope.",
                    "unknown_state_scope",
                    format!("Unknown get_state scope \"{scope}\". Valid scopes: connections, transfers, settings, targets, health, active."),
                    "read",
                )
                .with_next_actions(vec![serde_json::json!({
                        "action": "get_state",
                        "args": { "scope": "targets" },
                        "reason": "Inspect valid target state instead."
                    })]);
        }
        let data = match scope {
            "targets" => {
                let view_targets = |view: &str| {
                    self.targets
                        .iter()
                        .filter(|target| target_in_ai_view(target, view))
                        .filter_map(|target| self.model_target_json(target))
                        .collect::<Vec<_>>()
                };
                let connections = view_targets("connections");
                let live_sessions = view_targets("live_sessions");
                let app_surfaces = view_targets("app_surfaces");
                let files = view_targets("files");
                serde_json::json!({
                    "views": {
                        "connections": { "count": connections.len(), "targets": connections },
                        "live_sessions": { "count": live_sessions.len(), "targets": live_sessions },
                        "app_surfaces": { "count": app_surfaces.len(), "targets": app_surfaces },
                        "files": { "count": files.len(), "targets": files },
                        "all": { "count": self.targets.len() },
                    },
                })
            }
            "settings" => self.model_visible_settings.clone(),
            "connections" => {
                let model_targets = self
                    .targets
                    .iter()
                    .filter(|target| target_in_ai_view(target, "connections"))
                    .filter_map(|target| self.model_target_json(target))
                    .collect::<Vec<_>>();
                let counts = ai_connection_counts(&self.targets);
                serde_json::json!({
                    "total": counts.total,
                    "counts": {
                        "saved": counts.saved,
                        "live": counts.live,
                        "linkDown": counts.link_down,
                        "error": counts.error,
                    },
                    "targets": model_targets,
                })
            }
            "transfers" => self.transfers_state.clone(),
            "health" => self.health_state.clone(),
            "active" => serde_json::json!({
                "activeTab": self.active_tab.clone(),
                "activeNode": self.active_node.clone(),
                "targets": self.targets.iter().filter(|target| {
                    target_matches_active_context(
                        target,
                        self.active_tab_id.as_deref(),
                        self.active_node_id.as_deref(),
                        self.active_session_id.as_deref(),
                    )
                }).filter_map(|target| self.model_target_json(target)).collect::<Vec<_>>(),
            }),
            _ => unreachable!("scope was validated above"),
        };
        let state_version = match scope {
            "targets" => make_ai_state_version(
                "targets",
                [
                    self.targets.len().to_string(),
                    self.targets
                        .iter()
                        .filter(|target| target_in_ai_view(target, "connections"))
                        .count()
                        .to_string(),
                    self.targets
                        .iter()
                        .filter(|target| target_in_ai_view(target, "live_sessions"))
                        .count()
                        .to_string(),
                    self.targets
                        .iter()
                        .filter(|target| target_in_ai_view(target, "app_surfaces"))
                        .count()
                        .to_string(),
                    self.targets
                        .iter()
                        .filter(|target| target_in_ai_view(target, "files"))
                        .count()
                        .to_string(),
                ],
            ),
            "active" => make_ai_state_version(
                "active",
                [
                    self.active_tab.is_some().to_string(),
                    self.active_node.is_some().to_string(),
                    data.get("targets")
                        .and_then(serde_json::Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0)
                        .to_string(),
                ],
            ),
            "connections" => make_ai_state_version(
                "connections",
                [
                    self.targets
                        .iter()
                        .filter(|target| target_in_ai_view(target, "connections"))
                        .count()
                        .to_string(),
                    self.targets
                        .iter()
                        .filter(|target| target.kind == "ssh-node" && target.state == "connected")
                        .count()
                        .to_string(),
                    self.targets
                        .iter()
                        .filter(|target| target.kind == "ssh-node" && target.state == "stale")
                        .count()
                        .to_string(),
                    self.targets
                        .iter()
                        .filter(|target| {
                            target.kind == "ssh-node"
                                && target
                                    .metadata
                                    .get("status")
                                    .and_then(serde_json::Value::as_str)
                                    == Some("error")
                        })
                        .count()
                        .to_string(),
                ],
            ),
            "transfers" => make_ai_state_version(
                "transfers",
                [
                    data.get("total")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                    data.pointer("/counts/active")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                    data.pointer("/counts/pending")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                    data.pointer("/counts/error")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                ],
            ),
            "settings" => make_ai_state_version(
                "settings",
                [
                    data.pointer("/ai/enabled")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                        .to_string(),
                    data.pointer("/terminal/renderer")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    data.pointer("/terminal/encoding")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                ],
            ),
            "health" => make_ai_state_version(
                "health",
                [
                    data.pointer("/tabs/open")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                    data.pointer("/terminalRegistry/entries")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                    data.pointer("/transfers/total")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                    data.pointer("/recentEvents/total")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        .to_string(),
                ],
            ),
            _ => unreachable!("scope was validated above"),
        };
        let result_targets = match scope {
            "targets" => self.targets.clone(),
            "connections" => self
                .targets
                .iter()
                .filter(|target| target_in_ai_view(target, "connections"))
                .cloned()
                .collect::<Vec<_>>(),
            "active" => self
                .targets
                .iter()
                .filter(|target| {
                    target_matches_active_context(
                        target,
                        self.active_tab_id.as_deref(),
                        self.active_node_id.as_deref(),
                        self.active_session_id.as_deref(),
                    )
                })
                .cloned()
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let summary = match scope {
            "targets" => format!("Found {} total targets across views.", self.targets.len()),
            "active" => {
                if self.active_tab.is_some() || self.active_node.is_some() {
                    "Read active runtime state.".to_string()
                } else {
                    "No active tab or terminal session.".to_string()
                }
            }
            "settings" => "Read settings summary.".to_string(),
            "connections" => format!("Found {} connection targets.", result_targets.len()),
            "transfers" => format!(
                "Found {} tracked transfers.",
                data.get("total")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
            ),
            "health" => "Read OxideTerm health state.".to_string(),
            _ => unreachable!("scope was validated above"),
        };
        let result = self
            .ok(
                summary,
                serde_json::to_string_pretty(&data).unwrap_or_default(),
                data,
                "read",
            )
            .with_targets(result_targets)
            .with_state_version(state_version);
        result
    }

    pub fn ok(
        &self,
        summary: impl Into<String>,
        output: impl Into<String>,
        data: serde_json::Value,
        risk: &'static str,
    ) -> AiActionResultLite {
        AiActionResultLite {
            images: Vec::new(),
            ok: true,
            summary: summary.into(),
            output: output.into(),
            data,
            error_code: None,
            error_message: None,
            risk,
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        }
    }

    pub fn fail(
        &self,
        summary: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
        risk: &'static str,
    ) -> AiActionResultLite {
        let message = message.into();
        AiActionResultLite {
            images: Vec::new(),
            ok: false,
            summary: summary.into(),
            output: message.clone(),
            data: serde_json::Value::Null,
            error_code: Some(code.into()),
            error_message: Some(message),
            risk,
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        }
    }

    pub fn to_executed_tool_result(
        &self,
        tool_call_id: String,
        tool_name: String,
        mut result: AiActionResultLite,
        duration_ms: u128,
    ) -> AiExecutedToolResult {
        let safe_summary = ai_model_safe_runtime_text(&result.summary);
        let safe_output = ai_model_safe_runtime_text(&result.output);
        let safe_error = result
            .error_message
            .as_deref()
            .map(ai_model_safe_runtime_text);
        // Source URLs are validated by the web adapter and must not be shortened into broken links.
        let sources = crate::web::is_web_tool(&tool_name)
            .then(|| {
                result
                    .data
                    .as_object_mut()
                    .and_then(|data| data.remove("sources"))
            })
            .flatten();
        let (safe_data, data_truncated) = ai_model_safe_runtime_value_with_limits(&result.data);
        let (output, raw_output, output_preview, truncated) = prepare_ai_tool_output(&safe_output);
        let targets = result
            .target
            .iter()
            .chain(result.targets.iter())
            .filter_map(|target| self.model_tool_result_target_json(target))
            .collect::<Vec<_>>();
        let next_actions = result
            .next_actions
            .iter()
            .filter_map(ai_next_action_json)
            .map(|action| ai_model_safe_runtime_value(&action))
            .collect::<Vec<_>>();
        let waiting_for_input = safe_data
            .get("waitingForInput")
            .and_then(serde_json::Value::as_bool);
        let data_is_internal_waiting_hint = safe_data
            .as_object()
            .is_some_and(|object| object.len() == 1 && object.contains_key("waitingForInput"));
        let mut envelope = serde_json::Map::new();
        if let Some(sources) = sources {
            envelope.insert("sources".into(), sources);
        }
        envelope.insert("ok".to_string(), serde_json::json!(result.ok));
        envelope.insert("summary".to_string(), serde_json::json!(safe_summary));
        envelope.insert("output".to_string(), serde_json::json!(output));
        // Keep compact interaction facts available when model formatting drops
        // the full UI-only data payload (including the terminal screen).
        for key in ["inputWaitReason", "tuiState"] {
            if let Some(value) = safe_data.get(key).and_then(serde_json::Value::as_str) {
                envelope.insert(key.to_string(), serde_json::json!(value));
            }
        }
        if let Some(observation) = safe_data.get("terminalObservation") {
            envelope.insert("terminalObservation".to_string(), observation.clone());
        }
        // Tauri omits `data` when an action did not provide it. Preserve that
        // shape so models do not learn data=null as a meaningful result.
        if !safe_data.is_null() && !data_is_internal_waiting_hint {
            envelope.insert("data".to_string(), safe_data);
        }
        if let Some(raw_output) = raw_output {
            envelope.insert("rawOutput".to_string(), serde_json::json!(raw_output));
        }
        envelope.insert("outputPreview".to_string(), output_preview);
        if truncated && !envelope.contains_key("rawOutput") {
            envelope.insert("warnings".to_string(), serde_json::json!([
                "Full output exceeded the UI retention limit; showing a head/tail preview. Use a narrower command such as grep, tail -n, or find ... | head for exact data."
            ]));
        }
        if let Some(message) = safe_error.as_ref() {
            envelope.insert(
                "error".to_string(),
                serde_json::json!({
                    "code": result.error_code.clone().unwrap_or_else(|| "tool_error".to_string()),
                    "message": message,
                    "recoverable": true,
                }),
            );
            envelope.insert("recoverable".to_string(), serde_json::json!(true));
        }
        if !targets.is_empty() {
            envelope.insert("targets".to_string(), serde_json::json!(targets));
        }
        if !next_actions.is_empty() {
            envelope.insert("nextActions".to_string(), serde_json::json!(next_actions));
        }
        if !result.observations.is_empty() {
            envelope.insert(
                "observations".to_string(),
                serde_json::json!(
                    result
                        .observations
                        .iter()
                        .map(|observation| ai_model_safe_runtime_text(observation))
                        .collect::<Vec<_>>()
                ),
            );
        }
        if let Some(waiting_for_input) = waiting_for_input {
            envelope.insert(
                "waitingForInput".to_string(),
                serde_json::json!(waiting_for_input),
            );
        }
        let mut meta = serde_json::Map::new();
        meta.insert("toolName".to_string(), serde_json::json!(tool_name));
        meta.insert("durationMs".to_string(), serde_json::json!(duration_ms));
        meta.insert(
            "verified".to_string(),
            serde_json::json!(result.verified.unwrap_or_else(|| {
                ai_tool_verified_default(result.ok, result.error_message.as_deref())
            })),
        );
        if let Some(capability) = risk_to_capability(result.risk) {
            meta.insert("capability".to_string(), serde_json::json!(capability));
        }
        if let Some(target) = result.target.as_ref() {
            if let Some(handle) = self.runtime_handles.get(&target.id) {
                meta.insert("handleId".to_string(), serde_json::json!(handle.handle_id));
            }
        }
        meta.insert("truncated".to_string(), serde_json::json!(truncated));
        meta.insert(
            "dataTruncated".to_string(),
            serde_json::json!(data_truncated),
        );
        envelope.insert("meta".to_string(), serde_json::Value::Object(meta));
        if !result.images.is_empty() {
            envelope.insert(
                "imageIds".into(),
                serde_json::json!(
                    result
                        .images
                        .iter()
                        .map(|image| &image.id)
                        .collect::<Vec<_>>()
                ),
            );
        }
        let envelope = serde_json::Value::Object(envelope);
        AiExecutedToolResult {
            images: result.images,
            tool_call_id,
            tool_name,
            success: result.ok,
            output,
            error: safe_error,
            duration_ms,
            envelope,
        }
    }
}

const AI_INTERNAL_RUNTIME_FIELD_NAMES: &[&str] = &[
    "targetid",
    "nodeid",
    "sessionid",
    "tabid",
    "paneid",
    "runtimeepoch",
    "registryepoch",
    "ownerkey",
    "ownergeneration",
    "refs",
];
const AI_MODEL_RESULT_DATA_MAX_CHARS: usize = 24_000;
const AI_MODEL_RESULT_DATA_MAX_NODES: usize = 512;
const AI_MODEL_RESULT_DATA_MAX_DEPTH: usize = 16;
const AI_MODEL_RESULT_STRING_MAX_CHARS: usize = 12_000;

pub fn ai_model_safe_runtime_value(value: &serde_json::Value) -> serde_json::Value {
    ai_model_safe_runtime_value_with_limits(value).0
}

fn ai_model_safe_runtime_value_with_limits(value: &serde_json::Value) -> (serde_json::Value, bool) {
    let mut remaining_chars = AI_MODEL_RESULT_DATA_MAX_CHARS;
    let mut remaining_nodes = AI_MODEL_RESULT_DATA_MAX_NODES;
    project_ai_model_runtime_value(
        value,
        AI_MODEL_RESULT_DATA_MAX_DEPTH,
        &mut remaining_chars,
        &mut remaining_nodes,
    )
}

/// Applies identifier redaction and one aggregate size budget before structured
/// tool results cross into the model-visible envelope.
fn project_ai_model_runtime_value(
    value: &serde_json::Value,
    remaining_depth: usize,
    remaining_chars: &mut usize,
    remaining_nodes: &mut usize,
) -> (serde_json::Value, bool) {
    if remaining_depth == 0 || *remaining_nodes == 0 {
        return (serde_json::Value::Null, true);
    }
    *remaining_nodes -= 1;
    match value {
        serde_json::Value::Object(object) => {
            let mut projected = serde_json::Map::new();
            let mut truncated = false;
            for (key, value) in object {
                if ai_internal_runtime_field_name(key) {
                    continue;
                }
                if *remaining_nodes == 0 || key.chars().count() > *remaining_chars {
                    truncated = true;
                    break;
                }
                *remaining_chars = remaining_chars.saturating_sub(key.chars().count());
                let (value, child_truncated) = project_ai_model_runtime_value(
                    value,
                    remaining_depth - 1,
                    remaining_chars,
                    remaining_nodes,
                );
                projected.insert(key.clone(), value);
                truncated |= child_truncated;
            }
            (serde_json::Value::Object(projected), truncated)
        }
        serde_json::Value::Array(values) => {
            let mut projected = Vec::new();
            let mut truncated = false;
            for value in values {
                if *remaining_nodes == 0 {
                    truncated = true;
                    break;
                }
                let (value, child_truncated) = project_ai_model_runtime_value(
                    value,
                    remaining_depth - 1,
                    remaining_chars,
                    remaining_nodes,
                );
                projected.push(value);
                truncated |= child_truncated;
            }
            (serde_json::Value::Array(projected), truncated)
        }
        serde_json::Value::String(value) => {
            let sanitized = ai_model_safe_runtime_text(value);
            let char_count = sanitized.chars().count();
            let retained_chars = char_count
                .min(AI_MODEL_RESULT_STRING_MAX_CHARS)
                .min(*remaining_chars);
            let projected = sanitized.chars().take(retained_chars).collect::<String>();
            *remaining_chars = remaining_chars.saturating_sub(retained_chars);
            (
                serde_json::Value::String(projected),
                retained_chars < char_count,
            )
        }
        other => (other.clone(), false),
    }
}

pub fn ai_model_safe_runtime_text(value: &str) -> String {
    let mut sanitized = crate::sanitize_for_ai(value);
    for prefix in [
        "saved-connection:",
        "ssh-node:",
        "terminal-session:",
        "sftp-session:",
        "ide-workspace:",
        "app-surface:",
        "local-shell:",
        "settings:",
        "rag-index:",
    ] {
        sanitized = redact_runtime_target_prefix(&sanitized, prefix);
    }
    sanitized
}

fn ai_internal_runtime_field_name(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    AI_INTERNAL_RUNTIME_FIELD_NAMES.contains(&normalized.as_str())
}

fn redact_runtime_target_prefix(value: &str, prefix: &str) -> String {
    let mut redacted = String::with_capacity(value.len());
    let mut remainder = value;
    while let Some(index) = remainder.find(prefix) {
        let (before, matched) = remainder.split_at(index);
        redacted.push_str(before);
        let suffix = &matched[prefix.len()..];
        let identifier_length = suffix
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
            .map(char::len_utf8)
            .sum::<usize>();
        if identifier_length == 0 {
            redacted.push_str(prefix);
            remainder = suffix;
            continue;
        }
        redacted.push_str("[runtime target]");
        remainder = &suffix[identifier_length..];
    }
    redacted.push_str(remainder);
    redacted
}

impl AiOrchestratorRuntimeSnapshot {
    /// Produces the model-facing form of a target without exposing a terminal's
    /// reusable session id or tab-local location.
    pub fn model_target_json(&self, target: &AiOrchestratorTarget) -> Option<serde_json::Value> {
        if let Some(resource_ref) = ai_stable_resource_ref_for_target(target) {
            return Some(serde_json::json!({
                "authority": {
                    "kind": "stable_resource",
                    "resource_ref": resource_ref,
                },
                "kind": target.kind,
                "label": ai_model_safe_runtime_text(&target.label),
                "state": target.state,
                "capabilities": target.capabilities,
            }));
        }
        if !matches!(
            target.kind.as_str(),
            "terminal-session"
                | "local-shell"
                | "ssh-node"
                | "sftp-session"
                | "ide-workspace"
                | "app-surface"
        ) {
            // Raw node identifiers are not model authorities. Live targets
            // appear only when their real owner adapters issued a handle.
            return None;
        }
        let handle = self.runtime_handles.get(&target.id)?;
        Some(serde_json::json!({
            "authority": {
                "kind": "runtime_handle",
                "handle_id": handle.handle_id,
            },
            "kind": target.kind,
            "label": ai_model_safe_runtime_text(&target.label),
            "state": target.state,
            "capabilities": handle.capabilities,
        }))
    }

    fn model_tool_result_target_json(
        &self,
        target: &AiOrchestratorTarget,
    ) -> Option<serde_json::Value> {
        if let Some(resource_ref) = ai_stable_resource_ref_for_target(target) {
            return Some(serde_json::json!({
                "authority": {
                    "kind": "stable_resource",
                    "resource_ref": resource_ref,
                },
                "kind": target.kind,
                "label": ai_model_safe_runtime_text(&target.label),
                "capabilities": target.capabilities,
            }));
        }
        if !matches!(
            target.kind.as_str(),
            "terminal-session"
                | "local-shell"
                | "ssh-node"
                | "sftp-session"
                | "ide-workspace"
                | "app-surface"
        ) {
            return None;
        }
        let handle = self.runtime_handles.get(&target.id)?;
        Some(serde_json::json!({
            "authority": {
                "kind": "runtime_handle",
                "handle_id": handle.handle_id,
            },
            "kind": target.kind,
            "label": ai_model_safe_runtime_text(&target.label),
            "capabilities": handle.capabilities,
        }))
    }
}

pub fn ai_stable_resource_ref_for_target(
    target: &AiOrchestratorTarget,
) -> Option<crate::StableResourceRef> {
    let (kind, id) = match target.kind.as_str() {
        "saved-connection" => (
            crate::StableResourceKind::SavedConnection,
            target.refs.get("connectionId")?.clone(),
        ),
        "settings" => (crate::StableResourceKind::SettingsScope, "app".to_string()),
        "rag-index" => (crate::StableResourceKind::RagIndex, "default".to_string()),
        _ => return None,
    };
    crate::StableResourceRef::new(kind, id, Some(ai_model_safe_runtime_text(&target.label))).ok()
}

pub fn make_ai_state_version(scope: &str, parts: impl IntoIterator<Item = String>) -> String {
    std::iter::once(scope.to_string())
        .chain(parts.into_iter().map(|part| {
            if part.is_empty() {
                "none".to_string()
            } else {
                part
            }
        }))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_observation::*;
    use crate::tool_result_protocol::{
        ai_tool_result_message, annotate_executed_ai_tool_result_policy,
    };

    #[test]
    fn web_search_sources_survive_model_output_truncation() {
        let snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        let sources = serde_json::json!([
            {"title":"Rust releases", "url":"https://blog.rust-lang.org/releases/?page=2#stable"},
            {"title":"Rust installation", "url":"https://www.rust-lang.org/tools/install"}
        ]);
        let result = snapshot.to_executed_tool_result(
            "search-1".into(),
            "search_web".into(),
            snapshot.ok(
                "Web request completed.",
                "article text ".repeat(2000),
                serde_json::json!({"sources":sources,"truncated":false}),
                "read",
            ),
            0,
        );
        assert_eq!(result.envelope["sources"], sources);
        let model: serde_json::Value =
            serde_json::from_str(&crate::ai_tool_result_model_content(&result)).unwrap();
        assert_eq!(model["sources"], sources);
        assert_eq!(model["truncated"], true);
    }

    #[test]
    fn terminal_input_state_reaches_model_without_false_secret_waits() {
        let snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        for (buffer, known_secret, alternate, expected_reason, expected_tui) in [
            (
                "[sudo] password for deploy:",
                false,
                false,
                Some("possible_credential_prompt"),
                "prompt",
            ),
            (
                "Enter passphrase for key '/tmp/key':",
                false,
                false,
                Some("possible_credential_prompt"),
                "prompt",
            ),
            (
                "请输入验证码：",
                false,
                false,
                Some("possible_credential_prompt"),
                "prompt",
            ),
            ("sudo: command not found", false, false, None, "shell"),
            ("password updated successfully", false, false, None, "shell"),
            (
                "Password:\nSelect an item and press Enter",
                false,
                true,
                Some("interactive_prompt"),
                "alternate_screen",
            ),
            (
                "Protected input",
                true,
                true,
                Some("credential_prompt"),
                "alternate_screen",
            ),
            (
                "Proceed ([y]/n)?",
                false,
                false,
                Some("interactive_prompt"),
                "shell",
            ),
            (
                "Proceed [y/n]:",
                false,
                false,
                Some("interactive_prompt"),
                "shell",
            ),
            (
                "Select an item and press Enter",
                false,
                false,
                Some("interactive_prompt"),
                "shell",
            ),
        ] {
            let reason = ai_terminal_input_wait_reason(buffer, known_secret);
            let screen = serde_json::json!({"isAlternateBuffer": alternate});
            let result = snapshot.to_executed_tool_result(
                "observe-1".into(),
                "observe_terminal".into(),
                snapshot.ok(
                    "Terminal observed.",
                    buffer,
                    serde_json::json!({
                        "waitingForInput": reason.is_some(),
                        "inputWaitReason": reason,
                        "tuiState": ai_terminal_tui_state(Some(&screen), buffer),
                    }),
                    "read",
                ),
                0,
            );
            let content = crate::ai_tool_result_model_content(&result);
            let model: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(
                model["waitingForInput"],
                expected_reason.is_some(),
                "{buffer}"
            );
            assert_eq!(
                model
                    .get("inputWaitReason")
                    .and_then(serde_json::Value::as_str),
                expected_reason,
                "{buffer}"
            );
            assert_eq!(model["tuiState"], expected_tui, "{buffer}");
        }
    }

    fn sample_result() -> AiExecutedToolResult {
        AiExecutedToolResult {
            images: Vec::new(),
            tool_call_id: "tool-1".to_string(),
            tool_name: "run_command".to_string(),
            success: true,
            output: "ok".to_string(),
            error: None,
            duration_ms: 7,
            envelope: serde_json::json!({
                "ok": true,
                "summary": "ok",
                "output": "ok",
                "meta": {
                    "toolName": "run_command",
                    "durationMs": 7,
                },
            }),
        }
    }

    #[test]
    fn tool_images_remain_attachments_when_building_model_messages() {
        let image = crate::AiImageAttachment::new(
            "screen.png".into(),
            "image/png".into(),
            32,
            32,
            b"pixel",
            b"thumb",
        )
        .unwrap();
        let mut result = sample_result();
        result.images.push(image.clone());
        let message = ai_tool_result_message(result, 0);
        assert_eq!(message.tool_call_id.as_deref(), Some("tool-1"));
        assert_eq!(crate::message_images(&message), &[image]);
        assert!(!message.content.contains("cGl4ZWw="));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&message.content).unwrap()["output"],
            "ok"
        );
    }

    fn sample_target() -> AiOrchestratorTarget {
        let mut refs = std::collections::BTreeMap::new();
        refs.insert("nodeId".to_string(), "prod-node-1".to_string());
        refs.insert(
            "connectionId".to_string(),
            "4cb736c8-579f-40de-9f71-4efe2c90e7ef".to_string(),
        );
        AiOrchestratorTarget {
            id: "ssh-node:prod-node-1".to_string(),
            kind: "ssh-node".to_string(),
            label: "prod.example.com".to_string(),
            state: "connected".to_string(),
            capabilities: vec!["filesystem.read".to_string()],
            refs,
            metadata: serde_json::json!({
                "host": "prod.example.com",
                "username": "deploy",
            }),
            terminal_buffer: None,
            terminal_screen: None,
        }
    }

    #[test]
    fn target_query_uses_only_safe_ranking_fields() {
        let target = sample_target();

        assert!(target_matches_ai_query(&target, "prod.example.com"));
        assert!(target_matches_ai_query(
            &target,
            "4cb736c8-579f-40de-9f71-4efe2c90e7ef"
        ));
        assert!(!target_matches_ai_query(&target, "4cb736c8"));
        assert!(!target_matches_ai_query(&target, "prod-node-1"));
        assert!(!target_matches_ai_query(&target, "deploy"));
        assert!(!target_matches_ai_query(&target, "staging"));
    }

    #[test]
    fn duplicate_labels_remain_explicitly_ambiguous() {
        let first = sample_target();
        let mut second = sample_target();
        second.id = "ssh-node:second-internal-owner".to_string();
        second
            .refs
            .insert("nodeId".to_string(), "second-internal-owner".to_string());
        let mut snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        snapshot.targets = vec![first, second];

        let result = snapshot.select_target(&serde_json::json!({
            "query": "prod.example.com",
            "intent": "connection",
            "kind": "ssh-node",
        }));

        assert!(!result.ok);
        assert_eq!(
            result.error_code.as_deref(),
            Some("target_disambiguation_required")
        );
        assert_eq!(result.targets.len(), 2);
    }

    #[test]
    fn target_discovery_applies_a_hard_result_limit() {
        let mut snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        snapshot.targets = (0..=AI_TARGET_DISCOVERY_LIMIT)
            .map(|index| {
                let mut target = sample_target();
                target.id = format!("ssh-node:internal-{index}");
                target.label = format!("host-{index}.example.com");
                target
            })
            .collect();

        let result = snapshot.list_targets(&serde_json::json!({
            "view": "connections",
            "kind": "ssh-node",
        }));

        assert!(result.ok);
        assert_eq!(result.targets.len(), AI_TARGET_DISCOVERY_LIMIT);
    }

    #[test]
    fn long_tool_output_uses_head_tail_preview_metadata() {
        let output = format!(
            "HEAD{}OMITTED{}TAIL",
            "a".repeat(15_000),
            "b".repeat(15_000)
        );

        let (preview, raw_output, output_preview, truncated) = prepare_ai_tool_output(&output);

        assert!(truncated);
        assert_eq!(raw_output.as_deref(), Some(output.as_str()));
        assert!(preview.starts_with("HEAD"));
        assert!(preview.ends_with("TAIL"));
        assert!(!preview.contains("OMITTED"));
        assert!(preview.contains("showing head and tail"));
        assert_eq!(
            output_preview.get("strategy"),
            Some(&serde_json::json!("head_tail"))
        );
        assert_eq!(
            output_preview.get("rawOutputStored"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn run_command_execution_summary_preserves_visibility_and_state() {
        let mut result = sample_result();
        result.envelope = serde_json::json!({
            "ok": true,
            "summary": "Command sent to terminal.",
            "output": "Command sent: uptime",
            "data": {
                "executionState": "sent",
                "commandId": "command-1",
                "visibleInTerminal": true
            },
            "targets": [{
                "id": "ssh-node:prod-node-1",
                "kind": "ssh-node",
                "label": "prod.example.com",
                "metadata": { "state": "connected", "refs": { "sessionId": "42" } }
            }],
            "meta": { "toolName": "run_command", "durationMs": 7, "truncated": false }
        });

        annotate_ai_run_command_execution_result(
            &mut result,
            &serde_json::json!({ "command": "uptime" }),
        );

        let model: serde_json::Value =
            serde_json::from_str(&crate::ai_tool_result_model_content(&result)).unwrap();
        assert_eq!(model["execution"]["commandId"], "command-1");
        assert_eq!(
            result.envelope.pointer("/execution/state"),
            Some(&serde_json::json!("sent"))
        );
        assert_eq!(
            result.envelope.pointer("/execution/visibleInTerminal"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn model_result_projection_removes_internal_runtime_identifiers() {
        let value = serde_json::json!({
            "nodeId": "node-1",
            "session_id": "42",
            "runtimeEpoch": "epoch-1",
            "target": "terminal-session:42",
            "refs": { "tabId": "tab-1" },
            "stable": { "kind": "saved_connection", "id": "4e22e673-067e-46e2-8b9f-902d7b21af4c" },
        });
        let projected = ai_model_safe_runtime_value(&value);

        assert!(projected.get("nodeId").is_none());
        assert!(projected.get("session_id").is_none());
        assert!(projected.get("runtimeEpoch").is_none());
        assert!(projected.get("refs").is_none());
        assert!(!projected.to_string().contains("terminal-session:42"));
        assert_eq!(
            projected.pointer("/stable/id"),
            Some(&serde_json::json!("4e22e673-067e-46e2-8b9f-902d7b21af4c"))
        );
    }

    #[test]
    fn model_result_projection_bounds_structured_data() {
        let value = serde_json::json!({
            "content": "x".repeat(AI_MODEL_RESULT_STRING_MAX_CHARS + 1),
            "items": vec![serde_json::Value::Null; AI_MODEL_RESULT_DATA_MAX_NODES],
        });

        let (projected, truncated) = ai_model_safe_runtime_value_with_limits(&value);

        assert!(truncated);
        assert!(
            projected["content"]
                .as_str()
                .is_some_and(|content| content.chars().count() <= AI_MODEL_RESULT_STRING_MAX_CHARS)
        );
        assert!(
            projected["items"]
                .as_array()
                .is_some_and(|items| items.len() < AI_MODEL_RESULT_DATA_MAX_NODES)
        );
    }

    #[test]
    fn active_context_matches_tab_session_or_node_refs() {
        let mut target = sample_target();
        target.refs.insert("tabId".to_string(), "7".to_string());
        target
            .refs
            .insert("sessionId".to_string(), "42".to_string());

        assert!(target_matches_active_context(
            &target,
            Some("7"),
            None,
            None
        ));
        assert!(target_matches_active_context(
            &target,
            None,
            Some("prod-node-1"),
            None
        ));
        assert!(target_matches_active_context(
            &target,
            None,
            None,
            Some("42")
        ));
        assert!(!target_matches_active_context(
            &target,
            Some("8"),
            Some("staging-node"),
            Some("43")
        ));
    }

    #[test]
    fn default_policy_annotation_does_not_mark_bypass() {
        let mut result = sample_result();
        let decision = crate::AiPolicyDecision {
            decision: crate::AiPolicyDecisionKind::Allow,
            risk: crate::AiActionRisk::Read,
            reason_code: "read_only_auto_allowed".to_string(),
            reason_text_key: "ai.tool_use.policy_reason_read_only".to_string(),
            matched_policy_key: "list_targets".to_string(),
            approval_mode: crate::AiPolicySafetyMode::Default,
            profile_id: None,
        };

        annotate_executed_ai_tool_result_policy(&mut result, &decision);

        assert!(result.envelope.pointer("/meta/approvalMode").is_none());
        assert_eq!(
            result.envelope.pointer("/meta/policyDecision/approvalMode"),
            Some(&serde_json::json!("default"))
        );
        assert!(
            result
                .envelope
                .pointer("/meta/policyDecision/profileId")
                .is_none()
        );
    }

    #[test]
    fn unconfirmed_observations_reach_the_model_without_claiming_success_or_freeing_input() {
        let snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        for (completed, prompt, reason, expired, expected_state, can_run, exit) in [
            (false, false, None, true, "output_captured", false, None),
            (false, true, None, false, "shell_ready", true, None),
            (
                false,
                false,
                Some("interactive_prompt"),
                false,
                "waiting_input",
                false,
                None,
            ),
            (true, false, None, false, "completed", true, Some(7)),
        ] {
            let mut result = snapshot.to_executed_tool_result(
                "call".into(),
                "run_command".into(),
                snapshot.ok(
                    "Terminal observation",
                    "progress",
                    ai_terminal_observation_data(
                        Some("command-1"),
                        completed,
                        prompt,
                        reason,
                        Some(7),
                        expired,
                    ),
                    "read",
                ),
                0,
            );
            annotate_ai_run_command_execution_result(
                &mut result,
                &serde_json::json!({"command":"install"}),
            );
            let model: serde_json::Value =
                serde_json::from_str(&crate::ai_tool_result_model_content(&result)).unwrap();
            assert_eq!(
                model["terminalObservation"],
                serde_json::json!({
                    "state": expected_state, "commandId":"command-1", "completionConfirmed":completed,
                    "canRunCommand":can_run, "exitCode":exit, "waitExpired":expired,
                })
            );
            assert_eq!(model["execution"]["exitCode"], serde_json::json!(exit));
            assert_eq!(model["waitingForInput"], reason.is_some());
        }
    }
}
