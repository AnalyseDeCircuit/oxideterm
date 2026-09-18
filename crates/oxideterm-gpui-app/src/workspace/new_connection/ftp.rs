use gpui::{Context, Window};
use oxideterm_connections::{FtpProfile, FtpSecurity, SaveFtpProfileRequest, SecretString};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

use super::{ConnectionFormState, NewConnectionTransport, form_state::NewConnectionSubmitAction};
use crate::workspace::{WorkspaceApp, sftp::ftp::FtpRuntime};

impl WorkspaceApp {
    pub(in crate::workspace) fn submit_ftp_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        cx: &mut Context<Self>,
    ) {
        let prepared = self.with_connection_form_mut(cx, |this, form, cx| {
            let form = form?;
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
                    .and_then(|id| this.connection_store.get_ftp_profile(id))
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
                    .map_err(|_| this.i18n.t("modals.new_connection.ftp_invalid"))?;
                profile.connect_timeout_seconds = form
                    .connect_timeout_seconds_text
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| this.i18n.t("ssh.form.connect_timeout_invalid"))?;
                profile.group = if form.group.trim().is_empty()
                    || form.group == this.i18n.t("ssh.form.ungrouped")
                {
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
                    .map_err(|_| this.i18n.t("modals.new_connection.ftp_invalid"))?;
                if form.password.contains(['\r', '\n', '\0']) {
                    return Err(this.i18n.t("modals.new_connection.ftp_invalid"));
                }
                profile.upstream_proxy =
                    crate::workspace::session_manager::saved_upstream_proxy_policy_from_form(form)
                        .map_err(|e| e.to_string())?;
                let password = if form.password.is_empty() && profile.password_keychain_id.is_some()
                {
                    this.connection_store
                        .get_ftp_password(&profile.id)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default()
                } else {
                    SecretString::from(std::mem::take(&mut form.password))
                };
                let should_save =
                    action != NewConnectionSubmitAction::Connect || form.ftp_profile_id.is_some();
                let password = if should_save && form.save_password {
                    profile = this
                        .connection_store
                        .upsert_ftp_profile(SaveFtpProfileRequest {
                            profile,
                            password: Some(password),
                            clear_password: false,
                        })
                        .map_err(|e| e.to_string())?;
                    this.connection_store
                        .get_ftp_password(&profile.id)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default()
                } else {
                    if should_save {
                        profile = this
                            .connection_store
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
                    &this.connection_store,
                    this.settings_store.settings(),
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
                    cx.notify();
                    None
                }
            }
        });
        let Some((profile, options, saved)) = prepared else {
            return;
        };
        if saved {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        if action == NewConnectionSubmitAction::Save {
            self.update_connection_form_state(cx, ConnectionFormState::clear);
            cx.notify();
            return;
        }
        let attempt_id = uuid::Uuid::new_v4();
        let cancellation = CancellationToken::new();
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.ftp_attempt = Some((attempt_id, cancellation.clone()));
            }
        });
        let task = self
            .forwarding_runtime
            .spawn(FtpRuntime::connect(options, cancellation));
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .map_err(|_| "FTP connection task ended".to_string())
                .and_then(|result| result);
            let _ = this.update(cx, |this, cx| {
                if !this
                    .connection_form_state(cx)
                    .form
                    .as_ref()
                    .is_some_and(|form| {
                        form.ftp_attempt
                            .as_ref()
                            .is_some_and(|(id, _)| *id == attempt_id)
                    })
                {
                    return;
                }
                match result {
                    Ok(runtime) => {
                        if saved {
                            let _ = this.connection_store.mark_ftp_profile_used(&profile.id);
                        }
                        this.ftp_sessions
                            .insert(profile.id.clone(), Arc::new(runtime));
                        this.update_connection_form_state(cx, ConnectionFormState::clear);
                        this.open_ftp_tab(&profile, cx);
                    }
                    Err(error) => {
                        let error = format!(
                            "{}: {error}",
                            this.i18n.t("modals.new_connection.ftp_failed")
                        );
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.pending = false;
                                form.error = Some(error);
                                form.ftp_attempt = None;
                            }
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn open_ftp_tab(
        &mut self,
        profile: &FtpProfile,
        cx: &mut Context<Self>,
    ) {
        self.open_standalone_sftp_tab_surface(
            profile.id.clone(),
            profile.name.clone(),
            Some(profile.initial_path.clone()),
            cx,
        );
    }

    pub(in crate::workspace) fn open_saved_ftp_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_ftp_profile_editor(id, window, cx);
        self.submit_ftp_connection_form(NewConnectionSubmitAction::Connect, cx);
    }

    pub(in crate::workspace) fn open_saved_ftp_profile_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.connection_store.get_ftp_profile(id).cloned() else {
            return;
        };
        self.open_new_connection_form(window, cx);
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                super::form_state::apply_saved_upstream_proxy_to_form(
                    form,
                    &profile.upstream_proxy,
                );
                form.icon = profile.icon.clone().unwrap_or_default();
                form.color = profile.color.clone().unwrap_or_default();
                form.icon_background_color =
                    profile.icon_background_color.clone().unwrap_or_default();
                form.transport = NewConnectionTransport::Ftp;
                form.ftp_profile_id = Some(profile.id);
                form.ftp_tls = profile.security == FtpSecurity::ExplicitTls;
                form.name = profile.name;
                form.host = profile.host;
                form.port = profile.port.to_string();
                form.username = profile.username;
                form.group = profile.group.unwrap_or_default();
                form.notes = profile.notes.unwrap_or_default();
                form.sftp_initial_remote_path = profile.initial_path;
                form.connect_timeout_seconds_text = profile.connect_timeout_seconds.to_string();
                form.saved_password_keychain_id = profile.password_keychain_id;
                form.save_password = form.saved_password_keychain_id.is_some();
            }
        });
        cx.notify();
    }
}
