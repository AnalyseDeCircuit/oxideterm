use gpui::{
    AnyElement, App, Bounds, Div, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, ParentElement, Pixels, Styled, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

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
    pub marked_text: Option<&'a str>,
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
            window.on_next_frame(move |window, cx| on_bounds(anchor, window, cx));
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
    let theme = tokens.ui;
    let empty = view.value.is_empty();
    let marked = view.marked_text.unwrap_or_default();
    let display = if empty && marked.is_empty() {
        view.placeholder
    } else if empty {
        String::new()
    } else if view.secret {
        "•".repeat(view.value.chars().count())
    } else {
        view.value.to_string()
    };
    let marked_display = if view.secret {
        "•".repeat(marked.chars().count())
    } else {
        marked.to_string()
    };
    let visually_empty = empty && marked.is_empty();
    let show_selection = view.focused && view.selected_all && !empty && marked.is_empty();

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
        .cursor_pointer()
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .when(view.focused && visually_empty, |row| {
                    row.child(text_caret(tokens, view.caret_visible))
                })
                .child(
                    div()
                        .when(show_selection, |text| {
                            text.px(px(tokens.metrics.form_selection_padding_x))
                                .rounded(px(tokens.radii.xs))
                                .bg(rgb(theme.accent))
                                .text_color(rgb(theme.accent_text))
                        })
                        .text_color(if visually_empty {
                            rgb(theme.text_muted)
                        } else if show_selection {
                            rgb(theme.accent_text)
                        } else {
                            rgb(theme.text)
                        })
                        .child(display),
                )
                .when(view.focused && !marked.is_empty(), |row| {
                    row.child(
                        div()
                            .underline()
                            .text_color(rgb(theme.text))
                            .child(marked_display),
                    )
                })
                .when(view.focused && !visually_empty && !show_selection, |row| {
                    row.child(text_caret(tokens, view.caret_visible))
                }),
        )
}

pub fn text_caret(tokens: &ThemeTokens, visible: bool) -> Div {
    div()
        .w(px(tokens.metrics.form_caret_width))
        .h(px(tokens.metrics.form_caret_height))
        .bg(rgb(tokens.ui.accent))
        .opacity(if visible { 1.0 } else { 0.0 })
}
