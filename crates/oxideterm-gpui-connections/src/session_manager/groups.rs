use super::*;
use gpui::{App, Context, Entity};
use oxideterm_connections::ConnectionStore;
use oxideterm_forwarding::{PersistedForward, SavedForwardStore};
use oxideterm_i18n::I18n;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionManagerMoveInteraction {
    BatchMenu,
    DragDrop,
}
pub fn connection_count_for_group(
    group: &str,
    store: &ConnectionStore,
    tunnels: &[PersistedForward],
) -> usize {
    let connection_count = store
        .connections()
        .iter()
        .filter(|conn| {
            conn.group
                .as_deref()
                .is_some_and(|candidate| session_group_path_is_within(candidate, group))
        })
        .count();
    let serial_count = store
        .serial_profiles()
        .iter()
        .filter(|profile| {
            profile
                .group
                .as_deref()
                .is_some_and(|candidate| session_group_path_is_within(candidate, group))
        })
        .count();
    let local_count = store
        .local_terminal_profiles()
        .iter()
        .filter(|profile| {
            profile
                .group
                .as_deref()
                .is_some_and(|candidate| session_group_path_is_within(candidate, group))
        })
        .count();
    let telnet_count = store
        .telnet_profiles()
        .iter()
        .filter(|profile| {
            profile
                .group
                .as_deref()
                .is_some_and(|candidate| session_group_path_is_within(candidate, group))
        })
        .count();
    let mosh_count = store
        .mosh_profiles()
        .iter()
        .filter(|profile| {
            profile
                .group
                .as_deref()
                .is_some_and(|candidate| session_group_path_is_within(candidate, group))
        })
        .count();
    let standalone_sftp_count = store
        .standalone_sftp_profiles()
        .iter()
        .filter(|profile| {
            profile
                .group
                .as_deref()
                .is_some_and(|candidate| session_group_path_is_within(candidate, group))
        })
        .count();
    let remote_desktop_count = store
        .remote_desktop_profiles()
        .iter()
        .filter(|profile| {
            profile
                .group
                .as_deref()
                .is_some_and(|candidate| session_group_path_is_within(candidate, group))
        })
        .count();
    let ble_count = store
        .ble_profiles()
        .iter()
        .filter(|profile| {
            profile.group.as_deref().is_some_and(|candidate| {
                candidate == group || candidate.starts_with(&format!("{group}/"))
            })
        })
        .count();
    connection_count
        + store
            .plugin_profiles()
            .iter()
            .filter(|profile| {
                profile
                    .group
                    .as_deref()
                    .is_some_and(|candidate| session_group_path_is_within(candidate, group))
            })
            .count()
        + store
            .ftp_profiles()
            .iter()
            .filter(|profile| {
                profile
                    .group
                    .as_deref()
                    .is_some_and(|candidate| session_group_path_is_within(candidate, group))
            })
            .count()
        + local_count
        + serial_count
        + telnet_count
        + mosh_count
        + standalone_sftp_count
        + remote_desktop_count
        + ble_count
        + tunnels
            .iter()
            .filter(|saved| {
                saved
                    .tunnel
                    .as_ref()
                    .and_then(|profile| profile.group.as_deref())
                    .is_some_and(|candidate| session_group_path_is_within(candidate, group))
            })
            .count()
}

pub fn session_group_tree(
    store: &ConnectionStore,
    tunnels: &[PersistedForward],
) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let mut paths = HashSet::new();
    for group in store.groups() {
        add_group_path_segments(group, &mut paths);
    }
    for saved in tunnels.iter() {
        if let Some(group) = saved
            .tunnel
            .as_ref()
            .and_then(|profile| profile.group.as_deref())
        {
            add_group_path_segments(group, &mut paths);
        }
    }
    for conn in store.connections() {
        if let Some(group) = conn.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }
    for profile in store.serial_profiles() {
        if let Some(group) = profile.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }
    for profile in store.local_terminal_profiles() {
        if let Some(group) = profile.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }
    for profile in store.telnet_profiles() {
        if let Some(group) = profile.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }
    for profile in store.ftp_profiles() {
        if let Some(group) = profile.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }
    for profile in store.mosh_profiles() {
        if let Some(group) = profile.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }
    for profile in store.standalone_sftp_profiles() {
        if let Some(group) = profile.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }
    for profile in store.remote_desktop_profiles() {
        if let Some(group) = profile.group.as_deref() {
            add_group_path_segments(group, &mut paths);
        }
    }

    let mut sorted = paths.into_iter().collect::<Vec<_>>();
    sorted.sort();
    let mut roots = Vec::new();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for path in sorted {
        if let Some((parent, _name)) = path.rsplit_once('/') {
            children.entry(parent.to_string()).or_default().push(path);
        } else {
            roots.push(path);
        }
    }
    (roots, children)
}

pub struct CatalogGroups<'a> {
    pub manager: &'a Entity<SessionManagerState>,
    pub store: &'a mut ConnectionStore,
    pub i18n: &'a I18n,
    pub tunnels: Vec<PersistedForward>,
    pub forward_store: Option<Arc<SavedForwardStore>>,
    pub wrote: bool,
}

impl SessionManagerState {
    pub fn handle_group_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> (bool, Option<crate::views::ConnectionsWindowRequest>) {
        use crate::views::ConnectionsWindowRequest as Request;
        use SessionManagerBasicDialogFooterAction as Footer;
        use oxideterm_gpui_ui::confirm::{
            ModalFooterInputKeyAction as Key, modal_footer_input_key_action,
        };
        let modifiers = event.keystroke.modifiers;
        if !self.show_group_manager
            || self.group_editor.is_none()
            || modifiers.platform
            || modifiers.control
        {
            return (false, None);
        }
        let request = match modal_footer_input_key_action(
            event.keystroke.key.as_str(),
            modifiers.shift,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            true,
            self.focused_input == Some(SessionManagerInput::GroupName),
            self.focused_basic_dialog_footer_action,
            Footer::Cancel,
            None,
        ) {
            Some(Key::Cancel | Key::Activate(Footer::Cancel)) => {
                self.cancel_group_editor(cx);
                Some(Request::ClearComposition)
            }
            Some(Key::FocusInput) => {
                self.focused_input = Some(SessionManagerInput::GroupName);
                self.focused_basic_dialog_footer_action = None;
                cx.notify();
                Some(Request::ClearComposition)
            }
            Some(Key::FocusFooter(action)) => {
                self.focused_input = None;
                self.focused_basic_dialog_footer_action = Some(action);
                cx.notify();
                Some(Request::ClearComposition)
            }
            Some(Key::Activate(Footer::Primary)) => {
                if !self.can_save_group() {
                    return (true, None);
                }
                self.focused_basic_dialog_footer_action = None;
                cx.notify();
                Some(Request::Catalog(super::CatalogAction::SaveGroup))
            }
            None => return (false, None),
        };
        (true, request)
    }

    pub(super) fn can_save_group(&self) -> bool {
        let Some(editor) = &self.group_editor else {
            return false;
        };
        let parent = match editor {
            SessionManagerGroupEditor::Create { parent_path }
            | SessionManagerGroupEditor::Rename { parent_path, .. } => parent_path.as_deref(),
        };
        let Some(candidate) = session_group_path_from_leaf(parent, self.group_name_draft.trim())
        else {
            return false;
        };
        !matches!(editor, SessionManagerGroupEditor::Rename { old_path, .. } if old_path == &candidate)
    }

    pub fn cancel_group_editor(&mut self, cx: &mut Context<Self>) {
        self.group_editor = None;
        self.group_name_draft.clear();
        self.group_editor_error = None;
        self.focused_input = None;
        self.focused_basic_dialog_footer_action = None;
        cx.notify();
    }
}

impl CatalogGroups<'_> {
    pub fn submit_session_group_editor(&mut self, cx: &mut App) {
        let (leaf_name, editor) = {
            let session_manager = self.manager.read(cx);
            (
                session_manager.group_name_draft.trim().to_string(),
                session_manager.group_editor.clone(),
            )
        };
        let Some(editor) = editor else {
            return;
        };
        let parent_path = match &editor {
            SessionManagerGroupEditor::Create { parent_path }
            | SessionManagerGroupEditor::Rename { parent_path, .. } => parent_path.as_deref(),
        };
        let Some(name) = session_group_path_from_leaf(parent_path, &leaf_name) else {
            let status = self.i18n.t("sessionManager.folder_tree.group_name_invalid");
            self.manager.update(cx, |session_manager, cx| {
                session_manager.group_editor_error = Some(status);
                cx.notify();
            });
            return;
        };
        if let SessionManagerGroupEditor::Rename { old_path, .. } = editor {
            if old_path == name {
                self.cancel_session_group_editor(cx);
                return;
            }
            self.rename_session_group(&old_path, name, cx);
            return;
        }

        match self.store.create_group(name.clone()) {
            Ok(()) => {
                self.manager.update(cx, |session_manager, cx| {
                    session_manager.selected_group = Some(name);
                    expand_group_path(
                        session_manager
                            .selected_group
                            .as_deref()
                            .unwrap_or_default(),
                        &mut session_manager.expanded_groups,
                    );
                    session_manager.group_editor = None;
                    session_manager.group_name_draft.clear();
                    session_manager.group_editor_error = None;
                    session_manager.focused_input = None;
                    session_manager.focused_basic_dialog_footer_action = None;
                    cx.notify();
                });
                self.wrote = true;
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.create_group_failed")
                );
                self.manager.update(cx, |session_manager, cx| {
                    session_manager.group_editor_error = Some(status);
                    cx.notify();
                });
            }
        }
    }

    pub fn open_session_group_creation(&mut self, cx: &mut App) {
        self.open_session_group_creation_at(None, cx);
    }

    pub fn open_session_subgroup_creation(&mut self, parent_path: &str, cx: &mut App) {
        self.open_session_group_creation_at(Some(parent_path.to_string()), cx);
    }

    fn open_session_group_creation_at(&mut self, parent_path: Option<String>, cx: &mut App) {
        self.manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.show_group_manager = true;
            // The editor keeps the parent immutable so users only enter one segment.
            session_manager.group_editor = Some(SessionManagerGroupEditor::Create { parent_path });
            session_manager.group_name_draft.clear();
            session_manager.group_editor_error = None;
            session_manager.group_manager_error = None;
            session_manager.focused_input = Some(SessionManagerInput::GroupName);
            session_manager.focused_basic_dialog_footer_action = None;
            cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
            cx.notify();
        });
    }

    pub fn open_session_group_rename(&mut self, group: &str, cx: &mut App) {
        let (parent_path, leaf_name) = split_session_group_path(group);
        let old_path = group.to_string();
        let parent_path = parent_path.map(ToOwned::to_owned);
        let leaf_name = leaf_name.to_string();
        self.manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.show_group_manager = true;
            session_manager.group_name_draft = leaf_name;
            session_manager.group_editor = Some(SessionManagerGroupEditor::Rename {
                old_path,
                parent_path,
            });
            session_manager.group_editor_error = None;
            session_manager.group_manager_error = None;
            session_manager.focused_input = Some(SessionManagerInput::GroupName);
            session_manager.focused_basic_dialog_footer_action = None;
            cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
            cx.notify();
        });
    }

    fn rename_session_group(&mut self, old_name: &str, new_name: String, cx: &mut App) {
        let previous_tunnel_groups = match self.change_tunnel_group_path(old_name, Some(&new_name))
        {
            Ok(groups) => groups,
            Err(error) => {
                self.manager
                    .update(cx, |state, cx| state.set_status(Some(error), cx));
                return;
            }
        };
        match self.store.rename_group(old_name, new_name.clone()) {
            Ok(_) => {
                self.manager.update(cx, |session_manager, cx| {
                    session_manager.selected_group = session_manager
                        .selected_group
                        .as_deref()
                        .and_then(|group| renamed_session_group_path(group, old_name, &new_name))
                        .or_else(|| session_manager.selected_group.clone());
                    session_manager.expanded_groups = session_manager
                        .expanded_groups
                        .iter()
                        .map(|group| {
                            renamed_session_group_path(group, old_name, &new_name)
                                .unwrap_or_else(|| group.clone())
                        })
                        .collect();
                    expand_group_path(&new_name, &mut session_manager.expanded_groups);
                    session_manager.group_editor = None;
                    session_manager.group_name_draft.clear();
                    session_manager.group_editor_error = None;
                    session_manager.focused_input = None;
                    session_manager.focused_basic_dialog_footer_action = None;
                    cx.notify();
                });
                // Group metadata is persisted independently from live node ownership.
                self.wrote = true;
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.rename_group_failed")
                );
                let status = match self.apply_tunnel_groups(&previous_tunnel_groups) {
                    Ok(_) => status,
                    Err(rollback) => format!("{status}: {rollback}"),
                };
                self.manager.update(cx, |session_manager, cx| {
                    session_manager.group_editor_error = Some(status);
                    cx.notify();
                });
            }
        }
    }

    pub fn request_delete_session_group(&mut self, group: &str, cx: &mut App) {
        let name = group.to_string();
        self.manager.update(cx, |session_manager, cx| {
            close_session_menu_state(session_manager);
            session_manager.reopen_group_manager_after_delete = session_manager.show_group_manager;
            session_manager.show_group_manager = false;
            session_manager.group_editor = None;
            session_manager.group_editor_error = None;
            session_manager.focused_input = None;
            session_manager.focused_basic_dialog_footer_action = None;
            session_manager.delete_confirm = Some(SessionManagerDeleteConfirm::Group { name });
            cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
            cx.notify();
        });
    }

    pub fn close_session_group_manager(&mut self, cx: &mut App) {
        self.manager.update(cx, |session_manager, cx| {
            if session_manager.show_group_manager {
                session_manager.show_group_manager = false;
                session_manager.group_editor = None;
                session_manager.group_name_draft.clear();
                session_manager.group_editor_error = None;
                session_manager.group_manager_error = None;
                session_manager.focused_input = None;
                session_manager.focused_basic_dialog_footer_action = None;
                cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
                cx.notify();
            }
        });
    }

    pub fn delete_session_group(&mut self, group: &str, cx: &mut App) {
        let previous_tunnel_groups = match self.change_tunnel_group_path(group, None) {
            Ok(groups) => groups,
            Err(error) => {
                self.manager
                    .update(cx, |state, cx| state.set_status(Some(error), cx));
                return;
            }
        };
        match self.store.delete_group(group) {
            Ok(()) => {
                self.manager.update(cx, |session_manager, cx| {
                    if session_manager
                        .selected_group
                        .as_deref()
                        .is_some_and(|selected| session_group_path_is_within(selected, group))
                    {
                        session_manager.selected_group = None;
                    }
                    session_manager
                        .expanded_groups
                        .retain(|expanded| !session_group_path_is_within(expanded, group));
                    session_manager.show_group_manager =
                        session_manager.reopen_group_manager_after_delete;
                    session_manager.reopen_group_manager_after_delete = false;
                    session_manager.group_manager_error = None;
                    cx.notify();
                });
                // Deleting a group only reassigns saved metadata; active nodes keep running.
                self.wrote = true;
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.toast.delete_group_failed")
                );
                let status = match self.apply_tunnel_groups(&previous_tunnel_groups) {
                    Ok(_) => status,
                    Err(rollback) => format!("{status}: {rollback}"),
                };
                self.manager.update(cx, |session_manager, cx| {
                    let reopen_manager = session_manager.reopen_group_manager_after_delete;
                    session_manager.show_group_manager = reopen_manager;
                    session_manager.reopen_group_manager_after_delete = false;
                    if reopen_manager {
                        session_manager.group_manager_error = Some(status);
                        cx.notify();
                    } else {
                        session_manager.set_status(Some(status), cx);
                    }
                });
            }
        }
    }

    pub fn move_selected_connections(&mut self, group: Option<&str>, cx: &mut App) {
        let targets = self
            .manager
            .read(cx)
            .selected_items
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        self.move_session_targets_to_group(
            &targets,
            group,
            SessionManagerMoveInteraction::BatchMenu,
            cx,
        );
    }

    pub fn move_dragged_sessions_to_group(
        &mut self,
        targets: &[SessionManagerSelectionTarget],
        group: Option<&str>,
        cx: &mut App,
    ) {
        self.move_session_targets_to_group(
            targets,
            group,
            SessionManagerMoveInteraction::DragDrop,
            cx,
        );
    }

    fn move_session_targets_to_group(
        &mut self,
        targets: &[SessionManagerSelectionTarget],
        group: Option<&str>,
        interaction: SessionManagerMoveInteraction,
        cx: &mut App,
    ) {
        let mut connection_ids = Vec::new();
        let mut serial_profile_ids = Vec::new();
        let mut ble_profile_ids = Vec::new();
        let mut local_terminal_profile_ids = Vec::new();
        let mut telnet_profile_ids = Vec::new();
        let mut ftp_profile_ids = Vec::new();
        let mut mosh_profile_ids = Vec::new();
        let mut standalone_sftp_profile_ids = Vec::new();
        let mut remote_desktop_ids = Vec::new();
        let mut plugin_ids = Vec::new();
        let mut tunnel_ids = Vec::new();
        for target in targets.iter().filter(|target| {
            // Drop evaluation never writes metadata; only assets whose group changes reach storage.
            self.session_target_group(target)
                .is_some_and(|current_group| current_group.as_deref() != group)
        }) {
            match target {
                SessionManagerSelectionTarget::Tunnel(id) => tunnel_ids.push(id.clone()),
                SessionManagerSelectionTarget::Plugin(id) => plugin_ids.push(id.clone()),
                SessionManagerSelectionTarget::Connection(id) => connection_ids.push(id.clone()),
                SessionManagerSelectionTarget::Serial(id) => serial_profile_ids.push(id.clone()),
                SessionManagerSelectionTarget::Ble(id) => ble_profile_ids.push(id.clone()),
                SessionManagerSelectionTarget::LocalTerminal(id) => {
                    local_terminal_profile_ids.push(id.clone())
                }
                SessionManagerSelectionTarget::Telnet(id) => telnet_profile_ids.push(id.clone()),
                SessionManagerSelectionTarget::Ftp(id) => ftp_profile_ids.push(id.clone()),
                SessionManagerSelectionTarget::Mosh(id) => mosh_profile_ids.push(id.clone()),
                SessionManagerSelectionTarget::StandaloneSftp(id) => {
                    standalone_sftp_profile_ids.push(id.clone())
                }
                SessionManagerSelectionTarget::RemoteDesktop(id) => {
                    remote_desktop_ids.push(id.clone())
                }
            }
        }
        let has_group_changes = !connection_ids.is_empty()
            || !tunnel_ids.is_empty()
            || !plugin_ids.is_empty()
            || !ftp_profile_ids.is_empty()
            || !serial_profile_ids.is_empty()
            || !ble_profile_ids.is_empty()
            || !local_terminal_profile_ids.is_empty()
            || !telnet_profile_ids.is_empty()
            || !mosh_profile_ids.is_empty()
            || !standalone_sftp_profile_ids.is_empty()
            || !remote_desktop_ids.is_empty();
        if !has_group_changes {
            if interaction == SessionManagerMoveInteraction::BatchMenu {
                self.manager.update(cx, |session_manager, cx| {
                    session_manager.selected_items.clear();
                    session_manager.selection_anchor = None;
                    session_manager.show_batch_move = false;
                    cx.notify();
                });
            }
            return;
        }
        let previous_tunnel_groups = match self.move_ssh_tunnels_to_group(&tunnel_ids, group) {
            Ok(groups) => groups,
            Err(error) => {
                self.manager
                    .update(cx, |state, cx| state.set_status(Some(error), cx));
                return;
            }
        };
        match self.store.move_session_assets_to_group(
            &connection_ids,
            &serial_profile_ids,
            &ble_profile_ids,
            &local_terminal_profile_ids,
            &telnet_profile_ids,
            &mosh_profile_ids,
            &standalone_sftp_profile_ids,
            &remote_desktop_ids,
            &ftp_profile_ids,
            &plugin_ids,
            group,
        ) {
            Ok(count) => {
                let count = count + tunnel_ids.len();
                self.manager.update(cx, |session_manager, cx| {
                    match interaction {
                        SessionManagerMoveInteraction::BatchMenu => {
                            session_manager.status = Some(connections_moved_label(
                                &self.i18n,
                                count,
                                group_label(&self.i18n, group),
                            ));
                            session_manager.selected_items.clear();
                            session_manager.selection_anchor = None;
                            session_manager.show_batch_move = false;
                        }
                        SessionManagerMoveInteraction::DragDrop => {
                            // Keep moved rows selected and reveal their destination as direct feedback.
                            if let Some(group) = group {
                                expand_group_path(group, &mut session_manager.expanded_groups);
                            }
                        }
                    }
                    cx.notify();
                });
                if count > 0 {
                    self.wrote = true;
                }
            }
            Err(error) => {
                let status = error.to_string();
                let status = match self.apply_tunnel_groups(&previous_tunnel_groups) {
                    Ok(_) => status,
                    Err(rollback) => format!("{status}: {rollback}"),
                };
                self.manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx)
                });
            }
        }
    }

    fn session_target_group(
        &self,
        target: &SessionManagerSelectionTarget,
    ) -> Option<Option<String>> {
        // Returning an outer Option distinguishes missing assets from the ungrouped root.
        match target {
            SessionManagerSelectionTarget::Tunnel(id) => {
                return self
                    .tunnels
                    .iter()
                    .cloned()
                    .into_iter()
                    .find(|saved| &saved.id == id)
                    .and_then(|saved| saved.tunnel)
                    .map(|profile| profile.group);
            }
            SessionManagerSelectionTarget::Plugin(id) => self
                .store
                .plugin_profiles()
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.group.as_deref()),
            SessionManagerSelectionTarget::Ftp(id) => self
                .store
                .get_ftp_profile(id)
                .map(|profile| profile.group.as_deref()),
            SessionManagerSelectionTarget::Connection(id) => self
                .store
                .get(id)
                .map(|connection| connection.group.as_deref()),
            SessionManagerSelectionTarget::Serial(id) => self
                .store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == id.as_str())
                .map(|profile| profile.group.as_deref()),
            SessionManagerSelectionTarget::Ble(id) => self
                .store
                .ble_profiles()
                .iter()
                .find(|profile| profile.id == id.as_str())
                .map(|profile| profile.group.as_deref()),
            SessionManagerSelectionTarget::LocalTerminal(id) => self
                .store
                .local_terminal_profiles()
                .iter()
                .find(|profile| profile.id == id.as_str())
                .map(|profile| profile.group.as_deref()),
            SessionManagerSelectionTarget::Telnet(id) => self
                .store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == id.as_str())
                .map(|profile| profile.group.as_deref()),
            SessionManagerSelectionTarget::Mosh(id) => self
                .store
                .get_mosh_profile(id)
                .map(|profile| profile.group.as_deref()),
            SessionManagerSelectionTarget::StandaloneSftp(id) => self
                .store
                .get_standalone_sftp_profile(id)
                .map(|profile| profile.group.as_deref()),
            SessionManagerSelectionTarget::RemoteDesktop(id) => self
                .store
                .get_remote_desktop_profile(id)
                .map(|profile| profile.group.as_deref()),
        }
        .map(|group| group.map(str::to_string))
    }

    pub fn cancel_session_group_editor(&self, cx: &mut App) {
        self.manager
            .update(cx, SessionManagerState::cancel_group_editor);
    }

    fn move_ssh_tunnels_to_group(
        &self,
        ids: &[String],
        group: Option<&str>,
    ) -> Result<Vec<(String, Option<String>)>, String> {
        let groups = self
            .tunnels
            .clone()
            .into_iter()
            .filter(|saved| ids.contains(&saved.id))
            .map(|saved| (saved.id, group.map(str::to_string)))
            .collect::<Vec<_>>();
        self.apply_tunnel_groups(&groups)
    }

    fn change_tunnel_group_path(
        &self,
        old: &str,
        new: Option<&str>,
    ) -> Result<Vec<(String, Option<String>)>, String> {
        let groups = self
            .tunnels
            .clone()
            .into_iter()
            .filter_map(|saved| {
                let group = saved.tunnel?.group?;
                let suffix = group.strip_prefix(old)?;
                if !suffix.is_empty() && !suffix.starts_with('/') {
                    return None;
                }
                Some((saved.id, new.map(|new| format!("{new}{suffix}"))))
            })
            .collect::<Vec<_>>();
        self.apply_tunnel_groups(&groups)
    }

    fn apply_tunnel_groups(
        &self,
        groups: &[(String, Option<String>)],
    ) -> Result<Vec<(String, Option<String>)>, String> {
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        let store = self
            .forward_store
            .as_ref()
            .ok_or_else(|| self.i18n.t("ssh_tunnel.save_failed"))?;
        let before = self
            .tunnels
            .clone()
            .into_iter()
            .filter(|saved| groups.iter().any(|(id, _)| id == &saved.id))
            .map(|saved| (saved.id, saved.tunnel.and_then(|profile| profile.group)))
            .collect();
        store
            .update_tunnel_groups(groups)
            .map_err(|_| self.i18n.t("ssh_tunnel.save_failed"))?;
        Ok(before)
    }
}
