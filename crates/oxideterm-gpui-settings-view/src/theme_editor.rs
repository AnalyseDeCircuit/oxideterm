use crate::components::{
    SettingsImeFrame, SettingsWindowDispatch, SettingsWindowRequest,
    render_settings_select_control, render_settings_text_field,
};
use oxideterm_gpui_ui::{
    select::{BrowserFocusOrigin, browser_focus_visible, select_trigger_with_focus_visible},
    text_input::TextInputContentAlign,
};

use std::{ops::Deref, rc::Rc};

use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, ScrollHandle, SharedString,
    Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    modal::{dismissible_dialog_backdrop, rounded_shell_child_radius},
    motion::ExitPhase,
    scroll::ScrollableElement,
};
use oxideterm_i18n::I18n;
use oxideterm_settings_model::{
    TERMINAL_THEME_COLOR_FIELDS, ThemeColorField, ThemeEditorSection, ThemeEditorState,
    UI_THEME_COLOR_FIELDS, parse_color_hex,
};
use oxideterm_theme::ThemeTokens;

use crate::{SettingsInput, appearance::*, surface::nav::render_icon, theme_display_name};

const THEME_EDITOR_MODAL_WIDTH: f32 = 672.0; // Tauri ThemeEditorModal max-w-2xl.
const THEME_EDITOR_MODAL_MAX_HEIGHT: f32 = 760.0; // Tauri max-h-[85vh] on the default native window.
const THEME_EDITOR_HEADER_PADDING_X: f32 = 16.0; // DialogHeader px-4.
const THEME_EDITOR_HEADER_PADDING_Y: f32 = 12.0; // DialogHeader py-3.
const THEME_EDITOR_BODY_PADDING_X: f32 = 16.0; // Body px-4.
const THEME_EDITOR_BODY_PADDING_Y: f32 = 12.0; // Body py-3.
const THEME_EDITOR_BODY_GAP: f32 = 16.0; // Tauri space-y-4.

pub fn prepare_theme_editor(
    settings: &oxideterm_settings::PersistedSettings,
    target: oxideterm_settings_model::ThemeTarget,
    edit_theme_id: Option<String>,
    scheme: Option<bool>,
    system_dark: bool,
    default_name: String,
) -> ThemeEditorState {
    prepare_theme_editor_with_base(
        settings,
        target,
        target.scheme_id(settings, scheme.unwrap_or(system_dark)),
        edit_theme_id,
        scheme,
        default_name,
    )
}

pub(crate) fn prepare_theme_editor_with_base(
    settings: &oxideterm_settings::PersistedSettings,
    target: oxideterm_settings_model::ThemeTarget,
    base: &str,
    edit_theme_id: Option<String>,
    scheme: Option<bool>,
    default_name: String,
) -> ThemeEditorState {
    let mut initial = settings.clone();
    target.apply(&mut initial, base.to_owned());
    let mut editor = oxideterm_settings_model::theme_editor_from_settings(
        &initial,
        target,
        edit_theme_id.clone(),
        default_name,
    );
    editor.system_dark = scheme;
    editor.background = edit_theme_id
        .as_deref()
        .and_then(|id| settings.custom_themes.get(id))
        .and_then(|theme| theme.get("background"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_else(|| settings.terminal.background_for_scheme(scheme));
    editor
}

#[derive(Clone, Copy)]
pub enum ThemeEditorRequest {
    Close,
    Save,
    Delete,
    Focus(SettingsInput),
    ChangeSection,
}

pub fn is_theme_editor_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::CustomThemeName
            | SettingsInput::CustomThemeTerminalColor(_)
            | SettingsInput::CustomThemeUiColor(_)
    )
}

pub struct ThemeEditorFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub mono_font: SharedString,
    pub scroll: ScrollHandle,
    pub open_select: Option<crate::SettingsSelect>,
    pub select_origin: Option<BrowserFocusOrigin>,
}

pub struct ThemeEditorBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<ThemeEditorFrame>>,
    pub ime:
        Rc<dyn for<'a> Fn(SettingsInput, &'a str, bool, &'a App) -> Option<SettingsImeFrame<'a>>>,
    pub window: SettingsWindowDispatch,
    pub dispatch: Rc<dyn Fn(ThemeEditorRequest, &mut Window, &mut App)>,
}

#[cfg(not(target_family = "wasm"))]
use crate::entity::SettingsWorkspaceEntity;
#[cfg(not(target_family = "wasm"))]
use gpui::{Render, Subscription};

#[cfg(not(target_family = "wasm"))]
pub struct ThemeEditorDialog {
    settings: Entity<SettingsWorkspaceEntity>,
    bindings: ThemeEditorBindings,
    background: Rc<dyn Fn(&mut App) -> AnyElement>,
    _changed: Subscription,
}

#[cfg(not(target_family = "wasm"))]
impl ThemeEditorDialog {
    pub fn new(
        settings: &Entity<SettingsWorkspaceEntity>,
        bindings: ThemeEditorBindings,
        background: Rc<dyn Fn(&mut App) -> AnyElement>,
        cx: &mut Context<Self>,
    ) -> Self {
        let changed = cx.observe(settings, |_, settings, cx| {
            if settings.read(cx).theme_editor().is_some() {
                cx.notify();
            }
        });
        Self {
            settings: settings.clone(),
            bindings,
            background,
            _changed: changed,
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl Render for ThemeEditorDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.settings.read(cx);
        let Some(editor) = state.theme_editor_snapshot() else {
            return div().into_any_element();
        };
        let phase = state.theme_editor_phase();
        let Some(frame) = (self.bindings.frame)(cx) else {
            return div().into_any_element();
        };
        let body = ThemeEditorBody {
            settings: &self.settings,
            ime: &*self.bindings.ime,
            window: self.bindings.window.clone(),
            dispatch: self.bindings.dispatch.clone(),
            background: Some(&*self.background),
            preview: &|editor, frame, cx| {
                Some(
                    crate::appearance_page::AppearancePreview {
                        tokens: frame.tokens,
                        i18n: &frame.i18n,
                        state: &self.settings,
                        open_settings_select: None,
                        hovered_theme: None,
                    }
                    .render_editor(
                        editor,
                        oxideterm_settings_model::editor_terminal_theme(&editor.terminal_colors),
                        oxideterm_settings_model::editor_ui_colors(&editor.ui_colors),
                        frame.mono_font.clone(),
                        cx,
                    ),
                )
            },
            frame,
        };
        body.render_modal(&editor, phase, cx)
            .unwrap_or_else(|| div().into_any_element())
    }
}

pub(crate) trait ThemeEditorOwner: Sized + 'static {
    fn settings_entity_input_value(&self, input: SettingsInput) -> Option<&str>;
    fn settings_entity_focused_input(&self) -> Option<SettingsInput>;
    fn select_theme_editor_section(&mut self, section: ThemeEditorSection, cx: &mut Context<Self>);
    fn derive_theme_editor_ui_colors(&mut self, cx: &mut Context<Self>);
}

#[cfg(not(target_family = "wasm"))]
impl ThemeEditorOwner for SettingsWorkspaceEntity {
    fn settings_entity_input_value(&self, input: SettingsInput) -> Option<&str> {
        Self::settings_entity_input_value(self, input)
    }
    fn settings_entity_focused_input(&self) -> Option<SettingsInput> {
        Self::settings_entity_focused_input(self)
    }
    fn select_theme_editor_section(&mut self, section: ThemeEditorSection, cx: &mut Context<Self>) {
        Self::select_theme_editor_section(self, section, cx);
    }
    fn derive_theme_editor_ui_colors(&mut self, cx: &mut Context<Self>) {
        Self::derive_theme_editor_ui_colors(self, cx);
    }
}
pub(crate) struct ThemeEditorBody<'a, S: ThemeEditorOwner> {
    pub settings: &'a Entity<S>,
    pub ime:
        &'a dyn for<'b> Fn(SettingsInput, &'b str, bool, &'b App) -> Option<SettingsImeFrame<'b>>,
    pub window: SettingsWindowDispatch,
    pub dispatch: Rc<dyn Fn(ThemeEditorRequest, &mut Window, &mut App)>,
    pub background: Option<&'a dyn Fn(&mut App) -> AnyElement>,
    pub preview: &'a dyn Fn(&ThemeEditorState, &ThemeEditorFrame, &mut App) -> Option<AnyElement>,
    pub frame: ThemeEditorFrame,
}

impl<S: ThemeEditorOwner> Deref for ThemeEditorBody<'_, S> {
    type Target = ThemeEditorFrame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl<S: ThemeEditorOwner> ThemeEditorBody<'_, S> {
    fn action(
        &self,
        request: ThemeEditorRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<S> {
        let dispatch = self.dispatch.clone();
        move |_, window, cx| {
            dispatch(request, window, cx);
            cx.stop_propagation();
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(&self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                }
                cx.stop_propagation();
            },
        )
    }
    pub(crate) fn render_modal(
        &self,
        editor: &ThemeEditorState,
        editor_phase: ExitPhase,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let title_key = if editor.edit_theme_id.is_some() {
            "settings_view.custom_theme.edit_title"
        } else {
            "settings_view.custom_theme.create_title"
        };
        let save_disabled = editor.name.trim().is_empty();
        let form_visible = editor_phase == oxideterm_gpui_ui::motion::ExitPhase::Visible;

        let dialog = div()
            .w(px(THEME_EDITOR_MODAL_WIDTH))
            .max_h(px(THEME_EDITOR_MODAL_MAX_HEIGHT))
            .rounded(px(self.tokens.radii.md))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            // Header/footer/body each paint to the rounded shell edge. Keeping
            // the shell itself background-free avoids a second corner color
            // when GPUI clips overflow with a rectangular mask.
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    // Tauri's DialogContent clips this painted header through
                    // the modal radius; mirror that edge ownership in GPUI.
                    .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.description")),
                    ),
            )
            .child(
                div()
                    .id("theme-editor-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.frame.scroll)
                    .vertical_scrollbar(&self.frame.scroll)
                    .px(px(THEME_EDITOR_BODY_PADDING_X))
                    .py(px(THEME_EDITOR_BODY_PADDING_Y))
                    .bg(rgb(self.tokens.ui.bg_elevated))
                    .flex()
                    .flex_col()
                    .gap(px(THEME_EDITOR_BODY_GAP))
                    .child(self.theme_editor_name_duplicate_row(editor, cx))
                    .children((self.preview)(editor, &self.frame, cx))
                    .child(self.theme_editor_section_tabs(editor, cx))
                    .children(if editor.active_section == ThemeEditorSection::Background {
                        self.background.map(|render| render(cx))
                    } else {
                        Some(self.theme_editor_color_grid(editor, cx))
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_t_1()
                    .border_color(rgb(self.tokens.ui.border))
                    // The footer background sits on the shell edge, so it must
                    // own the bottom corners instead of relying on rectangular clipping.
                    .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .child(if editor.edit_theme_id.is_some() {
                        self.theme_editor_footer_button(
                            "lucide/trash-2.svg",
                            self.i18n.t("settings_view.custom_theme.delete"),
                            self.tokens.ui.error,
                            self.action(ThemeEditorRequest::Delete),
                        )
                        .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                // ThemeEditorModal uses normal shadcn Button
                                // footer actions in Tauri; route clicks through
                                // the shared workspace guard rather than a raw
                                // primitive-level mouse handler.
                                self.workspace_toolbar_action_button(
                                    self.i18n.t("settings_view.custom_theme.cancel"),
                                    None,
                                    ToolbarButtonOptions {
                                        button: ButtonOptions {
                                            variant: ButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                        ..ToolbarButtonOptions::default()
                                    },
                                    self.action(ThemeEditorRequest::Close),
                                ),
                            )
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.custom_theme.save"),
                                Some(render_icon(
                                    "lucide/save.svg",
                                    12.0,
                                    rgb(self.tokens.ui.accent_text),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: save_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                self.action(ThemeEditorRequest::Save),
                            )),
                    ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(MouseButton::Left, self.action(ThemeEditorRequest::Close))
                .child(oxideterm_gpui_ui::motion::form_transition(
                    &self.tokens,
                    "theme-editor-form-enter",
                    dialog,
                    form_visible,
                ))
                .when(!form_visible, |backdrop| {
                    backdrop.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll(|_event, _window, cx| cx.stop_propagation()),
                    )
                })
                .into_any_element(),
        )
    }

    fn render_input(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        kind: ThemeEditorTextInputKind,
        cx: &App,
    ) -> AnyElement {
        let settings = self.settings.read(cx);
        let value = settings.settings_entity_input_value(input).unwrap_or(value);
        let focused = settings.settings_entity_focused_input() == Some(input);
        let Some(ime) = (self.ime)(input, value, focused, cx) else {
            return div().into_any_element();
        };
        let dismiss = self.window.clone();
        render_settings_text_field(
            &self.tokens,
            ime,
            placeholder,
            TextInputContentAlign::Start,
            false,
            |control| {
                crate::settings_theme_editor_text_input(
                    &self.tokens,
                    control,
                    kind,
                    self.mono_font.clone(),
                )
                .on_mouse_down_out(move |_, window, cx| {
                    dismiss(SettingsWindowRequest::DismissInput(input), window, cx);
                })
            },
            self.window.clone(),
        )
    }

    fn duplicate_select(&self, label: String) -> AnyElement {
        let select = crate::SettingsSelect::CustomThemeDuplicate;
        let focus_visible =
            browser_focus_visible(self.open_select == Some(select), self.select_origin);
        render_settings_select_control(
            select,
            false,
            Some(180.0),
            select_trigger_with_focus_visible(&self.tokens, label, false, false, focus_visible)
                .h(px(32.0)),
            self.window.clone(),
        )
    }

    fn theme_editor_name_duplicate_row(
        &self,
        editor: &ThemeEditorState,
        cx: &mut App,
    ) -> AnyElement {
        settings_theme_editor_name_duplicate_row(
            self.theme_editor_label("settings_view.custom_theme.name"),
            self.render_input(
                SettingsInput::CustomThemeName,
                &editor.name,
                self.i18n.t("settings_view.custom_theme.name_placeholder"),
                ThemeEditorTextInputKind::Form,
                cx,
            ),
            editor
                .edit_theme_id
                .is_none()
                .then(|| self.theme_editor_duplicate_row(editor, cx)),
        )
    }

    fn theme_editor_duplicate_row(&self, editor: &ThemeEditorState, _cx: &mut App) -> AnyElement {
        let value = if editor.duplicate_theme_touched {
            theme_display_name(&editor.duplicate_theme)
        } else {
            self.i18n.t("settings_view.custom_theme.select_base")
        };
        settings_theme_editor_duplicate_row(
            self.theme_editor_label("settings_view.custom_theme.duplicate_from"),
            self.duplicate_select(value),
        )
    }
    fn theme_editor_section_tabs(&self, editor: &ThemeEditorState, cx: &mut App) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Terminal,
                "settings_view.custom_theme.terminal_colors",
                editor.active_section,
                cx,
            ))
            .when(self.background.is_some(), |tabs| {
                tabs.child(self.theme_editor_section_tab(
                    ThemeEditorSection::Background,
                    "settings_view.terminal.bg_title",
                    editor.active_section,
                    cx,
                ))
            })
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Ui,
                "settings_view.custom_theme.ui_colors",
                editor.active_section,
                cx,
            ))
            .into_any_element()
    }

    fn theme_editor_section_tab(
        &self,
        section: ThemeEditorSection,
        label_key: &str,
        active_section: ThemeEditorSection,
        _cx: &mut App,
    ) -> AnyElement {
        let active = section == active_section;
        div()
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .bg(rgba(0x00000000))
            .cursor_pointer()
            .hover(|tab| tab.text_color(rgb(self.tokens.ui.text)))
            .child(div().child(self.i18n.t(label_key)))
            .child(div().h(px(2.0)).w_full().bg(if active {
                rgb(self.tokens.ui.accent)
            } else {
                rgba(0x00000000)
            }))
            .on_mouse_down(MouseButton::Left, {
                let settings = self.settings.downgrade();
                let dispatch = self.dispatch.clone();
                move |_, window, cx| {
                    dispatch(ThemeEditorRequest::ChangeSection, window, cx);
                    let _ = settings.update(cx, |settings, cx| {
                        settings.select_theme_editor_section(section, cx);
                    });
                    cx.stop_propagation();
                }
            })
            .into_any_element()
    }

    fn theme_editor_color_grid(&self, editor: &ThemeEditorState, cx: &mut App) -> AnyElement {
        if editor.active_section == ThemeEditorSection::Ui {
            return self.theme_editor_ui_color_sections(editor, cx);
        }

        let (fields, colors, section) = match editor.active_section {
            ThemeEditorSection::Terminal => (
                TERMINAL_THEME_COLOR_FIELDS,
                editor.terminal_colors.as_slice(),
                ThemeEditorSection::Terminal,
            ),
            ThemeEditorSection::Ui | ThemeEditorSection::Background => {
                unreachable!("non-terminal sections render separately")
            }
        };
        self.theme_editor_color_grid_for_fields(fields, colors, section, cx)
    }

    fn theme_editor_ui_color_sections(
        &self,
        editor: &ThemeEditorState,
        cx: &mut App,
    ) -> AnyElement {
        let colors = editor.ui_colors.as_slice();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.ui_colors_hint")),
                    )
                    .child(
                        // Mirrors Tauri ThemeEditorModal's outline Button with
                        // Copy icon, with activation guarded by the shared
                        // workspace Button wrapper.
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.custom_theme.auto_derive"),
                            Some(render_icon(
                                "lucide/copy.svg",
                                12.0,
                                rgb(self.tokens.ui.text),
                            )),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            {
                                let settings = self.settings.downgrade();
                                move |_, _, cx| {
                                    let _ = settings.update(cx, |settings, cx| {
                                        settings.derive_theme_editor_ui_colors(cx);
                                    });
                                    cx.stop_propagation();
                                }
                            },
                        ),
                    ),
            )
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_background",
                &[0, 1, 2, 3, 4, 5, 6, 7],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_text",
                &[8, 9, 10],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_border",
                &[12, 13, 14],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_accent",
                &[15, 16, 17, 18],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_semantic",
                &[19, 20, 21, 22],
                colors,
                cx,
            ))
            .into_any_element()
    }

    fn theme_editor_ui_section(
        &self,
        title_key: &str,
        indexes: &[usize],
        colors: &[String],
        cx: &mut App,
    ) -> AnyElement {
        let mut cells = Vec::new();
        for &index in indexes {
            let Some(field) = UI_THEME_COLOR_FIELDS.get(index) else {
                continue;
            };
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            cells.push(self.theme_editor_color_cell(
                field,
                color,
                SettingsInput::CustomThemeUiColor(index),
                cx,
            ));
        }

        settings_theme_editor_color_section(&self.tokens, self.i18n.t(title_key), cells)
    }

    fn theme_editor_color_grid_for_fields(
        &self,
        fields: &[ThemeColorField],
        colors: &[String],
        section: ThemeEditorSection,
        cx: &mut App,
    ) -> AnyElement {
        let mut cells = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            let input = match section {
                ThemeEditorSection::Terminal => SettingsInput::CustomThemeTerminalColor(index),
                ThemeEditorSection::Ui => SettingsInput::CustomThemeUiColor(index),
                ThemeEditorSection::Background => unreachable!("background has no palette fields"),
            };
            cells.push(self.theme_editor_color_cell(field, color, input, cx));
        }
        settings_theme_editor_color_grid(cells)
    }

    fn theme_editor_color_cell(
        &self,
        field: &ThemeColorField,
        color: String,
        input: SettingsInput,
        cx: &mut App,
    ) -> AnyElement {
        let parsed = parse_color_hex(&color).unwrap_or(0);
        let focused = self.settings.read(cx).settings_entity_focused_input() == Some(input);
        let label = self.i18n.t(&format!(
            "settings_view.custom_theme.colors.{}",
            field.label_key
        ));
        let swatch = settings_theme_editor_color_swatch(&self.tokens, parsed)
            .on_mouse_down(
                MouseButton::Left,
                self.action(ThemeEditorRequest::Focus(input)),
            )
            .into_any_element();
        let value_control = if focused {
            self.render_input(
                input,
                &color,
                "#RRGGBB".to_string(),
                ThemeEditorTextInputKind::InlineColor,
                cx,
            )
        } else {
            settings_theme_editor_color_value(&self.tokens, color, self.frame.mono_font.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    self.action(ThemeEditorRequest::Focus(input)),
                )
                .into_any_element()
        };
        settings_theme_editor_color_cell(&self.tokens, label, swatch, value_control)
    }

    fn theme_editor_label(&self, key: &str) -> AnyElement {
        settings_theme_editor_label(&self.tokens, self.i18n.t(key))
    }

    fn theme_editor_footer_button(
        &self,
        icon: &'static str,
        label: String,
        color: u32,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Theme editor delete uses a color-tinted outline button in Tauri.
        // Keep only the tint local; activation still goes through the shared
        // Button guard used by the rest of the modal footer.
        self.workspace_toolbar_action_button(
            label,
            Some(render_icon(icon, 12.0, rgb(color))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_gap: Some(4.0),
                border: Some(rgba((color << 8) | 0x4d)),
                text_color: Some(rgb(color)),
                hover_background: Some(rgba((color << 8) | 0x1a)),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }
}
