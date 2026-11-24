use super::*;
use gpui::{Div, StatefulInteractiveElement, point};
use oxideterm_remote_desktop::RemoteDesktopSessionStatus;
use oxideterm_settings::SessionSortOrder;
use oxideterm_terminal::BleConnectionStatus;
use oxideterm_theme::AppUiColors;

fn session_item_surface(theme: AppUiColors, selected: bool, accent_alpha: u32) -> Div {
    if selected {
        div().bg(rgba((theme.accent << 8) | accent_alpha))
    } else {
        div().hover(move |style| style.bg(rgb(theme.bg_hover)))
    }
}

/// Vertical selection marker shared by row surfaces: a 2px accent pill inset
/// from the row edges, drawn only while the row joins the batch selection.
fn session_row_accent_bar(theme: AppUiColors) -> Div {
    div()
        .absolute()
        .left_0()
        .top(px(5.0))
        .bottom(px(5.0))
        .w(px(2.0))
        .rounded_full()
        .bg(rgb(theme.accent))
}

fn active_connection_count(rows: &[ActiveSessionSidebarRow]) -> usize {
    // Count connection owners, including SSH nodes with only SFTP/forwarding consumers.
    rows.iter()
        .filter(|row| {
            matches!(
                row.node_view.status(),
                ActiveSessionStatus::Active | ActiveSessionStatus::Connected
            )
        })
        .map(|row| {
            if row.local_group {
                row.active_local_session_count
            } else {
                1
            }
        })
        .sum()
}

fn sidebar_terminal_post_connect_command(
    saved_connection: Option<&oxideterm_connections::SavedConnection>,
    node_router: &NodeRouter,
    node_id: &NodeId,
) -> Option<String> {
    if let Some(connection) = saved_connection {
        // An explicitly cleared saved command must not fall back to an older runtime value.
        return connection.post_connect_command().map(ToOwned::to_owned);
    }
    // Temporary nodes have no saved profile. Read their zeroizing config only
    // for the explicit open action and move the command into the terminal request.
    node_router
        .node_runtime_snapshot(node_id)
        .and_then(|mut snapshot| snapshot.config.post_connect_command.take())
}

impl standalone_connections::StandaloneConnectionKind {
    fn icon(self) -> LucideIcon {
        match self {
            standalone_connections::StandaloneConnectionKind::Mosh => LucideIcon::Wifi,
            standalone_connections::StandaloneConnectionKind::Telnet => LucideIcon::Terminal,
            standalone_connections::StandaloneConnectionKind::Serial => LucideIcon::Cable,
            standalone_connections::StandaloneConnectionKind::Ble => LucideIcon::Bluetooth,
            standalone_connections::StandaloneConnectionKind::Rdp
            | standalone_connections::StandaloneConnectionKind::Vnc => LucideIcon::Monitor,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct StandaloneActiveSession {
    connection_id: String,
    kind: standalone_connections::StandaloneConnectionKind,
    target: Option<standalone_connections::StandaloneConnectionSurface>,
}

/// A multi-select batch-delete target. Only rows that own a Remove action can
/// join; synthetic host/user group rows and local rows never participate, so
/// batch deletes stay confined to inactive session records.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::workspace) enum ActiveSessionSelectionTarget {
    Node(NodeId),
    Standalone(String),
}

/// How a click combines with the current row selection. Plain clicks select a
/// single row; modifier clicks mirror the Session Manager semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveSessionSelectionMode {
    Single,
    Toggle,
    Range,
}

/// Contiguous removable rows between `anchor` and `target` in render order,
/// both endpoints included, or `None` when either endpoint is not part of the
/// visible list (hidden by a collapsed group or the search filter).
fn active_session_selection_range(
    visible: &[ActiveSessionSelectionTarget],
    anchor: &ActiveSessionSelectionTarget,
    target: &ActiveSessionSelectionTarget,
) -> Option<Vec<ActiveSessionSelectionTarget>> {
    let anchor_index = visible.iter().position(|candidate| candidate == anchor)?;
    let target_index = visible.iter().position(|candidate| candidate == target)?;
    let (start, end) = if anchor_index <= target_index {
        (anchor_index, target_index)
    } else {
        (target_index, anchor_index)
    };
    Some(visible[start..=end].to_vec())
}

/// Rows that can carry the selection rail: session records in any status plus
/// local rows. Synthetic group rows never participate; in Host view a folded
/// single-member user row rides on its member node instead.
fn active_session_row_selection_target(
    row: &ActiveSessionSidebarRow,
) -> Option<ActiveSessionSelectionTarget> {
    if row.group.is_some() {
        return None;
    }
    if let Some(session) = &row.standalone_session {
        return Some(ActiveSessionSelectionTarget::Standalone(
            session.connection_id.clone(),
        ));
    }
    Some(ActiveSessionSelectionTarget::Node(row.node_id.clone()))
}

/// Selection targets a batch delete may actually execute. Local rows are
/// workspaces rather than session records, and live sessions must return to
/// Idle/Error before their record can be removed.
fn active_session_row_removal_target(
    row: &ActiveSessionSidebarRow,
) -> Option<ActiveSessionSelectionTarget> {
    if row.local_group
        || !matches!(
            row.node_view.status(),
            ActiveSessionStatus::Idle | ActiveSessionStatus::Error
        )
    {
        return None;
    }
    active_session_row_selection_target(row)
}

/// Execution order for a batch delete: the selection filtered to rows that are
/// still present and still removable, in render order. A target whose row is
/// gone (removed alongside an earlier subtree sibling, or closed elsewhere) or
/// whose session turned active is skipped instead of deleted.
fn planned_active_session_removals(
    rows: &[ActiveSessionSidebarRow],
    selection: &HashSet<ActiveSessionSelectionTarget>,
) -> Vec<ActiveSessionSelectionTarget> {
    rows.iter()
        .filter_map(|row| {
            active_session_row_removal_target(row).filter(|target| selection.contains(target))
        })
        .collect()
}

/// Whether the whole subtree under `node_id` is safe to drop: every node must
/// be disconnected with no live connection, terminal, SFTP session, reconnect
/// job or connecting lock. Search and grouping can hide descendants that a
/// removal would still affect, so this walks the router instead of the
/// visible rows.
fn session_subtree_is_inactive(
    router: &NodeRouter,
    node_id: &NodeId,
    has_workspace_owner: impl Fn(&NodeId) -> bool,
) -> bool {
    router.subtree_postorder(node_id).into_iter().all(|id| {
        router.node_metadata(&id).is_some_and(|node| {
            matches!(
                node.readiness,
                NodeReadiness::Disconnected | NodeReadiness::Error
            ) && node.connection_id.is_none()
                && node.terminal_session_id.is_none()
                && node.sftp_session_id.is_none()
                && !has_workspace_owner(&id)
        })
    })
}

/// Footer feedback for one batch removal: how many targets were deleted and
/// how many selected targets were not (live rows, local rows, or targets that
/// vanished before execution).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct ActiveSessionRemovalOutcome {
    removed: usize,
    skipped: usize,
}

#[derive(Clone)]
pub(in crate::workspace) struct ActiveSessionSidebarRow {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    saved_connection_id: Option<String>,
    title: String,
    host: String,
    username: String,
    port: u16,
    node_view: ActiveSessionNode,
    depth: usize,
    is_last: bool,
    has_children: bool,
    standalone_session: Option<StandaloneActiveSession>,
    local_group: bool,
    local_profile_id: Option<String>,
    active_local_session_count: usize,
    group: Option<SessionGroupRow>,
}

#[derive(Clone)]
struct SessionGroupRow {
    // Host headers only carry a label; user rows embed their member session
    // rows so option items render without re-querying the node maps.
    user_level: bool,
    label: String,
    /// Saved connection names owned by a user row's members, deduplicated in
    /// member order. SSH node titles mirror the saved name, so grouping can
    /// name the owning connections without reading the connection store.
    connection_name: Option<String>,
    members: Vec<ActiveSessionSidebarRow>,
}

/// Group rows fold several nodes into one line, so the saved connection name is
/// the only place a user row can still say which connection it belongs to.
/// Nodes without a saved connection have no name to show; their title is
/// `user@host` and would just repeat the row label.
fn session_group_connection_name(members: &[ActiveSessionSidebarRow]) -> Option<String> {
    let mut names = Vec::<&str>::new();
    for member in members
        .iter()
        .filter(|member| member.saved_connection_id.is_some())
    {
        if !names.contains(&member.title.as_str()) {
            names.push(member.title.as_str());
        }
    }
    (!names.is_empty()).then(|| names.join(" · "))
}

fn session_endpoint_label(username: &str, host: &str, port: u16) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    if username.is_empty() {
        format!("{host}:{port}")
    } else {
        format!("{username}@{host}:{port}")
    }
}

struct LocalSessionRowState {
    terminal_ids: Vec<TerminalSessionId>,
    search: String,
    readiness: ActiveSessionReadiness,
    active_count: usize,
}

impl Default for LocalSessionRowState {
    fn default() -> Self {
        Self {
            terminal_ids: Vec::new(),
            search: String::new(),
            readiness: ActiveSessionReadiness::Disconnected,
            active_count: 0,
        }
    }
}

fn terminal_lifecycle_readiness(lifecycle: &TerminalLifecycle) -> ActiveSessionReadiness {
    match lifecycle {
        TerminalLifecycle::Running => ActiveSessionReadiness::Ready,
        TerminalLifecycle::Exited(_) => ActiveSessionReadiness::Error,
        TerminalLifecycle::Closed => ActiveSessionReadiness::Disconnected,
    }
}

/// BLE reports a lost link through the worker rather than the pane lifecycle.
/// A dropped peer is a failure the user must act on, so it reads as Error;
/// only closing the tab releases the surface and returns the row to Idle.
fn ble_connection_readiness(status: BleConnectionStatus) -> ActiveSessionReadiness {
    match status {
        BleConnectionStatus::Connecting => ActiveSessionReadiness::Connecting,
        BleConnectionStatus::Connected => ActiveSessionReadiness::Ready,
        BleConnectionStatus::Disconnected => ActiveSessionReadiness::Error,
    }
}

fn remote_desktop_readiness(status: RemoteDesktopSessionStatus) -> ActiveSessionReadiness {
    match status {
        RemoteDesktopSessionStatus::Connected => ActiveSessionReadiness::Ready,
        RemoteDesktopSessionStatus::Connecting | RemoteDesktopSessionStatus::Reconnecting => {
            ActiveSessionReadiness::Connecting
        }
        RemoteDesktopSessionStatus::Failed => ActiveSessionReadiness::Error,
        RemoteDesktopSessionStatus::Idle | RemoteDesktopSessionStatus::Disconnected => {
            ActiveSessionReadiness::Disconnected
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionNodeRowAction {
    Connect,
    Reconnect,
    Disconnect,
    CancelReconnect,
    Remove,
}

fn session_node_row_actions(
    status: ActiveSessionStatus,
    reconnecting: bool,
) -> [Option<SessionNodeRowAction>; 2] {
    use SessionNodeRowAction::*;
    if reconnecting {
        return [Some(CancelReconnect), None];
    }
    match status {
        ActiveSessionStatus::Active | ActiveSessionStatus::Connected => [Some(Disconnect), None],
        ActiveSessionStatus::Error => [Some(Reconnect), Some(Remove)],
        ActiveSessionStatus::Idle => [Some(Connect), Some(Remove)],
        ActiveSessionStatus::Connecting => [None, None],
    }
}

fn standalone_session_actions(status: ActiveSessionStatus) -> [Option<SessionNodeRowAction>; 2] {
    use SessionNodeRowAction::*;
    match status {
        ActiveSessionStatus::Active
        | ActiveSessionStatus::Connected
        | ActiveSessionStatus::Connecting => [Some(Disconnect), None],
        ActiveSessionStatus::Error | ActiveSessionStatus::Idle => [Some(Reconnect), Some(Remove)],
    }
}

const SESSION_SORT_OPTIONS: [(SessionSortOrder, &str); 4] = [
    (SessionSortOrder::Default, "sidebar.sort.default"),
    (
        SessionSortOrder::NameAscending,
        "sidebar.sort.name_ascending",
    ),
    (
        SessionSortOrder::NameDescending,
        "sidebar.sort.name_descending",
    ),
    (
        SessionSortOrder::ConnectedFirst,
        "sidebar.sort.connected_first",
    ),
];

fn sort_active_session_rows(
    rows: Vec<ActiveSessionSidebarRow>,
    order: SessionSortOrder,
    manual_order: &[String],
) -> Vec<ActiveSessionSidebarRow> {
    if order == SessionSortOrder::Default && manual_order.is_empty() {
        return rows;
    }
    let known: HashSet<_> = rows.iter().map(|row| row.node_id.clone()).collect();
    let names: Vec<_> = rows.iter().map(|row| row.title.to_lowercase()).collect();
    let mut siblings: HashMap<Option<NodeId>, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let parent = row
            .parent_id
            .as_ref()
            .filter(|id| known.contains(id))
            .cloned();
        siblings.entry(parent).or_default().push(index);
    }
    let readiness = |row: &ActiveSessionSidebarRow| match row.node_view.readiness {
        ActiveSessionReadiness::Ready => 0,
        ActiveSessionReadiness::Connecting => 1,
        ActiveSessionReadiness::Error => 2,
        ActiveSessionReadiness::Disconnected => 3,
    };
    let ranks: HashMap<_, _> = manual_order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for group in siblings.values_mut() {
        group.sort_by(|a, b| match order {
            SessionSortOrder::NameAscending => names[*a].cmp(&names[*b]),
            SessionSortOrder::NameDescending => names[*b].cmp(&names[*a]),
            SessionSortOrder::ConnectedFirst => readiness(&rows[*a])
                .cmp(&readiness(&rows[*b]))
                .then_with(|| names[*a].cmp(&names[*b])),
            SessionSortOrder::Default => ranks
                .get(rows[*a].node_id.0.as_str())
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(
                    &ranks
                        .get(rows[*b].node_id.0.as_str())
                        .copied()
                        .unwrap_or(usize::MAX),
                ),
        });
    }
    let roots = siblings.remove(&None).unwrap_or_default();
    let mut pending: Vec<_> = roots
        .iter()
        .enumerate()
        .rev()
        .map(|(position, index)| (*index, 0, position + 1 == roots.len()))
        .collect();
    let mut output = Vec::with_capacity(rows.len());
    let mut rows: Vec<_> = rows.into_iter().map(Some).collect();
    // Walk sorted sibling groups in preorder so descendants always follow their parent.
    while let Some((index, depth, is_last)) = pending.pop() {
        let mut row = rows[index].take().unwrap();
        row.depth = depth;
        row.is_last = is_last;
        if let Some(children) = siblings.remove(&Some(row.node_id.clone())) {
            pending.extend(
                children
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(position, index)| (*index, depth + 1, position + 1 == children.len())),
            );
        }
        output.push(row);
    }
    output
}

fn filter_active_session_rows(
    mut rows: Vec<ActiveSessionSidebarRow>,
    query: &str,
) -> Vec<ActiveSessionSidebarRow> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return rows;
    }
    let parents: HashMap<_, _> = rows
        .iter()
        .map(|row| (row.node_id.clone(), row.parent_id.clone()))
        .collect();
    let mut retained = HashSet::new();
    for row in &rows {
        let protocol = if row.local_group {
            "local".to_string()
        } else {
            row.standalone_session
                .as_ref()
                .map(|session| format!("{:?}", session.kind))
                .unwrap_or_else(|| "ssh".into())
        };
        let text = format!(
            "{} {} {} {} {}",
            row.title, row.host, row.username, row.port, protocol
        )
        .to_lowercase();
        if query.split_whitespace().all(|term| text.contains(term)) {
            let mut next = Some(row.node_id.clone());
            while let Some(id) = next {
                if !retained.insert(id.clone()) {
                    break;
                }
                next = parents.get(&id).cloned().flatten();
            }
        }
    }
    rows.retain(|row| retained.contains(&row.node_id));
    let last_children: HashMap<_, _> = rows
        .iter()
        .map(|row| (row.parent_id.clone(), row.node_id.clone()))
        .collect();
    for row in &mut rows {
        row.is_last = last_children.get(&row.parent_id) == Some(&row.node_id);
        row.has_children = last_children.contains_key(&Some(row.node_id.clone()));
    }
    rows
}

const SESSION_HOST_GROUP_ID_PREFIX: &str = "session-host-group:";
/// Separator between host and user keys inside a synthetic user row id. A
/// control character cannot appear in a DNS name or username, so ids stay
/// collision-free against real node ids.
const SESSION_HOST_GROUP_ID_SEPARATOR: char = '\u{1}';
/// Placeholder label for SSH nodes without a username. Punctuation keeps it out
/// of the locale catalogs.
const SESSION_HOST_GROUP_UNKNOWN_USER: &str = "—";

struct SessionUserGroup {
    key: String,
    label: String,
    members: Vec<ActiveSessionSidebarRow>,
}

struct SessionHostGroup {
    key: String,
    label: String,
    users: Vec<SessionUserGroup>,
    user_indices: HashMap<String, usize>,
}

fn readiness_rank(readiness: &ActiveSessionReadiness) -> u8 {
    match readiness {
        ActiveSessionReadiness::Ready => 0,
        ActiveSessionReadiness::Connecting => 1,
        ActiveSessionReadiness::Error => 2,
        ActiveSessionReadiness::Disconnected => 3,
    }
}

fn readiness_from_rank(rank: u8) -> ActiveSessionReadiness {
    match rank {
        0 => ActiveSessionReadiness::Ready,
        1 => ActiveSessionReadiness::Connecting,
        2 => ActiveSessionReadiness::Error,
        _ => ActiveSessionReadiness::Disconnected,
    }
}

fn aggregate_group_readiness<'a>(
    members: impl Iterator<Item = &'a ActiveSessionSidebarRow>,
) -> ActiveSessionReadiness {
    // Only ConnectedFirst ordering reads this value; the group rows never draw
    // a status dot. Report the healthiest member so a group hides nothing.
    members
        .map(|row| readiness_rank(&row.node_view.readiness))
        .min()
        .map(readiness_from_rank)
        .unwrap_or(ActiveSessionReadiness::Disconnected)
}

fn host_group_row_id(host_key: &str) -> NodeId {
    NodeId::new(format!("{SESSION_HOST_GROUP_ID_PREFIX}{host_key}"))
}

fn user_group_row_id(host_key: &str, user_key: &str) -> NodeId {
    NodeId::new(format!(
        "{SESSION_HOST_GROUP_ID_PREFIX}{host_key}{SESSION_HOST_GROUP_ID_SEPARATOR}{user_key}"
    ))
}

/// Synthetic ids must never collide with a real node id: restored session-tree
/// snapshots are unvalidated strings, and a duplicate id would let the collapsed
/// host filter hide rows that belong to a real node.
fn unique_session_group_row_id(base: NodeId, taken: &mut HashSet<String>) -> NodeId {
    if taken.insert(base.0.clone()) {
        return base;
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{}#{}", base.0, suffix);
        if taken.insert(candidate.clone()) {
            return NodeId::new(candidate);
        }
        suffix += 1;
    }
}

/// Groups rows and applies the configured ordering to the groups. Manual order
/// only ranks real node ids, so the default mode must not re-sort grouped rows:
/// doing so would rank every synthetic id last and push the draggable
/// local/standalone rows above all host groups.
fn arrange_active_session_group_rows(
    rows: Vec<ActiveSessionSidebarRow>,
    sort_order: SessionSortOrder,
    manual_order: &[String],
) -> Vec<ActiveSessionSidebarRow> {
    let grouped = group_active_session_rows_by_host_user(rows);
    if sort_order == SessionSortOrder::Default {
        // `manual_order` is deliberately ignored: it ranks real node ids, and a
        // synthetic group id would rank last, pushing ranked pass-through rows
        // ahead of every group.
        return grouped;
    }
    sort_active_session_rows(grouped, sort_order, manual_order)
}

fn group_active_session_rows_by_host_user(
    rows: Vec<ActiveSessionSidebarRow>,
) -> Vec<ActiveSessionSidebarRow> {
    let mut taken_ids = rows
        .iter()
        .map(|row| row.node_id.0.clone())
        .collect::<HashSet<_>>();
    let mut host_indices = HashMap::<String, usize>::new();
    let mut hosts = Vec::<SessionHostGroup>::new();
    let mut passthrough = Vec::new();

    for row in rows {
        let host = row.host.trim();
        let is_ssh_node = !row.local_group && row.standalone_session.is_none();
        if !is_ssh_node || host.is_empty() {
            // Local groups, local profiles, standalone connections, and jump
            // hosts without a known endpoint keep their original row shape.
            passthrough.push(row);
            continue;
        }
        let host_key = host.to_lowercase();
        let host_index = match host_indices.get(&host_key) {
            Some(index) => *index,
            None => {
                hosts.push(SessionHostGroup {
                    key: host_key.clone(),
                    // First spelling seen wins so the header matches the label
                    // the user typed for that connection.
                    label: host.to_string(),
                    users: Vec::new(),
                    user_indices: HashMap::new(),
                });
                let index = hosts.len() - 1;
                host_indices.insert(host_key, index);
                index
            }
        };
        let host_group = &mut hosts[host_index];
        let user_key = row.username.clone();
        let user_index = match host_group.user_indices.get(&user_key) {
            Some(index) => *index,
            None => {
                host_group.users.push(SessionUserGroup {
                    key: user_key.clone(),
                    label: if user_key.is_empty() {
                        SESSION_HOST_GROUP_UNKNOWN_USER.to_string()
                    } else {
                        user_key.clone()
                    },
                    members: Vec::new(),
                });
                let index = host_group.users.len() - 1;
                host_group.user_indices.insert(user_key, index);
                index
            }
        };
        host_group.users[user_index].members.push(row);
    }

    let root_count = hosts.len() + passthrough.len();
    let host_count = hosts.len();
    let mut output = Vec::with_capacity(hosts.len() + root_count);
    for (host_position, host) in hosts.into_iter().enumerate() {
        let host_id = unique_session_group_row_id(host_group_row_id(&host.key), &mut taken_ids);
        let host_label = host.label.clone();
        let host_readiness =
            aggregate_group_readiness(host.users.iter().flat_map(|user| user.members.iter()));
        let user_count = host.users.len();
        let mut user_rows = Vec::with_capacity(user_count);
        for (user_position, user) in host.users.into_iter().enumerate() {
            let user_id = unique_session_group_row_id(
                user_group_row_id(&host.key, &user.key),
                &mut taken_ids,
            );
            let user_readiness = aggregate_group_readiness(user.members.iter());
            let user_connection_name = session_group_connection_name(&user.members);
            user_rows.push(ActiveSessionSidebarRow {
                node_id: user_id.clone(),
                parent_id: Some(host_id.clone()),
                saved_connection_id: None,
                title: user.label.clone(),
                host: host_label.clone(),
                username: user.key,
                port: 0,
                node_view: ActiveSessionNode {
                    id: user_id.0,
                    title: user.label.clone(),
                    port: 0,
                    terminal_ids: Vec::new(),
                    readiness: user_readiness,
                },
                depth: 1,
                is_last: user_position + 1 == user_count,
                has_children: false,
                local_group: false,
                local_profile_id: None,
                active_local_session_count: 0,
                standalone_session: None,
                group: Some(SessionGroupRow {
                    user_level: true,
                    label: user.label,
                    connection_name: user_connection_name,
                    members: user.members,
                }),
            });
        }
        output.push(ActiveSessionSidebarRow {
            node_id: host_id.clone(),
            parent_id: None,
            saved_connection_id: None,
            title: host_label.clone(),
            host: host_label.clone(),
            username: String::new(),
            port: 0,
            node_view: ActiveSessionNode {
                id: host_id.0,
                title: host_label.clone(),
                port: 0,
                terminal_ids: Vec::new(),
                readiness: host_readiness,
            },
            depth: 0,
            // Depth and last-child flags are set here because the default sort
            // order early-returns without recomputing them.
            is_last: host_position + 1 == root_count,
            has_children: user_count > 0,
            local_group: false,
            local_profile_id: None,
            active_local_session_count: 0,
            standalone_session: None,
            group: Some(SessionGroupRow {
                user_level: false,
                label: host_label,
                connection_name: None,
                members: Vec::new(),
            }),
        });
        output.extend(user_rows);
    }
    for (position, mut row) in passthrough.into_iter().enumerate() {
        row.parent_id = None;
        row.depth = 0;
        row.has_children = false;
        row.is_last = host_count + position + 1 == root_count;
        output.push(row);
    }
    output
}

/// Computes which outer guide-rail columns (`0..row.depth - 1`) have following
/// siblings below the row at `target_index`.
fn session_row_ancestor_rails(rows: &[ActiveSessionSidebarRow], target_index: usize) -> Vec<bool> {
    // Index `d` tracks whether the active ancestor at depth `d` continues below
    // its subtree (`!is_last`). Index 0 is unused because roots have no outer rail.
    let mut continues_at_depth: Vec<bool> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        continues_at_depth.resize(row.depth, false);
        if index == target_index {
            return if row.depth > 1 {
                continues_at_depth[1..row.depth].to_vec()
            } else {
                Vec::new()
            };
        }
        continues_at_depth.push(row.depth > 0 && !row.is_last);
    }
    Vec::new()
}

struct SessionActionSpec {
    debug_key: &'static str,
    icon: LucideIcon,
    label: String,
    variant: SessionActionVariant,
    listener: Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_session_search_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.session_search_open;
        let theme = self.tokens.ui;
        self.workspace_tooltip_icon_button(
            LucideIcon::Search,
            self.tokens.metrics.sidebar_action_icon_size,
            rgb(if open { theme.text } else { theme.text_muted }),
            IconButtonOptions {
                has_background: open,
                background: open.then_some(rgb(theme.bg_active)),
                hover_background: Some(rgb(theme.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    self.tokens.metrics.sidebar_action_size,
                    ButtonRadius::Md,
                )
            },
            self.i18n.t("sidebar.search.title"),
            "session-search",
            false,
            cx.listener(|this, _, window, cx| {
                this.prepare_modal_interaction_boundary(cx);
                this.session_search_open = !this.session_search_open;
                this.begin_disclosure_motion("session-search".into(), this.session_search_open, cx);
                this.clear_ime_selection();
                if this.session_search_open {
                    this.selected_ime_target = Some(ime::WorkspaceImeTarget::ActiveSessionSearch);
                    window.focus(&this.focus_handle, cx);
                    this.show_active_input_caret(cx);
                } else {
                    this.session_search_query.clear();
                }
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_session_search_input(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sidebar_search_row(self.workspace_sidebar_background(self.tokens.ui.shell_bg()))
            .child(self.render_overlay_query_input(
                ime::WorkspaceImeTarget::ActiveSessionSearch,
                self.session_search_query.clone(),
                self.i18n.t("sidebar.search.placeholder"),
                self.tokens.metrics.sidebar_title_font_size,
                20.0,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_sort_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.session_sort_menu_open;
        let theme = self.tokens.ui;
        let button = self.workspace_tooltip_icon_button(
            LucideIcon::ArrowDownAZ,
            self.tokens.metrics.sidebar_action_icon_size,
            rgb(if open { theme.text } else { theme.text_muted }),
            IconButtonOptions {
                has_background: open,
                background: open.then_some(rgb(theme.bg_active)),
                hover_background: Some(rgb(theme.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    self.tokens.metrics.sidebar_action_size,
                    ButtonRadius::Md,
                )
            },
            self.i18n.t("sidebar.sort.title"),
            "session-sort",
            false,
            cx.listener(|this, _, window, cx| {
                let open = !this.session_sort_menu_open;
                this.prepare_modal_interaction_boundary(cx);
                this.session_sort_menu_open = open;
                window.focus(&this.focus_handle, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        );
        let workspace = cx.entity();
        div()
            .ml(px(2.0))
            .child(oxideterm_gpui_ui::select::select_anchor_probe(
                SelectAnchorId::ActiveSessionSort,
                button,
                move |anchor, window, cx| {
                    window.defer(cx, move |_, cx| {
                        workspace.update(cx, |this, cx| this.update_select_anchor(anchor, cx));
                    })
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_sort_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.session_sort_menu_open
            || self.rails.left.collapsed()
            || self.effective_sidebar_panel_section() != SidebarSection::Sessions
        {
            return None;
        }
        let anchor = self
            .select_anchors
            .get(&SelectAnchorId::ActiveSessionSort)?;
        let selected = self.settings_store.settings().sidebar_ui.session_sort_order;
        let mut popup = oxideterm_gpui_ui::select::select_overlay_popup(&self.tokens, 220.0);
        for (order, label) in SESSION_SORT_OPTIONS {
            popup = popup.child(oxideterm_gpui_ui::select::select_option_action(
                oxideterm_gpui_ui::select::select_option(
                    &self.tokens,
                    self.i18n.t(label),
                    order == selected,
                ),
                false,
                false,
                cx.listener(move |this, _, _, cx| {
                    this.settings_store
                        .settings_mut()
                        .sidebar_ui
                        .session_sort_order = order;
                    this.session_sort_menu_open = false;
                    this.persist_sidebar_settings(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.session_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.session_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(Corner::TopRight)
                            .position(anchor.bounds.bottom_right())
                            .offset(point(px(0.0), px(4.0)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                                &self.tokens,
                                "session-sort-menu-motion",
                                div().child(popup),
                                -6.0,
                                point(0.5, 0.0),
                            )),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    /// Selectable rows in the current render order. Host view folds SSH nodes
    /// into synthetic user rows, so their member nodes are expanded in render
    /// order to keep Shift-range selection aligned with the visible labels.
    fn active_session_visible_selection_targets(
        &self,
        cx: &App,
    ) -> Vec<ActiveSessionSelectionTarget> {
        let mut targets = Vec::new();
        for row in self.active_session_sidebar_display_rows(cx) {
            if let Some(group) = row.group.as_ref().filter(|group| group.user_level) {
                // Member labels render only while the group is expanded, so a
                // collapsed multi-member group must not feed hidden rows into
                // Shift ranges. A folded single-member row is still the
                // member's visible surface (the header mirrors it) and stays
                // in the target order.
                if group.members.len() == 1 || self.session_group_row_expanded(&row) {
                    for member in &group.members {
                        targets.push(ActiveSessionSelectionTarget::Node(member.node_id.clone()));
                    }
                }
                continue;
            }
            if let Some(target) = active_session_row_selection_target(&row) {
                targets.push(target);
            }
        }
        targets
    }

    fn apply_active_session_selection(
        &mut self,
        target: ActiveSessionSelectionTarget,
        mode: ActiveSessionSelectionMode,
        cx: &mut Context<Self>,
    ) {
        self.active_session_selection_summary = None;
        match mode {
            ActiveSessionSelectionMode::Single => {
                self.active_session_selection.clear();
                self.active_session_selection.insert(target.clone());
                self.active_session_selection_anchor = Some(target);
            }
            ActiveSessionSelectionMode::Toggle => {
                if !self.active_session_selection.remove(&target) {
                    self.active_session_selection.insert(target.clone());
                }
                self.active_session_selection_anchor = Some(target);
            }
            ActiveSessionSelectionMode::Range => {
                let visible = self.active_session_visible_selection_targets(cx);
                let range = self
                    .active_session_selection_anchor
                    .as_ref()
                    .and_then(|anchor| active_session_selection_range(&visible, anchor, &target))
                    .unwrap_or_else(|| vec![target.clone()]);
                self.active_session_selection.clear();
                self.active_session_selection.extend(range);
                self.active_session_selection_anchor = Some(target);
            }
        }
        cx.notify();
    }

    /// Node ownership of the currently visible content tab (SFTP/IDE/forwards),
    /// mirroring the lookups in `sync_active_tab_surface`.
    fn active_content_tab_ssh_node_id(&self, cx: &App) -> Option<NodeId> {
        let Some(active_tab_id) = self.active_content_tab_id(cx) else {
            return None;
        };
        let Some(tab) = self.active_content_tab(cx) else {
            return None;
        };
        match &tab.kind {
            oxideterm_workspace::TabKind::Sftp => self.sftp_tab_nodes.get(&active_tab_id).cloned(),
            oxideterm_workspace::TabKind::Ide => self
                .ide_workspace
                .read(cx)
                .node_for_tab(active_tab_id)
                .cloned(),
            oxideterm_workspace::TabKind::Forwards => {
                self.forwarding.read(cx).node_for_tab(active_tab_id)
            }
            _ => None,
        }
    }

    /// Tool-row highlight of the currently visible content tab: the action-row
    /// debug_key plus its owning node, so the matching tool row inside the
    /// node's tree shows the accent state. Terminal child rows keep their own
    /// highlight via the active terminal session.
    fn active_content_tab_action_key(&self, cx: &App) -> (&'static str, Option<NodeId>) {
        let Some(tab) = self.active_content_tab(cx) else {
            return ("", None);
        };
        let key = match &tab.kind {
            oxideterm_workspace::TabKind::Sftp => "sftp",
            oxideterm_workspace::TabKind::Ide => "ide",
            oxideterm_workspace::TabKind::Forwards => "port-forwarding",
            _ => return ("", None),
        };
        (key, self.active_content_tab_ssh_node_id(cx))
    }

    fn clear_active_session_selection(&mut self, cx: &mut Context<Self>) {
        if self.active_session_selection.is_empty()
            && self.active_session_selection_anchor.is_none()
            && self.active_session_selection_summary.is_none()
        {
            return;
        }
        self.active_session_selection.clear();
        self.active_session_selection_anchor = None;
        self.active_session_selection_summary = None;
        cx.notify();
    }

    /// Drops selection targets and the range anchor whose session rows no
    /// longer exist. Selection lives in the sidebar, but nodes can be removed
    /// from other surfaces (Session Manager edits, remote cleanup), and dead
    /// targets would otherwise inflate the footer's selected count forever.
    pub(in crate::workspace) fn prune_active_session_selection(&mut self, cx: &mut Context<Self>) {
        if self.active_session_selection.is_empty()
            && self.active_session_selection_anchor.is_none()
        {
            return;
        }
        // Unfiltered row list, not the search-filtered display rows: hiding a
        // row behind the search box must not deselect it. Flat rows carry no
        // synthetic group rows, so every selectable member appears as its own
        // row here.
        let mut live = HashSet::new();
        for row in self.unfiltered_active_session_sidebar_rows(cx) {
            if let Some(target) = active_session_row_selection_target(&row) {
                live.insert(target);
            }
        }
        let selected_before = self.active_session_selection.len();
        self.active_session_selection
            .retain(|target| live.contains(target));
        let anchor_gone = self
            .active_session_selection_anchor
            .as_ref()
            .is_some_and(|anchor| !live.contains(anchor));
        if anchor_gone {
            self.active_session_selection_anchor = None;
        }
        if self.active_session_selection.len() != selected_before || anchor_gone {
            cx.notify();
        }
    }

    /// A batch removal must confirm the entire subtree is quiet before
    /// dropping it: an idle-looking parent may still own descendants that
    /// hold SFTP sessions, terminals or reconnect jobs, and the removal
    /// would tear those down with the subtree.
    fn can_remove_inactive_session_subtree(&self, node_id: &NodeId, cx: &App) -> bool {
        let runtime = self.workspace_runtime.read(cx);
        let dedicated_sftp = self.dedicated_sftp_connections.lock();
        session_subtree_is_inactive(&self.node_router, node_id, |id| {
            runtime.connecting_node_locks().contains(id)
                || runtime.has_active_reconnect_job(id)
                || self
                    .ssh_nodes
                    .get(id)
                    .is_some_and(|node| !node.terminal_ids.is_empty())
                || dedicated_sftp.get(id).is_some_and(|route| {
                    // A live dedicated route (or one currently locked by an
                    // in-flight acquisition) marks the node as workspace-owned;
                    // treat an unreadable slot conservatively as owned.
                    route
                        .slot
                        .upgrade()
                        .is_some_and(|slot| slot.try_lock().map_or(true, |lease| lease.is_some()))
                })
        })
    }

    fn remove_selected_active_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.active_session_sidebar_rows(cx);
        let planned = planned_active_session_removals(&rows, &self.active_session_selection);
        let selected = self.active_session_selection.len();
        let mut removed = 0usize;
        // Nodes deleted together with an earlier selected ancestor are already
        // gone from the user's view, so they must not count as skipped.
        let mut covered = HashSet::new();
        for target in planned {
            match target {
                ActiveSessionSelectionTarget::Node(node_id) => {
                    // A subtree removal already cleared descendant rows, so a
                    // stale selected child must not re-enter cleanup.
                    if !self.ssh_nodes.contains_key(&node_id) {
                        continue;
                    }
                    // An inactive-looking root must not drop descendants that
                    // still own live consumers; the target reports as skipped.
                    if !self.can_remove_inactive_session_subtree(&node_id, cx) {
                        continue;
                    }
                    for descendant in self.node_router.subtree_postorder(&node_id) {
                        // Postorder ends with the root itself; it is counted as
                        // removed and must not double as covered.
                        if descendant != node_id {
                            covered.insert(ActiveSessionSelectionTarget::Node(descendant));
                        }
                    }
                    self.remove_inactive_session_tree_node(&node_id, window, cx);
                    removed += 1;
                }
                ActiveSessionSelectionTarget::Standalone(connection_id) => {
                    if self.standalone_connections.record(&connection_id).is_none() {
                        continue;
                    }
                    self.remove_standalone_connection(&connection_id, window, cx);
                    removed += 1;
                }
            }
        }
        let covered_selected = covered
            .iter()
            .filter(|target| self.active_session_selection.contains(target))
            .count();
        let skipped = selected.saturating_sub(removed + covered_selected);
        self.active_session_selection.clear();
        self.active_session_selection_anchor = None;
        self.active_session_context_menu = None;
        if removed > 0 || skipped > 0 {
            self.active_session_selection_summary =
                Some(ActiveSessionRemovalOutcome { removed, skipped });
        }
        cx.notify();
    }

    /// Right-click menu over the sessions list while a multi-select batch is
    /// active; anchored at the pointer like the other workspace context menus.
    pub(in crate::workspace) fn render_active_session_selection_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (pointer_x, pointer_y) = self.active_session_context_menu?;
        if self.active_session_selection.is_empty()
            || self.rails.left.collapsed()
            || self.effective_sidebar_panel_section() != SidebarSection::Sessions
        {
            return None;
        }
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            pointer_x,
            pointer_y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            ACTIVE_SESSION_SELECTION_MENU_WIDTH,
            ACTIVE_SESSION_SELECTION_MENU_HEIGHT,
            self.tokens.spacing.two,
        );
        let mut popup = oxideterm_gpui_ui::select::select_overlay_popup(
            &self.tokens,
            ACTIVE_SESSION_SELECTION_MENU_WIDTH,
        );
        popup = popup.child(oxideterm_gpui_ui::select::select_option_action(
            oxideterm_gpui_ui::select::select_option(
                &self.tokens,
                self.i18n.t("sessions.tree.actions.remove_selected"),
                false,
            ),
            false,
            false,
            cx.listener(move |this, _, window, cx| {
                this.remove_selected_active_sessions(window, cx);
            }),
        ));
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.active_session_context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.active_session_context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(Corner::TopLeft)
                            .position(point(px(placement.x), px(placement.y)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                                &self.tokens,
                                "session-selection-menu-motion",
                                div().child(popup),
                                -6.0,
                                point(0.5, 0.0),
                            )),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    /// Opens the selection context menu; a no-op while nothing is selected.
    fn open_active_session_selection_menu_at(&mut self, position: gpui::Point<gpui::Pixels>) {
        self.session_sort_menu_open = false;
        self.active_session_context_menu = Some((f32::from(position.x), f32::from(position.y)));
    }

    /// Right-click on a session row: opens the batch selection menu only while
    /// a selection exists; with nothing selected the event falls through
    /// because this list has no per-row context menu of its own.
    fn open_active_session_selection_menu_if_selected(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.active_session_selection.is_empty() {
            return;
        }
        self.open_active_session_selection_menu_at(position);
        cx.stop_propagation();
        cx.notify();
    }

    pub(in crate::workspace) fn session_tree_node_height(&self) -> f32 {
        (28.0_f32).max((self.tokens.metrics.ui_text_xs + 16.0).round())
    }

    fn session_tree_item_height(&self) -> f32 {
        (24.0_f32).max((self.tokens.metrics.ui_text_xs + 12.0).round())
    }

    fn local_profile_session_icon(
        &self,
        local_profile_id: Option<&str>,
    ) -> super::super::session_icons::SessionIcon {
        local_profile_id
            .and_then(|id| {
                self.connection_store
                    .local_terminal_profiles()
                    .iter()
                    .find(|profile| profile.id == id)
            })
            .and_then(|profile| {
                super::super::session_icons::session_icon_from_id(profile.icon.as_deref())
            })
            .unwrap_or(LucideIcon::Terminal.into())
    }

    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_sidebar_node(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.queue_ssh_terminal_tab_for_sidebar_node_with_sftp(node_id, false, window, cx)
    }

    /// Ctrl-clicking "New Terminal" forces the same SFTP-alongside-terminal
    /// behavior as opening a connection with the per-connection option.
    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_sidebar_node_with_sftp(
        &mut self,
        node_id: NodeId,
        open_sftp_on_connect_override: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .ok_or_else(|| anyhow::anyhow!("SSH node {} not found", node_id.0))?;
        let saved_connection_id = self
            .ssh_nodes
            .get(&node_id)
            .and_then(|node| node.saved_connection_id.clone());
        let post_connect_command = sidebar_terminal_post_connect_command(
            saved_connection_id
                .as_deref()
                .and_then(|id| self.connection_store.get(id)),
            &self.node_router,
            &node_id,
        );
        if self.node_is_ready_for_terminal(&node_id) {
            let open_sftp_on_connect = open_sftp_on_connect_override
                || saved_connection_id
                    .as_deref()
                    .and_then(|id| self.connection_store.get(id))
                    .is_some_and(|connection| connection.options.open_sftp_on_connect);
            return self.queue_ssh_terminal_tab_for_existing_node(
                node_id,
                post_connect_command,
                title,
                open_sftp_on_connect,
                window,
                cx,
            );
        }

        let config = self
            .node_router
            .node_runtime_snapshot(&node_id)
            .map(|snapshot| snapshot.config)
            .ok_or_else(|| anyhow::anyhow!("SSH node {} has no runtime config", node_id.0))?;
        // Keep secret-bearing config out of virtual rows and retained listeners.
        // A disconnected node copies it only at the explicit connect action.
        self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            post_connect_command,
            config,
            title,
            saved_connection_id,
            None,
            None,
            open_sftp_on_connect_override,
            window,
            cx,
        )
    }

    pub(in crate::workspace) fn render_active_sessions_sidebar_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Node churn can outlive the groups that owned a disclosure entry.
        self.prune_session_group_expanded();
        self.prune_active_session_selection(cx);
        let rows = self.active_session_sidebar_display_rows(cx);
        if rows.is_empty() {
            if !self.session_search_query.trim().is_empty() {
                return div()
                    .p_4()
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("sidebar.search.empty"))
                    .into_any_element();
            }
            return self.render_empty_sessions_sidebar_content(cx);
        }

        let list_mode = self.active_session_sidebar_view_mode;
        self.sync_active_session_sidebar_list_state(&rows, list_mode, cx);
        let state = self.active_session_sidebar_list_state.clone();
        let spec = self.active_session_sidebar_list_spec();
        let workspace = cx.entity();
        div()
            .id("active-sessions-sidebar-scroll")
            // Clicking blank list space dismisses the row selection; every
            // interactive row stops the click from reaching this container.
            .on_click(cx.listener(|this, _, _, cx| {
                this.clear_active_session_selection(cx);
            }))
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_active_session_sidebar_list_item(index, cx)
                    })
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn active_session_sidebar_rows(
        &self,
        cx: &App,
    ) -> Vec<ActiveSessionSidebarRow> {
        filter_active_session_rows(
            sort_active_session_rows(
                self.unfiltered_active_session_sidebar_rows(cx),
                self.settings_store.settings().sidebar_ui.session_sort_order,
                &self
                    .settings_store
                    .settings()
                    .sidebar_ui
                    .session_manual_order,
            ),
            &self.session_search_query,
        )
    }

    fn active_session_sidebar_display_rows(&self, cx: &App) -> Vec<ActiveSessionSidebarRow> {
        let rows = self.active_session_sidebar_rows(cx);
        if self.active_session_sidebar_view_mode != ActiveSessionSidebarViewMode::Host {
            return rows;
        }
        // Host view folds real SSH nodes into synthetic host/user rows; every
        // other consumer keeps the ungrouped list so counts and drag order stay
        // tied to real nodes.
        let mut rows = arrange_active_session_group_rows(
            rows,
            self.settings_store.settings().sidebar_ui.session_sort_order,
            &self
                .settings_store
                .settings()
                .sidebar_ui
                .session_manual_order,
        );
        // User rows are their own virtual rows, so collapsing a host drops them
        // from the list instead of animating height inside the header.
        let collapsed_hosts = rows
            .iter()
            .filter(|row| row.parent_id.is_none() && !self.session_group_row_expanded(row))
            .map(|row| row.node_id.clone())
            .collect::<HashSet<_>>();
        if !collapsed_hosts.is_empty() {
            rows.retain(|row| match row.parent_id.as_ref() {
                Some(parent) => !collapsed_hosts.contains(parent),
                None => true,
            });
        }
        rows
    }

    fn unfiltered_active_session_sidebar_rows(&self, cx: &App) -> Vec<ActiveSessionSidebarRow> {
        let mut tree_nodes = self.node_router.flatten_tree();
        let flat_node_child_counts = tree_nodes
            .iter()
            .filter_map(|node| node.parent_id.as_ref())
            .fold(HashMap::<String, usize>::new(), |mut counts, parent_id| {
                *counts.entry(parent_id.clone()).or_default() += 1;
                counts
            });

        let mut rows = tree_nodes
            .drain(..)
            .filter_map(|flat_node| {
                let flat_node_id = flat_node.id.clone();
                let node_id = NodeId::new(flat_node_id.clone());
                let node = self.ssh_nodes.get(&node_id)?.clone();
                let node_view = ActiveSessionNode {
                    id: flat_node_id.clone(),
                    title: node.title.clone(),
                    port: flat_node.port,
                    terminal_ids: node.terminal_ids.clone(),
                    readiness: active_session_readiness(&node.readiness),
                };
                Some(ActiveSessionSidebarRow {
                    node_id,
                    parent_id: flat_node.parent_id.map(NodeId::new),
                    saved_connection_id: node.saved_connection_id.clone(),
                    title: node.title.clone(),
                    host: node.endpoint.host.clone(),
                    username: node.endpoint.username.clone(),
                    port: node.endpoint.port,
                    node_view,
                    depth: flat_node.depth as usize,
                    is_last: flat_node.is_last_child,
                    has_children: flat_node_child_counts
                        .get(&flat_node_id)
                        .is_some_and(|count| *count > 0),
                    local_group: false,
                    local_profile_id: None,
                    active_local_session_count: 0,
                    standalone_session: None,
                    group: None,
                })
            })
            .collect::<Vec<_>>();

        // Standalone records remain visible after their current surface is closed.
        rows.extend(
            self.standalone_connections
                .records()
                .iter()
                .map(|record| self.standalone_active_session_sidebar_row(record, cx)),
        );
        let host = self.tab_host.read(cx);
        let mut local_instances = host.local_sessions.iter().collect::<Vec<_>>();
        local_instances.sort_by_key(|(id, _)| id.0);
        let profiles = self.connection_store.local_terminal_profiles();
        let known_profiles = profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<HashSet<_>>();
        let mut local_rows = HashMap::<Option<String>, LocalSessionRowState>::new();
        for (id, instance) in local_instances {
            let Some(location) = host.terminal_location(*id) else {
                continue;
            };
            let Some(pane) = host.panes().get(&location.pane_id) else {
                continue;
            };
            let profile_id = instance
                .profile_id
                .as_deref()
                .filter(|profile_id| known_profiles.contains(*profile_id))
                .map(str::to_owned);
            let state = local_rows.entry(profile_id).or_default();
            state.terminal_ids.push(*id);
            state.search.push(' ');
            state.search.push_str(&instance.title);
            if let Some(cwd) = &instance.cwd {
                state.search.push(' ');
                state.search.push_str(&cwd.display().to_string());
            }
            let session_readiness = terminal_lifecycle_readiness(&pane.read(cx).lifecycle());
            if session_readiness == ActiveSessionReadiness::Ready {
                state.active_count += 1;
            }
            if session_readiness == ActiveSessionReadiness::Ready
                || state.readiness == ActiveSessionReadiness::Disconnected
            {
                state.readiness = session_readiness;
            }
        }
        for profile in profiles {
            if self
                .settings_store
                .settings()
                .sidebar_ui
                .hidden_local_terminal_profile_ids
                .contains(&profile.id)
            {
                continue;
            }
            let state = local_rows
                .remove(&Some(profile.id.clone()))
                .unwrap_or_default();
            let node_id = super::super::local_sessions::local_profile_node_id(&profile.id);
            rows.push(ActiveSessionSidebarRow {
                node_id: node_id.clone(),
                parent_id: None,
                saved_connection_id: None,
                title: profile.name.clone(),
                host: format!(
                    "{} {}",
                    profile.cwd.as_deref().unwrap_or_default(),
                    state.search
                ),
                username: String::new(),
                port: 0,
                node_view: ActiveSessionNode {
                    id: node_id.0,
                    title: profile.name.clone(),
                    port: 0,
                    terminal_ids: state.terminal_ids,
                    readiness: state.readiness,
                },
                depth: 0,
                is_last: true,
                has_children: false,
                standalone_session: None,
                local_group: true,
                local_profile_id: Some(profile.id.clone()),
                active_local_session_count: state.active_count,
                group: None,
            });
        }
        if let Some(state) = local_rows.remove(&None) {
            let title = self
                .i18n
                .t("modals.new_connection.transport_local_terminal");
            let node_id = "local-terminal-group".to_string();
            rows.push(ActiveSessionSidebarRow {
                node_id: NodeId::new(node_id.clone()),
                parent_id: None,
                saved_connection_id: None,
                title: title.clone(),
                host: state.search,
                username: String::new(),
                port: 0,
                node_view: ActiveSessionNode {
                    id: node_id,
                    title,
                    port: 0,
                    terminal_ids: state.terminal_ids,
                    readiness: state.readiness,
                },
                depth: 0,
                is_last: true,
                has_children: false,
                standalone_session: None,
                local_group: true,
                local_profile_id: None,
                active_local_session_count: state.active_count,
                group: None,
            });
        }
        rows
    }

    pub(in crate::workspace) fn render_active_sessions_footer(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Search and tree expansion affect presentation, never the workspace total.
        let rows = self.unfiltered_active_session_sidebar_rows(cx);
        let node_connections: HashSet<_> = rows
            .iter()
            .filter_map(|row| self.node_router.connection_id_for_node(&row.node_id))
            .collect();
        let standalone_sftp_count = self
            .standalone_sftp_sessions
            .values()
            .filter(|runtime| {
                matches!(
                    runtime.handle.state(),
                    oxideterm_ssh::ConnectionState::Active | oxideterm_ssh::ConnectionState::Idle
                ) && !node_connections.contains(&runtime.connection_id)
            })
            .map(|runtime| &runtime.connection_id)
            .collect::<HashSet<_>>()
            .len();
        // FTP/FTPS runtimes are registered after connect and own any transfer
        // sockets. Count the runtime once, not its tabs or temporary sockets.
        let ftp_count = self
            .ftp_sessions
            .values()
            .filter(|runtime| !runtime.cancel.is_cancelled())
            .count();
        let count = active_connection_count(&rows) + standalone_sftp_count + ftp_count;
        // Deletion outcome takes over the footer label until the next
        // selection action resets it; otherwise both live counts are shown.
        let label = if let Some(outcome) = self.active_session_selection_summary {
            if outcome.skipped > 0 {
                self.i18n
                    .t("sidebar.session_removed_count_skipped")
                    .replace("{{removed}}", &outcome.removed.to_string())
                    .replace("{{skipped}}", &outcome.skipped.to_string())
            } else {
                self.i18n
                    .t("sidebar.session_removed_count")
                    .replace("{{count}}", &outcome.removed.to_string())
            }
        } else {
            self.i18n
                .t("sidebar.session_footer_counts")
                .replace("{{active}}", &count.to_string())
                .replace(
                    "{{selected}}",
                    &self.active_session_selection.len().to_string(),
                )
        };
        div()
            .w_full()
            .min_w_0()
            .flex_none()
            .h(px(
                crate::workspace::terminal_command_bar::TERMINAL_SENDER_COMPACT_HEIGHT,
            ))
            .px(px(self.tokens.spacing.three))
            .py_0()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .border_t_1()
            .border_color(self.workspace_chrome_divider())
            .bg(self.workspace_sidebar_background(theme.shell_bg()))
            .child(
                div()
                    .flex_none()
                    .size(px(6.0))
                    .rounded_full()
                    .opacity(if count > 0 { 1.0 } else { 0.55 })
                    .bg(rgb(if count > 0 {
                        theme.success
                    } else {
                        theme.text_muted
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_features(oxideterm_gpui_ui::tabular_nums_features())
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .into_any_element()
    }

    fn standalone_active_session_sidebar_row(
        &self,
        record: &standalone_connections::StandaloneConnectionRecord,
        cx: &App,
    ) -> ActiveSessionSidebarRow {
        let (readiness, terminal_ids) = match record.surface {
            Some(standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab_id)) => {
                let Some(session) = self.remote_desktop.read(cx).session(tab_id) else {
                    return self.disconnected_standalone_sidebar_row(record);
                };
                let session = session.read(cx);
                (
                    remote_desktop_readiness(session.active_session_status()),
                    Vec::new(),
                )
            }
            Some(standalone_connections::StandaloneConnectionSurface::Terminal(session_id)) => {
                let Some(shared_session) = self
                    .tab_host
                    .read(cx)
                    .terminal_location(session_id)
                    .and_then(|location| {
                        self.tab_host
                            .read(cx)
                            .panes()
                            .get(&location.pane_id)
                            .map(|pane| pane.read(cx).shared_session())
                    })
                else {
                    return self.disconnected_standalone_sidebar_row(record);
                };
                let terminal = shared_session.lock();
                let readiness =
                    if record.kind == standalone_connections::StandaloneConnectionKind::Mosh {
                        terminal
                            .mosh_connection_status()
                            .map(|status| match status {
                                oxideterm_terminal::MoshConnectionStatus::Connecting => {
                                    ActiveSessionReadiness::Connecting
                                }
                                oxideterm_terminal::MoshConnectionStatus::Connected => {
                                    ActiveSessionReadiness::Ready
                                }
                                oxideterm_terminal::MoshConnectionStatus::Interrupted => {
                                    ActiveSessionReadiness::Error
                                }
                            })
                            .unwrap_or(ActiveSessionReadiness::Connecting)
                    } else if record.kind == standalone_connections::StandaloneConnectionKind::Ble {
                        terminal
                            .ble_connection_status()
                            .map(ble_connection_readiness)
                            .unwrap_or_else(|| terminal_lifecycle_readiness(&terminal.lifecycle()))
                    } else {
                        terminal_lifecycle_readiness(&terminal.lifecycle())
                    };
                (readiness, vec![session_id])
            }
            None => (record.readiness.clone(), Vec::new()),
        };

        let row_id = format!("standalone-connection-{}", record.id);
        ActiveSessionSidebarRow {
            node_id: NodeId::new(row_id.clone()),
            parent_id: None,
            saved_connection_id: None,
            title: record.title.clone(),
            host: String::new(),
            username: String::new(),
            port: 0,
            node_view: ActiveSessionNode {
                id: row_id,
                title: record.title.clone(),
                port: 0,
                terminal_ids,
                readiness,
            },
            depth: 0,
            is_last: true,
            has_children: false,
            local_group: false,
            local_profile_id: None,
            active_local_session_count: 0,
            group: None,
            standalone_session: Some(StandaloneActiveSession {
                connection_id: record.id.clone(),
                kind: record.kind,
                target: record.surface,
            }),
        }
    }

    fn disconnected_standalone_sidebar_row(
        &self,
        record: &standalone_connections::StandaloneConnectionRecord,
    ) -> ActiveSessionSidebarRow {
        let row_id = format!("standalone-connection-{}", record.id);
        ActiveSessionSidebarRow {
            node_id: NodeId::new(row_id.clone()),
            parent_id: None,
            saved_connection_id: None,
            title: record.title.clone(),
            host: String::new(),
            username: String::new(),
            port: 0,
            node_view: ActiveSessionNode {
                id: row_id,
                title: record.title.clone(),
                port: 0,
                terminal_ids: Vec::new(),
                readiness: record.readiness.clone(),
            },
            depth: 0,
            is_last: true,
            has_children: false,
            local_group: false,
            local_profile_id: None,
            active_local_session_count: 0,
            group: None,
            standalone_session: Some(StandaloneActiveSession {
                connection_id: record.id.clone(),
                kind: record.kind,
                target: None,
            }),
        }
    }

    pub(in crate::workspace) fn sync_active_session_sidebar_list_state(
        &mut self,
        rows: &[ActiveSessionSidebarRow],
        view_mode: ActiveSessionSidebarViewMode,
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| self.active_session_sidebar_row_signature(row, &rows, view_mode, cx))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.active_session_sidebar_list_state,
            &mut self.active_session_sidebar_list_cache.borrow_mut(),
            "active-sessions-sidebar",
            &signatures,
            self.active_session_sidebar_list_spec(),
        );
    }

    fn active_session_sidebar_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT),
            ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN,
        )
    }

    pub(in crate::workspace) fn render_active_session_sidebar_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.active_session_sidebar_display_rows(cx);
        let Some(row) = rows.get(index).cloned() else {
            return div().into_any_element();
        };
        let ancestor_rails = session_row_ancestor_rails(&rows, index);
        div()
            .px_1()
            .child(self.render_active_session_node_with_rails(row, &ancestor_rails, &rows, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn active_session_sidebar_row_signature(
        &self,
        row: &ActiveSessionSidebarRow,
        rows: &[ActiveSessionSidebarRow],
        view_mode: ActiveSessionSidebarViewMode,
        cx: &App,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();
        // This virtual row owns the node header plus expanded action/terminal
        // children. Hash all state that can change its visible height or labels.
        view_mode.hash(&mut hasher);
        self.tokens.metrics.ui_text_xs.to_bits().hash(&mut hasher);
        row.node_id.hash(&mut hasher);
        row.parent_id.hash(&mut hasher);
        row.title.hash(&mut hasher);
        row.port.hash(&mut hasher);
        row.node_view.title.hash(&mut hasher);
        row.host.hash(&mut hasher);
        row.node_view.terminal_ids.hash(&mut hasher);
        format!("{:?}", row.node_view.status()).hash(&mut hasher);
        row.depth.hash(&mut hasher);
        row.is_last.hash(&mut hasher);
        row.has_children.hash(&mut hasher);
        row.standalone_session.hash(&mut hasher);
        row.local_group.hash(&mut hasher);
        row.local_profile_id.hash(&mut hasher);
        // Owning a saved connection swaps "save as connection" for the
        // port-forwarding row, so it changes the option stack height.
        row.saved_connection_id.hash(&mut hasher);
        self.can_save_runtime_node_as_connection(&row.node_id, row.saved_connection_id.as_deref())
            .hash(&mut hasher);
        if row.local_group {
            row.active_local_session_count.hash(&mut hasher);
            if row.local_profile_id.is_none() {
                self.local_session_group_expanded.hash(&mut hasher);
            }
            self.active_terminal_session_id(cx).hash(&mut hasher);
        }
        // Saved local profile rows light their icon from the active terminal
        // session; the group row above already re-renders via that id.
        if row.local_profile_id.is_some() && !row.local_group {
            (self
                .active_terminal_session_id(cx)
                .is_some_and(|id| row.node_view.terminal_ids.contains(&id)))
            .hash(&mut hasher);
        }
        row.standalone_session
            .as_ref()
            .is_some_and(|session| {
                // Tab ownership drives the record's icon and accent bar.
                self.standalone_session_owns_active_tab(session, cx)
                    .hash(&mut hasher);
                self.expanded_standalone_connections
                    .contains(&session.connection_id)
            })
            .hash(&mut hasher);
        self.disclosure_motions
            .signature(&format!("session:{}:", row.node_id.0))
            .hash(&mut hasher);
        self.expanded_ssh_nodes
            .contains(&row.node_id)
            .hash(&mut hasher);
        self.has_active_reconnect_job(&row.node_id, cx)
            .hash(&mut hasher);
        self.session_node_row_is_focused(&row.node_id, cx)
            .hash(&mut hasher);
        // SFTP/IDE/forwards tab ownership can highlight a node while
        // active_ssh_node_id still points at the previously focused node, so
        // the signature must track it or the cached row keeps the old accent.
        (self.active_content_tab_ssh_node_id(cx).as_ref() == Some(&row.node_id)).hash(&mut hasher);
        // Tool rows (sftp/ide/port-forwarding) light per content-tab kind, so
        // both the owner match and the matched key belong in the signature.
        let (active_action_key, active_action_node) = self.active_content_tab_action_key(cx);
        (active_action_node.as_ref() == Some(&row.node_id)).hash(&mut hasher);
        active_action_key.hash(&mut hasher);
        if let Some(target) = active_session_row_selection_target(row) {
            self.active_session_selection
                .contains(&target)
                .hash(&mut hasher);
        }
        if let Some(group) = &row.group {
            group.user_level.hash(&mut hasher);
            group.label.hash(&mut hasher);
            group.connection_name.hash(&mut hasher);
            // Expansion decides whether the user row renders its option stack,
            // and the group disclosure keys live outside the tree namespace, so
            // they need their own signature or the collapse animation would
            // leave the measured expanded height cached.
            self.session_group_row_expanded(row).hash(&mut hasher);
            self.disclosure_motions
                .signature(&format!("session-group:{}:", row.node_id.0))
                .hash(&mut hasher);
            for member in &group.members {
                // Reuse the full member signature so option visibility, lifecycle
                // actions, and reconnect state all invalidate the cached height.
                self.active_session_sidebar_row_signature(member, rows, view_mode, cx)
                    .hash(&mut hasher);
                self.session_group_member_route(member).hash(&mut hasher);
            }
            // Host rows derive their accent from sibling user rows, so that
            // resolved focus state must invalidate the cached row as well.
            self.session_group_row_is_focused(row, rows, cx)
                .hash(&mut hasher);
        }
        hasher.finish()
    }

    pub(in crate::workspace) fn toggle_active_session_sidebar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.active_session_sidebar_view_mode = match self.active_session_sidebar_view_mode {
            ActiveSessionSidebarViewMode::Tree => ActiveSessionSidebarViewMode::Host,
            ActiveSessionSidebarViewMode::Host => ActiveSessionSidebarViewMode::Tree,
        };
        cx.notify();
    }

    fn render_local_session_sidebar_row(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = if row.local_profile_id.is_some() {
            self.expanded_ssh_nodes.contains(&row.node_id)
        } else {
            self.local_session_group_expanded
        };
        let owns_active_tab = self
            .active_terminal_session_id(cx)
            .is_some_and(|id| row.node_view.terminal_ids.contains(&id));
        let in_selection = self
            .active_session_selection
            .contains(&ActiveSessionSelectionTarget::Node(row.node_id.clone()));
        let selected = owns_active_tab || in_selection;
        let status = self.session_node_status(row.node_view.status());
        let motion_key = format!("session:{}:children", row.node_id.0);
        let mut children = Vec::new();
        if self.disclosure_motions.retained(&motion_key, expanded) {
            let profile_id = row.local_profile_id.clone();
            let tools = vec![SessionActionSpec {
                debug_key: "new-terminal",
                icon: LucideIcon::Plus,
                label: self.i18n.t("sessions.tree.actions.new_terminal"),
                variant: SessionActionVariant::Primary,
                listener: Box::new(cx.listener(move |this, _, window, cx| {
                    if let Some(profile_id) = profile_id.as_deref() {
                        this.open_saved_local_terminal_profile(profile_id, window, cx);
                    } else {
                        this.open_local_shell_launcher(cx);
                    }
                    cx.stop_propagation();
                })),
            }];
            let mut lifecycle = Vec::new();
            if let Some(profile_id) = row.local_profile_id.clone() {
                lifecycle.push(SessionActionSpec {
                    debug_key: "remove",
                    icon: LucideIcon::Trash2,
                    label: self.i18n.t("sessions.tree.actions.remove_session"),
                    variant: SessionActionVariant::Danger,
                    listener: Box::new(cx.listener(move |this, _, window, cx| {
                        this.remove_local_profile_session(&profile_id, window, cx);
                        cx.stop_propagation();
                    })),
                });
            }
            children = self.render_session_child_segments(
                &row.node_id.0,
                1,
                &[],
                false,
                &row.node_view.terminal_ids,
                tools,
                lifecycle,
                cx,
            );
        }
        let header = self.render_session_node_header(
            row.node_id,
            row.node_view,
            expanded,
            selected,
            in_selection,
            status,
            true,
            row.local_profile_id,
            cx,
        );

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .when(!children.is_empty(), |group| {
                group.child(self.disclosure_motions.render(
                    &motion_key,
                    &self.tokens,
                    div().w_full().flex().flex_col().children(children),
                    None,
                ))
            })
            .into_any_element()
    }

    fn render_session_segment_separator(
        &self,
        depth: usize,
        ancestor_continues: &[bool],
    ) -> AnyElement {
        self.render_session_tree_child_with_rails(
            depth,
            ancestor_continues,
            false,
            div()
                .w_full()
                .h(px(7.0))
                .pl(px(6.0))
                .pr(px(8.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .bg(rgba((self.tokens.ui.border << 8) | 0x38)),
                )
                .into_any_element(),
        )
    }

    fn render_session_segment_gap(&self, depth: usize, ancestor_continues: &[bool]) -> AnyElement {
        self.render_session_tree_child_with_rails(
            depth,
            ancestor_continues,
            false,
            div().w_full().h(px(2.0)).into_any_element(),
        )
    }

    fn render_session_child_segments(
        &self,
        owner_key: &str,
        depth: usize,
        ancestor_continues: &[bool],
        innermost_continues_below: bool,
        terminal_ids: &[TerminalSessionId],
        tools: Vec<SessionActionSpec>,
        lifecycle: Vec<SessionActionSpec>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let has_terminals = !terminal_ids.is_empty();
        let has_tools = !tools.is_empty();
        let has_lifecycle = !lifecycle.is_empty();
        let mut children =
            Vec::with_capacity(terminal_ids.len() + tools.len() + lifecycle.len() + 2);
        // The active SFTP/IDE/forwards content tab lights its matching tool row
        // inside the owning node's tree, mirroring the terminal child rows.
        let (active_action_key, active_action_node) = self.active_content_tab_action_key(cx);
        let owner_has_active_tool = active_action_node
            .as_ref()
            .is_some_and(|node| node.0 == owner_key);

        let terminal_count = terminal_ids.len();
        for (index, &session_id) in terminal_ids.iter().enumerate() {
            let line_stops_here = !innermost_continues_below
                && !has_tools
                && !has_lifecycle
                && index + 1 == terminal_count;
            children.push(self.render_session_terminal_item_with_rails(
                depth,
                ancestor_continues,
                line_stops_here,
                session_id,
                index + 1,
                cx,
            ));
        }

        if has_terminals && (has_tools || has_lifecycle) {
            children.push(self.render_session_segment_separator(depth, ancestor_continues));
        }

        let tool_count = tools.len();
        for (index, action) in tools.into_iter().enumerate() {
            let line_stops_here =
                !innermost_continues_below && !has_lifecycle && index + 1 == tool_count;
            let active = owner_has_active_tool && action.debug_key == active_action_key;
            let listener = action.listener;
            children.push(self.render_session_action_item_with_rails(
                owner_key,
                depth,
                ancestor_continues,
                line_stops_here,
                action.debug_key,
                action.icon,
                action.label,
                action.variant,
                active,
                move |event, window, cx| listener(event, window, cx),
                cx,
            ));
        }

        if has_tools && has_lifecycle {
            children.push(self.render_session_segment_gap(depth, ancestor_continues));
        }

        let lifecycle_count = lifecycle.len();
        for (index, action) in lifecycle.into_iter().enumerate() {
            let line_stops_here = !innermost_continues_below && index + 1 == lifecycle_count;
            let listener = action.listener;
            children.push(self.render_session_action_item_with_rails(
                owner_key,
                depth,
                ancestor_continues,
                line_stops_here,
                action.debug_key,
                action.icon,
                action.label,
                action.variant,
                false,
                move |event, window, cx| listener(event, window, cx),
                cx,
            ));
        }

        children
    }

    /// Renders the children a node shows when expanded. Host view stacks this
    /// block under a user group row, so the block must not depend on disclosure
    /// state.
    fn session_node_option_items(
        &self,
        node_id: &NodeId,
        node_view: &ActiveSessionNode,
        saved_connection_id: Option<&str>,
        depth: usize,
        ancestor_continues: &[bool],
        innermost_continues_below: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut terminals: &[TerminalSessionId] = &[];
        let mut tools = Vec::new();

        if !self.has_active_reconnect_job(node_id, cx) {
            if matches!(
                node_view.status(),
                ActiveSessionStatus::Active | ActiveSessionStatus::Connected
            ) {
                terminals = &node_view.terminal_ids;
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        let _ = this.queue_ssh_terminal_tab_for_sidebar_node_with_sftp(
                            node_id.clone(),
                            event.modifiers.control,
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                });
                tools.push(SessionActionSpec {
                    debug_key: "new-terminal",
                    icon: LucideIcon::Plus,
                    label: self.i18n.t("sessions.tree.actions.new_terminal"),
                    variant: SessionActionVariant::Primary,
                    listener: Box::new(listener),
                });
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_sftp_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                tools.push(SessionActionSpec {
                    debug_key: "sftp",
                    icon: LucideIcon::FolderOpen,
                    label: self.i18n.t("sessions.tree.actions.sftp"),
                    variant: SessionActionVariant::Primary,
                    listener: Box::new(listener),
                });
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_ide_folder_picker_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                tools.push(SessionActionSpec {
                    debug_key: "ide",
                    icon: LucideIcon::Code2,
                    label: "IDE".to_string(),
                    variant: SessionActionVariant::Primary,
                    listener: Box::new(listener),
                });
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_forwards_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                tools.push(SessionActionSpec {
                    debug_key: "port-forwarding",
                    icon: LucideIcon::ArrowLeftRight,
                    label: self.i18n.t("sessions.tree.actions.port_forwarding"),
                    variant: SessionActionVariant::Primary,
                    listener: Box::new(listener),
                });
                if self.can_save_runtime_node_as_connection(node_id, saved_connection_id) {
                    let listener = cx.listener({
                        let node_id = node_id.clone();
                        move |this, _event, window, cx| {
                            this.open_save_runtime_node_form(node_id.clone(), window, cx);
                            cx.stop_propagation();
                        }
                    });
                    tools.push(SessionActionSpec {
                        debug_key: "save-connection",
                        icon: LucideIcon::Save,
                        label: self.i18n.t("sessions.tree.actions.save_as_connection"),
                        variant: SessionActionVariant::Primary,
                        listener: Box::new(listener),
                    });
                }
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_drill_down_form(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                tools.push(SessionActionSpec {
                    debug_key: "drill-in",
                    icon: LucideIcon::ArrowDownRight,
                    label: self.i18n.t("sessions.tree.actions.drill_in"),
                    variant: SessionActionVariant::Primary,
                    listener: Box::new(listener),
                });
            } else if matches!(node_view.status(), ActiveSessionStatus::Error) {
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    let saved_connection_id = saved_connection_id.map(str::to_owned);
                    move |this, _event, window, cx| {
                        if let Some(saved_connection_id) = saved_connection_id.as_deref() {
                            this.open_saved_connection_reconnect_editor(
                                node_id.clone(),
                                saved_connection_id,
                                window,
                                cx,
                            );
                        } else {
                            this.open_runtime_node_reconnect_editor(node_id.clone(), window, cx);
                        }
                        cx.stop_propagation();
                    }
                });
                tools.push(SessionActionSpec {
                    debug_key: "edit-reconnect",
                    icon: LucideIcon::Pencil,
                    label: self.i18n.t("sessions.tree.actions.edit_and_reconnect"),
                    variant: SessionActionVariant::Primary,
                    listener: Box::new(listener),
                });
            }
        }

        if saved_connection_id.is_some()
            && !matches!(
                node_view.status(),
                ActiveSessionStatus::Active | ActiveSessionStatus::Connected
            )
        {
            let forward_node = node_id.clone();
            tools.push(SessionActionSpec {
                debug_key: "port-forwarding",
                icon: LucideIcon::ArrowLeftRight,
                label: self.i18n.t("sessions.tree.actions.port_forwarding"),
                variant: SessionActionVariant::Primary,
                listener: Box::new(cx.listener(move |this, _, window, cx| {
                    this.open_forwards_tab(forward_node.clone(), window, cx);
                    cx.stop_propagation();
                })),
            });
        }

        let lifecycle = self.session_node_lifecycle_specs(node_id, node_view.status(), cx);
        self.render_session_child_segments(
            &node_id.0,
            depth,
            ancestor_continues,
            innermost_continues_below,
            terminals,
            tools,
            lifecycle,
            cx,
        )
    }

    fn session_group_row_expanded(&self, row: &ActiveSessionSidebarRow) -> bool {
        // Host headers open by default so the host/user outline is visible at a
        // glance; user rows stay closed so option stacks never crowd the list.
        let default_expanded = row.group.as_ref().is_some_and(|group| !group.user_level);
        self.session_group_expanded
            .get(&row.node_id)
            .copied()
            .unwrap_or(default_expanded)
    }

    fn session_group_chevron(
        &self,
        node_id: NodeId,
        expanded: bool,
        default_expanded: bool,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.session_group_toggle(
            SharedString::from(format!("session-group-toggle-{}", node_id.0)),
            node_id.clone(),
            expanded,
            default_expanded,
            label,
            cx,
        )
        .child(self.render_animated_chevron(
            (
                SharedString::from(format!("session-group-chevron-{}", node_id.0)),
                expanded as usize,
            ),
            expanded,
            12.0,
            rgb(theme.text_muted),
        ))
        .into_any_element()
    }

    fn session_group_toggle(
        &self,
        id: SharedString,
        node_id: NodeId,
        expanded: bool,
        default_expanded: bool,
        label: String,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let theme = self.tokens.ui;
        let expanded_target = !expanded;
        div()
            .id(id)
            .w(px(12.0))
            .h(px(20.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .role(gpui::Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .focus_visible(move |style| style.border_1().border_color(rgb(theme.accent)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_session_group_expanded(node_id.clone(), expanded_target, default_expanded);
                // The user-level option block animates through the same
                // disclosure set as tree nodes; host-level rows are separate
                // virtual rows, so their motion key only resets state.
                this.begin_disclosure_motion(
                    format!("session-group:{}:children", node_id.0),
                    expanded_target,
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            }))
    }

    fn set_session_group_expanded(
        &mut self,
        node_id: NodeId,
        expanded: bool,
        default_expanded: bool,
    ) {
        // Store only deviations from the level default so a group that comes
        // back after node churn cannot inherit a stale disclosure state.
        if expanded == default_expanded {
            self.session_group_expanded.remove(&node_id);
        } else {
            self.session_group_expanded.insert(node_id, expanded);
        }
    }

    /// Drops disclosure entries whose host or user no longer exists. Called from
    /// the sidebar content renderer so node churn cannot grow the map forever.
    fn prune_session_group_expanded(&mut self) {
        if self.session_group_expanded.is_empty() {
            return;
        }
        let mut live = HashSet::new();
        for node in self.ssh_nodes.values() {
            let host = node.endpoint.host.trim();
            if host.is_empty() {
                continue;
            }
            let host_key = host.to_lowercase();
            live.insert(host_group_row_id(&host_key).0);
            live.insert(user_group_row_id(&host_key, &node.endpoint.username).0);
        }
        self.session_group_expanded.retain(|node_id, _| {
            let id = node_id.0.as_str();
            // Suffixed ids come from the collision guard, so compare the base.
            live.contains(id)
                || id
                    .rsplit_once('#')
                    .is_some_and(|(base, _)| live.contains(base))
        });
    }

    /// Whether the node owns the currently visible surface. Derived entirely
    /// at render time from the active terminal session and content tab, so a
    /// tab switch puts the accent out immediately; the stored
    /// `active_ssh_node_id` is stale-prone because activation flows bypass
    /// `sync_active_tab_surface`, so it must not drive row lighting.
    fn session_node_row_is_focused(&self, node_id: &NodeId, cx: &App) -> bool {
        let terminal_owner = self.active_terminal_session_id(cx).and_then(|session_id| {
            self.workspace_runtime
                .read(cx)
                .ssh_terminal_node_id(session_id)
        });
        terminal_owner.as_ref() == Some(node_id)
            || self.active_content_tab_ssh_node_id(cx).as_ref() == Some(node_id)
    }

    /// Whether a host-view group row carries the focused node's accent. User
    /// rows fold their member nodes; host rows only reference their user rows
    /// as siblings, so the visible row list is needed to resolve them.
    fn session_group_row_is_focused(
        &self,
        row: &ActiveSessionSidebarRow,
        rows: &[ActiveSessionSidebarRow],
        cx: &App,
    ) -> bool {
        let Some(group) = row.group.as_ref() else {
            return false;
        };
        if group.user_level {
            return group
                .members
                .iter()
                .any(|member| self.session_node_row_is_focused(&member.node_id, cx));
        }
        rows.iter()
            .filter(|candidate| candidate.parent_id.as_ref() == Some(&row.node_id))
            .any(|user| {
                user.group.as_ref().is_some_and(|user_group| {
                    user_group
                        .members
                        .iter()
                        .any(|member| self.session_node_row_is_focused(&member.node_id, cx))
                })
            })
    }

    fn render_session_group_row(
        &self,
        row: &ActiveSessionSidebarRow,
        rows: &[ActiveSessionSidebarRow],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(group) = row.group.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let node_height = self.session_tree_node_height();
        let text_size = self.tokens.metrics.ui_text_xs;
        let meta_text_size = self.tokens.metrics.ui_text_2xs;
        let expanded = self.session_group_row_expanded(row);
        let chevron_label = self.i18n.t(if expanded {
            "settings_view.tool_use_collapse"
        } else {
            "settings_view.tool_use_expand"
        });
        let node_id = row.node_id.clone();
        // Group rows carry no selection target of their own. A folded
        // single-member user row is the record row's Host-view surface, so it
        // mirrors its member's selection; ownership keeps the selected
        // background while the accent bar marks row selection only.
        let focused = self.session_group_row_is_focused(row, rows, cx);
        let member_target = if group.user_level && group.members.len() == 1 {
            Some(ActiveSessionSelectionTarget::Node(
                group.members[0].node_id.clone(),
            ))
        } else {
            None
        };
        let in_selection = member_target
            .as_ref()
            .is_some_and(|target| self.active_session_selection.contains(target));
        let selected = focused || in_selection;
        let motion_key = format!("session-group:{}:children", node_id.0);
        let default_expanded = !group.user_level;
        let icon_selector = format!("session-header-icon-{}", node_id.0);
        let title_selector = format!("session-header-title-{}", node_id.0);
        // Both levels share the unified header geometry: 12px chevron, 6px gap,
        // 14px centered icon slot, 6px gap, and medium-weight title.
        let header = self
            .session_sidebar_row(selected, in_selection, node_height)
            .id(SharedString::from(format!(
                "session-group-header-{}",
                node_id.0
            )))
            .child(self.session_group_chevron(
                node_id.clone(),
                expanded,
                default_expanded,
                chevron_label.clone(),
                cx,
            ))
            .child(
                div()
                    .ml(px(6.0))
                    .w(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .debug_selector(move || icon_selector.clone())
                    // Tab ownership lights the host/user group icon too.
                    .child(Self::render_lucide_icon(
                        if group.user_level {
                            LucideIcon::UserRound
                        } else {
                            LucideIcon::Server
                        },
                        SESSION_TREE_ICON_SIZE,
                        if focused {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    )),
            );
        let header = if group.user_level {
            header
                .child(
                    div()
                        .ml(px(6.0))
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_size(px(text_size))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .debug_selector(move || title_selector.clone())
                        .child(group.label.clone()),
                )
                .when_some(group.connection_name.as_ref(), |header, connection_name| {
                    header.child(
                        div()
                            .ml(px(6.0))
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(meta_text_size))
                            .text_color(rgb(theme.text_muted))
                            .debug_selector(|| "session-group-connection-name".to_string())
                            .child(connection_name.clone()),
                    )
                })
        } else {
            let host_color = mix_rgb(theme.text, theme.text_muted, 0.2);
            header.child(
                div()
                    .ml(px(6.0))
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(text_size))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(host_color))
                    .debug_selector(move || title_selector.clone())
                    .child(group.label.clone()),
            )
        };
        let toggle_id = node_id.clone();
        let expanded_target = !expanded;
        let selection_target = member_target.clone();
        let header = header
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                let gpui::ClickEvent::Mouse(click) = event else {
                    return;
                };
                let modifiers = click.down.modifiers;
                match selection_target.as_ref() {
                    Some(target) => {
                        // The folded record row keeps its expand/collapse
                        // behavior while carrying the member's selection.
                        let mode = if modifiers.control {
                            ActiveSessionSelectionMode::Toggle
                        } else if modifiers.shift {
                            ActiveSessionSelectionMode::Range
                        } else {
                            ActiveSessionSelectionMode::Single
                        };
                        this.apply_active_session_selection(target.clone(), mode, cx);
                        if mode != ActiveSessionSelectionMode::Single {
                            // Modifier clicks only steer the selection.
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                    }
                    None => this.clear_active_session_selection(cx),
                }
                this.set_session_group_expanded(
                    toggle_id.clone(),
                    expanded_target,
                    default_expanded,
                );
                this.begin_disclosure_motion(
                    format!("session-group:{}:children", toggle_id.0),
                    expanded_target,
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.open_active_session_selection_menu_if_selected(event.position, cx);
                }),
            )
            .into_any_element();

        if !group.user_level {
            return header;
        }

        let mut items = Vec::new();
        if self.disclosure_motions.retained(&motion_key, expanded) {
            let user_outer_rails = [!row.is_last];
            let member_count = group.members.len();
            let multi_member = member_count > 1;
            for (index, member) in group.members.iter().enumerate() {
                let member_is_last = index + 1 == member_count;
                if multi_member {
                    // Two nodes on the same host and user would otherwise render
                    // two identical option stacks, so label which node owns them.
                    // The user-level rail stops at the last member label.
                    items.push(self.render_session_tree_child_with_rails(
                        2,
                        &user_outer_rails,
                        member_is_last,
                        self.render_session_group_member_label(member, cx),
                    ));
                    let member_rails = [!row.is_last, !member_is_last];
                    items.extend(self.session_node_option_items(
                        &member.node_id,
                        &member.node_view,
                        member.saved_connection_id.as_deref(),
                        3,
                        &member_rails,
                        false,
                        cx,
                    ));
                } else {
                    items.extend(self.session_node_option_items(
                        &member.node_id,
                        &member.node_view,
                        member.saved_connection_id.as_deref(),
                        2,
                        &user_outer_rails,
                        false,
                        cx,
                    ));
                }
            }
        }

        let mut container = div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.render_session_tree_child_with_rails(row.depth, &[], row.is_last, header));
        if !items.is_empty() {
            // The inner id makes the rendered option stack observable in tests
            // without depending on a private cache key.
            container = container.child(
                self.disclosure_motions.render(
                    &motion_key,
                    &self.tokens,
                    div().w_full().flex().flex_col().child(
                        div()
                            .id(SharedString::from(format!(
                                "session-group-options-{}",
                                row.node_id.0
                            )))
                            .debug_selector(|| "session-group-options".to_string())
                            .w_full()
                            .flex()
                            .flex_col()
                            .children(items)
                            .into_any_element(),
                    ),
                    None,
                ),
            );
        }
        container.into_any_element()
    }

    fn session_group_member_route(&self, member: &ActiveSessionSidebarRow) -> Option<String> {
        member.parent_id.as_ref()?;
        // Use the complete node-owned route, not a terminal pane or secret-bearing config.
        // Two routes can share the same final jump host in different private networks.
        let path = self.node_router.path_to_node(&member.node_id).ok()?;
        let endpoints = path
            .iter()
            .map(|id| {
                let node = self.node_router.node_metadata(id)?;
                Some(session_endpoint_label(
                    &node.username,
                    &node.host,
                    node.port,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(endpoints.join(" → "))
    }

    fn render_session_group_member_label(
        &self,
        member: &ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let item_height = self.session_tree_item_height();
        let text_size = self.tokens.metrics.ui_text_xs;
        let meta_text_size = self.tokens.metrics.ui_text_2xs;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let route = self.session_group_member_route(member);
        let endpoint = session_endpoint_label(&member.username, &member.host, member.port);
        let tooltip = format!(
            "{}\n{}",
            member.title,
            route.as_deref().unwrap_or(&endpoint)
        );
        let tooltip_id = format!("session-group-member-{}", member.node_id.0);
        let leave_id = tooltip_id.clone();
        let icon_selector = format!("session-group-member-icon-{}", member.node_id.0);
        let title_selector = format!("session-group-member-title-{}", member.node_id.0);
        let port_selector = format!("session-group-member-port-{}", member.node_id.0);
        let route_selector = format!("session-group-member-route-{}", member.node_id.0);
        let lead_icon = if route.is_some() {
            LucideIcon::Network
        } else {
            LucideIcon::Server
        };
        // The member label is the record row's Host-view surface when a user
        // folds several nodes, so it carries the selection background, the
        // accent bar, and the tab-ownership icon lighting of single-member
        // user rows.
        let member_target = ActiveSessionSelectionTarget::Node(member.node_id.clone());
        let in_selection = self.active_session_selection.contains(&member_target);
        let owns_active_tab = self.session_node_row_is_focused(&member.node_id, cx);
        let icon_color = if owns_active_tab {
            rgb(theme.accent)
        } else {
            rgb(theme.text_muted)
        };
        div()
            .id(SharedString::from(tooltip_id.clone()))
            .relative()
            .when(in_selection, |row| {
                row.bg(rgba((theme.accent << 8) | SESSION_ROW_SELECTED_BG_ALPHA))
                    .child(session_row_accent_bar(theme))
            })
            .w_full()
            .min_w_0()
            .pl(px(6.0))
            .pr(px(8.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(item_height))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .debug_selector(move || icon_selector.clone())
                            .child(Self::render_lucide_icon(
                                lead_icon,
                                SESSION_TREE_CHILD_ICON_SIZE,
                                icon_color,
                            )),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(text_size))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .debug_selector(move || title_selector.clone())
                            .child(member.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .debug_selector(move || port_selector.clone())
                            .child(oxideterm_gpui_ui::monospace_datum(
                                &self.tokens,
                                format!(":{}", member.port),
                                Some(mono_font_family),
                                oxideterm_gpui_ui::MonospaceDatumOptions::new(
                                    oxideterm_gpui_ui::MonospaceDatumTone::Muted,
                                )
                                .text_size(meta_text_size),
                            )),
                    )
                    .child(self.render_session_status_dot(
                        self.session_node_status(member.node_view.status()),
                    )),
            )
            .when_some(route, |row, route| {
                row.child(
                    div()
                        .h(px(item_height))
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w(px(14.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Self::render_lucide_icon(
                                    LucideIcon::ArrowDownRight,
                                    SESSION_TREE_CHILD_ICON_SIZE,
                                    rgb(theme.text_muted),
                                )),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_size(px(meta_text_size))
                                .text_color(rgb(theme.text_muted))
                                .debug_selector(move || route_selector.clone())
                                .child(route),
                        ),
                )
            })
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                this.queue_workspace_tooltip(
                    tooltip_id.clone(),
                    tooltip.clone(),
                    f32::from(event.position.x) + 12.0,
                    f32::from(event.position.y) + 16.0,
                    cx,
                );
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip(&leave_id, cx);
                }
            }))
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                let gpui::ClickEvent::Mouse(click) = event else {
                    return;
                };
                let modifiers = click.down.modifiers;
                let mode = if modifiers.control {
                    ActiveSessionSelectionMode::Toggle
                } else if modifiers.shift {
                    ActiveSessionSelectionMode::Range
                } else {
                    ActiveSessionSelectionMode::Single
                };
                this.apply_active_session_selection(member_target.clone(), mode, cx);
                cx.stop_propagation();
                cx.notify();
            }))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_active_session_node_with_rails(
        &self,
        row: ActiveSessionSidebarRow,
        ancestor_rails: &[bool],
        rows: &[ActiveSessionSidebarRow],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if row.group.is_some() {
            return self.render_session_group_row(&row, rows, cx);
        }
        if row.local_group {
            return self.render_local_session_sidebar_row(row, cx);
        }
        if row.standalone_session.is_some() {
            return self.render_standalone_session_sidebar_row(row, cx);
        }
        let node_id = row.node_id;
        let node_view = row.node_view;
        let node_depth = row.depth;
        let is_last = row.is_last;
        let has_children = row.has_children;
        let expanded = self.expanded_ssh_nodes.contains(&node_id);
        let motion_key = format!("session:{}:children", node_id.0);
        // The accent bar marks row selection; tab ownership shows in the
        // header icon and keeps the selected background. Node ownership of
        // the visible surface is derived at render time via
        // `session_node_row_is_focused`, so a tab switch puts the accent out
        // immediately even when the activation flow skipped
        // `sync_active_tab_surface`.
        let focused = self.session_node_row_is_focused(&node_id, cx);
        let in_selection = self
            .active_session_selection
            .contains(&ActiveSessionSelectionTarget::Node(node_id.clone()));
        let selected = focused || in_selection;
        let status = self.session_node_status(node_view.status());
        let mut children = Vec::new();

        if self.disclosure_motions.retained(&motion_key, expanded) {
            let mut child_ancestor_rails = ancestor_rails.to_vec();
            if node_depth > 0 {
                child_ancestor_rails.push(!is_last);
            }
            // Host view stacks the same option block under a user row, so the
            // block is shared instead of duplicated per view mode.
            children.extend(self.session_node_option_items(
                &node_id,
                &node_view,
                row.saved_connection_id.as_deref(),
                node_depth + 1,
                &child_ancestor_rails,
                has_children,
                cx,
            ));
        }

        let header = self.render_session_node_header(
            node_id,
            node_view,
            expanded,
            selected,
            in_selection,
            status,
            false,
            None,
            cx,
        );
        let header = if node_depth == 0 {
            header
        } else {
            self.render_session_tree_child_with_rails(node_depth, ancestor_rails, is_last, header)
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .when(!children.is_empty(), |node| {
                node.child(self.disclosure_motions.render(
                    &motion_key,
                    &self.tokens,
                    div().w_full().flex().flex_col().children(children),
                    None,
                ))
            })
            .into_any_element()
    }

    fn session_sidebar_row(&self, selected: bool, accent_bar: bool, height: f32) -> gpui::Div {
        let theme = self.tokens.ui;
        session_item_surface(theme, selected, SESSION_ROW_SELECTED_BG_ALPHA)
            .relative()
            .w_full()
            .min_w_0()
            .h(px(height))
            .px_2()
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .when(accent_bar, |row| row.child(session_row_accent_bar(theme)))
    }

    fn session_node_lifecycle_specs(
        &self,
        node_id: &NodeId,
        status: ActiveSessionStatus,
        cx: &mut Context<Self>,
    ) -> Vec<SessionActionSpec> {
        let actions = session_node_row_actions(status, self.has_active_reconnect_job(node_id, cx));
        actions
            .into_iter()
            .flatten()
            .map(|action| {
                let (debug_key, icon, key, danger) = match action {
                    SessionNodeRowAction::Connect => (
                        "connect",
                        LucideIcon::Power,
                        "sessions.actions.connect",
                        false,
                    ),
                    SessionNodeRowAction::Reconnect => (
                        "reconnect",
                        LucideIcon::RefreshCw,
                        "sessions.actions.reconnect",
                        false,
                    ),
                    SessionNodeRowAction::Disconnect => (
                        "disconnect",
                        LucideIcon::WifiOff,
                        "sessions.tree.actions.disconnect",
                        true,
                    ),
                    SessionNodeRowAction::CancelReconnect => (
                        "cancel-reconnect",
                        LucideIcon::X,
                        "sessions.tree.actions.cancel_reconnect",
                        false,
                    ),
                    SessionNodeRowAction::Remove => (
                        "remove",
                        LucideIcon::Trash2,
                        "sessions.tree.actions.remove_session",
                        true,
                    ),
                };
                let id = node_id.clone();
                SessionActionSpec {
                    debug_key,
                    icon,
                    label: self.i18n.t(key),
                    variant: if danger {
                        SessionActionVariant::Danger
                    } else {
                        SessionActionVariant::Primary
                    },
                    listener: Box::new(cx.listener(
                        move |this, event: &MouseDownEvent, window, cx| {
                            match action {
                                SessionNodeRowAction::Remove => {
                                    this.remove_inactive_session_tree_node(&id, window, cx)
                                }
                                SessionNodeRowAction::CancelReconnect => {
                                    this.cancel_reconnect_for_node(&id, cx)
                                }
                                SessionNodeRowAction::Disconnect => {
                                    this.request_disconnect_ssh_node(&id, window, cx)
                                }
                                SessionNodeRowAction::Connect | SessionNodeRowAction::Reconnect => {
                                    let _ = this.queue_ssh_terminal_tab_for_sidebar_node_with_sftp(
                                        id.clone(),
                                        event.modifiers.control,
                                        window,
                                        cx,
                                    );
                                }
                            }
                            cx.stop_propagation();
                        },
                    )),
                }
            })
            .collect()
    }

    /// Whether the active tab belongs to this standalone connection record.
    /// Shared by the row renderer and the row-signature cache so both agree
    /// on when the record's icon and accent bar change.
    fn standalone_session_owns_active_tab(
        &self,
        session: &StandaloneActiveSession,
        cx: &App,
    ) -> bool {
        match session.target {
            Some(standalone_connections::StandaloneConnectionSurface::Terminal(session_id)) => {
                self.active_terminal_session_id(cx) == Some(session_id)
            }
            Some(standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab_id)) => {
                self.active_tab_id(cx) == Some(tab_id)
            }
            None => false,
        }
    }

    fn render_standalone_session_sidebar_row(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = row.standalone_session else {
            return div().into_any_element();
        };
        let serial_details = self
            .standalone_connections
            .record(&session.connection_id)
            .and_then(|record| match &record.launch {
                standalone_connections::StandaloneConnectionLaunch::Serial { config, .. }
                | standalone_connections::StandaloneConnectionLaunch::SavedSerial {
                    config, ..
                } => Some(format!("{} · {}", config.port_path, config.baud_rate)),
                _ => None,
            });
        let theme = self.tokens.ui;
        let node_height = self.session_tree_node_height();
        let text_size = self.tokens.metrics.ui_text_xs;
        let meta_text_size = self.tokens.metrics.ui_text_2xs;
        // The accent bar marks row selection; tab ownership shows in the
        // record icon and keeps the selected background.
        let tab_active = self.standalone_session_owns_active_tab(&session, cx);
        let in_selection =
            self.active_session_selection
                .contains(&ActiveSessionSelectionTarget::Standalone(
                    session.connection_id.clone(),
                ));
        let active = tab_active || in_selection;
        let status = self.session_node_status(row.node_view.status());
        let expanded = self
            .expanded_standalone_connections
            .contains(&session.connection_id);
        let motion_key = format!("session:{}:children", row.node_id.0);
        let expand_id = session.connection_id.clone();
        let expand_motion_key = motion_key.clone();
        let mut children = Vec::new();
        if self.disclosure_motions.retained(&motion_key, expanded) {
            let mut terminal_ids = Vec::new();
            let mut tools = Vec::new();
            match session.target {
                Some(standalone_connections::StandaloneConnectionSurface::Terminal(id)) => {
                    terminal_ids.push(id);
                }
                Some(standalone_connections::StandaloneConnectionSurface::RemoteDesktop(id)) => {
                    tools.push(SessionActionSpec {
                        debug_key: "open-session",
                        icon: session.kind.icon(),
                        label: self.i18n.t("sessions.tree.actions.open_session"),
                        variant: SessionActionVariant::Primary,
                        listener: Box::new(cx.listener(move |this, _, window, cx| {
                            this.set_active_tab(id, window, cx);
                            cx.stop_propagation();
                        })),
                    });
                }
                None => {}
            }
            let lifecycle = standalone_session_actions(row.node_view.status())
                .into_iter()
                .flatten()
                .map(|action| {
                    let connection_id = session.connection_id.clone();
                    let (debug_key, icon, key, variant) = match action {
                        SessionNodeRowAction::Disconnect => (
                            "disconnect",
                            LucideIcon::WifiOff,
                            "sessions.tree.actions.disconnect",
                            SessionActionVariant::Danger,
                        ),
                        SessionNodeRowAction::Remove => (
                            "remove",
                            LucideIcon::Trash2,
                            "sessions.tree.actions.remove_session",
                            SessionActionVariant::Danger,
                        ),
                        _ => (
                            "reconnect",
                            LucideIcon::RefreshCw,
                            "sessions.actions.reconnect",
                            SessionActionVariant::Primary,
                        ),
                    };
                    SessionActionSpec {
                        debug_key,
                        icon,
                        label: self.i18n.t(key),
                        variant,
                        listener: Box::new(cx.listener(move |this, _, window, cx| {
                            match action {
                                SessionNodeRowAction::Disconnect => this
                                    .disconnect_standalone_connection(&connection_id, window, cx),
                                SessionNodeRowAction::Remove => {
                                    this.remove_standalone_connection(&connection_id, window, cx)
                                }
                                _ => {
                                    this.reconnect_standalone_connection(&connection_id, window, cx)
                                }
                            }
                            cx.stop_propagation();
                        })),
                    }
                })
                .collect();
            children = self.render_session_child_segments(
                &row.node_id.0,
                1,
                &[],
                false,
                &terminal_ids,
                tools,
                lifecycle,
                cx,
            );
        }
        let icon_selector = format!("session-header-icon-{}", row.node_id.0);
        let title_selector = format!("session-header-title-{}", row.node_id.0);
        let header = self
            .reorderable_session_row(
                self.session_sidebar_row(active, in_selection, node_height),
                row.node_id.clone(),
                None,
                row.title.clone(),
                cx,
            )
            .child(
                div()
                    .w(px(12.0))
                    .h(px(20.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.render_animated_chevron(
                        (
                            SharedString::from(format!(
                                "standalone-chevron-{}",
                                session.connection_id
                            )),
                            expanded as usize,
                        ),
                        expanded,
                        12.0,
                        rgb(theme.text_muted),
                    )),
            )
            .child(
                div()
                    .ml(px(6.0))
                    .w(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .debug_selector(move || icon_selector.clone())
                    // Tab ownership lights the standalone record's icon too.
                    .child(Self::render_lucide_icon(
                        session.kind.icon(),
                        SESSION_TREE_ICON_SIZE,
                        if tab_active {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    )),
            )
            .child(
                div()
                    .ml(px(6.0))
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .debug_selector(move || title_selector.clone())
                    .child(
                        div()
                            .truncate()
                            .text_size(px(text_size))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .when(serial_details.is_some(), |title| {
                                title.line_height(px(node_height / 2.0))
                            })
                            .text_color(rgb(theme.text))
                            .child(row.title),
                    )
                    .when_some(serial_details, |label, details| {
                        label.child(
                            div()
                                .truncate()
                                .text_size(px(meta_text_size))
                                .line_height(px(node_height / 2.0))
                                .text_color(rgb(theme.text_muted))
                                .child(details),
                        )
                    }),
            )
            .child(self.render_session_status_dot(status))
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                    let gpui::ClickEvent::Mouse(click) = event else {
                        return;
                    };
                    let modifiers = click.down.modifiers;
                    // Every standalone record joins the selection in any status;
                    // batch deletes still filter to removable records on execution.
                    let mode = if modifiers.control {
                        ActiveSessionSelectionMode::Toggle
                    } else if modifiers.shift {
                        ActiveSessionSelectionMode::Range
                    } else {
                        ActiveSessionSelectionMode::Single
                    };
                    this.apply_active_session_selection(
                        ActiveSessionSelectionTarget::Standalone(expand_id.clone()),
                        mode,
                        cx,
                    );
                    if mode != ActiveSessionSelectionMode::Single {
                        // Modifier clicks only steer the selection; expansion stays put.
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    if !this
                        .expanded_standalone_connections
                        .insert(expand_id.clone())
                    {
                        this.expanded_standalone_connections.remove(&expand_id);
                    }
                    this.begin_disclosure_motion(
                        expand_motion_key.clone(),
                        this.expanded_standalone_connections.contains(&expand_id),
                        cx,
                    );
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.open_active_session_selection_menu_if_selected(event.position, cx);
                }),
            );
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .when(!children.is_empty(), |row| {
                row.child(self.disclosure_motions.render(
                    &motion_key,
                    &self.tokens,
                    div().w_full().flex().flex_col().children(children),
                    None,
                ))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn can_save_runtime_node_as_connection(
        &self,
        node_id: &NodeId,
        saved_connection_id: Option<&str>,
    ) -> bool {
        if saved_connection_id.is_some() {
            return false;
        }
        let Some(snapshot) = self.node_router.node_metadata(node_id) else {
            return true;
        };
        // ManualPreset/Restored are already saved-connection materializations,
        // while legacy AutoRoute nodes came from derived topology. Only live
        // drill-down nodes and genuinely unsaved direct nodes expose this action.
        matches!(
            snapshot.origin,
            NodeOrigin::DrillDown { .. } | NodeOrigin::Direct
        )
    }

    pub(in crate::workspace) fn render_session_node_header(
        &self,
        node_id: NodeId,
        node: ActiveSessionNode,
        expanded: bool,
        selected: bool,
        accent_bar: bool,
        status: SessionStatusStyle,
        local_group: bool,
        local_profile_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let node_height = self.session_tree_node_height();
        let text_size = self.tokens.metrics.ui_text_xs;
        let meta_text_size = self.tokens.metrics.ui_text_2xs;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let muted_text = rgb(theme.text_muted);
        let row_text = rgb(theme.text);
        // Tab ownership lights the header icon regardless of the accent bar,
        // which now marks row selection. Local rows derive ownership from the
        // active terminal session; SSH nodes reuse the focused-node lookup.
        let owns_active_tab = if local_group {
            self.active_terminal_session_id(cx)
                .is_some_and(|id| node.terminal_ids.contains(&id))
        } else {
            self.session_node_row_is_focused(&node_id, cx)
        };
        let owned_tab_icon_color = if owns_active_tab {
            rgb(theme.accent)
        } else {
            muted_text
        };
        let port_text = format!(":{}", node.port);
        let terminal_count = node.terminal_ids.len();
        let chevron_node_id = node_id.clone();
        let chevron_is_unsaved_local_group = local_group && local_profile_id.is_none();
        let chevron_label = self.i18n.t(if expanded {
            "settings_view.tool_use_collapse"
        } else {
            "settings_view.tool_use_expand"
        });
        let icon_selector = format!("session-header-icon-{}", node_id.0);
        let title_selector = format!("session-header-title-{}", node_id.0);
        let leading_icon = if matches!(status.icon, LucideIcon::LoaderCircle) {
            self.render_connecting_session_icon(
                (
                    gpui::SharedString::from(format!("session-connecting-{node_id:?}")),
                    0usize,
                ),
                SESSION_TREE_ICON_SIZE,
                row_text,
                cx,
            )
        } else if local_group {
            self.local_profile_session_icon(local_profile_id.as_deref())
                .render(
                    SESSION_TREE_ICON_SIZE,
                    owned_tab_icon_color,
                    owns_active_tab,
                )
        } else {
            self.node_session_icon(&node_id).render(
                SESSION_TREE_ICON_SIZE,
                owned_tab_icon_color,
                owns_active_tab,
            )
        };
        self.reorderable_session_row(
            self.session_sidebar_row(selected, accent_bar, node_height),
            node_id.clone(),
            self.node_router
                .node_metadata(&node_id)
                .and_then(|node| node.parent_id),
            node.title.clone(),
            cx,
        )
        .child(
            div()
                .id(gpui::SharedString::from(format!(
                    "session-node-toggle-{}",
                    node_id.0
                )))
                .w(px(12.0))
                .h(px(20.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .role(gpui::Role::Button)
                .aria_label(chevron_label)
                .focusable()
                .tab_stop(true)
                .focus_visible(move |style| style.border_1().border_color(rgb(theme.accent)))
                .child(self.render_animated_chevron(
                    (
                        gpui::SharedString::from(format!("session-node-chevron-{}", node_id.0)),
                        expanded as usize,
                    ),
                    expanded,
                    12.0,
                    muted_text,
                ))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if cx.has_active_drag() {
                        cx.stop_propagation();
                        return;
                    }
                    let expanded = if chevron_is_unsaved_local_group {
                        this.local_session_group_expanded = !this.local_session_group_expanded;
                        this.local_session_group_expanded
                    } else if !this.expanded_ssh_nodes.insert(chevron_node_id.clone()) {
                        this.expanded_ssh_nodes.remove(&chevron_node_id);
                        false
                    } else {
                        true
                    };
                    this.begin_disclosure_motion(
                        format!("session:{}:children", chevron_node_id.0),
                        expanded,
                        cx,
                    );
                    cx.stop_propagation();
                    cx.notify();
                })),
        )
        .child(
            div()
                .ml(px(6.0))
                .w(px(14.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .debug_selector(move || icon_selector.clone())
                .child(leading_icon),
        )
        .child(
            div()
                .ml(px(6.0))
                .min_w(px(0.0))
                .flex_1()
                .truncate()
                .text_size(px(text_size))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(row_text)
                .debug_selector(move || title_selector.clone())
                .child(node.title),
        )
        .when(!local_group && node.port != 22, |row| {
            row.child(
                div()
                    .ml_2()
                    .flex_none()
                    .child(oxideterm_gpui_ui::monospace_datum(
                        &self.tokens,
                        port_text,
                        Some(mono_font_family.clone()),
                        oxideterm_gpui_ui::MonospaceDatumOptions::new(
                            oxideterm_gpui_ui::MonospaceDatumTone::Muted,
                        )
                        .text_size(meta_text_size),
                    )),
            )
        })
        .when(terminal_count > 0, |row| {
            row.child(
                div()
                    .ml_2()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(3.0))
                    .when(!local_group, |count| {
                        count.child(Self::render_lucide_icon(
                            LucideIcon::Terminal,
                            SESSION_TREE_CHILD_ICON_SIZE,
                            muted_text,
                        ))
                    })
                    .child(oxideterm_gpui_ui::monospace_datum(
                        &self.tokens,
                        terminal_count.to_string(),
                        Some(mono_font_family),
                        oxideterm_gpui_ui::MonospaceDatumOptions::new(
                            oxideterm_gpui_ui::MonospaceDatumTone::Muted,
                        )
                        .text_size(meta_text_size),
                    )),
            )
        })
        .child(self.render_session_status_dot(status))
        .when(local_profile_id.is_some(), |row| {
            row.child(
                div()
                    .ml(px(4.0))
                    .flex_none()
                    .child(Self::render_lucide_icon(
                        LucideIcon::ArrowUpDown,
                        12.0,
                        muted_text,
                    )),
            )
        })
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                let gpui::ClickEvent::Mouse(click) = event else {
                    return;
                };
                let modifiers = click.down.modifiers;
                // Every record and local row joins the selection in any status;
                // batch deletes still filter to removable records on execution.
                let mode = if modifiers.control {
                    ActiveSessionSelectionMode::Toggle
                } else if modifiers.shift {
                    ActiveSessionSelectionMode::Range
                } else {
                    ActiveSessionSelectionMode::Single
                };
                this.apply_active_session_selection(
                    ActiveSessionSelectionTarget::Node(node_id.clone()),
                    mode,
                    cx,
                );
                if mode != ActiveSessionSelectionMode::Single {
                    // Modifier clicks only steer the selection; expansion stays put.
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                let expanded = if local_profile_id.is_some() {
                    if !this.expanded_ssh_nodes.insert(node_id.clone()) {
                        this.expanded_ssh_nodes.remove(&node_id);
                    }
                    this.expanded_ssh_nodes.contains(&node_id)
                } else if local_group {
                    this.local_session_group_expanded = !this.local_session_group_expanded;
                    this.local_session_group_expanded
                } else {
                    // The accent bar follows tab ownership only; clicking the row
                    // toggles expansion without stealing the highlight.
                    if !this.expanded_ssh_nodes.insert(node_id.clone()) {
                        this.expanded_ssh_nodes.remove(&node_id);
                    }
                    this.expanded_ssh_nodes.contains(&node_id)
                };
                this.begin_disclosure_motion(
                    format!("session:{}:children", node_id.0),
                    expanded,
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.open_active_session_selection_menu_if_selected(event.position, cx);
            }),
        )
        .into_any_element()
    }

    fn render_connecting_session_icon(
        &self,
        id: impl Into<gpui::ElementId>,
        size: f32,
        color: gpui::Rgba,
        cx: &App,
    ) -> AnyElement {
        // Authentication waits on user input; background animation would keep
        // repainting the window even when the user has not entered anything.
        if self
            .connections
            .flow
            .read(cx)
            .has_keyboard_interactive_challenge()
        {
            return Self::render_lucide_icon(LucideIcon::LoaderCircle, size, color);
        }
        self.render_loading_icon(id, size, color)
    }

    pub(in crate::workspace) fn render_session_status_dot(
        &self,
        status: SessionStatusStyle,
    ) -> AnyElement {
        div()
            .ml(px(6.0))
            .size(px(6.0))
            .flex_none()
            .rounded_full()
            .bg(rgb(status.dot_color))
            .into_any_element()
    }

    fn session_terminal_label_icon(
        &self,
        session_id: TerminalSessionId,
        index: usize,
        cx: &App,
    ) -> (String, super::super::session_icons::SessionIcon) {
        let local_instance = self
            .tab_host
            .read(cx)
            .local_sessions
            .get(&session_id)
            .map(|instance| (instance.title.clone(), instance.profile_id.clone()));
        let text = local_instance
            .as_ref()
            .map(|(title, _)| format!("{title} #{}", session_id.0))
            .unwrap_or_else(|| {
                self.i18n
                    .t("sessions.tree.terminal")
                    .replace("{{number}}", &index.to_string())
            });
        let icon = local_instance
            .as_ref()
            .and_then(|(_, profile_id)| profile_id.as_deref())
            .and_then(|id| {
                self.connection_store
                    .local_terminal_profiles()
                    .iter()
                    .find(|profile| profile.id == id)
            })
            .and_then(|profile| {
                super::super::session_icons::session_icon_from_id(profile.icon.as_deref())
            })
            .unwrap_or(LucideIcon::Terminal.into());
        (text, icon)
    }

    pub(in crate::workspace) fn render_session_terminal_item_with_rails(
        &self,
        depth: usize,
        ancestor_continues: &[bool],
        line_stops_here: bool,
        session_id: TerminalSessionId,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let item_height = self.session_tree_item_height();
        let text_size = self.tokens.metrics.ui_text_xs;
        let active = self.active_terminal_session_id(cx) == Some(session_id);
        let (text, icon) = self.session_terminal_label_icon(session_id, index, cx);
        let icon_color = if active {
            rgb(theme.accent)
        } else {
            rgb(theme.text_muted)
        };
        let icon_selector = format!("session-terminal-icon-{}", session_id.0);
        let label_selector = format!("session-terminal-label-{}", session_id.0);

        self.render_session_tree_child_with_rails(
            depth,
            ancestor_continues,
            line_stops_here,
            session_item_surface(theme, active, SESSION_ROW_SELECTED_BG_ALPHA)
                .id(SharedString::from(format!(
                    "session-terminal-row-{}",
                    session_id.0
                )))
                .group("session-terminal-item")
                .relative()
                .w_full()
                .h(px(item_height))
                .pl(px(6.0))
                .pr(px(8.0))
                .gap(px(6.0))
                .flex()
                .flex_row()
                .items_center()
                .rounded(px(self.tokens.radii.sm))
                .cursor_pointer()
                .child(
                    div()
                        .w(px(14.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .debug_selector(move || icon_selector.clone())
                        .child(icon.render(SESSION_TREE_CHILD_ICON_SIZE, icon_color, active)),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_size(px(text_size))
                        .font_weight(if active {
                            gpui::FontWeight::MEDIUM
                        } else {
                            gpui::FontWeight::NORMAL
                        })
                        .text_color(rgb(theme.text))
                        .debug_selector(move || label_selector.clone())
                        .child(text),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "session-terminal-close-{}",
                            session_id.0
                        )))
                        .size(px(20.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.sm))
                        .opacity(0.0)
                        .group_hover("session-terminal-item", |button| button.opacity(0.75))
                        .hover(move |button| button.opacity(1.0).bg(rgb(theme.bg_hover)))
                        .child(Self::render_lucide_icon(
                            LucideIcon::X,
                            12.0,
                            rgb(theme.text_muted),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, window, cx| {
                                this.close_terminal_session(session_id, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation())),
                )
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        // Modifier clicks must not focus the terminal or wipe
                        // the active selection.
                        if event.modifiers.control || event.modifiers.shift {
                            cx.stop_propagation();
                            return;
                        }
                        this.focus_terminal_session(session_id, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_session_action_item_with_rails(
        &self,
        owner_key: &str,
        depth: usize,
        ancestor_continues: &[bool],
        line_stops_here: bool,
        debug_key: &'static str,
        icon: LucideIcon,
        label: String,
        variant: SessionActionVariant,
        active: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let item_height = self.session_tree_item_height();
        let text_size = self.tokens.metrics.ui_text_xs;
        let idle_label_color = mix_rgb(theme.text, theme.text_muted, 0.42);
        let idle_icon_color = theme.text_muted;
        let (hover_bg, hover_fg) = match variant {
            SessionActionVariant::Primary => (theme.bg_hover, theme.text),
            SessionActionVariant::Danger => {
                (mix_rgb(theme.bg_hover, theme.error, 0.14), theme.error)
            }
        };
        let row_selector = format!("session-action-row-{debug_key}");
        let icon_selector = format!("session-action-icon-{debug_key}");
        let label_selector = format!("session-action-label-{debug_key}");

        self.render_session_tree_child_with_rails(
            depth,
            ancestor_continues,
            line_stops_here,
            div()
                // Stateful id + row text_color ensures GPUI resolves hovered label
                // colors during request_layout, before GroupHitboxes::push runs in paint.
                .id(SharedString::from(format!(
                    "session-action-{owner_key}-{debug_key}"
                )))
                .group("session-action-row")
                .h(px(item_height))
                .w_full()
                .pl(px(6.0))
                .pr(px(8.0))
                .gap(px(6.0))
                .flex()
                .flex_row()
                .items_center()
                .relative()
                .rounded(px(self.tokens.radii.sm))
                .text_size(px(text_size))
                .text_color(rgb(if active { theme.text } else { idle_label_color }))
                .cursor_pointer()
                // The active SFTP/IDE/forwards row keeps the selected
                // background and accent icon; the rail is reserved for row
                // selection.
                .when(active, |row| {
                    row.bg(rgba((theme.accent << 8) | SESSION_ROW_SELECTED_BG_ALPHA))
                })
                .hover(move |row| row.bg(rgb(hover_bg)).text_color(rgb(hover_fg)))
                .debug_selector(move || row_selector.clone())
                .child(
                    div()
                        .w(px(14.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .debug_selector(move || icon_selector.clone())
                        .child(
                            svg()
                                .path(icon.path())
                                .size(px(SESSION_TREE_CHILD_ICON_SIZE))
                                // Mirror the terminal child rows: the icon takes
                                // the accent color while the row is active.
                                .text_color(rgb(if active {
                                    theme.accent
                                } else {
                                    idle_icon_color
                                }))
                                .group_hover("session-action-row", move |icon| {
                                    icon.text_color(rgb(hover_fg))
                                }),
                        ),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .debug_selector(move || label_selector.clone())
                        .child(label),
                )
                // Modifier clicks on the expanded rows must neither fire the
                // row's command nor wipe the active selection.
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        if event.modifiers.control || event.modifiers.shift {
                            cx.stop_propagation();
                            return;
                        }
                        listener(event, window, cx);
                        cx.stop_propagation();
                    },
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_session_tree_child_with_rails(
        &self,
        depth: usize,
        ancestor_continues: &[bool],
        line_stops_here: bool,
        child: AnyElement,
    ) -> AnyElement {
        tree_guide_child(
            &self.tokens,
            TreeGuideMetrics::session_tree(),
            depth,
            ancestor_continues,
            line_stops_here,
            child,
        )
    }

    pub(in crate::workspace) fn session_node_status(
        &self,
        status: ActiveSessionStatus,
    ) -> SessionStatusStyle {
        match status {
            ActiveSessionStatus::Connecting => SessionStatusStyle {
                icon: LucideIcon::LoaderCircle,
                dot_color: self.tokens.ui.info,
            },
            ActiveSessionStatus::Active | ActiveSessionStatus::Connected => SessionStatusStyle {
                icon: LucideIcon::Server,
                dot_color: self.tokens.ui.success,
            },
            ActiveSessionStatus::Error => SessionStatusStyle {
                icon: LucideIcon::WifiOff,
                dot_color: self.tokens.ui.error,
            },
            ActiveSessionStatus::Idle => SessionStatusStyle {
                icon: LucideIcon::Server,
                dot_color: self.tokens.ui.text_muted,
            },
        }
    }
}

#[cfg(test)]
mod terminal_open_tests {
    use super::*;
    use gpui::{Render, TestAppContext};

    struct ConnectingIconView {
        workspace: Entity<WorkspaceApp>,
    }

    impl Render for ConnectingIconView {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.workspace.update(cx, |workspace, cx| {
                workspace.render_connecting_session_icon(
                    "connecting-fixture",
                    16.0,
                    rgb(0xffffff),
                    cx,
                )
            })
        }
    }

    #[gpui::test]
    fn authentication_prompt_pauses_connecting_animation_until_dismissed(cx: &mut TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_AUTH_ANIMATION_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace storage is process-wide; a portable child keeps user data untouched.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            fs::hard_link(&executable, &child).unwrap();
            fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let settings_path = default_settings_path();
        assert!(settings_path.starts_with(PathBuf::from(fixture_dir)));
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();
        let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
        let (view, cx) = cx.add_window_view(move |window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            workspace.update(cx, |workspace, cx| {
                workspace.tokens.motion.enabled = true;
                workspace.connections.flow.update(cx, |flow, cx| {
                    flow.open_keyboard_interactive_challenge(
                        oxideterm_ssh::KeyboardInteractivePromptRequest {
                            flow_id: "authentication-fixture".into(),
                            name: String::new(),
                            instructions: String::new(),
                            prompts: vec![oxideterm_ssh::KeyboardInteractivePrompt {
                                prompt: "Verification code".into(),
                                echo: false,
                            }],
                            chained: true,
                        },
                        response_tx,
                        cx,
                    );
                });
            });
            ConnectingIconView { workspace }
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(
                window.simulate_next_frame(cx),
                0,
                "waiting for authentication must not schedule spinner frames"
            );
        });
        view.update(cx, |view, cx| {
            view.workspace.update(cx, |workspace, cx| {
                workspace.connections.flow.update(cx, |flow, cx| {
                    flow.cancel_keyboard_interactive_challenge(Duration::ZERO, cx);
                });
            });
            cx.notify();
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(
                window.simulate_next_frame(cx) > 0,
                "connecting animation must resume after dismissal"
            );
        });
    }

    #[test]
    fn node_row_actions_keep_disconnect_and_removal_separate() {
        use SessionNodeRowAction::*;
        for (status, expected) in [
            (ActiveSessionStatus::Active, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connected, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connecting, [None, None]),
            (ActiveSessionStatus::Error, [Some(Reconnect), Some(Remove)]),
            (ActiveSessionStatus::Idle, [Some(Connect), Some(Remove)]),
        ] {
            assert_eq!(session_node_row_actions(status, false), expected);
            assert_eq!(
                session_node_row_actions(status, true),
                [Some(CancelReconnect), None]
            );
        }
    }

    #[test]
    fn standalone_lifecycle_actions_do_not_remove_live_or_connecting_sessions() {
        use SessionNodeRowAction::*;
        for (status, expected) in [
            (ActiveSessionStatus::Active, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connected, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connecting, [Some(Disconnect), None]),
            (ActiveSessionStatus::Error, [Some(Reconnect), Some(Remove)]),
            (ActiveSessionStatus::Idle, [Some(Reconnect), Some(Remove)]),
        ] {
            assert_eq!(standalone_session_actions(status), expected);
        }
    }

    #[test]
    fn sidebar_terminal_uses_current_saved_command_or_temporary_node_command() {
        let node_id = NodeId::new("temporary");
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        router.upsert_node(
            node_id.clone(),
            SshConfig {
                post_connect_command: Some("cd /srv/original".into()),
                ..Default::default()
            },
        );
        let mut saved: oxideterm_connections::SavedConnection =
            serde_json::from_value(serde_json::json!({
                "id": "saved", "name": "Saved connection", "host": "example.com",
                "port": 22, "username": "ops", "auth": { "type": "password" },
                "created_at": "2026-01-01T00:00:00Z"
            }))
            .unwrap();
        for command in [Some("cd /srv/updated"), None] {
            saved.post_connect_command = command.map(str::to_owned);
            assert_eq!(
                sidebar_terminal_post_connect_command(Some(&saved), &router, &node_id).as_deref(),
                command,
            );
        }
        assert_eq!(
            sidebar_terminal_post_connect_command(None, &router, &node_id).as_deref(),
            Some("cd /srv/original"),
        );
    }

    #[test]
    fn inactive_cleanup_checks_runtime_descendants() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let parent = NodeId::new("parent");
        let child = NodeId::new("child");
        let terminal = NodeId::new("terminal");
        router.upsert_node(parent.clone(), SshConfig::default());
        let store = router.runtime_store();
        store
            .upsert_child_node(parent.clone(), child.clone(), SshConfig::default())
            .unwrap();
        store
            .upsert_child_node(child.clone(), terminal.clone(), SshConfig::default())
            .unwrap();
        let can_remove = |id| session_subtree_is_inactive(&router, id, |_| false);
        assert!(can_remove(&parent));

        router.bind_terminal_session(&terminal, "terminal").unwrap();
        assert!(!can_remove(&parent));
        assert!(!can_remove(&child));
        router.remove_runtime_subtree(&terminal);

        router.bind_sftp_session(&child, "sftp", None).unwrap();
        assert!(
            !can_remove(&parent),
            "an SFTP-only descendant protects its parent"
        );
        router.remove_runtime_subtree(&child);
        assert!(can_remove(&parent));
    }
}

#[cfg(test)]
mod sorting_tests {
    use super::*;

    fn row(id: &str, title: &str, parent: Option<&str>, ready: bool) -> ActiveSessionSidebarRow {
        ActiveSessionSidebarRow {
            node_id: NodeId::new(id),
            parent_id: parent.map(NodeId::new),
            saved_connection_id: None,
            title: title.into(),
            host: String::new(),
            username: String::new(),
            port: 22,
            node_view: ActiveSessionNode {
                id: id.into(),
                title: title.into(),
                port: 22,
                terminal_ids: Vec::new(),
                readiness: if ready {
                    ActiveSessionReadiness::Ready
                } else {
                    ActiveSessionReadiness::Disconnected
                },
            },
            depth: usize::from(parent.is_some()),
            is_last: false,
            has_children: false,
            local_group: false,
            local_profile_id: None,
            active_local_session_count: 0,
            standalone_session: None,
            group: None,
        }
    }

    #[test]
    fn manual_reorder_moves_siblings_with_their_subtrees() {
        let ids = |rows: &[ActiveSessionSidebarRow]| {
            rows.iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        let rows = vec![
            row("ssh", "SSH", None, true),
            row("child", "Child", Some("ssh"), true),
            row("mosh", "Mosh", None, true),
            row("last", "Last", None, false),
        ];
        let order =
            reordered_session_ids(&rows, &NodeId::new("ssh"), &NodeId::new("last")).unwrap();
        let sorted = sort_active_session_rows(rows.clone(), SessionSortOrder::Default, &order);
        assert_eq!(ids(&sorted), "mosh,last,ssh,child");
        assert_eq!(sorted[3].parent_id, Some(NodeId::new("ssh")));
        let order =
            reordered_session_ids(&sorted, &NodeId::new("ssh"), &NodeId::new("mosh")).unwrap();
        assert_eq!(
            ids(&sort_active_session_rows(
                sorted,
                SessionSortOrder::Default,
                &order
            )),
            "ssh,child,mosh,last"
        );
        assert_eq!(
            reordered_session_ids(&rows, &NodeId::new("child"), &NodeId::new("mosh")),
            None
        );
        assert_eq!(
            reordered_session_ids(&rows, &NodeId::new("missing"), &NodeId::new("mosh")),
            None
        );
        let order = vec!["mosh".to_string(), "ssh".to_string()];
        assert_eq!(
            ids(&sort_active_session_rows(
                rows,
                SessionSortOrder::Default,
                &order
            )),
            "mosh,ssh,child,last"
        );
    }

    #[test]
    fn active_connection_total_counts_connections_and_running_local_terminals() {
        let mut ssh = row("ssh", "SSH", None, true);
        ssh.node_view.terminal_ids = vec![TerminalSessionId(1), TerminalSessionId(2)];
        let sftp_only = row("sftp", "SFTP only", None, true);
        let jump_child = row("child", "Jump child", Some("ssh"), true);
        let mut rows = vec![ssh, sftp_only, jump_child];
        assert_eq!(active_connection_count(&rows), 3);
        for (index, kind) in [
            standalone_connections::StandaloneConnectionKind::Mosh,
            standalone_connections::StandaloneConnectionKind::Telnet,
            standalone_connections::StandaloneConnectionKind::Serial,
            standalone_connections::StandaloneConnectionKind::Rdp,
            standalone_connections::StandaloneConnectionKind::Vnc,
        ]
        .into_iter()
        .enumerate()
        {
            let id = format!("standalone-{index}");
            let mut standalone = row(&id, "Standalone", None, true);
            standalone.standalone_session = Some(StandaloneActiveSession {
                connection_id: id,
                kind,
                target: None,
            });
            rows.push(standalone);
            assert_eq!(active_connection_count(&rows), 4 + index, "{kind:?}");
        }
        let mut local = row("local", "Local Terminal", None, true);
        local.local_group = true;
        local.node_view.terminal_ids = vec![
            TerminalSessionId(3),
            TerminalSessionId(4),
            TerminalSessionId(5),
        ];
        local.active_local_session_count = 2;
        rows.push(local);
        assert_eq!(active_connection_count(&rows), 10);
        for readiness in [
            ActiveSessionReadiness::Disconnected,
            ActiveSessionReadiness::Error,
            ActiveSessionReadiness::Connecting,
        ] {
            let mut inactive = row("inactive", "Inactive", None, false);
            inactive.node_view.readiness = readiness;
            rows.push(inactive);
        }
        assert_eq!(active_connection_count(&rows), 10);
        rows[0].node_view.readiness = ActiveSessionReadiness::Disconnected;
        assert_eq!(active_connection_count(&rows), 9);
    }

    #[test]
    fn batch_delete_targets_only_cover_rows_with_a_remove_action() {
        let idle = row("idle", "Idle", None, false);
        assert_eq!(
            active_session_row_removal_target(&idle),
            Some(ActiveSessionSelectionTarget::Node(NodeId::new("idle")))
        );
        let mut errored = row("errored", "Errored", None, false);
        errored.node_view.readiness = ActiveSessionReadiness::Error;
        assert_eq!(
            active_session_row_removal_target(&errored),
            Some(ActiveSessionSelectionTarget::Node(NodeId::new("errored")))
        );
        // Jump children are their own records, so they stay selectable.
        let child = row("child", "Child", Some("parent"), false);
        assert_eq!(
            active_session_row_removal_target(&child),
            Some(ActiveSessionSelectionTarget::Node(NodeId::new("child")))
        );
        let mut active = row("active", "Active", None, true);
        active.node_view.terminal_ids = vec![TerminalSessionId(1)];
        assert_eq!(active_session_row_removal_target(&active), None);
        let mut connecting = row("connecting", "Connecting", None, false);
        connecting.node_view.readiness = ActiveSessionReadiness::Connecting;
        assert_eq!(active_session_row_removal_target(&connecting), None);
        let mut standalone_idle = row("mosh", "Mosh", None, false);
        standalone_idle.standalone_session = Some(StandaloneActiveSession {
            connection_id: "mosh".into(),
            kind: standalone_connections::StandaloneConnectionKind::Mosh,
            target: None,
        });
        assert_eq!(
            active_session_row_removal_target(&standalone_idle),
            Some(ActiveSessionSelectionTarget::Standalone("mosh".into()))
        );
        let mut standalone_connected = row("rdp", "Rdp", None, true);
        standalone_connected.standalone_session = Some(StandaloneActiveSession {
            connection_id: "rdp".into(),
            kind: standalone_connections::StandaloneConnectionKind::Rdp,
            target: None,
        });
        assert_eq!(
            active_session_row_removal_target(&standalone_connected),
            None
        );
        let mut group = row("host-group", "Host", None, false);
        group.group = Some(SessionGroupRow {
            user_level: false,
            label: "Host".into(),
            connection_name: None,
            members: Vec::new(),
        });
        assert_eq!(active_session_row_removal_target(&group), None);
        let mut local = row("local", "Local", None, false);
        local.local_group = true;
        assert_eq!(active_session_row_removal_target(&local), None);
    }

    #[test]
    fn shift_range_covers_the_contiguous_removable_span() {
        let target = |id: &str| ActiveSessionSelectionTarget::Node(NodeId::new(id));
        let visible = vec![target("a"), target("b"), target("c"), target("d")];
        assert_eq!(
            active_session_selection_range(&visible, &target("b"), &target("d")),
            Some(vec![target("b"), target("c"), target("d")])
        );
        assert_eq!(
            active_session_selection_range(&visible, &target("d"), &target("b")),
            Some(vec![target("b"), target("c"), target("d")])
        );
        assert_eq!(
            active_session_selection_range(&visible, &target("b"), &target("b")),
            Some(vec![target("b")])
        );
        assert_eq!(
            active_session_selection_range(&visible, &target("gone"), &target("b")),
            None
        );
        assert_eq!(
            active_session_selection_range(&visible, &target("b"), &target("gone")),
            None
        );
    }

    #[test]
    fn planned_batch_removals_follow_row_order_and_skip_stale_targets() {
        let idle = row("idle", "Idle", None, false);
        let mut live = row("live", "Live", None, true);
        live.node_view.terminal_ids = vec![TerminalSessionId(1)];
        let mut standalone_idle = row("mosh", "Mosh", None, false);
        standalone_idle.standalone_session = Some(StandaloneActiveSession {
            connection_id: "mosh".into(),
            kind: standalone_connections::StandaloneConnectionKind::Mosh,
            target: None,
        });
        let mut standalone_connected = row("rdp", "Rdp", None, true);
        standalone_connected.standalone_session = Some(StandaloneActiveSession {
            connection_id: "rdp".into(),
            kind: standalone_connections::StandaloneConnectionKind::Rdp,
            target: None,
        });
        let selection: HashSet<ActiveSessionSelectionTarget> = [
            ActiveSessionSelectionTarget::Node(NodeId::new("idle")),
            ActiveSessionSelectionTarget::Node(NodeId::new("live")),
            ActiveSessionSelectionTarget::Node(NodeId::new("ghost")),
            ActiveSessionSelectionTarget::Standalone("mosh".into()),
            ActiveSessionSelectionTarget::Standalone("rdp".into()),
        ]
        .into_iter()
        .collect();
        // Active sessions, a connected standalone record, and a target whose
        // row no longer exists must not reach the delete loop; the remaining
        // targets execute in render order.
        assert_eq!(
            planned_active_session_removals(
                &[live, idle, standalone_connected, standalone_idle],
                &selection,
            ),
            vec![
                ActiveSessionSelectionTarget::Node(NodeId::new("idle")),
                ActiveSessionSelectionTarget::Standalone("mosh".into()),
            ]
        );
        assert!(
            planned_active_session_removals(&[row("idle2", "Idle", None, false)], &HashSet::new(),)
                .is_empty()
        );
    }

    #[test]
    fn local_session_search_keeps_the_group_and_its_terminal_instances() {
        let mut local = row("local-terminal-group", "Local Terminal", None, true);
        local.local_group = true;
        local.host = "Zsh /projects Bash /tmp".into();
        local.node_view.terminal_ids = vec![TerminalSessionId(7), TerminalSessionId(8)];
        let remote = row("ssh", "Zsh server", None, true);

        let filtered = filter_active_session_rows(vec![remote, local], "local zsh");
        assert_eq!(
            filtered
                .iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["local-terminal-group"],
        );
        assert_eq!(
            filtered[0].node_view.terminal_ids,
            vec![TerminalSessionId(7), TerminalSessionId(8)],
        );
    }

    #[test]
    fn session_search_retains_ancestors_and_respects_sort_order() {
        let mut a = row("a", "Alpha", Some("parent"), true);
        a.host = "prod.example".into();
        a.username = "ops".into();
        let mut z = row("z", "Zulu", Some("parent"), false);
        z.host = "prod.example".into();
        z.username = "ops".into();
        let rows = vec![
            row("parent", "Gateway", None, true),
            z,
            a,
            row("other", "Unrelated", None, true),
        ];
        for (order, expected) in [
            (SessionSortOrder::NameAscending, vec!["parent", "a", "z"]),
            (SessionSortOrder::NameDescending, vec!["parent", "z", "a"]),
        ] {
            let sorted = sort_active_session_rows(rows.clone(), order, &[]);
            let filtered = filter_active_session_rows(sorted, " PROD ops ");
            assert_eq!(
                filtered
                    .iter()
                    .map(|row| row.node_id.0.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(filtered[0].has_children);
            assert!(!filtered[1].is_last);
            assert!(filtered[2].is_last);
        }
        assert_eq!(
            filter_active_session_rows(rows.clone(), " ")
                .iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["parent", "z", "a", "other"]
        );
        assert!(filter_active_session_rows(rows, "missing").is_empty());
    }

    #[test]
    fn session_sort_preserves_subtrees_and_updates_branch_ends() {
        let rows = vec![
            row("parent", "Zulu", None, true),
            row("z", "zeta", Some("parent"), false),
            row("a", "Alpha", Some("parent"), true),
            row("other", "Beta", None, false),
        ];
        let ids = |rows: &[ActiveSessionSidebarRow]| {
            rows.iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        assert_eq!(
            ids(&sort_active_session_rows(
                rows.clone(),
                SessionSortOrder::Default,
                &[]
            )),
            "parent,z,a,other"
        );
        let sorted = sort_active_session_rows(rows.clone(), SessionSortOrder::NameAscending, &[]);
        assert_eq!(ids(&sorted), "other,parent,a,z");
        assert_eq!(
            sorted
                .iter()
                .map(|row| (row.depth, row.is_last))
                .collect::<Vec<_>>(),
            [(0, false), (0, true), (1, false), (1, true)]
        );
        assert_eq!(
            ids(&sort_active_session_rows(
                rows.clone(),
                SessionSortOrder::NameDescending,
                &[]
            )),
            "parent,z,a,other"
        );
        assert_eq!(
            ids(&sort_active_session_rows(
                rows,
                SessionSortOrder::ConnectedFirst,
                &[]
            )),
            "parent,a,z,other"
        );
    }
}

#[derive(Clone)]
struct ActiveSessionDrag {
    id: NodeId,
    parent: Option<NodeId>,
    title: String,
    tokens: ThemeTokens,
}

impl Render for ActiveSessionDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .text_color(rgb(self.tokens.ui.text))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .child(self.title.clone())
    }
}

fn reordered_session_ids(
    rows: &[ActiveSessionSidebarRow],
    source: &NodeId,
    target: &NodeId,
) -> Option<Vec<String>> {
    if source == target {
        return None;
    }
    let source_row = rows.iter().find(|row| &row.node_id == source)?;
    let target_row = rows.iter().find(|row| &row.node_id == target)?;
    if source_row.parent_id != target_row.parent_id {
        return None;
    }
    let mut siblings: Vec<_> = rows
        .iter()
        .filter(|row| row.parent_id == source_row.parent_id)
        .map(|row| row.node_id.0.clone())
        .collect();
    let from = siblings.iter().position(|id| id == &source.0)?;
    let to = siblings.iter().position(|id| id == &target.0)?;
    let moved = siblings.remove(from);
    siblings.insert(to, moved);
    let mut reordered = siblings.into_iter();
    Some(
        rows.iter()
            .map(|row| {
                if row.parent_id == source_row.parent_id {
                    reordered.next().unwrap()
                } else {
                    row.node_id.0.clone()
                }
            })
            .collect(),
    )
}

impl WorkspaceApp {
    fn reorderable_session_row(
        &self,
        row: Div,
        id: NodeId,
        parent: Option<NodeId>,
        title: String,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let drag = ActiveSessionDrag {
            id: id.clone(),
            parent: parent.clone(),
            title,
            tokens: self.tokens,
        };
        let drop_id = id.clone();
        let accent = self.tokens.ui.accent;
        let base = row.id(SharedString::from(format!("session-reorder-{}", id.0)));
        if self.active_session_sidebar_view_mode == ActiveSessionSidebarViewMode::Host {
            // Host groups are derived ordering; dragging here would persist
            // synthetic group parents into session_manual_order.
            return base;
        }
        base.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .can_drop(move |value, _, _| {
                value
                    .downcast_ref::<ActiveSessionDrag>()
                    .is_some_and(|drag| drag.parent == parent && drag.id != id)
            })
            .drag_over::<ActiveSessionDrag>(move |style, _, _, _| {
                style.bg(rgba((accent << 8) | 0x26))
            })
            .on_drop(cx.listener(move |this, drag: &ActiveSessionDrag, _, cx| {
                let rows = sort_active_session_rows(
                    this.unfiltered_active_session_sidebar_rows(cx),
                    this.settings_store.settings().sidebar_ui.session_sort_order,
                    &this
                        .settings_store
                        .settings()
                        .sidebar_ui
                        .session_manual_order,
                );
                if let Some(order) = reordered_session_ids(&rows, &drag.id, &drop_id) {
                    let sidebar = &mut this.settings_store.settings_mut().sidebar_ui;
                    sidebar.session_manual_order = order;
                    sidebar.session_sort_order = SessionSortOrder::Default;
                    this.persist_sidebar_settings(cx);
                    cx.notify();
                }
                cx.stop_propagation();
            }))
    }
}

#[cfg(test)]
mod host_view_tests {
    use super::*;

    fn session_row(
        id: &str,
        title: &str,
        host: &str,
        username: &str,
        readiness: ActiveSessionReadiness,
    ) -> ActiveSessionSidebarRow {
        ActiveSessionSidebarRow {
            node_id: NodeId::new(id),
            parent_id: None,
            saved_connection_id: None,
            title: title.into(),
            host: host.into(),
            username: username.into(),
            port: 22,
            node_view: ActiveSessionNode {
                id: id.into(),
                title: title.into(),
                port: 22,
                terminal_ids: Vec::new(),
                readiness,
            },
            depth: 0,
            is_last: false,
            has_children: false,
            local_group: false,
            local_profile_id: None,
            active_local_session_count: 0,
            standalone_session: None,
            group: None,
        }
    }

    fn ids(rows: &[ActiveSessionSidebarRow]) -> Vec<String> {
        rows.iter().map(|row| row.node_id.0.clone()).collect()
    }

    #[test]
    fn host_view_groups_by_host_then_user() {
        let mut standalone = session_row(
            "standalone-connection-1",
            "Serial",
            "",
            "",
            ActiveSessionReadiness::Ready,
        );
        standalone.standalone_session = Some(StandaloneActiveSession {
            connection_id: "1".into(),
            kind: standalone_connections::StandaloneConnectionKind::Serial,
            target: None,
        });
        let mut local = session_row(
            "local-terminal-group",
            "Local",
            "",
            "",
            ActiveSessionReadiness::Ready,
        );
        local.local_group = true;

        let grouped = group_active_session_rows_by_host_user(vec![
            session_row(
                "node-a",
                "root@HKTkosl1102",
                "HKTkosl1102",
                "root",
                ActiveSessionReadiness::Ready,
            ),
            session_row(
                "node-b",
                "root@hktkosl1102",
                "hktkosl1102",
                "root",
                ActiveSessionReadiness::Ready,
            ),
            session_row(
                "node-c",
                "admin@hktkosl1102",
                "hktkosl1102",
                "admin",
                ActiveSessionReadiness::Disconnected,
            ),
            // Unix usernames are case-sensitive: `Root` is a different account
            // than `root` and must not be folded into the same user row.
            session_row(
                "node-d",
                "Root@hktkosl1102",
                "hktkosl1102",
                "Root",
                ActiveSessionReadiness::Disconnected,
            ),
            session_row(
                "node-empty-host",
                "orphan",
                "",
                "root",
                ActiveSessionReadiness::Disconnected,
            ),
            session_row(
                "node-anon",
                "anon",
                "other-host",
                "",
                ActiveSessionReadiness::Disconnected,
            ),
            standalone,
            local,
        ]);

        assert_eq!(
            ids(&grouped),
            vec![
                "session-host-group:hktkosl1102",
                "session-host-group:hktkosl1102\u{1}root",
                "session-host-group:hktkosl1102\u{1}admin",
                "session-host-group:hktkosl1102\u{1}Root",
                "session-host-group:other-host",
                "session-host-group:other-host\u{1}",
                "node-empty-host",
                "standalone-connection-1",
                "local-terminal-group",
            ]
        );

        let host = &grouped[0];
        let host_group = host.group.as_ref().unwrap();
        assert!(!host_group.user_level);
        assert_eq!(host_group.label, "HKTkosl1102");
        assert_eq!(
            host_group.connection_name, None,
            "host rows span connections and never name one"
        );
        assert_eq!(host.depth, 0);
        assert!(host.has_children);
        assert!(!host.is_last);

        let root = &grouped[1];
        assert_eq!(root.parent_id, Some(host.node_id.clone()));
        assert_eq!(root.depth, 1);
        assert!(!root.is_last);
        let root_group = root.group.as_ref().unwrap();
        assert!(root_group.user_level);
        assert_eq!(root_group.label, "root");
        assert_eq!(
            root_group
                .members
                .iter()
                .map(|member| (member.node_id.0.as_str(), member.host.as_str()))
                .collect::<Vec<_>>(),
            vec![("node-a", "HKTkosl1102"), ("node-b", "hktkosl1102")]
        );
        assert_eq!(root_group.members[1].title, "root@hktkosl1102");
        assert_eq!(
            root_group.connection_name, None,
            "nodes created without a saved connection have no name to show"
        );

        let admin = &grouped[2];
        assert_eq!(admin.parent_id, Some(host.node_id.clone()));
        assert!(!admin.is_last);
        assert_eq!(
            ids(&admin.group.as_ref().unwrap().members),
            vec!["node-c".to_string()]
        );
        let case_sensitive = &grouped[3];
        assert_eq!(case_sensitive.group.as_ref().unwrap().label, "Root");
        assert!(case_sensitive.is_last);

        let anon = &grouped[5];
        assert_eq!(anon.group.as_ref().unwrap().label, "—");
        assert_eq!(anon.username, "");

        assert_eq!(
            ids(&grouped[6..]),
            vec![
                "node-empty-host",
                "standalone-connection-1",
                "local-terminal-group"
            ]
        );
        for row in &grouped[6..8] {
            assert!(row.group.is_none());
            assert!(row.parent_id.is_none());
            assert_eq!(row.depth, 0);
            assert!(!row.is_last);
        }
        assert!(grouped[8].group.is_none());
        assert!(grouped[8].is_last);
    }

    #[test]
    fn host_view_user_row_names_its_saved_connections() {
        let saved = |id: &str, title: &str, connection: &str| {
            let mut row = session_row(
                id,
                title,
                "prod.example",
                "root",
                ActiveSessionReadiness::Ready,
            );
            row.saved_connection_id = Some(connection.into());
            row
        };
        // The same saved connection opened twice must not repeat its name, and a
        // second connection on the same host and user must still be reported.
        let grouped = group_active_session_rows_by_host_user(vec![
            saved("node-web", "prod-web", "conn-web"),
            saved("node-db", "prod-db", "conn-db"),
            saved("node-web-again", "prod-web", "conn-web"),
            session_row(
                "node-manual",
                "root@prod.example",
                "prod.example",
                "root",
                ActiveSessionReadiness::Disconnected,
            ),
        ]);

        assert_eq!(
            ids(&grouped),
            vec![
                "session-host-group:prod.example",
                "session-host-group:prod.example\u{1}root",
            ]
        );
        assert_eq!(
            grouped[1]
                .group
                .as_ref()
                .unwrap()
                .connection_name
                .as_deref(),
            Some("prod-web · prod-db")
        );
    }

    #[test]
    fn host_view_default_order_keeps_groups_before_pass_through_rows() {
        // A dragged local row can sit in `session_manual_order`, which ranks real
        // ids only. Default ordering must still keep every pass-through row after
        // the host groups instead of letting its rank win.
        let mut local = session_row(
            "local-terminal-group",
            "Local",
            "",
            "",
            ActiveSessionReadiness::Ready,
        );
        local.local_group = true;
        let ordered = arrange_active_session_group_rows(
            vec![
                local,
                session_row(
                    "node-a",
                    "root@z-host",
                    "z-host",
                    "root",
                    ActiveSessionReadiness::Ready,
                ),
            ],
            SessionSortOrder::Default,
            &["local-terminal-group".to_string()],
        );
        assert_eq!(
            ids(&ordered),
            vec![
                "session-host-group:z-host",
                "session-host-group:z-host\u{1}root",
                "local-terminal-group",
            ]
        );
        assert_eq!(ordered[1].depth, 1);
        assert_eq!(ordered[2].depth, 0);
        assert!(ordered[2].is_last);
    }

    #[test]
    fn host_view_group_ids_stay_disjoint_from_real_node_ids() {
        // Restored snapshots are unvalidated strings, so a real node can already
        // own the id a host group would generate.
        let colliding = session_row(
            "session-host-group:prod",
            "real node",
            "elsewhere.test",
            "root",
            ActiveSessionReadiness::Ready,
        );
        let grouped = group_active_session_rows_by_host_user(vec![
            colliding,
            session_row(
                "node-prod",
                "root@prod",
                "prod",
                "root",
                ActiveSessionReadiness::Ready,
            ),
        ]);
        assert_eq!(
            ids(&grouped),
            vec![
                "session-host-group:elsewhere.test",
                "session-host-group:elsewhere.test\u{1}root",
                "session-host-group:prod#2",
                "session-host-group:prod\u{1}root",
            ]
        );
        // The real node keeps its own id; the synthetic host for `prod` had to
        // pick a different one.
        assert_eq!(
            grouped[1].group.as_ref().unwrap().members[0].node_id.0,
            "session-host-group:prod"
        );
        assert_ne!(grouped[2].node_id.0, "session-host-group:prod");
    }

    #[test]
    fn host_view_sorts_groups_by_name() {
        let sorted = sort_active_session_rows(
            group_active_session_rows_by_host_user(vec![
                session_row(
                    "b-node",
                    "b",
                    "b-host",
                    "root",
                    ActiveSessionReadiness::Disconnected,
                ),
                session_row(
                    "a-zed",
                    "z",
                    "a-host",
                    "zed",
                    ActiveSessionReadiness::Disconnected,
                ),
                session_row(
                    "a-bob",
                    "b",
                    "a-host",
                    "bob",
                    ActiveSessionReadiness::Disconnected,
                ),
            ]),
            SessionSortOrder::NameAscending,
            &[],
        );
        assert_eq!(
            ids(&sorted),
            vec![
                "session-host-group:a-host",
                "session-host-group:a-host\u{1}bob",
                "session-host-group:a-host\u{1}zed",
                "session-host-group:b-host",
                "session-host-group:b-host\u{1}root",
            ]
        );
        assert_eq!(sorted[1].depth, 1);
        assert_eq!(sorted[2].depth, 1);
        assert!(sorted[4].is_last);
    }

    #[test]
    fn host_view_connected_first_uses_aggregate_readiness() {
        let sorted = sort_active_session_rows(
            group_active_session_rows_by_host_user(vec![
                session_row(
                    "down-node",
                    "down",
                    "down-host",
                    "root",
                    ActiveSessionReadiness::Disconnected,
                ),
                // The healthy member sits between two disconnected ones so a
                // first-member or last-member shortcut cannot pass this case.
                session_row(
                    "up-idle",
                    "up",
                    "up-host",
                    "root",
                    ActiveSessionReadiness::Disconnected,
                ),
                session_row(
                    "up-ready",
                    "up",
                    "up-host",
                    "root",
                    ActiveSessionReadiness::Ready,
                ),
                session_row(
                    "up-admin",
                    "up",
                    "up-host",
                    "admin",
                    ActiveSessionReadiness::Disconnected,
                ),
            ]),
            SessionSortOrder::ConnectedFirst,
            &[],
        );
        assert_eq!(
            ids(&sorted),
            vec![
                "session-host-group:up-host",
                "session-host-group:up-host\u{1}root",
                "session-host-group:up-host\u{1}admin",
                "session-host-group:down-host",
                "session-host-group:down-host\u{1}root",
            ]
        );
    }
}

#[cfg(test)]
mod selection_prune_tests {
    use super::*;
    use crate::workspace::session_manager::tests::isolated_workspace_fixture;
    use gpui::TestAppContext;

    fn ssh_config(host: &str) -> SshConfig {
        SshConfig {
            host: host.into(),
            username: "test".into(),
            auth: oxideterm_ssh::AuthMethod::Agent,
            ..Default::default()
        }
    }

    #[gpui::test]
    fn search_filter_does_not_prune_hidden_rows(cx: &mut TestAppContext) {
        let Some(_dir) = isolated_workspace_fixture(cx, "OXIDETERM_SELECTION_PRUNE_DIR") else {
            return;
        };
        let mut settings = SettingsStore::load_from_path(default_settings_path()).unwrap();
        settings.settings_mut().onboarding_completed = true;
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.save().unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let ws = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(ws, window, cx)
        });
        let ws = shell.read_with(cx, |shell, _| shell.session_entity());
        ws.update(cx, |ws, cx| {
            let alpha =
                ws.materialize_ssh_root_node(ssh_config("alpha.invalid"), "Alpha".into(), None);
            let beta =
                ws.materialize_ssh_root_node(ssh_config("beta.invalid"), "Beta".into(), None);
            let target = |id: &NodeId| ActiveSessionSelectionTarget::Node(id.clone());
            ws.active_session_selection = HashSet::from([target(&alpha), target(&beta)]);
            ws.active_session_selection_anchor = Some(target(&alpha));
            // The query hides the Alpha row; pruning must still treat it as
            // live because the unfiltered row list is the ownership source.
            ws.session_search_query = "beta".into();
            ws.prune_active_session_selection(cx);
            assert!(
                ws.active_session_selection.contains(&target(&alpha)),
                "search must not deselect a hidden row"
            );
            assert!(ws.active_session_selection.contains(&target(&beta)));
            assert_eq!(
                ws.active_session_selection_anchor.as_ref(),
                Some(&target(&alpha))
            );
        });
    }
}

#[cfg(test)]
mod host_view_render_tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn hover_highlights_only_unselected_sessions(cx: &mut TestAppContext) {
        struct Rows(AppUiColors);
        impl gpui::Render for Rows {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let rows = [true, false].map(|selected| {
                    session_item_surface(self.0, selected, SESSION_ROW_SELECTED_BG_ALPHA)
                        .w(px(100.0))
                        .h(px(24.0))
                });
                div().flex().flex_col().children(rows)
            }
        }

        let theme = ThemeTokens::from_builtin(oxideterm_theme::theme_by_id("default")).ui;
        let (_, cx) = cx.add_window_view(|_, _| Rows(theme));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let accent = rgba((theme.accent << 8) | SESSION_ROW_SELECTED_BG_ALPHA).into();
        let hover = rgb(theme.bg_hover).into();
        for (y, expect_hover) in [(36.0, true), (12.0, false)] {
            cx.simulate_mouse_move(point(px(50.0), px(y)), None, gpui::Modifiers::default());
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                let quads = window.painted_quads();
                let painted = |color| quads.iter().any(|q| q.background.as_solid() == Some(color));
                assert!(painted(accent), "selected row lost its tint at y={y}");
                assert_eq!(painted(hover), expect_hover, "hover background at y={y}");
            });
        }
    }

    fn isolated_fixture(cx: &mut TestAppContext, fixture_key: &str) -> Option<std::path::PathBuf> {
        isolated_fixture_named(cx.test_function_name().unwrap(), fixture_key)
    }

    fn isolated_fixture_named(test_name: &str, fixture_key: &str) -> Option<std::path::PathBuf> {
        let executable = std::env::current_exe().unwrap();
        if let Some(fixture_dir) = std::env::var_os(fixture_key) {
            let fixture_dir = std::path::PathBuf::from(fixture_dir);
            assert!(default_settings_path().starts_with(&fixture_dir));
            return Some(fixture_dir);
        }
        let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
        let child = directory.path().join(executable.file_name().unwrap());
        std::fs::hard_link(&executable, &child).unwrap();
        std::fs::write(directory.path().join("portable"), []).unwrap();
        let output = std::process::Command::new(child)
            .arg(test_name)
            .arg("--nocapture")
            .env(fixture_key, directory.path())
            .env_remove("APPIMAGE")
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        None
    }

    #[gpui::test]
    fn host_view_renders_grouped_session_rows(cx: &mut TestAppContext) {
        let Some(_fixture_dir) = isolated_fixture(cx, "OXIDETERM_HOST_VIEW_SMOKE_DIR") else {
            return;
        };
        let settings_path = default_settings_path();
        let mut store = SettingsStore::load_from_path(settings_path).unwrap();
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.settings_mut().onboarding_completed = true;
        store.settings_mut().general.language = oxideterm_settings::Language::En;
        store.save().unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.run_until_parked();

        workspace.update(cx, |workspace, cx| {
            for (id, username, port, saved_connection_id, title) in [
                ("smoke-root", "root", 22, Some("smoke-conn"), "Smoke Prod"),
                (
                    "smoke-alt",
                    "root",
                    2222,
                    Some("smoke-alt-conn"),
                    "Smoke Prod",
                ),
                ("smoke-admin", "admin", 22, None, "admin@smoke.invalid"),
            ] {
                let config = SshConfig {
                    host: "smoke.invalid".into(),
                    port,
                    username: username.into(),
                    ..Default::default()
                };
                let node_id = NodeId::new(id);
                workspace
                    .node_router
                    .upsert_node(node_id.clone(), config.clone());
                workspace.ssh_nodes.insert(
                    node_id,
                    WorkspaceSshNode::new(
                        saved_connection_id.map(str::to_string),
                        &config,
                        title.to_string(),
                        Vec::new(),
                        NodeReadiness::Disconnected,
                    ),
                );
            }
            workspace.toggle_active_session_sidebar_view(cx);
        });

        let rows = workspace.read_with(cx, |workspace, cx| {
            workspace.active_session_sidebar_display_rows(cx)
        });
        assert_eq!(
            rows.iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>(),
            vec![
                "session-host-group:smoke.invalid",
                "session-host-group:smoke.invalid\u{1}root",
                "session-host-group:smoke.invalid\u{1}admin",
            ]
        );
        assert_eq!(
            rows[1]
                .group
                .as_ref()
                .unwrap()
                .members
                .iter()
                .map(|member| (member.node_id.0.as_str(), member.port))
                .collect::<Vec<_>>(),
            [("smoke-root", 22), ("smoke-alt", 2222)]
        );
        assert_eq!(
            rows[1].group.as_ref().unwrap().connection_name.as_deref(),
            Some("Smoke Prod")
        );
        assert_eq!(
            rows[2].group.as_ref().unwrap().connection_name,
            None,
            "a node without a saved connection has no name to show"
        );

        let draw_sidebar = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        };
        // The sidebar list state is only published by the content renderer, so a
        // matching count proves the grouped rows reached the virtual list.
        let rendered_items = |cx: &mut gpui::VisualTestContext| {
            workspace.read_with(cx, |workspace, _| {
                workspace.active_session_sidebar_list_state.item_count()
            })
        };
        // The option stack is the only observable that proves the user row
        // actually rendered its actions, not just that a hash changed.
        let options_bounds =
            |cx: &mut gpui::VisualTestContext| cx.debug_bounds("session-group-options");
        // Host headers open by default, user rows stay closed until asked.
        draw_sidebar(cx);
        assert_eq!(rendered_items(cx), 3, "host header plus two user rows");
        assert!(
            cx.debug_bounds("session-group-connection-name").is_some(),
            "the saved connection name must render on the user row"
        );
        assert!(
            options_bounds(cx).is_none(),
            "a collapsed user row must not render its option stack"
        );

        workspace.update(cx, |workspace, _| {
            workspace.set_session_group_expanded(
                NodeId::new("session-host-group:smoke.invalid"),
                false,
                true,
            );
        });
        draw_sidebar(cx);
        assert_eq!(rendered_items(cx), 1, "collapsed host hides its user rows");

        workspace.update(cx, |workspace, _| {
            workspace.set_session_group_expanded(
                NodeId::new("session-host-group:smoke.invalid"),
                true,
                true,
            );
            workspace.set_session_group_expanded(
                NodeId::new("session-host-group:smoke.invalid\u{1}root"),
                true,
                false,
            );
        });
        draw_sidebar(cx);
        assert_eq!(
            rendered_items(cx),
            3,
            "expanded host restores its user rows"
        );
        let expanded = options_bounds(cx).expect("expanded user row renders its option stack");
        assert!(
            expanded.size.height > px(0.0),
            "the option stack must occupy height, got {expanded:?}"
        );
        workspace.update(cx, |workspace, _| {
            for id in ["smoke-root", "smoke-alt"] {
                workspace.ssh_nodes.get_mut(&NodeId::new(id)).unwrap().title =
                    "Smoke Prod with a very long connection name ".repeat(8);
            }
        });
        draw_sidebar(cx);
        for selector in [
            "session-group-member-port-smoke-root",
            "session-group-member-port-smoke-alt",
        ] {
            let port = cx
                .debug_bounds(selector)
                .expect("each member must expose its port beside its title");
            assert!(port.size.width > px(0.0) && port.size.height > px(0.0));
            assert!(
                port.right() <= expanded.right(),
                "port must remain visible inside the option stack"
            );
        }

        workspace.update(cx, |workspace, _| {
            workspace.set_session_group_expanded(
                NodeId::new("session-host-group:smoke.invalid\u{1}root"),
                false,
                false,
            );
        });
        draw_sidebar(cx);
        assert!(
            options_bounds(cx).is_none(),
            "collapsing the user row removes its option stack"
        );

        // The view action must take one click in either direction, regardless
        // of the host disclosure state.
        workspace.update(cx, |workspace, cx| {
            workspace.toggle_active_session_sidebar_view(cx)
        });
        draw_sidebar(cx);
        assert!(cx.debug_bounds("session-group-connection-name").is_none());
        workspace.read_with(cx, |workspace, cx| {
            let mut ids = workspace
                .active_session_sidebar_display_rows(cx)
                .into_iter()
                .map(|row| row.node_id.0)
                .collect::<Vec<_>>();
            ids.sort_unstable();
            assert_eq!(ids, ["smoke-admin", "smoke-alt", "smoke-root"]);
        });
        workspace.update(cx, |workspace, cx| {
            workspace.toggle_active_session_sidebar_view(cx)
        });

        // The final jump endpoint is identical in both private networks. Only the
        // complete ancestry distinguishes these targets from each other and from direct SSH.
        workspace.update(cx, |workspace, _| {
            for (branch, gateway) in [("a", "jump-a.invalid"), ("b", "jump-b.invalid")] {
                for (id, parent, username, host, port) in [
                    (format!("jump-{branch}"), None, "ops", gateway, 2201),
                    (
                        format!("bridge-{branch}"),
                        Some(format!("jump-{branch}")),
                        "relay",
                        "2001:db8::1",
                        22,
                    ),
                    (
                        format!("smoke-via-{branch}"),
                        Some(format!("bridge-{branch}")),
                        "root",
                        "smoke.invalid",
                        22,
                    ),
                ] {
                    let config = SshConfig {
                        host: host.into(),
                        port,
                        username: username.into(),
                        ..Default::default()
                    };
                    let node_id = NodeId::new(id);
                    if let Some(parent) = parent {
                        workspace
                            .node_router
                            .runtime_store()
                            .upsert_child_node(NodeId::new(parent), node_id.clone(), config.clone())
                            .unwrap();
                    } else {
                        workspace
                            .node_router
                            .upsert_node(node_id.clone(), config.clone());
                    }
                    workspace.ssh_nodes.insert(
                        node_id,
                        WorkspaceSshNode::new(
                            None,
                            &config,
                            "Smoke Prod".into(),
                            Vec::new(),
                            NodeReadiness::Disconnected,
                        ),
                    );
                }
            }
            workspace.set_session_group_expanded(
                NodeId::new("session-host-group:smoke.invalid\u{1}root"),
                true,
                false,
            );
        });
        let before_signature = workspace.read_with(cx, |workspace, cx| {
            let row = workspace
                .active_session_sidebar_display_rows(cx)
                .into_iter()
                .find(|row| row.node_id.0 == "session-host-group:smoke.invalid\u{1}root")
                .unwrap();
            let members = &row.group.as_ref().unwrap().members;
            let route = |id: &str| {
                workspace.session_group_member_route(
                    members
                        .iter()
                        .find(|member| member.node_id.0 == id)
                        .unwrap(),
                )
            };
            assert_eq!(route("smoke-root"), None);
            assert_eq!(
                route("smoke-via-a").as_deref(),
                Some("ops@jump-a.invalid:2201 → relay@[2001:db8::1]:22 → root@smoke.invalid:22")
            );
            assert_eq!(
                route("smoke-via-b").as_deref(),
                Some("ops@jump-b.invalid:2201 → relay@[2001:db8::1]:22 → root@smoke.invalid:22")
            );
            workspace.active_session_sidebar_row_signature(
                &row,
                &workspace.active_session_sidebar_display_rows(cx),
                ActiveSessionSidebarViewMode::Host,
                cx,
            )
        });
        draw_sidebar(cx);
        for selector in [
            "session-group-member-route-smoke-via-a",
            "session-group-member-route-smoke-via-b",
        ] {
            let route = cx
                .debug_bounds(selector)
                .expect("the jump path must be visible on its member row");
            assert!(route.size.width > px(0.0) && route.size.height > px(0.0));
        }
        assert!(
            cx.debug_bounds("session-group-member-route-smoke-root")
                .is_none()
        );

        // Changing an ancestor endpoint must invalidate the child group's cached row even
        // though the member id, immediate parent, target endpoint, and title are unchanged.
        workspace.update(cx, |workspace, _| {
            let id = NodeId::new("jump-a");
            let config = SshConfig {
                host: "renamed.invalid".into(),
                port: 2202,
                username: "ops".into(),
                ..Default::default()
            };
            workspace
                .node_router
                .upsert_node(id.clone(), config.clone());
            workspace.ssh_nodes.get_mut(&id).unwrap().endpoint =
                WorkspaceSshNodeEndpoint::from_config(&config);
        });
        workspace.read_with(cx, |workspace, cx| {
            let row = workspace
                .active_session_sidebar_display_rows(cx)
                .into_iter()
                .find(|row| row.node_id.0 == "session-host-group:smoke.invalid\u{1}root")
                .unwrap();
            let member = row
                .group
                .as_ref()
                .unwrap()
                .members
                .iter()
                .find(|member| member.node_id.0 == "smoke-via-a")
                .unwrap();
            assert_eq!(
                workspace.session_group_member_route(member).as_deref(),
                Some("ops@renamed.invalid:2202 → relay@[2001:db8::1]:22 → root@smoke.invalid:22")
            );
            assert_ne!(
                workspace.active_session_sidebar_row_signature(
                    &row,
                    &workspace.active_session_sidebar_display_rows(cx),
                    ActiveSessionSidebarViewMode::Host,
                    cx
                ),
                before_signature
            );
        });
    }

    #[gpui::test]
    fn tree_and_host_views_align_columns_segment_items_and_hover_disconnect(
        cx: &mut TestAppContext,
    ) {
        let Some(_fixture_dir) = isolated_fixture(cx, "OXIDETERM_SESSION_LAYOUT_TEST_DIR") else {
            return;
        };
        let settings_path = default_settings_path();
        let mut store = SettingsStore::load_from_path(settings_path).unwrap();
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.settings_mut().onboarding_completed = true;
        store.settings_mut().general.language = oxideterm_settings::Language::En;
        store.save().unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.run_until_parked();

        let standalone_id = workspace.update(cx, |workspace, _| {
            let ssh_config = SshConfig {
                host: "layout-prod.invalid".into(),
                port: 22,
                username: "ops".into(),
                ..Default::default()
            };
            let ssh_id = NodeId::new("layout-ssh");
            workspace
                .node_router
                .upsert_node(ssh_id.clone(), ssh_config.clone());
            workspace.ssh_nodes.insert(
                ssh_id.clone(),
                WorkspaceSshNode::new(
                    None,
                    &ssh_config,
                    "Layout SSH".into(),
                    vec![TerminalSessionId(9001)],
                    NodeReadiness::Ready,
                ),
            );
            workspace.expanded_ssh_nodes.insert(ssh_id);

            let standalone_id = workspace.standalone_connections.insert_pending(
                standalone_connections::StandaloneConnectionKind::Rdp,
                "Layout RDP".into(),
                standalone_connections::StandaloneConnectionLaunch::SavedRemoteDesktop {
                    profile_id: "rdp-profile-1".into(),
                },
            );
            workspace
                .connection_store
                .upsert_local_terminal_profile(
                    oxideterm_connections::SaveLocalTerminalProfileRequest {
                        pre_connect_command: None,
                        id: Some("local-layout".into()),
                        name: "Layout Zsh".into(),
                        group: None,
                        icon: None,
                        color: None,
                        icon_background_color: None,
                        shell_id: None,
                        cwd: None,
                    },
                )
                .unwrap();
            standalone_id
        });

        let draw_sidebar = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        };
        draw_sidebar(cx);

        // 1. Tree view: SSH, Standalone, and Local headers align on both icon X and title X.
        let ssh_icon = cx
            .debug_bounds("session-header-icon-layout-ssh")
            .expect("ssh header icon bounds");
        let ssh_title = cx
            .debug_bounds("session-header-title-layout-ssh")
            .expect("ssh header title bounds");
        let standalone_selector =
            format!("session-header-icon-standalone-connection-{standalone_id}");
        let standalone_title_selector =
            format!("session-header-title-standalone-connection-{standalone_id}");
        let standalone_icon = cx
            .debug_bounds(Box::leak(standalone_selector.into_boxed_str()))
            .expect("standalone header icon bounds");
        let standalone_title = cx
            .debug_bounds(Box::leak(standalone_title_selector.into_boxed_str()))
            .expect("standalone header title bounds");
        let local_icon = cx
            .debug_bounds("session-header-icon-local-profile-local-layout")
            .expect("local header icon bounds");
        let local_title = cx
            .debug_bounds("session-header-title-local-profile-local-layout")
            .expect("local header title bounds");

        assert_eq!(ssh_icon.origin.x, standalone_icon.origin.x);
        assert_eq!(ssh_icon.origin.x, local_icon.origin.x);
        assert_eq!(ssh_title.origin.x, standalone_title.origin.x);
        assert_eq!(ssh_title.origin.x, local_title.origin.x);

        // 2. Expanded SSH node puts terminals above tools, Disconnect at the bottom,
        //    and aligns child icons & labels identically with the parent title.
        let term_icon = cx
            .debug_bounds("session-terminal-icon-9001")
            .expect("terminal icon bounds");
        let term_label = cx
            .debug_bounds("session-terminal-label-9001")
            .expect("terminal label bounds");
        let new_term_row = cx
            .debug_bounds("session-action-row-new-terminal")
            .expect("new terminal row bounds");
        let new_term_icon = cx
            .debug_bounds("session-action-icon-new-terminal")
            .expect("new terminal icon bounds");
        let new_term_label = cx
            .debug_bounds("session-action-label-new-terminal")
            .expect("new terminal label bounds");
        let disconnect_row = cx
            .debug_bounds("session-action-row-disconnect")
            .expect("disconnect row bounds");
        let disconnect_label = cx
            .debug_bounds("session-action-label-disconnect")
            .expect("disconnect label bounds");

        assert_eq!(term_icon.origin.x, new_term_icon.origin.x);
        assert_eq!(term_label.origin.x, new_term_label.origin.x);
        assert_eq!(term_label.origin.x, ssh_title.origin.x);
        assert_eq!(disconnect_label.origin.x, new_term_label.origin.x);
        assert!(term_label.origin.y < new_term_label.origin.y);
        assert!(new_term_label.origin.y < disconnect_label.origin.y);

        // Non-last root SSH node with !has_children must not dangle a rail past Disconnect
        // into the standalone header below it.
        let (rail_color, hover_danger_bg) = workspace.read_with(cx, |workspace, _| {
            let ui = workspace.tokens.ui;
            (
                rgba((ui.text_muted << 8) | 0x28),
                rgb(mix_rgb(ui.bg_hover, ui.error, 0.14)),
            )
        });

        cx.update(|window, _| {
            let scale = window.scale_factor();
            let quads = window.painted_quads();
            assert!(
                !quads
                    .iter()
                    .any(|q| q.background.as_solid() == Some(hover_danger_bg.into())),
                "Disconnect must stay neutral at rest"
            );
            let rail_quads: Vec<_> = quads
                .iter()
                .filter(|q| {
                    q.background.as_solid() == Some(rail_color.into())
                        && q.bounds.size.width == px(1.0).scale(scale)
                })
                .collect();
            assert!(
                !rail_quads.is_empty(),
                "guide rail must render for expanded SSH children"
            );
            for rail in rail_quads {
                assert!(
                    rail.bounds.bottom() <= disconnect_row.bottom().scale(scale),
                    "guide rail must terminate by Disconnect bottom ({:?} <= {:?})",
                    rail.bounds.bottom(),
                    disconnect_row.bottom().scale(scale)
                );
                assert!(
                    rail.bounds.bottom() <= standalone_icon.top().scale(scale),
                    "guide rail must not extend into the next root header"
                );
            }
        });

        // 3. Hovering a sibling action must keep Disconnect neutral; hovering Disconnect
        // reveals the danger hover tint.
        cx.simulate_mouse_move(new_term_row.center(), None, gpui::Modifiers::default());
        draw_sidebar(cx);
        cx.update(|window, _| {
            assert!(
                !window
                    .painted_quads()
                    .iter()
                    .any(|q| q.background.as_solid() == Some(hover_danger_bg.into())),
                "hovering sibling action must not paint Disconnect danger tint"
            );
        });

        cx.simulate_mouse_move(disconnect_row.center(), None, gpui::Modifiers::default());
        draw_sidebar(cx);
        cx.update(|window, _| {
            let hovered = window
                .painted_quads()
                .iter()
                .any(|q| q.background.as_solid() == Some(hover_danger_bg.into()));
            assert!(hovered, "hovering Disconnect must paint danger hover tint");
        });

        // 4. Host view: single-member & multi-member user rows align children and preserve selectors.
        workspace.update(cx, |workspace, cx| {
            let second_config = SshConfig {
                host: "layout-prod.invalid".into(),
                port: 2222,
                username: "ops".into(),
                ..Default::default()
            };
            let second_id = NodeId::new("layout-ssh-2");
            workspace
                .node_router
                .upsert_node(second_id.clone(), second_config.clone());
            workspace.ssh_nodes.insert(
                second_id,
                WorkspaceSshNode::new(
                    None,
                    &second_config,
                    "Layout SSH Alt".into(),
                    vec![TerminalSessionId(9002)],
                    NodeReadiness::Ready,
                ),
            );
            workspace.toggle_active_session_sidebar_view(cx);
            workspace.set_session_group_expanded(
                NodeId::new("session-host-group:layout-prod.invalid\u{1}ops"),
                true,
                false,
            );
        });
        draw_sidebar(cx);

        let host_icon = cx
            .debug_bounds("session-header-icon-session-host-group:layout-prod.invalid")
            .expect("host header icon bounds");
        let user_icon = cx
            .debug_bounds("session-header-icon-session-host-group:layout-prod.invalid\u{1}ops")
            .expect("user header icon bounds");
        let user_title = cx
            .debug_bounds("session-header-title-session-host-group:layout-prod.invalid\u{1}ops")
            .expect("user header title bounds");
        let member_port = cx
            .debug_bounds("session-group-member-port-layout-ssh-2")
            .expect("member port bounds");
        let member_title = cx
            .debug_bounds("session-group-member-title-layout-ssh-2")
            .expect("member title bounds");
        let host_term_label = cx
            .debug_bounds("session-terminal-label-9002")
            .expect("host view terminal label bounds");
        let host_action_label = cx
            .debug_bounds("session-action-label-new-terminal")
            .expect("host view action label bounds");

        assert_eq!(user_icon.origin.x - host_icon.origin.x, px(20.0));
        assert_eq!(member_title.origin.x, user_title.origin.x);
        assert_eq!(host_term_label.origin.x - member_title.origin.x, px(20.0));
        assert_eq!(host_term_label.origin.x, host_action_label.origin.x);
        assert!(host_term_label.origin.y < host_action_label.origin.y);
        assert!(member_port.size.width > px(0.0));
    }

    #[test]
    fn action_row_hover_changes_label_glyph_color() {
        use std::borrow::Cow;
        use std::cell::RefCell;
        use std::rc::Rc;
        use std::sync::Arc;

        let Some(_fixture_dir) = isolated_fixture_named(
            "action_row_hover_changes_label_glyph_color",
            "OXIDETERM_ACTION_HOVER_TEST_DIR",
        ) else {
            return;
        };
        let mut store = SettingsStore::load_from_path(default_settings_path()).unwrap();
        store.settings_mut().ssh_config.auto_load_hosts = false;
        store.settings_mut().onboarding_completed = true;
        store.settings_mut().general.language = oxideterm_settings::Language::En;
        store.save().unwrap();

        struct GlyphTestTextSystem(gpui::NoopTextSystem);

        impl gpui::PlatformTextSystem for GlyphTestTextSystem {
            fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> anyhow::Result<()> {
                self.0.add_fonts(fonts)
            }
            fn all_font_names(&self) -> Vec<String> {
                self.0.all_font_names()
            }
            fn font_id(&self, descriptor: &gpui::Font) -> anyhow::Result<gpui::FontId> {
                self.0.font_id(descriptor)
            }
            fn font_metrics(&self, font_id: gpui::FontId) -> gpui::FontMetrics {
                self.0.font_metrics(font_id)
            }
            fn typographic_bounds(
                &self,
                font_id: gpui::FontId,
                glyph_id: gpui::GlyphId,
            ) -> anyhow::Result<Bounds<f32>> {
                self.0.typographic_bounds(font_id, glyph_id)
            }
            fn advance(
                &self,
                font_id: gpui::FontId,
                glyph_id: gpui::GlyphId,
            ) -> anyhow::Result<gpui::Size<f32>> {
                self.0.advance(font_id, glyph_id)
            }
            fn glyph_for_char(&self, font_id: gpui::FontId, ch: char) -> Option<gpui::GlyphId> {
                self.0.glyph_for_char(font_id, ch)
            }
            fn glyph_raster_bounds(
                &self,
                _params: &gpui::RenderGlyphParams,
            ) -> anyhow::Result<Bounds<gpui::DevicePixels>> {
                Ok(Bounds {
                    origin: point(gpui::DevicePixels(0), gpui::DevicePixels(0)),
                    size: gpui::size(gpui::DevicePixels(8), gpui::DevicePixels(10)),
                })
            }
            fn rasterize_glyph(
                &self,
                _params: &gpui::RenderGlyphParams,
                raster_bounds: Bounds<gpui::DevicePixels>,
            ) -> anyhow::Result<(gpui::Size<gpui::DevicePixels>, Vec<u8>)> {
                let len = (raster_bounds.size.width.0.max(0) * raster_bounds.size.height.0.max(0))
                    as usize;
                Ok((raster_bounds.size, vec![255; len]))
            }
            fn layout_line(
                &self,
                text: &str,
                font_size: Pixels,
                runs: &[gpui::FontRun],
            ) -> gpui::LineLayout {
                self.0.layout_line(text, font_size, runs)
            }
            fn recommended_rendering_mode(
                &self,
                font_id: gpui::FontId,
                font_size: Pixels,
            ) -> gpui::TextRenderingMode {
                self.0.recommended_rendering_mode(font_id, font_size)
            }
        }

        struct CapturedScene {
            quads: Vec<gpui::Quad>,
            monochrome_sprites: Vec<gpui::MonochromeSprite>,
            subpixel_sprites: Vec<gpui::SubpixelSprite>,
        }

        struct SceneRecorder {
            atlas: Arc<gpui::HeadlessAtlas>,
            captured: Rc<RefCell<Option<CapturedScene>>>,
        }

        impl gpui::PlatformHeadlessRenderer for SceneRecorder {
            fn render_scene(
                &mut self,
                scene: &gpui::Scene,
                _: gpui::Size<gpui::DevicePixels>,
            ) -> anyhow::Result<()> {
                *self.captured.borrow_mut() = Some(CapturedScene {
                    quads: scene.quads.clone(),
                    monochrome_sprites: scene.monochrome_sprites.clone(),
                    subpixel_sprites: scene.subpixel_sprites.clone(),
                });
                Ok(())
            }
            fn render_scene_to_image(
                &mut self,
                scene: &gpui::Scene,
                size: gpui::Size<gpui::DevicePixels>,
            ) -> anyhow::Result<image::RgbaImage> {
                self.render_scene(scene, size)?;
                Ok(image::RgbaImage::new(0, 0))
            }
            fn sprite_atlas(&self) -> Arc<dyn gpui::PlatformAtlas> {
                self.atlas.clone()
            }
        }

        struct ActionHoverHarness {
            workspace: Entity<WorkspaceApp>,
        }

        impl gpui::Render for ActionHoverHarness {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let (new_term, disconnect) = self.workspace.update(cx, |workspace, cx| {
                    let new_term = workspace.render_session_action_item_with_rails(
                        "hover-test",
                        1,
                        &[],
                        false,
                        "new-terminal",
                        LucideIcon::Plus,
                        "New Terminal".to_string(),
                        SessionActionVariant::Primary,
                        false,
                        |_, _, _| {},
                        cx,
                    );
                    let disconnect = workspace.render_session_action_item_with_rails(
                        "hover-test",
                        1,
                        &[],
                        true,
                        "disconnect",
                        LucideIcon::WifiOff,
                        "Disconnect".to_string(),
                        SessionActionVariant::Danger,
                        false,
                        |_, _, _| {},
                        cx,
                    );
                    (new_term, disconnect)
                });
                div()
                    .w(px(240.0))
                    .flex()
                    .flex_col()
                    .child(new_term)
                    .child(disconnect)
            }
        }

        let captured = Rc::new(RefCell::new(None));
        let atlas = Arc::new(gpui::HeadlessAtlas::default());
        let mut cx = gpui::HeadlessAppContext::with_platform(
            Arc::new(GlyphTestTextSystem(gpui::NoopTextSystem)),
            Arc::new(()),
            {
                let captured = captured.clone();
                let atlas = atlas.clone();
                move || {
                    Ok(Some(Box::new(SceneRecorder {
                        atlas: atlas.clone(),
                        captured: captured.clone(),
                    })))
                }
            },
        );
        cx.allow_parking();

        let window = cx
            .open_window(gpui::size(px(260.0), px(120.0)), |window, cx| {
                let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
                cx.new(|_| ActionHoverHarness { workspace })
            })
            .unwrap();
        cx.run_until_parked();

        let (item_height, ui) = cx.update(|app| {
            let workspace = window.read(app).unwrap().workspace.read(app);
            (workspace.session_tree_item_height(), workspace.tokens.ui)
        });
        let scale = cx
            .update_window(window.into(), |_, window, _| window.scale_factor())
            .unwrap();
        let idle_label_hsla: gpui::SceneHsla =
            gpui::Hsla::from(rgb(mix_rgb(ui.text, ui.text_muted, 0.42))).into();
        let primary_hover_hsla: gpui::SceneHsla = gpui::Hsla::from(rgb(ui.text)).into();
        let error_label_hsla: gpui::SceneHsla = gpui::Hsla::from(rgb(ui.error)).into();
        let hover_primary_bg = rgb(ui.bg_hover);
        let hover_danger_bg = rgb(mix_rgb(ui.bg_hover, ui.error, 0.14));

        let capture_at = |cx: &mut gpui::HeadlessAppContext,
                          hover_pos: Option<gpui::Point<Pixels>>|
         -> CapturedScene {
            if let Some(pos) = hover_pos {
                cx.update_window(window.into(), |_, window, cx| {
                    window.simulate_mouse_move(pos, cx);
                })
                .unwrap();
                cx.run_until_parked();
            }
            cx.update_window(window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
            })
            .unwrap();
            cx.capture_screenshot(window.into()).unwrap();
            captured.borrow_mut().take().expect("captured scene")
        };

        let row_label_glyph_colors = |scene: &CapturedScene, row_index: usize| {
            // The scene stores device-scaled bounds; input and layout use logical pixels.
            let y_top = px(item_height * row_index as f32).scale(scale);
            let y_bottom = px(item_height * (row_index + 1) as f32).scale(scale);
            let label_left = px(40.0).scale(scale);
            let mut colors: Vec<gpui::SceneHsla> = scene
                .monochrome_sprites
                .iter()
                .filter(|sprite| {
                    sprite.bounds.origin.x >= label_left
                        && sprite.bounds.origin.y >= y_top
                        && sprite.bounds.origin.y < y_bottom
                })
                .map(|sprite| sprite.color)
                .collect();
            if colors.is_empty() {
                colors = scene
                    .subpixel_sprites
                    .iter()
                    .filter(|sprite| {
                        sprite.bounds.origin.x >= label_left
                            && sprite.bounds.origin.y >= y_top
                            && sprite.bounds.origin.y < y_bottom
                    })
                    .map(|sprite| sprite.color)
                    .collect();
            }
            colors
        };

        // 1. At rest: Disconnect label uses idle neutral ink and no danger hover tint.
        let rest_scene = capture_at(&mut cx, None);
        assert!(
            !rest_scene
                .quads
                .iter()
                .any(|q| q.background.as_solid() == Some(hover_danger_bg.into())),
            "Disconnect must stay neutral at rest"
        );
        let rest_disconnect = row_label_glyph_colors(&rest_scene, 1);
        assert!(
            !rest_disconnect.is_empty()
                && rest_disconnect.iter().all(|&c| c == idle_label_hsla)
                && !rest_disconnect.iter().any(|&c| c == error_label_hsla),
            "Disconnect label must use idle neutral ink at rest"
        );

        // 2. Hovering sibling New Terminal changes New Terminal to primary hover ink
        // while keeping Disconnect neutral.
        let sibling_scene = capture_at(&mut cx, Some(point(px(80.0), px(item_height * 0.5))));
        assert!(
            sibling_scene
                .quads
                .iter()
                .any(|q| q.background.as_solid() == Some(hover_primary_bg.into())),
            "hovering New Terminal must paint primary hover background"
        );
        assert!(
            !sibling_scene
                .quads
                .iter()
                .any(|q| q.background.as_solid() == Some(hover_danger_bg.into())),
            "hovering New Terminal must not paint Disconnect danger tint"
        );
        let sibling_new_term = row_label_glyph_colors(&sibling_scene, 0);
        assert!(
            !sibling_new_term.is_empty()
                && sibling_new_term.iter().all(|&c| c == primary_hover_hsla),
            "hovering New Terminal must paint the label in {primary_hover_hsla:?}, got {sibling_new_term:?}"
        );
        let sibling_disconnect = row_label_glyph_colors(&sibling_scene, 1);
        assert!(
            !sibling_disconnect.is_empty()
                && sibling_disconnect.iter().all(|&c| c == idle_label_hsla),
            "hovering New Terminal must keep Disconnect label neutral"
        );

        // 3. Hovering Disconnect paints danger hover tint and switches Disconnect
        // label glyph sprites to error ink.
        let danger_scene = capture_at(&mut cx, Some(point(px(80.0), px(item_height * 1.5))));
        assert!(
            danger_scene
                .quads
                .iter()
                .any(|q| q.background.as_solid() == Some(hover_danger_bg.into())),
            "hovering Disconnect must paint danger hover tint"
        );
        let hovered_disconnect = row_label_glyph_colors(&danger_scene, 1);
        assert!(
            !hovered_disconnect.is_empty()
                && hovered_disconnect.iter().all(|&c| c == error_label_hsla),
            "hovering Disconnect must paint Disconnect label glyphs in error ink"
        );
    }
}
