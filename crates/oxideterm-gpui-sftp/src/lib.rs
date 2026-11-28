// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod bookmarks;
pub use bookmarks::SftpBookmarks;
mod browser;
mod dialog;
pub use browser::{SelectionRole, SftpBrowserFrame, SftpBrowserRequest, SftpBrowserView};
mod diff;
pub use dialog::{
    SftpDialogFrame, SftpDialogIme, SftpDialogRequest, SftpDialogView, SftpPreviewEditorPreferences,
};
#[cfg(not(target_family = "wasm"))]
pub mod native_video;
pub use diff::{SftpDiffFrame, SftpDiffView};
mod presentation;
pub use presentation::{render_presentation_choice, sftp_open_presentation, sidebar_sftp_target};
mod queue;
pub mod style;

pub use queue::{
    QueuePresentation, QueueTextKey, SftpQueueData, SftpQueueFrame, SftpQueueRequest, SftpQueueView,
};

mod file_list;
mod tree;

pub use file_list::{
    SftpFileListData, SftpFileListFrame, SftpFileListRequest, SftpFileListView, SftpPane,
    sftp_file_list_virtual_spec,
};
pub use tree::{SftpTreeData, SftpTreeRow};

use gpui::{
    App, Context, Entity, EventEmitter, Pixels, Point, ScrollHandle, Subscription, Task,
    UniformListScrollHandle, prelude::*, px,
};
#[cfg(not(target_family = "wasm"))]
use native_video::SharedSftpNativeVideoSurface;
use oxideterm_editor_syntax::LanguageId;
use oxideterm_gpui_editor::TextEditorView;
use oxideterm_gpui_file_manager::PathCompletionState;
use oxideterm_gpui_markdown::{MarkdownPreviewCache, MarkdownVirtualListScrollHandle};
#[cfg(not(target_family = "wasm"))]
use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};
#[cfg(target_family = "wasm")]
mod notice;
#[cfg(target_family = "wasm")]
pub use notice::{TerminalNotice, TerminalNoticeVariant};
use oxideterm_gpui_ui::virtual_list::{
    TauriVirtualScrollAlign, scroll_tauri_virtual_list_to_index, uniform_list_edge_autoscroll,
};
use oxideterm_local_files::PathCompletionCandidate;
use oxideterm_preview::{
    AudioPreviewBackend, AudioPreviewCommand, AudioPreviewState, PreviewAssetOwner, TextLineEnding,
    font_family_name_from_bytes, normalize_text_line_endings, restore_text_line_endings,
};
use oxideterm_sftp::{
    AssetFileKind, BackgroundTransferDirection, BackgroundTransferSnapshot,
    BackgroundTransferState, LocalDownloadDisposition, NodeId, PreviewContent, RemoteSftpListing,
    SftpFileEntry, SftpFileType, SftpPreviewSaveResult, StoredTransferProgress,
    TransferConflict as SftpConflictInfo, TransferDirection as SftpTransferDirection,
    TransferProtocol as RemoteTransferProtocol,
    browser_files::{SftpSortDirection, SftpSortField},
    join_remote_path as join_sftp_path, normalize_remote_path,
};
use oxideterm_workspace::{TabId, delivery};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    sync::Arc,
    time::Duration,
};

mod browser_state;
mod keyboard;
mod layout;
mod path_state;
pub use keyboard::{SftpKeyAction, SftpKeyResult};
pub use path_state::{SftpCompletionLoad, SftpPathLoad, remote_path_completion_request};
mod editor_state;
mod file_mutation;
mod local_host;
pub use file_mutation::{
    SftpDialogSubmission, SftpFileOperation, SftpRemoteMutation, sftp_i18n_count,
};
use local_host::BrowseAudit;
pub use local_host::SftpLocalHost;
mod menu_state;
mod state;
mod state_helpers;
mod transfer_state;
mod worker;

pub use browser_state::*;
#[cfg(not(target_family = "wasm"))]
pub use menu_state::build_sftp_pending_uploads;
pub use menu_state::{SftpConflictDecision, sftp_terminal_drop_conflict_resolution};
pub use state::*;
pub use state_helpers::*;
pub use transfer_state::{
    SftpRelayLaunch, SftpResumeLaunch, SftpTransferControl, SftpTransferLaunch,
};

#[derive(Default)]
struct SftpViews {
    queue: Option<Entity<SftpQueueView<SftpRemoteId>>>,
    dialog: Option<Entity<SftpDialogView>>,
    browser: Option<Entity<SftpBrowserView>>,
    local_list: Option<Entity<SftpFileListView>>,
    remote_list: Option<Entity<SftpFileListView>>,
}
