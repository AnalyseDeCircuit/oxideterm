use super::*;
use crate::{
    appearance_page::{
        APPEARANCE_BORDER_RADIUS_MAX, APPEARANCE_BORDER_RADIUS_MIN, APPEARANCE_UI_FONT_SIZE_MAX,
        APPEARANCE_UI_FONT_SIZE_MIN, AppearanceAction,
    },
    components::{SettingsWindowDispatch, SettingsWindowRequest},
    keybinding_page::{
        KeybindingOwner, KeybindingRecordingKeyAction, KeybindingRequest,
        KeybindingResetConfirmSnapshot,
    },
    keybindings::KeybindingSide,
    select_menu::{CoreSelectOwner, SelectMenuRequest},
    theme_editor::{ThemeEditorOwner, ThemeEditorRequest},
    theme_picker::SettingsThemePickerFrame,
};
use gpui::WeakEntity;
use oxideterm_gpui_ui::{motion::ExitPhase, slider::slider_pointer_percent};
use oxideterm_settings_model::{
    ThemeEditorSection, app_ui_colors_to_colors, editor_terminal_theme, terminal_theme_to_colors,
};

pub(super) fn ai_providers_view(
    owner: &Entity<PageState>,
    picker: &Entity<SettingsThemePicker>,
    cx: &mut Context<PortableSettingsSurface>,
) -> Entity<crate::ai::providers::AiProviderSettingsView> {
    use crate::ai::{
        confirm::AiSettingsConfirmIntent,
        providers::{AiProviderBindings, AiProviderRequest, AiProviderSettingsView},
    };
    let controls_owner = owner.downgrade();
    let controls_picker = picker.downgrade();
    let status_owner = owner.downgrade();
    let req_owner = owner.downgrade();
    cx.new(|cx| {
        let view_weak: WeakEntity<AiProviderSettingsView> = cx.weak_entity();
        AiProviderSettingsView::new(AiProviderBindings {
            controls: Rc::new(move |cx| {
                let entity = controls_owner.upgrade()?;
                let state = entity.read(cx);
                Some(state.controls(
                    &state.tokens_cache,
                    window_dispatch(controls_owner.clone(), controls_picker.clone()),
                ))
            }),
            status: Rc::new(move |id, cx| {
                let has_key = status_owner
                    .upgrade()
                    .is_some_and(|s| s.read(cx).ai_api_keys.contains_key(id));
                (has_key, false)
            }),
            request: Rc::new(move |req, cx| {
                let view_weak = view_weak.clone();
                let _ = req_owner.update(cx, |s, cx| match req {
                    AiProviderRequest::Edit(edit) => {
                        edit(&mut s.settings);
                        s.changed(cx);
                    }
                    AiProviderRequest::SaveKey { index, provider_id } => {
                        if s.fields.active == Some(SettingsInput::AiProviderApiKey(index)) {
                            s.fields.blur();
                        }
                        if let Some(key) = s.ai_api_key_drafts.remove(&index)
                            && !key.trim().is_empty()
                        {
                            s.ai_api_keys.insert(provider_id, key);
                            s.changed(cx);
                        }
                    }
                    AiProviderRequest::ClearModelDraft(SettingsInput::AiProviderNewModel(
                        index,
                    )) => {
                        if s.fields.active == Some(SettingsInput::AiProviderNewModel(index)) {
                            s.fields.blur();
                        }
                        s.ai_new_model_drafts.remove(&index);
                        cx.notify();
                    }
                    AiProviderRequest::Confirm(intent, _) => match intent {
                        AiSettingsConfirmIntent::Enable => {
                            crate::options::set_ai_enabled(&mut s.settings, true);
                            s.changed(cx);
                        }
                        AiSettingsConfirmIntent::RemoveProviderKey { provider_id } => {
                            s.ai_api_keys.remove(&provider_id);
                            s.changed(cx);
                        }
                        AiSettingsConfirmIntent::RemoveProvider { provider_id } => {
                            s.ai_api_keys.remove(&provider_id);
                            if let Some(index) = s.settings.ai.providers.iter().position(|p| {
                                oxideterm_ai::provider_view(p)
                                    .is_some_and(|view| view.id == provider_id)
                            }) {
                                oxideterm_ai::remove_provider_at(
                                    &mut s.settings.ai.providers,
                                    &mut s.settings.ai.active_provider_id,
                                    &mut s.settings.ai.active_model,
                                    index,
                                );
                            }
                            let _ =
                                view_weak.update(cx, |v, cx| v.remove_provider(&provider_id, cx));
                            s.changed(cx);
                        }
                    },
                    AiProviderRequest::RefreshModels(_)
                    | AiProviderRequest::LoadKeyStatus(_)
                    | AiProviderRequest::ClearModelDraft(_) => {}
                });
            }),
        })
    })
}

pub(super) fn theme_picker(
    owner: &Entity<PageState>,
    cx: &mut Context<PortableSettingsSurface>,
) -> Entity<SettingsThemePicker> {
    let frame = owner.downgrade();
    let apply = owner.downgrade();
    cx.new(|_| {
        SettingsThemePicker::for_surface(
            Rc::new(move |window, cx| {
                let state = frame.upgrade()?.read(cx);
                let select = state.select?;
                let anchor = state.anchors.get(&select.anchor_id())?;
                Some(SettingsThemePickerFrame {
                    select,
                    width: f32::from(anchor.bounds.size.width),
                    i18n: state.i18n.clone(),
                    tokens: pages::tokens(&state.settings, system_dark(window)),
                })
            }),
            Rc::new(move |select, id, cx| {
                let _ = apply.update(cx, |state, cx| state.apply_theme(select, id, cx));
            }),
        )
    })
}

pub(super) fn window_dispatch(
    owner: WeakEntity<PageState>,
    picker: WeakEntity<SettingsThemePicker>,
) -> SettingsWindowDispatch {
    Rc::new(move |request, window, cx| {
        let _ = owner.update(cx, |s, cx| {
            use SettingsWindowRequest::*;
            match request {
                Edit(edit) => {
                    edit(&mut s.settings);
                    s.fields.blur();
                    s.changed(cx);
                }
                OpenSelect(select) => {
                    s.focus.focus(window, cx);
                    s.fields.blur();
                    s.select = (s.select != Some(select)).then_some(select);
                    if s.select.is_some() && select.theme_target().is_some() {
                        let _ = picker.update(cx, |p, cx| p.open(select, &s.settings, cx));
                    }
                    cx.notify();
                }
                SelectAnchor(anchor) | SliderAnchor(anchor) => {
                    let previous = s.anchors.insert(anchor.id, anchor);
                    if previous != Some(anchor)
                        && s.select.is_some_and(|id| id.anchor_id() == anchor.id)
                    {
                        cx.notify();
                    }
                }
                BeginInputSelection(id, event) => {
                    s.select = None;
                    s.focus.focus(window, cx);
                    s.focus_input(id);
                    s.fields
                        .select_at(event.position, event.click_count, event.modifiers.shift);
                    cx.notify();
                }
                DragInputSelection(event) if event.dragging() => {
                    s.fields.drag_to(event.position);
                    cx.notify();
                }
                DismissInput(id) if s.fields.active == Some(id) => {
                    s.fields.blur();
                    cx.notify();
                }
                BeginSlider(slider, x) => {
                    s.fields.blur();
                    s.slider = Some(slider);
                    s.move_slider(x, system_dark(window), cx);
                }
                MoveSlider(event) if event.dragging() => {
                    s.move_slider(f32::from(event.position.x), system_dark(window), cx)
                }
                FinishSlider => s.slider = None,
                ConfirmAiEnable => {
                    crate::options::set_ai_enabled(&mut s.settings, true);
                    s.changed(cx);
                }
                _ => {}
            }
        });
    })
}

impl PageState {
    fn move_slider(&mut self, x: f32, dark: bool, cx: &mut Context<Self>) {
        let Some(slider) = self.slider else { return };
        let thumb = pages::tokens(&self.settings, dark)
            .metrics
            .ui_slider_thumb_size;
        let (anchor, min, max, field) = match slider {
            SettingsSlider::TerminalFontSize => (
                SelectAnchorId::SettingsTerminalFontSizeSlider,
                8.0,
                32.0,
                &mut self.settings.terminal.font_size,
            ),
            SettingsSlider::AppearanceBorderRadius => (
                SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                APPEARANCE_BORDER_RADIUS_MIN,
                APPEARANCE_BORDER_RADIUS_MAX,
                &mut self.settings.appearance.border_radius,
            ),
            SettingsSlider::AppearanceUiFontSize => (
                SelectAnchorId::SettingsAppearanceUiFontSizeSlider,
                APPEARANCE_UI_FONT_SIZE_MIN,
                APPEARANCE_UI_FONT_SIZE_MAX,
                &mut self.settings.appearance.ui_font_size,
            ),
            _ => return,
        };
        let Some(anchor) = self.anchors.get(&anchor) else {
            return;
        };
        let position = slider_pointer_percent(
            x - f32::from(anchor.bounds.left()),
            f32::from(anchor.bounds.size.width).max(1.0),
            thumb,
        );
        let value = (min + (max - min) * position).round() as i64;
        if *field != value {
            *field = value;
            self.changed(cx);
        }
    }

    pub(super) fn menu_action(&mut self, request: SelectMenuRequest, cx: &mut Context<Self>) {
        match request {
            SelectMenuRequest::Close => self.select = None,
            SelectMenuRequest::Blur => self.fields.blur(),
            SelectMenuRequest::Focus(id) => {
                self.select = None;
                self.focus_input(id);
            }
            SelectMenuRequest::Edit(edit) => {
                edit(&mut self.settings);
                self.changed(cx);
            }
            SelectMenuRequest::ProxyAuth(mode) => {
                self.proxy_action(crate::proxy_page::ProxyRequest::Auth(mode), cx);
            }
            SelectMenuRequest::SftpPresentation(p) => {
                self.settings.sftp.presentation = p;
                self.changed(cx);
            }
            #[cfg(not(target_family = "wasm"))]
            _ => unreachable!("desktop selectors are not mounted"),
        }
        cx.notify();
    }

    fn apply_theme(&mut self, select: SettingsSelect, id: String, cx: &mut Context<Self>) {
        if let Some(target) = select.theme_target() {
            target.apply_for_scheme(&mut self.settings, select.theme_system_dark(), id);
            self.select = None;
            self.changed(cx);
        }
    }

    pub(super) fn appearance(
        &mut self,
        action: AppearanceAction,
        dark: bool,
        cx: &mut Context<Self>,
    ) {
        self.fields.blur();
        self.select = None;
        match action {
            AppearanceAction::Mode(scheme) => {
                oxideterm_settings_model::apply_appearance_mode(&mut self.settings, scheme);
                self.scheme = scheme.map(|s| s.is_dark());
            }
            AppearanceAction::Scheme(dark) => {
                self.scheme = Some(dark);
                cx.notify();
                return;
            }
            AppearanceAction::Assign(d) => oxideterm_settings_model::assign_current_appearance(
                &mut self.settings,
                oxideterm_settings::AppearanceScheme::from_dark(d),
                dark,
            ),
            AppearanceAction::Corner(style) => self.settings.appearance.corner_style = style,
            AppearanceAction::Icon(variant) => self.settings.appearance.app_icon = variant,
            AppearanceAction::ZenModeKeepTabbar(keep) => {
                self.settings.appearance.zen_mode_keep_tabbar = keep
            }
            AppearanceAction::New(target) | AppearanceAction::Edit(target) => {
                let scheme = self.edit_scheme(dark);
                let base = scheme.map_or_else(
                    || target.selected_id(&self.settings),
                    |d| target.scheme_id(&self.settings, d),
                );
                let id = matches!(action, AppearanceAction::Edit(_)).then(|| base.to_owned());
                self.editor = Some(crate::theme_editor::prepare_theme_editor_with_base(
                    &self.settings,
                    target,
                    base,
                    id,
                    scheme,
                    self.i18n.t("settings_view.custom_theme.new_theme_name"),
                ));
                self.editor_scroll = ScrollHandle::new();
                cx.notify();
                return;
            }
            _ => unreachable!("desktop appearance controls are not mounted"),
        }
        self.changed(cx);
    }

    pub(super) fn editor_action(&mut self, request: ThemeEditorRequest, cx: &mut Context<Self>) {
        self.select = None;
        match request {
            ThemeEditorRequest::Focus(id) => self.focus_input(id),
            ThemeEditorRequest::ChangeSection => self.fields.blur(),
            ThemeEditorRequest::Close => {
                self.fields.blur();
                self.editor = None;
            }
            ThemeEditorRequest::Save => {
                if let Some(editor) = self.editor.as_ref()
                    && oxideterm_settings_model::save_theme_editor_snapshot_to_settings(
                        &mut self.settings,
                        editor,
                    )
                    .is_some()
                {
                    self.fields.blur();
                    self.editor = None;
                    self.changed(cx);
                }
            }
            ThemeEditorRequest::Delete => {
                if let Some(editor) = self.editor.take()
                    && let Some(id) = editor.edit_theme_id
                {
                    oxideterm_settings_model::delete_custom_theme_from_settings(
                        &mut self.settings,
                        &id,
                        &editor.duplicate_theme,
                    );
                    self.fields.blur();
                    self.changed(cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn shortcut(&mut self, request: KeybindingRequest, cx: &mut Context<Self>) {
        use serde_json::Value;
        let side = KeybindingSide::current();
        let (id, new) = match request {
            KeybindingRequest::Reset(id) => (id, None),
            KeybindingRequest::Unbind(id) => (id, Some(Value::Null)),
            KeybindingRequest::CommitRecording => {
                let Some(commit) = self.shortcuts.take_keybinding_recording_commit(cx) else {
                    return;
                };
                let Some(action) = self.catalog.iter().find(|a| a.id == commit.action_id) else {
                    return;
                };
                let value = (&commit.combo != action.default_combo(side))
                    .then(|| serde_json::json!(commit.combo));
                (commit.action_id, value)
            }
            KeybindingRequest::CloseReset { apply } => {
                self.reset_open = false;
                if apply {
                    self.settings.keybindings.overrides.clear();
                    self.changed(cx);
                }
                cx.notify();
                return;
            }
            KeybindingRequest::Import | KeybindingRequest::Export => {
                unreachable!("file actions are hidden")
            }
        };
        let overrides = &mut self.settings.keybindings.overrides;
        if let Some(value) = new {
            let mut entry = overrides
                .remove(&id)
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            entry.insert(side.json_key().into(), value);
            overrides.insert(id, Value::Object(entry));
        } else if let Some(entry) = overrides.get_mut(&id).and_then(Value::as_object_mut) {
            entry.remove(side.json_key());
            if entry.is_empty() {
                overrides.remove(&id);
            }
        }
        self.shortcuts.stop_keybinding_recording(cx);
        self.changed(cx);
    }

    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        picker: &Entity<SettingsThemePicker>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if self.document.is_some() {
            if key == "escape" {
                self.document = None;
                cx.notify();
            } else {
                return;
            }
        } else if let Some(select) = self.select {
            if key == "escape" {
                self.select = None;
                cx.notify();
            } else if select.theme_target().is_some() {
                if key == "enter" {
                    if let Some(id) = picker.read(cx).preview().map(str::to_owned) {
                        self.apply_theme(select, id, cx);
                    }
                } else {
                    picker.update(cx, |p, cx| p.navigate(key, cx));
                }
            }
        } else if self.reset_open {
            if key == "escape" {
                self.shortcut(KeybindingRequest::CloseReset { apply: false }, cx);
            }
        } else if self.shortcuts.keybinding_recording_action_id().is_some() {
            self.fields.blur();
            if matches!(
                self.shortcuts.handle_keybinding_recording_key(
                    event,
                    &self.settings.keybindings.overrides,
                    &self.catalog,
                    cx
                ),
                Some(KeybindingRecordingKeyAction::Confirm)
            ) {
                self.shortcut(KeybindingRequest::CommitRecording, cx);
            }
        } else if self.fields.active.is_some() {
            if !self.field_key(event, window, cx) {
                return;
            }
        } else if self.editor.is_some() && key == "escape" {
            self.editor_action(ThemeEditorRequest::Close, cx);
        } else {
            return;
        }
        cx.stop_propagation();
    }
}

impl CoreSelectOwner for PageState {
    fn custom_cjk(&self, ide: bool) -> bool {
        if ide {
            self.ide_custom
        } else {
            self.terminal_custom
        }
    }
    fn set_custom_cjk(&mut self, ide: bool, custom: bool) {
        if ide {
            self.ide_custom = custom
        } else {
            self.terminal_custom = custom
        }
    }
    fn duplicate_id(&self) -> Option<&str> {
        self.editor.as_ref().map(|e| e.duplicate_theme.as_str())
    }
    fn duplicate(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            let theme = oxideterm_theme::theme_by_id(id);
            editor.duplicate_theme = theme.id.into();
            editor.duplicate_theme_touched = true;
            editor.terminal_colors = terminal_theme_to_colors(theme.terminal);
            editor.ui_colors = app_ui_colors_to_colors(ThemeTokens::from_builtin(theme).ui);
            cx.notify();
        }
    }
}

impl ThemeEditorOwner for PageState {
    fn settings_entity_input_value(&self, id: SettingsInput) -> Option<&str> {
        self.fields.value(id)
    }
    fn settings_entity_focused_input(&self) -> Option<SettingsInput> {
        self.fields.active
    }
    fn select_theme_editor_section(&mut self, section: ThemeEditorSection, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            editor.active_section = section;
            cx.notify();
        }
    }
    fn derive_theme_editor_ui_colors(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            editor.ui_colors =
                app_ui_colors_to_colors(oxideterm_theme::derive_ui_colors_from_terminal(
                    editor_terminal_theme(&editor.terminal_colors),
                ));
            cx.notify();
        }
    }
}

impl KeybindingOwner for PageState {
    const FILE_ACTIONS: bool = false;
    fn shortcuts(&self) -> &KeybindingState {
        &self.shortcuts
    }
    fn shortcuts_mut(&mut self) -> &mut KeybindingState {
        self.fields.blur();
        &mut self.shortcuts
    }
    fn open_keybinding_reset_confirm(&mut self, cx: &mut Context<Self>) {
        self.fields.blur();
        self.reset_open = true;
        cx.notify();
    }
    fn keybinding_reset_confirm_snapshot(&self) -> Option<KeybindingResetConfirmSnapshot> {
        self.reset_open.then_some(KeybindingResetConfirmSnapshot {
            phase: ExitPhase::Visible,
            focused_action: None,
        })
    }
}
