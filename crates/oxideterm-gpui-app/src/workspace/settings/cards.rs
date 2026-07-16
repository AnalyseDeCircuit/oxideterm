use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_select_trigger(
        &self,
        select_id: SettingsSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
    ) -> Div {
        let focused = self.open_settings_select == Some(select_id);
        // Browser focus-visible depends on keyboard vs pointer origin. Keep the
        // setting select trigger path shared so individual settings pages do
        // not reimplement the same modality check.
        select_trigger_with_focus_visible(
            &self.tokens,
            value,
            placeholder,
            disabled,
            browser_behavior::browser_focus_visible(focused, self.settings_select_focus_origin),
        )
    }

    pub(in crate::workspace) fn settings_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        disabled: bool,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_select_control_with_trigger_style(
            select_id,
            value,
            disabled,
            width,
            |trigger| trigger,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_select_control_with_trigger_style(
        &self,
        select_id: SettingsSelect,
        value: String,
        disabled: bool,
        width: Option<f32>,
        trigger_style: impl FnOnce(Div) -> Div,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = trigger_style(
            self.settings_select_trigger(select_id, value, false, disabled),
        )
        .when(!disabled, |trigger| {
            trigger.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.open_settings_select_from_pointer(select_id, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
        });
        // Settings selects all share the same Radix-like trigger contract:
        // pointer-open sets focus origin, anchor bounds are refreshed in the
        // same paint pass, and scroll-close is owned by the settings surface.
        div()
            .relative()
            .min_w(px(0.0))
            .when_some(width, |control, width| control.w(px(width)).max_w_full())
            .when(width.is_none(), |control| control.w_full())
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_card(
        &self,
        title_key: &str,
        _description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(
                div()
                    .mb(px(self.tokens.metrics.settings_card_title_nudge_y))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(title_key).to_uppercase()),
            )
            .children(rows);
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub(in crate::workspace) fn plain_settings_card(&self, rows: Vec<AnyElement>) -> AnyElement {
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .children(rows);
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub(in crate::workspace) fn terminal_input_settings_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.input_safety")
                            .to_uppercase(),
                    ),
            )
            .child(self.checkbox_row(
                "settings_view.terminal.paste_protection",
                "settings_view.terminal.paste_protection_hint",
                settings.terminal.paste_protection,
                set_paste_protection,
                cx,
            ))
            .child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.osc52_clipboard",
                    "settings_view.terminal.osc52_clipboard_hint",
                    settings.terminal.osc52_clipboard,
                    set_osc52_clipboard,
                    cx,
                ),
                16.0,
            ))
            .child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.osc52_clipboard_read",
                    "settings_view.terminal.osc52_clipboard_read_hint",
                    settings.terminal.osc52_clipboard_read,
                    set_osc52_clipboard_read,
                    cx,
                ),
                16.0,
            ));

        if !cfg!(target_os = "macos") {
            rows = rows.child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.smart_copy",
                    "settings_view.terminal.smart_copy_hint",
                    settings.terminal.smart_copy,
                    set_smart_copy,
                    cx,
                ),
                16.0,
            ));
        }

        let rows = rows
            .child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.copy_on_select",
                    "settings_view.terminal.copy_on_select_hint",
                    settings.terminal.copy_on_select,
                    set_copy_on_select,
                    cx,
                ),
                16.0,
            ))
            .child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.middle_click_paste",
                    "settings_view.terminal.middle_click_paste_hint",
                    settings.terminal.middle_click_paste,
                    set_middle_click_paste,
                    cx,
                ),
                16.0,
            ))
            .child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.selection_requires_shift",
                    "settings_view.terminal.selection_requires_shift_hint",
                    settings.terminal.selection_requires_shift,
                    set_selection_requires_shift,
                    cx,
                ),
                16.0,
            ))
            .child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.free_type_cursor_positioning",
                    "settings_view.terminal.free_type_cursor_positioning_hint",
                    settings.terminal.free_type_cursor_positioning,
                    set_free_type_cursor_positioning,
                    cx,
                ),
                16.0,
            ))
            .child(
                div()
                    .my(px(20.0))
                    .h(px(1.0))
                    .w_full()
                    .bg(rgba((self.tokens.ui.border << 8) | 0x80)),
            )
            .child(self.checkbox_row(
                "settings_view.terminal.autosuggest_local_history",
                "settings_view.terminal.autosuggest_local_history_hint",
                settings.terminal.autosuggest.local_shell_history,
                set_autosuggest_local_history,
                cx,
            ));
        self.settings_card_surface(rows, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_row_with_margin(
        &self,
        row: AnyElement,
        margin_top: f32,
    ) -> AnyElement {
        div().mt(px(margin_top)).child(row).into_any_element()
    }

    pub(in crate::workspace) fn card_title(&self, title_key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(title_key).to_uppercase())
            .into_any_element()
    }

    pub(in crate::workspace) fn card_separator(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .w_full()
            .bg(rgba((self.tokens.ui.border << 8) | 0x80))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_background_active(&self) -> bool {
        self.background_surface_active("settings")
    }

    pub(in crate::workspace) fn settings_panel_background(&self, color: u32) -> Rgba {
        if self.settings_background_active() {
            rgba((color << 8) | oxideterm_gpui_ui::theme_glass_card_background_alpha(&self.tokens))
        } else {
            rgb(color)
        }
    }

    pub(in crate::workspace) fn settings_card_surface(&self, card: Div, _color: u32) -> Div {
        let chrome = oxideterm_gpui_ui::surface_chrome(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::None)
                .has_background_image(self.settings_background_active()),
        );
        // Settings cards already own their inner padding and children, so only
        // shared chrome and theme-aware elevation are applied here.
        let card = card
            .rounded(px(chrome.radius))
            .border_color(chrome.border)
            .bg(chrome.background);
        oxideterm_gpui_ui::theme_card_surface_shadow(card, &self.tokens)
    }

    pub(in crate::workspace) fn text_badge(&self, label: String, color: u32) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | 0x1a))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    pub(in crate::workspace) fn standard_footer_action_button(
        &self,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // Tauri DialogFooter buttons are normal shadcn Buttons, but their
        // focus-visible ring is owned by keyboard navigation rather than mouse
        // hover. Route activation through the workspace Button guard so
        // disabled/loading footers cannot dispatch while preserving that ring.
        self.workspace_confirm_footer_action_button(
            label,
            variant,
            action,
            disabled,
            self.standard_confirm_focus(),
            move |this, event, window, cx| {
                this.clear_standard_confirm_focus();
                listener(this, event, window, cx);
                cx.stop_propagation();
            },
            cx,
        )
    }

    pub(in crate::workspace) fn split_confirm_footer_button(
        &self,
        label: String,
        action: ConfirmDialogAction,
        destructive: bool,
        draw_right_separator: bool,
    ) -> Div {
        let text_color = if destructive {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text_muted
        };
        let hover_bg = if destructive {
            rgba((self.tokens.ui.error << 8) | 0x1a)
        } else {
            rgba((self.tokens.ui.bg_hover << 8) | 0x80)
        };
        let hover_text = if destructive {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text
        };

        // Some Tauri confirm dialogs use a split footer instead of shadcn
        // DialogFooter spacing. Use the shared split footer primitive so AI
        // and settings confirms share button focus-visible behavior.
        split_footer_button(
            &self.tokens,
            label,
            SplitFooterButtonOptions {
                text_color: rgb(text_color),
                hover_text_color: rgb(hover_text),
                hover_background: hover_bg,
                font_weight: if destructive {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::MEDIUM
                },
                focus_visible: self.standard_confirm_focus() == Some(action),
                right_separator: draw_right_separator,
                separator_color: Some(rgba((self.tokens.ui.border << 8) | 0x66)),
                disabled: false,
                loading: false,
                height: None,
                padding_y: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                edge: SplitFooterButtonEdge::None,
            },
        )
    }

    pub(in crate::workspace) fn split_confirm_footer_action_button(
        &self,
        label: String,
        action: ConfirmDialogAction,
        destructive: bool,
        draw_right_separator: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // Split confirm footers are visually different from DialogFooter, but
        // Tauri still routes pointer activation through the same Radix action
        // lifecycle. Keep focus cleanup and event isolation shared with
        // standard_footer_action_button.
        self.split_confirm_footer_button(label, action, destructive, draw_right_separator)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    this.clear_standard_confirm_focus();
                    listener(this, event, window, cx);
                    cx.stop_propagation();
                }),
            )
    }

    pub(in crate::workspace) fn terminal_page_switcher(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pages = TerminalSettingsPage::all();
        let active_index = pages
            .iter()
            .position(|page| *page == self.settings_page.terminal_page)
            .unwrap_or(0);
        let previous_index = pages
            .iter()
            .position(|page| *page == self.settings_page.previous_terminal_page)
            .unwrap_or(active_index);
        let mut items = Vec::with_capacity(pages.len());
        for (page_index, page) in pages.iter().enumerate() {
            let page_id = *page;
            let active = self.settings_page.terminal_page == page_id;
            let item = oxideterm_gpui_ui::segmented_control_item(
                &self.tokens,
                self.i18n.t(page_id.label_key()),
                active,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.settings_page.terminal_page != page_id {
                        this.settings_page.set_terminal_page(page_id);
                        this.begin_user_segmented_control_transition(
                            selection_motion::TERMINAL_SETTINGS_SWITCHER_ID,
                            page_index,
                            cx,
                        );
                    }
                    cx.notify();
                }),
            );
            items.push(item.into_any_element());
        }
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            selection_motion::TERMINAL_SETTINGS_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(
                active_index,
                previous_index,
                pages.len(),
            )
            .user_transition_active(self.segmented_control_user_transition_active(
                selection_motion::TERMINAL_SETTINGS_SWITCHER_ID,
                active_index,
            ))
            .has_background_image(self.settings_background_active()),
            items,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_page_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        let pages = AiSettingsPage::all();
        let active_index = pages
            .iter()
            .position(|page| *page == self.settings_page.ai_page)
            .unwrap_or(0);
        let previous_index = pages
            .iter()
            .position(|page| *page == self.settings_page.previous_ai_page)
            .unwrap_or(active_index);
        let mut items = Vec::with_capacity(pages.len());
        for (page_index, page) in pages.iter().enumerate() {
            let page_id = *page;
            let active = self.settings_page.ai_page == page_id;
            let item = oxideterm_gpui_ui::segmented_control_item(
                &self.tokens,
                self.i18n.t(page_id.label_key()),
                active,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.settings_page.ai_page != page_id {
                        this.settings_page.set_ai_page(page_id);
                        this.begin_user_segmented_control_transition(
                            selection_motion::AI_SETTINGS_SWITCHER_ID,
                            page_index,
                            cx,
                        );
                    }
                    cx.notify();
                }),
            );
            items.push(item.into_any_element());
        }
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            selection_motion::AI_SETTINGS_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(
                active_index,
                previous_index,
                pages.len(),
            )
            .user_transition_active(self.segmented_control_user_transition_active(
                selection_motion::AI_SETTINGS_SWITCHER_ID,
                active_index,
            ))
            .has_background_image(self.settings_background_active()),
            items,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn update_select_anchor(
        &mut self,
        anchor: OverlayAnchor,
        cx: &mut Context<Self>,
    ) {
        let should_notify = self
            .open_settings_select
            .is_some_and(|select| select.anchor_id() == anchor.id)
            || matches!(
                (self.open_new_connection_select, anchor.id),
                (
                    Some(NewConnectionSelect::Group),
                    SelectAnchorId::NewConnectionGroup
                ) | (
                    Some(NewConnectionSelect::KeyAuthSource),
                    SelectAnchorId::NewConnectionKeyAuthSource
                ) | (
                    Some(NewConnectionSelect::ManagedKey),
                    SelectAnchorId::NewConnectionManagedKey
                ) | (
                    Some(NewConnectionSelect::JumpSavedConnection),
                    SelectAnchorId::NewConnectionJumpSavedConnection
                ) | (
                    Some(NewConnectionSelect::JumpKeyAuthSource),
                    SelectAnchorId::NewConnectionJumpKeyAuthSource
                ) | (
                    Some(NewConnectionSelect::JumpManagedKey),
                    SelectAnchorId::NewConnectionJumpManagedKey
                ) | (
                    Some(NewConnectionSelect::UpstreamProxyPolicy),
                    SelectAnchorId::NewConnectionUpstreamProxyPolicy
                ) | (
                    Some(NewConnectionSelect::UpstreamProxyProtocol),
                    SelectAnchorId::NewConnectionUpstreamProxyProtocol
                ) | (
                    Some(NewConnectionSelect::UpstreamProxyAuth),
                    SelectAnchorId::NewConnectionUpstreamProxyAuth
                ) | (
                    Some(NewConnectionSelect::SerialPort),
                    SelectAnchorId::NewConnectionSerialPort
                ) | (
                    Some(NewConnectionSelect::SerialDataBits),
                    SelectAnchorId::NewConnectionSerialDataBits
                ) | (
                    Some(NewConnectionSelect::SerialStopBits),
                    SelectAnchorId::NewConnectionSerialStopBits
                ) | (
                    Some(NewConnectionSelect::SerialParity),
                    SelectAnchorId::NewConnectionSerialParity
                ) | (
                    Some(NewConnectionSelect::SerialFlowControl),
                    SelectAnchorId::NewConnectionSerialFlowControl
                )
            )
            || matches!(
                (self.cloud_sync.view.open_select, anchor.id),
                (
                    Some(crate::workspace::cloud_sync::CloudSyncSelect::Backend),
                    SelectAnchorId::CloudSyncBackend
                ) | (
                    Some(crate::workspace::cloud_sync::CloudSyncSelect::AuthMode),
                    SelectAnchorId::CloudSyncAuthMode
                ) | (
                    Some(crate::workspace::cloud_sync::CloudSyncSelect::ConflictStrategy),
                    SelectAnchorId::CloudSyncConflictStrategy
                )
            )
            || (matches!(
                anchor.id,
                SelectAnchorId::AiPanelRoot
                    | SelectAnchorId::AiConversationList
                    | SelectAnchorId::AiChatMenu
                    | SelectAnchorId::AiModelSelector
                    | SelectAnchorId::AiInlineModelSelector
                    | SelectAnchorId::AiSafetyMenu
                    | SelectAnchorId::AiContextPopover
            ) && self.has_ai_sidebar_floating_overlay())
            || (anchor.id == SelectAnchorId::TerminalBroadcastMenu
                && self.terminal_broadcast_menu_open)
            || (anchor.id == SelectAnchorId::TerminalCommandBar
                && self.terminal_quick_commands_open)
            || (anchor.id == SelectAnchorId::TerminalCwdMenu && self.terminal_cwd_picker.open)
            || (anchor.id == SelectAnchorId::TerminalGitBranchMenu
                && self.terminal_git_branch_picker.open)
            || (anchor.id == SelectAnchorId::TerminalProjectMenu
                && self.terminal_project_panel.open)
            || (anchor.id == SelectAnchorId::SessionManagerViewMode
                && self.session_manager.view_mode_menu_open)
            || (anchor.id == SelectAnchorId::SessionManagerSort
                && self.session_manager.sort_menu_open)
            || (anchor.id == SelectAnchorId::SessionManagerBatchMove
                && self.session_manager.show_batch_move)
            || self
                .settings_slider_drag
                .is_some_and(|slider| settings_slider_anchor_id(slider) == anchor.id);
        if !should_notify && !select_anchor_tracks_while_closed(anchor.id) {
            self.select_anchors.remove(&anchor.id);
            return;
        }
        if self.select_anchors.get(&anchor.id) != Some(&anchor) {
            self.select_anchors.insert(anchor.id, anchor);
            if should_notify {
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn deferred_ai_select_anchor_update(
        workspace: gpui::Entity<Self>,
    ) -> impl FnOnce(OverlayAnchor, &mut Window, &mut App) {
        move |anchor, window, cx| {
            // AI popovers are rendered from floating overlay probes. Updating the
            // workspace synchronously from prepaint can re-enter WorkspaceApp
            // when a click opened another modal in the same effect cycle.
            window.defer(cx, move |_window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            });
        }
    }

    pub(in crate::workspace) fn deferred_ai_text_input_anchor_update(
        workspace: gpui::Entity<Self>,
    ) -> impl FnOnce(TextInputAnchor, &mut Window, &mut App) {
        move |anchor, window, cx| {
            // AI sidebar text anchors can be repainted while a floating menu
            // click opens another overlay. Defer the write to avoid re-entering
            // WorkspaceApp from GPUI prepaint.
            window.defer(cx, move |_window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            });
        }
    }

    pub(in crate::workspace) fn handle_settings_input_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.focused_settings_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        match key {
            "tab" if self.ai.models.mcp_add_dialog.is_some() && input.is_ai_mcp() => {
                // Tauri MCP add dialog lets Tab leave the active input and enter
                // the DialogFooter. GPUI settings inputs are manually owned, so
                // delegate the input-to-footer edge to the shared browser model.
                if let Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(action)) =
                    browser_behavior::modal_footer_input_key_action(
                        key,
                        event.keystroke.modifiers.shift,
                        &CONFIRM_DIALOG_FOOTER_ACTIONS,
                        true,
                        true,
                        self.standard_confirm_focus_owner(),
                        ConfirmDialogAction::Cancel,
                        None,
                    )
                {
                    self.focused_settings_input = None;
                    self.clear_settings_input_draft(input);
                    self.set_standard_confirm_focus(action);
                    self.new_connection_caret_visible = true;
                    cx.notify();
                    return true;
                }

                false
            }
            "escape"
                if input == SettingsInput::TerminalCommandSpecsJson
                    && self.terminal_command_specs_editor_open =>
            {
                // This input belongs to a workspace modal, so Escape dismisses
                // the editor instead of leaving an unfocused modal behind.
                self.close_terminal_command_specs_editor(cx);
                self.new_connection_caret_visible = true;
                true
            }
            "escape" => {
                self.focused_settings_input = None;
                self.clear_settings_input_draft(input);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "enter" => {
                if input.accepts_newline() {
                    self.settings_input_draft.push('\n');
                    self.apply_settings_input_draft(input, cx);
                    return true;
                }
                self.focused_settings_input = None;
                self.clear_settings_input_draft(input);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" | "delete" if !modifiers.platform && !modifiers.control => {
                if self.settings_input_draft.pop().is_some() {
                    // Empty Backspace/Delete does not change the draft value.
                    self.apply_settings_input_draft(input, cx);
                }
                true
            }
            _ => true,
        }
    }

    pub(in crate::workspace) fn blur_text_inputs(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(input) = self.focused_settings_input.take() {
            self.clear_settings_input_draft(input);
            self.ime_marked_text = None;
            self.clear_ime_selection();
            changed = true;
        }
        if self.open_settings_select.is_some() {
            self.ime_marked_text = None;
            self.close_settings_select();
            changed = true;
        }
        if self.open_new_connection_select.is_some() {
            self.ime_marked_text = None;
            self.close_new_connection_select();
            changed = true;
        }
        if self.terminal_command_bar_focused {
            self.terminal_command_bar_focused = false;
            self.ime_marked_text = None;
            changed = true;
        }
        if let Some(player) = self.terminal_cast_player.as_mut()
            && player.search_focused
        {
            player.search_focused = false;
            self.ime_marked_text = None;
            changed = true;
        }
        if self.terminal_quick_commands_open || self.terminal_quick_command_pending.is_some() {
            self.close_terminal_quick_commands_popover();
            changed = true;
        }
        if self.close_terminal_git_branch_picker() {
            changed = true;
        }
        if self.session_manager.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.forwarding_view.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.file_manager.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.launcher.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.graphics.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.sftp_view.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.ai.models.selector_search_focused || self.ai.models.selector_open {
            // The AI model selector can live either in the sidebar portal or
            // inside the terminal inline panel. A generic outside blur should
            // release the searchable select without restoring inline focus.
            self.ai.models.selector_search_focused = false;
            self.ai.models.selector_open = false;
            self.ai.models.selector_scope = None;
            self.ai.models.selector_focus_origin = None;
            self.ai.models.selector_search_query.clear();
            self.ai.models.selector_highlighted_model = None;
            self.ime_marked_text = None;
            changed = true;
        }
        if self.ai.chat.inline_panel.prompt_focused {
            // The inline AI prompt is rendered inside the terminal pane rather
            // than as a normal form control, so it must explicitly join the
            // shared blur path or it remains the active IME target after an
            // outside click.
            self.ai.chat.inline_panel.prompt_focused = false;
            self.ime_marked_text = None;
            changed = true;
        }
        if self.ai.chat.input_focused {
            self.ai.chat.input_focused = false;
            self.ai.chat.autocomplete_suppressed = true;
            self.ime_marked_text = None;
            changed = true;
        }
        if self.ai.chat.editing_message_focused {
            self.ai.chat.editing_message_focused = false;
            self.ime_marked_text = None;
            changed = true;
        }
        if let Some(form) = self.new_connection_form.as_mut()
            && form.field_focused
        {
            form.field_focused = false;
            form.selected_field = None;
            self.ime_marked_text = None;
            changed = true;
        }
        if changed {
            self.clear_ime_selection();
            self.new_connection_caret_visible = true;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn update_settings_slider_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(slider) = self.settings_slider_drag {
            self.apply_settings_slider_from_position(slider, f32::from(event.position.x), cx);
        }
    }

    pub(in crate::workspace) fn apply_settings_slider_from_position(
        &mut self,
        slider: SettingsSlider,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        match slider {
            SettingsSlider::TerminalFontSize => {
                self.set_font_size_from_position(x, cx);
            }
            SettingsSlider::AppearanceUiFontSize => {
                let Some(value) = self.settings_slider_value_from_position(
                    settings_slider_anchor_id(slider),
                    x,
                    APPEARANCE_UI_FONT_SIZE_MIN,
                    APPEARANCE_UI_FONT_SIZE_MAX,
                ) else {
                    return;
                };
                let value = value.round() as i64;
                if self.settings_store.settings().appearance.ui_font_size != value {
                    self.edit_settings(|settings| settings.appearance.ui_font_size = value, cx);
                }
            }
            SettingsSlider::AppearanceBorderRadius
            | SettingsSlider::VersionMigrationBorderRadius => {
                let Some(value) = self.settings_slider_value_from_position(
                    settings_slider_anchor_id(slider),
                    x,
                    APPEARANCE_BORDER_RADIUS_MIN,
                    APPEARANCE_BORDER_RADIUS_MAX,
                ) else {
                    return;
                };
                let value = value.round() as i64;
                if self.settings_store.settings().appearance.border_radius != value {
                    self.edit_settings(|settings| settings.appearance.border_radius = value, cx);
                }
            }
            SettingsSlider::AppearanceBackgroundOpacity => {
                let Some(value) = self.settings_slider_value_from_position(
                    SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                    x,
                    3.0,
                    50.0,
                ) else {
                    return;
                };
                let value = value.round() as f64 / 100.0;
                if self.settings_store.settings().terminal.background_opacity != value {
                    self.edit_settings(|settings| settings.terminal.background_opacity = value, cx);
                }
            }
            SettingsSlider::AppearanceBackgroundBlur => {
                self.set_background_blur_preview_from_position(x, cx);
            }
        }
    }

    pub(in crate::workspace) fn finish_settings_slider_drag(&mut self, cx: &mut Context<Self>) {
        if self.settings_slider_drag.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn focus_settings_input(
        &mut self,
        input: SettingsInput,
        current_value: String,
        cx: &mut Context<Self>,
    ) {
        self.close_settings_select();
        if let Some(previous_input) = self
            .focused_settings_input
            .filter(|previous| *previous != input)
        {
            self.clear_settings_input_draft(previous_input);
        }
        self.focused_settings_input = Some(input);
        self.clear_ime_selection();
        self.settings_input_draft = current_value;
        self.new_connection_caret_visible = true;
        cx.notify();
    }

    pub(in crate::workspace) fn close_settings_select(&mut self) {
        browser_behavior::close_browser_trigger_select(
            &mut self.open_settings_select,
            &mut self.settings_select_focus_origin,
        );
    }

    pub(in crate::workspace) fn clear_settings_input_draft(&mut self, input: SettingsInput) {
        if input.is_secret() {
            zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
        }
        self.settings_input_draft.clear();
    }

    pub(in crate::workspace) fn apply_focused_cloud_sync_input_draft(&mut self) -> bool {
        let Some(input) = self.focused_settings_input else {
            return false;
        };
        if !apply_cloud_sync_form_input_draft(
            &mut self.cloud_sync.view.form,
            input,
            &self.settings_input_draft,
        ) {
            return false;
        }
        // Cloud Sync configuration commits can be triggered by tab/action
        // changes, so release the manually owned input after its latest draft
        // has been copied into the form.
        self.focused_settings_input = None;
        self.clear_settings_input_draft(input);
        true
    }

    pub(in crate::workspace) fn current_settings_input_value(
        &self,
        input: SettingsInput,
    ) -> String {
        let settings = self.settings_store.settings();
        if let Some(value) = persisted_settings_input_value(settings, input) {
            return value;
        }
        if let Some(value) = self.settings_page.page_input_value(input) {
            return value;
        }
        if let Some(value) = ai_mcp_draft_input_value(self.ai.models.mcp_add_dialog.as_ref(), input)
        {
            return value;
        }
        if let Some(value) = cloud_sync_form_input_value(&self.cloud_sync.view.form, input) {
            return value;
        }
        match input {
            SettingsInput::TerminalCommandSpecsJson => {
                self.terminal_command_specs_editor_initial_value()
            }
            SettingsInput::NativePluginInstallUrl => {
                self.native_plugin_manager.install_url_draft.clone()
            }
            SettingsInput::NativePluginInstallChecksum => {
                self.native_plugin_manager.install_checksum_draft.clone()
            }
            SettingsInput::NativePluginRegistryUrl => {
                self.native_plugin_manager.registry_url_draft.clone()
            }
            SettingsInput::PortableCurrentPassword => self.portable_current_password.clone(),
            SettingsInput::PortableNewPassword => self.portable_new_password.clone(),
            SettingsInput::PortableConfirmPassword => self.portable_confirm_password.clone(),
            SettingsInput::AppLockCurrentPassword
            | SettingsInput::AppLockNewPassword
            | SettingsInput::AppLockConfirmPassword => self.app_lock_input_value(input).to_string(),
            SettingsInput::ManagedKeyFilePath => self.settings_managed_key_file_path.clone(),
            SettingsInput::ManagedKeyFileName => self.settings_managed_key_file_name.clone(),
            SettingsInput::ManagedKeyFilePassphrase => {
                self.settings_managed_key_file_passphrase.clone()
            }
            SettingsInput::ManagedKeyPasteName => self.settings_managed_key_paste_name.clone(),
            SettingsInput::ManagedKeyPastePrivateKey => {
                self.settings_managed_key_paste_private_key.clone()
            }
            SettingsInput::ManagedKeyPastePassphrase => {
                self.settings_managed_key_paste_passphrase.clone()
            }
            SettingsInput::ManagedKeyRenameName => self.settings_managed_key_rename_name.clone(),
            SettingsInput::ConnectionImportTargetGroup => {
                self.settings_connection_import_target_group.clone()
            }
            SettingsInput::LocalPrivilegeLabel => self.settings_local_privilege_draft.label.clone(),
            SettingsInput::LocalPrivilegeUsernameHint => {
                self.settings_local_privilege_draft.username_hint.clone()
            }
            SettingsInput::LocalPrivilegeSecret => {
                self.settings_local_privilege_draft.secret.clone()
            }
            SettingsInput::LocalPrivilegePromptPatterns => {
                self.settings_local_privilege_draft.prompt_patterns.clone()
            }
            SettingsInput::PluginSetting(index) => self
                .native_plugin_runtime
                .registry
                .contributions()
                .settings
                .get(index)
                .and_then(|setting| {
                    self.native_plugin_runtime
                        .registry
                        .plugin_setting_value(&setting.plugin_id, &setting.definition.id)
                })
                .map(|value| plugin_setting_input_value(&value))
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    pub(in crate::workspace) fn apply_settings_input_draft(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) {
        let mut next_settings = self.settings_store.settings().clone();
        match apply_persisted_settings_input_draft(
            &mut next_settings,
            input,
            &self.settings_input_draft,
        ) {
            SettingsInputDraftApply::Applied => {
                self.edit_settings(move |settings| *settings = next_settings, cx);
                return;
            }
            SettingsInputDraftApply::Invalid => {
                cx.notify();
                return;
            }
            SettingsInputDraftApply::Unhandled => {}
        }

        if self
            .settings_page
            .apply_page_input_draft(input, &self.settings_input_draft)
        {
            cx.notify();
            return;
        }
        if apply_cloud_sync_form_input_draft(
            &mut self.cloud_sync.view.form,
            input,
            &self.settings_input_draft,
        ) {
            cx.notify();
            return;
        }
        if apply_ai_mcp_draft_input(
            self.ai.models.mcp_add_dialog.as_mut(),
            input,
            &self.settings_input_draft,
        ) {
            cx.notify();
            return;
        }

        match input {
            SettingsInput::TerminalCommandSpecsJson => {
                cx.notify();
            }
            SettingsInput::AiProviderApiKey(_) => {
                cx.notify();
            }
            SettingsInput::NativePluginInstallUrl => {
                self.native_plugin_manager.install_url_draft =
                    self.settings_input_draft.trim().to_string();
                cx.notify();
            }
            SettingsInput::NativePluginInstallChecksum => {
                self.native_plugin_manager.install_checksum_draft =
                    self.settings_input_draft.trim().to_string();
                cx.notify();
            }
            SettingsInput::NativePluginRegistryUrl => {
                self.native_plugin_manager.registry_url_draft =
                    self.settings_input_draft.trim().to_string();
                cx.notify();
            }
            SettingsInput::PortableCurrentPassword => {
                zeroize::Zeroize::zeroize(&mut self.portable_current_password);
                self.portable_current_password = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::PortableNewPassword => {
                zeroize::Zeroize::zeroize(&mut self.portable_new_password);
                self.portable_new_password = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::PortableConfirmPassword => {
                zeroize::Zeroize::zeroize(&mut self.portable_confirm_password);
                self.portable_confirm_password = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::AppLockCurrentPassword
            | SettingsInput::AppLockNewPassword
            | SettingsInput::AppLockConfirmPassword => {
                let draft = self.settings_input_draft.clone();
                let _ = self.set_app_lock_input_value(input, &draft);
                cx.notify();
            }
            SettingsInput::ManagedKeyFilePath => {
                self.settings_managed_key_file_path = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::ManagedKeyFileName => {
                self.settings_managed_key_file_name = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::ManagedKeyFilePassphrase => {
                zeroize::Zeroize::zeroize(&mut self.settings_managed_key_file_passphrase);
                self.settings_managed_key_file_passphrase = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::ManagedKeyPasteName => {
                self.settings_managed_key_paste_name = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::ManagedKeyPastePrivateKey => {
                zeroize::Zeroize::zeroize(&mut self.settings_managed_key_paste_private_key);
                self.settings_managed_key_paste_private_key = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::ManagedKeyPastePassphrase => {
                zeroize::Zeroize::zeroize(&mut self.settings_managed_key_paste_passphrase);
                self.settings_managed_key_paste_passphrase = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::ManagedKeyRenameName => {
                self.settings_managed_key_rename_name = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::ConnectionImportTargetGroup => {
                self.settings_connection_import_target_group = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::LocalPrivilegeLabel => {
                self.settings_local_privilege_draft.label = self.settings_input_draft.clone();
                self.settings_local_privilege_error = None;
                cx.notify();
            }
            SettingsInput::LocalPrivilegeUsernameHint => {
                self.settings_local_privilege_draft.username_hint =
                    self.settings_input_draft.clone();
                self.settings_local_privilege_error = None;
                cx.notify();
            }
            SettingsInput::LocalPrivilegeSecret => {
                zeroize::Zeroize::zeroize(&mut self.settings_local_privilege_draft.secret);
                self.settings_local_privilege_draft.secret = self.settings_input_draft.clone();
                self.settings_local_privilege_error = None;
                cx.notify();
            }
            SettingsInput::LocalPrivilegePromptPatterns => {
                self.settings_local_privilege_draft.prompt_patterns =
                    self.settings_input_draft.clone();
                self.settings_local_privilege_error = None;
                cx.notify();
            }
            SettingsInput::NetworkProxyTestHost => {
                self.settings_network_proxy_test_host = self.settings_input_draft.clone();
                self.settings_network_proxy_test_status = None;
                cx.notify();
            }
            SettingsInput::NetworkProxyTestPort => {
                self.settings_network_proxy_test_port = self.settings_input_draft.clone();
                self.settings_network_proxy_test_status = None;
                cx.notify();
            }
            SettingsInput::PluginSetting(index) => {
                let Some(setting) = self
                    .native_plugin_runtime
                    .registry
                    .contributions()
                    .settings
                    .get(index)
                    .cloned()
                else {
                    cx.notify();
                    return;
                };
                let value = match plugin_setting_draft_to_value(
                    &setting.definition.setting_type,
                    &self.settings_input_draft,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.native_plugin_runtime
                            .registry
                            .record_manager_error(setting.plugin_id.clone(), error);
                        cx.notify();
                        return;
                    }
                };
                if let Err(error) = self.set_native_plugin_setting_value_and_emit(
                    &setting.plugin_id,
                    &setting.definition.id,
                    value,
                    cx,
                ) {
                    self.native_plugin_runtime
                        .registry
                        .record_manager_error(setting.plugin_id.clone(), error);
                }
                cx.notify();
            }
            _ => {
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn edit_highlight_rule(
        &mut self,
        index: usize,
        edit: impl FnOnce(&mut HighlightRule),
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            move |settings| {
                if let Some(rule) = settings.terminal.highlight_rules.get_mut(index) {
                    edit(rule);
                }
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone());
            },
            cx,
        );
    }

    pub(in crate::workspace) fn add_highlight_rule(&mut self, cx: &mut Context<Self>) {
        self.add_highlight_preset(vec![create_default_highlight_rule(|_| {})], cx);
    }

    pub(in crate::workspace) fn add_highlight_preset(
        &mut self,
        rules: Vec<HighlightRule>,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            move |settings| {
                settings.terminal.highlight_rules.extend(rules);
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone())
                        .into_iter()
                        .take(MAX_HIGHLIGHT_RULES)
                        .collect();
            },
            cx,
        );
    }

    pub(in crate::workspace) fn remove_highlight_rule(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            move |settings| {
                if index < settings.terminal.highlight_rules.len() {
                    settings.terminal.highlight_rules.remove(index);
                }
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone());
            },
            cx,
        );
    }

    pub(in crate::workspace) fn move_highlight_rule(
        &mut self,
        index: usize,
        direction: isize,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            move |settings| {
                let len = settings.terminal.highlight_rules.len();
                let next = if direction < 0 {
                    index.checked_sub(1)
                } else if index + 1 < len {
                    Some(index + 1)
                } else {
                    None
                };
                if let Some(next) = next {
                    settings.terminal.highlight_rules.swap(index, next);
                }
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone());
            },
            cx,
        );
    }

    pub(in crate::workspace) fn set_font_size_from_position(
        &mut self,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SettingsTerminalFontSizeSlider)
            .copied()
        else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        let value = (8.0 + percent * (32.0 - 8.0)).round() as i64;
        if self.settings_store.settings().terminal.font_size != value {
            self.edit_settings(|settings| settings.terminal.font_size = value, cx);
        }
    }

    pub(in crate::workspace) fn settings_slider_value_from_position(
        &self,
        anchor_id: SelectAnchorId,
        x: f32,
        min: f32,
        max: f32,
    ) -> Option<f32> {
        let Some(anchor) = self.select_anchors.get(&anchor_id).copied() else {
            return None;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        // Slider mousemove can fire many times inside one rounded setting step.
        // Callers compare the resulting persisted value before notifying.
        Some(min + percent * (max - min))
    }

    pub(in crate::workspace) fn set_background_blur_preview_from_position(
        &mut self,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SettingsAppearanceBackgroundBlurSlider)
            .copied()
        else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        let value = (percent * 20.0).round() as i64;
        let persisted_background_blur = self.settings_store.settings().terminal.background_blur;
        let Some(generation) = self
            .settings_page
            .update_background_blur_preview(persisted_background_blur, value)
        else {
            return;
        };
        cx.notify();

        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(150)).await;
            let _ = weak.update(cx, |this, cx| {
                this.commit_background_blur_preview(generation, cx);
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn commit_background_blur_preview(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(value) = self.settings_page.take_background_blur_preview(generation) else {
            return;
        };
        if self.settings_store.settings().terminal.background_blur != value {
            self.edit_settings(|settings| settings.terminal.background_blur = value, cx);
        } else {
            cx.notify();
        }
    }
}

pub(in crate::workspace) fn select_anchor_tracks_while_closed(anchor_id: SelectAnchorId) -> bool {
    // Browser/Radix selects can synchronously read their trigger rect on the
    // opening click. GPUI portals cannot, so modal select triggers keep a
    // closed-state anchor cache without notifying; that makes first-click open
    // immediate while scroll handlers still clear stale coordinates.
    if anchor_id.is_settings_select_trigger()
        || anchor_id.is_new_connection_select_trigger()
        || anchor_id.is_cloud_sync_select_trigger()
    {
        return true;
    }

    // Sliders and non-settings overlays also need an anchor before pointer-down
    // can open or drag them.
    matches!(
        anchor_id,
        SelectAnchorId::SettingsAppearanceUiFontSizeSlider
            | SelectAnchorId::SettingsAppearanceBorderRadiusSlider
            | SelectAnchorId::VersionMigrationBorderRadiusSlider
            | SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider
            | SelectAnchorId::SettingsAppearanceBackgroundBlurSlider
            | SelectAnchorId::SettingsTerminalFontSizeSlider
            | SelectAnchorId::AiPanelRoot
            | SelectAnchorId::AiConversationList
            | SelectAnchorId::AiChatMenu
            | SelectAnchorId::AiModelSelector
            | SelectAnchorId::AiInlineModelSelector
            | SelectAnchorId::AiSafetyMenu
            | SelectAnchorId::AiContextPopover
            | SelectAnchorId::NewConnectionGroup
            | SelectAnchorId::NewConnectionKeyAuthSource
            | SelectAnchorId::NewConnectionManagedKey
            | SelectAnchorId::NewConnectionJumpSavedConnection
            | SelectAnchorId::NewConnectionJumpKeyAuthSource
            | SelectAnchorId::NewConnectionJumpManagedKey
            | SelectAnchorId::NewConnectionSerialPort
            | SelectAnchorId::NewConnectionSerialDataBits
            | SelectAnchorId::NewConnectionSerialStopBits
            | SelectAnchorId::NewConnectionSerialParity
            | SelectAnchorId::NewConnectionSerialFlowControl
            | SelectAnchorId::IdeAgentStatus
            // Broadcast targets are rendered through the root backdrop, but
            // Tauri/Radix positions them from the trigger button. Keep the
            // closed trigger rect warm so the first pointer-down opens at the
            // command-bar/tabbar button even when the AI sidebar changes root width.
            | SelectAnchorId::TerminalBroadcastMenu
            // Quick Commands uses Tauri's `min(860px, calc(100% - 1.5rem))`
            // width against the command bar. Keep the bar rect warm so the
            // first open and later resizes can compute the same adaptive width.
            | SelectAnchorId::TerminalCommandBar
            | SelectAnchorId::TerminalCwdMenu
            | SelectAnchorId::TerminalGitBranchMenu
            | SelectAnchorId::TerminalProjectMenu
            | SelectAnchorId::TerminalCastSeekbar
            // Session Manager toolbar menus use window-anchored overlays, so
            // their trigger bounds must be cached before pointer-down.
            | SelectAnchorId::SessionManagerViewMode
            | SelectAnchorId::SessionManagerSort
            | SelectAnchorId::SessionManagerBatchMove
    )
}
