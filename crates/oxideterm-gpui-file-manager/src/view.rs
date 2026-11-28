#[cfg(not(target_family = "wasm"))]
use crate::filesystem::desktop::DesktopFileSystem;
use crate::{
    PathCompletionState,
    bridge::*,
    filesystem::{BatchHandle, FileBatch, FileSystemBridge},
};
use gpui::prelude::*;
use gpui::{
    self, AnchoredPositionMode, AnyElement, AnyView, App, ClipboardItem, Context, Corner,
    ElementId, Entity, EventEmitter, IntoElement, KeyDownEvent, ListAlignment, ListState,
    MouseButton, MouseDownEvent, MouseMoveEvent, ObjectFit, ParentElement, Pixels, Point, Render,
    Rgba, ScrollEvent, ScrollHandle, SharedString, Styled, StyledImage, Subscription, Task,
    UniformListScrollHandle, Window, anchored, deferred, div, px, relative, rgb, rgba, svg,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_editor::{EditorContextMenuLabels, TextEditorView};
#[cfg(test)]
use oxideterm_gpui_markdown::MarkdownDocument;
use oxideterm_gpui_markdown::MarkdownPreviewCache;
use oxideterm_gpui_markdown::{MarkdownOptions, MarkdownVirtualListScrollHandle, highlight};
use oxideterm_gpui_ui::{
    breadcrumb_scroll::scroll_breadcrumb_by_wheel, confirm::ConfirmDialogAction, icons::LucideIcon,
    virtual_list::*,
};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, ButtonVariant, IconButtonOptions, ToolbarButtonOptions},
    context_menu::{ContextMenuActionableStyle, context_menu_event_boundary},
    modal::{
        TauriBackdropRole, overlay_content_boundary, rounded_shell_child_radius,
        staged_dialog_overlay, staged_overlay_root, staged_overlay_scrim,
    },
    scroll::ScrollableElement,
    surface::{
        MaterialRole, color_for_background, color_with_background_scaled_alpha, material_surface,
    },
    text_input::TextInputView,
};
#[cfg(test)]
use oxideterm_i18n::I18n;
use oxideterm_i18n::Locale;
use oxideterm_local_files::{
    BOOKMARKS_FILENAME as FILE_MANAGER_BOOKMARKS_FILENAME, LocalArchiveEntry, LocalArchiveInfo,
    LocalBookmark, LocalChecksumResult, LocalClipboard, LocalClipboardMode, LocalDrive,
    LocalFileEntry, LocalFileType, LocalPreview, LocalPreviewMetadata, LocalSidebarLocation,
    LocalSidebarLocationKind, LocalSortDirection, LocalSortField,
};
use oxideterm_local_files::{
    PathCompletionCandidate, append_local_preview_chunk, local_path_completion_request,
};
use oxideterm_preview::{
    AudioPreviewBackend, AudioPreviewCommand, AudioPreviewState, font_family_name_from_bytes,
};
use oxideterm_settings::{
    FILE_MANAGER_SIDEBAR_DEFAULT_WIDTH, FILE_MANAGER_SIDEBAR_MAX_WIDTH,
    FILE_MANAGER_SIDEBAR_MIN_WIDTH,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::delivery;
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::{HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

mod actions;
mod chrome;
mod dialogs;
mod helpers;
mod layout;
mod render;

use self::chrome::*;
use self::helpers::*;
use self::layout::FileManagerBookmarksResize;
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
const FILE_MANAGER_CONTEXT_MENU_VIEWPORT_MARGIN: f32 = 8.0;
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
pub enum FileManagerInput {
    Path,
    Filter,
    DialogValue,
}

impl FileManagerInput {
    pub fn anchor_key(self) -> u64 {
        match self {
            Self::Path => 1,
            Self::Filter => 2,
            Self::DialogValue => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FileManagerContextMenu {
    file: Option<LocalFileEntry>,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
pub enum FileManagerDialog {
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
        details: oxideterm_local_files::LocalFileProperties,
    },
    Preview {
        entry: LocalFileEntry,
    },
}

#[derive(Clone, Debug)]
enum FileManagerWorkspaceEvent {
    Error(String),
    OperationSucceeded,
    BookmarksSaved(actions::bookmarks::BookmarkChange),
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

struct FileManagerSortedFilesCache {
    source_revision: u64,
    icon_revision: u64,
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
    icon: oxideterm_gpui_ui::file_icons::FileIcon,
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
        let icon = file_icon_for_entry(file);
        Self {
            display_name: display_name.into(),
            size_text: size_text.into(),
            modified_text: modified_text.into(),
            icon,
        }
    }
}

pub struct FileManagerState {
    fs: Rc<dyn FileSystemBridge>,
    pub(super) path: String,
    pub(super) path_input: String,
    pub path_completion: PathCompletionState,
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
    pub focused_input: Option<FileManagerInput>,
    pub(super) focused_dialog_footer_action: Option<ConfirmDialogAction>,
    pub(super) context_menu: Option<FileManagerContextMenu>,
    pub(super) context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) context_menu_exit_generation: Option<u64>,
    pub(super) context_menu_exit_task: Option<Task<()>>,
    pub dialog: Option<FileManagerDialog>,
    pub(super) dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    dialog_exit_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    folder_picker_task: Option<Task<()>>,
    listing_task: Option<Task<()>>,
    completion_task: Option<Task<()>>,
    preview_task: Option<Task<()>>,
    stream_task: Option<Task<()>>,
    mutation_task: Option<Task<()>>,
    pub(super) dialog_value: String,
    pub(super) clipboard: Option<LocalClipboard>,
    pub(super) bookmarks: Vec<LocalBookmark>,
    bookmark_edits: actions::bookmarks::BookmarkEdits,
    pub(super) sidebar_locations: Vec<LocalSidebarLocation>,
    pub(super) drives: Vec<LocalDrive>,
    pub(super) bookmarks_path: PathBuf,
    pub(super) bookmarks_visible: bool,
    bookmarks_surface_size: Option<(gpui::WindowId, gpui::Size<Pixels>)>,
    bookmarks_motion: oxideterm_gpui_ui::motion::SidebarMotion,
    bookmarks_resize: Option<FileManagerBookmarksResize>,
    bookmarks_resize_activation: Option<Subscription>,
    bookmarks_resize_close: Option<Subscription>,
    pub(super) list_scroll: UniformListScrollHandle,
    // Preview payloads can contain large text or archive listings. Share the
    // immutable payload across render snapshots instead of cloning its contents.
    pub(super) preview: Option<Arc<LocalPreview>>,
    pub(super) preview_plugin: Option<AnyView>,
    pub(super) preview_editor: Option<Entity<TextEditorView>>,
    pub(super) preview_metadata: Option<LocalPreviewMetadata>,
    pub(super) preview_show_metadata: bool,
    pub(super) preview_markdown_source: bool,
    pub(super) preview_image_zoom: f32,
    pub(super) preview_image_rotation: i32,
    pub(super) preview_code_scroll: UniformListScrollHandle,
    pub(super) preview_markdown_scroll: MarkdownVirtualListScrollHandle,
    pub(super) preview_markdown_cache: MarkdownPreviewCache<LocalPreview>,
    pub preview_document_scroll: ScrollHandle,
    pub preview_metadata_scroll: ScrollHandle,
    pub(super) preview_archive_list_state: ListState,
    pub(super) preview_archive_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) preview_stream: FileManagerPreviewStreamState,
    pub(super) preview_audio: Box<dyn AudioPreviewBackend>,
    pub(super) preview_video_surface: FileManagerVideo,
    pub(super) preview_font_family: Option<String>,
    pub(super) preview_font_error: Option<String>,
    pub(super) preview_font_size: f32,
    pub(super) operation_progress: Option<FileManagerOperationProgress>,
    operation_tx: Option<delivery::ActiveDeliverySender<FileManagerOperationEvent>>,
    operation_rx: Option<std::sync::mpsc::Receiver<FileManagerOperationEvent>>,
    operation_worker: Option<BatchHandle>,
    pub(super) properties_checksum: Option<LocalChecksumResult>,
    pub(super) properties_checksum_loading: bool,
    pub(super) properties_checksum_task: Option<Task<()>>,
}

#[cfg(not(target_family = "wasm"))]
impl Default for FileManagerState {
    fn default() -> Self {
        Self::initial(
            Rc::new(DesktopFileSystem),
            oxideterm_local_files::default_file_manager_bookmarks_path(),
        )
    }
}

impl FileManagerState {
    fn initial(fs: Rc<dyn FileSystemBridge>, bookmarks_path: PathBuf) -> Self {
        let path = fs.home();
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
            #[cfg(not(target_family = "wasm"))]
            folder_picker_task: None,
            listing_task: None,
            completion_task: None,
            preview_task: None,
            stream_task: None,
            mutation_task: None,
            dialog_value: String::new(),
            clipboard: None,
            bookmarks: Vec::new(),
            bookmark_edits: Default::default(),
            // Resolve system folders once so paint does not repeatedly query the filesystem.
            sidebar_locations: fs.sidebar_locations(),
            // Disk discovery performs synchronous system and filesystem queries.
            // Cache it outside render and refresh only at explicit interaction boundaries.
            drives: fs.drives(),
            bookmarks_path,
            bookmarks_visible: true,
            bookmarks_surface_size: None,
            bookmarks_motion: oxideterm_gpui_ui::motion::SidebarMotion::new(
                FILE_MANAGER_SIDEBAR_DEFAULT_WIDTH as f32,
            ),
            bookmarks_resize: None,
            bookmarks_resize_activation: None,
            bookmarks_resize_close: None,
            list_scroll: UniformListScrollHandle::new(),
            preview: None,
            preview_plugin: None,
            preview_editor: None,
            preview_metadata: None,
            preview_show_metadata: true,
            preview_markdown_source: false,
            preview_image_zoom: 1.0,
            preview_image_rotation: 0,
            preview_code_scroll: UniformListScrollHandle::new(),
            preview_markdown_scroll: MarkdownVirtualListScrollHandle::new(),
            preview_markdown_cache: MarkdownPreviewCache::default(),
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
            preview_audio: fs.audio(),
            fs,
            preview_video_surface: FileManagerVideo::default(),
            preview_font_family: None,
            preview_font_error: None,
            preview_font_size: 32.0,
            operation_progress: None,
            operation_tx: None,
            operation_rx: None,
            operation_worker: None,
            properties_checksum: None,
            properties_checksum_loading: false,
            properties_checksum_task: None,
        }
    }
}

impl FileManagerState {
    fn close_dialog(&mut self, cx: &mut Context<Self>) {
        self.preview_task = None;
        self.stream_task = None;
        if let Err(error) = self.preview_audio.command(AudioPreviewCommand::Stop) {
            cx.emit(FileManagerWorkspaceEvent::Error(error));
        }
        (self.preview_video_surface.detach)();
        self.dialog = None;
        self.dialog_exit_task = None;
        self.focused_input = None;
        self.focused_dialog_footer_action = None;
        self.dialog_value.clear();
        self.preview = None;
        self.fs.release_preview();
        self.preview_plugin = None;
        self.preview_editor = None;
        self.preview_metadata = None;
        self.preview_markdown_source = false;
        self.preview_code_scroll = UniformListScrollHandle::new();
        self.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.preview_markdown_cache = MarkdownPreviewCache::default();
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
        (self.preview_video_surface.detach)();
        if delay.is_zero() {
            self.finish_rich_dialog_exit(generation, cx);
            return true;
        }
        self.dialog_exit_task = Some(cx.spawn(async move |entity, cx| {
            cx.background_executor().timer(delay).await;
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

    #[cfg(not(target_family = "wasm"))]
    fn start_folder_picker(
        &mut self,
        selection: crate::filesystem::FolderSelection,
        cx: &mut Context<Self>,
    ) {
        if self.folder_picker_task.is_some() {
            return;
        }
        let begin_browse = self.fs.browse_audit();
        self.folder_picker_task = Some(cx.spawn(async move |entity, cx| {
            let selected_path = selection.await;
            let _ = entity.update(cx, |file_manager, cx| {
                file_manager.folder_picker_task = None;
                if let Some(path) = selected_path {
                    let path = path.to_string_lossy().into_owned();
                    let finish = begin_browse(&path);
                    file_manager.set_path(path, Some(finish), cx);
                }
                cx.notify();
            });
        }));
    }

    fn start_operation(&mut self, kind: FileBatch, paths: Vec<String>, cx: &mut Context<Self>) {
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
        let total = match &kind {
            #[cfg(target_family = "wasm")]
            FileBatch::BrowserDrop { payload, .. } => payload.0.len(),
            _ => self.fs.batch_units(&paths),
        };
        self.operation_progress = Some(FileManagerOperationProgress {
            current: 0,
            total: total.max(1),
            file_name: String::new(),
            active: true,
        });
        let updates = sender.clone();
        self.operation_worker = Some(self.fs.start_batch(
            kind,
            paths,
            total,
            Box::new(move |progress| {
                let _ = updates.send(FileManagerOperationEvent::Progress(
                    FileManagerOperationProgress {
                        current: progress.current,
                        total: total.max(1),
                        file_name: progress.file_name,
                        active: true,
                    },
                ));
            }),
            Box::new(move |result| {
                let _ = sender.send(FileManagerOperationEvent::Finished(result));
            }),
            cx,
        ));
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
                    if let Some(finish) = self.operation_worker.take() {
                        finish();
                    }
                    if let Some(progress) = self.operation_progress.as_mut() {
                        progress.active = false;
                        if result.is_ok() {
                            progress.current = progress.total;
                        }
                        progress.file_name.clear();
                    }
                    // A failed copy or move may already have changed the directory.
                    self.refresh(None, cx);
                    match result {
                        Ok(()) => {
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
        let checksum_task = self.fs.checksum(path, cx);
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

    fn refresh(&mut self, finish: Option<Box<dyn FnOnce(bool)>>, cx: &mut Context<Self>) {
        self.listing_task = None;
        self.loading = true;
        self.error = None;
        self.listing_task = self
            .fs
            .list(&self.path)
            .deliver(self, cx, move |state, result, cx| {
                if let Some(finish) = finish {
                    finish(result.is_ok());
                }
                match result {
                    Ok(files) => {
                        state.replace_files(files);
                        state.prune_selection();
                    }
                    Err(error) => {
                        state.clear_files();
                        state.error = Some(error);
                    }
                }
                state.loading = false;
                cx.notify();
            });
    }

    fn refresh_drives(&mut self) {
        self.drives = self.fs.drives();
    }

    fn set_path(
        &mut self,
        path: String,
        finish: Option<Box<dyn FnOnce(bool)>>,
        cx: &mut Context<Self>,
    ) {
        let normalized = normalize_local_path(&path);
        if self.path != normalized {
            self.clear_files();
        }
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
        self.refresh(finish, cx);
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
            && browser_behavior::preserve_or_move_context_selection(
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

    pub fn focused_input(&self) -> Option<FileManagerInput> {
        self.focused_input
    }

    pub fn input_value(&self, input: FileManagerInput) -> &str {
        match input {
            FileManagerInput::Path => &self.path_input,
            FileManagerInput::Filter => &self.filter,
            FileManagerInput::DialogValue => &self.dialog_value,
        }
    }

    pub fn replace_input(
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

    pub fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
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
            cx.background_executor().timer(delay).await;
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
        let icon_revision = oxideterm_gpui_ui::file_icons::plugin_icon_revision();
        if let Some(cache) = self.sorted_files_cache.borrow().as_ref()
            && cache.source_revision == self.source_revision
            && cache.icon_revision == icon_revision
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
            icon_revision,
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

    fn load(settings_path: &Path, fs: Rc<dyn FileSystemBridge>, cx: &mut Context<Self>) -> Self {
        let bookmarks_path = settings_path
            .parent()
            .unwrap_or(settings_path)
            .join(FILE_MANAGER_BOOKMARKS_FILENAME);
        let (operation_tx, operation_rx) = delivery::ActiveDeliverySender::channel();
        let mut state = Self {
            operation_tx: Some(operation_tx),
            operation_rx: Some(operation_rx),
            ..Self::initial(fs, bookmarks_path)
        };
        state.read_bookmarks(cx);
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

    #[test]
    fn markdown_preview_cache_tracks_payload_options_and_release() {
        let mut cache = MarkdownPreviewCache::default();
        let mut scroll = MarkdownVirtualListScrollHandle::new();
        let owner = Arc::new(LocalPreview::Markdown {
            content: "'quoted'".into(),
        });
        let text = |document: &MarkdownDocument| {
            let mut fragments = Vec::new();
            oxideterm_gpui_markdown::render::visit_selectable_text(
                document,
                &MarkdownOptions::default(),
                &mut |_, text| fragments.push(text.to_owned()),
            );
            fragments.join("\n")
        };
        let document = cache.document(&owner, "'quoted'", true, &mut scroll);
        let reused = cache.document(&owner, "'quoted'", true, &mut scroll);
        assert!(Arc::ptr_eq(&document.blocks, &reused.blocks));
        assert_eq!(text(&document), "‘quoted’");
        let plain = cache.document(&owner, "'quoted'", false, &mut scroll);
        assert_eq!(text(&plain), "'quoted'");
        let blocks = Arc::downgrade(&plain.blocks);
        drop(plain);
        cache.release(&mut scroll);
        assert!(blocks.upgrade().is_none());
        let resumed = cache.document(&owner, "'quoted'", false, &mut scroll);
        assert_eq!(text(&resumed), "'quoted'");
        let next = Arc::new(LocalPreview::Markdown {
            content: "# Replacement".into(),
        });
        let replacement = cache.document(&next, "# Replacement", false, &mut scroll);
        assert_eq!(text(&replacement), "Replacement");
        assert_eq!(text(&resumed), "'quoted'");
    }

    #[gpui::test]
    fn markdown_preview_cache_releases_on_close(cx: &mut TestAppContext) {
        let manager = cx.new(|_| FileManagerState::default());
        manager.update(cx, |state, cx| {
            let owner = Arc::new(LocalPreview::Markdown {
                content: "# Local".into(),
            });
            let document = state.preview_markdown_cache.document(
                &owner,
                "# Local",
                true,
                &mut state.preview_markdown_scroll,
            );
            let blocks = Arc::downgrade(&document.blocks);
            drop(document);
            state.preview = Some(owner);
            state.close_dialog(cx);
            assert!(
                blocks.upgrade().is_none(),
                "closed local preview retained its parsed document"
            );
        });
    }

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
        assert_eq!(
            refreshed
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["beta", "beta-2"]
        );
        assert_eq!(
            refreshed_rows
                .iter()
                .map(|row| row.display_name.as_ref())
                .collect::<Vec<_>>(),
            vec!["beta", "beta-2"]
        );
    }

    #[gpui::test]
    fn selecting_and_opening_a_file_preserves_selection_and_emits_its_identity(
        cx: &mut TestAppContext,
    ) {
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
            let visible = vec![cache_entry("alpha"), cache_entry("beta")];
            file_manager.select_entry("alpha".to_string(), gpui::Modifiers::default(), &visible);
            file_manager.activate_entry(cache_entry("alpha"), cx);
            assert_eq!(file_manager.selected, HashSet::from(["alpha".to_string()]));
            assert_eq!(file_manager.last_selected.as_deref(), Some("alpha"));
        });

        assert!(observed.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn operation_delivery_finishes_without_a_mounted_workspace(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let file_manager = cx.new(|cx| {
            let (operation_tx, operation_rx) = delivery::ActiveDeliverySender::channel();
            let state = FileManagerState {
                path: directory.path().to_str().unwrap().to_owned(),
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
        std::fs::write(directory.path().join("first"), b"copied").unwrap();
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
            assert_eq!((progress.current, progress.total), (1, 2));
            assert_eq!(
                file_manager
                    .files
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                ["first"]
            );
            assert!(progress.file_name.is_empty());
        });
    }
}

#[cfg(target_family = "wasm")]
mod fields;

pub struct FileManagerView {
    #[cfg(target_family = "wasm")]
    fields: fields::BrowserFields,
    file_manager: Entity<FileManagerState>,
    bindings: FileManagerBindings,
    frame: FileManagerEnvironment,
    dialog_only: bool,
    segmented_control_user_motion: oxideterm_gpui_ui::motion::UserSegmentedControlMotionState,
    _state_observation: Subscription,
    _state_events: Option<Subscription>,
}

impl std::ops::Deref for FileManagerView {
    type Target = FileManagerEnvironment;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl FileManagerView {
    fn new(
        state: Entity<FileManagerState>,
        bindings: FileManagerBindings,
        frame: FileManagerEnvironment,
        dialog_only: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut window_interaction = (false, None);
        let observation = cx.observe(&state, move |this, state, cx| {
            if !dialog_only {
                let state = state.read(cx);
                let interaction = (
                    state.dialog.is_some(),
                    state.bookmarks_resize.map(|drag| drag.window_id),
                );
                if interaction != window_interaction {
                    window_interaction = interaction;
                    this.request(FileManagerRequest::WindowInteractionChanged, cx);
                }
            }
            cx.notify();
        });
        let events = (!dialog_only).then(|| {
            cx.subscribe(&state, |this, _, event, cx| match event {
                FileManagerWorkspaceEvent::Error(error) => this.push_file_manager_toast(
                    this.i18n.t("fileManager.error"),
                    Some(this.i18n.t(error)),
                    FileManagerNoticeKind::Error,
                    cx,
                ),
                FileManagerWorkspaceEvent::OperationSucceeded => this.push_file_manager_toast(
                    this.i18n.t("fileManager.operationSuccess"),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                ),
                FileManagerWorkspaceEvent::BookmarksSaved(change) => {
                    this.bookmark_saved(change, cx)
                }
                FileManagerWorkspaceEvent::OpenEntry(entry) => {
                    this.open_file_manager_entry(entry.clone(), cx)
                }
            })
        });
        Self {
            #[cfg(target_family = "wasm")]
            fields: fields::BrowserFields::new(dialog_only, frame.tokens, cx),
            file_manager: state,
            bindings,
            frame,
            dialog_only,
            segmented_control_user_motion: Default::default(),
            _state_observation: observation,
            _state_events: events,
        }
    }

    fn request(&self, request: FileManagerRequest, cx: &mut App) {
        let dispatch = self.bindings.dispatch.clone();
        cx.defer(move |cx| dispatch(request, cx));
    }

    fn pointer(&self, request: FileManagerPointer, window: &mut Window, cx: &mut App) {
        if matches!(
            request,
            FileManagerPointer::DismissTransient | FileManagerPointer::OpenTerminal(_)
        ) {
            let dispatch = self.bindings.pointer.clone();
            window.defer(cx, move |window, cx| dispatch(request, window, cx));
        } else {
            (self.bindings.pointer)(request, window, cx);
        }
    }
}

impl Render for FileManagerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(frame) = (self.bindings.frame)(cx) {
            self.frame = frame;
        }
        #[cfg(target_family = "wasm")]
        self.sync_fields(window, cx);
        if self.dialog_only {
            self.render_file_manager_dialog(window, self.modal_background, cx)
        } else {
            self.render_file_manager_surface(window, cx)
        }
    }
}

pub struct FileManagerViews {
    state: Entity<FileManagerState>,
    pub body: Entity<FileManagerView>,
    pub dialog: Entity<FileManagerView>,
}

impl FileManagerViews {
    #[cfg(not(target_family = "wasm"))]
    pub fn new(
        path: &Path,
        video: FileManagerVideo,
        bindings: FileManagerBindings,
        frame: FileManagerEnvironment,
        cx: &mut App,
    ) -> Self {
        Self::with_bridge(path, Rc::new(DesktopFileSystem), video, bindings, frame, cx)
    }

    pub fn with_bridge(
        path: &Path,
        fs: Rc<dyn FileSystemBridge>,
        video: FileManagerVideo,
        bindings: FileManagerBindings,
        frame: FileManagerEnvironment,
        cx: &mut App,
    ) -> Self {
        let state = cx.new(|cx| {
            let mut state = FileManagerState::load(path, fs, cx);
            state.preview_video_surface = video;
            state
        });
        let body = cx.new(|cx| {
            FileManagerView::new(state.clone(), bindings.clone(), frame.clone(), false, cx)
        });
        let dialog = cx.new(|cx| FileManagerView::new(state.clone(), bindings, frame, true, cx));
        Self {
            state,
            body,
            dialog,
        }
    }
}

impl std::ops::Deref for FileManagerViews {
    type Target = Entity<FileManagerState>;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl FileManagerInput {
    fn index(self) -> usize {
        match self {
            Self::Path => 0,
            Self::Filter => 1,
            Self::DialogValue => 2,
        }
    }

    fn anchor_id(self) -> oxideterm_gpui_ui::text_input::TextInputAnchorId {
        oxideterm_gpui_ui::text_input::TextInputAnchorId(1800 + self.anchor_key())
    }
}

#[cfg(test)]
struct TestFileManagerRoot(Entity<FileManagerView>);

#[cfg(test)]
impl Render for TestFileManagerRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

#[cfg(test)]
fn test_views(
    path: &Path,
    dispatch: impl Fn(FileManagerRequest, &mut App) + 'static,
    cx: &mut App,
) -> FileManagerViews {
    let tokens = oxideterm_theme::default_tokens();
    let frame = FileManagerEnvironment {
        tokens,
        i18n: I18n::new(oxideterm_i18n::Locale::En),
        background: false,
        modal_background: false,
        chrome_divider: rgb(tokens.ui.border),
        ui_font_family: "monospace".into(),
        mono_font_family: "monospace".into(),
        mono_font_size: 13.0,
        editor: FileManagerEditorFonts {
            editor_font_family: "monospace".into(),
            editor_font_weight: 400.0,
            editor_font_fallback: None,
            editor_font_size: 13.0,
            editor_line_height: 1.2,
            word_wrap: false,
            background_active: false,
        },
        markdown: MarkdownOptions::from_theme(&tokens),
        caret_visible: false,
        ime: Default::default(),
        bookmarks_width: FILE_MANAGER_SIDEBAR_DEFAULT_WIDTH,
    };
    let bindings = FileManagerBindings {
        frame: Rc::new(|_| None),
        dispatch: Rc::new(dispatch),
        pointer: Rc::new(|_, _, _| {}),
        anchor: Rc::new(|_| {}),
        selectable: Rc::new(|text, _| text.text.into_any_element()),
        document: Rc::new(|_, _, _, _, _| None),
        mermaid_zoom: None,
    };
    FileManagerViews::new(path, FileManagerVideo::default(), bindings, frame, cx)
}
