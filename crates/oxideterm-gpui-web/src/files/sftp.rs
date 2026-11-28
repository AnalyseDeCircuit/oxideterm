use super::memory::MemoryFs;
use crate::{
    demo::{DemoDrives, DemoFiles, Files, SftpTarget, TransferDestination},
    host::Workspace,
};
use gpui::{prelude::*, *};
use oxideterm_gpui_sftp::*;
use oxideterm_gpui_ui::text_input::TextInputAnchorId;
use oxideterm_i18n::I18n;
use oxideterm_sftp::{
    NodeId, PreviewContent, SftpFileEntry, SftpFileType, SftpRemoteId, TransferDirection,
    TransferProtocol,
};
use oxideterm_theme::ThemeTokens;
use std::{rc::Rc, time::Duration};
mod device;
mod device_transfer;
mod fields;
mod transfer;
mod upload;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SidebarFrame {
    pub(super) has_target: bool,
    pub(super) pinned: bool,
    pub(super) active_tab: bool,
    pub(super) tree: bool,
}

pub struct Browser {
    pub target: SftpTarget,
    surface: SftpSurfaceId,
    sidebar: Option<SidebarFrame>,
    directory_session: Option<oxideterm_workspace::TerminalSessionId>,
    input_bounds: Rc<std::cell::RefCell<std::collections::HashMap<SftpInput, Bounds<Pixels>>>>,
    fields: fields::InputFields,
    left: Rc<MemoryFs>,
    right: Rc<MemoryFs>,
    remote_title: String,
    primary_title: Option<String>,
    remote_id: SftpRemoteId,
    state: Entity<SftpWorkspaceEntity>,
    _events: Subscription,
    preview_revision: Option<u64>,
    bookmarks: Entity<SftpBookmarks>,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub ui_font: SharedString,
    focus: FocusHandle,
    host: WeakEntity<Workspace>,
    conflict_destination: Option<transfer::Batch>,
    uploads: upload::UploadJobs,
    device: device::DevicePane,
}
impl Browser {
    pub(crate) fn sidebar_dialog_open(&self, cx: &App) -> bool {
        self.sidebar.is_some_and(|s| s.has_target && !s.active_tab)
            && self.state.read(cx).dialog_is_open()
    }
    fn notice_target(&self, owner: EntityId) -> Option<crate::host::NoticeTarget> {
        Some(match self.surface {
            SftpSurfaceId::Tab(tab) => crate::host::NoticeTarget::Page(tab),
            SftpSurfaceId::Sidebar => crate::host::NoticeTarget::EmbeddedFiles(owner),
        })
    }

    pub(super) fn set_sidebar(&mut self, frame: SidebarFrame, cx: &mut Context<Self>) {
        if self.sidebar != Some(frame) {
            self.sidebar = Some(frame);
            cx.notify();
        }
    }

    pub fn new(
        drives: &DemoDrives,
        target: SftpTarget,
        surface: SftpSurfaceId,
        title: String,
        tokens: ThemeTokens,
        i18n: I18n,
        ui_font: SharedString,
        host: WeakEntity<Workspace>,
        bookmarks: Entity<SftpBookmarks>,
        cx: &mut Context<Self>,
    ) -> Self {
        let [(left_drive, left_path), (right_drive, right_path)] = target.panes();
        let left = Rc::new(MemoryFs::new(drives.files(left_drive)));
        let right = Rc::new(MemoryFs::new(drives.files(right_drive)));
        let (remote_id, remote_title, primary_title) = match &target.profile {
            Some(p) => (
                SftpRemoteId::Standalone(target.id.clone()),
                p.secondary.as_ref().unwrap_or(&p.primary).label(),
                p.secondary.as_ref().map(|_| p.primary.label()),
            ),
            None => (SftpRemoteId::Node(NodeId::new(&target.id)), title, None),
        };
        let state = cx.new(|cx| {
            let mut s = SftpWorkspaceEntity::with_local_host(left.clone(), cx);
            if primary_title.is_some() {
                s.activate_pair_view(
                    surface,
                    SftpRemoteId::Standalone(format!("{}:primary", target.id)),
                    remote_id.clone(),
                    Some(left_path.into()),
                );
                s.apply_pair_primary_path(left_path.into());
                s.pair_primary_loading = false;
            } else {
                s.activate_view(surface, remote_id.clone());
                let _ = s.apply_local_path(left_path.into());
            }
            s.apply_remote_path(right_path.into());
            s.remote_loading = false;
            s.remote_load_pending = false;
            s
        });
        let events = cx.subscribe(&state, |browser, state, event, cx| {
            if let SftpWorkspaceEvent::OpenFileRequested { pane, file } = event {
                browser.open(*pane, file, cx);
                return;
            }
            if let SftpWorkspaceEvent::CancelOrRemoveTransferRequested { id } = event {
                state.update(cx, |s, cx| {
                    if s.cancel_or_remove_transfer(*id, cx).is_some() {
                        let batch = s
                            .transfers
                            .iter_mut()
                            .find(|i| i.id == *id)
                            .and_then(|i| i.batch_id.take());
                        if let Some(batch) = batch {
                            s.complete_transfer_batch_item(batch, SftpTransferState::Cancelled);
                        }
                    }
                });
                browser.uploads.wake(*id);
                return;
            }
            if let SftpWorkspaceEvent::TransferStateRequested {
                id,
                state: requested,
            } = event
            {
                state.update(cx, |s, cx| s.set_transfer_state(*id, *requested, cx));
                browser.uploads.wake(*id);
                return;
            }
            if let SftpWorkspaceEvent::WorkerEffectsReady(effects) = event {
                for effect in effects.take() {
                    match effect {
                        SftpWorkspaceEffect::Toast {
                            title,
                            description,
                            variant,
                        } => browser.notice(title, description, variant, cx),
                        SftpWorkspaceEffect::RemoteLoadPending { remote_id, .. }
                            if remote_id == browser.remote_id =>
                        {
                            browser.reload(SftpPane::Remote, cx)
                        }
                        SftpWorkspaceEffect::ReloadPairPrimaryDirectory => {
                            browser.reload(SftpPane::Local, cx)
                        }
                        SftpWorkspaceEffect::StartTreeChildrenLoad {
                            surface_id,
                            remote_id,
                            view_generation,
                            tree_generation,
                            path,
                        } if remote_id == browser.remote_id => {
                            let result = browser.right.sftp_list(&path).map(|files| {
                                oxideterm_sftp::RemoteSftpListing {
                                    cwd: path.clone(),
                                    files,
                                }
                            });
                            let _ = effects.delivery().send(SftpWorkerResult::TreeChildren {
                                surface_id,
                                remote_id,
                                view_generation,
                                tree_generation,
                                path,
                                result,
                            });
                        }
                        SftpWorkspaceEffect::TreeChildrenLoadFailed { path, error } => browser
                            .notice(
                                browser.i18n.t("sftp.preview.network_error"),
                                Some(format!("{path}: {error}")),
                                TerminalNoticeVariant::Error,
                                cx,
                            ),
                        _ => {}
                    }
                }
                return;
            }
            let SftpWorkspaceEvent::PreviewSaveRequested {
                expected_version,
                path,
                content,
                encoding,
                line_ending,
                generation,
                ..
            } = event
            else {
                return;
            };
            let s = state.read(cx);
            if *generation != s.preview_generation
                || s.preview_path.as_deref() != Some(path.as_str())
            {
                return;
            }
            let Some(pane) = s.preview_pane else {
                return;
            };
            let bom = matches!(
                s.preview_content.as_deref(),
                Some(PreviewContent::Text { has_bom: true, .. })
            );
            if pane == SftpPane::Local && browser.device.folder.is_some() {
                browser.save_device_preview(
                    *generation,
                    path.clone(),
                    content.clone(),
                    encoding,
                    *line_ending,
                    bom,
                    cx,
                );
                return;
            }
            let result = browser
                .fs(pane)
                .save_preview(
                    path,
                    expected_version.as_deref(),
                    content,
                    encoding,
                    *line_ending,
                    bom,
                )
                .map_err(|key| browser.i18n.t(key).replace("{{fileName}}", path));
            if let Ok(saved) = &result {
                browser.preview_revision = saved.version.as_deref().and_then(|v| v.parse().ok());
                browser.files_changed(pane, cx);
            }
            state.update(cx, |s, cx| {
                s.apply_preview_saved(
                    *generation,
                    path.clone(),
                    content.clone(),
                    result,
                    &browser.i18n.t("sftp.preview.network_error"),
                    cx,
                );
                cx.notify();
            });
        });
        let mut browser = Self {
            _events: events,
            preview_revision: None,
            state,
            target,
            surface,
            sidebar: None,
            directory_session: None,
            input_bounds: Default::default(),
            fields: fields::InputFields::new(tokens, cx),
            left,
            right,
            remote_title,
            primary_title,
            remote_id,
            tokens,
            i18n,
            ui_font,
            bookmarks,
            focus: cx.focus_handle(),
            host,
            conflict_destination: None,
            uploads: Default::default(),
            device: Default::default(),
        };
        browser.reload(SftpPane::Local, cx);
        browser.reload(SftpPane::Remote, cx);
        browser
    }
    fn fs(&self, pane: SftpPane) -> &MemoryFs {
        match pane {
            SftpPane::Local => &self.left,
            SftpPane::Remote => &self.right,
        }
    }
    fn reload(&mut self, pane: SftpPane, cx: &mut Context<Self>) {
        if pane == SftpPane::Local && self.device.folder.is_some() {
            self.reload_device(cx);
            return;
        }
        self.state.update(cx, |s, cx| {
            let (path, rows) = match pane {
                SftpPane::Local => (&s.local_path, &mut s.local_files),
                SftpPane::Remote => (&s.remote_path, &mut s.remote_files),
            };
            match self.fs(pane).sftp_list(path) {
                Ok(files) => {
                    *rows = files;
                    s.select_completed_path(pane);
                }
                Err(error) => {
                    rows.clear();
                    s.init_error = Some(SftpViewError::Directory(format!("{path}: {error}")));
                }
            }
            if pane == SftpPane::Remote {
                s.remote_loading = false;
                s.remote_load_pending = false;
            } else {
                s.pair_primary_loading = false;
            }
            cx.notify();
        });
    }
    pub fn reload_drive(&mut self, drive: &Files, cx: &mut Context<Self>) {
        for pane in [SftpPane::Local, SftpPane::Remote] {
            if !(pane == SftpPane::Local && self.device.folder.is_some())
                && Rc::ptr_eq(drive, &self.fs(pane).files)
            {
                if pane == SftpPane::Remote && self.surface == SftpSurfaceId::Sidebar {
                    self.state.update(cx, |s, _| s.clear_sftp_tree_state());
                }
                self.reload(pane, cx);
            }
        }
    }
    fn bookmark_key(&self) -> String {
        let key = self.remote_id.storage_key();
        if self.device.folder.is_some()
            && let SftpSurfaceId::Tab(id) = self.surface
        {
            return format!("{key}:device:{}:{}", id.0, self.device.generation);
        }
        key
    }

    fn files_changed(&self, pane: SftpPane, cx: &mut Context<Self>) {
        let drive = self.fs(pane).files.clone();
        let host = self.host.clone();
        cx.defer(move |cx| {
            let _ = host.update(cx, |h, cx| {
                if let Some(f) = &h.files {
                    f.refresh_targets(&drive, cx);
                    h.projects.refresh_drive(&drive, cx);
                }
            });
        });
    }
    pub(super) fn navigate(&mut self, pane: SftpPane, path: String, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| {
            s.init_error = None;
            match pane {
                SftpPane::Remote => {
                    if s.remote_path != path {
                        s.clear_sftp_tree_state();
                    }
                    s.apply_remote_path(path);
                    s.remote_loading = false;
                    s.remote_load_pending = false;
                }
                SftpPane::Local if s.pair_primary_remote_id.is_some() => {
                    s.apply_pair_primary_path(path);
                    s.pair_primary_loading = false;
                }
                SftpPane::Local => {
                    let _ = s.apply_local_path(path);
                }
            }
        });
        self.reload(pane, cx);
    }
    pub(crate) fn dirty_preview(&self, cx: &App) -> Option<String> {
        let s = self.state.read(cx);
        s.preview_editor_dirty
            .then(|| s.preview_path.clone())
            .flatten()
    }
    fn open(&mut self, p: SftpPane, f: &SftpFileEntry, cx: &mut Context<Self>) {
        if f.file_type == SftpFileType::Directory {
            return self.navigate(p, f.path.clone(), cx);
        }
        if p == SftpPane::Local && self.device.folder.is_some() {
            self.open_device_file(f, cx);
            return;
        }
        let result = self.fs(p).sftp_preview(&f.path, 0);
        self.preview_revision = result.as_ref().ok().map(|(rev, _)| *rev);
        let result = result.map(|(_, c)| c).map_err(|e| self.i18n.t(&e));
        self.state.update(cx, |s, cx| {
            let generation = s.begin_remote_preview(p, f, None, cx);
            s.apply_preview_loaded(generation, f.path.clone(), result, None, cx);
            cx.notify();
        });
    }
    fn transfer(&mut self, dir: TransferDirection, names: Vec<String>, cx: &mut Context<Self>) {
        if dir == TransferDirection::Download
            && self.is_tree(cx)
            && let Some(file) = self.state.read(cx).selected_tree_file()
        {
            self.transfer_entries(dir, [(file.name, Some(file.path))], cx);
        } else {
            self.transfer_entries(dir, names.into_iter().map(|n| (n, None)), cx);
        }
    }

    fn is_tree(&self, cx: &App) -> bool {
        self.state
            .read(cx)
            .tree_mode(self.sidebar.is_some_and(|s| s.tree))
    }

    fn transfer_entries(
        &mut self,
        dir: TransferDirection,
        entries: impl IntoIterator<Item = (String, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        if self.surface == SftpSurfaceId::Sidebar && dir == TransferDirection::Download {
            let base = &self.state.read(cx).remote_path;
            let paths: Vec<_> = entries
                .into_iter()
                .map(|(name, path)| path.unwrap_or_else(|| DemoFiles::path(base, &name)))
                .collect();
            if !paths.is_empty()
                && let Err(error) = self.download_from_sidebar(&paths)
            {
                self.notice(
                    self.i18n.t("sftp.toast.download_failed"),
                    Some(self.i18n.t(&error)),
                    TerminalNoticeVariant::Error,
                    cx,
                );
            }
            return;
        }
        self.queue_entries(dir, entries, cx);
    }

    fn transfer_notice(
        &self,
        dir: TransferDirection,
        completed: usize,
        failed: usize,
        skipped: usize,
        cx: &mut Context<Self>,
    ) {
        if completed + failed + skipped == 0 {
            return;
        }
        if completed + failed == 0 {
            let summary = self
                .i18n
                .t("sftp.toast.transferred_skipped")
                .replace("{{count}}", "0")
                .replace("{{skipped}}", &skipped.to_string());
            self.notice(summary, None, TerminalNoticeVariant::Default, cx);
            return;
        }
        use oxideterm_gpui_ui::toast::ToastVariant;
        let (variant, title, body) = match (dir, completed, failed) {
            (direction, _, 0) => (
                ToastVariant::Success,
                if direction == TransferDirection::Upload {
                    "sftp.toast.upload_complete"
                } else {
                    "sftp.toast.download_complete"
                },
                self.i18n
                    .t(if skipped == 0 {
                        "sftp.toast.transferred_count"
                    } else {
                        "sftp.toast.transferred_skipped"
                    })
                    .replace("{{count}}", &completed.to_string())
                    .replace("{{skipped}}", &skipped.to_string()),
            ),
            (direction, 0, _) => (
                ToastVariant::Error,
                if direction == TransferDirection::Upload {
                    "sftp.toast.upload_failed"
                } else {
                    "sftp.toast.download_failed"
                },
                self.i18n
                    .t("sftp.toast.failed_count")
                    .replace("{{count}}", &failed.to_string()),
            ),
            (direction, _, _) => (
                ToastVariant::Warning,
                if direction == TransferDirection::Upload {
                    "sftp.toast.upload_partial"
                } else {
                    "sftp.toast.download_partial"
                },
                self.i18n
                    .t("sftp.toast.partial_detail")
                    .replace("{{success}}", &completed.to_string())
                    .replace("{{failed}}", &failed.to_string())
                    .replace("{{skipped}}", &skipped.to_string()),
            ),
        };
        let target = self.notice_target(cx.entity_id());
        let _ = self.host.update(cx, |host, cx| {
            host.record_notice(
                oxideterm_notification_center::NotificationKind::Transfer,
                variant,
                self.i18n.t(title),
                Some(body),
                target,
                cx,
            )
        });
    }
    fn req(&mut self, r: SftpBrowserRequest, w: Option<&mut Window>, cx: &mut Context<Self>) {
        use SftpBrowserRequest::*;
        match r {
            BrowseUploads => self.choose_uploads(cx),
            BrowseFolder => self.pick_local_directory(cx),
            BrowserDrop(files) => {
                if !self.state.read(cx).dialog_is_open() {
                    self.dropped_uploads(files, cx);
                }
            }
            Focus | InputPressed(..) => {
                if let Some(w) = w {
                    w.focus(&self.focus, cx);
                }
            }
            CloseSidebar(_) => {
                let _ = self.host.update(cx, |h, cx| h.close_files_sidebar(cx));
            }
            TogglePin => {
                let _ = self
                    .host
                    .update(cx, |h, cx| h.pin_files_sidebar(&self.target.id, cx));
            }
            ToggleTreeMode => {
                let _ = self.host.update(cx, |h, cx| {
                    h.settings.update(cx, |s, cx| {
                        s.edit_preferences(cx, |p| {
                            use oxideterm_settings::SftpSidebarViewMode::{Flat, Tree};
                            p.sftp.sidebar_view_mode = if p.sftp.sidebar_view_mode == Tree {
                                Flat
                            } else {
                                Tree
                            };
                        })
                    })
                });
            }
            OpenTab(node, path) => {
                let _ = self
                    .host
                    .update(cx, |h, cx| h.open_node_files_tab(&node.0, Some(path), cx));
            }
            TreeDownload(file) => self.transfer_entries(
                TransferDirection::Download,
                [(file.name, Some(file.path))],
                cx,
            ),
            RevealTerminalDirectory => {
                let path = self
                    .host
                    .read_with(cx, |h, cx| {
                        if !h
                            .settings
                            .read(cx)
                            .settings(cx)
                            .terminal
                            .command_bar
                            .current_directory_awareness
                        {
                            return None;
                        }
                        let (id, path) = h.file_terminal(&self.target.id)?;
                        (Some(id) == self.directory_session).then(|| path.to_owned())
                    })
                    .ok()
                    .flatten();
                if let Some(path) = path {
                    self.navigate(SftpPane::Remote, path, cx);
                }
            }
            SyncTerminalDirectory(path) => {
                if let (Some(session), Some(w)) = (self.directory_session, w) {
                    let result = self.host.update(cx, |h, cx| {
                        h.change_file_terminal_directory(session, &self.target.id, &path, w, cx)
                    });
                    if let Ok(Err(key)) = result {
                        self.notice(self.i18n.t(key), None, TerminalNoticeVariant::Error, cx);
                    }
                }
            }
            LoadPairPrimary => self.reload(SftpPane::Local, cx),
            LoadRemote => {
                self.state.update(cx, |s, _| s.clear_sftp_tree_state());
                self.reload(SftpPane::Remote, cx);
            }
            Preview(p, f) => self.open(p, &f, cx),
            Transfer(p, d) => {
                let s = self.state.read(cx);
                let names = if p == SftpPane::Local {
                    &s.local_selected
                } else {
                    &s.remote_selected
                };
                self.transfer(d, names.iter().cloned().collect(), cx);
            }
            TransferFiles(_, d, n) => self.transfer(d, n, cx),
            SaveBookmark => {
                let s = self.state.read(cx);
                let paths = (s.local_path.clone(), s.remote_path.clone());
                let result = self.bookmarks.update(cx, |b, cx| {
                    b.save_paths(
                        self.bookmark_key(),
                        paths.0,
                        paths.1,
                        &self.i18n.t("sftp.bookmarks.default_name"),
                        cx,
                    )
                });
                match result {
                    Ok(true) => self.notice(
                        self.i18n.t("sftp.bookmarks.saved"),
                        None,
                        TerminalNoticeVariant::Success,
                        cx,
                    ),
                    Ok(false) => (),
                    Err(e) => self.notice(
                        self.i18n.t("sftp.bookmarks.save_failed"),
                        Some(e.to_string()),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
            RemoveBookmark(id) => {
                if let Err(error) = self.bookmarks.update(cx, |b, cx| b.remove(&id, cx)) {
                    self.notice(
                        self.i18n.t("sftp.bookmarks.save_failed"),
                        Some(error.to_string()),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                }
            }
            Extract(file) => {
                let Some(parent) = std::path::Path::new(&file.path)
                    .parent()
                    .and_then(std::path::Path::to_str)
                else {
                    return;
                };
                let result = self
                    .right
                    .files
                    .borrow_mut()
                    .extract_zip(&file.path, parent);
                match result {
                    Ok(()) => {
                        self.files_changed(SftpPane::Remote, cx);
                        self.notice(
                            self.i18n.t("sftp.toast.extract_complete"),
                            Some(file.name),
                            TerminalNoticeVariant::Success,
                            cx,
                        );
                    }
                    Err(error) => self.notice(
                        self.i18n.t("sftp.toast.extract_failed"),
                        Some(self.i18n.t(&error)),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
            _ => {}
        }
    }
    fn notice(
        &self,
        title: String,
        detail: Option<String>,
        variant: TerminalNoticeVariant,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_ui::toast::ToastVariant;
        let variant = match variant {
            TerminalNoticeVariant::Success => ToastVariant::Success,
            TerminalNoticeVariant::Error => ToastVariant::Error,
            TerminalNoticeVariant::Warning => ToastVariant::Warning,
            TerminalNoticeVariant::Default => ToastVariant::Default,
        };
        let target = self.notice_target(cx.entity_id());
        let _ = self.host.update(cx, |h, cx| {
            h.record_notice(
                oxideterm_notification_center::NotificationKind::Transfer,
                variant,
                title,
                detail,
                target,
                cx,
            )
        });
    }

    fn dialog(&mut self, r: SftpDialogRequest, cx: &mut Context<Self>) {
        match r {
            SftpDialogRequest::Accept
                if matches!(self.state.read(cx).dialog(), Some(SftpDialog::Conflict)) =>
            {
                self.resolve_conflict(SftpConflictResolution::Rename, cx);
                return;
            }
            SftpDialogRequest::Accept => {
                let s = self.state.read(cx);
                if self.device.folder.is_some()
                    && matches!(
                        s.dialog(),
                        Some(
                            SftpDialog::NewFolder {
                                pane: SftpPane::Local,
                                ..
                            } | SftpDialog::Rename {
                                pane: SftpPane::Local,
                                ..
                            }
                        )
                    )
                    && crate::browser_file::validate_filename(
                        s.input_value(SftpInput::DialogValue).trim(),
                    )
                    .is_err()
                {
                    self.notice(
                        self.i18n.t("ide.validation.nameInvalid"),
                        None,
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                    return;
                }
                let result = self
                    .state
                    .update(cx, |s, cx| s.submit_dialog(&self.i18n, cx));
                if let SftpDialogSubmission::Remote(m) = result {
                    if m.pane == SftpPane::Local && self.device.folder.is_some() {
                        self.mutate_device(m, cx);
                        return;
                    }
                    let fs = self.fs(m.pane);
                    let result = match m.operation() {
                        SftpFileOperation::NewFolder(p) => fs.create(p, true).map(|_| 0),
                        SftpFileOperation::Rename { from, to } => {
                            fs.duplicate(from, to, true).map(|_| 0)
                        }
                        SftpFileOperation::Delete(v) => v.iter().try_fold(0, |count, p| {
                            fs.remove_counted(p, true).map(|removed| count + removed)
                        }),
                    };
                    if result.is_ok() {
                        if m.clear_selection {
                            self.state.update(cx, |s, cx| {
                                s.clear_selection(m.pane);
                                cx.notify();
                            });
                        }
                        self.files_changed(m.pane, cx);
                    }
                    let _ = self.state.read(cx).worker_sender().send(m.complete(result));
                } else if let SftpDialogSubmission::LocalNotice(notice) = result {
                    if notice.variant == TerminalNoticeVariant::Success {
                        self.files_changed(SftpPane::Local, cx);
                    }
                    self.notice(notice.title, notice.description, notice.variant, cx);
                } else if matches!(result, SftpDialogSubmission::Wait) {
                    return;
                }
            }
            SftpDialogRequest::CompareDeviceFile(name) => {self.compare_device_file(&name,cx);return;}
            SftpDialogRequest::Download(name) => {
                let Some(path) = self.state.read(cx).preview_path.clone() else {
                    return;
                };
                if self.surface == SftpSurfaceId::Sidebar {
                    self.transfer_entries(TransferDirection::Download, [(name, Some(path))], cx);
                } else {
                    // The preview's button closes its dialog before any conflict prompt can take its place.
                    let view = cx.weak_entity();
                    cx.defer(move |cx| {
                        let _ = view.update(cx, |b, cx| {
                            b.transfer_entries(
                                TransferDirection::Download,
                                [(name, Some(path))],
                                cx,
                            )
                        });
                    });
                }
                return;
            }
            SftpDialogRequest::Resolve(resolution) => {
                self.resolve_conflict(resolution, cx);
                return;
            }
            SftpDialogRequest::CancelConflicts => {
                self.conflict_destination = None;
                self.state
                    .update(cx, |s, cx| s.cancel_transfer_conflicts(cx));
            }
            SftpDialogRequest::LoadHex {
                path,
                offset,
                generation,
            } => {
                let s = self.state.read(cx);
                if s.preview_generation != generation
                    || s.preview_path.as_deref() != Some(path.as_str())
                {
                    return;
                }
                let Some(pane) = s.preview_pane else {
                    return;
                };
                if pane == SftpPane::Local && self.device.folder.is_some() {
                    self.device_hex(path, offset, generation, cx);
                    return;
                }
                let result = self
                    .fs(pane)
                    .sftp_preview(&path, offset)
                    .and_then(|(rev, c)| {
                        if Some(rev) == self.preview_revision {
                            Ok(c)
                        } else {
                            Err(self
                                .i18n
                                .t("ide.conflict_desc")
                                .replace("{{fileName}}", &path))
                        }
                    });
                let _ = s.worker_sender().send(SftpWorkerResult::PreviewHexLoaded {
                    generation,
                    path,
                    error_prefix: self.i18n.t("sftp.toast.load_more_failed"),
                    result,
                });
                return;
            }
            SftpDialogRequest::Close => {
                self.conflict_destination = None;
            }
            _ => return,
        }
        self.state
            .update(cx, |s, cx| s.begin_dialog_exit(Duration::ZERO, cx));
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let action = self
            .host
            .read_with(cx, |host, cx| {
                SftpKeyAction::resolve(self.state.read(cx).dialog().as_ref(), |id| {
                    (id != "sftp.revealTerminalDirectory" || self.directory_session.is_some())
                        && host.matches_key(&e.keystroke, id, cx)
                })
            })
            .ok()
            .flatten();
        if self.handle_sftp_key(&e.keystroke, action, w, cx) {
            w.prevent_default();
            cx.stop_propagation();
        }
    }

    fn handle_sftp_key(
        &mut self,
        key: &Keystroke,
        action: Option<SftpKeyAction>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let tree = self.is_tree(cx);
        match self
            .state
            .update(cx, |s, cx| s.handle_key(key, action, tree, w, cx))
        {
            SftpKeyResult::Ignored => return false,
            SftpKeyResult::CommitPath(p) => {
                let path = self.state.read(cx).committed_path_input(p);
                if !path.is_empty() {
                    let load = self.state.update(cx, |s, cx| s.set_path(p, path, cx));
                    self.load_path(load, cx);
                }
            }
            SftpKeyResult::AcceptCompletion(p, index) => {
                if let Some(load) = self
                    .state
                    .update(cx, |s, cx| s.accept_completion(p, index, cx))
                {
                    self.load_path(load, cx);
                }
            }
            SftpKeyResult::RevealTerminalDirectory => {
                self.req(SftpBrowserRequest::RevealTerminalDirectory, Some(w), cx)
            }
            SftpKeyResult::OpenFile(p, f) => self.open(p, &f, cx),
            SftpKeyResult::Transfer(p, d) => {
                self.req(SftpBrowserRequest::Transfer(p, d), Some(w), cx)
            }
            SftpKeyResult::CloseDialog => self.dialog(SftpDialogRequest::Close, cx),
            SftpKeyResult::AcceptDialog => self.dialog(SftpDialogRequest::Accept, cx),
            _ => {}
        }
        true
    }

    fn load_path(&mut self, load: SftpPathLoad, cx: &mut Context<Self>) {
        match load {
            SftpPathLoad::Local => (),
            SftpPathLoad::PairPrimary => self.req(SftpBrowserRequest::LoadPairPrimary, None, cx),
            SftpPathLoad::Remote => self.req(SftpBrowserRequest::LoadRemote, None, cx),
        }
    }
}
impl Browser {
    fn input_frame(&self, input: SftpInput) -> SftpDialogIme {
        let bounds = self.input_bounds.clone();
        SftpDialogIme {
            field: self.fields.views.get(&input).cloned(),
            anchor_id: TextInputAnchorId(input.anchor_key()),
            viewport: Default::default(),
            caret_visible: true,
            selected_range: None,
            active_offset: None,
            marked_text: None,
            update_anchor: Rc::new(move |anchor| {
                bounds.borrow_mut().insert(input, anchor.bounds);
            }),
        }
    }

    pub(crate) fn mount_dialog(
        &self,
        editor: oxideterm_gpui_ide::IdeRuntimeSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.sidebar.is_some_and(|s| !s.has_target || s.active_tab) {
            return div().into_any_element();
        }
        let owner = cx.weak_entity();
        let dialog = SftpDialogFrame {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            markdown_options: Default::default(),
            markdown_actions: Default::default(),
            editor_preferences: SftpPreviewEditorPreferences {
                editor_font_family: editor.editor_font_family,
                editor_font_weight: editor.editor_font_weight,
                editor_font_fallback: editor.editor_font_fallback,
                editor_font_size: editor.editor_font_size,
                editor_line_height: editor.editor_line_height,
                word_wrap: editor.word_wrap,
                background_active: editor.background_active,
            },
            search_labels: oxideterm_gpui_ide::IdeLabels::localized(&self.i18n).editor_search,
            mono_font: "Lilex".into(),
            ui_font: self.ui_font.clone(),
            ime: self.input_frame(SftpInput::DialogValue),
            has_background: false,
            selectable: Rc::new(|_, s, c| div().child(s).text_color(rgb(c)).into_any_element()),
            dispatch: Rc::new(move |r, _, cx| {
                let _ = owner.update(cx, |b, cx| b.dialog(r, cx));
            }),
        };
        SftpDialogView::mount(&self.state, dialog, cx)
    }
}

impl Render for Browser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.bind_input_fields(window, cx);
        let owner = cx.weak_entity();
        let terminal = self
            .target
            .profile
            .is_none()
            .then(|| {
                self.host
                    .read_with(cx, |h, cx| {
                        let (session, path) = h.file_terminal(&self.target.id)?;
                        let aware = h
                            .settings
                            .read(cx)
                            .settings(cx)
                            .terminal
                            .command_bar
                            .current_directory_awareness;
                        Some((
                            session,
                            aware.then(|| path.to_string()),
                            h.shortcut("sftp.revealTerminalDirectory", cx),
                        ))
                    })
                    .ok()
                    .flatten()
            })
            .flatten();
        self.directory_session = terminal.as_ref().map(|t| t.0);
        let frame = SftpBrowserFrame {
            device_folder_picker: self.primary_title.is_none() && super::can_pick_directory(),
            sidebar: self.sidebar.is_some(),
            sidebar_target: self
                .sidebar
                .filter(|s| s.has_target)
                .map(|_| NodeId::new(&self.target.id)),
            sidebar_open: self.sidebar.map(|_| NodeId::new(&self.target.id)),
            active_sftp_tab: self.sidebar.is_none_or(|s| s.active_tab),
            embedded_sftp_pinned: self.sidebar.is_some_and(|s| s.pinned),
            has_background: false,
            remote_title: self.remote_title.clone(),
            pair_primary_title: self
                .device
                .folder
                .as_ref()
                .map(|f| f.name.clone())
                .or_else(|| self.primary_title.clone()),
            terminal_directory: terminal.as_ref().and_then(|t| t.1.clone()),
            reveal_terminal_key: terminal.as_ref().and_then(|t| t.2.clone()),
            can_sync_terminal: terminal.is_some(),
            tree_enabled: self.sidebar.is_some_and(|s| s.tree),
            bookmarks: self.bookmarks.clone(),
            bookmark_remote_key: Some(self.bookmark_key()),
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            focus_handle: self.focus.clone(),
            mono_font: "Lilex".into(),
            ime: [
                SftpInput::LocalPath,
                SftpInput::RemotePath,
                SftpInput::LocalFilter,
                SftpInput::RemoteFilter,
            ]
            .into_iter()
            .map(|input| (input, self.input_frame(input)))
            .collect(),
            completion_anchor: {
                let bounds = self.input_bounds.clone();
                Rc::new(move |input| bounds.borrow().get(&input).copied())
            },
            selectable: Rc::new(|_, _, _, _, text, color| {
                div().child(text).text_color(rgb(color)).into_any_element()
            }),
            dispatch: Rc::new(move |r, w, cx| {
                let _ = owner.update(cx, |b, cx| b.req(r, w, cx));
            }),
        };
        let dialog = self.sidebar.is_none().then(|| {
            let editor = self
                .host
                .read_with(cx, |h, cx| {
                    super::projects::runtime_settings(h.settings.read(cx).settings(cx))
                })
                .unwrap_or_default();
            self.mount_dialog(editor, cx)
        });
        div()
            .size_full()
            .when(self.sidebar.is_some(), |el| {
                el.flex().flex_col().min_h_0().overflow_hidden()
            })
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(SftpBrowserView::mount(&self.state, frame, cx))
            .children(dialog)
    }
}
