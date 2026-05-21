use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, CursorStyle, Entity, Hsla, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Point, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, StyledText, TextLayout, TextRun, Timer, Window, div, font,
    prelude::FluentBuilder, px, rgb,
};
use oxideterm_gpui_ui::{
    tauri_ui_font_family,
    text_input::{TextInputAnchor, text_input_anchor_probe},
};

use super::ime::WorkspaceImeTarget;
use super::{SelectableTextFragmentState, WorkspaceApp};

const SELECTABLE_TEXT_AUTOSCROLL_EDGE_PX: f32 = 48.0;
const SELECTABLE_TEXT_AUTOSCROLL_MAX_STEP_PX: f32 = 26.0;
const BROWSER_SCROLL_STICKY_BOTTOM_PX: f32 = 30.0;

pub(crate) trait SelectableTextScrollExt:
    StatefulInteractiveElement + gpui_component::scroll::ScrollableElement + Sized
{
    fn selectable_overflow_y_scroll(self, handle: &ScrollHandle) -> Self {
        self.overflow_y_scroll().track_scroll(handle)
    }

    fn selectable_overflow_y_scrollbar(self, handle: &ScrollHandle) -> Self {
        self.overflow_y_scroll()
            .track_scroll(handle)
            .vertical_scrollbar(handle)
    }
}

impl<T> SelectableTextScrollExt for T where
    T: StatefulInteractiveElement + gpui_component::scroll::ScrollableElement + Sized
{
}

#[derive(Clone)]
pub(super) struct SelectableTextRenderState {
    workspace: Entity<WorkspaceApp>,
    ui_font_family: SharedString,
    accent: u32,
    active_group_selection: Option<(u64, Range<usize>)>,
    fragments: HashMap<u64, SelectableTextFragmentState>,
}

pub(super) fn selectable_text_id(scope: &str, key: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scope.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn selectable_document_group_id() -> u64 {
    selectable_text_id("workspace-selectable-document", 0usize)
}

impl WorkspaceApp {
    pub(super) fn begin_selectable_text_frame(&mut self) {
        self.selectable_text_generation = self.selectable_text_generation.saturating_add(1);
        let oldest_live_generation = self.selectable_text_generation.saturating_sub(1);
        self.selectable_text_fragments
            .retain(|_, fragment| fragment.generation >= oldest_live_generation);
    }

    pub(super) fn stop_selectable_text_autoscroll(&mut self) {
        self.selectable_text_autoscroll_position = None;
    }

    pub(super) fn selectable_text_scroll_handle(&self, key: impl Into<String>) -> ScrollHandle {
        self.selectable_text_scroll_handles
            .borrow_mut()
            .entry(key.into())
            .or_insert_with(ScrollHandle::new)
            .clone()
    }

    pub(super) fn schedule_browser_scroll_to_bottom_if_sticky(
        &self,
        handle: ScrollHandle,
        cx: &mut Context<Self>,
    ) {
        if !browser_scroll_handle_is_near_bottom(&handle) {
            return;
        }
        // Tauri EventLogPanel keeps auto-scroll enabled while the browser
        // scroll container is within 30px of the bottom, then applies the
        // bottom scroll after React commits the new row. GPUI needs the same
        // post-layout turn because max_offset is only fresh after paint.
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, move |_this, cx| {
                if scroll_handle_to_bottom(&handle) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn update_selectable_text_autoscroll(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.read_only_selection_drag_active() {
            self.stop_selectable_text_autoscroll();
            return;
        }
        self.selectable_text_autoscroll_position = Some(position);
        self.schedule_selectable_text_autoscroll(cx);

        if self.apply_selectable_text_autoscroll(position) {
            cx.notify();
        }
    }

    fn schedule_selectable_text_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.selectable_text_autoscroll_scheduled {
            return;
        }
        self.selectable_text_autoscroll_scheduled = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, |this, cx| {
                this.selectable_text_autoscroll_scheduled = false;
                let Some(position) = this.selectable_text_autoscroll_position else {
                    return;
                };
                if !this.read_only_selection_drag_active() {
                    this.stop_selectable_text_autoscroll();
                    return;
                }
                if this.apply_selectable_text_autoscroll(position) {
                    this.update_read_only_selection_drag_at_position(position, cx);
                }
                this.schedule_selectable_text_autoscroll(cx);
            });
        })
        .detach();
    }

    fn apply_selectable_text_autoscroll(&mut self, position: Point<Pixels>) -> bool {
        let mut scrolled = false;
        if let Some(delta) = self.selectable_text_ai_chat_autoscroll_delta(position) {
            self.ai_chat_list_state.scroll_by(px(delta));
            scrolled = true;
        }
        let handles = self
            .selectable_text_scroll_handles
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for handle in handles {
            scrolled |= self.selectable_text_scroll_handle_autoscroll(&handle, position);
        }
        scrolled
    }

    fn selectable_text_ai_chat_autoscroll_delta(&self, position: Point<Pixels>) -> Option<f32> {
        if !self.ai_sidebar_visible() {
            return None;
        }
        let bounds = self.ai_chat_list_state.viewport_bounds();
        if bounds.size.height <= px(1.0) || bounds.size.width <= px(1.0) {
            return None;
        }
        if position.x < bounds.left() || position.x > bounds.right() {
            return None;
        }

        selectable_text_edge_scroll_step(bounds.top(), bounds.bottom(), position.y)
    }

    fn selectable_text_scroll_handle_autoscroll(
        &self,
        handle: &ScrollHandle,
        position: Point<Pixels>,
    ) -> bool {
        let bounds = handle.bounds();
        let max_offset = handle.max_offset();
        if max_offset.height <= px(0.0)
            || bounds.size.height <= px(1.0)
            || bounds.size.width <= px(1.0)
            || position.x < bounds.left()
            || position.x > bounds.right()
        {
            return false;
        }

        let Some(step) =
            selectable_text_edge_scroll_step(bounds.top(), bounds.bottom(), position.y)
        else {
            return false;
        };
        let offset = handle.offset();
        let next_y = (offset.y - px(step)).clamp(-max_offset.height, px(0.0));
        if next_y == offset.y {
            return false;
        }
        handle.set_offset(Point::new(offset.x, next_y));
        true
    }

    pub(super) fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<SharedString>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        let value = text.to_string();
        self.render_selectable_text(selectable_text_id(scope, (key, value)), text, color, cx)
    }

    pub(super) fn render_selectable_display_text(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        self.render_selectable_text_scoped(scope, (key, text.as_str()), text.clone(), color, cx)
    }

    pub(super) fn render_display_text_with_role(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        let value = text.clone();
        let run = self.selectable_plain_text_run(&value, color);
        match role {
            SelectableTextRole::PlainDocument => {
                self.render_selectable_text_scoped(scope, (key, value.as_str()), text, color, cx)
            }
            SelectableTextRole::RowSafe => self.render_selectable_styled_text_in_group_with_role(
                selectable_document_group_id(),
                selectable_text_id(scope, (key, value.as_str())),
                0,
                text.into(),
                vec![run],
                role,
                cx,
            ),
            SelectableTextRole::NonSelectable => render_non_selectable_styled_text(text, vec![run]),
        }
    }

    pub(super) fn render_display_text_with_role_and_alpha(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        alpha: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        let value = text.clone();
        let run = self.selectable_plain_text_run_with_font_and_alpha(&value, color, None, alpha);
        match role {
            SelectableTextRole::PlainDocument | SelectableTextRole::RowSafe => {
                // Alpha-bearing labels use the styled path so Tauri opacity semantics survive selection.
                self.render_selectable_styled_text_in_group_with_role(
                    selectable_document_group_id(),
                    selectable_text_id(scope, (key, value.as_str())),
                    0,
                    text.into(),
                    vec![run],
                    role,
                    cx,
                )
            }
            SelectableTextRole::NonSelectable => render_non_selectable_styled_text(text, vec![run]),
        }
    }

    pub(super) fn selectable_text_render_state(
        &self,
        cx: &mut Context<Self>,
    ) -> SelectableTextRenderState {
        let active_group_selection =
            if let Some(WorkspaceImeTarget::ReadOnlyText(group_id)) = self.active_ime_target() {
                self.ime_selected_range_for_target(WorkspaceImeTarget::ReadOnlyText(group_id))
                    .map(|range| (group_id, range))
            } else {
                None
            };
        SelectableTextRenderState {
            workspace: cx.entity(),
            ui_font_family: tauri_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ),
            accent: self.tokens.ui.accent,
            active_group_selection,
            fragments: self.selectable_text_fragments.clone(),
        }
    }

    pub(super) fn render_row_safe_selectable_display_text_in_group(
        &self,
        group_id: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: impl Into<String>,
        color: u32,
        font_family: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_row_safe_selectable_display_text_in_group_with_alpha(
            group_id,
            scope,
            key,
            order,
            text,
            color,
            1.0,
            font_family,
            cx,
        )
    }

    pub(super) fn render_row_safe_selectable_display_text_in_group_with_alpha(
        &self,
        group_id: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: impl Into<String>,
        color: u32,
        alpha: f32,
        font_family: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        let fragment_id = selectable_text_id(scope, (key, text.as_str()));
        let run =
            self.selectable_plain_text_run_with_font_and_alpha(&text, color, font_family, alpha);
        self.render_selectable_styled_text_in_group_with_role(
            group_id,
            fragment_id,
            order,
            text.into(),
            vec![run],
            SelectableTextRole::RowSafe,
            cx,
        )
    }

    pub(super) fn begin_selectable_text_group_from_mouse_down(
        &mut self,
        group_id: u64,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.blur_text_inputs(cx);
        window.focus(&self.focus_handle);
        self.begin_ime_selection_from_mouse_down(
            WorkspaceImeTarget::ReadOnlyText(group_id),
            event,
            window,
            cx,
        );
    }

    pub(super) fn render_selectable_text(
        &self,
        id: u64,
        text: impl Into<SharedString>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_selectable_text_with_style(id, text, color, None, cx)
    }

    pub(super) fn render_selectable_text_in_group(
        &self,
        group_id: u64,
        fragment_id: u64,
        order: usize,
        text: impl Into<SharedString>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        let value = text.to_string();
        let run = self.selectable_plain_text_run(&value, color);
        self.render_selectable_styled_text_in_group(
            group_id,
            fragment_id,
            order,
            text,
            vec![run],
            cx,
        )
    }

    pub(super) fn render_selectable_text_with_style(
        &self,
        id: u64,
        text: impl Into<SharedString>,
        color: u32,
        selected_range_override: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        let value = text.to_string();
        if selected_range_override.is_none() {
            let run = self.selectable_plain_text_run(&value, color);
            return self.render_selectable_styled_text_in_group(
                selectable_document_group_id(),
                id,
                0,
                text,
                vec![run],
                cx,
            );
        }

        let target = WorkspaceImeTarget::ReadOnlyText(id);
        let selection_range = selected_range_override.or_else(|| {
            self.ime_selected_range_for_target(target)
                .filter(|range| range.start < range.end)
        });
        let workspace = cx.entity();
        let value_for_anchor = value.clone();
        let value_for_mouse = value.clone();
        let run = self.selectable_plain_text_run(&value, color);
        let runs = selection_range
            .clone()
            .map(|range| {
                selected_text_runs(
                    &value,
                    &[run.clone()],
                    range,
                    selection_bg(self.tokens.ui.accent),
                )
            })
            .unwrap_or_else(|| vec![run]);
        let styled_text = StyledText::new(text).with_runs(runs);
        let layout = styled_text.layout().clone();

        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .min_w(px(0.0))
                .text_color(rgb(color))
                .cursor(CursorStyle::IBeam)
                .child(styled_text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.selectable_text_values
                            .insert(id, value_for_mouse.clone());
                        this.blur_text_inputs(cx);
                        window.focus(&this.focus_handle);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(
                    cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    }),
                ),
            move |anchor, _window: &mut Window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_selectable_text_anchor(id, value_for_anchor, layout, anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn selectable_plain_text_run(&self, value: &str, color: u32) -> TextRun {
        self.selectable_plain_text_run_with_font(value, color, None)
    }

    fn selectable_plain_text_run_with_font(
        &self,
        value: &str,
        color: u32,
        font_family: Option<SharedString>,
    ) -> TextRun {
        self.selectable_plain_text_run_with_font_and_alpha(value, color, font_family, 1.0)
    }

    fn selectable_plain_text_run_with_font_and_alpha(
        &self,
        value: &str,
        color: u32,
        font_family: Option<SharedString>,
        alpha: f32,
    ) -> TextRun {
        let mut color: Hsla = rgb(color).into();
        color.a = alpha.clamp(0.0, 1.0);
        TextRun {
            len: value.len(),
            font: font(font_family.unwrap_or_else(|| {
                tauri_ui_font_family(&self.settings_store.settings().appearance.ui_font_family)
            })),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    pub(super) fn render_selectable_styled_text_in_group(
        &self,
        group_id: u64,
        fragment_id: u64,
        order: usize,
        text: SharedString,
        runs: Vec<TextRun>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_selectable_styled_text_in_group_with_role(
            group_id,
            fragment_id,
            order,
            text,
            runs,
            SelectableTextRole::PlainDocument,
            cx,
        )
    }

    fn render_selectable_styled_text_in_group_with_role(
        &self,
        group_id: u64,
        fragment_id: u64,
        order: usize,
        text: SharedString,
        runs: Vec<TextRun>,
        role: SelectableTextRole,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if role == SelectableTextRole::NonSelectable {
            return render_non_selectable_styled_text(text.to_string(), runs);
        }
        let target = WorkspaceImeTarget::ReadOnlyText(group_id);
        let value = text.to_string();
        let selection_range = self
            .ime_selected_range_for_target(target)
            .and_then(|range| {
                self.local_range_for_selectable_fragment(group_id, fragment_id, range)
            })
            .filter(|range| range.start < range.end);
        let display_runs = selection_range
            .map(|range| {
                selected_text_runs(&value, &runs, range, selection_bg(self.tokens.ui.accent))
            })
            .unwrap_or(runs);
        let workspace = cx.entity();
        let value_for_anchor = value.clone();
        let value_for_mouse = value.clone();
        let styled_text = StyledText::new(text).with_runs(display_runs);
        let layout = styled_text.layout().clone();

        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .min_w(px(0.0))
                // RowSafe is reserved for table/tree/breadcrumb cells; Tauri renders these as single-line cells.
                .when(role == SelectableTextRole::RowSafe, |text| {
                    text.whitespace_nowrap()
                })
                .cursor(CursorStyle::IBeam)
                .child(styled_text)
                .when(role.is_interactive(), |element| {
                    element
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                if !selectable_text_should_begin_selection(role, event.click_count)
                                {
                                    return;
                                }
                                this.selectable_text_fragments
                                    .entry(fragment_id)
                                    .and_modify(|fragment| fragment.text = value_for_mouse.clone());
                                this.blur_text_inputs(cx);
                                window.focus(&this.focus_handle);
                                this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                                if selectable_text_should_stop_propagation(role) {
                                    cx.stop_propagation();
                                }
                            }),
                        )
                        .on_mouse_move(cx.listener(
                            |this, event: &gpui::MouseMoveEvent, window, cx| {
                                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                            },
                        ))
                }),
            move |anchor, _window: &mut Window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_selectable_text_group_fragment(
                        group_id,
                        fragment_id,
                        order,
                        value_for_anchor,
                        layout,
                        anchor,
                        cx,
                    );
                });
            },
        )
        .into_any_element()
    }

    fn update_selectable_text_anchor(
        &mut self,
        id: u64,
        value: String,
        layout: TextLayout,
        anchor: TextInputAnchor,
        cx: &mut Context<Self>,
    ) {
        let changed = self
            .selectable_text_values
            .get(&id)
            .is_none_or(|stored| stored != &value);
        if changed {
            self.selectable_text_values.insert(id, value);
        }
        self.selectable_text_layouts.insert(id, layout);
        self.update_text_input_anchor(anchor, cx);
    }

    fn update_selectable_text_group_fragment(
        &mut self,
        group_id: u64,
        fragment_id: u64,
        order: usize,
        text: String,
        layout: TextLayout,
        anchor: TextInputAnchor,
        cx: &mut Context<Self>,
    ) {
        if group_id != selectable_document_group_id() && order == 0 {
            self.selectable_text_fragments
                .retain(|_, fragment| fragment.group_id != group_id);
        }
        self.selectable_text_fragments.insert(
            fragment_id,
            SelectableTextFragmentState {
                group_id,
                order,
                generation: self.selectable_text_generation,
                text,
                layout,
                anchor,
            },
        );
        if self
            .active_ime_target()
            .is_some_and(|target| target == WorkspaceImeTarget::ReadOnlyText(group_id))
        {
            cx.notify();
        }
    }

    pub(super) fn selectable_text_group_text(&self, group_id: u64) -> Option<String> {
        let fragments = self.ordered_selectable_text_fragments(group_id);
        if fragments.is_empty() {
            return None;
        }
        let mut text = String::new();
        for (index, fragment) in fragments.into_iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            text.push_str(&fragment.text);
        }
        Some(text)
    }

    pub(super) fn selectable_text_group_index_for_position(
        &self,
        group_id: u64,
        position: Point<Pixels>,
    ) -> Option<usize> {
        let fragments = self.ordered_selectable_text_fragments(group_id);
        let fragment = fragments.iter().copied().min_by(|a, b| {
            let a_distance = distance_from_bounds(position, a.anchor.bounds);
            let b_distance = distance_from_bounds(position, b.anchor.bounds);
            a_distance
                .partial_cmp(&b_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.order.cmp(&b.order))
        })?;
        let local_byte_index = match fragment.layout.index_for_position(position) {
            Ok(index) | Err(index) => index.min(fragment.text.len()),
        };
        let local_utf16 = utf16_offset_for_byte_index(&fragment.text, local_byte_index);
        let global_range = self.selectable_text_fragment_global_range(group_id, fragment)?;
        Some(global_range.start + local_utf16)
    }

    fn local_range_for_selectable_fragment(
        &self,
        group_id: u64,
        fragment_id: u64,
        group_range: Range<usize>,
    ) -> Option<Range<usize>> {
        let fragment = self.selectable_text_fragments.get(&fragment_id)?;
        let fragment_range = self.selectable_text_fragment_global_range(group_id, fragment)?;
        let start = group_range.start.max(fragment_range.start);
        let end = group_range.end.min(fragment_range.end);
        (start < end).then(|| start - fragment_range.start..end - fragment_range.start)
    }

    fn selectable_text_fragment_global_range(
        &self,
        group_id: u64,
        target_fragment: &SelectableTextFragmentState,
    ) -> Option<Range<usize>> {
        let mut cursor = 0usize;
        for (index, fragment) in self
            .ordered_selectable_text_fragments(group_id)
            .into_iter()
            .enumerate()
        {
            if index > 0 {
                cursor = cursor.saturating_add(1);
            }
            let start = cursor;
            let end = start + fragment.text.encode_utf16().count();
            if std::ptr::eq(fragment, target_fragment) {
                return Some(start..end);
            }
            cursor = end;
        }
        None
    }

    fn ordered_selectable_text_fragments(
        &self,
        group_id: u64,
    ) -> Vec<&SelectableTextFragmentState> {
        let mut fragments = self
            .selectable_text_fragments
            .values()
            .filter(|fragment| fragment.group_id == group_id)
            .collect::<Vec<_>>();
        fragments.sort_by(|a, b| {
            a.order
                .cmp(&b.order)
                .then_with(|| {
                    f32::from(a.anchor.bounds.top())
                        .partial_cmp(&f32::from(b.anchor.bounds.top()))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    f32::from(a.anchor.bounds.left())
                        .partial_cmp(&f32::from(b.anchor.bounds.left()))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        fragments
    }
}

fn browser_scroll_handle_is_near_bottom(handle: &ScrollHandle) -> bool {
    let max_offset = handle.max_offset();
    if max_offset.height <= px(0.0) {
        return true;
    }
    let distance_from_bottom = max_offset.height + handle.offset().y;
    distance_from_bottom <= px(BROWSER_SCROLL_STICKY_BOTTOM_PX)
}

fn scroll_handle_to_bottom(handle: &ScrollHandle) -> bool {
    let offset = handle.offset();
    let max_offset = handle.max_offset();
    let next_y = -max_offset.height;
    if offset.y == next_y {
        return false;
    }
    handle.set_offset(Point::new(offset.x, next_y));
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectableTextRole {
    PlainDocument,
    RowSafe,
    NonSelectable,
}

impl SelectableTextRole {
    fn is_interactive(self) -> bool {
        self != Self::NonSelectable
    }
}

fn selectable_text_should_begin_selection(role: SelectableTextRole, click_count: usize) -> bool {
    match role {
        SelectableTextRole::PlainDocument => true,
        SelectableTextRole::RowSafe => click_count <= 1,
        SelectableTextRole::NonSelectable => false,
    }
}

fn selectable_text_should_stop_propagation(role: SelectableTextRole) -> bool {
    role == SelectableTextRole::PlainDocument
}

fn render_non_selectable_styled_text(
    text: impl Into<SharedString>,
    runs: Vec<TextRun>,
) -> AnyElement {
    div()
        .min_w(px(0.0))
        .child(StyledText::new(text.into()).with_runs(runs))
        .into_any_element()
}

impl SelectableTextRenderState {
    pub(super) fn render_display_text_with_role_in_group(
        &self,
        role: SelectableTextRole,
        group_id: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: impl Into<String>,
        color: u32,
        _cx: &mut App,
    ) -> AnyElement {
        let text = text.into();
        let fragment_id = selectable_text_id(scope, (key, text.as_str()));
        let run = TextRun {
            len: text.len(),
            font: font(self.ui_font_family.clone()),
            color: rgb(color).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        match role {
            SelectableTextRole::PlainDocument | SelectableTextRole::RowSafe => self
                .render_styled_text_in_group(
                    role,
                    group_id,
                    fragment_id,
                    order,
                    text.into(),
                    vec![run],
                ),
            SelectableTextRole::NonSelectable => render_non_selectable_styled_text(text, vec![run]),
        }
    }

    pub(super) fn render_row_safe_display_text_in_group(
        &self,
        group_id: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.render_display_text_with_role_in_group(
            SelectableTextRole::RowSafe,
            group_id,
            scope,
            key,
            order,
            text,
            color,
            cx,
        )
    }

    fn render_styled_text_in_group(
        &self,
        role: SelectableTextRole,
        group_id: u64,
        fragment_id: u64,
        order: usize,
        text: SharedString,
        runs: Vec<TextRun>,
    ) -> AnyElement {
        debug_assert_ne!(role, SelectableTextRole::NonSelectable);
        let target = WorkspaceImeTarget::ReadOnlyText(group_id);
        let value = text.to_string();
        let selection_range = self
            .active_group_selection
            .as_ref()
            .filter(|(active_group_id, _)| *active_group_id == group_id)
            .and_then(|(_, range)| {
                self.local_range_for_selectable_fragment(group_id, fragment_id, range.clone())
            })
            .filter(|range| range.start < range.end);
        let display_runs = selection_range
            .map(|range| selected_text_runs(&value, &runs, range, selection_bg(self.accent)))
            .unwrap_or(runs);
        let workspace = self.workspace.clone();
        let workspace_for_mouse = self.workspace.clone();
        let value_for_anchor = value.clone();
        let styled_text = StyledText::new(text).with_runs(display_runs);
        let layout = styled_text.layout().clone();

        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .min_w(px(0.0))
                // Virtualized RowSafe cells share the same one-line browser table-cell contract.
                .when(role == SelectableTextRole::RowSafe, |text| {
                    text.whitespace_nowrap()
                })
                .cursor(CursorStyle::IBeam)
                .child(styled_text)
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &gpui::MouseDownEvent, window, cx| {
                        if !selectable_text_should_begin_selection(role, event.click_count) {
                            return;
                        }
                        let _ = workspace_for_mouse.update(cx, |this, cx| {
                            this.selectable_text_fragments
                                .entry(fragment_id)
                                .and_modify(|fragment| fragment.text = value.clone());
                            this.begin_selectable_text_group_from_mouse_down(
                                group_id, event, window, cx,
                            );
                        });
                        if selectable_text_should_stop_propagation(role) {
                            // Virtual rows use the same role contract as normal
                            // selectable text: standalone document text owns the
                            // click, while row-safe cells bubble to their row.
                            cx.stop_propagation();
                        }
                    },
                ),
            move |anchor, _window: &mut Window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_selectable_text_group_fragment(
                        group_id,
                        fragment_id,
                        order,
                        value_for_anchor,
                        layout,
                        anchor,
                        cx,
                    );
                });
            },
        )
        .into_any_element()
    }

    fn local_range_for_selectable_fragment(
        &self,
        group_id: u64,
        fragment_id: u64,
        group_range: Range<usize>,
    ) -> Option<Range<usize>> {
        let fragment_range = self.selectable_text_fragment_global_range(group_id, fragment_id)?;
        let start = group_range.start.max(fragment_range.start);
        let end = group_range.end.min(fragment_range.end);
        (start < end).then(|| start - fragment_range.start..end - fragment_range.start)
    }

    fn selectable_text_fragment_global_range(
        &self,
        group_id: u64,
        target_fragment_id: u64,
    ) -> Option<Range<usize>> {
        let mut cursor = 0usize;
        for (index, (fragment_id, fragment)) in self
            .ordered_selectable_text_fragments(group_id)
            .into_iter()
            .enumerate()
        {
            if index > 0 {
                cursor = cursor.saturating_add(1);
            }
            let start = cursor;
            let end = start + fragment.text.encode_utf16().count();
            if fragment_id == target_fragment_id {
                return Some(start..end);
            }
            cursor = end;
        }
        None
    }

    fn ordered_selectable_text_fragments(
        &self,
        group_id: u64,
    ) -> Vec<(u64, &SelectableTextFragmentState)> {
        let mut fragments = self
            .fragments
            .iter()
            .filter(|(_, fragment)| fragment.group_id == group_id)
            .collect::<Vec<_>>();
        fragments.sort_by(|(_, a), (_, b)| {
            a.order
                .cmp(&b.order)
                .then_with(|| {
                    f32::from(a.anchor.bounds.top())
                        .partial_cmp(&f32::from(b.anchor.bounds.top()))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    f32::from(a.anchor.bounds.left())
                        .partial_cmp(&f32::from(b.anchor.bounds.left()))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        fragments
            .into_iter()
            .map(|(id, fragment)| (*id, fragment))
            .collect()
    }
}

fn selection_bg(accent: u32) -> Hsla {
    let mut color: Hsla = rgb(accent).into();
    color.a = 0.25;
    color
}

fn selectable_text_edge_scroll_step(top: Pixels, bottom: Pixels, y: Pixels) -> Option<f32> {
    let edge = SELECTABLE_TEXT_AUTOSCROLL_EDGE_PX;
    let top = f32::from(top);
    let bottom = f32::from(bottom);
    let y = f32::from(y);
    let step = if y < top + edge {
        -((top + edge - y) / edge).clamp(0.0, 1.0) * SELECTABLE_TEXT_AUTOSCROLL_MAX_STEP_PX
    } else if y > bottom - edge {
        ((y - (bottom - edge)) / edge).clamp(0.0, 1.0) * SELECTABLE_TEXT_AUTOSCROLL_MAX_STEP_PX
    } else {
        0.0
    };
    (step.abs() >= 1.0).then_some(step)
}

fn selected_text_runs(
    text: &str,
    runs: &[TextRun],
    selection_range: Range<usize>,
    selection_bg: Hsla,
) -> Vec<TextRun> {
    let selection_start = byte_index_for_utf16(text, selection_range.start);
    let selection_end = byte_index_for_utf16(text, selection_range.end);
    if selection_start >= selection_end {
        return runs.to_vec();
    }

    let mut split_runs = Vec::with_capacity(runs.len() + 2);
    let mut cursor = 0usize;
    for run in runs {
        let run_start = cursor;
        let run_end = cursor.saturating_add(run.len);
        cursor = run_end;
        if run.len == 0 {
            continue;
        }
        let cuts = [
            run_start,
            selection_start.clamp(run_start, run_end),
            selection_end.clamp(run_start, run_end),
            run_end,
        ];
        for pair in cuts.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start >= end {
                continue;
            }
            let mut part = run.clone();
            part.len = end - start;
            if start >= selection_start && end <= selection_end {
                part.background_color = Some(selection_bg);
            }
            split_runs.push(part);
        }
    }
    split_runs
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

fn utf16_offset_for_byte_index(value: &str, byte_index: usize) -> usize {
    value[..byte_index.min(value.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

fn distance_from_bounds(point: Point<Pixels>, bounds: gpui::Bounds<Pixels>) -> f32 {
    let x = f32::from(point.x);
    let y = f32::from(point.y);
    let left = f32::from(bounds.left());
    let right = f32::from(bounds.right());
    let top = f32::from(bounds.top());
    let bottom = f32::from(bounds.bottom());
    let dx = if x < left {
        left - x
    } else if x > right {
        x - right
    } else {
        0.0
    };
    let dy = if y < top {
        top - y
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    };
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_safe_selectable_single_click_can_select_and_still_bubble() {
        assert!(selectable_text_should_begin_selection(
            SelectableTextRole::RowSafe,
            1,
        ));
        assert!(!selectable_text_should_stop_propagation(
            SelectableTextRole::RowSafe,
        ));
    }

    #[test]
    fn row_safe_selectable_double_click_leaves_row_double_click_intact() {
        assert!(!selectable_text_should_begin_selection(
            SelectableTextRole::RowSafe,
            2,
        ));
        assert!(!selectable_text_should_stop_propagation(
            SelectableTextRole::RowSafe,
        ));
    }

    #[test]
    fn intercepting_selectable_keeps_existing_standalone_text_behavior() {
        assert!(selectable_text_should_begin_selection(
            SelectableTextRole::PlainDocument,
            2,
        ));
        assert!(selectable_text_should_stop_propagation(
            SelectableTextRole::PlainDocument,
        ));
    }

    #[test]
    fn non_selectable_role_matches_tauri_select_none() {
        assert!(!selectable_text_should_begin_selection(
            SelectableTextRole::NonSelectable,
            1,
        ));
        assert!(!selectable_text_should_stop_propagation(
            SelectableTextRole::NonSelectable,
        ));
    }
}
