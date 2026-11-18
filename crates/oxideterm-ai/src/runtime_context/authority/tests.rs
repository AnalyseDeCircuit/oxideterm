use super::*;

#[test]
fn tool_sessions_are_isolated_and_same_run_replacement_revokes_old_authority() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let first = entity.begin_tool_session(7);
    let second = entity.begin_tool_session(8);

    assert!(entity.is_active_tool_session(7, &first));
    assert!(entity.is_active_tool_session(8, &second));
    let replacement = entity.begin_tool_session(7);
    assert!(!entity.is_active_tool_session(7, &first));
    assert!(entity.is_active_tool_session(7, &replacement));
    assert!(entity.is_active_tool_session(8, &second));
    entity.finish_tool_session(
        7,
        crate::RuntimeRevocationReason::ToolSessionCancelled,
    );
    assert!(!entity.is_active_tool_session(7, &replacement));
    assert!(entity.is_active_tool_session(8, &second));
}

#[test]
fn shutdown_rejects_late_broker_callbacks() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let session = entity.begin_tool_session(7);

    entity.shutdown();
    entity.shutdown();

    assert!(!entity.is_active_tool_session(7, &session));
    assert!(entity.terminal_owners.is_empty());
    assert!(entity.node_owners.is_empty());
    assert!(entity.sftp_owners.is_empty());
    assert!(entity.ide_owners.is_empty());
    assert!(entity.app_surface_owners.is_empty());
}

#[test]
fn closed_terminal_handle_cannot_be_validated() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let session_id = TerminalSessionId(42);
    entity.register_terminal_session(session_id, "Terminal".to_string());
    let tool_session_id = entity.begin_tool_session(7);
    let handle = entity
        .issue_terminal_handle(&tool_session_id, session_id)
        .expect("terminal handle issues");

    entity.revoke_terminal_session(session_id);

    let error = entity
        .validate_terminal_handle(
            &tool_session_id,
            Some(handle.handle_id.as_str()),
            crate::RuntimeCapability::TerminalObserve,
        )
        .expect_err("closed terminal does not retain authority");
    assert_eq!(error.public_code(), "runtime_owner_closed");
}

#[test]
fn delegated_discovery_cannot_issue_or_reuse_another_targets_handle() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let first = TerminalSessionId(41);
    let second = TerminalSessionId(42);
    entity.register_terminal_session(first, "First".into());
    entity.register_terminal_session(second, "Second".into());
    let parent = entity.begin_tool_session(1);
    let outside = entity.issue_terminal_handle(&parent, second).unwrap();
    let child = entity.begin_tool_session(2);
    let stale = entity.issue_terminal_handle(&child, second).unwrap();
    let mut scope = crate::agent::AgentScope::default();
    scope
        .targets
        .insert(entity.terminal_resource_key(first).unwrap());
    entity.restrict_agent_session(&child, &scope).unwrap();
    assert!(entity.issue_terminal_handle(&child, first).is_ok());
    assert!(entity.issue_terminal_handle(&child, second).is_err());
    for handle in [outside, stale] {
        assert!(
            entity
                .validate_terminal_handle(
                    &child,
                    Some(handle.handle_id.as_str()),
                    crate::RuntimeCapability::TerminalObserve
                )
                .is_err()
        );
    }
    scope
        .targets
        .insert(entity.terminal_resource_key(second).unwrap());
    entity.restrict_agent_session(&child, &scope).unwrap();
    assert!(!entity.agent_can_observe_terminal(&child, second));
    assert!(entity.is_active_tool_session(1, &parent));
}

#[test]
fn detaching_terminal_preserves_session_owner() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let session_id = TerminalSessionId(42);
    entity.register_terminal_session(session_id, "Terminal".to_string());
    let tool_session_id = entity.begin_tool_session(7);
    let handle = entity
        .issue_terminal_handle(&tool_session_id, session_id)
        .expect("terminal handle issues");

    // Detach and return re-announce the same session owner instead of
    // creating a new terminal generation.
    entity.register_terminal_session(session_id, "Detached terminal".to_string());

    assert!(
        entity
            .validate_terminal_handle(
                &tool_session_id,
                Some(handle.handle_id.as_str()),
                crate::RuntimeCapability::TerminalObserve,
            )
            .is_ok()
    );
}

#[test]
fn closing_terminal_does_not_revoke_shared_node() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let node_id = String::from("node-a");
    let session_id = TerminalSessionId(42);
    entity.register_node_connection(
        node_id.clone(),
        "connection-a".to_string(),
        "Node".to_string(),
        None,
    );
    entity.register_terminal_session(session_id, "Terminal".to_string());
    let tool_session_id = entity.begin_tool_session(7);
    let node_handle = entity
        .issue_node_handle(&tool_session_id, &node_id)
        .expect("node handle issues");

    entity.revoke_terminal_session(session_id);

    assert!(
        entity
            .registry
            .validate_handle(
                &tool_session_id,
                Some(&node_handle.handle_id),
                crate::RuntimeCapability::NodeInspect,
            )
            .is_ok(),
        "terminal teardown must not revoke the shared SSH node"
    );
}

#[test]
fn sftp_replacement_revokes_only_the_previous_sftp_generation() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let node_id = String::from("node-a");
    let terminal_session_id = TerminalSessionId(42);
    entity.register_terminal_session(terminal_session_id, "Terminal".to_string());
    entity.register_sftp_session(
        node_id.clone(),
        "connection-a".to_string(),
        7,
        "SFTP owner".to_string(),
        None,
    );
    let tool_session_id = entity.begin_tool_session(7);
    let terminal_handle = entity
        .issue_terminal_handle(&tool_session_id, terminal_session_id)
        .expect("terminal handle issues");
    let old_sftp_handle = entity
        .issue_sftp_handle(&tool_session_id, &node_id)
        .expect("SFTP handle issues");

    entity.register_sftp_session(
        node_id.clone(),
        "connection-a".to_string(),
        8,
        "SFTP owner".to_string(),
        None,
    );

    let error = entity
        .validate_sftp_handle(
            &tool_session_id,
            Some(old_sftp_handle.handle_id.as_str()),
            crate::RuntimeCapability::SftpRead,
        )
        .expect_err("the prior concrete channel generation must be stale");
    assert_eq!(error.public_code(), "runtime_owner_replaced");
    assert!(
        entity
            .validate_terminal_handle(
                &tool_session_id,
                Some(terminal_handle.handle_id.as_str()),
                crate::RuntimeCapability::TerminalObserve,
            )
            .is_ok(),
        "replacing SFTP must not revoke an unrelated terminal owner"
    );
}

#[test]
fn node_disconnect_revokes_sftp_and_ide_but_not_terminal_handles() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let node_id = String::from("node-a");
    let terminal_session_id = TerminalSessionId(42);
    let ide_tab_id = TabId(13);
    entity.register_node_connection(
        node_id.clone(),
        "connection-a".to_string(),
        "Node".to_string(),
        None,
    );
    entity.register_terminal_session(terminal_session_id, "Terminal".to_string());
    entity.register_sftp_session(
        node_id.clone(),
        "connection-a".to_string(),
        7,
        "SFTP owner".to_string(),
        None,
    );
    entity.register_ide_surface(ide_tab_id, node_id.clone(), "IDE owner".to_string(), None);
    let tool_session_id = entity.begin_tool_session(7);
    let terminal_handle = entity
        .issue_terminal_handle(&tool_session_id, terminal_session_id)
        .expect("terminal handle issues");
    let node_handle = entity
        .issue_node_handle(&tool_session_id, &node_id)
        .expect("node handle issues");
    let sftp_handle = entity
        .issue_sftp_handle(&tool_session_id, &node_id)
        .expect("SFTP handle issues");
    let ide_handle = entity
        .issue_ide_handle(&tool_session_id, ide_tab_id)
        .expect("IDE handle issues");

    entity.revoke_node_connection(&node_id);

    assert!(
        entity
            .registry
            .validate_handle(
                &tool_session_id,
                Some(&node_handle.handle_id),
                crate::RuntimeCapability::NodeInspect,
            )
            .is_err()
    );
    assert!(
        entity
            .validate_sftp_handle(
                &tool_session_id,
                Some(sftp_handle.handle_id.as_str()),
                crate::RuntimeCapability::SftpRead,
            )
            .is_err()
    );
    assert!(
        entity
            .validate_ide_handle(
                &tool_session_id,
                Some(ide_handle.handle_id.as_str()),
                crate::RuntimeCapability::IdeRead,
            )
            .is_err()
    );
    assert!(
        entity
            .validate_terminal_handle(
                &tool_session_id,
                Some(terminal_handle.handle_id.as_str()),
                crate::RuntimeCapability::TerminalObserve,
            )
            .is_ok(),
        "node teardown must not revoke the independently owned terminal session"
    );
}

#[test]
fn node_metadata_refresh_preserves_the_connection_generation() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let node_id = String::from("node-a");
    entity.register_node_connection(
        node_id.clone(),
        "connection-a".to_string(),
        "Initial label".to_string(),
        None,
    );
    let tool_session_id = entity.begin_tool_session(7);
    let handle = entity
        .issue_node_handle(&tool_session_id, &node_id)
        .expect("node handle issues");

    // A label update is metadata, not a replacement of the physical connection.
    entity.register_node_connection(
        node_id,
        "connection-a".to_string(),
        "Updated label".to_string(),
        None,
    );

    assert!(
        entity
            .registry
            .validate_handle(
                &tool_session_id,
                Some(&handle.handle_id),
                crate::RuntimeCapability::NodeInspect,
            )
            .is_ok()
    );
}

#[test]
fn closing_one_ide_surface_does_not_select_another_surface() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let first_tab = TabId(11);
    let second_tab = TabId(12);
    entity.register_ide_surface(
        first_tab,
        String::from("node-a"),
        "First IDE".to_string(),
        None,
    );
    entity.register_ide_surface(
        second_tab,
        String::from("node-a"),
        "Second IDE".to_string(),
        None,
    );
    let tool_session_id = entity.begin_tool_session(7);
    let first_handle = entity
        .issue_ide_handle(&tool_session_id, first_tab)
        .expect("first IDE handle issues");
    let second_handle = entity
        .issue_ide_handle(&tool_session_id, second_tab)
        .expect("second IDE handle issues");

    entity.revoke_ide_surface(first_tab);

    assert!(
        entity
            .validate_ide_handle(
                &tool_session_id,
                Some(first_handle.handle_id.as_str()),
                crate::RuntimeCapability::IdeRead,
            )
            .is_err(),
        "a closed surface must not rebind to another surface on the same node"
    );
    assert_eq!(
        entity
            .validate_ide_handle(
                &tool_session_id,
                Some(second_handle.handle_id.as_str()),
                crate::RuntimeCapability::IdeRead,
            )
            .expect("the second surface remains valid")
            .0,
        second_tab
    );
}

#[test]
fn closed_app_surface_handle_cannot_focus_another_tab() {
    let mut entity = AiRuntimeAuthority::<String>::new();
    let first_tab = TabId(21);
    let second_tab = TabId(22);
    entity.register_app_surface(first_tab, "First settings".to_string(), None);
    entity.register_app_surface(second_tab, "Second settings".to_string(), None);
    let tool_session_id = entity.begin_tool_session(9);
    let first_handle = entity
        .issue_app_surface_handle(&tool_session_id, first_tab)
        .expect("surface handle issues");

    entity.revoke_app_surface(first_tab);

    let error = entity
        .validate_app_surface_handle(&tool_session_id, Some(first_handle.handle_id.as_str()))
        .expect_err("closed surface cannot rebind to another tab");
    assert_eq!(error.public_code(), "runtime_owner_closed");
    let second_handle = entity
        .issue_app_surface_handle(&tool_session_id, second_tab)
        .expect("remaining surface handle issues");
    assert_eq!(
        entity
            .validate_app_surface_handle(
                &tool_session_id,
                Some(second_handle.handle_id.as_str()),
            )
            .expect("remaining surface stays valid"),
        second_tab
    );
}
