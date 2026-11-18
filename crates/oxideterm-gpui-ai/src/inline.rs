use std::{rc::Rc, time::Duration};

use crate::{model_picker::AiModelPickerView, state::AiModelSelectorScope};
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, EventEmitter, MouseButton, MouseDownEvent,
    Render, Rgba, SharedString, Task, Timer, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button},
    icons::LucideIcon,
    surface::{MaterialRole, material_surface},
    text_input::{
        TextInputAnchor, TextInputAnchorId, TextInputContentAlign, TextInputView,
        TextInputViewport, text_input_anchor_probe, text_input_with_viewport_and_content_align,
    },
};

use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::state::AiWorkspaceEntity;

pub struct AiInlinePanelFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub mono_font: SharedString,
    pub cursor: Option<AiInlineCursorAnchor>,
    pub submit_label: Option<String>,
    pub insert_label: Option<String>,
    pub input_anchor: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub active_offset: Option<usize>,
    pub selected_range: Option<std::ops::Range<usize>>,
    pub marked_text: Option<String>,
    pub caret_visible: bool,
}

pub enum AiInlineWindowRequest {
    Close,
    PromptPointer(MouseDownEvent),
    PromptDrag(gpui::MouseMoveEvent),
    Insert,
    Execute,
    Regenerate,
}

pub(crate) struct AiInlineResponseChanged;

impl EventEmitter<AiInlineResponseChanged> for AiWorkspaceEntity {}

pub struct AiInlinePanelView {
    state: Entity<AiWorkspaceEntity>,
    model_trigger: Entity<AiModelPickerView>,
    model_menu: Entity<AiModelPickerView>,
    anchor: Rc<dyn Fn(TextInputAnchor, &mut Window, &mut App)>,
    frame: Rc<dyn Fn(&App) -> Option<AiInlinePanelFrame>>,
    request: Rc<dyn Fn(AiInlineWindowRequest, &mut Window, &mut App)>,
    tokens: ThemeTokens,
    i18n: I18n,
    copy_reset: Option<Task<()>>,
}

impl AiInlinePanelView {
    pub fn new(
        state: Entity<AiWorkspaceEntity>,
        model_trigger: Entity<AiModelPickerView>,
        model_menu: Entity<AiModelPickerView>,
        anchor: Rc<dyn Fn(TextInputAnchor, &mut Window, &mut App)>,
        frame: Rc<dyn Fn(&App) -> Option<AiInlinePanelFrame>>,
        request: Rc<dyn Fn(AiInlineWindowRequest, &mut Window, &mut App)>,
        tokens: ThemeTokens,
        i18n: I18n,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&state, |_, _, _: &AiInlineResponseChanged, cx| cx.notify())
            .detach();
        Self {
            state,
            model_trigger,
            model_menu,
            anchor,
            frame,
            request,
            tokens,
            i18n,
            copy_reset: None,
        }
    }

    fn render_panel(&self, frame: &AiInlinePanelFrame, cx: &mut Context<Self>) -> AnyElement {
        let (prompt_focused, prompt, panel_loading, has_selection) = {
            let panel = self.state.read(cx).terminal_inline_panel();
            if !panel.open {
                return div().into_any_element();
            }
            (
                panel.prompt_focused,
                panel.prompt.clone(),
                panel.loading,
                panel.has_selection,
            )
        };

        let theme = self.tokens.ui;
        let focused = prompt_focused;
        let marked_text = frame.marked_text.as_deref();
        let selected_range = frame.selected_range.clone();
        let placeholder = if has_selection {
            self.i18n.t("terminal.ai.selection_placeholder")
        } else {
            self.i18n.t("terminal.ai.inline_placeholder")
        };
        let anchor = self.anchor.clone();
        let ai = self.state.read(cx);
        let panel = ai.terminal_inline_panel();
        let estimated_height =
            if panel.loading || panel.error.is_some() || !panel.response.is_empty() {
                AI_INLINE_PANEL_EXPANDED_HEIGHT
            } else {
                AI_INLINE_PANEL_COLLAPSED_HEIGHT
            };
        let placement = terminal_ai_inline_panel_placement(frame.cursor, estimated_height);

        div()
            .absolute()
            .top(px(placement.top))
            .left(px(placement.left))
            .child(
                material_surface(&self.tokens, div(), MaterialRole::Popover)
                    .relative()
                    .w(px(AI_INLINE_PANEL_WIDTH))
                    .rounded(px(self.tokens.radii.md))
                    // Tauri inline panels clip loading strips and action bars
                    // through the rounded shell; GPUI needs the same explicit
                    // panel clipping before any edge child paints.
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .shadow_lg()
                    .when(panel_loading, |panel| {
                        panel.child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .left(px(0.0))
                                .right(px(0.0))
                                .h(px(AI_INLINE_PANEL_LOADING_BAR_HEIGHT))
                                .rounded_t(px(
                                    oxideterm_gpui_ui::modal::rounded_shell_child_radius(
                                        self.tokens.radii.md,
                                    ),
                                ))
                                .bg(rgb(theme.accent)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Sparkles,
                                16.0,
                                rgb(theme.accent),
                            ))
                            .child(self.model_trigger.clone())
                            .child(text_input_anchor_probe(
                                frame.input_anchor,
                                text_input_with_viewport_and_content_align(
                                    &self.tokens,
                                    TextInputView {
                                        value: &prompt,
                                        placeholder,
                                        focused,
                                        caret_visible: frame.caret_visible,
                                        secret: false,
                                        selected_all: false,
                                        selected_range,
                                        marked_text,
                                    },
                                    TextInputContentAlign::Start,
                                    &frame.viewport,
                                    frame.active_offset,
                                )
                                .h(px(22.0))
                                .flex_1()
                                .min_w_0()
                                .border_0()
                                .bg(rgba(0x00000000))
                                .px(px(0.0))
                                .font_family(frame.mono_font.clone())
                                .text_size(px(13.0))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            this.state.update(cx, |ai, _cx| {
                                                ai.terminal_inline_panel_mut().prompt_focused =
                                                    true;
                                            });
                                            this.state.update(cx, |ai, _cx| {
                                                ai.set_model_selector_search_focused(false);
                                            });
                                            (this.request)(
                                                AiInlineWindowRequest::PromptPointer(event.clone()),
                                                window,
                                                cx,
                                            );
                                            cx.notify();
                                            cx.stop_propagation();
                                        },
                                    ),
                                )
                                .on_mouse_move(cx.listener(
                                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                                        (this.request)(
                                            AiInlineWindowRequest::PromptDrag(event.clone()),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                                move |value, window, cx| anchor(value, window, cx),
                            ))
                            .child(self.render_terminal_ai_inline_hints(frame, cx))
                            .child(
                                icon_button(
                                    &self.tokens,
                                    Self::render_lucide_icon(
                                        LucideIcon::X,
                                        14.0,
                                        rgb(theme.text_muted),
                                    ),
                                    IconButtonOptions {
                                        background: Some(rgba(0x00000000)),
                                        hover_background: Some(rgb(theme.bg_hover)),
                                        ..IconButtonOptions::opaque_toolbar(24.0, ButtonRadius::Md)
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        (this.request)(AiInlineWindowRequest::Close, window, cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    )
                    .child(self.render_response(frame, cx))
                    .when(
                        self.state
                            .read(cx)
                            .model_selector_is_open(AiModelSelectorScope::TerminalInline),
                        |panel| {
                            panel.child(
                                div()
                                    .absolute()
                                    .top(px(40.0))
                                    .left(px(32.0))
                                    .child(self.model_menu.clone()),
                            )
                        },
                    ),
            )
            .into_any_element()
    }

    fn render_terminal_ai_inline_hints(
        &self,
        frame: &AiInlinePanelFrame,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (prompt_has_text, response_has_text, loading) = {
            let panel = self.state.read(cx).terminal_inline_panel();
            (
                !panel.prompt.trim().is_empty(),
                !panel.response.trim().is_empty(),
                panel.loading,
            )
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(10.0))
            .text_color(rgb(theme.text_muted))
            .when(!response_has_text && !loading && prompt_has_text, |hints| {
                hints.when_some(frame.submit_label.clone(), |hints, key| {
                    hints
                        .child(inline_ai_keycap(&self.tokens, key))
                        .child(self.i18n.t("terminal.ai.to_send"))
                })
            })
            .when(response_has_text && !loading, |hints| {
                hints
                    .when_some(frame.insert_label.clone(), |hints, key| {
                        hints
                            .child(inline_ai_keycap(&self.tokens, key))
                            .child(self.i18n.t("terminal.ai.to_insert"))
                    })
                    .when_some(frame.submit_label.clone(), |hints, key| {
                        hints
                            .child(inline_ai_keycap(&self.tokens, key))
                            .child(self.i18n.t("terminal.ai.to_run"))
                    })
            })
            .into_any_element()
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn copy_terminal_ai_inline_response(&mut self, cx: &mut Context<Self>) {
        let command = extract_terminal_ai_inline_command(
            &self.state.read(cx).terminal_inline_panel().response,
        );
        if command.trim().is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(command));
        let generation = self.state.update(cx, |ai, _| {
            let panel = ai.terminal_inline_panel_mut();
            panel.copied = true;
            panel.generation
        });
        self.copy_reset = Some(cx.spawn(async move |view, cx| {
            Timer::after(Duration::from_millis(1500)).await;
            let _ = view.update(cx, |view, cx| {
                let changed = view.state.update(cx, |state, _| {
                    let panel = state.terminal_inline_panel_mut();
                    if panel.generation != generation {
                        return false;
                    }
                    panel.copied = false;
                    true
                });
                if changed {
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    fn render_terminal_ai_inline_notice(
        &self,
        icon: LucideIcon,
        message: String,
        bg: Rgba,
        border: Rgba,
        fg: Rgba,
    ) -> AnyElement {
        div()
            .mx(px(12.0))
            .mb(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border)
            .bg(bg)
            .px(px(8.0))
            .py(px(6.0))
            .text_size(px(12.0))
            .text_color(fg)
            .child(Self::render_lucide_icon(icon, 14.0, fg))
            .child(div().truncate().child(message))
            .into_any_element()
    }

    fn render_terminal_ai_inline_action(
        &self,
        icon: LucideIcon,
        label: String,
        primary: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let fg = if primary {
            rgb(0xffffff)
        } else {
            rgb(theme.text)
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(4.0))
            .text_size(px(11.0))
            .text_color(fg)
            .bg(if primary {
                rgb(theme.accent)
            } else {
                rgba(0x00000000)
            })
            .hover(move |style| {
                style.bg(if primary {
                    rgb(theme.accent_hover)
                } else {
                    rgb(theme.bg_hover)
                })
            })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(listener))
            .child(Self::render_lucide_icon(icon, 12.0, fg))
            .child(label)
            .into_any_element()
    }
}

impl AiInlinePanelView {
    fn render_response(&self, frame: &AiInlinePanelFrame, cx: &mut Context<Self>) -> AnyElement {
        let panel = self.state.read(cx).terminal_inline_panel();
        if !panel.open {
            return div().into_any_element();
        }
        let theme = self.tokens.ui;
        let panel_loading = panel.loading;
        let has_api_key = panel.has_api_key;
        let panel_error = panel.error.clone();
        let response_is_empty = panel.response.is_empty();
        let response_command = extract_terminal_ai_inline_command(&panel.response);
        let panel_copied = panel.copied;
        div()
            .w_full()
            .flex()
            .flex_col()
            .when(has_api_key == Some(false) && !panel_loading, |panel| {
                panel.child(self.render_terminal_ai_inline_notice(
                    LucideIcon::AlertCircle,
                    self.i18n.t("terminal.ai.api_key_hint"),
                    rgba((self.tokens.ui.warning << 8) | 0x1a),
                    rgba((self.tokens.ui.warning << 8) | 0x4d),
                    rgb(self.tokens.ui.warning),
                ))
            })
            .when_some(panel_error.as_ref(), |panel, error| {
                panel.child(self.render_terminal_ai_inline_notice(
                    LucideIcon::AlertCircle,
                    error.clone(),
                    rgba((self.tokens.ui.error << 8) | 0x1a),
                    rgba((self.tokens.ui.error << 8) | 0x4d),
                    rgb(self.tokens.ui.error),
                ))
            })
            .when(
                (panel_loading || !response_is_empty) && panel_error.is_none(),
                |panel| {
                    panel.child(
                        div()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .max_h(px(120.0))
                                    .overflow_hidden()
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .bg(rgb(theme.bg_sunken))
                                    .font_family(frame.mono_font.clone())
                                    .text_size(px(13.0))
                                    .line_height(px(20.0))
                                    .text_color(rgb(theme.accent))
                                    .child(if response_command.is_empty() {
                                        self.i18n.t("terminal.ai.generating")
                                    } else {
                                        response_command.clone()
                                    }),
                            )
                            .when(!response_is_empty && !panel_loading, |preview| {
                                preview.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .border_t_1()
                                        .border_color(rgb(theme.border))
                                        .bg(rgb(theme.bg_elevated))
                                        .rounded_b(px(
                                            oxideterm_gpui_ui::modal::rounded_shell_child_radius(
                                                self.tokens.radii.md,
                                            ),
                                        ))
                                        .px(px(8.0))
                                        .py(px(6.0))
                                        .child(self.render_terminal_ai_inline_action(
                                            LucideIcon::Play,
                                            self.i18n.t("terminal.ai.execute"),
                                            true,
                                            |this, _event, window, cx| {
                                                (this.request)(
                                                    AiInlineWindowRequest::Execute,
                                                    window,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            },
                                            cx,
                                        ))
                                        .child(self.render_terminal_ai_inline_action(
                                            LucideIcon::CornerDownLeft,
                                            self.i18n.t("terminal.ai.insert"),
                                            false,
                                            |this, _event, window, cx| {
                                                (this.request)(
                                                    AiInlineWindowRequest::Insert,
                                                    window,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            },
                                            cx,
                                        ))
                                        .child(self.render_terminal_ai_inline_action(
                                            if panel_copied {
                                                LucideIcon::Check
                                            } else {
                                                LucideIcon::Copy
                                            },
                                            if panel_copied {
                                                self.i18n.t("terminal.ai.copied")
                                            } else {
                                                self.i18n.t("terminal.ai.copy")
                                            },
                                            false,
                                            |this, _event, _window, cx| {
                                                this.copy_terminal_ai_inline_response(cx);
                                                cx.stop_propagation();
                                            },
                                            cx,
                                        ))
                                        .child(self.render_terminal_ai_inline_action(
                                            LucideIcon::RotateCcw,
                                            self.i18n.t("terminal.ai.regenerate"),
                                            false,
                                            |this, _event, window, cx| {
                                                (this.request)(
                                                    AiInlineWindowRequest::Regenerate,
                                                    window,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            },
                                            cx,
                                        )),
                                )
                            }),
                    )
                },
            )
            .into_any_element()
    }
}

impl Render for AiInlinePanelView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.frame)(cx) else {
            return div().into_any_element();
        };
        self.tokens = frame.tokens;
        self.i18n = frame.i18n.clone();
        self.render_panel(&frame, cx)
    }
}

#[derive(Clone, Copy)]
pub struct AiInlineCursorAnchor {
    pub y: f32,
    pub line_height: f32,
    pub container_width: f32,
    pub container_height: f32,
}

const AI_INLINE_PANEL_WIDTH: f32 = 520.0;
const AI_INLINE_PANEL_TOP: f32 = 48.0;
const AI_INLINE_PANEL_MARGIN: f32 = 12.0;
const AI_INLINE_PANEL_VERTICAL_OFFSET: f32 = 4.0;
const AI_INLINE_PANEL_COLLAPSED_HEIGHT: f32 = 56.0;
const AI_INLINE_PANEL_EXPANDED_HEIGHT: f32 = 160.0;
const AI_INLINE_PANEL_LOADING_BAR_HEIGHT: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct AiInlinePanelPlacement {
    left: f32,
    top: f32,
}

fn inline_ai_keycap(tokens: &ThemeTokens, label: String) -> AnyElement {
    div()
        .rounded(px(tokens.radii.sm))
        .bg(rgb(tokens.ui.bg_hover))
        .px(px(4.0))
        .py(px(1.0))
        .text_size(px(9.0))
        .child(label)
        .into_any_element()
}

fn terminal_ai_inline_panel_placement(
    anchor: Option<AiInlineCursorAnchor>,
    estimated_height: f32,
) -> AiInlinePanelPlacement {
    let Some(anchor) = anchor else {
        return AiInlinePanelPlacement {
            left: AI_INLINE_PANEL_MARGIN,
            top: AI_INLINE_PANEL_TOP,
        };
    };

    let mut top = anchor.y + anchor.line_height + AI_INLINE_PANEL_VERTICAL_OFFSET;
    let mut left = (anchor.container_width - AI_INLINE_PANEL_WIDTH) / 2.0;
    if left < AI_INLINE_PANEL_MARGIN {
        left = AI_INLINE_PANEL_MARGIN;
    } else if left + AI_INLINE_PANEL_WIDTH > anchor.container_width - AI_INLINE_PANEL_MARGIN {
        left = anchor.container_width - AI_INLINE_PANEL_WIDTH - AI_INLINE_PANEL_MARGIN;
    }

    if top + estimated_height > anchor.container_height - AI_INLINE_PANEL_MARGIN {
        top = anchor.y - estimated_height - AI_INLINE_PANEL_VERTICAL_OFFSET;
        if top < AI_INLINE_PANEL_MARGIN {
            top = AI_INLINE_PANEL_MARGIN;
        }
    }

    AiInlinePanelPlacement { left, top }
}

pub fn extract_terminal_ai_inline_command(text: &str) -> String {
    if let Some(command) = extract_fenced_code_block(text) {
        return command.trim().to_string();
    }
    if let Some(command) = extract_inline_code(text) {
        return command.trim().to_string();
    }
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.trim()
                .strip_prefix("$ ")
                .or_else(|| line.trim().strip_prefix("> "))
                .unwrap_or_else(|| line.trim())
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| text.trim().to_string())
}

fn extract_fenced_code_block(text: &str) -> Option<&str> {
    let start = text.find("```")?;
    let after_start = &text[start + 3..];
    let content_start = after_start
        .find('\n')
        .map(|index| index + 1)
        .unwrap_or_default();
    let content = &after_start[content_start..];
    let end = content.find("```")?;
    Some(&content[..end])
}

fn extract_inline_code(text: &str) -> Option<&str> {
    let start = text.find('`')?;
    let rest = &text[start + 1..];
    let end = rest.find('`')?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn extracts_multiline_fenced_command() {
        let command = extract_terminal_ai_inline_command("```bash\nmkdir demo\ncd demo\n```");
        assert_eq!(command, "mkdir demo\ncd demo");
    }

    #[test]
    pub fn strips_shell_prompt_from_first_non_empty_line() {
        assert_eq!(
            extract_terminal_ai_inline_command("\n$ cargo test\nexplanation"),
            "cargo test",
        );
    }
}

#[cfg(test)]
mod terminal_inline_tests {
    use super::{
        AI_INLINE_PANEL_COLLAPSED_HEIGHT, AI_INLINE_PANEL_EXPANDED_HEIGHT, AiInlineCursorAnchor,
        terminal_ai_inline_panel_placement,
    };

    #[test]
    fn places_panel_below_cursor_when_space_allows() {
        let placement = terminal_ai_inline_panel_placement(
            Some(AiInlineCursorAnchor {
                y: 100.0,
                line_height: 20.0,
                container_width: 800.0,
                container_height: 600.0,
            }),
            AI_INLINE_PANEL_COLLAPSED_HEIGHT,
        );
        assert_eq!(placement.left, 140.0);
        assert_eq!(placement.top, 124.0);
    }

    #[test]
    fn flips_panel_above_cursor_near_bottom() {
        let placement = terminal_ai_inline_panel_placement(
            Some(AiInlineCursorAnchor {
                y: 560.0,
                line_height: 20.0,
                container_width: 800.0,
                container_height: 600.0,
            }),
            AI_INLINE_PANEL_EXPANDED_HEIGHT,
        );
        assert_eq!(placement.left, 140.0);
        assert_eq!(placement.top, 396.0);
    }
}
