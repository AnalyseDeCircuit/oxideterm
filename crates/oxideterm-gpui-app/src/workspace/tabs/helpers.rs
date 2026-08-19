use chrono::{DateTime, Utc};
use oxideterm_connections::ConnectionStore;
use oxideterm_remote_desktop::RemoteDesktopProtocol;

const WELCOME_STACKED_LAYOUT_MAX_WIDTH: f32 = 800.0;

/// Keeps the start-page breakpoint testable without coupling it to GPUI rendering.
pub(super) fn welcome_layout_is_stacked(available_width: f32) -> bool {
    available_width < WELCOME_STACKED_LAYOUT_MAX_WIDTH
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Identifies the transport without carrying any connection credentials.
pub(super) enum WelcomeRecentKind {
    Ssh,
    Serial,
    Telnet,
    Mosh,
    Rdp,
    Vnc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Keeps the start page decoupled from transport-specific open implementations.
pub(super) enum WelcomeRecentTarget {
    Ssh(String),
    Serial(String),
    Telnet(String),
    Mosh(String),
    RemoteDesktop(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WelcomeRecentConnection {
    pub name: String,
    pub subtitle: String,
    pub kind: WelcomeRecentKind,
    pub target: WelcomeRecentTarget,
    last_used_at: DateTime<Utc>,
}

/// Projects every saved transport into one non-secret, most-recently-used list.
pub(super) fn welcome_recent_connections(
    store: &ConnectionStore,
    limit: usize,
) -> Vec<WelcomeRecentConnection> {
    let mut recent = Vec::new();

    recent.extend(store.connections().iter().filter_map(|connection| {
        Some(WelcomeRecentConnection {
            name: connection.name.clone(),
            subtitle: format!(
                "{}@{}:{}",
                connection.username, connection.host, connection.port
            ),
            kind: WelcomeRecentKind::Ssh,
            target: WelcomeRecentTarget::Ssh(connection.id.clone()),
            last_used_at: connection.last_used_at?,
        })
    }));
    recent.extend(store.serial_profiles().iter().filter_map(|profile| {
        Some(WelcomeRecentConnection {
            name: profile.name.clone(),
            subtitle: format!("{} · {}", profile.port_path, profile.baud_rate),
            kind: WelcomeRecentKind::Serial,
            target: WelcomeRecentTarget::Serial(profile.id.clone()),
            last_used_at: profile.last_used_at?,
        })
    }));
    recent.extend(store.telnet_profiles().iter().filter_map(|profile| {
        Some(WelcomeRecentConnection {
            name: profile.name.clone(),
            subtitle: format!("{}:{}", profile.host, profile.port),
            kind: WelcomeRecentKind::Telnet,
            target: WelcomeRecentTarget::Telnet(profile.id.clone()),
            last_used_at: profile.last_used_at?,
        })
    }));
    recent.extend(store.mosh_profiles().iter().filter_map(|profile| {
        Some(WelcomeRecentConnection {
            name: profile.name.clone(),
            subtitle: format!("{}@{}:{}", profile.username, profile.host, profile.ssh_port),
            kind: WelcomeRecentKind::Mosh,
            target: WelcomeRecentTarget::Mosh(profile.id.clone()),
            last_used_at: profile.last_used_at?,
        })
    }));
    recent.extend(
        store
            .remote_desktop_profiles()
            .iter()
            .filter_map(|profile| {
                let subtitle = match profile.username.as_deref() {
                    Some(username) if !username.is_empty() => {
                        format!("{username}@{}:{}", profile.host, profile.port)
                    }
                    _ => format!("{}:{}", profile.host, profile.port),
                };
                Some(WelcomeRecentConnection {
                    name: profile.name.clone(),
                    subtitle,
                    kind: match profile.protocol {
                        RemoteDesktopProtocol::Rdp => WelcomeRecentKind::Rdp,
                        RemoteDesktopProtocol::Vnc => WelcomeRecentKind::Vnc,
                    },
                    target: WelcomeRecentTarget::RemoteDesktop(profile.id.clone()),
                    last_used_at: profile.last_used_at?,
                })
            }),
    );

    newest_welcome_connections(recent, limit)
}

fn newest_welcome_connections(
    mut recent: Vec<WelcomeRecentConnection>,
    limit: usize,
) -> Vec<WelcomeRecentConnection> {
    // Stable secondary keys keep equal timestamps deterministic across repaints.
    recent.sort_by(|left, right| {
        right
            .last_used_at
            .cmp(&left.last_used_at)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.subtitle.cmp(&right.subtitle))
    });
    recent.truncate(limit);
    recent
}

// Keep empty-workspace hints aligned with the same effective bindings used by dispatch.
pub(super) fn effective_shortcut_label(
    action_id: &str,
    overrides: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let definition = crate::keybindings::action_definition(action_id)?;
    let combo = crate::keybindings::effective_combo(
        definition,
        overrides,
        crate::keybindings::KeybindingSide::current(),
    )?;
    Some(crate::keybindings::format_combo(&combo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn recent_for_test(name: &str, minute: u32) -> WelcomeRecentConnection {
        WelcomeRecentConnection {
            name: name.to_string(),
            subtitle: format!("{name}.example:22"),
            kind: WelcomeRecentKind::Ssh,
            target: WelcomeRecentTarget::Ssh(name.to_string()),
            last_used_at: Utc.with_ymd_and_hms(2026, 8, 13, 12, minute, 0).unwrap(),
        }
    }

    #[test]
    fn empty_workspace_recent_connections_are_newest_first_and_limited() {
        let recent = newest_welcome_connections(
            vec![
                recent_for_test("old", 1),
                recent_for_test("new", 3),
                recent_for_test("middle", 2),
            ],
            2,
        );

        assert_eq!(
            recent
                .iter()
                .map(|connection| connection.name.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "middle"]
        );
    }

    #[test]
    fn empty_workspace_shortcut_uses_effective_override() {
        let side = crate::keybindings::KeybindingSide::current();
        let combo = crate::keybindings::KeyCombo {
            key: "p".to_string(),
            ctrl: !cfg!(target_os = "macos"),
            shift: true,
            alt: false,
            meta: cfg!(target_os = "macos"),
        };
        let expected = crate::keybindings::format_combo(&combo);
        let mut overrides = serde_json::Map::new();

        // Use the public override path so the test covers persisted settings semantics too.
        crate::keybindings::set_override(&mut overrides, "app.commandPalette", side, combo);

        assert_eq!(
            effective_shortcut_label("app.commandPalette", &overrides),
            Some(expected)
        );

        crate::keybindings::set_unbound_override(&mut overrides, "app.commandPalette", side);
        assert_eq!(
            effective_shortcut_label("app.commandPalette", &overrides),
            None
        );
    }
}
