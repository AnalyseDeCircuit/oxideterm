use std::panic::Location;

use gpui::{
    AnyElement, App, Div, Element, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, ScrollHandle, Stateful, StatefulInteractiveElement, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder, px,
};

const SCROLLBAR_LAYER_WIDTH: f32 = 10.0;
const SCROLLBAR_THUMB_WIDTH: f32 = 5.0;
const SCROLLBAR_THUMB_RADIUS: f32 = 3.0;
const SCROLLBAR_THUMB_RIGHT_INSET: f32 = 2.0;
const SCROLLBAR_MIN_THUMB_LENGTH: f32 = 32.0;
const SCROLLBAR_THUMB_ALPHA: f32 = 0.28;

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
    let max_offset_y = f32::from(scroll_handle.max_offset().height);
    let offset_y = f32::from(scroll_handle.offset().y).clamp(0.0, max_offset_y);
    if viewport_height <= 0.0 || max_offset_y <= 0.0 {
        return div().id(id).into_any_element();
    }

    let content_height = viewport_height + max_offset_y;
    let thumb_height = (viewport_height / content_height * viewport_height)
        .clamp(SCROLLBAR_MIN_THUMB_LENGTH, viewport_height);
    let thumb_top = if max_offset_y <= 0.0 {
        0.0
    } else {
        offset_y / max_offset_y * (viewport_height - thumb_height)
    };
    let thumb_color = window.text_style().color.alpha(SCROLLBAR_THUMB_ALPHA);

    // The layer is visual-only; wheel/trackpad input remains owned by the
    // GPUI scroll container through the shared ScrollHandle.
    div()
        .id(id)
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(SCROLLBAR_LAYER_WIDTH))
        .child(
            div()
                .absolute()
                .right(px(SCROLLBAR_THUMB_RIGHT_INSET))
                .top(px(thumb_top))
                .w(px(SCROLLBAR_THUMB_WIDTH))
                .h(px(thumb_height))
                .rounded(px(SCROLLBAR_THUMB_RADIUS))
                .bg(thumb_color),
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
    let max_offset_x = f32::from(scroll_handle.max_offset().width);
    let offset_x = f32::from(scroll_handle.offset().x).clamp(0.0, max_offset_x);
    if viewport_width <= 0.0 || max_offset_x <= 0.0 {
        return div().id(id).into_any_element();
    }

    let content_width = viewport_width + max_offset_x;
    let thumb_width = (viewport_width / content_width * viewport_width)
        .clamp(SCROLLBAR_MIN_THUMB_LENGTH, viewport_width);
    let thumb_left = if max_offset_x <= 0.0 {
        0.0
    } else {
        offset_x / max_offset_x * (viewport_width - thumb_width)
    };
    let thumb_color = window.text_style().color.alpha(SCROLLBAR_THUMB_ALPHA);

    div()
        .id(id)
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(SCROLLBAR_LAYER_WIDTH))
        .child(
            div()
                .absolute()
                .left(px(thumb_left))
                .bottom(px(SCROLLBAR_THUMB_RIGHT_INSET))
                .w(px(thumb_width))
                .h(px(SCROLLBAR_THUMB_WIDTH))
                .rounded(px(SCROLLBAR_THUMB_RADIUS))
                .bg(thumb_color),
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
