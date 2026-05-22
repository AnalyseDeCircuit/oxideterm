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

pub(crate) fn browser_focus_visible(focused: bool, origin: Option<BrowserFocusOrigin>) -> bool {
    // Browser :focus-visible depends on both ownership and input modality:
    // keyboard focus gets the ring, mouse focus does not.
    focused && origin.is_some_and(BrowserFocusOrigin::is_focus_visible)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserSelectKeyDirection {
    Previous,
    Next,
}

pub(crate) fn browser_select_next_index(
    current: usize,
    option_count: usize,
    direction: BrowserSelectKeyDirection,
) -> usize {
    // Radix Select clamps keyboard highlight movement at the first/last item.
    // Keep the clamp shared so Cloud Sync, new connection, and future native
    // selects do not each define their own arrow-key boundary behavior.
    if option_count == 0 {
        return 0;
    }
    match direction {
        BrowserSelectKeyDirection::Previous => current.saturating_sub(1),
        BrowserSelectKeyDirection::Next => (current + 1).min(option_count - 1),
    }
}

pub(crate) fn close_browser_select_on_container_scroll<T>(
    open_select: &mut Option<T>,
    focused_select: &mut Option<T>,
    highlighted_option: &mut Option<(T, usize)>,
) -> bool
where
    T: Copy,
{
    let Some(select) = open_select.take() else {
        return false;
    };

    // Radix Select closes its content when an owning scroll container moves,
    // but the trigger remains the browser focus anchor. Keep this shared so
    // Cloud Sync, settings inline selects, and future form selects do not clear
    // focus ownership while only trying to dismiss the popup content.
    *focused_select = Some(select);
    *highlighted_option = None;
    true
}

pub(crate) fn open_browser_highlighted_select_from_keyboard<T>(
    open_select: &mut Option<T>,
    focused_select: &mut Option<T>,
    focus_origin: &mut Option<BrowserFocusOrigin>,
    highlighted_option: &mut Option<(T, usize)>,
    select: T,
    selected_index: usize,
) where
    T: Copy,
{
    // Radix Select opens from Enter/Space with keyboard focus-visible ownership
    // and highlights the current value before arrow navigation starts.
    *focused_select = Some(select);
    *focus_origin = Some(BrowserFocusOrigin::Keyboard);
    *open_select = Some(select);
    *highlighted_option = Some((select, selected_index));
}

pub(crate) fn toggle_browser_highlighted_select_from_pointer<T>(
    open_select: &mut Option<T>,
    focused_select: &mut Option<T>,
    focus_origin: &mut Option<BrowserFocusOrigin>,
    highlighted_option: &mut Option<(T, usize)>,
    select: T,
    selected_index: usize,
) -> bool
where
    T: Copy + Eq,
{
    // Pointer-opened SelectTrigger keeps DOM focus on the trigger, but the focus
    // ring stays hidden. Keep the open/highlight state paired with that origin.
    *focused_select = Some(select);
    *focus_origin = Some(BrowserFocusOrigin::Pointer);
    if *open_select == Some(select) {
        *open_select = None;
        *highlighted_option = None;
        return false;
    }

    *open_select = Some(select);
    *highlighted_option = Some((select, selected_index));
    true
}

pub(crate) fn clear_browser_highlighted_select_focus<T>(
    open_select: &mut Option<T>,
    focused_select: &mut Option<T>,
    focus_origin: &mut Option<BrowserFocusOrigin>,
    highlighted_option: &mut Option<(T, usize)>,
) {
    // Moving focus to a sibling control releases the Select trigger owner and
    // closes any content, matching browser/Radix focus transfer.
    *open_select = None;
    *focused_select = None;
    *focus_origin = None;
    *highlighted_option = None;
}

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

pub(crate) fn toggle_browser_trigger_select_from_pointer<T>(
    open_select: &mut Option<T>,
    focus_origin: &mut Option<BrowserFocusOrigin>,
    select: T,
) -> bool
where
    T: Copy + Eq,
{
    if *open_select == Some(select) {
        close_browser_trigger_select(open_select, focus_origin);
        return false;
    }

    // Radix SelectTrigger opened with the mouse owns focus, but it does not
    // match :focus-visible. Keep pointer modality and open-state paired here
    // so every form/settings select uses the same browser rule.
    *focus_origin = Some(BrowserFocusOrigin::Pointer);
    *open_select = Some(select);
    true
}

pub(crate) fn close_browser_trigger_select_on_container_scroll<T>(
    open_select: &mut Option<T>,
    focus_origin: &mut Option<BrowserFocusOrigin>,
) -> bool {
    // Settings/new-connection selects are trigger-owned form controls. When
    // their scroll container moves the anchor, Radix closes the popup and drops
    // focus-visible ownership rather than leaving a stale ring.
    close_browser_trigger_select(open_select, focus_origin)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FocusCycle<'a, T> {
    actions: &'a [T],
}

impl<'a, T> FocusCycle<'a, T>
where
    T: Copy + Eq,
{
    pub(crate) const fn new(actions: &'a [T]) -> Self {
        Self { actions }
    }

    pub(crate) fn next(self, current: Option<T>, forward: bool) -> Option<T> {
        // GPUI does not provide the browser/Radix footer tab loop. Keep the
        // wrapping action order in one tested helper instead of duplicating it
        // in every modal, select, and recorder footer.
        let Some(first) = self.actions.first().copied() else {
            return None;
        };
        let last = self.actions.last().copied().unwrap_or(first);
        let Some(current) = current else {
            return Some(if forward { first } else { last });
        };
        let Some(index) = self
            .actions
            .iter()
            .position(|candidate| *candidate == current)
        else {
            return Some(if forward { first } else { last });
        };

        if forward {
            self.actions.get(index + 1).copied().or(Some(first))
        } else {
            index
                .checked_sub(1)
                .and_then(|previous| self.actions.get(previous).copied())
                .or(Some(last))
        }
    }
}

pub(crate) fn next_modal_footer_focus<T>(
    actions: &[T],
    current: Option<T>,
    forward: bool,
) -> Option<T>
where
    T: Copy + Eq,
{
    // Radix/Dialog footer buttons follow DOM tab order even when buttons are
    // conditionally hidden. Keep modal footers on this explicit entry point so
    // settings, AI, keybinding, and import/export dialogs do not reimplement
    // their own wrapping rules.
    FocusCycle::new(actions).next(current, forward)
}

pub(crate) fn next_required_modal_footer_focus<T>(
    actions: &[T],
    current: Option<T>,
    forward: bool,
    fallback: T,
) -> T
where
    T: Copy + Eq,
{
    next_modal_footer_focus(actions, current, forward).unwrap_or(fallback)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModalFooterKeyAction<T> {
    Cancel,
    Focus(T),
    Activate(T),
}

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

pub(crate) fn modal_footer_key_action<T>(
    key: &str,
    shift: bool,
    actions: &[T],
    current: Option<T>,
    fallback: T,
) -> Option<ModalFooterKeyAction<T>>
where
    T: Copy + Eq,
{
    // Dialog footer key handling has the same browser contract across standard
    // confirms, Cloud Sync confirms, keybinding recorder, and .oxide
    // import/export: Escape closes, Tab/arrows move focus, Home/End jump to
    // the footer edges, and Enter/Space activates the focused action.
    match key {
        "escape" => Some(ModalFooterKeyAction::Cancel),
        "tab" | "arrowleft" | "left" | "arrowright" | "right" => {
            let forward = modal_footer_key_moves_forward(key, shift);
            Some(ModalFooterKeyAction::Focus(
                next_required_modal_footer_focus(actions, current, forward, fallback),
            ))
        }
        "home" => actions
            .first()
            .copied()
            .or(Some(fallback))
            .map(ModalFooterKeyAction::Focus),
        "end" => actions
            .last()
            .copied()
            .or(Some(fallback))
            .map(ModalFooterKeyAction::Focus),
        "enter" | "space" | " " => {
            Some(ModalFooterKeyAction::Activate(current.unwrap_or(fallback)))
        }
        _ => None,
    }
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

pub(crate) fn modal_footer_key_moves_forward(key: &str, shift: bool) -> bool {
    // Browser/Radix dialogs let Shift+Tab and left-arrow walk backward through
    // footer actions. Keep key-direction mapping shared so standard confirms,
    // Cloud Sync confirms, and import/export modals do not drift apart.
    !shift && !matches!(key, "arrowleft" | "left")
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BrowserOverlayPlacement {
    pub x: f32,
    pub y: f32,
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
        BrowserFocusOrigin, BrowserPointerCaptureOwner, BrowserPointerCaptureState, FocusCycle,
        browser_focus_visible, clamp_context_menu_position, clear_browser_highlighted_select_focus,
        close_browser_select_on_container_scroll, modal_footer_input_key_action,
        modal_footer_key_action, modal_footer_key_moves_forward, next_required_modal_footer_focus,
        open_browser_highlighted_select_from_keyboard, preserve_or_move_context_selection,
        resolve_browser_pointer_capture_owner, toggle_browser_highlighted_select_from_pointer,
        toggle_browser_trigger_select_from_pointer,
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
    fn context_menu_position_collides_with_viewport_edges() {
        let placement = clamp_context_menu_position(760.0, 580.0, 800.0, 600.0, 220.0, 180.0, 8.0);

        assert_eq!(
            placement,
            super::BrowserOverlayPlacement { x: 572.0, y: 412.0 }
        );
    }

    #[test]
    fn context_menu_position_keeps_viewport_margin() {
        let placement = clamp_context_menu_position(-20.0, 2.0, 800.0, 600.0, 220.0, 180.0, 8.0);

        assert_eq!(placement, super::BrowserOverlayPlacement { x: 8.0, y: 8.0 });
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
    fn close_browser_select_on_scroll_preserves_trigger_focus() {
        let mut open_select = Some("backend");
        let mut focused_select = Some("auth");
        let mut highlighted_option = Some(("backend", 2));

        assert!(close_browser_select_on_container_scroll(
            &mut open_select,
            &mut focused_select,
            &mut highlighted_option,
        ));
        assert_eq!(open_select, None);
        assert_eq!(focused_select, Some("backend"));
        assert_eq!(highlighted_option, None);

        assert!(!close_browser_select_on_container_scroll(
            &mut open_select,
            &mut focused_select,
            &mut highlighted_option,
        ));
        assert_eq!(focused_select, Some("backend"));
    }

    #[test]
    fn highlighted_select_keyboard_open_tracks_focus_visible_origin() {
        let mut open_select = None;
        let mut focused_select = None;
        let mut focus_origin = None;
        let mut highlighted_option = None;

        open_browser_highlighted_select_from_keyboard(
            &mut open_select,
            &mut focused_select,
            &mut focus_origin,
            &mut highlighted_option,
            "backend",
            2,
        );

        assert_eq!(open_select, Some("backend"));
        assert_eq!(focused_select, Some("backend"));
        assert_eq!(focus_origin, Some(BrowserFocusOrigin::Keyboard));
        assert_eq!(highlighted_option, Some(("backend", 2)));
        assert!(browser_focus_visible(true, focus_origin));
    }

    #[test]
    fn highlighted_select_pointer_toggle_preserves_trigger_focus() {
        let mut open_select = None;
        let mut focused_select = None;
        let mut focus_origin = None;
        let mut highlighted_option = None;

        assert!(toggle_browser_highlighted_select_from_pointer(
            &mut open_select,
            &mut focused_select,
            &mut focus_origin,
            &mut highlighted_option,
            "backend",
            1,
        ));
        assert_eq!(open_select, Some("backend"));
        assert_eq!(focused_select, Some("backend"));
        assert_eq!(focus_origin, Some(BrowserFocusOrigin::Pointer));
        assert_eq!(highlighted_option, Some(("backend", 1)));
        assert!(!browser_focus_visible(true, focus_origin));

        assert!(!toggle_browser_highlighted_select_from_pointer(
            &mut open_select,
            &mut focused_select,
            &mut focus_origin,
            &mut highlighted_option,
            "backend",
            1,
        ));
        assert_eq!(open_select, None);
        assert_eq!(focused_select, Some("backend"));
        assert_eq!(focus_origin, Some(BrowserFocusOrigin::Pointer));
        assert_eq!(highlighted_option, None);
    }

    #[test]
    fn highlighted_select_focus_clear_releases_all_owners() {
        let mut open_select = Some("backend");
        let mut focused_select = Some("backend");
        let mut focus_origin = Some(BrowserFocusOrigin::Keyboard);
        let mut highlighted_option = Some(("backend", 1));

        clear_browser_highlighted_select_focus(
            &mut open_select,
            &mut focused_select,
            &mut focus_origin,
            &mut highlighted_option,
        );

        assert_eq!(open_select, None);
        assert_eq!(focused_select, None);
        assert_eq!(focus_origin, None);
        assert_eq!(highlighted_option, None);
    }

    #[test]
    fn trigger_select_pointer_toggle_tracks_focus_origin() {
        let mut open_select = None;
        let mut focus_origin = None;

        assert!(toggle_browser_trigger_select_from_pointer(
            &mut open_select,
            &mut focus_origin,
            "language",
        ));
        assert_eq!(open_select, Some("language"));
        assert_eq!(focus_origin, Some(BrowserFocusOrigin::Pointer));
        assert!(!browser_focus_visible(true, focus_origin));

        assert!(!toggle_browser_trigger_select_from_pointer(
            &mut open_select,
            &mut focus_origin,
            "language",
        ));
        assert_eq!(open_select, None);
        assert_eq!(focus_origin, None);

        assert!(toggle_browser_trigger_select_from_pointer(
            &mut open_select,
            &mut focus_origin,
            "theme",
        ));
        assert_eq!(open_select, Some("theme"));
        assert_eq!(focus_origin, Some(BrowserFocusOrigin::Pointer));
    }

    #[test]
    fn focus_cycle_uses_browser_footer_order() {
        let actions = ["cancel", "confirm", "extra"];
        let cycle = FocusCycle::new(&actions);

        assert_eq!(cycle.next(None, true), Some("cancel"));
        assert_eq!(cycle.next(None, false), Some("extra"));
        assert_eq!(cycle.next(Some("cancel"), true), Some("confirm"));
        assert_eq!(cycle.next(Some("cancel"), false), Some("extra"));
        assert_eq!(cycle.next(Some("extra"), true), Some("cancel"));
    }

    #[test]
    fn focus_cycle_recovers_from_missing_or_empty_actions() {
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
    fn modal_footer_focus_uses_required_fallback_when_no_action_is_rendered() {
        let actions: [&str; 0] = [];

        assert_eq!(
            next_required_modal_footer_focus(&actions, Some("stale"), true, "cancel"),
            "cancel"
        );
    }

    #[test]
    fn modal_footer_key_direction_matches_browser_tab_and_arrow_rules() {
        assert!(modal_footer_key_moves_forward("tab", false));
        assert!(modal_footer_key_moves_forward("arrowright", false));
        assert!(!modal_footer_key_moves_forward("tab", true));
        assert!(!modal_footer_key_moves_forward("arrowleft", false));
        assert!(!modal_footer_key_moves_forward("left", false));
    }

    #[test]
    fn modal_footer_key_action_centralizes_cancel_focus_and_activate() {
        let actions = ["cancel", "confirm"];

        assert_eq!(
            modal_footer_key_action("escape", false, &actions, Some("confirm"), "cancel"),
            Some(super::ModalFooterKeyAction::Cancel)
        );
        assert_eq!(
            modal_footer_key_action("tab", false, &actions, Some("cancel"), "cancel"),
            Some(super::ModalFooterKeyAction::Focus("confirm"))
        );
        assert_eq!(
            modal_footer_key_action("tab", true, &actions, Some("cancel"), "cancel"),
            Some(super::ModalFooterKeyAction::Focus("confirm"))
        );
        assert_eq!(
            modal_footer_key_action("enter", false, &actions, Some("confirm"), "cancel"),
            Some(super::ModalFooterKeyAction::Activate("confirm"))
        );
        assert_eq!(
            modal_footer_key_action("home", false, &actions, Some("confirm"), "cancel"),
            Some(super::ModalFooterKeyAction::Focus("cancel"))
        );
        assert_eq!(
            modal_footer_key_action("end", false, &actions, Some("cancel"), "cancel"),
            Some(super::ModalFooterKeyAction::Focus("confirm"))
        );
        assert_eq!(
            modal_footer_key_action("a", false, &actions, Some("confirm"), "cancel"),
            None
        );
    }

    #[test]
    fn modal_footer_input_key_action_models_input_then_footer_cycle() {
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
    }

    #[test]
    fn modal_footer_input_key_action_keeps_activation_explicit() {
        let actions = ["cancel", "confirm"];

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
