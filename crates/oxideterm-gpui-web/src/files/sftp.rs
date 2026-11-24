use super::memory::MemoryFs;
use crate::{
    demo::{DemoFiles, LOCAL, REMOTE},
    host::Workspace,
    transport::GatewayRequest,
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
use std::{rc::Rc, sync::Arc, time::Duration};

pub struct Browser {
    fs: Rc<MemoryFs>,
    state: Entity<SftpWorkspaceEntity>,
    bookmarks: Entity<SftpBookmarks>,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    focus: FocusHandle,
    host: WeakEntity<Workspace>,
    next: u64,
}
impl Browser {
    pub fn new(
        fs: Rc<MemoryFs>,
        tokens: ThemeTokens,
        i18n: I18n,
        host: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|cx| {
            let mut s = SftpWorkspaceEntity::with_local_host(fs.clone(), cx);
            s.current_remote_id = Some(SftpRemoteId::Node(NodeId::new("demo")));
            s.apply_local_path(LOCAL.into())
                .expect("seeded local directory");
            s.apply_remote_path(REMOTE.into());
            s.remote_files = fs.sftp_list(REMOTE).expect("seeded remote directory");
            s.remote_loading = false;
            s
        });
        Self {
            state,
            fs,
            tokens,
            i18n,
            bookmarks: cx.new(|_| SftpBookmarks::in_memory()),
            focus: cx.focus_handle(),
            host,
            next: 0,
        }
    }
    fn navigate(&self, pane: SftpPane, path: String, cx: &mut Context<Self>) {
        if let Ok(list) = self.fs.sftp_list(&path) {
            self.state.update(cx, |s, cx| {
                if pane == SftpPane::Remote {
                    s.apply_remote_path(path);
                    s.remote_files = list;
                    s.remote_loading = false;
                    s.remote_load_pending = false;
                } else {
                    let _ = s.apply_local_path(path);
                }
                cx.notify();
            });
        }
    }
    fn open(&self, p: SftpPane, f: &SftpFileEntry, cx: &mut Context<Self>) {
        if f.file_type == SftpFileType::Directory {
            return self.navigate(p, f.path.clone(), cx);
        }
        self.state.update(cx, |s, cx| {
            s.preview_pane = Some(p);
            s.preview_path = Some(f.path.clone());
            let content = self
                .fs
                .bytes(&f.path)
                .and_then(|v| String::from_utf8(v).map_err(|_| "EILSEQ".into()));
            s.preview_error = content.as_ref().err().cloned();
            s.preview_content = content.ok().map(|data| {
                Arc::new(PreviewContent::Text {
                    data,
                    version: None,
                    mime_type: Some("text/plain".into()),
                    language: None,
                    encoding: "UTF-8".into(),
                    confidence: 1.,
                    has_bom: false,
                })
            });
            s.set_dialog(SftpDialog::Preview {
                name: f.name.clone(),
            });
            cx.notify();
        });
    }
    fn transfer(&mut self, dir: TransferDirection, names: Vec<String>, cx: &mut Context<Self>) {
        for name in names {
            let s = self.state.read(cx);
            let local = DemoFiles::path(&s.local_path, &name);
            let remote = DemoFiles::path(&s.remote_path, &name);
            let (from, to) = if dir == TransferDirection::Upload {
                (&local, &remote)
            } else {
                (&remote, &local)
            };
            let size = self
                .fs
                .files
                .borrow()
                .entries
                .get(from)
                .and_then(|e| e.data.as_ref())
                .map_or(0, |v| v.len() as u64);
            self.next += 1;
            let id = self.next;
            let request = GatewayRequest::Copy {
                transfer: id,
                node: "demo".into(),
                from: from.clone(),
                to: to.clone(),
            };
            self.state.update(cx, |s, cx| {
                s.transfers.push(SftpTransferItem {
                    id,
                    transfer_id: id.to_string(),
                    batch_id: None,
                    remote_id: SftpRemoteId::Node(NodeId::new("demo")),
                    name,
                    local_path: local,
                    remote_path: remote,
                    direction: dir,
                    protocol: TransferProtocol::Sftp,
                    size,
                    transferred: 0,
                    speed: 0,
                    state: SftpTransferState::Active,
                    error: None,
                });
                cx.notify();
            });
            let _ = self.host.update(cx, |h, cx| h.send(request, cx));
        }
    }
    pub fn completed(&self, id: u64, ok: bool, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            if let Some(t) = s.transfers.iter_mut().find(|t| t.id == id) {
                t.state = if ok {
                    SftpTransferState::Completed
                } else {
                    SftpTransferState::Error
                };
                if ok {
                    t.transferred = t.size;
                }
            }
            if let Ok(v) = self.fs.sftp_list(&s.local_path) {
                s.local_files = v;
            }
            if let Ok(v) = self.fs.sftp_list(&s.remote_path) {
                s.remote_files = v;
            }
            cx.notify();
        });
    }
    fn req(&mut self, r: SftpBrowserRequest, w: Option<&mut Window>, cx: &mut Context<Self>) {
        use SftpBrowserRequest::*;
        match r {
            Focus | InputPressed(..) => {
                if let Some(w) = w {
                    w.focus(&self.focus, cx);
                }
            }
            LoadRemote => self.navigate(
                SftpPane::Remote,
                self.state.read(cx).remote_path.clone(),
                cx,
            ),
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
            RemoveBookmark(id) => {
                self.bookmarks.update(cx, |b, cx| b.remove(&id, cx)).ok();
            }
            _ => {}
        }
    }
    fn dialog(&mut self, r: SftpDialogRequest, cx: &mut Context<Self>) {
        match r {
            SftpDialogRequest::Accept => {
                let result = self
                    .state
                    .update(cx, |s, cx| s.submit_dialog(&self.i18n, cx));
                if let SftpDialogSubmission::Remote(m) = result {
                    let result = match m.operation() {
                        SftpFileOperation::NewFolder(p) => self.fs.create(p, true),
                        SftpFileOperation::Rename { from, to } => self.fs.duplicate(from, to, true),
                        SftpFileOperation::Delete(v) => {
                            v.iter().try_for_each(|p| self.fs.remove(p, true))
                        }
                    };
                    if let Err(e) = result {
                        let _ = self.host.update(cx, |h, cx| h.notifications_notice(e, cx));
                    }
                    self.navigate(
                        SftpPane::Remote,
                        self.state.read(cx).remote_path.clone(),
                        cx,
                    );
                } else if matches!(result, SftpDialogSubmission::Wait) {
                    return;
                }
            }
            SftpDialogRequest::Download(name) => {
                self.transfer(TransferDirection::Download, vec![name], cx)
            }
            SftpDialogRequest::Close => {}
            _ => return,
        }
        self.state
            .update(cx, |s, cx| s.begin_dialog_exit(Duration::ZERO, cx));
    }
    fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        match self
            .state
            .update(cx, |s, cx| s.handle_key(&e.keystroke, None, false, w, cx))
        {
            SftpKeyResult::Ignored => {
                let edited = self.state.update(cx, |s, cx| {
                    let Some(input) = s.focused_input() else {
                        return false;
                    };
                    if e.keystroke.modifiers.platform || e.keystroke.modifiers.control {
                        return false;
                    }
                    let value = s.input_value_mut(input);
                    if e.keystroke.key == "backspace" {
                        value.pop();
                    } else if let Some(text) = &e.keystroke.key_char {
                        value.push_str(text);
                    } else {
                        return false;
                    }
                    cx.notify();
                    true
                });
                if !edited {
                    return;
                }
            }
            SftpKeyResult::CommitPath(p) => {
                let path = self.state.read(cx).committed_path_input(p);
                self.navigate(p, path, cx);
            }
            SftpKeyResult::OpenFile(p, f) => self.open(p, &f, cx),
            SftpKeyResult::Transfer(p, d) => {
                self.req(SftpBrowserRequest::Transfer(p, d), Some(w), cx)
            }
            SftpKeyResult::CloseDialog => self.dialog(SftpDialogRequest::Close, cx),
            SftpKeyResult::AcceptDialog => self.dialog(SftpDialogRequest::Accept, cx),
            _ => {}
        }
        w.prevent_default();
        cx.stop_propagation();
    }
}
impl Render for Browser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        let frame = SftpBrowserFrame {
            sidebar: false,
            sidebar_target: None,
            sidebar_open: None,
            active_sftp_tab: true,
            embedded_sftp_pinned: false,
            has_background: false,
            remote_title: "oxide.demo".into(),
            pair_primary_title: None,
            terminal_directory: None,
            reveal_terminal_key: None,
            can_sync_terminal: false,
            tree_enabled: false,
            bookmarks: self.bookmarks.clone(),
            bookmark_remote_key: Some("demo".into()),
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            focus_handle: self.focus.clone(),
            mono_font: "Lilex".into(),
            ime: Default::default(),
            completion_anchor: Rc::new(|_| None),
            selectable: Rc::new(|_, _, _, _, text, color| {
                div().child(text).text_color(rgb(color)).into_any_element()
            }),
            dispatch: Rc::new(move |r, w, cx| {
                let _ = owner.update(cx, |b, cx| b.req(r, w, cx));
            }),
        };
        let owner = cx.weak_entity();
        let dialog = SftpDialogFrame {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            markdown_options: Default::default(),
            markdown_actions: Default::default(),
            editor_preferences: SftpPreviewEditorPreferences {
                editor_font_family: "Lilex".into(),
                editor_font_weight: 400.,
                editor_font_fallback: None,
                editor_font_size: 14.,
                editor_line_height: 1.3,
                word_wrap: false,
                background_active: false,
            },
            search_labels: oxideterm_gpui_ide::IdeLabels::localized(&self.i18n).editor_search,
            mono_font: "Lilex".into(),
            ui_font: "IBM Plex Sans".into(),
            ime: SftpDialogIme {
                anchor_id: TextInputAnchorId(5),
                viewport: Default::default(),
                caret_visible: true,
                selected_range: None,
                active_offset: None,
                marked_text: None,
                update_anchor: Rc::new(|_| {}),
            },
            has_background: false,
            selectable: Rc::new(|_, s, c| div().child(s).text_color(rgb(c)).into_any_element()),
            dispatch: Rc::new(move |r, _, cx| {
                let _ = owner.update(cx, |b, cx| b.dialog(r, cx));
            }),
        };
        div()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(SftpBrowserView::mount(&self.state, frame, cx))
            .child(SftpDialogView::mount(&self.state, dialog, cx))
    }
}
