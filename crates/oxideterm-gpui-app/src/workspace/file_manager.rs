use std::borrow::Cow;

use super::ime::WorkspaceImeTarget;
use super::*;
use gpui::{
    AnchoredPositionMode, Corner, UniformListScrollHandle, anchored, deferred, prelude::*,
    uniform_list,
};
use gpui_component::scroll::ScrollableElement;
use oxideterm_gpui_markdown::{
    MarkdownOptions, MarkdownVirtualListScrollHandle, highlight, markdown_virtual_with_options,
};
use oxideterm_gpui_ui::{
    modal::{dialog_backdrop, popover_backdrop, quicklook_backdrop},
    surface::{color_for_background, color_with_background_scaled_alpha},
    text_input::{TextInputView, text_caret, text_input, text_input_anchor_probe},
};
use oxideterm_local_files::{
    BOOKMARKS_FILENAME as FILE_MANAGER_BOOKMARKS_FILENAME, LocalArchiveEntry, LocalArchiveInfo,
    LocalBookmark, LocalChecksumResult, LocalClipboardMode, LocalFileEntry, LocalFileType,
    LocalPreview, LocalPreviewMetadata, LocalSortDirection, LocalSortField,
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

const FILE_MANAGER_ROOT_PADDING: f32 = 8.0; // Tauri LocalFileManager/FileList p-2.
const FILE_MANAGER_GAP: f32 = 8.0; // Tauri gap-2.
const FILE_MANAGER_HEADER_HEIGHT: f32 = 40.0; // Tauri h-10.
const FILE_MANAGER_ROW_HEIGHT: f32 = 25.0; // Tauri FileList row px-2 py-1 text-xs.
const FILE_MANAGER_PREVIEW_CODE_WRAP_COLUMNS: usize = 96; // Virtual rows pre-wrap long `whitespace-pre` lines.
const FILE_MANAGER_PREVIEW_CODE_GUTTER_ALPHA: u32 = 0x4d; // Tauri CodeHighlight line-number opacity 30%.
const FILE_MANAGER_SIDEBAR_WIDTH: f32 = 220.0; // Tauri favorites sidebar column.
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
const FILE_MANAGER_BG_ACTIVE_BG_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg 40%.
const FILE_MANAGER_BG_ACTIVE_PANEL_ALPHA: u32 = 0x66; // [data-bg-active] --color-theme-bg-panel 40%.
const FILE_MANAGER_BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // [data-bg-active] --color-theme-bg-hover 50%.
const FILE_MANAGER_PANEL_80_ALPHA: u32 = 0xcc; // Tauri bg-theme-bg-panel/80.
const FILE_MANAGER_ACTIVE_BORDER_ALPHA: u32 = 0x80; // Tauri border-oxide-accent/50.
const FILE_MANAGER_SELECTED_BG_ALPHA: u32 = 0x33; // Tauri bg-theme-accent/20.
const FILE_MANAGER_BREADCRUMB_ACTIVE_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-hover/30.
const FILE_MANAGER_BREADCRUMB_HOVER_ALPHA: u32 = 0x80; // Tauri hover:bg-theme-bg-hover/50.
const FILE_MANAGER_DIALOG_BORDER_ALPHA: u32 = 0x99;
const FILE_MANAGER_RED: u32 = 0xf87171; // Tauri text-red-400.
const FILE_MANAGER_BLUE: u32 = 0x60a5fa; // Tauri text-blue-400.
const FILE_MANAGER_GREEN: u32 = 0x22c55e; // Tauri text-green-500.
const FILE_MANAGER_ORANGE: u32 = 0xfb923c; // Tauri text-orange-400.
const FILE_MANAGER_PURPLE: u32 = 0xc084fc; // Tauri preview/file accent family.

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
pub(super) struct FileManagerOperationProgress {
    pub(super) current: usize,
    pub(super) total: usize,
    pub(super) file_name: String,
    pub(super) active: bool,
}

#[derive(Debug)]
pub(super) enum FileManagerOperationEvent {
    Progress(FileManagerOperationProgress),
    Finished(Result<(), String>),
}

pub(super) struct FileManagerState {
    pub(super) path: String,
    pub(super) path_input: String,
    pub(super) editing_path: bool,
    pub(super) filter: String,
    pub(super) files: Vec<LocalFileEntry>,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    pub(super) selected: HashSet<String>,
    pub(super) last_selected: Option<String>,
    pub(super) sort_field: LocalSortField,
    pub(super) sort_direction: LocalSortDirection,
    pub(super) focused_input: Option<FileManagerInput>,
    pub(super) context_menu: Option<FileManagerContextMenu>,
    pub(super) dialog: Option<FileManagerDialog>,
    pub(super) dialog_value: String,
    pub(super) clipboard: Option<LocalClipboard>,
    pub(super) bookmarks: Vec<LocalBookmark>,
    pub(super) bookmarks_path: PathBuf,
    pub(super) bookmarks_visible: bool,
    pub(super) list_scroll: UniformListScrollHandle,
    pub(super) preview: Option<LocalPreview>,
    pub(super) preview_metadata: Option<LocalPreviewMetadata>,
    pub(super) preview_show_metadata: bool,
    pub(super) preview_markdown_source: bool,
    pub(super) preview_image_zoom: f32,
    pub(super) preview_image_rotation: i32,
    pub(super) preview_code_scroll: UniformListScrollHandle,
    pub(super) preview_markdown_scroll: MarkdownVirtualListScrollHandle,
    pub(super) preview_audio: RodioAudioPreviewBackend,
    pub(super) preview_video_surface: SharedSftpNativeVideoSurface,
    pub(super) preview_font_family: Option<String>,
    pub(super) preview_font_error: Option<String>,
    pub(super) preview_font_size: f32,
    pub(super) operation_progress: Option<FileManagerOperationProgress>,
    pub(super) operation_rx: Option<std::sync::mpsc::Receiver<FileManagerOperationEvent>>,
    pub(super) operation_poll_active: bool,
    pub(super) properties_checksum: Option<LocalChecksumResult>,
    pub(super) properties_checksum_loading: bool,
    pub(super) properties_checksum_rx:
        Option<std::sync::mpsc::Receiver<Result<LocalChecksumResult, String>>>,
    pub(super) properties_checksum_poll_active: bool,
}

impl Default for FileManagerState {
    fn default() -> Self {
        let path = home_path();
        Self {
            path: path.clone(),
            path_input: path,
            editing_path: false,
            filter: String::new(),
            files: Vec::new(),
            loading: false,
            error: None,
            selected: HashSet::new(),
            last_selected: None,
            sort_field: LocalSortField::Name,
            sort_direction: LocalSortDirection::Asc,
            focused_input: None,
            context_menu: None,
            dialog: None,
            dialog_value: String::new(),
            clipboard: None,
            bookmarks: Vec::new(),
            bookmarks_path: default_file_manager_bookmarks_path(),
            bookmarks_visible: true,
            list_scroll: UniformListScrollHandle::new(),
            preview: None,
            preview_metadata: None,
            preview_show_metadata: true,
            preview_markdown_source: false,
            preview_image_zoom: 1.0,
            preview_image_rotation: 0,
            preview_code_scroll: UniformListScrollHandle::new(),
            preview_markdown_scroll: MarkdownVirtualListScrollHandle::new(),
            preview_audio: RodioAudioPreviewBackend::default(),
            preview_video_surface: SharedSftpNativeVideoSurface::default(),
            preview_font_family: None,
            preview_font_error: None,
            preview_font_size: 32.0,
            operation_progress: None,
            operation_rx: None,
            operation_poll_active: false,
            properties_checksum: None,
            properties_checksum_loading: false,
            properties_checksum_rx: None,
            properties_checksum_poll_active: false,
        }
    }
}

impl FileManagerState {
    pub(super) fn load(settings_path: &std::path::Path) -> Self {
        let bookmarks_path = settings_path
            .parent()
            .unwrap_or(settings_path)
            .join(FILE_MANAGER_BOOKMARKS_FILENAME);
        let mut state = Self {
            bookmarks_path,
            ..Self::default()
        };
        if let Ok(bytes) = std::fs::read(&state.bookmarks_path)
            && let Ok(bookmarks) = serde_json::from_slice::<Vec<LocalBookmark>>(&bytes)
        {
            state.bookmarks = bookmarks;
        }
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
