use super::*;
use oxideterm_connections::SavedProxyHop;

pub(super) fn base_form() -> NewConnectionForm {
    let mut form = NewConnectionForm::default();
    form.name = "Home".to_string();
    form.host = "192.168.1.2".to_string();
    form.port = "22".to_string();
    form.username = "me".to_string();
    form.group = "Ungrouped".to_string();
    form
}

pub(super) fn saved_connection_fixture(auth: SavedAuth) -> SavedConnection {
    let now = Utc::now();
    SavedConnection {
        id: "conn-1".to_string(),
        version: 1,
        name: "Home".to_string(),
        group: Some("Ungrouped".to_string()),
        notes: None,
        host: "192.168.1.2".to_string(),
        port: 22,
        username: "me".to_string(),
        auth,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        proxy_command: None,
        options: oxideterm_connections::ConnectionOptions::default(),
        created_at: now,
        last_used_at: None,
        updated_at: Some(now),
        color: None,
        icon_background_color: None,
        icon: None,
        tags: Vec::new(),
        post_connect_command: None,
        privilege_credentials: Vec::new(),
    }
}

#[test]
pub(super) fn ssh_config_display_projection_never_copies_proxy_command_secrets() {
    let host = SshConfigHost {
        alias: "safe-alias".to_string(),
        hostname: Some("example.com".to_string()),
        proxy_command: Some(vec![SecretString::new("secret-proxy-token")]),
        ..SshConfigHost::default()
    };
    let item =
        SessionManagerDisplayItem::SshConfig(SessionManagerSshConfigDisplayItem::from(&host));

    let search_text = item.search_text();
    assert!(search_text.contains("safe-alias"));
    assert!(!search_text.contains("secret-proxy-token"));
}

#[test]
#[ignore = "manual session group projection benchmark"]
fn session_group_projection_performance() {
    for (group_count, item_count) in [(20, 200), (100, 1000), (500, 5000)] {
        let roots: Vec<_> = (0..group_count).map(|i| format!("group-{i}")).collect();
        let expanded = roots.iter().cloned().collect();
        let children = HashMap::new();
        let mut connection = saved_connection_fixture(SavedAuth::Agent);
        let items: Vec<_> = (0..item_count)
            .map(|i| {
                connection.id = format!("connection-{i}");
                connection.group = Some(roots[i % group_count].clone());
                SessionManagerDisplayItem::Connection(ConnectionInfo::from(&connection))
            })
            .collect();
        let expected: Vec<_> = (0..group_count)
            .flat_map(|group| (group..item_count).step_by(group_count))
            .collect();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let rows = std::hint::black_box(session_manager_tree_rows(
                &items, &roots, &children, &expanded,
            ));
            let indices: Vec<_> = rows
                .iter()
                .filter_map(|row| match row {
                    SessionManagerTreeRow::Item { item_index, .. } => Some(*item_index),
                    _ => None,
                })
                .collect();
            assert_eq!(indices, expected);
        }
        eprintln!(
            "groups={group_count} connections={item_count} mean_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0 / 20.0
        );
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let rows = std::hint::black_box(session_manager_grid_rows(
                &items,
                &roots,
                "Recent".into(),
                "Hosts".into(),
                4,
                4,
            ));
            let indices: Vec<_> = rows
                .into_iter()
                .filter_map(|row| match row {
                    SessionManagerGridRow::Cards { item_indices } => Some(item_indices),
                    _ => None,
                })
                .flatten()
                .collect();
            assert_eq!(indices, expected);
        }
        eprintln!(
            "groups={group_count} connections={item_count} grid_mean_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0 / 20.0
        );
    }
}

#[test]
fn session_grid_groups_preserve_subtrees_order_and_ungrouped_cards() {
    let mut connection = saved_connection_fixture(SavedAuth::Agent);
    let items: Vec<_> = [
        Some("A/sub"),
        None,
        Some("B/deep/sub"),
        Some("A-other"),
        Some("A"),
        Some("B"),
    ]
    .into_iter()
    .map(|group| {
        connection.group = group.map(str::to_owned);
        SessionManagerDisplayItem::Connection(ConnectionInfo::from(&connection))
    })
    .collect();
    let rows = session_manager_grid_rows(
        &items,
        &["B".into(), "A".into(), "Empty".into()],
        "Recent".into(),
        "Hosts".into(),
        1,
        4,
    );
    assert_eq!(
        rows,
        vec![
            SessionManagerGridRow::SectionHeader {
                title: "B".into(),
                item_count: 2
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![2]
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![5]
            },
            SessionManagerGridRow::SectionHeader {
                title: "A".into(),
                item_count: 2
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![0]
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![4]
            },
            SessionManagerGridRow::SectionHeader {
                title: "Hosts".into(),
                item_count: 1
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![1]
            },
        ]
    );
    assert_eq!(
        session_manager_grid_rows(&items, &[], "Recent".into(), "Hosts".into(), 4, 4),
        vec![
            SessionManagerGridRow::SectionHeader {
                title: "Hosts".into(),
                item_count: 6
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![0, 1, 2, 3]
            },
            SessionManagerGridRow::Cards {
                item_indices: vec![4, 5]
            },
        ]
    );
}

#[test]
#[ignore = "manual session display sorting benchmark"]
fn session_display_sort_performance() {
    for count in [200usize, 1000, 5000] {
        let mut connection = saved_connection_fixture(SavedAuth::Agent);
        let items: Vec<_> = (0..count)
            .map(|index| {
                let rank = (index * 73) % count;
                connection.id = format!("connection-{rank:05}");
                connection.name = format!("Production-Server-{rank:05}");
                SessionManagerDisplayItem::Connection(ConnectionInfo::from(&connection))
            })
            .collect();
        let expected: Vec<_> = (0..count)
            .map(|rank| format!("connection-{rank:05}"))
            .collect();
        let mut elapsed = std::time::Duration::ZERO;
        for _ in 0..20 {
            let mut input = items.clone();
            let start = std::time::Instant::now();
            sort_session_manager_items(&mut input, SessionSortField::Name, SortDirection::Asc);
            elapsed += start.elapsed();
            assert_eq!(
                input
                    .iter()
                    .map(SessionManagerDisplayItem::id)
                    .collect::<Vec<_>>(),
                expected
            );
        }
        eprintln!(
            "connections={count} sort_mean_ms={:.3}",
            elapsed.as_secs_f64() * 1000.0 / 20.0
        );
    }
}

#[test]
fn session_display_sort_preserves_each_column_and_direction() {
    let base = saved_connection_fixture(SavedAuth::Agent);
    let items: Vec<_> = [
        ("b", "Alpha", "z", 22, "zoe", Some("b"), Some(2)),
        ("a", "alpha", "A", 2200, "Amy", None, None),
        ("c", "Beta", "m", 220, "zoe", Some("B"), Some(1)),
        ("d", "alpha", "a", 22, "amy", Some(""), Some(2)),
    ]
    .into_iter()
    .map(|(id, name, host, port, username, group, seconds)| {
        let mut connection = ConnectionInfo::from(&base);
        connection.id = id.into();
        connection.name = name.into();
        connection.host = host.into();
        connection.port = port;
        connection.username = username.into();
        connection.group = group.map(str::to_owned);
        connection.last_used_at =
            seconds.map(|seconds| format!("2026-01-01T00:00:0{seconds}+00:00"));
        if id == "b" {
            connection.auth_type = AuthType::Password;
        }
        SessionManagerDisplayItem::Connection(connection)
    })
    .collect();
    for (field, expected) in [
        (SessionSortField::Name, ["a", "b", "d", "c"]),
        (SessionSortField::Host, ["a", "d", "c", "b"]),
        (SessionSortField::Port, ["b", "d", "c", "a"]),
        (SessionSortField::Username, ["a", "d", "b", "c"]),
        (SessionSortField::AuthType, ["a", "d", "c", "b"]),
        (SessionSortField::Group, ["a", "d", "b", "c"]),
        (SessionSortField::LastUsed, ["a", "c", "b", "d"]),
    ] {
        for direction in [SortDirection::Asc, SortDirection::Desc] {
            let mut input = items.clone();
            let mut expected = expected;
            if direction == SortDirection::Desc {
                expected.reverse();
            }
            sort_session_manager_items(&mut input, field, direction);
            assert_eq!(
                input
                    .iter()
                    .map(SessionManagerDisplayItem::id)
                    .collect::<Vec<_>>(),
                expected,
                "{field:?} {direction:?}"
            );
        }
    }
}

#[test]
#[ignore = "manual recent sessions grid projection benchmark"]
fn recent_sessions_grid_performance() {
    for count in [200usize, 1000, 5000] {
        let mut connection = saved_connection_fixture(SavedAuth::Agent);
        connection.group = None;
        let items: Vec<_> = (0..count)
            .map(|index| {
                connection.id = format!("connection-{index}");
                connection.last_used_at = chrono::DateTime::from_timestamp(
                    1_700_000_000 + ((index * 73) % count) as i64,
                    0,
                );
                SessionManagerDisplayItem::Connection(ConnectionInfo::from(&connection))
            })
            .collect();
        let expected: Vec<_> = (count - 8..count).rev().collect();
        let start = std::time::Instant::now();
        for _ in 0..30 {
            let rows = std::hint::black_box(session_manager_grid_rows(
                &items,
                &[],
                "Recent".into(),
                "Hosts".into(),
                4,
                4,
            ));
            let ranks: Vec<_> = rows
                .iter()
                .filter_map(|row| match row {
                    SessionManagerGridRow::RecentItems { item_indices, .. } => Some(item_indices),
                    _ => None,
                })
                .flatten()
                .map(|index| (index * 73) % count)
                .collect();
            assert_eq!(ranks, expected);
        }
        eprintln!(
            "connections={count} recent_grid_mean_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0 / 30.0
        );
    }
}

#[test]
fn recent_sessions_grid_preserves_ties_and_excludes_unused_connections() {
    let mut connection = saved_connection_fixture(SavedAuth::Agent);
    connection.group = None;
    let items: Vec<_> = [
        None,
        Some(1),
        Some(3),
        Some(2),
        Some(3),
        Some(2),
        Some(3),
        Some(1),
        Some(3),
        Some(3),
        Some(3),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, seconds)| {
        connection.id = format!("connection-{index}");
        connection.last_used_at = seconds
            .and_then(|seconds| chrono::DateTime::from_timestamp(1_700_000_000 + seconds, 0));
        SessionManagerDisplayItem::Connection(ConnectionInfo::from(&connection))
    })
    .collect();
    for (length, expected) in [
        (1, vec![]),
        (5, vec![2, 4, 3, 1]),
        (11, vec![2, 4, 6, 8, 9, 10, 3, 5]),
    ] {
        let rows =
            session_manager_grid_rows(&items[..length], &[], "Recent".into(), "Hosts".into(), 4, 4);
        let recent: Vec<_> = rows
            .into_iter()
            .filter_map(|row| match row {
                SessionManagerGridRow::RecentItems { item_indices, .. } => Some(item_indices),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(recent, expected);
    }
}

#[test]
fn session_tree_projection_preserves_nested_groups_and_ungrouped_items() {
    let mut connection = saved_connection_fixture(SavedAuth::Agent);
    let items: Vec<_> = [Some("A/nested"), Some("A"), None, Some("A/nested-extra")]
        .into_iter()
        .map(|group| {
            connection.group = group.map(str::to_string);
            SessionManagerDisplayItem::Connection(ConnectionInfo::from(&connection))
        })
        .collect();
    let roots = vec!["A".into(), "Empty".into()];
    let children = HashMap::from([("A".into(), vec!["A/nested".into()])]);
    for nested_expanded in [false, true] {
        let mut expanded = HashSet::from(["A".into()]);
        if nested_expanded {
            expanded.insert("A/nested".into());
        }
        let mut expected = vec![
            SessionManagerTreeRow::Group {
                path: "A".into(),
                depth: 0,
                expanded: true,
                has_children: true,
            },
            SessionManagerTreeRow::Group {
                path: "A/nested".into(),
                depth: 1,
                expanded: nested_expanded,
                has_children: true,
            },
        ];
        if nested_expanded {
            expected.push(SessionManagerTreeRow::Item {
                item_index: 0,
                depth: 2,
            });
        }
        expected.extend([
            SessionManagerTreeRow::Item {
                item_index: 1,
                depth: 1,
            },
            SessionManagerTreeRow::Group {
                path: "Empty".into(),
                depth: 0,
                expanded: false,
                has_children: false,
            },
            SessionManagerTreeRow::Item {
                item_index: 2,
                depth: 0,
            },
        ]);
        assert_eq!(
            session_manager_tree_rows(&items, &roots, &children, &expanded),
            expected
        );
    }
}

#[test]
pub(super) fn unnamed_save_request_preserves_custom_icon_and_independent_colors() {
    let mut form = base_form();
    form.name.clear();
    form.icon = "cloud".to_string();
    form.color = "#7dd3fc".to_string();
    form.icon_background_color = "#082f49".to_string();
    let request = save_request_from_form(&mut form, Some("conn-1".to_string())).unwrap();

    assert_eq!(request.name, "me@192.168.1.2");
    assert_eq!(request.icon.as_deref(), Some("cloud"));
    assert_eq!(request.color.as_deref(), Some("#7dd3fc"));
    assert_eq!(request.icon_background_color.as_deref(), Some("#082f49"));
}

#[test]
pub(super) fn save_request_moves_manual_proxy_command_into_a_redacted_secret_owner() {
    let mut form = base_form();
    form.proxy_command_enabled = true;
    form.proxy_command = "helper --token proxy-command-secret".to_string();

    let request = save_request_from_form(&mut form, None).unwrap();
    let saved_command = request.proxy_command.unwrap();

    assert!(form.proxy_command.is_empty());
    assert_eq!(
        saved_command
            .plaintext_command
            .as_ref()
            .unwrap()
            .expose_secret(),
        "helper --token proxy-command-secret"
    );
    assert!(!format!("{saved_command:?}").contains("proxy-command-secret"));
}

#[test]
pub(super) fn new_connection_save_password_false_does_not_request_keychain_storage() {
    let mut form = base_form();
    form.password = "secret".to_string();
    form.save_password = false;

    let request = save_request_from_form(&mut form, None).unwrap();

    match request.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: None,
            ..
        } => {}
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn password_form_distinguishes_missing_from_explicit_empty_password() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConnectionStore::load(dir.path().join("connections.json")).unwrap();
    for empty_password in [false, true] {
        let mut form = base_form();
        form.password.clear();
        form.save_password = true;
        form.empty_password = empty_password;
        let request = save_request_from_form(&mut form, None).unwrap();
        assert_eq!(request.auth.uses_empty_password(), empty_password);
        let auth =
            oxideterm_session_adapter::auth_method_from_saved_auth(&store, &request.auth).unwrap();
        match auth {
            oxideterm_ssh::AuthMethod::Password { password, prompt } => {
                assert_eq!(prompt, !empty_password);
                assert_eq!(password.as_str(), "");
            }
            _ => panic!("expected password authentication"),
        }
        assert!(matches!(
            request.auth,
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
                ..
            }
        ));
    }
}

#[test]
pub(super) fn edit_properties_optional_name_preserves_connection_identity_and_saved_password() {
    let existing = SavedAuth::Password {
        empty_password: false,

        keychain_id: Some("kc-password".to_string()),
        plaintext_password: None,
    };
    for (name, expected) in [
        ("Home", "Home"),
        ("", "deploy@server.example.com"),
        (" \t ", "deploy@server.example.com"),
    ] {
        let mut form = base_form();
        form.name = name.to_string();
        form.host = " server.example.com ".to_string();
        form.username = " deploy ".to_string();
        form.password_loaded = false;
        form.save_password = true;

        let request = save_request_from_form_with_existing_auth(
            &mut form,
            Some("conn-1".to_string()),
            Some(&existing),
        )
        .unwrap();
        assert_eq!(request.name, expected);
        assert_eq!(request.id.as_deref(), Some("conn-1"));
        assert_eq!(request.host, "server.example.com");
        assert_eq!(request.username, "deploy");
        match request.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
                ..
            } => {
                assert_eq!(keychain_id, "kc-password");
            }
            other => panic!("unexpected auth: {other:?}"),
        }
    }
}

#[test]
pub(super) fn edit_properties_switch_from_agent_to_password_submits_new_password() {
    let existing = SavedAuth::Agent;
    let connect_timeout_seconds = 120;
    let mut saved_connection = saved_connection_fixture(existing.clone());
    saved_connection.options.connect_timeout_seconds = Some(connect_timeout_seconds);
    let mut form = form_from_saved_connection(&saved_connection, None);
    form.auth_tab = SshAuthTab::Password;
    form.password = "new-secret".to_string();

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some(saved_connection.id),
        Some(&existing),
    )
    .unwrap();
    assert_eq!(request.connect_timeout_seconds, connect_timeout_seconds);

    match request.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(password),
            ..
        } => assert_eq!(password, "new-secret"),
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn edit_properties_saved_keychain_password_starts_unloaded() {
    let saved_connection = saved_connection_fixture(SavedAuth::Password {
        empty_password: false,

        keychain_id: Some("kc-password".to_string()),
        plaintext_password: None,
    });

    let form = form_from_saved_connection(&saved_connection, None);

    assert!(!form.password_loaded);
    assert_eq!(
        form.saved_password_keychain_id.as_deref(),
        Some("kc-password")
    );
}

#[test]
pub(super) fn edit_properties_restores_proxy_chain_without_loading_secrets() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.proxy_chain = vec![SavedProxyHop {
        host: "jump.example.com".to_string(),
        port: 2222,
        username: "ops".to_string(),
        auth: SavedAuth::Password {
            empty_password: false,

            keychain_id: Some("proxy-password-keychain-id".to_string()),
            plaintext_password: None,
        },
        agent_forwarding: true,
        identity_agent: Some("/tmp/proxy-agent.sock".to_string()),
        agent_forwarding_socket: Some("/tmp/proxy-forward.sock".to_string()),
        legacy_ssh_compatibility: true,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    }];
    let form = form_from_saved_connection(&saved_connection, None);

    assert!(form.proxy_chain_expanded);
    assert_eq!(form.proxy_hops.len(), 1);
    let hop = &form.proxy_hops[0];
    assert_eq!(hop.persisted_proxy_hop_index, Some(0));
    assert_eq!(hop.host, "jump.example.com");
    assert_eq!(hop.port, "2222");
    assert_eq!(hop.username, "ops");
    assert_eq!(hop.auth_tab, SshAuthTab::Password);
    assert!(hop.password.is_empty());
    assert!(hop.passphrase.is_empty());
    assert!(hop.agent_forwarding);
    assert_eq!(hop.identity_agent, "/tmp/proxy-agent.sock");
    assert_eq!(
        hop.agent_forwarding_socket.as_deref(),
        Some("/tmp/proxy-forward.sock")
    );
    assert!(hop.legacy_ssh_compatibility);
}

#[test]
pub(super) fn edit_properties_can_remove_the_entire_proxy_chain() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.proxy_chain = vec![SavedProxyHop {
        host: "jump.example.com".to_string(),
        port: 22,
        username: "ops".to_string(),
        auth: SavedAuth::Agent,
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
    }];
    let mut form = form_from_saved_connection(&saved_connection, None);
    form.proxy_hops.clear();

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some(saved_connection.id.clone()),
        Some(&saved_connection.auth),
    )
    .unwrap();

    assert!(request.proxy_chain.is_empty());
}

#[test]
pub(super) fn edit_properties_preserves_ssh_compatibility_policy() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.legacy_ssh_compatibility = true;
    saved_connection.options.ssh_algorithms.mac = vec!["hmac-sha1".to_string()];
    saved_connection.options.dedicated_new_terminal_connection = true;

    // Editing and saving an existing connection must round-trip its transport policy.
    let mut form = form_from_saved_connection(&saved_connection, None);
    let request = save_request_from_form(&mut form, Some(saved_connection.id)).unwrap();

    assert!(form.legacy_ssh_compatibility);
    assert!(request.legacy_ssh_compatibility);
    assert_eq!(form.ssh_algorithms.mac, ["hmac-sha1"]);
    assert_eq!(request.ssh_algorithms.mac, ["hmac-sha1"]);
    assert!(form.dedicated_new_terminal_connection);
    assert!(request.dedicated_new_terminal_connection);
}

#[test]
pub(super) fn edit_properties_round_trips_host_terminal_overrides() {
    let mut saved_connection = saved_connection_fixture(SavedAuth::Agent);
    saved_connection.options.terminal = ConnectionTerminalOptions {
        encoding: Some(oxideterm_connections::ConnectionTerminalEncoding::Gb18030),
        backspace_sequence: Some(
            oxideterm_connections::ConnectionTerminalBackspaceSequence::ControlH,
        ),
        delete_sequence: Some(oxideterm_connections::ConnectionTerminalDeleteSequence::Delete),
        semantic_scheme: Some("conservative".to_string()),
        highlight_rule_set: Some("network-devices".to_string()),
        session_log_policy: oxideterm_connections::ConnectionTerminalSessionLogPolicy::Manual,
    };

    let mut form = form_from_saved_connection(&saved_connection, None);
    let request = save_request_from_form(&mut form, Some(saved_connection.id.clone())).unwrap();

    assert_eq!(form.terminal, saved_connection.options.terminal);
    assert_eq!(request.terminal, saved_connection.options.terminal);
}

#[test]
pub(super) fn edit_properties_same_key_empty_passphrase_submits_no_new_secret() {
    let existing = SavedAuth::Key {
        key_path: "/tmp/id_ed25519".to_string(),
        has_passphrase: true,
        passphrase_keychain_id: Some("kc-passphrase".to_string()),
        plaintext_passphrase: None,
    };
    let mut form = base_form();
    form.auth_tab = SshAuthTab::SshKey;
    form.key_path = "/tmp/id_ed25519".to_string();
    form.passphrase = String::new();

    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some("conn-1".to_string()),
        Some(&existing),
    )
    .unwrap();

    match request.auth {
        SavedAuth::Key {
            key_path,
            has_passphrase,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        } => {
            assert_eq!(key_path, "/tmp/id_ed25519");
            assert!(!has_passphrase);
        }
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn new_connection_request_carries_proxy_chain() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::Agent;
    form.identity_agent = "  /tmp/target-agent.sock  ".to_string();
    form.agent_forwarding_socket = Some("/tmp/target-forward.sock".to_string());
    form.proxy_hops
        .push(crate::workspace::new_connection::NewConnectionProxyHop {
            empty_password: false,
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: None,
            host: "jump.example.com".to_string(),
            port: "2222".to_string(),
            username: "ops".to_string(),
            auth_tab: SshAuthTab::Password,
            password: "jump-secret".to_string(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
            agent_forwarding: true,
            identity_agent: "  /tmp/jump-agent.sock  ".to_string(),
            agent_forwarding_socket: Some("/tmp/jump-forward.sock".to_string()),
            legacy_ssh_compatibility: true,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
        });

    let request = save_request_from_form(&mut form, None).unwrap();

    assert_eq!(
        request.identity_agent.as_deref(),
        Some("/tmp/target-agent.sock")
    );
    assert_eq!(
        request.agent_forwarding_socket.as_deref(),
        Some("/tmp/target-forward.sock")
    );
    assert_eq!(request.proxy_chain.len(), 1);
    let hop = &request.proxy_chain[0];
    assert_eq!(hop.host, "jump.example.com");
    assert_eq!(hop.port, 2222);
    assert_eq!(hop.username, "ops");
    assert!(hop.agent_forwarding);
    assert_eq!(hop.identity_agent.as_deref(), Some("/tmp/jump-agent.sock"));
    assert_eq!(
        hop.agent_forwarding_socket.as_deref(),
        Some("/tmp/jump-forward.sock")
    );
    assert!(hop.legacy_ssh_compatibility);
    match &hop.auth {
        SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(password),
            ..
        } => assert_eq!(password, "jump-secret"),
        other => panic!("unexpected proxy auth: {other:?}"),
    }
}

#[test]
pub(super) fn save_request_moves_all_visible_password_allocations_and_redacts_debug() {
    let mut form = base_form();
    form.password = "target-secret-marker".to_string();
    form.save_password = true;
    let target_pointer = form.password.as_ptr();

    let mut hop = crate::workspace::new_connection::NewConnectionProxyHop::new();
    hop.host = "jump.example.com".to_string();
    hop.username = "ops".to_string();
    hop.auth_tab = SshAuthTab::Password;
    hop.password = "jump-secret-marker".to_string();
    let hop_pointer = hop.password.as_ptr();
    form.proxy_hops.push(hop);

    form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
    form.upstream_proxy_host = "proxy.example.com".to_string();
    form.upstream_proxy_port = "1080".to_string();
    form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
    form.upstream_proxy_username = "proxy-user".to_string();
    form.upstream_proxy_password = "upstream-secret-marker".to_string();
    let upstream_pointer = form.upstream_proxy_password.as_ptr();

    let request = save_request_from_form(&mut form, None).unwrap();

    assert!(form.password.is_empty());
    assert!(form.proxy_hops[0].password.is_empty());
    assert!(form.upstream_proxy_password.is_empty());
    match &request.auth {
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => assert_eq!(password.expose_secret().as_ptr(), target_pointer),
        other => panic!("unexpected target auth: {other:?}"),
    }
    match &request.proxy_chain[0].auth {
        SavedAuth::Password {
            plaintext_password: Some(password),
            ..
        } => assert_eq!(password.expose_secret().as_ptr(), hop_pointer),
        other => panic!("unexpected proxy auth: {other:?}"),
    }
    match &request.upstream_proxy {
        SavedUpstreamProxyPolicy::Custom { proxy } => match &proxy.auth {
            oxideterm_connections::SavedUpstreamProxyAuth::Password {
                plaintext_password: Some(password),
                ..
            } => assert_eq!(password.expose_secret().as_ptr(), upstream_pointer),
            other => panic!("unexpected upstream auth: {other:?}"),
        },
        other => panic!("unexpected upstream policy: {other:?}"),
    }

    let debug = format!("{request:?}");
    for secret in [
        "target-secret-marker",
        "jump-secret-marker",
        "upstream-secret-marker",
    ] {
        assert!(!debug.contains(secret));
    }
}

#[test]
pub(super) fn upstream_proxy_test_handoff_preserves_visible_password() {
    let store = ConnectionStore::load_read_only(std::path::PathBuf::new()).unwrap();
    let mut form = base_form();
    form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Custom;
    form.upstream_proxy_host = "proxy.example.com".to_string();
    form.upstream_proxy_port = "1080".to_string();
    form.upstream_proxy_auth = NewConnectionUpstreamProxyAuth::Password;
    form.upstream_proxy_username = "proxy-user".to_string();
    form.upstream_proxy_password = "upstream-secret-marker".to_string();

    let config = runtime_upstream_proxy_config_from_form(
        &store,
        &mut form,
        RuntimeSecretHandoff::CopyForTest,
    )
    .unwrap();

    assert_eq!(form.upstream_proxy_password, "upstream-secret-marker");
    assert!(matches!(
        config.auth,
        UpstreamProxyAuth::Password { ref password, .. }
            if password.as_str() == "upstream-secret-marker"
    ));
}

#[test]
pub(super) fn save_request_moves_key_passphrase_allocation() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::SshKey;
    form.key_path = "/tmp/id_ed25519".to_string();
    form.passphrase = "passphrase-secret-marker".to_string();
    let passphrase_pointer = form.passphrase.as_ptr();

    let request = save_request_from_form(&mut form, None).unwrap();

    assert!(form.passphrase.is_empty());
    match request.auth {
        SavedAuth::Key {
            plaintext_passphrase: Some(passphrase),
            ..
        } => assert_eq!(passphrase.expose_secret().as_ptr(), passphrase_pointer),
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[test]
pub(super) fn save_validation_failure_keeps_secret_allocations_in_the_form() {
    let mut form = base_form();
    form.host.clear();
    form.password = "validation-secret-marker".to_string();
    form.save_password = true;
    let password_pointer = form.password.as_ptr();

    let error = save_request_from_form(&mut form, None).unwrap_err();

    assert!(error.to_string().contains("Host is required"));
    assert_eq!(form.password, "validation-secret-marker");
    assert_eq!(form.password.as_ptr(), password_pointer);
}

#[test]
pub(super) fn proxy_hop_two_factor_is_saved_as_keyboard_interactive() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::Agent;
    form.proxy_hops
        .push(crate::workspace::new_connection::NewConnectionProxyHop {
            empty_password: false,
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: None,
            host: "jump.example.com".to_string(),
            port: "22".to_string(),
            username: "ops".to_string(),
            auth_tab: SshAuthTab::TwoFactor,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
            agent_forwarding: false,
            identity_agent: String::new(),
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
        });

    let request = save_request_from_form(&mut form, None).unwrap();

    assert!(matches!(
        request.proxy_chain[0].auth,
        oxideterm_connections::SavedAuth::KeyboardInteractive
    ));
}

#[test]
pub(super) fn runtime_proxy_hops_are_prepended_without_cloning_the_connection_form() {
    let mut form = base_form();
    form.auth_tab = SshAuthTab::Agent;
    let mut form_hop = crate::workspace::new_connection::NewConnectionProxyHop::new();
    form_hop.host = "form-hop.example.com".to_string();
    form_hop.username = "form-user".to_string();
    form.proxy_hops.push(form_hop);

    let mut runtime_hop = crate::workspace::new_connection::NewConnectionProxyHop::new();
    runtime_hop.host = "runtime-hop.example.com".to_string();
    runtime_hop.username = "runtime-user".to_string();
    let request = save_request_from_form_with_proxy_hop_prefix(
        &mut form,
        std::slice::from_mut(&mut runtime_hop),
        None,
    )
    .unwrap();

    assert_eq!(request.proxy_chain.len(), 2);
    assert_eq!(request.proxy_chain[0].host, "runtime-hop.example.com");
    assert_eq!(request.proxy_chain[1].host, "form-hop.example.com");
}

#[test]
fn viewing_saved_password_does_not_replace_stored_credential() {
    let existing = SavedAuth::Password {
        empty_password: false,

        keychain_id: Some("stored-owner".into()),
        plaintext_password: None,
    };
    let mut form = base_form();
    form.saved_password_keychain_id = Some("stored-owner".into());
    form.password = "revealed-test-value".into();
    form.password_from_store = true;
    form.password_loaded = true;
    let request = save_request_from_form_with_existing_auth(
        &mut form,
        Some("connection".into()),
        Some(&existing),
    )
    .unwrap();
    assert!(
        matches!(request.auth, SavedAuth::Password { keychain_id: Some(ref id), plaintext_password: None ,
            ..
} if id == "stored-owner")
    );
    crate::workspace::new_connection::password_draft_mut(&mut form).push_str("-edited");
    let edited = save_request_from_form_with_existing_auth(
        &mut form,
        Some("connection".into()),
        Some(&existing),
    )
    .unwrap();
    assert!(
        matches!(edited.auth, SavedAuth::Password { plaintext_password: Some(ref password), .. } if password.expose_secret() == "revealed-test-value-edited")
    );
    assert!(form.password.is_empty());
}

#[test]
fn standalone_sftp_empty_password_roundtrip_keeps_both_endpoints_independent() {
    for empty in [false, true] {
        let auth = |empty_password| SavedAuth::Password {
            empty_password,
            keychain_id: None,
            plaintext_password: None,
        };
        let mut profile = oxideterm_connections::StandaloneSftpProfile::new(
            "pair",
            "first.test",
            22,
            "first",
            auth(!empty),
        );
        profile.transfer_mode = oxideterm_connections::StandaloneSftpTransferMode::RemoteRemote;
        profile.secondary_endpoint = Some(oxideterm_connections::StandaloneSftpEndpoint::new(
            "second.test",
            22,
            "second",
            auth(empty),
        ));
        let form = form_from_standalone_sftp_profile(&profile);
        assert_eq!(form.empty_password, !empty);
        assert_eq!(form.standalone_sftp_secondary.empty_password, empty);
    }
}
