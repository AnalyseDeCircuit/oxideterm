use std::borrow::Cow;

use super::ime::WorkspaceImeTarget;
use super::*;
use gpui::{
    AnchoredPositionMode, Corner, EventEmitter, Task, UniformListScrollHandle, anchored, deferred,
    prelude::*,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_markdown::{
    MarkdownOptions, MarkdownVirtualListScrollHandle, highlight, markdown_virtual_with_code_actions,
};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, ButtonVariant, IconButtonOptions, ToolbarButtonOptions},
    context_menu::{ContextMenuActionableStyle, context_menu_event_boundary},
    modal::{
        dismissible_dialog_backdrop, overlay_content_boundary, quicklook_backdrop,
        rounded_shell_child_radius,
    },
    scroll::ScrollableElement,
    surface::{color_for_background, color_with_background_scaled_alpha},
    text_input::{TextInputView, text_input},
};
use oxideterm_local_files::{
    BOOKMARKS_FILENAME as FILE_MANAGER_BOOKMARKS_FILENAME, LocalArchiveEntry, LocalArchiveInfo,
    LocalBookmark, LocalChecksumResult, LocalClipboardMode, LocalDrive, LocalFileEntry,
    LocalFileType, LocalPreview, LocalPreviewMetadata, LocalSidebarLocation,
    LocalSidebarLocationKind, LocalSortDirection, LocalSortField,
};
use oxideterm_preview::{
    AudioPreviewBackend, AudioPreviewCommand, AudioPreviewState, RodioAudioPreviewBackend,
    font_family_name_from_bytes,
};

mod actions;
mod dialogs;
mod helpers;
mod render;

use self::actions::{open_path_external, reveal_path_external};
use self::helpers::*;
use super::sftp::native_video::{SharedSftpNativeVideoSurface, sftp_native_video_element};

const FILE_MANAGER_HEADER_HEIGHT: f32 = 40.0; // Tauri h-10.
const FILE_MANAGER_HEADER_GAP: f32 = 6.0;
const FILE_MANAGER_HEADER_TITLE_MIN_WIDTH: f32 = 32.0;
const FILE_MANAGER_PATH_BAR_HORIZONTAL_PADDING: f32 = 4.0;
const FILE_MANAGER_BREADCRUMB_ROW_GAP: f32 = 1.0;
const FILE_MANAGER_BREADCRUMB_SEGMENT_PADDING: f32 = 3.0;
const FILE_MANAGER_BREADCRUMB_CONTENT_GAP: f32 = 2.0;
const FILE_MANAGER_TOOLBAR_HEIGHT: f32 = 48.0; // Shared top-level tool-page toolbar height.
const FILE_MANAGER_ROW_HEIGHT: f32 = 28.0; // Tauri FileList FILE_ROW_HEIGHT.
const FILE_MANAGER_VIRTUAL_OVERSCAN: usize = 15; // Tauri useVirtualizer overscan.
const FILE_MANAGER_ARCHIVE_LIST_INITIAL_ITEM_COUNT: usize = 0;
const FILE_MANAGER_ARCHIVE_ROW_HEIGHT: f32 = 28.0; // Tauri archive preview row min-h-7.
const FILE_MANAGER_ARCHIVE_LIST_OVERSCAN: usize = 12;
const FILE_MANAGER_PREVIEW_CODE_OVERSCAN: usize = 20; // Tauri VirtualTextPreview OVERSCAN_LINES.
const FILE_MANAGER_PREVIEW_CODE_WRAP_COLUMNS: usize = 96; // Virtual rows pre-wrap long `whitespace-pre` lines.
const FILE_MANAGER_PREVIEW_STREAM_CHUNK_SIZE: u64 = 128 * 1024; // Tauri VirtualTextPreview CHUNK_SIZE.
const FILE_MANAGER_PREVIEW_CODE_GUTTER_ALPHA: u32 = 0x4d; // Tauri CodeHighlight line-number opacity 30%.
const FILE_MANAGER_SIDEBAR_WIDTH: f32 = 184.0; // Compact favorites rail keeps file content visually dominant.
const FILE_MANAGER_SIDEBAR_HIDDEN_WIDTH: f32 = 0.0; // Hidden favorites return all horizontal space to file content.
const FILE_MANAGER_SIDEBAR_ROW_HEIGHT: f32 = 30.0;
const FILE_MANAGER_SIDEBAR_SECTION_HEADER_HEIGHT: f32 = 28.0;
const FILE_MANAGER_SIDEBAR_SECTION_GAP: f32 = 10.0;
const FILE_MANAGER_TEXT_XS: f32 = 12.0;
const FILE_MANAGER_TEXT_SM: f32 = 14.0;
const FILE_MANAGER_ICON_SM: f32 = 12.0;
const FILE_MANAGER_ICON_MD: f32 = 14.0;
const FILE_MANAGER_TOOL_BUTTON: f32 = 24.0;
const FILE_MANAGER_SIZE_COL: f32 = 80.0;
const FILE_MANAGER_MODIFIED_COL: f32 = 96.0;
const FILE_MANAGER_CONTEXT_MENU_WIDTH: f32 = 180.0; // Tauri min-w-[180px].
const FILE_MANAGER_CONTEXT_MENU_MAX_HEIGHT: f32 = 520.0; // Tauri max-h-[80vh], clamped per viewport.
const FILE_MANAGER_CONTEXT_MENU_PADDING: f32 = 4.0;
const FILE_MANAGER_CONTEXT_MENU_ITEM_HEIGHT: f32 = 30.0;
const FILE_MANAGER_DIALOG_WIDTH_SM: f32 = 384.0;
const FILE_MANAGER_QUICKLOOK_WIDTH: f32 = 1000.0; // Tauri QuickLook width: min(90vw, 1000px).
const FILE_MANAGER_QUICKLOOK_HEIGHT: f32 = 800.0; // Tauri QuickLook height: min(90vh, 800px).
const FILE_MANAGER_QUICKLOOK_MIN_WIDTH: f32 = 400.0; // Tauri QuickLook minWidth: min(400px, 95vw).
const FILE_MANAGER_QUICKLOOK_MIN_HEIGHT: f32 = 300.0; // Tauri QuickLook minHeight: min(300px, 95vh).
const FILE_MANAGER_PREVIEW_MIN_ZOOM: f32 = 0.25; // Tauri QuickLook image/PDF minimum zoom.
const FILE_MANAGER_PREVIEW_MAX_ZOOM: f32 = 3.0; // Tauri QuickLook image/PDF maximum zoom.
const FILE_MANAGER_BG_ACTIVE_BG_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg 40%.
const FILE_MANAGER_BG_ACTIVE_PANEL_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg-panel 40%.
const FILE_MANAGER_BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // [data-bg-active] --color-theme-bg-hover 50%.
const FILE_MANAGER_PANEL_80_ALPHA: u32 = 0xcc; // Tauri bg-theme-bg-panel/80.
const FILE_MANAGER_SELECTED_BG_ALPHA: u32 = 0x33; // Tauri bg-theme-accent/20.
const FILE_MANAGER_BREADCRUMB_ACTIVE_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-hover/30.
const FILE_MANAGER_BREADCRUMB_HOVER_ALPHA: u32 = 0x80; // Tauri hover:bg-theme-bg-hover/50.
const FILE_MANAGER_DIALOG_BORDER_ALPHA: u32 = 0x99;
const FILE_MANAGER_RED: u32 = 0xf87171; // Tauri text-red-400.
const FILE_MANAGER_BLUE: u32 = 0x60a5fa; // Tauri text-blue-400.
const FILE_MANAGER_GREEN: u32 = 0x22c55e; // Tauri text-green-500.
const FILE_MANAGER_ORANGE: u32 = 0xfb923c; // Tauri text-orange-400.
const FILE_MANAGER_PURPLE: u32 = 0xc084fc; // Tauri preview/file accent family.

#[derive(Clone, Copy, Debug, PartialEq)]
struct FileManagerSidebarItemGeometry {
    transition_index: usize,
    top: f32,
}

impl FileManagerSidebarItemGeometry {
    const FIRST: Self = Self {
        transition_index: 0,
        top: 0.0,
    };

    fn next(self) -> Self {
        // Sidebar rows have a fixed height, so their relative motion remains
        // stable even when the scroll viewport itself moves.
        Self {
            transition_index: self.transition_index + 1,
            top: self.top + FILE_MANAGER_SIDEBAR_ROW_HEIGHT,
        }
    }

    fn after_section_header(self) -> Self {
        // The Locations heading contributes real space between the two row groups.
        Self {
            top: self.top
                + FILE_MANAGER_SIDEBAR_SECTION_GAP
                + FILE_MANAGER_SIDEBAR_SECTION_HEADER_HEIGHT,
            ..self
        }
    }
}

fn file_manager_list_virtual_spec() -> TauriVirtualListSpec {
    // Tauri FileList owns FILE_ROW_HEIGHT and useVirtualizer overscan as one
    // contract. Keep native render/scroll call sites on the same named spec so
    // row height and overdraw cannot drift independently.
    TauriVirtualListSpec::new(px(FILE_MANAGER_ROW_HEIGHT), FILE_MANAGER_VIRTUAL_OVERSCAN)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum FileManagerInput {
    Path,
    Filter,
    DialogValue,
}

impl FileManagerInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Path => 1,
            Self::Filter => 2,
            Self::DialogValue => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LocalClipboard {
    mode: LocalClipboardMode,
    paths: Vec<String>,
    source_dir: String,
}

#[derive(Clone, Debug)]
pub(super) struct FileManagerContextMenu {
    file: Option<LocalFileEntry>,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
pub(super) enum FileManagerDialog {
    Drives,
    NewFolder,
    NewFile,
    Rename {
        old_name: String,
    },
    Delete {
        files: Vec<String>,
    },
    EditBookmark {
        id: String,
        path: String,
    },
    Properties {
        entry: LocalFileEntry,
        details: FileManagerProperties,
    },
    Preview {
        entry: LocalFileEntry,
    },
}

#[derive(Clone, Debug)]
pub(super) struct FileManagerProperties {
    kind_label: String,
    location: String,
    size: u64,
    modified: Option<i64>,
    accessed: Option<i64>,
    readonly: bool,
    dir_files: Option<u64>,
    dir_dirs: Option<u64>,
    total_size: Option<u64>,
    created: Option<i64>,
    mode: Option<u32>,
    mime_type: Option<String>,
    is_symlink: bool,
}

#[derive(Clone, Debug)]
pub(super) enum FileManagerWorkspaceEvent {
    Error(String),
    OperationSucceeded,
    OpenEntry(LocalFileEntry),
}

impl EventEmitter<FileManagerWorkspaceEvent> for FileManagerState {}

#[derive(Clone, Debug)]
pub(super) struct FileManagerOperationProgress {
    pub(super) current: usize,
    pub(super) total: usize,
    pub(super) file_name: String,
    pub(super) active: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct FileManagerPreviewStreamState {
    pub(super) path: String,
    pub(super) size: u64,
    pub(super) language: Option<String>,
    pub(super) lines: Vec<String>,
    pub(super) loaded_bytes: u64,
    pub(super) eof: bool,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    pub(super) carry_text: String,
    pub(super) carry_bytes: Vec<u8>,
}

#[derive(Debug)]
pub(super) enum FileManagerOperationEvent {
    Progress(FileManagerOperationProgress),
    Finished(Result<(), String>),
}

pub(super) struct FileManagerRotatedPreviewImage {
    pub(super) path: String,
    pub(super) rotation: i32,
    pub(super) image: Arc<RenderImage>,
}

struct FileManagerSortedFilesCache {
    source_revision: u64,
    filter: String,
    sort_field: LocalSortField,
    sort_direction: LocalSortDirection,
    files: Arc<Vec<LocalFileEntry>>,
    rows: Arc<Vec<FileManagerListRow>>,
}

#[derive(Clone)]
struct FileManagerListRow {
    display_name: SharedString,
    size_text: SharedString,
    modified_text: SharedString,
    icon: LucideIcon,
    icon_color: u32,
}

impl FileManagerListRow {
    fn new(file: &LocalFileEntry) -> Self {
        // These values depend only on directory data, so compute them once instead
        // of repeating extension and local-time formatting on every scroll frame.
        let display_name = file
            .symlink_target
            .as_ref()
            .map(|target| format!("{} -> {target}", file.name))
            .unwrap_or_else(|| file.name.clone());
        let size_text = if file.file_type == LocalFileType::Directory {
            "-".to_string()
        } else {
            format_file_size(file.size)
        };
        let modified_text = format_modified(file.modified);
        let (icon, icon_color) = file_icon_for_entry(file);
        Self {
            display_name: display_name.into(),
            size_text: size_text.into(),
            modified_text: modified_text.into(),
            icon,
            icon_color,
        }
    }
}

pub(super) struct FileManagerState {
    pub(super) path: String,
    pub(super) path_input: String,
    pub(super) path_completion: PathCompletionState,
    pub(super) path_scroll: ScrollHandle,
    pub(super) editing_path: bool,
    pub(super) filter: String,
    pub(super) files: Vec<LocalFileEntry>,
    // Directory refreshes advance this revision so cached filtering and sorting never outlive data.
    source_revision: u64,
    sorted_files_cache: RefCell<Option<FileManagerSortedFilesCache>>,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    pub(super) selected: HashSet<String>,
    pub(super) last_selected: Option<String>,
    pub(super) sort_field: LocalSortField,
    pub(super) sort_direction: LocalSortDirection,
    pub(super) focused_input: Option<FileManagerInput>,
    pub(super) focused_dialog_footer_action: Option<ConfirmDialogAction>,
    pub(super) context_menu: Option<FileManagerContextMenu>,
    pub(super) context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) context_menu_exit_generation: Option<u64>,
    pub(super) context_menu_exit_task: Option<Task<()>>,
    pub(super) dialog: Option<FileManagerDialog>,
    pub(super) dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    dialog_exit_task: Option<Task<()>>,
    folder_picker_task: Option<Task<()>>,
    pub(super) dialog_value: String,
    pub(super) clipboard: Option<LocalClipboard>,
    pub(super) bookmarks: Vec<LocalBookmark>,
    pub(super) sidebar_locations: Vec<LocalSidebarLocation>,
    pub(super) drives: Vec<LocalDrive>,
    pub(super) bookmarks_path: PathBuf,
    pub(super) bookmarks_visible: bool,
    pub(super) list_scroll: UniformListScrollHandle,
    // Preview payloads can contain large text or archive listings. Share the
    // immutable payload across render snapshots instead of cloning its contents.
    pub(super) preview: Option<Arc<LocalPreview>>,
    pub(super) preview_metadata: Option<LocalPreviewMetadata>,
    pub(super) preview_show_metadata: bool,
    pub(super) preview_markdown_source: bool,
    pub(super) preview_image_zoom: f32,
    pub(super) preview_image_rotation: i32,
    pub(super) preview_rotated_image_cache: RefCell<Option<FileManagerRotatedPreviewImage>>,
    pub(super) preview_retired_images: RefCell<Vec<Arc<RenderImage>>>,
    pub(super) preview_code_scroll: UniformListScrollHandle,
    pub(super) preview_markdown_scroll: MarkdownVirtualListScrollHandle,
    pub(in crate::workspace) preview_document_scroll: ScrollHandle,
    pub(in crate::workspace) preview_metadata_scroll: ScrollHandle,
    pub(super) preview_archive_list_state: ListState,
    pub(super) preview_archive_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) preview_stream: FileManagerPreviewStreamState,
    pub(super) preview_audio: RodioAudioPreviewBackend,
    pub(super) preview_video_surface: SharedSftpNativeVideoSurface,
    pub(super) preview_font_family: Option<String>,
    pub(super) preview_font_error: Option<String>,
    pub(super) preview_font_size: f32,
    pub(super) operation_progress: Option<FileManagerOperationProgress>,
    operation_tx: Option<delivery::ActiveDeliverySender<FileManagerOperationEvent>>,
    operation_rx: Option<std::sync::mpsc::Receiver<FileManagerOperationEvent>>,
    operation_thread: Option<std::thread::JoinHandle<()>>,
    pub(super) properties_checksum: Option<LocalChecksumResult>,
    pub(super) properties_checksum_loading: bool,
    pub(super) properties_checksum_task: Option<Task<()>>,
}

impl Default for FileManagerState {
    fn default() -> Self {
        let path = home_path();
        Self {
            path: path.clone(),
            path_input: path,
            path_completion: PathCompletionState::default(),
            path_scroll: ScrollHandle::new(),
            editing_path: false,
            filter: String::new(),
            files: Vec::new(),
            source_revision: 0,
            sorted_files_cache: RefCell::new(None),
            loading: false,
            error: None,
            selected: HashSet::new(),
            last_selected: None,
            sort_field: LocalSortField::Name,
            sort_direction: LocalSortDirection::Asc,
            focused_input: None,
            focused_dialog_footer_action: None,
            context_menu: None,
            context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            context_menu_exit_generation: None,
            context_menu_exit_task: None,
            dialog: None,
            dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            dialog_exit_task: None,
            folder_picker_task: None,
            dialog_value: String::new(),
            clipboard: None,
            bookmarks: Vec::new(),
            // Resolve system folders once so paint does not repeatedly query the filesystem.
            sidebar_locations: local_sidebar_locations(),
            // Disk discovery performs synchronous system and filesystem queries.
            // Cache it outside render and refresh only at explicit interaction boundaries.
            drives: local_drives(),
            bookmarks_path: default_file_manager_bookmarks_path(),
            bookmarks_visible: true,
            list_scroll: UniformListScrollHandle::new(),
            preview: None,
            preview_metadata: None,
            preview_show_metadata: true,
            preview_markdown_source: false,
            preview_image_zoom: 1.0,
            preview_image_rotation: 0,
            preview_rotated_image_cache: RefCell::new(None),
            preview_retired_images: RefCell::new(Vec::new()),
            preview_code_scroll: UniformListScrollHandle::new(),
            preview_markdown_scroll: MarkdownVirtualListScrollHandle::new(),
            preview_document_scroll: ScrollHandle::new(),
            preview_metadata_scroll: ScrollHandle::new(),
            // Archive previews can contain thousands of entries. Keep the file
            // rows on ListState instead of rebuilding the entire archive tree.
            preview_archive_list_state: ListState::new(
                FILE_MANAGER_ARCHIVE_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(FILE_MANAGER_ARCHIVE_ROW_HEIGHT),
                    FILE_MANAGER_ARCHIVE_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            preview_archive_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            preview_stream: FileManagerPreviewStreamState::default(),
            preview_audio: RodioAudioPreviewBackend::default(),
            preview_video_surface: SharedSftpNativeVideoSurface::default(),
            preview_font_family: None,
            preview_font_error: None,
            preview_font_size: 32.0,
            operation_progress: None,
            operation_tx: None,
            operation_rx: None,
            operation_thread: None,
            properties_checksum: None,
            properties_checksum_loading: false,
            properties_checksum_task: None,
        }
    }
}

impl FileManagerState {
    fn close_dialog(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.preview_audio.command(AudioPreviewCommand::Stop) {
            cx.emit(FileManagerWorkspaceEvent::Error(error));
        }
        self.preview_video_surface.detach();
        self.dialog = None;
        self.dialog_exit_task = None;
        self.focused_input = None;
        self.focused_dialog_footer_action = None;
        self.dialog_value.clear();
        self.preview = None;
        self.preview_metadata = None;
        self.preview_markdown_source = false;
        self.preview_code_scroll = UniformListScrollHandle::new();
        self.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.preview_document_scroll = ScrollHandle::new();
        self.preview_metadata_scroll = ScrollHandle::new();
        self.preview_stream = FileManagerPreviewStreamState::default();
        self.properties_checksum = None;
        self.properties_checksum_loading = false;
        self.properties_checksum_task = None;
        cx.notify();
    }

    fn begin_rich_dialog_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        if !matches!(
            self.dialog,
            Some(FileManagerDialog::Preview { .. } | FileManagerDialog::Properties { .. })
        ) {
            self.close_dialog(cx);
            return true;
        }
        let Some(generation) = self.dialog_presence.begin_exit() else {
            return false;
        };
        if let Err(error) = self.preview_audio.command(AudioPreviewCommand::Stop) {
            cx.emit(FileManagerWorkspaceEvent::Error(error));
        }
        self.preview_video_surface.detach();
        if delay.is_zero() {
            self.finish_rich_dialog_exit(generation, cx);
            return true;
        }
        self.dialog_exit_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |file_manager, cx| {
                file_manager.finish_rich_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_rich_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.dialog_presence.finish_exit(generation) {
            return false;
        }
        self.close_dialog(cx);
        self.dialog_presence.reopen();
        true
    }

    fn start_folder_picker(
        &mut self,
        selection: impl std::future::Future<Output = Option<PathBuf>> + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.folder_picker_task.is_some() {
            return;
        }
        self.folder_picker_task = Some(cx.spawn(async move |entity, cx| {
            let selected_path = selection.await;
            let _ = entity.update(cx, |file_manager, cx| {
                file_manager.folder_picker_task = None;
                if let Some(path) = selected_path {
                    file_manager.set_path(path.to_string_lossy().to_string());
                }
                cx.notify();
            });
        }));
    }

    fn start_operation(
        &mut self,
        total: usize,
        work: impl FnOnce(
            delivery::ActiveDeliverySender<FileManagerOperationEvent>,
        ) -> Result<(), String>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        if self
            .operation_progress
            .as_ref()
            .is_some_and(|progress| progress.active)
        {
            return;
        }
        let Some(sender) = self.operation_tx.clone() else {
            cx.emit(FileManagerWorkspaceEvent::Error(
                "File operation delivery is unavailable.".to_string(),
            ));
            return;
        };
        self.operation_progress = Some(FileManagerOperationProgress {
            current: 0,
            total: total.max(1),
            file_name: String::new(),
            active: true,
        });
        self.operation_thread = Some(std::thread::spawn(move || {
            let result = work(sender.clone());
            let _ = sender.send(FileManagerOperationEvent::Finished(result));
        }));
        cx.notify();
    }

    fn schedule_operation_delivery(&self, cx: &mut Context<Self>) {
        let Some(operation_tx) = self.operation_tx.as_ref() else {
            return;
        };
        let operation_wake = operation_tx.wake();
        let release_wake = operation_wake.clone();
        cx.on_release(move |_, _| {
            // Local filesystem operations may finish after the surface closes,
            // but their delivery waiter must not outlive the owning entity.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                operation_wake.wait().await;
                let should_drain = operation_wake.take();
                let stopped = operation_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |file_manager, cx| {
                            file_manager.drain_operation_results(cx)
                        })
                        .unwrap_or(false);
                    if backlog_remaining {
                        operation_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_operation_results(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.operation_rx.as_ref() else {
            return false;
        };
        let batch = delivery::drain_channel(receiver, delivery::USER_ACTION_DELIVERY_BUDGET);
        let changed = !batch.items.is_empty();
        for event in batch.items {
            match event {
                FileManagerOperationEvent::Progress(progress) => {
                    self.operation_progress = Some(progress);
                }
                FileManagerOperationEvent::Finished(result) => {
                    if let Some(worker) = self.operation_thread.take() {
                        // Finished is sent as the worker's final action, so this
                        // join only reaps an already-completing owned thread.
                        let _ = worker.join();
                    }
                    if let Some(progress) = self.operation_progress.as_mut() {
                        progress.active = false;
                        progress.current = progress.total;
                        progress.file_name.clear();
                    }
                    match result {
                        Ok(()) => {
                            self.refresh();
                            cx.emit(FileManagerWorkspaceEvent::OperationSucceeded);
                        }
                        Err(error) => cx.emit(FileManagerWorkspaceEvent::Error(error)),
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
        batch.outcome.backlog_remaining
    }

    fn calculate_properties_checksum(&mut self, cx: &mut Context<Self>) {
        if self.properties_checksum_loading {
            return;
        }
        let Some(FileManagerDialog::Properties { entry, .. }) = self.dialog.as_ref() else {
            return;
        };
        if entry.file_type != LocalFileType::File {
            return;
        }
        let path = entry.path.clone();
        self.properties_checksum = None;
        self.properties_checksum_loading = true;
        let checksum_task = cx
            .background_executor()
            .spawn(async move { calculate_local_checksum(&path) });
        self.properties_checksum_task = Some(cx.spawn(async move |entity, cx| {
            let result = checksum_task.await;
            let _ = entity.update(cx, |file_manager, cx| {
                file_manager.properties_checksum_loading = false;
                file_manager.properties_checksum_task = None;
                match result {
                    Ok(checksum) => file_manager.properties_checksum = Some(checksum),
                    Err(error) => cx.emit(FileManagerWorkspaceEvent::Error(error)),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn refresh(&mut self) {
        self.loading = true;
        match list_local_files(&self.path) {
            Ok(files) => {
                self.replace_files(files);
                self.error = None;
                self.prune_selection();
            }
            Err(error) => {
                self.clear_files();
                self.error = Some(error.to_string());
            }
        }
        self.loading = false;
    }

    fn refresh_drives(&mut self) {
        self.drives = local_drives();
    }

    fn set_path(&mut self, path: String) {
        let normalized = normalize_local_path(&path);
        self.path = normalized.clone();
        self.path_input = normalized;
        self.path_completion.dismiss();
        self.path_scroll.set_offset(Point::new(px(0.0), px(0.0)));
        self.editing_path = false;
        self.focused_input = None;
        self.selected.clear();
        self.last_selected = None;
        self.clear_context_menu_immediately();
        self.list_scroll = UniformListScrollHandle::new();
        self.refresh();
    }

    fn clear_context_menu_immediately(&mut self) -> bool {
        let changed = self.context_menu.take().is_some();
        self.context_menu_exit_generation = None;
        self.context_menu_presence.reopen();
        self.context_menu_exit_task = None;
        changed
    }

    fn blur_inline_inputs(&mut self) {
        // Row interactions mirror the browser's input blur behavior without
        // routing page-local focus state through WorkspaceApp.
        if self.editing_path || self.focused_input == Some(FileManagerInput::Path) {
            self.path_input = self.path.clone();
            self.path_completion.dismiss();
            self.editing_path = false;
            self.focused_input = None;
        } else if self.focused_input == Some(FileManagerInput::Filter) {
            self.focused_input = None;
        }
    }

    fn select_entry(
        &mut self,
        name: String,
        modifiers: gpui::Modifiers,
        visible_files: &[LocalFileEntry],
    ) {
        self.blur_inline_inputs();
        if modifiers.shift {
            let anchor = self.last_selected.clone().unwrap_or_else(|| name.clone());
            let start = visible_files
                .iter()
                .position(|file| file.name == anchor)
                .unwrap_or(0);
            let end = visible_files
                .iter()
                .position(|file| file.name == name)
                .unwrap_or(start);
            self.selected.clear();
            for file in &visible_files[start.min(end)..=start.max(end)] {
                self.selected.insert(file.name.clone());
            }
        } else if modifiers.platform || modifiers.control {
            if !self.selected.insert(name.clone()) {
                self.selected.remove(&name);
            }
            self.last_selected = Some(name);
        } else {
            self.selected.clear();
            self.selected.insert(name.clone());
            self.last_selected = Some(name);
        }
    }

    fn activate_entry(&mut self, entry: LocalFileEntry, cx: &mut Context<Self>) {
        self.blur_inline_inputs();
        self.clear_context_menu_immediately();
        cx.emit(FileManagerWorkspaceEvent::OpenEntry(entry));
        cx.notify();
    }

    fn open_context_menu(
        &mut self,
        file: Option<LocalFileEntry>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.blur_inline_inputs();
        if let Some(file) = file.as_ref()
            && crate::workspace::browser_behavior::preserve_or_move_context_selection(
                &mut self.selected,
                file.name.clone(),
            )
        {
            self.last_selected = Some(file.name.clone());
        }
        self.context_menu_presence.reopen();
        self.context_menu_exit_generation = None;
        self.context_menu = Some(FileManagerContextMenu { file, x, y });
        cx.notify();
    }

    fn selected_names(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    fn selected_entries(&self) -> Vec<LocalFileEntry> {
        self.files
            .iter()
            .filter(|file| self.selected.contains(&file.name))
            .cloned()
            .collect()
    }

    fn single_selected_file(&self) -> Option<LocalFileEntry> {
        if self.selected.len() != 1 {
            return None;
        }
        let name = self.selected.iter().next()?;
        self.files.iter().find(|file| &file.name == name).cloned()
    }

    fn prune_selection(&mut self) {
        let names = self
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<HashSet<_>>();
        self.selected.retain(|name| names.contains(name.as_str()));
        if self
            .last_selected
            .as_ref()
            .is_some_and(|name| !names.contains(name.as_str()))
        {
            self.last_selected = None;
        }
    }

    pub(super) fn focused_input(&self) -> Option<FileManagerInput> {
        self.focused_input
    }

    pub(super) fn input_value(&self, input: FileManagerInput) -> &str {
        match input {
            FileManagerInput::Path => &self.path_input,
            FileManagerInput::Filter => &self.filter,
            FileManagerInput::DialogValue => &self.dialog_value,
        }
    }

    pub(super) fn replace_input(
        &mut self,
        input: FileManagerInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focused_input != Some(input) {
            return false;
        }
        let value = match input {
            FileManagerInput::Path => &mut self.path_input,
            FileManagerInput::Filter => &mut self.filter,
            FileManagerInput::DialogValue => &mut self.dialog_value,
        };
        replace_utf16(value, replacement_range, text);
        cx.notify();
        true
    }

    pub(super) fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(super) fn schedule_context_menu_exit(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(generation) = self.context_menu_exit_generation else {
            self.context_menu_exit_task = None;
            return;
        };
        if self.context_menu_exit_task.is_some() {
            return;
        }
        self.context_menu_exit_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |file_manager, cx| {
                if file_manager.context_menu_presence.finish_exit(generation) {
                    file_manager.context_menu = None;
                    file_manager.context_menu_exit_generation = None;
                    file_manager.context_menu_exit_task = None;
                    cx.notify();
                }
            });
        }));
    }

    fn sorted_files(&self) -> Arc<Vec<LocalFileEntry>> {
        if let Some(cache) = self.sorted_files_cache.borrow().as_ref()
            && cache.source_revision == self.source_revision
            && cache.filter == self.filter
            && cache.sort_field == self.sort_field
            && cache.sort_direction == self.sort_direction
        {
            return cache.files.clone();
        }

        let files = Arc::new(sorted_local_files(
            &self.files,
            &self.filter,
            self.sort_field,
            self.sort_direction,
        ));
        let rows = Arc::new(
            files
                .iter()
                .map(FileManagerListRow::new)
                .collect::<Vec<_>>(),
        );
        *self.sorted_files_cache.borrow_mut() = Some(FileManagerSortedFilesCache {
            source_revision: self.source_revision,
            filter: self.filter.clone(),
            sort_field: self.sort_field,
            sort_direction: self.sort_direction,
            files: files.clone(),
            rows,
        });
        files
    }

    fn sorted_file_rows(&self) -> Arc<Vec<FileManagerListRow>> {
        // Populate both aligned caches through the same validation path.
        let _ = self.sorted_files();
        self.sorted_files_cache
            .borrow()
            .as_ref()
            .expect("sorted file rows should exist after sorting")
            .rows
            .clone()
    }

    fn replace_files(&mut self, files: Vec<LocalFileEntry>) {
        // The revision keeps cache validation constant-time even when a directory is large.
        self.source_revision = self.source_revision.wrapping_add(1);
        self.files = files;
        self.sorted_files_cache.get_mut().take();
    }

    fn clear_files(&mut self) {
        self.replace_files(Vec::new());
    }

    pub(super) fn load(settings_path: &std::path::Path, cx: &mut Context<Self>) -> Self {
        let bookmarks_path = settings_path
            .parent()
            .unwrap_or(settings_path)
            .join(FILE_MANAGER_BOOKMARKS_FILENAME);
        let (operation_tx, operation_rx) = delivery::ActiveDeliverySender::channel();
        let mut state = Self {
            bookmarks_path,
            operation_tx: Some(operation_tx),
            operation_rx: Some(operation_rx),
            ..Self::default()
        };
        if let Ok(bytes) = std::fs::read(&state.bookmarks_path)
            && let Ok(bookmarks) = serde_json::from_slice::<Vec<LocalBookmark>>(&bytes)
        {
            state.bookmarks = bookmarks;
        }
        state.schedule_operation_delivery(cx);
        state
    }
}

fn file_manager_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, FILE_MANAGER_BG_ACTIVE_BG_ALPHA)
}

fn file_manager_panel_bg(color: u32, has_background: bool, alpha: u32) -> Rgba {
    color_with_background_scaled_alpha(
        color,
        has_background,
        alpha,
        FILE_MANAGER_BG_ACTIVE_PANEL_ALPHA,
    )
}

fn file_manager_hover_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, FILE_MANAGER_BG_ACTIVE_HOVER_ALPHA)
}

fn file_manager_border(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, 0x99)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn cache_entry(name: &str) -> LocalFileEntry {
        LocalFileEntry {
            name: name.to_string(),
            path: format!("/tmp/{name}"),
            file_type: LocalFileType::File,
            size: 0,
            modified: None,
            readonly: false,
            symlink_target: None,
        }
    }

    #[test]
    fn sorted_files_cache_reuses_results_until_the_query_changes() {
        let mut state = FileManagerState::default();
        state.replace_files(vec![cache_entry("beta"), cache_entry("alpha")]);

        // Unchanged render queries must reuse the same allocation.
        let initial = state.sorted_files();
        let reused = state.sorted_files();
        let initial_rows = state.sorted_file_rows();
        let reused_rows = state.sorted_file_rows();
        assert!(Arc::ptr_eq(&initial, &reused));
        assert!(Arc::ptr_eq(&initial_rows, &reused_rows));
        assert_eq!(initial_rows[0].display_name.as_ref(), "alpha");
        assert_eq!(initial_rows[0].size_text.as_ref(), "0 B");
        assert_eq!(initial_rows[0].modified_text.as_ref(), "-");

        // Filter changes invalidate the query without requiring an explicit mutation hook.
        state.filter = "beta".to_string();
        let filtered = state.sorted_files();
        let filtered_rows = state.sorted_file_rows();
        assert!(!Arc::ptr_eq(&initial, &filtered));
        assert!(!Arc::ptr_eq(&initial_rows, &filtered_rows));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "beta");

        // A directory refresh invalidates cached results even when the query is unchanged.
        state.replace_files(vec![cache_entry("beta"), cache_entry("beta-2")]);
        let refreshed = state.sorted_files();
        let refreshed_rows = state.sorted_file_rows();
        assert!(!Arc::ptr_eq(&filtered, &refreshed));
        assert!(!Arc::ptr_eq(&filtered_rows, &refreshed_rows));
        assert_eq!(refreshed.len(), 2);
    }

    #[test]
    fn file_row_selection_is_owned_by_file_manager_entity() {
        let mut state = FileManagerState::default();
        let visible = vec![cache_entry("alpha"), cache_entry("beta")];

        state.select_entry("alpha".to_string(), gpui::Modifiers::default(), &visible);

        assert_eq!(state.selected, HashSet::from(["alpha".to_string()]));
        assert_eq!(state.last_selected.as_deref(), Some("alpha"));
    }

    #[gpui::test]
    fn file_activation_emits_typed_workspace_intent(cx: &mut TestAppContext) {
        let file_manager = cx.new(|_| FileManagerState::default());
        let observed = Arc::new(AtomicBool::new(false));
        let observed_event = observed.clone();
        let _subscription = file_manager.update(cx, |_, cx| {
            cx.subscribe(
                &file_manager,
                move |_, _, event: &FileManagerWorkspaceEvent, _cx| {
                    if matches!(
                        event,
                        FileManagerWorkspaceEvent::OpenEntry(entry) if entry.name == "alpha"
                    ) {
                        observed_event.store(true, Ordering::Release);
                    }
                },
            )
        });

        file_manager.update(cx, |file_manager, cx| {
            file_manager.activate_entry(cache_entry("alpha"), cx);
        });

        assert!(observed.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn operation_delivery_finishes_without_a_mounted_workspace(cx: &mut TestAppContext) {
        let file_manager = cx.new(|cx| {
            let (operation_tx, operation_rx) = delivery::ActiveDeliverySender::channel();
            let state = FileManagerState {
                operation_tx: Some(operation_tx),
                operation_rx: Some(operation_rx),
                ..FileManagerState::default()
            };
            state.schedule_operation_delivery(cx);
            state
        });
        let sender = file_manager.read_with(cx, |file_manager, _cx| {
            file_manager
                .operation_tx
                .as_ref()
                .expect("operation sender")
                .clone()
        });
        sender
            .send(FileManagerOperationEvent::Progress(
                FileManagerOperationProgress {
                    current: 1,
                    total: 2,
                    file_name: "first".to_string(),
                    active: true,
                },
            ))
            .expect("progress delivery");
        sender
            .send(FileManagerOperationEvent::Finished(Err(
                "expected test failure".to_string(),
            )))
            .expect("completion delivery");

        // Delivery remains entity-owned when no File Manager page or root app is mounted.
        cx.run_until_parked();

        file_manager.read_with(cx, |file_manager, _cx| {
            let progress = file_manager.operation_progress.as_ref().expect("progress");
            assert!(!progress.active);
            assert_eq!(progress.current, progress.total);
            assert!(progress.file_name.is_empty());
        });
    }
}
