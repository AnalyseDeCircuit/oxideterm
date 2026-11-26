// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod dispatch;
pub use dispatch::*;

use gpui::Context;
use oxideterm_ble::BleAdapterGuard;
use oxideterm_connections::{
    ConnectionStore, ConnectionTerminalOptions, FtpProfile, FtpSecurity, RemoteDesktopProfile,
    SaveBleProfileRequest, SaveFtpProfileRequest, SaveMoshProfileRequest,
    SaveRemoteDesktopProfileRequest, SaveSerialProfileRequest, SaveStandaloneSftpProfileRequest,
    SaveTelnetProfileRequest, SavedProxyHop, SavedUpstreamProxyAuth, SavedUpstreamProxyPolicy,
    SecretString, StandaloneSftpTransferMode,
};
use oxideterm_i18n::I18n;
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopProtocol,
};
use oxideterm_session_adapter::{
    proxy_chain_config_from_saved_connection, ssh_config_from_saved_connection_with_auth,
    ssh_config_from_saved_connection_with_runtime_secrets,
};
use oxideterm_settings::PersistedSettings;
use oxideterm_ssh::{AuthMethod, SshConfig};
use oxideterm_terminal::{SerialSessionConfig, TelnetSessionConfig};
use std::time::Duration;
use zeroize::Zeroize;

use crate::{
    ble::{BleOpenRequest, BleStandaloneTarget, BleTraceTarget},
    configuration::{
        RuntimeSecretHandoff, save_request_from_form_with_existing_auth,
        save_request_from_form_with_proxy_hop_prefix, saved_upstream_proxy_policy_from_form,
    },
    conversion::*,
    entity::ConnectionFlowEntity,
    form_state::{
        NewConnectionForm, NewConnectionSubmitAction, SshAuthTab, identity_agent_from_form,
        terminal_serial_line_ending_from_profile, terminal_serial_runtime_options_from_ble_form,
        terminal_serial_runtime_options_from_profile,
    },
    launch::{MoshConnectionOptions, SshConnectionIntent, SshTerminalConnectionOptions},
    profiles::*,
};

pub struct SavedPromptLaunch {
    pub id: String,
    pub config: SshConfig,
    pub title: String,
    pub action: crate::SavedConnectionPromptAction,
    pub open_sftp: bool,
    pub auth_to_save: Option<crate::saved_auth::SavedAuthSaveTarget>,
    pub remaining_tunnels: std::collections::VecDeque<(String, Vec<String>)>,
}

pub struct SavedSshEdit {
    pub id: String,
    pub reconnect_node: Option<oxideterm_ssh::NodeId>,
}

pub enum SavedSftpOpen {
    Existing {
        title: String,
        primary: (String, Option<String>),
        secondary: Option<(String, Option<String>)>,
    },
    Preflight(SshFormOpen),
}

impl ConnectionFlowEntity {
    pub fn prepare_saved_sftp_open(
        &mut self,
        id: &str,
        store: &ConnectionStore,
        settings: &PersistedSettings,
        existing: impl Fn(&str) -> bool,
        i18n: &I18n,
    ) -> Result<Option<SavedSftpOpen>, String> {
        let Some(profile) = store.get_standalone_sftp_profile(id) else {
            return Ok(None);
        };
        if existing(id) {
            let secondary_id = format!("{id}:secondary");
            let secondary = (profile.transfer_mode == StandaloneSftpTransferMode::RemoteRemote
                && existing(&secondary_id))
            .then(|| {
                (
                    secondary_id,
                    profile
                        .secondary_endpoint
                        .as_ref()
                        .and_then(|endpoint| endpoint.initial_remote_path.clone()),
                )
            });
            return Ok(Some(SavedSftpOpen::Existing {
                title: profile.name.clone(),
                primary: (id.to_owned(), profile.initial_remote_path.clone()),
                secondary,
            }));
        }
        let secrets = store
            .load_standalone_sftp_profile_runtime_secrets(id)
            .map_err(|error| error.to_string())?;
        let (config, intent) = self
            .prepare_standalone_sftp_launch(store, settings, profile, secrets, None, None)
            .ok_or_else(|| i18n.t("sftp.standalone.missing_credentials"))?;
        Ok(Some(SavedSftpOpen::Preflight((
            config,
            profile.name.clone(),
            intent,
        ))))
    }
}

impl ConnectionFlowEntity {
    pub fn default_submit_action(&self) -> NewConnectionSubmitAction {
        if self
            .form
            .form
            .as_ref()
            .is_some_and(|form| form.standalone_connection_id.is_some())
        {
            NewConnectionSubmitAction::Connect
        } else {
            NewConnectionSubmitAction::SaveAndConnect
        }
    }

    fn plan_submission(
        &mut self,
        action: NewConnectionSubmitAction,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<FormSubmission> {
        let plan = plan_form_submission(&mut self.form, action, i18n);
        cx.notify();
        plan
    }

    fn persist_ssh(
        &mut self,
        store: &mut ConnectionStore,
        prefix: anyhow::Result<Vec<crate::form_state::NewConnectionProxyHop>>,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<SavedConnectionRuntimeHandoff> {
        let handoff = persist_ssh_form(self.form.form.as_mut()?, store, prefix, i18n);
        cx.notify();
        handoff
    }

    pub fn arm_reconnect_after_edit(
        &mut self,
        connection_id: &str,
        node_id: oxideterm_ssh::NodeId,
        cx: &mut Context<Self>,
    ) {
        if self.form.editing_saved_connection_id.as_deref() == Some(connection_id) {
            self.form
                .editing_saved_connection_connect_after_save_node_id = Some(node_id);
            cx.notify();
        }
    }

    pub fn begin_connection_form(
        &mut self,
        transport: crate::form_state::NewConnectionTransport,
        parent: Option<oxideterm_ssh::NodeId>,
        settings: &PersistedSettings,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) {
        use crate::NewConnectionField as Field;
        use crate::form_state::{
            NewConnectionTransport, NewConnectionUpstreamProxyPolicy, TELNET_DEFAULT_PORT_TEXT,
        };
        let mut form = NewConnectionForm::default();
        form.transport = transport;
        form.group = i18n.t("ssh.form.ungrouped");
        form.save_connection = settings.new_connection.save_connection;
        form.agent_available = detect_ssh_agent_available(&form.identity_agent);
        match transport {
            NewConnectionTransport::Serial => {
                form.focused_field = Field::SerialPortPath;
                form.field_focused = false;
            }
            NewConnectionTransport::Telnet => {
                form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Direct;
                form.port = TELNET_DEFAULT_PORT_TEXT.to_owned();
                form.focused_field = Field::Host;
                form.field_focused = false;
            }
            _ => {}
        }
        if parent.is_some() {
            form.auth_tab = SshAuthTab::Agent;
            form.focused_field = Field::Host;
            form.save_connection = false;
            form.username.clear();
        }
        self.form.replace_with_new_form(form);
        self.form.drill_down_parent_node_id = parent;
        cx.notify();
    }

    pub fn open_saved_form(
        &mut self,
        store: &ConnectionStore,
        id: &str,
        prompt: Option<crate::SavedConnectionPromptAction>,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(connection) = store.get(id) else {
            return false;
        };
        let mut form = crate::configuration::form_from_saved_connection(connection, error);
        if prompt.is_some() {
            form.save_password = false;
        }
        crate::configuration::restore_legacy_jump_host_in_form(&mut form, connection, store);
        self.form.replace_with_new_form(form);
        self.form.editing_saved_connection_id = Some(id.to_owned());
        self.form.saved_connection_prompt_action = prompt;
        cx.notify();
        true
    }

    fn submit_saved_prompt(
        &mut self,
        store: &ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<SavedPromptLaunch> {
        use crate::SavedConnectionPromptAction::{Connect, Test, Tunnel};
        let action = self.form.saved_connection_prompt_action?;
        let id = self.form.editing_saved_connection_id.clone()?;
        let form = self.form.form.as_mut()?;
        let open_sftp = form.open_sftp_on_connect;
        let connection = store.get(&id);
        let auth_to_save = (form.save_password && action != Test)
            .then_some(connection)
            .flatten()
            .map(crate::saved_auth::SavedAuthSaveTarget::new);
        let handoff = if action == Test {
            RuntimeSecretHandoff::CopyForTest
        } else {
            RuntimeSecretHandoff::Move
        };
        let (mut config, title) = match build_ssh_config(form, store, settings, i18n, handoff) {
            Ok(value) => value,
            Err(message) => {
                self.set_form_feedback(None, Some(message), cx);
                return None;
            }
        };
        if let Some(connection) = connection {
            if config.proxy_command.is_none()
                && let Some(saved) = ssh_config_from_saved_connection_with_auth(
                    store,
                    settings,
                    connection,
                    Some(AuthMethod::Agent),
                )
                && saved.proxy_command.is_some()
            {
                // Supplying credentials must leave an imported ProxyCommand route in place.
                config.proxy_command = saved.proxy_command;
                config.proxy_chain = None;
                config.upstream_proxy = None;
            }
            if config.proxy_chain.is_none()
                && config.proxy_command.is_none()
                && let Some(chain) = proxy_chain_config_from_saved_connection(store, connection)
                && !chain.is_empty()
            {
                config.proxy_chain = Some(chain);
                config.strict_host_key_checking = true;
            }
        }
        let remaining_tunnels = match action {
            Tunnel => {
                let groups = std::mem::take(&mut self.form.tunnel_start_groups);
                self.clear_form_after_submit(cx);
                groups
            }
            Connect => {
                self.set_form_feedback(Some(true), Some(i18n.t("ssh.form.checking_host_key")), cx);
                Default::default()
            }
            Test => Default::default(),
        };
        Some(SavedPromptLaunch {
            id,
            config,
            title,
            action,
            open_sftp,
            auth_to_save,
            remaining_tunnels,
        })
    }

    fn submit_ssh_edit(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        duplicate: bool,
        cx: &mut Context<Self>,
    ) -> Option<SavedSshEdit> {
        let source = if duplicate {
            self.form.duplicating_saved_connection_id.as_ref()
        } else {
            self.form.editing_saved_connection_id.as_ref()
        }?
        .clone();
        let form = self.form.form.as_mut()?;
        if !duplicate && store.get(&source).is_none() {
            return None;
        }
        match prepare_ssh_profile_update(form, store, &source, duplicate, i18n)
            .and_then(|request| Ok(store.upsert(request)?))
        {
            Ok(saved) => {
                let reconnect_node = self
                    .form
                    .editing_saved_connection_connect_after_save_node_id
                    .take();
                self.clear_form_after_submit(cx);
                Some(SavedSshEdit {
                    id: saved.id,
                    reconnect_node,
                })
            }
            Err(error) => {
                self.set_form_feedback(None, Some(error.to_string()), cx);
                None
            }
        }
    }

    fn launch_persisted_ssh_form(
        &mut self,
        store: &ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        handoff: SavedConnectionRuntimeHandoff,
        parent: Option<(oxideterm_ssh::NodeId, usize)>,
        cx: &mut Context<Self>,
    ) -> Result<SshFormOpen, String> {
        let missing = || i18n.t("modals.new_connection.save_failed");
        let connection = store.get(&handoff.connection_id).ok_or_else(missing)?;
        let mut config = ssh_config_from_saved_connection_with_runtime_secrets(
            store,
            settings,
            connection,
            handoff.secrets,
            handoff.auth_override,
        )
        .ok_or_else(missing)?;
        let intent = if let Some((parent_id, prefix_count)) = parent {
            if prefix_count > 0 {
                let chain = config
                    .proxy_chain
                    .as_mut()
                    .filter(|chain| chain.len() >= prefix_count)
                    .ok_or_else(missing)?;
                // The live ancestors already own the persisted prefix.
                chain.drain(..prefix_count);
                if chain.is_empty() {
                    config.proxy_chain = None;
                }
            }
            SshConnectionIntent::DrillDown {
                parent_id,
                saved_connection_id: Some(connection.id.clone()),
                terminal_options: SshTerminalConnectionOptions {
                    terminal: connection.options.terminal.clone(),
                    dedicated_new_terminal_connection: connection
                        .options
                        .dedicated_new_terminal_connection,
                    open_sftp_on_connect: connection.options.open_sftp_on_connect,
                    ssh_channel_strategy: connection.options.ssh_channel_strategy,
                },
            }
        } else {
            SshConnectionIntent::ConnectSaved {
                id: connection.id.clone(),
                auth_save_target: None,
                open_sftp_on_connect: connection.options.open_sftp_on_connect,
            }
        };
        if let Some(form) = self.form.form.as_mut() {
            form.save_connection = false;
        }
        cx.notify();
        Ok((config, connection.name.clone(), intent))
    }
}

pub struct ProfileSubmission<T> {
    pub open: Option<T>,
    pub persisted: bool,
}

pub struct SerialFormOpen {
    pub config: SerialSessionConfig,
    pub title: String,
    pub terminal: ConnectionTerminalOptions,
    pub edited_profile: Option<SaveSerialProfileRequest>,
}

pub struct TelnetFormOpen {
    pub config: TelnetSessionConfig,
    pub proxy: SavedUpstreamProxyPolicy,
    pub terminal: ConnectionTerminalOptions,
    pub profile_id: Option<String>,
    pub edited_profile: Option<SaveTelnetProfileRequest>,
}

pub struct DesktopFormOpen {
    pub profile: RemoteDesktopConnectionProfile,
    pub password: Option<SecretString>,
    pub gateway: Option<String>,
    pub reconnect_id: Option<String>,
}

pub type SshFormOpen = (SshConfig, String, SshConnectionIntent);

impl ConnectionFlowEntity {
    fn profile_error(
        &mut self,
        i18n: &I18n,
        key: &str,
        error: impl std::fmt::Display,
        cx: &mut Context<Self>,
    ) {
        self.set_form_feedback(Some(false), Some(format!("{}: {error}", i18n.t(key))), cx);
    }

    fn finish_profile_save<T>(
        &mut self,
        action: NewConnectionSubmitAction,
        persisted: bool,
        open: impl FnOnce() -> T,
        cx: &mut Context<Self>,
    ) -> ProfileSubmission<T> {
        if action == NewConnectionSubmitAction::Save {
            self.clear_form_after_submit(cx);
            ProfileSubmission {
                open: None,
                persisted,
            }
        } else {
            ProfileSubmission {
                open: Some(open()),
                persisted,
            }
        }
    }

    fn submit_serial(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        action: NewConnectionSubmitAction,
        cx: &mut Context<Self>,
    ) -> Option<ProfileSubmission<SerialFormOpen>> {
        let prepared = prepare_serial_form(self.form.form.as_mut()?, store, i18n, action);
        cx.notify();
        let (config, title, terminal, mut edited_profile) = prepared?;
        let mut persisted = false;
        if action != NewConnectionSubmitAction::Connect
            && let Some(request) = edited_profile.take()
        {
            if let Err(error) = store.upsert_serial_profile(request) {
                self.profile_error(i18n, "modals.new_connection.serial_save_failed", error, cx);
                return None;
            }
            persisted = true;
        }
        Some(self.finish_profile_save(
            action,
            persisted,
            || SerialFormOpen {
                config,
                title,
                terminal,
                edited_profile,
            },
            cx,
        ))
    }

    fn submit_telnet(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        action: NewConnectionSubmitAction,
        cx: &mut Context<Self>,
    ) -> Option<ProfileSubmission<TelnetFormOpen>> {
        let prepared = prepare_telnet_form(self.form.form.as_mut()?, store, i18n, action);
        cx.notify();
        let (config, proxy, terminal, mut edited_profile) = prepared?;
        let mut profile_id = None;
        if action != NewConnectionSubmitAction::Connect
            && let Some(request) = edited_profile.take()
        {
            match store.upsert_telnet_profile(request) {
                Ok(profile) => profile_id = Some(profile.id),
                Err(error) => {
                    self.profile_error(i18n, "modals.new_connection.telnet_save_failed", error, cx);
                    return None;
                }
            }
        }
        Some(self.finish_profile_save(
            action,
            profile_id.is_some(),
            || TelnetFormOpen {
                config,
                proxy,
                terminal,
                profile_id,
                edited_profile,
            },
            cx,
        ))
    }

    fn submit_ble(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        action: NewConnectionSubmitAction,
        cx: &mut Context<Self>,
    ) -> Option<ProfileSubmission<BleOpenRequest>> {
        let prepared = prepare_ble_form(self.form.form.as_mut()?, store, i18n, action);
        cx.notify();
        let (open, request) = prepared?;
        let mut profile_id = None;
        if action != NewConnectionSubmitAction::Connect
            && let Some(request) = request
        {
            match store.upsert_ble_profile(request) {
                Ok(profile) => profile_id = Some(profile.id),
                Err(error) => {
                    self.profile_error(i18n, "modals.new_connection.ble_save_failed", error, cx);
                    return None;
                }
            }
        }
        Some(self.finish_profile_save(
            action,
            profile_id.is_some(),
            || BleOpenRequest {
                target: BleTraceTarget {
                    node_id: format!(
                            "ble-form:{}",
                            open.device_mac
                                .as_deref()
                                .or(open.device_name.as_deref())
                                .unwrap_or("")
                        ),
                    label: open.label,
                    endpoint: open.device_name.clone().or_else(|| open.device_mac.clone()),
                },
                adapter_guard: open.adapter_guard,
                selected_device: open.selected_device,
                adapter_mac: open.adapter_mac,
                device_mac: open.device_mac,
                device_name: open.device_name,
                platform_identifier: open.platform_identifier,
                service_uuid: open.service_uuid,
                title: open.title,
                terminal_options: open.terminal_options,
                runtime_options: open.runtime_options,
                mtu: open.mtu,
                write_without_response: open.write_without_response,
                pre_connect_command: open.pre_connect_command,
                profile_id,
                form_id: Some(open.form_id),
                standalone: BleStandaloneTarget::New,
            },
            cx,
        ))
    }

    fn submit_mosh(
        &mut self,
        store: &mut ConnectionStore,
        i18n: &I18n,
        action: NewConnectionSubmitAction,
        cx: &mut Context<Self>,
    ) -> Option<ProfileSubmission<PreparedMoshConnect>> {
        let prepared = prepare_mosh_form(self.form.form.as_mut()?, store, i18n, action);
        cx.notify();
        let (direct, request, auth_override) = prepared?;
        if let Some(open) = direct {
            return Some(ProfileSubmission {
                open: Some(open),
                persisted: false,
            });
        }
        let (profile, secrets) = match store.upsert_mosh_profile_with_runtime_secrets(request?) {
            Ok(saved) => saved,
            Err(error) => {
                self.profile_error(i18n, "mosh.form.save_failed", error, cx);
                return None;
            }
        };
        if action == NewConnectionSubmitAction::Save {
            self.clear_form_after_submit(cx);
            return Some(ProfileSubmission {
                open: None,
                persisted: true,
            });
        }
        let Some(config) = runtime_mosh_config_from_saved(store, &profile, secrets, auth_override)
        else {
            self.set_form_feedback(
                Some(false),
                Some(i18n.t("mosh.form.missing_credentials")),
                cx,
            );
            return Some(ProfileSubmission {
                open: None,
                persisted: true,
            });
        };
        self.set_form_feedback(None, Some(i18n.t("ssh.form.checking_host_key")), cx);
        let options = mosh_options_from_profile(&profile);
        Some(ProfileSubmission {
            open: Some(PreparedMoshConnect {
                config,
                title: profile.name,
                options,
            }),
            persisted: true,
        })
    }

    fn submit_saved_sftp(
        &mut self,
        store: &mut ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        action: NewConnectionSubmitAction,
        cx: &mut Context<Self>,
    ) -> Option<ProfileSubmission<SshFormOpen>> {
        let prepared = prepare_standalone_sftp_form(self.form.form.as_mut()?, store, i18n, action);
        cx.notify();
        let (request, auth, secondary_auth) = prepared?;
        let (profile, secrets) =
            match store.upsert_standalone_sftp_profile_with_runtime_secrets(request) {
                Ok(saved) => saved,
                Err(error) => {
                    self.profile_error(i18n, "sftp.standalone.save_failed", error, cx);
                    return None;
                }
            };
        if action == NewConnectionSubmitAction::Save {
            self.clear_form_after_submit(cx);
            return Some(ProfileSubmission {
                open: None,
                persisted: true,
            });
        }
        let open = self
            .prepare_standalone_sftp_launch(
                store,
                settings,
                &profile,
                secrets,
                auth,
                secondary_auth,
            )
            .map(|(config, intent)| (config, profile.name, intent));
        if open.is_none() {
            self.set_form_feedback(
                Some(false),
                Some(i18n.t("sftp.standalone.missing_credentials")),
                cx,
            );
        }
        Some(ProfileSubmission {
            open,
            persisted: true,
        })
    }

    fn submit_desktop(
        &mut self,
        store: &mut ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        action: NewConnectionSubmitAction,
        reconnect_profile: Option<&RemoteDesktopProfile>,
        cx: &mut Context<Self>,
    ) -> Option<ProfileSubmission<DesktopFormOpen>> {
        let form = self.form.form.as_mut()?;
        let reconnect_id = form.standalone_connection_id.clone();
        let prepared =
            prepare_remote_desktop_form(form, store, i18n, action, settings, reconnect_profile);
        cx.notify();
        let (mut profile, request, mut password, gateway) = prepared?;
        let mut persisted = false;
        if let Some(request) = request {
            match store.upsert_remote_desktop_profile(request) {
                Ok(saved) => {
                    profile.id = saved.id;
                    profile.label = saved.name;
                    profile.credential_ref = saved.credential_ref;
                    persisted = true;
                }
                Err(error) => {
                    self.profile_error(
                        i18n,
                        "modals.new_connection.remote_desktop_save_failed",
                        error,
                        cx,
                    );
                    return None;
                }
            }
            if action != NewConnectionSubmitAction::Save && password.is_none() {
                match store.get_remote_desktop_credential(&profile.id) {
                    Ok(credential) => password = credential,
                    Err(error) => {
                        self.profile_error(
                            i18n,
                            "sessionManager.remote_desktop_profiles.open_failed",
                            error,
                            cx,
                        );
                        return Some(ProfileSubmission {
                            open: None,
                            persisted,
                        });
                    }
                }
            }
        }
        self.clear_form_after_submit(cx);
        Some(ProfileSubmission {
            open: (action != NewConnectionSubmitAction::Save).then(|| DesktopFormOpen {
                profile,
                password,
                gateway,
                reconnect_id,
            }),
            persisted,
        })
    }
}

/// Carries credentials to exactly one launch after the protected-store transaction.
struct SavedConnectionRuntimeHandoff {
    pub connection_id: String,
    pub secrets: oxideterm_connections::SavedConnectionRuntimeSecrets,
    pub auth_override: Option<AuthMethod>,
}

fn persist_ssh_form(
    form: &mut NewConnectionForm,
    store: &mut ConnectionStore,
    prefix: anyhow::Result<Vec<crate::NewConnectionProxyHop>>,
    i18n: &I18n,
) -> Option<SavedConnectionRuntimeHandoff> {
    let fallback_name = if form.name.trim().is_empty() {
        let (host, user) = (form.host.trim(), form.username.trim());
        if host.is_empty() || user.is_empty() {
            None
        } else {
            Some(format!("{user}@{host}"))
        }
    } else {
        Some(form.name.trim().to_owned())
    };
    if let Some(name) = fallback_name {
        form.name = if store
            .connections()
            .iter()
            .any(|record| record.name.trim().eq_ignore_ascii_case(&name))
        {
            crate::configuration::duplicate_connection_template_name(
                &name,
                store
                    .connections()
                    .iter()
                    .map(|record| record.name.as_str()),
            )
        } else {
            name
        };
    }

    let request = (|| {
        let mut prefix = prefix?;
        let copies = saved_proxy_hop_auth_copies(
            store,
            &form.proxy_hops,
            &i18n.t("sessions.saved_next_hop.missing_credentials"),
        )?;
        let prefix_len = prefix.len();
        let mut request = save_request_from_form_with_proxy_hop_prefix(form, &mut prefix, None)?;
        apply_saved_proxy_hop_auth_copies(&mut request, prefix_len, copies);
        Ok::<_, anyhow::Error>(request)
    })();
    let request = match request {
        Ok(request) => request,
        Err(error) => {
            form.error = Some(error.to_string());
            return None;
        }
    };
    let auth_override = (form.auth_tab == SshAuthTab::Password && !form.save_password).then(|| {
        if form.empty_password {
            form.password.zeroize();
            AuthMethod::password("")
        } else if form.password.is_empty() {
            AuthMethod::password_prompt()
        } else {
            AuthMethod::password_secret(take_zeroizing_secret(&mut form.password))
        }
    });
    match store.upsert_with_runtime_secrets(request) {
        Ok((connection, secrets)) => Some(SavedConnectionRuntimeHandoff {
            connection_id: connection.id,
            secrets,
            auth_override,
        }),
        Err(error) => {
            form.error = Some(format!(
                "{}: {error}",
                i18n.t("modals.new_connection.save_failed")
            ));
            None
        }
    }
}

pub fn prepare_ssh_profile_update(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    source_id: &str,
    duplicate: bool,
    i18n: &I18n,
) -> anyhow::Result<oxideterm_connections::SaveConnectionRequest> {
    let source = store
        .get(source_id)
        .ok_or_else(|| anyhow::anyhow!("Source connection is no longer available"))?;
    let missing_credentials = i18n.t("sessions.saved_next_hop.missing_credentials");
    let copies = saved_proxy_hop_auth_copies(store, &form.proxy_hops, &missing_credentials)?;
    let edit_sources = form
        .proxy_hops
        .iter()
        .map(|hop| EditedProxyHopSource {
            persisted_proxy_hop_index: hop.persisted_proxy_hop_index,
            has_explicit_secret_draft: hop.has_explicit_secret_draft(),
        })
        .collect();
    let mut request = crate::configuration::save_request_from_form_with_existing_auth(
        form,
        (!duplicate).then(|| source_id.to_owned()),
        Some(&source.auth),
    )?;
    preserve_edited_proxy_hop_auth(
        &mut request.proxy_chain,
        &source.proxy_chain,
        edit_sources,
        copies,
        &missing_credentials,
    )?;
    if duplicate {
        request.auth = auth_for_duplicate_owner(store, request.auth)?;
        for hop in &mut request.proxy_chain {
            hop.auth = auth_for_duplicate_owner(
                store,
                std::mem::replace(&mut hop.auth, oxideterm_connections::SavedAuth::Agent),
            )?;
        }
        request.upstream_proxy = upstream_proxy_for_duplicate_owner(store, request.upstream_proxy)?;
        if request.proxy_command.is_some()
            && let Some(command) = source.proxy_command.as_ref()
        {
            request.proxy_command = Some(oxideterm_connections::SavedProxyCommand {
                keychain_id: None,
                plaintext_command: Some(store.get_saved_proxy_command(command)?),
            });
        }
    }
    Ok(request)
}

pub struct PreparedMoshConnect {
    pub config: SshConfig,
    pub title: String,
    pub options: MoshConnectionOptions,
}

pub enum SavedMoshOpen {
    Missing,
    SupplyCredentials(NewConnectionForm),
    Connect(PreparedMoshConnect),
}

pub fn prepare_saved_mosh_open(
    store: &ConnectionStore,
    id: &str,
    attempt: Option<String>,
    reconnect_id: Option<String>,
    i18n: &I18n,
) -> SavedMoshOpen {
    use crate::NewConnectionField as Field;
    let Some(profile) = store.get_mosh_profile(id) else {
        return SavedMoshOpen::Missing;
    };
    let load = store.load_mosh_profile_runtime_secrets(id);
    let load_failed = load.is_err();
    match load
        .ok()
        .and_then(|secrets| runtime_mosh_config_from_saved(store, profile, secrets, None))
    {
        Some(config) => {
            let mut options = mosh_options_from_profile(profile);
            options.runtime_connection_attempt_id = attempt;
            SavedMoshOpen::Connect(PreparedMoshConnect {
                config,
                title: profile.name.clone(),
                options,
            })
        }
        None => {
            let mut draft =
                crate::form_state::form_from_mosh_profile(profile, i18n.t("ssh.form.ungrouped"));
            draft.standalone_connection_id = reconnect_id;
            draft.error = Some(i18n.t("sessionManager.mosh_profiles.missing_credentials"));
            // Store-load failure can involve any protected hop; otherwise point at the primary auth field.
            draft.focused_field = if load_failed {
                Field::Name
            } else {
                match draft.auth_tab {
                    SshAuthTab::Password => Field::Password,
                    SshAuthTab::DefaultKey
                    | SshAuthTab::SshKey
                    | SshAuthTab::ManagedKey
                    | SshAuthTab::Certificate => Field::Passphrase,
                    SshAuthTab::Agent | SshAuthTab::TwoFactor => Field::Name,
                }
            };
            draft.field_focused = false;
            SavedMoshOpen::SupplyCredentials(draft)
        }
    }
}

enum FormSubmission {
    Ftp,
    LocalTerminal,
    Serial,
    Telnet,
    Ble,
    Mosh,
    StandaloneSftp,
    RemoteDesktop,
    WslGraphics,
    SavedAuthentication,
    Edit,
    Duplicate,
    Connect(crate::launch::SshConnectionIntent),
    Save {
        parent: Option<oxideterm_ssh::NodeId>,
        connect: bool,
    },
}

fn plan_form_submission(
    state: &mut crate::ConnectionFormState,
    action: NewConnectionSubmitAction,
    i18n: &I18n,
) -> Option<FormSubmission> {
    use crate::{
        form_state::{
            NewConnectionFormMode, NewConnectionTransport, connection_timeout_drafts_valid,
        },
        launch::{SshConnectionIntent, SshTerminalConnectionOptions},
    };
    let mode = state.mode();
    let parent = state.drill_down_parent_node_id.clone();
    let form = state.form.as_mut()?;
    let transport = form.transport;
    if matches!(
        transport,
        NewConnectionTransport::Ssh
            | NewConnectionTransport::Mosh
            | NewConnectionTransport::StandaloneSftp
    ) && !connection_timeout_drafts_valid(form)
    {
        form.error = Some(i18n.t("ssh.form.connect_timeout_invalid"));
        return None;
    }
    if transport == NewConnectionTransport::Ssh {
        if !crate::form_state::connection_keep_alive_interval_valid(form) {
            form.error = Some(i18n.t("ssh.form.keep_alive_interval_invalid"));
            return None;
        }
        if !crate::form_state::connection_keep_alive_count_max_valid(form) {
            form.error = Some(i18n.t("ssh.form.keep_alive_count_max_invalid"));
            return None;
        }
    }
    match transport {
        NewConnectionTransport::Ftp => return Some(FormSubmission::Ftp),
        NewConnectionTransport::StandaloneSftp if parent.is_none() => {
            return Some(FormSubmission::StandaloneSftp);
        }
        _ => {}
    }
    if parent.is_none() && mode == NewConnectionFormMode::NewConnection {
        let native = match transport {
            NewConnectionTransport::LocalTerminal => Some(FormSubmission::LocalTerminal),
            NewConnectionTransport::Serial => Some(FormSubmission::Serial),
            NewConnectionTransport::Telnet => Some(FormSubmission::Telnet),
            NewConnectionTransport::Ble => Some(FormSubmission::Ble),
            NewConnectionTransport::Mosh => Some(FormSubmission::Mosh),
            NewConnectionTransport::Rdp | NewConnectionTransport::Vnc => {
                Some(FormSubmission::RemoteDesktop)
            }
            NewConnectionTransport::WslGraphics => Some(FormSubmission::WslGraphics),
            _ => None,
        };
        if native.is_some() {
            return native;
        }
    }
    if parent.is_none() {
        match mode {
            NewConnectionFormMode::SavedConnectionPrompt => {
                return Some(FormSubmission::SavedAuthentication);
            }
            NewConnectionFormMode::EditProperties => return Some(FormSubmission::Edit),
            NewConnectionFormMode::DuplicateTemplate => return Some(FormSubmission::Duplicate),
            NewConnectionFormMode::NewConnection => {}
        }
    }
    Some(match action {
        NewConnectionSubmitAction::Connect => {
            form.save_connection = false;
            let terminal_options = SshTerminalConnectionOptions::from_form(form);
            FormSubmission::Connect(match parent {
                Some(parent_id) => SshConnectionIntent::DrillDown {
                    parent_id,
                    saved_connection_id: None,
                    terminal_options,
                },
                None => SshConnectionIntent::Connect(terminal_options),
            })
        }
        NewConnectionSubmitAction::Save => FormSubmission::Save {
            parent,
            connect: false,
        },
        NewConnectionSubmitAction::SaveAndConnect => FormSubmission::Save {
            parent,
            connect: true,
        },
    })
}

pub struct PendingBleOpen {
    pub form_id: uuid::Uuid,
    pub adapter_guard: Option<BleAdapterGuard>,
    pub selected_device: Option<oxideterm_ble::BlePeripheralId>,
    pub adapter_mac: Option<String>,
    pub device_mac: Option<String>,
    pub device_name: Option<String>,
    pub platform_identifier: Option<String>,
    pub service_uuid: String,
    pub label: String,
    pub title: String,
    pub terminal_options: ConnectionTerminalOptions,
    pub runtime_options: oxideterm_terminal::SerialRuntimeOptions,
    pub mtu: Option<u16>,
    pub write_without_response: bool,
    pub pre_connect_command: Option<String>,
}

pub fn prepare_serial_form(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    i18n: &I18n,
    action: NewConnectionSubmitAction,
) -> Option<(
    SerialSessionConfig,
    String,
    ConnectionTerminalOptions,
    Option<SaveSerialProfileRequest>,
)> {
    let port_path = form.serial_port_path.trim().to_string();
    let baud_rate = form.serial_baud_rate.trim().parse::<u32>().ok();
    if port_path.is_empty() {
        form.error = Some(i18n.t("modals.new_connection.serial_port_required"));
        return None;
    }
    let Some(baud_rate) = baud_rate.filter(|baud| *baud > 0) else {
        form.error = Some(i18n.t("modals.new_connection.serial_invalid_baud_rate"));
        return None;
    };
    let runtime_options = form
        .serial_profile_id
        .as_deref()
        .and_then(|id| {
            store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == id)
        })
        .map(terminal_serial_runtime_options_from_profile)
        .unwrap_or_default();
    let config = SerialSessionConfig {
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
        port_path: port_path.clone(),
        baud_rate,
        data_bits: form.serial_data_bits,
        stop_bits: form.serial_stop_bits,
        parity: form.serial_parity,
        flow_control: form.serial_flow_control,
        runtime_options,
    };
    let editing_profile_id = form.serial_profile_id.clone();
    let existing_connect_on_open = editing_profile_id.as_deref().and_then(|id| {
        store
            .serial_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.connect_on_open)
    });
    // Editing always updates the persisted asset and preserves hidden sync metadata.
    let should_save_profile =
        editing_profile_id.is_some() || action != NewConnectionSubmitAction::Connect;
    let save_request = should_save_profile.then(|| SaveSerialProfileRequest {
        id: editing_profile_id,
        name: serial_profile_name_or_port(&form.serial_profile_name, &port_path),
        group: serial_profile_group_from_form(&form.group, i18n),
        notes: saved_profile_notes(&form.notes),
        icon: asset_icon_from_form(&form.icon),
        color: asset_color_from_form(&form.color),
        icon_background_color: asset_color_from_form(&form.icon_background_color),
        port_path,
        baud_rate: Some(baud_rate),
        data_bits: Some(form.serial_data_bits),
        stop_bits: Some(form.serial_stop_bits),
        parity: Some(serial_profile_parity_from_terminal(form.serial_parity)),
        flow_control: Some(serial_profile_flow_from_terminal(form.serial_flow_control)),
        input_line_ending: None,
        output_line_ending: None,
        terminal: form.terminal.clone(),
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
        connect_on_open: existing_connect_on_open,
    });
    form.pending = true;
    form.error = None;
    Some((
        config,
        form.serial_profile_name.clone(),
        form.terminal.clone(),
        save_request,
    ))
}

pub fn prepare_telnet_form(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    i18n: &I18n,
    action: NewConnectionSubmitAction,
) -> Option<(
    TelnetSessionConfig,
    SavedUpstreamProxyPolicy,
    ConnectionTerminalOptions,
    Option<SaveTelnetProfileRequest>,
)> {
    let host = form.host.trim().to_string();
    let port = form.port.trim().parse::<u16>().ok();
    if host.is_empty() {
        form.error = Some(i18n.t("modals.new_connection.telnet_host_required"));
        return None;
    }
    let Some(port) = port else {
        form.error = Some(i18n.t("modals.new_connection.telnet_invalid_port"));
        return None;
    };
    let editing_profile_id = form.telnet_profile_id.clone();
    let mut upstream_proxy = match saved_upstream_proxy_policy_from_form(form) {
        Ok(policy) => policy,
        Err(error) => {
            form.error = Some(error.to_string());
            return None;
        }
    };
    if let SavedUpstreamProxyPolicy::Custom { proxy } = &mut upstream_proxy
        && let SavedUpstreamProxyAuth::Password {
            keychain_id,
            plaintext_password,
            ..
        } = &mut proxy.auth
        && plaintext_password.is_some()
    {
        // An explicitly edited password overrides the old protected-store reference.
        *keychain_id = None;
    }
    let existing_connect_on_open = editing_profile_id.as_deref().and_then(|id| {
        store
            .telnet_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.connect_on_open)
    });
    // Editing always updates the persisted asset and preserves hidden sync metadata.
    let should_save_profile =
        editing_profile_id.is_some() || action != NewConnectionSubmitAction::Connect;
    let save_request = should_save_profile.then(|| SaveTelnetProfileRequest {
        input_line_ending: Some(form.telnet_input_line_ending),
        output_line_ending: Some(form.telnet_output_line_ending),
        upstream_proxy: Some(upstream_proxy.clone()),
        id: editing_profile_id,
        name: telnet_profile_name_or_endpoint(&form.telnet_profile_name, &host, port),
        group: serial_profile_group_from_form(&form.group, i18n),
        notes: saved_profile_notes(&form.notes),
        icon: asset_icon_from_form(&form.icon),
        color: asset_color_from_form(&form.color),
        icon_background_color: asset_color_from_form(&form.icon_background_color),
        host: host.clone(),
        port,
        terminal: form.terminal.clone(),
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
        connect_on_open: existing_connect_on_open,
    });
    let config = TelnetSessionConfig {
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
        host,
        port,
        line_endings: oxideterm_terminal::TelnetLineEndings {
            input: terminal_serial_line_ending_from_profile(form.telnet_input_line_ending),
            output: terminal_serial_line_ending_from_profile(form.telnet_output_line_ending),
        },
    };
    let terminal_options = form.terminal.clone();
    form.pending = true;
    form.error = None;
    Some((config, upstream_proxy, terminal_options, save_request))
}

pub fn prepare_mosh_form(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    i18n: &I18n,
    action: NewConnectionSubmitAction,
) -> Option<(
    Option<PreparedMoshConnect>,
    Option<SaveMoshProfileRequest>,
    Option<AuthMethod>,
)> {
    let host = form.host.trim().to_string();
    let username = form.username.trim().to_string();
    let ssh_port = form
        .port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0);
    if host.is_empty() || username.is_empty() || ssh_port.is_none() {
        form.error = Some(i18n.t("ssh.form.validation_required"));
        return None;
    }
    match form.auth_tab {
        SshAuthTab::SshKey if form.key_path.trim().is_empty() => {
            form.error = Some(i18n.t("ssh.form.key_path_required"));
            return None;
        }
        SshAuthTab::ManagedKey if form.managed_key_id.trim().is_empty() => {
            form.error = Some(i18n.t("ssh.form.managed_key_required"));
            return None;
        }
        SshAuthTab::Certificate
            if form.key_path.trim().is_empty() || form.cert_path.trim().is_empty() =>
        {
            form.error = Some(i18n.t("ssh.form.certificate_paths_required"));
            return None;
        }
        _ => {}
    }
    if let Err(error) = validate_proxy_chain_form(form) {
        form.error = Some(error);
        return None;
    }
    let udp_port = match parse_mosh_udp_port(&form.mosh_udp_port) {
        Ok(udp_port) => udp_port,
        Err(_) => {
            form.error = Some(i18n.t("mosh.form.invalid_udp_port"));
            return None;
        }
    };
    let server_executable = form.mosh_server_executable.trim().to_string();
    if server_executable.is_empty() {
        form.error = Some(i18n.t("mosh.form.server_executable_required"));
        return None;
    }
    let title = if form.name.trim().is_empty() {
        format!("{username}@{host}")
    } else {
        form.name.trim().to_string()
    };
    let options = MoshConnectionOptions {
        saved_profile_id: form
            .standalone_connection_id
            .as_ref()
            .and(form.mosh_profile_id.clone()),
        server_executable: server_executable.clone(),
        udp_host_override: (!form.mosh_udp_host.trim().is_empty())
            .then(|| form.mosh_udp_host.trim().to_string()),
        udp_port,
        ip_family: form.mosh_ip_family,
        prediction: form.mosh_prediction,
        locale: (!form.mosh_locale.trim().is_empty()).then(|| form.mosh_locale.trim().to_string()),
        terminal: form.terminal.clone(),
        public_mcp_open_token: None,
        runtime_connection_attempt_id: None,
    };
    let ssh_port = ssh_port.expect("validated Mosh SSH port must exist");

    if action == NewConnectionSubmitAction::Connect {
        let saved_proxy_hop_auth = match saved_proxy_hop_auth_from_store(
            store,
            form,
            &i18n.t("sessions.saved_next_hop.missing_credentials"),
        ) {
            Ok(auth) => auth,
            Err(error) => {
                form.error = Some(error);
                return None;
            }
        };
        let proxy_chain = proxy_chain_from_form(
            store,
            form,
            RuntimeSecretHandoff::Move,
            saved_proxy_hop_auth,
        );
        let auth = runtime_mosh_auth_from_form(form);
        let config = SshConfig {
            host,
            port: ssh_port,
            username,
            auth,
            proxy_chain,
            identity_agent: identity_agent_from_form(&form.identity_agent),
            legacy_ssh_compatibility: form.legacy_ssh_compatibility,
            ssh_algorithms: form.ssh_algorithms.clone(),
            strict_host_key_checking: true,
            pre_connect_command: (!form.pre_connect_command.trim().is_empty())
                .then(|| form.pre_connect_command.trim().to_string()),
            ..SshConfig::default()
        };
        form.pending = true;
        form.error = Some(i18n.t("ssh.form.checking_host_key"));
        return Some((
            Some(PreparedMoshConnect {
                config,
                title,
                options,
            }),
            None,
            None,
        ));
    }

    let existing_profile = form
        .mosh_profile_id
        .as_deref()
        .and_then(|id| store.get_mosh_profile(id))
        .cloned();
    let proxy_chain = match (|| -> anyhow::Result<Vec<SavedProxyHop>> {
        let missing_credentials_message = i18n.t("sessions.saved_next_hop.missing_credentials");
        let saved_auth_copies =
            saved_proxy_hop_auth_copies(store, &form.proxy_hops, &missing_credentials_message)?;
        let edit_sources = form
            .proxy_hops
            .iter()
            .map(|hop| EditedProxyHopSource {
                persisted_proxy_hop_index: hop.persisted_proxy_hop_index,
                has_explicit_secret_draft: hop.has_explicit_secret_draft(),
            })
            .collect();
        let mut proxy_chain = form
            .proxy_hops
            .iter_mut()
            .map(saved_standalone_sftp_proxy_hop_from_form)
            .collect::<anyhow::Result<Vec<_>>>()?;
        preserve_edited_proxy_hop_auth(
            &mut proxy_chain,
            existing_profile
                .as_ref()
                .map_or(&[], |profile| profile.proxy_chain.as_slice()),
            edit_sources,
            saved_auth_copies,
            &missing_credentials_message,
        )?;
        Ok(proxy_chain)
    })() {
        Ok(proxy_chain) => proxy_chain,
        Err(error) => {
            form.error = Some(error.to_string());
            return None;
        }
    };
    let auth_override = (action == NewConnectionSubmitAction::SaveAndConnect
        && form.auth_tab == SshAuthTab::Password
        && !mosh_password_draft_is_persistent(form))
    .then(|| runtime_mosh_auth_from_form(form));
    let auth = saved_mosh_auth_from_form(form);
    let request = SaveMoshProfileRequest {
        id: form.mosh_profile_id.clone(),
        name: title,
        group: serial_profile_group_from_form(&form.group, i18n),
        notes: saved_profile_notes(&form.notes),
        icon: asset_icon_from_form(&form.icon),
        color: asset_color_from_form(&form.color),
        icon_background_color: asset_color_from_form(&form.icon_background_color),
        host,
        ssh_port,
        username,
        auth,
        proxy_chain,
        server_executable,
        udp_host_override: options.udp_host_override,
        udp_port,
        ip_family: options.ip_family,
        prediction: options.prediction,
        locale: options.locale,
        identity_agent: identity_agent_from_form(&form.identity_agent),
        legacy_ssh_compatibility: form.legacy_ssh_compatibility,
        ssh_algorithms: form.ssh_algorithms.clone(),
        terminal: form.terminal.clone(),
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
    };
    form.pending = true;
    form.error = None;
    Some((None, Some(request), auth_override))
}

pub fn prepare_standalone_sftp_form(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    i18n: &I18n,
    action: NewConnectionSubmitAction,
) -> Option<(
    SaveStandaloneSftpProfileRequest,
    Option<AuthMethod>,
    Option<AuthMethod>,
)> {
    if form.name.trim().is_empty()
        && !form.host.trim().is_empty()
        && !form.username.trim().is_empty()
    {
        form.name = format!("{}@{}", form.username.trim(), form.host.trim());
    }
    let existing_profile = form
        .standalone_sftp_profile_id
        .as_deref()
        .and_then(|id| store.get_standalone_sftp_profile(id))
        .cloned();
    let existing_auth = existing_profile
        .as_ref()
        .map(|profile| profile.auth.clone());
    let base_request = if existing_auth.is_some() {
        save_request_from_form_with_existing_auth(
            form,
            form.standalone_sftp_profile_id.clone(),
            existing_auth.as_ref(),
        )
    } else {
        save_request_from_form_with_proxy_hop_prefix(
            form,
            &mut [],
            form.standalone_sftp_profile_id.clone(),
        )
    };
    let base_request = match base_request {
        Ok(request) => request,
        Err(error) => {
            form.error = Some(error.to_string());
            return None;
        }
    };
    let auth_override = (action == NewConnectionSubmitAction::SaveAndConnect
        && form.auth_tab == SshAuthTab::Password
        && !form.save_password)
        .then(|| {
            if form.empty_password {
                form.password.zeroize();
                AuthMethod::password("")
            } else if form.password.is_empty() {
                AuthMethod::password_prompt()
            } else {
                AuthMethod::password_secret(take_zeroizing_secret(&mut form.password))
            }
        });
    let secondary_endpoint =
        if form.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote {
            match saved_standalone_sftp_secondary_endpoint_from_form(
                store,
                &mut form.standalone_sftp_secondary,
                existing_profile
                    .as_ref()
                    .and_then(|profile| profile.secondary_endpoint.as_ref()),
                &i18n.t("sessions.saved_next_hop.missing_credentials"),
            ) {
                Ok(endpoint) => Some(endpoint),
                Err(error) => {
                    form.error = Some(error.to_string());
                    return None;
                }
            }
        } else {
            None
        };
    let secondary_auth_override = (action == NewConnectionSubmitAction::SaveAndConnect
        && form.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote
        && form.standalone_sftp_secondary.auth_tab == SshAuthTab::Password
        && !form.standalone_sftp_secondary.save_password)
        .then(|| {
            let endpoint = &mut form.standalone_sftp_secondary;
            if endpoint.empty_password {
                endpoint.password.zeroize();
                AuthMethod::password("")
            } else if endpoint.password.is_empty() {
                AuthMethod::password_prompt()
            } else {
                AuthMethod::password_secret(take_zeroizing_secret(&mut endpoint.password))
            }
        });
    let request = SaveStandaloneSftpProfileRequest {
        id: base_request.id,
        name: base_request.name,
        group: base_request.group,
        notes: base_request.notes,
        icon: base_request.icon,
        color: base_request.color,
        icon_background_color: base_request.icon_background_color,
        host: base_request.host,
        port: base_request.port,
        username: base_request.username,
        auth: base_request.auth,
        connect_timeout_seconds: base_request.connect_timeout_seconds,
        proxy_chain: base_request.proxy_chain,
        upstream_proxy: base_request.upstream_proxy,
        proxy_command: base_request.proxy_command,
        identity_agent: base_request.identity_agent,
        legacy_ssh_compatibility: base_request.legacy_ssh_compatibility,
        ssh_algorithms: base_request.ssh_algorithms,
        initial_remote_path: (!form.sftp_initial_remote_path.trim().is_empty())
            .then(|| form.sftp_initial_remote_path.trim().to_string()),
        transfer_mode: form.standalone_sftp_transfer_mode,
        secondary_endpoint,
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
    };
    form.pending = true;
    form.error = None;
    Some((request, auth_override, secondary_auth_override))
}

pub fn prepare_remote_desktop_form(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    i18n: &I18n,
    action: NewConnectionSubmitAction,
    settings: &PersistedSettings,
    reconnect_profile: Option<&RemoteDesktopProfile>,
) -> Option<(
    RemoteDesktopConnectionProfile,
    Option<SaveRemoteDesktopProfileRequest>,
    Option<SecretString>,
    Option<String>,
)> {
    let Some(protocol) = remote_desktop_protocol_for_transport(form.transport) else {
        return None;
    };
    let host = form.host.trim().to_string();
    let port = form
        .port
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0);
    if host.is_empty() {
        form.error = Some(i18n.t("modals.new_connection.remote_desktop_host_required"));
        return None;
    }
    let Some(port) = port else {
        form.error = Some(i18n.t("modals.new_connection.remote_desktop_invalid_port"));
        return None;
    };
    if protocol == RemoteDesktopProtocol::Rdp && form.username.trim().is_empty() {
        form.error = Some(i18n.t("modals.new_connection.remote_desktop_username_required"));
        return None;
    }
    if form
        .remote_desktop_ssh_gateway_connection_id
        .as_deref()
        .is_some_and(|connection_id| store.get(connection_id).is_none())
    {
        form.error = Some(i18n.t("modals.new_connection.remote_desktop_ssh_gateway_missing"));
        return None;
    }
    let editing_profile_id = form.remote_desktop_profile_id.clone();
    let existing_profile = editing_profile_id
        .as_deref()
        .and_then(|id| store.get_remote_desktop_profile(id))
        .cloned();
    let has_saved_credential = form.saved_password_keychain_id.is_some();
    if protocol == RemoteDesktopProtocol::Rdp
        && action != NewConnectionSubmitAction::Save
        && form.password.is_empty()
        && !form.empty_password
        && !has_saved_credential
    {
        form.error = Some(i18n.t("modals.new_connection.remote_desktop_password_required"));
        return None;
    }
    let proxy_policy = if protocol == RemoteDesktopProtocol::Rdp {
        match rdp_proxy_policy_from_form(form) {
            Ok(policy) => policy,
            Err(key) => {
                form.error = Some(i18n.t(key));
                return None;
            }
        }
    } else {
        SavedUpstreamProxyPolicy::Direct
    };
    let socks_proxy =
        if protocol == RemoteDesktopProtocol::Rdp && action != NewConnectionSubmitAction::Save {
            match oxideterm_session_adapter::rdp_socks_proxy_from_saved_policy(
                store,
                settings,
                &proxy_policy,
                form.remote_desktop_ssh_gateway_connection_id.is_some(),
            ) {
                Ok(proxy) => proxy,
                Err(error) => {
                    form.error = Some(format!(
                        "{}: {error}",
                        i18n.t("modals.new_connection.remote_desktop_proxy_failed")
                    ));
                    return None;
                }
            }
        } else {
            None
        };
    let label = remote_desktop_profile_label(&form.name, protocol, &host, port);
    let username = Some(form.username.trim().to_string()).filter(|username| !username.is_empty());
    let password = if form.empty_password {
        Some(SecretString::default())
    } else if !form.password.is_empty() {
        // Move the UI draft into a zeroizing type before saving or starting a worker.
        Some(SecretString::from(std::mem::take(&mut form.password)))
    } else {
        None
    };
    let save_credential = form.save_password || form.empty_password;
    let ssh_gateway_connection_id = form.remote_desktop_ssh_gateway_connection_id.clone();
    let should_save = editing_profile_id.is_some() || action != NewConnectionSubmitAction::Connect;
    let (credential_to_save, runtime_password) = if should_save && save_credential {
        // Saving and connecting reloads the protected value below instead of cloning it.
        (password, None)
    } else {
        (None, password)
    };
    let clear_credential = credential_to_save.is_none()
        && ((has_saved_credential && !save_credential)
            || existing_profile.as_ref().is_some_and(|p| p.empty_password));

    let source_profile = existing_profile.as_ref().or(reconnect_profile);
    let domain = source_profile.and_then(|profile| profile.domain.clone());
    let read_only = source_profile.is_some_and(|profile| profile.read_only);
    let save_request = should_save.then(|| SaveRemoteDesktopProfileRequest {
        id: editing_profile_id,
        name: label.clone(),
        group: serial_profile_group_from_form(&form.group, i18n),
        notes: saved_profile_notes(&form.notes),
        icon: asset_icon_from_form(&form.icon),
        color: asset_color_from_form(&form.color),
        icon_background_color: asset_color_from_form(&form.icon_background_color),
        protocol,
        host: host.clone(),
        port,
        username: username.clone(),
        domain: domain.clone(),
        ssh_gateway_connection_id: form.remote_desktop_ssh_gateway_connection_id.clone(),
        upstream_proxy: Some(proxy_policy),
        credential_ref: None,
        credential: credential_to_save,
        clear_credential,
        read_only,
        session_options: form.remote_desktop_session_options,
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
    });
    let profile = RemoteDesktopConnectionProfile {
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
        id: format!("new-remote-desktop-{}", uuid::Uuid::new_v4()),
        label,
        protocol,
        endpoint: RemoteDesktopEndpoint::new(host, port),
        transport_endpoint: None,
        socks_proxy,
        username,
        domain,
        credential_ref: None,
        read_only,
        // A reconnect reuses the profile, so keep the user's per-session
        // redirection choices on the profile instead of rebuilding defaults.
        session_options: form.remote_desktop_session_options,
    };
    form.pending = true;
    form.error = None;
    Some((
        profile,
        save_request,
        runtime_password,
        ssh_gateway_connection_id,
    ))
}

pub fn prepare_ble_form(
    form: &mut NewConnectionForm,
    store: &ConnectionStore,
    i18n: &I18n,
    action: NewConnectionSubmitAction,
) -> Option<(PendingBleOpen, Option<SaveBleProfileRequest>)> {
    let editing_id = form.ble.profile_id.clone();
    let existing = editing_id.as_deref().and_then(|id| {
        store
            .ble_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
    });
    // The open button is gated on a selected service that carries
    // both notify and write characteristics; re-check the selection
    // here because stale scan state can outlive the gating pass.
    // Saving an existing profile keeps working without a live scan.
    let selection = form
        .ble
        .selected_device_id
        .clone()
        .zip(form.ble.selected_service_uuid);
    if selection.is_none() && (action != NewConnectionSubmitAction::Save || existing.is_none()) {
        form.error = Some(i18n.t("modals.new_connection.ble_open_failed"));
        return None;
    }
    let (device_id, service_uuid) = selection.unzip();
    let device = device_id.as_ref().and_then(|device_id| {
        form.ble
            .devices
            .iter()
            .find(|device| &device.id == device_id)
            .cloned()
    });
    let device_label = device
        .as_ref()
        .map(|device| {
            device
                .name
                .clone()
                .unwrap_or_else(|| device.address.clone())
        })
        .unwrap_or_default();
    // A live selection is the authority; otherwise the stored identity
    // keeps an edit saveable while the device is out of scan range.
    let (adapter_mac, device_mac, device_name, service_uuid_text) = if let Some(device) = &device {
        (
            form.ble
                .adapters
                .get(form.ble.selected_adapter_index)
                .and_then(|adapter| adapter.adapter_mac.clone()),
            device.persistent_address().map(ToOwned::to_owned),
            device.name.clone(),
            service_uuid
                .expect("selection paired a service")
                .to_string(),
        )
    } else {
        match &existing {
            Some(profile) => (
                profile.adapter_mac.clone(),
                profile.device_mac.clone(),
                profile.device_name.clone(),
                profile.service_uuid.clone(),
            ),
            None => (None, None, None, String::new()),
        }
    };
    let profile_name = {
        let name = form.ble.profile_name.trim();
        if name.is_empty() {
            device_label
        } else {
            name.to_string()
        }
    };
    // The macOS CoreBluetooth identifier is the only platform-local
    // identity worth persisting: a live selection refreshes it, an
    // edit without re-selection keeps the stored value.
    let platform_identifier = if let Some(device_id) = &device_id {
        cfg!(target_vendor = "apple").then(|| device_id.to_string())
    } else {
        existing
            .as_ref()
            .and_then(|profile| profile.platform_identifier.clone())
    };
    // MTU is a text input in the form; an empty draft means "platform
    // default" and anything else must parse inside the BLE spec range.
    let mtu_text = form.ble.mtu.trim();
    let mtu = if mtu_text.is_empty() {
        None
    } else {
        match mtu_text.parse::<u16>() {
            Ok(value)
                if (oxideterm_connections::BLE_ATT_MTU_MIN
                    ..=oxideterm_connections::BLE_ATT_MTU_MAX)
                    .contains(&value) =>
            {
                Some(value)
            }
            _ => {
                form.error = Some(i18n.t("modals.new_connection.ble_mtu_invalid"));
                return None;
            }
        }
    };
    let should_save = editing_id.is_some() || action != NewConnectionSubmitAction::Connect;
    let save_request = should_save.then(|| SaveBleProfileRequest {
        id: editing_id.clone(),
        name: profile_name.clone(),
        group: serial_profile_group_from_form(&form.group, i18n),
        notes: saved_profile_notes(&form.notes),
        icon: asset_icon_from_form(&form.icon),
        color: asset_color_from_form(&form.color),
        icon_background_color: asset_color_from_form(&form.icon_background_color),
        adapter_mac: adapter_mac.clone(),
        device_mac: device_mac.clone(),
        device_name: device_name.clone(),
        platform_identifier: platform_identifier.clone(),
        service_uuid: service_uuid_text.clone(),
        terminal: form.terminal.clone(),
        input_line_ending: Some(form.ble.input_line_ending),
        output_line_ending: Some(form.ble.output_line_ending),
        send_mode: Some(form.ble.send_mode),
        display_mode: Some(form.ble.display_mode),
        local_echo: Some(form.ble.local_echo),
        mtu: Some(mtu),
        write_without_response: Some(form.ble.write_without_response),
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
        connect_on_open: existing.as_ref().map(|profile| profile.connect_on_open),
    });
    let open = PendingBleOpen {
        form_id: form.ble.instance_id,
        // The adapter that produced the user's selection is handed
        // to the resolver so the discovered peripheral stays
        // addressable while the card reports the short re-check.
        adapter_guard: form.ble.adapter_guard.take(),
        selected_device: device_id,
        adapter_mac,
        device_mac,
        device_name,
        platform_identifier,
        service_uuid: service_uuid_text,
        title: format!("BLE {}", profile_name),
        label: profile_name,
        terminal_options: form.terminal.clone(),
        runtime_options: terminal_serial_runtime_options_from_ble_form(&form),
        mtu,
        write_without_response: form.ble.write_without_response,
        pre_connect_command: (!form.pre_connect_command.trim().is_empty())
            .then(|| form.pre_connect_command.trim().to_string()),
    };
    form.pending = true;
    form.error = None;
    Some((open, save_request))
}

pub fn prepare_ftp_form(
    form: &mut NewConnectionForm,
    store: &mut ConnectionStore,
    i18n: &I18n,
    action: NewConnectionSubmitAction,
    settings: &PersistedSettings,
) -> Option<(FtpProfile, oxideterm_ftp::ConnectOptions, bool)> {
    if form.pending {
        return None;
    }
    let mut prepare = || -> Result<_, String> {
        let security = if form.ftp_tls {
            FtpSecurity::ExplicitTls
        } else {
            FtpSecurity::Plain
        };
        let mut profile = form
            .ftp_profile_id
            .as_deref()
            .and_then(|id| store.get_ftp_profile(id))
            .cloned()
            .unwrap_or_else(|| {
                FtpProfile::new(
                    form.name.clone(),
                    form.host.clone(),
                    form.username.clone(),
                    security,
                )
            });
        profile.name = form.name.clone();
        profile.host = form.host.trim().into();
        profile.username = form.username.trim().into();
        profile.security = security;
        profile.port = form
            .port
            .trim()
            .parse::<u16>()
            .map_err(|_| i18n.t("modals.new_connection.ftp_invalid"))?;
        profile.connect_timeout_seconds =
            form.connect_timeout_seconds_text
                .trim()
                .parse::<u64>()
                .map_err(|_| i18n.t("ssh.form.connect_timeout_invalid"))?;
        profile.group =
            if form.group.trim().is_empty() || form.group == i18n.t("ssh.form.ungrouped") {
                None
            } else {
                Some(form.group.clone())
            };
        profile.notes = Some(form.notes.clone());
        profile.initial_path = form.sftp_initial_remote_path.clone();
        profile.icon = Some(form.icon.clone());
        profile.color = Some(form.color.clone());
        profile.icon_background_color = Some(form.icon_background_color.clone());
        profile
            .validate()
            .map_err(|_| i18n.t("modals.new_connection.ftp_invalid"))?;
        if form.password.contains(['\r', '\n', '\0']) {
            return Err(i18n.t("modals.new_connection.ftp_invalid"));
        }
        profile.upstream_proxy =
            saved_upstream_proxy_policy_from_form(form).map_err(|e| e.to_string())?;
        let password = if form.password.is_empty() && profile.password_keychain_id.is_some() {
            store
                .get_ftp_password(&profile.id)
                .map_err(|e| e.to_string())?
                .unwrap_or_default()
        } else {
            SecretString::from(std::mem::take(&mut form.password))
        };
        let should_save =
            action != NewConnectionSubmitAction::Connect || form.ftp_profile_id.is_some();
        let password = if should_save && form.save_password {
            profile = store
                .upsert_ftp_profile(SaveFtpProfileRequest {
                    profile,
                    password: Some(password),
                    clear_password: false,
                })
                .map_err(|e| e.to_string())?;
            store
                .get_ftp_password(&profile.id)
                .map_err(|e| e.to_string())?
                .unwrap_or_default()
        } else {
            if should_save {
                profile = store
                    .upsert_ftp_profile(SaveFtpProfileRequest {
                        profile,
                        password: None,
                        clear_password: true,
                    })
                    .map_err(|e| e.to_string())?;
            }
            password
        };
        let proxy = oxideterm_session_adapter::upstream_proxy_config_from_saved_policy(
            store,
            settings,
            &profile.upstream_proxy,
        )?;
        if should_save {
            form.ftp_profile_id = Some(profile.id.clone());
            form.saved_password_keychain_id = profile.password_keychain_id.clone();
        }
        let options = oxideterm_ftp::ConnectOptions {
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            password: password.into_zeroizing(),
            security: if security == FtpSecurity::ExplicitTls {
                oxideterm_ftp::Security::ExplicitTls
            } else {
                oxideterm_ftp::Security::Plain
            },
            timeout: Duration::from_secs(profile.connect_timeout_seconds),
            proxy,
        };
        Ok((profile, options, should_save))
    };
    match prepare() {
        Ok(result) => {
            form.pending = action != NewConnectionSubmitAction::Save;
            form.error = None;
            Some(result)
        }
        Err(error) => {
            form.error = Some(error);
            None
        }
    }
}

impl crate::entity::ConnectionFlowEntity {
    pub fn prepare_standalone_sftp_launch(
        &mut self,
        store: &ConnectionStore,
        settings: &PersistedSettings,
        profile: &oxideterm_connections::StandaloneSftpProfile,
        mut secrets: oxideterm_connections::SavedStandaloneSftpProfileRuntimeSecrets,
        auth_override: Option<AuthMethod>,
        secondary_auth_override: Option<AuthMethod>,
    ) -> Option<(SshConfig, crate::launch::SshConnectionIntent)> {
        let secondary_secrets = secrets.secondary_endpoint.take();
        let primary = oxideterm_session_adapter::ssh_config_from_standalone_sftp_profile_with_runtime_secrets(
            store, settings, profile, secrets, auth_override,
        )?;
        let secondary = profile.secondary_endpoint.as_ref().zip(secondary_secrets).and_then(|(endpoint, secrets)| {
            oxideterm_session_adapter::ssh_config_from_standalone_sftp_endpoint_with_runtime_secrets(
                store, settings, &profile.name, endpoint, secrets, secondary_auth_override,
            )
        });
        if profile.transfer_mode == StandaloneSftpTransferMode::RemoteRemote && secondary.is_none()
        {
            return None;
        }
        let pair_launch_token = secondary.map(|secondary| {
            self.stage_sftp_pair_launch(crate::launch::PendingStandaloneSftpPairLaunch {
                saved_profile_id: profile.id.clone(),
                title: profile.name.clone(),
                primary_initial_remote_path: profile.initial_remote_path.clone(),
                secondary_initial_remote_path: profile
                    .secondary_endpoint
                    .as_ref()
                    .and_then(|endpoint| endpoint.initial_remote_path.clone()),
                primary_config: None,
                secondary_config: Some(secondary),
            })
        });
        Some((
            primary,
            crate::launch::SshConnectionIntent::StandaloneSftp {
                saved_profile_id: Some(profile.id.clone()),
                initial_remote_path: profile.initial_remote_path.clone(),
                pair_launch_token,
            },
        ))
    }
}

pub enum SavedDesktopOpen {
    Prompt(Box<NewConnectionForm>),
    Launch {
        profile: RemoteDesktopConnectionProfile,
        password: Option<SecretString>,
        gateway: Option<String>,
    },
}

pub fn prepare_saved_desktop_open(
    store: &ConnectionStore,
    settings: &PersistedSettings,
    id: &str,
    i18n: &I18n,
) -> Result<Option<SavedDesktopOpen>, String> {
    let Some(saved) = store.get_remote_desktop_profile(id).cloned() else {
        return Ok(None);
    };
    let password = store.get_remote_desktop_credential(id).map_err(|error| {
        format!(
            "{}: {error}",
            i18n.t("sessionManager.remote_desktop_profiles.open_failed")
        )
    })?;
    let socks_proxy = if saved.protocol == RemoteDesktopProtocol::Rdp {
        if password.is_none() {
            // Imported and synced profiles may lack device-local credentials.
            let mut form =
                crate::form_state::form_from_remote_desktop_profile(&saved, String::new());
            form.error = Some(i18n.t("modals.new_connection.remote_desktop_password_required"));
            form.focused_field = crate::NewConnectionField::Password;
            return Ok(Some(SavedDesktopOpen::Prompt(Box::new(form))));
        }
        oxideterm_session_adapter::rdp_socks_proxy_from_saved_policy(
            store,
            settings,
            &saved.upstream_proxy,
            saved.ssh_gateway_connection_id.is_some(),
        )
        .map_err(|error| {
            format!(
                "{}: {error}",
                i18n.t("modals.new_connection.remote_desktop_proxy_failed")
            )
        })?
    } else {
        None
    };
    let profile = RemoteDesktopConnectionProfile {
        pre_connect_command: saved.pre_connect_command.clone(),
        id: saved.id.clone(),
        label: saved.name,
        protocol: saved.protocol,
        endpoint: RemoteDesktopEndpoint::new(saved.host, saved.port),
        transport_endpoint: None,
        socks_proxy,
        username: saved.username,
        domain: saved.domain,
        credential_ref: saved.credential_ref,
        read_only: saved.read_only,
        session_options: saved.session_options,
    };
    Ok(Some(SavedDesktopOpen::Launch {
        profile,
        password,
        gateway: saved.ssh_gateway_connection_id,
    }))
}
