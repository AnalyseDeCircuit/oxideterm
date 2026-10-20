use super::*;
use oxideterm_terminal_triggers::{SavedConnectionKind, SavedConnectionRef};

fn saved_console_history_identity(
    connections: &ConnectionStore,
    connection: &SavedConnectionRef,
) -> Option<(String, String)> {
    // Scope uses connection metadata only. Credentials and transport tuning never enter the key.
    let (scope, label) = match connection.kind {
        SavedConnectionKind::Telnet => {
            let profile = connections
                .telnet_profiles()
                .iter()
                .find(|p| p.id == connection.id)?;
            (
                serde_json::json!([connection.kind, profile.id, profile.host, profile.port]),
                &profile.name,
            )
        }
        SavedConnectionKind::Mosh => {
            let profile = connections.get_mosh_profile(&connection.id)?;
            (
                serde_json::json!([
                    connection.kind,
                    profile.id,
                    profile.host,
                    profile.ssh_port,
                    profile.username
                ]),
                &profile.name,
            )
        }
        SavedConnectionKind::Serial => {
            let profile = connections
                .serial_profiles()
                .iter()
                .find(|p| p.id == connection.id)?;
            (
                serde_json::json!([connection.kind, profile.id, profile.port_path]),
                &profile.name,
            )
        }
        SavedConnectionKind::Ssh => return None,
    };
    Some((scope.to_string(), label.clone()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SavedConsoleProfileKind {
    Telnet,
    Mosh,
    Serial,
}

impl SavedConsoleProfileKind {
    fn connection_kind(self) -> SavedConnectionKind {
        match self {
            Self::Telnet => SavedConnectionKind::Telnet,
            Self::Mosh => SavedConnectionKind::Mosh,
            Self::Serial => SavedConnectionKind::Serial,
        }
    }
}

pub(in crate::workspace) fn delete_saved_console_profile(
    connections: &mut ConnectionStore,
    terminal_history_store: &TerminalHistoryStore,
    profile_kind: SavedConsoleProfileKind,
    id: &str,
) -> anyhow::Result<bool> {
    let connection = SavedConnectionRef {
        kind: profile_kind.connection_kind(),
        id: id.to_owned(),
    };
    let history_scope =
        saved_console_history_identity(connections, &connection).map(|(scope, _)| scope);
    let deleted = match profile_kind {
        SavedConsoleProfileKind::Telnet => connections.delete_telnet_profile(id),
        SavedConsoleProfileKind::Mosh => connections.delete_mosh_profile(id),
        SavedConsoleProfileKind::Serial => connections.delete_serial_profile(id),
    }?;
    if deleted && let Some(history_scope) = history_scope {
        terminal_history_store.remove_scope(&history_scope);
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_connections::{
        ConnectionStoreData, MoshProfile, SavedAuth, SerialProfile, TelnetProfile,
    };

    #[test]
    fn saved_console_history_follows_protocol_target_and_account() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connections.json");
        let mut telnet = TelnetProfile::new("Console", "device.test", 23);
        let mut mosh = MoshProfile::new("Console", "device.test", 22, "deploy", SavedAuth::Agent);
        let mut serial = SerialProfile::new("Console", "COM3");
        for id in [&mut telnet.id, &mut mosh.id, &mut serial.id] {
            *id = "same-profile-id".into();
        }
        let data = ConnectionStoreData {
            telnet_profiles: vec![telnet],
            mosh_profiles: vec![mosh],
            serial_profiles: vec![serial],
            ..ConnectionStoreData::default()
        };
        let identity = |data: &ConnectionStoreData, kind| {
            std::fs::write(&path, serde_json::to_vec(data).unwrap()).unwrap();
            let connections = ConnectionStore::load_read_only(&path).unwrap();
            saved_console_history_identity(
                &connections,
                &SavedConnectionRef {
                    kind,
                    id: "same-profile-id".into(),
                },
            )
            .unwrap()
        };
        let telnet_scope = identity(&data, SavedConnectionKind::Telnet).0;
        let mosh_scope = identity(&data, SavedConnectionKind::Mosh).0;
        let serial_scope = identity(&data, SavedConnectionKind::Serial).0;
        assert_ne!(telnet_scope, mosh_scope);
        assert_ne!(telnet_scope, serial_scope);
        assert_ne!(mosh_scope, serial_scope);

        let mut renamed = data.clone();
        renamed.mosh_profiles[0].name = "Renamed".into();
        renamed.mosh_profiles[0].auth = SavedAuth::KeyboardInteractive;
        assert_eq!(
            identity(&renamed, SavedConnectionKind::Mosh),
            (mosh_scope.clone(), "Renamed".into())
        );

        for change in ["host", "port", "account"] {
            let mut changed = data.clone();
            match change {
                "host" => changed.mosh_profiles[0].host = "another.test".into(),
                "port" => changed.mosh_profiles[0].ssh_port = 2222,
                "account" => changed.mosh_profiles[0].username = "admin".into(),
                _ => unreachable!(),
            }
            assert_ne!(
                identity(&changed, SavedConnectionKind::Mosh).0,
                mosh_scope,
                "{change}"
            );
        }
        let mut changed = data.clone();
        changed.telnet_profiles[0].port = 2323;
        changed.serial_profiles[0].port_path = "COM4".into();
        assert_ne!(
            identity(&changed, SavedConnectionKind::Telnet).0,
            telnet_scope
        );
        assert_ne!(
            identity(&changed, SavedConnectionKind::Serial).0,
            serial_scope
        );
    }

    #[test]
    fn deleting_a_saved_console_profile_retires_only_its_history_scope() {
        let kinds = [
            SavedConsoleProfileKind::Telnet,
            SavedConsoleProfileKind::Mosh,
            SavedConsoleProfileKind::Serial,
        ];
        for (deleted_index, deleted_kind) in kinds.into_iter().enumerate() {
            let directory = tempfile::tempdir().unwrap();
            let connections_path = directory.path().join("connections.json");
            let mut telnet = TelnetProfile::new("Telnet", "telnet.test", 23);
            let mut mosh = MoshProfile::new("Mosh", "mosh.test", 22, "deploy", SavedAuth::Agent);
            let mut serial = SerialProfile::new("Serial", "COM3");
            for id in [&mut telnet.id, &mut mosh.id, &mut serial.id] {
                *id = "shared-profile-id".into();
            }
            let data = ConnectionStoreData {
                telnet_profiles: vec![telnet],
                mosh_profiles: vec![mosh],
                serial_profiles: vec![serial],
                ..ConnectionStoreData::default()
            };
            std::fs::write(&connections_path, serde_json::to_vec(&data).unwrap()).unwrap();
            let mut connections = ConnectionStore::load(connections_path).unwrap();
            let (terminal_history_store, _updates) =
                TerminalHistoryStore::new(directory.path().join("history.sqlite")).unwrap();
            let scopes = kinds.map(|kind| {
                let scope = saved_console_history_identity(
                    &connections,
                    &SavedConnectionRef {
                        kind: kind.connection_kind(),
                        id: "shared-profile-id".into(),
                    },
                )
                .unwrap()
                .0;
                let _history = terminal_history_store
                    .history(scope.clone(), format!("{:?}", kind.connection_kind()));
                scope
            });

            assert!(
                delete_saved_console_profile(
                    &mut connections,
                    &terminal_history_store,
                    deleted_kind,
                    "shared-profile-id",
                )
                .unwrap()
            );
            assert!(match deleted_kind {
                SavedConsoleProfileKind::Telnet => connections
                    .telnet_profiles()
                    .iter()
                    .all(|profile| profile.id != "shared-profile-id"),
                SavedConsoleProfileKind::Mosh =>
                    connections.get_mosh_profile("shared-profile-id").is_none(),
                SavedConsoleProfileKind::Serial => connections
                    .serial_profiles()
                    .iter()
                    .all(|profile| profile.id != "shared-profile-id"),
            });

            let remaining_scopes = terminal_history_store
                .histories()
                .into_iter()
                .map(|(scope, _, _)| scope)
                .collect::<HashSet<_>>();
            let expected_scopes = scopes
                .iter()
                .enumerate()
                .filter(|(scope_index, _)| *scope_index != deleted_index)
                .map(|(_, scope)| scope.clone())
                .collect::<HashSet<_>>();
            assert_eq!(remaining_scopes, expected_scopes);
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn bind_saved_console_command_history(
        &self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.terminal_saved_connection_refs.get(&session_id) else {
            return;
        };
        let Some((scope, label)) =
            saved_console_history_identity(&self.connection_store, connection)
        else {
            return;
        };
        let pane = {
            let host = self.tab_host.read(cx);
            host.terminal_location(session_id)
                .and_then(|location| host.panes().get(&location.pane_id).cloned())
        };
        if let Some(pane) = pane {
            let history = self.terminal_history_store.history(scope, label);
            pane.update(cx, |pane, cx| pane.set_command_history(history, cx));
        }
    }

    pub(in crate::workspace) fn command_history_icon_button(
        &self,
        icon: LucideIcon,
        size: f32,
        label_key: &'static str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_tooltip_icon_button(
            icon,
            size,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions::compact(24.0),
            self.i18n.t(label_key),
            "command-history-action",
            true,
            listener,
            cx.entity(),
        )
    }
    pub(in crate::workspace) fn open_command_history_clear_confirm(
        &mut self,
        all: bool,
        cx: &mut Context<Self>,
    ) {
        self.overlay.update(cx, |overlay, cx| {
            overlay.open_confirm(
                overlay::WorkspaceOverlayConfirmKind::CommandHistoryClear { all },
                cx,
            );
        });
    }
    pub(in crate::workspace) fn ssh_command_history(
        &self,
        node_id: &NodeId,
    ) -> Option<SharedTerminalCommandHistory> {
        let node = self.ssh_nodes.get(node_id)?;
        if let Some(saved_id) = &node.saved_connection_id {
            // Renames keep history; changing the endpoint or account selects a different scope.
            let scope = serde_json::to_string(&(
                saved_id,
                &node.endpoint.host,
                node.endpoint.port,
                &node.endpoint.username,
            ))
            .ok()?;
            Some(
                self.terminal_history_store
                    .history(scope, node.title.clone()),
            )
        } else {
            self.ssh_terminal_command_histories.get(node_id).cloned()
        }
    }

    pub(in crate::workspace) fn active_command_history(
        &self,
        cx: &App,
    ) -> Option<SharedTerminalCommandHistory> {
        self.active_pane(cx)
            .map(|pane| pane.read(cx).command_history())
    }

    pub(in crate::workspace) fn remove_history_suggestion(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_history_all_connections {
            for (_, _, history) in self.terminal_history_store.histories() {
                history.remove(command);
            }
        }
        if let Some(history) = self.active_command_history(cx) {
            history.remove(command);
        }
        self.terminal_command_sender
            .update(cx, |sender, cx| sender.show_compact_history(cx));
        cx.notify();
    }
}
