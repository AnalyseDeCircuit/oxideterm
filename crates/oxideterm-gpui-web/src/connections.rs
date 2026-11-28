mod import;
pub(crate) mod storage;
pub(crate) use import::ConnectionImports;

use crate::{
    ble::{BleProfile, BleSessionConfig, is_ble_node, known_service_label},
    demo::{Endpoint, SftpProfile, SftpTarget},
    host::{Action, Workspace},
};
use gpui::{prelude::*, *};
use oxideterm_gpui_connections::form_model::StandaloneSftpTransferMode;
use oxideterm_gpui_connections::form_state::NewConnectionTransport;
use oxideterm_gpui_connections::views::portable_form::{
    PortableConnectionDraft, PortableProtocolDraft, PortableSshEndpoint,
};
use oxideterm_gpui_connections::{NewConnectionForm, session_manager::*, views::*};
use oxideterm_workspace::TabKind;

impl From<&PortableSshEndpoint> for Endpoint {
    fn from(e: &PortableSshEndpoint) -> Self {
        Self {
            host: e.host.clone(),
            port: e.port,
            username: e.username.clone(),
            path: e.initial_remote_path.clone(),
        }
    }
}

pub fn catalog(demo: bool) -> PortableCatalogData {
    let mut data = PortableCatalogData::default();
    if demo {
        let mut primary = CatalogRow::new(
            SessionManagerOpenTarget::Connection("demo".into()),
            "OxideTerm",
        );
        primary.host = "oxide.demo".into();
        primary.username = "demo".into();
        primary.port = 22;
        primary.subtitle = "demo@oxide.demo:22".into();
        data.items.push(primary);

        let mut secondary = CatalogRow::new(
            SessionManagerOpenTarget::Connection("prod-api".into()),
            "prod-api-01",
        );
        secondary.host = "10.0.12.40".into();
        secondary.username = "ops".into();
        secondary.port = 22;
        secondary.subtitle = "ops@10.0.12.40:22".into();
        data.items.push(secondary);
    }
    data
}

#[derive(Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProfileDetails {
    notes: String,
    icon: String,
    pub(crate) open_sftp: bool,
}

impl Workspace {
    fn save_profile(
        &mut self,
        mut row: CatalogRow,
        draft: &PortableConnectionDraft,
        cx: &mut Context<Self>,
    ) {
        let color = |value: &str| (!value.trim().is_empty()).then(|| value.to_owned());
        row.group = (!oxideterm_gpui_connections::form_state::connection_group_is_ungrouped(
            &draft.group,
            &self.i18n,
        ))
        .then(|| draft.group.clone());
        row.tags = draft.tags.clone();
        row.color = color(&draft.color);
        row.icon_background_color = color(&draft.icon_background_color);
        if let Some(icon) =
            oxideterm_gpui_connections::icons::session_icon_from_id(Some(&draft.icon))
        {
            row.icon = icon;
        }
        if let Some(previous) = self
            .catalog_data
            .items
            .iter()
            .find(|r| r.target == row.target)
        {
            row.last_used = previous.last_used.clone();
            row.active = previous.active;
        }
        self.profile_details.insert(row.id().to_owned(), ProfileDetails {
            notes: draft.notes.clone(), icon: draft.icon.clone(),
            open_sftp: matches!(&draft.protocol, PortableProtocolDraft::Ssh(_, opts) if opts.open_sftp_on_connect),
        });
        self.catalog_data.upsert_row(row);
        self.refresh_catalog(cx);
    }

    fn restore_profile(&self, form: &mut NewConnectionForm, row: &CatalogRow) {
        form.group = row.group.clone().unwrap_or_default();
        form.tags = row.tags.clone();
        form.color = row.color.clone().unwrap_or_default();
        form.icon_background_color = row.icon_background_color.clone().unwrap_or_default();
        if let Some(details) = self.profile_details.get(row.id()) {
            form.icon = details.icon.clone();
            form.notes = details.notes.clone();
            form.open_sftp_on_connect = details.open_sftp;
        }
    }

    fn ssh_form(&self, row: &CatalogRow) -> NewConnectionForm {
        let mut form = NewConnectionForm::for_transport(NewConnectionTransport::Ssh);
        form.name = row.name.clone();
        form.host = row.host.clone();
        form.port = row.port.to_string();
        form.username = row.username.clone();
        self.restore_profile(&mut form, row);
        form
    }
}

impl Workspace {
    pub fn connection_env(&self) -> ConnectionsEnvironment {
        ConnectionsEnvironment {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
        }
    }
    pub fn connect_catalog_item(&mut self, id: &str, w: &mut Window, cx: &mut Context<Self>) {
        if self
            .files
            .as_ref()
            .is_some_and(|f| f.sftp_profiles.contains_key(id))
        {
            self.open_node_sftp(id, cx);
        } else {
            self.new_terminal(id.into(), None, w, cx);
        }
    }

    pub fn catalog_event(
        &mut self,
        event: &PortableCatalogEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use PortableCatalogEvent::*;
        match event {
            Connect {
                target: SessionManagerOpenTarget::Connection(id),
                open_sftp,
            } => {
                self.new_terminal_with_sftp(id.clone(), None, open_sftp.then_some(true), w, cx);
            }
            Connect {
                target: SessionManagerOpenTarget::StandaloneSftp(id),
                ..
            } => self.open_node_sftp(id, cx),
            Connect {
                target: SessionManagerOpenTarget::Ble(id),
                ..
            } => self.new_terminal(id.clone(), None, w, cx),
            Edit(
                SessionManagerOpenTarget::Connection(id)
                | SessionManagerOpenTarget::Ble(id)
                | SessionManagerOpenTarget::StandaloneSftp(id),
            ) => self.form(Some(id), None, w, cx),
            OpenSshConfigImporter => self.open_ssh_import(w, cx),
            CreateConnection { group } => self.form(None, group.clone(), w, cx),
            Duplicate(id) => {
                let Some(row) = self.catalog_data.items.iter().find(
                    |r| matches!(&r.target, SessionManagerOpenTarget::Connection(key) if key == id),
                ) else {
                    return;
                };
                let mut draft = self.ssh_form(row);
                draft.name =
                    oxideterm_gpui_connections::form_state::duplicate_connection_template_name(
                        &row.name,
                        self.catalog_data.items.iter().map(|r| r.name.as_str()),
                    );
                let env = self.browser_form_environment();
                let form = cx.new(|cx| {
                    PortableConnectionFormView::duplicate_ssh(id.clone(), draft, env, cx)
                });
                self.show_form(form, w, cx);
            }
            Delete(targets) => {
                self.catalog_data.items.retain(|r| !targets.iter().any(|t| matches!((t, &r.target),
                    (SessionManagerSelectionTarget::StandaloneSftp(a), SessionManagerOpenTarget::StandaloneSftp(b))
                    | (SessionManagerSelectionTarget::Ble(a), SessionManagerOpenTarget::Ble(b))
                    | (SessionManagerSelectionTarget::Connection(a), SessionManagerOpenTarget::Connection(b)) if a == b)));
                self.profile_details
                    .retain(|id, _| self.catalog_data.items.iter().any(|r| r.id() == id));
                for target in targets {
                    if let SessionManagerSelectionTarget::Ble(id) = target {
                        self.ble_profiles.remove(id);
                    } else if let SessionManagerSelectionTarget::StandaloneSftp(id) = target
                        && let Some(f) = &mut self.files
                    {
                        f.sftp_profiles.remove(id);
                    }
                }
                if self
                    .selected_connection_id
                    .as_ref()
                    .is_some_and(|id| !self.catalog_data.items.iter().any(|row| row.id() == id))
                {
                    self.selected_connection_id = None;
                }
                self.refresh_catalog(cx);
            }
            CreateGroup(group) => {
                self.catalog_data.ensure_group(group);
                self.refresh_catalog(cx);
                self.catalog
                    .update(cx, |v, cx| v.set_group_filter(Some(group.clone()), cx));
            }
            RenameGroup { old, new } => {
                if self.catalog_data.rename_group(old, new) {
                    self.persist_catalog(cx);
                    self.catalog.update(cx, |v, cx| {
                        v.set_renamed_catalog_data(self.catalog_data.clone(), old, new, cx)
                    });
                    cx.notify();
                } else {
                    self.notice("sessionManager.toast.rename_group_failed", cx);
                }
            }
            DeleteGroup(group) => {
                self.catalog_data.delete_group(group);
                self.refresh_catalog(cx);
            }
            MoveToGroup { targets, group } => {
                self.catalog_data.move_to_group(targets, group.as_deref());
                self.refresh_catalog(cx);
            }
            _ => self.notice("remote_desktop.capability_unsupported", cx),
        }
    }
    pub fn form(
        &mut self,
        id: Option<&String>,
        group: Option<String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form_with_transport(id, group, None, w, cx);
    }

    pub fn form_for_transport(
        &mut self,
        transport: NewConnectionTransport,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form_with_transport(None, None, Some(transport), w, cx);
    }

    fn browser_form_environment(&self) -> PortableConnectionFormEnvironment {
        let mut env = PortableConnectionFormEnvironment::new(self.connection_env());
        env.form.groups = self.catalog_data.group_paths().into_iter().collect();
        env.form.browser = Some(BrowserConnectionCapabilities {
            simulated_profiles: self.demo,
            bluetooth: crate::ble::available(),
        });
        env
    }

    pub(crate) fn open_quick_ssh(
        &mut self,
        launch: oxideterm_ssh_launch::TemporarySshLaunch,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.demo {
            // Open accepts a node identifier, not authentication material.
            if launch.password.is_none() && self.ready {
                self.new_terminal(
                    oxideterm_ssh_launch::format_user_host_port_target(
                        &launch.username,
                        &launch.host,
                        launch.port,
                    ),
                    None,
                    w,
                    cx,
                );
            }
            return;
        }
        let mut draft = NewConnectionForm::for_transport(NewConnectionTransport::Ssh);
        draft.name = launch.host.clone();
        draft.host = launch.host;
        draft.port = launch.port.to_string();
        draft.username = launch.username;
        // A simulated connection has no authentication step.
        drop(launch.password);
        draft.focused_field = oxideterm_gpui_connections::NewConnectionField::Host;
        let env = self.browser_form_environment();
        let form = cx.new(|cx| PortableConnectionFormView::from_form(draft, env, cx));
        self.show_form(form, w, cx);
    }

    fn form_with_transport(
        &mut self,
        id: Option<&String>,
        group: Option<String>,
        transport: Option<NewConnectionTransport>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.browser_form_environment();
        let transport = transport.unwrap_or_else(|| {
            if id.is_some_and(|id| is_ble_node(id)) {
                NewConnectionTransport::Ble
            } else if id.is_some_and(|id| {
                self.files
                    .as_ref()
                    .is_some_and(|f| f.sftp_profiles.contains_key(id))
            }) {
                NewConnectionTransport::StandaloneSftp
            } else {
                NewConnectionTransport::Ssh
            }
        });
        if env
            .form
            .browser
            .is_some_and(|caps| !caps.supports(transport))
        {
            return;
        }
        let mut draft = NewConnectionForm::for_transport(transport);
        if let Some(group) = group {
            draft.group = group;
        }
        if draft.transport == NewConnectionTransport::Ble
            && let Some(paired) = &self.paired_ble
        {
            draft.ble.web_device_id = Some(paired.id.clone());
            draft.ble.web_device_name = Some(paired.name.clone());
            draft.ble.web_services = paired.services.clone();
            draft.ble.selected_service_uuid = paired
                .services
                .iter()
                .find(|s| (s.has_write || s.has_write_without_response) && s.has_notify)
                .or_else(|| paired.services.first())
                .map(|s| s.uuid);
            if draft.ble.profile_name.is_empty() {
                draft.ble.profile_name = paired.name.clone();
            }
        }
        let form = if let Some(id) = id {
            if let Some(p) = self.files.as_ref().and_then(|f| f.sftp_profiles.get(id)) {
                let mut form =
                    NewConnectionForm::for_transport(NewConnectionTransport::StandaloneSftp);
                form.standalone_sftp_profile_id = Some(id.clone());
                form.host = p.primary.host.clone();
                form.port = p.primary.port.to_string();
                form.username = p.primary.username.clone();
                form.sftp_initial_remote_path = p.primary.path.clone();
                if let Some(second) = &p.secondary {
                    form.set_standalone_sftp_transfer_mode(
                        StandaloneSftpTransferMode::RemoteRemote,
                    );
                    let s = &mut form.standalone_sftp_secondary;
                    s.host = second.host.clone();
                    s.port = second.port.to_string();
                    s.username = second.username.clone();
                    s.initial_remote_path = second.path.clone();
                }
                if let Some(row) = self.catalog_data.items.iter().find(|r| r.id() == id) {
                    form.name = row.name.clone();
                    self.restore_profile(&mut form, row);
                }
                cx.new(|cx| PortableConnectionFormView::from_form(form, env, cx))
            } else if let Some(ble) = self.ble_profiles.get(id).cloned() {
                let mut ble_draft = NewConnectionForm::for_transport(NewConnectionTransport::Ble);
                if let Some(row) = self.catalog_data.items.iter().find(|r| r.id() == id) {
                    self.restore_profile(&mut ble_draft, row);
                }
                ble_draft.ble.profile_id = Some(id.clone());
                ble_draft.ble.profile_name = ble.name.clone();
                ble_draft.ble.web_device_id = ble.device_id.clone();
                ble_draft.ble.web_device_name = Some(ble.device_name.clone());
                ble_draft.ble.selected_service_uuid = ble.config.service_uuid;
                ble_draft.ble.input_line_ending = ble.config.input_line_ending;
                ble_draft.ble.output_line_ending = ble.config.output_line_ending;
                ble_draft.ble.local_echo = ble.config.local_echo;
                ble_draft.ble.write_without_response = ble.config.write_without_response;
                if let Some(mtu) = ble.config.mtu {
                    ble_draft.ble.mtu = mtu.to_string();
                }
                if let Some(paired) = &self.paired_ble
                    && ble.device_id.as_deref() == Some(paired.id.as_str())
                {
                    ble_draft.ble.web_services = paired.services.clone();
                }
                cx.new(|cx| PortableConnectionFormView::from_form(ble_draft, env, cx))
            } else {
                let Some(row) = self
                    .catalog_data
                    .items
                    .iter()
                    .find(|r| r.target == SessionManagerOpenTarget::Connection(id.clone()))
                else {
                    return;
                };
                cx.new(|cx| {
                    PortableConnectionFormView::edit_saved_ssh(
                        id.clone(),
                        self.ssh_form(row),
                        env,
                        cx,
                    )
                })
            }
        } else {
            cx.new(|cx| PortableConnectionFormView::from_form(draft, env, cx))
        };
        self.show_form(form, w, cx);
    }

    fn show_form(
        &mut self,
        form: Entity<PortableConnectionFormView>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(&form, w, |h, form_entity, event, w, cx| {
            use PortableConnectionFormEvent::*;
            match event {
                RequestBleScan => {
                    let Some(draft) = form_entity.read(cx).form(cx) else {
                        return;
                    };
                    if draft.transport != NewConnectionTransport::Ble || draft.ble.scanning {
                        return;
                    }
                    let selection =
                        crate::ble::pair_and_inspect(draft.ble.selected_service_uuid, None);
                    form_entity.update(cx, |v, cx| v.set_web_ble_scanning(true, None, cx));
                    let form_weak = form_entity.downgrade();
                    h.ble_scan = Some(cx.spawn(async move |owner, cx| {
                        let result = selection.await;
                        let _ = owner.update(cx, |h, cx| {
                            let Some(form) = form_weak.upgrade() else {
                                return;
                            };
                            if !form
                                .read(cx)
                                .form(cx)
                                .is_some_and(|f| f.transport == NewConnectionTransport::Ble)
                            {
                                form.update(cx, |v, cx| v.set_web_ble_scanning(false, None, cx));
                                return;
                            }
                            match result {
                                Ok(Some(paired)) => {
                                    let id = paired.id.clone();
                                    let name = paired.name.clone();
                                    let services = paired.services.clone();
                                    h.paired_ble = Some(paired);
                                    form.update(cx, |v, cx| {
                                        v.set_web_ble_device(id, name, services, cx);
                                    });
                                }
                                Ok(None) => {
                                    form.update(cx, |v, cx| v.set_web_ble_scanning(false, None, cx))
                                }
                                Err(err) => {
                                    form.update(cx, |v, cx| {
                                        v.set_web_ble_scanning(false, Some(h.i18n.t(err)), cx);
                                    });
                                }
                            }
                            h.changed(cx);
                        });
                    }));
                }
                Save(d) | SaveAndConnect(d) | Connect(d) => {
                    if let PortableProtocolDraft::Ble {
                        device_id,
                        device_name,
                        service_uuid,
                        input_line_ending,
                        output_line_ending,
                        local_echo,
                        mtu,
                        write_without_response,
                    } = &d.protocol
                    {
                        let id = d
                            .profile_id
                            .clone()
                            .unwrap_or_else(|| format!("ble:{}", uuid::Uuid::new_v4()));
                        let fallback_name = if !device_name.trim().is_empty() {
                            device_name.trim().to_owned()
                        } else {
                            h.i18n.t("modals.new_connection.transport_ble")
                        };
                        let name = if d.name.trim().is_empty() {
                            fallback_name.clone()
                        } else {
                            d.name.trim().to_owned()
                        };
                        let profile = BleProfile {
                            name: name.clone(),
                            device_id: device_id.clone(),
                            device_name: fallback_name,
                            config: BleSessionConfig {
                                service_uuid: *service_uuid,
                                input_line_ending: *input_line_ending,
                                output_line_ending: *output_line_ending,
                                local_echo: *local_echo,
                                mtu: *mtu,
                                write_without_response: *write_without_response,
                            },
                        };
                        if !matches!(event, Connect(_)) {
                            h.ble_profiles.insert(id.clone(), profile.clone());
                            let target = SessionManagerOpenTarget::Ble(id.clone());
                            let mut row = CatalogRow::new(target.clone(), &name);
                            row.host = profile.device_name.clone();
                            row.port = 0;
                            row.subtitle = service_uuid
                                .map(known_service_label)
                                .unwrap_or_else(|| "BLE GATT".into());
                            h.save_profile(row, d, cx);
                        }
                        h.close_form(cx);
                        if !matches!(event, Save(_)) {
                            h.open_ble_terminal(id, profile, w, cx);
                        } else if h.demo {
                            h.open_page(TabKind::SessionManager, Action::Catalog.key(), cx);
                        }
                        h.changed(cx);
                        return;
                    }
                    let (endpoint, sftp, profile) = match &d.protocol {
                        PortableProtocolDraft::Ssh(e, o) => (e, o.open_sftp_on_connect, None),
                        PortableProtocolDraft::StandaloneSftp { primary, secondary } => (
                            primary,
                            true,
                            Some(SftpProfile {
                                primary: primary.into(),
                                secondary: secondary.as_ref().map(Into::into),
                            }),
                        ),
                        _ => {
                            h.notice("remote_desktop.capability_unsupported", cx);
                            return;
                        }
                    };
                    let id = d
                        .profile_id
                        .clone()
                        .unwrap_or_else(|| format!("profile-{}", uuid::Uuid::new_v4()));
                    if !matches!(event, Connect(_)) {
                        let target = if let Some(p) = &profile {
                            let Some(files) = &mut h.files else {
                                h.notice("remote_desktop.capability_unsupported", cx);
                                return;
                            };
                            files.sftp_profiles.insert(id.clone(), p.clone());
                            SessionManagerOpenTarget::StandaloneSftp(id.clone())
                        } else {
                            SessionManagerOpenTarget::Connection(id.clone())
                        };
                        let mut row = CatalogRow::new(
                            target.clone(),
                            if d.name.is_empty() {
                                &endpoint.host
                            } else {
                                &d.name
                            },
                        );
                        row.host = endpoint.host.clone();
                        row.port = endpoint.port.into();
                        row.username = endpoint.username.clone();
                        row.subtitle = if let Some(p) = &profile
                            && let Some(second) = &p.secondary
                        {
                            format!("{} ↔ {}", p.primary.label(), second.label())
                        } else {
                            format!("{}@{}:{}", row.username, row.host, row.port)
                        };
                        h.save_profile(row, d, cx);
                    }
                    h.close_form(cx);
                    if !matches!(event, Save(_)) {
                        if profile.is_some() {
                            h.open_sftp_target(SftpTarget { id, profile }, cx)
                        } else {
                            h.new_terminal_with_sftp(id, None, Some(sftp), w, cx)
                        }
                    } else {
                        h.open_page(TabKind::SessionManager, Action::Catalog.key(), cx);
                    }
                }
                Cancel => h.close_form(cx),
                _ => h.notice("remote_desktop.capability_unsupported", cx),
            }
            h.changed(cx);
        })
        .detach();
        w.focus(&form.read(cx).focus_handle(cx), cx);
        self.form_close_generation = self.form_close_generation.wrapping_add(1);
        self.form = Some(form);
        self.ble_scan = None;
        self.changed(cx);
    }
    pub fn close_form(&mut self, cx: &mut Context<Self>) {
        self.ble_scan = None;
        let Some(form) = self.form.clone() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::exit_settle_duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.form_close_generation = self.form_close_generation.wrapping_add(1);
        let generation = self.form_close_generation;
        if delay.is_zero() || !form.update(cx, |f, cx| f.begin_exit(cx)) {
            self.form = None;
            cx.notify();
            return;
        }
        oxideterm_gpui_shell::overlays::transients::at_deadline(
            web_time::Instant::now() + delay,
            cx,
            move |h, _, cx| {
                if h.form_close_generation == generation {
                    h.form = None;
                    cx.notify();
                }
            },
        )
        .detach();
        cx.notify();
    }
    pub fn record_connection_use(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(row) = self.catalog_data.items.iter_mut().find(|r| r.id() == id) {
            row.last_used = Some(js_sys::Date::new_0().to_iso_string().into());
            if self.catalog_storage.can_record_use() {
                self.persist_catalog(cx);
            }
            self.publish_catalog(cx);
        }
    }

    fn refresh_catalog(&mut self, cx: &mut Context<Self>) {
        self.persist_catalog(cx);
        self.publish_catalog(cx);
    }

    fn publish_catalog(&self, cx: &mut Context<Self>) {
        self.catalog.update(cx, |v, cx| {
            v.set_catalog_data(self.catalog_data.clone(), cx)
        });
        cx.notify();
    }
}
