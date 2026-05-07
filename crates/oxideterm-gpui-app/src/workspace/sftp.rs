use super::ime::WorkspaceImeTarget;
use super::*;
use gpui::{
    AnchoredPositionMode, Corner, ObjectFit, StatefulInteractiveElement, UniformListScrollHandle,
    anchored, deferred, prelude::*, uniform_list,
};
use oxideterm_gpui_ui::{
    surface::{color_for_background, color_with_background_scaled_alpha},
    text_input::{text_caret, text_input_anchor_probe},
};
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
    Rename {
        pane: SftpPane,
        old_name: String,
    },
    NewFolder {
        pane: SftpPane,
    },
    Delete {
        pane: SftpPane,
        files: Vec<String>,
    },
    Conflict,
    Diff {
        local_path: String,
        local_content: String,
        remote_path: String,
        remote_content: String,
    },
    Preview {
        name: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpDiffLineKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Clone, Debug)]
struct SftpDiffLine {
    kind: SftpDiffLineKind,
    content: String,
    left_line_num: Option<usize>,
    right_line_num: Option<usize>,
}

#[derive(Default)]
struct SftpDiffStats {
    added: usize,
    removed: usize,
    unchanged: usize,
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
    local_file_scroll: UniformListScrollHandle,
    remote_file_scroll: UniformListScrollHandle,
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
            local_file_scroll: UniformListScrollHandle::new(),
            remote_file_scroll: UniformListScrollHandle::new(),
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

include!("sftp/runtime.rs");
include!("sftp/surface.rs");
include!("sftp/transfers.rs");
include!("sftp/menus.rs");
include!("sftp/dialogs.rs");
include!("sftp/controls.rs");
include!("sftp/actions.rs");
include!("sftp/helpers.rs");
