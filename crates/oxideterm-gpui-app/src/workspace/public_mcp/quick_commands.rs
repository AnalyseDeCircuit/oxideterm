use std::collections::HashSet;

use gpui::Context;
use oxideterm_public_mcp::{
    ClientRef, DomainRequest, PreparedQuickCommandRunArgs, PublicQuickCommandConfirmationPolicy,
    PublicQuickCommandParameter, PublicQuickCommandParameterKind, PublicQuickCommandTargetProtocol,
    PublicToolCall, QuickCommandRef, ToolEnvelope, ToolGroup,
};
use oxideterm_quick_commands::{
    QuickCommand, QuickCommandConfirmationPolicy, QuickCommandContextValues, QuickCommandDraft,
    QuickCommandParameter, QuickCommandParameterKind, QuickCommandRisk, QuickCommandTargetContext,
    QuickCommandTargetProtocol, QuickCommandsSnapshot, classify_command_risk, delete_quick_command,
    load_snapshot, new_quick_command_id, now_ms, prepare_quick_command, save_snapshot,
    upsert_quick_command,
};
use serde::Serialize;
use serde_json::json;
use zeroize::Zeroizing;

use super::{WorkspaceApp, finish_serialized, node_lease_for_client};

#[derive(Serialize)]
struct QuickCommandSummary {
    quickcommand_ref: QuickCommandRef,
    name: String,
    category: String,
    description: Option<String>,
    host_pattern: Option<String>,
    host_patterns: Vec<String>,
    protocols: Vec<QuickCommandTargetProtocol>,
    parameter_count: usize,
    confirmation: QuickCommandConfirmationPolicy,
    risk: Option<&'static str>,
    updated_at: u64,
}

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_quick_commands_list(&mut self, request: DomainRequest) {
        let PublicToolCall::QuickCommandsList(args) = &request.call else {
            return;
        };
        let snapshot = match load_snapshot(&self.public_mcp.settings_path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        self.public_mcp
            .sync_quick_command_refs(&request.client_ref, &snapshot);
        let query = args
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let commands = snapshot
            .commands
            .into_iter()
            .filter(|command| quick_command_matches_query(command, &query))
            .map(|command| {
                self.public_mcp
                    .quick_command_summary(&request.client_ref, command)
            })
            .collect::<Vec<_>>();
        finish_serialized(
            request,
            json!({ "revision": snapshot.updated_at, "commands": commands }),
        );
    }

    pub(super) fn handle_public_mcp_quick_commands_describe(&mut self, request: DomainRequest) {
        let PublicToolCall::QuickCommandsDescribe(args) = &request.call else {
            return;
        };
        let quickcommand_ref = args.quickcommand_ref.clone();
        let snapshot = match load_snapshot(&self.public_mcp.settings_path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        let Some(command_id) =
            self.public_mcp
                .quick_command_id(&request.client_ref, &quickcommand_ref, &snapshot)
        else {
            request.finish(ToolEnvelope::failed(
                "The Quick Command handle is unavailable",
            ));
            return;
        };
        let Some(command) = snapshot
            .commands
            .into_iter()
            .find(|command| command.id == command_id)
        else {
            request.finish(ToolEnvelope::failed(
                "The Quick Command handle is unavailable",
            ));
            return;
        };
        finish_serialized(
            request,
            json!({
                "quickcommand_ref": quickcommand_ref,
                "name": command.name,
                "command": command.command,
                "category": command.category,
                "description": command.description,
                "host_pattern": command.availability.host_patterns.first(),
                "host_patterns": command.availability.host_patterns,
                "protocols": command.availability.protocols,
                "parameters": command.parameters.into_iter().map(public_quick_command_parameter).collect::<Vec<_>>(),
                "confirmation": command.confirmation,
                "revision": snapshot.updated_at,
            }),
        );
    }

    pub(super) fn handle_public_mcp_quick_commands_save(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::QuickCommandsSave(args) = &request.call else {
            return;
        };
        let mut snapshot = match load_snapshot(&self.public_mcp.settings_path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if snapshot.updated_at != args.expected_revision {
            request.finish(ToolEnvelope::failed(
                "The Quick Commands store changed after the requested revision",
            ));
            return;
        }
        let command_id = match args.quickcommand_ref.as_ref() {
            Some(quickcommand_ref) => {
                let Some(command_id) = self.public_mcp.quick_command_id(
                    &request.client_ref,
                    quickcommand_ref,
                    &snapshot,
                ) else {
                    request.finish(ToolEnvelope::failed(
                        "The Quick Command handle is unavailable",
                    ));
                    return;
                };
                command_id
            }
            None => new_quick_command_id(),
        };
        // A single revision is shared by the record and the persisted snapshot.
        let new_revision = next_revision(snapshot.updated_at);
        let saved = upsert_quick_command(
            &mut snapshot.commands,
            &snapshot.categories,
            QuickCommandDraft {
                id: Some(command_id.clone()),
                name: args.name.clone(),
                command: args.command.to_string(),
                category: Some(args.category.clone()),
                description: args.description.clone(),
                parameters: args.parameters.as_ref().map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| QuickCommandParameter {
                            name: parameter.name.clone(),
                            label: parameter.label.clone(),
                            kind: match parameter.kind {
                                PublicQuickCommandParameterKind::Text => {
                                    QuickCommandParameterKind::Text
                                }
                                PublicQuickCommandParameterKind::Choice => {
                                    QuickCommandParameterKind::Choice
                                }
                                PublicQuickCommandParameterKind::Secret => {
                                    QuickCommandParameterKind::Secret
                                }
                            },
                            default_value: parameter.default_value.clone(),
                            choices: parameter.choices.clone(),
                            required: parameter.required,
                        })
                        .collect()
                }),
                protocols: args.protocols.as_ref().map(|protocols| {
                    protocols
                        .iter()
                        .copied()
                        .map(core_quick_command_protocol)
                        .collect()
                }),
                host_patterns: args.host_patterns.clone().or_else(|| {
                    args.host_pattern
                        .clone()
                        .map(|host_pattern| vec![host_pattern])
                }),
                confirmation: args.confirmation.map(|confirmation| match confirmation {
                    PublicQuickCommandConfirmationPolicy::Inherit => {
                        QuickCommandConfirmationPolicy::Inherit
                    }
                    PublicQuickCommandConfirmationPolicy::Always => {
                        QuickCommandConfirmationPolicy::Always
                    }
                }),
            },
            new_revision,
        );
        if !saved {
            request.finish(ToolEnvelope::failed(
                "The Quick Command definition is invalid",
            ));
            return;
        }
        snapshot.updated_at = new_revision;
        if let Err(error) = save_snapshot(&self.public_mcp.settings_path, &snapshot) {
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        let quickcommand_ref = self
            .public_mcp
            .quick_command_ref(&request.client_ref, &command_id);
        self.reload_quick_commands_surface(cx);
        // Quick Commands are a structured Cloud Sync section, so invalidate its local snapshot.
        self.queue_cloud_sync_dirty_refresh(cx);
        finish_serialized(
            request,
            json!({
                "quickcommand_ref": quickcommand_ref,
                "revision": snapshot.updated_at,
            }),
        );
    }

    pub(super) fn handle_public_mcp_quick_commands_remove(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::QuickCommandsRemove(args) = &request.call else {
            return;
        };
        let mut snapshot = match load_snapshot(&self.public_mcp.settings_path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if snapshot.updated_at != args.expected_revision {
            request.finish(ToolEnvelope::failed(
                "The Quick Commands store changed after the requested revision",
            ));
            return;
        }
        let Some(command_id) = self.public_mcp.quick_command_id(
            &request.client_ref,
            &args.quickcommand_ref,
            &snapshot,
        ) else {
            request.finish(ToolEnvelope::failed(
                "The Quick Command handle is unavailable",
            ));
            return;
        };
        if !delete_quick_command(&mut snapshot.commands, &command_id) {
            request.finish(ToolEnvelope::failed(
                "The Quick Command handle is unavailable",
            ));
            return;
        }
        snapshot.updated_at = next_revision(snapshot.updated_at);
        if let Err(error) = save_snapshot(&self.public_mcp.settings_path, &snapshot) {
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        self.public_mcp
            .remove_quick_command_ref(&request.client_ref, &args.quickcommand_ref);
        self.reload_quick_commands_surface(cx);
        // Keep Cloud Sync preview and dirty state aligned with the persisted command store.
        self.queue_cloud_sync_dirty_refresh(cx);
        finish_serialized(
            request,
            json!({ "removed": true, "revision": snapshot.updated_at }),
        );
    }

    pub(super) fn handle_public_mcp_quick_commands_run(&mut self, request: DomainRequest) {
        let PublicToolCall::QuickCommandsRun(args) = &request.call else {
            return;
        };
        let node_ref = args.node_ref.clone();
        let mut snapshot = match load_snapshot(&self.public_mcp.settings_path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if snapshot.updated_at != args.expected_revision {
            request.finish(ToolEnvelope::failed(
                "The Quick Commands store changed after the requested revision",
            ));
            return;
        }
        let Some(command_id) = self.public_mcp.quick_command_id(
            &request.client_ref,
            &args.quickcommand_ref,
            &snapshot,
        ) else {
            request.finish(ToolEnvelope::failed(
                "The Quick Command handle is unavailable",
            ));
            return;
        };
        let Some(index) = snapshot
            .commands
            .iter()
            .position(|command| command.id == command_id)
        else {
            request.finish(ToolEnvelope::failed(
                "The Quick Command handle is unavailable",
            ));
            return;
        };
        let Some(lease) = node_lease_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &node_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        };
        let Some(metadata) = self.node_router.node_metadata(&lease.node_id) else {
            request.finish(ToolEnvelope::failed("The SSH node is no longer ready"));
            return;
        };
        let command = snapshot.commands.swap_remove(index);
        let saved_connection = metadata
            .connection_id
            .as_deref()
            .and_then(|connection_id| self.connection_store.get(connection_id));
        let context = QuickCommandTargetContext {
            target_id: node_ref.to_string(),
            label: saved_connection
                .map(|connection| connection.name.clone())
                .unwrap_or_else(|| metadata.host.clone()),
            protocol: QuickCommandTargetProtocol::Ssh,
            values: QuickCommandContextValues {
                host: Some(Zeroizing::new(metadata.host.clone())),
                username: Some(Zeroizing::new(metadata.username.clone())),
                port: Some(metadata.port),
                connection: saved_connection
                    .map(|connection| Zeroizing::new(connection.name.clone())),
                group: saved_connection
                    .and_then(|connection| connection.group.clone())
                    .map(Zeroizing::new),
                ..QuickCommandContextValues::default()
            },
        };
        // The request retains zeroizing inputs only until expansion creates the owned command.
        let Ok(prepared) = prepare_quick_command(&command, &[context], &args.arguments) else {
            request.finish(ToolEnvelope::failed(
                "The Quick Command requires interactive parameters or unavailable context",
            ));
            return;
        };
        let confirmation_required = prepared.confirmation_required;
        let Some(prepared_target) = prepared.targets.into_iter().next() else {
            request.finish(ToolEnvelope::failed(
                "The Quick Command is not available for this node",
            ));
            return;
        };
        if request.requires_standard_approval() || confirmation_required {
            let prepared_call =
                PublicToolCall::PreparedQuickCommandRun(PreparedQuickCommandRunArgs {
                    quickcommand_ref: args.quickcommand_ref.clone(),
                    node_ref: node_ref.clone(),
                    command: prepared_target.command,
                });
            let approval = match self
                .public_mcp
                .state
                .approvals
                .stage(request.client_ref.clone(), prepared_call)
            {
                Ok(approval) => approval,
                Err(error) => {
                    request.finish(ToolEnvelope::failed(error.to_string()));
                    return;
                }
            };
            self.public_mcp.state.broker.notify_state_changed();
            let response = ToolEnvelope::accepted(json!({
                "outcome": "approval_required",
                "approval": approval,
            }))
            .unwrap_or_else(|error| ToolEnvelope::failed(error.to_string()));
            request.finish(response);
            return;
        }
        self.start_public_mcp_node_command(
            request,
            node_ref,
            prepared_target.command,
            ToolGroup::QuickCommandExecute,
        );
    }

    pub(super) fn handle_public_mcp_prepared_quick_command_run(&self, request: DomainRequest) {
        let PublicToolCall::PreparedQuickCommandRun(args) = &request.call else {
            return;
        };
        let node_ref = args.node_ref.clone();
        // The approval store owns the frozen source; this bounded copy transfers it to the task.
        let command = Zeroizing::new(args.command.to_string());
        self.start_public_mcp_node_command(
            request,
            node_ref,
            command,
            ToolGroup::QuickCommandExecute,
        );
    }

    fn reload_quick_commands_surface(&self, cx: &mut Context<Self>) {
        let _ = self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.store.reload_from_store();
        });
    }
}

impl super::PublicMcpWorkspaceBridge {
    fn sync_quick_command_refs(
        &mut self,
        client_ref: &ClientRef,
        snapshot: &QuickCommandsSnapshot,
    ) {
        let ids = snapshot
            .commands
            .iter()
            .map(|command| command.id.as_str())
            .collect::<HashSet<_>>();
        let removed_refs = self
            .quick_command_refs
            .extract_if(|(owner, id), _| owner == client_ref && !ids.contains(id.as_str()))
            .map(|(_, quickcommand_ref)| quickcommand_ref)
            .collect::<HashSet<_>>();
        self.quick_command_ids
            .retain(|quickcommand_ref, _| !removed_refs.contains(quickcommand_ref));
        for command in &snapshot.commands {
            let _ = self.quick_command_ref(client_ref, &command.id);
        }
    }

    fn quick_command_ref(&mut self, client_ref: &ClientRef, command_id: &str) -> QuickCommandRef {
        let key = (client_ref.clone(), command_id.to_owned());
        let quickcommand_ref = self.quick_command_refs.entry(key).or_default().clone();
        self.quick_command_ids
            .entry(quickcommand_ref.clone())
            .or_insert_with(|| (client_ref.clone(), command_id.to_owned()));
        quickcommand_ref
    }

    fn quick_command_id(
        &mut self,
        client_ref: &ClientRef,
        quickcommand_ref: &QuickCommandRef,
        snapshot: &QuickCommandsSnapshot,
    ) -> Option<String> {
        self.sync_quick_command_refs(client_ref, snapshot);
        self.quick_command_ids
            .get(quickcommand_ref)
            .filter(|(owner, _)| owner == client_ref)
            .map(|(_, command_id)| command_id.clone())
    }

    fn quick_command_summary(
        &mut self,
        client_ref: &ClientRef,
        command: QuickCommand,
    ) -> QuickCommandSummary {
        let quickcommand_ref = self.quick_command_ref(client_ref, &command.id);
        QuickCommandSummary {
            quickcommand_ref,
            name: command.name,
            category: command.category,
            description: command.description,
            host_pattern: command.availability.host_patterns.first().cloned(),
            host_patterns: command.availability.host_patterns,
            protocols: command.availability.protocols,
            parameter_count: command.parameters.len(),
            confirmation: command.confirmation,
            risk: classify_command_risk(&command.command)
                .map(quick_command_risk_name)
                .or_else(|| {
                    oxideterm_quick_commands::quick_command_has_runtime_substitutions(
                        &command.command,
                    )
                    .then_some("dynamic")
                }),
            updated_at: command.updated_at,
        }
    }

    fn remove_quick_command_ref(
        &mut self,
        client_ref: &ClientRef,
        quickcommand_ref: &QuickCommandRef,
    ) {
        let Some((owner, command_id)) = self.quick_command_ids.get(quickcommand_ref) else {
            return;
        };
        if owner != client_ref {
            return;
        }
        let owner = owner.clone();
        let command_id = command_id.clone();
        self.quick_command_ids.remove(quickcommand_ref);
        self.quick_command_refs.remove(&(owner, command_id));
    }
}

fn core_quick_command_protocol(
    protocol: PublicQuickCommandTargetProtocol,
) -> QuickCommandTargetProtocol {
    match protocol {
        PublicQuickCommandTargetProtocol::Local => QuickCommandTargetProtocol::Local,
        PublicQuickCommandTargetProtocol::Ssh => QuickCommandTargetProtocol::Ssh,
        PublicQuickCommandTargetProtocol::Mosh => QuickCommandTargetProtocol::Mosh,
        PublicQuickCommandTargetProtocol::Telnet => QuickCommandTargetProtocol::Telnet,
        PublicQuickCommandTargetProtocol::Serial => QuickCommandTargetProtocol::Serial,
        PublicQuickCommandTargetProtocol::Tmux => QuickCommandTargetProtocol::Tmux,
    }
}

fn public_quick_command_parameter(parameter: QuickCommandParameter) -> PublicQuickCommandParameter {
    PublicQuickCommandParameter {
        name: parameter.name,
        label: parameter.label,
        kind: match parameter.kind {
            QuickCommandParameterKind::Text => PublicQuickCommandParameterKind::Text,
            QuickCommandParameterKind::Choice => PublicQuickCommandParameterKind::Choice,
            QuickCommandParameterKind::Secret => PublicQuickCommandParameterKind::Secret,
        },
        default_value: parameter.default_value,
        choices: parameter.choices,
        required: parameter.required,
    }
}

fn quick_command_matches_query(command: &QuickCommand, query: &str) -> bool {
    query.is_empty()
        || command.name.to_lowercase().contains(query)
        || command.category.to_lowercase().contains(query)
        || command
            .description
            .as_deref()
            .is_some_and(|description| description.to_lowercase().contains(query))
}

fn quick_command_risk_name(risk: QuickCommandRisk) -> &'static str {
    match risk {
        QuickCommandRisk::Medium => "medium",
        QuickCommandRisk::High => "high",
    }
}

fn next_revision(current: u64) -> u64 {
    now_ms().max(current.saturating_add(1))
}
