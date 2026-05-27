use std::ops::Range;

use gpui::{
    AnyElement, App, Bounds, CursorStyle, Div, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ParentElement, Pixels, Styled, Window, div,
    prelude::*, px, rgb, rgba,
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
    let input_range = if view.focused && !empty && marked.is_empty() {
        view.selected_range.or_else(|| {
            view.selected_all
                .then_some(0..view.value.encode_utf16().count())
        })
    } else {
        None
    }
    .map(|range| text_input_visual_range(view.value, view.secret, range));
    let selection_range = input_range.clone().filter(|range| range.start < range.end);
    let caret_offset = input_range
        .as_ref()
        .filter(|range| range.start == range.end)
        .map(|range| range.start);
    let show_selection = selection_range.is_some();
    let show_positioned_caret = caret_offset.is_some() && !show_selection;

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
            .child(text_input_value_segments(
                tokens,
                &display,
                visually_empty,
                selection_range,
                caret_offset,
                view.caret_visible,
            ))
            .when(view.focused && !marked.is_empty(), |row| {
                row.child(
                    div()
                        .underline()
                        .text_color(rgb(theme.text))
                        .child(marked_display),
                )
            })
            .when(
                view.focused && !visually_empty && !show_selection && !show_positioned_caret,
                |row| row.child(text_caret(tokens, view.caret_visible)),
            )
        })
}

pub fn text_caret(tokens: &ThemeTokens, visible: bool) -> Div {
    div()
        .relative()
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

    let Some(range) = selection_range else {
        if let Some(offset) = caret_offset {
            return text_input_value_with_caret(tokens, base, display, offset, caret_visible);
        }
        return base.child(display.to_string());
    };

    let len = display.encode_utf16().count();
    let start = range.start.min(len);
    let end = range.end.min(len);
    if start >= end {
        return base.child(display.to_string());
    }

    let before = utf16_slice(display, 0..start);
    let selected = utf16_slice(display, start..end);
    let after = utf16_slice(display, end..len);

    base.flex()
        .flex_row()
        .items_center()
        .when(!before.is_empty(), |row| row.child(before))
        .child(
            div()
                .rounded(px(tokens.radii.xs))
                .bg(rgba((theme.accent << 8) | TEXT_INPUT_SELECTION_BG_ALPHA))
                .text_color(rgb(theme.text))
                .child(selected),
        )
        .when(!after.is_empty(), |row| row.child(after))
}

fn text_input_value_with_caret(
    tokens: &ThemeTokens,
    base: Div,
    display: &str,
    offset: usize,
    caret_visible: bool,
) -> Div {
    let len = display.encode_utf16().count();
    let offset = offset.min(len);
    let before = utf16_slice(display, 0..offset);
    let after = utf16_slice(display, offset..len);

    base.flex()
        .flex_row()
        .items_center()
        .when(!before.is_empty(), |row| row.child(before))
        .child(text_caret(tokens, caret_visible))
        .when(!after.is_empty(), |row| row.child(after))
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
    use super::{text_input_secret_mask, text_input_visual_range};

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
}
