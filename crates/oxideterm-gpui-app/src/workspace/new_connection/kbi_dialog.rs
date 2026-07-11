use gpui::{
    AnyElement, Context, KeyDownEvent, MouseButton, ParentElement, Styled, Timer, Window, div,
    prelude::*, px, rgb, rgba,
};
use oxideterm_ssh::{
    KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, SshPromptError,
};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

use crate::workspace::WorkspaceApp;
use crate::workspace::ime::{WorkspaceImeTarget, keystroke_uses_text_edit_modifier};
use oxideterm_gpui_ui::{
    TextInputView,
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions},
    form_field,
    modal::{dismissible_dialog_backdrop, rounded_shell_child_radius},
    text_input, text_input_anchor_probe,
};

const KBI_PROMPT_TIMEOUT_SECS: u64 = 60;

pub(in crate::workspace) struct KeyboardInteractiveChallenge {
    presence: oxideterm_gpui_ui::motion::ExitPresence,
    request: KeyboardInteractivePromptRequest,
    pub(in crate::workspace) responses: KeyboardInteractiveResponses,
    pub(in crate::workspace) focused_prompt: usize,
    expires_at: Instant,
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
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            request,
            responses,
            focused_prompt: 0,
            expires_at: Instant::now() + Duration::from_secs(KBI_PROMPT_TIMEOUT_SECS),
            response_tx: Some(response_tx),
        }
    }

    pub(in crate::workspace) fn timed_out(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    fn seconds_left(&self) -> u64 {
        self.expires_at
            .saturating_duration_since(Instant::now())
            .as_secs()
    }

    fn all_responses_filled(&self) -> bool {
        self.responses.iter().all(|response| !response.is_empty())
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
        if let Some(existing) = self.keyboard_interactive_challenge.as_ref()
            && existing.request.flow_id != request.flow_id
        {
            // Tauri keeps the active GlobalKbiDialog owner and cancels a
            // competing auth flow instead of letting a later prompt steal the
            // dialog from the flow the user is already answering.
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
            return;
        }
        if let Some(mut existing) = self.keyboard_interactive_challenge.take()
            && let Some(existing_tx) = existing.response_tx.take()
        {
            // A same-flow replacement is unexpected for the native oneshot
            // bridge, but closing the old sender prevents a stale prompt from
            // waiting until the transport-side KBI timeout.
            let _ = existing_tx.send(Err(SshPromptError::Cancelled));
        }
        self.prepare_modal_interaction_boundary();
        self.keyboard_interactive_challenge =
            Some(KeyboardInteractiveChallenge::new(request, response_tx));
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.schedule_keyboard_interactive_timer(self.keyboard_interactive_timer_generation, cx);
        self.new_connection_caret_visible = true;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn schedule_keyboard_interactive_timer(&self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_secs(1)).await;
                let keep_ticking = weak
                    .update(cx, |this, cx| {
                        let Some(challenge) = this.keyboard_interactive_challenge.as_ref() else {
                            return false;
                        };
                        if this.keyboard_interactive_timer_generation != generation {
                            return false;
                        }
                        cx.notify();
                        !challenge.timed_out()
                    })
                    .unwrap_or(false);
                if !keep_ticking {
                    break;
                }
            }
        })
        .detach();
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
        if challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
            return true;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if keystroke_uses_text_edit_modifier(&event.keystroke) {
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
                if !challenge.timed_out() && challenge.all_responses_filled() {
                    self.submit_keyboard_interactive_challenge(window, cx);
                }
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
                if !challenge.timed_out()
                    && let Some(response) = challenge.responses.get_mut(challenge.focused_prompt)
                    && response.pop().is_some()
                {
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
                true
            }
            _ => true,
        }
    }

    fn paste_into_keyboard_interactive_field(&mut self, cx: &mut Context<Self>) {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return;
        };
        if challenge.timed_out() {
            return;
        }
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
        if challenge.timed_out() || !challenge.all_responses_filled() {
            self.keyboard_interactive_challenge = Some(challenge);
            cx.notify();
            return;
        }
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
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
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return;
        };
        let Some(generation) = challenge.presence.begin_exit() else {
            return;
        };
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        if let Some(response_tx) = challenge.response_tx.take() {
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
        }
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = Some(self.i18n.t("ssh.kbi.cancelled"));
        }
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.finish_keyboard_interactive_exit(generation);
        } else {
            cx.spawn(async move |weak, cx| {
                Timer::after(delay).await;
                let _ = weak.update(cx, |this, cx| {
                    if this.finish_keyboard_interactive_exit(generation) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn finish_keyboard_interactive_exit(&mut self, generation: u64) -> bool {
        if !self
            .keyboard_interactive_challenge
            .as_ref()
            .is_some_and(|challenge| challenge.presence.finish_exit(generation))
        {
            return false;
        }
        // Dropping the retained payload after the exit zeroizes all secret answers.
        self.keyboard_interactive_challenge = None;
        true
    }

    pub(in crate::workspace) fn render_keyboard_interactive_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(challenge) = self.keyboard_interactive_challenge.as_ref() else {
            return div().into_any_element();
        };
        let dialog_visible =
            challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let theme = self.tokens.ui;
        let title = if challenge.request.name.trim().is_empty() {
            self.i18n.t("ssh.kbi.title")
        } else {
            challenge.request.name.clone()
        };
        let timed_out = challenge.timed_out();
        let seconds_left = challenge.seconds_left();
        let can_submit = !timed_out && challenge.all_responses_filled();

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
                            selected_range: self.ime_selected_range_for_target(target),
                            marked_text: self.marked_text_for_target(target),
                        },
                    )
                    .id(("kbi-prompt", index))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            if let Some(challenge) = this.keyboard_interactive_challenge.as_mut() {
                                challenge.focused_prompt = index;
                            }
                            this.ime_marked_text = None;
                            this.new_connection_caret_visible = true;
                            window.focus(&this.focus_handle);
                            this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(
                        |this, event: &gpui::MouseMoveEvent, window, cx| {
                            this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                        },
                    )),
                    move |anchor, _window, cx| {
                        let _ = workspace.update(cx, |this, cx| {
                            this.update_text_input_anchor(anchor, cx);
                        });
                    },
                ),
            ));
        }

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri GlobalKbiDialog maps Radix outside-close to
                    // handleCancel(), which rejects the pending KBI prompt.
                    this.cancel_keyboard_interactive_challenge(cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "keyboard-interactive-dialog-transition",
                div()
                    .w(px(self.tokens.metrics.modal_width))
                    .rounded(px(self.tokens.radii.md))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .bg(rgb(theme.bg_panel))
                            // Browser DialogContent clips the painted header
                            // into the shell radius; keep native KBI prompts
                            // from exposing square top-corner pixels.
                            .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
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
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(if seconds_left <= 15 {
                                        rgba((theme.error << 8) | 0x80)
                                    } else {
                                        rgb(theme.border)
                                    })
                                    .bg(if seconds_left <= 15 {
                                        rgba((theme.error << 8) | 0x18)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x80)
                                    })
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(if seconds_left <= 15 {
                                        rgb(theme.error)
                                    } else {
                                        rgb(theme.text_muted)
                                    })
                                    .child(if timed_out {
                                        self.i18n.t("modals.kbi.timeout")
                                    } else {
                                        self.i18n_replace(
                                            "modals.kbi.time_remaining",
                                            &[("seconds", seconds_left.to_string())],
                                        )
                                    }),
                            )
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
                            // Footer chrome is flush with the dialog bottom,
                            // so it owns the inner bottom corners too.
                            .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            .child(self.render_keyboard_interactive_button(
                                self.i18n.t("ssh.form.cancel"),
                                false,
                                false,
                                false,
                                cx,
                            ))
                            .child(self.render_keyboard_interactive_button(
                                self.i18n.t("ssh.kbi.continue"),
                                true,
                                true,
                                !can_submit,
                                cx,
                            )),
                    ),
                dialog_visible,
            ))
            .into_any_element()
    }

    fn render_keyboard_interactive_button(
        &self,
        label: String,
        _primary: bool,
        submit: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let variant = if submit {
            ButtonVariant::Default
        } else {
            ButtonVariant::Ghost
        };
        // Keyboard-interactive prompts are authentication-protected dialogs, so
        // keep submit/cancel ownership here while sharing the Tauri Button
        // variant chrome.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                height: Some(self.tokens.metrics.form_button_height),
                padding_x: Some(self.tokens.metrics.form_button_padding_x),
                font_size: Some(self.tokens.metrics.form_text_font_size),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                if disabled {
                    return;
                }
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
