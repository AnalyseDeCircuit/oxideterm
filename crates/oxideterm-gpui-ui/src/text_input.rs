use std::ops::Range;

use gpui::{
    AnyElement, App, Bounds, CursorStyle, Div, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ParentElement, Pixels, Styled, StyledText, Window,
    div, fill, point, prelude::*, px, rgb, rgba, size,
};
use oxideterm_theme::ThemeTokens;

const TEXT_INPUT_SELECTION_BG_ALPHA: u32 = 0x40; // Tauri ::selection uses theme-selection at ~25%.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TextInputAnchorId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputAnchor {
    pub id: TextInputAnchorId,
    pub bounds: Bounds<Pixels>,
}

type TextInputBoundsCallback = Box<dyn FnOnce(TextInputAnchor, &mut Window, &mut App)>;

pub struct TextInputAnchorProbe {
    id: TextInputAnchorId,
    child: Option<AnyElement>,
    on_bounds: Option<TextInputBoundsCallback>,
}

pub struct TextInputView<'a> {
    pub value: &'a str,
    pub placeholder: String,
    pub focused: bool,
    pub caret_visible: bool,
    pub secret: bool,
    pub selected_all: bool,
    pub selected_range: Option<Range<usize>>,
    pub marked_text: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextInputContentAlign {
    Start,
    Center,
}

pub fn text_input_anchor_probe(
    id: TextInputAnchorId,
    child: impl IntoElement,
    on_bounds: impl FnOnce(TextInputAnchor, &mut Window, &mut App) + 'static,
) -> TextInputAnchorProbe {
    TextInputAnchorProbe {
        id,
        child: Some(child.into_any_element()),
        on_bounds: Some(Box::new(on_bounds)),
    }
}

impl IntoElement for TextInputAnchorProbe {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextInputAnchorProbe {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = self
            .child
            .as_mut()
            .expect("text input anchor child should render once")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        if let Some(on_bounds) = self.on_bounds.take() {
            let anchor = TextInputAnchor {
                id: self.id,
                bounds,
            };
            // Keep text input anchors in the same draw pass as their trigger.
            // Deferring this by a frame makes anchored UI drift when the trigger
            // lives inside a scrolling or resizing container.
            on_bounds(anchor, window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

pub fn text_input(tokens: &ThemeTokens, view: TextInputView<'_>) -> Div {
    text_input_with_content_align(tokens, view, TextInputContentAlign::Start)
}

pub fn text_input_with_content_align(
    tokens: &ThemeTokens,
    view: TextInputView<'_>,
    align: TextInputContentAlign,
) -> Div {
    let theme = tokens.ui;
    let empty = view.value.is_empty();
    let marked = view.marked_text.unwrap_or_default();
    let display = if empty && marked.is_empty() {
        view.placeholder
    } else if empty {
        String::new()
    } else if view.secret {
        text_input_secret_mask(view.value)
    } else {
        view.value.to_string()
    };
    let marked_display = if view.secret {
        text_input_secret_mask(marked)
    } else {
        marked.to_string()
    };
    let visually_empty = empty && marked.is_empty();
    let raw_input_range = if view.focused {
        view.selected_range.or_else(|| {
            view.selected_all
                .then_some(0..view.value.encode_utf16().count())
        })
    } else {
        None
    };
    let input_range = if !empty && marked.is_empty() {
        raw_input_range
            .clone()
            .map(|range| text_input_visual_range(view.value, view.secret, range))
    } else {
        None
    };
    let marked_range = if view.focused && !marked.is_empty() {
        let end = view.value.encode_utf16().count();
        let range = raw_input_range.clone().unwrap_or(end..end);
        Some(text_input_visual_range(
            view.value,
            view.secret,
            range.start.min(end)..range.end.min(end),
        ))
    } else {
        None
    };
    let selection_range = input_range.clone().filter(|range| range.start < range.end);
    let caret_offset = input_range
        .as_ref()
        .filter(|range| range.start == range.end)
        .map(|range| range.start);
    let show_selection = selection_range.is_some();
    let show_positioned_caret = caret_offset.is_some() && !show_selection;
    let show_marked_text = marked_range.is_some();

    div()
        .h(px(tokens.metrics.ui_control_height))
        .px(px(tokens.metrics.ui_control_padding_x))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .bg(rgba((theme.bg << 8) | 0x80))
        .border_1()
        .border_color(if view.focused {
            rgb(theme.accent)
        } else {
            rgb(theme.border)
        })
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(if visually_empty {
            rgb(theme.text_muted)
        } else {
            rgb(theme.text)
        })
        .cursor(CursorStyle::IBeam)
        .overflow_hidden()
        .child({
            // Browser inputs align text inside the padded control box. GPUI
            // text is composed from segments for selection/caret support, so
            // centered number fields need the segment row to span the input.
            let row = div()
                .flex()
                .flex_row()
                .items_center()
                .when(align == TextInputContentAlign::Center, |row| {
                    row.w_full().justify_center()
                });

            row.when(view.focused && visually_empty, |row| {
                row.child(text_caret(tokens, view.caret_visible))
            })
            .child({
                if let Some(marked_range) = marked_range {
                    text_input_value_segments_with_marked_text(
                        tokens,
                        &display,
                        &marked_display,
                        marked_range,
                    )
                } else {
                    text_input_value_segments(
                        tokens,
                        &display,
                        visually_empty,
                        selection_range,
                        caret_offset,
                        view.caret_visible,
                    )
                }
            })
            .when(
                view.focused
                    && !visually_empty
                    && !show_selection
                    && !show_positioned_caret
                    && !show_marked_text,
                |row| row.child(text_caret(tokens, view.caret_visible)),
            )
        })
}

pub fn text_caret(tokens: &ThemeTokens, visible: bool) -> Div {
    div()
        .relative()
        .flex_none()
        .w(px(0.0))
        .h(px(tokens.metrics.form_caret_height))
        // Browser carets are painted over the text flow. The zero-width anchor
        // keeps GPUI flex rows from measuring the caret as an extra character.
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(px(tokens.metrics.form_caret_width))
                .h(px(tokens.metrics.form_caret_height))
                .bg(rgb(tokens.ui.accent))
                .opacity(if visible { 1.0 } else { 0.0 }),
        )
}

pub fn text_input_value_segments(
    tokens: &ThemeTokens,
    display: &str,
    visually_empty: bool,
    selection_range: Option<Range<usize>>,
    caret_offset: Option<usize>,
    caret_visible: bool,
) -> Div {
    text_input_value_segments_with_color(
        tokens,
        display,
        visually_empty,
        selection_range,
        caret_offset,
        caret_visible,
        None,
    )
}

fn text_input_value_segments_with_marked_text(
    tokens: &ThemeTokens,
    display: &str,
    marked_display: &str,
    marked_range: Range<usize>,
) -> Div {
    let theme = tokens.ui;
    let (before, after) = text_input_marked_display_parts(display, marked_range);

    div()
        .flex()
        .flex_row()
        .items_center()
        .text_color(rgb(theme.text))
        .when(!before.is_empty(), |row| row.child(before))
        .child(
            div()
                .underline()
                .text_color(rgb(theme.text))
                .child(marked_display.to_string()),
        )
        .when(!after.is_empty(), |row| row.child(after))
}

fn text_input_marked_display_parts(display: &str, marked_range: Range<usize>) -> (String, String) {
    let len = display.encode_utf16().count();
    let start = marked_range.start.min(len);
    let end = marked_range.end.min(len);
    (
        utf16_slice(display, 0..start),
        utf16_slice(display, end..len),
    )
}

pub fn text_input_value_segments_with_color(
    tokens: &ThemeTokens,
    display: &str,
    visually_empty: bool,
    selection_range: Option<Range<usize>>,
    caret_offset: Option<usize>,
    caret_visible: bool,
    text_color: Option<u32>,
) -> Div {
    let theme = tokens.ui;
    let base = div().text_color(if visually_empty {
        rgb(theme.text_muted)
    } else {
        rgb(text_color.unwrap_or(theme.text))
    });
    let len = display.encode_utf16().count();
    let selection_range = selection_range.and_then(|range| text_input_clamped_range(len, range));
    let caret_offset = caret_offset.map(|offset| offset.min(len));

    if selection_range.is_none() && caret_offset.is_none() {
        return base.child(display.to_string());
    }
    let has_selection = selection_range.is_some();
    let caret_offset = if has_selection { None } else { caret_offset };

    text_input_value_with_overlays(
        tokens,
        base,
        display,
        selection_range,
        caret_offset,
        caret_visible,
    )
}

fn text_input_value_with_overlays(
    tokens: &ThemeTokens,
    base: Div,
    display: &str,
    selection_range: Option<Range<usize>>,
    caret_offset: Option<usize>,
    caret_visible: bool,
) -> Div {
    base.min_w_0().child(TextInputOverlayValue::new(
        tokens,
        display,
        selection_range,
        caret_offset,
        caret_visible,
    ))
}

struct TextInputOverlayValue {
    display: String,
    text: StyledText,
    selection_range: Option<Range<usize>>,
    caret_offset: Option<usize>,
    caret_visible: bool,
    accent: u32,
    caret_width: f32,
    caret_height: f32,
}

impl TextInputOverlayValue {
    fn new(
        tokens: &ThemeTokens,
        display: &str,
        selection_range: Option<Range<usize>>,
        caret_offset: Option<usize>,
        caret_visible: bool,
    ) -> Self {
        Self {
            display: display.to_string(),
            text: StyledText::new(display.to_string()),
            selection_range,
            caret_offset,
            caret_visible,
            accent: tokens.ui.accent,
            caret_width: tokens.metrics.form_caret_width,
            caret_height: tokens.metrics.form_caret_height,
        }
    }

    fn byte_index_for_offset(&self, offset: usize) -> usize {
        byte_index_for_utf16(&self.display, offset)
    }
}

impl IntoElement for TextInputOverlayValue {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextInputOverlayValue {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(id, inspector_id, bounds, request_layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.text.layout();
        let line_height = layout.line_height();
        let overlay_height = px(self.caret_height).min(line_height);
        let overlay_y = bounds.origin.y + (line_height - overlay_height) / 2.0;

        if let Some(range) = self.selection_range.as_ref() {
            let start_index = self.byte_index_for_offset(range.start);
            let end_index = self.byte_index_for_offset(range.end);
            if let (Some(start), Some(end)) = (
                layout.position_for_index(start_index),
                layout.position_for_index(end_index),
            ) {
                let left = start.x.min(end.x);
                let width = (end.x - start.x).abs().max(px(self.caret_width));
                // Selection is painted beneath the shaped text so it never
                // changes layout width or kerning.
                window.paint_quad(fill(
                    Bounds {
                        origin: point(left, overlay_y),
                        size: size(width, overlay_height),
                    },
                    rgba((self.accent << 8) | TEXT_INPUT_SELECTION_BG_ALPHA),
                ));
            }
        }

        let caret_bounds = if self.caret_visible {
            self.caret_offset
                .and_then(|offset| layout.position_for_index(self.byte_index_for_offset(offset)))
                .map(|position| Bounds {
                    origin: point(position.x, overlay_y),
                    size: size(px(self.caret_width), overlay_height),
                })
        } else {
            None
        };

        self.text.paint(
            id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );

        if let Some(caret_bounds) = caret_bounds {
            window.paint_quad(fill(caret_bounds, rgb(self.accent)));
        }
    }
}

fn text_input_clamped_range(len: usize, range: Range<usize>) -> Option<Range<usize>> {
    let start = range.start.min(len);
    let end = range.end.min(len);
    (start < end).then_some(start..end)
}

fn utf16_slice(value: &str, range: Range<usize>) -> String {
    let start = byte_index_for_utf16(value, range.start);
    let end = byte_index_for_utf16(value, range.end);
    value[start..end].to_string()
}

fn byte_index_for_utf16(value: &str, offset: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_index, ch) in value.char_indices() {
        if utf16_count >= offset {
            return byte_index;
        }
        utf16_count += ch.len_utf16();
    }
    value.len()
}

pub fn text_input_secret_mask(value: &str) -> String {
    "•".repeat(value.chars().count())
}

/// Convert an IME UTF-16 range from the real input value into the range used by
/// the rendered text. Password fields draw one bullet per scalar value, so the
/// raw range cannot be applied directly to the masked display string.
pub fn text_input_visual_range(raw_value: &str, secret: bool, range: Range<usize>) -> Range<usize> {
    if secret {
        secret_mask_offset_for_utf16(raw_value, range.start)
            ..secret_mask_offset_for_utf16(raw_value, range.end)
    } else {
        range
    }
}

fn secret_mask_offset_for_utf16(raw_value: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut mask_offset = 0;
    for ch in raw_value.chars() {
        if utf16_offset >= offset {
            return mask_offset;
        }
        let next_utf16_offset = utf16_offset + ch.len_utf16();
        if offset < next_utf16_offset {
            return mask_offset;
        }
        utf16_offset = next_utf16_offset;
        mask_offset += 1;
    }
    mask_offset
}

#[cfg(test)]
mod tests {
    use super::{text_input_marked_display_parts, text_input_secret_mask, text_input_visual_range};

    #[test]
    fn secret_mask_uses_one_visible_glyph_per_scalar() {
        assert_eq!(text_input_secret_mask("ab"), "••");
        assert_eq!(text_input_secret_mask("a🔒b"), "•••");
    }

    #[test]
    fn secret_visual_range_maps_utf16_offsets_to_mask_offsets() {
        let value = "a🔒b";

        assert_eq!(text_input_visual_range(value, true, 0..4), 0..3);
        assert_eq!(text_input_visual_range(value, true, 1..3), 1..2);
        assert_eq!(text_input_visual_range(value, true, 3..3), 2..2);
        assert_eq!(text_input_visual_range(value, false, 1..3), 1..3);
    }

    #[test]
    fn marked_display_parts_insert_at_utf16_position() {
        assert_eq!(
            text_input_marked_display_parts("abcd", 2..2),
            ("ab".to_string(), "cd".to_string())
        );
    }

    #[test]
    fn marked_display_parts_replace_selected_utf16_range() {
        assert_eq!(
            text_input_marked_display_parts("a🔒b", 1..3),
            ("a".to_string(), "b".to_string())
        );
    }
}
