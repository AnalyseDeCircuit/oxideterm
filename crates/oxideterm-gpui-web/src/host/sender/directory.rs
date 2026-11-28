use super::*;
use oxideterm_environment::{CurrentDirectoryEntry, current_directory_shell_path_argument};
use oxideterm_gpui_command_sender::cwd::{self, Entry, Kind};
use oxideterm_gpui_ui::{
    CommandPanelOptions, EntityListRowOptions,
    button::{ContextChipOptions, context_chip},
    command_panel, entity_list_row,
};

#[derive(Clone, PartialEq)]
pub(super) struct Directory {
    session: TerminalSessionId,
    node: String,
    path: String,
}

pub(super) struct Picker {
    path: String,
    entries: Vec<CurrentDirectoryEntry>,
    rows: Vec<Entry>,
    selected: usize,
    pub(super) search: Entity<LineField>,
    scroll: ScrollHandle,
    _events: Subscription,
}

#[derive(Clone, Copy)]
enum Action {
    Switch,
    Copy,
    Files,
    Ide,
}

impl Workspace {
    fn sender_directory(&self, cx: &App) -> Option<Directory> {
        let bar = &self.settings.read(cx).settings(cx).terminal.command_bar;
        if !(bar.enabled && bar.current_directory_awareness && bar.show_current_directory) {
            return None;
        }
        let id = self.active_session()?;
        let session = self.sessions.get(&id)?;
        if session.is_ble() || !session.writable(self.ready) {
            return None;
        }
        Some(Directory {
            session: id,
            node: session.node.clone(),
            path: self.transport.as_ref()?.directory(id.0)?.into(),
        })
    }

    pub(in crate::host) fn sync_sender_directory(&self, cx: &mut App) {
        let current = self.sender_directory(cx);
        if self.sender.read(cx).directory != current {
            self.sender.update(cx, |s, cx| {
                s.directory = current;
                s.picker = None;
                cx.notify();
            });
        }
    }
}

impl WebSender {
    fn current_directory(&self, cx: &App) -> Option<&Directory> {
        let origin = self.directory.as_ref()?;
        self.host
            .read_with(cx, |h, cx| h.sender_directory(cx).as_ref() == Some(origin))
            .unwrap_or(false)
            .then_some(origin)
    }

    fn browse(&mut self, path: String, w: &mut Window, cx: &mut Context<Self>) {
        let Some(origin) = self.current_directory(cx) else {
            return;
        };
        let Ok(Some((tokens, label, entries))) = self.host.read_with(cx, |h, _| {
            Some((
                h.tokens,
                h.i18n.t("terminal.cwd.search_directories"),
                h.files.as_ref()?.cwd_entries(&origin.node, &path)?,
            ))
        }) else {
            let _ = self
                .host
                .update(cx, |h, cx| h.notice("terminal.cwd.unavailable", cx));
            return;
        };
        let search = cx.new(|cx| LineField::new(tokens, label, false, cx));
        let events = cx.subscribe_in(&search, w, |s, field, event, w, cx| {
            let Some(p) = &mut s.picker else { return };
            if field != &p.search {
                return;
            }
            match event {
                FieldEvent::Changed => {
                    let query = field.read(cx).text();
                    p.rows = cwd::choices(&p.path, &p.entries, query);
                    p.selected = if query.trim().is_empty() {
                        0
                    } else {
                        p.rows
                            .iter()
                            .position(|r| r.kind != Kind::Parent)
                            .unwrap_or(0)
                    };
                    p.scroll.scroll_to_item(p.selected);
                    cx.notify();
                }
                FieldEvent::Submit => s.accept_directory(w, cx),
                FieldEvent::Cancel => {
                    s.picker = None;
                    s.focus_draft(w, cx);
                    cx.notify();
                }
            }
        });
        w.focus(&search.focus_handle(cx), cx);
        self.picker = Some(Picker {
            rows: cwd::choices(&path, &entries, ""),
            path,
            entries,
            selected: 0,
            search,
            scroll: ScrollHandle::new(),
            _events: events,
        });
        cx.notify();
    }

    fn accept_directory(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.current_directory(cx).is_none() {
            return;
        }
        let Some(p) = &self.picker else { return };
        let Some(row) = p.rows.get(p.selected) else {
            return;
        };
        if row.kind == Kind::File {
            if let Some(arg) = current_directory_shell_path_argument(&row.path) {
                self.model
                    .update(cx, |m, cx| m.append_active_text(&arg, true, cx));
                self.picker = None;
                self.focus_draft(w, cx);
                cx.notify();
            }
        } else {
            let path = if row.kind == Kind::TypedPath {
                match row.path.strip_prefix('~') {
                    Some("") => LOCAL.into(),
                    Some(tail) if tail.starts_with('/') => DemoFiles::path(LOCAL, &tail[1..]),
                    _ => DemoFiles::path(&p.path, &row.path),
                }
            } else {
                row.path.clone()
            };
            self.directory_action(Action::Switch, path, w, cx);
        }
    }

    fn directory_action(
        &mut self,
        action: Action,
        path: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(origin) = self.current_directory(cx).cloned() else {
            return;
        };
        let Ok(done) = self.host.update(cx, |h, cx| {
            match action {
                Action::Switch => {
                    if let Err(key) =
                        h.change_file_terminal_directory(origin.session, &origin.node, &path, w, cx)
                    {
                        h.notice(key, cx);
                        return false;
                    }
                }
                Action::Copy => cx.write_to_clipboard(ClipboardItem::new_string(path)),
                Action::Files if origin.node == "local" => h.open_file_manager_at(Some(path), cx),
                Action::Files => h.present_files_at(&origin.node, Some(path), cx),
                Action::Ide => h.open_node_ide_at(&origin.node, Some(&path), w, cx),
            }
            true
        }) else {
            return;
        };
        if done {
            self.picker = None;
            if matches!(action, Action::Copy) {
                self.focus_draft(w, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn directory_chip(&self, t: &ThemeTokens, cx: &Context<Self>) -> Option<AnyElement> {
        let current = self.directory.as_ref()?;
        let open = self.picker.is_some();
        let chip = context_chip(
            t,
            ContextChipOptions::new()
                .max_width(260.)
                .radius(ButtonRadius::Sm)
                .border_color(rgba(0x00000000))
                .background_color(rgba(0x00000000))
                .text_color(rgb(if open { t.ui.accent } else { t.ui.text }))
                .hover_background_color(rgb(t.ui.bg_hover)),
            Some(icon(LucideIcon::Folder, t)),
            div()
                .truncate()
                .child(cwd::chip_label(&current.path))
                .into_any_element(),
            vec![],
        );
        Some(
            chip.id("sender-cwd")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |s, _, w, cx| {
                        if open {
                            s.picker = None;
                            s.focus_draft(w, cx);
                            cx.notify();
                        } else if let Some(origin) = s.current_directory(cx) {
                            s.browse(origin.path.clone(), w, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    pub(super) fn directory_panel(
        &self,
        t: &ThemeTokens,
        i: &I18n,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let p = self.picker.as_ref()?;
        let remote = self.directory.as_ref()?.node != "local";
        let mut header = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().min_w_0().flex_1().truncate().child(p.path.clone()));
        for (action, glyph, key) in [
            (
                Action::Switch,
                LucideIcon::Check,
                "terminal.cwd.switch_to_directory",
            ),
            (Action::Copy, LucideIcon::Copy, "terminal.cwd.copy_path"),
            if remote {
                (Action::Files, LucideIcon::Cloud, "terminal.cwd.open_sftp")
            } else {
                (
                    Action::Files,
                    LucideIcon::FolderOpen,
                    "terminal.cwd.open_file_manager",
                )
            },
            (Action::Ide, LucideIcon::FileCode, "terminal.cwd.open_ide"),
        ] {
            if !remote && matches!(action, Action::Ide) {
                continue;
            }
            let path = p.path.clone();
            header = header.child(icon_button_with_tooltip(
                t,
                glyph.path(),
                14.,
                rgb(t.ui.text_muted),
                IconButtonOptions::opaque_toolbar(24., ButtonRadius::Sm),
                i.t(key),
                key,
                true,
                cx.listener(move |s, _, w, cx| {
                    s.directory_action(action, path.clone(), w, cx);
                    cx.stop_propagation();
                }),
            ));
        }
        let mut list = div()
            .id("cwd-choices")
            .overflow_y_scroll()
            .track_scroll(&p.scroll)
            .max_h(px(420.));
        for (ix, row) in p.rows.iter().enumerate() {
            let (glyph, label) = match row.kind {
                Kind::Parent => (LucideIcon::ArrowUp, i.t("terminal.cwd.parent_directory")),
                Kind::Directory => (LucideIcon::Folder, row.name.clone()),
                Kind::File => (LucideIcon::FileText, row.name.clone()),
                Kind::TypedPath => (LucideIcon::CornerDownLeft, i.t("terminal.cwd.go_to_path")),
            };
            let mut trailing = Vec::new();
            if matches!(row.kind, Kind::Directory | Kind::Parent) {
                let path = row.path.clone();
                trailing.push(
                    div()
                        .id(("cwd-browse", ix))
                        .cursor_pointer()
                        .p(px(5.))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |s, _, w, cx| {
                                s.browse(path.clone(), w, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(icon(LucideIcon::ChevronRight, t))
                        .into_any_element(),
                );
            }
            list = list.child(
                entity_list_row(
                    t,
                    EntityListRowOptions::new()
                        .compact()
                        .active(ix == p.selected),
                    Some(icon(glyph, t)),
                    div().truncate().child(label).into_any_element(),
                    Some(
                        div()
                            .truncate()
                            .text_size(px(10.))
                            .text_color(rgb(t.ui.text_muted))
                            .child(row.path.clone())
                            .into_any_element(),
                    ),
                    vec![],
                    trailing,
                )
                .id(("cwd-entry", ix))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |s, e: &MouseDownEvent, w, cx| {
                        if let Some(p) = &mut s.picker {
                            p.selected = ix;
                            w.focus(&p.search.focus_handle(cx), cx);
                            if e.click_count >= 2 {
                                s.accept_directory(w, cx);
                            }
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        if p.rows.is_empty() {
            list = list.child(i.t("terminal.cwd.no_directories"));
        }
        Some(
            command_panel(
                t,
                CommandPanelOptions::new()
                    .width(520.)
                    .max_width_ratio(0.96)
                    .terminal_owned(),
            )
            .id("cwd-picker")
            .absolute()
            .bottom(px(oxideterm_gpui_ui::WORKSPACE_STATUS_BAR_HEIGHT))
            .left(px(4.))
            .occlude()
            .text_size(px(12.))
            .child(header)
            .child(p.search.clone())
            .child(list)
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.picker = None;
                cx.notify();
            }))
            .on_key_down(cx.listener(|s, e: &KeyDownEvent, w, cx| {
                let Some(p) = &mut s.picker else { return };
                match e.keystroke.key.as_str() {
                    "up" => p.selected = p.selected.saturating_sub(1),
                    "down" => p.selected = (p.selected + 1).min(p.rows.len().saturating_sub(1)),
                    _ => return,
                }
                p.scroll.scroll_to_item(p.selected);
                w.prevent_default();
                cx.stop_propagation();
                cx.notify();
            }))
            .into_any_element(),
        )
    }
}

fn icon(glyph: LucideIcon, t: &ThemeTokens) -> AnyElement {
    svg()
        .path(glyph.path())
        .size(px(13.))
        .text_color(rgb(t.ui.text_muted))
        .into_any_element()
}
