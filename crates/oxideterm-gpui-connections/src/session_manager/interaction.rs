use super::{SessionManagerSelectionTarget, SessionManagerState};
use gpui::{Context, KeyDownEvent};

use super::{SessionManagerInput, session_manager_input_is_active};

pub enum CatalogInputAction {
    Pass,
    Handled,
    ClearComposition,
    CloseSidebarSearch,
    PruneSelection,
    SubmitGroup,
    CommitTemplateName,
    CancelTemplateName,
}

/// How a row click combines with the current selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowSelectionMode {
    Replace,
    Toggle,
    Range,
}

/// Returns the contiguous visible rows between `anchor` and `target` (both
/// endpoints included) in render order, or `None` when either endpoint is not
/// part of the visible list (e.g. hidden by a collapsed group or a search).
pub fn session_manager_selection_range(
    visible: &[SessionManagerSelectionTarget],
    anchor: &SessionManagerSelectionTarget,
    target: &SessionManagerSelectionTarget,
) -> Option<Vec<SessionManagerSelectionTarget>> {
    let anchor_index = visible.iter().position(|candidate| candidate == anchor)?;
    let target_index = visible.iter().position(|candidate| candidate == target)?;
    let (start, end) = if anchor_index <= target_index {
        (anchor_index, target_index)
    } else {
        (target_index, anchor_index)
    };
    Some(visible[start..=end].to_vec())
}

pub fn close_session_menu_state(session_manager: &mut SessionManagerState) -> bool {
    // SessionManager floating menus share one ContextMenu dismissal owner for
    // outside click, Esc, and guarded item activation.
    let changed = session_manager.view_mode_menu_open
        || session_manager.sort_menu_open
        || session_manager.show_batch_move
        || session_manager.row_action_menu.is_some();
    session_manager.view_mode_menu_open = false;
    session_manager.sort_menu_open = false;
    session_manager.show_batch_move = false;
    session_manager.row_action_menu = None;
    changed
}

impl SessionManagerState {
    pub fn toggle_session_group_expanded(&mut self, group: &str, cx: &mut Context<Self>) {
        if self.expanded_groups.contains(group) {
            self.expanded_groups.remove(group);
        } else {
            self.expanded_groups.insert(group.to_string());
        }
        cx.notify();
    }

    pub fn toggle_session_selection(
        &mut self,
        target: SessionManagerSelectionTarget,
        cx: &mut Context<Self>,
    ) {
        if self.selected_items.contains(&target) {
            self.selected_items.remove(&target);
        } else {
            self.selected_items.insert(target);
        }
        cx.notify();
    }

    pub fn apply_session_manager_selection(
        &mut self,
        target: SessionManagerSelectionTarget,
        mode: RowSelectionMode,
        cx: &mut Context<Self>,
    ) {
        match mode {
            RowSelectionMode::Replace => {
                self.selected_items.clear();
                self.selected_items.insert(target.clone());
                self.selection_anchor = Some(target);
            }
            RowSelectionMode::Toggle => {
                if self.selected_items.contains(&target) {
                    self.selected_items.remove(&target);
                } else {
                    self.selected_items.insert(target.clone());
                }
                self.selection_anchor = Some(target);
            }
            RowSelectionMode::Range => {
                // Keep the anchor fixed so repeated Shift+clicks extend the
                // same range; without a visible anchor fall back to Replace.
                let range = self.selection_anchor.as_ref().and_then(|anchor| {
                    session_manager_selection_range(
                        &self.visible_selection_targets,
                        anchor,
                        &target,
                    )
                });
                match range {
                    Some(range) => {
                        self.selected_items = range.into_iter().collect();
                    }
                    None => {
                        self.selected_items.clear();
                        self.selected_items.insert(target.clone());
                        self.selection_anchor = Some(target);
                    }
                }
            }
        }
        cx.notify();
    }

    pub fn set_session_manager_visible_selection_targets(
        &mut self,
        targets: Vec<SessionManagerSelectionTarget>,
    ) {
        // Rewritten on every render from the same data, so no notify is needed.
        if self.visible_selection_targets != targets {
            self.visible_selection_targets = targets;
        }
    }

    pub fn clear_session_manager_selection(&mut self, cx: &mut Context<Self>) {
        if self.selected_items.is_empty() && self.selection_anchor.is_none() {
            return;
        }
        self.selected_items.clear();
        self.selection_anchor = None;
        cx.notify();
    }
}

impl SessionManagerState {
    pub fn open_sidebar_search(&mut self, cx: &mut Context<Self>) {
        self.sidebar_search_open = true;
        self.focused_input = Some(SessionManagerInput::Search);
        cx.notify();
    }

    pub fn close_sidebar_search(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.sidebar_search_open {
            return false;
        }
        self.sidebar_search_open = false;
        self.search_query.clear();
        if self.focused_input == Some(SessionManagerInput::Search) {
            self.focused_input = None;
        }
        cx.notify();
        true
    }

    pub fn set_sidebar_sort_open(&mut self, open: bool, cx: &mut Context<Self>) -> bool {
        if self.sidebar_sort_open == open {
            return false;
        }
        self.sidebar_sort_open = open;
        cx.notify();
        true
    }

    pub fn active_input(
        &self,
        tab_active: bool,
        sidebar_visible: bool,
    ) -> Option<SessionManagerInput> {
        let input = self.focused_input?;
        let sidebar_input = sidebar_visible
            && match input {
                SessionManagerInput::Search => self.sidebar_search_open && !self.sidebar_sort_open,
                SessionManagerInput::GroupName => self.show_group_manager,
                _ => false,
            };
        (sidebar_input
            || session_manager_input_is_active(
                input,
                tab_active,
                self.oxide_import_dialog.as_ref(),
                self.oxide_export_dialog.as_ref(),
            ))
        .then_some(input)
    }

    pub fn handle_input_key(
        &mut self,
        event: &KeyDownEvent,
        tab_active: bool,
        cx: &mut Context<Self>,
    ) -> CatalogInputAction {
        use CatalogInputAction::*;
        let Some(input) = self.focused_input else {
            return Pass;
        };
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return Pass;
        }
        match (event.keystroke.key.as_str(), input) {
            ("escape", SessionManagerInput::BatchTemplateName) => CancelTemplateName,
            ("escape", SessionManagerInput::Search) if self.sidebar_search_open && !tab_active => {
                CloseSidebarSearch
            }
            ("escape", _) => {
                self.focused_input = None;
                cx.notify();
                ClearComposition
            }
            ("enter", SessionManagerInput::GroupName) => SubmitGroup,
            ("enter", SessionManagerInput::BatchTemplateName) => CommitTemplateName,
            ("backspace", _) => {
                let changed = match input {
                    SessionManagerInput::Search => self.search_query.pop().is_some(),
                    SessionManagerInput::GroupName => self.group_name_draft.pop().is_some(),
                    SessionManagerInput::OxideImportPassword => {
                        self.oxide_import_dialog.as_mut().is_some_and(|dialog| {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    SessionManagerInput::OxideExportPassword => {
                        self.oxide_export_dialog.as_mut().is_some_and(|dialog| {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    SessionManagerInput::OxideExportConfirmPassword => {
                        self.oxide_export_dialog.as_mut().is_some_and(|dialog| {
                            dialog.confirm_password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    SessionManagerInput::OxideExportDescription => {
                        self.oxide_export_dialog.as_mut().is_some_and(|dialog| {
                            dialog.description.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    SessionManagerInput::BatchPassword => {
                        self.batch_add.as_mut().is_some_and(|dialog| {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    SessionManagerInput::BatchKeyPath => {
                        self.batch_add.as_mut().is_some_and(|dialog| {
                            dialog.key_path.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    SessionManagerInput::BatchTemplateName => self
                        .batch_add
                        .as_mut()
                        .is_some_and(|dialog| dialog.rename_draft.pop().is_some()),
                };
                if changed {
                    cx.notify();
                    if input == SessionManagerInput::Search {
                        return PruneSelection;
                    }
                }
                Handled
            }
            _ => Pass,
        }
    }
}

impl SessionManagerState {
    pub fn retain_visible_selection(
        &mut self,
        items: Vec<super::SessionManagerDisplayItem>,
        cx: &mut Context<Self>,
    ) {
        let visible_items = items
            .into_iter()
            .filter_map(|item| item.selection_target())
            .collect::<std::collections::HashSet<_>>();
        let previous_count = self.selected_items.len();
        self.selected_items
            .retain(|target| visible_items.contains(target));
        // A hidden anchor row would make the next Shift+click degrade or
        // span across rows the user cannot see; keep anchors that are
        // still visible.
        if self
            .selection_anchor
            .as_ref()
            .is_some_and(|anchor| !visible_items.contains(anchor))
        {
            self.selection_anchor = None;
        }
        if self.selected_items.len() != previous_count {
            cx.notify();
        }
    }
}
