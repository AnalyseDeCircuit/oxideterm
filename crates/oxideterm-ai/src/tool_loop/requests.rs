pub async fn execute_ai_tool(
    services: &AiModelBackendServices,
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
    tool_call_id: String,
    tool_name: String,
    args: serde_json::Value,
    post_user_approval: bool,
    dangerous_command_approved: bool,
    mut execution: Option<&mut AgentExecution>,
    dispatch: Option<crate::agent::AgentDispatch>,
) -> AiExecutedToolResult {
    if dispatch
        .as_ref()
        .is_some_and(|guard| guard.check().is_err())
    {
        return rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "agent_direction_changed",
            "Task direction changed before dispatch.",
        );
    }
    let mut leases = Vec::new();
    if let Some(agent) = execution.as_mut() {
        if !agent
            .runtime
            .snapshot(&agent.run)
            .is_ok_and(|snapshot| snapshot.scope.tools.contains(&tool_name))
        {
            return rejected_ai_tool_result(
                tool_call_id,
                tool_name,
                "agent_scope_denied",
                "This tool is outside the delegated scope.",
            );
        }
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if send_ai_stream_delivery(
            ui_tx,
            generation,
            conversation_id,
            assistant_id,
            AiStreamDeliveryEvent::ToolResourcesRequested {
                tool_session_id: tool_session_id.clone(),
                name: tool_name.clone(),
                args: args.clone(),
                sender,
            },
        )
        .is_err()
        {
            return rejected_ai_tool_result(
                tool_call_id,
                tool_name,
                "operation_cancelled",
                "Tool host is unavailable.",
            );
        }
        let keys = match receiver.await {
            Ok(Ok(keys)) => keys,
            _ => {
                return rejected_ai_tool_result(
                    tool_call_id,
                    tool_name,
                    "agent_scope_denied",
                    "The target is unavailable or outside this run's scope.",
                );
            }
        };
        for key in keys {
            let resources = agent.resources.clone();
            if tool_name == "send_terminal_input" {
                if let Some(lease) = resources.owned_by(&key, &agent.run) {
                    leases.push(crate::agent::AgentToolLease::borrow_command(
                        resources, lease,
                    ));
                    continue;
                }
            }
            let cancellation = match agent.runtime.cancellation(&agent.run) {
                Ok(value) => value,
                Err(_) => {
                    return rejected_ai_tool_result(
                        tool_call_id,
                        tool_name,
                        "operation_cancelled",
                        "Agent run was cancelled.",
                    );
                }
            };
            let acquire = resources.acquire(key, agent.run.clone(), cancellation);
            match ai_pending_dispatch(
                dispatch.as_ref(),
                agent.wait(AgentState::AwaitingResource, acquire),
            )
            .await
            {
                Ok(Ok(Ok(lease))) => {
                    leases.push(crate::agent::AgentToolLease::new(resources.clone(), lease))
                }
                Err(_) => {
                    return rejected_ai_tool_result(
                        tool_call_id,
                        tool_name,
                        "agent_direction_changed",
                        "Task direction changed before dispatch.",
                    );
                }
                Ok(Ok(Err(crate::agent::AgentError::ResourceUnresolved))) => {
                    return rejected_ai_tool_result(
                        tool_call_id,
                        tool_name,
                        "resource_execution_unresolved",
                        "The terminal changed while this request was queued. Rediscover its current state before submitting a new request.",
                    );
                }
                _ => {
                    return rejected_ai_tool_result(
                        tool_call_id,
                        tool_name,
                        "operation_cancelled",
                        "Agent run ended while waiting for the resource.",
                    );
                }
            }
        }
    }
    match ai_pending_dispatch(
        dispatch.as_ref(),
        await_ai_history_commit(ui_tx, generation, conversation_id, assistant_id),
    )
    .await
    {
        Ok(true) => {}
        _ => {
            return rejected_ai_tool_result(
                tool_call_id,
                tool_name,
                "operation_cancelled",
                "The task stopped or changed direction before its execution record was saved.",
            );
        }
    }
    let _response = crate::agent::AgentToolResponse(leases.clone());
    if !ai_tool_requires_ui_thread(&tool_name, &args) {
        for lease in &leases {
            lease.dispatched();
        }
    }
    let cancelled_call = (tool_call_id.clone(), tool_name.clone());
    let future = execute_ai_tool_uncoordinated(
        services,
        ui_tx,
        generation,
        tool_session_id,
        conversation_id,
        assistant_id,
        tool_call_id,
        tool_name,
        args,
        post_user_approval,
        dangerous_command_approved,
        leases.clone(),
        dispatch,
    );
    let result = if let Some(agent) = execution {
        agent.wait(AgentState::AwaitingCondition, future).await.unwrap_or_else(|_|
            rejected_ai_tool_result(cancelled_call.0, cancelled_call.1, "operation_cancelled", "The task was stopped; an already dispatched remote operation may still be running."))
    } else {
        future.await
    };
    for lease in leases {
        lease.finish_response(result.success);
    }
    if !result.images.is_empty() {
        let _ = send_ai_stream_delivery(
            ui_tx,
            generation,
            conversation_id,
            assistant_id,
            AiStreamDeliveryEvent::ToolImages(result.images.clone()),
        );
    }
    result
}

pub fn ai_tool_requires_ui_thread(tool_name: &str, _args: &serde_json::Value) -> bool {
    // Every application tool crosses the same UI-owned broker. Background
    // provider tasks therefore never execute against their frozen snapshots.
    crate::is_orchestrator_tool_name(tool_name)
}

async fn execute_backend_ai_tool(
    services: &AiModelBackendServices,
    tool_call_id: String,
    tool_name: String,
    args: serde_json::Value,
) -> AiExecutedToolResult {
    let started = std::time::Instant::now();
    let snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
    let result = match tool_name.as_str() {
        "list_mcp_resources" => list_mcp_resources(services, &snapshot).await,
        "read_mcp_resource" => read_mcp_resource(services, &snapshot, &args).await,
        name if crate::is_orchestrator_tool_name(name) => snapshot.fail(
            "Application tool requires the current runtime broker.",
            "runtime_broker_required",
            "The tool cannot run from a frozen background snapshot.",
            "read",
        ),
        _ if crate::is_mcp_tool_name(&tool_name) => {
            call_mcp_tool(services, &snapshot, &tool_name, args).await
        }
        _ => snapshot.fail(
            "Unknown orchestrator tool.",
            "unknown_tool",
            format!("{tool_name} is not an OxideSens task tool."),
            "read",
        ),
    };
    snapshot.to_executed_tool_result(
        tool_call_id,
        tool_name,
        result,
        started.elapsed().as_millis(),
    )
}

async fn list_mcp_resources(
    services: &AiModelBackendServices,
    snapshot: &AiOrchestratorRuntimeSnapshot,
) -> AiActionResultLite {
    let resources = services.ai_mcp_registry.resources().await;
    if resources.is_empty() {
        return snapshot.ok(
            "No MCP resources available.",
            "No MCP resources available. Either no MCP servers are connected, or none expose resources.",
            serde_json::json!([]),
            "read",
        );
    }
    let data = resources
        .iter()
        .map(|(resource, server_id, server_name)| {
            serde_json::json!({
                "serverId": server_id,
                "serverName": server_name,
                "uri": resource.uri,
                "name": resource.name,
                "description": resource.description,
                "mimeType": resource.mime_type,
            })
        })
        .collect::<Vec<_>>();
    let output = resources
        .iter()
        .map(|(resource, server_id, server_name)| {
            let mime = resource
                .mime_type
                .as_deref()
                .map(|mime| format!(" [{mime}]"))
                .unwrap_or_default();
            let description = resource
                .description
                .as_deref()
                .map(|description| format!(" — {description}"))
                .unwrap_or_default();
            format!(
                "[{server_name}] {} ({}){mime}{description}  server_id={server_id}",
                resource.name, resource.uri
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    snapshot.ok(
        format!(
            "Found {} MCP resource{}.",
            resources.len(),
            if resources.len() == 1 { "" } else { "s" }
        ),
        output,
        serde_json::Value::Array(data),
        "read",
    )
}

async fn read_mcp_resource(
    services: &AiModelBackendServices,
    snapshot: &AiOrchestratorRuntimeSnapshot,
    args: &serde_json::Value,
) -> AiActionResultLite {
    let server_id = args
        .get("server_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let uri = args
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if server_id.is_empty() || uri.is_empty() {
        return snapshot.fail(
            "MCP resource arguments are required.",
            "missing_mcp_resource_args",
            "Both server_id and uri are required.",
            "read",
        );
    }
    match services.ai_mcp_registry.read_resource(server_id, uri).await {
        Ok(content) => {
            let (output, truncated) = crate::mcp_resource_output(&content);
            snapshot
                .ok(
                    format!("Read MCP resource {uri}."),
                    output,
                    serde_json::json!(content),
                    "read",
                )
                .with_verified(!truncated)
        }
        Err(error) => {
            let mut result = snapshot.fail(
                "MCP resource read failed.",
                "mcp_resource_read_failed",
                error.to_string(),
                "read",
            );
            result.data = serde_json::json!({"recovery":error.recovery()});
            result
        }
    }
}

async fn call_mcp_tool(
    services: &AiModelBackendServices,
    snapshot: &AiOrchestratorRuntimeSnapshot,
    tool_name: &str,
    args: serde_json::Value,
) -> AiActionResultLite {
    match services
        .ai_mcp_registry
        .call_prefixed_tool(tool_name, args)
        .await
    {
        Ok(mut result) => {
            let prepared = tokio::task::spawn_blocking(move || {
                let images = result.take_images()?;
                Ok::<_, anyhow::Error>((result, images))
            })
            .await;
            let (result, images) = match prepared {
                Ok(Ok(value)) => value,
                _ => {
                    return snapshot.fail(
                        "Could not read tool images.",
                        "ai_image_tool_invalid",
                        "The tool returned an invalid, unsupported, or oversized image.",
                        "read",
                    );
                }
            };
            let (success, output, truncated) = crate::mcp_tool_output(&result);
            if success {
                let mut action = snapshot
                    .ok(
                        format!("Executed MCP tool {tool_name}."),
                        output,
                        serde_json::json!(result),
                        "write",
                    )
                    .with_verified(!truncated);
                action.images = images;
                action
            } else {
                let mut action = snapshot.fail(
                    "MCP tool returned an error.",
                    "mcp_tool_error",
                    if output.is_empty() {
                        "MCP tool returned an error with no message.".to_string()
                    } else {
                        output
                    },
                    "write",
                );
                action.images = images;
                action
            }
        }
        Err(error) => snapshot.fail(
            "MCP tool execution failed.",
            "mcp_tool_execution_failed",
            error.to_string(),
            "write",
        ),
    }
}
async fn execute_ai_tool_uncoordinated(
    services: &AiModelBackendServices,
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
    tool_call_id: String,
    tool_name: String,
    args: serde_json::Value,
    post_user_approval: bool,
    dangerous_command_approved: bool,
    leases: Vec<crate::agent::AgentToolLease>,
    dispatch: Option<crate::agent::AgentDispatch>,
) -> AiExecutedToolResult {
    if dispatch
        .as_ref()
        .is_some_and(|guard| guard.check().is_err())
    {
        return rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "agent_direction_changed",
            "Task direction changed before dispatch.",
        );
    }
    if ai_rejects_legacy_live_target_argument(&tool_name, &args) {
        return rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "runtime_capability_unavailable",
            "This live resource must be rediscovered through the current v2 runtime context.",
        );
    }
    if ai_tool_requires_ui_thread(&tool_name, &args) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if send_ai_stream_delivery(
            ui_tx,
            generation,
            conversation_id,
            assistant_id,
            AiStreamDeliveryEvent::ToolExecutionRequested {
                audit_context: oxideterm_audit::AuditContext::current_request(),
                dispatch,
                leases,
                tool_session_id: tool_session_id.clone(),
                tool_call_id: tool_call_id.clone(),
                name: tool_name.clone(),
                args,
                post_user_approval,
                dangerous_command_approved,
                sender,
            },
        )
        .is_err()
        {
            return rejected_ai_tool_result(
                tool_call_id,
                tool_name,
                "ui_delivery_failed",
                "The native UI executor is no longer available.",
            );
        }
        return receiver.await.unwrap_or_else(|_| {
            rejected_ai_tool_result(
                tool_call_id,
                tool_name,
                "ui_executor_cancelled",
                "The native UI executor cancelled the tool call.",
            )
        });
    }

    execute_backend_ai_tool(services, tool_call_id, tool_name, args).await
}

#[allow(clippy::too_many_arguments)]
pub async fn preflight_ai_tool(
    ui_tx: &AiStreamDeliverySender,
    generation: u64,
    tool_session_id: &ToolSessionId,
    conversation_id: &str,
    assistant_id: &str,
    tool_call_id: String,
    tool_name: String,
    args: serde_json::Value,
) -> Option<AiExecutedToolResult> {
    if ai_rejects_legacy_live_target_argument(&tool_name, &args) {
        return Some(rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "runtime_capability_unavailable",
            "This live resource must be rediscovered through the current v2 runtime context.",
        ));
    }
    if !ai_tool_requires_ui_thread(&tool_name, &args) {
        return None;
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    if send_ai_stream_delivery(
        ui_tx,
        generation,
        conversation_id,
        assistant_id,
        AiStreamDeliveryEvent::ToolPreflightRequested {
            tool_session_id: tool_session_id.clone(),
            tool_call_id: tool_call_id.clone(),
            name: tool_name.clone(),
            args,
            sender,
        },
    )
    .is_err()
    {
        return Some(rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "ui_delivery_failed",
            "The native UI executor is no longer available.",
        ));
    }
    receiver.await.unwrap_or_else(|_| {
        Some(rejected_ai_tool_result(
            tool_call_id,
            tool_name,
            "ui_executor_cancelled",
            "The native UI executor cancelled the tool preflight.",
        ))
    })
}
