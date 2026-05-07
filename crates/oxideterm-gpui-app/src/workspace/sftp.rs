use super::ime::WorkspaceImeTarget;
use super::*;
use gpui::{ObjectFit, StatefulInteractiveElement, prelude::*};
use oxideterm_gpui_ui::text_input::{text_caret, text_input_anchor_probe};
use oxideterm_sftp::{
    AssetFileKind, FileInfo as RemoteFileInfo, FileType as RemoteFileType,
    ListFilter as RemoteListFilter, PreviewContent, SftpError, SftpSession,
    SortOrder as RemoteSortOrder, StoredTransferProgress, TransferProgress,
    TransferState as RemoteTransferState, TransferStrategy as RemoteTransferStrategy,
    TransferType as RemoteTransferType, probe_tar_compression, probe_tar_support,
    tar_download_directory, tar_upload_directory,
};

const SFTP_ROOT_PADDING: f32 = 8.0; // Tauri p-2
const SFTP_GAP: f32 = 8.0; // Tauri gap-2
const SFTP_PANE_HEADER_HEIGHT: f32 = 40.0; // Tauri h-10
const SFTP_QUEUE_HEIGHT: f32 = 192.0; // Tauri h-48
const SFTP_TEXT_XS: f32 = 12.0; // Tauri text-xs
const SFTP_TEXT_SM: f32 = 13.0; // Tauri text-sm
const SFTP_TEXT_10: f32 = 10.0; // Tauri text-[10px]
const SFTP_ICON_SM: f32 = 12.0; // Tauri h-3 w-3
const SFTP_ICON_MD: f32 = 14.0; // Tauri h-3.5 w-3.5
const SFTP_TOOL_BUTTON: f32 = 24.0; // Tauri h-6 w-6
const SFTP_ROW_HEIGHT: f32 = 25.0; // Tauri px-2 py-1 text-xs
const SFTP_SIZE_COL: f32 = 80.0; // Tauri w-20
const SFTP_MODIFIED_COL: f32 = 96.0; // Tauri w-24
const SFTP_BG_ACTIVE_BG_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg 40%
const SFTP_BG_ACTIVE_PANEL_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg-panel 40%
const SFTP_BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // [data-bg-active] --color-theme-bg-hover 50%
const SFTP_PANEL_80_ALPHA: u32 = 0xcc; // Tauri bg-theme-bg-panel/80
const SFTP_ACTIVE_BORDER_ALPHA: u32 = 0x80; // Tauri border-oxide-accent/50
const SFTP_HEADER_ACTIVE_BG_ALPHA: u32 = 0x80; // Tauri bg-theme-bg-hover/50
const SFTP_HEADER_ACTIVE_BORDER_ALPHA: u32 = 0x4d; // Tauri border-oxide-accent/30
#[allow(dead_code)]
const SFTP_DRAG_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10
#[allow(dead_code)]
const SFTP_DRAG_RING_ALPHA: u32 = 0x4d; // Tauri ring-oxide-accent/30
const SFTP_SELECTED_BG_ALPHA: u32 = 0x33; // Tauri bg-theme-accent/20
const SFTP_BREADCRUMB_ACTIVE_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-hover/30
const SFTP_BREADCRUMB_HOVER_ALPHA: u32 = 0x80; // Tauri hover:bg-theme-bg-hover/50
const SFTP_FOLDER_BLUE: u32 = 0x60a5fa; // Tauri text-blue-400
const SFTP_GREEN: u32 = 0x22c55e; // Tauri text-green-500
const SFTP_YELLOW: u32 = 0xeab308; // Tauri text-yellow-500
const SFTP_RED: u32 = 0xf87171; // Tauri text-red-400
const SFTP_CONTEXT_MENU_WIDTH: f32 = 180.0; // Tauri min-w-[180px]
const SFTP_CONTEXT_MENU_MAX_HEIGHT: f32 = 252.0; // 7 items + separators, clamped like fixed portal menu
const SFTP_CONTEXT_MENU_PADDING: f32 = 4.0; // Tauri py-1
const SFTP_CONTEXT_MENU_ITEM_HEIGHT: f32 = 30.0; // Tauri px-3 py-1.5 text-xs
const SFTP_DIALOG_OVERLAY_ALPHA: u32 = 0x99; // Tauri Dialog overlay opacity
const SFTP_DIALOG_SHADOW_ALPHA: u32 = 0x40; // Tauri shadow-lg-ish overlay shadow

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum SftpInput {
    LocalPath,
    RemotePath,
    LocalFilter,
    RemoteFilter,
    DialogValue,
}

impl SftpInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::LocalPath => 1,
            Self::RemotePath => 2,
            Self::LocalFilter => 3,
            Self::RemoteFilter => 4,
            Self::DialogValue => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpPane {
    Local,
    Remote,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpFileType {
    File,
    Directory,
}

#[derive(Clone, Debug)]
pub(super) struct SftpFileEntry {
    name: String,
    path: String,
    file_type: SftpFileType,
    size: u64,
    modified: Option<i64>,
    permissions: Option<String>,
    owner: Option<String>,
    group: Option<String>,
    is_symlink: bool,
    symlink_target: Option<String>,
}

#[derive(Debug)]
pub(super) enum SftpWorkerResult {
    RemoteList {
        tab_id: TabId,
        node_id: NodeId,
        session_id: String,
        path: String,
        result: Result<RemoteSftpListing, String>,
    },
    TransferProgress {
        id: u64,
        transferred: u64,
        total: u64,
        state: SftpTransferState,
        error: Option<String>,
    },
    TransferComplete {
        id: u64,
        result: Result<(), String>,
        refresh_remote: bool,
        refresh_local: bool,
    },
    RemoteMutationComplete {
        result: Result<(), String>,
        refresh_remote: bool,
        refresh_local: bool,
    },
    PreviewLoaded {
        path: String,
        result: Result<PreviewContent, String>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct RemoteSftpListing {
    cwd: String,
    files: Vec<SftpFileEntry>,
}

#[derive(Clone, Debug)]
struct SftpContextMenu {
    pane: SftpPane,
    file: Option<SftpFileEntry>,
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpSortField {
    Name,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpSortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpTransferDirection {
    Upload,
    Download,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SftpTransferState {
    Pending,
    Active,
    Paused,
    Completed,
    Cancelled,
    Error,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct SftpTransferItem {
    id: u64,
    name: String,
    local_path: String,
    remote_path: String,
    direction: SftpTransferDirection,
    size: u64,
    transferred: u64,
    state: SftpTransferState,
    error: Option<String>,
}

#[derive(Default)]
struct DirectoryProgressAccumulator {
    files: HashMap<(String, String), (u64, u64)>,
}

impl DirectoryProgressAccumulator {
    fn update(&mut self, progress: TransferProgress) -> TransferProgress {
        self.files.insert(
            (progress.remote_path.clone(), progress.local_path.clone()),
            (progress.transferred_bytes, progress.total_bytes),
        );
        let transferred_bytes = self
            .files
            .values()
            .map(|(transferred, _)| *transferred)
            .sum();
        let total_bytes = self.files.values().map(|(_, total)| *total).sum();
        TransferProgress {
            transferred_bytes,
            total_bytes,
            ..progress
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
enum SftpDialog {
    Drives,
    Rename { pane: SftpPane, old_name: String },
    NewFolder { pane: SftpPane },
    Delete { pane: SftpPane, files: Vec<String> },
    Conflict,
    Diff,
    Preview { name: String },
}

#[derive(Clone, Debug)]
struct SftpDrive {
    name: String,
    path: String,
    drive_type: &'static str,
    total_space: u64,
    available_space: u64,
    read_only: bool,
}

pub(super) struct SftpViewState {
    active_pane: SftpPane,
    local_path: String,
    remote_path: String,
    local_path_input: String,
    remote_path_input: String,
    local_filter: String,
    remote_filter: String,
    local_sort_field: SftpSortField,
    remote_sort_field: SftpSortField,
    local_sort_direction: SftpSortDirection,
    remote_sort_direction: SftpSortDirection,
    local_selected: HashSet<String>,
    remote_selected: HashSet<String>,
    local_last_selected: Option<String>,
    remote_last_selected: Option<String>,
    local_files: Vec<SftpFileEntry>,
    remote_files: Vec<SftpFileEntry>,
    remote_loading: bool,
    remote_load_pending: bool,
    remote_load_inflight: bool,
    init_error: Option<String>,
    pub(super) focused_input: Option<SftpInput>,
    editing_local_path: bool,
    editing_remote_path: bool,
    dialog: Option<SftpDialog>,
    dialog_value: String,
    preview_path: Option<String>,
    preview_content: Option<PreviewContent>,
    preview_error: Option<String>,
    preview_loading: bool,
    transfers: Vec<SftpTransferItem>,
    show_incomplete: bool,
    context_menu: Option<SftpContextMenu>,
    next_transfer_id: u64,
}

impl Default for SftpViewState {
    fn default() -> Self {
        let local_path = home_path_mock();
        let remote_path = String::new();
        Self {
            active_pane: SftpPane::Remote,
            local_path_input: local_path.clone(),
            remote_path_input: remote_path.clone(),
            local_path: local_path.clone(),
            remote_path,
            local_filter: String::new(),
            remote_filter: String::new(),
            local_sort_field: SftpSortField::Name,
            remote_sort_field: SftpSortField::Name,
            local_sort_direction: SftpSortDirection::Asc,
            remote_sort_direction: SftpSortDirection::Asc,
            local_selected: HashSet::new(),
            remote_selected: HashSet::new(),
            local_last_selected: None,
            remote_last_selected: None,
            local_files: list_local_files(&local_path).unwrap_or_else(|_| Vec::new()),
            remote_files: Vec::new(),
            remote_loading: false,
            remote_load_pending: false,
            remote_load_inflight: false,
            init_error: None,
            focused_input: None,
            editing_local_path: false,
            editing_remote_path: false,
            dialog: None,
            dialog_value: String::new(),
            preview_path: None,
            preview_content: None,
            preview_error: None,
            preview_loading: false,
            transfers: Vec::new(),
            show_incomplete: false,
            context_menu: None,
            next_transfer_id: 1,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_sftp_tab(
        &mut self,
        node_id: NodeId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("{} · {}", self.i18n.t("sidebar.panels.sftp"), node_title);
        let tab_id = if let Some((tab_id, _)) = self
            .sftp_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| *existing_node_id == &node_id)
        {
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Sftp,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.sftp_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_sidebar_section = SidebarSection::Sessions;
        self.active_ssh_node_id = Some(node_id);
        self.sftp_view.remote_load_pending = true;
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn maybe_start_sftp_remote_load(&mut self, cx: &mut Context<Self>) {
        if self.sftp_view.remote_load_inflight || !self.sftp_view.remote_load_pending {
            return;
        }
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        if self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_none_or(|tab| tab.kind != TabKind::Sftp)
        {
            return;
        }
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let path = self.sftp_view.remote_path.clone();
        self.start_sftp_remote_load(tab_id, node_id, path, cx);
    }

    fn start_sftp_remote_load(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let session_id = format!("node:{}:sftp", node_id.0);
        let resolved = match self
            .node_router
            .acquire_connection(&node_id, ConnectionConsumer::Sftp(session_id.clone()))
        {
            Ok(resolved) => resolved,
            Err(error) => {
                self.sftp_view.remote_loading = false;
                self.sftp_view.remote_load_pending = false;
                self.sftp_view.remote_load_inflight = false;
                self.sftp_view.init_error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.sftp_connection_consumers.insert(
            session_id.clone(),
            (
                resolved.connection_id.clone(),
                ConnectionConsumer::Sftp(session_id.clone()),
            ),
        );
        self.sftp_view.remote_loading = true;
        self.sftp_view.remote_load_pending = false;
        self.sftp_view.remote_load_inflight = true;
        self.sftp_view.init_error = None;

        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        let router = self.node_router.clone();
        runtime.spawn(async move {
            let result = load_remote_sftp_listing(router, &node_id, &path).await;
            let _ = tx.send(SftpWorkerResult::RemoteList {
                tab_id,
                node_id,
                session_id,
                path,
                result,
            });
        });
        cx.notify();
    }

    pub(super) fn poll_sftp_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut results = Vec::new();
        while let Ok(result) = self.sftp_worker_rx.try_recv() {
            results.push(result);
        }

        let mut changed = false;
        for result in results {
            match result {
                SftpWorkerResult::RemoteList {
                    tab_id,
                    node_id,
                    session_id,
                    path,
                    result,
                } => {
                    if Some(tab_id) == self.active_tab_id {
                        self.sftp_view.remote_load_inflight = false;
                        self.sftp_view.remote_loading = false;
                        match result {
                            Ok(listing) => {
                                let cwd = listing.cwd;
                                self.sftp_view.remote_path = cwd.clone();
                                self.sftp_view.remote_path_input = cwd.clone();
                                self.sftp_view.remote_files = listing.files;
                                self.sftp_view.remote_selected.clear();
                                self.sftp_view.remote_last_selected = None;
                                self.sftp_view.init_error = None;
                                // GPUI still carries a session id for tab/UI compatibility, but
                                // the real SFTP owner lives in ConnectionEntry via NodeRouter.
                                if let Ok(event) = self.node_router.bind_sftp_session(
                                    &node_id,
                                    session_id,
                                    Some(cwd),
                                ) {
                                    self.emit_node_event(event);
                                }
                            }
                            Err(error) => {
                                self.sftp_view.init_error = Some(format!("{}: {error}", path));
                            }
                        }
                        changed = true;
                    }
                }
                SftpWorkerResult::TransferProgress {
                    id,
                    transferred,
                    total,
                    state,
                    error,
                } => {
                    if let Some(item) = self
                        .sftp_view
                        .transfers
                        .iter_mut()
                        .find(|item| item.id == id)
                    {
                        item.transferred = transferred;
                        item.size = total.max(item.size);
                        item.state = state;
                        item.error = error;
                        changed = true;
                    }
                }
                SftpWorkerResult::TransferComplete {
                    id,
                    result,
                    refresh_remote,
                    refresh_local,
                } => {
                    if let Some(item) = self
                        .sftp_view
                        .transfers
                        .iter_mut()
                        .find(|item| item.id == id)
                    {
                        match result {
                            Ok(()) => {
                                item.transferred = item.size;
                                item.state = SftpTransferState::Completed;
                                item.error = None;
                            }
                            Err(error) => {
                                item.state = SftpTransferState::Error;
                                item.error = Some(error);
                            }
                        }
                    }
                    if refresh_remote {
                        self.sftp_view.remote_load_pending = true;
                    }
                    if refresh_local && let Ok(files) = list_local_files(&self.sftp_view.local_path)
                    {
                        self.sftp_view.local_files = files;
                    }
                    changed = true;
                }
                SftpWorkerResult::RemoteMutationComplete {
                    result,
                    refresh_remote,
                    refresh_local,
                } => {
                    if let Err(error) = result {
                        self.sftp_view.init_error = Some(error);
                    }
                    if refresh_remote {
                        self.sftp_view.remote_load_pending = true;
                    }
                    if refresh_local && let Ok(files) = list_local_files(&self.sftp_view.local_path)
                    {
                        self.sftp_view.local_files = files;
                    }
                    changed = true;
                }
                SftpWorkerResult::PreviewLoaded { path, result } => {
                    self.sftp_view.preview_loading = false;
                    self.sftp_view.preview_path = Some(path);
                    match result {
                        Ok(content) => {
                            self.sftp_view.preview_content = Some(content);
                            self.sftp_view.preview_error = None;
                        }
                        Err(error) => {
                            self.sftp_view.preview_content = None;
                            self.sftp_view.preview_error = Some(error);
                        }
                    }
                    changed = true;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(super) fn apply_sftp_ready_event(
        &mut self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
    ) {
        if self
            .active_tab_id
            .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
            != Some(node_id)
        {
            return;
        }
        self.sftp_view.remote_loading = !ready;
        if let Some(cwd) = cwd {
            self.sftp_view.remote_path = cwd.clone();
            self.sftp_view.remote_path_input = cwd;
        }
    }

    pub(super) fn render_sftp_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(tab_id) = self.active_tab_id else {
            return self.render_empty_workspace(cx);
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return self.render_empty_workspace(cx);
        };
        let has_background = self.terminal_background_preferences("sftp").is_some();
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.as_str())
            .unwrap_or("mock-host");

        let mut root = div()
            .id("sftp-view")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .p(px(SFTP_ROOT_PADDING))
            .gap(px(SFTP_GAP))
            .bg(sftp_bg(theme.bg, has_background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.sftp_view.context_menu = None;
                    cx.notify();
                }),
            )
            .when_some(self.sftp_view.init_error.as_ref(), |root, error| {
                root.child(self.render_sftp_init_error(error, has_background, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    .gap(px(SFTP_GAP))
                    .child(self.render_sftp_pane(
                        SftpPane::Local,
                        self.i18n.t("sftp.file_list.local"),
                        &self.sftp_view.local_path,
                        &self.sftp_view.local_filter,
                        self.sftp_view.local_sort_field,
                        self.sftp_view.local_sort_direction,
                        &self.sftp_view.local_files,
                        &self.sftp_view.local_selected,
                        self.sftp_view.editing_local_path,
                        &self.sftp_view.local_path_input,
                        self.sftp_view.focused_input,
                        false,
                        has_background,
                        cx,
                    ))
                    .child(
                        self.render_sftp_pane(
                            SftpPane::Remote,
                            self.i18n
                                .t("sftp.file_list.remote")
                                .replace("{{host}}", node_title),
                            &self.sftp_view.remote_path,
                            &self.sftp_view.remote_filter,
                            self.sftp_view.remote_sort_field,
                            self.sftp_view.remote_sort_direction,
                            &self.sftp_view.remote_files,
                            &self.sftp_view.remote_selected,
                            self.sftp_view.editing_remote_path,
                            &self.sftp_view.remote_path_input,
                            self.sftp_view.focused_input,
                            self.sftp_view.remote_loading,
                            has_background,
                            cx,
                        ),
                    ),
            )
            .child(self.render_sftp_transfer_queue(has_background, cx));

        if let Some(dialog) = self.sftp_view.dialog.as_ref() {
            root = root.child(self.render_sftp_dialog(dialog.clone(), has_background, cx));
        } else if let Some(menu) = self.sftp_view.context_menu.clone() {
            root = root.child(self.render_sftp_context_menu(menu, window, has_background, cx));
        }

        root.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_sftp_pane(
        &self,
        pane: SftpPane,
        title: String,
        path: &str,
        filter: &str,
        sort_field: SftpSortField,
        sort_direction: SftpSortDirection,
        files: &[SftpFileEntry],
        selected: &HashSet<String>,
        path_editing: bool,
        path_input: &str,
        focused_input: Option<SftpInput>,
        loading: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.sftp_view.active_pane == pane;
        let filtered = sorted_sftp_files(files, filter, sort_field, sort_direction);
        let transfer_direction = if pane == SftpPane::Local {
            SftpTransferDirection::Upload
        } else {
            SftpTransferDirection::Download
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(if active {
                rgba((theme.accent << 8) | SFTP_ACTIVE_BORDER_ALPHA)
            } else {
                sftp_border(theme.border, has_background)
            })
            .bg(sftp_bg(theme.bg, has_background))
            .hover(move |pane| pane.border_color(rgba((theme.accent << 8) | 0x40)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.sftp_view.active_pane = pane;
                    cx.notify();
                }),
            )
            .child(self.render_sftp_pane_header(
                pane,
                title,
                path,
                path_editing,
                path_input,
                focused_input,
                selected.len(),
                transfer_direction,
                active,
                has_background,
                cx,
            ))
            .child(self.render_sftp_column_header(
                pane,
                sort_field,
                sort_direction,
                has_background,
                cx,
            ))
            .child(self.render_sftp_filter(pane, filter, focused_input, has_background, cx))
            .child(self.render_sftp_file_list(
                pane,
                path,
                filtered,
                selected,
                loading,
                has_background,
                cx,
            ))
            .into_any_element()
    }

    fn render_sftp_pane_header(
        &self,
        pane: SftpPane,
        title: String,
        path: &str,
        path_editing: bool,
        path_input: &str,
        focused_input: Option<SftpInput>,
        selected_count: usize,
        transfer_direction: SftpTransferDirection,
        active: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let input = if pane == SftpPane::Local {
            SftpInput::LocalPath
        } else {
            SftpInput::RemotePath
        };
        let mut header = div()
            .h(px(SFTP_PANE_HEADER_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .p(px(8.0))
            .border_b_1()
            .border_color(if active {
                rgba((theme.accent << 8) | SFTP_HEADER_ACTIVE_BORDER_ALPHA)
            } else {
                sftp_border(theme.border, has_background)
            })
            .bg(if active {
                rgba((theme.bg_hover << 8) | SFTP_HEADER_ACTIVE_BG_ALPHA)
            } else {
                sftp_panel_bg(theme.bg_panel, has_background, 0xff)
            })
            .child(
                div()
                    .min_w(px(48.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(title.to_uppercase()),
            )
            .child(self.render_sftp_path_bar(
                pane,
                input,
                path,
                path_input,
                path_editing,
                focused_input,
                cx,
            ));

        if pane == SftpPane::Local {
            header = header
                .child(self.render_sftp_icon_button(
                    LucideIcon::HardDrive,
                    self.i18n.t("sftp.toolbar.show_drives"),
                    cx.listener(|this, _event, _window, cx| {
                        this.sftp_view.dialog = Some(SftpDialog::Drives);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .child(self.render_sftp_icon_button(
                    LucideIcon::FolderOpen,
                    self.i18n.t("sftp.toolbar.browse_folder"),
                    cx.listener(|this, _event, _window, cx| {
                        this.sftp_view.dialog = Some(SftpDialog::Drives);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
        }

        header = header
            .child(self.render_sftp_nav_button(
                pane,
                "..",
                LucideIcon::ArrowUp,
                "sftp.toolbar.go_up",
                cx,
            ))
            .child(self.render_sftp_nav_button(
                pane,
                "~",
                LucideIcon::Home,
                "sftp.toolbar.home",
                cx,
            ))
            .child(self.render_sftp_refresh_button(pane, cx));

        if selected_count > 0 {
            let label = match transfer_direction {
                SftpTransferDirection::Upload => self
                    .i18n
                    .t("sftp.toolbar.upload_count")
                    .replace("{{count}}", &selected_count.to_string()),
                SftpTransferDirection::Download => self
                    .i18n
                    .t("sftp.toolbar.download_count")
                    .replace("{{count}}", &selected_count.to_string()),
            };
            let icon = match transfer_direction {
                SftpTransferDirection::Upload => LucideIcon::Upload,
                SftpTransferDirection::Download => LucideIcon::Download,
            };
            header = header.child(self.render_sftp_transfer_button(
                pane,
                transfer_direction,
                icon,
                label,
                cx,
            ));
        }

        header.into_any_element()
    }

    fn render_sftp_path_bar(
        &self,
        pane: SftpPane,
        input: SftpInput,
        path: &str,
        path_input: &str,
        editing: bool,
        focused_input: Option<SftpInput>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = focused_input == Some(input);
        let value = if editing { path_input } else { path };
        let path_bar = div()
            .flex_1()
            .min_w(px(0.0))
            .h(px(24.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if focused {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            })
            .bg(rgba((theme.bg_sunken << 8) | 0xcc))
            .overflow_hidden()
            .cursor_pointer()
            .when(editing, |bar| {
                bar.child(self.render_sftp_inline_text(
                    input,
                    value,
                    "sftp.file_list.path_placeholder",
                    focused,
                    cx,
                ))
                .child(
                    div()
                        .size(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.sm))
                        .hover(move |button| button.bg(rgb(theme.bg_hover)))
                        .child(Self::render_lucide_icon(
                            LucideIcon::ArrowRight,
                            SFTP_ICON_SM,
                            rgb(theme.text),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.commit_sftp_path_input(pane);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .when(!editing, |bar| {
                bar.child(self.render_sftp_breadcrumb(pane, path, cx))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.sftp_view.active_pane = pane;
                    if editing || event.click_count >= 2 {
                        match pane {
                            SftpPane::Local => {
                                this.sftp_view.editing_local_path = true;
                                this.sftp_view.local_path_input = this.sftp_view.local_path.clone();
                            }
                            SftpPane::Remote => {
                                this.sftp_view.editing_remote_path = true;
                                this.sftp_view.remote_path_input =
                                    this.sftp_view.remote_path.clone();
                            }
                        }
                        this.sftp_view.focused_input = Some(input);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        path_bar.into_any_element()
    }

    fn render_sftp_breadcrumb(
        &self,
        pane: SftpPane,
        path: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let segments = sftp_path_segments(path, pane == SftpPane::Remote);
        let mut row = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.0))
            .overflow_hidden()
            .text_size(px(SFTP_TEXT_SM));

        for (index, segment) in segments.into_iter().enumerate() {
            if index > 0 {
                row = row.child(Self::render_lucide_icon(
                    LucideIcon::ChevronRight,
                    SFTP_ICON_MD,
                    rgb(theme.text_muted),
                ));
            }
            let is_last = index + 1 == sftp_path_segments(path, pane == SftpPane::Remote).len();
            let full_path = segment.full_path.clone();
            row = row.child(
                div()
                    .max_w(px(120.0))
                    .h(px(20.0))
                    .px(px(6.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(if is_last {
                        rgba((theme.bg_hover << 8) | SFTP_BREADCRUMB_ACTIVE_ALPHA)
                    } else {
                        rgba(theme.bg_hover << 8)
                    })
                    .hover(move |crumb| {
                        crumb.bg(rgba((theme.bg_hover << 8) | SFTP_BREADCRUMB_HOVER_ALPHA))
                    })
                    .text_color(if is_last {
                        rgb(theme.text_heading)
                    } else {
                        rgb(theme.text)
                    })
                    .when(index == 0, |item| {
                        item.child(Self::render_lucide_icon(
                            if pane == SftpPane::Remote {
                                LucideIcon::Server
                            } else {
                                LucideIcon::Home
                            },
                            SFTP_ICON_MD,
                            rgb(theme.text_muted),
                        ))
                    })
                    .child(div().truncate().child(segment.name))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_sftp_path(pane, full_path.clone());
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }
        row.into_any_element()
    }

    fn render_sftp_column_header(
        &self,
        pane: SftpPane,
        sort_field: SftpSortField,
        sort_direction: SftpSortDirection,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(25.0))
            .flex()
            .flex_row()
            .items_center()
            .px(px(8.0))
            .py(px(4.0))
            .bg(sftp_panel_bg(self.tokens.ui.bg_panel, has_background, 0xff))
            .border_b_1()
            .border_color(sftp_border(self.tokens.ui.border, has_background))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Name,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_name"),
                None,
                cx,
            ))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Size,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_size"),
                Some(SFTP_SIZE_COL),
                cx,
            ))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Modified,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_modified"),
                Some(SFTP_MODIFIED_COL),
                cx,
            ))
            .into_any_element()
    }

    fn render_sftp_sort_header(
        &self,
        pane: SftpPane,
        field: SftpSortField,
        active_field: SftpSortField,
        _direction: SftpSortDirection,
        label: String,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .when_some(width, |header, width| header.w(px(width)).justify_end())
            .when(width.is_none(), |header| header.flex_1())
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .text_color(if active_field == field {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |header| header.text_color(rgb(theme.text)))
            .cursor_pointer()
            .child(div().truncate().child(label))
            .when(active_field == field, |header| {
                header.child(Self::render_lucide_icon(
                    LucideIcon::ArrowUpDown,
                    SFTP_ICON_SM,
                    rgb(theme.accent),
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.toggle_sftp_sort(pane, field);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_filter(
        &self,
        pane: SftpPane,
        filter: &str,
        focused_input: Option<SftpInput>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = if pane == SftpPane::Local {
            SftpInput::LocalFilter
        } else {
            SftpInput::RemoteFilter
        };
        let focused = focused_input == Some(input);
        let theme = self.tokens.ui;
        div()
            .h(px(30.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .py(px(4.0))
            .bg(sftp_panel_bg(
                theme.bg_panel,
                has_background,
                SFTP_PANEL_80_ALPHA,
            ))
            .border_b_1()
            .border_color(sftp_border(theme.border, has_background))
            .child(Self::render_lucide_icon(
                LucideIcon::Search,
                SFTP_ICON_SM,
                rgb(theme.text_muted),
            ))
            .child(self.render_sftp_inline_text(
                input,
                filter,
                "sftp.file_list.filter_placeholder",
                focused,
                cx,
            ))
            .when(!filter.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .hover(move |x| x.text_color(rgb(theme.text)))
                        .cursor_pointer()
                        .child("×")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                *this.sftp_input_value_mut(input) = String::new();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.sftp_view.active_pane = pane;
                    this.sftp_view.focused_input = Some(input);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_inline_text(
        &self,
        input: SftpInput,
        value: &str,
        placeholder_key: &'static str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let text = if value.is_empty() {
            self.i18n.t(placeholder_key)
        } else {
            value.to_string()
        };
        let target = WorkspaceImeTarget::Sftp(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .text_size(px(SFTP_TEXT_XS))
                .text_color(if value.is_empty() {
                    rgb(theme.text_muted)
                } else {
                    rgb(theme.text)
                })
                .when(focused && value.is_empty(), |input| {
                    input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                })
                .child(div().truncate().child(text))
                .when_some(self.marked_text_for_target(target), |input, marked| {
                    input.child(div().underline().child(marked.to_string()))
                })
                .when(focused && !value.is_empty(), |input| {
                    input.child(text_caret(&self.tokens, self.new_connection_caret_visible))
                }),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, _cx| {
                    this.text_input_anchors.insert(anchor.id, anchor);
                });
            },
        )
        .into_any_element()
    }

    fn render_sftp_file_list(
        &self,
        pane: SftpPane,
        _path: &str,
        files: Vec<SftpFileEntry>,
        selected: &HashSet<String>,
        loading: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut list = div()
            .id(("sftp-file-list-scroll", pane as u64))
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .bg(sftp_bg(theme.bg, has_background));

        if loading {
            return list
                .child(
                    div()
                        .w_full()
                        .py(px(48.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::LoaderCircle,
                            20.0,
                            rgb(theme.text_muted),
                        ))
                        .child(self.i18n.t("sftp.file_list.loading")),
                )
                .into_any_element();
        }

        if files.is_empty() {
            return list
                .child(
                    div()
                        .w_full()
                        .py(px(48.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .text_size(px(SFTP_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(
                            div()
                                .mb(px(8.0))
                                .opacity(0.4)
                                .child(Self::render_lucide_icon(
                                    LucideIcon::FolderOpen,
                                    32.0,
                                    rgb(theme.text_muted),
                                )),
                        )
                        .child(self.i18n.t("sftp.file_list.empty")),
                )
                .into_any_element();
        }

        for file in files {
            let name = file.name.clone();
            let row_file = file.clone();
            let context_file = file.clone();
            let display_name = if let Some(target) = file.symlink_target.as_ref() {
                format!("{} -> {target}", file.name)
            } else {
                file.name.clone()
            };
            let _metadata_fields_consumed = (&file.permissions, &file.owner, &file.group);
            let is_selected = selected.contains(&name);
            list = list.child(
                div()
                    .h(px(SFTP_ROW_HEIGHT))
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(px(8.0))
                    .py(px(4.0))
                    .border_b_1()
                    .border_color(rgba(theme.border << 8))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(if is_selected {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.text)
                    })
                    .bg(if is_selected {
                        rgba((theme.accent << 8) | SFTP_SELECTED_BG_ALPHA)
                    } else {
                        rgba(theme.bg << 8)
                    })
                    .hover(move |row| row.bg(sftp_hover_bg(theme.bg_hover, has_background)))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                if file.is_symlink {
                                    LucideIcon::Link2
                                } else if file.file_type == SftpFileType::Directory {
                                    LucideIcon::Folder
                                } else {
                                    LucideIcon::File
                                },
                                SFTP_ICON_MD,
                                if file.file_type == SftpFileType::Directory {
                                    rgb(SFTP_FOLDER_BLUE)
                                } else if file.is_symlink {
                                    rgb(theme.accent)
                                } else {
                                    rgb(theme.text_muted)
                                },
                            ))
                            .child(div().truncate().child(display_name)),
                    )
                    .child(
                        div()
                            .w(px(SFTP_SIZE_COL))
                            .text_align(gpui::TextAlign::Right)
                            .text_color(rgb(theme.text_muted))
                            .child(if file.file_type == SftpFileType::Directory {
                                "-".to_string()
                            } else {
                                format_file_size(file.size)
                            }),
                    )
                    .child(
                        div()
                            .w(px(SFTP_MODIFIED_COL))
                            .text_align(gpui::TextAlign::Right)
                            .text_color(rgb(theme.text_muted))
                            .child(format_modified(file.modified)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.sftp_view.context_menu = None;
                            if event.click_count >= 2 {
                                this.open_or_preview_sftp_file(pane, &row_file);
                            } else {
                                this.select_sftp_file(pane, name.clone(), event.modifiers);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.open_sftp_context_menu(
                                pane,
                                Some(context_file.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                            );
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }

        list.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.sftp_view.context_menu = None;
                this.clear_sftp_selection(pane);
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                this.open_sftp_context_menu(
                    pane,
                    None,
                    f32::from(event.position.x),
                    f32::from(event.position.y),
                );
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_sftp_transfer_queue(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active_count = self
            .sftp_view
            .transfers
            .iter()
            .filter(|item| {
                matches!(
                    item.state,
                    SftpTransferState::Active | SftpTransferState::Pending
                )
            })
            .count();
        let has_completed = self.sftp_view.transfers.iter().any(|item| {
            matches!(
                item.state,
                SftpTransferState::Completed | SftpTransferState::Cancelled
            )
        });

        div()
            .h(px(SFTP_QUEUE_HEIGHT))
            .flex_none()
            .flex()
            .flex_col()
            .bg(sftp_bg(theme.bg, has_background))
            .border_t_1()
            .border_color(sftp_border(theme.border, has_background))
            .child(
                div()
                    .h(px(29.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(8.0))
                    .py(px(4.0))
                    .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                    .border_b_1()
                    .border_color(sftp_border(theme.border, has_background))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(SFTP_TEXT_XS))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_muted))
                            .child(self.queue_title(active_count))
                            .when(true, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .text_color(rgb(theme.accent))
                                        .cursor_pointer()
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Clock,
                                            SFTP_ICON_SM,
                                            rgb(theme.accent),
                                        ))
                                        .child(
                                            self.i18n
                                                .t("sftp.queue.incomplete_count")
                                                .replace("{{count}}", "1"),
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.sftp_view.show_incomplete =
                                                    !this.sftp_view.show_incomplete;
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        ),
                                )
                            }),
                    )
                    .when(has_completed, |header| {
                        header.child(
                            div()
                                .h(px(24.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(self.tokens.radii.sm))
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(theme.text))
                                .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                .cursor_pointer()
                                .child(self.i18n.t("sftp.queue.clear_done"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.sftp_view.transfers.retain(|item| {
                                            !matches!(
                                                item.state,
                                                SftpTransferState::Completed
                                                    | SftpTransferState::Cancelled
                                            )
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        )
                    }),
            )
            .when(self.sftp_view.show_incomplete, |queue| {
                queue.child(self.render_sftp_incomplete_section(has_background, cx))
            })
            .child(
                div()
                    .id("sftp-transfer-queue-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .when(self.sftp_view.transfers.is_empty(), |body| {
                        body.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(SFTP_TEXT_SM))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("sftp.queue.empty")),
                        )
                    })
                    .children(self.sftp_view.transfers.iter().cloned().map(|transfer| {
                        self.render_sftp_transfer_row(transfer, has_background, cx)
                    })),
            )
            .into_any_element()
    }

    fn render_sftp_incomplete_section(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .border_b_1()
            .border_color(sftp_border(theme.border, has_background))
            .bg(sftp_panel_bg(theme.bg_card, has_background, 0xff))
            .child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(SFTP_TEXT_10))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sftp.queue.incomplete_title").to_uppercase()),
            )
            .child(
                div()
                    .id("sftp-incomplete-transfer-scroll")
                    .max_h(px(128.0))
                    .overflow_y_scroll()
                    .p(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .p(px(8.0))
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba((SFTP_YELLOW << 8) | 0x4d))
                            .bg(sftp_panel_bg(
                                theme.bg_panel,
                                has_background,
                                SFTP_PANEL_80_ALPHA,
                            ))
                            .text_size(px(SFTP_TEXT_XS))
                            .child(
                                div()
                                    .w(px(16.0))
                                    .text_center()
                                    .text_color(rgb(SFTP_YELLOW))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("↓"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_color(rgb(theme.text))
                                            .child("archive.tar"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .text_size(px(SFTP_TEXT_10))
                                            .text_color(rgb(theme.text_muted))
                                            .child("Download")
                                            .child("•")
                                            .child("42%")
                                            .child("•")
                                            .child("18.0 MB / 42.0 MB"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(SFTP_TEXT_10))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("sftp.queue.status_paused")),
                            )
                            .child(self.render_sftp_icon_button(
                                LucideIcon::Play,
                                self.i18n.t("sftp.queue.resume_tooltip"),
                                cx.listener(|this, _event, _window, cx| {
                                    this.sftp_view.show_incomplete = false;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_sftp_transfer_row(
        &self,
        transfer: SftpTransferItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let progress = if transfer.size == 0 {
            0.0
        } else {
            (transfer.transferred as f32 / transfer.size as f32).clamp(0.0, 1.0)
        };
        let status_color = match transfer.state {
            SftpTransferState::Error => SFTP_RED,
            SftpTransferState::Cancelled => SFTP_YELLOW,
            _ => theme.text_muted,
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .p(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(match transfer.state {
                SftpTransferState::Error => rgba((SFTP_RED << 8) | 0x80),
                SftpTransferState::Cancelled => rgba((SFTP_YELLOW << 8) | 0x4d),
                _ => rgba(theme.border << 8),
            })
            .bg(sftp_panel_bg(
                theme.bg_panel,
                has_background,
                SFTP_PANEL_80_ALPHA,
            ))
            .hover(move |row| row.border_color(rgb(theme.border)))
            .text_size(px(SFTP_TEXT_SM))
            .child(
                div()
                    .w(px(16.0))
                    .text_center()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(match transfer.direction {
                        SftpTransferDirection::Upload => "↑",
                        SftpTransferDirection::Download => "↓",
                    }),
            )
            .child(
                div()
                    .w(px(192.0))
                    .truncate()
                    .text_color(rgb(theme.text))
                    .child(transfer.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .h(px(6.0))
                            .w_full()
                            .overflow_hidden()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            .child(div().h_full().w(relative(progress)).bg(rgb(theme.accent))),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(SFTP_TEXT_10))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(
                                "{} / {}",
                                format_file_size(transfer.transferred),
                                format_file_size(transfer.size)
                            ))
                            .child(format!("{}%", (progress * 100.0).round() as u32)),
                    ),
            )
            .child(
                div()
                    .w(px(96.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(SFTP_TEXT_XS))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(status_color))
                    .child(self.transfer_status_text(&transfer)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(match transfer.state {
                        SftpTransferState::Completed => {
                            Self::render_lucide_icon(LucideIcon::Check, 16.0, rgb(SFTP_GREEN))
                        }
                        SftpTransferState::Cancelled | SftpTransferState::Error => {
                            Self::render_lucide_icon(
                                LucideIcon::ShieldQuestion,
                                16.0,
                                rgb(status_color),
                            )
                        }
                        _ => div().w(px(0.0)).into_any_element(),
                    })
                    .when(
                        matches!(
                            transfer.state,
                            SftpTransferState::Active | SftpTransferState::Pending
                        ),
                        |actions| {
                            actions.child(self.render_sftp_icon_button(
                                LucideIcon::Pause,
                                self.i18n.t("sftp.queue.pause_tooltip"),
                                cx.listener({
                                    let id = transfer.id;
                                    move |this, _event, _window, cx| {
                                        this.set_sftp_transfer_state(id, SftpTransferState::Paused);
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }),
                            ))
                        },
                    )
                    .when(transfer.state == SftpTransferState::Paused, |actions| {
                        actions.child(self.render_sftp_icon_button(
                            LucideIcon::Play,
                            self.i18n.t("sftp.queue.resume_tooltip"),
                            cx.listener({
                                let id = transfer.id;
                                move |this, _event, _window, cx| {
                                    this.set_sftp_transfer_state(id, SftpTransferState::Pending);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }),
                        ))
                    })
                    .child(self.render_sftp_icon_button(
                        LucideIcon::X,
                        self.i18n.t(
                            if matches!(
                                transfer.state,
                                SftpTransferState::Active
                                    | SftpTransferState::Pending
                                    | SftpTransferState::Paused
                            ) {
                                "sftp.queue.cancel_tooltip"
                            } else {
                                "sftp.queue.remove_tooltip"
                            },
                        ),
                        cx.listener({
                            let id = transfer.id;
                            move |this, _event, _window, cx| {
                                this.cancel_or_remove_sftp_transfer(id);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_sftp_context_menu(
        &self,
        menu: SftpContextMenu,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - SFTP_CONTEXT_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - SFTP_CONTEXT_MENU_MAX_HEIGHT - 8.0)
            .max(8.0);
        let selected_count = self.sftp_selected_names(menu.pane).len();
        let direction = if menu.pane == SftpPane::Local {
            SftpTransferDirection::Upload
        } else {
            SftpTransferDirection::Download
        };
        let transfer_label = if menu.pane == SftpPane::Local {
            self.i18n.t("sftp.context.upload")
        } else {
            self.i18n.t("sftp.context.download")
        };

        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(SFTP_CONTEXT_MENU_WIDTH))
            .p(px(SFTP_CONTEXT_MENU_PADDING))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(sftp_border(theme.border, has_background))
            .bg(sftp_panel_bg(theme.bg_elevated, has_background, 0xf2))
            .shadow_lg()
            .when(selected_count > 0, |menu_el| {
                menu_el.child(self.render_sftp_context_menu_item(
                    if menu.pane == SftpPane::Local {
                        LucideIcon::Upload
                    } else {
                        LucideIcon::Download
                    },
                    transfer_label,
                    false,
                    has_background,
                    cx.listener(move |this, _event, _window, cx| {
                        this.queue_sftp_transfers(menu.pane, direction);
                        this.sftp_view.context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .when_some(menu.file.clone(), |menu_el, file| {
                if file.file_type == SftpFileType::Directory {
                    menu_el
                } else {
                    menu_el.child(self.render_sftp_context_menu_item(
                        LucideIcon::Eye,
                        self.i18n.t("sftp.context.preview"),
                        false,
                        has_background,
                        cx.listener({
                            let file = file.clone();
                            move |this, _event, _window, cx| {
                                this.open_or_preview_sftp_file(menu.pane, &file);
                                this.sftp_view.context_menu = None;
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ))
                }
            })
            .when(menu.file.is_some() && selected_count == 1, |menu_el| {
                menu_el.child(self.render_sftp_context_menu_item(
                    LucideIcon::Pencil,
                    self.i18n.t("sftp.context.rename"),
                    false,
                    has_background,
                    cx.listener({
                        let file = menu.file.clone();
                        move |this, _event, _window, cx| {
                            if let Some(file) = file.as_ref() {
                                this.open_sftp_rename_dialog(menu.pane, file.name.clone());
                            }
                            this.sftp_view.context_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                ))
            })
            .when_some(menu.file.clone(), |menu_el, file| {
                menu_el.child(self.render_sftp_context_menu_item(
                    LucideIcon::Copy,
                    self.i18n.t("sftp.context.copy_path"),
                    false,
                    has_background,
                    cx.listener(move |this, _event, _window, cx| {
                        let base = match menu.pane {
                            SftpPane::Local => &this.sftp_view.local_path,
                            SftpPane::Remote => &this.sftp_view.remote_path,
                        };
                        cx.write_to_clipboard(ClipboardItem::new_string(join_sftp_path(
                            base, &file.name,
                        )));
                        this.sftp_view.context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .when(selected_count > 0, |menu_el| {
                menu_el.child(self.render_sftp_context_menu_item(
                    LucideIcon::Trash2,
                    self.i18n.t("sftp.context.delete"),
                    true,
                    has_background,
                    cx.listener(move |this, _event, _window, cx| {
                        let files = this.sftp_selected_names(menu.pane);
                        this.sftp_view.dialog = Some(SftpDialog::Delete {
                            pane: menu.pane,
                            files,
                        });
                        this.sftp_view.context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .child(
                div()
                    .h(px(1.0))
                    .my(px(SFTP_CONTEXT_MENU_PADDING))
                    .bg(sftp_border(theme.border, has_background)),
            )
            .child(self.render_sftp_context_menu_item(
                LucideIcon::FolderOpen,
                self.i18n.t("sftp.context.new_folder"),
                false,
                has_background,
                cx.listener(move |this, _event, _window, cx| {
                    this.open_sftp_new_folder_dialog(menu.pane);
                    this.sftp_view.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event, _window, cx| {
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_context_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        danger: bool,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let color = if danger { SFTP_RED } else { theme.text };
        div()
            .h(px(SFTP_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(self.tokens.radii.xs))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(color))
            .cursor_pointer()
            .hover(move |item| item.bg(sftp_hover_bg(theme.bg_hover, has_background)))
            .child(Self::render_lucide_icon(icon, SFTP_ICON_SM, rgb(color)))
            .child(div().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_sftp_dialog(
        &self,
        dialog: SftpDialog,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (title, description, body, primary) = match dialog.clone() {
            SftpDialog::Drives => (
                self.i18n.t("sftp.dialogs.select_drive"),
                self.i18n.t("sftp.dialogs.select_drive_desc"),
                self.render_sftp_drives_dialog_body(has_background, cx),
                None,
            ),
            SftpDialog::Rename { .. } => (
                self.i18n.t("sftp.dialogs.rename"),
                self.i18n.t("sftp.dialogs.rename_desc"),
                self.render_sftp_dialog_input("sftp.dialogs.rename_desc", cx),
                Some(self.i18n.t("sftp.dialogs.rename")),
            ),
            SftpDialog::NewFolder { .. } => (
                self.i18n.t("sftp.dialogs.new_folder"),
                self.i18n.t("sftp.dialogs.new_folder_desc"),
                self.render_sftp_dialog_input("sftp.dialogs.new_folder_placeholder", cx),
                Some(self.i18n.t("sftp.dialogs.create")),
            ),
            SftpDialog::Delete { files, .. } => (
                self.i18n.t("sftp.dialogs.delete"),
                self.i18n
                    .t("sftp.dialogs.delete_confirm")
                    .replace("{{count}}", &files.len().to_string()),
                self.render_sftp_delete_dialog_body(files, has_background),
                Some(self.i18n.t("sftp.dialogs.delete")),
            ),
            SftpDialog::Conflict => (
                self.i18n.t("sftp.conflict.title"),
                self.i18n.t("sftp.conflict.description"),
                self.render_sftp_conflict_body(has_background),
                Some(self.i18n.t("sftp.conflict.overwrite")),
            ),
            SftpDialog::Diff => (
                self.i18n.t("sftp.diff.title"),
                self.i18n.t("sftp.diff.description"),
                self.render_sftp_diff_body(has_background),
                Some(self.i18n.t("sftp.diff.close")),
            ),
            SftpDialog::Preview { name } => (
                name,
                self.i18n.t("sftp.preview.description"),
                self.render_sftp_preview_body(has_background),
                Some(self.i18n.t("sftp.preview.close")),
            ),
        };

        div()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(SFTP_DIALOG_OVERLAY_ALPHA))
            .child(
                div()
                    .w(px(match dialog {
                        SftpDialog::Diff | SftpDialog::Preview { .. } => 960.0,
                        _ => 512.0,
                    }))
                    .max_w(relative(0.9))
                    .max_h(relative(0.9))
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(sftp_panel_bg(theme.bg_elevated, has_background, 0xff))
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::Hsla::from(rgba(SFTP_DIALOG_SHADOW_ALPHA)),
                        offset: gpui::point(px(0.0), px(16.0)),
                        blur_radius: px(32.0),
                        spread_radius: px(0.0),
                    }])
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                            .child(
                                div()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(6.0))
                                    .text_size(px(SFTP_TEXT_SM))
                                    .text_color(rgb(theme.text_muted))
                                    .child(description),
                            ),
                    )
                    .child(body)
                    .child(self.render_sftp_dialog_footer(
                        dialog.clone(),
                        primary,
                        has_background,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_sftp_dialog_footer(
        &self,
        dialog: SftpDialog,
        primary: Option<String>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let footer = div()
            .px(px(16.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_end()
            .gap(px(8.0));

        if matches!(dialog, SftpDialog::Conflict) {
            return footer
                .justify_between()
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.skip"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.skip_older"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.keep_both"),
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                        .child(self.render_sftp_text_button(
                            self.i18n.t("sftp.conflict.overwrite"),
                            true,
                            cx.listener(|this, _event, _window, cx| {
                                this.accept_sftp_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }

        footer
            .child(self.render_sftp_text_button(
                self.i18n.t("sftp.dialogs.cancel"),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.close_sftp_dialog();
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .when_some(primary, |footer, label| {
                footer.child(self.render_sftp_text_button(
                    label,
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.accept_sftp_dialog();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .into_any_element()
    }

    fn render_sftp_drives_dialog_body(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .overflow_hidden()
                    .children(mock_drives().into_iter().map(|drive| {
                        let path = drive.path.clone();
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(12.0))
                            .py(px(10.0))
                            .border_b_1()
                            .border_color(rgba((theme.border << 8) | 0x80))
                            .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                            .hover(move |row| row.bg(rgb(theme.bg_hover)))
                            .cursor_pointer()
                            .child(Self::render_lucide_icon(
                                if drive.drive_type == "network" {
                                    LucideIcon::Network
                                } else {
                                    LucideIcon::HardDrive
                                },
                                16.0,
                                rgb(theme.text_muted),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .text_size(px(SFTP_TEXT_SM))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(drive.name)
                                            .when(drive.read_only, |row| {
                                                row.child(
                                                    div()
                                                        .rounded(px(self.tokens.radii.xs))
                                                        .px(px(4.0))
                                                        .py(px(2.0))
                                                        .text_size(px(SFTP_TEXT_10))
                                                        .bg(rgba((SFTP_YELLOW << 8) | 0x26))
                                                        .text_color(rgb(SFTP_YELLOW))
                                                        .child(
                                                            self.i18n.t("sftp.dialogs.readOnly"),
                                                        ),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .mt(px(2.0))
                                            .text_size(px(SFTP_TEXT_XS))
                                            .text_color(rgb(theme.text_muted))
                                            .child(path.clone()),
                                    )
                                    .child(
                                        div()
                                            .mt(px(2.0))
                                            .text_size(px(SFTP_TEXT_10))
                                            .text_color(rgb(theme.text_muted))
                                            .child(format!(
                                                "{} {} / {}",
                                                format_file_size(drive.available_space),
                                                self.i18n.t("sftp.dialogs.available"),
                                                format_file_size(drive.total_space),
                                            )),
                                    ),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.sftp_view.local_path = path.clone();
                                    this.sftp_view.local_path_input = path.clone();
                                    this.close_sftp_dialog();
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                    })),
            )
            .into_any_element()
    }

    fn render_sftp_delete_dialog_body(
        &self,
        files: Vec<String>,
        has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .id("sftp-drives-scroll")
                    .max_h(px(128.0))
                    .overflow_y_scroll()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(sftp_bg(theme.bg_sunken, has_background))
                    .p(px(8.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .children(files.into_iter().map(|file| div().child(file))),
            )
            .into_any_element()
    }

    fn render_sftp_dialog_input(
        &self,
        placeholder_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self.sftp_view.focused_input == Some(SftpInput::DialogValue);
        div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .h(px(36.0))
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if focused {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(rgba((theme.bg << 8) | 0x80))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .child(self.render_sftp_inline_text(
                        SftpInput::DialogValue,
                        &self.sftp_view.dialog_value,
                        placeholder_key,
                        focused,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view.focused_input = Some(SftpInput::DialogValue);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_sftp_conflict_body(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(sftp_panel_bg(theme.bg_panel, has_background, 0xff))
                    .text_size(px(SFTP_TEXT_SM))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("config.toml"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.render_sftp_file_compare_card(
                                "sftp.conflict.local_file",
                                true,
                                has_background,
                            )),
                    )
                    .child(
                        div()
                            .w(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowRight,
                                20.0,
                                rgb(theme.text_muted),
                            )),
                    )
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.render_sftp_file_compare_card(
                            "sftp.conflict.remote_file",
                            false,
                            has_background,
                        ),
                    )),
            )
            .into_any_element()
    }

    fn render_sftp_file_compare_card(
        &self,
        label_key: &'static str,
        newer: bool,
        has_background: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .p(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if newer {
                rgb(0x16a34a)
            } else {
                rgb(theme.border)
            })
            .bg(if newer {
                rgba(0x052e1680)
            } else {
                sftp_panel_bg(theme.bg_panel, has_background, 0xff)
            })
            .child(
                div()
                    .mb(px(8.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(label_key).to_uppercase()),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::HardDrive,
                        SFTP_ICON_MD,
                        rgb(theme.text_muted),
                    ))
                    .child("4.2 KB"),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .gap(px(8.0))
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Clock,
                        SFTP_ICON_MD,
                        rgb(theme.text_muted),
                    ))
                    .child("2026-05-07 14:30"),
            )
            .into_any_element()
    }

    fn render_sftp_diff_body(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        let lines = [
            ("", "1", "host = \"server\"", "", "1", "host = \"server\""),
            ("-", "2", "port = 22", "+", "2", "port = 2222"),
            ("", "3", "user = \"lipsc\"", "", "3", "user = \"lipsc\""),
        ];
        div()
            .h(px(480.0))
            .flex()
            .flex_col()
            .bg(sftp_bg(theme.bg_sunken, has_background))
            .child(
                div()
                    .flex()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .text_size(px(SFTP_TEXT_XS))
                    .child(
                        div()
                            .flex_1()
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba(0x7f1d1d33))
                            .text_color(rgb(0xfca5a5))
                            .child("Local: config.toml"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(rgba(0x14532d33))
                            .text_color(rgb(0x86efac))
                            .child("Remote: config.toml"),
                    ),
            )
            .child(
                div()
                    .id("sftp-diff-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(SFTP_TEXT_XS))
                    .children(lines.into_iter().map(|line| {
                        let removed = line.0 == "-";
                        let added = line.3 == "+";
                        div()
                            .flex()
                            .border_b_1()
                            .border_color(rgba((theme.border << 8) | 0x80))
                            .child(diff_cell(line.1, line.2, removed, theme.border, true))
                            .child(diff_cell(line.4, line.5, added, theme.border, false))
                    })),
            )
            .into_any_element()
    }

    fn render_sftp_preview_body(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        let body = if self.sftp_view.preview_loading {
            self.render_sftp_preview_text(self.i18n.t("common.loading"))
        } else if let Some(error) = &self.sftp_view.preview_error {
            self.render_sftp_preview_text(error.clone())
        } else if let Some(content) = &self.sftp_view.preview_content {
            self.render_sftp_preview_content(content)
        } else {
            self.render_sftp_preview_text(String::new())
        };
        div()
            .h(px(520.0))
            .flex()
            .flex_col()
            .bg(sftp_bg(theme.bg_sunken, has_background))
            .child(
                div()
                    .id("sftp-preview-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(16.0))
                    .text_color(rgb(theme.text))
                    .child(body),
            )
            .into_any_element()
    }

    fn render_sftp_preview_text(&self, text: String) -> AnyElement {
        div()
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(SFTP_TEXT_XS))
            .child(text)
            .into_any_element()
    }

    fn render_sftp_preview_content(&self, content: &PreviewContent) -> AnyElement {
        match content {
            PreviewContent::Image { mime_type, data } => {
                let source = format!("data:{mime_type};base64,{data}");
                self.render_sftp_preview_image(source, mime_type.clone())
            }
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Image,
            } => self.render_sftp_preview_image(std::path::PathBuf::from(path), mime_type.clone()),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind,
            } => self.render_sftp_preview_asset_placeholder(path, mime_type, kind.clone()),
            _ => self.render_sftp_preview_text(preview_content_text(content)),
        }
    }

    fn render_sftp_preview_image(
        &self,
        source: impl Into<gpui::ImageSource>,
        fallback_label: String,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        gpui::img(source)
            .w_full()
            .h(px(456.0))
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                div()
                    .w_full()
                    .h(px(456.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text_muted))
                    .child(fallback_label.clone())
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_sftp_preview_asset_placeholder(
        &self,
        path: &str,
        mime_type: &str,
        kind: AssetFileKind,
    ) -> AnyElement {
        // Non-image assets require the dedicated GPUI/WebView preview surface.
        // Keep the real temp asset path visible instead of masquerading as a media viewer.
        self.render_sftp_preview_text(format!("{kind:?} asset\n{mime_type}\n{path}"))
    }

    fn render_sftp_init_error(
        &self,
        error: &str,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((SFTP_YELLOW << 8) | 0x66))
            .bg(rgba((SFTP_YELLOW << 8) | 0x1a))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(self.tokens.ui.text))
            .child(format!("SFTP waiting for connection sync: {error}"))
            .child(self.render_sftp_text_button(
                "Retry".to_string(),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.sftp_view.init_error = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn render_sftp_icon_button(
        &self,
        icon: LucideIcon,
        _title: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size(px(SFTP_TOOL_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .text_color(rgb(theme.text))
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                SFTP_ICON_SM,
                rgb(theme.text),
            ))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_sftp_nav_button(
        &self,
        pane: SftpPane,
        target: &'static str,
        icon: LucideIcon,
        label_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_sftp_icon_button(
            icon,
            self.i18n.t(label_key),
            cx.listener(move |this, _event, _window, cx| {
                this.navigate_sftp_path(pane, target);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    fn render_sftp_refresh_button(&self, pane: SftpPane, cx: &mut Context<Self>) -> AnyElement {
        self.render_sftp_icon_button(
            LucideIcon::LoaderCircle,
            self.i18n.t("sftp.toolbar.refresh"),
            cx.listener(move |this, _event, _window, cx| {
                if pane == SftpPane::Remote {
                    this.sftp_view.remote_load_pending = true;
                    this.sftp_view.remote_loading = true;
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    fn render_sftp_transfer_button(
        &self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(24.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(theme.text))
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                SFTP_ICON_SM,
                rgb(theme.text),
            ))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.queue_sftp_transfers(pane, direction);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_text_button(
        &self,
        label: String,
        primary: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if primary {
                rgba(theme.text << 8)
            } else {
                rgb(theme.border)
            })
            .bg(if primary {
                rgb(theme.text)
            } else {
                rgba(theme.bg << 8)
            })
            .text_color(if primary {
                rgb(theme.bg)
            } else {
                rgb(theme.text)
            })
            .text_size(px(SFTP_TEXT_XS))
            .font_weight(gpui::FontWeight::MEDIUM)
            .hover(move |button| {
                if primary {
                    button.opacity(0.9)
                } else {
                    button.bg(rgb(theme.bg_hover))
                }
            })
            .cursor_pointer()
            .child(label)
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn queue_title(&self, active_count: usize) -> String {
        let mut title = self.i18n.t("sftp.queue.title").to_uppercase();
        if active_count > 0 {
            title.push(' ');
            title.push_str(
                &self
                    .i18n
                    .t("sftp.queue.active_count")
                    .replace("{{count}}", &active_count.to_string()),
            );
        }
        title
    }

    fn transfer_status_text(&self, transfer: &SftpTransferItem) -> String {
        match transfer.state {
            SftpTransferState::Pending => self.i18n.t("sftp.queue.status_waiting"),
            SftpTransferState::Active => "1.2 MB/s".to_string(),
            SftpTransferState::Paused => self.i18n.t("sftp.queue.status_paused"),
            SftpTransferState::Completed => self.i18n.t("sftp.queue.status_completed"),
            SftpTransferState::Cancelled => self.i18n.t("sftp.queue.status_cancelled"),
            SftpTransferState::Error => transfer
                .error
                .clone()
                .unwrap_or_else(|| self.i18n.t("sftp.queue.status_error")),
        }
    }

    pub(super) fn handle_sftp_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            match key {
                "a" => {
                    self.select_all_sftp_files(self.sftp_view.active_pane);
                    self.sftp_view.context_menu = None;
                    cx.notify();
                    return true;
                }
                "l" => {
                    self.start_sftp_path_edit(self.sftp_view.active_pane);
                    self.sftp_view.context_menu = None;
                    cx.notify();
                    return true;
                }
                _ => return false,
            }
        }
        if self.sftp_view.context_menu.is_some() && key == "escape" {
            self.sftp_view.context_menu = None;
            cx.notify();
            return true;
        }
        if self.sftp_view.dialog.is_some() && self.sftp_view.focused_input.is_none() {
            match key {
                "escape" => {
                    self.close_sftp_dialog();
                    cx.notify();
                    return true;
                }
                "enter" => {
                    self.accept_sftp_dialog();
                    cx.notify();
                    return true;
                }
                _ => {}
            }
        }
        if let Some(input) = self.sftp_view.focused_input {
            match key {
                "escape" => {
                    self.sftp_view.focused_input = None;
                    self.sftp_view.editing_local_path = false;
                    self.sftp_view.editing_remote_path = false;
                    self.ime_marked_text = None;
                    cx.notify();
                    return true;
                }
                "enter" => {
                    match input {
                        SftpInput::LocalPath | SftpInput::RemotePath => {
                            let pane = if input == SftpInput::LocalPath {
                                SftpPane::Local
                            } else {
                                SftpPane::Remote
                            };
                            self.commit_sftp_path_input(pane);
                        }
                        SftpInput::DialogValue => self.accept_sftp_dialog(),
                        _ => {}
                    }
                    cx.notify();
                    return true;
                }
                "backspace" => {
                    self.sftp_input_value_mut(input).pop();
                    cx.notify();
                    return true;
                }
                _ => {}
            }
        }
        match key {
            "escape" => {
                self.sftp_view.context_menu = None;
                self.sftp_view.focused_input = None;
                cx.notify();
                true
            }
            "enter" => {
                if let Some(file) = self.single_selected_sftp_file(self.sftp_view.active_pane) {
                    self.open_or_preview_sftp_file(self.sftp_view.active_pane, &file);
                    cx.notify();
                    true
                } else {
                    false
                }
            }
            "space" | " " => {
                if let Some(file) = self.single_selected_sftp_file(self.sftp_view.active_pane)
                    && file.file_type != SftpFileType::Directory
                {
                    self.sftp_view.dialog = Some(SftpDialog::Preview { name: file.name });
                    cx.notify();
                    return true;
                }
                false
            }
            "right" | "arrowright" => {
                if self.sftp_view.active_pane == SftpPane::Local
                    && !self.sftp_view.local_selected.is_empty()
                {
                    self.queue_sftp_transfers(SftpPane::Local, SftpTransferDirection::Upload);
                    cx.notify();
                    return true;
                }
                false
            }
            "left" | "arrowleft" => {
                if self.sftp_view.active_pane == SftpPane::Remote
                    && !self.sftp_view.remote_selected.is_empty()
                {
                    self.queue_sftp_transfers(SftpPane::Remote, SftpTransferDirection::Download);
                    cx.notify();
                    return true;
                }
                false
            }
            "delete" | "backspace" => {
                let files = self.sftp_selected_names(self.sftp_view.active_pane);
                if !files.is_empty() {
                    self.sftp_view.dialog = Some(SftpDialog::Delete {
                        pane: self.sftp_view.active_pane,
                        files,
                    });
                    cx.notify();
                    return true;
                }
                false
            }
            "f2" | "F2" => {
                if let Some(file) = self.single_selected_sftp_file(self.sftp_view.active_pane) {
                    self.open_sftp_rename_dialog(self.sftp_view.active_pane, file.name);
                    cx.notify();
                    return true;
                }
                false
            }
            "up" | "arrowup" => {
                self.move_sftp_selection(self.sftp_view.active_pane, -1);
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                self.move_sftp_selection(self.sftp_view.active_pane, 1);
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(super) fn sftp_input_value(&self, input: SftpInput) -> &str {
        match input {
            SftpInput::LocalPath => &self.sftp_view.local_path_input,
            SftpInput::RemotePath => &self.sftp_view.remote_path_input,
            SftpInput::LocalFilter => &self.sftp_view.local_filter,
            SftpInput::RemoteFilter => &self.sftp_view.remote_filter,
            SftpInput::DialogValue => &self.sftp_view.dialog_value,
        }
    }

    pub(super) fn sftp_input_value_mut(&mut self, input: SftpInput) -> &mut String {
        match input {
            SftpInput::LocalPath => &mut self.sftp_view.local_path_input,
            SftpInput::RemotePath => &mut self.sftp_view.remote_path_input,
            SftpInput::LocalFilter => &mut self.sftp_view.local_filter,
            SftpInput::RemoteFilter => &mut self.sftp_view.remote_filter,
            SftpInput::DialogValue => &mut self.sftp_view.dialog_value,
        }
    }

    fn set_sftp_path(&mut self, pane: SftpPane, path: String) {
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_path = path.clone();
                self.sftp_view.local_path_input = path.clone();
                self.sftp_view.editing_local_path = false;
                self.sftp_view.local_files = list_local_files(&path).unwrap_or_else(|error| {
                    vec![sftp_file_entry(
                        format!("Unable to read folder: {error}"),
                        path.clone(),
                        SftpFileType::File,
                        0,
                        None,
                    )]
                });
                self.sftp_view.local_selected.clear();
                self.sftp_view.local_last_selected = None;
            }
            SftpPane::Remote => {
                self.sftp_view.remote_path = path.clone();
                self.sftp_view.remote_path_input = path;
                self.sftp_view.editing_remote_path = false;
                self.sftp_view.remote_loading = true;
                self.sftp_view.remote_load_pending = true;
                self.sftp_view.remote_selected.clear();
                self.sftp_view.remote_last_selected = None;
            }
        }
        self.sftp_view.focused_input = None;
        self.sftp_view.context_menu = None;
    }

    fn start_sftp_path_edit(&mut self, pane: SftpPane) {
        self.sftp_view.active_pane = pane;
        match pane {
            SftpPane::Local => {
                self.sftp_view.editing_local_path = true;
                self.sftp_view.local_path_input = self.sftp_view.local_path.clone();
                self.sftp_view.focused_input = Some(SftpInput::LocalPath);
            }
            SftpPane::Remote => {
                self.sftp_view.editing_remote_path = true;
                self.sftp_view.remote_path_input = self.sftp_view.remote_path.clone();
                self.sftp_view.focused_input = Some(SftpInput::RemotePath);
            }
        }
    }

    fn commit_sftp_path_input(&mut self, pane: SftpPane) {
        let path = match pane {
            SftpPane::Local => self.sftp_view.local_path_input.trim().to_string(),
            SftpPane::Remote => normalize_remote_path(&self.sftp_view.remote_path_input),
        };
        if !path.is_empty() {
            self.set_sftp_path(pane, path);
        }
    }

    fn navigate_sftp_path(&mut self, pane: SftpPane, target: &str) {
        let next = match (pane, target) {
            (SftpPane::Local, "~") => home_path_mock(),
            (SftpPane::Remote, "~") => "/home/lipsc".to_string(),
            (SftpPane::Local, "..") => parent_path(&self.sftp_view.local_path, false),
            (SftpPane::Remote, "..") => parent_path(&self.sftp_view.remote_path, true),
            _ => target.to_string(),
        };
        self.set_sftp_path(pane, next);
    }

    fn toggle_sftp_sort(&mut self, pane: SftpPane, field: SftpSortField) {
        let (sort_field, sort_direction) = match pane {
            SftpPane::Local => (
                &mut self.sftp_view.local_sort_field,
                &mut self.sftp_view.local_sort_direction,
            ),
            SftpPane::Remote => (
                &mut self.sftp_view.remote_sort_field,
                &mut self.sftp_view.remote_sort_direction,
            ),
        };
        if *sort_field == field {
            *sort_direction = match *sort_direction {
                SftpSortDirection::Asc => SftpSortDirection::Desc,
                SftpSortDirection::Desc => SftpSortDirection::Asc,
            };
        } else {
            *sort_field = field;
            *sort_direction = SftpSortDirection::Asc;
        }
    }

    fn select_sftp_file(&mut self, pane: SftpPane, name: String, modifiers: gpui::Modifiers) {
        self.sftp_view.active_pane = pane;
        self.sftp_view.context_menu = None;
        let range_names = self.sftp_ordered_file_names(pane);
        let (selected, last_selected) = match pane {
            SftpPane::Local => (
                &mut self.sftp_view.local_selected,
                &mut self.sftp_view.local_last_selected,
            ),
            SftpPane::Remote => (
                &mut self.sftp_view.remote_selected,
                &mut self.sftp_view.remote_last_selected,
            ),
        };
        if modifiers.shift
            && let Some(last) = last_selected.as_ref()
            && let (Some(start), Some(end)) = (
                range_names.iter().position(|item| item == last),
                range_names.iter().position(|item| item == &name),
            )
        {
            selected.clear();
            let (min, max) = (start.min(end), start.max(end));
            selected.extend(range_names[min..=max].iter().cloned());
            *last_selected = Some(name);
            return;
        }
        if modifiers.platform || modifiers.control {
            if !selected.insert(name.clone()) {
                selected.remove(&name);
            }
        } else {
            selected.clear();
            selected.insert(name.clone());
        }
        *last_selected = Some(name);
    }

    fn clear_sftp_selection(&mut self, pane: SftpPane) {
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_selected.clear();
                self.sftp_view.local_last_selected = None;
            }
            SftpPane::Remote => {
                self.sftp_view.remote_selected.clear();
                self.sftp_view.remote_last_selected = None;
            }
        }
    }

    fn select_all_sftp_files(&mut self, pane: SftpPane) {
        let names = self.sftp_ordered_file_names(pane);
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_selected = names.iter().cloned().collect();
                self.sftp_view.local_last_selected = names.last().cloned();
            }
            SftpPane::Remote => {
                self.sftp_view.remote_selected = names.iter().cloned().collect();
                self.sftp_view.remote_last_selected = names.last().cloned();
            }
        }
    }

    fn move_sftp_selection(&mut self, pane: SftpPane, delta: isize) {
        let names = self.sftp_ordered_file_names(pane);
        if names.is_empty() {
            return;
        }
        let current = self
            .sftp_selected_names(pane)
            .first()
            .and_then(|name| names.iter().position(|candidate| candidate == name))
            .unwrap_or(if delta > 0 { names.len() - 1 } else { 0 });
        let next = if delta > 0 {
            (current + 1) % names.len()
        } else if current == 0 {
            names.len() - 1
        } else {
            current - 1
        };
        let name = names[next].clone();
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_selected.clear();
                self.sftp_view.local_selected.insert(name.clone());
                self.sftp_view.local_last_selected = Some(name);
            }
            SftpPane::Remote => {
                self.sftp_view.remote_selected.clear();
                self.sftp_view.remote_selected.insert(name.clone());
                self.sftp_view.remote_last_selected = Some(name);
            }
        }
    }

    fn sftp_ordered_file_names(&self, pane: SftpPane) -> Vec<String> {
        let (files, filter, field, direction) = match pane {
            SftpPane::Local => (
                &self.sftp_view.local_files,
                &self.sftp_view.local_filter,
                self.sftp_view.local_sort_field,
                self.sftp_view.local_sort_direction,
            ),
            SftpPane::Remote => (
                &self.sftp_view.remote_files,
                &self.sftp_view.remote_filter,
                self.sftp_view.remote_sort_field,
                self.sftp_view.remote_sort_direction,
            ),
        };
        sorted_sftp_files(files, filter, field, direction)
            .into_iter()
            .map(|file| file.name)
            .collect()
    }

    fn sftp_selected_names(&self, pane: SftpPane) -> Vec<String> {
        let selected = match pane {
            SftpPane::Local => &self.sftp_view.local_selected,
            SftpPane::Remote => &self.sftp_view.remote_selected,
        };
        self.sftp_ordered_file_names(pane)
            .into_iter()
            .filter(|name| selected.contains(name))
            .collect()
    }

    fn single_selected_sftp_file(&self, pane: SftpPane) -> Option<SftpFileEntry> {
        let selected = self.sftp_selected_names(pane);
        if selected.len() != 1 {
            return None;
        }
        let name = selected.first()?;
        let files = match pane {
            SftpPane::Local => &self.sftp_view.local_files,
            SftpPane::Remote => &self.sftp_view.remote_files,
        };
        files.iter().find(|file| &file.name == name).cloned()
    }

    fn open_or_preview_sftp_file(&mut self, pane: SftpPane, file: &SftpFileEntry) {
        self.sftp_view.active_pane = pane;
        self.sftp_view.context_menu = None;
        if file.file_type == SftpFileType::Directory {
            let base = match pane {
                SftpPane::Local => self.sftp_view.local_path.clone(),
                SftpPane::Remote => self.sftp_view.remote_path.clone(),
            };
            self.set_sftp_path(pane, join_sftp_path(&base, &file.name));
        } else {
            self.sftp_view.preview_path = Some(file.path.clone());
            self.sftp_view.preview_content = None;
            self.sftp_view.preview_error = None;
            self.sftp_view.preview_loading = pane == SftpPane::Remote;
            self.sftp_view.dialog = Some(SftpDialog::Preview {
                name: file.name.clone(),
            });
            if pane == SftpPane::Remote {
                self.spawn_remote_sftp_preview(file.path.clone());
            } else {
                self.sftp_view.preview_loading = true;
                self.spawn_local_sftp_preview(file.path.clone());
            }
        }
    }

    fn spawn_remote_sftp_preview(&self, path: String) {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let router = self.node_router.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = load_remote_sftp_preview(router, &node_id, &path).await;
            let _ = tx.send(SftpWorkerResult::PreviewLoaded { path, result });
        });
    }

    fn spawn_local_sftp_preview(&self, path: String) {
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = load_local_sftp_preview(&path).await;
            let _ = tx.send(SftpWorkerResult::PreviewLoaded { path, result });
        });
    }

    fn open_sftp_context_menu(
        &mut self,
        pane: SftpPane,
        file: Option<SftpFileEntry>,
        x: f32,
        y: f32,
    ) {
        self.sftp_view.active_pane = pane;
        if let Some(file) = file.as_ref() {
            let selected = match pane {
                SftpPane::Local => &mut self.sftp_view.local_selected,
                SftpPane::Remote => &mut self.sftp_view.remote_selected,
            };
            if !selected.contains(&file.name) {
                selected.clear();
                selected.insert(file.name.clone());
                match pane {
                    SftpPane::Local => self.sftp_view.local_last_selected = Some(file.name.clone()),
                    SftpPane::Remote => {
                        self.sftp_view.remote_last_selected = Some(file.name.clone())
                    }
                }
            }
        }
        self.sftp_view.context_menu = Some(SftpContextMenu { pane, file, x, y });
    }

    fn open_sftp_rename_dialog(&mut self, pane: SftpPane, old_name: String) {
        self.sftp_view.dialog_value = old_name.clone();
        self.sftp_view.dialog = Some(SftpDialog::Rename { pane, old_name });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn open_sftp_new_folder_dialog(&mut self, pane: SftpPane) {
        self.sftp_view.dialog_value.clear();
        self.sftp_view.dialog = Some(SftpDialog::NewFolder { pane });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn queue_sftp_transfers(&mut self, pane: SftpPane, direction: SftpTransferDirection) {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let selected = match pane {
            SftpPane::Local => self.sftp_view.local_selected.clone(),
            SftpPane::Remote => self.sftp_view.remote_selected.clone(),
        };
        if selected.is_empty() {
            return;
        }
        let source_files = match pane {
            SftpPane::Local => self.sftp_view.local_files.clone(),
            SftpPane::Remote => self.sftp_view.remote_files.clone(),
        };
        for name in selected {
            let file = source_files.iter().find(|file| file.name == name);
            let id = self.sftp_view.next_transfer_id;
            self.sftp_view.next_transfer_id += 1;
            let size = file.map(|file| file.size).unwrap_or_default().max(1);
            let is_directory = file.is_some_and(|file| file.file_type == SftpFileType::Directory);
            let local_path = match (direction, file) {
                (SftpTransferDirection::Upload, Some(file)) => file.path.clone(),
                _ => join_local_path(&self.sftp_view.local_path, &name),
            };
            let remote_path = match (direction, file) {
                (SftpTransferDirection::Download, Some(file)) => file.path.clone(),
                _ => join_sftp_path(&self.sftp_view.remote_path, &name),
            };
            self.sftp_view.transfers.push(SftpTransferItem {
                id,
                name: if is_directory {
                    format!("{name}/")
                } else {
                    name.clone()
                },
                local_path: local_path.clone(),
                remote_path: remote_path.clone(),
                direction,
                size,
                transferred: 0,
                state: SftpTransferState::Pending,
                error: None,
            });
            self.spawn_sftp_transfer_task(
                id,
                node_id.clone(),
                direction,
                is_directory,
                local_path,
                remote_path,
            );
        }
        self.clear_sftp_selection(pane);
    }

    fn spawn_sftp_transfer_task(
        &self,
        id: u64,
        node_id: NodeId,
        direction: SftpTransferDirection,
        is_directory: bool,
        local_path: String,
        remote_path: String,
    ) {
        let router = self.node_router.clone();
        let manager = self.sftp_transfer_manager.clone();
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let transfer_id = id.to_string();
            let mut directory_progress = is_directory.then(|| {
                let transfer_type = match direction {
                    SftpTransferDirection::Upload => RemoteTransferType::Upload,
                    SftpTransferDirection::Download => RemoteTransferType::Download,
                };
                let mut progress = StoredTransferProgress::new(
                    transfer_id.clone(),
                    transfer_type,
                    match direction {
                        SftpTransferDirection::Upload => local_path.clone().into(),
                        SftpTransferDirection::Download => remote_path.clone().into(),
                    },
                    match direction {
                        SftpTransferDirection::Upload => remote_path.clone().into(),
                        SftpTransferDirection::Download => local_path.clone().into(),
                    },
                    0,
                    format!("node:{}", node_id.0),
                );
                progress.strategy = RemoteTransferStrategy::DirectoryRecursive;
                progress
            });
            if let Some(progress) = directory_progress.as_ref() {
                let _ = progress_store.save(progress).await;
            }
            let _ = tx.send(SftpWorkerResult::TransferProgress {
                id,
                transferred: 0,
                total: 0,
                state: SftpTransferState::Active,
                error: None,
            });
            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::channel::<TransferProgress>(100);
            let progress_ui_tx = tx.clone();
            let progress_store_for_task = progress_store.clone();
            tokio::spawn(async move {
                let mut accumulator = DirectoryProgressAccumulator::default();
                while let Some(progress) = progress_rx.recv().await {
                    let progress = if is_directory {
                        accumulator.update(progress)
                    } else {
                        progress
                    };
                    if let Some(stored) = directory_progress.as_mut() {
                        stored.total_bytes = stored.total_bytes.max(progress.total_bytes);
                        stored.update_progress(progress.transferred_bytes);
                        let _ = progress_store_for_task.save(stored).await;
                    }
                    let _ = progress_ui_tx.send(SftpWorkerResult::TransferProgress {
                        id,
                        transferred: progress.transferred_bytes,
                        total: progress.total_bytes,
                        state: sftp_transfer_state_from_remote(progress.state),
                        error: progress.error,
                    });
                }
            });

            let result = async {
                let _permit = manager.acquire_permit().await;
                let sftp = router
                    .acquire_transfer_sftp(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                match (direction, is_directory) {
                    (SftpTransferDirection::Upload, true) => {
                        let resolved = router
                            .resolve_connection(&node_id)
                            .map_err(|error| error.to_string())?;
                        if probe_tar_support(&resolved.handle).await {
                            {
                                let shared = router
                                    .acquire_sftp(&node_id)
                                    .await
                                    .map_err(|error| error.to_string())?;
                                let shared = shared.lock().await;
                                for prefix in remote_directory_prefixes(&remote_path) {
                                    let _ = shared.mkdir(&prefix).await;
                                }
                            }
                            let compression = probe_tar_compression(&resolved.handle).await;
                            let tar_result = tar_upload_directory(
                                &resolved.handle,
                                &local_path,
                                &remote_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(compression),
                            )
                            .await;
                            match tar_result {
                                Ok(_) => {}
                                Err(error)
                                    if !manager
                                        .get_control(&transfer_id)
                                        .is_some_and(|control| control.is_cancelled()) =>
                                {
                                    sftp.upload_dir(
                                        &local_path,
                                        &remote_path,
                                        &transfer_id,
                                        Some(progress_tx),
                                        Some(manager.clone()),
                                    )
                                    .await
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar upload failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    })?;
                                }
                                Err(error) => return Err(error.to_string()),
                            }
                        } else {
                            sftp.upload_dir(
                                &local_path,
                                &remote_path,
                                &transfer_id,
                                Some(progress_tx),
                                Some(manager.clone()),
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                        }
                    }
                    (SftpTransferDirection::Upload, false) => {
                        sftp.upload_with_resume(
                            &local_path,
                            &remote_path,
                            progress_store.clone(),
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(transfer_id.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    }
                    (SftpTransferDirection::Download, true) => {
                        let resolved = router
                            .resolve_connection(&node_id)
                            .map_err(|error| error.to_string())?;
                        if probe_tar_support(&resolved.handle).await {
                            let compression = probe_tar_compression(&resolved.handle).await;
                            let tar_result = tar_download_directory(
                                &resolved.handle,
                                &remote_path,
                                &local_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(compression),
                            )
                            .await;
                            match tar_result {
                                Ok(_) => {}
                                Err(error)
                                    if !manager
                                        .get_control(&transfer_id)
                                        .is_some_and(|control| control.is_cancelled()) =>
                                {
                                    sftp.download_dir(
                                        &remote_path,
                                        &local_path,
                                        &transfer_id,
                                        Some(progress_tx),
                                        Some(manager.clone()),
                                    )
                                    .await
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar download failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    })?;
                                }
                                Err(error) => return Err(error.to_string()),
                            }
                        } else {
                            sftp.download_dir(
                                &remote_path,
                                &local_path,
                                &transfer_id,
                                Some(progress_tx),
                                Some(manager.clone()),
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                        }
                    }
                    (SftpTransferDirection::Download, false) => {
                        sftp.download_with_resume(
                            &remote_path,
                            &local_path,
                            progress_store.clone(),
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(transfer_id.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    }
                }
                Ok::<(), String>(())
            }
            .await
            .map_err(|error| error.to_string());

            if is_directory {
                match &result {
                    Ok(()) => {
                        let _ = progress_store.delete(&transfer_id).await;
                    }
                    Err(error) if error.to_ascii_lowercase().contains("cancel") => {
                        let _ = progress_store.delete(&transfer_id).await;
                    }
                    Err(error) => {
                        if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                            progress.mark_failed(error.clone());
                            let _ = progress_store.save(&progress).await;
                        }
                    }
                }
            }

            let _ = tx.send(SftpWorkerResult::TransferComplete {
                id,
                result,
                refresh_remote: matches!(direction, SftpTransferDirection::Upload),
                refresh_local: matches!(direction, SftpTransferDirection::Download),
            });
        });
    }

    fn set_sftp_transfer_state(&mut self, id: u64, state: SftpTransferState) {
        match state {
            SftpTransferState::Paused => {
                self.sftp_transfer_manager.pause(&id.to_string());
            }
            SftpTransferState::Pending | SftpTransferState::Active => {
                self.sftp_transfer_manager.resume(&id.to_string());
            }
            SftpTransferState::Cancelled => {
                self.sftp_transfer_manager.cancel(&id.to_string());
            }
            SftpTransferState::Completed | SftpTransferState::Error => {}
        }
        if let Some(item) = self
            .sftp_view
            .transfers
            .iter_mut()
            .find(|item| item.id == id)
        {
            item.state = state;
        }
    }

    fn cancel_or_remove_sftp_transfer(&mut self, id: u64) {
        if let Some(index) = self
            .sftp_view
            .transfers
            .iter()
            .position(|item| item.id == id)
        {
            let active = matches!(
                self.sftp_view.transfers[index].state,
                SftpTransferState::Active | SftpTransferState::Pending | SftpTransferState::Paused
            );
            if active {
                self.sftp_transfer_manager.cancel(&id.to_string());
                self.sftp_view.transfers[index].state = SftpTransferState::Cancelled;
            } else {
                self.sftp_view.transfers.remove(index);
            }
        }
    }

    fn spawn_remote_sftp_mutation<F>(&self, operation: F)
    where
        F: FnOnce(
                SftpSession,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
            > + Send
            + 'static,
    {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let router = self.node_router.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = async {
                let sftp = router
                    .acquire_transfer_sftp(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                operation(sftp).await
            }
            .await;
            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                result,
                refresh_remote: true,
                refresh_local: false,
            });
        });
    }

    fn close_sftp_dialog(&mut self) {
        self.sftp_view.dialog = None;
        self.sftp_view.dialog_value.clear();
        self.sftp_view.focused_input = None;
        self.ime_marked_text = None;
    }

    fn accept_sftp_dialog(&mut self) {
        let Some(dialog) = self.sftp_view.dialog.clone() else {
            return;
        };
        match dialog {
            SftpDialog::Rename { pane, old_name } => {
                let new_name = self.sftp_view.dialog_value.trim().to_string();
                if !new_name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let old_path = join_local_path(&self.sftp_view.local_path, &old_name);
                            let new_path = join_local_path(&self.sftp_view.local_path, &new_name);
                            let _ = std::fs::rename(old_path, new_path);
                            if let Ok(files) = list_local_files(&self.sftp_view.local_path) {
                                self.sftp_view.local_files = files;
                            }
                        }
                        SftpPane::Remote => {
                            let old_path = self
                                .sftp_view
                                .remote_files
                                .iter()
                                .find(|file| file.name == old_name)
                                .map(|file| file.path.clone())
                                .unwrap_or_else(|| {
                                    join_sftp_path(&self.sftp_view.remote_path, &old_name)
                                });
                            let new_path = join_sftp_path(&parent_path(&old_path, true), &new_name);
                            self.spawn_remote_sftp_mutation(move |sftp| {
                                Box::pin(async move {
                                    sftp.rename(&old_path, &new_path)
                                        .await
                                        .map_err(|error| error.to_string())
                                })
                            });
                        }
                    }
                }
            }
            SftpDialog::NewFolder { pane } => {
                let name = self.sftp_view.dialog_value.trim().to_string();
                if !name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let path = join_local_path(&self.sftp_view.local_path, &name);
                            let _ = std::fs::create_dir_all(path);
                            if let Ok(files) = list_local_files(&self.sftp_view.local_path) {
                                self.sftp_view.local_files = files;
                            }
                        }
                        SftpPane::Remote => {
                            let path = join_sftp_path(&self.sftp_view.remote_path, &name);
                            self.spawn_remote_sftp_mutation(move |sftp| {
                                Box::pin(async move {
                                    sftp.mkdir(&path).await.map_err(|error| error.to_string())
                                })
                            });
                        }
                    }
                }
            }
            SftpDialog::Delete { pane, files } => {
                match pane {
                    SftpPane::Local => {
                        for name in files {
                            let path = join_local_path(&self.sftp_view.local_path, &name);
                            if std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
                                let _ = std::fs::remove_dir_all(path);
                            } else {
                                let _ = std::fs::remove_file(path);
                            }
                        }
                        if let Ok(files) = list_local_files(&self.sftp_view.local_path) {
                            self.sftp_view.local_files = files;
                        }
                    }
                    SftpPane::Remote => {
                        let remote_files = self.sftp_view.remote_files.clone();
                        let targets = files
                            .into_iter()
                            .filter_map(|name| {
                                remote_files
                                    .iter()
                                    .find(|file| file.name == name)
                                    .map(|file| file.path.clone())
                            })
                            .collect::<Vec<_>>();
                        self.spawn_remote_sftp_mutation(move |sftp| {
                            Box::pin(async move {
                                for path in targets {
                                    sftp.delete_recursive(&path)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                }
                                Ok(())
                            })
                        });
                    }
                }
                self.clear_sftp_selection(pane);
            }
            _ => {}
        }
        self.close_sftp_dialog();
    }
}

#[derive(Clone)]
struct PathSegment {
    name: String,
    full_path: String,
}

fn sftp_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | SFTP_BG_ACTIVE_BG_ALPHA)
    } else {
        rgb(color)
    }
}

fn sftp_panel_bg(color: u32, has_background: bool, alpha: u32) -> Rgba {
    let alpha = if has_background {
        ((alpha as f32) * (SFTP_BG_ACTIVE_PANEL_ALPHA as f32 / 255.0)).round() as u32
    } else {
        alpha
    };
    rgba((color << 8) | alpha)
}

fn sftp_hover_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | SFTP_BG_ACTIVE_HOVER_ALPHA)
    } else {
        rgb(color)
    }
}

fn sftp_border(color: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba((color << 8) | 0x99)
    } else {
        rgb(color)
    }
}

fn home_path_mock() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/Users/lipsc".to_string())
}

fn list_local_files(path: &str) -> std::io::Result<Vec<SftpFileEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let full_path = entry.path().to_string_lossy().to_string();
        let file_type = if metadata.is_dir() {
            SftpFileType::Directory
        } else {
            SftpFileType::File
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64);
        entries.push(SftpFileEntry {
            name,
            path: full_path,
            file_type,
            size: metadata.len(),
            modified,
            permissions: None,
            owner: None,
            group: None,
            is_symlink: metadata.file_type().is_symlink(),
            symlink_target: std::fs::read_link(entry.path())
                .ok()
                .map(|target| target.to_string_lossy().to_string()),
        });
    }
    entries.sort_by(|left, right| match (left.file_type, right.file_type) {
        (SftpFileType::Directory, SftpFileType::File) => std::cmp::Ordering::Less,
        (SftpFileType::File, SftpFileType::Directory) => std::cmp::Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    });
    Ok(entries)
}

fn mock_drives() -> Vec<SftpDrive> {
    vec![
        SftpDrive {
            name: "Macintosh HD".to_string(),
            path: "/".to_string(),
            drive_type: "system",
            total_space: 512 * 1024 * 1024 * 1024,
            available_space: 128 * 1024 * 1024 * 1024,
            read_only: false,
        },
        SftpDrive {
            name: "Network Share".to_string(),
            path: "/Volumes/share".to_string(),
            drive_type: "network",
            total_space: 1024 * 1024 * 1024 * 1024,
            available_space: 620 * 1024 * 1024 * 1024,
            read_only: false,
        },
    ]
}

fn sftp_file_entry(
    name: String,
    path: String,
    file_type: SftpFileType,
    size: u64,
    modified: Option<i64>,
) -> SftpFileEntry {
    SftpFileEntry {
        name,
        path,
        file_type,
        size,
        modified,
        permissions: None,
        owner: None,
        group: None,
        is_symlink: false,
        symlink_target: None,
    }
}

fn sorted_sftp_files(
    files: &[SftpFileEntry],
    filter: &str,
    sort_field: SftpSortField,
    sort_direction: SftpSortDirection,
) -> Vec<SftpFileEntry> {
    let filter = filter.trim().to_lowercase();
    let mut filtered = files
        .iter()
        .filter(|file| filter.is_empty() || file.name.to_lowercase().contains(&filter))
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| {
        if left.file_type == SftpFileType::Directory && right.file_type != SftpFileType::Directory {
            return std::cmp::Ordering::Less;
        }
        if left.file_type != SftpFileType::Directory && right.file_type == SftpFileType::Directory {
            return std::cmp::Ordering::Greater;
        }
        let ordering = match sort_field {
            SftpSortField::Name => left.name.cmp(&right.name),
            SftpSortField::Size => left.size.cmp(&right.size),
            SftpSortField::Modified => left.modified.cmp(&right.modified),
        };
        match sort_direction {
            SftpSortDirection::Asc => ordering,
            SftpSortDirection::Desc => ordering.reverse(),
        }
    });
    filtered
}

fn sftp_path_segments(path: &str, is_remote: bool) -> Vec<PathSegment> {
    let normalized = if is_remote {
        normalize_remote_path(path)
    } else {
        path.replace('\\', "/")
    };
    let mut segments = Vec::new();
    segments.push(PathSegment {
        name: "/".to_string(),
        full_path: "/".to_string(),
    });
    let without_root = normalized.trim_start_matches('/');
    let mut current = String::from("/");
    for part in without_root.split('/').filter(|part| !part.is_empty()) {
        current = if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };
        segments.push(PathSegment {
            name: part.to_string(),
            full_path: current.clone(),
        });
    }
    segments
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    let normalized = trimmed.replace('\\', "/").replace("//", "/");
    if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

fn parent_path(path: &str, remote: bool) -> String {
    let normalized = if remote {
        normalize_remote_path(path)
    } else {
        path.replace('\\', "/")
    };
    if normalized == "/" {
        return "/".to_string();
    }
    let mut parts = normalized
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    parts.pop();
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn join_sftp_path(base: &str, name: &str) -> String {
    let normalized = base.trim_end_matches('/');
    if normalized.is_empty() {
        format!("/{name}")
    } else if normalized == "/" {
        format!("/{name}")
    } else {
        format!("{normalized}/{name}")
    }
}

fn remote_directory_prefixes(path: &str) -> Vec<String> {
    let mut prefixes = Vec::new();
    let absolute = path.starts_with('/');
    let components: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    for index in 0..components.len() {
        let joined = components[..=index].join("/");
        prefixes.push(if absolute {
            format!("/{joined}")
        } else {
            joined
        });
    }
    prefixes
}

fn join_local_path(base: &str, name: &str) -> String {
    let mut path = std::path::PathBuf::from(base);
    path.push(name);
    path.to_string_lossy().to_string()
}

fn sftp_transfer_state_from_remote(state: RemoteTransferState) -> SftpTransferState {
    match state {
        RemoteTransferState::Pending => SftpTransferState::Pending,
        RemoteTransferState::InProgress => SftpTransferState::Active,
        RemoteTransferState::Paused => SftpTransferState::Paused,
        RemoteTransferState::Completed => SftpTransferState::Completed,
        RemoteTransferState::Failed => SftpTransferState::Error,
        RemoteTransferState::Cancelled => SftpTransferState::Cancelled,
    }
}

fn preview_content_text(content: &PreviewContent) -> String {
    match content {
        PreviewContent::Text {
            data,
            encoding,
            confidence,
            has_bom,
            ..
        } => {
            let bom = if *has_bom { ", BOM" } else { "" };
            format!(
                "encoding: {encoding} ({:.0}%{bom})\n\n{data}",
                confidence * 100.0
            )
        }
        PreviewContent::Image { mime_type, data } => {
            format!(
                "{mime_type}\nimage preview payload: {} base64 chars",
                data.len()
            )
        }
        PreviewContent::AssetFile {
            path,
            mime_type,
            kind,
        } => {
            format!("{kind:?} asset\n{mime_type}\n{path}")
        }
        PreviewContent::Hex {
            data,
            total_size,
            offset,
            chunk_size,
            has_more,
        } => {
            format!(
                "hex preview: offset {offset}, chunk {chunk_size}, total {total_size}, has_more {has_more}\n\n{data}"
            )
        }
        PreviewContent::TooLarge {
            size,
            max_size,
            recommend_download,
        } => {
            format!(
                "too large to preview: {size} bytes (limit {max_size}), recommend_download={recommend_download}"
            )
        }
        PreviewContent::Unsupported { mime_type, reason } => {
            format!("unsupported preview: {mime_type}\n{reason}")
        }
    }
}

async fn load_remote_sftp_listing(
    router: NodeRouter,
    node_id: &NodeId,
    path: &str,
) -> Result<RemoteSftpListing, String> {
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(|error| error.to_string())?;
    match list_remote_sftp_once(&sftp, path).await {
        Ok(listing) => Ok(listing),
        Err(error) if error.is_channel_recoverable() => {
            let sftp = router
                .invalidate_and_reacquire_sftp(node_id)
                .await
                .map_err(|route_error| route_error.to_string())?;
            list_remote_sftp_once(&sftp, path)
                .await
                .map_err(|retry_error| retry_error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn load_remote_sftp_preview(
    router: NodeRouter,
    node_id: &NodeId,
    path: &str,
) -> Result<PreviewContent, String> {
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(|error| error.to_string())?;
    let sftp = sftp.lock().await;
    sftp.preview(path).await.map_err(|error| error.to_string())
}

async fn load_local_sftp_preview(path: &str) -> Result<PreviewContent, String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| error.to_string())?;
    let size = metadata.len();
    const MAX_LOCAL_TEXT_PREVIEW: u64 = 2 * 1024 * 1024;
    if size > MAX_LOCAL_TEXT_PREVIEW {
        return Ok(PreviewContent::TooLarge {
            size,
            max_size: MAX_LOCAL_TEXT_PREVIEW,
            recommend_download: false,
        });
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| error.to_string())?;
    match String::from_utf8(bytes.clone()) {
        Ok(data) => Ok(PreviewContent::Text {
            data,
            mime_type: None,
            language: None,
            encoding: "UTF-8".to_string(),
            confidence: 1.0,
            has_bom: false,
        }),
        Err(_) => Ok(PreviewContent::Hex {
            data: local_hex_dump(&bytes, 0),
            total_size: size,
            offset: 0,
            chunk_size: bytes.len() as u64,
            has_more: false,
        }),
    }
}

fn local_hex_dump(data: &[u8], offset: u64) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (index, chunk) in data.chunks(16).enumerate() {
        let address = offset + (index * 16) as u64;
        let _ = write!(output, "{address:08X}  ");
        for (column, byte) in chunk.iter().enumerate() {
            if column == 8 {
                output.push(' ');
            }
            let _ = write!(output, "{byte:02X} ");
        }
        output.push('\n');
    }
    output
}

async fn list_remote_sftp_once(
    sftp: &std::sync::Arc<tokio::sync::Mutex<SftpSession>>,
    path: &str,
) -> Result<RemoteSftpListing, SftpError> {
    let sftp = sftp.lock().await;
    let cwd = sftp.canonicalize(path).await?;
    let entries = sftp
        .list_dir(
            &cwd,
            Some(RemoteListFilter {
                show_hidden: true,
                pattern: None,
                sort: RemoteSortOrder::Name,
            }),
        )
        .await?;
    Ok(remote_listing_from_file_infos(cwd, entries))
}

fn remote_listing_from_file_infos(cwd: String, entries: Vec<RemoteFileInfo>) -> RemoteSftpListing {
    let mut files = entries
        .into_iter()
        .map(|entry| SftpFileEntry {
            name: entry.name,
            path: entry.path,
            file_type: match entry.file_type {
                RemoteFileType::Directory => SftpFileType::Directory,
                RemoteFileType::File | RemoteFileType::Symlink | RemoteFileType::Unknown => {
                    SftpFileType::File
                }
            },
            size: entry.size,
            modified: Some(entry.modified),
            permissions: Some(entry.permissions),
            owner: entry.owner,
            group: entry.group,
            is_symlink: entry.is_symlink,
            symlink_target: entry.symlink_target,
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| match (left.file_type, right.file_type) {
        (SftpFileType::Directory, SftpFileType::File) => std::cmp::Ordering::Less,
        (SftpFileType::File, SftpFileType::Directory) => std::cmp::Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    });
    RemoteSftpListing { cwd, files }
}

fn format_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index < units.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{} {}", value.round() as u64, units[index])
    } else {
        format!("{value:.1} {}", units[index])
    }
}

fn format_modified(modified: Option<i64>) -> String {
    if modified.is_some() {
        "2026/5/7".to_string()
    } else {
        "-".to_string()
    }
}

fn diff_cell(
    number: &str,
    content: &str,
    highlighted: bool,
    border: u32,
    left: bool,
) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .border_r_1()
        .border_color(rgb(border))
        .bg(if highlighted {
            if left {
                rgba(0x7f1d1d4d)
            } else {
                rgba(0x14532d4d)
            }
        } else {
            rgba(0x00000000)
        })
        .child(
            div()
                .w(px(48.0))
                .px(px(8.0))
                .py(px(2.0))
                .text_align(gpui::TextAlign::Right)
                .text_color(if highlighted {
                    if left { rgb(SFTP_RED) } else { rgb(SFTP_GREEN) }
                } else {
                    rgb(0xa1a1aa)
                })
                .border_r_1()
                .border_color(rgb(border))
                .child(number.to_string()),
        )
        .child(
            div()
                .flex_1()
                .px(px(8.0))
                .py(px(2.0))
                .child(content.to_string()),
        )
        .into_any_element()
}
