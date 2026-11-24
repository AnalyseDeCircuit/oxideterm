use super::*;
use SessionManagerDeleteConfirm as Confirm;
use gpui::Context;

impl SessionManagerState {
    pub(super) fn group_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> (bool, Option<super::catalog_view::CatalogRequest>) {
        use super::catalog_view::CatalogRequest as Request;
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

    pub(super) fn open_group_editor(
        &mut self,
        editor: SessionManagerGroupEditor,
        name: String,
        cx: &mut Context<Self>,
    ) {
        close_session_menu_state(self);
        self.show_group_manager = true;
        self.group_name_draft = name;
        self.group_editor = Some(editor);
        self.group_editor_error = None;
        self.group_manager_error = None;
        self.focused_input = Some(SessionManagerInput::GroupName);
        self.focused_basic_dialog_footer_action = None;
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
    }

    pub(super) fn rename_group_editor(&mut self, group: String, cx: &mut Context<Self>) {
        let (parent, leaf) = split_session_group_path(&group);
        let parent_path = parent.map(ToOwned::to_owned);
        let name = leaf.to_owned();
        self.open_group_editor(
            SessionManagerGroupEditor::Rename {
                old_path: group,
                parent_path,
            },
            name,
            cx,
        );
    }

    pub(super) fn request_delete_session_group(&mut self, name: String, cx: &mut Context<Self>) {
        close_session_menu_state(self);
        self.reopen_group_manager_after_delete = self.show_group_manager;
        self.show_group_manager = false;
        self.group_editor = None;
        self.group_editor_error = None;
        self.focused_input = None;
        self.focused_basic_dialog_footer_action = None;
        self.delete_confirm = Some(SessionManagerDeleteConfirm::Group { name });
        cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        cx.notify();
    }

    pub fn close_session_group_manager(&mut self, cx: &mut Context<Self>) {
        if self.show_group_manager {
            self.show_group_manager = false;
            self.group_manager_error = None;
            self.cancel_group_editor(cx);
            cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
        }
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

impl SessionManagerState {
    pub(super) fn request_display_delete(
        &mut self,
        row: SessionManagerRowActionTarget,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        use SessionManagerRowActionTarget as Row;
        let confirm = match row {
            Row::Group(group) => return self.request_delete_session_group(group, cx),
            Row::Connection(id) => name.map(|name| Confirm::Single { id, name }),
            Row::Serial(id) => name.map(|name| Confirm::SerialProfile { id, name }),
            Row::Ble(id) => name.map(|name| Confirm::BleProfile { id, name }),
            Row::LocalTerminal(id) => name.map(|name| Confirm::LocalTerminalProfile { id, name }),
            Row::Telnet(id) => name.map(|name| Confirm::TelnetProfile { id, name }),
            Row::Mosh(id) => name.map(|name| Confirm::MoshProfile { id, name }),
            Row::StandaloneSftp(id) => name.map(|name| Confirm::StandaloneSftpProfile { id, name }),
            Row::RemoteDesktop(id) => name.map(|name| Confirm::RemoteDesktopProfile { id, name }),
            Row::Ftp(id) => Some(Confirm::Batch {
                targets: vec![SessionManagerSelectionTarget::Ftp(id)],
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
        self.delete_focus = None;
        let confirmed = self.delete_confirm.take();
        if confirmed.is_some() {
            cx.emit(SessionManagerWorkspaceEvent::ModalBoundaryChanged);
            cx.notify();
        }
        confirmed
    }
}

pub enum CatalogDeletion {
    Single(SessionManagerSelectionTarget),
    Batch(Vec<SessionManagerSelectionTarget>),
    Group(String),
}

impl SessionManagerState {
    pub fn take_catalog_deletion(&mut self, cx: &mut Context<Self>) -> Option<CatalogDeletion> {
        let target = match self.take_delete(cx)? {
            Confirm::Single { id, .. } => SessionManagerSelectionTarget::Connection(id),
            Confirm::SerialProfile { id, .. } => SessionManagerSelectionTarget::Serial(id),
            Confirm::BleProfile { id, .. } => SessionManagerSelectionTarget::Ble(id),
            Confirm::LocalTerminalProfile { id, .. } => {
                SessionManagerSelectionTarget::LocalTerminal(id)
            }
            Confirm::TelnetProfile { id, .. } => SessionManagerSelectionTarget::Telnet(id),
            Confirm::MoshProfile { id, .. } => SessionManagerSelectionTarget::Mosh(id),
            Confirm::StandaloneSftpProfile { id, .. } => {
                SessionManagerSelectionTarget::StandaloneSftp(id)
            }
            Confirm::RemoteDesktopProfile { id, .. } => {
                SessionManagerSelectionTarget::RemoteDesktop(id)
            }
            Confirm::Batch { targets } => return Some(CatalogDeletion::Batch(targets)),
            Confirm::Group { name } => return Some(CatalogDeletion::Group(name)),
        };
        Some(CatalogDeletion::Single(target))
    }
}
