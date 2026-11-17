use gpui::{
    App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, FontWeight, IntoElement,
    MouseButton, ParentElement, PromptButton, PromptLevel, PromptResponse, Render, Styled, Window,
    div, prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

use crate::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_focus_visible, button_with,
};
use crate::modal::{
    MODAL_KEY_CONTEXT, TauriBackdropRole, dialog_content, dialog_title, staged_overlay_root,
    staged_overlay_scrim,
};
use crate::radio_group::{radio_group, radio_group_item};

const CHOICE_DIALOG_WIDTH: f32 = 400.0;

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
        let theme = tokens.ui;
        let mut options = radio_group(tokens)
            .w_full()
            .gap(px(8.0))
            .text_align(gpui::TextAlign::Left);
        for (index, action) in self.actions.iter().take(2).enumerate() {
            let selected = self.selected == index;
            let row = div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(14.0))
                .py(px(11.0))
                .rounded(px(tokens.radii.md))
                .border_1()
                .cursor_pointer()
                .when(selected, |row| {
                    row.bg(rgba((theme.accent << 8) | 0x1a))
                        .border_color(rgba((theme.accent << 8) | 0x80))
                })
                .when(!selected, |row| {
                    row.bg(rgba((theme.bg_panel << 8) | 0x2e))
                        .border_color(rgba((theme.border << 8) | 0x66))
                        .hover(move |row| {
                            row.bg(rgb(theme.bg_hover)).border_color(rgb(theme.border))
                        })
                })
                .child(radio_group_item(tokens, selected, false).flex_none())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .line_height(px(tokens.metrics.ui_text_sm * 1.35))
                        .when(selected, |label| {
                            label
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.text_heading))
                        })
                        .when(!selected, |label| label.text_color(rgb(theme.text)))
                        .child(action.label().clone()),
                );
            options = options.child(
                button_focus_visible(
                    tokens,
                    row,
                    self.keyboard_focus && self.focused_control == ChoiceFocus::Options && selected,
                )
                .id(("choice-dialog-option", index))
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
        // Compact unified card layout avoids the fragmented three-band divider look
        // when description is absent on short two-choice prompts.
        let form = dialog_content(tokens)
            .id("choice-dialog-card")
            .w(px(CHOICE_DIALOG_WIDTH))
            .max_h(px(520.0))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .px(px(20.0))
                    .pt(px(20.0))
                    .pb(px(12.0))
                    .child(dialog_title(tokens, self.title.clone()))
                    .when_some(self.description.clone(), |header, description| {
                        header.child(
                            div()
                                .text_size(px(tokens.metrics.ui_text_sm))
                                .line_height(px(tokens.metrics.ui_text_sm * 1.45))
                                .text_color(rgb(theme.text_muted))
                                .child(description),
                        )
                    }),
            )
            .child(
                div()
                    .id("choice-dialog-body")
                    .w_full()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(20.0))
                    .py(px(4.0))
                    .child(options),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .px(px(20.0))
                    .pt(px(16.0))
                    .pb(px(20.0))
                    .child(
                        button_focus_visible(
                            tokens,
                            button_with(
                                tokens,
                                self.actions[2].label().to_string(),
                                ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Default,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                            ),
                            self.keyboard_focus && self.focused_control == ChoiceFocus::Cancel,
                        )
                        .id("choice-dialog-cancel")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| {
                                cx.emit(PromptResponse(2));
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .child(
                        button_focus_visible(
                            tokens,
                            button_with(
                                tokens,
                                self.confirm.clone(),
                                ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Default,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                            ),
                            self.keyboard_focus && self.focused_control == ChoiceFocus::Confirm,
                        )
                        .id("choice-dialog-confirm")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                cx.emit(PromptResponse(this.selected));
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            );
        let dialog = staged_overlay_root()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| {
                    cx.emit(PromptResponse(2));
                    cx.stop_propagation();
                }),
            )
            .child(staged_overlay_scrim(
                tokens,
                "choice-dialog-scrim",
                TauriBackdropRole::Dialog,
                true,
            ))
            .child(crate::motion::zoom_fade_in_y(
                tokens,
                "choice-dialog",
                div().child(form),
                8.0,
                0.95,
                gpui::point(0.5, 0.5),
                crate::motion::MotionDuration::Control,
            ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[gpui::test]
    fn choice_dialog_renders_compact_selectable_cards_and_routes_actions(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut tokens = oxideterm_theme::default_tokens();
        tokens.motion.enabled = false;
        tokens.motion.spatial_enabled = false;
        let emitted: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let emitted_clone = emitted.clone();

        let (view, cx) = cx.add_window_view(|_window, cx| ChoiceDialogView {
            tokens: tokens.clone(),
            title: "Select file access".to_string(),
            description: Some("Choose the transport used to browse remote files.".to_string()),
            actions: vec![
                PromptButton::new("SFTP"),
                PromptButton::new("SSH"),
                PromptButton::cancel("Cancel"),
            ],
            focus: cx.focus_handle(),
            selected: 0,
            keyboard_focus: false,
            focused_control: ChoiceFocus::Options,
            confirm: "OK".to_string(),
        });

        cx.simulate_resize(gpui::size(px(800.0), px(600.0)));
        let window_handle = cx.update(|window, app| {
            let sub = app.subscribe(&view, move |_view, response: &PromptResponse, _| {
                emitted_clone.set(Some(response.0));
            });
            std::mem::forget(sub);
            let focus = view.read(app).focus.clone();
            focus.focus(window, app);
            window.window_handle()
        });
        cx.run_until_parked();

        let (scale, quads) = cx.update(|window, app| {
            window.draw(app).clear(app);
            (window.scale_factor(), window.painted_quads())
        });

        // Compact dialog width (400px) rather than bloated 512px modal_width.
        let card_quad = quads
            .iter()
            .find(|quad| {
                (quad.bounds.size.width.0 / scale - CHOICE_DIALOG_WIDTH).abs() < 0.5
                    && quad.bounds.size.height.0 / scale > 100.0
            })
            .unwrap_or_else(|| {
                let sizes: Vec<_> = quads
                    .iter()
                    .map(|q| {
                        (
                            q.bounds.size.width.0 / scale,
                            q.bounds.size.height.0 / scale,
                        )
                    })
                    .collect();
                panic!("compact dialog card quad not found; painted sizes: {sizes:?}")
            });
        assert!((card_quad.bounds.size.width.0 / scale - CHOICE_DIALOG_WIDTH).abs() < 0.5);

        // Option cards fill the 400px card inner width (358px inside 2*20px padding + 2*1px card border);
        // verify only 1 radio dot quad (10x10) is painted for the active selection.
        let dot_count = quads
            .iter()
            .filter(|quad| {
                (quad.bounds.size.width.0 / scale - tokens.metrics.ui_radio_dot_size).abs() < 0.5
                    && (quad.bounds.size.height.0 / scale - tokens.metrics.ui_radio_dot_size).abs()
                        < 0.5
            })
            .count();
        assert_eq!(dot_count, 1);

        let selected_border: gpui::SceneHsla =
            gpui::Hsla::from(rgba((tokens.ui.accent << 8) | 0x80)).into();
        let unselected_border: gpui::SceneHsla =
            gpui::Hsla::from(rgba((tokens.ui.border << 8) | 0x66)).into();
        let mut option_quads: Vec<_> = quads
            .iter()
            .filter(|quad| {
                (quad.bounds.size.width.0 / scale - (CHOICE_DIALOG_WIDTH - 42.0)).abs() < 1.0
                    && quad.bounds.size.height.0 / scale >= 32.0
                    && (quad.border_color == selected_border
                        || quad.border_color == unselected_border)
            })
            .collect();
        option_quads.sort_by(|a, b| a.bounds.top().0.total_cmp(&b.bounds.top().0));
        option_quads.dedup_by(|a, b| (a.bounds.top().0 - b.bounds.top().0).abs() < 1.0);
        assert_eq!(option_quads.len(), 2);
        assert_eq!(option_quads[0].border_color, selected_border);
        assert_eq!(option_quads[1].border_color, unselected_border);

        let second_center = gpui::point(
            px(option_quads[1].bounds.center().x.0 / scale),
            px(option_quads[1].bounds.center().y.0 / scale),
        );
        cx.simulate_mouse_down(second_center, MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |this, _| assert_eq!(this.selected, 1));

        cx.dispatch_keystroke(window_handle, gpui::Keystroke::parse("up").expect("up"));
        cx.run_until_parked();
        view.read_with(cx, |this, _| assert_eq!(this.selected, 0));

        cx.dispatch_keystroke(
            window_handle,
            gpui::Keystroke::parse("enter").expect("enter"),
        );
        cx.run_until_parked();
        assert_eq!(emitted.get(), Some(0));

        cx.dispatch_keystroke(
            window_handle,
            gpui::Keystroke::parse("escape").expect("escape"),
        );
        cx.run_until_parked();
        assert_eq!(emitted.get(), Some(2));
    }
}
