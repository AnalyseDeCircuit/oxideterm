// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::{
    CatalogEntry, CatalogRow, SessionManagerGridRow, SessionManagerOpenTarget,
    SessionManagerRowActionTarget, SessionManagerSelectionTarget, SessionManagerTreeRow,
    SessionSortField, SortDirection, auth_label,
};
use crate::icons as session_icons;
use oxideterm_connections::{
    BleProfile, ConnectionInfo, ConnectionStore, LocalTerminalProfile, MoshProfile,
    RemoteDesktopProfile, SerialProfile, SshConfigHost, TelnetProfile,
};
use oxideterm_forwarding::PersistedForward;
use oxideterm_gpui_ui::icons::LucideIcon;
use std::collections::{HashMap, HashSet};
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
    Discovery(oxideterm_discovery::CatalogEntry),
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
        let items = saved_session_manager_items(store, tunnels)
            .into_iter()
            .chain(
                self.ssh_config_hosts
                    .iter()
                    .filter(|host| !host.already_imported)
                    .map(SessionManagerSshConfigDisplayItem::from)
                    .map(SessionManagerDisplayItem::SshConfig),
            )
            .chain(
                self.discovery_entries
                    .iter()
                    .cloned()
                    .map(SessionManagerDisplayItem::Discovery),
            );
        super::catalog::filtered_catalog(items, self)
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

impl SessionManagerDisplayItem {
    pub fn id(&self) -> &str {
        match self {
            Self::Tunnel(item) => &item.forward.id,
            Self::Connection(connection) => &connection.id,
            Self::SshConfig(host) => &host.alias,
            Self::Discovery(host) => &host.row_id,
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
        // Only saved profiles participate in batch edits.
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
            Self::Discovery(_) => None,
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
            Self::Discovery(host) => Some(SessionManagerRowActionTarget::Discovery(host.key())),
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
            Self::Discovery(host) => SessionManagerOpenTarget::Discovery(host.key()),
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
            Self::Discovery(host) => &host.label,
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
            Self::Discovery(_) => None,
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
            Self::Discovery(_) => None,
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
            Self::Discovery(host) => host.scope.as_deref().unwrap_or_default(),
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
            Self::Discovery(_) => 0,
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
            Self::Discovery(_) => "",
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
            Self::Discovery(host) => host.source_name.clone(),
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
            Self::Discovery(host) => match &host.detail {
                Some(detail) => format!("{detail} · {}", host.subtitle()),
                None => host.subtitle(),
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
            Self::Discovery(host) => host.search_text(),
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
            Self::Discovery(_) => LucideIcon::Server.into(),
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
            Self::Discovery(_) => None,
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
            Self::Discovery(_) => None,
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

impl CatalogEntry for SessionManagerDisplayItem {
    fn group_paths(&self) -> &[Vec<oxideterm_discovery::DiscoveryGroupSegment>] {
        match self {
            Self::Discovery(entry) => &entry.group_paths,
            _ => &[],
        }
    }
    fn discovery_source(&self) -> Option<&str> {
        match self {
            Self::Discovery(entry) => Some(&entry.source_id),
            _ => None,
        }
    }
    fn id(&self) -> &str {
        self.id()
    }
    fn name(&self) -> &str {
        self.name()
    }
    fn group(&self) -> Option<&str> {
        self.group()
    }
    fn last_used(&self) -> Option<String> {
        self.last_used()
    }
    fn host(&self) -> &str {
        self.host()
    }
    fn port_sort_key(&self) -> u32 {
        self.port_sort_key()
    }
    fn username(&self) -> &str {
        self.username()
    }
    fn auth_sort_key(&self) -> String {
        self.auth_sort_key()
    }
    fn subtitle(&self) -> String {
        self.subtitle()
    }
    fn icon_color(&self) -> Option<&str> {
        self.icon_color()
    }
    fn icon_background_color(&self) -> Option<&str> {
        self.icon_background_color()
    }
    fn selection_target(&self) -> Option<SessionManagerSelectionTarget> {
        self.selection_target()
    }
    fn hash_appearance(&self, hasher: &mut std::collections::hash_map::DefaultHasher) {
        use std::hash::Hash;
        std::mem::discriminant(self).hash(hasher);
    }
    fn matches_query(&self, query: &str) -> bool {
        self.search_text().to_lowercase().contains(query)
    }
}

impl From<SessionManagerDisplayItem> for CatalogRow {
    fn from(item: SessionManagerDisplayItem) -> Self {
        let discovery_row_id = match &item {
            SessionManagerDisplayItem::Discovery(entry) => entry.row_id.clone(),
            _ => String::new(),
        };
        Self {
            discovery_row_id,
            group_paths: item.group_paths().to_vec(),
            tags: match &item {
                SessionManagerDisplayItem::Connection(connection) => connection.tags.clone(),
                SessionManagerDisplayItem::Discovery(entry) => entry.tags.clone(),
                _ => Vec::new(),
            },
            protocol_badge: None,
            active: false,
            target: item.open_target(),
            name: item.name().to_owned(),
            group: item.group().map(str::to_owned),
            subtitle: item.subtitle(),
            host: item.host().to_owned(),
            port: item.port_sort_key(),
            username: item.username().to_owned(),
            auth_label: item.auth_sort_key(),
            last_used: item.last_used(),
            search_text: item.search_text(),
            icon: item.icon(),
            color: item.icon_color().map(str::to_owned),
            icon_background_color: item.icon_background_color().map(str::to_owned),
        }
    }
}

// These entry points also serve native callers that consume the concrete profile variants.
pub fn sort_session_manager_items(
    items: &mut [SessionManagerDisplayItem],
    field: SessionSortField,
    direction: SortDirection,
) {
    super::catalog::sort_session_manager_items(items, field, direction)
}

pub fn session_manager_grid_rows(
    items: &[SessionManagerDisplayItem],
    roots: &[String],
    recent_title: String,
    hosts_title: String,
    card_columns: usize,
    recent_columns: usize,
) -> Vec<SessionManagerGridRow> {
    super::catalog::session_manager_grid_rows(
        items,
        roots,
        recent_title,
        hosts_title,
        card_columns,
        recent_columns,
    )
}

pub fn session_manager_tree_rows(
    items: &[SessionManagerDisplayItem],
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
    search_active: bool,
) -> Vec<SessionManagerTreeRow> {
    super::catalog::session_manager_tree_rows(
        items,
        roots,
        children,
        expanded_groups,
        search_active,
    )
}

pub fn session_manager_tree_visible_targets(
    rows: &[SessionManagerTreeRow],
    items: &[SessionManagerDisplayItem],
) -> Vec<SessionManagerSelectionTarget> {
    super::catalog::session_manager_tree_visible_targets(rows, items)
}

pub fn session_manager_grid_visible_targets(
    rows: &[SessionManagerGridRow],
    items: &[SessionManagerDisplayItem],
) -> Vec<SessionManagerSelectionTarget> {
    super::catalog::session_manager_grid_visible_targets(rows, items)
}

pub fn session_manager_display_item_signature(item: &SessionManagerDisplayItem) -> u64 {
    super::catalog::session_manager_display_item_signature(item)
}

pub fn session_manager_grid_row_signature(
    row: &SessionManagerGridRow,
    items: &[SessionManagerDisplayItem],
) -> u64 {
    super::catalog::session_manager_grid_row_signature(row, items)
}

pub fn session_manager_tree_row_signature(
    row: &SessionManagerTreeRow,
    items: &[SessionManagerDisplayItem],
) -> u64 {
    super::catalog::session_manager_tree_row_signature(row, items)
}
