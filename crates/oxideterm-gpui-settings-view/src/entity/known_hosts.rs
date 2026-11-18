use super::*;
use gpui::ScrollHandle;
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
