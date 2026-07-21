// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/// Couples the shared text-input renderer to GPUI's platform text-input
/// contract. The renderer intentionally owns no editing state, so modal fields
/// must register an input handler during paint instead of synthesizing text
/// solely from key-down events.
struct TreeNameInputElement {
    child: Option<AnyElement>,
    surface: Entity<IdeSurface>,
    focus_handle: FocusHandle,
}

impl TreeNameInputElement {
    fn new(
        child: impl IntoElement,
        surface: Entity<IdeSurface>,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            child: Some(child.into_any_element()),
            surface,
            focus_handle,
        }
    }
}

impl IntoElement for TreeNameInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for TreeNameInputElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let layout_id = self
            .child
            .as_mut()
            .expect("tree name input should render once")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
        window.handle_input(
            &self.focus_handle,
            gpui::ElementInputHandler::new(bounds, self.surface.clone()),
            cx,
        );
    }
}

impl gpui::EntityInputHandler for IdeSurface {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.tree_name_text_with_marked()?;
        let text_len = text.encode_utf16().count();
        let range = range_utf16.start.min(text_len)..range_utf16.end.min(text_len);
        *adjusted_range = Some(range.clone());
        Some(tree_name_utf16_slice(&text, range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::UTF16Selection> {
        let input = self.tree_name_input.as_ref()?;
        let end = input.value.encode_utf16().count();
        Some(gpui::UTF16Selection {
            range: input.selection_range.clone().unwrap_or(end..end),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let marked = self.tree_name_input.as_ref()?.marked_text.as_ref()?;
        let start = marked.replacement_range.start;
        Some(start..start + marked.text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.tree_name_input.as_mut()
            && input.marked_text.take().is_some()
        {
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.tree_name_input.as_mut() else {
            return;
        };
        if input.submitting {
            return;
        }
        let fallback_end = input.value.encode_utf16().count();
        let range = input
            .marked_text
            .take()
            .map(|marked| marked.replacement_range)
            .or(range_utf16)
            .or_else(|| input.selection_range.clone())
            .unwrap_or(fallback_end..fallback_end);
        replace_tree_name_range(input, range, text);
        input.error = validate_file_name(input.value.trim());
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.tree_name_input.as_mut() else {
            return;
        };
        if input.submitting {
            return;
        }
        if new_text.is_empty() {
            if input.marked_text.take().is_some() {
                cx.notify();
            }
            return;
        }
        let fallback_end = input.value.encode_utf16().count();
        let replacement_range = input
            .marked_text
            .as_ref()
            .map(|marked| marked.replacement_range.clone())
            .or(range_utf16)
            .or_else(|| input.selection_range.clone())
            .unwrap_or(fallback_end..fallback_end);
        input.selection_range = Some(replacement_range.clone());
        input.marked_text = Some(TreeNameMarkedText {
            replacement_range,
            text: new_text.to_string(),
        });
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(Bounds {
            origin: element_bounds.origin + gpui::point(px(0.0), element_bounds.size.height),
            size: element_bounds.size,
        })
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        self.tree_name_input
            .as_ref()
            .is_some_and(|input| !input.submitting)
    }
}

impl IdeSurface {
    fn tree_name_text_with_marked(&self) -> Option<String> {
        let input = self.tree_name_input.as_ref()?;
        let Some(marked) = input.marked_text.as_ref() else {
            return Some(input.value.clone());
        };
        let mut text = input.value.clone();
        let start = utf16_offset_to_byte(&text, marked.replacement_range.start);
        let end = utf16_offset_to_byte(&text, marked.replacement_range.end);
        text.replace_range(start..end, &marked.text);
        Some(text)
    }
}

fn tree_name_utf16_slice(text: &str, range: Range<usize>) -> String {
    let start = utf16_offset_to_byte(text, range.start);
    let end = utf16_offset_to_byte(text, range.end);
    text[start..end].to_string()
}
