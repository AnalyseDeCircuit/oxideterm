use gpui::{prelude::*, *};
use oxideterm_gpui_editor::{EditorContextMenuLabels, TextEditorView};
use oxideterm_gpui_ui::search_bar::*;
use oxideterm_i18n::I18n;
use oxideterm_terminal_emulator::{
    TerminalSearchMatch,
    search::scrollback::{TerminalSearchOptions, TerminalSearchQuery},
};
use std::{sync::Arc, time::Duration};

use super::TerminalView;

pub(super) struct PaneSearch {
    editor: Entity<TextEditorView>,
    _subscription: Subscription,
    version: u64,
    i18n: I18n,
    options: TerminalSearchOptions,
    query: Option<TerminalSearchQuery>,
    invalid: bool,
    visible: bool,
    refresh_pending: bool,
    task: Option<Task<()>>,
    pub(super) matches: Arc<[TerminalSearchMatch]>,
    pub(super) selected: Option<usize>,
}

impl TerminalView {
    pub fn open_search(&mut self, i18n: &I18n, w: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_none() {
            let editor = TextEditorView::new_search_input(
                "",
                &self.theme.tokens,
                i18n.t("search.placeholder"),
                EditorContextMenuLabels {
                    copy: i18n.t("menu.copy"),
                    cut: i18n.t("fileManager.cut"),
                    paste: i18n.t("menu.paste"),
                    select_all: i18n.t("fileManager.selectAll"),
                },
                cx,
            );
            let subscription = cx.observe(&editor, |view, editor, cx| {
                let Some(s) = &mut view.search else { return };
                let revision = editor.read(cx).buffer().version();
                if s.visible && s.version != revision {
                    s.version = revision;
                    view.change_search(cx);
                }
            });
            self.search = Some(PaneSearch {
                version: editor.read(cx).buffer().version(),
                editor,
                _subscription: subscription,
                i18n: i18n.clone(),
                options: Default::default(),
                query: None,
                visible: false,
                invalid: false,
                refresh_pending: false,
                task: None,
                matches: Arc::default(),
                selected: None,
            });
        }
        let s = self.search.as_mut().unwrap();
        s.i18n = i18n.clone();
        w.focus(&s.editor.read(cx).focus_handle(cx), cx);
        s.editor.update(cx, |editor, cx| editor.select_all(cx));
        if !s.visible {
            s.visible = true;
            self.change_search(cx);
        }
    }

    pub fn search_focused(&self, w: &Window, cx: &App) -> bool {
        self.search
            .as_ref()
            .is_some_and(|s| s.visible && s.editor.read(cx).focus_handle(cx).is_focused(w))
    }

    fn close_search(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = &mut self.search {
            s.visible = false;
            s.task = None;
            s.query = None;
            s.matches = Arc::default();
            s.selected = None;
        }
        w.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn refresh_search_style(&self, cx: &mut Context<Self>) {
        if let Some(s) = &self.search {
            let t = &self.theme.tokens;
            s.editor.update(cx, |editor, cx| {
                editor.apply_runtime_settings(
                    t,
                    t.metrics.font_family.into(),
                    t.metrics.ui_text_sm,
                    1.5,
                    false,
                    false,
                    cx,
                )
            });
        }
    }

    fn change_search(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &mut self.search else { return };
        let text = s.editor.read(cx).buffer().text();
        s.task = None;
        s.matches = Arc::default();
        s.selected = None;
        s.refresh_pending = false;
        let compiled = (!text.is_empty())
            .then(|| TerminalSearchQuery::new(&text, s.options))
            .transpose();
        s.invalid = compiled.is_err();
        s.query = compiled.ok().flatten();
        self.refresh_search(cx);
        cx.notify();
    }

    pub(super) fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &mut self.search else { return };
        let Some(query) = s.query.as_ref() else {
            return;
        };
        if s.task.is_some() {
            s.refresh_pending = true;
            return;
        }
        let query = query.clone();
        s.task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(45))
                .await;
            let Ok(mut scan) = view.update(cx, |v, _| {
                v.search.as_mut().unwrap().refresh_pending = false;
                v.emulator.borrow().begin_search(&query)
            }) else {
                return;
            };
            loop {
                let Ok(done) = view.read_with(cx, |v, _| {
                    let emu = v.emulator.borrow();
                    let start = Instant::now();
                    loop {
                        if emu.advance_search(&mut scan) {
                            break true;
                        }
                        if start.elapsed() >= Duration::from_millis(3) {
                            break false;
                        }
                    }
                }) else {
                    return;
                };
                if done {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(1))
                    .await;
            }
            let _ = view.update(cx, |v, cx| {
                let matches = v.emulator.borrow().finish_search(scan);
                let s = v.search.as_mut().unwrap();
                let first = s.selected.is_none();
                s.selected =
                    (!matches.is_empty()).then(|| s.selected.unwrap_or(0).min(matches.len() - 1));
                s.matches = matches.into();
                s.task = None;
                let pending = s.refresh_pending;
                if first {
                    v.reveal_search_hit(cx);
                }
                if pending {
                    v.refresh_search(cx);
                }
                cx.notify();
            });
        }));
    }

    fn reveal_search_hit(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &self.search else { return };
        let Some(hit) = s.selected.and_then(|i| s.matches.get(i)) else {
            return;
        };
        let top = -(self.snapshot.display_offset as i32);
        let bottom = top + self.size.rows as i32 - 1;
        let last = hit.ranges.last().map_or(hit.line, |r| r.line);
        let delta = if hit.line < top {
            top - hit.line
        } else if last > bottom {
            (bottom - last).max(top - hit.line)
        } else {
            0
        };
        if delta != 0 {
            self.scroll_grid(delta, cx);
        }
    }

    fn navigate_search(&mut self, next: bool, cx: &mut Context<Self>) {
        let Some(s) = &mut self.search else { return };
        let n = s.matches.len();
        let Some(i) = s.selected.filter(|_| n > 0) else {
            return;
        };
        s.selected = Some(if next { (i + 1) % n } else { (i + n - 1) % n });
        self.reveal_search_hit(cx);
        cx.notify();
    }

    pub(super) fn search_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.search.as_ref().filter(|s| s.visible)?;
        let t = &self.theme.tokens;
        let mut modes = search_mode_group();
        for (regex, label, key, active) in [
            (false, "Aa", "search.match_case", s.options.case_sensitive),
            (true, ".*", "search.use_regex", s.options.regex),
        ] {
            let name = s.i18n.t(key);
            let tokens = *t;
            modes = modes.child(
                search_mode_button(t, label, active)
                    .id(label)
                    .aria_label(name.clone())
                    .tooltip(move |_, cx| {
                        oxideterm_gpui_ui::tooltip::tooltip_view(tokens, name.clone(), None, cx)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |v, _, w, cx| {
                            let s = v.search.as_mut().unwrap();
                            if regex {
                                s.options.regex = !s.options.regex;
                            } else {
                                s.options.case_sensitive = !s.options.case_sensitive;
                            }
                            w.focus(&s.editor.read(cx).focus_handle(cx), cx);
                            v.change_search(cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        let status = if s.invalid {
            s.i18n.t("search.invalid_regex")
        } else if let Some(i) = s.selected {
            format!("{}/{}", i + 1, s.matches.len())
        } else if s.query.is_some() && s.task.is_none() {
            s.i18n.t("search.no_results")
        } else {
            String::new()
        };
        let editor = s.editor.read(cx);
        let mut row = search_row()
            .child(
                svg()
                    .path("lucide/search.svg")
                    .size(px(15.))
                    .text_color(rgb(t.ui.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(100.))
                    .h(px(search_input_height(
                        editor.buffer().line_count() as usize,
                        editor.line_height(),
                    )))
                    .child(s.editor.clone()),
            )
            .child(modes)
            .child(div().text_color(rgb(t.ui.text_muted)).child(status));
        for (next, icon, key) in [
            (false, "lucide/arrow-up.svg", "ide.find_previous"),
            (true, "lucide/arrow-down.svg", "ide.find_next"),
        ] {
            row = row.child(
                search_action_button(t, key, icon, s.i18n.t(key), s.matches.is_empty())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |v, _, w, cx| {
                            w.focus(
                                &v.search.as_ref().unwrap().editor.read(cx).focus_handle(cx),
                                cx,
                            );
                            v.navigate_search(next, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        Some(
            search_panel(t)
                .id("terminal-search")
                .capture_key_down(cx.listener(|v, e: &KeyDownEvent, w, cx| {
                    let editor = &v.search.as_ref().unwrap().editor;
                    if editor.update(cx, |ed, cx| ed.marked_text_range(w, cx).is_some()) {
                        return;
                    }
                    match e.keystroke.key.as_str() {
                        "escape" => v.close_search(w, cx),
                        "enter" => v.navigate_search(!e.keystroke.modifiers.shift, cx),
                        "tab" => w.focus(&v.focus, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                    w.prevent_default();
                }))
                .child(
                    row.child(
                        search_action_button(
                            t,
                            "terminal-search-close",
                            "lucide/x.svg",
                            s.i18n.t("ide.close_search"),
                            false,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _, w, cx| {
                                v.close_search(w, cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
                )
                .child(
                    div()
                        .px(px(12.))
                        .py(px(7.))
                        .text_size(px(11.))
                        .text_color(rgb(t.ui.text_muted))
                        .child(s.i18n.t("search.visible_terminal_hint")),
                )
                .into_any_element(),
        )
    }
}
