use std::{ops::Range, rc::Rc};

use gpui::{AnyElement, App, Div, IntoElement, Rgba, Styled, px, svg};
use oxideterm_gpui_ui::text_input::{
    TextInputAnchor, TextInputAnchorId, TextInputContentAlign, TextInputView, TextInputViewport,
    text_input_with_viewport_and_content_align,
};
use oxideterm_theme::ThemeTokens;

use crate::{FilesystemSearchFrame, HostToolsTextInput};

/// Presentation for a non-secret input selected by the window. The window
/// owns focus, composition, and the anchor store.
#[derive(Clone)]
pub struct HostToolsPlainTextImeFrame {
    pub input: HostToolsTextInput,
    pub anchor_id: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub caret_visible: bool,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    pub marked_text: Option<String>,
    pub update_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

impl HostToolsPlainTextImeFrame {
    pub(crate) fn text_input(&self, tokens: &ThemeTokens, view: TextInputView<'_>) -> Div {
        text_input_with_viewport_and_content_align(
            tokens,
            view,
            TextInputContentAlign::Start,
            &self.viewport,
            self.active_offset,
        )
    }

    pub(crate) fn input(&self) -> HostToolsTextInput {
        self.input
    }

    pub(crate) fn anchor_id(&self) -> TextInputAnchorId {
        self.anchor_id
    }

    pub(crate) fn caret_visible(&self) -> bool {
        self.caret_visible
    }

    pub(crate) fn selected_range(&self) -> Option<Range<usize>> {
        self.selected_range.clone()
    }

    pub(crate) fn marked_text(&self) -> Option<&str> {
        self.marked_text.as_deref()
    }

    pub(crate) fn update_anchor(&self, anchor: TextInputAnchor) {
        (self.update_anchor)(anchor);
    }

    pub fn into_filesystem_search(self, focused: bool) -> FilesystemSearchFrame {
        FilesystemSearchFrame {
            anchor_id: self.anchor_id,
            viewport: self.viewport,
            focused,
            caret_visible: self.caret_visible,
            selected_range: self.selected_range,
            active_offset: self.active_offset,
            marked_text: self.marked_text,
            update_anchor: self.update_anchor,
        }
    }
}

#[derive(Clone, Copy)]
pub enum HostToolsTextRole {
    PlainDocument,
    NonSelectable,
}

type RenderLabel = dyn Fn(HostToolsTextRole, &str, &str, String, u32, &mut App) -> AnyElement;

/// A render-time callback, not an owner of the window's selection state.
#[derive(Clone)]
pub struct HostToolsText(Rc<RenderLabel>);

impl HostToolsText {
    pub fn new(
        render_label: impl Fn(HostToolsTextRole, &str, &str, String, u32, &mut App) -> AnyElement
        + 'static,
    ) -> Self {
        Self(Rc::new(render_label))
    }

    pub(crate) fn render_label(
        &self,
        role: HostToolsTextRole,
        scope: &str,
        key: impl AsRef<str>,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        (self.0)(role, scope, key.as_ref(), text.into(), color, cx)
    }
}

pub(crate) fn render_lucide_icon(path: &'static str, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(path)
        .size(px(size))
        .text_color(color)
        .into_any_element()
}
