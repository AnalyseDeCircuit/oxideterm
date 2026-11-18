use super::{SessionManagerSelectionTarget, SessionManagerState};
use gpui::Context;

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
