mod tests {
    use super::super::super::overlay::coalesce_connection_trace_running_events;
    use super::super::super::*;

    fn connection_trace_event(
        status: ConnectionTraceStatus,
        stage: ConnectionTraceStage,
        progress: f32,
    ) -> ConnectionTraceEvent {
        ConnectionTraceEvent {
            attempt_id: "attempt-1".to_string(),
            node_id: NodeId::new("node-1"),
            stage,
            status,
            progress,
            elapsed_ms: 0,
            detail: None,
            label: None,
            endpoint: None,
            step_index: Some(1),
            total_steps: Some(1),
            mode: ConnectionTraceMode::Connect,
        }
    }

    #[test]
    fn connection_trace_coalesces_running_progress_before_terminal_state() {
        let events = vec![
            connection_trace_event(
                ConnectionTraceStatus::Running,
                ConnectionTraceStage::Queued,
                5.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Running,
                ConnectionTraceStage::Authentication,
                62.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Ready,
                ConnectionTraceStage::Ready,
                100.0,
            ),
        ];

        let coalesced = coalesce_connection_trace_running_events(events);

        assert_eq!(coalesced.len(), 2);
        assert_eq!(coalesced[0].stage, ConnectionTraceStage::Authentication);
        assert_eq!(coalesced[1].status, ConnectionTraceStatus::Ready);
    }

    #[test]
    fn connection_trace_never_merges_terminal_transitions() {
        let events = vec![
            connection_trace_event(
                ConnectionTraceStatus::Ready,
                ConnectionTraceStage::Ready,
                100.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Failed,
                ConnectionTraceStage::Authentication,
                100.0,
            ),
            connection_trace_event(
                ConnectionTraceStatus::Cancelled,
                ConnectionTraceStage::Authentication,
                100.0,
            ),
        ];

        let coalesced = coalesce_connection_trace_running_events(events);

        assert_eq!(coalesced.len(), 3);
        assert_eq!(coalesced[0].status, ConnectionTraceStatus::Ready);
        assert_eq!(coalesced[1].status, ConnectionTraceStatus::Failed);
        assert_eq!(coalesced[2].status, ConnectionTraceStatus::Cancelled);
    }

    #[test]
    fn failed_session_tree_replace_preserves_previous_snapshot() {
        let tempdir =
            std::env::temp_dir().join(format!("oxideterm-session-tree-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tempdir).unwrap();
        let path = tempdir.join("session_tree.json");
        let previous = NodeTreePersistenceSnapshot {
            version: 1,
            exported_at_ms: 1,
            root_ids: Vec::new(),
            nodes: Vec::new(),
        };
        write_session_tree_snapshot(&path, &previous).unwrap();
        let previous_bytes = fs::read(&path).unwrap();
        let replacement = NodeTreePersistenceSnapshot {
            version: 1,
            exported_at_ms: 2,
            root_ids: Vec::new(),
            nodes: Vec::new(),
        };
        inject_session_tree_replace_failure();

        assert!(write_session_tree_snapshot(&path, &replacement).is_err());
        assert_eq!(fs::read(path).unwrap(), previous_bytes);
        let _ = fs::remove_dir_all(tempdir);
    }
}
