// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{App, Context};
use oxideterm_connections::{ConnectionStore, FtpProfile, FtpSecurity};
use oxideterm_ftp::{ConnectOptions, FtpSession};
use oxideterm_i18n::I18n;
use oxideterm_settings::PersistedSettings;
use tokio_util::sync::CancellationToken;

use super::{ConnectionsViews, ConnectionsWindowRequest};
use crate::{
    entity::ConnectionFlowEntity,
    form_state::{
        NewConnectionSubmitAction, NewConnectionTransport, apply_saved_upstream_proxy_to_form,
    },
    submission::prepare_ftp_form,
};

pub struct ConnectedFtp {
    pub profile: FtpProfile,
    pub options: ConnectOptions,
    pub session: FtpSession,
    pub saved: bool,
}

impl ConnectionFlowEntity {
    pub fn populate_ftp_form(&mut self, profile: FtpProfile, cx: &mut Context<Self>) {
        let Some(form) = self.form.form.as_mut() else {
            return;
        };
        apply_saved_upstream_proxy_to_form(form, &profile.upstream_proxy);
        form.icon = profile.icon.unwrap_or_default();
        form.color = profile.color.unwrap_or_default();
        form.icon_background_color = profile.icon_background_color.unwrap_or_default();
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
        cx.notify();
    }
}

impl ConnectionsViews {
    pub fn submit_ftp(
        &self,
        action: NewConnectionSubmitAction,
        store: &mut ConnectionStore,
        settings: &PersistedSettings,
        i18n: &I18n,
        runtime: &tokio::runtime::Handle,
        cx: &mut App,
    ) -> bool {
        self.flow.update(cx, |flow, cx| {
            let Some(form) = flow.form.form.as_mut() else {
                return false;
            };
            let prepared = prepare_ftp_form(form, store, i18n, action, settings);
            cx.notify();
            let Some((profile, options, saved)) = prepared else {
                return false;
            };
            if action == NewConnectionSubmitAction::Save {
                flow.clear_form_after_submit(cx);
                return saved;
            }
            let id = uuid::Uuid::new_v4();
            let cancel = CancellationToken::new();
            flow.form
                .form
                .as_mut()
                .expect("FTP draft still open")
                .ftp_attempt = Some((id, cancel.clone()));
            let worker = runtime.spawn(async move {
                let session = FtpSession::connect(&options, &cancel)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(ConnectedFtp {
                    profile,
                    options,
                    session,
                    saved,
                })
            });
            let dispatch = self.dispatch.clone();
            let failure_message = i18n.t("modals.new_connection.ftp_failed");
            cx.spawn(async move |weak, cx| {
                let result = worker
                    .await
                    .map_err(|_| "FTP connection task ended".to_owned())
                    .and_then(|result| result);
                let adoption = weak
                    .update(cx, |flow, cx| {
                        let form = flow.form.form.as_mut()?;
                        if !form
                            .ftp_attempt
                            .as_ref()
                            .is_some_and(|(current, _)| *current == id)
                        {
                            return None;
                        }
                        match result {
                            Ok(connected) => {
                                flow.clear_form_after_submit(cx);
                                Some(connected)
                            }
                            Err(error) => {
                                form.ftp_attempt = None;
                                flow.set_form_feedback(
                                    Some(false),
                                    Some(format!("{failure_message}: {error}")),
                                    cx,
                                );
                                None
                            }
                        }
                    })
                    .ok()
                    .flatten();
                if let Some(connected) = adoption {
                    let _ = cx.update(|cx| {
                        dispatch(ConnectionsWindowRequest::AdoptFtp(connected), None, cx)
                    });
                }
            })
            .detach();
            saved
        })
    }
}
