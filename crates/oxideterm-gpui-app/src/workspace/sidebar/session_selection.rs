use std::collections::HashSet;

use super::*;

/// Multi-select state for the Active Sessions sidebar.
///
/// Only runtime SSH node rows enter this set: saved connections and the local
/// profile groups mirror persisted assets, so a batch action driven by this
/// selection must never be able to address them. The mode and its selection are
/// process-local UI state and are deliberately not persisted.
#[derive(Default)]
pub(in crate::workspace) struct SessionNodeSelection {
    active: bool,
    selected: HashSet<NodeId>,
}

impl SessionNodeSelection {
    pub(in crate::workspace) fn is_active(&self) -> bool {
        self.active
    }

    /// Selection only counts while the mode is on, so leaving the mode can never
    /// leave a half-highlighted row behind.
    pub(in crate::workspace) fn is_selected(&self, node_id: &NodeId) -> bool {
        self.active && self.selected.contains(node_id)
    }

    pub(in crate::workspace) fn count(&self) -> usize {
        self.selected.len()
    }

    pub(in crate::workspace) fn enter(&mut self) {
        self.active = true;
        self.selected.clear();
    }

    /// Returns whether any visible state changed, so callers can skip repaints.
    pub(in crate::workspace) fn exit(&mut self) -> bool {
        if !self.active && self.selected.is_empty() {
            return false;
        }
        self.active = false;
        self.selected.clear();
        true
    }

    /// Marks a whole subtree selected or unselected. The caller resolves the
    /// subtree, so checking a parent can never select its ancestors.
    pub(in crate::workspace) fn set_selected_many(
        &mut self,
        node_ids: impl IntoIterator<Item = NodeId>,
        selected: bool,
    ) -> bool {
        if !self.active {
            return false;
        }
        let mut changed = false;
        for node_id in node_ids {
            if selected {
                changed |= self.selected.insert(node_id);
            } else {
                changed |= self.selected.remove(&node_id);
            }
        }
        changed
    }

    /// Replaces the selection with the nodes currently offered by the panel.
    pub(in crate::workspace) fn select_all(
        &mut self,
        nodes: impl IntoIterator<Item = NodeId>,
    ) -> bool {
        if !self.active {
            return false;
        }
        let next = nodes.into_iter().collect::<HashSet<_>>();
        if next == self.selected {
            return false;
        }
        self.selected = next;
        true
    }

    /// Drops selections that are no longer rendered. The Active Sessions panel
    /// prunes by the visible row set, so the confirm count always equals the
    /// highlighted rows the user can see.
    pub(in crate::workspace) fn retain(&mut self, rendered: &HashSet<NodeId>) -> bool {
        let before = self.selected.len();
        self.selected.retain(|node_id| rendered.contains(node_id));
        before != self.selected.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str) -> NodeId {
        NodeId::new(id)
    }

    #[test]
    fn exiting_the_mode_drops_every_row_state() {
        let mut selection = SessionNodeSelection::default();
        assert!(!selection.is_active());
        assert!(
            !selection.set_selected_many([node("a")], true),
            "selection must be inert while off"
        );
        assert_eq!(selection.count(), 0);

        selection.enter();
        assert!(selection.set_selected_many([node("a")], true));
        assert!(selection.is_selected(&node("a")));

        assert!(selection.exit());
        assert!(!selection.is_active());
        assert!(!selection.is_selected(&node("a")));
        assert_eq!(selection.count(), 0);
        assert!(!selection.exit(), "a second exit must not repaint");
    }

    #[test]
    fn selecting_and_deselecting_rows_updates_only_the_named_nodes() {
        let mut selection = SessionNodeSelection::default();
        selection.enter();

        selection.set_selected_many([node("a")], true);
        selection.set_selected_many([node("b")], true);
        assert_eq!(
            selection.selected.clone(),
            HashSet::from([node("a"), node("b")])
        );
        assert!(
            !selection.set_selected_many([node("a"), node("b")], true),
            "selecting already selected rows must not repaint"
        );

        selection.set_selected_many([node("a")], false);
        assert_eq!(selection.selected.clone(), HashSet::from([node("b")]));
        assert!(selection.set_selected_many([node("b")], false));
        assert_eq!(selection.count(), 0);
        assert!(
            !selection.set_selected_many([node("b")], false),
            "deselecting an unselected row must not repaint"
        );
    }

    #[test]
    fn select_all_follows_the_rendered_rows_and_pruning_keeps_only_visible_ones() {
        let mut selection = SessionNodeSelection::default();
        selection.enter();
        selection.set_selected_many([node("stale")], true);

        assert!(selection.select_all([node("a"), node("b"), node("c")]));
        assert_eq!(
            selection.selected.clone(),
            HashSet::from([node("a"), node("b"), node("c")])
        );
        assert!(
            !selection.select_all([node("c"), node("b"), node("a")]),
            "the same row set must not force a repaint"
        );

        assert!(selection.retain(&HashSet::from([node("a"), node("c")])));
        assert_eq!(
            selection.selected.clone(),
            HashSet::from([node("a"), node("c")])
        );
        assert!(!selection.retain(&HashSet::from([node("a"), node("c")])));
    }

    #[test]
    fn selection_survives_until_the_mode_exits() {
        let mut selection = SessionNodeSelection::default();
        selection.enter();
        selection.select_all([node("a"), node("b")]);

        assert_eq!(selection.count(), 2);
        assert!(selection.is_active());

        selection.exit();
        assert_eq!(selection.count(), 0);
        assert!(!selection.is_selected(&node("a")));
    }
}
