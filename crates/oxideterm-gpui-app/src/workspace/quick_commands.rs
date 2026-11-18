use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_gpui_quick_commands::{
    QuickCommandKeyDirection, QuickCommandsAccess, QuickCommandsBindings, QuickCommandsFrame,
    QuickCommandsIme, QuickCommandsNoticeKind, QuickCommandsRequest, QuickCommandsSelectableText,
    QuickCommandsViews, quick_command_space_inserts_literal,
};
use zeroize::Zeroizing;

pub(in crate::workspace) use oxideterm_gpui_quick_commands::{
    QUICK_COMMAND_TEXTAREA_LINE_HEIGHT, QUICK_COMMAND_TEXTAREA_VERTICAL_PADDING, QuickCommandInput,
    TerminalQuickCommandsState, quick_command_input_uses_monospace,
};
pub(in crate::workspace) use oxideterm_quick_commands::{
    QuickCommandIcon, QuickCommandImportStrategy,
};

pub(super) fn mount(
    terminal: &Entity<WorkspaceTerminalEntity>,
    tokens: ThemeTokens,
    i18n: &I18n,
    settings: &PersistedSettings,
    focus: &FocusHandle,
    anchors: &ime::TextInputAnchorStore,
    cx: &mut Context<WorkspaceApp>,
) {
    let owner = cx.weak_entity();
    let anchors = anchors.clone();
    let bindings = QuickCommandsBindings {
        frame: Rc::new({
            let owner = owner.clone();
            move |cx| {
                owner
                    .update(cx, |app, cx| app.quick_commands_frame(cx))
                    .ok()
            }
        }),
        dispatch: Rc::new({
            let owner = owner.clone();
            move |request, window, cx| {
                let _ = owner.update(cx, |app, cx| {
                    app.handle_quick_commands_request(request, window, cx)
                });
            }
        }),
        ime: Rc::new({
            let owner = owner.clone();
            move |input, cx| {
                let target = WorkspaceImeTarget::QuickCommand(input);
                let owner = owner.upgrade();
                let app = owner.as_ref().map(|owner| owner.read(cx));
                QuickCommandsIme {
                    anchor_id: target.anchor_id(),
                    active_offset: app.and_then(|app| app.ime_active_offset_for_target(target, cx)),
                    selection: app.and_then(|app| app.ime_selected_range_for_target(target, cx)),
                    marked: app
                        .and_then(|app| app.marked_text_for_target(target, cx))
                        .map(|text| Zeroizing::new(text.to_owned())),
                }
            }
        }),
        anchor: Rc::new(move |anchor| anchors.update(anchor)),
        selectable: Rc::new(move |text, cx| {
            owner
                .update(cx, |app, cx| app.render_quick_commands_selectable(text, cx))
                .unwrap_or_else(|_| div().into_any_element())
        }),
    };
    let state = QuickCommandsAccess::new(
        terminal.downgrade(),
        |owner| &owner.quick_commands,
        |owner| &mut owner.quick_commands,
    );
    let views = QuickCommandsViews::new(
        state,
        bindings,
        quick_commands_frame(tokens, i18n, settings, focus),
        cx,
    );
    terminal.update(cx, |owner, _| owner.quick_commands.views = Some(views));
}

fn quick_commands_frame(
    tokens: ThemeTokens,
    i18n: &I18n,
    settings: &PersistedSettings,
    focus: &FocusHandle,
) -> QuickCommandsFrame {
    QuickCommandsFrame {
        tokens,
        i18n: i18n.clone(),
        ui_font: settings_ui_font_family(&settings.appearance.ui_font_family),
        mono_font: settings_mono_font_family(settings),
        focus: focus.clone(),
        enabled: settings.terminal.command_bar.quick_commands_enabled,
        has_background: false,
        caret_visible: false,
        target: None,
        execution_targets: Vec::new(),
    }
}

impl WorkspaceApp {
    fn quick_commands_frame(&self, cx: &mut Context<Self>) -> QuickCommandsFrame {
        let mut frame = quick_commands_frame(
            self.tokens,
            &self.i18n,
            self.settings_store.settings(),
            &self.focus_handle,
        );
        frame.has_background = self.window_background_preferences().is_some();
        frame.caret_visible = self.input_caret.visible();
        if self.terminal.read(cx).quick_commands.has_open_or_pending() {
            frame.target = self
                .active_pane_id(cx)
                .and_then(|pane_id| self.quick_command_context_for_pane(pane_id, cx));
            if self
                .terminal
                .read(cx)
                .quick_commands
                .pending_execution
                .is_some()
            {
                frame.execution_targets = self
                    .quick_command_target_contexts(cx)
                    .into_iter()
                    .map(|(_, target)| target)
                    .collect();
            }
        }
        frame
    }

    fn render_quick_commands_selectable(
        &self,
        text: QuickCommandsSelectableText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some((group, order, alpha, font)) = text.row {
            self.render_row_safe_selectable_display_text_in_group_with_alpha(
                group,
                "quick-commands-row",
                text.id,
                order,
                text.text,
                text.color,
                alpha,
                font,
                cx,
            )
        } else {
            self.render_selectable_text_scoped("quick-commands", text.id, text.text, text.color, cx)
        }
    }

    fn handle_quick_commands_request(
        &mut self,
        request: QuickCommandsRequest,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        match request {
            QuickCommandsRequest::WindowLayoutChanged => {
                self.notify_quick_commands_views(cx);
                cx.notify();
            }
            QuickCommandsRequest::ClearIme => {
                self.ime_marked_text = None;
                self.clear_ime_selection();
            }
            QuickCommandsRequest::FocusInput(input, event) => {
                let Some(window) = window else {
                    return;
                };
                self.close_terminal_command_overlays(cx);
                self.terminal_command_sender.update(cx, |sender, cx| {
                    sender.set_compact_focused(false, cx);
                });
                self.ime_marked_text = None;
                window.focus(&self.focus_handle, cx);
                self.begin_ime_selection_from_mouse_down(
                    WorkspaceImeTarget::QuickCommand(input),
                    &event,
                    window,
                    cx,
                );
            }
            QuickCommandsRequest::DragInput(event) => {
                if let Some(window) = window {
                    self.update_ime_selection_drag_from_mouse_move(&event, window, cx);
                }
            }
            QuickCommandsRequest::OpenManager => {
                if let Some(window) = window {
                    self.open_quick_commands_manager(window, cx);
                }
            }
            QuickCommandsRequest::FocusPane => {
                if let Some(window) = window {
                    self.focus_active_pane(window, cx);
                }
            }
            QuickCommandsRequest::Run(command) => {
                if let Some(window) = window {
                    self.run_quick_command_model(&command, window, cx);
                }
            }
            QuickCommandsRequest::ConfirmExecution => {
                if let Some(window) = window {
                    self.confirm_quick_command_execution(window, cx);
                }
            }
            QuickCommandsRequest::Insert(command) => {
                self.ensure_terminal_command_sender_visible(cx);
                let sender_id = self.replace_terminal_command_sender_text(command, cx);
                if let Some(window) = window {
                    self.focus_terminal_command_sender_input(sender_id, window, cx);
                }
                cx.notify();
            }
            QuickCommandsRequest::CheckQuickBarCategory => self.check_quick_bar_category(cx),
            QuickCommandsRequest::Tooltip { id, label, x, y } => {
                self.queue_workspace_tooltip(id, label, x, y, cx)
            }
            QuickCommandsRequest::ClearTooltip(id) => self.clear_workspace_tooltip(id, cx),
            QuickCommandsRequest::Notice(notice) => self.push_workspace_notice(
                TerminalNotice {
                    title: notice.title,
                    description: notice.description,
                    status_text: None,
                    progress: None,
                    variant: match notice.variant {
                        QuickCommandsNoticeKind::Success => TerminalNoticeVariant::Success,
                        QuickCommandsNoticeKind::Error => TerminalNoticeVariant::Error,
                    },
                },
                cx,
            ),
        }
    }

    fn quick_commands_views<'a>(&self, cx: &'a App) -> &'a QuickCommandsViews {
        self.terminal
            .read(cx)
            .quick_commands
            .views
            .as_ref()
            .expect("mounted quick commands")
    }

    pub(super) fn notify_quick_commands_views(&self, cx: &mut App) {
        let views = self.quick_commands_views(cx);
        let (dock, manager) = (views.dock.entity_id(), views.manager.entity_id());
        cx.notify(dock);
        cx.notify(manager);
    }

    pub(in crate::workspace) fn render_terminal_quick_commands_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.quick_commands_views(cx)
            .dock
            .clone()
            .into_any_element()
    }

    pub(in crate::workspace) fn render_quick_commands_manager_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.quick_commands_views(cx)
            .manager
            .clone()
            .into_any_element()
    }

    fn check_quick_bar_category(&mut self, cx: &mut Context<Self>) {
        let category = self
            .settings_store
            .settings()
            .terminal
            .command_bar
            .quick_bar_category
            .as_ref();
        if category.is_some_and(|id| {
            !self
                .terminal
                .read(cx)
                .quick_commands
                .store
                .categories
                .iter()
                .any(|category| &category.id == id)
        }) {
            self.edit_settings(
                |settings| settings.terminal.command_bar.quick_bar_category = None,
                cx,
            );
        }
    }

    fn save_quick_command_editor(&mut self, cx: &mut Context<Self>) {
        let manager = self.quick_commands_views(cx).manager.clone();
        manager.update(cx, |manager, cx| manager.save_quick_command_editor(cx));
    }

    pub(in crate::workspace) fn update_terminal_quick_commands_resize(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let (changed, released) = self.terminal.update(cx, |terminal, _| {
            terminal.quick_commands.panel.update_drag(event)
        });
        if changed {
            self.notify_quick_commands_views(cx);
        }
        if released {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn finish_terminal_quick_commands_resize(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.update(cx, |terminal, _| {
            terminal.quick_commands.panel.finish_resize()
        }) {
            self.notify_quick_commands_views(cx);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn toggle_terminal_quick_commands_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .terminal
            .update(cx, |terminal, _| terminal.quick_commands.toggle_open());
        if open {
            self.prepare_terminal_quick_commands_panel(window, cx);
        } else {
            self.ime_marked_text = None;
            self.clear_ime_selection();
            self.focus_active_pane(window, cx);
        }
        self.notify_quick_commands_views(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn prepare_terminal_quick_commands_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The sender retains its drafts and running jobs while its expanded surface closes.
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.set_expanded(false, cx);
            sender.set_compact_focused(false, cx);
            sender.dismiss_compact_suggestions();
        });
        self.close_terminal_command_overlays(cx);
        self.ime_marked_text = None;
        self.clear_ime_selection();
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn blur_terminal_quick_commands_input(
        &mut self,
        cx: &mut App,
    ) -> bool {
        let changed = self
            .terminal
            .update(cx, |terminal, _| terminal.quick_commands.blur_input());
        if changed {
            self.notify_quick_commands_views(cx);
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        changed
    }

    pub(in crate::workspace) fn close_terminal_quick_commands_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self
            .terminal
            .update(cx, |terminal, _cx| terminal.quick_commands.close());
        if changed {
            self.notify_quick_commands_views(cx);
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        changed
    }

    pub(in crate::workspace) fn open_quick_commands_manager(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        self.terminal
            .update(cx, |terminal, _cx| terminal.quick_commands.open_manager());
        self.notify_quick_commands_views(cx);
        self.ime_marked_text = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_quick_commands_manager(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self
            .terminal
            .update(cx, |terminal, _cx| terminal.quick_commands.close_manager());
        if changed {
            self.notify_quick_commands_views(cx);
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn cancel_quick_command_category_delete(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.cancel_category_delete()
        });
        if changed {
            self.notify_quick_commands_views(cx);
        }
        changed
    }

    pub(in crate::workspace) fn confirm_quick_command_category_delete(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.confirm_category_delete()
        });
        if changed {
            self.check_quick_bar_category(cx);
            self.notify_quick_commands_views(cx);
        }
        changed
    }

    pub(in crate::workspace) fn finish_terminal_quick_command_execution(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.finish_execution()
        });
        self.notify_quick_commands_views(cx);
    }

    pub(in crate::workspace) fn handle_quick_commands_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.terminal.read(cx).quick_commands.focused_input() else {
            return;
        };
        let target = self
            .active_pane_id(cx)
            .and_then(|pane_id| self.quick_command_context_for_pane(pane_id, cx));
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if input == QuickCommandInput::Search {
            match key {
                "escape" if !modifiers.platform && !modifiers.control => {
                    if self.terminal.read(cx).quick_commands.manager_open() {
                        self.terminal
                            .update(cx, |terminal, _cx| terminal.quick_commands.blur_input());
                    } else {
                        // Escape from the launcher returns input to the terminal.
                        self.close_terminal_quick_commands_panel(cx);
                        self.focus_active_pane(window, cx);
                        cx.notify();
                    }
                    self.ime_marked_text = None;
                    self.notify_quick_commands_views(cx);
                    return;
                }
                "arrowdown" | "down" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .move_highlight(target.as_ref(), QuickCommandKeyDirection::Next)
                    });
                    self.notify_quick_commands_views(cx);
                    return;
                }
                "arrowup" | "up" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .move_highlight(target.as_ref(), QuickCommandKeyDirection::Previous)
                    });
                    self.notify_quick_commands_views(cx);
                    return;
                }
                "home" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .highlight_edge(target.as_ref(), false)
                    });
                    self.notify_quick_commands_views(cx);
                    return;
                }
                "end" if !modifiers.platform && !modifiers.control => {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .highlight_edge(target.as_ref(), true)
                    });
                    self.notify_quick_commands_views(cx);
                    return;
                }
                "enter" if !modifiers.platform && !modifiers.control => {
                    let command = self.terminal.update(cx, |terminal, _cx| {
                        terminal
                            .quick_commands
                            .prepare_highlighted_insertion(target.as_ref())
                    });
                    if let Some(command) = command {
                        self.ensure_terminal_command_sender_visible(cx);
                        let sender_id = self.replace_terminal_command_sender_text(command, cx);
                        self.focus_terminal_command_sender_input(sender_id, window, cx);
                        self.notify_quick_commands_views(cx);
                        cx.notify();
                    }
                    return;
                }
                _ => {}
            }
        }
        match key {
            "tab" if !modifiers.platform && !modifiers.control => {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal
                        .quick_commands
                        .cycle_editor_focus(input, !modifiers.shift)
                }) {
                    self.clear_ime_selection();
                    self.ime_marked_text = None;
                    let commands = &self.terminal.read(cx).quick_commands;
                    if commands.focused_input() == Some(QuickCommandInput::CommandText)
                        && let Some(editor) = commands.command_input.as_ref()
                    {
                        window.focus(&editor.read(cx).focus_handle(cx), cx);
                    } else {
                        window.focus(&self.focus_handle, cx);
                    }
                    self.notify_quick_commands_views(cx);
                }
            }
            "escape" => {
                if self
                    .terminal
                    .update(cx, |terminal, _cx| terminal.quick_commands.blur_input())
                {
                    self.ime_marked_text = None;
                    window.focus(&self.focus_handle, cx);
                    self.notify_quick_commands_views(cx);
                }
            }
            "enter"
                if matches!(
                    input,
                    QuickCommandInput::CategoryName | QuickCommandInput::CategoryColor
                ) =>
            {
                if self.terminal.update(cx, |terminal, _cx| {
                    terminal.quick_commands.save_category_editor()
                }) {
                    self.notify_quick_commands_views(cx);
                }
            }
            "enter"
                if matches!(
                    input,
                    QuickCommandInput::CommandName
                        | QuickCommandInput::CommandText
                        | QuickCommandInput::CommandDescription
                        | QuickCommandInput::CommandHostPattern
                ) =>
            {
                self.save_quick_command_editor(cx);
            }
            "space" | " "
                if quick_command_space_inserts_literal(
                    modifiers.platform,
                    modifiers.control,
                    modifiers.alt,
                ) =>
            {
                // Some GPUI platforms deliver Space without key_char, so the
                // platform text owner never commits it. Route that fallback
                // through the same IME replacement path as ordinary text.
                let target = WorkspaceImeTarget::QuickCommand(input);
                let replacement_range = self.ime_selection_range_for_target(target, cx);
                let caret = replacement_range
                    .as_ref()
                    .map(|range| range.start + " ".encode_utf16().count());
                self.replace_ime_target_text(target, replacement_range, " ", caret, cx);
                self.clear_ime_selection();
                if let Some(caret) = caret {
                    self.set_ime_selection_from_anchor(target, caret, caret);
                }
            }
            _ => {}
        }
    }

    pub(in crate::workspace) fn quick_command_input_value(
        &self,
        input: QuickCommandInput,
        cx: &App,
    ) -> Option<String> {
        self.terminal
            .read(cx)
            .quick_commands
            .input_value(input)
            .map(str::to_string)
    }

    pub(in crate::workspace) fn quick_command_text_editor_focused(
        &self,
        window: &gpui::Window,
        cx: &App,
    ) -> bool {
        let commands = &self.terminal.read(cx).quick_commands;
        commands.manager_open()
            && !commands.category_delete_pending()
            && commands
                .command_input
                .as_ref()
                .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
    }
}
