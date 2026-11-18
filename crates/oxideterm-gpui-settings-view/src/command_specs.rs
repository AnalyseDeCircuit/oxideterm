use gpui::{
    AnyElement, App, Context, Entity, MouseButton, MouseDownEvent, Render, ScrollHandle,
    SharedString, Subscription, Window, div, prelude::*, px, rgb,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    modal::{
        dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary,
    },
    scroll::ScrollableElement,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use std::{ops::Deref, rc::Rc};
use zeroize::Zeroizing;

use crate::{SettingsInput, entity::SettingsWorkspaceEntity};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCommandSpecsAction {
    Format,
    Example,
    Save,
    Close,
}

// The command-spec document needs enough room for structured editing while
// remaining bounded by the workspace viewport.
const TERMINAL_COMMAND_SPECS_MODAL_WIDTH: f32 = 880.0;
const TERMINAL_COMMAND_SPECS_MODAL_HEIGHT: f32 = 720.0;

pub struct CommandSpecsFrame {
    pub i18n: I18n,
    pub tokens: ThemeTokens,
    pub mono_font: SharedString,
    pub scroll: ScrollHandle,
}

pub struct CommandSpecsBindings {
    pub frame: Rc<dyn Fn(&App) -> Option<CommandSpecsFrame>>,
    pub editor: Rc<dyn Fn(&str, &App) -> AnyElement>,
    pub dispatch: Rc<dyn Fn(TerminalCommandSpecsAction, &mut Window, &mut App)>,
}

pub struct CommandSpecsDialog {
    state: Entity<SettingsWorkspaceEntity>,
    bindings: CommandSpecsBindings,
    _changed: Subscription,
}

impl CommandSpecsDialog {
    pub fn new(
        state: &Entity<SettingsWorkspaceEntity>,
        bindings: CommandSpecsBindings,
        cx: &mut Context<Self>,
    ) -> Self {
        let changed = cx.observe(state, |_, state, cx| {
            if state.read(cx).command_specs_open() {
                cx.notify();
            }
        });
        Self {
            state: state.clone(),
            bindings,
            _changed: changed,
        }
    }
}

impl Render for CommandSpecsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(draft) = self.state.read(cx).command_specs.as_ref() else {
            return div().into_any_element();
        };
        let Some(frame) = (self.bindings.frame)(cx) else {
            return div().into_any_element();
        };
        let editor = (self.bindings.editor)(draft.value.as_str(), cx);
        CommandSpecsBody {
            frame,
            dispatch: &self.bindings.dispatch,
        }
        .render_dialog(draft, editor, cx)
    }
}

struct CommandSpecsBody<'a> {
    frame: CommandSpecsFrame,
    dispatch: &'a Rc<dyn Fn(TerminalCommandSpecsAction, &mut Window, &mut App)>,
}

impl Deref for CommandSpecsBody<'_> {
    type Target = CommandSpecsFrame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl CommandSpecsBody<'_> {
    fn press(
        &self,
        action: TerminalCommandSpecsAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, window, cx| {
            dispatch(action, window, cx);
            cx.stop_propagation();
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        toolbar_button(&self.tokens, label, icon, options).on_mouse_down(MouseButton::Left, press)
    }

    fn render_dialog(&self, draft: &CommandSpecsDraft, editor: AnyElement, cx: &App) -> AnyElement {
        let theme = self.tokens.ui;
        let dialog = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(TERMINAL_COMMAND_SPECS_MODAL_WIDTH))
            .max_w_full()
            .h(px(TERMINAL_COMMAND_SPECS_MODAL_HEIGHT))
            .max_h_full()
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.command_specs"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.command_specs_hint"),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_family(self.frame.mono_font.clone())
                            .text_color(rgb(theme.text_muted))
                            .truncate()
                            .child(draft.path.clone()),
                    ),
            )
            .child(
                // The modal body is the sole scroll owner. The editor grows with
                // its JSON document instead of introducing a nested scroll view.
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .id("terminal-command-specs-editor-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.frame.scroll)
                    .vertical_scrollbar(&self.frame.scroll)
                    .bg(rgb(theme.bg_elevated))
                    .child(editor),
            )
            .child(
                dialog_footer(&self.tokens)
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(draft.summary.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(self.tokens.metrics.modal_field_gap))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_format",
                                TerminalCommandSpecsAction::Format,
                                cx,
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_example",
                                TerminalCommandSpecsAction::Example,
                                cx,
                            ))
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.terminal.command_specs_close"),
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
                                self.press(TerminalCommandSpecsAction::Close),
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_save",
                                TerminalCommandSpecsAction::Save,
                                cx,
                            )),
                    ),
            );

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                self.press(TerminalCommandSpecsAction::Close),
            )
            .child(overlay_content_boundary(dialog))
            .into_any_element()
    }

    fn terminal_command_specs_button(
        &self,
        label_key: &'static str,
        action: TerminalCommandSpecsAction,
        _cx: &App,
    ) -> AnyElement {
        // Command-spec editor actions behave like Tauri Button onClick handlers:
        // disabled/loading guards live at the shared workspace boundary, not in
        // each feature listener.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if action == TerminalCommandSpecsAction::Save {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            self.press(action),
        )
        .into_any_element()
    }
}

pub(crate) struct CommandSpecsDraft {
    pub value: Zeroizing<String>,
    path: String,
    summary: String,
}

impl SettingsWorkspaceEntity {
    pub fn open_command_specs(
        &mut self,
        path: String,
        summary: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.command_specs = Some(CommandSpecsDraft {
            path,
            summary,
            value: Zeroizing::new(value),
        });
        self.settings_focused_input = Some(SettingsInput::TerminalCommandSpecsJson);
        cx.notify();
    }

    pub fn close_command_specs(&mut self, cx: &mut Context<Self>) -> bool {
        if self.command_specs.take().is_none() {
            return false;
        }
        if self.settings_focused_input == Some(SettingsInput::TerminalCommandSpecsJson) {
            self.settings_focused_input = None;
        }
        cx.notify();
        true
    }

    pub fn command_specs_open(&self) -> bool {
        self.command_specs.is_some()
    }

    pub fn update_command_specs(
        &mut self,
        value: Zeroizing<String>,
        summary: Option<String>,
        focus: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.command_specs.as_mut() {
            draft.value = value;
            if let Some(summary) = summary {
                draft.summary = summary;
            }
            if focus {
                self.settings_focused_input = Some(SettingsInput::TerminalCommandSpecsJson);
            }
            cx.notify();
        }
    }
}
