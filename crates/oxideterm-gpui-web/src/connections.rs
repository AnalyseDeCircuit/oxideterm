use crate::{
    ble::{BleProfile, BleSessionConfig, is_ble_node, known_service_label},
    host::{Action, Workspace},
};
use gpui::{prelude::*, *};
use oxideterm_gpui_connections::form_state::NewConnectionTransport;
use oxideterm_gpui_connections::views::portable_form::PortableProtocolDraft;
use oxideterm_gpui_connections::{NewConnectionForm, session_manager::*, views::*};
use oxideterm_workspace::TabKind;

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

impl Workspace {
    pub fn connection_env(&self) -> ConnectionsEnvironment {
        ConnectionsEnvironment {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
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
                if *open_sftp {
                    self.action(Action::Sftp, w, cx)
                } else {
                    self.new_terminal(id.clone(), None, w, cx)
                }
            }
            Edit(SessionManagerOpenTarget::Connection(id)) => self.form(Some(id), None, w, cx),
            CreateConnection { group } => self.form(None, group.clone(), w, cx),
            Delete(targets) => {
                self.catalog_data.items.retain(|r|!targets.iter().any(|t|matches!((t,&r.target),(SessionManagerSelectionTarget::Connection(a),SessionManagerOpenTarget::Connection(b)) if a==b)));
                for target in targets {
                    if let SessionManagerSelectionTarget::Connection(id) = target {
                        self.ble_profiles.remove(id);
                    }
                }
                self.refresh_catalog(cx);
            }
            CreateGroup(group) => {
                self.catalog_data.groups.push(group.clone());
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

    fn form_with_transport(
        &mut self,
        id: Option<&String>,
        group: Option<String>,
        transport: Option<NewConnectionTransport>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_ble = transport == Some(NewConnectionTransport::Ble)
            || id.is_some_and(|pid| is_ble_node(pid));
        if !self.demo && !is_ble {
            self.notice("remote_desktop.capability_unsupported", cx);
            return;
        }
        let mut draft = match transport {
            Some(t) => NewConnectionForm::for_transport(t),
            None => NewConnectionForm::default(),
        };
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
        let env = PortableConnectionFormEnvironment::new(self.connection_env());
        let form = if let Some(id) = id {
            if let Some(ble) = self.ble_profiles.get(id).cloned() {
                let mut ble_draft = NewConnectionForm::for_transport(NewConnectionTransport::Ble);
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
                draft.name = row.name.clone();
                draft.host = row.host.clone();
                draft.port = row.port.to_string();
                draft.username = row.username.clone();
                draft.group = row.group.clone().unwrap_or_default();
                cx.new(|cx| PortableConnectionFormView::edit_saved_ssh(id.clone(), draft, env, cx))
            }
        } else {
            cx.new(|cx| PortableConnectionFormView::from_form(draft, env, cx))
        };
        cx.subscribe_in(&form, w, |h, form_entity, event, w, cx| {
            use PortableConnectionFormEvent::*;
            match event {
                RequestBleScan => {
                    let extra_service = form_entity
                        .read(cx)
                        .form(cx)
                        .and_then(|f| f.ble.selected_service_uuid);
                    form_entity.update(cx, |v, cx| v.set_web_ble_scanning(true, None, cx));
                    let form_weak = form_entity.downgrade();
                    cx.spawn(async move |owner, cx| {
                        let result = crate::ble::pair_and_inspect(extra_service).await;
                        let _ = owner.update(cx, |h, cx| {
                            match result {
                                Ok(paired) => {
                                    let id = paired.id.clone();
                                    let name = paired.name.clone();
                                    let services = paired.services.clone();
                                    h.paired_ble = Some(paired);
                                    let _ = form_weak.update(cx, |v, cx| {
                                        v.set_web_ble_device(id, name, services, cx);
                                    });
                                }
                                Err(err) => {
                                    let _ = form_weak.update(cx, |v, cx| {
                                        v.set_web_ble_scanning(false, Some(err), cx);
                                    });
                                }
                            }
                            h.changed(cx);
                        });
                    })
                    .detach();
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
                        let id = d.profile_id.clone().unwrap_or_else(|| {
                            h.next_profile += 1;
                            format!("ble:{}", h.next_profile)
                        });
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
                        h.ble_profiles.insert(id.clone(), profile.clone());
                        if !matches!(event, Connect(_)) {
                            let target = SessionManagerOpenTarget::Connection(id.clone());
                            let mut row = CatalogRow::new(target.clone(), &name);
                            row.host = profile.device_name.clone();
                            row.port = 0;
                            row.subtitle = service_uuid
                                .map(known_service_label)
                                .unwrap_or_else(|| "BLE GATT".into());
                            row.group = (!d.group.is_empty()).then(|| d.group.clone());
                            row.tags = d.tags.clone();
                            if let Some(r) =
                                h.catalog_data.items.iter_mut().find(|r| r.target == target)
                            {
                                *r = row;
                            } else {
                                h.catalog_data.items.push(row);
                            }
                            h.refresh_catalog(cx);
                        }
                        h.close_form(cx);
                        if !matches!(event, Save(_)) {
                            h.open_ble_terminal(id, profile, w, cx);
                        } else {
                            h.open_page(TabKind::SessionManager, Action::Catalog.key(), cx);
                        }
                        h.changed(cx);
                        return;
                    }
                    let (endpoint, sftp) = match &d.protocol {
                        PortableProtocolDraft::Ssh(e, o) => (e, o.open_sftp_on_connect),
                        PortableProtocolDraft::StandaloneSftp { primary, .. } => (primary, true),
                        _ => {
                            h.notice("remote_desktop.capability_unsupported", cx);
                            return;
                        }
                    };
                    let id = d.profile_id.clone().unwrap_or_else(|| {
                        h.next_profile += 1;
                        format!("profile-{}", h.next_profile)
                    });
                    if !matches!(event, Connect(_)) {
                        let target = SessionManagerOpenTarget::Connection(id.clone());
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
                        row.subtitle = format!("{}@{}:{}", row.username, row.host, row.port);
                        row.group = (!d.group.is_empty()).then(|| d.group.clone());
                        row.tags = d.tags.clone();
                        if let Some(r) =
                            h.catalog_data.items.iter_mut().find(|r| r.target == target)
                        {
                            *r = row
                        } else {
                            h.catalog_data.items.push(row)
                        }
                        h.refresh_catalog(cx);
                    }
                    h.close_form(cx);
                    if !matches!(event, Save(_)) {
                        if sftp {
                            h.action(Action::Sftp, w, cx)
                        } else {
                            h.new_terminal(id, None, w, cx)
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
        self.changed(cx);
    }
    pub fn close_form(&mut self, cx: &mut Context<Self>) {
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
    fn refresh_catalog(&self, cx: &mut Context<Self>) {
        self.catalog.update(cx, |v, cx| {
            v.set_catalog_data(self.catalog_data.clone(), cx)
        });
        cx.notify();
    }
}
