// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::{
    CatalogAction, SessionManagerOpenTarget, SessionManagerRowActionTarget,
    SessionManagerSelectionTarget, SessionSortField, SortDirection,
    session_manager_row_menu_selection_target,
};
use crate::icons::SessionIcon;
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_plugin_protocol::discovery::DiscoveryGroupSegment;
use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

/// Display metadata only. Hosts keep credentials, launch commands and protocol configuration
/// behind the stable target; none is needed by a virtual row or its context menu.
#[derive(Clone)]
pub struct CatalogRow {
    pub target: SessionManagerOpenTarget,
    /// Target IDs are scoped to a source, so discovered rows use a composite display ID.
    pub discovery_row_id: String,
    pub group_paths: Vec<Vec<DiscoveryGroupSegment>>,
    pub name: String,
    pub group: Option<String>,
    pub subtitle: String,
    pub host: String,
    pub port: u32,
    pub username: String,
    pub auth_label: String,
    pub last_used: Option<String>,
    /// Additional non-secret terms, such as a service name or device identifier.
    pub search_text: String,
    pub tags: Vec<String>,
    pub protocol_badge: Option<String>,
    pub active: bool,
    pub icon: SessionIcon,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
}

impl CatalogRow {
    pub fn new(target: SessionManagerOpenTarget, name: impl Into<String>) -> Self {
        use SessionManagerOpenTarget::*;
        let icon = match &target {
            Tunnel(_) => LucideIcon::Network,
            Plugin(_) => LucideIcon::Puzzle,
            Connection(_) | Discovery(_) => LucideIcon::Server,
            SshConfig(_) => LucideIcon::FileTerminal,
            Serial(_) => LucideIcon::Radio,
            Ble(_) => LucideIcon::Bluetooth,
            LocalTerminal(_) | Telnet(_) => LucideIcon::Terminal,
            Ftp(_) | StandaloneSftp(_) => LucideIcon::FolderSync,
            Mosh(_) => LucideIcon::Wifi,
            RemoteDesktop(_) => LucideIcon::Monitor,
        }
        .into();
        Self {
            discovery_row_id: match &target {
                Discovery((source, target)) => {
                    format!("discovery:{}:{source}{target}", source.len())
                }
                _ => String::new(),
            },
            group_paths: Vec::new(),
            tags: Vec::new(),
            protocol_badge: None,
            active: false,
            target,
            name: name.into(),
            icon,
            group: None,
            subtitle: String::new(),
            host: String::new(),
            port: 0,
            username: String::new(),
            auth_label: String::new(),
            last_used: None,
            search_text: String::new(),
            color: None,
            icon_background_color: None,
        }
    }
    pub fn id(&self) -> &str {
        use SessionManagerOpenTarget::*;
        match &self.target {
            Tunnel(id) | Plugin(id) | Connection(id) | SshConfig(id) | Serial(id) | Ble(id)
            | LocalTerminal(id) | Telnet(id) | Ftp(id) | Mosh(id) | StandaloneSftp(id)
            | RemoteDesktop(id) => id,
            Discovery(_) => &self.discovery_row_id,
        }
    }
    pub(super) fn row_action_target(&self) -> SessionManagerRowActionTarget {
        use SessionManagerOpenTarget as Open;
        use SessionManagerRowActionTarget as Row;
        match &self.target {
            Open::Tunnel(id) => Row::Tunnel(id.clone()),
            Open::Plugin(id) => Row::Plugin(id.clone()),
            Open::Connection(id) => Row::Connection(id.clone()),
            Open::SshConfig(id) => Row::SshConfig(id.clone()),
            Open::Discovery(key) => Row::Discovery(key.clone()),
            Open::Serial(id) => Row::Serial(id.clone()),
            Open::Ble(id) => Row::Ble(id.clone()),
            Open::LocalTerminal(id) => Row::LocalTerminal(id.clone()),
            Open::Telnet(id) => Row::Telnet(id.clone()),
            Open::Ftp(id) => Row::Ftp(id.clone()),
            Open::Mosh(id) => Row::Mosh(id.clone()),
            Open::StandaloneSftp(id) => Row::StandaloneSftp(id.clone()),
            Open::RemoteDesktop(id) => Row::RemoteDesktop(id.clone()),
        }
    }
}

pub(super) trait CatalogEntry {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn group(&self) -> Option<&str>;
    fn discovery_source(&self) -> Option<&str>;
    fn group_paths(&self) -> &[Vec<DiscoveryGroupSegment>];
    fn last_used(&self) -> Option<String>;
    fn host(&self) -> &str;
    fn port_sort_key(&self) -> u32;
    fn username(&self) -> &str;
    fn auth_sort_key(&self) -> String;
    fn subtitle(&self) -> String;
    fn icon_color(&self) -> Option<&str>;
    fn icon_background_color(&self) -> Option<&str>;
    fn selection_target(&self) -> Option<SessionManagerSelectionTarget>;
    fn hash_appearance(&self, hasher: &mut DefaultHasher);
    fn matches_query(&self, query: &str) -> bool;
}

impl CatalogEntry for CatalogRow {
    fn group_paths(&self) -> &[Vec<DiscoveryGroupSegment>] {
        &self.group_paths
    }
    fn discovery_source(&self) -> Option<&str> {
        match &self.target {
            SessionManagerOpenTarget::Discovery((source, _)) => Some(source),
            _ => None,
        }
    }
    fn hash_appearance(&self, hasher: &mut DefaultHasher) {
        std::mem::discriminant(&self.target).hash(hasher);
        if self.active || self.protocol_badge.is_some() {
            self.active.hash(hasher);
            self.protocol_badge.hash(hasher);
        }
    }
    fn matches_query(&self, query: &str) -> bool {
        [
            &self.name,
            &self.subtitle,
            &self.host,
            &self.username,
            &self.auth_label,
            &self.search_text,
        ]
        .into_iter()
        .chain(self.group.iter())
        .chain(self.protocol_badge.iter())
        .chain(&self.tags)
        .any(|field| field.to_lowercase().contains(query))
            || (self.port != 0 && self.port.to_string().contains(query))
    }
    fn id(&self) -> &str {
        self.id()
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
    fn last_used(&self) -> Option<String> {
        self.last_used.clone()
    }
    fn host(&self) -> &str {
        &self.host
    }
    fn port_sort_key(&self) -> u32 {
        self.port
    }
    fn username(&self) -> &str {
        &self.username
    }
    fn auth_sort_key(&self) -> String {
        self.auth_label.clone()
    }
    fn subtitle(&self) -> String {
        self.subtitle.clone()
    }
    fn icon_color(&self) -> Option<&str> {
        self.color.as_deref()
    }
    fn icon_background_color(&self) -> Option<&str> {
        self.icon_background_color.as_deref()
    }
    fn selection_target(&self) -> Option<SessionManagerSelectionTarget> {
        session_manager_row_menu_selection_target(&self.row_action_target())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TunnelDisplayStatus {
    Starting,
    Active,
    Suspended,
    #[default]
    Stopped,
    Error,
}
impl TunnelDisplayStatus {
    pub fn can_stop(self) -> bool {
        matches!(self, Self::Starting | Self::Active | Self::Suspended)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionManagerGridRow {
    SectionHeader {
        title: String,
        item_count: usize,
    },
    RecentItems {
        item_indices: Vec<usize>,
        is_last_in_section: bool,
    },
    Cards {
        item_indices: Vec<usize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DiscoveryCatalogGroup {
    pub id: String,
    pub name: String,
    pub failed: bool,
    pub hint: Option<String>,
}

impl DiscoveryCatalogGroup {
    pub(super) fn menu_actions(&self) -> Vec<(&'static str, &'static str, CatalogAction)> {
        vec![
            (
                if self.failed {
                    "settings_view.help.retry"
                } else {
                    "settings_view.known_hosts.refresh"
                },
                "lucide/refresh-cw.svg",
                CatalogAction::RefreshDiscovery(self.id.clone()),
            ),
            (
                "settings_view.discovery.edit_source",
                "lucide/pencil.svg",
                CatalogAction::EditDiscoverySource(self.id.clone()),
            ),
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionManagerTreeRow {
    DiscoveryGroup {
        group: DiscoveryCatalogGroup,
        expanded: bool,
        count: usize,
    },
    DiscoverySubgroup {
        key: Vec<String>,
        label: String,
        count: usize,
        expanded: bool,
    },
    DiscoveryStatus(String),
    Group {
        path: String,
        depth: usize,
        expanded: bool,
        has_children: bool,
    },
    Item {
        item_index: usize,
        depth: usize,
    },
}

impl SessionManagerTreeRow {
    pub fn depth(&self) -> usize {
        match self {
            Self::Group { depth, .. } | Self::Item { depth, .. } => *depth,
            Self::DiscoveryGroup { .. } => 0,
            Self::DiscoveryStatus(_) => 1,
            Self::DiscoverySubgroup { key, .. } => key.len() - 1,
        }
    }
}

pub(super) fn group_tree_from_paths(
    paths: HashSet<String>,
) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let mut sorted = paths.into_iter().collect::<Vec<_>>();
    sorted.sort();
    let mut roots = Vec::new();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for path in sorted {
        if let Some((parent, _name)) = path.rsplit_once('/') {
            children.entry(parent.to_string()).or_default().push(path);
        } else {
            roots.push(path);
        }
    }
    (roots, children)
}

fn session_manager_tree_row_is_last(rows: &[SessionManagerTreeRow], index: usize) -> bool {
    let Some(row) = rows.get(index) else {
        return true;
    };
    let target_depth = row.depth();
    for next in rows.iter().skip(index + 1) {
        let next_depth = next.depth();
        if next_depth < target_depth {
            return true;
        }
        if next_depth == target_depth {
            return false;
        }
    }
    true
}

/// Computes which outer guide-rail columns (`0..depth - 1`) continue past the
/// row at `target_index`, and whether the row itself ends its parent's rail.
pub fn session_manager_tree_row_rails(
    rows: &[SessionManagerTreeRow],
    target_index: usize,
) -> (Vec<bool>, bool) {
    let mut continues_at_depth: Vec<bool> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let depth = row.depth();
        continues_at_depth.resize(depth, false);
        let is_last = session_manager_tree_row_is_last(rows, index);
        if index == target_index {
            let outer_rails = if depth > 1 {
                continues_at_depth[1..depth].to_vec()
            } else {
                Vec::new()
            };
            return (outer_rails, is_last);
        }
        continues_at_depth.push(depth > 0 && !is_last);
    }
    (Vec::new(), true)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionManagerItemPointerAction {
    None,
    Open,
    SelectReplace,
    SelectToggle,
    SelectRange,
}

/// Keeps card and list-row pointer behavior aligned across Session Manager layouts.
/// Double-click always opens; a single click on a selectable row replaces the
/// selection, Ctrl toggles it, and Shift extends from the anchor to this row.
pub fn session_manager_item_pointer_action(
    click_count: usize,
    selectable: bool,
    control: bool,
    shift: bool,
) -> SessionManagerItemPointerAction {
    match click_count {
        2 => SessionManagerItemPointerAction::Open,
        1 if selectable && shift => SessionManagerItemPointerAction::SelectRange,
        1 if selectable && control => SessionManagerItemPointerAction::SelectToggle,
        1 if selectable => SessionManagerItemPointerAction::SelectReplace,
        _ => SessionManagerItemPointerAction::None,
    }
}

pub(super) fn filtered_catalog<T: CatalogEntry>(
    items: impl Iterator<Item = T>,
    state: &super::SessionManagerState,
) -> Vec<T> {
    let query = state.search_query.trim().to_lowercase();
    let mut items = items
        .filter(|item| query.is_empty() || item.matches_query(&query))
        .collect::<Vec<_>>();
    sort_session_manager_items(&mut items, state.sort_field, state.sort_direction);
    items
}

pub(super) fn sort_session_manager_items<T: CatalogEntry>(
    items: &mut [T],
    field: SessionSortField,
    direction: SortDirection,
) {
    fn sort<T: CatalogEntry, K: Ord>(
        items: &mut [T],
        direction: SortDirection,
        primary_key: impl Fn(&T) -> K,
    ) {
        // Cache normalized text for this sort only; display settings and item
        // edits remain visible immediately without persistent invalidation state.
        let key = |item: &T| {
            (
                primary_key(item),
                item.name().to_lowercase(),
                item.id().to_owned(),
            )
        };
        match direction {
            SortDirection::Asc => items.sort_by_cached_key(key),
            SortDirection::Desc => items.sort_by_cached_key(|item| std::cmp::Reverse(key(item))),
        }
    }
    match field {
        SessionSortField::Name => sort(items, direction, |_| ()),
        SessionSortField::Host => sort(items, direction, |item| item.host().to_lowercase()),
        SessionSortField::Port => sort(items, direction, T::port_sort_key),
        SessionSortField::Username => sort(items, direction, |item| item.username().to_lowercase()),
        SessionSortField::AuthType => sort(items, direction, T::auth_sort_key),
        SessionSortField::Group => sort(items, direction, |item| {
            item.group().unwrap_or_default().to_lowercase()
        }),
        SessionSortField::LastUsed => sort(items, direction, T::last_used),
    }
}

pub(super) fn session_manager_grid_rows<T: CatalogEntry>(
    items: &[T],
    roots: &[String],
    recent_title: String,
    hosts_title: String,
    card_columns: usize,
    recent_columns: usize,
) -> Vec<SessionManagerGridRow> {
    let mut rows = Vec::new();
    let recent_indices = recent_session_item_indices(items);
    push_session_manager_grid_section(
        &mut rows,
        recent_title,
        &recent_indices,
        recent_columns,
        true,
    );

    // Route items through their ancestor paths once instead of scanning all
    // connections separately for each root's subtree. Indices retain display order.
    let mut grouped_items: HashMap<&str, Vec<usize>> = roots
        .iter()
        .map(|root| (root.as_str(), Vec::new()))
        .collect();
    let mut host_indices = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut group = item.group();
        if roots.is_empty() || group.is_none() {
            host_indices.push(index);
            continue;
        }
        while let Some(path) = group {
            if let Some(indices) = grouped_items.get_mut(path) {
                indices.push(index);
            }
            group = path.rsplit_once('/').map(|(parent, _)| parent);
        }
    }

    // Grid mode keeps each root group as one section containing its subtree.
    for group in roots {
        let group_indices = grouped_items
            .get(group.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        push_session_manager_grid_section(
            &mut rows,
            group_display_name(group),
            group_indices,
            card_columns,
            false,
        );
    }

    push_session_manager_grid_section(&mut rows, hosts_title, &host_indices, card_columns, false);
    rows
}

fn push_session_manager_grid_section(
    rows: &mut Vec<SessionManagerGridRow>,
    title: String,
    item_indices: &[usize],
    columns: usize,
    recent: bool,
) {
    if item_indices.is_empty() {
        return;
    }
    rows.push(SessionManagerGridRow::SectionHeader {
        title,
        item_count: item_indices.len(),
    });
    let chunk_count = item_indices.len().div_ceil(columns.max(1));
    for (row_index, chunk) in item_indices.chunks(columns.max(1)).enumerate() {
        if recent {
            rows.push(SessionManagerGridRow::RecentItems {
                item_indices: chunk.to_vec(),
                is_last_in_section: row_index + 1 == chunk_count,
            });
        } else {
            rows.push(SessionManagerGridRow::Cards {
                item_indices: chunk.to_vec(),
            });
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub(super) fn session_manager_tree_rows<T: CatalogEntry>(
    items: &[T],
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
    search_active: bool,
) -> Vec<SessionManagerTreeRow> {
    session_manager_tree_with_sources(
        items,
        roots,
        children,
        expanded_groups,
        &[],
        &HashSet::new(),
        search_active,
    )
}

pub(super) fn session_manager_tree_with_sources<T: CatalogEntry>(
    items: &[T],
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
    sources: &[DiscoveryCatalogGroup],
    expanded_discovery: &HashSet<Vec<String>>,
    search_active: bool,
) -> Vec<SessionManagerTreeRow> {
    let mut discovery_items: HashMap<&str, Vec<usize>> = HashMap::new();
    // Index direct membership once; scanning every connection for each visible
    // group makes a tree repaint quadratic as saved connections accumulate.
    let mut grouped_items: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(source) = item.discovery_source() {
            discovery_items.entry(source).or_default().push(index);
        } else {
            grouped_items.entry(item.group()).or_default().push(index);
        }
    }
    // Items are filtered before the tree is projected, so a collapsed group
    // would silently hide every match. While searching, collect the groups on
    // each matching item's ancestor path: those are the only groups worth
    // rendering, and they must ignore the saved disclosure state.
    let matching_groups = search_active.then(|| {
        let mut groups = HashSet::new();
        for group in items.iter().filter_map(CatalogEntry::group) {
            let mut path = Some(group);
            while let Some(current) = path {
                groups.insert(current);
                path = current.rsplit_once('/').map(|(parent, _)| parent);
            }
        }
        groups
    });
    let mut rows = Vec::new();
    for root in roots {
        push_session_manager_tree_group_rows(
            &mut rows,
            root,
            0,
            &grouped_items,
            children,
            expanded_groups,
            matching_groups.as_ref(),
        );
    }
    rows.extend(
        grouped_items
            .remove(&None)
            .unwrap_or_default()
            .into_iter()
            .map(|item_index| SessionManagerTreeRow::Item {
                item_index,
                depth: 0,
            }),
    );
    for group in sources {
        let indices = discovery_items
            .remove(group.id.as_str())
            .unwrap_or_default();
        if search_active && indices.is_empty() {
            continue;
        }
        let mut key = vec![group.id.clone()];
        let expanded = search_active || expanded_discovery.contains(&key);
        rows.push(SessionManagerTreeRow::DiscoveryGroup {
            group: group.clone(),
            expanded,
            count: indices.len(),
        });
        if expanded && let Some(hint) = &group.hint {
            rows.push(SessionManagerTreeRow::DiscoveryStatus(hint.clone()));
        }
        if expanded {
            let mut root = DiscoveryBranch::default();
            for index in indices {
                let paths = items[index].group_paths();
                if paths.is_empty() {
                    root.items.push(index);
                }
                for path in paths {
                    let mut branch = &mut root;
                    for segment in path {
                        branch =
                            branch
                                .children
                                .entry(&segment.id)
                                .or_insert_with(|| DiscoveryBranch {
                                    label: &segment.label,
                                    ..Default::default()
                                });
                        // All paths of a target are visited together, including shared ancestors.
                        if branch.last_member != Some(index) {
                            branch.count += 1;
                            branch.last_member = Some(index);
                        }
                    }
                    if branch.items.last() != Some(&index) {
                        branch.items.push(index);
                    }
                }
            }
            root.append_rows(&mut rows, &mut key, expanded_discovery, search_active);
        }
    }
    rows
}

#[derive(Default)]
struct DiscoveryBranch<'a> {
    label: &'a str,
    count: usize,
    last_member: Option<usize>,
    children: HashMap<&'a str, DiscoveryBranch<'a>>,
    items: Vec<usize>,
}

impl DiscoveryBranch<'_> {
    fn append_rows(
        self,
        rows: &mut Vec<SessionManagerTreeRow>,
        key: &mut Vec<String>,
        open: &HashSet<Vec<String>>,
        searching: bool,
    ) {
        let mut children: Vec<_> = self.children.into_iter().collect();
        children.sort_unstable_by_key(|(id, child)| (child.label, *id));
        for (id, child) in children {
            key.push(id.to_owned());
            let expanded = searching || open.contains(key);
            rows.push(SessionManagerTreeRow::DiscoverySubgroup {
                key: key.clone(),
                label: child.label.to_owned(),
                count: child.count,
                expanded,
            });
            if expanded {
                child.append_rows(rows, key, open, searching);
            }
            key.pop();
        }
        rows.extend(
            self.items
                .into_iter()
                .map(|item_index| SessionManagerTreeRow::Item {
                    item_index,
                    depth: key.len(),
                }),
        );
    }
}

/// Selectable tree rows in render order, used by Shift+click range selection.
pub(super) fn session_manager_tree_visible_targets<T: CatalogEntry>(
    rows: &[SessionManagerTreeRow],
    items: &[T],
) -> Vec<SessionManagerSelectionTarget> {
    rows.iter()
        .filter_map(|row| match row {
            SessionManagerTreeRow::Item { item_index, .. } => items
                .get(*item_index)
                .and_then(|item| item.selection_target()),
            SessionManagerTreeRow::Group { .. }
            | SessionManagerTreeRow::DiscoveryGroup { .. }
            | SessionManagerTreeRow::DiscoverySubgroup { .. }
            | SessionManagerTreeRow::DiscoveryStatus(_) => None,
        })
        .collect()
}

/// Selectable grid cards (and recent-strip items) in render order.
pub(super) fn session_manager_grid_visible_targets<T: CatalogEntry>(
    rows: &[SessionManagerGridRow],
    items: &[T],
) -> Vec<SessionManagerSelectionTarget> {
    rows.iter()
        .flat_map(|row| match row {
            SessionManagerGridRow::SectionHeader { .. } => Vec::new(),
            SessionManagerGridRow::RecentItems { item_indices, .. }
            | SessionManagerGridRow::Cards { item_indices } => item_indices
                .iter()
                .filter_map(|index| items.get(*index).and_then(|item| item.selection_target()))
                .collect::<Vec<_>>(),
        })
        .collect()
}

fn push_session_manager_tree_group_rows(
    rows: &mut Vec<SessionManagerTreeRow>,
    group: &str,
    depth: usize,
    grouped_items: &HashMap<Option<&str>, Vec<usize>>,
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
    matching_groups: Option<&HashSet<&str>>,
) {
    // A search must not leave an empty group header behind: it would suggest
    // the group still has results while every row inside it is filtered out.
    if matching_groups
        .as_ref()
        .is_some_and(|groups| !groups.contains(group))
    {
        return;
    }
    let group_item_indices = grouped_items
        .get(&Some(group))
        .map(Vec::as_slice)
        .unwrap_or_default();
    let child_groups = children.get(group).map(Vec::as_slice).unwrap_or_default();
    // Matches stay reachable even when the user collapsed their group; the
    // saved disclosure state only applies while no search is active.
    let expanded = matching_groups.is_some() || expanded_groups.contains(group);
    let has_visible_child = child_groups.iter().any(|child| {
        matching_groups
            .as_ref()
            .is_none_or(|groups| groups.contains(child.as_str()))
    });
    rows.push(SessionManagerTreeRow::Group {
        path: group.to_string(),
        depth,
        expanded,
        has_children: !group_item_indices.is_empty() || has_visible_child,
    });
    if !expanded {
        return;
    }

    for child_group in child_groups {
        push_session_manager_tree_group_rows(
            rows,
            child_group,
            depth + 1,
            grouped_items,
            children,
            expanded_groups,
            matching_groups,
        );
    }
    rows.extend(
        group_item_indices
            .iter()
            .copied()
            .map(|item_index| SessionManagerTreeRow::Item {
                item_index,
                depth: depth + 1,
            }),
    );
}

fn recent_session_item_indices<T: CatalogEntry>(items: &[T]) -> Vec<usize> {
    const RECENT_SESSION_LIMIT: usize = 8;
    let mut candidates: Vec<_> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            item.last_used()
                .map(|time| (std::cmp::Reverse(time), index))
        })
        .collect();
    // Only the recent strip needs ordering. The original index breaks ties so
    // partial selection preserves the display order for equal timestamps.
    if candidates.len() > RECENT_SESSION_LIMIT {
        candidates.select_nth_unstable(RECENT_SESSION_LIMIT);
        candidates.truncate(RECENT_SESSION_LIMIT);
    }
    candidates.sort_unstable();
    candidates.into_iter().map(|(_, index)| index).collect()
}

pub(super) fn session_manager_display_item_signature<T: CatalogEntry>(item: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    item.hash_appearance(&mut hasher);
    item.id().hash(&mut hasher);
    item.name().hash(&mut hasher);
    item.group().hash(&mut hasher);
    item.subtitle().hash(&mut hasher);
    item.last_used().hash(&mut hasher);
    item.icon_color().hash(&mut hasher);
    item.icon_background_color().hash(&mut hasher);
    hasher.finish()
}

pub(super) fn session_manager_grid_row_signature<T: CatalogEntry>(
    row: &SessionManagerGridRow,
    items: &[T],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        SessionManagerGridRow::SectionHeader { title, item_count } => {
            0_u8.hash(&mut hasher);
            title.hash(&mut hasher);
            item_count.hash(&mut hasher);
        }
        SessionManagerGridRow::RecentItems {
            item_indices,
            is_last_in_section,
        } => {
            1_u8.hash(&mut hasher);
            is_last_in_section.hash(&mut hasher);
            hash_session_manager_row_items(item_indices, items, &mut hasher);
        }
        SessionManagerGridRow::Cards { item_indices } => {
            2_u8.hash(&mut hasher);
            hash_session_manager_row_items(item_indices, items, &mut hasher);
        }
    }
    hasher.finish()
}

pub(super) fn session_manager_tree_row_signature<T: CatalogEntry>(
    row: &SessionManagerTreeRow,
    items: &[T],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        SessionManagerTreeRow::DiscoverySubgroup {
            key,
            label,
            expanded,
            count,
        } => {
            4_u8.hash(&mut hasher);
            (key, label, expanded, count).hash(&mut hasher);
        }
        SessionManagerTreeRow::DiscoveryStatus(message) => {
            3_u8.hash(&mut hasher);
            message.hash(&mut hasher);
        }
        SessionManagerTreeRow::DiscoveryGroup {
            group,
            expanded,
            count,
        } => {
            2_u8.hash(&mut hasher);
            (group, expanded, count).hash(&mut hasher);
        }
        SessionManagerTreeRow::Group {
            path,
            depth,
            expanded,
            has_children,
        } => {
            0_u8.hash(&mut hasher);
            path.hash(&mut hasher);
            depth.hash(&mut hasher);
            expanded.hash(&mut hasher);
            has_children.hash(&mut hasher);
        }
        SessionManagerTreeRow::Item { item_index, depth } => {
            1_u8.hash(&mut hasher);
            depth.hash(&mut hasher);
            if let Some(item) = items.get(*item_index) {
                session_manager_display_item_signature(item).hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn hash_session_manager_row_items<T: CatalogEntry>(
    item_indices: &[usize],
    items: &[T],
    hasher: &mut DefaultHasher,
) {
    for item_index in item_indices {
        if let Some(item) = items.get(*item_index) {
            session_manager_display_item_signature(item).hash(hasher);
        }
    }
}

pub fn group_display_name(group: &str) -> String {
    group.rsplit('/').next().unwrap_or(group).to_string()
}

pub fn collect_session_group_paths(
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    output: &mut HashSet<String>,
) {
    for root in roots {
        output.insert(root.clone());
        if let Some(child_groups) = children.get(root) {
            collect_session_group_paths(child_groups, children, output);
        }
    }
}
