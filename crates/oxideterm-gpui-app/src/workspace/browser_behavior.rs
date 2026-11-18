use std::{collections::HashSet, hash::Hash};

use super::WorkspaceApp;
use gpui::Context;

pub(crate) use oxideterm_gpui_ui::select::{
    BrowserFocusOrigin, BrowserSelectKeyDirection, browser_focus_visible, browser_select_next_index,
};
pub(crate) fn close_browser_trigger_select<T>(
    open_select: &mut Option<T>,
    focus_origin: &mut Option<BrowserFocusOrigin>,
) -> bool {
    let had_open_select = open_select.take().is_some();
    if had_open_select {
        // Trigger-owned selects model ordinary DOM/Radix form controls: closing
        // the popup also releases the transient focus-origin owner, so a stale
        // pointer/keyboard source cannot leak into the next open.
        *focus_origin = None;
    }
    had_open_select
}

pub(crate) use oxideterm_gpui_ui::confirm::{
    ModalFooterKeyAction, modal_footer_key_action, modal_footer_key_moves_forward,
    next_required_modal_footer_focus,
};
#[cfg(test)]
use oxideterm_gpui_ui::confirm::FocusCycle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModalFooterInputKeyAction<T> {
    Cancel,
    FocusInput,
    FocusFooter(T),
    Activate(T),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InlineFooterInputKeyAction<T> {
    ClearFocus,
    FocusInput,
    FocusFooter(T),
    Activate(T),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModalFooterBodyInputKeyAction<T, I> {
    Cancel,
    FocusInput(I),
    FocusFooter(T),
    Activate(T),
}


pub(crate) fn modal_footer_input_key_action<T>(
    key: &str,
    shift: bool,
    actions: &[T],
    input_available: bool,
    input_focused: bool,
    current: Option<T>,
    fallback: T,
    activation_fallback: Option<T>,
) -> Option<ModalFooterInputKeyAction<T>>
where
    T: Copy + Eq,
{
    // Some Tauri dialogs place a real input before the footer buttons. GPUI has
    // no DOM tab order, so keep the "input, cancel, primary" focus loop here
    // instead of reimplementing it in each dialog key handler.
    match key {
        "escape" => Some(ModalFooterInputKeyAction::Cancel),
        "tab" => {
            let forward = modal_footer_key_moves_forward(key, shift);
            if input_available && input_focused {
                return Some(ModalFooterInputKeyAction::FocusFooter(
                    next_required_modal_footer_focus(actions, None, forward, fallback),
                ));
            }

            if input_available {
                let first = actions.first().copied().unwrap_or(fallback);
                let last = actions.last().copied().unwrap_or(fallback);
                if (current == Some(first) && !forward) || (current == Some(last) && forward) {
                    return Some(ModalFooterInputKeyAction::FocusInput);
                }
            }

            Some(ModalFooterInputKeyAction::FocusFooter(
                next_required_modal_footer_focus(actions, current, forward, fallback),
            ))
        }
        "arrowleft" | "left" | "arrowright" | "right" | "home" | "end" => {
            modal_footer_key_action(key, shift, actions, current, fallback).map(|action| {
                match action {
                    ModalFooterKeyAction::Cancel => ModalFooterInputKeyAction::Cancel,
                    ModalFooterKeyAction::Focus(action) => {
                        ModalFooterInputKeyAction::FocusFooter(action)
                    }
                    ModalFooterKeyAction::Activate(action) => {
                        ModalFooterInputKeyAction::Activate(action)
                    }
                }
            })
        }
        "enter" | "space" | " " => current
            .or(activation_fallback)
            .map(ModalFooterInputKeyAction::Activate),
        _ => None,
    }
}

pub(crate) fn inline_footer_input_key_action<T>(
    key: &str,
    shift: bool,
    actions: &[T],
    input_focused: bool,
    current: Option<T>,
    fallback: T,
) -> Option<InlineFooterInputKeyAction<T>>
where
    T: Copy + Eq,
{
    // Inline browser controls such as the AI chat composer are not modal focus
    // traps: Tab moves from the textarea to the footer action, then out of the
    // control group, while Shift+Tab from the action returns to the textarea.
    // Keep that DOM-like order shared instead of hand-writing it in each input.
    let has_actions = !actions.is_empty();
    match key {
        "escape" => Some(InlineFooterInputKeyAction::ClearFocus),
        "tab" if input_focused && !shift && has_actions => {
            Some(InlineFooterInputKeyAction::FocusFooter(
                next_required_modal_footer_focus(actions, None, true, fallback),
            ))
        }
        "tab" if input_focused && !shift => Some(InlineFooterInputKeyAction::ClearFocus),
        "tab" if input_focused => Some(InlineFooterInputKeyAction::ClearFocus),
        "tab" if shift => Some(InlineFooterInputKeyAction::FocusInput),
        "tab" => Some(InlineFooterInputKeyAction::ClearFocus),
        "arrowleft" | "left" | "arrowright" | "right" | "home" | "end"
            if !input_focused && has_actions =>
        {
            modal_footer_key_action(key, shift, actions, current, fallback).map(|action| {
                match action {
                    ModalFooterKeyAction::Cancel => InlineFooterInputKeyAction::ClearFocus,
                    ModalFooterKeyAction::Focus(action) => {
                        InlineFooterInputKeyAction::FocusFooter(action)
                    }
                    ModalFooterKeyAction::Activate(action) => {
                        InlineFooterInputKeyAction::Activate(action)
                    }
                }
            })
        }
        "enter" | "space" | " " if !input_focused && has_actions => Some(
            InlineFooterInputKeyAction::Activate(current.unwrap_or(fallback)),
        ),
        _ => None,
    }
}

pub(crate) fn modal_footer_body_input_key_action<T, I>(
    key: &str,
    shift: bool,
    actions: &[T],
    current_footer: Option<T>,
    inputs: &[I],
    current_input: Option<I>,
    fallback: T,
    activation_fallback: Option<T>,
) -> Option<ModalFooterBodyInputKeyAction<T, I>>
where
    T: Copy + Eq,
    I: Copy + Eq,
{
    // Dialogs with several body inputs need browser focus edges, not a single
    // "input vs footer" bit: Tab from the last body input enters the footer,
    // Shift+Tab from the first footer action returns to the last body input,
    // and inner body-to-body movement is left to the owning input group.
    match key {
        "escape" => Some(ModalFooterBodyInputKeyAction::Cancel),
        "tab" => {
            let forward = modal_footer_key_moves_forward(key, shift);
            if let Some(input) = current_input {
                let Some(index) = inputs.iter().position(|candidate| *candidate == input) else {
                    return None;
                };
                if forward {
                    if let Some(next_input) = inputs.get(index + 1).copied() {
                        return Some(ModalFooterBodyInputKeyAction::FocusInput(next_input));
                    }
                    return Some(ModalFooterBodyInputKeyAction::FocusFooter(
                        next_required_modal_footer_focus(actions, None, forward, fallback),
                    ));
                }

                if let Some(previous) = index.checked_sub(1).and_then(|i| inputs.get(i).copied()) {
                    return Some(ModalFooterBodyInputKeyAction::FocusInput(previous));
                }
                return Some(ModalFooterBodyInputKeyAction::FocusFooter(
                    next_required_modal_footer_focus(actions, None, forward, fallback),
                ));
            }

            if let (Some(first), Some(last)) = (inputs.first().copied(), inputs.last().copied()) {
                let first_action = actions.first().copied().unwrap_or(fallback);
                let last_action = actions.last().copied().unwrap_or(fallback);
                if current_footer == Some(first_action) && !forward {
                    return Some(ModalFooterBodyInputKeyAction::FocusInput(last));
                }
                if current_footer == Some(last_action) && forward {
                    return Some(ModalFooterBodyInputKeyAction::FocusInput(first));
                }
            }

            Some(ModalFooterBodyInputKeyAction::FocusFooter(
                next_required_modal_footer_focus(actions, current_footer, forward, fallback),
            ))
        }
        "arrowleft" | "left" | "arrowright" | "right" | "home" | "end"
            if current_input.is_none() =>
        {
            modal_footer_key_action(key, shift, actions, current_footer, fallback).map(|action| {
                match action {
                    ModalFooterKeyAction::Cancel => ModalFooterBodyInputKeyAction::Cancel,
                    ModalFooterKeyAction::Focus(action) => {
                        ModalFooterBodyInputKeyAction::FocusFooter(action)
                    }
                    ModalFooterKeyAction::Activate(action) => {
                        ModalFooterBodyInputKeyAction::Activate(action)
                    }
                }
            })
        }
        "enter" | "space" | " " if current_input.is_none() => current_footer
            .or(activation_fallback)
            .map(ModalFooterBodyInputKeyAction::Activate),
        _ => None,
    }
}


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserPointerCaptureOwner {
    KnowledgeResize,
    SidebarResize,
    EmbeddedSftpSidebarResize,
    AiSidebarResize,
    SftpPaneResize,
    SftpQueueResize,
    TerminalCommandSenderResize,
    TerminalQuickCommandsResize,
    PaneSplitter,
    SettingsSlider,
    TerminalCastSeekbar,
    TextSelection,
    SftpFileDrag,
    TabDrag,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BrowserOverlayPlacement {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct BrowserPointerCaptureState {
    sidebar_resizing: bool,
    embedded_sftp_sidebar_resizing: bool,
    ai_sidebar_resizing: bool,
    sftp_pane_resizing: bool,
    sftp_queue_resizing: bool,
    terminal_command_sender_resizing: bool,
    terminal_quick_commands_resizing: bool,
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

pub(crate) fn clamp_context_menu_position(
    pointer_x: f32,
    pointer_y: f32,
    viewport_width: f32,
    viewport_height: f32,
    menu_width: f32,
    menu_height: f32,
    viewport_margin: f32,
) -> BrowserOverlayPlacement {
    // Browser/Radix context menus collide against the viewport instead of
    // letting the menu spill off-screen. Native popovers use window coordinates,
    // so clamp once here and keep every file/tree/table menu on the same rule.
    BrowserOverlayPlacement {
        x: pointer_x
            .min(viewport_width - menu_width - viewport_margin)
            .max(viewport_margin),
        y: pointer_y
            .min(viewport_height - menu_height - viewport_margin)
            .max(viewport_margin),
    }
}

pub(crate) fn pointer_capture_needs_workspace_overlay(owner: BrowserPointerCaptureOwner) -> bool {
    // Structural resizes and thin scrollbars must keep receiving movement after
    // the pointer crosses terminal, list, or selectable-text event owners.
    matches!(
        owner,
        BrowserPointerCaptureOwner::SidebarResize
            | BrowserPointerCaptureOwner::KnowledgeResize
            | BrowserPointerCaptureOwner::EmbeddedSftpSidebarResize
            | BrowserPointerCaptureOwner::AiSidebarResize
            | BrowserPointerCaptureOwner::SftpPaneResize
            | BrowserPointerCaptureOwner::SftpQueueResize
            | BrowserPointerCaptureOwner::TerminalCommandSenderResize
            | BrowserPointerCaptureOwner::TerminalQuickCommandsResize
            | BrowserPointerCaptureOwner::TabDrag
    )
}

impl WorkspaceApp {
    pub(super) fn browser_pointer_capture_owner(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<BrowserPointerCaptureOwner> {
        if self.knowledge_resize_active(cx) {
            return Some(BrowserPointerCaptureOwner::KnowledgeResize);
        }
        let sftp = self.sftp_view().read(cx);
        resolve_browser_pointer_capture_owner(BrowserPointerCaptureState {
            sidebar_resizing: self.sidebar_resizing,
            embedded_sftp_sidebar_resizing: self.embedded_sftp_sidebar_resizing,
            ai_sidebar_resizing: self.ai_entity.read(cx).chat_ui().sidebar_resizing,
            sftp_pane_resizing: sftp.pane_resize_active(),
            sftp_queue_resizing: sftp.queue_resize_active(),
            terminal_command_sender_resizing: self.terminal_command_sender.read(cx).is_resizing(),
            terminal_quick_commands_resizing: self
                .terminal
                .read(cx)
                .quick_commands
                .panel
                .is_resizing(),
            pane_splitter_dragging: self.split_drag.is_some(),
            settings_slider_dragging: self.settings_workspace.read(cx).slider_drag().is_some(),
            terminal_cast_seekbar_dragging: self.terminal.read(cx).cast_seek_dragging(),
            text_selection_dragging: self.ime_drag_selection.is_some(),
            sftp_file_dragging: sftp.has_drag_capture(),
            tab_dragging: self.main_window_tabs.drag.is_some(),
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
    } else if state.embedded_sftp_sidebar_resizing {
        Some(BrowserPointerCaptureOwner::EmbeddedSftpSidebarResize)
    } else if state.ai_sidebar_resizing {
        Some(BrowserPointerCaptureOwner::AiSidebarResize)
    } else if state.sftp_pane_resizing {
        Some(BrowserPointerCaptureOwner::SftpPaneResize)
    } else if state.sftp_queue_resizing {
        Some(BrowserPointerCaptureOwner::SftpQueueResize)
    } else if state.terminal_quick_commands_resizing {
        Some(BrowserPointerCaptureOwner::TerminalQuickCommandsResize)
    } else if state.terminal_command_sender_resizing {
        Some(BrowserPointerCaptureOwner::TerminalCommandSenderResize)
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
        BrowserFocusOrigin, BrowserPointerCaptureOwner, BrowserPointerCaptureState, FocusCycle,
        browser_focus_visible, clamp_context_menu_position, modal_footer_input_key_action,
        modal_footer_key_action, pointer_capture_needs_workspace_overlay,
        preserve_or_move_context_selection, resolve_browser_pointer_capture_owner,
    };
    use std::collections::HashSet;

    #[test]
    fn context_target_preserves_or_replaces_selection() {
        let mut selected = HashSet::from(["one".to_string(), "two".to_string()]);

        let changed = preserve_or_move_context_selection(&mut selected, "two".to_string());

        assert!(!changed);
        assert_eq!(
            selected,
            HashSet::from(["one".to_string(), "two".to_string()])
        );

        let changed = preserve_or_move_context_selection(&mut selected, "three".to_string());

        assert!(changed);
        assert_eq!(selected, HashSet::from(["three".to_string()]));
    }

    #[test]
    fn context_menu_position_clamps_to_both_viewport_edges() {
        let placement = clamp_context_menu_position(760.0, 580.0, 800.0, 600.0, 220.0, 180.0, 8.0);

        assert_eq!(
            placement,
            super::BrowserOverlayPlacement { x: 572.0, y: 412.0 }
        );
        let placement = clamp_context_menu_position(-20.0, 2.0, 800.0, 600.0, 220.0, 180.0, 8.0);

        assert_eq!(placement, super::BrowserOverlayPlacement { x: 8.0, y: 8.0 });
    }

    #[test]
    fn pointer_capture_owner_follows_priority_and_active_gesture() {
        let cases = [
            (BrowserPointerCaptureState::default(), None),
            (
                BrowserPointerCaptureState {
                    sidebar_resizing: true,
                    text_selection_dragging: true,
                    sftp_file_dragging: true,
                    ..BrowserPointerCaptureState::default()
                },
                Some(BrowserPointerCaptureOwner::SidebarResize),
            ),
            (
                BrowserPointerCaptureState {
                    terminal_quick_commands_resizing: true,
                    ..BrowserPointerCaptureState::default()
                },
                Some(BrowserPointerCaptureOwner::TerminalQuickCommandsResize),
            ),
            (
                BrowserPointerCaptureState {
                    terminal_command_sender_resizing: true,
                    ..BrowserPointerCaptureState::default()
                },
                Some(BrowserPointerCaptureOwner::TerminalCommandSenderResize),
            ),
            (
                BrowserPointerCaptureState {
                    sftp_file_dragging: true,
                    tab_dragging: true,
                    ..BrowserPointerCaptureState::default()
                },
                Some(BrowserPointerCaptureOwner::SftpFileDrag),
            ),
        ];

        for (state, expected) in cases {
            assert_eq!(resolve_browser_pointer_capture_owner(state), expected);
        }
    }

    #[test]
    fn pointer_capture_overlay_covers_structural_and_scrollbar_drags() {
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::SidebarResize
        ));
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::EmbeddedSftpSidebarResize
        ));
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::AiSidebarResize
        ));
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::SftpPaneResize
        ));
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::SftpQueueResize
        ));
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::TerminalCommandSenderResize
        ));
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::TerminalQuickCommandsResize
        ));
        assert!(pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::TabDrag
        ));
        assert!(!pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::TextSelection
        ));
        assert!(!pointer_capture_needs_workspace_overlay(
            BrowserPointerCaptureOwner::SftpFileDrag
        ));
    }

    #[test]
    fn browser_focus_visible_requires_keyboard_owned_focus() {
        assert!(browser_focus_visible(
            true,
            Some(BrowserFocusOrigin::Keyboard)
        ));
        assert!(!browser_focus_visible(
            true,
            Some(BrowserFocusOrigin::Pointer)
        ));
        assert!(!browser_focus_visible(
            false,
            Some(BrowserFocusOrigin::Keyboard)
        ));
        assert!(!browser_focus_visible(true, None));
    }

    #[test]
    fn browser_select_next_index_clamps_like_radix_select() {
        assert_eq!(
            super::browser_select_next_index(0, 3, super::BrowserSelectKeyDirection::Previous),
            0
        );
        assert_eq!(
            super::browser_select_next_index(0, 3, super::BrowserSelectKeyDirection::Next),
            1
        );
        assert_eq!(
            super::browser_select_next_index(2, 3, super::BrowserSelectKeyDirection::Next),
            2
        );
        assert_eq!(
            super::browser_select_next_index(0, 0, super::BrowserSelectKeyDirection::Next),
            0
        );
    }

    #[test]
    fn focus_cycle_uses_browser_order_and_recovers_from_stale_focus() {
        let actions = ["cancel", "confirm", "extra"];
        let cycle = FocusCycle::new(&actions);

        assert_eq!(cycle.next(None, true), Some("cancel"));
        assert_eq!(cycle.next(None, false), Some("extra"));
        assert_eq!(cycle.next(Some("cancel"), true), Some("confirm"));
        assert_eq!(cycle.next(Some("cancel"), false), Some("extra"));
        assert_eq!(cycle.next(Some("extra"), true), Some("cancel"));
        let actions = ["cancel", "confirm"];

        assert_eq!(
            FocusCycle::new(&actions).next(Some("stale"), true),
            Some("cancel")
        );
        assert_eq!(
            FocusCycle::new(&actions).next(Some("stale"), false),
            Some("confirm")
        );
        assert_eq!(FocusCycle::<&str>::new(&[]).next(None, true), None);
    }

    #[test]
    fn modal_footer_key_action_centralizes_cancel_focus_and_activate() {
        use super::ModalFooterKeyAction::{Activate, Cancel, Focus};

        let actions = ["cancel", "confirm", "extra"];
        for (key, shift, current, expected) in [
            ("enter", false, None, Some(Activate("cancel"))),
            ("tab", false, None, Some(Focus("cancel"))),
            ("escape", false, Some("confirm"), Some(Cancel)),
            ("tab", false, Some("cancel"), Some(Focus("confirm"))),
            ("tab", true, Some("cancel"), Some(Focus("extra"))),
            ("arrowright", false, Some("confirm"), Some(Focus("extra"))),
            ("arrowleft", false, Some("confirm"), Some(Focus("cancel"))),
            ("left", false, Some("confirm"), Some(Focus("cancel"))),
            ("enter", false, Some("confirm"), Some(Activate("confirm"))),
            ("home", false, Some("confirm"), Some(Focus("cancel"))),
            ("end", false, Some("cancel"), Some(Focus("extra"))),
            ("a", false, Some("confirm"), None),
        ] {
            assert_eq!(
                modal_footer_key_action(key, shift, &actions, current, "cancel"),
                expected,
                "{key}, shift={shift}, current={current:?}"
            );
        }
        assert_eq!(
            modal_footer_key_action("tab", false, &[], Some("stale"), "cancel"),
            Some(Focus("cancel"))
        );
    }

    #[test]
    fn modal_footer_input_key_action_models_focus_cycle_and_activation() {
        let actions = ["cancel", "confirm"];

        assert_eq!(
            modal_footer_input_key_action("tab", false, &actions, true, true, None, "cancel", None),
            Some(super::ModalFooterInputKeyAction::FocusFooter("cancel"))
        );
        assert_eq!(
            modal_footer_input_key_action(
                "tab",
                false,
                &actions,
                true,
                false,
                Some("confirm"),
                "cancel",
                None
            ),
            Some(super::ModalFooterInputKeyAction::FocusInput)
        );
        assert_eq!(
            modal_footer_input_key_action(
                "tab",
                true,
                &actions,
                true,
                false,
                Some("cancel"),
                "cancel",
                None
            ),
            Some(super::ModalFooterInputKeyAction::FocusInput)
        );

        assert_eq!(
            modal_footer_input_key_action(
                "enter", false, &actions, true, false, None, "cancel", None
            ),
            None
        );
        assert_eq!(
            modal_footer_input_key_action(
                "enter",
                false,
                &actions,
                true,
                false,
                None,
                "cancel",
                Some("confirm")
            ),
            Some(super::ModalFooterInputKeyAction::Activate("confirm"))
        );
    }

    #[test]
    fn inline_footer_input_key_action_matches_browser_tab_exit_order() {
        let actions = ["submit"];

        assert_eq!(
            super::inline_footer_input_key_action("tab", false, &actions, true, None, "submit"),
            Some(super::InlineFooterInputKeyAction::FocusFooter("submit"))
        );
        assert_eq!(
            super::inline_footer_input_key_action(
                "tab",
                false,
                &actions,
                false,
                Some("submit"),
                "submit",
            ),
            Some(super::InlineFooterInputKeyAction::ClearFocus)
        );
        assert_eq!(
            super::inline_footer_input_key_action(
                "tab",
                true,
                &actions,
                false,
                Some("submit"),
                "submit",
            ),
            Some(super::InlineFooterInputKeyAction::FocusInput)
        );
        assert_eq!(
            super::inline_footer_input_key_action("tab", false, &[], true, None, "submit"),
            Some(super::InlineFooterInputKeyAction::ClearFocus)
        );
    }

    #[test]
    fn modal_footer_body_input_key_action_keeps_multi_input_edges_browser_like() {
        let actions = ["cancel", "confirm"];

        assert_eq!(
            super::modal_footer_body_input_key_action(
                "tab",
                false,
                &actions,
                None,
                &["first", "middle", "last"],
                Some("last"),
                "cancel",
                None,
            ),
            Some(super::ModalFooterBodyInputKeyAction::FocusFooter("cancel"))
        );
        assert_eq!(
            super::modal_footer_body_input_key_action(
                "tab",
                true,
                &actions,
                Some("cancel"),
                &["first", "middle", "last"],
                None,
                "cancel",
                None,
            ),
            Some(super::ModalFooterBodyInputKeyAction::FocusInput("last"))
        );
        assert_eq!(
            super::modal_footer_body_input_key_action(
                "tab",
                false,
                &actions,
                Some("confirm"),
                &["first", "middle", "last"],
                None,
                "cancel",
                None,
            ),
            Some(super::ModalFooterBodyInputKeyAction::FocusInput("first"))
        );
        assert_eq!(
            super::modal_footer_body_input_key_action(
                "tab",
                false,
                &actions,
                None,
                &["first", "middle", "last"],
                Some("first"),
                "cancel",
                None,
            ),
            Some(super::ModalFooterBodyInputKeyAction::FocusInput("middle"))
        );
    }
}
