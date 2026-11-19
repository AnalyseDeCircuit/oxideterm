use std::{ops::Range, rc::Rc};

use gpui::{
    App, Context, CursorStyle, Div, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, Render,
    SharedString, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::text_input::{
    TextInputAnchor, TextInputAnchorId, text_caret, text_input_anchor_probe,
    text_input_value_segments_with_color, text_input_value_segments_with_marked_range,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::state::AiWorkspaceEntity;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AiEditorKind {
    Composer,
    MessageEdit,
}

pub struct AiEditorFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub font: SharedString,
    pub width: f32,
    pub anchor: TextInputAnchorId,
    pub model_selected: bool,
    pub marked_range: Option<Range<usize>>,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    pub composed_text: Option<String>,
    pub caret_visible: bool,
}

pub enum AiEditorInput {
    PointerDown(MouseDownEvent),
    Drag(MouseMoveEvent),
}

#[derive(Clone)]
pub struct AiEditorBindings {
    pub frame: Rc<dyn Fn(AiEditorKind, &App) -> Option<AiEditorFrame>>,
    pub input: Rc<dyn Fn(AiEditorKind, AiEditorInput, &mut Window, &mut App)>,
    pub anchor: Rc<dyn Fn(TextInputAnchor, &mut Window, &mut App)>,
}

pub struct AiEditorView {
    kind: AiEditorKind,
    state: Entity<AiWorkspaceEntity>,
    frame: Rc<dyn Fn(AiEditorKind, &App) -> Option<AiEditorFrame>>,
    request: Rc<dyn Fn(AiEditorKind, AiEditorInput, &mut Window, &mut App)>,
    anchor: Rc<dyn Fn(TextInputAnchor, &mut Window, &mut App)>,
}

impl AiEditorView {
    pub fn new(
        kind: AiEditorKind,
        state: Entity<AiWorkspaceEntity>,
        bindings: AiEditorBindings,
    ) -> Self {
        Self {
            kind,
            state,
            frame: bindings.frame,
            request: bindings.input,
            anchor: bindings.anchor,
        }
    }
}

impl Render for AiEditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.frame)(self.kind, cx) else {
            return div().into_any_element();
        };
        let ai = self.state.read(cx);
        let editing = self.kind == AiEditorKind::MessageEdit;
        let enabled = editing || ai.history_ready();
        let placeholder = if editing {
            String::new()
        } else if !enabled {
            frame.i18n.t("ai.input.placeholder_disabled")
        } else if !frame.model_selected {
            frame.i18n.t("ai.model_selector.select_model")
        } else if ai.active_user_question().is_some() {
            frame.i18n.t("ai.questions.hint")
        } else {
            frame.i18n.t("ai.input.placeholder")
        };
        let (focused, draft) = if editing {
            (
                ai.chat_ui().editing_message_focused,
                ai.chat_ui().editing_message_draft.clone(),
            )
        } else {
            (ai.chat_ui().input_focused, ai.chat_ui().draft.clone())
        };
        let marked_range = frame.marked_range;
        let selected_range = frame.selected_range;
        let showing_placeholder = draft.is_empty() && marked_range.is_none();
        let input_text = if showing_placeholder {
            placeholder
        } else {
            // IME composition is a virtual replacement of the draft selection.
            // Render that projection inline instead of appending marked text as
            // another visual line below the editor.
            frame.composed_text.unwrap_or(draft)
        };
        let caret_offset = selected_range
            .as_ref()
            .filter(|range| range.start == range.end)
            .map(|range| range.start);
        let wrap_width = frame.width;
        let visual_lines = if editing {
            ai_input_measured_visual_lines(
                &input_text,
                (wrap_width - frame.tokens.metrics.form_caret_width).max(1.0),
                gpui::font(frame.font),
                &gpui::WindowTextSystem::new(cx.text_system().clone()),
            )
        } else {
            ai_input_visual_lines(
                &input_text,
                ai_input_soft_wrap_columns(wrap_width + AI_INPUT_SOFT_WRAP_CHROME_PX),
            )
        };

        let caret_line =
            caret_offset.map(|offset| ai_input_line_index_for_offset(&visual_lines, offset));
        if editing && focused {
            let focus = marked_range
                .as_ref()
                .map(|range| range.end)
                .or(frame.active_offset)
                .unwrap_or_else(|| input_text.encode_utf16().count());
            let line = ai_input_line_index_for_offset(&visual_lines, focus);
            let position = (focus, line, wrap_width.to_bits() as usize);
            if ai
                .chat_ui()
                .editing_message_caret_position
                .replace(Some(position))
                != Some(position)
            {
                ai.chat_ui().editing_message_scroll.scroll_to_item(line);
            }
        }
        let mut input = div()
            .id(("ai-text-editor", frame.anchor.0))
            .w_full()
            .min_h(px(20.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(if showing_placeholder {
                rgba((frame.tokens.ui.text_muted << 8) | 0x4d)
            } else {
                rgb(frame.tokens.ui.text)
            })
            .opacity(if enabled { 1.0 } else { 0.5 })
            .cursor(CursorStyle::IBeam)
            .when(editing, |input| {
                input
                    .max_h(px(240.0))
                    .overflow_y_scroll()
                    .track_scroll(&ai.chat_ui().editing_message_scroll)
                    .on_scroll(|_, _, cx| cx.stop_propagation())
            });
        for (index, visual_line) in visual_lines.iter().enumerate() {
            let is_last_line = index + 1 == visual_lines.len();
            let line = visual_line.text;
            let line_len = visual_line.utf16_len();
            let line_range = visual_line.utf16_start..visual_line.utf16_end;
            let line_marked_range = marked_range
                .as_ref()
                .and_then(|marked| ai_input_local_marked_range(marked, &line_range));
            let line_selection = if showing_placeholder || marked_range.is_some() {
                None
            } else {
                selected_range.as_ref().and_then(|selection| {
                    let start = selection.start.max(line_range.start).min(line_range.end);
                    let end = selection.end.max(line_range.start).min(line_range.end);
                    (start < end).then_some(start - line_range.start..end - line_range.start)
                })
            };
            let line_caret = if showing_placeholder || marked_range.is_some() {
                None
            } else {
                caret_offset
                    .filter(|_| caret_line == Some(index))
                    .map(|offset| offset.saturating_sub(line_range.start).min(line_len))
            };
            input = input.child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(20.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .when(focused && showing_placeholder && index == 0, |line| {
                        line.child(text_caret(&frame.tokens, frame.caret_visible))
                    })
                    .child(ai_input_line_segments(
                        &frame.tokens,
                        line,
                        line_selection,
                        line_caret,
                        focused && frame.caret_visible,
                        line_marked_range,
                    ))
                    .when(
                        focused
                            && is_last_line
                            && !showing_placeholder
                            && selected_range.is_none()
                            && marked_range.is_none(),
                        |line| line.child(text_caret(&frame.tokens, frame.caret_visible)),
                    ),
            );
        }
        let input = input
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.state.update(cx, |ai, _cx| {
                        if editing {
                            ai.focus_message_edit();
                        } else {
                            ai.focus_chat_input();
                        }
                        ai.set_model_selector_search_focused(false);
                    });
                    (this.request)(
                        this.kind,
                        AiEditorInput::PointerDown(event.clone()),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    (this.request)(this.kind, AiEditorInput::Drag(event.clone()), window, cx);
                }),
            );
        let place_anchor = self.anchor.clone();
        text_input_anchor_probe(frame.anchor, input, move |anchor, window, cx| {
            place_anchor(anchor, window, cx)
        })
        .into_any_element()
    }
}

pub fn ai_input_line_segments(
    tokens: &oxideterm_theme::ThemeTokens,
    line: &str,
    selection_range: Option<std::ops::Range<usize>>,
    caret_offset: Option<usize>,
    caret_visible: bool,
    marked_range: Option<std::ops::Range<usize>>,
) -> Div {
    // Reuse the shared input renderer so caret and selection overlays never
    // split the editable line into separate layout text runs.
    let segments = if let Some(marked_range) = marked_range {
        text_input_value_segments_with_marked_range(tokens, line, marked_range)
    } else {
        text_input_value_segments_with_color(
            tokens,
            line,
            false,
            selection_range,
            caret_offset,
            caret_visible,
            Some(tokens.ui.text),
        )
    };
    segments.min_w_0().max_w_full().flex().items_center()
}

pub fn ai_input_local_marked_range(
    marked_range: &std::ops::Range<usize>,
    line_range: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    // A composition may cross a soft-wrap boundary. Each rendered line owns
    // only the intersecting UTF-16 segment of the virtual marked range.
    let start = marked_range.start.max(line_range.start);
    let end = marked_range.end.min(line_range.end);
    (start < end).then(|| start - line_range.start..end - line_range.start)
}

#[derive(Clone, Copy)]
pub struct AiInputVisualLine<'a> {
    text: &'a str,
    utf16_start: usize,
    utf16_end: usize,
}

impl AiInputVisualLine<'_> {
    pub fn utf16_len(&self) -> usize {
        self.utf16_end.saturating_sub(self.utf16_start)
    }

    pub fn utf16_range(&self) -> std::ops::Range<usize> {
        self.utf16_start..self.utf16_end
    }
}

pub fn ai_input_measured_visual_lines<'a>(
    input: &'a str,
    width: f32,
    font: gpui::Font,
    text_system: &gpui::WindowTextSystem,
) -> Vec<AiInputVisualLine<'a>> {
    let mut lines = Vec::new();
    let mut utf16_start = 0;
    for text in input.split('\n') {
        let shaped = text_system.shape_line(
            text.to_owned().into(),
            px(13.0),
            &[gpui::TextRun {
                len: text.len(),
                font: font.clone(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
                letter_spacing: None,
            }],
            None,
        );
        let mut byte_start = 0;
        let mut segment_start = utf16_start;
        let mut offset = utf16_start;
        let mut start_x = px(0.0);
        let mut previous_x = px(0.0);
        let mut glyphs = shaped.runs.iter().flat_map(|run| &run.glyphs).peekable();
        for (byte, ch) in text.char_indices() {
            // Walk shaped glyphs once; repeated x_for_index calls are quadratic for long lines.
            let end = byte + ch.len_utf8();
            while glyphs.peek().is_some_and(|glyph| glyph.index < end) {
                glyphs.next();
            }
            let end_x = glyphs
                .peek()
                .map(|glyph| glyph.position.x)
                .unwrap_or(shaped.width);
            if byte > byte_start && end_x - start_x > px(width) {
                lines.push(AiInputVisualLine {
                    text: &text[byte_start..byte],
                    utf16_start: segment_start,
                    utf16_end: offset,
                });
                byte_start = byte;
                segment_start = offset;
                start_x = previous_x;
            }
            offset += ch.len_utf16();
            previous_x = end_x;
        }
        lines.push(AiInputVisualLine {
            text: &text[byte_start..],
            utf16_start: segment_start,
            utf16_end: offset,
        });
        utf16_start = offset + 1;
    }
    lines
}

pub fn ai_input_line_index_for_offset(lines: &[AiInputVisualLine<'_>], offset: usize) -> usize {
    lines
        .iter()
        .rposition(|line| line.utf16_start <= offset)
        .unwrap_or(0)
}

pub const AI_INPUT_SOFT_WRAP_CHROME_PX: f32 = 56.0;
pub const AI_INPUT_SOFT_WRAP_HALF_WIDTH_PX: f32 = 7.0;
pub const AI_INPUT_SOFT_WRAP_MIN_COLUMNS: usize = 12;

pub fn ai_input_soft_wrap_columns(sidebar_width: f32) -> usize {
    let text_width = (sidebar_width - AI_INPUT_SOFT_WRAP_CHROME_PX).max(80.0);
    ((text_width / AI_INPUT_SOFT_WRAP_HALF_WIDTH_PX).floor() as usize)
        .max(AI_INPUT_SOFT_WRAP_MIN_COLUMNS)
}

pub fn ai_input_visual_lines(input: &str, wrap_columns: usize) -> Vec<AiInputVisualLine<'_>> {
    let wrap_columns = wrap_columns.max(AI_INPUT_SOFT_WRAP_MIN_COLUMNS);
    let mut visual_lines = Vec::new();
    let mut utf16_line_start = 0;

    for line in input.split('\n') {
        ai_push_wrapped_input_line(line, utf16_line_start, wrap_columns, &mut visual_lines);
        utf16_line_start += line.encode_utf16().count() + 1;
    }

    if visual_lines.is_empty() {
        visual_lines.push(AiInputVisualLine {
            text: "",
            utf16_start: 0,
            utf16_end: 0,
        });
    }
    visual_lines
}

pub fn ai_push_wrapped_input_line<'a>(
    line: &'a str,
    utf16_line_start: usize,
    wrap_columns: usize,
    visual_lines: &mut Vec<AiInputVisualLine<'a>>,
) {
    if line.is_empty() {
        visual_lines.push(AiInputVisualLine {
            text: line,
            utf16_start: utf16_line_start,
            utf16_end: utf16_line_start,
        });
        return;
    }

    let mut segment_byte_start = 0;
    let mut segment_utf16_start = utf16_line_start;
    let mut segment_columns = 0;
    let mut utf16_offset = utf16_line_start;

    for (byte_index, ch) in line.char_indices() {
        let char_columns = ai_input_char_columns(ch);
        if segment_columns > 0 && segment_columns + char_columns > wrap_columns {
            visual_lines.push(AiInputVisualLine {
                text: &line[segment_byte_start..byte_index],
                utf16_start: segment_utf16_start,
                utf16_end: utf16_offset,
            });
            segment_byte_start = byte_index;
            segment_utf16_start = utf16_offset;
            segment_columns = 0;
        }

        segment_columns += char_columns;
        utf16_offset += ch.len_utf16();
    }

    visual_lines.push(AiInputVisualLine {
        text: &line[segment_byte_start..],
        utf16_start: segment_utf16_start,
        utf16_end: utf16_offset,
    });
}

pub fn ai_input_char_columns(ch: char) -> usize {
    // GPUI does not expose textarea-style wrapping here, so this estimates
    // terminal-adjacent text width with UTF-16-safe boundaries for IME state.
    if ch == '\t' {
        4
    } else if ch.is_ascii() {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod input_render_tests {
    use super::ai_input_measured_visual_lines;

    #[gpui::test]
    fn history_editor_wraps_measured_text_without_losing_unicode_or_empty_lines(
        cx: &mut gpui::TestAppContext,
    ) {
        let system = gpui::WindowTextSystem::new(cx.update(|cx| cx.text_system().clone()));
        let font = gpui::font("Arial");
        let text = "W中😀\n\nZ";
        let narrow = ai_input_measured_visual_lines(text, 1.0, font.clone(), &system);
        assert_eq!(
            narrow
                .iter()
                .map(|line| (line.text, line.utf16_range()))
                .collect::<Vec<_>>(),
            vec![
                ("W", 0..1),
                ("中", 1..2),
                ("😀", 2..4),
                ("", 5..5),
                ("Z", 6..7)
            ]
        );
        let wide = ai_input_measured_visual_lines(text, 1000.0, font, &system);
        assert_eq!(
            wide.iter()
                .map(|line| (line.text, line.utf16_range()))
                .collect::<Vec<_>>(),
            vec![("W中😀", 0..4), ("", 5..5), ("Z", 6..7)]
        );
    }
}

impl AiEditorView {
    pub fn model_selected(ai: &oxideterm_settings::AiSettings) -> bool {
        if ai.active_backend == oxideterm_settings::AiActiveBackend::Acp {
            ai.active_acp_agent_id.is_some()
        } else {
            ai.active_provider_id.is_some()
                && oxideterm_ai::active_model_selection(ai.active_model.as_deref()).is_some()
        }
    }

    pub fn mount_pair(
        state: &Entity<AiWorkspaceEntity>,
        bindings: AiEditorBindings,
        cx: &mut App,
    ) -> (Entity<Self>, Entity<Self>) {
        let composer = gpui::AppContext::new(cx, |_| {
            Self::new(AiEditorKind::Composer, state.clone(), bindings.clone())
        });
        let edit = gpui::AppContext::new(cx, |_| {
            Self::new(AiEditorKind::MessageEdit, state.clone(), bindings)
        });
        (composer, edit)
    }
}
