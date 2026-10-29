// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnyElement, AppContext, Context, Entity, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, StatefulInteractiveElement, Styled, Subscription,
    Window, div, px, rgb,
};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonSize, button_with},
    search_bar::{
        search_action_button, search_input_height, search_mode_button, search_mode_group,
        search_panel, search_row,
    },
    tooltip::tooltip_view,
};

use super::{EditorPresentation, TextEditorView};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSearchLabels {
    pub find_placeholder: String,
    pub insert_newline: String,
    pub replace_placeholder: String,
    pub find_next: String,
    pub find_previous: String,
    pub match_case: String,
    pub toggle_replace: String,
    pub close_search: String,
    pub replace_btn: String,
    pub replace_all_btn: String,
    pub use_regex: String,
    pub invalid_regex: String,
    pub no_results: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ClipboardItem, Modifiers, TestAppContext, size};

    fn labels() -> EditorSearchLabels {
        EditorSearchLabels {
            find_placeholder: "Find".into(),
            insert_newline: "Insert newline".into(),
            replace_placeholder: "Replace".into(),
            find_next: "Next".into(),
            find_previous: "Previous".into(),
            match_case: "Match case".into(),
            toggle_replace: "Replace".into(),
            close_search: "Close".into(),
            replace_btn: "Replace".into(),
            replace_all_btn: "All".into(),
            use_regex: "Use regular expression".into(),
            invalid_regex: "Invalid regular expression".into(),
            no_results: "No results".into(),
        }
    }

    #[gpui::test]
    fn search_regex_dot_requires_explicit_cross_line_mode(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|_, cx| {
            TextEditorView::new("foo\nbar", &oxideterm_theme::default_tokens(), cx)
        });
        editor.update(cx, |editor, cx| {
            editor.set_find_regex(true, cx);
            for (query, expected) in [
                (".*", vec![0..3, 4..7]),
                ("foo.bar", vec![]),
                (r"foo\nbar", vec![0..7]),
                ("(?s).*", vec![0..7]),
            ] {
                editor.set_find_query(query, cx);
                assert_eq!(
                    editor
                        .find_matches
                        .iter()
                        .map(|hit| hit.range.start.0..hit.range.end.0)
                        .collect::<Vec<_>>(),
                    expected,
                    "query={query}"
                );
            }
            editor.set_find_query(".*", cx);
            editor.replace_all_find_matches("x", cx);
            assert_eq!(editor.buffer.text(), "x\nx");
        });
    }

    #[gpui::test]
    fn search_regex_controls_match_across_lines_and_expand_replacements(cx: &mut TestAppContext) {
        let source = "Name: Alpha\nvalue=12\nName: alpha\nvalue=34\nName: beta\nvalue=56";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let mut editor = TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx);
            editor.set_search_labels(labels());
            window.focus(&editor.focus_handle, cx);
            editor
        });
        cx.simulate_resize(size(px(900.0), px(400.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.simulate_keystrokes(&format!("{modifier}-f"));
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(
                "Name: (Alpha)\nvalue=(\\d+)".into(),
            ))
        });
        cx.simulate_keystrokes(&format!("{modifier}-v"));
        editor.read_with(cx, |editor, _| assert!(editor.find_matches.is_empty()));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let regex = cx.debug_bounds("find-use-regex").unwrap();
        cx.simulate_click(regex.center(), Modifiers::default());
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                vec![0..20, 21..41]
            )
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let case = cx.debug_bounds("find-match-case").unwrap();
        cx.simulate_click(case.center(), Modifiers::default());
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                vec![0..20]
            )
        });
        editor.update(cx, |editor, cx| {
            editor.replace_current_find_match("<${1}:${2}>", cx);
            assert_eq!(
                editor.buffer.text(),
                "<Alpha:12>\nName: alpha\nvalue=34\nName: beta\nvalue=56"
            );
            editor.set_find_case_sensitive(false, cx);
            editor.replace_all_find_matches("${1}=${2}", cx);
            assert_eq!(
                editor.buffer.text(),
                "<Alpha:12>\nalpha=34\nName: beta\nvalue=56"
            );
            editor.set_find_query("[", cx);
            assert!(editor.find_regex_error);
            assert!(editor.find_matches.is_empty());
            editor.replace_all_find_matches("changed", cx);
            assert_eq!(
                editor.buffer.text(),
                "<Alpha:12>\nalpha=34\nName: beta\nvalue=56"
            );
            editor.set_find_query("^", cx);
            assert!(!editor.find_regex_error);
            assert!(editor.find_matches.is_empty());
            editor.set_find_query("alph", cx);
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                vec![1..5, 11..15]
            );
            let mut settings = editor.settings.clone();
            settings.find_whole_word = true;
            editor.set_settings(settings.clone(), cx);
            assert!(editor.find_matches.is_empty());
            settings.find_whole_word = false;
            editor.set_settings(settings, cx);
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                vec![1..5, 11..15]
            );
        });
    }

    #[gpui::test]
    fn search_inputs_preserve_multiline_edits_and_replace_the_document(cx: &mut TestAppContext) {
        let source = "α\nβ\nx\nα\nβ";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let mut editor = TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx);
            editor.set_search_labels(labels());
            window.focus(&editor.focus_handle, cx);
            editor
        });
        cx.simulate_resize(size(px(800.0), px(300.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        cx.simulate_keystrokes(&format!("{modifier}-f"));
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("α\r\nβ".into())));
        cx.simulate_keystrokes(&format!("{modifier}-v"));
        cx.simulate_input("!");
        editor.read_with(cx, |editor, _| assert_eq!(editor.find_query, "α\nβ!"));
        cx.simulate_keystrokes("backspace");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        editor.read_with(cx, |editor, cx| {
            assert_eq!(
                editor
                    .search_bar
                    .as_ref()
                    .unwrap()
                    .query
                    .read(cx)
                    .buffer
                    .text(),
                "α\nβ"
            );
            let input = editor.search_bar.as_ref().unwrap().query.read(cx);
            assert!(
                f32::from(input.content_bounds.unwrap().size.height) >= input.line_height() * 2.0
            );
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                vec![0..5, 8..13]
            );
            assert_eq!(editor.buffer.text(), source);
        });
        let input_bounds = cx.debug_bounds("editor-find-input").unwrap();
        for selector in ["find-match-case", "find-use-regex"] {
            let mode_bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                (f32::from(mode_bounds.center().y - input_bounds.center().y)).abs() < 1.0,
                "{selector} moved to another row"
            );
            assert!(mode_bounds.left() >= input_bounds.right());
        }
        cx.simulate_keystrokes("enter");
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.active_find_position(), Some((2, 2)))
        });
        cx.simulate_keystrokes(&format!("{modifier}-enter"));
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.find_query, "α\nβ\n");
            assert_eq!(editor.buffer.text(), source);
        });
        cx.simulate_keystrokes("backspace");
        let toggle = cx.debug_bounds("toggle-replace").unwrap();
        cx.simulate_click(toggle.center(), Modifiers::default());
        cx.simulate_keystrokes("tab");
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("新\n行".into())));
        cx.simulate_keystrokes(&format!("{modifier}-v"));
        let newline = cx.debug_bounds("replace-newline").unwrap();
        cx.simulate_click(newline.center(), Modifiers::default());
        editor.read_with(cx, |editor, cx| {
            assert_eq!(
                editor
                    .search_bar
                    .as_ref()
                    .unwrap()
                    .replacement
                    .read(cx)
                    .buffer
                    .text(),
                "新\n行\n"
            );
            assert_eq!(editor.buffer.text(), source);
        });
        cx.simulate_keystrokes("backspace");
        cx.simulate_resize(size(px(250.0), px(500.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        for replacement in [false, true] {
            editor.read_with(cx, |editor, cx| {
                let input = editor.search_input(replacement).unwrap();
                let bounds = input.read(cx).content_bounds.unwrap();
                assert!(bounds.size.width >= px(120.0), "search input was squeezed");
                assert!(bounds.left() >= px(0.0) && bounds.right() <= px(250.0));
            });
        }
        for selector in ["find-next", "find-close", "replace-current", "replace-all"] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.left() >= px(0.0) && bounds.right() <= px(250.0),
                "{selector}"
            );
        }
        let replace = cx.debug_bounds("replace-current").unwrap();
        cx.simulate_click(replace.center(), Modifiers::default());
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.buffer.text(), "新\n行\nx\nα\nβ")
        });
        let replace_all = cx.debug_bounds("replace-all").unwrap();
        cx.simulate_click(replace_all.center(), Modifiers::default());
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.buffer.text(), "新\n行\nx\n新\n行")
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.undo(cx);
                assert_eq!(editor.buffer.text(), "新\n行\nx\nα\nβ");
                editor.set_read_only(true);
                editor.replace_current_find_match("changed", cx);
                editor.replace_all_find_matches("changed", cx);
                assert_eq!(editor.buffer.text(), "新\n行\nx\nα\nβ");
                window.focus(&editor.focus_handle, cx);
            });
        });
        cx.simulate_keystrokes("escape");
        editor.read_with(cx, |editor, _| assert!(!editor.search_is_open()));
        cx.simulate_keystrokes(&format!("{modifier}-f"));
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("一\n二\n三\n四\n五\n六".into()))
        });
        cx.simulate_keystrokes(&format!("{modifier}-v"));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let query = editor.read(cx).search_bar.as_ref().unwrap().query.clone();
            query.update(cx, |query, _| {
                let bounds = query.content_bounds.unwrap();
                let caret =
                    query.bounds_for_byte_offset(query.cursor.selection().head, bounds, window);
                assert!(
                    caret.top() >= bounds.top() && caret.bottom() <= bounds.bottom(),
                    "multiline search caret is outside the input viewport"
                );
            });
        });
    }
}

pub(super) struct EditorSearchBar {
    query: Entity<TextEditorView>,
    replacement: Entity<TextEditorView>,
    query_version: u64,
    replacement_version: u64,
    replace_open: bool,
    focus_query: bool,
    _subscriptions: [Subscription; 2],
}

impl TextEditorView {
    pub fn set_search_labels(&mut self, labels: EditorSearchLabels) {
        self.search_labels = Some(labels);
    }

    pub fn search_is_open(&self) -> bool {
        self.search_bar.is_some()
    }

    pub(super) fn sync_search_input(&mut self, cx: &mut Context<Self>) {
        if let Some(bar) = self.search_bar.as_mut()
            && !bar
                .query
                .read(cx)
                .buffer
                .with_text(|text| text == self.find_query)
        {
            bar.query.update(cx, |input, cx| {
                input.replace_text_external(self.find_query.clone(), cx)
            });
            bar.query_version = bar.query.read(cx).buffer.version();
        }
    }

    pub(super) fn apply_search_theme(
        &self,
        tokens: &oxideterm_theme::ThemeTokens,
        cx: &mut Context<Self>,
    ) {
        if let Some(bar) = self.search_bar.as_ref() {
            for input in [&bar.query, &bar.replacement] {
                input.update(cx, |input, cx| {
                    input.apply_runtime_settings(
                        tokens,
                        tokens.metrics.font_family.to_string(),
                        tokens.metrics.ui_text_sm,
                        1.5,
                        false,
                        false,
                        cx,
                    )
                });
            }
        }
    }

    pub fn open_search(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(labels) = self.search_labels.as_ref() else {
            return false;
        };
        if let Some(bar) = self.search_bar.as_mut() {
            bar.focus_query = true;
            cx.notify();
            return true;
        }
        let selected = self.cursor.selection();
        let query_text = if selected.is_caret() {
            self.find_query.clone()
        } else {
            self.buffer.slice(selected.range()).unwrap_or_default()
        };
        let query = Self::new_search_input(
            &query_text,
            &self.language_notice_tokens,
            labels.find_placeholder.clone(),
            self.context_menu_labels.clone(),
            cx,
        );
        let replacement = Self::new_search_input(
            "",
            &self.language_notice_tokens,
            labels.replace_placeholder.clone(),
            self.context_menu_labels.clone(),
            cx,
        );
        let query_version = query.read(cx).buffer.version();
        let replacement_version = replacement.read(cx).buffer.version();
        // Only buffer changes trigger a search. Caret blinking and selection in
        // the inputs must not rescan the document or move the active match.
        let query_subscription = cx.observe(&query, |this, query, cx| {
            let version = query.read(cx).buffer.version();
            let Some(bar) = this.search_bar.as_mut() else {
                return;
            };
            if bar.query_version == version {
                return;
            }
            bar.query_version = version;
            let text = query.read(cx).buffer.text();
            this.set_find_query(text, cx);
        });
        let replacement_subscription = cx.observe(&replacement, |this, replacement, cx| {
            let version = replacement.read(cx).buffer.version();
            if let Some(bar) = this.search_bar.as_mut()
                && bar.replacement_version != version
            {
                bar.replacement_version = version;
                cx.notify();
            }
        });
        self.search_bar = Some(EditorSearchBar {
            query,
            replacement,
            query_version,
            replacement_version,
            replace_open: false,
            focus_query: true,
            _subscriptions: [query_subscription, replacement_subscription],
        });
        self.set_find_query(query_text, cx);
        true
    }

    pub fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.search_bar.take().is_none() {
            return false;
        }
        self.set_find_query("", cx);
        window.focus(&self.focus_handle, cx);
        true
    }

    pub fn new_search_input<T: 'static>(
        text: &str,
        tokens: &ThemeTokens,
        placeholder: String,
        context_labels: super::EditorContextMenuLabels,
        cx: &mut Context<T>,
    ) -> Entity<Self> {
        let tokens = *tokens;
        cx.new(|cx| {
            let mut input = Self::new(text, &tokens, cx);
            input.set_presentation(EditorPresentation::Inline, cx);
            input.apply_runtime_settings(
                &tokens,
                tokens.metrics.font_family.to_string(),
                tokens.metrics.ui_text_sm,
                1.5,
                false,
                false,
                cx,
            );
            input.settings.soft_wrap = false;
            input.settings.highlight_current_line = false;
            input.settings.highlight_special_chars = false;
            input.settings.indentation_markers = false;
            input.set_border_visible(false);
            input.set_placeholder(Some(placeholder), cx);
            input.set_context_menu_labels(context_labels);
            input
        })
    }

    fn search_input(&self, replacement: bool) -> Option<Entity<Self>> {
        let bar = self.search_bar.as_ref()?;
        Some(if replacement {
            bar.replacement.clone()
        } else {
            bar.query.clone()
        })
    }

    fn insert_search_newline(
        &mut self,
        replacement: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.search_input(replacement) {
            input.update(cx, |input, cx| input.insert_text("\n", cx));
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    fn handle_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bar) = self.search_bar.as_ref() else {
            return;
        };
        let query_focused = bar.query.read(cx).focus_handle.is_focused(window);
        let replacement_focused = bar.replacement.read(cx).focus_handle.is_focused(window);
        if !query_focused && !replacement_focused {
            return;
        }
        let shortcut = if let Some(bindings) = cx.try_global::<super::EditorKeybindings>() {
            bindings.resolve(&event.keystroke)
        } else if event.keystroke.modifiers.secondary()
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
        {
            match event.keystroke.key.as_str() {
                "f" => Some(super::EditorShortcut::Find),
                "s" => Some(super::EditorShortcut::Save),
                _ => None,
            }
        } else {
            None
        };
        if matches!(
            shortcut,
            Some(super::EditorShortcut::Find | super::EditorShortcut::Save)
        ) {
            if shortcut == Some(super::EditorShortcut::Save) {
                self.save(window, cx);
            } else {
                self.open_search(cx);
            }
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        let active = if replacement_focused {
            &bar.replacement
        } else {
            &bar.query
        };
        // Let the platform finish an IME candidate before Enter navigates matches.
        if active.read(cx).marked_text.is_some() && event.keystroke.key != "escape" {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_search(window, cx);
            }
            "enter" if event.keystroke.modifiers.secondary() => {
                self.insert_search_newline(replacement_focused, window, cx);
            }
            "enter" => {
                if event.keystroke.modifiers.shift {
                    self.select_previous_find_match(cx);
                } else {
                    self.select_next_find_match(cx);
                }
            }
            "tab" => {
                let focus = if bar.replace_open {
                    self.search_input(query_focused)
                        .unwrap()
                        .read(cx)
                        .focus_handle(cx)
                } else {
                    self.focus_handle.clone()
                };
                window.focus(&focus, cx);
                cx.notify();
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    fn search_icon_button(
        &self,
        id: &'static str,
        path: &'static str,
        label: String,
        disabled: bool,
    ) -> gpui::Stateful<gpui::Div> {
        search_action_button(&self.language_notice_tokens, id, path, label, disabled)
            .debug_selector(move || id.to_string())
    }

    fn render_search_input(
        &self,
        replacement: bool,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.search_input(replacement).unwrap();
        let height = search_input_height(
            input.read(cx).buffer.line_count() as usize,
            input.read(cx).line_height(),
        );
        let labels = self.search_labels.as_ref().unwrap();
        div()
            .id(if replacement {
                "editor-replace-input"
            } else {
                "editor-find-input"
            })
            .debug_selector(move || {
                if replacement {
                    "editor-replace-input"
                } else {
                    "editor-find-input"
                }
                .to_string()
            })
            .aria_label(if replacement {
                labels.replace_placeholder.clone()
            } else {
                labels.find_placeholder.clone()
            })
            .flex_1()
            .min_w(px(160.0))
            .max_w_full()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(div().flex_1().min_w_0().h(px(height)).child(input))
            .child(
                self.search_icon_button(
                    if replacement {
                        "replace-newline"
                    } else {
                        "find-newline"
                    },
                    "lucide/corner-down-left.svg",
                    format!(
                        "{} ({}+Enter)",
                        labels.insert_newline,
                        if cfg!(target_os = "macos") {
                            "⌘"
                        } else {
                            "Ctrl"
                        }
                    ),
                    false,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.insert_search_newline(replacement, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    pub(super) fn render_search_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bar = self.search_bar.as_mut().unwrap();
        if self.read_only && bar.replace_open {
            bar.replace_open = false;
            bar.focus_query = true;
        }
        if bar.focus_query {
            bar.focus_query = false;
            bar.query.update(cx, |input, cx| input.select_all(cx));
            window.focus(&bar.query.read(cx).focus_handle(cx), cx);
        }
        let replace_open = bar.replace_open;
        let tokens = self.language_notice_tokens;
        let labels = self.search_labels.as_ref().unwrap().clone();
        let match_count = self.find_matches.len();
        let current = self
            .active_find_position()
            .map_or(0, |(current, _)| current);
        let match_label = if self.find_regex_error {
            labels.invalid_regex.clone()
        } else if self.find_query.is_empty() || self.find_work.pending {
            String::new()
        } else if match_count == 0 {
            labels.no_results.clone()
        } else {
            format!("{current}/{match_count}")
        };
        let mut row = search_row()
            .child(
                self.search_icon_button(
                    "toggle-replace",
                    if replace_open {
                        "lucide/chevron-down.svg"
                    } else {
                        "lucide/chevron-right.svg"
                    },
                    labels.toggle_replace,
                    self.read_only,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        if !this.read_only
                            && let Some(bar) = this.search_bar.as_mut()
                        {
                            bar.replace_open = !bar.replace_open;
                            if !bar.replace_open {
                                window.focus(&bar.query.read(cx).focus_handle(cx), cx);
                            }
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(self.render_search_input(false, window, cx));
        row = row
            .child(
                search_mode_group()
                    .child(
                        search_mode_button(&tokens, "Aa", self.settings.find_case_sensitive)
                            .id("find-match-case")
                            .debug_selector(|| "find-match-case".into())
                            .aria_label(labels.match_case.clone())
                            .tooltip(move |_, cx| {
                                tooltip_view(tokens, labels.match_case.clone(), None, cx)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.set_find_case_sensitive(
                                        !this.settings.find_case_sensitive,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        search_mode_button(&tokens, ".*", self.find_regex)
                            .id("find-use-regex")
                            .debug_selector(|| "find-use-regex".into())
                            .aria_label(labels.use_regex.clone())
                            .tooltip(move |_, cx| {
                                tooltip_view(tokens, labels.use_regex.clone(), None, cx)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.set_find_regex(!this.find_regex, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .min_h(px(28.0))
                    .max_w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .child(match_label),
            )
            .child(
                self.search_icon_button(
                    "find-previous",
                    "lucide/arrow-up.svg",
                    labels.find_previous,
                    match_count == 0,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.select_previous_find_match(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                self.search_icon_button(
                    "find-next",
                    "lucide/arrow-down.svg",
                    labels.find_next,
                    match_count == 0,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.select_next_find_match(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                self.search_icon_button("find-close", "lucide/x.svg", labels.close_search, false)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.close_search(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        let mut panel = search_panel(&tokens)
            .id("editor-search-bar")
            .capture_key_down(cx.listener(Self::handle_search_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(row);
        if replace_open && !self.read_only {
            let mut row = search_row()
                .border_t_1()
                .border_color(rgb(tokens.ui.border))
                .child(div().w(px(28.0)).flex_none())
                .child(self.render_search_input(true, window, cx));
            for (all, label) in [(false, labels.replace_btn), (true, labels.replace_all_btn)] {
                row = row.child(
                    button_with(
                        &tokens,
                        label,
                        ButtonOptions {
                            size: ButtonSize::Sm,
                            disabled: match_count == 0,
                            ..Default::default()
                        },
                    )
                    .debug_selector(move || {
                        if all {
                            "replace-all"
                        } else {
                            "replace-current"
                        }
                        .to_string()
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if !this.read_only
                                && let Some(input) = this.search_input(true)
                            {
                                let replacement = input.read(cx).buffer.text();
                                if all {
                                    this.replace_all_find_matches(replacement, cx);
                                } else {
                                    this.replace_current_find_match(replacement, cx);
                                }
                            }
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
            panel = panel.child(row);
        }
        panel.into_any_element()
    }
}
