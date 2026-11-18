use oxideterm_gpui_settings_view::select_menu::{
    ExternalSelect, ExternalSelectInputs, KnowledgeSelectInputs, LocalShellOption, SelectMenuRequest, SettingsSelectMenu,
    settings_select_popover,
};
use super::*;
use oxideterm_gpui_settings_view::components::{SettingsWindowDispatch, SettingsWindowRequest};
use oxideterm_gpui_settings_view::theme_picker::{SettingsThemePickerFrame};

impl WorkspaceApp {
    pub(in crate::workspace) fn theme_picker_bindings(cx: &Context<Self>) -> (Rc<dyn Fn(&Window, &App) -> Option<SettingsThemePickerFrame>>, Rc<dyn Fn(SettingsSelect, String, &mut App)>) {
        let owner = cx.entity().downgrade();
        let frame = Rc::new({
            let owner = owner.clone();
            move |window: &Window, cx: &App| {
                let owner = owner.upgrade()?;
                let app = owner.read(cx);
                let select = app.open_settings_select?;
                select.theme_target()?;
                let window_id = window.window_handle().window_id();
                if app.open_settings_select_owner_window_id != Some(window_id) {
                    return None;
                }
                let anchor = app.settings_select_anchors.get(&(window_id, select.anchor_id()))?;
                Some(SettingsThemePickerFrame {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    width: f32::from(anchor.bounds.size.width).max(app.tokens.metrics.ui_select_min_width),
                    select,
                })
            }
        });
        let apply = Rc::new(move |select: SettingsSelect, id, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| {
                if app.open_settings_select == Some(select) && let Some(target) = select.theme_target() {
                    app.close_settings_select();
                    app.edit_settings(|settings| target.apply_for_scheme(settings, select.theme_system_dark(), id), cx);
                }
            });
        });
        (frame, apply)
    }

    pub(super) fn settings_window_dispatch(cx: &Context<Self>) -> SettingsWindowDispatch {
        Self::settings_window_dispatch_for(cx.entity().downgrade())
    }

    pub(super) fn settings_window_dispatch_for(owner: gpui::WeakEntity<Self>) -> SettingsWindowDispatch {
        Rc::new(move |request, window, cx| {
            let _ = owner.update(cx, |app, cx| match request {
                SettingsWindowRequest::Edit(edit) => app.edit_settings(edit, cx),
                SettingsWindowRequest::OpenSelect(select) => {
                    app.open_settings_select_from_pointer(select, window.window_handle().window_id(), cx);
                    cx.notify();
                }
                SettingsWindowRequest::SelectAnchor(anchor) => {
                    app.update_settings_select_anchor(window.window_handle().window_id(), anchor, cx);
                }
                SettingsWindowRequest::BeginInputSelection(input, event) => {
                    let current = app.current_settings_input_value(input, cx);
                    app.focus_settings_input(input, current, cx);
                    app.ime_marked_text = None;
                    window.focus(&app.focus_handle, cx);
                    app.begin_ime_selection_from_mouse_down(WorkspaceImeTarget::Settings(input), &event, window, cx);
                }
                SettingsWindowRequest::DragInputSelection(event) => {
                    app.update_ime_selection_drag_from_mouse_move(&event, window, cx);
                }
                SettingsWindowRequest::DismissInput(input) => {
                    if app.settings_workspace.read(cx).settings_entity_focused_input() == Some(input) {
                        app.blur_text_inputs(cx);
                    }
                }
                SettingsWindowRequest::SliderAnchor(anchor) => app.update_select_anchor(anchor, cx),
                SettingsWindowRequest::BeginSlider(slider, position) => {
                    app.close_settings_select();
                    app.focused_settings_input = None;
                    app.settings_workspace.update(cx, |settings, _| settings.set_slider_drag(Some(slider)));
                    app.apply_settings_slider_from_position(slider, position, cx);
                }
                SettingsWindowRequest::MoveSlider(event) => app.update_settings_slider_drag(&event, cx),
                SettingsWindowRequest::FinishSlider => app.finish_settings_slider_drag(cx),
                SettingsWindowRequest::PickSessionLogDirectory => app.pick_terminal_session_log_directory(cx),
            });
        })
    }

    pub(in crate::workspace) fn render_settings_select_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Select content follows the logical open state directly. Dropdowns
        // never retain a second render-only state for exit animation.
        let open_select = self.open_settings_select?;
        let window_id = window.window_handle().window_id();
        if self.open_settings_select_owner_window_id != Some(window_id) {
            return None;
        }
        let anchor = self
            .settings_select_anchors
            .get(&(window_id, open_select.anchor_id()))
            .copied()?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let popup = if open_select.theme_target().is_some() {
            self.settings_views.theme_picker.clone().into_any_element()
        } else {
            self.render_settings_select_popup(open_select, window_id, width, cx)?
        };

        let owner = cx.entity().downgrade();
        Some(settings_select_popover(&self.tokens, open_select, anchor, popup, move |window, cx| {
            let _ = owner.update(cx, |app, cx| {
                app.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
            });
        }))
    }

    fn render_settings_select_popup(
        &self,
        open_select: SettingsSelect,
        window_id: gpui::WindowId,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings = self.settings_store.settings();
        let knowledge_dialog_visible = self
            .ai_entity
            .read(cx)
            .knowledge_document_dialog_owned_by(window_id);
        let active_tab = if knowledge_dialog_visible {
            SettingsTab::Knowledge
        } else {
            self.settings_workspace.read(cx).route_snapshot().active_tab
        };

        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, window: &mut Window, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match request {
                SelectMenuRequest::Close => {
                    app.close_settings_select();
                    cx.notify();
                }
                SelectMenuRequest::Blur => app.blur_text_inputs(cx),
                SelectMenuRequest::Focus(input) => {
                    let value = app.current_settings_input_value(input, cx);
                    app.focus_settings_input(input, value, cx);
                    window.focus(&app.focus_handle, cx);
                }
                SelectMenuRequest::Edit(edit) => app.edit_settings(edit, cx),
                SelectMenuRequest::Background(edit) => app.edit_background_style(edit, cx),
                SelectMenuRequest::RemoteShell(mode) => app.remote_shell_integration_mode_changed(mode, cx),
                SelectMenuRequest::Trigger(clear_input, edit) => app.edit_terminal_trigger(clear_input, edit, cx),
                SelectMenuRequest::KnowledgeCollectionScope(scope) => {
                    app.ai_entity.update(cx, |entity, cx| {
                        entity.set_knowledge_collection_connection_id(scope);
                        cx.notify();
                    });
                }
                SelectMenuRequest::KnowledgeDocumentFormat(format) => {
                    app.ai_entity.update(cx, |entity, cx| {
                        entity.set_knowledge_document_format(format.to_owned());
                        cx.notify();
                    });
                }
                SelectMenuRequest::External(action) => match action {
                    ExternalSelect::MarketplaceSort(sort) => app.set_plugin_marketplace_sort(sort, cx),
                    ExternalSelect::ProviderType(provider_type) => {
                        app.ai_entity.update(cx, |ai, cx| ai.select_settings_provider_type(provider_type, cx));
                    }
                    ExternalSelect::McpTransport(transport) => {
                        app.ai_entity.update(cx, |ai, cx| ai.set_mcp_transport(transport, cx));
                    }
                    ExternalSelect::McpAuth(mode) => {
                        app.ai_entity.update(cx, |ai, cx| ai.set_mcp_auth_mode(mode, cx));
                    }
                },
                SelectMenuRequest::CloseSemantic => app.close_semantic_scheme_editor(cx),
                SelectMenuRequest::CloseHighlight => app.close_highlight_rule_editor(cx),
                SelectMenuRequest::Highlight(index, edit) => app.edit_settings(|settings| {
                    let _ = oxideterm_gpui_settings_view::highlight_page::HighlightEdit::Rule(index, edit).apply(settings);
                }, cx),
                SelectMenuRequest::HighlightPreset(rules) => app.edit_settings(|settings| {
                    let _ = oxideterm_gpui_settings_view::highlight_page::HighlightEdit::Preset(rules).apply(settings);
                }, cx),
                SelectMenuRequest::ProxyAuth(mode) => app.handle_proxy_request(
                    oxideterm_gpui_settings_view::proxy_page::ProxyRequest::Auth(mode), cx,
                ),
                SelectMenuRequest::SftpPresentation(preference) => {
                    app.edit_settings(|settings| settings.sftp.presentation = preference, cx);
                    if preference != oxideterm_settings::SftpPresentationPreference::Sidebar
                        && let Some(node_id) = app.embedded_sftp_node_id.clone()
                    {
                        app.close_embedded_sftp_for_node(&node_id, cx);
                    }
                }
            });
        });
        let shells = if matches!(open_select, SettingsSelect::LocalShell | SettingsSelect::LocalShellSemanticScheme(_)) {
            self.effective_local_shells_for_settings(settings).into_iter().map(|shell| LocalShellOption {
                id: shell.id, label: shell.label,
            }).collect()
        } else {
            Vec::new()
        };
        let commands = &self.terminal.read(cx).quick_commands.store.commands;
        let trigger_commands = if open_select == SettingsSelect::TerminalTriggerQuickCommand {
            commands.iter()
                .filter(|command| oxideterm_quick_commands::quick_command_can_run_non_interactively(command))
                .map(|command| (command.id.clone(), command.name.clone())).collect()
        } else {
            Vec::new()
        };
        let ai = self.ai_entity.read(cx);
        let select_menu = SettingsSelectMenu {
            settings,
            i18n: &self.i18n,
            tokens: &self.tokens,
            settings_workspace: &self.settings_workspace,
            external: ExternalSelectInputs {
                provider_type: ai.settings_new_provider_type(),
                mcp_transport: ai.mcp_transport(),
                mcp_auth: ai.mcp_auth_mode(),
            },
            knowledge: KnowledgeSelectInputs {
                connections: self.connection_store.connections(),
                collection_scope: ai.knowledge_new_collection_connection_id(),
                document_format: ai.knowledge_new_document_format(),
            },
            totp_credentials: self.connection_store.totp_credentials(),
            shells,
            first_command: commands.first().map(|command| command.id.clone()).unwrap_or_default(),
            trigger_commands,
            dispatch,
        };
        select_menu.render(active_tab, open_select, width, cx)
    }

    pub(in crate::workspace) fn open_settings_select_from_pointer(
        &mut self,
        select_id: SettingsSelect,
        owner_window_id: gpui::WindowId,
        cx: &mut Context<Self>,
    ) {
        // Browser select triggers opened by pointer do not show a focus-visible
        // ring. Keep the origin and open/toggle rule in one place so settings,
        // AI provider, and knowledge selects do not drift apart.
        self.clear_terminal_trigger_input_focus(cx);
        self.blur_highlight_input(oxideterm_gpui_settings_view::highlight_page::is_highlight_input, cx);
        self.focused_settings_input = None;
        self.ai_entity.update(cx, |entity, cx| {
            entity.blur_settings_input(cx);
        });
        if self.open_settings_select == Some(select_id)
            && self.open_settings_select_owner_window_id == Some(owner_window_id)
        {
            self.close_settings_select();
            return;
        }
        if select_id.theme_target().is_some() {
            let settings = self.settings_store.settings();
            self.settings_views.theme_picker.update(cx, |picker, cx| picker.open(select_id, settings, cx));
        }
        self.open_settings_select = Some(select_id);
        self.open_settings_select_owner_window_id = Some(owner_window_id);
        self.settings_select_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Pointer);
    }

    pub(in crate::workspace) fn handle_appearance_theme_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self
            .open_settings_select
            .and_then(SettingsSelect::theme_target)
        else {
            return false;
        };
        let system_dark = self
            .open_settings_select
            .and_then(SettingsSelect::theme_system_dark);
        if event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" | "tab" => {
                self.close_settings_select();
                cx.notify();
                true
            }
            "enter" | "space" | " " => {
                if let Some(id) = self.settings_views.theme_picker.read(cx).preview().map(str::to_owned) {
                    self.close_settings_select();
                    self.edit_settings(
                        |settings| target.apply_for_scheme(settings, system_dark, id),
                        cx,
                    );
                }
                true
            }
            "up" | "arrowup" | "down" | "arrowdown" | "home" | "end" => {
                self.settings_views.theme_picker.update(cx, |picker, cx| picker.navigate(&event.keystroke.key, cx));
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn select_setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self.settings_select_control(select_id, value, false, Some(width), cx);

        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn bool_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self.settings_components().toggle_control(
            label_key, checked, setter, Self::settings_window_dispatch(cx),
        );
        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Numeric rows use the same focus, IME, caret, and draft pipeline as
        // every other settings text field instead of simulating a click stepper.
        let control = self.number_input(
            input,
            value.to_string(),
            self.tokens.metrics.settings_number_input_width,
            cx,
        );
        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        control: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.render_selectable_text_scoped(
            "settings-row-label",
            label_key,
            self.i18n.t(label_key),
            self.tokens.ui.text,
            cx,
        );
        let hint = (!hint_key.is_empty()).then(|| {
            self.render_selectable_text_scoped(
                "settings-row-hint",
                hint_key,
                self.i18n.t(hint_key),
                self.tokens.ui.text_muted,
                cx,
            )
        });
        settings_row(&self.tokens, label, hint, control)
    }
}
