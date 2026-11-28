// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// Hallmark · pre-emit critique: P4 H5 E4 S5 R5 V4
// Hallmark · macrostructure: Workbench · genre: modern-minimal · tone: technical and restrained

use super::*;
use crate::workspace::terminal_command_sender::{
    CompactSuggestionMode, TERMINAL_SENDER_MIN_HEIGHT, TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT,
    TerminalCommandSenderDocumentSnapshot, TerminalCommandSenderId,
    TerminalCommandSenderStatus, TerminalCommandSenderTarget, TerminalCommandSenderTargetScope,
};
use oxideterm_editor_core::utf16::{byte_index_for_utf16, replace_utf16};
use oxideterm_terminal::TerminalSenderInputMode;
use zeroize::Zeroizing;

const TERMINAL_SENDER_COMPACT_HORIZONTAL_PADDING: f32 = 12.0;
const TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT: f32 = 24.0;
const TERMINAL_SENDER_COMPACT_BACKGROUND_ALPHA: u32 = 0xf2;
const TERMINAL_SENDER_COMPACT_BORDER_ALPHA: u32 = 0x73;
const QUICK_BAR_COMMAND_MAX_WIDTH: f32 = 220.0;
const QUICK_BAR_PREVIEW_MAX_CHARS: usize = 600;
const QUICK_BAR_PREVIEW_MAX_LINES: usize = 8;
const QUICK_BAR_PREVIEW_MAX_WIDTH: f32 = 480.0;

struct QuickCommandPreview {
    tokens: ThemeTokens,
    text: Zeroizing<String>,
}

impl Render for QuickCommandPreview {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Keep command text out of tooltip animation identities and diagnostic state.
        oxideterm_gpui_ui::tooltip::tooltip_surface(&self.tokens, self.text.to_string(), None)
            .max_w(px(
                QUICK_BAR_PREVIEW_MAX_WIDTH.min(f32::from(window.viewport_size().width))
            ))
    }
}

fn quick_command_preview(name: &str, command: &str) -> Zeroizing<String> {
    let mut preview = Zeroizing::new(format!("{name}\n"));
    let mut lines = 1;
    for (index, ch) in command.chars().enumerate() {
        if index == QUICK_BAR_PREVIEW_MAX_CHARS
            || (ch == '\n' && lines == QUICK_BAR_PREVIEW_MAX_LINES)
        {
            preview.push('…');
            break;
        }
        preview.push(ch);
        if ch == '\n' {
            lines += 1;
        }
    }
    preview
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_terminal_command_sender_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sender = self.terminal_command_sender.read(cx);
        let target_height = sender.panel_height();
        let mut tokens = self.tokens;
        if sender.is_resizing() {
            tokens.motion.spatial_enabled = false;
        }
        let allow_overflow = target_height
            >= if sender.is_expanded() {
                TERMINAL_SENDER_MIN_HEIGHT
            } else {
                TERMINAL_SENDER_COMPACT_HEIGHT
            };
        let reveal = oxideterm_gpui_ui::motion::auto_height(
            &tokens,
            "terminal-command-sender-height",
            Some(self.render_terminal_command_sender_content(window, cx)),
        )
        .target_height(target_height);
        if allow_overflow {
            reveal.overflow_when_settled().into_any_element()
        } else {
            reveal.into_any_element()
        }
    }

    fn render_terminal_command_sender_content(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let sender_expanded = self.terminal_command_sender.read(cx).is_expanded();
        let Some(active) = self
            .terminal_command_sender
            .read(cx)
            .active_document_snapshot()
        else {
            return div().into_any_element();
        };
        if !sender_expanded {
            return self.render_compact_terminal_command_sender(&active, window, cx);
        }
        let panel_height = self.terminal_command_sender.read(cx).panel_height();
        let targets = self.terminal_command_sender_target_entries(cx);
        let sender_is_resizing = self.terminal_command_sender.read(cx).is_resizing();
        let has_background = self.window_background_preferences().is_some();

        let panel = div()
            .relative()
            .when(panel_height < TERMINAL_SENDER_MIN_HEIGHT, |panel| {
                panel.overflow_hidden()
            })
            .flex_none()
            .h(px(panel_height))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(self.workspace_chrome_divider())
            .bg(if has_background {
                rgba(
                    (theme.bg << 8)
                        | oxideterm_gpui_ui::surface::theme_glass_card_background_alpha(
                            &self.tokens,
                        ),
                )
            } else {
                rgb(theme.bg)
            })
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_terminal_command_sender_resize(event, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.finish_terminal_command_sender_resize(cx);
                }),
            )
            .child(
                div()
                    .id("terminal-command-sender-resize")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(TERMINAL_SENDER_RESIZE_HOTZONE_HEIGHT))
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(2.0))
                    .cursor(CursorStyle::ResizeRow)
                    .occlude()
                    .child(
                        div()
                            .w(px(28.0))
                            .h(px(2.0))
                            .rounded_full()
                            .bg(rgba((theme.text_muted << 8) | 0x38)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            if event.click_count >= 2 {
                                this.terminal_command_sender.update(cx, |sender, cx| {
                                    sender.reset_height(cx);
                                });
                            } else {
                                this.terminal_command_sender.update(cx, |sender, cx| {
                                    sender.start_resize(event.position.y, cx);
                                });
                            }
                            window.prevent_default();
                            cx.stop_propagation();
                        }),
                    ),
            )
            .when(sender_is_resizing, |panel| {
                // Keep pointer capture inside the sender surface while its divider moves.
                panel.cursor(CursorStyle::ResizeRow)
            })
            .child(self.expanded_sender(targets, cx))
            .into_any_element();
        if has_background {
            oxideterm_gpui_ui::surface::tab_surface(&self.tokens, panel)
        } else {
            panel
        }
    }

    fn render_compact_terminal_command_sender(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
        let sender = self.terminal_command_sender.read(cx);
        let focused = sender.compact_focused();
        let draft = Zeroizing::new(
            sender
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        let viewport = sender.active_compact_viewport().unwrap_or_default();
        let placeholder = sender.compact_placeholder().to_string();
        let suggestion_mode = sender.compact_suggestion_mode();
        let suggestion_highlighted = sender.compact_suggestion_highlighted();
        let suggestions = if focused {
            self.terminal_command_sender_compact_suggestions(snapshot, suggestion_mode, cx)
        } else {
            Vec::new()
        };
        let show_history = suggestion_mode == CompactSuggestionMode::History
            && Self::terminal_command_sender_supports_suggestions(snapshot);
        let show_suggestions = focused && (show_history || !suggestions.is_empty());
        let selected_range = self.ime_selected_range_for_target(target, cx);
        let marked_text = self.marked_text_for_target(target, cx);
        let active_offset = self.ime_active_offset_for_target(target, cx);
        let ghost_text = (focused && !show_history && suggestion_highlighted.is_none())
            .then(|| self.terminal_command_sender_compact_ghost_text(snapshot, &draft, cx))
            .flatten();
        let quick_commands_enabled = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_commands_enabled;
        let quick_commands_open = self.terminal.read(cx).quick_commands.is_open();
        let background = if self.window_background_preferences().is_some() {
            self.workspace_chrome_background(theme.bg)
        } else {
            rgba((theme.bg << 8) | TERMINAL_SENDER_COMPACT_BACKGROUND_ALPHA)
        };
        let terminal_settings = &self.settings_store.settings().terminal;
        let workspace = cx.entity();
        let compact_input = text_input_anchor_probe(
            target.anchor_id(),
            text_input_with_viewport_and_ghost_text(
                &self.tokens,
                TextInputView {
                    value: &draft,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range,
                    marked_text,
                },
                &viewport,
                active_offset,
                ghost_text.as_deref(),
            )
            .h(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
            .px_0()
            .border_0()
            .rounded_none()
            .bg(rgba(0x00000000))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(terminal_settings.font_size as f32))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.focus_compact_terminal_command_sender(window, cx);
                    this.ime_marked_text = None;
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );

        div()
            .relative()
            .flex_none()
            .h(px(TERMINAL_SENDER_COMPACT_HEIGHT))
            .px(px(TERMINAL_SENDER_COMPACT_HORIZONTAL_PADDING))
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_t_1()
            .border_color(rgba(
                ((if focused { theme.accent } else { theme.border }) << 8)
                    | TERMINAL_SENDER_COMPACT_BORDER_ALPHA,
            ))
            .bg(background)
            .when(show_suggestions, |row| {
                row.child(self.render_terminal_command_sender_suggestions(
                    &suggestions,
                    suggestion_highlighted,
                    show_history,
                    cx,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .overflow_hidden()
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.focus_compact_terminal_command_sender(window, cx);
                            this.ime_marked_text = None;
                            this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ChevronRight,
                                16.0,
                                rgb(theme.text_muted),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                            .overflow_hidden()
                            .child(compact_input),
                    ),
            )
            .when(quick_commands_enabled, |row| {
                row.child(
                    div()
                        .id("terminal-command-quick-commands-compact")
                        .flex_none()
                        .size(px(TERMINAL_SENDER_COMPACT_EDITOR_HEIGHT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .cursor_pointer()
                        .bg(if quick_commands_open {
                            rgba((theme.accent << 8) | 0x1a)
                        } else {
                            rgba(0x00000000)
                        })
                        .hover(move |style| style.bg(rgb(theme.bg_hover)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, window, cx| {
                                this.toggle_terminal_quick_commands_panel(window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(Self::render_lucide_icon(
                            LucideIcon::Zap,
                            14.0,
                            if quick_commands_open {
                                rgb(theme.accent)
                            } else {
                                rgb(theme.text_muted)
                            },
                        )),
                )
            })
            .into_any_element()
    }

    fn terminal_command_sender_compact_ghost_text(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        draft: &str,
        cx: &App,
    ) -> Option<String> {
        if snapshot.input_mode != TerminalSenderInputMode::Text
            || snapshot.target_scope != TerminalCommandSenderTargetScope::Current
            || snapshot.status == TerminalCommandSenderStatus::Running
            || draft.is_empty()
            || draft
                .chars()
                .any(|character| matches!(character, '\n' | '\r'))
        {
            return None;
        }
        self.active_pane(cx)
            .and_then(|pane| pane.read(cx).history_ghost_text_for_input(draft))
            .filter(|suffix| {
                !suffix
                    .chars()
                    .any(|character| matches!(character, '\n' | '\r'))
            })
    }

    fn terminal_command_sender_supports_suggestions(
        snapshot: &TerminalCommandSenderDocumentSnapshot,
    ) -> bool {
        snapshot.input_mode == TerminalSenderInputMode::Text
            && snapshot.target_scope == TerminalCommandSenderTargetScope::Current
            && snapshot.status != TerminalCommandSenderStatus::Running
    }

    fn terminal_command_sender_compact_suggestions(
        &self,
        snapshot: &TerminalCommandSenderDocumentSnapshot,
        mode: CompactSuggestionMode,
        cx: &mut Context<Self>,
    ) -> Vec<TerminalCommandSuggestion> {
        if !Self::terminal_command_sender_supports_suggestions(snapshot) {
            return Vec::new();
        }
        let draft = self
            .terminal_command_sender
            .read(cx)
            .active_compact_draft()
            .unwrap_or_default();
        match mode {
            CompactSuggestionMode::Automatic => {
                let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
                if self.marked_text_for_target(target, cx).is_some() {
                    return Vec::new();
                }
                let cursor = self
                    .ime_active_selection_offset(target)
                    .map_or(draft.len(), |offset| byte_index_for_utf16(draft, offset));
                self.terminal_command_sender_spec_suggestions(draft, cursor)
            }
            CompactSuggestionMode::History => {
                let draft = Zeroizing::new(draft.to_string());
                self.terminal_command_sender_visible_history_suggestions(&draft, cx)
            }
            CompactSuggestionMode::Dismissed => Vec::new(),
        }
    }

    pub(in crate::workspace) fn accept_terminal_command_sender_suggestion(
        &mut self,
        suggestion: &TerminalCommandSuggestion,
        cx: &mut Context<Self>,
    ) {
        let mut draft = Zeroizing::new(
            self.terminal_command_sender
                .read(cx)
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        let replacement = suggestion.replacement.clone();
        if draft.get(replacement.clone()).is_none() {
            return;
        }
        let end = replacement.start + suggestion.insert_text.len();
        draft.replace_range(replacement, &suggestion.insert_text);
        let caret = draft[..end].encode_utf16().count();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.replace_active_compact_text(std::mem::take(&mut *draft), cx);
        });
        let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
        self.set_ime_selection_from_anchor(target, caret, caret);
        self.ime_marked_text = None;
        self.show_active_input_caret(cx);
        cx.notify();
    }



    fn expanded_sender(&self, entries: Vec<(TerminalCommandSenderTarget, String, TabKind)>, cx: &mut Context<Self>) -> AnyElement {
        use oxideterm_gpui_command_sender::{SenderAction, ExpandedSender, SenderPanelFrame};
        let owner = cx.weak_entity();
        let frame = SenderPanelFrame {
            tokens: self.tokens,
            mono_font: settings_mono_font_family(self.settings_store.settings()),
            divider: self.workspace_chrome_divider(),
            has_background: self.window_background_preferences().is_some(),
            current: self.terminal_command_active_target_label(cx),
            targets: entries.into_iter().map(|(target, label, _)| (target.pane_id, label)).collect(),
            groups: self.terminal_broadcast_groups().iter().map(|group| (group.id, group.name.clone())).collect(),
        };
        ExpandedSender::new(self.terminal_command_sender.clone(), frame, self.i18n.clone(), move |action, w, cx| {
            let _ = owner.update(cx, |app, cx| match action {
                SenderAction::Start(id) => { app.start_terminal_command_sender(id, cx); }
                SenderAction::Group(id, group) => {
                    let panes = app.resolve_terminal_broadcast_group(group, cx);
                    app.terminal_command_sender.update(cx, |s, cx| s.set_target_group(id, group, &panes, cx));
                }
                SenderAction::FocusEditor(id) => app.focus_terminal_command_sender_editor(id, w, cx),
                SenderAction::FocusCompact => { app.clear_ime_selection(); app.focus_compact_terminal_command_sender(w, cx); }
                SenderAction::BlurHostInput => { if app.blur_host_tools_input(cx) { cx.notify(); } }
            });
        }).into_any_element()
    }

    fn terminal_command_sender_target_entries(
        &self,
        cx: &App,
    ) -> Vec<(TerminalCommandSenderTarget, String, TabKind)> {
        let tab_host = self.tab_host.read(cx);
        self.terminal_broadcast_entries(cx)
            .into_iter()
            .filter_map(|entry| {
                tab_host.panes().get(&entry.pane_id).map(|pane| {
                    let pane = pane.downgrade();
                    (
                        TerminalCommandSenderTarget {
                            pane_id: entry.pane_id,
                            write: std::rc::Rc::new(move |frame, cx| {
                                use oxideterm_terminal::TerminalSenderFrame::*;
                                pane.update(cx, |pane, cx| match frame {
                                    TextLine(line) => pane.send_command_sender_line(line, cx),
                                    TextChunk(text) => pane.send_command_sender_text_chunk(text, cx),
                                    RawBytes(bytes) => pane.send_command_sender_raw_bytes(bytes, cx),
                                }).ok()
                            }),
                        },
                        format!("{} · #{}", entry.label, entry.pane_id.0),
                        entry.kind,
                    )
                })
            })
            .collect()
    }

    fn start_terminal_command_sender(
        &mut self,
        sender_id: TerminalCommandSenderId,
        cx: &mut Context<Self>,
    ) -> bool {
        let selected_group_id = self
            .terminal_command_sender
            .read(cx)
            .document_snapshots()
            .into_iter()
            .find(|document| document.id == sender_id)
            .filter(|document| document.target_scope == TerminalCommandSenderTargetScope::Group)
            .and_then(|document| document.selected_group_id);
        if let Some(group_id) = selected_group_id {
            // Resolve at run time so closed and newly opened saved sessions are reflected
            // without opening any connection on behalf of the sender.
            let targets = self.resolve_terminal_broadcast_group(group_id, cx);
            self.terminal_command_sender.update(cx, |sender, cx| {
                sender.set_target_group(sender_id, group_id, &targets, cx);
            });
        }
        let current_pane_id = self.active_pane_id(cx);
        let entries = self.terminal_command_sender_target_entries(cx);
        let live_panes = entries
            .iter()
            .map(|(target, _, _)| target.pane_id)
            .collect::<HashSet<_>>();
        let candidates = entries.into_iter().map(|(target, _, _)| target).collect();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.retain_live_targets(&live_panes, cx);
            sender.start(sender_id, current_pane_id, candidates, cx)
        })
    }

    pub(in crate::workspace) fn ensure_terminal_command_sender_visible(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_sender.update(cx, |sender, cx| {
            if !sender.is_visible() {
                sender.toggle_visible(cx);
            }
        });
    }

    pub(in crate::workspace) fn replace_terminal_command_sender_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        let was_expanded = self.terminal_command_sender.read(cx).is_expanded();
        let sender_id = self
            .terminal_command_sender
            .update(cx, |sender, cx| sender.replace_active_text(text, cx));
        if !was_expanded && self.terminal_command_sender.read(cx).is_expanded() {
            self.cleanup_after_terminal_command_sender_expand(cx);
        }
        sender_id
    }

    pub(in crate::workspace) fn append_terminal_command_sender_text(
        &mut self,
        text: &str,
        separate_with_space: bool,
        cx: &mut Context<Self>,
    ) -> TerminalCommandSenderId {
        let was_expanded = self.terminal_command_sender.read(cx).is_expanded();
        let sender_id = self.terminal_command_sender.update(cx, |sender, cx| {
            sender.append_active_text(text, separate_with_space, cx)
        });
        if !was_expanded && self.terminal_command_sender.read(cx).is_expanded() {
            self.cleanup_after_terminal_command_sender_expand(cx);
        }
        sender_id
    }

    pub(in crate::workspace) fn cleanup_after_terminal_command_sender_expand(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_quick_commands_panel(cx);
        self.close_terminal_command_overlays(cx);
        self.ime_marked_text = None;
        self.clear_ime_selection();
    }

    pub(in crate::workspace) fn focus_terminal_command_sender_editor(
        &mut self,
        sender_id: TerminalCommandSenderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self
            .terminal_command_sender
            .read(cx)
            .document_snapshots()
            .into_iter()
            .find(|document| document.id == sender_id)
            .map(|document| document.editor);
        if let Some(editor) = editor {
            if self.blur_host_tools_input(cx) {
                cx.notify();
            }
            let focus_handle = editor.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }

    pub(super) fn focus_compact_terminal_command_sender(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.blur_terminal_quick_commands_input(cx);
        // Both inputs use the workspace FocusHandle, so GPUI cannot blur the sidebar for us.
        if self.blur_host_tools_input(cx) {
            cx.notify();
        }
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.set_compact_focused(true, cx);
        });
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn focus_terminal_command_sender_input(
        &mut self,
        sender_id: TerminalCommandSenderId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let expanded = self.terminal_command_sender.read(cx).is_expanded();
        if expanded {
            self.blur_terminal_quick_commands_input(cx);
            self.focus_terminal_command_sender_editor(sender_id, window, cx);
            return;
        }
        self.terminal_command_sender.update(cx, |sender, cx| {
            if !sender.is_visible() {
                sender.toggle_visible(cx);
            }
        });
        self.clear_ime_selection();
        self.focus_compact_terminal_command_sender(window, cx);
    }

    pub(in crate::workspace) fn terminal_command_sender_editor_focused(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        let sender = self.terminal_command_sender.read(cx);
        sender.is_expanded()
            && sender.active_document_snapshot().is_some_and(|document| {
                document.editor.read(cx).focus_handle(cx).is_focused(window)
            })
    }

    pub(in crate::workspace) fn handle_compact_terminal_command_sender_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let sender = self.terminal_command_sender.read(cx);
        if !sender.is_visible() || sender.is_expanded() {
            return false;
        }
        let Some(document) = sender.active_document_snapshot() else {
            return false;
        };
        if !sender.compact_focused() {
            return false;
        }
        let mode = sender.compact_suggestion_mode();
        let suggestion_highlighted = sender.compact_suggestion_highlighted();

        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" if !modifiers.platform => {
                if mode == CompactSuggestionMode::History
                    || !self
                        .terminal_command_sender_compact_suggestions(&document, mode, cx)
                        .is_empty()
                {
                    self.terminal_command_sender.update(cx, |sender, cx| {
                        sender.hide_compact_suggestions(cx);
                    });
                    return true;
                }
                self.terminal_command_sender.update(cx, |sender, cx| {
                    sender.set_compact_focused(false, cx);
                });
                self.clear_ime_selection();
                self.focus_active_pane(window, cx);
                true
            }
            "enter" if !modifiers.platform && !modifiers.shift && !modifiers.alt => {
                if let Some(index) = suggestion_highlighted {
                    let suggestions =
                        self.terminal_command_sender_compact_suggestions(&document, mode, cx);
                    if let Some(suggestion) = suggestions.get(index) {
                        self.accept_terminal_command_sender_suggestion(suggestion, cx);
                    }
                    // The candidate list may have changed since it was highlighted.
                    // Never send a command in place of the missing candidate.
                    return true;
                }
                let sender_id = document.id;
                if self.start_terminal_command_sender(sender_id, cx) {
                    self.terminal_command_sender.update(cx, |sender, cx| {
                        sender.clear_compact_text(sender_id, cx);
                    });
                    self.clear_ime_selection();
                }
                true
            }
            "enter"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && modifiers.shift =>
            {
                let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
                let mut draft = Zeroizing::new(
                    self.terminal_command_sender
                        .read(cx)
                        .active_compact_draft()
                        .unwrap_or_default()
                        .to_string(),
                );
                let replacement_range = self.ime_selection_range_for_target(target, cx);
                replace_utf16(&mut draft, replacement_range, "\n");
                let sender_id =
                    self.replace_terminal_command_sender_text(std::mem::take(&mut *draft), cx);
                self.focus_terminal_command_sender_input(sender_id, window, cx);
                true
            }
            "up" | "arrowup" | "down" | "arrowdown"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift
                    && self
                        .marked_text_for_target(
                            WorkspaceImeTarget::TerminalCommandSenderCompact,
                            cx,
                        )
                        .is_none() =>
            {
                let move_down = matches!(event.keystroke.key.as_str(), "down" | "arrowdown");
                // Up still opens history until the user selects a Spec candidate.
                let mut mode = if mode == CompactSuggestionMode::Dismissed
                    || (!move_down && suggestion_highlighted.is_none())
                {
                    CompactSuggestionMode::History
                } else {
                    mode
                };
                let mut suggestions =
                    self.terminal_command_sender_compact_suggestions(&document, mode, cx);
                if suggestions.is_empty() && mode == CompactSuggestionMode::Automatic {
                    mode = CompactSuggestionMode::History;
                    suggestions =
                        self.terminal_command_sender_compact_suggestions(&document, mode, cx);
                }
                if suggestions.is_empty() {
                    return false;
                }
                self.terminal_command_sender.update(cx, |sender, cx| {
                    sender.move_compact_suggestion_selection(
                        suggestions.len(),
                        move_down,
                        mode,
                        cx,
                    );
                });
                true
            }
            "tab"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift =>
            {
                if let Some(index) = suggestion_highlighted
                    && let Some(suggestion) = self
                        .terminal_command_sender_compact_suggestions(&document, mode, cx)
                        .get(index)
                {
                    self.accept_terminal_command_sender_suggestion(suggestion, cx);
                    return true;
                }
                if self.accept_terminal_command_sender_ghost_text(&document, cx) {
                    return true;
                }
                if mode == CompactSuggestionMode::Automatic
                    && let Some(suggestion) = self
                        .terminal_command_sender_compact_suggestions(&document, mode, cx)
                        .first()
                {
                    self.accept_terminal_command_sender_suggestion(suggestion, cx);
                    return true;
                }
                false
            }
            "right" | "arrowright"
                if !modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.shift =>
            {
                self.accept_terminal_command_sender_ghost_text(&document, cx)
            }
            _ => false,
        }
    }

    fn accept_terminal_command_sender_ghost_text(
        &mut self,
        document: &TerminalCommandSenderDocumentSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        let target = WorkspaceImeTarget::TerminalCommandSenderCompact;
        let mut draft = Zeroizing::new(
            self.terminal_command_sender
                .read(cx)
                .active_compact_draft()
                .unwrap_or_default()
                .to_string(),
        );
        let draft_len = draft.encode_utf16().count();
        let caret_is_at_end = self
            .ime_selection_range_for_target(target, cx)
            .is_some_and(|range| range.start == draft_len && range.end == draft_len)
            && self.marked_text_for_target(target, cx).is_none();
        let Some(ghost_text) = caret_is_at_end
            .then(|| self.terminal_command_sender_compact_ghost_text(&document, &draft, cx))
            .flatten()
        else {
            return false;
        };
        draft.push_str(&ghost_text);
        let caret = draft.encode_utf16().count();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.replace_active_compact_text(std::mem::take(&mut *draft), cx);
        });
        self.set_ime_selection_from_anchor(target, caret, caret);
        self.show_active_input_caret(cx);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn update_terminal_command_sender_resize(
        &mut self,
        event: &MouseMoveEvent,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.update_resize(event.position.y, event.dragging(), cx);
        });
    }

    pub(in crate::workspace) fn finish_terminal_command_sender_resize(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_sender
            .update(cx, |sender, cx| sender.finish_resize(cx));
    }

    pub(in crate::workspace) fn sync_terminal_command_sender_appearance(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let terminal_settings = &self.settings_store.settings().terminal;
        let command_bar_enabled = terminal_settings.command_bar.enabled;
        let quick_commands_enabled = terminal_settings.command_bar.quick_commands_enabled;
        let font_family = settings_mono_font_family(self.settings_store.settings()).to_string();
        let font_size = terminal_settings.font_size as f32;
        let line_height = terminal_settings.line_height as f32;
        let tokens = self.tokens;
        let background_active = self.window_background_preferences().is_some();
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.sync_editor_appearance(
                tokens,
                font_family,
                font_size,
                line_height,
                background_active,
                cx,
            );
            if !command_bar_enabled {
                // Disabling the owning surface must not leave hidden jobs with
                // no visible stop control.
                sender.set_expanded(false, cx);
                sender.stop_all(cx);
            }
        });
        if !command_bar_enabled || !quick_commands_enabled {
            // Removing the dock's owning surface also releases any parameter draft and IME target.
            self.close_terminal_quick_commands_panel(cx);
        }
    }

    pub(in crate::workspace) fn render_terminal_quick_bar(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = self
            .active_pane_id(cx)
            .and_then(|pane_id| self.quick_command_context_for_pane(pane_id, cx));
        let (categories, commands) = self.terminal.read(cx).quick_commands.quick_bar_snapshot(
            target.as_ref(),
            self.settings_store
                .settings()
                .terminal
                .command_bar
                .quick_bar_category
                .as_deref(),
        );
        let selected = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_bar_category
            .as_deref()
            .and_then(|id| categories.iter().find(|category| category.id == id));
        let label = selected
            .map(|category| category.name.clone())
            .unwrap_or_else(|| self.i18n.t("terminal.quick_commands.all_groups"));
        let workspace = cx.entity().downgrade();
        let selector_width = oxideterm_gpui_ui::select::estimate_select_trigger_width(
            &self.tokens,
            160.0,
            260.0,
            [label.as_str()],
        );
        let selector =
            oxideterm_gpui_ui::select::select_trigger(&self.tokens, label.clone(), false, false)
                .id("quick-bar-category")
                .w(px(selector_width))
                .h(px(28.0))
                .flex_none()
                .role(gpui::Role::Button)
                .aria_label(label)
                .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.open_quick_bar_category_picker(window, cx)
                    });
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.open_quick_bar_category_picker(window, cx);
                        cx.stop_propagation();
                    }),
                );
        let filtered = selected.is_some();

        let mut content_row = div()
            .h_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .when(commands.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("terminal.quick_commands.group_empty")),
                )
            });
        for category in categories {
            let category_icon = terminal_quick_bar_icon(category.icon);
            let icon_color = category
                .icon_color
                .as_deref()
                .and_then(oxideterm_quick_commands::parse_icon_color)
                .unwrap_or(self.tokens.ui.text_muted);
            let category_commands = commands
                .iter()
                .filter(|command| command.category == category.id)
                .cloned()
                .collect::<Vec<_>>();
            if category_commands.is_empty() {
                continue;
            }
            content_row = content_row.when(!filtered, |row| {
                row.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .text_size(px(10.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(Self::render_lucide_icon(
                            category_icon,
                            11.0,
                            rgb(icon_color),
                        ))
                        .child(category.name),
                )
            });
            for command in category_commands {
                let command_for_run = command.clone();
                let tooltip_name = command.name.clone();
                // The render callback owns this preview source only while the chip exists.
                let tooltip_command = Zeroizing::new(command.command);
                let tokens = self.tokens;
                content_row = content_row.child(
                    action_chip(
                        &self.tokens,
                        command.name,
                        Some(Self::render_lucide_icon(
                            category_icon,
                            11.0,
                            rgb(icon_color),
                        )),
                        ActionChipOptions::new()
                            .idle_text_tone(ActionChipTextTone::Muted)
                            .max_width(QUICK_BAR_COMMAND_MAX_WIDTH),
                    )
                    .id(format!("quick-bar-command-{}", command.id))
                    .tooltip(move |_, cx| {
                        cx.new(|_| QuickCommandPreview {
                            tokens,
                            text: quick_command_preview(&tooltip_name, &tooltip_command),
                        })
                        .into()
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.run_quick_command_model(&command_for_run, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
        }
        // Scrollable transfers the viewport style to its outer wrapper, so the
        // content row must remain a child to preserve horizontal flex layout.
        div()
            .flex_none()
            .h(px(34.0))
            .min_w_0()
            .flex()
            .items_center()
            .pl(px(self.tokens.spacing.two))
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .child(selector)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_x_scrollbar()
                    .child(content_row),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn quick_bar_category_choices(
        &self,
        cx: &App,
    ) -> Vec<(Option<String>, String, LucideIcon)> {
        let mut choices = vec![(
            None,
            self.i18n.t("terminal.quick_commands.all_groups"),
            LucideIcon::Layers,
        )];
        choices.extend(
            self.terminal
                .read(cx)
                .quick_commands
                .store
                .categories
                .iter()
                .map(|category| {
                    (
                        Some(category.id.clone()),
                        category.name.clone(),
                        terminal_quick_bar_icon(category.icon),
                    )
                }),
        );
        choices
    }
}

fn terminal_quick_bar_icon(icon: crate::workspace::quick_commands::QuickCommandIcon) -> LucideIcon {
    match icon {
        crate::workspace::quick_commands::QuickCommandIcon::Terminal => LucideIcon::Terminal,
        crate::workspace::quick_commands::QuickCommandIcon::Server => LucideIcon::Server,
        crate::workspace::quick_commands::QuickCommandIcon::Folder => LucideIcon::Folder,
        crate::workspace::quick_commands::QuickCommandIcon::Docker => LucideIcon::Server,
        crate::workspace::quick_commands::QuickCommandIcon::Zap => LucideIcon::Zap,
    }
}

#[cfg(test)]
mod quick_command_preview_tests {
    use super::*;

    #[test]
    fn preview_preserves_short_commands_and_bounds_unicode_and_multiline_text() {
        assert_eq!(
            quick_command_preview("状态", "echo '你好'\nprintf ok").as_str(),
            "状态\necho '你好'\nprintf ok"
        );
        let exact = "中".repeat(600);
        assert_eq!(
            quick_command_preview("命令", &exact).as_str(),
            format!("命令\n{exact}")
        );
        assert_eq!(
            quick_command_preview("命令", &format!("{exact}🙂")).as_str(),
            format!("命令\n{exact}…")
        );
        assert_eq!(
            quick_command_preview("多行", "1\n2\n3\n4\n5\n6\n7\n8\n9").as_str(),
            "多行\n1\n2\n3\n4\n5\n6\n7\n8…"
        );
    }

    struct ChipFixture;

    impl Render for ChipFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().gap(px(5.0)).children(
                [
                    ("short", "Status".to_string()),
                    ("long", "Long command name ".repeat(30)),
                ]
                .into_iter()
                .map(|(id, label)| {
                    action_chip(
                        &oxideterm_theme::default_tokens(),
                        label,
                        Some(
                            div()
                                .size(px(11.0))
                                .debug_selector(move || format!("{id}-icon"))
                                .into_any_element(),
                        ),
                        ActionChipOptions::new().max_width(QUICK_BAR_COMMAND_MAX_WIDTH),
                    )
                    .debug_selector(move || id.into())
                }),
            )
        }
    }

    #[gpui::test]
    fn long_names_keep_chips_bounded_without_shrinking_icons(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| ChipFixture);
        cx.simulate_resize(gpui::size(px(800.0), px(100.0)));
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        let short = cx.debug_bounds("short").unwrap();
        let long = cx.debug_bounds("long").unwrap();
        assert!(short.size.width < px(220.0));
        assert_eq!(long.size.width, px(220.0));
        assert_eq!(long.left(), short.right() + px(5.0));
        for id in ["short-icon", "long-icon"] {
            assert_eq!(
                cx.debug_bounds(id).unwrap().size,
                gpui::size(px(11.0), px(11.0))
            );
        }
    }
}
