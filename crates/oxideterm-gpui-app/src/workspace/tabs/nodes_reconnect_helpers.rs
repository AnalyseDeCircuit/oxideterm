use super::*;

pub(super) fn event_log_severity_for_connection_status(status: &str) -> WorkspaceEventSeverity {
    match status {
        // Mirrors Tauri `useEventLogCapture.statusSeverity`: link loss is the
        // disruptive event, while a final explicit disconnect is informational.
        "link_down" => WorkspaceEventSeverity::Error,
        "reconnecting" => WorkspaceEventSeverity::Warn,
        "connected" | "disconnected" => WorkspaceEventSeverity::Info,
        _ => WorkspaceEventSeverity::Info,
    }
}

pub(super) fn event_log_title_for_node_readiness(readiness: &NodeReadiness) -> &'static str {
    match readiness {
        NodeReadiness::Ready => "event_log.events.node_state_ready",
        NodeReadiness::Connecting => "event_log.events.node_state_connecting",
        NodeReadiness::Error => "event_log.events.node_state_error",
        NodeReadiness::Disconnected => "event_log.events.node_state_disconnected",
    }
}

pub(super) fn node_readiness_became_ready(
    previous: Option<&NodeReadiness>,
    current: &NodeReadiness,
) -> bool {
    !matches!(previous, Some(NodeReadiness::Ready)) && matches!(current, NodeReadiness::Ready)
}

pub(super) fn node_readiness_became_unavailable(
    previous: Option<&NodeReadiness>,
    current: &NodeReadiness,
) -> bool {
    !matches!(
        previous,
        Some(NodeReadiness::Error | NodeReadiness::Disconnected)
    ) && matches!(current, NodeReadiness::Error | NodeReadiness::Disconnected)
}

pub(super) fn reconnect_cascade_child_should_start(readiness: &NodeReadiness) -> bool {
    matches!(readiness, NodeReadiness::Error | NodeReadiness::Connecting)
}

#[cfg(test)]
mod node_reconnect_helper_tests {
    use super::*;

    #[test]
    fn ready_transition_requires_a_non_ready_previous_state() {
        assert!(node_readiness_became_ready(
            Some(&NodeReadiness::Connecting),
            &NodeReadiness::Ready
        ));
        assert!(node_readiness_became_ready(None, &NodeReadiness::Ready));
        assert!(!node_readiness_became_ready(
            Some(&NodeReadiness::Ready),
            &NodeReadiness::Ready
        ));
        assert!(!node_readiness_became_ready(
            Some(&NodeReadiness::Error),
            &NodeReadiness::Disconnected
        ));
        assert!(node_readiness_became_unavailable(
            Some(&NodeReadiness::Connecting),
            &NodeReadiness::Error
        ));
        assert!(node_readiness_became_unavailable(
            Some(&NodeReadiness::Ready),
            &NodeReadiness::Disconnected
        ));
        assert!(!node_readiness_became_unavailable(
            Some(&NodeReadiness::Error),
            &NodeReadiness::Disconnected
        ));
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
    fn node_readiness_event_titles_match_tauri_event_log_keys() {
        assert_eq!(
            event_log_title_for_node_readiness(&NodeReadiness::Ready),
            "event_log.events.node_state_ready"
        );
        assert_eq!(
            event_log_title_for_node_readiness(&NodeReadiness::Connecting),
            "event_log.events.node_state_connecting"
        );
        assert_eq!(
            event_log_title_for_node_readiness(&NodeReadiness::Error),
            "event_log.events.node_state_error"
        );
        assert_eq!(
            event_log_title_for_node_readiness(&NodeReadiness::Disconnected),
            "event_log.events.node_state_disconnected"
        );
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
