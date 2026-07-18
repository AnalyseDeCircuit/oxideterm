use std::{cell::Cell, panic::Location, rc::Rc};

use gpui::{
    AnyElement, App, AppContext, CursorStyle, Div, Element, ElementId, EmptyView,
    InteractiveElement, IntoElement, ParentElement, Point, RenderOnce, ScrollHandle, Stateful,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, prelude::FluentBuilder, px,
};

const SCROLLBAR_LAYER_WIDTH: f32 = 10.0;
const SCROLLBAR_THUMB_WIDTH: f32 = 5.0;
const SCROLLBAR_THUMB_RADIUS: f32 = 3.0;
const SCROLLBAR_THUMB_RIGHT_INSET: f32 = 2.0;
const SCROLLBAR_MIN_THUMB_LENGTH: f32 = 32.0;
const SCROLLBAR_THUMB_ALPHA: f32 = 0.28;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarGeometry {
    viewport_length: f32,
    max_offset: f32,
    thumb_length: f32,
    thumb_start: f32,
}

#[derive(Clone)]
struct ScrollbarDragState {
    scroll_handle: ScrollHandle,
    axis: ScrollbarAxis,
    grab_offset: Rc<Cell<f32>>,
}

fn scroll_position_from_handle_offset(offset: f32, max_offset: f32) -> f32 {
    // GPUI stores scroll offsets as negative content translations.
    (-offset).clamp(0.0, max_offset)
}

fn scrollbar_geometry(
    viewport_length: f32,
    max_offset: f32,
    scroll_position: f32,
) -> Option<ScrollbarGeometry> {
    if viewport_length <= 0.0 || max_offset <= 0.0 {
        return None;
    }
    let content_length = viewport_length + max_offset;
    // Compact scroll surfaces can be shorter than the preferred thumb size.
    // Cap the minimum first so `clamp` always receives an ordered range.
    let minimum_thumb_length = SCROLLBAR_MIN_THUMB_LENGTH.min(viewport_length);
    let thumb_length = (viewport_length / content_length * viewport_length)
        .clamp(minimum_thumb_length, viewport_length);
    let thumb_travel = (viewport_length - thumb_length).max(0.0);
    let thumb_start = scroll_position.clamp(0.0, max_offset) / max_offset * thumb_travel;
    Some(ScrollbarGeometry {
        viewport_length,
        max_offset,
        thumb_length,
        thumb_start,
    })
}

fn scroll_position_for_thumb_start(thumb_start: f32, geometry: ScrollbarGeometry) -> f32 {
    let thumb_travel = (geometry.viewport_length - geometry.thumb_length).max(0.0);
    if thumb_travel <= 0.0 {
        return 0.0;
    }
    thumb_start.clamp(0.0, thumb_travel) / thumb_travel * geometry.max_offset
}

impl ScrollbarDragState {
    fn update(&self, pointer: Point<gpui::Pixels>, window: &mut Window) {
        let bounds = self.scroll_handle.bounds();
        let max_offset = self.scroll_handle.max_offset();
        let (viewport_length, maximum, pointer_position, track_start) = match self.axis {
            ScrollbarAxis::Vertical => (
                f32::from(bounds.size.height),
                f32::from(max_offset.y),
                f32::from(pointer.y),
                f32::from(bounds.top()),
            ),
            ScrollbarAxis::Horizontal => (
                f32::from(bounds.size.width),
                f32::from(max_offset.x),
                f32::from(pointer.x),
                f32::from(bounds.left()),
            ),
            ScrollbarAxis::Both => return,
        };
        let Some(geometry) = scrollbar_geometry(viewport_length, maximum, 0.0) else {
            return;
        };
        let thumb_start = pointer_position - track_start - self.grab_offset.get();
        let scroll_position = scroll_position_for_thumb_start(thumb_start, geometry);
        let current = self.scroll_handle.offset();
        let next = match self.axis {
            ScrollbarAxis::Vertical => Point::new(current.x, px(-scroll_position)),
            ScrollbarAxis::Horizontal => Point::new(px(-scroll_position), current.y),
            ScrollbarAxis::Both => return,
        };
        if current != next {
            self.scroll_handle.set_offset(next);
            window.refresh();
        }
    }
}

pub trait ScrollableElement: InteractiveElement + Styled + ParentElement + Element + Sized {
    fn vertical_scrollbar(self, scroll_handle: &ScrollHandle) -> Self {
        self.child(
            Scrollbar::new(scroll_handle)
                .id("scrollbar_layer")
                .axis(ScrollbarAxis::Vertical),
        )
    }

    fn horizontal_scrollbar(self, scroll_handle: &ScrollHandle) -> Self {
        self.child(
            Scrollbar::new(scroll_handle)
                .id("scrollbar_layer")
                .axis(ScrollbarAxis::Horizontal),
        )
    }

    fn overflow_y_scrollbar(self) -> Scrollable<Self> {
        Scrollable::new(self, ScrollbarAxis::Vertical)
    }

    fn overflow_x_scrollbar(self) -> Scrollable<Self> {
        Scrollable::new(self, ScrollbarAxis::Horizontal)
    }

    fn overflow_scrollbar(self) -> Scrollable<Self> {
        Scrollable::new(self, ScrollbarAxis::Both)
    }
}

impl ScrollableElement for Div {}

impl<E> ScrollableElement for Stateful<E>
where
    E: ParentElement + Styled + Element,
    Self: InteractiveElement,
{
}

#[derive(IntoElement)]
pub struct Scrollable<E: InteractiveElement + Styled + ParentElement + Element> {
    id: ElementId,
    element: E,
    axis: ScrollbarAxis,
}

impl<E> Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    #[track_caller]
    fn new(element: E, axis: ScrollbarAxis) -> Self {
        Self {
            id: ElementId::CodeLocation(*Location::caller()),
            element,
            axis,
        }
    }
}

impl<E> Styled for Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl<E> ParentElement for Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.element.extend(elements);
    }
}

impl InteractiveElement for Scrollable<Div> {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.element.interactivity()
    }
}

impl InteractiveElement for Scrollable<Stateful<Div>> {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.element.interactivity()
    }
}

impl<E> RenderOnce for Scrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element + 'static,
{
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scroll_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::new())
            .read(cx)
            .clone();
        let style = self.element.style().clone();
        *self.element.style() = StyleRefinement::default();

        let mut root = div().id(self.id).size_full().relative();
        *root.style() = style;

        root.child(
            div()
                .id("scroll-area")
                .flex()
                .size_full()
                .map(|this| match self.axis {
                    ScrollbarAxis::Vertical => this.flex_col().overflow_y_scroll(),
                    ScrollbarAxis::Horizontal => this.flex_row().overflow_x_scroll(),
                    ScrollbarAxis::Both => this.overflow_scroll(),
                })
                .track_scroll(&scroll_handle)
                .child(self.element.flex_1()),
        )
        .child(
            Scrollbar::new(&scroll_handle)
                .id("scrollbar")
                .axis(self.axis),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollbarAxis {
    Vertical,
    Horizontal,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollViewportKind {
    VirtualList,
    TrackedOverflow,
    Terminal,
    HorizontalTabs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScrollViewportContract {
    pub kind: ScrollViewportKind,
    pub visible_scrollbar: bool,
    pub anchored_overlays: bool,
}

impl ScrollViewportContract {
    pub const fn new(kind: ScrollViewportKind) -> Self {
        Self {
            kind,
            visible_scrollbar: matches!(
                kind,
                ScrollViewportKind::TrackedOverflow | ScrollViewportKind::Terminal
            ),
            anchored_overlays: !matches!(kind, ScrollViewportKind::HorizontalTabs),
        }
    }
}

#[derive(IntoElement)]
pub struct Scrollbar {
    id: ElementId,
    scroll_handle: ScrollHandle,
    axis: ScrollbarAxis,
}

impl Scrollbar {
    pub fn new(scroll_handle: &ScrollHandle) -> Self {
        Self {
            id: "scrollbar".into(),
            scroll_handle: scroll_handle.clone(),
            axis: ScrollbarAxis::Vertical,
        }
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    pub fn axis(mut self, axis: ScrollbarAxis) -> Self {
        self.axis = axis;
        self
    }
}

impl RenderOnce for Scrollbar {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        match self.axis {
            ScrollbarAxis::Vertical => {
                render_vertical_scrollbar(self.id, &self.scroll_handle, window)
            }
            ScrollbarAxis::Horizontal => {
                render_horizontal_scrollbar(self.id, &self.scroll_handle, window)
            }
            ScrollbarAxis::Both => div()
                .id(self.id)
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(render_vertical_scrollbar(
                    "vertical-scrollbar",
                    &self.scroll_handle,
                    window,
                ))
                .child(render_horizontal_scrollbar(
                    "horizontal-scrollbar",
                    &self.scroll_handle,
                    window,
                ))
                .into_any_element(),
        }
    }
}

fn render_vertical_scrollbar(
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    window: &mut Window,
) -> AnyElement {
    let bounds = scroll_handle.bounds();
    let viewport_height = f32::from(bounds.size.height);
    let max_offset_y = f32::from(scroll_handle.max_offset().y);
    let scroll_position =
        scroll_position_from_handle_offset(f32::from(scroll_handle.offset().y), max_offset_y);
    let Some(geometry) = scrollbar_geometry(viewport_height, max_offset_y, scroll_position) else {
        return div().id(id).into_any_element();
    };
    let thumb_color = window.text_style().color.alpha(SCROLLBAR_THUMB_ALPHA);
    let drag_state = ScrollbarDragState {
        scroll_handle: scroll_handle.clone(),
        axis: ScrollbarAxis::Vertical,
        grab_offset: Rc::new(Cell::new(0.0)),
    };

    div()
        .id(id)
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(SCROLLBAR_LAYER_WIDTH))
        .child(
            div()
                .id("vertical-scrollbar-thumb")
                .absolute()
                .right(px(SCROLLBAR_THUMB_RIGHT_INSET))
                .top(px(geometry.thumb_start))
                .w(px(SCROLLBAR_THUMB_WIDTH))
                .h(px(geometry.thumb_length))
                .rounded(px(SCROLLBAR_THUMB_RADIUS))
                .bg(thumb_color)
                .cursor(CursorStyle::OpenHand)
                .on_drag(drag_state.clone(), |drag, position, _window, cx| {
                    drag.grab_offset.set(f32::from(position.y));
                    cx.new(|_| EmptyView)
                })
                .on_drag_move::<ScrollbarDragState>(|event, window, cx| {
                    event.drag(cx).update(event.event.position, window);
                    cx.stop_propagation();
                }),
        )
        .into_any_element()
}

fn render_horizontal_scrollbar(
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    window: &mut Window,
) -> AnyElement {
    let bounds = scroll_handle.bounds();
    let viewport_width = f32::from(bounds.size.width);
    let max_offset_x = f32::from(scroll_handle.max_offset().x);
    let scroll_position =
        scroll_position_from_handle_offset(f32::from(scroll_handle.offset().x), max_offset_x);
    let Some(geometry) = scrollbar_geometry(viewport_width, max_offset_x, scroll_position) else {
        return div().id(id).into_any_element();
    };
    let thumb_color = window.text_style().color.alpha(SCROLLBAR_THUMB_ALPHA);
    let drag_state = ScrollbarDragState {
        scroll_handle: scroll_handle.clone(),
        axis: ScrollbarAxis::Horizontal,
        grab_offset: Rc::new(Cell::new(0.0)),
    };

    div()
        .id(id)
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(SCROLLBAR_LAYER_WIDTH))
        .child(
            div()
                .id("horizontal-scrollbar-thumb")
                .absolute()
                .left(px(geometry.thumb_start))
                .bottom(px(SCROLLBAR_THUMB_RIGHT_INSET))
                .w(px(geometry.thumb_length))
                .h(px(SCROLLBAR_THUMB_WIDTH))
                .rounded(px(SCROLLBAR_THUMB_RADIUS))
                .bg(thumb_color)
                .cursor(CursorStyle::OpenHand)
                .on_drag(drag_state.clone(), |drag, position, _window, cx| {
                    drag.grab_offset.set(f32::from(position.x));
                    cx.new(|_| EmptyView)
                })
                .on_drag_move::<ScrollbarDragState>(|event, window, cx| {
                    event.drag(cx).update(event.event.position, window);
                    cx.stop_propagation();
                }),
        )
        .into_any_element()
}

pub fn vertical_scrollbar_layer(id: impl Into<ElementId>, handle: &ScrollHandle) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(Scrollbar::new(handle).id(id))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_viewport_contract_documents_default_scrollbar_behavior() {
        assert!(ScrollViewportContract::new(ScrollViewportKind::TrackedOverflow).visible_scrollbar);
        assert!(!ScrollViewportContract::new(ScrollViewportKind::VirtualList).visible_scrollbar);
    }

    #[test]
    fn horizontal_tab_scroll_contract_does_not_require_overlay_anchors() {
        assert!(!ScrollViewportContract::new(ScrollViewportKind::HorizontalTabs).anchored_overlays);
    }

    #[test]
    fn scrollbar_position_uses_negative_gpui_content_offset() {
        assert_eq!(scroll_position_from_handle_offset(-125.0, 300.0), 125.0);
        assert_eq!(scroll_position_from_handle_offset(20.0, 300.0), 0.0);
    }

    #[test]
    fn scrollbar_thumb_edges_map_to_scroll_range_edges() {
        let geometry = scrollbar_geometry(200.0, 600.0, 300.0).expect("scrollbar geometry");
        let thumb_travel = geometry.viewport_length - geometry.thumb_length;

        assert_eq!(scroll_position_for_thumb_start(0.0, geometry), 0.0);
        assert_eq!(
            scroll_position_for_thumb_start(thumb_travel, geometry),
            geometry.max_offset
        );
    }

    #[test]
    fn scrollbar_thumb_fits_viewports_shorter_than_preferred_minimum() {
        let geometry = scrollbar_geometry(24.0, 24.0, 0.0).expect("short scrollbar geometry");

        assert_eq!(geometry.thumb_length, 24.0);
        assert_eq!(geometry.thumb_start, 0.0);
        assert_eq!(scroll_position_for_thumb_start(0.0, geometry), 0.0);
    }
}
