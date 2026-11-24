mod ide;
mod memory;
mod sftp;
use crate::{
    demo::{Files, LOCAL, REMOTE},
    host::Workspace,
};
use gpui::{prelude::*, *};
use memory::MemoryFs;
use oxideterm_gpui_file_manager::*;
use oxideterm_gpui_ide::{
    IdeLabels, IdeModalLayer, IdeRuntimeSettings, IdeSurface, IdeSurfaceMount,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabKind;
use std::{path::Path, rc::Rc};

pub struct FileViews {
    manager: FileManagerViews,
    ide: Entity<IdeSurface>,
    ide_modal: Entity<IdeModalLayer>,
    pub sftp: Entity<sftp::Browser>,
}
fn frame(t: ThemeTokens, i: &I18n) -> FileManagerEnvironment {
    FileManagerEnvironment {
        tokens: t,
        i18n: i.clone(),
        background: false,
        modal_background: false,
        chrome_divider: rgb(t.ui.border),
        ui_font_family: "IBM Plex Sans".into(),
        mono_font_family: "Lilex".into(),
        mono_font_size: 14.,
        editor: FileManagerEditorFonts {
            editor_font_family: "Lilex".into(),
            editor_font_weight: 400.,
            editor_font_fallback: None,
            editor_font_size: 14.,
            editor_line_height: 1.3,
            word_wrap: false,
            background_active: false,
        },
        markdown: Default::default(),
        caret_visible: true,
        ime: Default::default(),
        bookmarks_width: 180,
    }
}
impl FileViews {
    pub fn new(files: Files, t: ThemeTokens, i: &I18n, cx: &mut Context<Workspace>) -> Self {
        let fs = Rc::new(MemoryFs::new(files));
        let owner = cx.weak_entity();
        let read = owner.clone();
        let dispatch = owner.clone();
        let manager = FileManagerViews::with_bridge(
            Path::new(LOCAL),
            fs.clone(),
            Default::default(),
            FileManagerBindings {
                frame: Rc::new(move |cx| read.read_with(cx, |h, _| frame(h.tokens, &h.i18n)).ok()),
                dispatch: Rc::new(move |req, cx| {
                    let _ = dispatch.update(cx, |h, cx| match req {
                        FileManagerRequest::Notice { title, .. } => {
                            h.notifications_notice(title, cx)
                        }
                        FileManagerRequest::WindowInteractionChanged => cx.notify(),
                        _ => {}
                    });
                }),
                pointer: Rc::new(move |r, w, cx| {
                    let _ = owner.update(cx, |h, cx| match r {
                        FileManagerPointer::OpenTerminal(path) => {
                            h.new_terminal("local".into(), None, w, cx);
                            if let Some(id) = h.active_session() {
                                h.input(id, format!("cd {path}\r").as_bytes(), cx);
                            }
                        }
                        _ => w.focus(&h.focus, cx),
                    });
                }),
                anchor: Rc::new(|_| {}),
                selectable: Rc::new(|v, _| {
                    div()
                        .child(v.text)
                        .text_color(rgb(v.color))
                        .into_any_element()
                }),
                document: Rc::new(|_, _, _, _, _| None),
                mermaid_zoom: None,
            },
            frame(t, i),
            cx,
        );
        let ide = cx.new(|cx| {
            let mut v = IdeSurface::new(
                fs.clone(),
                t,
                IdeLabels::localized(i),
                Default::default(),
                cx,
            );
            v.set_mount(IdeSurfaceMount::MainWindow, cx);
            v.open_remote_project("demo", REMOTE, cx);
            v
        });
        let ide_modal = cx.new(|cx| IdeModalLayer::new(ide.clone(), cx));
        let host = cx.weak_entity();
        let sftp = cx.new(|cx| sftp::Browser::new(fs, t, i.clone(), host, cx));
        Self {
            manager,
            ide,
            ide_modal,
            sftp,
        }
    }
    pub fn page(&self, kind: &TabKind, _: &App) -> AnyElement {
        match kind {
            TabKind::FileManager => div()
                .relative()
                .size_full()
                .child(self.manager.body.clone())
                .child(self.manager.dialog.clone())
                .into_any_element(),
            TabKind::Sftp => self.sftp.clone().into_any_element(),
            TabKind::Ide => div()
                .relative()
                .size_full()
                .child(self.ide.clone())
                .child(self.ide_modal.clone())
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }
    pub fn manager_key(&self, e: &KeyDownEvent, cx: &mut App) -> bool {
        if self.manager.body.update(cx, |v, cx| {
            v.handle_file_manager_dialog_input_footer_key(e, cx)
                || v.handle_file_manager_key(e, None, None, cx)
        }) {
            return true;
        }
        self.manager.update(cx, |s, cx| {
            let Some(input) = s.focused_input() else {
                return false;
            };
            if e.keystroke.modifiers.platform || e.keystroke.modifiers.control {
                return false;
            }
            if e.keystroke.key == "backspace" {
                let text = s.input_value(input);
                let end = text.encode_utf16().count();
                let start = end - text.chars().last().map_or(0, char::len_utf16);
                return s.replace_input(input, Some(start..end), "", cx);
            }
            e.keystroke
                .key_char
                .as_deref()
                .is_some_and(|t| s.replace_input(input, None, t, cx))
        })
    }
    pub fn copied(&self, id: u64, ok: bool, cx: &mut App) {
        self.sftp.update(cx, |s, cx| s.completed(id, ok, cx));
        if ok {
            self.manager
                .body
                .update(cx, |s, cx| s.open_location(None, cx));
            self.ide.update(cx, |v, cx| v.refresh_project_tree_root(cx));
        }
    }
    pub fn refresh(&self, t: ThemeTokens, i: &I18n, cx: &mut App) {
        self.ide.update(cx, |v, cx| {
            v.set_visual_and_runtime_settings(t, IdeRuntimeSettings::default(), cx);
            v.set_labels(IdeLabels::localized(i), cx);
        });
        self.sftp.update(cx, |s, cx| {
            s.tokens = t;
            s.i18n = i.clone();
            cx.notify();
        });
        App::notify(cx, self.manager.body.entity_id());
        App::notify(cx, self.manager.dialog.entity_id());
    }
}
