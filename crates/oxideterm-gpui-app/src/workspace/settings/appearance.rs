use super::*;
use oxideterm_gpui_settings_view::appearance_page::{
    AppearanceAction, AppearancePageData, render_appearance_slider,
};

impl WorkspaceApp {
    pub(super) fn handle_appearance_request(
        &mut self,
        action: AppearanceAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            AppearanceAction::Mode(scheme) => {
                self.close_settings_select();
                self.blur_text_inputs(cx);
                self.finish_settings_slider_drag(cx);
                self.settings_workspace
                    .update(cx, |settings, cx| settings.finish_background_blur_drag(cx));
                self.change_appearance_mode(scheme, cx);
                cx.stop_propagation();
            }
            AppearanceAction::Scheme(dark) => {
                self.blur_text_inputs(cx);
                self.finish_settings_slider_drag(cx);
                self.settings_workspace.update(cx, |settings, cx| {
                    settings.appearance_edit_dark = Some(dark);
                    cx.notify();
                });
                cx.stop_propagation();
            }
            AppearanceAction::Assign(selected) => {
                let scheme = oxideterm_settings::AppearanceScheme::from_dark(selected);
                let system_dark = self.system_dark;
                self.edit_settings(
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
                self.settings_workspace.update(cx, |s, cx| {
                    s.pick_theme_import(
                        target,
                        &self.i18n,
                        self.forwarding_runtime.handle().clone(),
                        cx,
                    );
                });
                cx.stop_propagation();
            }
            AppearanceAction::Edit(target) => {
                let dark = self.appearance_edit_scheme(cx).unwrap_or(self.system_dark);
                let id = target
                    .scheme_id(self.settings_store.settings(), dark)
                    .to_string();
                self.open_theme_editor(target, Some(id), cx);
                cx.stop_propagation();
            }
            AppearanceAction::New(target) => {
                self.open_theme_editor(target, None, cx);
                cx.stop_propagation();
            }
            AppearanceAction::Corner(style) => {
                self.close_settings_select();
                self.blur_text_inputs(cx);
                self.finish_settings_slider_drag(cx);
                self.edit_settings(|settings| settings.appearance.corner_style = style, cx);
                cx.stop_propagation();
            }
            AppearanceAction::Icon(variant) => {
                self.edit_settings(|settings| settings.appearance.app_icon = variant, cx);
                cx.stop_propagation();
            }
            AppearanceAction::Titlebar(checked) => {
                self.edit_settings(
                    |settings| settings.appearance.show_window_titlebar = !checked,
                    cx,
                );
                // Detached windows read the same workspace settings but
                // need an explicit repaint after their chrome changes.
                cx.refresh_windows();
            }
        }
    }

    pub(super) fn appearance_page_data(&self, cx: &App) -> AppearancePageData {
        AppearancePageData {
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
        }
    }

    pub(super) fn handle_background_request(
        &mut self,
        request: oxideterm_gpui_settings_view::background_page::BackgroundRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_settings_view::background_page::{
            BackgroundGalleryEdit, BackgroundRequest::*,
        };
        let operation = match request {
            Marketplace => return self.open_background_plugin_marketplace(window, cx),
            Pick => BackgroundGalleryEdit::Pick,
            Clear => BackgroundGalleryEdit::Clear,
            Remove(path) => BackgroundGalleryEdit::Remove(path),
        };
        self.settings_workspace.update(cx, |s, cx| {
            s.edit_background_gallery(
                operation,
                self.settings_store.settings(),
                self.settings_store.path(),
                self.forwarding_runtime.handle().clone(),
                is_bundled_workspace_background,
                &self.i18n,
                cx,
            )
        });
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

    pub(in crate::workspace) fn theme_editor_bindings(
        cx: &Context<Self>,
    ) -> oxideterm_gpui_settings_view::theme_editor::ThemeEditorBindings {
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
                        open_select: app.open_settings_select,
                        select_origin: app.settings_select_focus_origin,
                    })
                }
            }),
            ime: Rc::new({
                let owner = owner.clone();
                move |input, value, focused, cx| {
                    Some(
                        owner
                            .upgrade()?
                            .read(cx)
                            .settings_ime_frame(input, value, focused, cx),
                    )
                }
            }),
            window: Self::settings_window_dispatch_for(owner.clone()),
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
        bindings
    }
    pub(in crate::workspace) fn open_theme_editor(
        &mut self,
        target: ThemeTarget,
        edit_theme_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let editor = oxideterm_gpui_settings_view::theme_editor::prepare_theme_editor(
            self.settings_store.settings(),
            target,
            edit_theme_id,
            self.appearance_edit_scheme(cx),
            self.system_dark,
            self.i18n.t("settings_view.custom_theme.new_theme_name"),
        );
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
}
