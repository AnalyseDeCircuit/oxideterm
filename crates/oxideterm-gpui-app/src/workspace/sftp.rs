use super::ime::WorkspaceImeTarget;
use super::*;
use gpui::{
    AnchoredPositionMode, Corner, Entity, ObjectFit, PathPromptOptions, Pixels, Point,
    SharedString, StyledText, Subscription, UniformListScrollHandle, anchored, deferred,
    prelude::*,
};
use oxideterm_code_editor::backend::input::{
    Input as CodeEditorInput, InputEvent as CodeEditorInputEvent,
    InputState as CodeEditorInputState,
};
use oxideterm_gpui_markdown::{
    MarkdownOptions, MarkdownVirtualListScrollHandle, highlight, markdown_virtual_with_code_actions,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    context_menu::{ContextMenuActionableStyle, context_menu_event_boundary},
    modal::{dismissible_dialog_backdrop, overlay_content_boundary, rounded_shell_child_radius},
    surface::{
        color_for_background, color_with_background_scaled_alpha, tauri_glass_surface_shadow,
    },
    text_input::{text_caret, text_input_anchor_probe},
};
use oxideterm_preview::{
    AudioPreviewBackend, AudioPreviewCommand, AudioPreviewState, PreviewAssetOwner, PreviewSession,
    RodioAudioPreviewBackend, font_family_name_from_bytes,
};
use oxideterm_sftp::{
    AssetFileKind, BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, FileInfo as RemoteFileInfo, FileType as RemoteFileType,
    ListFilter as RemoteListFilter, PreviewContent, SftpError, SftpSession,
    SortOrder as RemoteSortOrder, StoredTransferProgress, TarCompression, TransferProgress,
    TransferState as RemoteTransferState, TransferStrategy as RemoteTransferStrategy,
    TransferType as RemoteTransferType, encode_to_encoding, probe_tar_compression,
    probe_tar_support, tar_download_directory, tar_upload_directory,
};
use std::borrow::Cow;

pub(super) mod native_video;

use native_video::{SharedSftpNativeVideoSurface, sftp_native_video_element};

const SFTP_ROOT_PADDING: f32 = 8.0; // Tauri p-2
const SFTP_GAP: f32 = 8.0; // Tauri gap-2
const SFTP_PANE_HEADER_HEIGHT: f32 = 40.0; // Tauri h-10
const SFTP_QUEUE_HEIGHT: f32 = 192.0; // Tauri h-48
const SFTP_TRANSFER_QUEUE_LIST_INITIAL_ITEM_COUNT: usize = 0;
const SFTP_TRANSFER_QUEUE_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const SFTP_TRANSFER_QUEUE_LIST_OVERSCAN: usize = 6;
const SFTP_INCOMPLETE_TRANSFER_LIST_INITIAL_ITEM_COUNT: usize = 0;
const SFTP_INCOMPLETE_TRANSFER_LIST_ESTIMATED_HEIGHT: f32 = 52.0;
const SFTP_INCOMPLETE_TRANSFER_LIST_OVERSCAN: usize = 4;
const SFTP_TEXT_XS: f32 = 12.0; // Tauri text-xs
const SFTP_TEXT_SM: f32 = 14.0; // Tauri text-sm
const SFTP_TEXT_10: f32 = 10.0; // Tauri text-[10px]
const SFTP_ICON_SM: f32 = 12.0; // Tauri h-3 w-3
const SFTP_ICON_MD: f32 = 14.0; // Tauri h-3.5 w-3.5
const SFTP_TOOL_BUTTON: f32 = 24.0; // Tauri h-6 w-6
const SFTP_ROW_HEIGHT: f32 = 25.0; // Tauri px-2 py-1 text-xs
const SFTP_VIRTUAL_OVERSCAN: usize = 15; // Keep SFTP file panes aligned with FileList virtual overdraw.
const SFTP_DIFF_ROW_HEIGHT: f32 = 21.0; // Tauri FileDiffDialog text-xs py-0.5 border row
const SFTP_DIFF_VIRTUAL_OVERSCAN: usize = 15; // Diff dialog keeps the same file-list overdraw budget.
const SFTP_DIFF_LINE_NUMBER_COL: f32 = 48.0; // Tauri w-12
const SFTP_PREVIEW_CODE_LINE_HEIGHT: f32 = 20.0; // Tauri CodeHighlight text-xs leading-normal
const SFTP_PREVIEW_CODE_OVERSCAN: usize = 20; // Match Tauri VirtualTextPreview OVERSCAN_LINES.
const SFTP_PREVIEW_CODE_WRAP_COLUMNS: usize = 96; // GPUI virtual rows need soft-wrapped chunks instead of hidden overflow.
const SFTP_DIFF_WRAP_COLUMNS: usize = 64; // max-w-5xl split diff leaves roughly this many mono chars per side.
const SFTP_PREVIEW_FONT_DEFAULT_SIZE: f32 = 32.0; // Tauri FontPreview initial fontSize
const SFTP_SIZE_COL: f32 = 80.0; // Tauri w-20
const SFTP_MODIFIED_COL: f32 = 96.0; // Tauri w-24
const SFTP_DIRECTORY_PROGRESS_SAVE_INTERVAL_MS: u64 = 1_000; // Keep resume progress fresh without writing on every file tick.
const SFTP_BG_ACTIVE_BG_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg 40%
const SFTP_BG_ACTIVE_PANEL_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg-panel 40%
const SFTP_BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // [data-bg-active] --color-theme-bg-hover 50%
const SFTP_PANEL_80_ALPHA: u32 = 0xcc; // Tauri bg-theme-bg-panel/80
const SFTP_ACTIVE_BORDER_ALPHA: u32 = 0x80; // Tauri border-oxide-accent/50
const SFTP_HEADER_ACTIVE_BG_ALPHA: u32 = 0x80; // Tauri bg-theme-bg-hover/50
const SFTP_HEADER_ACTIVE_BORDER_ALPHA: u32 = 0x4d; // Tauri border-oxide-accent/30
const SFTP_TRANSFER_DEFAULT_BORDER_ALPHA: u32 = 0x00; // Tauri border-transparent until hover
const SFTP_TRANSFER_ERROR_BORDER_ALPHA: u32 = 0x80; // Tauri border-red-500/50
const SFTP_TRANSFER_CANCELLED_BORDER_ALPHA: u32 = 0x4d; // Tauri border-yellow-500/30
const SFTP_TRANSFER_INCOMPLETE_BORDER_ALPHA: u32 = 0x4d; // Tauri border-yellow-500/30
const SFTP_TRANSFER_INCOMPLETE_HOVER_BORDER_ALPHA: u32 = 0x80; // Tauri hover:border-yellow-500/50
const SFTP_TRANSFER_CONTROL_HOVER_ALPHA: u32 = 0x1a; // Tauri hover:bg-*-500/10
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
const SFTP_ORANGE: u32 = 0xfb923c; // Tauri text-orange-400
const SFTP_RED: u32 = 0xf87171; // Tauri text-red-400
const SFTP_CONTEXT_MENU_WIDTH: f32 = 180.0; // Tauri min-w-[180px]
const SFTP_CONTEXT_MENU_MAX_HEIGHT: f32 = 252.0; // 7 items + separators, clamped like fixed portal menu
const SFTP_CONTEXT_MENU_PADDING: f32 = 4.0; // Tauri py-1
const SFTP_CONTEXT_MENU_ITEM_HEIGHT: f32 = 30.0; // Tauri px-3 py-1.5 text-xs
const SFTP_BUTTON_TRANSPARENT_ALPHA: u32 = 0x00; // Tauri Button border-transparent/bg-transparent
const SFTP_DIALOG_SHADOW_ALPHA: u32 = 0x40; // Tauri shadow-lg-ish overlay shadow
const SFTP_DIALOG_BORDER_SUBTLE_ALPHA: u32 = 0x99; // Tauri border-theme-border/60
const SFTP_DIALOG_BORDER_HALF_ALPHA: u32 = 0x80; // Tauri border-theme-border/50
const SFTP_DIALOG_DIVIDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40
const SFTP_CONFIRM_ICON_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10
const SFTP_CONFIRM_ICON_RING_ALPHA: u32 = 0x33; // Tauri ring-theme-accent/20
const SFTP_CONFIRM_ACTION_HOVER_ALPHA: u32 = 0x1a; // Tauri hover:bg-theme-accent/10
const SFTP_EDITOR_RETRY_HOVER_ALPHA: u32 = 0x1a; // Tauri hover:bg-orange-500/10
const SFTP_CONFLICT_NEWER_BG_ALPHA: u32 = 0x4d; // Tauri bg-green-950/30
const SFTP_DIFF_HEADER_BG_ALPHA: u32 = 0x33; // Tauri bg-red/green-950/20
const SFTP_DIFF_LINE_BG_ALPHA: u32 = 0x4d; // Tauri bg-red/green-950/30
const SFTP_PREVIEW_CODE_GUTTER_ALPHA: u32 = 0x4d; // Tauri CodeHighlight line-number opacity 30%
const SFTP_READONLY_BADGE_BG_ALPHA: u32 = 0x26; // Tauri warning badge translucent fill
const SFTP_DIALOG_WIDTH_XS: f32 = 320.0; // Tauri max-w-xs
const SFTP_DIALOG_WIDTH_SM: f32 = 384.0; // Tauri max-w-sm
const SFTP_DIALOG_WIDTH_LG: f32 = 512.0; // Tauri max-w-lg
const SFTP_DIALOG_WIDTH_4XL: f32 = 896.0; // Tauri max-w-4xl
const SFTP_DIALOG_WIDTH_5XL: f32 = 1024.0; // Tauri max-w-5xl
const SFTP_EDITOR_DIALOG_WIDTH_6XL: f32 = 1152.0; // Tauri max-w-6xl
const SFTP_PREVIEW_DIALOG_HEIGHT_RATIO: f32 = 0.85; // Tauri SFTP preview/editor h-[85vh]
const SFTP_DIFF_DIALOG_HEIGHT_RATIO: f32 = 0.80; // Tauri FileDiffDialog h-[80vh]
const SFTP_HEX_PREVIEW_CHUNK_SIZE: u64 = 16 * 1024; // Tauri nodeSftpPreviewHex load-more step

fn sftp_file_list_virtual_spec() -> TauriVirtualListSpec {
    // Tauri SFTP FileList uses the same row estimate and overscan for rendering
    // and keyboard reveal. Keep them as one named native spec so scrollIntoView
    // parity does not split from virtualized row rendering.
    TauriVirtualListSpec::new(px(SFTP_ROW_HEIGHT), SFTP_VIRTUAL_OVERSCAN)
}

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
pub(super) enum SftpPane {
    Local,
    Remote,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpFileType {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpButtonVariant {
    Default,
    Secondary,
    Ghost,
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
pub(super) struct SftpMutationToast {
    success_title: String,
    success_description: Option<String>,
    error_title: String,
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
        speed: u64,
        state: SftpTransferState,
        error: Option<String>,
    },
    TransferComplete {
        node_id: NodeId,
        transfer_id: String,
        id: u64,
        result: Result<(), String>,
        refresh_remote: bool,
        refresh_local: bool,
    },
    ResumeIncompleteTransferLoaded {
        node_id: NodeId,
        transfer_id: String,
        result: Result<StoredTransferProgress, String>,
    },
    RemoteMutationComplete {
        result: Result<(), String>,
        refresh_remote: bool,
        refresh_local: bool,
        toast: Option<SftpMutationToast>,
    },
    IncompleteTransfersLoaded {
        node_id: NodeId,
        result: Result<Vec<StoredTransferProgress>, String>,
    },
    BackgroundTransfersLoaded {
        node_id: NodeId,
        result: Result<Vec<BackgroundTransferSnapshot>, String>,
    },
    PreviewLoaded {
        generation: u64,
        path: String,
        result: Result<PreviewContent, String>,
    },
    PreviewHexLoaded {
        generation: u64,
        path: String,
        offset: u64,
        result: Result<PreviewContent, String>,
    },
    PreviewSaved {
        generation: u64,
        path: String,
        content: String,
        encoding: String,
        result: Result<SftpPreviewSaveResult, String>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct RemoteSftpListing {
    cwd: String,
    files: Vec<SftpFileEntry>,
}

#[derive(Clone, Debug)]
pub(super) struct SftpPreviewSaveResult {
    mtime: Option<u64>,
    size: Option<u64>,
    encoding_used: String,
    atomic_write: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpConflictResolution {
    Skip,
    Overwrite,
    Rename,
    SkipOlder,
}

#[derive(Clone, Debug)]
struct SftpPendingTransfer {
    name: String,
    direction: SftpTransferDirection,
    source: SftpFileEntry,
}

#[derive(Clone, Debug)]
struct SftpConflictInfo {
    file_name: String,
    source_size: u64,
    source_modified: Option<i64>,
    target_size: u64,
    target_modified: Option<i64>,
    direction: SftpTransferDirection,
}

#[derive(Clone, Debug)]
struct SftpConflictState {
    conflicts: Vec<SftpConflictInfo>,
    current_index: usize,
    pending_transfers: Vec<SftpPendingTransfer>,
    resolved_actions: HashMap<String, SftpConflictResolution>,
    apply_to_all: bool,
}

#[derive(Clone, Debug)]
struct SftpDragState {
    source_pane: SftpPane,
    names: Vec<String>,
    start_x: f32,
    start_y: f32,
    active: bool,
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
    transfer_id: String,
    batch_id: Option<u64>,
    node_id: NodeId,
    name: String,
    local_path: String,
    remote_path: String,
    direction: SftpTransferDirection,
    size: u64,
    transferred: u64,
    speed: u64,
    state: SftpTransferState,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct SftpTransferBatch {
    direction: SftpTransferDirection,
    total: usize,
    success: usize,
    failed: usize,
    skipped: usize,
    queued: usize,
}

#[derive(Default)]
struct DirectoryProgressAccumulator {
    files: HashMap<(String, String), (u64, u64)>,
    transferred_bytes: u64,
    total_bytes: u64,
}

impl DirectoryProgressAccumulator {
    fn update(&mut self, progress: TransferProgress) -> TransferProgress {
        let key = (progress.remote_path.clone(), progress.local_path.clone());
        if let Some((previous_transferred, previous_total)) = self.files.get(&key).copied() {
            self.transferred_bytes = self.transferred_bytes.saturating_sub(previous_transferred);
            self.total_bytes = self.total_bytes.saturating_sub(previous_total);
        }

        // Directory transfers can emit many file progress events; keep aggregate
        // totals incrementally instead of re-summing the whole file map per tick.
        self.transferred_bytes = self
            .transferred_bytes
            .saturating_add(progress.transferred_bytes);
        self.total_bytes = self.total_bytes.saturating_add(progress.total_bytes);
        self.files
            .insert(key, (progress.transferred_bytes, progress.total_bytes));

        TransferProgress {
            transferred_bytes: self.transferred_bytes,
            total_bytes: self.total_bytes,
            ..progress
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(super) enum SftpDialog {
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
    Editor {
        name: String,
    },
    EditorCloseConfirm {
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
    drive_type: String,
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
    local_path_scroll_x: f32,
    remote_path_scroll_x: f32,
    diff_scroll: UniformListScrollHandle,
    preview_code_scroll: UniformListScrollHandle,
    preview_markdown_scroll: MarkdownVirtualListScrollHandle,
    local_last_selected: Option<String>,
    remote_last_selected: Option<String>,
    local_files: Vec<SftpFileEntry>,
    remote_files: Vec<SftpFileEntry>,
    remote_loading: bool,
    remote_load_pending: bool,
    remote_load_inflight: bool,
    remote_load_retry_count: u8,
    init_error: Option<String>,
    pub(super) focused_input: Option<SftpInput>,
    editing_local_path: bool,
    editing_remote_path: bool,
    pub(super) dialog: Option<SftpDialog>,
    conflict_state: Option<SftpConflictState>,
    dialog_value: String,
    preview_pane: Option<SftpPane>,
    preview_path: Option<String>,
    preview_content: Option<PreviewContent>,
    preview_asset_owner: Option<PreviewAssetOwner>,
    preview_session: PreviewSession,
    preview_generation: u64,
    preview_audio: RodioAudioPreviewBackend,
    preview_audio_tick_active: bool,
    preview_video_surface: SharedSftpNativeVideoSurface,
    preview_error: Option<String>,
    preview_loading: bool,
    preview_hex_loading_more: bool,
    preview_markdown_source_mode: bool,
    preview_font_family: Option<String>,
    preview_font_error: Option<String>,
    preview_font_size: f32,
    preview_editor_input: Option<Entity<CodeEditorInputState>>,
    preview_editor_subscription: Option<Subscription>,
    preview_editor_initial_content: String,
    preview_editor_language: Option<String>,
    preview_editor_encoding: String,
    preview_editor_dirty: bool,
    preview_editor_saving: bool,
    preview_editor_save_error: Option<String>,
    preview_editor_network_error: bool,
    preview_editor_retry_count: u32,
    preview_editor_last_saved_mtime: Option<u64>,
    preview_editor_last_atomic_write: Option<bool>,
    transfers: Vec<SftpTransferItem>,
    transfer_queue_list_state: ListState,
    transfer_queue_list_cache: RefCell<VirtualListSignatureCache>,
    transfer_batches: HashMap<u64, SftpTransferBatch>,
    incomplete_transfers: Vec<StoredTransferProgress>,
    incomplete_transfer_list_state: ListState,
    incomplete_transfer_list_cache: RefCell<VirtualListSignatureCache>,
    incomplete_load_inflight: bool,
    show_incomplete: bool,
    context_menu: Option<SftpContextMenu>,
    drag_state: Option<SftpDragState>,
    drag_over_pane: Option<SftpPane>,
    drag_autoscroll_position: Option<Point<Pixels>>,
    drag_autoscroll_scheduled: bool,
    next_transfer_id: u64,
    next_transfer_batch_id: u64,
}

impl Default for SftpViewState {
    fn default() -> Self {
        let local_path = home_path();
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
            local_path_scroll_x: 0.0,
            remote_path_scroll_x: 0.0,
            diff_scroll: UniformListScrollHandle::new(),
            preview_code_scroll: UniformListScrollHandle::new(),
            preview_markdown_scroll: MarkdownVirtualListScrollHandle::new(),
            local_last_selected: None,
            remote_last_selected: None,
            local_files: list_local_files(&local_path).unwrap_or_else(|_| Vec::new()),
            remote_files: Vec::new(),
            remote_loading: false,
            remote_load_pending: false,
            remote_load_inflight: false,
            remote_load_retry_count: 0,
            init_error: None,
            focused_input: None,
            editing_local_path: false,
            editing_remote_path: false,
            dialog: None,
            conflict_state: None,
            dialog_value: String::new(),
            preview_pane: None,
            preview_path: None,
            preview_content: None,
            preview_asset_owner: None,
            preview_session: PreviewSession::default(),
            preview_generation: 0,
            preview_audio: RodioAudioPreviewBackend::new(),
            preview_audio_tick_active: false,
            preview_video_surface: SharedSftpNativeVideoSurface::default(),
            preview_error: None,
            preview_loading: false,
            preview_hex_loading_more: false,
            preview_markdown_source_mode: false,
            preview_font_family: None,
            preview_font_error: None,
            preview_font_size: SFTP_PREVIEW_FONT_DEFAULT_SIZE,
            preview_editor_input: None,
            preview_editor_subscription: None,
            preview_editor_initial_content: String::new(),
            preview_editor_language: None,
            preview_editor_encoding: "UTF-8".to_string(),
            preview_editor_dirty: false,
            preview_editor_saving: false,
            preview_editor_save_error: None,
            preview_editor_network_error: false,
            preview_editor_retry_count: 0,
            preview_editor_last_saved_mtime: None,
            preview_editor_last_atomic_write: None,
            transfers: Vec::new(),
            // Transfer queues are fixed-height browser scroll regions; use the
            // shared variable list state so large transfer batches do not build
            // every row while progress/status updates are repainting.
            transfer_queue_list_state: ListState::new(
                SFTP_TRANSFER_QUEUE_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(SFTP_TRANSFER_QUEUE_LIST_ESTIMATED_HEIGHT),
                    SFTP_TRANSFER_QUEUE_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            transfer_queue_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            transfer_batches: HashMap::new(),
            incomplete_transfers: Vec::new(),
            // Incomplete transfer recovery is another fixed-height browser list;
            // keep its rows virtualized separately from the active queue because
            // loading/error rows follow a different identity set.
            incomplete_transfer_list_state: ListState::new(
                SFTP_INCOMPLETE_TRANSFER_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(SFTP_INCOMPLETE_TRANSFER_LIST_ESTIMATED_HEIGHT),
                    SFTP_INCOMPLETE_TRANSFER_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            incomplete_transfer_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            incomplete_load_inflight: false,
            show_incomplete: false,
            context_menu: None,
            drag_state: None,
            drag_over_pane: None,
            drag_autoscroll_position: None,
            drag_autoscroll_scheduled: false,
            next_transfer_id: 1,
            next_transfer_batch_id: 1,
        }
    }
}

impl SftpViewState {
    pub(super) fn current_remote_path(&self) -> &str {
        &self.remote_path
    }

    pub(super) fn selected_remote_files(&self) -> Vec<String> {
        let mut files = self.remote_selected.iter().cloned().collect::<Vec<_>>();
        files.sort();
        files
    }

    pub(super) fn dismiss_context_menu(&mut self) -> bool {
        // Root-level browser dismiss only needs to know whether a transient
        // menu existed; SFTP keeps the menu payload private to preserve row
        // action ownership inside the SFTP module.
        self.context_menu.take().is_some()
    }

    pub(super) fn has_drag_capture(&self) -> bool {
        // SFTP file drags use root-level pointer capture so releasing outside
        // both panes still clears the candidate and autoscroll state.
        self.drag_state.is_some() || self.drag_over_pane.is_some()
    }
}

include!("sftp/runtime.rs");
include!("sftp/surface.rs");
include!("sftp/file_list.rs");
include!("sftp/transfers.rs");
include!("sftp/menus.rs");
include!("sftp/dialogs.rs");
include!("sftp/controls.rs");
include!("sftp/actions.rs");
include!("sftp/helpers.rs");
