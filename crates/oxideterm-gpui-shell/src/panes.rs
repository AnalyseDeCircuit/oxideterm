use gpui::{
    AnyElement, Bounds, Context, CursorStyle, MouseButton, MouseDownEvent, Pixels, Point, Rgba,
    SharedString, Window, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button},
    icons::LucideIcon,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::{
    PaneId, PaneNode, SplitDirection, TabId, TerminalSessionId, adjusted_split_sizes,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub const SPLIT_HANDLE_LINE_ALPHA: u32 = 0xd9;
const SPLIT_HANDLE_HOVER_BG_ALPHA: u32 = 0x12;
const SPLIT_HANDLE_ACTIVE_BG_ALPHA: u32 = 0x1f;
const SPLIT_HANDLE_HOVER_LINE_ALPHA: u32 = 0xcc;
const SPLIT_HANDLE_ACTIVE_LINE_ALPHA: u32 = 0xff;
const SPLIT_HANDLE_LINE_WIDTH: f32 = 3.0;
const SPLIT_HANDLE_HOVER_LINE_WIDTH: f32 = 4.0;
const SPLIT_HANDLE_ACTIVE_LINE_WIDTH: f32 = 5.0;

#[derive(Clone)]
pub struct SplitDrag {
    pub tab_id: TabId,
    group_id: PaneId,
    handle_index: usize,
    direction: SplitDirection,
    start_position: gpui::Point<Pixels>,
    start_sizes: Vec<f32>,
    start_extent: f32,
}

impl SplitDrag {
    pub fn resize_at(&self, point: Point<Pixels>) -> ShellPaneRequest {
        let distance = match self.direction {
            SplitDirection::Horizontal => point.x - self.start_position.x,
            SplitDirection::Vertical => point.y - self.start_position.y,
        };
        let delta = f32::from(distance) / self.start_extent.max(1.0) * 100.;
        ShellPaneRequest::Resize(
            self.tab_id,
            self.group_id,
            adjusted_split_sizes(&self.start_sizes, self.handle_index, delta),
        )
    }
}

#[derive(Clone, Copy)]
pub enum PaneContent {
    Page(TabId),
    Terminal(TerminalSessionId),
}

#[derive(Clone)]
struct DraggedPane {
    tab: TabId,
    pane: PaneId,
    title: SharedString,
    tokens: ThemeTokens,
}
impl Render for DraggedPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_panel))
            .text_color(rgb(self.tokens.ui.text))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .child(self.title.clone())
    }
}

pub struct MountedPane {
    pub title: String,
    pub body: AnyElement,
    /// Embedded views own their pointer events, so select them during capture.
    pub capture_focus: bool,
    pub toolbar: Option<(AnyElement, f32)>,
    pub overlays: Vec<AnyElement>,
}

#[derive(Clone)]
pub enum ShellPaneRequest {
    Focus(TabId, PaneId, PaneContent),
    ToggleZoom(TabId, PaneId),
    Swap(TabId, PaneId, PaneId),
    MoveOut(TabId, PaneId, PaneContent, bool),
    ClosePane(TabId, PaneId, PaneContent),
    StartResize(SplitDrag),
    Resize(TabId, PaneId, Vec<f32>),
    Reset(TabId, PaneId),
    Drop {
        source: TabId,
        target: SplitDropTarget,
    },
}

pub trait PaneHost: Sized + 'static {
    fn mount_pane(
        &mut self,
        tab: TabId,
        pane: PaneId,
        content: PaneContent,
        split: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<MountedPane>;
    fn pane_request(
        &mut self,
        request: ShellPaneRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool;
}

pub struct PaneTree<'a> {
    pub tab: TabId,
    pub active: Option<PaneId>,
    pub zoomed: Option<PaneId>,
    pub drag: Option<SplitDrag>,
    pub supports_detach: bool,
    pub drop_regions: Option<Rc<RefCell<Vec<SplitDropRegion>>>>,
    pub tokens: ThemeTokens,
    pub i18n: &'a I18n,
    pub header_background: Rgba,
    pub divider: Rgba,
}

impl PaneTree<'_> {
    pub fn render<H: PaneHost>(
        &self,
        root: &PaneNode,
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let split = root.pane_count() > 1;
        if split
            && let Some((pane, session)) = self.zoomed.and_then(|pane| {
                root.session_id_for_pane(pane)
                    .map(|session| (pane, session))
            })
        {
            return self.leaf(
                pane,
                PaneContent::Terminal(session),
                split,
                host,
                window,
                cx,
            );
        }
        self.render_node(root, split, host, window, cx)
    }

    fn leaf<H: PaneHost>(
        &self,
        pane: PaneId,
        content: PaneContent,
        split: bool,
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let tab = self.tab;
        let Some(mount) = host.mount_pane(tab, pane, content, split, window, cx) else {
            return div().size_full().into_any_element();
        };
        let id = match content {
            PaneContent::Page(_) => "workspace-page",
            PaneContent::Terminal(_) => "workspace-pane",
        };
        let focus = cx.listener(move |host, _: &MouseDownEvent, window, cx| {
            host.pane_request(ShellPaneRequest::Focus(tab, pane, content), window, cx);
        });
        let header = split.then(|| self.header(pane, content, mount.title, cx));
        let frame = div()
            .id((id, pane.0))
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden();
        let frame = if mount.capture_focus {
            frame
                .flex()
                .flex_col()
                .capture_any_mouse_down(focus)
                .children(header)
                .child(div().flex_1().min_h_0().relative().child(mount.body))
        } else {
            let top = mount.toolbar.as_ref().map_or(0., |(_, height)| *height)
                + if split { 28. } else { 0. };
            frame
                .relative()
                .on_mouse_down(MouseButton::Left, focus)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .flex()
                        .flex_col()
                        .children(header)
                        .children(mount.toolbar.map(|(bar, _)| bar)),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(top))
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(mount.body),
                )
        }
        .on_drop(cx.listener(move |host, drag: &DraggedPane, window, cx| {
            if drag.tab == tab && drag.pane != pane {
                host.pane_request(ShellPaneRequest::Swap(tab, drag.pane, pane), window, cx);
                cx.stop_propagation()
            }
        }))
        .children(mount.overlays)
        .into_any_element();
        if let Some(regions) = &self.drop_regions {
            measure_drop_region(tab, Some(pane), frame, regions.clone(), |_, _| {})
        } else {
            frame
        }
    }

    fn header<H: PaneHost>(
        &self,
        pane: PaneId,
        content: PaneContent,
        title: String,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let tab = self.tab;
        let tokens = self.tokens;
        let terminal = matches!(content, PaneContent::Terminal(_));
        let action = |icon: LucideIcon, key: &'static str, request: ShellPaneRequest| {
            let label = self.i18n.t(key);
            icon_button(
                &tokens,
                svg()
                    .path(icon.path())
                    .size(px(13.))
                    .text_color(rgb(tokens.ui.text_muted))
                    .into_any_element(),
                IconButtonOptions::opaque_toolbar(22., ButtonRadius::Md),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host, _, window, cx| {
                    host.pane_request(request.clone(), window, cx);
                    cx.stop_propagation()
                }),
            )
            .id(key)
            .flex_none()
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .tooltip(move |_, cx| {
                oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
            })
        };
        let zoomed = self.zoomed == Some(pane);
        div()
            .h(px(28.))
            .flex_none()
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(4.))
            .bg(self.header_background)
            .border_b_1()
            .border_color(rgb(if self.active == Some(pane) {
                tokens.ui.accent
            } else {
                tokens.ui.border
            }))
            .text_size(px(tokens.metrics.ui_text_xs))
            .text_color(rgb(tokens.ui.text_muted))
            .child(
                div()
                    .id(("pane-drag-title", pane.0))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .cursor(CursorStyle::OpenHand)
                    .on_drag(
                        DraggedPane {
                            tab,
                            pane,
                            title: title.clone().into(),
                            tokens,
                        },
                        |drag, _, _, cx| cx.new(|_| drag.clone()),
                    )
                    .child(title),
            )
            .when(terminal, |row| {
                row.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |host, event: &MouseDownEvent, window, cx| {
                        if event.click_count == 2 {
                            host.pane_request(ShellPaneRequest::ToggleZoom(tab, pane), window, cx);
                            cx.stop_propagation()
                        }
                    }),
                )
                .child(format!("#{}", pane.0))
                .child(action(
                    if zoomed {
                        LucideIcon::Layers
                    } else {
                        LucideIcon::Square
                    },
                    if zoomed {
                        "tabbar.restore_pane_layout"
                    } else {
                        "tabbar.zoom_pane"
                    },
                    ShellPaneRequest::ToggleZoom(tab, pane),
                ))
            })
            .child(action(
                LucideIcon::AppWindow,
                "tabbar.move_pane_to_tab",
                ShellPaneRequest::MoveOut(tab, pane, content, false),
            ))
            .when(self.supports_detach, |row| {
                row.child(action(
                    LucideIcon::ExternalLink,
                    "tabbar.move_pane_to_window",
                    ShellPaneRequest::MoveOut(tab, pane, content, true),
                ))
            })
            .child(action(
                LucideIcon::X,
                "command_palette.cmd_close_pane",
                ShellPaneRequest::ClosePane(tab, pane, content),
            ))
            .into_any_element()
    }

    fn render_node<H: PaneHost>(
        &self,
        node: &PaneNode,
        split: bool,
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> AnyElement {
        let tab_id = self.tab;
        match node {
            PaneNode::Leaf {
                pane_id,
                session_id,
            } => self.leaf(
                *pane_id,
                PaneContent::Terminal(*session_id),
                split,
                host,
                window,
                cx,
            ),
            PaneNode::Page { pane_id, tab_id } => self.leaf(
                *pane_id,
                PaneContent::Page(*tab_id),
                split,
                host,
                window,
                cx,
            ),
            PaneNode::Group {
                id,
                direction,
                children,
            } => {
                let sizes = node.split_sizes();
                let extent = Rc::new(Cell::new(1.0_f32));
                let mut group = div()
                    .id(("workspace-pane-group", id.0))
                    .size_full()
                    .flex()
                    .overflow_hidden();
                group = match direction {
                    SplitDirection::Horizontal => group.flex_row(),
                    SplitDirection::Vertical => group.flex_col(),
                };

                for (index, child) in children.iter().enumerate() {
                    let basis = relative(sizes.get(index).copied().unwrap_or(0.0) / 100.0);
                    group = group.child(
                        div()
                            .flex_none()
                            .flex_basis(basis)
                            .flex_shrink_1()
                            .relative()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .bottom_0()
                                    .child(self.render_node(&child.node, split, host, window, cx)),
                            ),
                    );
                    if index + 1 < children.len() {
                        let group_id = *id;
                        let direction = *direction;
                        let start_sizes = sizes.clone();
                        let drag_extent = extent.clone();
                        let active_drag = self.drag.as_ref().is_some_and(|drag| {
                            drag.tab_id == tab_id
                                && drag.group_id == group_id
                                && drag.handle_index == index
                                && drag.direction == direction
                        });
                        let handle_bg = if active_drag {
                            rgba((self.tokens.ui.accent << 8) | SPLIT_HANDLE_ACTIVE_BG_ALPHA)
                        } else {
                            rgba(0x00000000)
                        };
                        let line_color = if active_drag {
                            rgba((self.tokens.ui.accent << 8) | SPLIT_HANDLE_ACTIVE_LINE_ALPHA)
                        } else {
                            self.divider
                        };
                        let line_width = if active_drag {
                            SPLIT_HANDLE_ACTIVE_LINE_WIDTH
                        } else {
                            SPLIT_HANDLE_LINE_WIDTH
                        };
                        let highlighted_line_width = if active_drag {
                            SPLIT_HANDLE_ACTIVE_LINE_WIDTH
                        } else {
                            SPLIT_HANDLE_HOVER_LINE_WIDTH
                        };
                        let highlighted_line_alpha = if active_drag {
                            SPLIT_HANDLE_ACTIVE_LINE_ALPHA
                        } else {
                            SPLIT_HANDLE_HOVER_LINE_ALPHA
                        };
                        let highlighted_line_color =
                            rgba((self.tokens.ui.accent << 8) | highlighted_line_alpha);
                        let split_handle_size = self.tokens.metrics.split_handle_size;
                        let handle_group = SharedString::from(format!(
                            "workspace-split-handle-{}-{index}",
                            group_id.0
                        ));
                        // The full handle remains easy to acquire while only the
                        // centered visual line grows on hover and during drag.
                        let line = div()
                            .absolute()
                            .bg(line_color)
                            .when(direction == SplitDirection::Horizontal, |line| {
                                line.top_0()
                                    .bottom_0()
                                    .left(px((split_handle_size - line_width) / 2.0))
                                    .w(px(line_width))
                            })
                            .when(direction == SplitDirection::Vertical, |line| {
                                line.left_0()
                                    .right_0()
                                    .top(px((split_handle_size - line_width) / 2.0))
                                    .h(px(line_width))
                            })
                            .group_hover(handle_group.clone(), move |style| {
                                let style = style.bg(highlighted_line_color);
                                match direction {
                                    SplitDirection::Horizontal => style
                                        .left(
                                            px((split_handle_size - highlighted_line_width) / 2.0),
                                        )
                                        .w(px(highlighted_line_width)),
                                    SplitDirection::Vertical => style
                                        .top(px((split_handle_size - highlighted_line_width) / 2.0))
                                        .h(px(highlighted_line_width)),
                                }
                            });
                        let mut handle = div()
                            .flex_none()
                            .relative()
                            .group(handle_group)
                            .bg(handle_bg)
                            .hover({
                                let accent = self.tokens.ui.accent;
                                let hover_bg_alpha = if active_drag {
                                    SPLIT_HANDLE_ACTIVE_BG_ALPHA
                                } else {
                                    SPLIT_HANDLE_HOVER_BG_ALPHA
                                };
                                move |style| style.bg(rgba((accent << 8) | hover_bg_alpha))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |host, event: &MouseDownEvent, window, cx| {
                                    let request = if event.click_count >= 2 {
                                        ShellPaneRequest::Reset(tab_id, group_id)
                                    } else {
                                        ShellPaneRequest::StartResize(SplitDrag {
                                            tab_id,
                                            group_id,
                                            handle_index: index,
                                            direction,
                                            start_position: event.position,
                                            start_sizes: start_sizes.clone(),
                                            start_extent: drag_extent.get(),
                                        })
                                    };
                                    host.pane_request(request, window, cx);
                                }),
                            )
                            .child(line);
                        handle = match direction {
                            SplitDirection::Horizontal => handle
                                .w(px(self.tokens.metrics.split_handle_size))
                                .h_full()
                                .cursor(CursorStyle::ResizeColumn),
                            SplitDirection::Vertical => handle
                                .h(px(self.tokens.metrics.split_handle_size))
                                .w_full()
                                .cursor(CursorStyle::ResizeRow),
                        };
                        group = group.child(handle);
                    }
                }

                let direction = *direction;
                let handle_extent =
                    self.tokens.metrics.split_handle_size * children.len().saturating_sub(1) as f32;
                div()
                    .size_full()
                    .child(group)
                    .on_children_prepainted(move |bounds, _, _| {
                        if let Some(bounds) = bounds.first() {
                            extent.set(
                                f32::from(match direction {
                                    SplitDirection::Horizontal => bounds.size.width,
                                    SplitDirection::Vertical => bounds.size.height,
                                }) - handle_extent,
                            );
                        }
                    })
                    .into_any_element()
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitDropEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl SplitDropEdge {
    pub fn placement(self) -> (SplitDirection, bool) {
        match self {
            Self::Left => (SplitDirection::Horizontal, true),
            Self::Right => (SplitDirection::Horizontal, false),
            Self::Top => (SplitDirection::Vertical, true),
            Self::Bottom => (SplitDirection::Vertical, false),
        }
    }
}

#[derive(Clone, Copy)]
pub struct SplitDropRegion {
    pub tab: TabId,
    pub pane: Option<PaneId>,
    pub bounds: Bounds<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitDropTarget {
    pub tab: TabId,
    pub pane: Option<PaneId>,
    pub edge: SplitDropEdge,
    pub bounds: Bounds<Pixels>,
}

pub fn drop_edge(bounds: Bounds<Pixels>, point: Point<Pixels>) -> Option<SplitDropEdge> {
    if !bounds.contains(&point) || bounds.size.width <= px(0.0) || bounds.size.height <= px(0.0) {
        return None;
    }
    let x = f32::from(point.x - bounds.origin.x) / f32::from(bounds.size.width);
    let y = f32::from(point.y - bounds.origin.y) / f32::from(bounds.size.height);
    [
        (x, SplitDropEdge::Left),
        (1.0 - x, SplitDropEdge::Right),
        (y, SplitDropEdge::Top),
        (1.0 - y, SplitDropEdge::Bottom),
    ]
    .into_iter()
    .filter(|(distance, _)| *distance <= 0.25)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, edge)| edge)
}

pub fn preview_bounds(mut bounds: Bounds<Pixels>, edge: SplitDropEdge) -> Bounds<Pixels> {
    match edge {
        SplitDropEdge::Left | SplitDropEdge::Right => {
            bounds.size.width /= 2.0;
            if edge == SplitDropEdge::Right {
                bounds.origin.x += bounds.size.width;
            }
        }
        SplitDropEdge::Top | SplitDropEdge::Bottom => {
            bounds.size.height /= 2.0;
            if edge == SplitDropEdge::Bottom {
                bounds.origin.y += bounds.size.height;
            }
        }
    }
    bounds
}

pub fn measure_drop_region(
    tab: TabId,
    pane: Option<PaneId>,
    content: AnyElement,
    regions: Rc<RefCell<Vec<SplitDropRegion>>>,
    after: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .size_full()
        .child(content)
        .on_children_prepainted(move |bounds, window, cx| {
            if let Some(bounds) = bounds.first() {
                regions.borrow_mut().push(SplitDropRegion {
                    tab,
                    pane,
                    bounds: Bounds::new(bounds.origin + window.bounds().origin, bounds.size),
                });
                after(window, cx);
            }
        })
        .into_any_element()
}

pub fn split_drop_target<A: Eq + std::hash::Hash + Clone, M>(
    layout: &crate::tabs::TabLayout<A, M>,
    regions: &[SplitDropRegion],
    source: TabId,
    point: Point<Pixels>,
) -> Option<SplitDropTarget> {
    regions.iter().find_map(|region| {
        if !layout.can_receive_tab_drop(source, region.tab) {
            return None;
        }
        let tab = layout.tab_by_id(region.tab)?;
        if region.pane.is_none() && tab.root_pane.is_some() {
            return None;
        }
        if region.pane.is_some_and(|pane| {
            tab.root_pane
                .as_ref()
                .is_none_or(|root| !root.contains_pane(pane))
        }) {
            return None;
        }
        Some(SplitDropTarget {
            tab: region.tab,
            pane: region.pane,
            edge: drop_edge(region.bounds, point)?,
            bounds: region.bounds,
        })
    })
}

impl SplitDropTarget {
    pub fn preview(self, window: &Window, tokens: &ThemeTokens) -> AnyElement {
        let mut bounds = preview_bounds(self.bounds, self.edge);
        bounds.origin -= window.bounds().origin;
        div()
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .bg(rgba((tokens.ui.accent << 8) | 0x30))
            .border_2()
            .border_color(rgb(tokens.ui.accent))
            .rounded(px(tokens.radii.md))
            .into_any_element()
    }
}
