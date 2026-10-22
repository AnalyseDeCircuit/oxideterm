use gpui::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, IntoElement, MouseButton,
    ParentElement, PromptButton, PromptLevel, PromptResponse, Render, Styled, Window, div,
    prelude::*, px, rgb,
};
use oxideterm_theme::ThemeTokens;

use crate::button::button_focus_visible;
use crate::confirm::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView, confirm_dialog_with_focus,
};
use crate::modal::MODAL_KEY_CONTEXT;
use crate::radio_group::{radio_group, radio_group_item};

pub struct ChoiceDialog {
    pub title: String,
    pub description: Option<String>,
    pub choices: [String; 2],
    pub cancel: String,
    pub confirm: String,
}

/// Uses GPUI's window-owned prompt lifetime and focus restoration with our dialog components.
pub fn choice_dialog(
    tokens: ThemeTokens,
    dialog: ChoiceDialog,
    window: &mut Window,
    cx: &mut App,
) -> impl Future<Output = Option<usize>> + use<> {
    // Build synchronously, then restore the platform builder so unrelated system
    // prompts keep their existing behavior. The rendered view belongs to this window.
    cx.set_prompt_builder(move |_, title, description, actions, handle, window, cx| {
        let view = cx.new(|cx| ChoiceDialogView {
            tokens,
            title: title.to_owned(),
            description: description.map(str::to_owned),
            actions: actions.to_vec(),
            focus: cx.focus_handle(),
            selected: 0,
            focused_control: ChoiceFocus::Options,
            keyboard_focus: false,
            confirm: dialog.confirm.clone(),
        });
        handle.with_view(view, window, cx)
    });
    let response = window.prompt(
        PromptLevel::Info,
        &dialog.title,
        dialog.description.as_deref(),
        &[
            PromptButton::new(dialog.choices[0].clone()),
            PromptButton::new(dialog.choices[1].clone()),
            PromptButton::cancel(dialog.cancel),
        ],
        cx,
    );
    cx.reset_prompt_builder();
    async move { response.await.ok().filter(|index| *index < 2) }
}

struct ChoiceDialogView {
    tokens: ThemeTokens,
    title: String,
    description: Option<String>,
    actions: Vec<PromptButton>,
    focus: FocusHandle,
    selected: usize,
    keyboard_focus: bool,
    focused_control: ChoiceFocus,
    confirm: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChoiceFocus {
    Options,
    Cancel,
    Confirm,
}

impl EventEmitter<PromptResponse> for ChoiceDialogView {}

impl Focusable for ChoiceDialogView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ChoiceDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = &self.tokens;
        let mut options = radio_group(tokens)
            .w_full()
            .text_align(gpui::TextAlign::Left);
        for (index, action) in self.actions.iter().take(2).enumerate() {
            options = options.child(
                button_focus_visible(
                    tokens,
                    div()
                        .flex()
                        .items_center()
                        .gap(px(tokens.spacing.two))
                        .py(px(tokens.spacing.two))
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .text_color(rgb(tokens.ui.text))
                        .cursor_pointer()
                        .child(radio_group_item(tokens, self.selected == index, false).flex_none())
                        .child(div().flex_1().min_w_0().child(action.label().clone())),
                    self.keyboard_focus
                        && self.focused_control == ChoiceFocus::Options
                        && self.selected == index,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.selected = index;
                        this.focused_control = ChoiceFocus::Options;
                        this.keyboard_focus = false;
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        let focused_action = self
            .keyboard_focus
            .then_some(self.focused_control)
            .and_then(|focus| match focus {
                ChoiceFocus::Options => None,
                ChoiceFocus::Cancel => Some(ConfirmDialogAction::Cancel),
                ChoiceFocus::Confirm => Some(ConfirmDialogAction::Confirm),
            });
        let dialog = confirm_dialog_with_focus(
            tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Default,
                title: div().child(self.title.clone()).into_any_element(),
                description: Some(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(tokens.spacing.three))
                        .when_some(self.description.clone(), |body, description| {
                            body.child(div().child(description))
                        })
                        .child(options)
                        .into_any_element(),
                ),
                cancel_label: div()
                    .child(self.actions[2].label().clone())
                    .into_any_element(),
                confirm_label: div().child(self.confirm.clone()).into_any_element(),
            },
            focused_action,
            cx.listener(|_, _, _, cx| {
                cx.emit(PromptResponse(2));
                cx.stop_propagation();
            }),
            cx.listener(|this, _, _, cx| {
                cx.emit(PromptResponse(this.selected));
                cx.stop_propagation();
            }),
        );
        div()
            .size_full()
            .font_family(crate::tauri_ui_font_family(tokens.metrics.font_family))
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .track_focus(&self.focus)
            .key_context(MODAL_KEY_CONTEXT)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => cx.emit(PromptResponse(2)),
                    "enter" => cx.emit(PromptResponse(
                        if this.focused_control == ChoiceFocus::Cancel {
                            2
                        } else {
                            this.selected
                        },
                    )),
                    "space" | " " if this.focused_control != ChoiceFocus::Options => {
                        cx.emit(PromptResponse(
                            if this.focused_control == ChoiceFocus::Cancel {
                                2
                            } else {
                                this.selected
                            },
                        ));
                    }
                    "tab" => {
                        this.focused_control =
                            match (this.focused_control, event.keystroke.modifiers.shift) {
                                (ChoiceFocus::Options, false) | (ChoiceFocus::Confirm, true) => {
                                    ChoiceFocus::Cancel
                                }
                                (ChoiceFocus::Cancel, false) | (ChoiceFocus::Options, true) => {
                                    ChoiceFocus::Confirm
                                }
                                _ => ChoiceFocus::Options,
                            }
                    }
                    "down" | "right" | "up" | "left"
                        if this.focused_control == ChoiceFocus::Options =>
                    {
                        this.selected = 1 - this.selected;
                    }
                    "home" if this.focused_control == ChoiceFocus::Options => this.selected = 0,
                    "end" if this.focused_control == ChoiceFocus::Options => this.selected = 1,
                    _ => {}
                }
                this.keyboard_focus = true;
                window.prevent_default();
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_up(|_, _, cx| cx.stop_propagation())
            .on_modifiers_changed(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .child(dialog)
    }
}
