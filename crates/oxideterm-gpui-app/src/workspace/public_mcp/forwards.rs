use std::collections::{HashMap, HashSet};

use gpui::Context;
use oxideterm_forwarding::{ForwardRule, ForwardStatus, ForwardType, ForwardUpdate};
use oxideterm_public_mcp::{
    ClientRef, DomainRequest, ForwardKind, ForwardRef, NodeRef, PublicToolCall, ToolEnvelope,
    calls::{ForwardPatch, ForwardsOpenArgs},
};
use oxideterm_ssh::NodeId;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    PublicMcpForwardRecord, PublicMcpRuntimeHandles, WorkspaceApp, finish_serialized,
    node_lease_for_client,
};
use crate::workspace::forwards::PublicMcpForwardMutation;

const PUBLIC_MCP_FORWARD_CAPACITY: usize = 512;
const PUBLIC_MCP_FORWARD_CAPACITY_PER_CLIENT: usize = 128;

#[derive(Serialize)]
struct PublicForwardProjection {
    forward_ref: ForwardRef,
    node_ref: NodeRef,
    kind: &'static str,
    bind_address: String,
    bind_port: u16,
    target_host: Option<String>,
    target_port: Option<u16>,
    status: &'static str,
    description: String,
    persisted: bool,
    created_by_client: bool,
    revision: String,
}

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_forwards_list(&mut self, request: DomainRequest) {
        let PublicToolCall::ForwardsList(args) = &request.call else {
            return;
        };
        let targets = public_forward_targets(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            args.node_ref.as_ref(),
        );
        if args.node_ref.is_some() && targets.is_empty() {
            request.finish(ToolEnvelope::failed("The node handle is unavailable"));
            return;
        }
        let include_stopped = args.include_stopped.unwrap_or(true);
        let mut forwards = Vec::new();
        for (node_ref, node_id) in targets {
            let rules = self.forwarding_service.public_mcp_rules_for_node(&node_id);
            sync_forward_refs(
                &self.public_mcp.runtime_handles,
                &self.forwarding_service,
                &request.client_ref,
                &node_ref,
                &node_id,
                &rules,
            );
            let handles = self.public_mcp.runtime_handles.lock();
            for rule in rules
                .iter()
                .filter(|rule| include_stopped || rule.status != ForwardStatus::Stopped)
            {
                let Some((forward_ref, record)) = handles.forwards.iter().find(|(_, record)| {
                    record.client_ref == request.client_ref
                        && record.node_id == node_id
                        && record.forward_id == rule.id
                }) else {
                    continue;
                };
                forwards.push(public_forward_projection(forward_ref.clone(), record, rule));
            }
        }
        finish_serialized(request, json!({ "forwards": forwards }));
    }

    pub(super) fn handle_public_mcp_forwards_open(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ForwardsOpen(args) = &request.call else {
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
        let mut rule = forward_rule_from_open_args(args);
        if let Some(description) = args.description.as_ref() {
            rule.description.clone_from(description);
        }
        let forward_ref = ForwardRef::new();
        {
            let mut handles = self.public_mcp.runtime_handles.lock();
            let client_count = handles
                .forwards
                .values()
                .filter(|record| record.client_ref == request.client_ref)
                .count();
            if handles.forwards.len() >= PUBLIC_MCP_FORWARD_CAPACITY
                || client_count >= PUBLIC_MCP_FORWARD_CAPACITY_PER_CLIENT
            {
                drop(handles);
                request.finish(ToolEnvelope::failed(
                    "The retained forward handle limit has been reached",
                ));
                return;
            }
            handles.forwards.insert(
                forward_ref.clone(),
                PublicMcpForwardRecord {
                    client_ref: request.client_ref.clone(),
                    node_ref: args.node_ref.clone(),
                    node_id: lease.node_id.clone(),
                    owner_connection_id: lease.saved_connection_id.clone(),
                    forward_id: rule.id.clone(),
                    created_by_client: true,
                    persisted: false,
                },
            );
        }

        let service = self.forwarding_service.clone();
        let owner_connection_id = lease.saved_connection_id.clone();
        let check_health = args.check_health.unwrap_or(true);
        let persist = args.persist;
        let node_id = lease.node_id;
        let worker_node_id = node_id.clone();
        let worker = self.forwarding_runtime.spawn(async move {
            service
                .public_mcp_open_forward(
                    &worker_node_id,
                    owner_connection_id.as_deref(),
                    rule,
                    check_health,
                    persist,
                )
                .await
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_forward_open(request, forward_ref, node_id, result, cx);
            });
        })
        .detach();
    }

    fn finish_public_mcp_forward_open(
        &mut self,
        request: DomainRequest,
        forward_ref: ForwardRef,
        node_id: NodeId,
        result: Result<Result<PublicMcpForwardMutation, String>, tokio::task::JoinError>,
        cx: &mut Context<Self>,
    ) {
        let Ok(Ok(mutation)) = result else {
            self.public_mcp
                .runtime_handles
                .lock()
                .forwards
                .remove(&forward_ref);
            request.finish(ToolEnvelope::failed("The forward could not be opened"));
            return;
        };
        if mutation.persisted {
            // Saved forwards are part of both .oxide export and Cloud Sync snapshots.
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        let projection = {
            let mut handles = self.public_mcp.runtime_handles.lock();
            handles.forwards.get_mut(&forward_ref).map(|record| {
                record.persisted = mutation.persisted;
                public_forward_projection(forward_ref.clone(), record, &mutation.rule)
            })
        };
        let Some(projection) = projection else {
            let service = self.forwarding_service.clone();
            let forward_id = mutation.rule.id;
            let persisted = mutation.persisted;
            self.forwarding_runtime.spawn(async move {
                service
                    .public_mcp_revoke_forward(&node_id, &forward_id, persisted)
                    .await;
            });
            request.finish(ToolEnvelope::failed(
                "The forward grant was revoked while opening",
            ));
            return;
        };
        finish_serialized(request, json!({ "forward": projection }));
    }

    pub(super) fn handle_public_mcp_forwards_change(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ForwardsChange(args) = &request.call else {
            return;
        };
        let Some(record) = forward_record_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.forward_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The forward handle is unavailable"));
            return;
        };
        let Some(current) = current_forward_rule(&self.forwarding_service, &record) else {
            request.finish(ToolEnvelope::failed("The forward no longer exists"));
            return;
        };
        if forward_revision(&current) != args.expected_revision {
            request.finish(ToolEnvelope::failed(
                "The forward changed after the expected revision",
            ));
            return;
        }
        let update = forward_update_from_patch(&args.patch);
        let mut candidate = current.clone();
        candidate.apply_update(update.clone());
        if !forward_rule_is_valid(&candidate) {
            request.finish(ToolEnvelope::failed(
                "The resulting forward definition is invalid",
            ));
            return;
        }

        let service = self.forwarding_service.clone();
        let forward_ref = args.forward_ref.clone();
        let owner_connection_id = record.owner_connection_id.clone();
        let worker = self.forwarding_runtime.spawn(async move {
            service
                .public_mcp_change_forward(
                    &record.node_id,
                    owner_connection_id.as_deref(),
                    &record.forward_id,
                    &current,
                    update,
                    record.persisted,
                )
                .await
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_forward_change(request, forward_ref, result, cx);
            });
        })
        .detach();
    }

    fn finish_public_mcp_forward_change(
        &mut self,
        request: DomainRequest,
        forward_ref: ForwardRef,
        result: Result<Result<PublicMcpForwardMutation, String>, tokio::task::JoinError>,
        cx: &mut Context<Self>,
    ) {
        let Ok(Ok(mutation)) = result else {
            request.finish(ToolEnvelope::failed("The forward could not be changed"));
            return;
        };
        if mutation.persisted {
            // Persisted rule edits must invalidate the structured sync snapshot after completion.
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        let projection = {
            let mut handles = self.public_mcp.runtime_handles.lock();
            handles
                .forwards
                .get_mut(&forward_ref)
                .filter(|live| live.client_ref == request.client_ref)
                .map(|live| {
                    live.persisted |= mutation.persisted;
                    public_forward_projection(forward_ref, live, &mutation.rule)
                })
        };
        let Some(projection) = projection else {
            request.finish(ToolEnvelope::failed(
                "The forward grant was revoked while changing",
            ));
            return;
        };
        finish_serialized(request, json!({ "forward": projection }));
    }

    pub(super) fn handle_public_mcp_forwards_stop(&self, request: DomainRequest) {
        let PublicToolCall::ForwardsStop(args) = &request.call else {
            return;
        };
        let forward_ref = args.forward_ref.clone();
        self.start_public_mcp_forward_stop(request, forward_ref);
    }

    pub(super) fn handle_public_mcp_forwards_restart(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ForwardsRestart(args) = &request.call else {
            return;
        };
        let forward_ref = args.forward_ref.clone();
        let Some(record) = forward_record_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &forward_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The forward handle is unavailable"));
            return;
        };
        let service = self.forwarding_service.clone();
        let owner_connection_id = record.owner_connection_id.clone();
        let worker = self.forwarding_runtime.spawn(async move {
            service
                .public_mcp_restart_forward(
                    &record.node_id,
                    owner_connection_id.as_deref(),
                    &record.forward_id,
                    record.persisted,
                )
                .await
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_forward_restart(request, forward_ref, result, cx);
            });
        })
        .detach();
    }

    fn finish_public_mcp_forward_restart(
        &mut self,
        request: DomainRequest,
        forward_ref: ForwardRef,
        result: Result<Result<PublicMcpForwardMutation, String>, tokio::task::JoinError>,
        cx: &mut Context<Self>,
    ) {
        let Ok(Ok(mutation)) = result else {
            request.finish(ToolEnvelope::failed("The forward could not be restarted"));
            return;
        };
        if mutation.persisted {
            // Restart updates the saved auto-start projection for persisted forwards.
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        let handles = self.public_mcp.runtime_handles.lock();
        let Some(live) = handles
            .forwards
            .get(&forward_ref)
            .filter(|live| live.client_ref == request.client_ref)
        else {
            request.finish(ToolEnvelope::failed(
                "The forward grant was revoked during the operation",
            ));
            return;
        };
        let projection = public_forward_projection(forward_ref, live, &mutation.rule);
        drop(handles);
        finish_serialized(request, json!({ "forward": projection }));
    }

    fn start_public_mcp_forward_stop(&self, request: DomainRequest, forward_ref: ForwardRef) {
        let Some(record) = forward_record_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &forward_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The forward handle is unavailable"));
            return;
        };
        let service = self.forwarding_service.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let owner_connection_id = record.owner_connection_id.clone();
        self.forwarding_runtime.spawn(async move {
            let result = service
                .public_mcp_stop_forward(
                    &record.node_id,
                    owner_connection_id.as_deref(),
                    &record.forward_id,
                )
                .await;
            match result {
                Ok(rule) => {
                    let handles = handles.lock();
                    let Some(live) = handles
                        .forwards
                        .get(&forward_ref)
                        .filter(|live| live.client_ref == request.client_ref)
                    else {
                        request.finish(ToolEnvelope::failed(
                            "The forward grant was revoked during the operation",
                        ));
                        return;
                    };
                    let projection = public_forward_projection(forward_ref, live, &rule);
                    drop(handles);
                    finish_serialized(request, json!({ "forward": projection }));
                }
                Err(_) => request.finish(ToolEnvelope::failed("The forward could not be stopped")),
            }
        });
    }

    pub(super) fn handle_public_mcp_forwards_remove(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let PublicToolCall::ForwardsRemove(args) = &request.call else {
            return;
        };
        let Some(record) = forward_record_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.forward_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The forward handle is unavailable"));
            return;
        };
        let service = self.forwarding_service.clone();
        let handles = self.public_mcp.runtime_handles.clone();
        let owner_connection_id = record.owner_connection_id.clone();
        let remove_saved = args.remove_saved;
        let node_id = record.node_id.clone();
        let forward_id = record.forward_id.clone();
        let worker = self.forwarding_runtime.spawn(async move {
            service
                .public_mcp_remove_forward(
                    &node_id,
                    owner_connection_id.as_deref(),
                    &forward_id,
                    remove_saved,
                )
                .await
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let Ok(Ok(saved_removed)) = result else {
                    request.finish(ToolEnvelope::failed("The forward could not be removed"));
                    return;
                };
                invalidate_forward_records(&handles, &record.node_id, &record.forward_id);
                if saved_removed {
                    // Removing a persisted rule changes both export and Cloud Sync contents.
                    workspace.queue_cloud_sync_dirty_refresh(cx);
                }
                finish_serialized(
                    request,
                    json!({ "removed": true, "saved_definition_removed": saved_removed }),
                );
            });
        })
        .detach();
    }

    pub(super) fn handle_public_mcp_forwards_metrics(&self, request: DomainRequest) {
        let PublicToolCall::ForwardsMetrics(args) = &request.call else {
            return;
        };
        let Some(record) = forward_record_for_client(
            &self.public_mcp.runtime_handles,
            &request.client_ref,
            &args.forward_ref,
        ) else {
            request.finish(ToolEnvelope::failed("The forward handle is unavailable"));
            return;
        };
        match self
            .forwarding_service
            .public_mcp_forward_stats(&record.node_id, &record.forward_id)
        {
            Ok(stats) => finish_serialized(request, json!({ "metrics": stats })),
            Err(_) => request.finish(ToolEnvelope::failed("Forward metrics are unavailable")),
        }
    }

    pub(super) fn handle_public_mcp_forwards_discover_ports(&self, request: DomainRequest) {
        let PublicToolCall::ForwardsDiscoverPorts(args) = &request.call else {
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
        let service = self.forwarding_service.clone();
        self.forwarding_runtime.spawn(async move {
            match service
                .public_mcp_discover_ports(&lease.node_id, lease.saved_connection_id.as_deref())
                .await
            {
                Ok(snapshot) => finish_serialized(
                    request,
                    json!({
                        "has_scanned": snapshot.has_scanned,
                        "new_ports": snapshot.new_ports,
                        "closed_ports": snapshot.closed_ports,
                        "all_ports": snapshot.all_ports,
                    }),
                ),
                Err(_) => request.finish(ToolEnvelope::failed(
                    "Remote listening ports could not be discovered",
                )),
            }
        });
    }
}

pub(super) fn revoke_client_forwards(
    handles: &std::sync::Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
) -> Vec<PublicMcpForwardRecord> {
    let mut handles = handles.lock();
    let owned = handles
        .forwards
        .values()
        .filter(|record| record.client_ref == *client_ref && record.created_by_client)
        .cloned()
        .collect::<Vec<_>>();
    let owned_keys = owned
        .iter()
        .map(|record| (record.node_id.clone(), record.forward_id.clone()))
        .collect::<HashSet<_>>();
    handles.forwards.retain(|_, record| {
        record.client_ref != *client_ref
            && !owned_keys.contains(&(record.node_id.clone(), record.forward_id.clone()))
    });
    owned
}

pub(super) fn invalidate_for_disconnected_nodes(
    handles: &mut PublicMcpRuntimeHandles,
    disconnected: &[NodeId],
) {
    handles
        .forwards
        .retain(|_, record| !disconnected.contains(&record.node_id));
}

fn public_forward_targets(
    handles: &std::sync::Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
    requested_node_ref: Option<&NodeRef>,
) -> Vec<(NodeRef, NodeId)> {
    let handles = handles.lock();
    let mut targets = HashMap::<NodeRef, NodeId>::new();
    for (node_ref, lease) in &handles.nodes {
        if lease.client_ref == *client_ref {
            targets.insert(node_ref.clone(), lease.node_id.clone());
        }
    }
    for record in handles
        .forwards
        .values()
        .filter(|record| record.client_ref == *client_ref)
    {
        targets
            .entry(record.node_ref.clone())
            .or_insert_with(|| record.node_id.clone());
    }
    if let Some(requested_node_ref) = requested_node_ref {
        return targets
            .remove(requested_node_ref)
            .map(|node_id| vec![(requested_node_ref.clone(), node_id)])
            .unwrap_or_default();
    }
    targets.into_iter().collect()
}

fn sync_forward_refs(
    handles: &std::sync::Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    service: &crate::workspace::forwards::ForwardingRuntimeService,
    client_ref: &ClientRef,
    node_ref: &NodeRef,
    node_id: &NodeId,
    rules: &[ForwardRule],
) {
    let live_ids = rules
        .iter()
        .map(|rule| rule.id.as_str())
        .collect::<HashSet<_>>();
    let mut handles = handles.lock();
    handles.forwards.retain(|_, record| {
        record.client_ref != *client_ref
            || record.node_id != *node_id
            || live_ids.contains(record.forward_id.as_str())
    });
    for rule in rules {
        let exists = handles.forwards.values().any(|record| {
            record.client_ref == *client_ref
                && record.node_id == *node_id
                && record.forward_id == rule.id
        });
        if !exists {
            let client_count = handles
                .forwards
                .values()
                .filter(|record| record.client_ref == *client_ref)
                .count();
            if handles.forwards.len() >= PUBLIC_MCP_FORWARD_CAPACITY
                || client_count >= PUBLIC_MCP_FORWARD_CAPACITY_PER_CLIENT
            {
                break;
            }
            handles.forwards.insert(
                ForwardRef::new(),
                PublicMcpForwardRecord {
                    client_ref: client_ref.clone(),
                    node_ref: node_ref.clone(),
                    node_id: node_id.clone(),
                    owner_connection_id: service.public_mcp_forward_owner_connection_id(&rule.id),
                    forward_id: rule.id.clone(),
                    created_by_client: false,
                    persisted: service.public_mcp_forward_is_persisted(&rule.id),
                },
            );
        }
    }
}

fn forward_record_for_client(
    handles: &std::sync::Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    client_ref: &ClientRef,
    forward_ref: &ForwardRef,
) -> Option<PublicMcpForwardRecord> {
    handles
        .lock()
        .forwards
        .get(forward_ref)
        .filter(|record| record.client_ref == *client_ref)
        .cloned()
}

fn current_forward_rule(
    service: &crate::workspace::forwards::ForwardingRuntimeService,
    record: &PublicMcpForwardRecord,
) -> Option<ForwardRule> {
    service
        .public_mcp_rules_for_node(&record.node_id)
        .into_iter()
        .find(|rule| rule.id == record.forward_id)
}

fn invalidate_forward_records(
    handles: &std::sync::Arc<parking_lot::Mutex<PublicMcpRuntimeHandles>>,
    node_id: &NodeId,
    forward_id: &str,
) {
    handles
        .lock()
        .forwards
        .retain(|_, record| record.node_id != *node_id || record.forward_id != forward_id);
}

fn forward_rule_from_open_args(args: &ForwardsOpenArgs) -> ForwardRule {
    match args.kind {
        ForwardKind::Local => ForwardRule::local(
            &args.bind_address,
            args.bind_port,
            args.target_host.as_deref().unwrap_or_default(),
            args.target_port.unwrap_or_default(),
        ),
        ForwardKind::Remote => ForwardRule::remote(
            &args.bind_address,
            args.bind_port,
            args.target_host.as_deref().unwrap_or_default(),
            args.target_port.unwrap_or_default(),
        ),
        ForwardKind::Dynamic => ForwardRule::dynamic(&args.bind_address, args.bind_port),
    }
}

fn forward_update_from_patch(patch: &ForwardPatch) -> ForwardUpdate {
    ForwardUpdate {
        forward_type: patch.kind.map(forward_type_from_public_kind),
        bind_address: patch.bind_address.clone(),
        bind_port: patch.bind_port,
        target_host: patch.target_host.clone(),
        target_port: patch.target_port,
        description: patch.description.clone(),
    }
}

fn forward_type_from_public_kind(kind: ForwardKind) -> ForwardType {
    match kind {
        ForwardKind::Local => ForwardType::Local,
        ForwardKind::Remote => ForwardType::Remote,
        ForwardKind::Dynamic => ForwardType::Dynamic,
    }
}

fn forward_rule_is_valid(rule: &ForwardRule) -> bool {
    !rule.bind_address.trim().is_empty()
        && match rule.forward_type {
            ForwardType::Local | ForwardType::Remote => {
                !rule.target_host.trim().is_empty() && rule.target_port > 0
            }
            ForwardType::Dynamic => true,
        }
}

fn public_forward_projection(
    forward_ref: ForwardRef,
    record: &PublicMcpForwardRecord,
    rule: &ForwardRule,
) -> PublicForwardProjection {
    let dynamic = rule.forward_type == ForwardType::Dynamic;
    PublicForwardProjection {
        forward_ref,
        node_ref: record.node_ref.clone(),
        kind: public_forward_kind(rule.forward_type),
        bind_address: rule.bind_address.clone(),
        bind_port: rule.bind_port,
        target_host: (!dynamic).then(|| rule.target_host.clone()),
        target_port: (!dynamic).then_some(rule.target_port),
        status: public_forward_status(&rule.status),
        description: rule.description.clone(),
        persisted: record.persisted,
        created_by_client: record.created_by_client,
        revision: forward_revision(rule),
    }
}

fn public_forward_kind(kind: ForwardType) -> &'static str {
    match kind {
        ForwardType::Local => "local",
        ForwardType::Remote => "remote",
        ForwardType::Dynamic => "dynamic",
    }
}

fn public_forward_status(status: &ForwardStatus) -> &'static str {
    match status {
        ForwardStatus::Starting => "starting",
        ForwardStatus::Active => "active",
        ForwardStatus::Stopped => "stopped",
        ForwardStatus::Error => "error",
        ForwardStatus::Suspended => "suspended",
    }
}

fn forward_revision(rule: &ForwardRule) -> String {
    let mut digest = Sha256::new();
    digest.update(public_forward_kind(rule.forward_type).as_bytes());
    digest.update([0]);
    digest.update(rule.bind_address.as_bytes());
    digest.update(rule.bind_port.to_be_bytes());
    digest.update(rule.target_host.as_bytes());
    digest.update(rule.target_port.to_be_bytes());
    digest.update(public_forward_status(&rule.status).as_bytes());
    digest.update(rule.description.as_bytes());
    format!("rev_{:x}", digest.finalize())
}
