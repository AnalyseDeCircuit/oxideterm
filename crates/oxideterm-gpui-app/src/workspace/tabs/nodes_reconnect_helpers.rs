async fn cleanup_reconnect_created_forwards(
    forwarding_registry: &ForwardingRegistry,
    created_forwards: &[(String, String)],
) {
    for (session_id, rule_id) in created_forwards {
        if let Some(manager) = forwarding_registry.get(session_id) {
            let _ = manager.delete_forward(rule_id).await;
        }
    }
}

fn release_reconnect_forward_bindings(
    router: &NodeRouter,
    bindings: &[(String, String, ConnectionConsumer)],
) {
    for (_, connection_id, consumer) in bindings {
        router.release_consumer(connection_id, consumer);
    }
}

fn reconnect_forward_rule_from_rule(rule: ForwardRule) -> ReconnectForwardRule {
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

fn forward_rule_from_reconnect_snapshot(rule: &ReconnectForwardRule) -> Option<ForwardRule> {
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
    // Tauri restore calls nodeCreateForward, which allocates a fresh id and
    // starts from Starting. Preserve that instead of resurrecting stale ids.
    restored.description = rule.description.clone();
    restored.status = ForwardStatus::Starting;
    Some(restored)
}

fn forward_restore_key_for_rule(rule: &ForwardRule) -> String {
    [
        forward_type_to_snapshot(rule.forward_type).to_string(),
        rule.bind_address.clone(),
        rule.bind_port.to_string(),
        rule.target_host.clone(),
        rule.target_port.to_string(),
    ]
    .join(":")
}

fn forward_restore_key_for_snapshot_rule(rule: &ReconnectForwardRule) -> String {
    [
        rule.forward_type.clone(),
        rule.bind_address.clone(),
        rule.bind_port.to_string(),
        rule.target_host.clone(),
        rule.target_port.to_string(),
    ]
    .join(":")
}

fn forward_restore_failure_label(rule: &ReconnectForwardRule) -> String {
    match rule.forward_type.as_str() {
        "dynamic" => format!("dynamic {}:{}", rule.bind_address, rule.bind_port),
        forward_type => format!(
            "{forward_type} {}:{} -> {}:{}",
            rule.bind_address, rule.bind_port, rule.target_host, rule.target_port
        ),
    }
}

fn forward_restore_result_detail(
    restored: u32,
    failures: u32,
    failure_details: &[String],
) -> String {
    if failures == 0 {
        return format!("restored {restored} forward(s)");
    }

    // Tauri surfaces forwarding failures as forwarding errors, while the native
    // reconnect pipeline wraps phase results in a reconnect toast. Keep the
    // phase wrapper but make the detail start with the failing subsystem so bind
    // denied, port occupied, and remote-open failures do not look like generic
    // reconnect verification drift.
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

fn reconnect_error_is_non_retryable(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "authentication failed",
        "hostkeymismatch",
        "host key",
        "permission denied",
        "user_cancelled",
        "cancelled",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

fn readiness_for_connection_status(status: &str) -> Option<NodeReadiness> {
    match status {
        "connected" => Some(NodeReadiness::Ready),
        "link_down" => Some(NodeReadiness::Error),
        "reconnecting" => Some(NodeReadiness::Connecting),
        "disconnected" => Some(NodeReadiness::Disconnected),
        _ => None,
    }
}

fn reason_for_connection_status(status: &str) -> String {
    match status {
        "connected" => "connection restored",
        "link_down" => "link down",
        "reconnecting" => "reconnecting",
        "disconnected" => "connection disconnected",
        _ => "connection status changed",
    }
    .to_string()
}

fn event_log_severity_for_connection_status(status: &str) -> WorkspaceEventSeverity {
    match status {
        // Mirrors Tauri `useEventLogCapture.statusSeverity`: link loss is the
        // disruptive event, while a final explicit disconnect is informational.
        "link_down" => WorkspaceEventSeverity::Error,
        "reconnecting" => WorkspaceEventSeverity::Warn,
        "connected" | "disconnected" => WorkspaceEventSeverity::Info,
        _ => WorkspaceEventSeverity::Info,
    }
}

fn reconnect_cascade_child_should_start(readiness: &NodeReadiness) -> bool {
    matches!(readiness, NodeReadiness::Error | NodeReadiness::Connecting)
}

#[cfg(test)]
mod node_reconnect_helper_tests {
    use super::*;

    #[test]
    fn reconnect_forward_restore_key_keeps_distinct_targets() {
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
    fn reconnect_forward_restore_allocates_fresh_starting_rule() {
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

        let restored =
            forward_rule_from_reconnect_snapshot(&snapshot).expect("dynamic snapshot should restore");

        assert_ne!(restored.id, snapshot.id);
        assert_eq!(restored.status, ForwardStatus::Starting);
        assert_eq!(restored.target_host, "0.0.0.0");
        assert_eq!(restored.target_port, 0);
        assert_eq!(restored.description, "socks");
    }

    #[test]
    fn reconnect_forward_restore_failure_detail_keeps_forward_error_class() {
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
    fn connection_status_event_severity_matches_tauri_event_log_capture() {
        assert_eq!(
            event_log_severity_for_connection_status("connected"),
            WorkspaceEventSeverity::Info
        );
        assert_eq!(
            event_log_severity_for_connection_status("link_down"),
            WorkspaceEventSeverity::Error
        );
        assert_eq!(
            event_log_severity_for_connection_status("reconnecting"),
            WorkspaceEventSeverity::Warn
        );
        assert_eq!(
            event_log_severity_for_connection_status("disconnected"),
            WorkspaceEventSeverity::Info
        );
    }

    #[test]
    fn reconnect_retry_filter_matches_tauri_non_retryable_errors() {
        assert!(reconnect_error_is_non_retryable("Authentication failed"));
        assert!(reconnect_error_is_non_retryable("HostKeyMismatch"));
        assert!(reconnect_error_is_non_retryable("host key changed"));
        assert!(reconnect_error_is_non_retryable("Permission denied"));
        assert!(reconnect_error_is_non_retryable("USER_CANCELLED"));
        assert!(reconnect_error_is_non_retryable("cancelled"));
        assert!(!reconnect_error_is_non_retryable("network timeout"));
    }

    #[test]
    fn reconnect_cascade_skips_user_disconnected_children_like_tauri_link_down_set() {
        assert!(reconnect_cascade_child_should_start(&NodeReadiness::Error));
        assert!(reconnect_cascade_child_should_start(
            &NodeReadiness::Connecting
        ));
        assert!(!reconnect_cascade_child_should_start(
            &NodeReadiness::Disconnected
        ));
        assert!(!reconnect_cascade_child_should_start(&NodeReadiness::Ready));
    }
}
