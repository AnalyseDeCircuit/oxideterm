use super::*;
use std::{path::Path, rc::Rc};

use crate::{
    components::SettingsControls,
    known_hosts_page::{KnownHostsRequest, PortableKnownHostEntry, PortableKnownHostsSnapshot},
};
use gpui::{AnyElement, App, ScrollHandle};
use oxideterm_ssh::{KnownHostEntry, KnownHostsSnapshot, list_known_hosts, remove_host_key};

#[derive(Default)]
pub struct KnownHostsState {
    pub snapshot: Option<KnownHostsSnapshot>,
    pub error: Option<String>,
    pub query: String,
    pub pending_delete: Option<KnownHostEntry>,
    pub task: Option<Task<()>>,
    pub scroll: ScrollHandle,
}

impl From<&KnownHostEntry> for PortableKnownHostEntry {
    fn from(entry: &KnownHostEntry) -> Self {
        Self {
            host: entry.host.clone(),
            port: entry.port,
            key_type: entry.key_type.clone(),
            fingerprint: entry.fingerprint.clone(),
            line_number: None,
            can_remove: entry.can_remove,
        }
    }
}

impl SettingsControls<'_> {
    pub fn settings_known_hosts_section(
        &self,
        settings: &gpui::Entity<SettingsWorkspaceEntity>,
        open_file: impl Fn(&Path, &mut App) + 'static,
        cx: &App,
    ) -> AnyElement {
        let state = &settings.read(cx).known_hosts;
        let file = state.snapshot.as_ref();
        let snapshot = PortableKnownHostsSnapshot {
            entries: file
                .map(|s| s.entries.iter().map(Into::into).collect())
                .unwrap_or_default(),
            source: file.map(|s| s.path.display().to_string()),
            loading: state.task.is_some(),
            error: state.error.clone(),
        };
        let owner = settings.downgrade();
        let open = file.map(|s| {
            let path = s.path.clone();
            let click: Rc<dyn Fn(&mut App)> = Rc::new(move |cx| open_file(&path, cx));
            (s.exists, click)
        });
        self.known_hosts_list(
            &snapshot,
            &state.query,
            state
                .pending_delete
                .as_ref()
                .map(PortableKnownHostEntry::from)
                .as_ref(),
            &state.scroll,
            true,
            open,
            Rc::new(move |request, cx| {
                let _ = owner.update(cx, |settings, cx| match request {
                    KnownHostsRequest::Refresh => settings.refresh_known_hosts(cx),
                    KnownHostsRequest::Select(entry) => {
                        settings.known_hosts.pending_delete = entry.map(|entry| KnownHostEntry {
                            host: entry.host,
                            port: entry.port,
                            key_type: entry.key_type,
                            fingerprint: entry.fingerprint,
                            can_remove: entry.can_remove,
                        });
                        cx.notify();
                    }
                    KnownHostsRequest::Delete(entry) => {
                        if settings
                            .known_hosts
                            .pending_delete
                            .as_ref()
                            .map(PortableKnownHostEntry::from)
                            == Some(entry)
                        {
                            settings.delete_known_host(cx);
                        }
                    }
                });
            }),
            cx,
        )
    }
}

impl SettingsWorkspaceEntity {
    pub fn refresh_known_hosts(&mut self, cx: &mut Context<Self>) {
        if self.known_hosts.task.is_some() {
            return;
        }
        self.known_hosts.pending_delete = None;
        self.known_hosts.error = None;
        let read = cx
            .background_executor()
            .spawn(async { list_known_hosts().map_err(|error| error.to_string()) });
        self.known_hosts.task = Some(cx.spawn(async move |settings, cx| {
            let result = read.await;
            let _ = settings.update(cx, |settings, cx| {
                settings.known_hosts.task = None;
                match result {
                    Ok(snapshot) => settings.known_hosts.snapshot = Some(snapshot),
                    Err(error) => settings.known_hosts.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn delete_known_host(&mut self, cx: &mut Context<Self>) {
        if self.known_hosts.task.is_some() {
            return;
        }
        let Some(entry) = self
            .known_hosts
            .pending_delete
            .take()
            .filter(|entry| entry.can_remove)
        else {
            return;
        };
        let Some(port) = entry.port else {
            return;
        };
        self.known_hosts.error = None;
        // The captured identity is the row confirmed by the user, not its current list index.
        let remove = cx.background_executor().spawn(async move {
            remove_host_key(&entry.host, port, &entry.key_type, &entry.fingerprint)
                .and_then(|()| list_known_hosts())
                .map_err(|error| error.to_string())
        });
        self.known_hosts.task = Some(cx.spawn(async move |settings, cx| {
            let result = remove.await;
            let _ = settings.update(cx, |settings, cx| {
                settings.known_hosts.task = None;
                match result {
                    Ok(snapshot) => settings.known_hosts.snapshot = Some(snapshot),
                    Err(error) => settings.known_hosts.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
