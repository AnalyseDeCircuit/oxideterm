use crate::host::{Action, Workspace};
use gpui::{prelude::*, *};
use oxideterm_gpui_connections::views::portable_form::PortableProtocolDraft;
use oxideterm_gpui_connections::{NewConnectionForm, session_manager::*, views::*};
use oxideterm_workspace::TabKind;

pub fn catalog(demo: bool) -> PortableCatalogData {
    let mut data = PortableCatalogData::default();
    if demo {
        let mut row = CatalogRow::new(
            SessionManagerOpenTarget::Connection("demo".into()),
            "OxideTerm",
        );
        row.host = "oxide.demo".into();
        row.username = "demo".into();
        row.port = 22;
        row.subtitle = "demo@oxide.demo:22".into();
        data.items.push(row);
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
        if !self.demo {
            self.notice("remote_desktop.capability_unsupported", cx);
            return;
        }
        let mut draft = NewConnectionForm::default();
        if let Some(group) = group {
            draft.group = group;
        }
        let env = PortableConnectionFormEnvironment::new(self.connection_env());
        let form = if let Some(id) = id {
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
        } else {
            cx.new(|cx| PortableConnectionFormView::from_form(draft, env, cx))
        };
        cx.subscribe_in(&form, w, |h, _, event, w, cx| {
            use PortableConnectionFormEvent::*;
            match event {
                Save(d) | SaveAndConnect(d) | Connect(d) => {
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
                    h.form = None;
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
                Cancel => h.form = None,
                _ => h.notice("remote_desktop.capability_unsupported", cx),
            }
            h.changed(cx);
        })
        .detach();
        w.focus(&form.read(cx).focus_handle(cx), cx);
        self.form = Some(form);
        self.changed(cx);
    }
    fn refresh_catalog(&self, cx: &mut App) {
        self.catalog.update(cx, |v, cx| {
            v.set_catalog_data(self.catalog_data.clone(), cx)
        });
    }
}
