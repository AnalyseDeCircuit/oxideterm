use oxideterm_workspace::{
    MAX_PANES_PER_TAB, PaneId, PaneNode, SplitDirection, Tab, TabId, TabKind, TabTitleSource,
    TerminalSessionId,
};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    hash::Hash,
};

const MAX_TAB_HISTORY: usize = 50;

pub fn tab_is_renamable(kind: &TabKind) -> bool {
    matches!(
        kind,
        TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal | TabKind::Workspace
    )
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum TabScope<A> {
    #[default]
    All,
    Affinity(A),
}

/// Terminal pages retain session IDs when merging removes their original tab descriptors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabContent {
    Page(TabId),
    Terminal(TerminalSessionId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TabMountId(u64);

pub struct RemovedTab<M> {
    pub tab: Tab,
    pub mount: Option<M>,
    pub previous_active_tab_id: Option<TabId>,
    pub next_active_tab_id: Option<TabId>,
}

#[derive(Default)]
struct TabHistory {
    visits: Vec<TabId>,
    cursor: Option<usize>,
    recent: Vec<TabId>,
}

impl TabHistory {
    fn record(&mut self, id: TabId) {
        self.recent.retain(|tab| *tab != id);
        self.recent.insert(0, id);
        self.recent.truncate(MAX_TAB_HISTORY);
        if self.cursor.is_some_and(|i| self.visits[i] == id) {
            return;
        }
        self.visits.truncate(self.cursor.map_or(0, |i| i + 1));
        self.visits.push(id);
        if self.visits.len() > MAX_TAB_HISTORY {
            self.visits.remove(0);
        }
        self.cursor = self.visits.len().checked_sub(1);
    }
    fn navigate(&mut self, forward: bool, visible: &HashSet<TabId>) -> Option<TabId> {
        let current = self.cursor?;
        let next = if forward {
            (current + 1..self.visits.len()).find(|i| visible.contains(&self.visits[*i]))
        } else {
            (0..current)
                .rev()
                .find(|i| visible.contains(&self.visits[*i]))
        }?;
        self.cursor = Some(next);
        Some(self.visits[next])
    }
}

/// One canonical collection across windows and embedded pages. The affinity key
/// and the optional detached mount payload belong to the host.
pub struct TabLayout<A = (), M = ()> {
    tabs: Vec<Tab>,
    scope: TabScope<A>,
    affinities: HashMap<TabId, Vec<A>>,
    history: HashMap<TabScope<A>, TabHistory>,
    // Utility descriptors retain their IDs while only their container appears in the tab strip.
    embedded_pages: HashMap<TabId, TabId>,
    zoomed_panes: HashMap<TabId, PaneId>,
    active_tab_id: Option<TabId>,
    active_tab_index_cache: Cell<Option<(TabId, usize)>>,
    next_tab_id: u64,
    next_pane_id: u64,
    tab_mounts: HashMap<TabId, M>,
    pending_detach_mounts: HashMap<TabId, TabMountId>,
    next_tab_mount_id: u64,
    terminal_output_highlight_enabled: bool,
    tabs_with_unread_terminal_output: HashSet<TabId>,
    tabs_with_bell_highlight: HashSet<TabId>,
}

impl<A: Eq + Hash + Clone, M> Default for TabLayout<A, M> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            embedded_pages: HashMap::new(),
            zoomed_panes: HashMap::new(),
            active_tab_id: None,
            scope: TabScope::All,
            affinities: HashMap::new(),
            history: HashMap::new(),
            active_tab_index_cache: Cell::new(None),
            next_tab_id: 1,
            next_pane_id: 1,
            tab_mounts: HashMap::new(),
            pending_detach_mounts: HashMap::new(),
            next_tab_mount_id: 1,
            terminal_output_highlight_enabled: true,
            tabs_with_unread_terminal_output: HashSet::new(),
            tabs_with_bell_highlight: HashSet::new(),
        }
    }
}

impl<A: Eq + Hash + Clone, M> TabLayout<A, M> {
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// The strip, navigation, and batch actions share this ordered window-local list.
    /// Indices still address the canonical collection used by tab lifecycle operations.
    pub fn main_tabs(&self) -> impl DoubleEndedIterator<Item = (usize, &Tab)> {
        self.unfiltered_main_tabs()
            .filter(|(_, tab)| self.matches_scope(tab.id))
    }

    pub fn unfiltered_main_tabs(&self) -> impl DoubleEndedIterator<Item = (usize, &Tab)> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| !self.is_outside_main_window(tab.id))
    }

    pub fn scope(&self) -> &TabScope<A> {
        &self.scope
    }

    fn matches_scope(&self, tab: TabId) -> bool {
        match &self.scope {
            TabScope::All => true,
            TabScope::Affinity(affinity) => self
                .affinities
                .get(&tab)
                .is_some_and(|values| values.contains(affinity)),
        }
    }

    pub fn update_affinities(&mut self, affinities: HashMap<TabId, Vec<A>>) {
        self.affinities = affinities;
        self.history.retain(|scope, _| match scope {
            TabScope::All => true,
            TabScope::Affinity(key) => self.affinities.values().any(|keys| keys.contains(key)),
        });
    }

    pub fn prepare_scope(&mut self, scope: TabScope<A>) {
        self.scope = scope;
    }

    pub fn switch_scope(&mut self, scope: TabScope<A>) -> Option<TabId> {
        // Clearing a filter reveals other tabs without leaving the current page.
        // Empty scopes still fall back to the remembered selection.
        let keep_active = (scope == TabScope::All)
            .then_some(self.active_tab_id)
            .flatten();
        self.prepare_scope(scope);
        let remembered = self.recent_tabs().next();
        let next = keep_active
            .filter(|id| self.main_tabs().any(|(_, tab)| tab.id == *id))
            .or(remembered)
            .or_else(|| self.main_tabs().next().map(|(_, tab)| tab.id));
        self.select_main_tab(next)
    }

    pub fn reconcile_scope_selection(&mut self) -> Option<TabId> {
        if self
            .active_tab_id
            .is_some_and(|id| self.main_tabs().any(|(_, tab)| tab.id == id))
        {
            return self.active_tab_id;
        }
        let next = self.main_tabs().next().map(|(_, tab)| tab.id);
        self.select_main_tab(next)
    }

    pub fn other_main_tab_ids(&self, tab_id: TabId) -> Vec<TabId> {
        self.main_tabs()
            .filter(|(_, tab)| tab.id != tab_id)
            .map(|(_, tab)| tab.id)
            .collect()
    }

    pub fn active_tab_id(&self) -> Option<TabId> {
        self.active_tab_id
    }

    pub fn active_tab_index(&self) -> Option<usize> {
        let active_tab_id = self.active_tab_id?;
        if let Some((cached_tab_id, cached_index)) = self.active_tab_index_cache.get()
            && cached_tab_id == active_tab_id
            && self
                .tabs
                .get(cached_index)
                .is_some_and(|tab| tab.id == active_tab_id)
        {
            return Some(cached_index);
        }
        let index = self.tabs.iter().position(|tab| tab.id == active_tab_id)?;
        self.active_tab_index_cache
            .set(Some((active_tab_id, index)));
        Some(index)
    }

    pub fn tab_index_by_id(&self, tab_id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    pub fn tab_by_id(&self, tab_id: TabId) -> Option<&Tab> {
        self.tab_index_by_id(tab_id)
            .and_then(|index| self.tabs.get(index))
    }

    /// A terminal title event must not overwrite the name of its split workspace.
    pub fn rename_terminal_tab(&mut self, tab_id: TabId, title: &str) -> bool {
        if self
            .tab_by_id(tab_id)
            .is_none_or(|t| t.kind == TabKind::Workspace)
        {
            return false;
        }
        self.rename_tab(tab_id, title)
    }

    pub fn rename_tab(&mut self, tab_id: TabId, title: &str) -> bool {
        let normalized_title = title.trim();
        let Some(tab) = self.tab_mut_by_id(tab_id) else {
            return false;
        };
        if normalized_title.is_empty() || !tab_is_renamable(&tab.kind) {
            return false;
        }
        tab.title.clear();
        tab.title.push_str(normalized_title);
        tab.title_source = TabTitleSource::Static;
        true
    }

    fn tab_mut_by_id(&mut self, tab_id: TabId) -> Option<&mut Tab> {
        let index = self.tab_index_by_id(tab_id)?;
        self.tabs.get_mut(index)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_index()
            .and_then(|index| self.tabs.get(index))
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active_tab_index()?;
        self.tabs.get_mut(index)
    }

    /// Inserts a tab into the canonical collection and invalidates index projections.
    pub fn insert_tab(&mut self, tab: Tab) {
        debug_assert!(
            self.tab_by_id(tab.id).is_none(),
            "tab identity must be unique inside one workspace"
        );
        self.tabs.push(tab);
        self.active_tab_index_cache.set(None);
    }

    pub fn insert_and_select_main_tab(&mut self, tab: Tab) -> Option<TabId> {
        let tab_id = tab.id;
        self.insert_tab(tab);
        self.select_main_tab(Some(tab_id))
    }

    /// Removes one canonical tab and chooses the next main-window selection atomically.
    pub fn remove_tab_at(&mut self, index: usize) -> Option<RemovedTab<M>> {
        let page_id = self.tabs.get(index)?.id;
        if let Some(container) = self.embedded_pages.remove(&page_id) {
            if let Some(tab) = self.tab_mut_by_id(container) {
                if tab
                    .root_pane
                    .as_ref()
                    .is_some_and(|root| root.pane_count() == 1)
                {
                    tab.root_pane = None;
                    tab.active_pane_id = None;
                } else if let Some(pane) = tab
                    .root_pane
                    .as_ref()
                    .and_then(|root| root.page_pane(page_id))
                {
                    self.close_pane(container, pane);
                }
            }
        }
        let previous_active_tab_id = self.active_tab_id;
        let tab = self.tabs.get(index)?;
        let removed_was_active = Some(tab.id) == previous_active_tab_id;
        let tab = self.tabs.remove(index);
        self.tabs_with_unread_terminal_output.remove(&tab.id);
        self.tabs_with_bell_highlight.remove(&tab.id);
        let mount = self.forget_mount(tab.id);
        let next_active_tab_id = if !removed_was_active
            && previous_active_tab_id.is_some_and(|tab_id| {
                self.tab_by_id(tab_id)
                    .is_some_and(|tab| !self.is_outside_main_window(tab.id))
            }) {
            previous_active_tab_id
        } else {
            self.main_tabs()
                .find(|(i, _)| *i >= index.min(self.tabs.len().saturating_sub(1)))
                .or_else(|| self.main_tabs().rev().find(|(i, _)| *i < index))
                .map(|(_, tab)| tab.id)
        };
        self.select_main_tab(next_active_tab_id);
        Some(RemovedTab {
            tab,
            mount,
            previous_active_tab_id,
            next_active_tab_id,
        })
    }

    /// Reorders a main-window tab among the visible main-window tabs.
    pub fn move_main_tab_to_visible_index(
        &mut self,
        tab_id: TabId,
        target_visible_index: usize,
    ) -> bool {
        let Some(source_index) = self
            .main_tabs()
            .find(|(_, tab)| tab.id == tab_id)
            .map(|(i, _)| i)
        else {
            return false;
        };
        let visible_slots = self.main_tabs().map(|(index, _)| index).collect::<Vec<_>>();
        let mut visible_tab_ids = self.main_tabs().map(|(_, tab)| tab.id).collect::<Vec<_>>();
        if self.scope != TabScope::All {
            let source = visible_slots
                .iter()
                .position(|index| *index == source_index)
                .unwrap();
            let target = target_visible_index.min(visible_slots.len().saturating_sub(1));
            if source == target {
                return false;
            }
            let moved = visible_tab_ids.remove(source);
            visible_tab_ids.insert(target, moved);
            // Rotate only visible slots: hidden and detached tabs keep their positions.
            for (slot, id) in visible_slots.into_iter().zip(visible_tab_ids) {
                let index = self.tab_index_by_id(id).unwrap();
                self.tabs.swap(slot, index);
            }
            self.active_tab_index_cache.set(None);
            return true;
        }
        visible_tab_ids.retain(|id| *id != tab_id);
        let target_visible_index = target_visible_index.min(visible_tab_ids.len());
        let anchor_tab_id = visible_tab_ids.get(target_visible_index).copied();
        let trailing_tab_id = target_visible_index
            .checked_sub(1)
            .and_then(|index| visible_tab_ids.get(index).copied());

        let moved_tab = self.tabs.remove(source_index);
        let insertion_index = anchor_tab_id
            .and_then(|anchor_id| self.tab_index_by_id(anchor_id))
            .or_else(|| {
                trailing_tab_id
                    .and_then(|trailing_id| self.tab_index_by_id(trailing_id))
                    .map(|index| index + 1)
            })
            .unwrap_or_else(|| source_index.min(self.tabs.len()))
            .min(self.tabs.len());
        let changed = insertion_index != source_index.min(self.tabs.len());
        self.tabs.insert(insertion_index, moved_tab);
        self.active_tab_index_cache.set(None);
        changed
    }

    pub fn set_active_pane(&mut self, tab_id: Option<TabId>, pane_id: PaneId) -> bool {
        let tab = match tab_id {
            Some(tab_id) => self.tab_mut_by_id(tab_id),
            None => self.active_tab_mut(),
        };
        let Some(tab) = tab else {
            return false;
        };
        if !tab
            .root_pane
            .as_ref()
            .is_some_and(|root| root.contains_pane(pane_id))
        {
            return false;
        }
        if tab.active_pane_id == Some(pane_id) {
            return false;
        }
        tab.active_pane_id = Some(pane_id);
        let id = tab.id;
        let terminal = tab
            .root_pane
            .as_ref()
            .and_then(|root| root.session_id_for_pane(pane_id))
            .is_some();
        if self.zoomed_panes.contains_key(&id) {
            if terminal {
                self.zoomed_panes.insert(id, pane_id);
            } else {
                self.zoomed_panes.remove(&id);
            }
        }
        true
    }

    pub fn split_pane(
        &mut self,
        tab_id: TabId,
        active_pane_id: PaneId,
        group_id: PaneId,
        direction: SplitDirection,
        pane_id: PaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        let Some(tab) = self.tab_mut_by_id(tab_id) else {
            return false;
        };
        if tab
            .root_pane
            .as_ref()
            .is_some_and(|root| root.pane_count() >= MAX_PANES_PER_TAB)
        {
            return false;
        }
        let split = tab.root_pane.as_mut().is_some_and(|root_pane| {
            root_pane.split_active(active_pane_id, group_id, direction, pane_id, session_id)
        });
        if split {
            tab.active_pane_id = Some(pane_id);
            self.zoomed_panes.remove(&tab_id);
        }
        split
    }

    /// Page identities remain stable when their containing layout changes.
    pub fn container_tab_id(&self, page: TabId) -> TabId {
        self.embedded_pages.get(&page).copied().unwrap_or(page)
    }

    pub fn focused_page_id(&self, container: TabId) -> TabId {
        self.tab_by_id(container)
            .and_then(|tab| {
                tab.root_pane
                    .as_ref()?
                    .page_id_for_pane(tab.active_pane_id?)
            })
            .unwrap_or(container)
    }

    pub fn tab_contents(&self, tab_id: TabId) -> Vec<TabContent> {
        fn collect(root: &PaneNode, contents: &mut Vec<TabContent>) {
            match root {
                PaneNode::Page { tab_id, .. } => contents.push(TabContent::Page(*tab_id)),
                PaneNode::Leaf { session_id, .. } => {
                    contents.push(TabContent::Terminal(*session_id))
                }
                PaneNode::Group { children, .. } => {
                    for child in children {
                        collect(&child.node, contents);
                    }
                }
            }
        }
        let Some(tab) = self.tab_by_id(tab_id) else {
            return Vec::new();
        };
        let mut contents = Vec::new();
        if let Some(root) = &tab.root_pane {
            collect(root, &mut contents);
        } else {
            contents.push(TabContent::Page(tab.id));
        }
        contents
    }

    pub fn surface_is_visible(&self, page: TabId) -> bool {
        let container = self.container_tab_id(page);
        (self.active_tab_id == Some(container) || self.is_detached(container))
            && (page == container || self.zoomed_pane(container).is_none())
    }

    pub fn unembed_page(&mut self, page: TabId) -> Option<TabId> {
        let container = self.embedded_pages.remove(&page)?;
        let tab = self.tab_mut_by_id(container)?;
        let root = tab.root_pane.as_ref()?;
        let pane = root.page_pane(page)?;
        if root.pane_count() == 1 {
            tab.root_pane = None;
            tab.active_pane_id = None;
        } else {
            self.close_pane(container, pane);
        }
        Some(container)
    }

    pub fn focus_content_page(&mut self, page: TabId) {
        let container = self.container_tab_id(page);
        self.zoomed_panes.remove(&container);
        if let Some(tab) = self.tab_mut_by_id(container) {
            if let Some(pane) = tab.root_pane.as_ref().and_then(|root| root.page_pane(page)) {
                tab.active_pane_id = Some(pane);
            }
        }
    }

    pub fn can_combine_pages(&self, source: TabId, target: TabId) -> bool {
        if source == target
            || self.is_outside_main_window(source)
            || self.is_outside_main_window(target)
        {
            return false;
        }
        self.pages_fit_together(source, target)
    }

    pub fn can_receive_tab_drop(&self, source: TabId, target: TabId) -> bool {
        source != target
            && self.container_tab_id(source) == source
            && !self.pending_detach_mounts.contains_key(&source)
            && !self.is_outside_main_window(target)
            && self.active_tab_id == Some(target)
            && self.pages_fit_together(source, target)
    }

    fn pages_fit_together(&self, source: TabId, target: TabId) -> bool {
        self.tab_by_id(source)
            .zip(self.tab_by_id(target))
            .is_some_and(|(a, b)| {
                a.kind != TabKind::Onboarding
                    && b.kind != TabKind::Onboarding
                    && a.root_pane.as_ref().map_or(1, PaneNode::pane_count)
                        + b.root_pane.as_ref().map_or(1, PaneNode::pane_count)
                        <= MAX_PANES_PER_TAB
            })
    }

    /// Page owners keep their IDs; only layout and terminal mount indexes change.
    pub fn combine_pages(
        &mut self,
        source: TabId,
        target: TabId,
        direction: SplitDirection,
    ) -> Option<(Tab, Vec<TabId>)> {
        self.combine_pages_at(source, target, None, direction, false)
    }

    pub fn combine_pages_at(
        &mut self,
        source: TabId,
        target: TabId,
        target_pane: Option<PaneId>,
        direction: SplitDirection,
        before: bool,
    ) -> Option<(Tab, Vec<TabId>)> {
        if !self.can_combine_pages(source, target) {
            return None;
        }
        let source_tab = self.tab_by_id(source)?.clone();
        let target_tab = self.tab_by_id(target)?.clone();
        if target_pane.is_some_and(|pane| {
            target_tab
                .root_pane
                .as_ref()
                .is_none_or(|root| !root.contains_pane(pane))
        }) {
            return None;
        }
        let source_root = source_tab
            .root_pane
            .clone()
            .unwrap_or_else(|| PaneNode::Page {
                pane_id: self.alloc_pane_id(),
                tab_id: source,
            });
        let mut root = target_tab
            .root_pane
            .clone()
            .unwrap_or_else(|| PaneNode::Page {
                pane_id: self.alloc_pane_id(),
                tab_id: target,
            });
        let focus = source_tab
            .active_pane_id
            .unwrap_or_else(|| source_root.first_pane_id());
        let target_pane = target_pane
            .or(target_tab.active_pane_id)
            .unwrap_or_else(|| root.first_pane_id());
        let group = self.alloc_pane_id();
        if !root.insert_beside(target_pane, group, direction, source_root, before) {
            return None;
        }
        let id = self.alloc_tab_id();
        let mut pages = Vec::new();
        root.collect_page_ids(&mut pages);
        for page in &pages {
            self.embedded_pages.insert(*page, id);
        }
        let removed = [source, target]
            .into_iter()
            .filter(|id| !pages.contains(id))
            .collect::<Vec<_>>();
        self.tabs.retain(|tab| !removed.contains(&tab.id));
        for old in &removed {
            self.zoomed_panes.remove(old);
            self.tabs_with_unread_terminal_output.remove(old);
            self.tabs_with_bell_highlight.remove(old);
        }
        let tab = Tab {
            id,
            kind: TabKind::Workspace,
            title: target_tab.title,
            title_source: TabTitleSource::Static,
            root_pane: Some(root),
            active_pane_id: Some(focus),
        };
        self.insert_and_select_main_tab(tab.clone());
        Some((tab, removed))
    }

    pub fn tile_terminal_panes(
        &mut self,
        panes: &[PaneId],
        title: String,
        landscape: bool,
    ) -> Option<(Tab, Vec<TabId>)> {
        let selected = panes.iter().copied().collect::<HashSet<_>>();
        if !(2..=MAX_PANES_PER_TAB).contains(&panes.len()) || selected.len() != panes.len() {
            return None;
        }
        let mut sources = Vec::new();
        let mut leaves = HashMap::new();
        for (_, tab) in self.unfiltered_main_tabs() {
            let Some(mut root) = tab.root_pane.clone() else {
                continue;
            };
            let mut ids = Vec::new();
            root.collect_pane_ids(&mut ids);
            let moving = ids
                .into_iter()
                .filter(|id| selected.contains(id))
                .collect::<Vec<_>>();
            if moving.is_empty() {
                continue;
            }
            let mut empty = false;
            for pane in moving {
                let session = root.session_id_for_pane(pane)?;
                leaves.insert(pane, PaneNode::leaf(pane, session));
                if root.pane_count() == 1 {
                    empty = true;
                } else {
                    root.close_pane(pane)?;
                    if let Some(only) = root.single_child_replacement() {
                        root = only;
                    }
                }
            }
            let next_active = tab
                .active_pane_id
                .filter(|id| !selected.contains(id))
                .unwrap_or_else(|| root.first_pane_id());
            sources.push((tab.id, (!empty).then_some(root), next_active));
        }
        let nodes = panes
            .iter()
            .map(|id| leaves.remove(id))
            .collect::<Option<Vec<_>>>()?;
        let root = PaneNode::tiled(nodes, landscape, || self.alloc_pane_id())?;
        let active = self
            .active_tab()
            .and_then(|tab| tab.active_pane_id)
            .filter(|id| selected.contains(id))
            .unwrap_or(panes[0]);
        let id = self.alloc_tab_id();
        let mut removed = Vec::new();
        for (source, root, active) in sources {
            self.zoomed_panes.remove(&source);
            if let Some(root) = root {
                let page_title = match &root {
                    PaneNode::Page { tab_id, .. } => self
                        .tab_by_id(*tab_id)
                        .map(|page| (page.title.clone(), page.title_source.clone())),
                    _ => None,
                };
                let tab = self.tab_mut_by_id(source).unwrap();
                if let Some((title, title_source)) = page_title {
                    tab.title = title;
                    tab.title_source = title_source;
                }
                tab.root_pane = Some(root);
                tab.active_pane_id = Some(active);
            } else {
                removed.push(source);
            }
        }
        self.tabs.retain(|tab| !removed.contains(&tab.id));
        for old in &removed {
            self.tabs_with_unread_terminal_output.remove(old);
            self.tabs_with_bell_highlight.remove(old);
            self.affinities.remove(old);
        }
        let tab = Tab {
            id,
            kind: TabKind::Workspace,
            title,
            title_source: TabTitleSource::Static,
            root_pane: Some(root),
            active_pane_id: Some(active),
        };
        self.insert_and_select_main_tab(tab.clone());
        Some((tab, removed))
    }

    /// Exchange leaf positions, retaining each slot's size and each mounted body's identity.
    pub fn swap_panes(&mut self, tab: TabId, source: PaneId, target: PaneId) -> bool {
        if source == target {
            return false;
        }
        let Some(root) = self
            .tab_mut_by_id(tab)
            .and_then(|tab| tab.root_pane.as_mut())
        else {
            return false;
        };
        let snapshot = |id| {
            root.session_id_for_pane(id)
                .map(|session| PaneNode::leaf(id, session))
                .or_else(|| {
                    root.page_id_for_pane(id).map(|tab_id| PaneNode::Page {
                        pane_id: id,
                        tab_id,
                    })
                })
        };
        let (Some(a), Some(b)) = (snapshot(source), snapshot(target)) else {
            return false;
        };
        fn swap(node: &mut PaneNode, a: &PaneNode, b: &PaneNode) {
            match node {
                PaneNode::Group { children, .. } => {
                    for child in children {
                        swap(&mut child.node, a, b)
                    }
                }
                _ if node.first_pane_id() == a.first_pane_id() => *node = b.clone(),
                _ if node.first_pane_id() == b.first_pane_id() => *node = a.clone(),
                _ => {}
            }
        }
        swap(root, &a, &b);
        true
    }

    pub fn zoomed_pane(&self, tab: TabId) -> Option<PaneId> {
        self.zoomed_panes.get(&tab).copied().filter(|pane| {
            self.tab_by_id(tab)
                .and_then(|tab| tab.root_pane.as_ref())
                .is_some_and(|root| root.pane_count() > 1 && root.contains_pane(*pane))
        })
    }

    pub fn toggle_pane_zoom(&mut self, tab_id: TabId, pane_id: PaneId) -> bool {
        let Some(root) = self
            .tab_by_id(tab_id)
            .and_then(|tab| tab.root_pane.as_ref())
        else {
            return false;
        };
        if root.pane_count() < 2 || root.session_id_for_pane(pane_id).is_none() {
            return false;
        }
        if self.zoomed_pane(tab_id) == Some(pane_id) {
            self.zoomed_panes.remove(&tab_id);
        } else {
            self.zoomed_panes.insert(tab_id, pane_id);
        }
        self.tab_mut_by_id(tab_id).unwrap().active_pane_id = Some(pane_id);
        true
    }

    /// Moving a pane preserves its Entity, session, subscriptions, and runtime consumers.
    pub fn move_terminal_pane_to_tab(
        &mut self,
        source_id: TabId,
        pane_id: PaneId,
        new_tab: Tab,
    ) -> bool {
        if self.tab_by_id(new_tab.id).is_some()
            || self.pending_detach_mounts.contains_key(&source_id)
        {
            return false;
        }
        let Some(source) = self.tab_by_id(source_id) else {
            return false;
        };
        let Some(root) = source.root_pane.as_ref() else {
            return false;
        };
        let Some(session_id) = root.session_id_for_pane(pane_id) else {
            return false;
        };
        if root.pane_count() <= 1
            || new_tab.root_pane.as_ref() != Some(&PaneNode::leaf(pane_id, session_id))
        {
            return false;
        }
        if self.close_pane(source_id, pane_id).is_none() {
            return false;
        }
        self.insert_tab(new_tab);
        true
    }

    pub fn close_pane(&mut self, tab_id: TabId, pane_id: PaneId) -> Option<PaneId> {
        if self.zoomed_panes.get(&tab_id) == Some(&pane_id) {
            self.zoomed_panes.remove(&tab_id);
        }
        let tab = self.tab_mut_by_id(tab_id)?;
        let previous_active = tab.active_pane_id;
        let root_pane = tab.root_pane.as_mut()?;
        let next_active_pane_id = root_pane.close_pane(pane_id)?;
        if let Some(replacement) = root_pane.single_child_replacement() {
            tab.root_pane = Some(replacement);
        }
        tab.active_pane_id = previous_active
            .filter(|id| *id != pane_id)
            .or(Some(next_active_pane_id));
        let remaining_page = match &tab.root_pane {
            Some(PaneNode::Page { tab_id, .. }) => Some(*tab_id),
            _ => None,
        };
        if let Some((title, title_source)) = remaining_page
            .and_then(|id| self.tab_by_id(id))
            .map(|page| (page.title.clone(), page.title_source.clone()))
        {
            let tab = self.tab_mut_by_id(tab_id)?;
            tab.title = title;
            tab.title_source = title_source;
        }
        Some(next_active_pane_id)
    }

    /// Restores a collapsed merged `TabKind::Workspace` container back to a
    /// single terminal tab once only one `PaneNode::Leaf` remains.
    pub fn restore_single_terminal_workspace_tab(
        &mut self,
        tab_id: TabId,
        kind: TabKind,
        title: String,
    ) -> Option<Tab> {
        let tab = self.tab_mut_by_id(tab_id)?;
        if tab.kind != TabKind::Workspace || !matches!(tab.root_pane, Some(PaneNode::Leaf { .. })) {
            return None;
        }
        tab.kind = kind;
        if !title.trim().is_empty() {
            tab.title = title;
            tab.title_source = TabTitleSource::Static;
        }
        Some(tab.clone())
    }

    pub fn update_group_sizes(
        &mut self,
        tab_id: Option<TabId>,
        group_id: PaneId,
        sizes: &[f32],
    ) -> bool {
        let tab = match tab_id {
            Some(tab_id) => self.tab_mut_by_id(tab_id),
            None => self.active_tab_mut(),
        };
        tab.and_then(|tab| tab.root_pane.as_mut())
            .is_some_and(|root_pane| root_pane.update_group_sizes(group_id, sizes))
    }

    pub fn reset_group_sizes(&mut self, tab_id: Option<TabId>, group_id: PaneId) -> bool {
        let tab = match tab_id {
            Some(tab_id) => self.tab_mut_by_id(tab_id),
            None => self.active_tab_mut(),
        };
        tab.and_then(|tab| tab.root_pane.as_mut())
            .is_some_and(|root_pane| root_pane.reset_group_sizes(group_id))
    }

    pub fn sync_tab_titles(&mut self, mut title_for_key: impl FnMut(&'static str) -> String) {
        for tab in &mut self.tabs {
            if let TabTitleSource::I18nKey(key) = tab.title_source {
                tab.title = title_for_key(key);
            }
        }
    }

    /// Updates the canonical main-window selection and navigation history together.
    pub fn select_main_tab(&mut self, active_tab_id: Option<TabId>) -> Option<TabId> {
        let active_tab_id = active_tab_id.map(|page| {
            let container = self.container_tab_id(page);
            if container != page
                && let Some(tab) = self.tab_mut_by_id(container)
            {
                if let Some(pane) = tab.root_pane.as_ref().and_then(|root| root.page_pane(page)) {
                    tab.active_pane_id = Some(pane);
                }
            }
            container
        });
        debug_assert!(
            active_tab_id.is_none_or(|tab_id| self.tab_by_id(tab_id).is_some()),
            "main-window selection must reference a canonical tab"
        );
        let previous_active_tab_id = self.active_tab_id;
        self.active_tab_id = active_tab_id;
        if let Some(tab_id) = active_tab_id {
            self.tabs_with_unread_terminal_output.remove(&tab_id);
            self.tabs_with_bell_highlight.remove(&tab_id);
        }
        self.active_tab_index_cache.set(None);
        self.observe_active_tab(active_tab_id);
        previous_active_tab_id
    }

    pub fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    pub fn alloc_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    pub fn bell_highlighted_tab_ids(&self) -> HashSet<TabId> {
        self.tabs_with_bell_highlight.clone()
    }

    pub fn mark_bell(&mut self, tab_id: TabId) -> Option<TabId> {
        if Some(tab_id) == self.active_tab_id || self.is_outside_main_window(tab_id) {
            return None;
        }
        self.tabs_with_bell_highlight
            .insert(tab_id)
            .then_some(tab_id)
    }

    pub fn unread_terminal_output_tab_ids(&self) -> HashSet<TabId> {
        // Rendering needs stable identities after releasing the Entity borrow.
        self.tabs_with_unread_terminal_output.clone()
    }

    pub fn configure_terminal_output_highlight(&mut self, enabled: bool) {
        self.terminal_output_highlight_enabled = enabled;
        if !enabled {
            self.tabs_with_unread_terminal_output.clear();
        }
    }

    pub fn mark_unread(&mut self, tab_id: TabId) -> Option<TabId> {
        if !self.terminal_output_highlight_enabled {
            return None;
        }
        if self.active_tab_id == Some(tab_id)
            || self.is_outside_main_window(tab_id)
            || self.tab_by_id(tab_id).is_none()
        {
            return None;
        }
        self.tabs_with_unread_terminal_output
            .insert(tab_id)
            .then_some(tab_id)
    }

    pub fn begin_detach(&mut self, tab_id: TabId) -> Option<TabMountId> {
        if self.pending_detach_mounts.contains_key(&tab_id) || self.tab_mounts.contains_key(&tab_id)
        {
            return None;
        }
        let mount_id = TabMountId(self.next_tab_mount_id);
        self.next_tab_mount_id = self.next_tab_mount_id.wrapping_add(1);
        self.pending_detach_mounts.insert(tab_id, mount_id);
        Some(mount_id)
    }

    pub fn commit_mount(&mut self, tab: TabId, reservation: TabMountId, mount: M) -> bool {
        if !self.rollback_detach(tab, reservation) {
            return false;
        }
        self.tab_mounts.insert(tab, mount);
        true
    }
    pub fn detached_mount(&self, tab: TabId) -> Option<&M> {
        self.tab_mounts.get(&tab)
    }
    pub fn return_mount(&mut self, tab: TabId) -> Option<M> {
        self.tab_mounts.remove(&tab)
    }
    fn forget_mount(&mut self, tab: TabId) -> Option<M> {
        self.zoomed_panes.remove(&tab);
        self.pending_detach_mounts.remove(&tab);
        self.return_mount(tab)
    }
    pub fn clear_highlights(&mut self, tab: TabId) {
        self.tabs_with_bell_highlight.remove(&tab);
        self.tabs_with_unread_terminal_output.remove(&tab);
    }
    pub fn rollback_detach(&mut self, tab_id: TabId, mount_id: TabMountId) -> bool {
        if self.pending_detach_mounts.get(&tab_id).copied() != Some(mount_id) {
            return false;
        }
        self.pending_detach_mounts.remove(&tab_id);
        true
    }

    pub fn is_outside_main_window(&self, tab_id: TabId) -> bool {
        self.embedded_pages.contains_key(&tab_id)
            || self.pending_detach_mounts.contains_key(&tab_id)
            || self.tab_mounts.contains_key(&tab_id)
    }

    pub fn nearest_main_tab(&self, index: usize) -> Option<TabId> {
        self.main_tabs()
            .find(|(i, _)| *i > index)
            .or_else(|| self.main_tabs().rev().find(|(i, _)| *i < index))
            .map(|(_, tab)| tab.id)
    }

    pub fn is_detached(&self, tab_id: TabId) -> bool {
        let tab_id = self.container_tab_id(tab_id);
        self.tab_mounts.contains_key(&tab_id)
    }

    pub fn recent_tabs(&self) -> impl Iterator<Item = TabId> + '_ {
        self.history
            .get(&self.scope)
            .into_iter()
            .flat_map(|history| history.recent.iter().copied())
            .filter(|id| self.main_tabs().any(|(_, tab)| tab.id == *id))
    }
    pub fn observe_active_tab(&mut self, tab: Option<TabId>) {
        if let Some(tab) = tab {
            self.history
                .entry(self.scope.clone())
                .or_default()
                .record(tab)
        }
    }
    pub fn navigate_history(&mut self, forward: bool, visible: &HashSet<TabId>) -> Option<TabId> {
        self.history
            .get_mut(&self.scope)?
            .navigate(forward, visible)
    }
}
