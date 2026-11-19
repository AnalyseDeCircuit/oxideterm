// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::{
    SessionManagerOpenTarget, SessionManagerRowActionTarget, SessionManagerSelectionTarget,
    SessionSortField, SortDirection, auth_label,
};
use crate::icons as session_icons;
use oxideterm_connections::{
    BleProfile, ConnectionInfo, ConnectionStore, LocalTerminalProfile, MoshProfile,
    RemoteDesktopProfile, SerialProfile, SshConfigHost, TelnetProfile,
};
use oxideterm_forwarding::PersistedForward;
use oxideterm_gpui_ui::icons::LucideIcon;
use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DirectConsoleProfileKind {
    Serial,
    Telnet,
}

impl DirectConsoleProfileKind {
    pub fn connection_kind(self) -> oxideterm_terminal_triggers::SavedConnectionKind {
        use oxideterm_terminal_triggers::SavedConnectionKind;
        match self {
            Self::Serial => SavedConnectionKind::Serial,
            Self::Telnet => SavedConnectionKind::Telnet,
        }
    }
}

pub enum SavedConsoleLaunch {
    Serial {
        config: oxideterm_terminal::SerialSessionConfig,
        title: String,
        terminal: oxideterm_connections::ConnectionTerminalOptions,
    },
    Telnet {
        config: oxideterm_terminal::TelnetSessionConfig,
        proxy: oxideterm_connections::SavedUpstreamProxyPolicy,
        terminal: oxideterm_connections::ConnectionTerminalOptions,
    },
}

pub fn prepare_saved_console_launch(
    store: &ConnectionStore,
    kind: DirectConsoleProfileKind,
    id: &str,
) -> Option<SavedConsoleLaunch> {
    use crate::form_state::{
        terminal_serial_flow_from_profile, terminal_serial_parity_from_profile,
        terminal_serial_runtime_options_from_profile, terminal_telnet_line_endings_from_profile,
    };
    match kind {
        DirectConsoleProfileKind::Serial => {
            let profile = store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == id)?;
            Some(SavedConsoleLaunch::Serial {
                config: oxideterm_terminal::SerialSessionConfig {
                    pre_connect_command: profile.pre_connect_command.clone(),
                    port_path: profile.port_path.clone(),
                    baud_rate: profile.baud_rate,
                    data_bits: profile.data_bits,
                    stop_bits: profile.stop_bits,
                    parity: terminal_serial_parity_from_profile(&profile.parity),
                    flow_control: terminal_serial_flow_from_profile(&profile.flow_control),
                    runtime_options: terminal_serial_runtime_options_from_profile(profile),
                },
                title: profile.name.clone(),
                terminal: profile.terminal.clone(),
            })
        }
        DirectConsoleProfileKind::Telnet => {
            let profile = store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == id)?;
            Some(SavedConsoleLaunch::Telnet {
                config: oxideterm_terminal::TelnetSessionConfig {
                    pre_connect_command: profile.pre_connect_command.clone(),
                    line_endings: terminal_telnet_line_endings_from_profile(profile),
                    host: profile.host.clone(),
                    port: profile.port,
                },
                proxy: profile.upstream_proxy.clone(),
                terminal: profile.terminal.clone(),
            })
        }
    }
}

pub fn mark_catalog_console_used(
    store: &mut ConnectionStore,
    kind: DirectConsoleProfileKind,
    id: &str,
) -> bool {
    match kind {
        DirectConsoleProfileKind::Serial => {
            let _ = store.mark_serial_profile_used(id);
            true
        }
        DirectConsoleProfileKind::Telnet => {
            let _ = store.mark_telnet_profile_used(id);
            false
        }
    }
}

impl super::SessionManagerState {
    pub fn report_console_open_failure(
        &mut self,
        kind: DirectConsoleProfileKind,
        error: impl std::fmt::Display,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = match kind {
            DirectConsoleProfileKind::Serial => "sessionManager.serial_profiles.open_failed",
            DirectConsoleProfileKind::Telnet => "sessionManager.telnet_profiles.open_failed",
        };
        self.set_status(Some(format!("{}: {error}", i18n.t(key))), cx);
    }
}

pub enum SavedSshTest {
    Missing(String),
    Prompt(String),
    Run {
        config: oxideterm_ssh::SshConfig,
        title: String,
    },
}

pub fn prepare_saved_ssh_test(
    store: &ConnectionStore,
    settings: &oxideterm_settings::PersistedSettings,
    id: &str,
    i18n: &oxideterm_i18n::I18n,
) -> SavedSshTest {
    let Some(connection) = store.get(id) else {
        return SavedSshTest::Missing(i18n.t("sessionManager.toast.test_failed"));
    };
    match oxideterm_session_adapter::ssh_config_from_saved_connection(store, settings, connection) {
        Some(config) => SavedSshTest::Run {
            config,
            title: connection.name.clone(),
        },
        None => SavedSshTest::Prompt(i18n.t("sessionManager.edit_properties.password_placeholder")),
    }
}

#[derive(Clone)]
pub enum SessionManagerDisplayItem {
    Tunnel(TunnelDisplayItem),
    Plugin(oxideterm_connections::PluginConnectionProfile),
    Connection(ConnectionInfo),
    SshConfig(SessionManagerSshConfigDisplayItem),
    Serial(SerialProfile),
    Ble(BleProfile),
    LocalTerminal(LocalTerminalProfile),
    Telnet(TelnetProfile),
    Ftp(oxideterm_connections::FtpProfile),
    Mosh(MoshProfile),
    StandaloneSftp(oxideterm_connections::StandaloneSftpProfile),
    RemoteDesktop(RemoteDesktopProfile),
}

pub fn saved_catalog_profile_form(
    store: &ConnectionStore,
    target: &SessionManagerOpenTarget,
    i18n: &oxideterm_i18n::I18n,
) -> Option<crate::form_state::NewConnectionForm> {
    use crate::{
        ble::form_from_ble_profile,
        configuration::form_from_standalone_sftp_profile,
        form_state::{
            form_from_mosh_profile, form_from_remote_desktop_profile, form_from_serial_profile,
            form_from_telnet_profile,
        },
    };
    use SessionManagerOpenTarget as Open;
    let ungrouped = || i18n.t("ssh.form.ungrouped");
    Some(match target {
        Open::Serial(id) => form_from_serial_profile(
            store.serial_profiles().iter().find(|p| p.id == *id)?,
            ungrouped(),
        ),
        Open::Ble(id) => form_from_ble_profile(
            store.ble_profiles().iter().find(|p| p.id == *id)?,
            ungrouped(),
        ),
        Open::Telnet(id) => form_from_telnet_profile(
            store.telnet_profiles().iter().find(|p| p.id == *id)?,
            ungrouped(),
        ),
        Open::Mosh(id) => form_from_mosh_profile(store.get_mosh_profile(id)?, ungrouped()),
        Open::StandaloneSftp(id) => {
            form_from_standalone_sftp_profile(store.get_standalone_sftp_profile(id)?)
        }
        Open::RemoteDesktop(id) => {
            form_from_remote_desktop_profile(store.get_remote_desktop_profile(id)?, ungrouped())
        }
        _ => return None,
    })
}

pub fn duplicate_catalog_connection_form(
    store: &ConnectionStore,
    id: &str,
) -> Option<crate::form_state::NewConnectionForm> {
    use crate::{
        configuration::{
            duplicate_connection_template_name, form_from_saved_connection,
            restore_legacy_jump_host_in_form,
        },
        form_state::NewConnectionField,
    };
    let saved = store.get(id)?;
    let mut form = form_from_saved_connection(saved, None);
    restore_legacy_jump_host_in_form(&mut form, saved, store);
    form.name = duplicate_connection_template_name(
        &saved.name,
        store
            .connections()
            .iter()
            .map(|connection| connection.name.as_str()),
    );
    form.focused_field = NewConnectionField::Name;
    form.field_focused = true;
    Some(form)
}

impl super::SessionManagerState {
    pub fn display_catalog(
        &self,
        store: &ConnectionStore,
        tunnels: Vec<PersistedForward>,
    ) -> Vec<SessionManagerDisplayItem> {
        let query = self.search_query.trim().to_lowercase();
        let mut items = saved_session_manager_items(store, tunnels)
            .into_iter()
            .chain(
                self.ssh_config_hosts
                    .iter()
                    .filter(|host| !host.already_imported)
                    .map(SessionManagerSshConfigDisplayItem::from)
                    .map(SessionManagerDisplayItem::SshConfig),
            )
            .filter(|item| {
                query.is_empty() || item.search_text().to_lowercase().contains(query.as_str())
            })
            .collect::<Vec<_>>();
        // Grid, list, and tree share the same ordering.
        sort_session_manager_items(&mut items, self.sort_field, self.sort_direction);
        items
    }
}

#[derive(Clone)]
pub struct TunnelDisplayItem {
    pub forward: oxideterm_forwarding::PersistedForward,
    pub profile: oxideterm_forwarding::SshTunnelProfile,
    pub host: Option<ConnectionInfo>,
}

#[derive(Clone)]
pub struct SessionManagerSshConfigDisplayItem {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
}

impl From<&SshConfigHost> for SessionManagerSshConfigDisplayItem {
    fn from(host: &SshConfigHost) -> Self {
        // Session Manager only needs non-secret SSH config metadata. In
        // particular, never copy proxy commands or their SecretString values
        // into the long-lived virtual-list projection.
        Self {
            alias: host.alias.clone(),
            hostname: host.hostname.clone(),
            user: host.user.clone(),
            port: host.port,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionManagerGridRow {
    SectionHeader {
        title: String,
        item_count: usize,
    },
    RecentItems {
        item_indices: Vec<usize>,
        is_last_in_section: bool,
    },
    Cards {
        item_indices: Vec<usize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionManagerTreeRow {
    Group {
        path: String,
        depth: usize,
        expanded: bool,
        has_children: bool,
    },
    Item {
        item_index: usize,
        depth: usize,
    },
}

impl SessionManagerTreeRow {
    pub fn depth(&self) -> usize {
        match self {
            Self::Group { depth, .. } | Self::Item { depth, .. } => *depth,
        }
    }
}

fn session_manager_tree_row_is_last(rows: &[SessionManagerTreeRow], index: usize) -> bool {
    let Some(row) = rows.get(index) else {
        return true;
    };
    let target_depth = row.depth();
    for next in rows.iter().skip(index + 1) {
        let next_depth = next.depth();
        if next_depth < target_depth {
            return true;
        }
        if next_depth == target_depth {
            return false;
        }
    }
    true
}

/// Computes which outer guide-rail columns (`0..depth - 1`) continue past the
/// row at `target_index`, and whether the row itself ends its parent's rail.
pub fn session_manager_tree_row_rails(
    rows: &[SessionManagerTreeRow],
    target_index: usize,
) -> (Vec<bool>, bool) {
    let mut continues_at_depth: Vec<bool> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let depth = row.depth();
        continues_at_depth.resize(depth, false);
        let is_last = session_manager_tree_row_is_last(rows, index);
        if index == target_index {
            let outer_rails = if depth > 1 {
                continues_at_depth[1..depth].to_vec()
            } else {
                Vec::new()
            };
            return (outer_rails, is_last);
        }
        continues_at_depth.push(depth > 0 && !is_last);
    }
    (Vec::new(), true)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionManagerItemPointerAction {
    None,
    Open,
    SelectReplace,
    SelectToggle,
    SelectRange,
}

/// Keeps card and list-row pointer behavior aligned across Session Manager layouts.
/// Double-click always opens; a single click on a selectable row replaces the
/// selection, Ctrl toggles it, and Shift extends from the anchor to this row.
pub fn session_manager_item_pointer_action(
    click_count: usize,
    selectable: bool,
    control: bool,
    shift: bool,
) -> SessionManagerItemPointerAction {
    match click_count {
        2 => SessionManagerItemPointerAction::Open,
        1 if selectable && shift => SessionManagerItemPointerAction::SelectRange,
        1 if selectable && control => SessionManagerItemPointerAction::SelectToggle,
        1 if selectable => SessionManagerItemPointerAction::SelectReplace,
        _ => SessionManagerItemPointerAction::None,
    }
}

impl SessionManagerDisplayItem {
    pub fn id(&self) -> &str {
        match self {
            Self::Tunnel(item) => &item.forward.id,
            Self::Connection(connection) => &connection.id,
            Self::SshConfig(host) => &host.alias,
            Self::Serial(profile) => &profile.id,
            Self::Ble(profile) => &profile.id,
            Self::LocalTerminal(profile) => &profile.id,
            Self::Plugin(profile) => &profile.id,
            Self::Telnet(profile) => &profile.id,
            Self::Ftp(profile) => &profile.id,
            Self::Mosh(profile) => &profile.id,
            Self::StandaloneSftp(profile) => &profile.id,
            Self::RemoteDesktop(profile) => &profile.id,
        }
    }

    pub fn selection_target(&self) -> Option<SessionManagerSelectionTarget> {
        // Only SSH config discoveries are transient; every saved profile can use batch actions.
        match self {
            Self::Tunnel(item) => Some(SessionManagerSelectionTarget::Tunnel(
                item.forward.id.clone(),
            )),
            Self::Plugin(profile) => {
                Some(SessionManagerSelectionTarget::Plugin(profile.id.clone()))
            }
            Self::Connection(connection) => Some(SessionManagerSelectionTarget::Connection(
                connection.id.clone(),
            )),
            Self::Serial(profile) => {
                Some(SessionManagerSelectionTarget::Serial(profile.id.clone()))
            }
            Self::Ble(profile) => Some(SessionManagerSelectionTarget::Ble(profile.id.clone())),
            Self::LocalTerminal(profile) => Some(SessionManagerSelectionTarget::LocalTerminal(
                profile.id.clone(),
            )),
            Self::Telnet(profile) => {
                Some(SessionManagerSelectionTarget::Telnet(profile.id.clone()))
            }
            Self::Ftp(profile) => Some(SessionManagerSelectionTarget::Ftp(profile.id.clone())),
            Self::Mosh(profile) => Some(SessionManagerSelectionTarget::Mosh(profile.id.clone())),
            Self::StandaloneSftp(profile) => Some(SessionManagerSelectionTarget::StandaloneSftp(
                profile.id.clone(),
            )),
            Self::RemoteDesktop(profile) => Some(SessionManagerSelectionTarget::RemoteDesktop(
                profile.id.clone(),
            )),
            Self::SshConfig(_) => None,
        }
    }

    pub fn row_action_target(&self) -> Option<SessionManagerRowActionTarget> {
        match self {
            Self::Tunnel(item) => Some(SessionManagerRowActionTarget::Tunnel(
                item.forward.id.clone(),
            )),
            Self::Plugin(profile) => {
                Some(SessionManagerRowActionTarget::Plugin(profile.id.clone()))
            }
            Self::Connection(connection) => Some(SessionManagerRowActionTarget::Connection(
                connection.id.clone(),
            )),
            Self::Serial(profile) => {
                Some(SessionManagerRowActionTarget::Serial(profile.id.clone()))
            }
            Self::Ble(profile) => Some(SessionManagerRowActionTarget::Ble(profile.id.clone())),
            Self::LocalTerminal(profile) => Some(SessionManagerRowActionTarget::LocalTerminal(
                profile.id.clone(),
            )),
            Self::Telnet(profile) => {
                Some(SessionManagerRowActionTarget::Telnet(profile.id.clone()))
            }
            Self::Ftp(profile) => Some(SessionManagerRowActionTarget::Ftp(profile.id.clone())),
            Self::Mosh(profile) => Some(SessionManagerRowActionTarget::Mosh(profile.id.clone())),
            Self::StandaloneSftp(profile) => Some(SessionManagerRowActionTarget::StandaloneSftp(
                profile.id.clone(),
            )),
            Self::RemoteDesktop(profile) => Some(SessionManagerRowActionTarget::RemoteDesktop(
                profile.id.clone(),
            )),
            Self::SshConfig(host) => {
                Some(SessionManagerRowActionTarget::SshConfig(host.alias.clone()))
            }
        }
    }

    pub fn open_target(&self) -> SessionManagerOpenTarget {
        match self {
            Self::Tunnel(item) => SessionManagerOpenTarget::Tunnel(item.forward.id.clone()),
            Self::Plugin(profile) => SessionManagerOpenTarget::Plugin(profile.id.clone()),
            Self::Connection(connection) => {
                SessionManagerOpenTarget::Connection(connection.id.clone())
            }
            Self::SshConfig(host) => SessionManagerOpenTarget::SshConfig(host.alias.clone()),
            Self::Serial(profile) => SessionManagerOpenTarget::Serial(profile.id.clone()),
            Self::Ble(profile) => SessionManagerOpenTarget::Ble(profile.id.clone()),
            Self::LocalTerminal(profile) => {
                SessionManagerOpenTarget::LocalTerminal(profile.id.clone())
            }
            Self::Telnet(profile) => SessionManagerOpenTarget::Telnet(profile.id.clone()),
            Self::Ftp(profile) => SessionManagerOpenTarget::Ftp(profile.id.clone()),
            Self::Mosh(profile) => SessionManagerOpenTarget::Mosh(profile.id.clone()),
            Self::StandaloneSftp(profile) => {
                SessionManagerOpenTarget::StandaloneSftp(profile.id.clone())
            }
            Self::RemoteDesktop(profile) => {
                SessionManagerOpenTarget::RemoteDesktop(profile.id.clone())
            }
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Tunnel(item) => &item.profile.name,
            Self::Plugin(profile) => &profile.name,
            Self::Connection(connection) => &connection.name,
            Self::SshConfig(host) => &host.alias,
            Self::Serial(profile) => &profile.name,
            Self::Ble(profile) => &profile.name,
            Self::LocalTerminal(profile) => &profile.name,
            Self::Telnet(profile) => &profile.name,
            Self::Ftp(profile) => &profile.name,
            Self::Mosh(profile) => &profile.name,
            Self::StandaloneSftp(profile) => &profile.name,
            Self::RemoteDesktop(profile) => &profile.name,
        }
    }

    pub fn group(&self) -> Option<&str> {
        match self {
            Self::Tunnel(item) => item.profile.group.as_deref(),
            Self::Plugin(profile) => profile.group.as_deref(),
            Self::Connection(connection) => connection.group.as_deref(),
            Self::SshConfig(_) => None,
            Self::Serial(profile) => profile.group.as_deref(),
            Self::Ble(profile) => profile.group.as_deref(),
            Self::LocalTerminal(profile) => profile.group.as_deref(),
            Self::Telnet(profile) => profile.group.as_deref(),
            Self::Ftp(profile) => profile.group.as_deref(),
            Self::Mosh(profile) => profile.group.as_deref(),
            Self::StandaloneSftp(profile) => profile.group.as_deref(),
            Self::RemoteDesktop(profile) => profile.group.as_deref(),
        }
    }

    pub fn last_used(&self) -> Option<String> {
        match self {
            Self::Tunnel(_) => None,
            Self::Plugin(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Connection(connection) => connection.last_used_at.clone(),
            Self::SshConfig(_) => None,
            Self::Serial(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Ble(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::LocalTerminal(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Telnet(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Ftp(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Mosh(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::StandaloneSftp(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::RemoteDesktop(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
        }
    }

    pub fn host(&self) -> &str {
        match self {
            Self::Tunnel(item) => item.host.as_ref().map_or("", |host| host.host.as_str()),
            Self::Plugin(profile) => &profile.plugin_id,
            Self::Connection(connection) => &connection.host,
            Self::SshConfig(host) => host.hostname.as_deref().unwrap_or(&host.alias),
            Self::Serial(profile) => &profile.port_path,
            Self::Ble(profile) => profile
                .device_name
                .as_deref()
                .or_else(|| profile.device_mac.as_deref())
                .unwrap_or(""),
            Self::LocalTerminal(profile) => profile.cwd.as_deref().unwrap_or_default(),
            Self::Telnet(profile) => &profile.host,
            Self::Ftp(profile) => &profile.host,
            Self::Mosh(profile) => &profile.host,
            Self::StandaloneSftp(profile) => &profile.host,
            Self::RemoteDesktop(profile) => &profile.host,
        }
    }

    pub fn port_sort_key(&self) -> u32 {
        match self {
            Self::Tunnel(item) => u32::from(item.forward.rule.bind_port),
            Self::Plugin(_) => 0,
            Self::Connection(connection) => u32::from(connection.port),
            Self::SshConfig(host) => u32::from(host.port.unwrap_or(22)),
            Self::Serial(profile) => profile.baud_rate,
            Self::Ble(_) => 0,
            Self::LocalTerminal(_) => 0,
            Self::Telnet(profile) => u32::from(profile.port),
            Self::Ftp(profile) => u32::from(profile.port),
            Self::Mosh(profile) => u32::from(profile.ssh_port),
            Self::StandaloneSftp(profile) => u32::from(profile.port),
            Self::RemoteDesktop(profile) => u32::from(profile.port),
        }
    }

    pub fn username(&self) -> &str {
        match self {
            Self::Tunnel(item) => item.host.as_ref().map_or("", |host| host.username.as_str()),
            Self::Plugin(_) => "",
            Self::Connection(connection) => &connection.username,
            Self::SshConfig(host) => host.user.as_deref().unwrap_or_default(),
            Self::Serial(_) | Self::Ble(_) | Self::Telnet(_) => "",
            Self::LocalTerminal(_) => "",
            Self::Ftp(profile) => &profile.username,
            Self::Mosh(profile) => &profile.username,
            Self::StandaloneSftp(profile) => &profile.username,
            Self::RemoteDesktop(profile) => profile.username.as_deref().unwrap_or_default(),
        }
    }

    pub fn auth_sort_key(&self) -> String {
        match self {
            Self::Tunnel(_) => "ssh tunnel".into(),
            Self::Plugin(profile) => profile.type_id.clone(),
            Self::Connection(connection) => auth_label(connection.auth_type).to_lowercase(),
            Self::SshConfig(_) => "ssh config".to_string(),
            Self::Serial(_) => "serial".to_string(),
            Self::Ble(_) => "ble".to_string(),
            Self::LocalTerminal(_) => "local".to_string(),
            Self::Telnet(_) => "telnet".to_string(),
            Self::Ftp(_) => "ftp".to_string(),
            Self::Mosh(_) => "mosh".to_string(),
            Self::StandaloneSftp(profile) => auth_label(profile.auth.auth_type()).to_lowercase(),
            Self::RemoteDesktop(profile) => profile.protocol.provider_id().to_string(),
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            Self::Tunnel(item) => {
                let rule = &item.forward.rule;
                if rule.forward_type == oxideterm_forwarding::ForwardType::Dynamic {
                    format!("SOCKS5 · {}:{}", rule.bind_address, rule.bind_port)
                } else {
                    format!(
                        "{}:{} → {}:{}",
                        rule.bind_address, rule.bind_port, rule.target_host, rule.target_port
                    )
                }
            }
            Self::Plugin(profile) => format!("{} · {}", profile.plugin_id, profile.type_id),
            Self::Connection(connection) => {
                format!(
                    "{}@{}:{}",
                    connection.username, connection.host, connection.port
                )
            }
            Self::SshConfig(host) => match host.user.as_deref() {
                Some(user) if !user.is_empty() => {
                    format!(
                        "{}@{}:{}",
                        user,
                        host.hostname.as_deref().unwrap_or(&host.alias),
                        host.port.unwrap_or(22)
                    )
                }
                _ => format!(
                    "{}:{}",
                    host.hostname.as_deref().unwrap_or(&host.alias),
                    host.port.unwrap_or(22)
                ),
            },
            Self::Serial(profile) => format!("{} · {}", profile.port_path, profile.baud_rate),
            Self::Ble(profile) => format!(
                "{} · {}",
                profile
                    .device_name
                    .clone()
                    .unwrap_or_else(|| profile.device_mac.clone().unwrap_or_default()),
                oxideterm_ble::service_display_name_from_str(&profile.service_uuid)
            ),
            Self::LocalTerminal(profile) => profile.cwd.clone().unwrap_or_default(),
            Self::Telnet(profile) => format!("{}:{}", profile.host, profile.port),
            Self::Ftp(profile) => format!("{}:{}", profile.host, profile.port),
            Self::Mosh(profile) => {
                format!("{}@{}:{}", profile.username, profile.host, profile.ssh_port)
            }
            Self::StandaloneSftp(profile) => {
                format!("{}@{}:{}", profile.username, profile.host, profile.port)
            }
            Self::RemoteDesktop(profile) => match profile.username.as_deref() {
                Some(username) if !username.is_empty() => {
                    format!("{username}@{}:{}", profile.host, profile.port)
                }
                _ => format!("{}:{}", profile.host, profile.port),
            },
        }
    }

    pub fn search_text(&self) -> String {
        match self {
            Self::Tunnel(item) => format!(
                "{}\n{}\n{}\n{}\nssh tunnel",
                item.profile.name,
                self.subtitle(),
                self.host(),
                item.profile.group.as_deref().unwrap_or_default()
            ),
            Self::Plugin(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile.plugin_id,
                profile.type_id,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::Connection(connection) => connection.search_text(),
            Self::SshConfig(host) => format!(
                "{}\n{}\n{}\n{}\nssh config",
                host.alias,
                host.hostname.as_deref().unwrap_or(&host.alias),
                host.port.unwrap_or(22),
                host.user.as_deref().unwrap_or_default()
            ),
            Self::Serial(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile.port_path,
                profile.baud_rate,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::Ble(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile
                    .device_name
                    .as_deref()
                    .or_else(|| profile.device_mac.as_deref())
                    .unwrap_or_default(),
                profile.service_uuid,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::LocalTerminal(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile.cwd.as_deref().unwrap_or_default(),
                profile.shell_id.as_deref().unwrap_or_default(),
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::Telnet(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile.host,
                profile.port,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::Ftp(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile.host,
                profile.port,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::Mosh(profile) => format!(
                "{}\n{}\n{}\n{}\nmosh",
                profile.name,
                profile.host,
                profile.username,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::StandaloneSftp(profile) => format!(
                "{}\n{}\n{}\n{}\n{}\nstandalone sftp",
                profile.name,
                profile.host,
                profile.username,
                profile.group.as_deref().unwrap_or_default(),
                profile.initial_remote_path.as_deref().unwrap_or_default()
            ),
            Self::RemoteDesktop(profile) => format!(
                "{}\n{}\n{}\n{}\n{}",
                profile.name,
                profile.protocol.provider_id(),
                profile.host,
                profile.port,
                profile.group.as_deref().unwrap_or_default()
            ),
        }
    }

    pub fn icon(&self) -> session_icons::SessionIcon {
        match self {
            Self::Tunnel(_) => LucideIcon::Network.into(),
            Self::Plugin(_) => LucideIcon::Puzzle.into(),
            Self::Connection(connection) => {
                session_icons::session_icon_from_id(connection.icon.as_deref())
                    .unwrap_or(LucideIcon::Server.into())
            }
            Self::SshConfig(_) => LucideIcon::FileTerminal.into(),
            Self::Serial(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::Radio.into()),
            Self::Ble(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::Bluetooth.into()),
            Self::LocalTerminal(profile) => {
                session_icons::session_icon_from_id(profile.icon.as_deref())
                    .unwrap_or(LucideIcon::Terminal.into())
            }
            Self::Telnet(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::Terminal.into()),
            Self::Ftp(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::FolderSync.into()),
            Self::Mosh(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::Wifi.into()),
            Self::StandaloneSftp(profile) => {
                session_icons::session_icon_from_id(profile.icon.as_deref())
                    .unwrap_or(LucideIcon::FolderSync.into())
            }
            Self::RemoteDesktop(profile) => {
                session_icons::session_icon_from_id(profile.icon.as_deref())
                    .unwrap_or(LucideIcon::Monitor.into())
            }
        }
    }

    pub fn icon_color(&self) -> Option<&str> {
        match self {
            Self::Tunnel(_) => None,
            Self::Plugin(_) => None,
            Self::Connection(connection) => connection.color.as_deref(),
            Self::Serial(profile) => profile.color.as_deref(),
            Self::Ble(profile) => profile.color.as_deref(),
            Self::LocalTerminal(profile) => profile.color.as_deref(),
            Self::Telnet(profile) => profile.color.as_deref(),
            Self::Ftp(profile) => profile.color.as_deref(),
            Self::Mosh(profile) => profile.color.as_deref(),
            Self::StandaloneSftp(profile) => profile.color.as_deref(),
            Self::RemoteDesktop(profile) => profile.color.as_deref(),
            Self::SshConfig(_) => None,
        }
    }

    pub fn icon_background_color(&self) -> Option<&str> {
        match self {
            Self::Tunnel(_) => None,
            Self::Plugin(_) => None,
            Self::Connection(connection) => connection.icon_background_color.as_deref(),
            Self::Serial(profile) => profile.icon_background_color.as_deref(),
            Self::Ble(profile) => profile.icon_background_color.as_deref(),
            Self::LocalTerminal(profile) => profile.icon_background_color.as_deref(),
            Self::Telnet(profile) => profile.icon_background_color.as_deref(),
            Self::Ftp(profile) => profile.icon_background_color.as_deref(),
            Self::Mosh(profile) => profile.icon_background_color.as_deref(),
            Self::StandaloneSftp(profile) => profile.icon_background_color.as_deref(),
            Self::RemoteDesktop(profile) => profile.icon_background_color.as_deref(),
            Self::SshConfig(_) => None,
        }
    }
}

pub fn saved_session_manager_items(
    store: &ConnectionStore,
    tunnels: Vec<PersistedForward>,
) -> Vec<SessionManagerDisplayItem> {
    let tunnel_hosts = store.connection_infos();
    store
        .connection_infos()
        .into_iter()
        .map(SessionManagerDisplayItem::Connection)
        .chain(tunnels.into_iter().filter_map(|forward| {
            let profile = forward.tunnel.clone()?;
            let host = tunnel_hosts
                .iter()
                .find(|host| Some(host.id.as_str()) == forward.owner_connection_id.as_deref());
            Some(SessionManagerDisplayItem::Tunnel(TunnelDisplayItem {
                forward,
                profile,
                host: host.cloned(),
            }))
        }))
        .chain(
            store
                .plugin_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::Plugin),
        )
        .chain(
            store
                .serial_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::Serial),
        )
        .chain(
            store
                .ble_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::Ble),
        )
        .chain(
            store
                .local_terminal_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::LocalTerminal),
        )
        .chain(
            store
                .telnet_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::Telnet),
        )
        .chain(
            store
                .ftp_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::Ftp),
        )
        .chain(
            store
                .mosh_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::Mosh),
        )
        .chain(
            store
                .standalone_sftp_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::StandaloneSftp),
        )
        .chain(
            store
                .remote_desktop_profiles()
                .iter()
                .cloned()
                .map(SessionManagerDisplayItem::RemoteDesktop),
        )
        .collect()
}

pub fn sort_session_manager_items(
    items: &mut [SessionManagerDisplayItem],
    field: SessionSortField,
    direction: SortDirection,
) {
    fn sort<K: Ord>(
        items: &mut [SessionManagerDisplayItem],
        direction: SortDirection,
        primary_key: impl Fn(&SessionManagerDisplayItem) -> K,
    ) {
        // Cache normalized text for this sort only; display settings and item
        // edits remain visible immediately without persistent invalidation state.
        let key = |item: &SessionManagerDisplayItem| {
            (
                primary_key(item),
                item.name().to_lowercase(),
                item.id().to_owned(),
            )
        };
        match direction {
            SortDirection::Asc => items.sort_by_cached_key(key),
            SortDirection::Desc => items.sort_by_cached_key(|item| std::cmp::Reverse(key(item))),
        }
    }
    match field {
        SessionSortField::Name => sort(items, direction, |_| ()),
        SessionSortField::Host => sort(items, direction, |item| item.host().to_lowercase()),
        SessionSortField::Port => sort(items, direction, SessionManagerDisplayItem::port_sort_key),
        SessionSortField::Username => sort(items, direction, |item| item.username().to_lowercase()),
        SessionSortField::AuthType => {
            sort(items, direction, SessionManagerDisplayItem::auth_sort_key)
        }
        SessionSortField::Group => sort(items, direction, |item| {
            item.group().unwrap_or_default().to_lowercase()
        }),
        SessionSortField::LastUsed => sort(items, direction, SessionManagerDisplayItem::last_used),
    }
}

pub fn session_manager_grid_rows(
    items: &[SessionManagerDisplayItem],
    roots: &[String],
    recent_title: String,
    hosts_title: String,
    card_columns: usize,
    recent_columns: usize,
) -> Vec<SessionManagerGridRow> {
    let mut rows = Vec::new();
    let recent_indices = recent_session_item_indices(items);
    push_session_manager_grid_section(
        &mut rows,
        recent_title,
        &recent_indices,
        recent_columns,
        true,
    );

    // Route items through their ancestor paths once instead of scanning all
    // connections separately for each root's subtree. Indices retain display order.
    let mut grouped_items: HashMap<&str, Vec<usize>> = roots
        .iter()
        .map(|root| (root.as_str(), Vec::new()))
        .collect();
    let mut host_indices = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut group = item.group();
        if roots.is_empty() || group.is_none() {
            host_indices.push(index);
            continue;
        }
        while let Some(path) = group {
            if let Some(indices) = grouped_items.get_mut(path) {
                indices.push(index);
            }
            group = path.rsplit_once('/').map(|(parent, _)| parent);
        }
    }

    // Grid mode keeps each root group as one section containing its subtree.
    for group in roots {
        let group_indices = grouped_items
            .get(group.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        push_session_manager_grid_section(
            &mut rows,
            group_display_name(group),
            group_indices,
            card_columns,
            false,
        );
    }

    push_session_manager_grid_section(&mut rows, hosts_title, &host_indices, card_columns, false);
    rows
}

fn push_session_manager_grid_section(
    rows: &mut Vec<SessionManagerGridRow>,
    title: String,
    item_indices: &[usize],
    columns: usize,
    recent: bool,
) {
    if item_indices.is_empty() {
        return;
    }
    rows.push(SessionManagerGridRow::SectionHeader {
        title,
        item_count: item_indices.len(),
    });
    let chunk_count = item_indices.len().div_ceil(columns.max(1));
    for (row_index, chunk) in item_indices.chunks(columns.max(1)).enumerate() {
        if recent {
            rows.push(SessionManagerGridRow::RecentItems {
                item_indices: chunk.to_vec(),
                is_last_in_section: row_index + 1 == chunk_count,
            });
        } else {
            rows.push(SessionManagerGridRow::Cards {
                item_indices: chunk.to_vec(),
            });
        }
    }
}

pub fn session_manager_tree_rows(
    items: &[SessionManagerDisplayItem],
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
) -> Vec<SessionManagerTreeRow> {
    // Index direct membership once; scanning every connection for each visible
    // group makes a tree repaint quadratic as saved connections accumulate.
    let mut grouped_items: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        grouped_items.entry(item.group()).or_default().push(index);
    }
    let mut rows = Vec::new();
    for root in roots {
        push_session_manager_tree_group_rows(
            &mut rows,
            root,
            0,
            &grouped_items,
            children,
            expanded_groups,
        );
    }
    rows.extend(
        grouped_items
            .remove(&None)
            .unwrap_or_default()
            .into_iter()
            .map(|item_index| SessionManagerTreeRow::Item {
                item_index,
                depth: 0,
            }),
    );
    rows
}

/// Selectable tree rows in render order, used by Shift+click range selection.
pub fn session_manager_tree_visible_targets(
    rows: &[SessionManagerTreeRow],
    items: &[SessionManagerDisplayItem],
) -> Vec<SessionManagerSelectionTarget> {
    rows.iter()
        .filter_map(|row| match row {
            SessionManagerTreeRow::Item { item_index, .. } => items
                .get(*item_index)
                .and_then(|item| item.selection_target()),
            SessionManagerTreeRow::Group { .. } => None,
        })
        .collect()
}

/// Selectable grid cards (and recent-strip items) in render order.
pub fn session_manager_grid_visible_targets(
    rows: &[SessionManagerGridRow],
    items: &[SessionManagerDisplayItem],
) -> Vec<SessionManagerSelectionTarget> {
    rows.iter()
        .flat_map(|row| match row {
            SessionManagerGridRow::SectionHeader { .. } => Vec::new(),
            SessionManagerGridRow::RecentItems { item_indices, .. }
            | SessionManagerGridRow::Cards { item_indices } => item_indices
                .iter()
                .filter_map(|index| items.get(*index).and_then(|item| item.selection_target()))
                .collect::<Vec<_>>(),
        })
        .collect()
}

fn push_session_manager_tree_group_rows(
    rows: &mut Vec<SessionManagerTreeRow>,
    group: &str,
    depth: usize,
    grouped_items: &HashMap<Option<&str>, Vec<usize>>,
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
) {
    let group_item_indices = grouped_items
        .get(&Some(group))
        .map(Vec::as_slice)
        .unwrap_or_default();
    let child_groups = children.get(group).map(Vec::as_slice).unwrap_or_default();
    let expanded = expanded_groups.contains(group);
    rows.push(SessionManagerTreeRow::Group {
        path: group.to_string(),
        depth,
        expanded,
        has_children: !child_groups.is_empty() || !group_item_indices.is_empty(),
    });
    if !expanded {
        return;
    }

    for child_group in child_groups {
        push_session_manager_tree_group_rows(
            rows,
            child_group,
            depth + 1,
            grouped_items,
            children,
            expanded_groups,
        );
    }
    rows.extend(
        group_item_indices
            .iter()
            .copied()
            .map(|item_index| SessionManagerTreeRow::Item {
                item_index,
                depth: depth + 1,
            }),
    );
}

fn recent_session_item_indices(items: &[SessionManagerDisplayItem]) -> Vec<usize> {
    const RECENT_SESSION_LIMIT: usize = 8;
    let mut candidates: Vec<_> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            item.last_used()
                .map(|time| (std::cmp::Reverse(time), index))
        })
        .collect();
    // Only the recent strip needs ordering. The original index breaks ties so
    // partial selection preserves the display order for equal timestamps.
    if candidates.len() > RECENT_SESSION_LIMIT {
        candidates.select_nth_unstable(RECENT_SESSION_LIMIT);
        candidates.truncate(RECENT_SESSION_LIMIT);
    }
    candidates.sort_unstable();
    candidates.into_iter().map(|(_, index)| index).collect()
}

pub fn session_manager_display_item_signature(item: &SessionManagerDisplayItem) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(item).hash(&mut hasher);
    item.id().hash(&mut hasher);
    item.name().hash(&mut hasher);
    item.group().hash(&mut hasher);
    item.subtitle().hash(&mut hasher);
    item.last_used().hash(&mut hasher);
    item.icon_color().hash(&mut hasher);
    item.icon_background_color().hash(&mut hasher);
    hasher.finish()
}

pub fn session_manager_grid_row_signature(
    row: &SessionManagerGridRow,
    items: &[SessionManagerDisplayItem],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        SessionManagerGridRow::SectionHeader { title, item_count } => {
            0_u8.hash(&mut hasher);
            title.hash(&mut hasher);
            item_count.hash(&mut hasher);
        }
        SessionManagerGridRow::RecentItems {
            item_indices,
            is_last_in_section,
        } => {
            1_u8.hash(&mut hasher);
            is_last_in_section.hash(&mut hasher);
            hash_session_manager_row_items(item_indices, items, &mut hasher);
        }
        SessionManagerGridRow::Cards { item_indices } => {
            2_u8.hash(&mut hasher);
            hash_session_manager_row_items(item_indices, items, &mut hasher);
        }
    }
    hasher.finish()
}

pub fn session_manager_tree_row_signature(
    row: &SessionManagerTreeRow,
    items: &[SessionManagerDisplayItem],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        SessionManagerTreeRow::Group {
            path,
            depth,
            expanded,
            has_children,
        } => {
            0_u8.hash(&mut hasher);
            path.hash(&mut hasher);
            depth.hash(&mut hasher);
            expanded.hash(&mut hasher);
            has_children.hash(&mut hasher);
        }
        SessionManagerTreeRow::Item { item_index, depth } => {
            1_u8.hash(&mut hasher);
            depth.hash(&mut hasher);
            if let Some(item) = items.get(*item_index) {
                session_manager_display_item_signature(item).hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn hash_session_manager_row_items(
    item_indices: &[usize],
    items: &[SessionManagerDisplayItem],
    hasher: &mut DefaultHasher,
) {
    for item_index in item_indices {
        if let Some(item) = items.get(*item_index) {
            session_manager_display_item_signature(item).hash(hasher);
        }
    }
}

pub fn group_display_name(group: &str) -> String {
    group.rsplit('/').next().unwrap_or(group).to_string()
}

pub fn collect_session_group_paths(
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    output: &mut HashSet<String>,
) {
    for root in roots {
        output.insert(root.clone());
        if let Some(child_groups) = children.get(root) {
            collect_session_group_paths(child_groups, children, output);
        }
    }
}
