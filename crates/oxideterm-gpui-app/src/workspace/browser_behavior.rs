use std::{collections::HashSet, hash::Hash};

use super::WorkspaceApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserFocusOrigin {
    Keyboard,
    Pointer,
}

impl BrowserFocusOrigin {
    pub(crate) fn is_focus_visible(self) -> bool {
        matches!(self, Self::Keyboard)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserPointerCaptureOwner {
    SidebarResize,
    AiSidebarResize,
    PaneSplitter,
    SettingsSlider,
    TerminalCastSeekbar,
    TextSelection,
    SftpFileDrag,
    TabDrag,
}

#[derive(Clone, Copy, Debug, Default)]
struct BrowserPointerCaptureState {
    sidebar_resizing: bool,
    ai_sidebar_resizing: bool,
    pane_splitter_dragging: bool,
    settings_slider_dragging: bool,
    terminal_cast_seekbar_dragging: bool,
    text_selection_dragging: bool,
    sftp_file_dragging: bool,
    tab_dragging: bool,
}

pub(crate) fn preserve_or_move_context_selection<T>(selected: &mut HashSet<T>, target: T) -> bool
where
    T: Clone + Eq + Hash,
{
    // Browser file/table context menus keep an existing multi-selection when
    // the secondary-click target is already selected, and otherwise move the
    // selection to the target before opening the menu.
    if selected.contains(&target) {
        false
    } else {
        selected.clear();
        selected.insert(target);
        true
    }
}

impl WorkspaceApp {
    pub(super) fn browser_pointer_capture_owner(&self) -> Option<BrowserPointerCaptureOwner> {
        resolve_browser_pointer_capture_owner(BrowserPointerCaptureState {
            sidebar_resizing: self.sidebar_resizing,
            ai_sidebar_resizing: self.ai_sidebar_resizing,
            pane_splitter_dragging: self.split_drag.is_some(),
            settings_slider_dragging: self.settings_slider_drag.is_some(),
            terminal_cast_seekbar_dragging: self.terminal_cast_seek_dragging,
            text_selection_dragging: self.ime_drag_selection.is_some(),
            sftp_file_dragging: self.sftp_view.has_drag_capture(),
            tab_dragging: self.tab_drag.is_some(),
        })
    }
}

fn resolve_browser_pointer_capture_owner(
    state: BrowserPointerCaptureState,
) -> Option<BrowserPointerCaptureOwner> {
    // Browser pointer capture has a single active owner. The order below favors
    // structural resize handles over content drags because resize gestures must
    // keep winning even when the cursor crosses selectable text or list rows.
    if state.sidebar_resizing {
        Some(BrowserPointerCaptureOwner::SidebarResize)
    } else if state.ai_sidebar_resizing {
        Some(BrowserPointerCaptureOwner::AiSidebarResize)
    } else if state.pane_splitter_dragging {
        Some(BrowserPointerCaptureOwner::PaneSplitter)
    } else if state.settings_slider_dragging {
        Some(BrowserPointerCaptureOwner::SettingsSlider)
    } else if state.terminal_cast_seekbar_dragging {
        Some(BrowserPointerCaptureOwner::TerminalCastSeekbar)
    } else if state.text_selection_dragging {
        Some(BrowserPointerCaptureOwner::TextSelection)
    } else if state.sftp_file_dragging {
        Some(BrowserPointerCaptureOwner::SftpFileDrag)
    } else if state.tab_dragging {
        Some(BrowserPointerCaptureOwner::TabDrag)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserFocusOrigin, BrowserPointerCaptureOwner, BrowserPointerCaptureState,
        preserve_or_move_context_selection, resolve_browser_pointer_capture_owner,
    };
    use std::collections::HashSet;

    #[test]
    fn keeps_multi_selection_when_context_target_is_selected() {
        let mut selected = HashSet::from(["one".to_string(), "two".to_string()]);

        let changed = preserve_or_move_context_selection(&mut selected, "two".to_string());

        assert!(!changed);
        assert_eq!(
            selected,
            HashSet::from(["one".to_string(), "two".to_string()])
        );
    }

    #[test]
    fn moves_selection_when_context_target_is_not_selected() {
        let mut selected = HashSet::from(["one".to_string(), "two".to_string()]);

        let changed = preserve_or_move_context_selection(&mut selected, "three".to_string());

        assert!(changed);
        assert_eq!(selected, HashSet::from(["three".to_string()]));
    }

    #[test]
    fn pointer_capture_reports_no_owner_when_idle() {
        assert_eq!(
            resolve_browser_pointer_capture_owner(BrowserPointerCaptureState::default()),
            None
        );
    }

    #[test]
    fn pointer_capture_prioritizes_structural_resize_handles() {
        let state = BrowserPointerCaptureState {
            sidebar_resizing: true,
            text_selection_dragging: true,
            sftp_file_dragging: true,
            ..BrowserPointerCaptureState::default()
        };

        assert_eq!(
            resolve_browser_pointer_capture_owner(state),
            Some(BrowserPointerCaptureOwner::SidebarResize)
        );
    }

    #[test]
    fn pointer_capture_keeps_content_drags_as_event_owners() {
        let state = BrowserPointerCaptureState {
            sftp_file_dragging: true,
            tab_dragging: true,
            ..BrowserPointerCaptureState::default()
        };

        assert_eq!(
            resolve_browser_pointer_capture_owner(state),
            Some(BrowserPointerCaptureOwner::SftpFileDrag)
        );
    }

    #[test]
    fn focus_visible_only_tracks_keyboard_origin() {
        assert!(BrowserFocusOrigin::Keyboard.is_focus_visible());
        assert!(!BrowserFocusOrigin::Pointer.is_focus_visible());
    }
}
