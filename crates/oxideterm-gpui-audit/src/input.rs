use std::{borrow::Cow, collections::VecDeque, ops::Range};

use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, GlobalElementId,
    InspectorElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Styled, Subscription, Task, Timer, UTF16Selection,
    Window, prelude::*, px,
};
use oxideterm_editor_core::utf16::{
    byte_index_for_utf16, next_utf16_boundary, next_word_boundary, previous_utf16_boundary,
    previous_word_boundary, replace_utf16, transpose_text_at_utf16_offset,
    utf16_offset_for_byte_index, utf16_slice, word_range_for_utf16_offset,
};
use oxideterm_gpui_ui::text_input::{TextInputView, TextInputViewport, text_input_with_viewport};
use zeroize::{Zeroize, Zeroizing};

use super::{AuditPage, AuditView, AuditWindowRequest, policy::AuditPolicyInput};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum AuditInput {
    Search,
    Policy(AuditPolicyInput),
}

impl AuditInput {
    fn index(self) -> usize {
        match self {
            Self::Search => 0,
            Self::Policy(AuditPolicyInput::Retention) => 1,
            Self::Policy(AuditPolicyInput::Capacity) => 2,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Selection {
    anchor: usize,
    head: usize,
}

impl Selection {
    fn caret(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

struct MarkedText {
    text: Zeroizing<String>,
    replacement: Range<usize>,
    selection: Range<usize>,
}

impl MarkedText {
    fn range(&self) -> Range<usize> {
        self.replacement.start..self.replacement.start + self.text.encode_utf16().count()
    }

    fn committed_offset(&self, offset: usize) -> usize {
        let marked = self.range();
        if offset <= marked.start {
            offset
        } else if offset >= marked.end {
            self.replacement.end + offset - marked.end
        } else {
            self.replacement.start
        }
    }
}

struct Snapshot {
    text: Zeroizing<String>,
    selection: Selection,
}

struct Edit {
    field: AuditInput,
    before: Snapshot,
    after: Snapshot,
}

#[derive(Default)]
struct History {
    undo: VecDeque<Edit>,
    redo: VecDeque<Edit>,
}

impl History {
    fn clear(&mut self, field: AuditInput) {
        self.undo.retain(|edit| edit.field != field);
        self.redo.retain(|edit| edit.field != field);
    }

    fn record(&mut self, field: AuditInput, before: Snapshot, after: Snapshot) {
        if before.text == after.text {
            return;
        }
        if self
            .undo
            .iter()
            .rfind(|edit| edit.field == field)
            .is_some_and(|edit| edit.after.text != before.text)
        {
            self.clear(field);
        }
        self.redo.retain(|edit| edit.field != field);
        self.undo.push_back(Edit {
            field,
            before,
            after,
        });
        // The audit query is plain text, but it may contain credentials.
        while self.undo.len() + self.redo.len() > 128
            || self
                .undo
                .iter()
                .chain(&self.redo)
                .map(|edit| edit.before.text.len() + edit.after.text.len())
                .sum::<usize>()
                > 4 * 1024 * 1024
        {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }

    fn restore(&mut self, field: AuditInput, current: &str, redo: bool) -> Option<Snapshot> {
        let source = if redo { &mut self.redo } else { &mut self.undo };
        let index = source.iter().rposition(|edit| edit.field == field)?;
        let edit = source.remove(index)?;
        let (expected, restored) = if redo {
            (&edit.before, &edit.after)
        } else {
            (&edit.after, &edit.before)
        };
        if expected.text.as_str() != current {
            self.clear(field);
            return None;
        }
        let snapshot = Snapshot {
            text: restored.text.clone(),
            selection: restored.selection,
        };
        if redo {
            self.undo.push_back(edit);
        } else {
            self.redo.push_back(edit);
        }
        Some(snapshot)
    }
}

pub(super) struct AuditInputs {
    pub(super) focus: FocusHandle,
    active: Option<AuditInput>,
    viewports: [TextInputViewport; 3],
    selection: Selection,
    marked: Option<MarkedText>,
    drag_anchor: Option<usize>,
    blur: Option<Subscription>,
    blink: Option<Task<()>>,
    caret_visible: bool,
    history: History,
}

impl AuditInputs {
    pub(super) fn new(focus: FocusHandle) -> Self {
        Self {
            focus,
            active: None,
            viewports: Default::default(),
            selection: Selection::default(),
            marked: None,
            drag_anchor: None,
            blur: None,
            blink: None,
            caret_visible: false,
            history: History::default(),
        }
    }
}

impl AuditView {
    pub fn audit_input_focused(&self, window: &Window) -> bool {
        self.active_input(window).is_some()
    }

    pub(super) fn blur_input(&mut self, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.active.take() {
            self.inputs.marked = None;
            self.inputs.drag_anchor = None;
            self.inputs.selection = Selection::default();
            self.inputs.viewports[input.index()] = TextInputViewport::default();
            self.inputs.blur = None;
            self.inputs.blink = None;
            cx.notify();
        }
    }

    fn active_input(&self, window: &Window) -> Option<AuditInput> {
        if !self.inputs.focus.is_focused(window) || self.audit.open_filter.is_some() {
            return None;
        }
        self.inputs.active.filter(|input| match input {
            AuditInput::Search => self.audit.view == AuditPage::Events,
            AuditInput::Policy(_) => {
                self.audit.settings_open && self.audit.view != AuditPage::Sessions
            }
        })
    }

    fn input_text(&self, input: AuditInput) -> Cow<'_, str> {
        match input {
            AuditInput::Search => Cow::Borrowed(&self.audit.search),
            AuditInput::Policy(input) => self.audit_policy_input_text(input),
        }
    }

    fn set_input_text(&mut self, input: AuditInput, value: &str) {
        match input {
            AuditInput::Search => {
                self.audit.search.zeroize();
                self.audit.search.push_str(value);
            }
            AuditInput::Policy(input) => {
                self.ensure_audit_policy_draft();
                if let Some(draft) = self.audit.policy_draft.as_mut() {
                    draft.set_value(input, value);
                }
            }
        }
    }

    fn update_caret(&mut self, cx: &mut Context<Self>) {
        self.inputs.caret_visible = true;
        self.inputs.blink = Some(cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(std::time::Duration::from_millis(530)).await;
                if weak
                    .update(cx, |view, cx| {
                        view.inputs.caret_visible = !view.inputs.caret_visible;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn input_snapshot(&self, input: AuditInput) -> Snapshot {
        Snapshot {
            text: Zeroizing::new(self.input_text(input).into_owned()),
            selection: self.inputs.selection,
        }
    }

    fn replace_input(
        &mut self,
        input: AuditInput,
        range: Range<usize>,
        replacement: &str,
        cx: &mut Context<Self>,
    ) {
        let before = self.input_snapshot(input);
        let len = before.text.encode_utf16().count();
        let start = range.start.min(len);
        let end = range.end.min(len).max(start);
        let start =
            utf16_offset_for_byte_index(&before.text, byte_index_for_utf16(&before.text, start));
        let mut single_line = Zeroizing::new(String::new());
        let replacement = if replacement.contains(['\r', '\n']) {
            // Keep pasted line boundaries as spaces without retaining a plaintext copy.
            let mut chars = replacement.chars().peekable();
            while let Some(ch) = chars.next() {
                if matches!(ch, '\r' | '\n') {
                    if ch == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    if chars.peek().is_some() {
                        single_line.push(' ');
                    }
                } else {
                    single_line.push(ch);
                }
            }
            single_line.as_str()
        } else {
            replacement
        };
        let mut text = before.text.clone();
        replace_utf16(&mut text, Some(start..end.max(start)), replacement);
        let selection = Selection::caret(start + replacement.encode_utf16().count());
        self.set_input_text(input, &text);
        self.inputs.selection = selection;
        self.inputs.marked = None;
        self.inputs.drag_anchor = None;
        self.inputs
            .history
            .record(input, before, Snapshot { text, selection });
        self.update_caret(cx);
    }

    fn visible_text(&self, input: AuditInput) -> Zeroizing<String> {
        let mut text = Zeroizing::new(self.input_text(input).into_owned());
        if let Some(marked) = &self.inputs.marked {
            replace_utf16(&mut text, Some(marked.replacement.clone()), &marked.text);
        }
        text
    }

    fn visible_index(&self, input: AuditInput, point: Point<Pixels>) -> Option<usize> {
        let byte = self.inputs.viewports[input.index()].byte_index_for_position(point)?;
        if self.inputs.marked.is_some() {
            Some(utf16_offset_for_byte_index(&self.visible_text(input), byte))
        } else {
            Some(utf16_offset_for_byte_index(&self.input_text(input), byte))
        }
    }

    fn focus_input(
        &mut self,
        input: AuditInput,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_active = self.active_input(window) == Some(input);
        if !was_active {
            self.dispatch(AuditWindowRequest::ClearWorkspaceTextFocus, cx);
            self.inputs.marked = None;
        }
        if let AuditInput::Policy(_) = input {
            self.ensure_audit_policy_draft();
        }
        self.audit.open_filter = None;
        let end = self.input_text(input).encode_utf16().count();
        let index = self
            .visible_index(input, event.position)
            .map_or(end, |index| {
                self.inputs
                    .marked
                    .as_ref()
                    .map_or(index, |marked| marked.committed_offset(index))
            });
        self.inputs.marked = None;
        self.inputs.active = Some(input);
        if event.click_count >= 3 {
            self.inputs.selection = Selection {
                anchor: 0,
                head: end,
            };
            self.inputs.drag_anchor = None;
        } else if event.click_count == 2 {
            let word = word_range_for_utf16_offset(&self.input_text(input), index);
            self.inputs.selection = Selection {
                anchor: word.start,
                head: word.end,
            };
            self.inputs.drag_anchor = None;
        } else {
            let anchor = if was_active && event.modifiers.shift {
                self.inputs.selection.anchor
            } else {
                index
            };
            self.inputs.selection = Selection {
                anchor,
                head: index,
            };
            self.inputs.drag_anchor = Some(anchor);
        }
        let focus = self.inputs.focus.clone();
        self.inputs.blur = Some(cx.on_blur(&focus, window, |view, _, cx| view.blur_input(cx)));
        window.focus(&focus, cx);
        window.request_virtual_keyboard();
        window.notify_text_selection_changed(&focus);
        self.update_caret(cx);
    }

    fn drag_input(
        &mut self,
        input: AuditInput,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_input(window) != Some(input) {
            return;
        }
        if !event.dragging() {
            self.inputs.drag_anchor = None;
            return;
        }
        let (Some(anchor), Some(head)) = (
            self.inputs.drag_anchor,
            self.visible_index(input, event.position),
        ) else {
            return;
        };
        if self.inputs.selection.head == head {
            return;
        }
        self.inputs.selection = Selection { anchor, head };
        window.notify_text_selection_changed(&self.inputs.focus);
        self.update_caret(cx);
    }

    pub(super) fn render_audit_search(&self, cx: &Context<Self>) -> AnyElement {
        self.render_input(
            AuditInput::Search,
            &self.audit.search,
            self.i18n.t("event_log.audit.search"),
            cx,
        )
    }

    pub(super) fn render_input(
        &self,
        input: AuditInput,
        value: &str,
        placeholder: String,
        cx: &Context<Self>,
    ) -> AnyElement {
        let focused = self.inputs.active == Some(input);
        let marked = self.inputs.marked.as_ref().filter(|_| focused);
        let selection = focused.then(|| {
            marked.map_or_else(
                || self.inputs.selection.range(),
                |marked| marked.replacement.clone(),
            )
        });
        let field = text_input_with_viewport(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused,
                caret_visible: self.inputs.caret_visible,
                secret: false,
                selected_all: false,
                selected_range: selection,
                marked_text: marked.map(|marked| marked.text.as_str()),
            },
            &self.inputs.viewports[input.index()],
            focused.then_some(self.inputs.selection.head),
        )
        .h(px(28.0))
        .when(input == AuditInput::Search, |field| {
            field.flex_1().min_w_0()
        })
        .when(input != AuditInput::Search, |field| field.w(px(88.0)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event, window, cx| {
                view.focus_input(input, event, window, cx);
                cx.stop_propagation();
            }),
        )
        .when(focused, |field| {
            field.on_mouse_down_out(cx.listener(move |view, _, window, cx| {
                if view.active_input(window) == Some(input) {
                    view.blur_input(cx);
                }
            }))
        });

        if focused {
            NativeAuditInput {
                child: field.into_any_element(),
                owner: cx.entity(),
                focus: self.inputs.focus.clone(),
                input,
            }
            .into_any_element()
        } else {
            field.into_any_element()
        }
    }

    pub(super) fn handle_audit_input_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.active_input(window) else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let edit_modifier = modifiers.platform || (modifiers.control && !cfg!(target_os = "macos"));
        if self.inputs.marked.is_some()
            && !(edit_modifier && matches!(key, "a" | "c" | "x" | "v" | "z" | "y"))
        {
            // Native composition owns Enter, Escape, navigation, and deletion until it commits.
            return false;
        }
        let selected = self.inputs.selection;
        let range = self
            .inputs
            .marked
            .as_ref()
            .map_or_else(|| selected.range(), |marked| marked.replacement.clone());
        let value = self.input_text(input);
        let value = value.as_ref();
        let boundary = |offset, forward| {
            if modifiers.platform {
                if forward {
                    value.encode_utf16().count()
                } else {
                    0
                }
            } else if modifiers.alt || modifiers.control {
                if forward {
                    next_word_boundary(value, offset)
                } else {
                    previous_word_boundary(value, offset)
                }
            } else if forward {
                next_utf16_boundary(value, offset)
            } else {
                previous_utf16_boundary(value, offset)
            }
        };
        match key {
            "enter" => {
                match input {
                    AuditInput::Search => self.apply_audit_search(cx),
                    AuditInput::Policy(_) => self.apply_audit_policy_inputs(cx),
                }
                return true;
            }
            "escape" => {
                self.blur_input(cx);
                return true;
            }
            "a" if edit_modifier => {
                self.inputs.selection = Selection {
                    anchor: 0,
                    head: value.encode_utf16().count(),
                };
                self.inputs.marked = None;
            }
            "c" | "x" if edit_modifier => {
                if !range.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(utf16_slice(
                        value,
                        range.clone(),
                    )));
                    if key == "x" {
                        self.replace_input(input, range, "", cx);
                        window.notify_text_input_changed(&self.inputs.focus);
                    }
                }
                return true;
            }
            "v" if edit_modifier => {
                if let Some(text) = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .map(Zeroizing::new)
                {
                    self.replace_input(input, range, &text, cx);
                    window.notify_text_input_changed(&self.inputs.focus);
                }
                return true;
            }
            "z" | "y"
                if edit_modifier
                    && !modifiers.alt
                    && (key == "z" || (!cfg!(target_os = "macos") && !modifiers.shift)) =>
            {
                if self.inputs.marked.take().is_none() {
                    let current = Zeroizing::new(self.input_text(input).into_owned());
                    if let Some(snapshot) =
                        self.inputs
                            .history
                            .restore(input, &current, modifiers.shift || key == "y")
                    {
                        self.set_input_text(input, &snapshot.text);
                        self.inputs.selection = snapshot.selection;
                        window.notify_text_input_changed(&self.inputs.focus);
                    }
                }
            }
            key if matches!(key, "backspace" | "delete")
                || (modifiers.control && matches!(key, "h" | "d" | "k" | "u")) =>
            {
                let range = if !range.is_empty() {
                    range
                } else {
                    let caret = range.start;
                    match key {
                        "backspace" => boundary(caret, false)..caret,
                        "delete" => caret..boundary(caret, true),
                        "h" => previous_utf16_boundary(value, caret)..caret,
                        "d" => caret..next_utf16_boundary(value, caret),
                        "k" => caret..value.encode_utf16().count(),
                        "u" => 0..caret,
                        _ => unreachable!(),
                    }
                };
                if !range.is_empty() {
                    self.replace_input(input, range, "", cx);
                    window.notify_text_input_changed(&self.inputs.focus);
                }
                return true;
            }
            "t" if modifiers.control && !modifiers.platform && !modifiers.alt => {
                if range.is_empty()
                    && let Some((text, caret)) =
                        transpose_text_at_utf16_offset(value, selected.head)
                {
                    let text = Zeroizing::new(text);
                    let len = value.encode_utf16().count();
                    self.replace_input(input, 0..len, &text, cx);
                    self.inputs.selection = Selection::caret(caret);
                    window.notify_text_input_changed(&self.inputs.focus);
                    window.notify_text_selection_changed(&self.inputs.focus);
                }
                return true;
            }
            key if matches!(
                key,
                "home"
                    | "end"
                    | "left"
                    | "right"
                    | "arrowleft"
                    | "arrowright"
                    | "up"
                    | "down"
                    | "arrowup"
                    | "arrowdown"
                    | "pageup"
                    | "pagedown"
            ) || (modifiers.control && matches!(key, "a" | "e" | "b" | "f")) =>
            {
                let forward = matches!(key, "right" | "arrowright" | "end");
                let head = match key {
                    "home" | "up" | "arrowup" | "pageup" | "a" => 0,
                    "end" | "down" | "arrowdown" | "pagedown" | "e" => value.encode_utf16().count(),
                    "b" => previous_utf16_boundary(value, selected.head),
                    "f" => next_utf16_boundary(value, selected.head),
                    _ if !modifiers.shift
                        && !range.is_empty()
                        && !modifiers.alt
                        && !modifiers.control
                        && !modifiers.platform =>
                    {
                        if forward {
                            range.end
                        } else {
                            range.start
                        }
                    }
                    _ => boundary(selected.head, forward),
                };
                self.inputs.selection = Selection {
                    anchor: if modifiers.shift {
                        selected.anchor
                    } else {
                        head
                    },
                    head,
                };
            }
            _ => return false,
        }
        self.inputs.drag_anchor = None;
        window.notify_text_selection_changed(&self.inputs.focus);
        self.update_caret(cx);
        true
    }
}

struct NativeAuditInput {
    child: AnyElement,
    owner: Entity<AuditView>,
    focus: FocusHandle,
    input: AuditInput,
}

impl IntoElement for NativeAuditInput {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for NativeAuditInput {
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
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
        window.handle_input(
            &self.focus,
            ElementInputHandler::new(bounds, self.owner.clone()),
            cx,
        );
        let input = self.input;
        let owner = self.owner.downgrade();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && let Some(owner) = owner.upgrade()
                && owner.read(cx).inputs.drag_anchor.is_some()
            {
                // Selection keeps capture when the pointer leaves the control.
                owner.update(cx, |view, cx| view.drag_input(input, event, window, cx));
            }
        });
        let owner = self.owner.downgrade();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Capture
                && event.button == MouseButton::Left
                && let Some(owner) = owner.upgrade()
                && owner.read(cx).inputs.drag_anchor.is_some()
            {
                owner.update(cx, |view, _| view.inputs.drag_anchor = None);
            }
        });
    }
}

impl EntityInputHandler for AuditView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let input = self.active_input(window)?;
        let text = self.visible_text(input);
        let len = text.encode_utf16().count();
        let start = range.start.min(len);
        let range = start..range.end.min(len).max(start);
        *adjusted = Some(range.clone());
        Some(utf16_slice(&text, range))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.active_input(window)?;
        if let Some(marked) = &self.inputs.marked {
            let start = marked.replacement.start;
            return Some(UTF16Selection {
                range: start + marked.selection.start..start + marked.selection.end,
                reversed: false,
            });
        }
        let selection = self.inputs.selection;
        Some(UTF16Selection {
            range: selection.range(),
            reversed: selection.head < selection.anchor,
        })
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.active_input(window)?;
        self.inputs.marked.as_ref().map(MarkedText::range)
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_input(window).is_some() && self.inputs.marked.take().is_some() {
            self.update_caret(cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.active_input(window) else {
            return;
        };
        let range = match (&self.inputs.marked, range) {
            (Some(marked), Some(range)) => {
                marked.committed_offset(range.start)..marked.committed_offset(range.end)
            }
            (Some(marked), None) => marked.replacement.clone(),
            (None, Some(range)) => range,
            (None, None) => self.inputs.selection.range(),
        };
        self.replace_input(input, range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.active_input(window) else {
            return;
        };
        if text.is_empty() {
            self.unmark_text(window, cx);
            return;
        }
        let replacement = self
            .inputs
            .marked
            .as_ref()
            .map(|marked| marked.replacement.clone())
            .or(range)
            .unwrap_or_else(|| self.inputs.selection.range());
        let value_len = self.input_text(input).encode_utf16().count();
        let start = replacement.start.min(value_len);
        let len = text.encode_utf16().count();
        let selection = selected.unwrap_or(len..len);
        let selection_start = selection.start.min(len);
        self.inputs.marked = Some(MarkedText {
            text: Zeroizing::new(text.to_owned()),
            replacement: start..replacement.end.min(value_len).max(start),
            selection: selection_start..selection.end.min(len).max(selection_start),
        });
        self.update_caret(cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let input = self.active_input(window)?;
        let text = self.visible_text(input);
        let byte = byte_index_for_utf16(&text, range.start);
        self.inputs.viewports[input.index()]
            .caret_bounds_for_byte_index(
                byte,
                px(self.tokens.metrics.form_caret_width),
                px(self.tokens.metrics.form_caret_height),
            )
            .or(Some(bounds))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.visible_index(self.active_input(window)?, point)
    }

    fn text_length_utf16(&mut self, window: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let input = self.active_input(window)?;
        Some(self.visible_text(input).encode_utf16().count())
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.active_input(window) else {
            return;
        };
        if let Some(marked) = self.inputs.marked.as_mut() {
            let len = marked.text.encode_utf16().count();
            let start = range
                .start
                .saturating_sub(marked.replacement.start)
                .min(len);
            marked.selection = start
                ..range
                    .end
                    .saturating_sub(marked.replacement.start)
                    .min(len)
                    .max(start);
        } else {
            let text = self.input_text(input);
            let offset =
                |index| utf16_offset_for_byte_index(&text, byte_index_for_utf16(&text, index));
            self.inputs.selection = Selection {
                anchor: offset(range.start),
                head: offset(range.end),
            };
        }
        self.update_caret(cx);
    }

    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.audit_input_focused(window)
    }
}
