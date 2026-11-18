use super::*;
use chrono::Utc;
use oxideterm_connections::{
    AuthType, ConnectionInfo, SavedAuth, SavedConnection, SavedUpstreamProxyPolicy, SecretString,
    SshConfigHost,
};
use std::collections::{HashMap, HashSet};

fn saved_connection_fixture(auth: SavedAuth) -> SavedConnection {
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
pub(super) fn row_menu_selection_target_maps_saved_profiles_and_skips_unselectable_rows() {
    // Every saved-profile kind participates in the row-menu multi-delete, so a
    // missing arm would silently downgrade that kind to single-row deletion.
    let saved_kinds = [
        (
            SessionManagerRowActionTarget::Tunnel("t".into()),
            SessionManagerSelectionTarget::Tunnel("t".into()),
        ),
        (
            SessionManagerRowActionTarget::Plugin("p".into()),
            SessionManagerSelectionTarget::Plugin("p".into()),
        ),
        (
            SessionManagerRowActionTarget::Connection("c".into()),
            SessionManagerSelectionTarget::Connection("c".into()),
        ),
        (
            SessionManagerRowActionTarget::Serial("s".into()),
            SessionManagerSelectionTarget::Serial("s".into()),
        ),
        (
            SessionManagerRowActionTarget::Ble("b".into()),
            SessionManagerSelectionTarget::Ble("b".into()),
        ),
        (
            SessionManagerRowActionTarget::LocalTerminal("l".into()),
            SessionManagerSelectionTarget::LocalTerminal("l".into()),
        ),
        (
            SessionManagerRowActionTarget::Telnet("te".into()),
            SessionManagerSelectionTarget::Telnet("te".into()),
        ),
        (
            SessionManagerRowActionTarget::Ftp("f".into()),
            SessionManagerSelectionTarget::Ftp("f".into()),
        ),
        (
            SessionManagerRowActionTarget::Mosh("m".into()),
            SessionManagerSelectionTarget::Mosh("m".into()),
        ),
        (
            SessionManagerRowActionTarget::StandaloneSftp("sf".into()),
            SessionManagerSelectionTarget::StandaloneSftp("sf".into()),
        ),
        (
            SessionManagerRowActionTarget::RemoteDesktop("rd".into()),
            SessionManagerSelectionTarget::RemoteDesktop("rd".into()),
        ),
    ];
    for (row_target, expected_selection) in saved_kinds {
        assert_eq!(
            session_manager_row_menu_selection_target(&row_target),
            Some(expected_selection),
            "row target {row_target:?} must map onto the matching selection target"
        );
    }
    // Groups and transient SSH-config discoveries are not selectable and keep
    // their dedicated single-row menu flows.
    assert_eq!(
        session_manager_row_menu_selection_target(&SessionManagerRowActionTarget::Group(
            "group-a".into()
        )),
        None
    );
    assert_eq!(
        session_manager_row_menu_selection_target(&SessionManagerRowActionTarget::GroupRoot),
        None
    );
    assert_eq!(
        session_manager_row_menu_selection_target(&SessionManagerRowActionTarget::SshConfig(
            "host".into()
        )),
        None
    );
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
fn shift_range_selection_selects_contiguous_visible_rows() {
    let visible: Vec<SessionManagerSelectionTarget> = ["a", "b", "c", "d"]
        .into_iter()
        .map(|name| SessionManagerSelectionTarget::Connection(name.into()))
        .collect();
    let target = |name: &str| SessionManagerSelectionTarget::Connection(name.into());

    // Endpoints are included and the result keeps render order even when the
    // anchor sits below the target (clicking upwards must not reverse ranges).
    assert_eq!(
        session_manager_selection_range(&visible, &target("a"), &target("c")),
        Some(vec![target("a"), target("b"), target("c")]),
    );
    assert_eq!(
        session_manager_selection_range(&visible, &target("c"), &target("a")),
        Some(vec![target("a"), target("b"), target("c")]),
    );
    // An anchor equal to the target selects exactly that row.
    assert_eq!(
        session_manager_selection_range(&visible, &target("b"), &target("b")),
        Some(vec![target("b")]),
    );
    // Endpoints hidden by a collapsed group or search must yield None so the
    // caller degrades to a plain single-row selection instead of a bogus range.
    assert_eq!(
        session_manager_selection_range(&visible, &target("z"), &target("c")),
        None,
    );
    assert_eq!(
        session_manager_selection_range(&visible, &target("a"), &target("z")),
        None,
    );
    assert_eq!(
        session_manager_selection_range(&[], &target("a"), &target("a")),
        None,
    );
}
