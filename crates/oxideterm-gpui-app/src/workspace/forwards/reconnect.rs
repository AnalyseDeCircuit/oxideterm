// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_forwarding::{ForwardRule, ForwardStatus, ForwardType, ForwardingRegistry};
use oxideterm_ssh::{ConnectionConsumer, NodeRouter, PhaseResult, ReconnectForwardRule};

pub(in crate::workspace) async fn cleanup_reconnect_created_forwards(
    forwarding_registry: &ForwardingRegistry,
    created_forwards: &[(String, String)],
) {
    for (session_id, rule_id) in created_forwards {
        if let Some(manager) = forwarding_registry.get(session_id) {
            let _ = manager.delete_forward(rule_id).await;
        }
    }
}

pub(in crate::workspace) fn release_reconnect_forward_bindings(
    router: &NodeRouter,
    bindings: &[(String, String, ConnectionConsumer)],
) {
    for (_, connection_id, consumer) in bindings {
        router.release_consumer(connection_id, consumer);
    }
}

pub(in crate::workspace) fn reconnect_forward_rule_from_rule(
    rule: ForwardRule,
) -> ReconnectForwardRule {
    ReconnectForwardRule {
        id: rule.id,
        forward_type: forward_type_to_snapshot(rule.forward_type).to_string(),
        bind_address: rule.bind_address,
        bind_port: rule.bind_port,
        target_host: rule.target_host,
        target_port: rule.target_port,
        status: forward_status_to_snapshot(&rule.status).to_string(),
        description: rule.description,
    }
}

pub(in crate::workspace) fn forward_rule_from_reconnect_snapshot(
    rule: &ReconnectForwardRule,
) -> Option<ForwardRule> {
    let mut restored = match rule.forward_type.as_str() {
        "local" => ForwardRule::local(
            rule.bind_address.clone(),
            rule.bind_port,
            rule.target_host.clone(),
            rule.target_port,
        ),
        "remote" => ForwardRule::remote(
            rule.bind_address.clone(),
            rule.bind_port,
            rule.target_host.clone(),
            rule.target_port,
        ),
        "dynamic" => ForwardRule {
            target_host: rule.target_host.clone(),
            target_port: rule.target_port,
            ..ForwardRule::dynamic(rule.bind_address.clone(), rule.bind_port)
        },
        _ => return None,
    };
    // Reconnect allocates a fresh forward id and starts a new runtime rule.
    restored.description = rule.description.clone();
    restored.status = ForwardStatus::Starting;
    Some(restored)
}

pub(in crate::workspace) fn forward_restore_key_for_rule(rule: &ForwardRule) -> String {
    [
        forward_type_to_snapshot(rule.forward_type).to_string(),
        rule.bind_address.clone(),
        rule.bind_port.to_string(),
        rule.target_host.clone(),
        rule.target_port.to_string(),
    ]
    .join(":")
}

pub(in crate::workspace) fn forward_restore_key_for_snapshot_rule(
    rule: &ReconnectForwardRule,
) -> String {
    [
        rule.forward_type.clone(),
        rule.bind_address.clone(),
        rule.bind_port.to_string(),
        rule.target_host.clone(),
        rule.target_port.to_string(),
    ]
    .join(":")
}

pub(in crate::workspace) fn forward_restore_failure_label(rule: &ReconnectForwardRule) -> String {
    match rule.forward_type.as_str() {
        "dynamic" => format!("dynamic {}:{}", rule.bind_address, rule.bind_port),
        forward_type => format!(
            "{forward_type} {}:{} -> {}:{}",
            rule.bind_address, rule.bind_port, rule.target_host, rule.target_port
        ),
    }
}

pub(in crate::workspace) fn forward_restore_result_detail(
    restored: u32,
    failures: u32,
    failure_details: &[String],
) -> String {
    if failures == 0 {
        return format!("restored {restored} forward(s)");
    }

    let mut detail =
        format!("forward restore failed: restored {restored} forward(s), {failures} failed");
    if !failure_details.is_empty() {
        detail.push_str(": ");
        let displayed = failure_details.iter().take(3).cloned().collect::<Vec<_>>();
        detail.push_str(&displayed.join("; "));
        let hidden = failure_details.len().saturating_sub(displayed.len());
        if hidden > 0 {
            detail.push_str(&format!("; +{hidden} more"));
        }
    }
    detail
}

pub(in crate::workspace) fn forward_restore_phase_result(_failures: u32) -> PhaseResult {
    // Forward restoration is best effort and does not abort later reconnect phases.
    PhaseResult::Ok
}

fn forward_type_to_snapshot(forward_type: ForwardType) -> &'static str {
    match forward_type {
        ForwardType::Local => "local",
        ForwardType::Remote => "remote",
        ForwardType::Dynamic => "dynamic",
    }
}

fn forward_status_to_snapshot(status: &ForwardStatus) -> &'static str {
    match status {
        ForwardStatus::Starting => "starting",
        ForwardStatus::Active => "active",
        ForwardStatus::Stopped => "stopped",
        ForwardStatus::Error => "error",
        ForwardStatus::Suspended => "suspended",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_key_keeps_distinct_targets() {
        let service_a = ReconnectForwardRule {
            forward_type: "local".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: 8080,
            target_host: "service-a".to_string(),
            target_port: 3000,
            ..ReconnectForwardRule::default()
        };
        let service_b = ReconnectForwardRule {
            target_host: "service-b".to_string(),
            target_port: 4000,
            ..service_a.clone()
        };

        assert_ne!(
            forward_restore_key_for_snapshot_rule(&service_a),
            forward_restore_key_for_snapshot_rule(&service_b)
        );
    }

    #[test]
    fn restore_allocates_fresh_starting_rule() {
        let snapshot = ReconnectForwardRule {
            id: "old-forward-id".to_string(),
            forward_type: "dynamic".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: 1080,
            target_host: "0.0.0.0".to_string(),
            target_port: 0,
            status: "active".to_string(),
            description: "socks".to_string(),
        };

        let restored = forward_rule_from_reconnect_snapshot(&snapshot)
            .expect("dynamic snapshot should restore");

        assert_ne!(restored.id, snapshot.id);
        assert_eq!(restored.status, ForwardStatus::Starting);
        assert_eq!(restored.target_host, "0.0.0.0");
        assert_eq!(restored.target_port, 0);
        assert_eq!(restored.description, "socks");
    }

    #[test]
    fn restore_failure_detail_keeps_forward_error_class() {
        let rule = ReconnectForwardRule {
            forward_type: "local".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: 8080,
            target_host: "localhost".to_string(),
            target_port: 3000,
            ..ReconnectForwardRule::default()
        };
        let details = vec![format!(
            "{}: Connection failed: Port already in use: 127.0.0.1:8080",
            forward_restore_failure_label(&rule)
        )];

        let detail = forward_restore_result_detail(0, 1, &details);

        assert!(detail.starts_with("forward restore failed:"));
        assert!(detail.contains("local 127.0.0.1:8080 -> localhost:3000"));
        assert!(detail.contains("Port already in use"));
    }

    #[test]
    fn restore_failures_do_not_abort_reconnect_pipeline() {
        assert_eq!(forward_restore_phase_result(0), PhaseResult::Ok);
        assert_eq!(forward_restore_phase_result(2), PhaseResult::Ok);
    }
}
