use super::ime::WorkspaceImeTarget;
use super::*;
use gpui::{
    AnchoredPositionMode, Corner, Entity, EventEmitter, Focusable, ObjectFit, PathPromptOptions,
    Pixels, Point, SharedString, Subscription, Task, UniformListScrollHandle, anchored, deferred,
    prelude::*,
};
use oxideterm_connections::SshChannelStrategy;
use oxideterm_editor_syntax::LanguageId;
use oxideterm_gpui_editor::{EditorContextMenuLabels, TextEditorView};
use oxideterm_gpui_markdown::{
    MarkdownVirtualListScrollHandle, markdown_virtual_with_code_actions,
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
    text_input::{TextInputView, text_input},
};
use oxideterm_preview::{
    AudioPreviewBackend, AudioPreviewCommand, AudioPreviewState, PreviewAssetOwner,
    RodioAudioPreviewBackend, TextLineEnding, font_family_name_from_bytes,
    normalize_text_line_endings, restore_text_line_endings,
};
use oxideterm_sftp::TransferConflict as SftpConflictInfo;
use oxideterm_sftp::{
    AssetFileKind, BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, FileInfo as RemoteFileInfo, FileType as RemoteFileType,
    ListFilter as RemoteListFilter, LocalDownloadDisposition, PreviewContent,
    RemoteRelayProgressContext, SftpError, SftpSession, SftpTransferGuard,
    SortOrder as RemoteSortOrder, StoredTransferProgress, TarCapabilities, TarTransferOptions,
    TransferDirection as SftpTransferDirection, TransferProgress,
    TransferProtocol as RemoteTransferProtocol, TransferStrategy as RemoteTransferStrategy,
    TransferType as RemoteTransferType, encode_to_encoding, profile_local_directory,
    scp_download_directory, scp_download_file, scp_upload_directory, scp_upload_file,
    tar_download_directory, tar_upload_directory,
};
pub(in crate::workspace::sftp) use oxideterm_sftp::{
    TextDiffLine as SftpDiffLine, TextDiffLineKind as SftpDiffLineKind,
    compute_text_diff as compute_sftp_diff, text_diff_stats as sftp_diff_stats,
};
use std::{
    borrow::Cow,
    collections::VecDeque,
    path::Path,
    time::{Duration, Instant},
};

pub(super) mod native_video;

use native_video::{SharedSftpNativeVideoSurface, sftp_native_video_element};

const SFTP_ROOT_PADDING: f32 = 8.0; // Tauri p-2
const SFTP_GAP: f32 = 8.0; // Tauri gap-2
const SFTP_PANE_SPLIT_DEFAULT_RATIO: f32 = 0.5;
const SFTP_PANE_SPLIT_MIN_RATIO: f32 = 0.2;
const SFTP_PANE_SPLIT_MAX_RATIO: f32 = 0.8;
const SFTP_PANE_SPLIT_HOTZONE_WIDTH: f32 = 14.0;
const SFTP_QUEUE_DEFAULT_HEIGHT: f32 = 192.0; // Tauri h-48
const SFTP_QUEUE_MIN_HEIGHT: f32 = 96.0;
const SFTP_QUEUE_MAX_VIEWPORT_RATIO: f32 = 0.65;
const SFTP_QUEUE_SPLIT_HOTZONE_HEIGHT: f32 = 14.0;
const SFTP_PANE_HEADER_HEIGHT: f32 = 40.0; // Tauri h-10
const SFTP_PANE_HEADER_GAP: f32 = 6.0;
const SFTP_PANE_HEADER_TITLE_MIN_WIDTH: f32 = 32.0;
const SFTP_PATH_BAR_HORIZONTAL_PADDING: f32 = 4.0;
const SFTP_BREADCRUMB_ROW_GAP: f32 = 1.0;
const SFTP_BREADCRUMB_SEGMENT_PADDING: f32 = 3.0;
const SFTP_BREADCRUMB_CONTENT_GAP: f32 = 2.0;
const SFTP_TRANSFER_QUEUE_LIST_INITIAL_ITEM_COUNT: usize = 0;
const SFTP_TRANSFER_QUEUE_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const SFTP_TRANSFER_QUEUE_LIST_OVERSCAN: usize = 6;
// The embedded browser keeps enough room for files while exposing the most
// relevant transfers owned by its current node.
const SFTP_SIDEBAR_TRANSFER_MAX_ROWS: usize = 3;
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
const SFTP_DIFF_WRAP_COLUMNS: usize = 64; // max-w-5xl split diff leaves roughly this many mono chars per side.
const SFTP_PREVIEW_FONT_DEFAULT_SIZE: f32 = 32.0; // Tauri FontPreview initial fontSize
const SFTP_SIZE_COL: f32 = 80.0; // Tauri w-20
const SFTP_MODIFIED_COL: f32 = 96.0; // Tauri w-24
const SFTP_DIRECTORY_PROGRESS_SAVE_INTERVAL_MS: u64 = 1_000; // Keep resume progress fresh without writing on every file tick.
const SFTP_DIRECTORY_SPEED_WINDOW: Duration = Duration::from_secs(2); // Smooth bursts from parallel file workers.
const SFTP_DIRECTORY_SPEED_SAMPLE_INTERVAL: Duration = Duration::from_millis(100); // Keep rolling history bounded at high event rates.
const SFTP_DIRECTORY_PROGRESS_DELIVERY_INTERVAL: Duration = Duration::from_millis(100); // Coalesce small-file completion bursts before crossing into GPUI.
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
const SFTP_DESTRUCTIVE_TEXT: u32 = 0xffffff;
const SFTP_CONTEXT_MENU_WIDTH: f32 = 180.0; // Tauri min-w-[180px]
const SFTP_CONTEXT_MENU_MAX_HEIGHT: f32 = 288.0; // 8 items + separators, clamped like fixed portal menu
const SFTP_CONTEXT_MENU_PADDING: f32 = 4.0; // Tauri py-1
const SFTP_CONTEXT_MENU_ITEM_HEIGHT: f32 = 30.0; // Tauri px-3 py-1.5 text-xs
const SFTP_BUTTON_TRANSPARENT_ALPHA: u32 = 0x00; // Tauri Button border-transparent/bg-transparent
const SFTP_DESTRUCTIVE_BG_ALPHA: u32 = 0xe6;
const SFTP_DESTRUCTIVE_BORDER_ALPHA: u32 = 0xcc;
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

fn configured_transfer_protocol(
    preference: oxideterm_settings::FileTransferProtocolPreference,
) -> RemoteTransferProtocol {
    match preference {
        oxideterm_settings::FileTransferProtocolPreference::Scp => RemoteTransferProtocol::Scp,
        oxideterm_settings::FileTransferProtocolPreference::Auto
        | oxideterm_settings::FileTransferProtocolPreference::Sftp => RemoteTransferProtocol::Sftp,
    }
}

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

#[derive(Clone, Copy, Debug)]
struct SftpPaneResizeDrag {
    start_cursor_x: Pixels,
    start_ratio: f32,
}

#[derive(Clone, Copy, Debug)]
struct SftpQueueResizeDrag {
    start_cursor_y: Pixels,
    start_height: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpFileType {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpButtonVariant {
    Default,
    Destructive,
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

// Surface identity prevents a hidden tab completion from replacing sidebar state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SftpSurfaceId {
    Tab(TabId),
    Sidebar,
}

/// Identifies the remote endpoint without implying NodeRouter ownership.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum SftpRemoteId {
    Node(NodeId),
    Standalone(String),
}

impl SftpRemoteId {
    fn storage_key(&self) -> String {
        match self {
            Self::Node(node_id) => node_id.0.clone(),
            Self::Standalone(profile_id) => format!("standalone-sftp:{profile_id}"),
        }
    }

    fn node_id(&self) -> Option<&NodeId> {
        match self {
            Self::Node(node_id) => Some(node_id),
            Self::Standalone(_) => None,
        }
    }

    fn standalone_endpoint_id(&self) -> Option<&str> {
        match self {
            Self::Node(_) => None,
            Self::Standalone(endpoint_id) => Some(endpoint_id),
        }
    }

    fn from_standalone_endpoint_id(endpoint_id: String) -> Self {
        Self::Standalone(endpoint_id)
    }
}

pub(super) struct StandaloneSftpRuntime {
    pub(super) connection_id: String,
    pub(super) consumer: ConnectionConsumer,
    pub(super) handle: SshConnectionHandle,
    pub(super) title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StandaloneSftpTabBinding {
    pub(super) primary_endpoint_id: String,
    pub(super) secondary_endpoint_id: Option<String>,
    pub(super) secondary_initial_remote_path: Option<String>,
}

impl StandaloneSftpTabBinding {
    pub(super) fn contains_endpoint(&self, endpoint_id: &str) -> bool {
        self.primary_endpoint_id == endpoint_id
            || self.secondary_endpoint_id.as_deref() == Some(endpoint_id)
    }
}

pub(super) struct StandaloneSftpConsumerLease {
    registry: SshConnectionRegistry,
    connection_id: String,
    consumer: ConnectionConsumer,
}

pub(super) type DedicatedSftpConnectionSlot =
    Arc<tokio::sync::Mutex<Option<Arc<DedicatedConnectionLease>>>>;

impl Drop for StandaloneSftpConsumerLease {
    fn drop(&mut self) {
        // A background transfer owns this consumer independently from its tab.
        self.registry.release(&self.connection_id, &self.consumer);
    }
}

#[derive(Clone)]
pub(super) enum SftpRemoteBackend {
    Node {
        router: NodeRouter,
        node_id: NodeId,
        channel_strategy: SshChannelStrategy,
        prompt_handler: Arc<dyn SshPromptHandler>,
        managed_key_resolver: ManagedKeyResolver,
        dedicated_slot: DedicatedSftpConnectionSlot,
    },
    Standalone {
        handle: SshConnectionHandle,
    },
}

impl SftpRemoteBackend {
    async fn node_connection(&self) -> Result<SshConnectionHandle, String> {
        let Self::Node {
            router,
            node_id,
            channel_strategy,
            prompt_handler,
            managed_key_resolver,
            dedicated_slot,
        } = self
        else {
            return Err("SFTP backend is not node-backed".to_string());
        };
        if !channel_strategy.requires_dedicated_consumers() {
            return router
                .resolve_connection(node_id)
                .await
                .map(|resolved| resolved.handle)
                .map_err(|error| error.to_string());
        }

        let mut slot = dedicated_slot.lock().await;
        if let Some(lease) = slot.as_ref()
            && matches!(
                lease.handle().state(),
                ConnectionState::Active | ConnectionState::Idle
            )
            && lease.handle().has_physical()
        {
            return Ok(lease.handle().clone());
        }
        *slot = None;
        let consumer = ConnectionConsumer::Sftp(format!("{}:browse", node_id.0));
        let lease = router
            .acquire_dedicated_connection(
                node_id,
                consumer,
                prompt_handler.clone(),
                managed_key_resolver.clone(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let lease = Arc::new(lease);
        let handle = lease.handle().clone();
        // The node-scoped slot owns browsing until explicit disconnect or a
        // failed transport replaces it; individual SFTP views only borrow it.
        *slot = Some(lease);
        Ok(handle)
    }

    async fn resolve_connection(&self) -> Result<SshConnectionHandle, String> {
        match self {
            Self::Node { .. } => self.node_connection().await,
            Self::Standalone { handle } => Ok(handle.clone()),
        }
    }

    async fn acquire_sftp(&self) -> Result<Arc<tokio::sync::Mutex<SftpSession>>, String> {
        match self {
            Self::Node { .. } => self
                .node_connection()
                .await?
                .acquire_sftp()
                .await
                .map_err(|error| error.to_string()),
            Self::Standalone { handle } => handle
                .acquire_sftp()
                .await
                .map_err(|error| error.to_string()),
        }
    }

    async fn acquire_transfer_sftp(&self) -> Result<SftpSession, String> {
        match self {
            Self::Node {
                router,
                node_id,
                channel_strategy,
                prompt_handler,
                managed_key_resolver,
                ..
            } if channel_strategy.requires_dedicated_consumers() => {
                let consumer = ConnectionConsumer::Sftp(format!(
                    "{}:transfer:{}",
                    node_id.0,
                    uuid::Uuid::new_v4()
                ));
                let lease = Arc::new(
                    router
                        .acquire_dedicated_connection(
                            node_id,
                            consumer,
                            prompt_handler.clone(),
                            managed_key_resolver.clone(),
                        )
                        .await
                        .map_err(|error| error.to_string())?,
                );
                let session = lease
                    .handle()
                    .acquire_transfer_sftp()
                    .await
                    .map_err(|error| error.to_string())?;
                let owner: Arc<dyn Send + Sync> = lease;
                Ok(session
                    .with_connection_owner(owner)
                    .with_single_channel_transport())
            }
            Self::Node {
                router, node_id, ..
            } => router
                .acquire_transfer_sftp(node_id)
                .await
                .map_err(|error| error.to_string()),
            Self::Standalone { handle } => handle
                .acquire_transfer_sftp()
                .await
                .map_err(|error| error.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SftpPresentationRequest {
    node_id: NodeId,
    remote_path: Option<String>,
}

#[derive(Debug)]
pub(super) enum SftpWorkerResult {
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

// Effects contain only stable identifiers, non-secret display errors, and
// owned runtime intents. Authentication material never crosses this boundary.
pub(in crate::workspace::sftp) enum SftpWorkspaceEffect {
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
    StartRemoteLoad {
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        path: String,
        view_generation: u64,
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
    ReloadLocalDirectory {
        view_generation: u64,
        path: String,
    },
    ReloadPairPrimaryDirectory,
}

pub(super) struct SftpWorkspaceEffects {
    delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
    effects: RefCell<VecDeque<SftpWorkspaceEffect>>,
}

impl SftpWorkspaceEffects {
    pub(in crate::workspace::sftp) fn delivery(
        &self,
    ) -> &delivery::ActiveDeliverySender<SftpWorkerResult> {
        &self.delivery
    }

    pub(in crate::workspace::sftp) fn take(&self) -> VecDeque<SftpWorkspaceEffect> {
        std::mem::take(&mut *self.effects.borrow_mut())
    }
}

pub(super) enum SftpWorkspaceEvent {
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
        path: String,
        content: Arc<str>,
        encoding: Arc<str>,
        line_ending: TextLineEnding,
        generation: u64,
        delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
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
    protocol_override: Option<RemoteTransferProtocol>,
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
    remote_id: SftpRemoteId,
    name: String,
    local_path: String,
    remote_path: String,
    direction: SftpTransferDirection,
    protocol: RemoteTransferProtocol,
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
    speed_samples: VecDeque<(Instant, u64)>,
}

impl DirectoryProgressAccumulator {
    fn update(&mut self, progress: TransferProgress) -> TransferProgress {
        self.update_at(progress, Instant::now())
    }

    fn update_at(&mut self, progress: TransferProgress, now: Instant) -> TransferProgress {
        let previous_aggregate_transferred = self.transferred_bytes;
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

        if self.transferred_bytes < previous_aggregate_transferred {
            // A restarted file can make the aggregate counter move backwards.
            self.speed_samples.clear();
        }
        let speed = self.aggregate_speed(now);
        let eta_seconds = if speed > 0 && self.total_bytes > self.transferred_bytes {
            Some((self.total_bytes - self.transferred_bytes).div_ceil(speed))
        } else {
            None
        };

        TransferProgress {
            transferred_bytes: self.transferred_bytes,
            total_bytes: self.total_bytes,
            speed,
            eta_seconds,
            ..progress
        }
    }

    fn aggregate_speed(&mut self, now: Instant) -> u64 {
        let window_start = now.checked_sub(SFTP_DIRECTORY_SPEED_WINDOW);
        while self.speed_samples.len() > 1
            && window_start.is_some_and(|window_start| {
                self.speed_samples
                    .get(1)
                    .is_some_and(|(sampled_at, _)| *sampled_at <= window_start)
            })
        {
            self.speed_samples.pop_front();
        }

        let speed = self
            .speed_samples
            .front()
            .and_then(|(sampled_at, sampled_bytes)| {
                now.checked_duration_since(*sampled_at)
                    .map(|elapsed| (elapsed, *sampled_bytes))
            })
            .filter(|(elapsed, _)| !elapsed.is_zero())
            .map(|(elapsed, sampled_bytes)| {
                (self.transferred_bytes.saturating_sub(sampled_bytes) as f64
                    / elapsed.as_secs_f64()) as u64
            })
            .unwrap_or(0);

        let should_sample = self.speed_samples.back().is_none_or(|(sampled_at, _)| {
            now.checked_duration_since(*sampled_at)
                .is_some_and(|elapsed| elapsed >= SFTP_DIRECTORY_SPEED_SAMPLE_INTERVAL)
        });
        if should_sample {
            self.speed_samples.push_back((now, self.transferred_bytes));
        }

        speed
    }
}

#[cfg(test)]
mod directory_progress_tests {
    use super::*;

    fn progress(file_name: &str, transferred_bytes: u64, total_bytes: u64) -> TransferProgress {
        TransferProgress {
            id: file_name.to_string(),
            remote_path: format!("/remote/{file_name}"),
            local_path: format!("/local/{file_name}"),
            direction: SftpTransferDirection::Download,
            state: oxideterm_sftp::TransferState::InProgress,
            total_bytes,
            transferred_bytes,
            speed: u64::MAX,
            eta_seconds: Some(u64::MAX),
            error: None,
        }
    }

    #[test]
    fn directory_progress_uses_aggregate_byte_delta_for_speed_and_eta() {
        // Explicit timestamps keep rolling-speed tests deterministic without sleeping.
        let started_at = Instant::now();
        let mut accumulator = DirectoryProgressAccumulator::default();

        let initial = accumulator.update_at(progress("first", 100, 1_000), started_at);
        assert_eq!(initial.speed, 0);
        assert_eq!(initial.eta_seconds, None);

        let first_update = accumulator.update_at(
            progress("first", 300, 1_000),
            started_at + Duration::from_secs(1),
        );
        assert_eq!(first_update.speed, 200);
        assert_eq!(first_update.eta_seconds, Some(4));

        let parallel_update = accumulator.update_at(
            progress("second", 400, 1_000),
            started_at + Duration::from_secs(1),
        );
        assert_eq!(parallel_update.transferred_bytes, 700);
        assert_eq!(parallel_update.total_bytes, 2_000);
        assert_eq!(parallel_update.speed, 600);
        assert_eq!(parallel_update.eta_seconds, Some(3));
    }

    #[test]
    fn directory_progress_speed_uses_only_the_rolling_window() {
        let started_at = Instant::now();
        let mut accumulator = DirectoryProgressAccumulator::default();

        accumulator.update_at(progress("file", 0, 1_000), started_at);
        accumulator.update_at(
            progress("file", 100, 1_000),
            started_at + Duration::from_secs(1),
        );
        let recent = accumulator.update_at(
            progress("file", 500, 1_000),
            started_at + Duration::from_secs(3),
        );

        assert_eq!(recent.speed, 200);
        assert_eq!(recent.eta_seconds, Some(3));
    }

    #[test]
    fn directory_progress_resets_speed_when_aggregate_bytes_move_backwards() {
        let started_at = Instant::now();
        let mut accumulator = DirectoryProgressAccumulator::default();

        accumulator.update_at(progress("file", 500, 1_000), started_at);
        let progressing = accumulator.update_at(
            progress("file", 700, 1_000),
            started_at + Duration::from_secs(1),
        );
        assert_eq!(progressing.speed, 200);

        let restarted = accumulator.update_at(
            progress("file", 200, 1_000),
            started_at + Duration::from_secs(2),
        );
        assert_eq!(restarted.speed, 0);
        assert_eq!(restarted.eta_seconds, None);
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

#[derive(Clone, Debug)]
struct SftpDrive {
    name: String,
    path: String,
    drive_type: String,
    total_space: u64,
    available_space: u64,
    read_only: bool,
}

pub(super) struct SftpWorkspaceEntity {
    active_pane: SftpPane,
    local_path: String,
    remote_path: String,
    local_path_input: String,
    remote_path_input: String,
    pub(in crate::workspace) local_path_completion: PathCompletionState,
    pub(in crate::workspace) remote_path_completion: PathCompletionState,
    remote_path_completion_pending_selection: Option<(String, String)>,
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
    local_path_scroll: ScrollHandle,
    remote_path_scroll: ScrollHandle,
    pane_split_ratio: f32,
    pane_resize_drag: Option<SftpPaneResizeDrag>,
    queue_height: f32,
    queue_resize_drag: Option<SftpQueueResizeDrag>,
    diff_scroll: UniformListScrollHandle,
    preview_markdown_scroll: MarkdownVirtualListScrollHandle,
    pub(in crate::workspace) diff_document_scroll: ScrollHandle,
    pub(in crate::workspace) preview_document_scroll: ScrollHandle,
    pub(in crate::workspace) font_preview_scroll: ScrollHandle,
    pub(in crate::workspace) drives_scroll: ScrollHandle,
    local_last_selected: Option<String>,
    remote_last_selected: Option<String>,
    local_files: Vec<SftpFileEntry>,
    remote_files: Vec<SftpFileEntry>,
    remote_loading: bool,
    remote_load_pending: bool,
    remote_load_inflight: bool,
    remote_load_retry_count: u8,
    remote_load_retry_task: Option<Task<()>>,
    pub(in crate::workspace) current_surface_id: Option<SftpSurfaceId>,
    pub(in crate::workspace) current_remote_id: Option<SftpRemoteId>,
    pair_primary_remote_id: Option<SftpRemoteId>,
    pair_primary_loading: bool,
    local_path_by_remote: HashMap<SftpRemoteId, String>,
    remote_path_by_remote: HashMap<SftpRemoteId, String>,
    remote_home_by_remote: HashMap<SftpRemoteId, String>,
    view_generation: u64,
    init_error: Option<String>,
    pub(super) focused_input: Option<SftpInput>,
    editing_local_path: bool,
    editing_remote_path: bool,
    pub(super) dialog: Option<SftpDialog>,
    dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    dialog_exit_generation: Option<u64>,
    dialog_exit_task: Option<Task<()>>,
    conflict_state: Option<SftpConflictState>,
    dialog_value: String,
    preview_pane: Option<SftpPane>,
    preview_path: Option<String>,
    // Preview payloads can contain large text or media buffers. Renderers share
    // immutable snapshots instead of cloning the payload on every frame.
    preview_content: Option<Arc<PreviewContent>>,
    preview_asset_owner: Option<PreviewAssetOwner>,
    preview_generation: u64,
    preview_audio: RodioAudioPreviewBackend,
    preview_audio_tick_active: bool,
    preview_audio_tick_task: Option<Task<()>>,
    preview_video_surface: SharedSftpNativeVideoSurface,
    preview_error: Option<String>,
    preview_loading: bool,
    preview_hex_loading_more: bool,
    preview_markdown_source_mode: bool,
    preview_font_family: Option<String>,
    preview_font_error: Option<String>,
    preview_font_size: f32,
    preview_editor: Option<Entity<TextEditorView>>,
    preview_editor_observer: Option<Subscription>,
    preview_editor_initial_content: Arc<str>,
    preview_editor_observed_content: Arc<str>,
    preview_editor_language: Option<String>,
    preview_editor_encoding: String,
    preview_editor_line_ending: TextLineEnding,
    preview_editor_dirty: bool,
    preview_editor_saving: bool,
    preview_editor_save_error: Option<String>,
    preview_editor_network_error: bool,
    preview_editor_retry_count: u32,
    preview_editor_last_saved_mtime: Option<u64>,
    preview_editor_last_atomic_write: Option<bool>,
    preview_editor_retry_task: Option<Task<()>>,
    transfers: Vec<SftpTransferItem>,
    transfer_queue_list_state: ListState,
    transfer_queue_list_cache: RefCell<VirtualListSignatureCache>,
    transfer_batches: HashMap<u64, SftpTransferBatch>,
    incomplete_transfers: Vec<StoredTransferProgress>,
    incomplete_transfer_list_state: ListState,
    incomplete_transfer_list_cache: RefCell<VirtualListSignatureCache>,
    incomplete_load_inflight: bool,
    incomplete_load_remote: Option<SftpRemoteId>,
    incomplete_load_pending_remote: Option<SftpRemoteId>,
    show_incomplete: bool,
    context_menu: Option<SftpContextMenu>,
    context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence,
    context_menu_exit_generation: Option<u64>,
    folder_picker_task: Option<Task<()>>,
    drag_state: Option<SftpDragState>,
    drag_over_pane: Option<SftpPane>,
    drag_autoscroll_position: Option<Point<Pixels>>,
    drag_autoscroll_scheduled: bool,
    next_transfer_id: u64,
    next_transfer_batch_id: u64,
    worker_tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    worker_rx: std::sync::mpsc::Receiver<SftpWorkerResult>,
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
            remote_file_scroll: UniformListScrollHandle::new(),
            local_path_scroll: ScrollHandle::new(),
            remote_path_scroll: ScrollHandle::new(),
            pane_split_ratio: SFTP_PANE_SPLIT_DEFAULT_RATIO,
            pane_resize_drag: None,
            queue_height: SFTP_QUEUE_DEFAULT_HEIGHT,
            queue_resize_drag: None,
            diff_scroll: UniformListScrollHandle::new(),
            preview_markdown_scroll: MarkdownVirtualListScrollHandle::new(),
            diff_document_scroll: ScrollHandle::new(),
            preview_document_scroll: ScrollHandle::new(),
            font_preview_scroll: ScrollHandle::new(),
            drives_scroll: ScrollHandle::new(),
            local_last_selected: None,
            remote_last_selected: None,
            local_files: list_local_files(&local_path).unwrap_or_else(|_| Vec::new()),
            remote_files: Vec::new(),
            remote_loading: false,
            remote_load_pending: false,
            remote_load_inflight: false,
            remote_load_retry_count: 0,
            remote_load_retry_task: None,
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
            preview_generation: 0,
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
            incomplete_load_remote: None,
            incomplete_load_pending_remote: None,
            show_incomplete: false,
            context_menu: None,
            context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            context_menu_exit_generation: None,
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
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let entity = Self::default();
        entity.schedule_worker_delivery(cx);
        entity
    }

    pub(in crate::workspace) fn worker_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<SftpWorkerResult> {
        // Background operations receive only the shallow delivery endpoint.
        self.worker_tx.clone()
    }

    fn schedule_worker_delivery(&self, cx: &mut Context<Self>) {
        let worker_wake = self.worker_tx.wake();
        let release_wake = worker_wake.clone();
        cx.on_release(move |_, _| {
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

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.worker_rx, delivery::LIFECYCLE_DELIVERY_BUDGET);
        let mut effects = VecDeque::new();
        let mut changed = false;
        for result in delivery_batch.items {
            changed |= self.reduce_worker_result(result, &mut effects, cx);
        }
        if changed {
            cx.notify();
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

    pub(in crate::workspace) fn input_value(&self, input: SftpInput) -> &str {
        match input {
            SftpInput::LocalPath => &self.local_path_input,
            SftpInput::RemotePath => &self.remote_path_input,
            SftpInput::LocalFilter => &self.local_filter,
            SftpInput::RemoteFilter => &self.remote_filter,
            SftpInput::DialogValue => &self.dialog_value,
        }
    }

    pub(in crate::workspace) fn input_value_mut(&mut self, input: SftpInput) -> &mut String {
        match input {
            SftpInput::LocalPath => &mut self.local_path_input,
            SftpInput::RemotePath => &mut self.remote_path_input,
            SftpInput::LocalFilter => &mut self.local_filter,
            SftpInput::RemoteFilter => &mut self.remote_filter,
            SftpInput::DialogValue => &mut self.dialog_value,
        }
    }

    pub(in crate::workspace) fn focused_input(&self) -> Option<SftpInput> {
        self.focused_input
    }

    pub(in crate::workspace) fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn dialog(&self) -> Option<SftpDialog> {
        self.dialog.clone()
    }

    pub(in crate::workspace) fn dialog_is_open(&self) -> bool {
        self.dialog.is_some()
    }

    pub(in crate::workspace) fn dialog_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        // Root keyboard routing must stop treating a fading editor as an
        // interactive child before its payload is finally retired.
        self.dialog_presence.phase()
    }

    pub(in crate::workspace::sftp) fn start_folder_picker(
        &mut self,
        selection: impl std::future::Future<Output = Option<String>> + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.folder_picker_task.is_some() {
            return;
        }
        self.folder_picker_task = Some(cx.spawn(async move |entity, cx| {
            let selected_path = selection.await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.folder_picker_task = None;
                if let Some(path) = selected_path {
                    if let Some(remote_id) = sftp.current_remote_id.clone() {
                        sftp.local_path_by_remote.insert(remote_id, path.clone());
                    }
                    sftp.apply_local_path(path);
                    cx.notify();
                }
            });
        }));
    }

    pub(in crate::workspace::sftp) fn begin_dialog_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.dialog.is_none() {
            return false;
        }
        self.stop_preview_media();
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let Some(generation) = self.dialog_presence.begin_exit() else {
            return false;
        };
        self.focused_input = None;
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

    pub(in crate::workspace::sftp) fn finish_dialog_exit(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.dialog_presence.finish_exit(generation) {
            return false;
        }
        self.dialog = None;
        self.dialog_exit_generation = None;
        self.dialog_exit_task = None;
        self.conflict_state = None;
        self.dialog_value.clear();
        self.preview_asset_owner = None;
        self.preview_hex_loading_more = false;
        self.preview_markdown_source_mode = false;
        self.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.preview_font_family = None;
        self.preview_font_error = None;
        self.preview_font_size = SFTP_PREVIEW_FONT_DEFAULT_SIZE;
        self.reset_preview_editor();
        self.focused_input = None;
        cx.notify();
        true
    }

    pub(in crate::workspace::sftp) fn reset_preview_editor(&mut self) {
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

    pub(in crate::workspace::sftp) fn stop_preview_media(&mut self) {
        let _ = self.preview_audio.command(AudioPreviewCommand::Stop);
        self.preview_audio_tick_active = false;
        self.preview_audio_tick_task = None;
        self.preview_video_surface.detach();
    }

    pub(in crate::workspace::sftp) fn toggle_preview_audio(&mut self, cx: &mut Context<Self>) {
        let _ = self.preview_audio.command(AudioPreviewCommand::PlayPause);
        self.schedule_preview_audio_tick(cx);
    }

    pub(in crate::workspace::sftp) fn seek_preview_audio(
        &mut self,
        position: Duration,
        cx: &mut Context<Self>,
    ) {
        let _ = self
            .preview_audio
            .command(AudioPreviewCommand::Seek(position));
        self.schedule_preview_audio_tick(cx);
    }

    fn schedule_preview_audio_tick(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn set_dialog(&mut self, dialog: SftpDialog) {
        // SftpDialog remains the only payload owner across replacements.
        self.dialog_exit_task = None;
        self.dialog_presence.reopen();
        self.dialog_exit_generation = None;
        self.dialog = Some(dialog);
    }

    pub(super) fn current_remote_path(&self) -> &str {
        &self.remote_path
    }

    pub(super) fn selected_remote_files(&self) -> Vec<String> {
        let mut files = self.remote_selected.iter().cloned().collect::<Vec<_>>();
        files.sort();
        files
    }

    pub(super) fn clear_context_menu_immediately(&mut self) -> bool {
        let changed = self.context_menu.take().is_some();
        self.context_menu_exit_generation = None;
        self.context_menu_presence.reopen();
        changed
    }

    pub(in crate::workspace::sftp) fn activate_file(
        &mut self,
        pane: SftpPane,
        file: SftpFileEntry,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = pane;
        self.clear_context_menu_immediately();
        cx.emit(SftpWorkspaceEvent::OpenFileRequested { pane, file });
        cx.notify();
    }

    pub(super) fn has_drag_capture(&self) -> bool {
        // SFTP file drags use root-level pointer capture so releasing outside
        // both panes still clears the candidate and autoscroll state.
        self.drag_state.is_some() || self.drag_over_pane.is_some()
    }

    pub(super) fn pane_resize_active(&self) -> bool {
        self.pane_resize_drag.is_some()
    }

    pub(super) fn queue_resize_active(&self) -> bool {
        self.queue_resize_drag.is_some()
    }
}

impl EventEmitter<SftpWorkspaceEvent> for SftpWorkspaceEntity {}

#[cfg(test)]
mod entity_delivery_tests {
    use super::*;
    use gpui::TestAppContext;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    fn completion(index: usize) -> SftpWorkerResult {
        SftpWorkerResult::RemoteMutationComplete {
            result: Ok(()),
            refresh_remote: false,
            refresh_local: false,
            toast: Some(SftpMutationToast {
                success_title: format!("completed-{index}"),
                success_description: None,
                error_title: "unused".to_string(),
            }),
        }
    }

    fn transfer_item() -> SftpTransferItem {
        SftpTransferItem {
            id: 1,
            transfer_id: "transfer-1".to_string(),
            batch_id: None,
            remote_id: SftpRemoteId::Node(NodeId::new("delivery-test")),
            name: "file.txt".to_string(),
            local_path: "/tmp/file.txt".to_string(),
            remote_path: "/file.txt".to_string(),
            direction: SftpTransferDirection::Upload,
            protocol: RemoteTransferProtocol::Sftp,
            size: 10,
            transferred: 0,
            speed: 0,
            state: SftpTransferState::Pending,
            error: None,
        }
    }

    fn file_entry(name: &str) -> SftpFileEntry {
        SftpFileEntry {
            name: name.to_string(),
            path: format!("/{name}"),
            file_type: SftpFileType::File,
            size: 1,
            modified: None,
            permissions: None,
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        }
    }

    #[test]
    fn file_row_selection_is_owned_by_sftp_entity() {
        let mut sftp = SftpWorkspaceEntity::default();
        sftp.local_files = vec![file_entry("alpha"), file_entry("beta")];

        sftp.select_file(
            SftpPane::Local,
            "alpha".to_string(),
            gpui::Modifiers::default(),
        );

        assert_eq!(sftp.local_selected, HashSet::from(["alpha".to_string()]));
        assert_eq!(sftp.local_last_selected.as_deref(), Some("alpha"));
        assert_eq!(sftp.active_pane, SftpPane::Local);
    }

    #[gpui::test]
    fn file_activation_emits_typed_workspace_intent(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        let observed = Arc::new(AtomicBool::new(false));
        let observed_event = observed.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &SftpWorkspaceEvent, _cx| {
                if matches!(
                    event,
                    SftpWorkspaceEvent::OpenFileRequested {
                        pane: SftpPane::Remote,
                        file
                    } if file.name == "remote.txt"
                ) {
                    observed_event.store(true, Ordering::Release);
                }
            })
        });

        entity.update(cx, |sftp, cx| {
            sftp.activate_file(SftpPane::Remote, file_entry("remote.txt"), cx);
        });

        assert!(observed.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn worker_state_is_applied_before_effect_event(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| sftp.transfers.push(transfer_item()));
        let observed_completed_state = Arc::new(AtomicBool::new(false));
        let observed_state = observed_completed_state.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |sftp, _, event: &SftpWorkspaceEvent, _cx| {
                if let SftpWorkspaceEvent::WorkerEffectsReady(effects) = event {
                    observed_state.store(
                        sftp.transfers[0].state == SftpTransferState::Completed,
                        Ordering::Release,
                    );
                    let _ = effects.take();
                }
            })
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());

        sender
            .send(SftpWorkerResult::TransferComplete {
                remote_id: SftpRemoteId::Node(NodeId::new("delivery-test")),
                transfer_id: "transfer-1".to_string(),
                id: 1,
                result: Ok(()),
                refresh_remote: false,
                refresh_local: false,
            })
            .expect("SFTP completion delivery");
        cx.run_until_parked();

        assert!(observed_completed_state.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn stale_remote_list_result_does_not_emit_effect(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| {
            sftp.current_surface_id = Some(SftpSurfaceId::Tab(TabId(1)));
            sftp.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("current-node")));
            sftp.view_generation = 2;
            sftp.remote_load_inflight = true;
        });
        let effect_events = Arc::new(AtomicUsize::new(0));
        let observed_events = effect_events.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &SftpWorkspaceEvent, _cx| {
                if matches!(event, SftpWorkspaceEvent::WorkerEffectsReady(_)) {
                    observed_events.fetch_add(1, Ordering::AcqRel);
                }
            })
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());

        sender
            .send(SftpWorkerResult::RemoteList {
                surface_id: SftpSurfaceId::Tab(TabId(1)),
                remote_id: SftpRemoteId::Node(NodeId::new("current-node")),
                view_generation: 1,
                session_id: "stale-session".to_string(),
                path: "/stale".to_string(),
                result: Ok(RemoteSftpListing {
                    cwd: "/stale".to_string(),
                    files: Vec::new(),
                }),
            })
            .expect("stale SFTP delivery");
        cx.run_until_parked();

        assert_eq!(effect_events.load(Ordering::Acquire), 0);
        cx.read(|cx| assert!(!entity.read(cx).remote_load_inflight));
    }

    #[gpui::test]
    fn pending_terminal_cwd_is_not_overwritten_by_inflight_listing(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| {
            // Opening SFTP queues the terminal cwd after the remembered directory starts loading.
            sftp.current_surface_id = Some(SftpSurfaceId::Tab(TabId(1)));
            sftp.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("current-node")));
            sftp.view_generation = 1;
            sftp.remote_path = "/root/.oxideterm".to_string();
            sftp.remote_path_input = sftp.remote_path.clone();
            sftp.remote_loading = true;
            sftp.remote_load_inflight = true;
            sftp.remote_load_pending = true;
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());

        sender
            .send(SftpWorkerResult::RemoteList {
                surface_id: SftpSurfaceId::Tab(TabId(1)),
                remote_id: SftpRemoteId::Node(NodeId::new("current-node")),
                view_generation: 1,
                session_id: "current-session".to_string(),
                path: "/root".to_string(),
                result: Ok(RemoteSftpListing {
                    cwd: "/root".to_string(),
                    files: Vec::new(),
                }),
            })
            .expect("inflight SFTP directory delivery");
        cx.run_until_parked();

        cx.read(|cx| {
            let sftp = entity.read(cx);
            assert_eq!(sftp.remote_path, "/root/.oxideterm");
            assert_eq!(sftp.remote_path_input, "/root/.oxideterm");
            assert!(sftp.remote_load_pending);
            assert!(!sftp.remote_load_inflight);
        });
    }

    #[gpui::test]
    fn hidden_entity_drains_backlog_by_budget_and_stops_on_release(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        let ready_events = Arc::new(AtomicUsize::new(0));
        let delivered_effects = Arc::new(AtomicUsize::new(0));
        let observed_events = ready_events.clone();
        let observed_effects = delivered_effects.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &SftpWorkspaceEvent, _cx| {
                if let SftpWorkspaceEvent::WorkerEffectsReady(effects) = event {
                    observed_events.fetch_add(1, Ordering::AcqRel);
                    observed_effects.fetch_add(effects.take().len(), Ordering::AcqRel);
                }
            })
        });
        let sender = cx.read(|cx| entity.read(cx).worker_sender());
        let wake = sender.wake();

        // More than two lifecycle budgets proves backlog continuation without rendering.
        for index in 0..130 {
            sender
                .send(completion(index))
                .expect("SFTP worker delivery");
        }
        cx.run_until_parked();

        assert_eq!(delivered_effects.load(Ordering::Acquire), 130);
        assert!(ready_events.load(Ordering::Acquire) >= 3);

        drop(entity);
        cx.update(|_cx| {});
        assert!(wake.is_stopped());
    }
}

// Keep each SFTP responsibility in a real module while preserving this file as the facade.
mod actions;
mod controls;
mod dialogs;
mod file_list;
mod helpers;
mod layout;
mod menus;
mod runtime;
mod surface;
mod transfers;

// Re-export only the cross-module helpers needed by the SFTP facade and its children.
pub(in crate::workspace::sftp) use actions::{SftpTransferLaunch, sftp_extract_archive_kind};
use helpers::{
    default_download_path, diff_cell, format_conflict_modified, format_file_size, format_modified,
    format_sftp_media_time, format_transfer_speed, home_path,
    is_sftp_incomplete_store_compat_error, join_local_path, join_sftp_path, list_local_files,
    load_remote_sftp_completion_listing, load_remote_sftp_listing, load_remote_sftp_preview,
    load_remote_sftp_preview_hex, local_drives, new_sftp_transfer_id,
    normalize_external_dropped_path, normalize_remote_path, parent_path, preview_content_text,
    refreshed_local_files, remote_directory_prefixes, save_remote_sftp_preview, sftp_bg,
    sftp_border, sftp_card_surface, sftp_conflict_resolution_from_settings, sftp_diff_visual_lines,
    sftp_editor_language, sftp_editor_language_id, sftp_file_name, sftp_hover_bg, sftp_panel_bg,
    sftp_path_segments, sftp_preview_editor_is_network_error, sftp_preview_is_markdown,
    sftp_source_not_newer_than_target, sftp_transfer_conflicts,
    sftp_transfer_state_from_background, sorted_sftp_files, unique_sftp_conflict_name,
};
