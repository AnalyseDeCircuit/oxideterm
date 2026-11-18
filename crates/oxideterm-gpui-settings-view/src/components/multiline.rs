use gpui::{
    AnyElement, CursorStyle, MouseButton, ScrollHandle, SharedString, div, prelude::*, px, rgb,
    rgba,
};
use oxideterm_gpui_ui::text_input::{
    TextInputAnchorId, text_input_anchor_probe, text_input_value_segments,
};
use oxideterm_settings_model::{settings_multiline_line_ranges, settings_multiline_line_selection};
use oxideterm_theme::ThemeTokens;

use crate::SettingsInput;

use super::{SettingsImeFrame, SettingsWindowDispatch, SettingsWindowRequest};

pub fn settings_multiline_field(
    tokens: &ThemeTokens,
    frame: SettingsImeFrame<'_>,
    mono_font: SharedString,
    scroll: &ScrollHandle,
    dispatch: SettingsWindowDispatch,
) -> AnyElement {
    let (height, placeholder, monospace, document) = match frame.input {
        SettingsInput::ManagedKeyPastePrivateKey => {
            (160.0, "-----BEGIN OPENSSH PRIVATE KEY-----", true, false)
        }
        SettingsInput::TerminalCommandBarFocusHandoff => (44.0, "aider, custom-tui", true, false),
        SettingsInput::TerminalCommandSpecsJson => (520.0, "", true, true),
        _ => (80.0, "", false, false),
    };
    render_multiline_field(
        tokens,
        frame,
        height,
        placeholder,
        monospace.then_some(mono_font),
        document,
        scroll,
        dispatch,
    )
}

pub fn settings_multiline_patterns_field(
    tokens: &ThemeTokens,
    frame: SettingsImeFrame<'_>,
    placeholder: &str,
    scroll: &ScrollHandle,
    dispatch: SettingsWindowDispatch,
) -> AnyElement {
    render_multiline_field(
        tokens,
        frame,
        80.0,
        placeholder,
        None,
        false,
        scroll,
        dispatch,
    )
}

fn render_multiline_field(
    tokens: &ThemeTokens,
    frame: SettingsImeFrame<'_>,
    min_height: f32,
    placeholder: &str,
    font: Option<SharedString>,
    document: bool,
    scroll: &ScrollHandle,
    dispatch: SettingsWindowDispatch,
) -> AnyElement {
    let input = frame.input;
    let line_height = input.textarea_line_height();
    let theme = tokens.ui;
    let mut textarea = div()
        .w_auto()
        .min_w_full()
        .min_h(px(min_height))
        .flex_shrink_0()
        .whitespace_nowrap()
        .rounded(px(tokens.radii.md))
        .border_1()
        .border_color(if frame.focused {
            rgba((theme.accent << 8) | 0x99)
        } else {
            rgb(theme.border)
        })
        .bg(rgb(theme.bg))
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(0.0))
        .cursor(CursorStyle::IBeam)
        .text_size(px(tokens.metrics.ui_text_sm))
        .line_height(px(line_height))
        .when_some(font, |field, font| field.font_family(font))
        .text_color(rgb(theme.text));
    let show_placeholder = frame.value.is_empty() && !document;
    let value = if show_placeholder {
        placeholder
    } else {
        frame.value
    };
    // The IME hit-tests physical lines; wrapping would move the caret away from the pointer.
    for (line_range, line_text) in settings_multiline_line_ranges(value) {
        let (selection, caret) =
            settings_multiline_line_selection(frame.selected_range.as_ref(), &line_range);
        let mut line = div()
            .h(px(line_height))
            .min_h(px(line_height))
            .flex_shrink_0()
            .whitespace_nowrap();
        if show_placeholder {
            line = line
                .text_color(rgb(theme.text_muted))
                .child(line_text.as_str().to_string());
        } else {
            line = line.child(text_input_value_segments(
                tokens,
                &line_text,
                false,
                selection,
                caret,
                frame.caret_visible,
            ));
        }
        textarea = textarea.child(line);
    }
    // Private-key paste never paints a separate copy of the marked text.
    if input != SettingsInput::ManagedKeyPastePrivateKey
        && let Some(marked) = frame.marked_text
    {
        textarea = textarea.child(
            div()
                .underline()
                .text_color(rgb(theme.text))
                .child(marked.to_string()),
        );
    }
    let select = dispatch.clone();
    let textarea = textarea
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            select(
                SettingsWindowRequest::BeginInputSelection(input, event.clone()),
                window,
                cx,
            );
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, window, cx| {
            dispatch(
                SettingsWindowRequest::DragInputSelection(event.clone()),
                window,
                cx,
            );
        });

    let anchor_id = frame.anchor_id;
    let unscrolled = text_input_anchor_probe(anchor_id, textarea, move |anchor, _, _| {
        (frame.update_anchor)(anchor)
    });
    multiline_input_viewport(anchor_id, unscrolled.into_any_element(), document, scroll)
}

pub fn multiline_input_viewport(
    anchor_id: TextInputAnchorId,
    content: AnyElement,
    document: bool,
    scroll: &ScrollHandle,
) -> AnyElement {
    div()
        .relative()
        .w_full()
        .min_w_0()
        .child(
            div()
                .id(("settings-multiline-viewport", anchor_id.0))
                .w_full()
                .min_w_0()
                .when(!document, |area| area.max_h(px(240.0)))
                .flex()
                .items_start()
                .overflow_scroll()
                .track_scroll(scroll)
                .child(content),
        )
        .child(
            oxideterm_gpui_ui::Scrollbar::new(scroll).axis(oxideterm_gpui_ui::ScrollbarAxis::Both),
        )
        .into_any_element()
}
