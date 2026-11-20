use gpui::{App, Context, Entity};
use oxideterm_connections::ConnectionStore;
use oxideterm_i18n::I18n;
use oxideterm_terminal_triggers::SavedConnectionKind;
use std::ops::DerefMut;

use super::{
    SessionManagerDeleteConfirm as Confirm, SessionManagerRowActionTarget as Row,
    SessionManagerSelectionTarget as Target, SessionManagerState, SessionManagerWorkspaceEvent,
    close_session_menu_state, connections_deleted_label,
};

impl SessionManagerState {
    pub fn request_row_delete(
        &mut self,
        target: Row,
        store: &ConnectionStore,
        cx: &mut Context<Self>,
    ) {
        let confirm = match target {
            Row::Group(group) => {
                self.request_delete_session_group(group, cx);
                return;
            }
            Row::Connection(id) => store.get(&id).map(|p| Confirm::Single {
                id,
                name: p.name.clone(),
            }),
            Row::Serial(id) => store
                .serial_profiles()
                .iter()
                .find(|p| p.id == id)
                .map(|p| Confirm::SerialProfile {
                    id,
                    name: p.name.clone(),
                }),
            Row::Ble(id) => {
                store
                    .ble_profiles()
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| Confirm::BleProfile {
                        id,
                        name: p.name.clone(),
                    })
            }
            Row::LocalTerminal(id) => store
                .local_terminal_profiles()
                .iter()
                .find(|p| p.id == id)
                .map(|p| Confirm::LocalTerminalProfile {
                    id,
                    name: p.name.clone(),
                }),
            Row::Telnet(id) => store
                .telnet_profiles()
                .iter()
                .find(|p| p.id == id)
                .map(|p| Confirm::TelnetProfile {
                    id,
                    name: p.name.clone(),
                }),
            Row::Mosh(id) => store.get_mosh_profile(&id).map(|p| Confirm::MoshProfile {
                id,
                name: p.name.clone(),
            }),
            Row::StandaloneSftp(id) => {
                store
                    .get_standalone_sftp_profile(&id)
                    .map(|p| Confirm::StandaloneSftpProfile {
                        id,
                        name: p.name.clone(),
                    })
            }
            Row::RemoteDesktop(id) => {
                store
                    .get_remote_desktop_profile(&id)
                    .map(|p| Confirm::RemoteDesktopProfile {
                        id,
                        name: p.name.clone(),
                    })
            }
            Row::Ftp(id) => Some(Confirm::Batch {
                targets: vec![Target::Ftp(id)],
            }),
            _ => None,
        };
        if let Some(confirm) = confirm {
            self.open_delete_confirmation(confirm, cx);
        }
    }

    pub fn request_selected_delete(&mut self, cx: &mut Context<Self>) {
        if !self.selected_items.is_empty() {
            self.open_delete_confirmation(
                Confirm::Batch {
                    targets: self.selected_items.iter().cloned().collect(),
                },
                cx,
            );
        }
    }

    fn open_delete_confirmation(&mut self, confirm: Confirm, cx: &mut Context<Self>) {
        close_session_menu_state(self);
        self.delete_confirm = Some(confirm);
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
    }

    pub fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        if matches!(self.delete_confirm, Some(Confirm::Group { .. })) {
            self.show_group_manager = self.reopen_group_manager_after_delete;
            self.reopen_group_manager_after_delete = false;
        }
        self.take_delete(cx);
    }

    pub fn take_delete(&mut self, cx: &mut Context<Self>) -> Option<Confirm> {
        let confirmed = self.delete_confirm.take();
        if confirmed.is_some() {
            cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
            cx.notify();
        }
        confirmed
    }

    pub fn finish_single_delete(
        &mut self,
        target: Target,
        result: anyhow::Result<bool>,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let (success, failure) = match &target {
            Target::Connection(_) => ("sessionManager.toast.connection_deleted", ""),
            Target::Serial(_) => (
                "sessionManager.serial_profiles.delete",
                "sessionManager.serial_profiles.delete_failed",
            ),
            Target::Ble(_) => (
                "sessionManager.ble_profiles.delete",
                "sessionManager.ble_profiles.delete_failed",
            ),
            Target::LocalTerminal(_) => (
                "sessionManager.local_terminal_profiles.delete",
                "sessionManager.local_terminal_profiles.delete_failed",
            ),
            Target::Telnet(_) => (
                "sessionManager.telnet_profiles.delete",
                "sessionManager.telnet_profiles.delete_failed",
            ),
            Target::Mosh(_) => (
                "sessionManager.mosh_profiles.delete",
                "sessionManager.mosh_profiles.delete_failed",
            ),
            Target::StandaloneSftp(_) => (
                "sessionManager.standalone_sftp_profiles.delete",
                "sessionManager.standalone_sftp_profiles.delete_failed",
            ),
            Target::RemoteDesktop(_) => (
                "sessionManager.remote_desktop_profiles.delete",
                "sessionManager.remote_desktop_profiles.delete_failed",
            ),
            _ => return false,
        };
        let changed = matches!(result, Ok(true));
        if changed {
            self.selected_items.remove(&target);
        }
        let status = match result {
            Ok(true) => i18n.t(success),
            Ok(false) => i18n.t(failure),
            Err(error) if failure.is_empty() => error.to_string(),
            Err(error) => format!("{}: {error}", i18n.t(failure)),
        };
        self.status = Some(status);
        cx.notify();
        changed
    }

    pub fn finish_batch_delete(&mut self, deleted: usize, i18n: &I18n, cx: &mut Context<Self>) {
        self.selected_items.clear();
        self.selection_anchor = None;
        self.show_batch_move = false;
        self.status = (deleted > 0).then(|| connections_deleted_label(i18n, deleted));
        cx.notify();
    }
}

fn delete_catalog_profile(store: &mut ConnectionStore, target: &Target) -> anyhow::Result<bool> {
    match target {
        Target::Plugin(id) => store.delete_plugin_profile(id),
        Target::Ble(id) => store.delete_ble_profile(id),
        Target::LocalTerminal(id) => store.delete_local_terminal_profile(id),
        Target::Ftp(id) => store.delete_ftp_profile(id),
        Target::StandaloneSftp(id) => store.delete_standalone_sftp_profile(id),
        Target::RemoteDesktop(id) => store.delete_remote_desktop_profile(id),
        _ => Ok(false),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavedConsoleProfileKind {
    Telnet,
    Mosh,
    Serial,
}

impl SavedConsoleProfileKind {
    pub fn connection_kind(self) -> SavedConnectionKind {
        match self {
            Self::Telnet => SavedConnectionKind::Telnet,
            Self::Mosh => SavedConnectionKind::Mosh,
            Self::Serial => SavedConnectionKind::Serial,
        }
    }
}

pub enum CatalogRemoval<'a> {
    Completed(bool),
    DeleteConsole {
        id: &'a str,
        kind: SavedConsoleProfileKind,
    },
    StopTunnel(&'a str),
    ReleaseSavedSsh(&'a str),
}

pub fn remove_catalog_target<'a>(
    store: &mut ConnectionStore,
    target: &'a Target,
    single: bool,
) -> anyhow::Result<CatalogRemoval<'a>> {
    Ok(match target {
        Target::Serial(id) => CatalogRemoval::DeleteConsole {
            id,
            kind: SavedConsoleProfileKind::Serial,
        },
        Target::Telnet(id) => CatalogRemoval::DeleteConsole {
            id,
            kind: SavedConsoleProfileKind::Telnet,
        },
        Target::Mosh(id) => CatalogRemoval::DeleteConsole {
            id,
            kind: SavedConsoleProfileKind::Mosh,
        },
        Target::Tunnel(id) => CatalogRemoval::StopTunnel(id),
        Target::Connection(id) => {
            let removed = match store.delete(id) {
                Err(_) if !single => return Ok(CatalogRemoval::Completed(false)),
                result => result?,
            };
            if removed || single {
                // A single delete still retires owner-bound resources if the saved row is gone.
                CatalogRemoval::ReleaseSavedSsh(id)
            } else {
                CatalogRemoval::Completed(false)
            }
        }
        _ => CatalogRemoval::Completed(delete_catalog_profile(store, target)?),
    })
}

pub enum CatalogDeletion {
    Single(Target),
    Batch(Vec<Target>),
    Group(String),
}

impl SessionManagerState {
    pub fn take_catalog_deletion(&mut self, cx: &mut Context<Self>) -> Option<CatalogDeletion> {
        let target = match self.take_delete(cx)? {
            Confirm::Single { id, .. } => Target::Connection(id),
            Confirm::SerialProfile { id, .. } => Target::Serial(id),
            Confirm::BleProfile { id, .. } => Target::Ble(id),
            Confirm::LocalTerminalProfile { id, .. } => Target::LocalTerminal(id),
            Confirm::TelnetProfile { id, .. } => Target::Telnet(id),
            Confirm::MoshProfile { id, .. } => Target::Mosh(id),
            Confirm::StandaloneSftpProfile { id, .. } => Target::StandaloneSftp(id),
            Confirm::RemoteDesktopProfile { id, .. } => Target::RemoteDesktop(id),
            Confirm::Batch { targets } => return Some(CatalogDeletion::Batch(targets)),
            Confirm::Group { name } => return Some(CatalogDeletion::Group(name)),
        };
        Some(CatalogDeletion::Single(target))
    }
}

pub fn delete_catalog_selection<C: DerefMut<Target = App>>(
    manager: &Entity<SessionManagerState>,
    targets: Vec<Target>,
    i18n: &I18n,
    cx: &mut C,
    mut remove: impl FnMut(&Target, &mut C) -> anyhow::Result<bool>,
) -> bool {
    let mut deleted = 0;
    for target in targets {
        match remove(&target, cx) {
            Ok(true) => deleted += 1,
            Ok(false) => {}
            Err(error) => {
                let status = match &target {
                    Target::Connection(_) | Target::Ftp(_) => error.to_string(),
                    Target::Plugin(_) => i18n.t("plugin.connections.failed"),
                    _ => continue,
                };
                manager.update(&mut **cx, |state, cx| state.set_status(Some(status), cx));
                if !matches!(target, Target::Ftp(_)) {
                    return false;
                }
            }
        }
    }
    manager.update(&mut **cx, |state, cx| {
        state.finish_batch_delete(deleted, i18n, cx)
    });
    deleted > 0
}
