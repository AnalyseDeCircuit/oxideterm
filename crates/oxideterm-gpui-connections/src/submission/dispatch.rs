// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::views::ConnectionsViews;
use gpui::App;
use oxideterm_ssh::NodeId;

pub struct ConnectionSubmission {
    pub persisted: bool,
    pub effect: Option<ConnectionSubmissionEffect>,
}

/// Only effects that need the workspace's stores, live sessions, or window.
pub enum ConnectionSubmissionEffect {
    Ftp,
    LocalTerminal(LocalFormSubmission),
    Console(ConsoleFormOpen),
    Ble(BleOpenRequest),
    Mosh(Box<PreparedMoshConnect>, Option<String>),
    Desktop(DesktopFormOpen),
    WslGraphics,
    SavedAuthentication(SavedPromptLaunch),
    SavedEdit { id: String, next: AfterSshEdit },
    Duplicate { status: String },
    Preflight(SshFormOpen),
    CloseForm,
    ReportSaveError(String),
}

pub enum AfterSshEdit {
    FocusPane {
        status: String,
    },
    Prompt {
        message: String,
    },
    Reconnect {
        node: NodeId,
        config: SshConfig,
        title: String,
        open_sftp: bool,
    },
}

pub enum ConsoleFormOpen {
    Serial(SerialFormOpen),
    Telnet(TelnetFormOpen),
}

pub enum ConsoleProfileAfterOpen {
    Serial(Option<SaveSerialProfileRequest>),
    Telnet(Option<SaveTelnetProfileRequest>, Option<String>),
}

impl<T> ProfileSubmission<T> {
    fn into_effect(
        self,
        make_effect: impl FnOnce(T) -> ConnectionSubmissionEffect,
    ) -> ConnectionSubmission {
        ConnectionSubmission {
            persisted: self.persisted,
            effect: self.open.map(make_effect),
        }
    }
}

fn effect(effect: ConnectionSubmissionEffect) -> ConnectionSubmission {
    ConnectionSubmission {
        persisted: false,
        effect: Some(effect),
    }
}

pub struct LocalFormSubmission {
    request: oxideterm_connections::SaveLocalTerminalProfileRequest,
    save: bool,
    pub launch: bool,
}

impl LocalFormSubmission {
    pub fn shell_id(&self) -> Option<&str> {
        self.request.shell_id.as_deref()
    }

    pub fn cwd(&self) -> Option<&str> {
        self.request.cwd.as_deref()
    }

    pub fn pre_connect_command(&self) -> Option<&str> {
        self.request.pre_connect_command.as_deref()
    }

    pub fn title(
        &self,
        shell_label: Option<&str>,
        default_title: impl FnOnce() -> String,
    ) -> String {
        if self.request.name.is_empty() {
            shell_label.map(str::to_owned).unwrap_or_else(default_title)
        } else {
            self.request.name.clone()
        }
    }
}

impl ConnectionFlowEntity {
    fn prepare_local_form(
        &mut self,
        action: NewConnectionSubmitAction,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<LocalFormSubmission> {
        let form = self.form.form.as_ref()?;
        let save = action != NewConnectionSubmitAction::Connect;
        let name = form.name.trim();
        if save && name.is_empty() {
            self.set_local_form_error(i18n.t("local_session.name_required"), cx);
            return None;
        }
        Some(LocalFormSubmission {
            request: oxideterm_connections::SaveLocalTerminalProfileRequest {
                id: form.local_profile_id.clone(),
                name: name.to_owned(),
                icon: (!form.icon.is_empty()).then(|| form.icon.clone()),
                color: (!form.color.is_empty()).then(|| form.color.clone()),
                icon_background_color: (!form.icon_background_color.is_empty())
                    .then(|| form.icon_background_color.clone()),
                shell_id: form.local_shell_id.clone(),
                cwd: (!form.local_cwd.trim().is_empty()).then(|| form.local_cwd.trim().to_owned()),
                pre_connect_command: (!form.pre_connect_command.trim().is_empty())
                    .then(|| form.pre_connect_command.trim().to_string()),
                group: (!connection_group_is_ungrouped(&form.group, i18n))
                    .then(|| form.group.trim().to_owned()),
            },
            save,
            launch: action != NewConnectionSubmitAction::Save,
        })
    }

    pub fn persist_local_form(
        &mut self,
        plan: LocalFormSubmission,
        store: &mut ConnectionStore,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Option<String>> {
        if !plan.save {
            return Ok(None);
        }
        let profile = store.upsert_local_terminal_profile(plan.request)?;
        if let Some(form) = self.form.form.as_mut() {
            form.local_profile_id = Some(profile.id.clone());
            cx.notify();
        }
        Ok(Some(profile.id))
    }

    pub fn set_local_form_error(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(form) = self.form.form.as_mut() {
            form.error = Some(message);
            cx.notify();
        }
    }

    pub fn set_form_group(&mut self, group: String, cx: &mut Context<Self>) {
        if let Some(form) = self.form.form.as_mut() {
            form.group = group;
            cx.notify();
        }
    }

    pub fn open_desktop_reconnect_form(
        &mut self,
        mut form: NewConnectionForm,
        reconnect_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        form.standalone_connection_id = reconnect_id;
        self.replace_form(form, cx);
    }

    pub fn open_duplicate_form(
        &mut self,
        form: NewConnectionForm,
        source_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.form.replace_with_new_form(form);
        self.form.duplicating_saved_connection_id = Some(source_id.to_owned());
        cx.notify();
    }

    pub fn submit_form<'a>(
        &mut self,
        action: NewConnectionSubmitAction,
        store: &mut ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        mut parent_prefix: impl FnMut(&NodeId) -> anyhow::Result<Vec<crate::NewConnectionProxyHop>>,
        reconnect_profile: impl FnOnce(&str) -> Option<&'a RemoteDesktopProfile>,
        cx: &mut Context<Self>,
    ) -> Option<ConnectionSubmission> {
        use ConnectionSubmissionEffect::*;
        if let Some(expected) = self.form.form.as_ref().and_then(|form| form.saved_identity)
            && let Some(id) = &self.form.editing_saved_connection_id
            && (store.refresh_if_changed().is_err()
                || store.saved_ssh_identity(id) != Some(expected))
        {
            self.set_form_feedback(
                None,
                Some(i18n.t("modals.new_connection.saved_identity_changed")),
                cx,
            );
            return None;
        }
        Some(match self.plan_submission(action, i18n, cx)? {
            FormSubmission::Ftp => effect(Ftp),
            FormSubmission::LocalTerminal => {
                effect(LocalTerminal(self.prepare_local_form(action, i18n, cx)?))
            }
            FormSubmission::Serial => self
                .submit_serial(store, i18n, action, cx)?
                .into_effect(|open| Console(ConsoleFormOpen::Serial(open))),
            FormSubmission::Telnet => self
                .submit_telnet(store, i18n, action, cx)?
                .into_effect(|open| Console(ConsoleFormOpen::Telnet(open))),
            FormSubmission::Ble => self.submit_ble(store, i18n, action, cx)?.into_effect(Ble),
            FormSubmission::Mosh => self
                .submit_mosh(store, i18n, action, cx)?
                .into_effect(|open| Mosh(Box::new(open), self.reconnect_id().map(str::to_owned))),
            FormSubmission::RemoteDesktop => {
                let restored = self.reconnect_id().and_then(reconnect_profile);
                self.submit_desktop(store, settings, i18n, action, restored, cx)?
                    .into_effect(Desktop)
            }
            FormSubmission::StandaloneSftp if action == NewConnectionSubmitAction::Connect => {
                let path = self.form.form.as_ref()?.sftp_initial_remote_path.trim();
                let intent = SshConnectionIntent::StandaloneSftp {
                    saved_profile_id: None,
                    initial_remote_path: (!path.is_empty()).then(|| path.to_owned()),
                    pair_launch_token: None,
                };
                effect(Preflight(
                    self.prepare_ssh_launch(store, settings, i18n, intent, cx)?,
                ))
            }
            FormSubmission::StandaloneSftp => self
                .submit_saved_sftp(store, settings, i18n, action, cx)?
                .into_effect(Preflight),
            FormSubmission::WslGraphics => effect(WslGraphics),
            FormSubmission::SavedAuthentication => effect(SavedAuthentication(
                self.submit_saved_prompt(store, settings, i18n, cx)?,
            )),
            FormSubmission::Edit => {
                let edit = self.submit_ssh_edit(store, i18n, false, cx)?;
                let next = match edit.reconnect_node {
                    None => AfterSshEdit::FocusPane {
                        status: i18n.t("sessionManager.edit_properties.save"),
                    },
                    Some(node) => store
                        .get(&edit.id)
                        .and_then(|connection| {
                            let config =
                                oxideterm_session_adapter::ssh_config_from_saved_connection(
                                    store, settings, connection,
                                )?;
                            Some(AfterSshEdit::Reconnect {
                                node,
                                config,
                                title: connection.name.clone(),
                                open_sftp: connection.options.open_sftp_on_connect,
                            })
                        })
                        .unwrap_or_else(|| AfterSshEdit::Prompt {
                            message: i18n.t("sessionManager.edit_properties.password_placeholder"),
                        }),
                };
                ConnectionSubmission {
                    persisted: true,
                    effect: Some(SavedEdit { id: edit.id, next }),
                }
            }
            FormSubmission::Duplicate => {
                self.submit_ssh_edit(store, i18n, true, cx)?;
                ConnectionSubmission {
                    persisted: true,
                    effect: Some(Duplicate {
                        status: i18n.t("sessionManager.toast.connection_duplicated"),
                    }),
                }
            }
            FormSubmission::Connect(intent) => effect(Preflight(
                self.prepare_ssh_launch(store, settings, i18n, intent, cx)?,
            )),
            FormSubmission::Save { parent, connect } => {
                let prefix = match parent.as_ref() {
                    Some(node) => parent_prefix(node),
                    None => Ok(Vec::new()),
                };
                let handoff = self.persist_ssh(store, prefix, i18n, cx)?;
                let next = if connect {
                    // Read the live route again at launch; the persisted prefix isn't a runtime lease.
                    let route = parent
                        .map(|node| parent_prefix(&node).map(|hops| (node, hops.len())))
                        .transpose();
                    match route.map_err(|error| error.to_string()).and_then(|route| {
                        self.launch_persisted_ssh_form(store, settings, i18n, handoff, route, cx)
                    }) {
                        Ok(open) => Preflight(open),
                        Err(error) => ReportSaveError(error),
                    }
                } else {
                    CloseForm
                };
                ConnectionSubmission {
                    persisted: true,
                    effect: Some(next),
                }
            }
        })
    }

    fn reconnect_id(&self) -> Option<&str> {
        self.form.form.as_ref()?.standalone_connection_id.as_deref()
    }

    pub fn prepare_ssh_launch(
        &mut self,
        store: &ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        intent: SshConnectionIntent,
        cx: &mut Context<Self>,
    ) -> Option<SshFormOpen> {
        let (config, title) = self.plan_form_ssh_preflight(store, settings, i18n, &intent, cx)?;
        Some((config, title, intent))
    }

    pub fn enable_prompt_sftp(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.form.form.as_mut() {
            form.open_sftp_on_connect = true;
            cx.notify();
        }
    }
}

impl ConnectionsViews {
    pub fn show_catalog_status(&self, status: String, cx: &mut App) {
        self.session_manager
            .update(cx, |manager, cx| manager.set_status(Some(status), cx));
    }

    pub fn report_connection_error(&self, message: String, cx: &mut App) {
        self.report_connection_feedback(false, message, cx);
    }

    pub(crate) fn report_connection_feedback(&self, pending: bool, message: String, cx: &mut App) {
        if !self.flow.update(cx, |flow, cx| {
            flow.set_form_feedback(Some(pending), Some(message.clone()), cx)
        }) {
            self.show_catalog_status(message, cx);
        }
    }

    /// Persist edits made with Connect only after the runtime accepts the terminal.
    pub fn finish_console_open(
        &self,
        result: Result<(), String>,
        save: ConsoleProfileAfterOpen,
        store: &mut ConnectionStore,
        i18n: &I18n,
        cx: &mut App,
    ) -> Option<ProfileSubmission<String>> {
        if let Err(error) = result {
            self.flow.update(cx, |flow, cx| {
                flow.set_form_feedback(Some(false), Some(error), cx)
            });
            return None;
        }
        let (saved, existing_id, error_key) = match save {
            ConsoleProfileAfterOpen::Serial(request) => (
                request.map(|request| {
                    store
                        .upsert_serial_profile(request)
                        .map(|profile| profile.id)
                }),
                None,
                "modals.new_connection.serial_save_failed",
            ),
            ConsoleProfileAfterOpen::Telnet(request, profile_id) => (
                request.map(|request| {
                    store
                        .upsert_telnet_profile(request)
                        .map(|profile| profile.id)
                }),
                profile_id,
                "modals.new_connection.telnet_save_failed",
            ),
        };
        let result = match saved {
            Some(Ok(id)) => ProfileSubmission {
                persisted: true,
                open: Some(id),
            },
            Some(Err(error)) => {
                self.show_catalog_status(format!("{}: {error}", i18n.t(error_key)), cx);
                ProfileSubmission {
                    persisted: false,
                    open: existing_id,
                }
            }
            None => ProfileSubmission {
                persisted: false,
                open: existing_id,
            },
        };
        self.flow.update(cx, |flow, cx| {
            flow.form.clear();
            cx.notify();
        });
        Some(result)
    }
}
