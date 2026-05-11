use gpui::{
    AnyElement, Context, KeyDownEvent, MouseButton, ParentElement, Styled, Window, div, prelude::*,
    px, rgb,
};
use oxideterm_ssh::{
    KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, SshPromptError,
};
use tokio::sync::oneshot;

use crate::workspace::WorkspaceApp;
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_ui::{
    TextInputView, form_field, modal::dialog_backdrop, text_input, text_input_anchor_probe,
};

pub(in crate::workspace) struct KeyboardInteractiveChallenge {
    request: KeyboardInteractivePromptRequest,
    pub(in crate::workspace) responses: KeyboardInteractiveResponses,
    pub(in crate::workspace) focused_prompt: usize,
    response_tx: Option<oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>>,
}

impl KeyboardInteractiveChallenge {
    fn new(
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
    ) -> Self {
        let responses =
            KeyboardInteractiveResponses::new(vec![String::new(); request.prompts.len()]);
        Self {
            request,
            responses,
            focused_prompt: 0,
            response_tx: Some(response_tx),
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_keyboard_interactive_challenge(
        &mut self,
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary();
        self.keyboard_interactive_challenge =
            Some(KeyboardInteractiveChallenge::new(request, response_tx));
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(in crate::workspace) fn handle_keyboard_interactive_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if modifiers.platform {
            if key == "v" {
                self.paste_into_keyboard_interactive_field(cx);
            }
            return true;
        }

        match key {
            "escape" => {
                self.cancel_keyboard_interactive_challenge(cx);
                true
            }
            "enter" => {
                self.submit_keyboard_interactive_challenge(window, cx);
                true
            }
            "tab" => {
                if !challenge.responses.is_empty() {
                    if modifiers.shift {
                        challenge.focused_prompt = challenge
                            .focused_prompt
                            .saturating_sub(1)
                            .min(challenge.responses.len() - 1);
                    } else {
                        challenge.focused_prompt =
                            (challenge.focused_prompt + 1).min(challenge.responses.len() - 1);
                    }
                }
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" => {
                if let Some(response) = challenge.responses.get_mut(challenge.focused_prompt) {
                    response.pop();
                }
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            _ => true,
        }
    }

    fn paste_into_keyboard_interactive_field(&mut self, cx: &mut Context<Self>) {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let single_line = normalized.lines().collect::<Vec<_>>().join(" ");
        if let Some(response) = challenge.responses.get_mut(challenge.focused_prompt) {
            response.push_str(&single_line);
        }
        self.new_connection_caret_visible = true;
        cx.notify();
    }

    fn submit_keyboard_interactive_challenge(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut challenge) = self.keyboard_interactive_challenge.take() else {
            return;
        };
        if let Some(response_tx) = challenge.response_tx.take() {
            let _ = response_tx.send(Ok(challenge.responses));
        }
        if self.new_connection_form.is_none() {
            self.focus_active_pane(window, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_keyboard_interactive_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(mut challenge) = self.keyboard_interactive_challenge.take() else {
            return;
        };
        if let Some(response_tx) = challenge.response_tx.take() {
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
        }
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = Some(self.i18n.t("ssh.kbi.cancelled"));
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_keyboard_interactive_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(challenge) = self.keyboard_interactive_challenge.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let title = if challenge.request.name.trim().is_empty() {
            self.i18n.t("ssh.kbi.title")
        } else {
            challenge.request.name.clone()
        };

        let mut prompt_list = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap));
        for (index, prompt) in challenge.request.prompts.iter().enumerate() {
            let target = WorkspaceImeTarget::KeyboardInteractive(index);
            let workspace = cx.entity();
            let focused = challenge.focused_prompt == index;
            let value = challenge
                .responses
                .get(index)
                .map(String::as_str)
                .unwrap_or_default();
            prompt_list = prompt_list.child(form_field(
                &self.tokens,
                prompt.prompt.clone(),
                text_input_anchor_probe(
                    target.anchor_id(),
                    text_input(
                        &self.tokens,
                        TextInputView {
                            value,
                            placeholder: String::new(),
                            focused,
                            caret_visible: self.new_connection_caret_visible,
                            secret: !prompt.echo,
                            selected_all: false,
                            marked_text: self.marked_text_for_target(target),
                        },
                    )
                    .id(("kbi-prompt", index))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            if let Some(challenge) = this.keyboard_interactive_challenge.as_mut() {
                                challenge.focused_prompt = index;
                            }
                            this.ime_marked_text = None;
                            this.new_connection_caret_visible = true;
                            window.focus(&this.focus_handle);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
                    move |anchor, _window, cx| {
                        let _ = workspace.update(cx, |this, cx| {
                            this.update_text_input_anchor(anchor, cx);
                        });
                    },
                ),
            ));
        }

        dialog_backdrop()
            .child(
                div()
                    .w(px(self.tokens.metrics.modal_width))
                    .rounded(px(self.tokens.radii.md))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .bg(rgb(theme.bg_panel))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(title),
                            )
                            .when(
                                !challenge.request.instructions.trim().is_empty(),
                                |header| {
                                    header.child(
                                        div()
                                            .mt_1()
                                            .text_size(px(self
                                                .tokens
                                                .metrics
                                                .modal_description_font_size))
                                            .text_color(rgb(theme.text_muted))
                                            .child(challenge.request.instructions.clone()),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.metrics.modal_body_padding))
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.modal_body_gap))
                            .child(prompt_list),
                    )
                    .child(
                        div()
                            .h(px(self.tokens.metrics.modal_footer_height))
                            .px(px(self.tokens.metrics.modal_footer_padding_x))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            .child(self.render_keyboard_interactive_button(
                                self.i18n.t("ssh.form.cancel"),
                                false,
                                false,
                                cx,
                            ))
                            .child(self.render_keyboard_interactive_button(
                                self.i18n.t("ssh.kbi.continue"),
                                true,
                                true,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_keyboard_interactive_button(
        &self,
        label: String,
        primary: bool,
        submit: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(self.tokens.metrics.form_button_height))
            .px(px(self.tokens.metrics.form_button_padding_x))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(if primary {
                rgb(theme.accent)
            } else {
                rgb(theme.bg_elevated)
            })
            .text_size(px(self.tokens.metrics.form_text_font_size))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if primary {
                rgb(theme.accent_text)
            } else {
                rgb(theme.text)
            })
            .cursor_pointer()
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if submit {
                        this.submit_keyboard_interactive_challenge(window, cx);
                    } else {
                        this.cancel_keyboard_interactive_challenge(cx);
                    }
                }),
            )
            .into_any_element()
    }
}
