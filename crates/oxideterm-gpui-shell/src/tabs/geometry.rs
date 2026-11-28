use super::strip::ShellTabRequest;
use gpui::{
    App, Bounds, Pixels, Point, ScrollHandle, SharedString, TextRun, Window, font, px, rgb,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::{TabId, TabKind};
use std::{cell::RefCell, collections::HashMap, time::Duration};
use web_time::Instant;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabStripItemKind {
    Live,
    Returning,
    Exiting(usize),
}

pub struct TabStripItem {
    pub tab_id: TabId,
    pub width: f32,
    pub kind: TabStripItemKind,
}

pub struct TabStripLayout {
    pub items: Vec<TabStripItem>,
    leading: f32,
}

impl TabStripLayout {
    pub fn return_index(&self) -> Option<usize> {
        let mut index = 0;
        for item in &self.items {
            match item.kind {
                TabStripItemKind::Returning => return Some(index),
                TabStripItemKind::Live => index += 1,
                TabStripItemKind::Exiting(_) => {}
            }
        }
        None
    }

    pub fn bounds(&self, tab_id: TabId) -> Option<std::ops::Range<f32>> {
        let mut left = self.leading;
        for item in &self.items {
            if item.tab_id == tab_id {
                return Some(left..left + item.width);
            }
            left += item.width;
        }
        None
    }

    pub fn content_width(&self) -> f32 {
        self.leading + self.items.iter().map(|item| item.width).sum::<f32>()
    }

    pub fn live_widths(&self) -> Vec<f32> {
        self.items
            .iter()
            .filter(|item| item.kind == TabStripItemKind::Live)
            .map(|item| item.width)
            .collect()
    }

    pub fn live_tab_at(&self, x: f32) -> Option<TabId> {
        let mut left = self.leading;
        for item in &self.items {
            let right = left + item.width;
            if (left..right).contains(&x) {
                return (item.kind == TabStripItemKind::Live).then_some(item.tab_id);
            }
            left = right;
        }
        None
    }

    pub fn insertion_index(&self, pointer_x: f32) -> usize {
        let mut left = self.leading;
        let mut index = 0;
        for item in &self.items {
            if item.kind == TabStripItemKind::Live {
                if pointer_x < left + item.width / 2.0 {
                    return index;
                }
                index += 1;
            }
            left += item.width;
        }
        index
    }

    pub fn insertion_at_pointer(&self, client_x: f32, viewport: &ScrollHandle) -> usize {
        self.insertion_index(client_x - f32::from(viewport.bounds().left() + viewport.offset().x))
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TabDragMode {
    Pending,
    Reorder,
    Detach,
    Content,
}

#[derive(Clone, Debug)]
pub struct TabDragState {
    pub tab_id: TabId,
    detachable: bool,
    pub from_index: usize,
    pub destination_tab: Option<TabId>,
    pub start_x: f32,
    pub start_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub tab_widths: Vec<f32>,
    pub last_edge_scroll_frame: Option<Instant>,
    pub active: bool,
    pub mode: TabDragMode,
    pub drop_target_index: usize,
}

#[derive(Clone, Debug)]
pub struct ExitingTabVisual {
    pub tab_id: TabId,
    pub kind: TabKind,
    pub title: String,
    pub width: f32,
    pub visual_index: usize,
    pub was_active: bool,
    pub started_at: Instant,
    pub duration: Duration,
}

impl ExitingTabVisual {
    pub fn exit_visibility(&self, now: Instant, motion_enabled: bool) -> f32 {
        if !motion_enabled || self.duration.is_zero() {
            return 0.0;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        1.0 - oxideterm_gpui_ui::motion::ease_in_out_cubic(
            elapsed.as_secs_f32() / self.duration.as_secs_f32(),
        )
    }
}
pub fn tab_exit_visual_index(live_visual_index: usize, occupied_indices: &[usize]) -> usize {
    let mut visual_index = live_visual_index;
    for occupied in occupied_indices {
        if *occupied <= visual_index {
            visual_index += 1;
        }
    }
    visual_index
}

pub const TAB_DRAG_THRESHOLD_PX: f32 = 10.0;

pub fn tab_drag_is_horizontal_reorder(delta_x: f32, delta_y: f32) -> bool {
    let horizontal = delta_x.abs();
    let vertical = delta_y.abs();
    horizontal > TAB_DRAG_THRESHOLD_PX && horizontal >= vertical
}

impl TabDragState {
    fn edge_scroll_step(&self, viewport: Bounds<Pixels>) -> Option<f32> {
        let pointer = gpui::point(px(self.current_x), px(self.current_y));
        if self.mode != TabDragMode::Reorder || !viewport.contains(&pointer) {
            return None;
        }
        oxideterm_gpui_ui::virtual_list::browser_drag_edge_scroll_step(
            viewport.left(),
            viewport.right(),
            pointer.x,
        )
    }
}

pub fn tab_drag_is_detach(delta_x: f32, delta_y: f32, tabbar_height: f32) -> bool {
    let threshold = (tabbar_height * 0.72).max(24.0);
    delta_y > threshold && delta_y.abs() >= delta_x.abs() * 0.85
}

// Pointer hit-testing returns a slot in the pre-removal strip. Moving right
// must discount the source tab after it is removed from that strip.
pub fn tab_reorder_target_visible_index(
    source_visible_index: usize,
    insertion_slot: usize,
) -> usize {
    insertion_slot.saturating_sub(usize::from(source_visible_index < insertion_slot))
}

pub fn tabbar_wheel_scroll_delta(delta_x: f32, delta_y: f32) -> f32 {
    // Minor cross-axis trackpad movement must not override the intended direction.
    if delta_x.abs() > delta_y.abs() {
        delta_x
    } else {
        delta_y
    }
}

// GPUI wheel deltas are applied to negative scroll offsets. The tab bar keeps a
// browser-like positive scrollLeft value, so advancing the strip subtracts delta.
pub fn tabbar_scroll_x_after_wheel(
    current_scroll_x: f32,
    wheel_delta: f32,
    max_scroll: f32,
) -> f32 {
    (current_scroll_x - wheel_delta).clamp(0.0, max_scroll)
}

// The main tab strip keeps a thin visual thumb while exposing a larger drag target.
pub(super) const TABBAR_SCROLLBAR_HEIGHT: f32 = 3.0;
pub(super) const TABBAR_SCROLLBAR_DRAG_HEIGHT: f32 = 10.0;
pub(super) const TABBAR_SCROLLBAR_HORIZONTAL_INSET: f32 = 8.0;
pub(super) const TABBAR_SCROLLBAR_MIN_THUMB_WIDTH: f32 = 32.0;
pub(super) const TABBAR_SCROLLBAR_MAX_THUMB_WIDTH: f32 = 160.0;
pub(super) const TABBAR_SCROLLBAR_RADIUS: f32 = 2.0;
pub(super) const TABBAR_SCROLLBAR_ALPHA: u32 = 0x40;
pub(super) const TABBAR_SCROLLBAR_HOVER_ALPHA: u32 = 0x99;

#[derive(Clone, Copy)]
pub(super) struct TabbarScrollbarGeometry {
    pub(super) viewport_left: f32,
    pub(super) track_width: f32,
    pub(super) thumb_width: f32,
    pub(super) thumb_left: f32,
    pub(super) max_scroll: f32,
}

// Keep the geometry calculation independent from GPUI events so edge cases remain testable.
pub(super) fn calculate_tabbar_scrollbar_geometry(
    viewport_left: f32,
    viewport_width: f32,
    max_scroll: f32,
    scroll_x: f32,
) -> Option<TabbarScrollbarGeometry> {
    let track_width = (viewport_width - TABBAR_SCROLLBAR_HORIZONTAL_INSET * 2.0).max(0.0);
    if viewport_width <= 1.0 || max_scroll <= 1.0 || track_width <= 1.0 {
        return None;
    }

    let content_width = viewport_width + max_scroll;
    let min_thumb_width = TABBAR_SCROLLBAR_MIN_THUMB_WIDTH.min(track_width);
    let max_thumb_width = TABBAR_SCROLLBAR_MAX_THUMB_WIDTH
        .max(min_thumb_width)
        .min(track_width);
    let thumb_width = (viewport_width / content_width * track_width)
        .max(min_thumb_width)
        .min(max_thumb_width);
    let thumb_travel = track_width - thumb_width;
    if thumb_travel <= 1.0 {
        return None;
    }

    let thumb_left = TABBAR_SCROLLBAR_HORIZONTAL_INSET
        + scroll_x.clamp(0.0, max_scroll) / max_scroll * thumb_travel;
    Some(TabbarScrollbarGeometry {
        viewport_left,
        track_width,
        thumb_width,
        thumb_left,
        max_scroll,
    })
}

// Convert a clamped thumb position back into the tab strip's logical scroll offset.
pub(super) fn tabbar_scroll_x_for_thumb_left(
    thumb_left: f32,
    geometry: TabbarScrollbarGeometry,
) -> f32 {
    let track_left = TABBAR_SCROLLBAR_HORIZONTAL_INSET;
    let thumb_travel = geometry.track_width - geometry.thumb_width;
    let clamped_left = thumb_left.clamp(track_left, track_left + thumb_travel);
    (clamped_left - track_left) / thumb_travel * geometry.max_scroll
}

pub struct TabStripState {
    pub drag: Option<TabDragState>,
    pub exiting_tabs: Vec<ExitingTabVisual>,
    pub scroll_handle: ScrollHandle,
    text_widths: RefCell<HashMap<(String, u32, String), f32>>,
}

impl Default for TabStripState {
    fn default() -> Self {
        Self {
            drag: None,
            exiting_tabs: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            text_widths: RefCell::default(),
        }
    }
}

impl TabStripState {
    pub fn layout(
        &self,
        live: impl Iterator<Item = (TabId, f32)>,
        returning: Option<(TabId, f32, f32)>,
        tokens: &ThemeTokens,
        now: Instant,
    ) -> TabStripLayout {
        let mut layout = TabStripLayout {
            items: live
                .map(|(tab_id, width)| TabStripItem {
                    tab_id,
                    width,
                    kind: TabStripItemKind::Live,
                })
                .collect(),
            leading: tokens.metrics.tabbar_leading_offset,
        };
        if let Some((tab_id, width, _)) = returning {
            layout.items.push(TabStripItem {
                tab_id,
                width,
                kind: TabStripItemKind::Returning,
            })
        }
        let mut exits = self
            .exiting_tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let width = if tokens.motion.spatial_enabled {
                    tab.width * tab.exit_visibility(now, tokens.motion.enabled)
                } else {
                    tab.width
                };
                (index, tab, width)
            })
            .collect::<Vec<_>>();
        exits.sort_by_key(|(_, tab, _)| tab.visual_index);
        for (index, exiting, width) in exits {
            layout.items.insert(
                exiting.visual_index.min(layout.items.len()),
                TabStripItem {
                    tab_id: exiting.tab_id,
                    width,
                    kind: TabStripItemKind::Exiting(index),
                },
            );
        }
        if let Some((_, _, x)) = returning
            && let Some(index) = layout
                .items
                .iter()
                .position(|item| item.kind == TabStripItemKind::Returning)
        {
            let item = layout.items.remove(index);
            let target = layout.insertion_index(x);
            let slot = layout
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.kind == TabStripItemKind::Live)
                .nth(target)
                .map_or(layout.items.len(), |(index, _)| index);
            layout.items.insert(slot, item);
        }
        layout
    }

    pub fn text_width(
        &self,
        text: &str,
        size: f32,
        font_family: SharedString,
        window: &Window,
    ) -> f32 {
        let key = (
            text.to_string(),
            size.to_bits(),
            font_family.as_ref().to_string(),
        );
        if let Some(width) = self.text_widths.borrow().get(&key).copied() {
            return width;
        }
        let text = SharedString::from(text.to_string());
        let run = TextRun {
            len: text.len(),
            font: font(font_family),
            color: rgb(0).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: None,
        };
        // Use the same shaper and font as the label, including fallback glyphs.
        let width = f32::from(
            window
                .text_system()
                .shape_line(text, px(size), &[run], None)
                .width,
        );
        let mut cache = self.text_widths.borrow_mut();
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, width);
        width
    }

    pub fn natural_width(
        &self,
        title: &str,
        controls: f32,
        font: SharedString,
        tokens: &ThemeTokens,
        window: &Window,
    ) -> f32 {
        let metrics = tokens.metrics;
        (self.text_width(title, metrics.tab_font_size, font, window)
            + metrics.tab_padding_x * 2.0
            + metrics.tab_icon_size
            + metrics.tab_gap * 2.0
            + controls
            + 1.0)
            .ceil()
    }

    pub fn set_offset(&self, offset: f32, max: f32) {
        self.scroll_handle
            .set_offset(Point::new(px(-offset.clamp(0., max)), px(0.)))
    }
    pub fn offset(&self, max: f32) -> f32 {
        f32::from(-self.scroll_handle.offset().x).clamp(0., max)
    }
    pub fn reveal(&self, layout: &TabStripLayout, active: Option<TabId>, viewport_width: f32) {
        let max = (layout.content_width() - viewport_width).max(0.);
        let current_scroll_x = self.offset(max);
        let Some(bounds) = active.and_then(|id| layout.bounds(id)) else {
            self.set_offset(current_scroll_x, max);
            return;
        };
        let tab_left = bounds.start;
        let tab_right = bounds.end;
        let mut next_scroll_x = current_scroll_x;
        if tab_left < current_scroll_x || tab_right - tab_left > viewport_width {
            next_scroll_x = tab_left;
        } else if tab_right > current_scroll_x + viewport_width {
            next_scroll_x = tab_right - viewport_width;
        }
        self.set_offset(next_scroll_x, max);
    }

    pub fn start_drag(
        &mut self,
        tab_id: TabId,
        visible_index: usize,
        destination_tab: Option<TabId>,
        position: Point<Pixels>,
        layout: &TabStripLayout,
        detachable: bool,
    ) {
        let start_x = f32::from(position.x);
        let start_y = f32::from(position.y);
        let drop_target_index = layout.insertion_at_pointer(start_x, &self.scroll_handle);
        self.drag = Some(TabDragState {
            tab_id,
            detachable,
            from_index: visible_index,
            destination_tab,
            start_x,
            start_y,
            current_x: start_x,
            current_y: start_y,
            tab_widths: layout.live_widths(),
            last_edge_scroll_frame: None,
            active: false,
            mode: TabDragMode::Pending,
            drop_target_index,
        });
    }
    pub fn advance_edge_scroll(&mut self, layout: &TabStripLayout, window: &Window, cx: &App) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let viewport = self.scroll_handle.bounds();
        let limit = (layout.content_width() - f32::from(viewport.size.width)).max(0.0);
        let offset = f32::from(-self.scroll_handle.offset().x).clamp(0.0, limit);
        let Some(step) = drag
            .edge_scroll_step(viewport)
            .filter(|step| (*step < 0.0 && offset > 0.0) || (*step > 0.0 && offset < limit))
        else {
            drag.last_edge_scroll_frame = None;
            return;
        };

        // Scroll on display frames, not mouse events. Use elapsed time so a high
        // refresh rate or an unrelated redraw doesn't make the strip move faster.
        let now = cx.background_executor().now();
        let elapsed = drag
            .last_edge_scroll_frame
            .replace(now)
            .map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f32().min(0.05)
            });
        let next = (offset + step * elapsed * 60.0).clamp(0.0, limit);
        self.scroll_handle
            .set_offset(gpui::point(px(-next), px(0.0)));
        drag.drop_target_index = layout.insertion_at_pointer(drag.current_x, &self.scroll_handle);
        if (step < 0.0 && next > 0.0) || (step > 0.0 && next < limit) {
            window.request_animation_frame();
        } else {
            drag.last_edge_scroll_frame = None;
        }
    }
}

impl TabDragState {
    pub fn update(
        &mut self,
        point: Point<Pixels>,
        layout: &TabStripLayout,
        scroll: &ScrollHandle,
        bar_height: f32,
        over_content: bool,
    ) -> bool {
        let drag = self;
        let was_active = drag.active;
        let previous_mode = drag.mode.clone();
        let previous_drop_target_index = drag.drop_target_index;
        let viewport = scroll.bounds();
        let previous_edge = drag.edge_scroll_step(viewport).map(f32::signum);
        // Browser tab drags keep pointer capture after leaving the tab label;
        // the root mouse-up is responsible for finishing or cancelling.
        drag.current_x = f32::from(point.x);
        drag.current_y = f32::from(point.y);
        drag.tab_widths = layout.live_widths();
        let delta_x = drag.current_x - drag.start_x;
        let delta_y = drag.current_y - drag.start_y;
        if over_content {
            drag.active = true;
            drag.mode = TabDragMode::Content;
        } else if drag.detachable && tab_drag_is_detach(delta_x, delta_y, bar_height) {
            drag.active = true;
            drag.mode = TabDragMode::Detach;
            drag.drop_target_index = drag.from_index;
        } else if tab_drag_is_horizontal_reorder(delta_x, delta_y) {
            drag.active = true;
            drag.mode = TabDragMode::Reorder;
            drag.drop_target_index = layout.insertion_at_pointer(drag.current_x, scroll);
        } else {
            drag.active = false;
            drag.mode = TabDragMode::Pending;
            drag.drop_target_index = drag.from_index;
        }
        let edge_changed = previous_edge != drag.edge_scroll_step(viewport).map(f32::signum);
        if edge_changed {
            drag.last_edge_scroll_frame = None;
        }
        edge_changed
            || drag.active != was_active
            || drag.mode != previous_mode
            || drag.drop_target_index != previous_drop_target_index
    }

    pub fn release(self) -> Option<ShellTabRequest> {
        match self.mode {
            TabDragMode::Detach if self.detachable => Some(ShellTabRequest::Detach(self)),
            TabDragMode::Reorder if self.active => Some(ShellTabRequest::Reorder(
                self.tab_id,
                tab_reorder_target_visible_index(self.from_index, self.drop_target_index),
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TabId, TabStripItem, TabStripItemKind, TabStripLayout};

    #[test]
    fn hovering_a_drag_skips_leading_space_and_transient_tabs() {
        use TabStripItemKind::{Exiting, Live, Returning};
        let layout = TabStripLayout {
            leading: 10.,
            items: [
                (1, 20., Live),
                (2, 12., Exiting(0)),
                (3, 18., Returning),
                (4, 30., Live),
            ]
            .map(|(id, width, kind)| TabStripItem {
                tab_id: TabId(id),
                width,
                kind,
            })
            .into(),
        };
        let hits = [9., 10., 29., 30., 42., 59., 60., 89., 90.].map(|x| layout.live_tab_at(x));
        assert_eq!(
            hits,
            [
                None,
                Some(TabId(1)),
                Some(TabId(1)),
                None,
                None,
                None,
                Some(TabId(4)),
                Some(TabId(4)),
                None
            ]
        );
    }
}
