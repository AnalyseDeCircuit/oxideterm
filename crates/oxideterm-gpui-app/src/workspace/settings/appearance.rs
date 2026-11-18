use super::*;
use oxideterm_gpui_settings_view::appearance_page::{
    AppearanceAction, AppearancePage, render_appearance_slider,
};
use oxideterm_gpui_settings_view::theme_editor::ThemeEditorDialog;
use oxideterm_gpui_settings_view::theme_picker::appearance_theme_palette;
use oxideterm_settings::BackgroundStyle;

pub(in crate::workspace) const THEME_EDITOR_INPUT_HEIGHT: f32 = 32.0; // Tauri Input h-8.
pub(in crate::workspace) const THEME_EDITOR_DUPLICATE_WIDTH: f32 = 180.0; // Tauri duplicate select w-[180px].

pub(super) use oxideterm_gpui_settings_view::appearance_page::window_opacity_from_slider_position;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_appearance_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section {
            SettingsSectionId::AppearanceTheme => self.appearance_theme_card(settings, cx),
            SettingsSectionId::AppearanceLayout => self.appearance_layout_card(settings, cx),
            SettingsSectionId::AppearanceEffects => self.appearance_effects_card(settings, cx),
            SettingsSectionId::AppearanceBackground => {
                self.appearance_background_card(&self.background_settings_for_controls(cx), cx)
            }
            SettingsSectionId::AppearanceAppIcon => self.appearance_app_icon_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn appearance_theme_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = self.appearance_theme_controls(settings, cx);
        rows.push(self.appearance_theme_preview(settings, cx));
        self.appearance_card(self.i18n.t("settings_view.appearance.theme"), None, rows)
    }

    pub(in crate::workspace) fn appearance_theme_controls(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.appearance_theme_controls_with_hints(settings, true, cx)
    }

    fn with_appearance_page<R>(
        &self,
        cx: &Context<Self>,
        render: impl FnOnce(AppearancePage<'_>, &App) -> R,
    ) -> R {
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |action, cx: &mut App| {
            let _ = owner.update(cx, |this, cx| match action {
                AppearanceAction::Mode(scheme) => {
                    this.close_settings_select();
                    this.blur_text_inputs(cx);
                    this.finish_settings_slider_drag(cx);
                    this.settings_workspace
                        .update(cx, |settings, cx| settings.finish_background_blur_drag(cx));
                    this.change_appearance_mode(scheme, cx);
                    cx.stop_propagation();
                }
                AppearanceAction::Scheme(dark) => {
                    this.blur_text_inputs(cx);
                    this.finish_settings_slider_drag(cx);
                    this.settings_workspace.update(cx, |settings, cx| {
                        settings.appearance_edit_dark = Some(dark);
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
                AppearanceAction::Assign(selected) => {
                    let scheme = oxideterm_settings::AppearanceScheme::from_dark(selected);
                    let system_dark = this.system_dark;
                    this.edit_settings(
                        |settings| {
                            oxideterm_settings_model::assign_current_appearance(
                                settings,
                                scheme,
                                system_dark,
                            )
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }
                AppearanceAction::Import(target) => {
                    this.import_theme_from_file(target, cx);
                    cx.stop_propagation();
                }
                AppearanceAction::Edit(target) => {
                    let dark = this.appearance_edit_scheme(cx).unwrap_or(this.system_dark);
                    let id = target
                        .scheme_id(this.settings_store.settings(), dark)
                        .to_string();
                    this.open_theme_editor(target, Some(id), cx);
                    cx.stop_propagation();
                }
                AppearanceAction::New(target) => {
                    this.open_theme_editor(target, None, cx);
                    cx.stop_propagation();
                }
                AppearanceAction::Corner(style) => {
                    this.close_settings_select();
                    this.blur_text_inputs(cx);
                    this.finish_settings_slider_drag(cx);
                    this.edit_settings(|settings| settings.appearance.corner_style = style, cx);
                    cx.stop_propagation();
                }
                AppearanceAction::Icon(variant) => {
                    this.edit_settings(|settings| settings.appearance.app_icon = variant, cx);
                    cx.stop_propagation();
                }
                AppearanceAction::Titlebar(checked) => {
                    this.edit_settings(
                        |settings| settings.appearance.show_window_titlebar = !checked,
                        cx,
                    );
                    // Detached windows read the same workspace settings but
                    // need an explicit repaint after their chrome changes.
                    cx.refresh_windows();
                }
            });
        });
        self.with_settings_controls(cx, |controls, cx| {
            render(
                AppearancePage {
                    controls,
                    system_dark: self.system_dark,
                    edit_scheme: self.appearance_edit_scheme(cx),
                    onboarding: self.onboarding.open,
                    render_policy: self.render_policy,
                    vibrancy_mode: effective_vibrancy_mode(
                        self.settings_store.settings(),
                        &self.render_policy,
                    ),
                    vibrancy_support: self.vibrancy_support,
                    icon_variants: crate::app_icon::APP_ICON_VARIANTS,
                    icon_size: crate::app_icon::APP_ICON_PREVIEW_SIZE,
                    icon_source: crate::app_icon::app_icon_preview_source,
                    dispatch,
                },
                cx,
            )
        })
    }
    pub(in crate::workspace) fn appearance_theme_controls_with_hints(
        &self,
        settings: &PersistedSettings,
        show_hints: bool,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        self.with_appearance_page(cx, |page, cx| {
            page.appearance_theme_controls_with_hints(settings, show_hints, cx)
        })
    }
    fn appearance_layout_card(
        &self,
        settings: &PersistedSettings,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_appearance_page(cx, |page, cx| page.appearance_layout_card(settings, cx))
    }
    fn appearance_effects_card(
        &self,
        settings: &PersistedSettings,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_appearance_page(cx, |page, cx| page.appearance_effects_card(settings, cx))
    }
    fn appearance_app_icon_card(
        &self,
        settings: &PersistedSettings,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.with_appearance_page(cx, |page, cx| page.appearance_app_icon_card(settings, cx))
    }
    pub(in crate::workspace) fn appearance_background_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use oxideterm_gpui_settings_view::background_page::{BackgroundPage, BackgroundRequest};

        let preview = if self.settings_workspace.read(cx).theme_editor().is_none() {
            let preview = self.settings_workspace.read(cx).background_preview.clone();
            Some(self.appearance_preview(self.settings_store.settings(), preview, true, cx))
        } else {
            None
        };
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new({
            let owner = owner.clone();
            move |request, window: &mut Window, cx: &mut App| {
                let _ = owner.update(cx, |app, cx| match request {
                    BackgroundRequest::Marketplace => {
                        app.open_background_plugin_marketplace(window, cx)
                    }
                    BackgroundRequest::Pick => app.pick_background_image(cx),
                    BackgroundRequest::Clear => app.clear_background_image_gallery(cx),
                    BackgroundRequest::Remove(path) => {
                        app.remove_background_image_from_gallery(path, cx)
                    }
                });
            }
        });
        let persist = Rc::new(
            move |edit: Box<dyn FnOnce(&mut BackgroundStyle)>, cx: &mut App| {
                let _ = owner.update(cx, |app, cx| app.edit_background_style(edit, cx));
            },
        );
        let is_built_in =
            |path: &Path| is_bundled_workspace_background(self.settings_store.path(), path);
        let animated_media = self.settings_workspace.update(cx, |entity, cx| {
            entity.background_media_is_animated(settings.terminal.background_image.as_deref(), cx)
        });
        self.with_appearance_page(cx, |appearance, cx| {
            BackgroundPage {
                appearance,
                state: &self.settings_workspace,
                preview,
                animated_media,
                is_built_in: &is_built_in,
                dispatch,
                persist,
            }
            .render(settings, cx)
        })
    }

    pub(in crate::workspace) fn appearance_card(
        &self,
        title: String,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            settings_appearance_card_header(&self.tokens, title, None, actions),
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_card_shell(
        &self,
        header: AnyElement,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        settings_appearance_card_shell(
            &self.tokens,
            self.settings_background_active(),
            header,
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_slider_control(
        &self,
        slider_id: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        render_appearance_slider(
            &self.tokens,
            slider_id,
            anchor_id,
            min,
            max,
            value,
            Self::settings_window_dispatch(cx),
        )
    }

    pub(in crate::workspace) fn appearance_theme_preview(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_theme_preview_with_hint(settings, true, cx)
    }

    pub(in crate::workspace) fn appearance_theme_preview_compact(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_theme_preview_with_hint(settings, false, cx)
    }

    fn appearance_theme_preview_with_hint(
        &self,
        settings: &PersistedSettings,
        show_hint: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preview = self
            .settings_workspace
            .read(cx)
            .theme_background_preview
            .clone();
        self.appearance_preview(settings, preview, show_hint, cx)
    }

    fn appearance_preview(
        &self,
        settings: &PersistedSettings,
        preview: Entity<oxideterm_gpui_background::BackgroundPreview>,
        show_hint: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let preview_target = self
            .open_settings_select
            .and_then(SettingsSelect::theme_target);
        let preview_dark = self
            .open_settings_select
            .and_then(SettingsSelect::theme_system_dark)
            .unwrap_or(self.appearance_edit_scheme(cx).unwrap_or(self.system_dark));
        let preview_theme = preview_target
            .and_then(|_| self.settings_theme_picker.read(cx).preview())
            .map(str::to_owned);
        let preview_id = |target: ThemeTarget| {
            if preview_target == Some(target) {
                preview_theme
                    .as_deref()
                    .unwrap_or_else(|| target.scheme_id(settings, preview_dark))
            } else {
                target.scheme_id(settings, preview_dark)
            }
        };
        let application_id = preview_id(ThemeTarget::Application);
        let terminal_id = preview_id(ThemeTarget::Terminal);
        // Resolve both halves locally so hovering never applies draft colors to the workspace.
        let mut preview_tokens = ThemeTokens {
            ui: oxideterm_settings_model::theme_ui_colors(settings, application_id),
            terminal: appearance_theme_palette(settings, terminal_id),
            ..self.tokens
        };
        preview_tokens.refresh_palette_metrics();
        let page = self.settings_workspace.read(cx).theme_preview_page;
        let mut style = settings.terminal.background_for_scheme(Some(preview_dark));
        if let Some(blur) = self
            .settings_workspace
            .read(cx)
            .background_blur_preview(self.appearance_edit_scheme(cx))
        {
            style.blur = blur;
        }
        let page_key = match page {
            ThemePreviewPage::Terminal => "terminal",
            ThemePreviewPage::Sftp => "sftp",
            _ => "session_manager",
        };
        let background = if style.scope == BackgroundScope::Window
            || style.enabled_tabs.iter().any(|tab| tab == page_key)
        {
            background_preferences_for_style(&style, &preview_tokens)
        } else {
            None
        };
        let has_background = background.is_some();
        preview.update(cx, |preview, cx| preview.set_preferences(background, cx));
        div()
            .id(("appearance-preview", preview.entity_id()))
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(settings_appearance_theme_preview(
                &preview_tokens,
                settings,
                custom_theme_display_name(settings, application_id),
                custom_theme_display_name(settings, terminal_id),
                oxideterm_gpui_settings_view::application_theme_description(
                    application_id,
                    preview_tokens.ui,
                    &self.i18n,
                ),
                &self.i18n,
                page,
                has_background.then(|| preview.clone().into_any_element()),
                style.scope,
                {
                    let workspace = self.settings_workspace.downgrade();
                    move |page, _window, cx| {
                        let _ = workspace.update(cx, |settings, cx| {
                            settings.theme_preview_page = page;
                            cx.notify();
                        });
                    }
                },
            ))
            .when(style.day_cycle && has_background, |view| {
                view.child(self.appearance_daylight_preview_control(preview.clone(), cx))
            })
            .when(show_hint, |view| {
                view.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.appearance.theme_preview_hint")),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn mount_theme_editor(
        settings: &Entity<SettingsWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<ThemeEditorDialog> {
        use oxideterm_gpui_settings_view::theme_editor::{
            ThemeEditorBindings, ThemeEditorFrame, ThemeEditorRequest,
        };

        let owner = cx.entity().downgrade();
        let bindings = ThemeEditorBindings {
            frame: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    Some(ThemeEditorFrame {
                        tokens: app.tokens,
                        i18n: app.i18n.clone(),
                        mono_font: settings_mono_font_family(app.settings_store.settings()),
                        scroll: app.selectable_text_scroll_handle("theme-editor-scroll"),
                    })
                }
            }),
            field: Rc::new({
                let owner = owner.clone();
                move |input, value, placeholder, kind, cx| {
                    owner
                        .update(cx, |app, cx| {
                            app.theme_editor_text_input(input, value, placeholder, kind, cx)
                        })
                        .unwrap_or_else(|_| div().into_any_element())
                }
            }),
            duplicate: Rc::new({
                let owner = owner.clone();
                move |value, cx| {
                    owner
                        .update(cx, |app, cx| app.theme_editor_duplicate_select(value, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                }
            }),
            preview: Rc::new({
                let owner = owner.clone();
                move |editor, cx| {
                    owner
                        .update(cx, |app, cx| {
                            let terminal = editor_terminal_theme(&editor.terminal_colors);
                            let ui = editor_ui_colors(&editor.ui_colors);
                            app.theme_editor_preview(editor, terminal, ui, cx)
                        })
                        .unwrap_or_else(|_| div().into_any_element())
                }
            }),
            background: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    owner
                        .update(cx, |app, cx| {
                            app.appearance_background_card(
                                &app.background_settings_for_controls(cx),
                                cx,
                            )
                        })
                        .unwrap_or_else(|_| div().into_any_element())
                }
            }),
            dispatch: Rc::new(move |request, window, cx| {
                let _ = owner.update(cx, |app, cx| match request {
                    ThemeEditorRequest::Close => app.close_theme_editor(cx),
                    ThemeEditorRequest::Save => app.save_theme_editor(cx),
                    ThemeEditorRequest::Delete => app.delete_theme_editor_theme(cx),
                    ThemeEditorRequest::Focus(input) => {
                        app.focus_settings_input(input, String::new(), cx);
                        app.ime_marked_text = None;
                        window.focus(&app.focus_handle, cx);
                    }
                    ThemeEditorRequest::ChangeSection => {
                        app.blur_text_inputs(cx);
                        app.finish_settings_slider_drag(cx);
                        app.close_settings_select();
                    }
                });
            }),
        };
        cx.new(|cx| ThemeEditorDialog::new(settings, bindings, cx))
    }
    pub(in crate::workspace) fn theme_editor_duplicate_select(
        &self,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::CustomThemeDuplicate;
        self.settings_select_control_with_trigger_style(
            select_id,
            value,
            false,
            Some(THEME_EDITOR_DUPLICATE_WIDTH),
            |trigger| trigger.h(px(THEME_EDITOR_INPUT_HEIGHT)),
            cx,
        )
    }

    pub(in crate::workspace) fn theme_editor_preview(
        &self,
        editor: &ThemeEditorState,
        terminal: TerminalTheme,
        ui: AppUiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut tokens = self.tokens;
        tokens.ui = ui;
        tokens.terminal = terminal;
        tokens.refresh_palette_metrics();
        let background = background_preferences_for_style(&editor.background, &tokens);
        let preview = self
            .settings_workspace
            .read(cx)
            .editor_background_preview
            .clone();
        let has_background = background.is_some();
        preview.update(cx, |preview, cx| preview.set_preferences(background, cx));
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(settings_theme_editor_preview(
                &tokens,
                &editor.name,
                terminal,
                ui,
                settings_mono_font_family(self.settings_store.settings()),
                has_background.then(|| preview.clone().into_any_element()),
                editor.background.scope,
            ))
            .when(editor.background.day_cycle && has_background, |view| {
                view.child(self.appearance_daylight_preview_control(preview, cx))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_text_input(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        kind: ThemeEditorTextInputKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings_workspace = self.settings_workspace.read(cx);
        let focused = settings_workspace.settings_entity_focused_input() == Some(input);
        let display_value = settings_workspace
            .settings_entity_input_value(input)
            .unwrap_or(value);
        let target = WorkspaceImeTarget::Settings(input);
        let control = settings_theme_editor_text_input(
            &self.tokens,
            self.workspace_text_input(
                target,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            ),
            kind,
            settings_mono_font_family(self.settings_store.settings()),
        )
        .on_mouse_down_out(cx.listener(move |this, _event, _window, cx| {
            // Settings inputs are manually focused rather than native controls.
            // Release this editor when the next pointer press lands elsewhere.
            let is_focused = this
                .settings_workspace
                .read(cx)
                .settings_entity_focused_input()
                == Some(input);
            if is_focused {
                this.blur_text_inputs(cx);
            }
        }));
        self.text_input_with_workspace_ime(
            target,
            control,
            move |this, cx| this.focus_settings_input(input, String::new(), cx),
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn open_theme_editor(
        &mut self,
        target: ThemeTarget,
        edit_theme_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let settings = self.settings_store.settings();
        let scheme = self.appearance_edit_scheme(cx);
        let mut initial = settings.clone();
        target.apply(
            &mut initial,
            target
                .scheme_id(settings, scheme.unwrap_or(self.system_dark))
                .to_string(),
        );
        let mut editor = theme_editor_from_settings(
            &initial,
            target,
            edit_theme_id.clone(),
            self.i18n.t("settings_view.custom_theme.new_theme_name"),
        );
        editor.system_dark = scheme;
        editor.background = edit_theme_id
            .as_deref()
            .and_then(|id| settings.custom_themes.get(id))
            .and_then(|theme| theme.get("background"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_else(|| settings.terminal.background_for_scheme(scheme));
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_theme_editor(editor, cx);
        });
        self.close_settings_select();
        self.focused_settings_input = None;
    }

    pub(in crate::workspace) fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.focused_settings_input = None;
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.cancel_theme_editor(delay, cx);
        });
    }

    pub(in crate::workspace) fn save_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.focused_settings_input = None;
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.save_theme_editor(delay, cx);
        });
    }

    pub(in crate::workspace) fn delete_theme_editor_theme(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.focused_settings_input = None;
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.delete_theme_editor(delay, cx);
        });
    }

    pub(in crate::workspace) fn import_theme_from_file(
        &mut self,
        target: ThemeTarget,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.appearance.theme_import"),
            )),
        });
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths.into_iter().next()
        };
        let runtime = self.forwarding_runtime.handle().clone();
        let system_dark = self.appearance_edit_scheme(cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_theme_import(target, system_dark, selection, runtime, cx);
        });
    }

    pub(in crate::workspace) fn send_settings_notice(
        &self,
        title: String,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(in crate::workspace) fn pick_background_image(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.terminal.bg_select_title"),
            )),
        });
        let settings_path = self.settings_store.path().to_path_buf();
        let control_settings = self.background_settings_for_controls(cx);
        let system_dark = self.appearance_edit_scheme(cx);
        let current_path = control_settings
            .terminal
            .background_image
            .as_ref()
            .map(PathBuf::from);
        let runtime = self.forwarding_runtime.handle().clone();
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            Some(paths)
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_background_image_import(
                selection,
                settings_path,
                current_path,
                system_dark,
                runtime,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn remove_background_image_from_gallery(
        &mut self,
        image_path: String,
        cx: &mut Context<Self>,
    ) {
        let settings_path = self.settings_store.path().to_path_buf();
        let runtime = self.forwarding_runtime.handle().clone();
        let current_path = self
            .background_settings_for_controls(cx)
            .terminal
            .background_image;
        let system_dark = self.appearance_edit_scheme(cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.remove_background_image(
                settings_path,
                image_path,
                current_path,
                system_dark,
                runtime,
                is_bundled_workspace_background,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn clear_background_image_gallery(&mut self, cx: &mut Context<Self>) {
        let settings_path = self.settings_store.path().to_path_buf();
        let runtime = self.forwarding_runtime.handle().clone();
        let current_path = self
            .background_settings_for_controls(cx)
            .terminal
            .background_image;
        let system_dark = self.appearance_edit_scheme(cx);
        self.settings_workspace.update(cx, |settings, cx| {
            settings.clear_background_image_gallery(
                settings_path,
                current_path,
                system_dark,
                runtime,
                is_bundled_workspace_background,
                cx,
            );
        });
    }
}
