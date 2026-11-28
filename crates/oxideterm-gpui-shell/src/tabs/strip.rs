use super::geometry::*;
use gpui::{
    AnyElement, App, Context, CursorStyle, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels,
    Point, Rgba, ScrollEvent, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    button::{IconButtonOptions, icon_button},
    icons::LucideIcon,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabId;
use std::rc::Rc;

pub enum ShellTabRequest {
    Select(TabId),
    CloseTab { tab: TabId, select_first: bool },
    Reorder(TabId, usize),
    NewTab,
    ContextMenu(TabId, Point<Pixels>),
    DoubleClick(TabId),
    Detach(TabDragState),
    ShowTitle(TabId, String, Point<Pixels>),
    HideTitle(TabId),
    DragChanged,
    CancelDrag(TabId),
}

pub struct TabVisual {
    pub title: String,
    pub icon: AnyElement,
    pub status: Option<AnyElement>,
    pub bell: bool,
    pub unread: bool,
    pub closeable: bool,
    pub draggable: bool,
    pub detachable: bool,
}
impl TabVisual {
    pub fn color(tokens: &ThemeTokens, active: bool, bell: bool, unread: bool) -> Rgba {
        rgb(if active {
            tokens.ui.text
        } else if bell {
            tokens.ui.warning
        } else if unread {
            tokens.ui.success
        } else {
            tokens.ui.text_muted
        })
    }
}
pub struct TabStripFrame {
    pub tokens: ThemeTokens,
    pub layout: TabStripLayout,
    /// In measured item order, including return placeholders and exit snapshots.
    pub visuals: Vec<TabVisual>,
    pub active: Option<TabId>,
    pub supports_detach: bool,
    pub background: Rgba,
    pub divider: Rgba,
    pub active_background: Rgba,
    pub idle_background: Rgba,
    pub translucent: bool,
    pub leading_margin: f32,
    pub filler: Option<AnyElement>,
    pub quick_actions: Option<AnyElement>,
    pub quick_actions_width: f32,
    pub new_tab_label: Option<String>,
}
type FrameSource = dyn Fn(&TabStripState, &Window, &mut App) -> Option<TabStripFrame>;
type Dispatch = dyn Fn(ShellTabRequest, &mut Window, &mut App);

/// Hosts with split targets can claim a drag outside the strip before native detach.
pub enum TabContentDrop<'a> {
    Probe(&'a mut TabDragState, &'a MouseMoveEvent),
    Commit(TabId, Point<Pixels>),
}
type ContentDrop = dyn Fn(TabContentDrop<'_>, &mut Window, &mut App) -> bool;

pub struct MeasuredTabStrip {
    content_drop: Option<Rc<ContentDrop>>,
    pub state: TabStripState,
    frame: Box<FrameSource>,
    dispatch: Rc<Dispatch>,
    scrollbar_hovered: bool,
    scrollbar_grab: Option<f32>,
    content_width: f32,
    painted_layout: Option<Rc<TabStripLayout>>,
}

impl MeasuredTabStrip {
    pub fn new(
        frame: impl Fn(&TabStripState, &Window, &mut App) -> Option<TabStripFrame> + 'static,
        dispatch: impl Fn(ShellTabRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            state: TabStripState::default(),
            frame: Box::new(frame),
            dispatch: Rc::new(dispatch),
            scrollbar_hovered: false,
            scrollbar_grab: None,
            content_width: 0.,
            painted_layout: None,
            content_drop: None,
        }
    }

    pub fn live_tab_at(&self, position: Point<Pixels>) -> Option<TabId> {
        let viewport = self.state.scroll_handle.bounds();
        if !viewport.contains(&position) {
            return None;
        }
        self.painted_layout.as_ref()?.live_tab_at(f32::from(
            position.x - viewport.origin.x - self.state.scroll_handle.offset().x,
        ))
    }

    pub fn displayed_tab_width(&self, id: TabId) -> Option<f32> {
        self.painted_layout
            .as_ref()?
            .bounds(id)
            .map(|b| b.end - b.start)
    }

    pub fn with_content_drop(
        mut self,
        handler: impl Fn(TabContentDrop<'_>, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.content_drop = Some(Rc::new(handler));
        self
    }

    fn pointer_capture(
        &self,
        layout: Rc<TabStripLayout>,
        height: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let owner = cx.weak_entity();
        let dispatch = self.dispatch.clone();
        let drop_target = self.content_drop.clone();
        gpui::canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let (owner, dispatch, drop_target, layout) = (
                    owner.clone(),
                    dispatch.clone(),
                    drop_target.clone(),
                    layout.clone(),
                );
                let (up_owner, up_dispatch, up_drop) =
                    (owner.clone(), dispatch.clone(), drop_target.clone());
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture {
                        return;
                    }
                    let Some(strip) = owner.upgrade() else { return };
                    if strip.read(cx).scrollbar_grab.is_some() {
                        strip.update(cx, |strip, cx| strip.drag_scrollbar(event, cx));
                        cx.stop_propagation();
                        return;
                    }
                    let Some(mut drag) = strip.read(cx).state.drag.clone() else {
                        return;
                    };
                    if event.pressed_button != Some(MouseButton::Left) {
                        strip.update(cx, |strip, cx| {
                            strip.state.drag = None;
                            cx.notify()
                        });
                        dispatch(ShellTabRequest::CancelDrag(drag.tab_id), window, cx);
                    } else {
                        let dx = f32::from(event.position.x) - drag.start_x;
                        let dy = f32::from(event.position.y) - drag.start_y;
                        let over = dx.hypot(dy) > TAB_DRAG_THRESHOLD_PX
                            && drop_target.as_ref().is_some_and(|target| {
                                target(TabContentDrop::Probe(&mut drag, event), window, cx)
                            });
                        let changed = strip.update(cx, |strip, cx| {
                            let changed = drag.update(
                                event.position,
                                &layout,
                                &strip.state.scroll_handle,
                                height,
                                over,
                            );
                            strip.state.drag = Some(drag);
                            if changed {
                                cx.notify()
                            }
                            changed
                        });
                        if changed {
                            dispatch(ShellTabRequest::DragChanged, window, cx)
                        }
                    }
                    cx.stop_propagation();
                });
                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let Some(strip) = up_owner.upgrade() else {
                        return;
                    };
                    if strip.read(cx).scrollbar_grab.is_some() {
                        strip.update(cx, |strip, cx| strip.finish_scrollbar(cx));
                        cx.stop_propagation();
                        return;
                    }
                    if strip.read(cx).state.drag.is_none() {
                        return;
                    }
                    let drag = strip.update(cx, |strip, cx| {
                        cx.notify();
                        strip.state.drag.take().unwrap()
                    });
                    let claimed = drag.active
                        && up_drop.as_ref().is_some_and(|target| {
                            target(
                                TabContentDrop::Commit(
                                    drag.tab_id,
                                    event.position + window.bounds().origin,
                                ),
                                window,
                                cx,
                            )
                        });
                    if !claimed && let Some(request) = drag.release() {
                        up_dispatch(request, window, cx)
                    }
                    up_dispatch(ShellTabRequest::DragChanged, window, cx);
                    cx.stop_propagation()
                });
            },
        )
        .absolute()
        .into_any_element()
    }

    fn scrollbar_geometry(&self) -> Option<TabbarScrollbarGeometry> {
        let bounds = self.state.scroll_handle.bounds();
        let width = f32::from(bounds.size.width);
        let max = (self.content_width - width).max(0.);
        calculate_tabbar_scrollbar_geometry(
            f32::from(bounds.left()),
            width,
            max,
            self.state.offset(max),
        )
    }
    fn scroll_thumb(&self, x: f32, g: TabbarScrollbarGeometry) {
        if let Some(grab) = self.scrollbar_grab {
            let left = x - g.viewport_left - grab;
            self.state
                .set_offset(tabbar_scroll_x_for_thumb_left(left, g), g.max_scroll);
        }
    }
    pub fn drag_scrollbar(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.scrollbar_grab.is_none() {
            return;
        }
        if let Some(g) = self.scrollbar_geometry().filter(|_| event.dragging()) {
            self.scroll_thumb(f32::from(event.position.x), g);
            cx.stop_propagation();
            cx.notify()
        } else {
            self.finish_scrollbar(cx)
        }
    }
    pub fn finish_scrollbar(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar_grab.take().is_some() {
            cx.notify()
        }
    }

    fn scrollbar(&self, tokens: &ThemeTokens, trailing: f32, cx: &mut Context<Self>) -> AnyElement {
        let Some(geometry) = self.scrollbar_geometry() else {
            return div().into_any_element();
        };
        let scrollbar_dragging = self.scrollbar_grab.is_some();
        let thumb_color = if scrollbar_dragging {
            rgb(tokens.ui.accent)
        } else if self.scrollbar_hovered {
            rgba((tokens.ui.accent << 8) | TABBAR_SCROLLBAR_HOVER_ALPHA)
        } else {
            rgba((tokens.ui.text_muted << 8) | TABBAR_SCROLLBAR_ALPHA)
        };

        // The visible thumb stays subtle, while the transparent hit target remains easy to drag.
        div()
            .id("workspace-tabbar-thin-scrollbar")
            .absolute()
            .left(px(0.0))
            .right(px(trailing))
            .bottom_0()
            .h(px(TABBAR_SCROLLBAR_DRAG_HEIGHT))
            .cursor(if scrollbar_dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                if this.scrollbar_hovered != *hovered {
                    this.scrollbar_hovered = *hovered;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    let Some(g) = this.scrollbar_geometry() else {
                        return;
                    };
                    let x = f32::from(event.position.x);
                    let offset = x - g.viewport_left - g.thumb_left;
                    this.scrollbar_grab = Some(if (0.0..=g.thumb_width).contains(&offset) {
                        offset
                    } else {
                        g.thumb_width / 2.
                    });
                    this.scroll_thumb(x, g);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(geometry.thumb_left))
                    .bottom_0()
                    .w(px(geometry.thumb_width))
                    .h(px(TABBAR_SCROLLBAR_HEIGHT))
                    .rounded(px(TABBAR_SCROLLBAR_RADIUS))
                    .bg(thumb_color),
            )
            .into_any_element()
    }

    pub fn begin_exit(&mut self, mut visual: ExitingTabVisual, cx: &mut Context<Self>) {
        let id = visual.tab_id;
        let timer = cx.background_executor().timer(visual.duration);
        visual.started_at = cx.background_executor().now();
        self.state.exiting_tabs.push(visual);
        cx.spawn(async move |weak, cx| {
            timer.await;
            let _ = weak.update(cx, |strip, cx| {
                if let Some(i) = strip.state.exiting_tabs.iter().position(|e| e.tab_id == id) {
                    let slot = strip.state.exiting_tabs.remove(i).visual_index;
                    for exit in &mut strip.state.exiting_tabs {
                        if exit.visual_index > slot {
                            exit.visual_index -= 1
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

fn icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

impl Render for MeasuredTabStrip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.frame)(&self.state, window, cx) else {
            return div().into_any_element();
        };
        let layout = Rc::new(frame.layout);
        self.content_width = layout.content_width();
        self.painted_layout = Some(layout.clone());
        self.state.advance_edge_scroll(&layout, window, cx);
        let tokens = frame.tokens;
        let theme = tokens.ui;
        let bar = div()
            .ml(px(frame.leading_margin))
            .h(px(tokens.metrics.tabbar_height))
            .flex()
            .flex_row()
            .items_center()
            .relative()
            .overflow_hidden()
            .border_b_1()
            .border_color(frame.divider)
            .bg(frame.background);
        // Tauri's scroll container measures the full inline-flex tab row as
        // scrollWidth. GPUI's ScrollHandle computes max_offset from direct
        // child bounds, so tab items must stay direct children of this viewport.
        // Keep overflow hidden instead of overflow_x_scroll: the custom wheel
        // adapter owns browser-style scrollLeft clamping, while track_scroll
        // still measures and applies the offset without GPUI boundary overscroll.
        let mut scroll_viewport = div()
            .id("workspace-tab-scroll-viewport")
            .debug_selector(|| "workspace-tab-scroll-viewport".into())
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .pl(px(tokens.metrics.tabbar_leading_offset))
            .overflow_hidden()
            .track_scroll(&self.state.scroll_handle)
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, _window, cx| {
                let max = (this.content_width
                    - f32::from(this.state.scroll_handle.bounds().size.width))
                .max(0.);
                let d = event.delta.pixel_delta(px(tokens.metrics.tabbar_height));
                let delta = tabbar_wheel_scroll_delta(f32::from(d.x), f32::from(d.y));
                let current = f32::from(-this.state.scroll_handle.offset().x);
                let next = if max <= 1. {
                    0.
                } else {
                    tabbar_scroll_x_after_wheel(current, delta, max)
                };
                if (current - next).abs() >= 0.01 {
                    this.state.set_offset(next, max);
                    cx.notify()
                }
                if delta != 0. || max <= 1. {
                    cx.stop_propagation()
                }
            }));
        if !self.state.exiting_tabs.is_empty() {
            window.request_animation_frame();
        }
        let live_tab_count = layout
            .items
            .iter()
            .filter(|item| item.kind == TabStripItemKind::Live)
            .count();
        let insertion_slot = self.state.drag.as_ref().and_then(|drag| {
            (drag.mode == TabDragMode::Reorder
                && tab_reorder_target_visible_index(drag.from_index, drag.drop_target_index)
                    != drag.from_index)
                .then_some(drag.drop_target_index)
        });
        let mut visible_tab_index = 0;

        for (item, visual) in layout.items.iter().zip(frame.visuals) {
            let tab_id = item.tab_id;
            let tab_width = item.width;
            if let TabStripItemKind::Exiting(index) = item.kind {
                let exiting = &self.state.exiting_tabs[index];
                let bg = if exiting.was_active {
                    frame.active_background
                } else {
                    frame.idle_background
                };
                scroll_viewport = scroll_viewport.child(exit_visual(
                    &tokens,
                    frame.divider,
                    bg,
                    visual.icon,
                    exiting,
                    item.width,
                    cx.background_executor().now(),
                ));
                continue;
            }
            let tab_text = visual.title;
            if item.kind == TabStripItemKind::Returning {
                scroll_viewport = scroll_viewport.child(return_placeholder(
                    &tokens,
                    tab_id,
                    visual.icon,
                    &tab_text,
                    tab_width,
                ));
                continue;
            }
            let active = frame.active == Some(tab_id);
            let has_bell_highlight = visual.bell;
            let show_reconnect_progress = visual.status.is_some();
            let current_visible_index = visible_tab_index;
            visible_tab_index += 1;
            let drag_state = self.state.drag.as_ref();
            let drag_active = drag_state.is_some_and(|drag| drag.active);
            let is_being_dragged = drag_state.is_some_and(|drag| drag.tab_id == tab_id);
            let after_last = insertion_slot == Some(live_tab_count)
                && current_visible_index + 1 == live_tab_count;

            let tab_text_color = TabVisual::color(&tokens, active, visual.bell, visual.unread);
            let select = self.dispatch.clone();
            let context = self.dispatch.clone();
            let close = self.dispatch.clone();
            let title_move = self.dispatch.clone();
            let title_leave = self.dispatch.clone();
            let close_button = self.dispatch.clone();
            let strip = cx.weak_entity();
            let drag_layout = layout.clone();
            let tooltip = tab_text.clone();
            let detachable = frame.supports_detach && visual.detachable;
            let tab_element = div()
                .id(("workspace-tab", tab_id.0))
                .debug_selector(move || format!("workspace-tab-{}", tab_id.0))
                .h_full()
                .flex_none()
                .w(px(tab_width))
                .px(px(tokens.metrics.tab_padding_x))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(tokens.metrics.tab_gap))
                .border_r_1()
                .border_color(if show_reconnect_progress {
                    rgb(theme.warning)
                } else {
                    frame.divider
                })
                .bg(if active {
                    frame.active_background
                } else {
                    frame.idle_background
                })
                .when(!active, |tab| {
                    tab.hover(|style| style.bg(rgb(theme.bg_hover)))
                })
                .text_color(tab_text_color)
                .opacity(if is_being_dragged && drag_active {
                    0.5
                } else {
                    1.0
                })
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    if visual.draggable && event.click_count == 1 {
                        let _ = strip.update(cx, |strip, cx| {
                            cx.notify();
                            strip.state.start_drag(
                                tab_id,
                                current_visible_index,
                                frame.active.filter(|id| *id != tab_id),
                                event.position,
                                &drag_layout,
                                detachable,
                            )
                        });
                    }
                    select(ShellTabRequest::Select(tab_id), window, cx);
                    if event.click_count == 2 {
                        select(ShellTabRequest::DoubleClick(tab_id), window, cx)
                    }
                    cx.stop_propagation()
                })
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    context(
                        ShellTabRequest::ContextMenu(tab_id, event.position),
                        window,
                        cx,
                    );
                    cx.stop_propagation()
                })
                .on_mouse_down(MouseButton::Middle, move |_, window, cx| {
                    if visual.closeable {
                        close(
                            ShellTabRequest::CloseTab {
                                tab: tab_id,
                                select_first: false,
                            },
                            window,
                            cx,
                        )
                    }
                    cx.stop_propagation()
                })
                .on_mouse_move(move |event, window, cx| {
                    title_move(
                        ShellTabRequest::ShowTitle(tab_id, tooltip.clone(), event.position),
                        window,
                        cx,
                    )
                })
                .on_hover(move |hovered, window, cx| {
                    if !hovered {
                        title_leave(ShellTabRequest::HideTitle(tab_id), window, cx)
                    }
                })
                .when(
                    insertion_slot == Some(current_visible_index) || after_last,
                    |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .when_else(after_last, |line| line.right_0(), |line| line.left_0())
                                .top_0()
                                .bottom_0()
                                .w(px(2.0))
                                .bg(rgb(theme.accent)),
                        )
                    },
                )
                .when(active || show_reconnect_progress, |tab| {
                    tab.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(tokens.metrics.tab_active_accent_height))
                            .bg(rgb(if show_reconnect_progress {
                                theme.warning
                            } else {
                                theme.accent
                            })),
                    )
                })
                .child(visual.icon)
                .child(
                    div()
                        .id(("workspace-tab-title", tab_id.0))
                        .debug_selector(move || format!("workspace-tab-title-{}", tab_id.0))
                        .flex_1()
                        .whitespace_nowrap()
                        .text_size(px(tokens.metrics.tab_font_size))
                        .child(tab_text),
                )
                .when(has_bell_highlight && !active, |tab| {
                    tab.child(
                        div()
                            .size(px(6.0))
                            .rounded_full()
                            .bg(rgb(theme.warning))
                            .flex_shrink_0(),
                    )
                })
                .children(visual.status)
                .when(visual.closeable && !show_reconnect_progress, |tab| {
                    tab.child(
                        div()
                            .size(px(tokens.metrics.tab_close_button_size))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(tokens.radii.sm))
                            .cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .child(icon(
                                LucideIcon::X,
                                tokens.metrics.tab_close_icon_size,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                close_button(
                                    ShellTabRequest::CloseTab {
                                        tab: tab_id,
                                        select_first: true,
                                    },
                                    window,
                                    cx,
                                );
                                cx.stop_propagation()
                            }),
                    )
                });
            scroll_viewport = scroll_viewport.child(oxideterm_gpui_ui::motion::slide_fade_in_x(
                &tokens,
                ("workspace-tab-open", tab_id.0),
                div()
                    .w(px(tab_width))
                    .h_full()
                    .flex_none()
                    .child(tab_element),
                -12.0,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            ));
        }

        let trailing = frame.quick_actions_width
            + if frame.new_tab_label.is_some() {
                tokens.metrics.tabbar_height
            } else {
                0.
            };
        let new_tab = self.dispatch.clone();
        let accessible_new_tab = new_tab.clone();
        let bar = bar
            .child(self.pointer_capture(layout, tokens.metrics.tabbar_height, cx))
            .child(scroll_viewport.children(frame.filler))
            .child(self.scrollbar(&tokens, trailing, cx))
            .when_some(frame.new_tab_label, |bar, label| {
                bar.child(
                    icon_button(
                        &tokens,
                        icon(
                            LucideIcon::Plus,
                            tokens.metrics.sidebar_action_icon_size,
                            rgb(theme.text_muted),
                        ),
                        IconButtonOptions::compact(tokens.metrics.tabbar_height),
                    )
                    .id("workspace-new-tab")
                    .flex_none()
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, cx| {
                        accessible_new_tab(ShellTabRequest::NewTab, window, cx)
                    })
                    .tooltip(move |_, cx| {
                        oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
                    })
                    .on_click(move |_, window, cx| new_tab(ShellTabRequest::NewTab, window, cx)),
                )
            })
            .children(frame.quick_actions)
            .into_any_element();
        if frame.translucent {
            oxideterm_gpui_ui::surface::tab_surface(&tokens, bar)
        } else {
            bar
        }
    }
}

fn return_placeholder(
    tokens: &ThemeTokens,
    tab_id: TabId,
    tab_icon: AnyElement,
    tab_title: &str,
    tab_width: f32,
) -> AnyElement {
    let theme = tokens.ui;
    let accent = theme.accent;
    let placeholder = div()
        .w_full()
        .h(px((tokens.metrics.tabbar_height - 8.0).max(24.0)))
        .px(px(tokens.metrics.tab_padding_x))
        .flex()
        .items_center()
        .gap(px(tokens.metrics.tab_gap))
        .rounded(px(tokens.radii.sm))
        .border_1()
        .border_color(rgba((accent << 8) | 0x88))
        .bg(rgba((accent << 8) | 0x18))
        .text_color(rgba((theme.text << 8) | 0xcc))
        .child(tab_icon)
        .child(
            div()
                .flex_1()
                .whitespace_nowrap()
                .text_size(px(tokens.metrics.tab_font_size))
                .child(tab_title.to_string()),
        )
        .child(icon(
            LucideIcon::PanelLeft,
            tokens.metrics.tab_close_icon_size,
            rgba((accent << 8) | 0xcc),
        ));

    // The placeholder participates in tab layout, so later tabs move out
    // of the future slot before the detached window is actually closed.
    div()
        .id(("detached-tab-return-placeholder", tab_id.0))
        .w(px(tab_width))
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .child(placeholder)
        .into_any_element()
}

fn exit_visual(
    tokens: &ThemeTokens,
    divider: Rgba,
    bg: Rgba,
    tab_icon: AnyElement,
    exiting: &ExitingTabVisual,
    width: f32,
    now: web_time::Instant,
) -> AnyElement {
    let theme = tokens.ui;
    let tab = div()
        .h_full()
        .flex_none()
        .w(px(exiting.width))
        .relative()
        .px(px(tokens.metrics.tab_padding_x))
        .flex()
        .items_center()
        .gap(px(tokens.metrics.tab_gap))
        .border_r_1()
        .border_color(divider)
        .bg(bg)
        .text_color(rgb(if exiting.was_active {
            theme.text
        } else {
            theme.text_muted
        }))
        .when(exiting.was_active, |tab| {
            tab.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(tokens.metrics.tab_active_accent_height))
                    .bg(rgb(theme.accent)),
            )
        })
        .child(tab_icon)
        .child(
            div()
                .flex_1()
                .whitespace_nowrap()
                .text_size(px(tokens.metrics.tab_font_size))
                .child(exiting.title.clone()),
        )
        .child(icon(
            LucideIcon::X,
            tokens.metrics.tab_close_icon_size,
            rgb(theme.text_muted),
        ));
    let visibility = exiting.exit_visibility(now, tokens.motion.enabled);
    let content_opacity = (visibility * 1.45).clamp(0.0, 1.0);
    // The strip and the clipping viewport consume the same animated width, while
    // inner text/icons clear earlier so labels never crush against the edge.
    let tab_id = exiting.tab_id;
    div()
        .id(("workspace-tab-exit", exiting.tab_id.0))
        .debug_selector(move || format!("workspace-tab-exit-{}", tab_id.0))
        .h_full()
        .w(px(width))
        .flex_none()
        .overflow_hidden()
        .opacity(visibility)
        .child(tab.opacity(content_opacity))
        .into_any_element()
}
