// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub const SFTP_PANE_SPLIT_DEFAULT_RATIO: f32 = 0.5;
pub const SFTP_QUEUE_DEFAULT_HEIGHT: f32 = 192.0; // Tauri h-48
pub const SFTP_ROW_HEIGHT: f32 = 25.0; // Tauri px-2 py-1 text-xs
pub const SFTP_PREVIEW_FONT_DEFAULT_SIZE: f32 = 32.0; // Tauri FontPreview initial fontSize

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SftpInput {
    LocalPath,
    RemotePath,
    LocalFilter,
    RemoteFilter,
    DialogValue,
}

impl SftpInput {
    pub fn anchor_key(self) -> u64 {
        match self {
            Self::LocalPath => 1,
            Self::RemotePath => 2,
            Self::LocalFilter => 3,
            Self::RemoteFilter => 4,
            Self::DialogValue => 5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SftpPaneResizeDrag {
    pub start_cursor_x: Pixels,
    pub start_ratio: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SftpQueueResizeDrag {
    pub start_cursor_y: Pixels,
    pub start_height: f32,
}

#[derive(Debug)]
pub struct SftpMutationToast {
    pub success_title: String,
    pub success_description: Option<String>,
    pub error_title: String,
}

// Surface identity prevents a hidden tab completion from replacing sidebar state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SftpSurfaceId {
    Tab(TabId),
    Sidebar,
}

/// Identifies the remote endpoint without implying NodeRouter ownership.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SftpRemoteId {
    Node(NodeId),
    Standalone(String),
    Ftp(String),
}

impl SftpRemoteId {
    pub fn storage_key(&self) -> String {
        match self {
            Self::Node(node_id) => node_id.0.clone(),
            Self::Standalone(profile_id) => format!("standalone-sftp:{profile_id}"),
            Self::Ftp(profile_id) => format!("ftp:{profile_id}"),
        }
    }

    pub fn node_id(&self) -> Option<&NodeId> {
        match self {
            Self::Node(node_id) => Some(node_id),
            Self::Standalone(_) | Self::Ftp(_) => None,
        }
    }

    pub fn standalone_endpoint_id(&self) -> Option<&str> {
        match self {
            Self::Node(_) | Self::Ftp(_) => None,
            Self::Standalone(endpoint_id) => Some(endpoint_id),
        }
    }
}

#[derive(Debug)]
pub enum SftpWorkerResult {
    StartRemoteLoad {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
    },
    RemoteList {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        view_generation: u64,
        session_id: String,
        path: String,
        result: Result<RemoteSftpListing, String>,
    },
    TreeChildren {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        view_generation: u64,
        tree_generation: u64,
        path: String,
        result: Result<RemoteSftpListing, String>,
    },
    PairPrimaryList {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        view_generation: u64,
        path: String,
        result: Result<RemoteSftpListing, String>,
    },
    RemotePathCompletion {
        generation: u64,
        remote_id: SftpRemoteId,
        parent_path: String,
        result: Result<Vec<PathCompletionCandidate>, String>,
    },
    PairPrimaryPathCompletion {
        generation: u64,
        remote_id: SftpRemoteId,
        parent_path: String,
        result: Result<Vec<PathCompletionCandidate>, String>,
    },
    TransferProgress {
        id: u64,
        transferred: u64,
        total: u64,
        speed: u64,
    },
    TransferProtocolResolved {
        id: u64,
        protocol: RemoteTransferProtocol,
    },
    TransferComplete {
        remote_id: SftpRemoteId,
        transfer_id: String,
        id: u64,
        result: Result<(), String>,
        refresh_remote: bool,
        refresh_local: bool,
    },
    ResumeIncompleteTransferLoaded {
        remote_id: SftpRemoteId,
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
        remote_id: SftpRemoteId,
        result: Result<Vec<StoredTransferProgress>, String>,
    },
    IncompleteTransferDiscarded {
        transfer_id: String,
        result: Result<(), String>,
    },
    BackgroundTransfersLoaded {
        remote_id: SftpRemoteId,
        result: Result<Vec<BackgroundTransferSnapshot>, String>,
    },
    PreviewLoaded {
        asset_owner: Option<PreviewAssetOwner>,
        generation: u64,
        path: String,
        result: Result<PreviewContent, String>,
    },
    PreviewHexLoaded {
        generation: u64,
        path: String,
        error_prefix: String,
        result: Result<PreviewContent, String>,
    },
    PreviewSaved {
        generation: u64,
        path: String,
        content: Arc<str>,
        network_error_message: String,
        result: Result<SftpPreviewSaveResult, String>,
    },
    LocalFilesLoaded {
        view_generation: u64,
        path: String,
        files: Vec<SftpFileEntry>,
    },
}

impl SftpWorkerResult {
    pub fn from_transfer_event(id: u64, event: oxideterm_sftp::BrowserTransferEvent) -> Self {
        use oxideterm_sftp::BrowserTransferEvent;

        match event {
            BrowserTransferEvent::ProtocolResolved(protocol) => {
                Self::TransferProtocolResolved { id, protocol }
            }
            BrowserTransferEvent::Progress {
                transferred,
                total,
                speed,
            } => Self::TransferProgress {
                id,
                transferred,
                total,
                speed,
            },
        }
    }
}

pub struct SftpRemoteLoadRequest {
    pub surface_id: SftpSurfaceId,
    pub remote_id: SftpRemoteId,
    pub view_generation: u64,
    pub path: String,
    pub audit_request: Option<oxideterm_audit::AuditContext>,
}

// Effects contain only stable identifiers, non-secret display errors, and
// owned runtime intents. Authentication material never crosses this boundary.
pub enum SftpWorkspaceEffect {
    BindSession {
        remote_id: SftpRemoteId,
        session_id: String,
        cwd: String,
    },
    LoadBackgroundTransfers {
        remote_id: SftpRemoteId,
    },
    LoadIncompleteTransfers {
        remote_id: SftpRemoteId,
    },
    RemoteLoadPending {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
    },
    StartRemoteLoad(SftpRemoteLoadRequest),
    TreeChildrenLoadFailed {
        path: String,
        error: String,
    },
    StartTreeChildrenLoad {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        view_generation: u64,
        tree_generation: u64,
        path: String,
    },
    TransferFinishedForReconnect {
        remote_id: SftpRemoteId,
        transfer_id: String,
        success: bool,
    },
    TransferBatchCompleted(SftpTransferBatch),
    StartTransfer(SftpTransferLaunch),
    Toast {
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    },
    ReloadPairPrimaryDirectory,
}

pub struct SftpWorkspaceEffects {
    pub delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
    pub(crate) effects: RefCell<VecDeque<SftpWorkspaceEffect>>,
}

impl SftpWorkspaceEffects {
    pub fn delivery(&self) -> &delivery::ActiveDeliverySender<SftpWorkerResult> {
        &self.delivery
    }

    pub fn take(&self) -> VecDeque<SftpWorkspaceEffect> {
        std::mem::take(&mut *self.effects.borrow_mut())
    }
}

pub enum SftpWorkspaceEvent {
    InputFocusChanged,
    WorkerEffectsReady(SftpWorkspaceEffects),
    OpenFileRequested {
        pane: SftpPane,
        file: SftpFileEntry,
    },
    TransferStateRequested {
        id: u64,
        state: SftpTransferState,
    },
    CancelOrRemoveTransferRequested {
        id: u64,
    },
    ResumeIncompleteTransferRequested {
        transfer_id: String,
    },
    DiscardIncompleteTransferRequested {
        transfer_id: String,
    },
    TooltipRequested {
        id: String,
        label: String,
        x: f32,
        y: f32,
    },
    TooltipCleared {
        id: String,
    },
    RemoteLoadReady {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
    },
    PreviewSaveRequested {
        expected_version: Option<String>,
        path: String,
        content: Arc<str>,
        encoding: Arc<str>,
        line_ending: TextLineEnding,
        generation: u64,
        delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
    },
}

#[derive(Clone, Debug)]
pub struct SftpContextMenu {
    pub pane: SftpPane,
    pub file: Option<SftpFileEntry>,
    pub x: f32,
    pub y: f32,
    /// Tree rows are not part of `remote_files`; their menu actions resolve
    /// absolute paths from the row instead of the flat-pane selection.
    pub tree: bool,
}

/// Window-anchored bookmark popover. The bookmark store is shared across tabs.
#[derive(Clone, Copy, Debug)]
pub struct SftpBookmarkMenu {
    pub x: f32,
    pub y: f32,
}

use oxideterm_sftp::browser_files::{SftpSortDirection, SftpSortField};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SftpConflictResolution {
    Skip,
    Overwrite,
    Rename,
    SkipOlder,
}

#[derive(Clone, Debug)]
pub struct SftpPendingTransfer {
    pub name: String,
    pub direction: SftpTransferDirection,
    pub source: SftpFileEntry,
    pub protocol_override: Option<RemoteTransferProtocol>,
}

#[derive(Clone, Debug)]
pub struct SftpConflictState {
    pub conflicts: Vec<SftpConflictInfo>,
    pub current_index: usize,
    pub pending_transfers: Vec<SftpPendingTransfer>,
    pub resolved_actions: HashMap<String, SftpConflictResolution>,
    pub apply_to_all: bool,
}

#[derive(Clone, Debug)]
pub struct SftpDragState {
    pub source_pane: SftpPane,
    pub names: Vec<String>,
    pub(crate) start_x: f32,
    pub(crate) start_y: f32,
    pub active: bool,
}

pub use oxideterm_sftp::SftpTransferState;
pub type SftpTransferItem = oxideterm_sftp::SftpTransferItem<SftpRemoteId>;

#[derive(Clone, Debug)]
pub struct SftpTransferBatch {
    pub direction: SftpTransferDirection,
    pub(crate) total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
    pub queued: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum SftpDialog {
    Drives,
    Rename {
        pane: SftpPane,
        old_name: String,
        /// Tree rows carry an absolute path; the flat pane resolves the name
        /// against its current directory.
        old_path: Option<String>,
    },
    BookmarkRename {
        bookmark_id: String,
    },
    NewFolder {
        pane: SftpPane,
        /// Tree rows create inside the focused directory, not the tree root.
        remote_path: Option<String>,
    },
    Delete {
        pane: SftpPane,
        files: Vec<String>,
        /// Absolute tree paths; empty for flat panes that resolve by name.
        paths: Vec<String>,
    },
    Conflict,
    Diff {
        view: Entity<SftpDiffView>,
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

#[derive(Clone, Debug)]
pub(crate) struct SftpDrive {
    pub name: String,
    pub path: String,
    pub drive_type: String,
    pub total_space: u64,
    pub available_space: u64,
    pub read_only: bool,
}

pub struct SftpDocumentPreview {
    view: gpui::AnyView,
    set_source: std::rc::Rc<dyn Fn(PreviewAssetOwner, &mut App)>,
}

impl SftpDocumentPreview {
    pub fn new<T: Render>(
        view: Entity<T>,
        set_source: fn(&mut T, PreviewAssetOwner, &mut Context<T>),
    ) -> Self {
        let target = view.clone();
        Self {
            view: view.into(),
            set_source: std::rc::Rc::new(move |owner, cx| {
                target.update(cx, |state, cx| set_source(state, owner, cx));
            }),
        }
    }

    pub(crate) fn view(&self) -> gpui::AnyView {
        self.view.clone()
    }

    pub(crate) fn set_source(&self, owner: PreviewAssetOwner, cx: &mut App) {
        (self.set_source)(owner, cx);
    }
}

pub struct SftpWorkspaceEntity {
    pub active_pane: SftpPane,
    pub local_path: String,
    pub remote_path: String,
    pub local_path_input: String,
    pub remote_path_input: String,
    pub local_path_completion: PathCompletionState,
    pub remote_path_completion: PathCompletionState,
    pub remote_path_completion_pending_selection: Option<(String, String)>,
    pub(crate) local_filter: String,
    pub remote_filter: String,
    pub(crate) local_sort_field: SftpSortField,
    pub(crate) remote_sort_field: SftpSortField,
    pub(crate) local_sort_direction: SftpSortDirection,
    pub(crate) remote_sort_direction: SftpSortDirection,
    pub local_selected: HashSet<String>,
    pub remote_selected: HashSet<String>,
    pub local_file_scroll: UniformListScrollHandle,
    pub file_row_height: f32,
    pub remote_file_scroll: UniformListScrollHandle,
    pub local_path_scroll: ScrollHandle,
    pub remote_path_scroll: ScrollHandle,
    pub surface_size: Option<gpui::Size<Pixels>>,
    pub pane_split_ratio: f32,
    pub pane_resize_drag: Option<SftpPaneResizeDrag>,
    pub queue_height: f32,
    pub queue_resize_drag: Option<SftpQueueResizeDrag>,
    pub preview_markdown_scroll: MarkdownVirtualListScrollHandle,
    pub preview_markdown_cache: MarkdownPreviewCache<PreviewContent>,
    pub preview_document_scroll: ScrollHandle,
    pub font_preview_scroll: ScrollHandle,
    pub drives_scroll: ScrollHandle,
    pub local_last_selected: Option<String>,
    pub remote_last_selected: Option<String>,
    // Sidebar-only tree browser. Paths are absolute remote paths.
    pub tree_expanded: HashSet<String>,
    pub tree_children: HashMap<String, Vec<SftpFileEntry>>,
    pub tree_loading: HashSet<String>,
    pub tree_pending: VecDeque<String>,
    /// Invalidates in-flight child listings when the tree is reset by a root
    /// reload or navigation, which do not change `view_generation`.
    pub tree_generation: u64,
    pub tree_selected: Option<String>,
    pub local_files: Vec<SftpFileEntry>,
    pub remote_files: Vec<SftpFileEntry>,
    pub remote_loading: bool,
    pub remote_load_pending: bool,
    pub remote_load_inflight: bool,
    pub(crate) remote_load_retry_count: u8,
    pub(crate) remote_load_retry_task: Option<Task<()>>,
    pub remote_browse_request: Option<(SftpRemoteId, String, oxideterm_audit::AuditContext)>,
    pub current_surface_id: Option<SftpSurfaceId>,
    pub current_remote_id: Option<SftpRemoteId>,
    pub pair_primary_remote_id: Option<SftpRemoteId>,
    pub pair_primary_loading: bool,
    pub local_path_by_remote: HashMap<SftpRemoteId, String>,
    pub(crate) remote_path_by_remote: HashMap<SftpRemoteId, String>,
    pub remote_home_by_remote: HashMap<SftpRemoteId, String>,
    pub view_generation: u64,
    pub init_error: Option<SftpViewError>,
    pub focused_input: Option<SftpInput>,
    pub editing_local_path: bool,
    pub editing_remote_path: bool,
    pub dialog: Option<SftpDialog>,
    pub dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(crate) dialog_exit_generation: Option<u64>,
    pub(crate) dialog_exit_task: Option<Task<()>>,
    pub conflict_state: Option<SftpConflictState>,
    pub dialog_value: String,
    pub preview_pane: Option<SftpPane>,
    pub preview_path: Option<String>,
    // Preview payloads can contain large text or media buffers. Renderers share
    // immutable snapshots instead of cloning the payload on every frame.
    pub preview_content: Option<Arc<PreviewContent>>,
    pub preview_asset_owner: Option<PreviewAssetOwner>,
    pub preview_plugin: Option<SftpDocumentPreview>,
    pub preview_generation: u64,
    pub preview_load_task: Option<tokio::task::AbortHandle>,
    pub preview_audio: RodioAudioPreviewBackend,
    pub(crate) preview_audio_tick_active: bool,
    pub(crate) preview_audio_tick_task: Option<Task<()>>,
    pub preview_video_surface: SharedSftpNativeVideoSurface,
    pub preview_error: Option<String>,
    pub preview_loading: bool,
    pub preview_hex_loading_more: bool,
    pub preview_markdown_source_mode: bool,
    pub preview_font_family: Option<String>,
    pub preview_font_error: Option<String>,
    pub preview_font_size: f32,
    pub preview_editor: Option<Entity<TextEditorView>>,
    pub preview_editor_observer: Option<Subscription>,
    pub preview_editor_initial_content: Arc<str>,
    pub preview_editor_observed_content: Arc<str>,
    pub preview_editor_language: Option<String>,
    pub preview_editor_encoding: String,
    pub preview_editor_line_ending: TextLineEnding,
    pub preview_editor_dirty: bool,
    pub preview_editor_saving: bool,
    pub preview_editor_save_error: Option<String>,
    pub preview_editor_network_error: bool,
    pub preview_editor_retry_count: u32,
    pub preview_editor_last_saved_mtime: Option<u64>,
    pub preview_editor_last_atomic_write: Option<bool>,
    pub(crate) preview_editor_retry_task: Option<Task<()>>,
    pub transfers: Vec<SftpTransferItem>,
    pub(crate) feature_views: SftpViews,
    pub(crate) transfer_batches: HashMap<u64, SftpTransferBatch>,
    pub incomplete_transfers: Vec<StoredTransferProgress>,
    pub incomplete_load_inflight: bool,
    pub(crate) incomplete_load_remote: Option<SftpRemoteId>,
    pub(crate) incomplete_load_pending_remote: Option<SftpRemoteId>,
    pub show_incomplete: bool,
    pub context_menu: Option<SftpContextMenu>,
    pub context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(crate) context_menu_exit_generation: Option<u64>,
    pub bookmark_menu: Option<SftpBookmarkMenu>,
    pub(crate) folder_picker_task: Option<Task<()>>,
    pub drag_state: Option<SftpDragState>,
    pub drag_over_pane: Option<SftpPane>,
    pub drag_autoscroll_position: Option<Point<Pixels>>,
    pub(crate) drag_autoscroll_scheduled: bool,
    pub next_transfer_id: u64,
    pub(crate) next_transfer_batch_id: u64,
    pub(crate) worker_tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    pub(crate) worker_rx: std::sync::mpsc::Receiver<SftpWorkerResult>,
}

impl Default for SftpWorkspaceEntity {
    fn default() -> Self {
        let local_path = default_download_path();
        let remote_path = String::new();
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        Self {
            active_pane: SftpPane::Remote,
            local_path_input: local_path.clone(),
            remote_path_input: remote_path.clone(),
            local_path_completion: PathCompletionState::default(),
            remote_path_completion: PathCompletionState::default(),
            remote_path_completion_pending_selection: None,
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
            file_row_height: SFTP_ROW_HEIGHT,
            remote_file_scroll: UniformListScrollHandle::new(),
            local_path_scroll: ScrollHandle::new(),
            remote_path_scroll: ScrollHandle::new(),
            surface_size: None,
            pane_split_ratio: SFTP_PANE_SPLIT_DEFAULT_RATIO,
            pane_resize_drag: None,
            queue_height: SFTP_QUEUE_DEFAULT_HEIGHT,
            queue_resize_drag: None,
            preview_markdown_scroll: MarkdownVirtualListScrollHandle::new(),
            preview_markdown_cache: MarkdownPreviewCache::default(),
            preview_document_scroll: ScrollHandle::new(),
            font_preview_scroll: ScrollHandle::new(),
            drives_scroll: ScrollHandle::new(),
            local_last_selected: None,
            remote_last_selected: None,
            tree_expanded: HashSet::new(),
            tree_children: HashMap::new(),
            tree_loading: HashSet::new(),
            tree_pending: VecDeque::new(),
            tree_generation: 0,
            tree_selected: None,
            local_files: list_local_files(&local_path).unwrap_or_else(|_| Vec::new()),
            remote_files: Vec::new(),
            remote_loading: false,
            remote_load_pending: false,
            remote_load_inflight: false,
            remote_load_retry_count: 0,
            remote_load_retry_task: None,
            remote_browse_request: None,
            current_surface_id: None,
            current_remote_id: None,
            pair_primary_remote_id: None,
            pair_primary_loading: false,
            local_path_by_remote: HashMap::new(),
            remote_path_by_remote: HashMap::new(),
            remote_home_by_remote: HashMap::new(),
            view_generation: 0,
            init_error: None,
            focused_input: None,
            editing_local_path: false,
            editing_remote_path: false,
            dialog: None,
            dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            dialog_exit_generation: None,
            dialog_exit_task: None,
            conflict_state: None,
            dialog_value: String::new(),
            preview_pane: None,
            preview_path: None,
            preview_content: None,
            preview_asset_owner: None,
            preview_plugin: None,
            preview_generation: 0,
            preview_load_task: None,
            preview_audio: RodioAudioPreviewBackend::new(),
            preview_audio_tick_active: false,
            preview_audio_tick_task: None,
            preview_video_surface: SharedSftpNativeVideoSurface::default(),
            preview_error: None,
            preview_loading: false,
            preview_hex_loading_more: false,
            preview_markdown_source_mode: false,
            preview_font_family: None,
            preview_font_error: None,
            preview_font_size: SFTP_PREVIEW_FONT_DEFAULT_SIZE,
            preview_editor: None,
            preview_editor_observer: None,
            preview_editor_initial_content: Arc::from(""),
            preview_editor_observed_content: Arc::from(""),
            preview_editor_language: None,
            preview_editor_encoding: "UTF-8".to_string(),
            preview_editor_line_ending: TextLineEnding::Lf,
            preview_editor_dirty: false,
            preview_editor_saving: false,
            preview_editor_save_error: None,
            preview_editor_network_error: false,
            preview_editor_retry_count: 0,
            preview_editor_last_saved_mtime: None,
            preview_editor_last_atomic_write: None,
            preview_editor_retry_task: None,
            transfers: Vec::new(),
            feature_views: SftpViews::default(),
            transfer_batches: HashMap::new(),
            incomplete_transfers: Vec::new(),
            incomplete_load_inflight: false,
            incomplete_load_remote: None,
            incomplete_load_pending_remote: None,
            show_incomplete: false,
            context_menu: None,
            context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            context_menu_exit_generation: None,
            bookmark_menu: None,
            folder_picker_task: None,
            drag_state: None,
            drag_over_pane: None,
            drag_autoscroll_position: None,
            drag_autoscroll_scheduled: false,
            next_transfer_id: 1,
            next_transfer_batch_id: 1,
            worker_tx,
            worker_rx,
        }
    }
}

impl SftpWorkspaceEntity {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let entity = Self::default();
        entity.schedule_worker_delivery(cx);
        entity
    }

    pub fn worker_sender(&self) -> delivery::ActiveDeliverySender<SftpWorkerResult> {
        // Background operations receive only the shallow delivery endpoint.
        self.worker_tx.clone()
    }

    pub(crate) fn schedule_worker_delivery(&self, cx: &mut Context<Self>) {
        let worker_wake = self.worker_tx.wake();
        let release_wake = worker_wake.clone();
        cx.on_release(move |this, _| {
            if let Some(task) = this.preview_load_task.take() {
                task.abort();
            }
            // Releasing the page owner stops UI delivery without cancelling
            // node-owned transfers or their backend tasks.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                worker_wake.wait().await;
                let should_drain = worker_wake.take();
                let stopped = worker_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |sftp, cx| sftp.drain_worker_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        // Continue one bounded batch at a time without a root heartbeat.
                        worker_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.worker_rx, delivery::LIFECYCLE_DELIVERY_BUDGET);
        let mut effects = VecDeque::new();
        let mut changed = false;
        let mut queue_changed = false;
        for result in delivery_batch.items {
            let queue_only = matches!(
                result,
                SftpWorkerResult::TransferProgress { .. }
                    | SftpWorkerResult::TransferProtocolResolved { .. }
            );
            if self.reduce_worker_result(result, &mut effects, cx) {
                if queue_only {
                    queue_changed = true;
                } else {
                    changed = true;
                }
            }
        }
        if changed {
            cx.notify();
        } else if queue_changed {
            self.notify_transfer_queue(cx);
        }
        if !effects.is_empty() {
            // State is fully reduced before observers can consume cross-system work.
            cx.emit(SftpWorkspaceEvent::WorkerEffectsReady(
                SftpWorkspaceEffects {
                    delivery: self.worker_tx.clone(),
                    effects: RefCell::new(effects),
                },
            ));
        }
        delivery_batch.outcome.backlog_remaining
    }

    pub fn input_value(&self, input: SftpInput) -> &str {
        match input {
            SftpInput::LocalPath => &self.local_path_input,
            SftpInput::RemotePath => &self.remote_path_input,
            SftpInput::LocalFilter => &self.local_filter,
            SftpInput::RemoteFilter => &self.remote_filter,
            SftpInput::DialogValue => &self.dialog_value,
        }
    }

    pub fn input_value_mut(&mut self, input: SftpInput) -> &mut String {
        match input {
            SftpInput::LocalPath => &mut self.local_path_input,
            SftpInput::RemotePath => &mut self.remote_path_input,
            SftpInput::LocalFilter => &mut self.local_filter,
            SftpInput::RemoteFilter => &mut self.remote_filter,
            SftpInput::DialogValue => &mut self.dialog_value,
        }
    }

    pub fn focused_input(&self) -> Option<SftpInput> {
        self.focused_input
    }

    pub(crate) fn set_input_focus(
        &mut self,
        input: Option<SftpInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focused_input == input {
            return false;
        }
        self.focused_input = input;
        cx.emit(SftpWorkspaceEvent::InputFocusChanged);
        true
    }

    pub fn refresh_ime(&self, input: SftpInput, ime: SftpDialogIme, cx: &mut Context<Self>) {
        if input == SftpInput::DialogValue {
            if let Some(view) = &self.feature_views.dialog {
                view.update(cx, |view, cx| view.refresh_ime(ime, cx));
            }
        } else if let Some(view) = &self.feature_views.browser {
            view.update(cx, |view, cx| view.refresh_ime(input, ime, cx));
        }
    }

    pub fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.set_input_focus(None, cx);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn dialog(&self) -> Option<SftpDialog> {
        self.dialog.clone()
    }

    pub fn dialog_is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn preview_text_editor_visible(&self) -> bool {
        self.preview_editor.is_some()
            && match self.dialog.as_ref() {
                Some(SftpDialog::Editor { .. }) => true,
                Some(SftpDialog::Preview { .. }) => matches!(self.preview_content.as_deref(),
                Some(PreviewContent::Text { language, mime_type, .. })
                    if self.preview_markdown_source_mode || !sftp_preview_is_markdown(language.as_deref(), mime_type.as_deref())),
                _ => false,
            }
    }

    pub fn dialog_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        // Root keyboard routing must stop treating a fading editor as an
        // interactive child before its payload is finally retired.
        self.dialog_presence.phase()
    }

    pub fn start_folder_picker(
        &mut self,
        selection: impl std::future::Future<Output = Option<String>> + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.folder_picker_task.is_some() {
            return;
        }
        let audit_context =
            oxideterm_local_files::local_file_audit_context(oxideterm_audit::AuditSource::User);
        self.folder_picker_task = Some(cx.spawn(async move |entity, cx| {
            let selected_path = selection.await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.folder_picker_task = None;
                if let Some(path) = selected_path {
                    if let Some(remote_id) = sftp.current_remote_id.clone() {
                        sftp.local_path_by_remote.insert(remote_id, path.clone());
                    }
                    let audit = oxideterm_audit::AuditOperation::in_context(
                        audit_context.as_ref(),
                        oxideterm_audit::AuditCategory::File,
                        "file_browse",
                        Some(&path),
                    );
                    sftp.set_input_focus(None, cx);
                    let result = sftp.apply_local_path(path);
                    audit.result(&result);
                    cx.notify();
                }
            });
        }));
    }

    pub fn begin_dialog_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        if self.dialog.is_none() {
            return false;
        }
        self.stop_preview_media();
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let Some(generation) = self.dialog_presence.begin_exit() else {
            return false;
        };
        self.set_input_focus(None, cx);
        if delay.is_zero() {
            self.finish_dialog_exit(generation, cx);
            return true;
        }
        self.dialog_exit_generation = Some(generation);
        self.dialog_exit_task = Some(cx.spawn(async move |entity, cx| {
            gpui::Timer::after(delay).await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.finish_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn finish_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.dialog_presence.finish_exit(generation) {
            return false;
        }
        self.dialog = None;
        self.dialog_exit_generation = None;
        self.dialog_exit_task = None;
        self.conflict_state = None;
        self.dialog_value.clear();
        self.preview_asset_owner = None;
        self.preview_plugin = None;
        self.preview_content = None;
        self.preview_path = None;
        self.preview_pane = None;
        self.preview_error = None;
        self.preview_loading = false;
        self.preview_hex_loading_more = false;
        self.preview_markdown_source_mode = false;
        self.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.preview_markdown_cache = MarkdownPreviewCache::default();
        self.preview_font_family = None;
        self.preview_font_error = None;
        self.preview_font_size = SFTP_PREVIEW_FONT_DEFAULT_SIZE;
        self.reset_preview_editor();
        self.set_input_focus(None, cx);
        cx.notify();
        true
    }

    pub fn reset_preview_editor(&mut self) {
        self.preview_editor = None;
        self.preview_editor_observer = None;
        self.preview_editor_initial_content = Arc::from("");
        self.preview_editor_observed_content = Arc::from("");
        self.preview_editor_language = None;
        self.preview_editor_encoding = "UTF-8".to_string();
        self.preview_editor_line_ending = TextLineEnding::Lf;
        self.preview_editor_dirty = false;
        self.preview_editor_saving = false;
        self.preview_editor_save_error = None;
        self.preview_editor_network_error = false;
        self.preview_editor_retry_count = 0;
        self.preview_editor_last_saved_mtime = None;
        self.preview_editor_last_atomic_write = None;
        self.preview_editor_retry_task = None;
    }

    pub fn stop_preview_media(&mut self) {
        if let Some(task) = self.preview_load_task.take() {
            task.abort();
        }
        self.preview_plugin = None;
        let _ = self.preview_audio.command(AudioPreviewCommand::Stop);
        self.preview_audio_tick_active = false;
        self.preview_audio_tick_task = None;
        self.preview_video_surface.detach();
    }

    pub fn toggle_preview_audio(&mut self, cx: &mut Context<Self>) {
        let _ = self.preview_audio.command(AudioPreviewCommand::PlayPause);
        self.schedule_preview_audio_tick(cx);
    }

    pub fn seek_preview_audio(&mut self, position: Duration, cx: &mut Context<Self>) {
        let _ = self
            .preview_audio
            .command(AudioPreviewCommand::Seek(position));
        self.schedule_preview_audio_tick(cx);
    }

    pub(crate) fn schedule_preview_audio_tick(&mut self, cx: &mut Context<Self>) {
        if self.preview_audio_tick_active {
            return;
        }
        self.preview_audio_tick_active = true;
        self.preview_audio_tick_task = Some(cx.spawn(async move |entity, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let should_continue = entity
                    .update(cx, |sftp, cx| {
                        let playing = matches!(
                            sftp.preview_audio.snapshot().state,
                            AudioPreviewState::Playing
                        );
                        if !playing {
                            sftp.preview_audio_tick_active = false;
                            sftp.preview_audio_tick_task = None;
                        }
                        cx.notify();
                        playing
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }

    pub fn set_dialog(&mut self, dialog: SftpDialog) {
        // SftpDialog remains the only payload owner across replacements.
        self.dialog_exit_task = None;
        self.dialog_presence.reopen();
        self.dialog_exit_generation = None;
        self.dialog = Some(dialog);
    }

    pub fn current_remote_path(&self) -> &str {
        &self.remote_path
    }

    pub fn selected_remote_files(&self) -> Vec<String> {
        let mut files = self.remote_selected.iter().cloned().collect::<Vec<_>>();
        files.sort();
        files
    }

    pub fn clear_context_menu_immediately(&mut self) -> bool {
        let changed = self.context_menu.take().is_some();
        let bookmark_changed = self.bookmark_menu.take().is_some();
        self.context_menu_exit_generation = None;
        self.context_menu_presence.reopen();
        changed || bookmark_changed
    }

    /// Collapse the sidebar tree whenever the root listing is replaced.
    pub fn clear_sftp_tree_state(&mut self) {
        self.tree_generation = self.tree_generation.wrapping_add(1);
        self.tree_expanded.clear();
        self.tree_children.clear();
        self.tree_loading.clear();
        self.tree_pending.clear();
        self.tree_selected = None;
    }

    pub fn activate_file(&mut self, pane: SftpPane, file: SftpFileEntry, cx: &mut Context<Self>) {
        self.active_pane = pane;
        self.clear_context_menu_immediately();
        cx.emit(SftpWorkspaceEvent::OpenFileRequested { pane, file });
        cx.notify();
    }

    pub(crate) fn toggle_tree_directory(&mut self, path: String, cx: &mut Context<Self>) {
        if self.tree_expanded.remove(&path) {
            // A collapsed directory should not keep a queued expansion.
            self.tree_pending.retain(|pending| pending != &path);
            cx.notify();
            return;
        }
        let (Some(surface_id), Some(remote_id)) =
            (self.current_surface_id, self.current_remote_id.clone())
        else {
            return;
        };
        self.tree_expanded.insert(path.clone());
        cx.notify();
        if self.tree_children.contains_key(&path)
            || self.tree_loading.contains(&path)
            || self.tree_pending.contains(&path)
        {
            return;
        }
        if !self.tree_loading.is_empty() {
            self.tree_pending.push_back(path);
            return;
        }
        self.tree_loading.insert(path.clone());
        let effect = SftpWorkspaceEffect::StartTreeChildrenLoad {
            surface_id,
            remote_id,
            view_generation: self.view_generation,
            tree_generation: self.tree_generation,
            path,
        };
        cx.emit(SftpWorkspaceEvent::WorkerEffectsReady(
            SftpWorkspaceEffects {
                delivery: self.worker_sender(),
                effects: RefCell::new(VecDeque::from([effect])),
            },
        ));
    }

    pub fn has_drag_capture(&self) -> bool {
        // SFTP file drags use root-level pointer capture so releasing outside
        // both panes still clears the candidate and autoscroll state.
        self.drag_state.is_some() || self.drag_over_pane.is_some()
    }

    pub fn pane_resize_active(&self) -> bool {
        self.pane_resize_drag.is_some()
    }

    pub fn queue_resize_active(&self) -> bool {
        self.queue_resize_drag.is_some()
    }
}

impl EventEmitter<SftpWorkspaceEvent> for SftpWorkspaceEntity {}

#[derive(Clone)]
pub enum SftpViewError {
    Operation(String),
    Directory(String),
}
