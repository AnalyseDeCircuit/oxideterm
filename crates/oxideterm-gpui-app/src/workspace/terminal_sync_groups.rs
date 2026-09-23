use std::collections::{HashMap, HashSet};

use super::PaneId;

// None identifies the temporary group; saved UUIDs retain their existing on-disk meaning.
pub(super) type SyncGroupId = Option<uuid::Uuid>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SyncMember {
    pub group: SyncGroupId,
    pub isolated: bool,
}

/// Runtime membership owns no transport, terminal entity, input buffer, or background task.
#[derive(Default)]
pub(super) struct TerminalSyncGroups {
    enabled: HashMap<SyncGroupId, bool>,
    members: HashMap<PaneId, SyncMember>,
}

impl TerminalSyncGroups {
    pub fn initialize(&mut self, group: SyncGroupId, candidates: &[PaneId]) {
        if self.enabled.contains_key(&group) {
            return;
        }
        self.enabled.insert(group, false);
        for pane in candidates {
            self.add(group, *pane);
        }
    }

    pub fn member(&self, pane: PaneId) -> Option<SyncMember> {
        self.members.get(&pane).copied()
    }

    pub fn enabled(&self, group: SyncGroupId) -> bool {
        self.enabled.get(&group).copied().unwrap_or(false)
    }

    pub fn add(&mut self, group: SyncGroupId, pane: PaneId) -> bool {
        // Joining another group requires explicitly leaving the previous one.
        if self.members.contains_key(&pane) {
            return false;
        }
        self.enabled.entry(group).or_insert(false);
        self.members.insert(
            pane,
            SyncMember {
                group,
                isolated: false,
            },
        );
        true
    }

    pub fn remove(&mut self, pane: PaneId) {
        if let Some(member) = self.members.remove(&pane) {
            if !self
                .members
                .values()
                .any(|other| other.group == member.group)
            {
                self.enabled.insert(member.group, false);
            }
        }
    }

    pub fn retain_groups(&mut self, saved: &HashSet<uuid::Uuid>) {
        self.members
            .retain(|_, member| member.group.is_none_or(|id| saved.contains(&id)));
        self.enabled
            .retain(|group, _| group.is_none_or(|id| saved.contains(&id)));
    }

    pub fn retain_panes(&mut self, live: &HashSet<PaneId>) {
        let closed: Vec<_> = self
            .members
            .keys()
            .filter(|pane| !live.contains(pane))
            .copied()
            .collect();
        for pane in closed {
            self.remove(pane);
        }
    }

    pub fn toggle_enabled(&mut self, group: SyncGroupId) {
        if self.members.values().any(|member| member.group == group) {
            let enabled = self.enabled.entry(group).or_insert(false);
            *enabled = !*enabled;
        }
    }

    pub fn toggle_isolated(&mut self, pane: PaneId) {
        if let Some(member) = self.members.get_mut(&pane) {
            member.isolated = !member.isolated;
        }
    }

    pub fn remount(&mut self, old: PaneId, new: PaneId) {
        if let Some(mut member) = self.members.remove(&old) {
            // A reconnected shell may no longer share the group's editor/cursor state.
            member.isolated = true;
            self.members.insert(new, member);
        }
    }

    pub fn panes(&self, group: SyncGroupId) -> Vec<PaneId> {
        let mut panes: Vec<_> = self
            .members
            .iter()
            .filter(|(_, member)| member.group == group)
            .map(|(pane, _)| *pane)
            .collect();
        panes.sort_by_key(|pane| pane.0);
        panes
    }

    pub fn targets(&self, source: PaneId) -> Vec<PaneId> {
        let Some(source_member) = self.member(source) else {
            return Vec::new();
        };
        if source_member.isolated || !self.enabled(source_member.group) {
            return Vec::new();
        }
        self.panes(source_member.group)
            .into_iter()
            .filter(|pane| {
                *pane != source && self.member(*pane).is_some_and(|member| !member.isolated)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_stays_inside_each_enabled_group_and_isolation_is_bidirectional() {
        let mut groups = TerminalSyncGroups::default();
        let mq = Some(uuid::Uuid::from_u128(1));
        let redis = Some(uuid::Uuid::from_u128(2));
        let [a, b, c1, c2, d, e, a2] = [1, 2, 3, 4, 5, 6, 7].map(PaneId);
        groups.initialize(mq, &[a, b, c1]);
        groups.initialize(redis, &[c2, d, e]);
        assert_eq!(groups.targets(a), vec![]);
        groups.toggle_enabled(mq);
        groups.toggle_enabled(redis);
        assert_eq!(groups.targets(a), vec![b, c1]);
        assert_eq!(groups.targets(d), vec![c2, e]);
        assert_eq!(groups.targets(a2), vec![]);
        assert!(!groups.add(redis, c1));
        groups.toggle_isolated(b);
        assert_eq!(groups.targets(a), vec![c1]);
        assert_eq!(groups.targets(b), vec![]);
        groups.toggle_isolated(b);
        assert_eq!(groups.targets(b), vec![a, c1]);
        groups.toggle_enabled(mq);
        assert_eq!(groups.targets(a), vec![]);
        assert_eq!(groups.targets(d), vec![c2, e]);
    }

    #[test]
    fn revisiting_templates_does_not_add_new_panes_and_reconnect_requires_resume() {
        let mut groups = TerminalSyncGroups::default();
        let group = Some(uuid::Uuid::from_u128(1));
        let [a, b, new_b, a2] = [1, 2, 3, 4].map(PaneId);
        groups.initialize(group, &[a, b]);
        groups.toggle_enabled(group);
        groups.initialize(group, &[a, b, a2]);
        assert_eq!(groups.targets(a), vec![b]);
        groups.remount(b, new_b);
        assert_eq!(groups.member(b), None);
        assert_eq!(
            groups.member(new_b),
            Some(SyncMember {
                group,
                isolated: true
            })
        );
        assert_eq!(groups.targets(a), vec![]);
        assert_eq!(groups.targets(new_b), vec![]);
        groups.toggle_isolated(new_b);
        assert_eq!(groups.targets(a), vec![new_b]);
        groups.retain_panes(&HashSet::from([a, a2]));
        assert_eq!(groups.targets(a), vec![]);
        groups.remove(a);
        assert!(!groups.enabled(group));
        assert_eq!(groups.targets(a2), vec![]);
        groups.add(group, a);
        groups.add(group, new_b);
        groups.toggle_enabled(group);
        groups.initialize(None, &[a2, PaneId(5)]);
        groups.toggle_enabled(None);
        groups.retain_groups(&HashSet::new());
        assert_eq!(groups.targets(a), vec![]);
        assert_eq!(groups.member(new_b), None);
        assert_eq!(groups.targets(a2), vec![PaneId(5)]);
    }
}
