// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/// Couples shared text-input renderers to GPUI's platform text-input contract.
/// The renderer intentionally owns no editing state, so IDE fields register
/// the surface input handler during paint.
struct IdeSurfaceInputElement {
    child: Option<AnyElement>,
    surface: Entity<IdeSurface>,
    focus_handle: FocusHandle,
}

impl IdeSurfaceInputElement {
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

impl IntoElement for IdeSurfaceInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for IdeSurfaceInputElement {
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
            .expect("IDE surface input should render once")
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
        let text = self.active_ide_input_text_with_marked()?;
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
        let (value, selection_range, reversed) = if let Some(input) = self.tree_name_input.as_ref() {
            (&input.value, input.selection_range.clone(), false)
        } else if self.search.open {
            (&self.search.query, self.search.selection_range.clone(), false)
        } else if self.folder_picker.open && self.folder_picker.path_input_focused {
            (
                &self.folder_picker.path_input,
                self.folder_picker.path_selection_range.clone(),
                self.folder_picker.path_selection_reversed,
            )
        } else {
            return None;
        };
        let end = value.encode_utf16().count();
        Some(gpui::UTF16Selection {
            range: selection_range.unwrap_or(end..end),
            reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let marked = if let Some(input) = self.tree_name_input.as_ref() {
            input.marked_text.as_ref()
        } else if self.search.open {
            self.search.marked_text.as_ref()
        } else if self.folder_picker.open && self.folder_picker.path_input_focused {
            self.folder_picker.path_marked_text.as_ref()
        } else {
            None
        }?;
        let start = marked.replacement_range.start;
        Some(start..start + marked.text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.tree_name_input.as_mut()
            && input.marked_text.take().is_some()
        {
            cx.notify();
        } else if self.search.open && self.search.marked_text.take().is_some() {
            cx.notify();
        } else if self.folder_picker.open
            && self.folder_picker.path_input_focused
            && self.folder_picker.path_marked_text.take().is_some()
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
        if let Some(input) = self.tree_name_input.as_mut() {
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
            return;
        }
        if self.search.open {
            let fallback_end = self.search.query.encode_utf16().count();
            let range = self
                .search
                .marked_text
                .take()
                .map(|marked| marked.replacement_range)
                .or(range_utf16)
                .or_else(|| self.search.selection_range.clone())
                .unwrap_or(fallback_end..fallback_end);
            replace_project_search_range(&mut self.search, range, text);
            self.schedule_project_search(cx);
            cx.notify();
            return;
        }
        if self.folder_picker.open && self.folder_picker.path_input_focused {
            let fallback_end = self.folder_picker.path_input.encode_utf16().count();
            let range = self
                .folder_picker
                .path_marked_text
                .take()
                .map(|marked| marked.replacement_range)
                .or(range_utf16)
                .or_else(|| self.folder_picker.path_selection_range.clone())
                .unwrap_or(fallback_end..fallback_end);
            replace_folder_picker_range(&mut self.folder_picker, range, text);
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.tree_name_input.as_mut() {
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
            input.marked_text = Some(IdeMarkedText {
                replacement_range,
                text: new_text.to_string(),
            });
            cx.notify();
            return;
        }
        if self.search.open {
            if new_text.is_empty() {
                if self.search.marked_text.take().is_some() {
                    cx.notify();
                }
                return;
            }
            let fallback_end = self.search.query.encode_utf16().count();
            let replacement_range = self
                .search
                .marked_text
                .as_ref()
                .map(|marked| marked.replacement_range.clone())
                .or(range_utf16)
                .or_else(|| self.search.selection_range.clone())
                .unwrap_or(fallback_end..fallback_end);
            self.search.selection_range = Some(replacement_range.clone());
            self.search.marked_text = Some(IdeMarkedText {
                replacement_range,
                text: new_text.to_string(),
            });
            cx.notify();
            return;
        }
        if self.folder_picker.open && self.folder_picker.path_input_focused {
            if new_text.is_empty() {
                if self.folder_picker.path_marked_text.take().is_some() {
                    cx.notify();
                }
                return;
            }
            let fallback_end = self.folder_picker.path_input.encode_utf16().count();
            let replacement_range = self
                .folder_picker
                .path_marked_text
                .as_ref()
                .map(|marked| marked.replacement_range.clone())
                .or(range_utf16)
                .or_else(|| self.folder_picker.path_selection_range.clone())
                .unwrap_or(fallback_end..fallback_end);
            self.folder_picker.path_selection_range = Some(replacement_range.clone());
            self.folder_picker.path_selection_reversed = false;
            self.folder_picker.path_selection_drag_anchor = None;
            self.folder_picker.path_marked_text = Some(IdeMarkedText {
                replacement_range,
                text: new_text.to_string(),
            });
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.folder_picker.open && self.folder_picker.path_input_focused {
            return Some(self.folder_picker_path_bounds_for_range(
                range_utf16,
                element_bounds,
                window,
            ));
        }
        Some(Bounds {
            origin: element_bounds.origin + gpui::point(px(0.0), element_bounds.size.height),
            size: element_bounds.size,
        })
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.folder_picker.open && self.folder_picker.path_input_focused {
            self.folder_picker_path_index_for_position(point, window)
        } else {
            None
        }
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        self.tree_name_input
            .as_ref()
            .is_some_and(|input| !input.submitting)
            || self.search.open
            || (self.folder_picker.open && self.folder_picker.path_input_focused)
    }
}

impl IdeSurface {
    fn active_ide_input_text_with_marked(&self) -> Option<String> {
        if self.tree_name_input.is_some() {
            self.tree_name_text_with_marked()
        } else if self.search.open {
            Some(self.project_search_text_with_marked())
        } else if self.folder_picker.open && self.folder_picker.path_input_focused {
            Some(self.folder_picker_path_text_with_marked())
        } else {
            None
        }
    }

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

    fn project_search_text_with_marked(&self) -> String {
        let Some(marked) = self.search.marked_text.as_ref() else {
            return self.search.query.clone();
        };
        let mut text = self.search.query.clone();
        let start = utf16_offset_to_byte(&text, marked.replacement_range.start);
        let end = utf16_offset_to_byte(&text, marked.replacement_range.end);
        text.replace_range(start..end, &marked.text);
        text
    }

    fn folder_picker_path_text_with_marked(&self) -> String {
        folder_picker_path_text_with_marked(&self.folder_picker)
    }
}

fn tree_name_utf16_slice(text: &str, range: Range<usize>) -> String {
    let start = utf16_offset_to_byte(text, range.start);
    let end = utf16_offset_to_byte(text, range.end);
    text[start..end].to_string()
}
