use std::hash::{Hash, Hasher};
use std::ops::Range;

use gpui::{
    AnyElement, Context, CursorStyle, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, SharedString, Styled, StyledText, TextLayout, TextRun, Window,
    div, font, px, rgb,
};
use oxideterm_gpui_ui::{
    tauri_ui_font_family,
    text_input::{TextInputAnchor, text_input_anchor_probe},
};

use super::ime::WorkspaceImeTarget;
use super::{SelectableTextFragmentState, WorkspaceApp};

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
        TextRun {
            len: value.len(),
            font: font(tauri_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            )),
            color: rgb(color).into(),
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
                .cursor(CursorStyle::IBeam)
                .child(styled_text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.selectable_text_fragments
                            .entry(fragment_id)
                            .and_modify(|fragment| fragment.text = value_for_mouse.clone());
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
        (start < end).then_some(start - fragment_range.start..end - fragment_range.start)
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

fn selection_bg(accent: u32) -> Hsla {
    let mut color: Hsla = rgb(accent).into();
    color.a = 0.25;
    color
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
    let dx = if point.x < bounds.left() {
        f32::from(bounds.left() - point.x)
    } else if point.x > bounds.right() {
        f32::from(point.x - bounds.right())
    } else {
        0.0
    };
    let dy = if point.y < bounds.top() {
        f32::from(bounds.top() - point.y)
    } else if point.y > bounds.bottom() {
        f32::from(point.y - bounds.bottom())
    } else {
        0.0
    };
    (dx * dx + dy * dy).sqrt()
}
