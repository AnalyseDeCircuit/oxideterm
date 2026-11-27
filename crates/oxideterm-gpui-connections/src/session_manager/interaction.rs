use super::{SessionManagerSelectionTarget, SessionManagerState};
use gpui::{Context, KeyDownEvent};

use super::SessionManagerInput;
#[cfg(not(target_family = "wasm"))]
use super::session_manager_input_is_active;

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
    pub fn search_active(&self) -> bool {
        !self.search_query.trim().is_empty()
    }

    pub fn toggle_session_group_expanded(&mut self, group: &str, cx: &mut Context<Self>) {
        // Matching parent groups are forced open during search; ignore toggles so
        // clicking a group header cannot silently alter the post-search fold state.
        if self.search_active() {
            return;
        }
        if self.expanded_groups.contains(group) {
            self.expanded_groups.remove(group);
        } else {
            self.expanded_groups.insert(group.to_string());
        }
        cx.notify();
    }

    pub fn expand_all_session_groups(
        &mut self,
        groups: std::collections::HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        if self.search_active() {
            return;
        }
        self.expanded_groups = groups;
        self.expanded_discovery
            .extend(self.discovery_groups.iter().map(|g| vec![g.id.clone()]));
        #[cfg(not(target_family = "wasm"))]
        for entry in &self.discovery_entries {
            for path in &entry.group_paths {
                let mut key = vec![entry.source_id.clone()];
                for segment in path {
                    key.push(segment.id.clone());
                    self.expanded_discovery.insert(key.clone());
                }
            }
        }
        cx.notify();
    }

    pub fn collapse_all_session_groups(&mut self, cx: &mut Context<Self>) {
        if self.search_active() {
            return;
        }
        self.expanded_groups.clear();
        self.expanded_discovery.clear();
        cx.notify();
    }

    pub fn focus_discovery_target(&mut self, target: (String, String), cx: &mut Context<Self>) {
        if self.focused_discovery.as_ref() != Some(&target) {
            self.selected_items.clear();
            self.selection_anchor = None;
            self.keyboard_cursor = None;
            self.focused_discovery = Some(target);
            cx.notify();
        }
    }

    pub fn toggle_session_selection(
        &mut self,
        target: SessionManagerSelectionTarget,
        cx: &mut Context<Self>,
    ) {
        self.focused_discovery = None;
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
        self.keyboard_cursor = None;
        self.focused_discovery = None;
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

    pub(super) fn step_selection(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) {
        // A grid can show the same session in Recent and in a group. Navigate each
        // identity once so crossing the second copy cannot send focus back to Recent.
        let mut seen = std::collections::HashSet::new();
        let order: Vec<_> = self
            .visible_selection_targets
            .iter()
            .filter(|target| seen.insert(*target))
            .collect();
        if order.is_empty() {
            return;
        }
        let current = self
            .keyboard_cursor
            .as_ref()
            .or(self.selection_anchor.as_ref());
        let index = current.and_then(|target| order.iter().position(|&v| v == target));
        let next = match (forward, index) {
            (true, Some(i)) => (i + 1).min(order.len() - 1),
            (true, None) => 0,
            (false, Some(i)) => i.saturating_sub(1),
            (false, None) => order.len() - 1,
        };
        let target = (*order[next]).clone();
        if extend
            && let Some(anchor) = self
                .selection_anchor
                .as_ref()
                .and_then(|anchor| order.iter().position(|&v| v == anchor))
        {
            self.selected_items = order[anchor.min(next)..=anchor.max(next)]
                .iter()
                .copied()
                .cloned()
                .collect();
        } else {
            self.apply_session_manager_selection(target.clone(), RowSelectionMode::Replace, cx);
        }
        self.keyboard_cursor = Some(target);
        self.reveal_keyboard_cursor = true;
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
        if self.focused_discovery.take().is_none()
            && self.selected_items.is_empty()
            && self.selection_anchor.is_none()
        {
            return;
        }
        self.selected_items.clear();
        self.selection_anchor = None;
        self.keyboard_cursor = None;
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

    #[cfg(not(target_family = "wasm"))]
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
                    #[cfg(not(target_family = "wasm"))]
                    SessionManagerInput::OxideImportPassword => {
                        self.oxide_import_dialog.as_mut().is_some_and(|dialog| {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    #[cfg(not(target_family = "wasm"))]
                    SessionManagerInput::OxideExportPassword => {
                        self.oxide_export_dialog.as_mut().is_some_and(|dialog| {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    #[cfg(not(target_family = "wasm"))]
                    SessionManagerInput::OxideExportConfirmPassword => {
                        self.oxide_export_dialog.as_mut().is_some_and(|dialog| {
                            dialog.confirm_password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    #[cfg(not(target_family = "wasm"))]
                    SessionManagerInput::OxideExportDescription => {
                        self.oxide_export_dialog.as_mut().is_some_and(|dialog| {
                            dialog.description.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    #[cfg(not(target_family = "wasm"))]
                    SessionManagerInput::BatchPassword => {
                        self.batch_add.as_mut().is_some_and(|dialog| {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    #[cfg(not(target_family = "wasm"))]
                    SessionManagerInput::BatchKeyPath => {
                        self.batch_add.as_mut().is_some_and(|dialog| {
                            dialog.key_path.pop().is_some() || dialog.error.take().is_some()
                        })
                    }
                    #[cfg(not(target_family = "wasm"))]
                    SessionManagerInput::BatchTemplateName => self
                        .batch_add
                        .as_mut()
                        .is_some_and(|dialog| dialog.rename_draft.pop().is_some()),
                    #[cfg(target_family = "wasm")]
                    _ => false,
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
    #[cfg(not(target_family = "wasm"))]
    pub fn retain_visible_selection(
        &mut self,
        items: Vec<super::SessionManagerDisplayItem>,
        cx: &mut Context<Self>,
    ) {
        let visible_items = items
            .into_iter()
            .filter_map(|item| item.selection_target())
            .collect::<std::collections::HashSet<_>>();
        self.retain_selection_targets(visible_items, cx);
    }

    pub(super) fn retain_selection_targets(
        &mut self,
        visible_items: std::collections::HashSet<SessionManagerSelectionTarget>,
        cx: &mut Context<Self>,
    ) {
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
