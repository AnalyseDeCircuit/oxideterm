use crate::{
    layout::{MarkdownBlockLayout, MarkdownLayoutItem},
    model::SourceSpan,
};
use gpui::{
    AnyElement, App, IntoElement, ParentElement, ScrollHandle, Styled, Window, div, point, px,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SourceAnchor {
    pub version: u64,
    pub position: f64,
    pub viewport_y: f32,
    pub at_start: bool,
    pub at_end: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Geometry {
    span: SourceSpan,
    top: f32,
    height: f32,
    collapsed: bool,
}

type ScrollCallback = Rc<dyn Fn(SourceAnchor, &mut Window, &mut App)>;

#[derive(Default)]
struct State {
    layout_id: usize,
    version: u64,
    sequence: u64,
    geometry: Vec<Geometry>,
    previous_geometry: Vec<Geometry>,
    anchor: Option<SourceAnchor>,
    pending: bool,
    user_input: bool,
    last_offset: Option<f32>,
    callback: Option<ScrollCallback>,
}

#[derive(Clone, Default)]
pub struct MarkdownScrollSync(Rc<RefCell<State>>);

impl std::fmt::Debug for MarkdownScrollSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MarkdownScrollSync")
    }
}

impl MarkdownScrollSync {
    pub fn set_callback(&self, callback: impl Fn(SourceAnchor, &mut Window, &mut App) + 'static) {
        self.0.borrow_mut().callback = Some(Rc::new(callback));
    }

    pub fn set_version(&self, version: u64) {
        let mut state = self.0.borrow_mut();
        if state.version != version {
            state.version = version;
            state.sequence += 1;
            state.geometry.clear();
            state.anchor = None;
            state.pending = false;
        }
    }

    pub fn request(&self, anchor: SourceAnchor) {
        let mut state = self.0.borrow_mut();
        if state.version != anchor.version {
            return;
        }
        state.sequence += 1;
        state.anchor = Some(anchor);
        state.pending = true;
        state.user_input = false;
    }

    pub fn user_input(&self) {
        let mut state = self.0.borrow_mut();
        state.sequence += 1;
        state.pending = false;
        state.user_input = true;
    }

    pub fn anchor(&self) -> Option<SourceAnchor> {
        self.0.borrow().anchor
    }

    pub(crate) fn begin(
        &self,
        layout: &MarkdownBlockLayout,
        gap: f32,
        scroll: &ScrollHandle,
    ) -> f32 {
        let mut state = self.0.borrow_mut();
        let layout_id = layout.measurement_id();
        if state.layout_id != layout_id && state.anchor.is_some() && !state.user_input {
            state.pending = true;
        }
        state.layout_id = layout_id;
        state.sequence += 1;
        state.geometry.clear();
        let mut top = 0.0;
        let mut target = None;
        if state.pending
            && let Some(anchor) = state.anchor
        {
            let mut distance = f64::INFINITY;
            for (item, size) in layout.items().iter().zip(layout.item_sizes().iter()) {
                let span = match item {
                    MarkdownLayoutItem::Block(block) => block.source_span(),
                    MarkdownLayoutItem::Footnotes(notes) => notes
                        .iter()
                        .flat_map(|note| &note.blocks)
                        .filter_map(|block| block.source_span())
                        .find(|span| contains(*span, anchor.position)),
                };
                if let Some(span) = span {
                    let next_distance = if contains(span, anchor.position) {
                        0.0
                    } else {
                        (anchor.position - span.start as f64)
                            .abs()
                            .min((anchor.position - span.end as f64).abs())
                    };
                    if next_distance < distance {
                        distance = next_distance;
                        let fraction = ((anchor.position - span.start as f64)
                            / span.end.saturating_sub(span.start).max(1) as f64)
                            .clamp(0.0, 1.0);
                        target = Some(
                            top + fraction as f32 * f32::from(size.height) - anchor.viewport_y,
                        );
                    }
                }
                top += f32::from(size.height) + gap;
            }
        }
        if let Some(target) = target {
            let viewport = f32::from(scroll.bounds().size.height);
            let maximum = (top - gap - viewport).max(0.0);
            let anchor = state.anchor.unwrap();
            let target = if anchor.at_start {
                0.0
            } else if anchor.at_end {
                maximum
            } else {
                target.clamp(0.0, maximum)
            };
            // The visible range and its translation must change in the same frame.
            // Measuring new rows while keeping the old offset paints only spacers.
            scroll.set_offset(point(scroll.offset().x, px(-target)));
            state.last_offset = Some(target);
            target
        } else {
            -f32::from(scroll.offset().y)
        }
    }

    pub(crate) fn wrap(&self, span: SourceSpan, element: AnyElement) -> AnyElement {
        self.wrap_visibility(span, element, false)
    }

    pub(crate) fn wrap_visibility(
        &self,
        span: SourceSpan,
        element: AnyElement,
        collapsed: bool,
    ) -> AnyElement {
        let sync = self.clone();
        div()
            .relative()
            .w_full()
            .min_w_0()
            .child(element)
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        if bounds.size.height > px(0.0) {
                            sync.0.borrow_mut().geometry.push(Geometry {
                                span,
                                top: f32::from(bounds.origin.y),
                                height: f32::from(bounds.size.height),
                                collapsed,
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    pub(crate) fn finish_probe(&self, scroll: &ScrollHandle) -> AnyElement {
        let sync = self.clone();
        let scroll = scroll.clone();
        gpui::canvas(
            move |_, window, cx| {
                let sequence = sync.0.borrow().sequence;
                let sync = sync.clone();
                let scroll = scroll.clone();
                window.defer(cx, move |window, cx| {
                    sync.finish(sequence, &scroll, window, cx)
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }

    fn finish(&self, sequence: u64, scroll: &ScrollHandle, window: &mut Window, cx: &mut App) {
        let mut state = self.0.borrow_mut();
        if state.sequence != sequence {
            return;
        }
        let offset = -f32::from(scroll.offset().y);
        let viewport = f32::from(scroll.bounds().origin.y);
        let maximum = f32::from(scroll.max_offset().y);
        let geometry = state
            .geometry
            .iter()
            .map(|g| Geometry {
                top: g.top - viewport + offset,
                ..*g
            })
            .collect::<Vec<_>>();
        let layout_changed = state.previous_geometry != geometry;
        state.previous_geometry = geometry.clone();
        let moved = state
            .last_offset
            .is_some_and(|last| (last - offset).abs() > 0.5);
        if state.user_input || (moved && !state.pending && !layout_changed) {
            state.user_input = false;
            state.pending = false;
            if let Some(position) = position_at_y(&geometry, offset) {
                let anchor = SourceAnchor {
                    version: state.version,
                    position,
                    viewport_y: 0.0,
                    at_start: offset <= 0.5,
                    at_end: maximum > 0.5 && offset >= maximum - 0.5,
                };
                state.anchor = Some(anchor);
                state.last_offset = Some(offset);
                let callback = state.callback.clone();
                drop(state);
                if moved && let Some(callback) = callback {
                    callback(anchor, window, cx);
                }
                return;
            }
        }
        if let Some(anchor) = state.anchor {
            let target = if anchor.at_start {
                Some(0.0)
            } else if anchor.at_end {
                Some(maximum)
            } else {
                y_at_position(&geometry, anchor.position).map(|y| y - anchor.viewport_y)
            };
            if let Some(target) = target {
                let target = target.clamp(0.0, maximum);
                state.pending = false;
                state.last_offset = Some(target);
                if (target - offset).abs() > 0.5 {
                    scroll.set_offset(point(scroll.offset().x, px(-target)));
                    window.refresh();
                }
                return;
            }
        }
        state.last_offset = Some(offset);
    }
}

fn contains(span: SourceSpan, position: f64) -> bool {
    position >= span.start as f64 && position <= span.end as f64
}

fn y_at_position(geometry: &[Geometry], position: f64) -> Option<f32> {
    let item = geometry
        .iter()
        .filter(|g| contains(g.span, position))
        .min_by(|a, b| {
            a.span
                .end
                .saturating_sub(a.span.start)
                .cmp(&b.span.end.saturating_sub(b.span.start))
                .then(a.height.total_cmp(&b.height))
        });
    let Some(item) = item else {
        let before = geometry
            .iter()
            .filter(|g| (g.span.end as f64) < position)
            .max_by_key(|g| g.span.end);
        let after = geometry
            .iter()
            .filter(|g| g.span.start as f64 > position)
            .min_by_key(|g| g.span.start);
        return match (before, after) {
            (Some(a), Some(b)) if b.top >= a.top + a.height => {
                let fraction = ((position - a.span.end as f64)
                    / (b.span.start - a.span.end).max(1) as f64)
                    .clamp(0.0, 1.0);
                Some(a.top + a.height + fraction as f32 * (b.top - a.top - a.height))
            }
            (Some(a), _) => Some(a.top + a.height),
            (_, Some(b)) => Some(b.top),
            _ => None,
        };
    };
    if item.collapsed {
        return Some(item.top);
    }
    let progress = ((position - item.span.start as f64)
        / item.span.end.saturating_sub(item.span.start).max(1) as f64)
        .clamp(0.0, 1.0);
    Some(item.top + progress as f32 * item.height)
}

fn position_at_y(geometry: &[Geometry], y: f32) -> Option<f64> {
    if let Some(item) = geometry
        .iter()
        .filter(|g| y >= g.top && y <= g.top + g.height)
        .min_by(|a, b| a.height.total_cmp(&b.height))
    {
        if item.collapsed {
            return Some(item.span.start as f64);
        }
        let progress = ((y - item.top) / item.height.max(1.0)).clamp(0.0, 1.0);
        return Some(
            item.span.start as f64
                + progress as f64 * item.span.end.saturating_sub(item.span.start) as f64,
        );
    }
    let before = geometry
        .iter()
        .filter(|g| g.top + g.height < y)
        .max_by(|a, b| (a.top + a.height).total_cmp(&(b.top + b.height)));
    let after = geometry
        .iter()
        .filter(|g| g.top > y)
        .min_by(|a, b| a.top.total_cmp(&b.top));
    match (before, after) {
        (Some(a), Some(b)) if a.span.end <= b.span.start => {
            let fraction =
                ((y - a.top - a.height) / (b.top - a.top - a.height).max(1.0)).clamp(0.0, 1.0);
            Some(a.span.end as f64 + fraction as f64 * (b.span.start - a.span.end) as f64)
        }
        (Some(a), _) => Some(a.span.end as f64),
        (_, Some(b)) => Some(b.span.start as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render};

    struct PreviewFixture {
        document: crate::MarkdownDocument,
        handle: crate::MarkdownVirtualListScrollHandle,
        options: crate::MarkdownOptions,
    }

    impl Render for PreviewFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            crate::render::render_document_virtual(
                "fixture",
                &self.document,
                &oxideterm_theme::default_tokens(),
                &self.options,
                &self.handle,
            )
        }
    }

    #[gpui::test]
    fn distant_scroll_paints_target_rows_before_deferred_correction(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| {
            let handle = crate::MarkdownVirtualListScrollHandle::new();
            handle.scroll_sync.set_version(1);
            PreviewFixture {
                document: crate::parser::parse_with_source_ranges(
                    &(0..100)
                        .map(|n| format!("paragraph {n}\n\n"))
                        .collect::<String>(),
                ),
                options: crate::MarkdownOptions {
                    scroll_sync: Some(handle.scroll_sync.clone()),
                    ..Default::default()
                },
                handle,
            }
        });
        cx.simulate_resize(gpui::size(px(480.0), px(180.0)));
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        for index in [80, 5, 95] {
            view.update(cx, |view, cx| {
                let position = view.document.blocks[index].source_span().unwrap().start as f64;
                view.handle.scroll_sync.request(SourceAnchor {
                    version: 1,
                    position,
                    ..Default::default()
                });
                cx.notify();
            });
            cx.update(|window, app| {
                window.draw(app).clear(app);
                view.read_with(app, |view, _| {
                    let span = view.document.blocks[index].source_span().unwrap();
                    let viewport = view.handle.bounds();
                    let state = view.handle.scroll_sync.0.borrow();
                    let target = state
                        .geometry
                        .iter()
                        .find(|g| g.span == span)
                        .expect("target block was laid out");
                    assert!(
                        target.top < f32::from(viewport.bottom())
                            && target.top + target.height > f32::from(viewport.top()),
                        "target {index} must be visible in the first paint"
                    );
                });
            });
            cx.run_until_parked();
        }
        let notifications = Rc::new(RefCell::new(Vec::new()));
        let captured = notifications.clone();
        view.update(cx, |view, cx| {
            view.handle
                .scroll_sync
                .set_callback(move |anchor, _, _| captured.borrow_mut().push(anchor));
            view.options.base_font_size = 20.0;
            cx.notify();
        });
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.handle.scroll_sync.anchor().unwrap().position,
                view.document.blocks[95].source_span().unwrap().start as f64
            );
        });
        assert!(
            notifications.borrow().is_empty(),
            "layout correction must not drive the editor back"
        );
        view.update(cx, |view, cx| {
            view.handle.scroll_sync.user_input();
            view.handle.set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        });
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        cx.run_until_parked();
        assert_eq!(
            *notifications.borrow(),
            vec![SourceAnchor {
                version: 1,
                position: 0.0,
                at_start: true,
                ..Default::default()
            }]
        );
    }

    #[test]
    fn block_and_gap_interpolation_share_endpoints() {
        let geometry = [
            Geometry {
                span: SourceSpan { start: 10, end: 30 },
                top: 100.0,
                height: 80.0,
                collapsed: false,
            },
            Geometry {
                span: SourceSpan { start: 40, end: 60 },
                top: 220.0,
                height: 100.0,
                collapsed: false,
            },
        ];
        for (source, y) in [
            (10.0, 100.0),
            (20.0, 140.0),
            (30.0, 180.0),
            (35.0, 200.0),
            (40.0, 220.0),
            (50.0, 270.0),
            (60.0, 320.0),
        ] {
            assert_eq!(y_at_position(&geometry, source), Some(y));
            assert_eq!(position_at_y(&geometry, y), Some(source));
        }
    }

    #[test]
    fn far_source_request_selects_an_unmeasured_block_and_rejects_old_versions() {
        let source = (0..100)
            .map(|n| format!("paragraph {n}\n\n"))
            .collect::<String>();
        let doc = crate::parser::parse_with_source_ranges(&source);
        let opts = crate::MarkdownOptions::default();
        let layout = MarkdownBlockLayout::from_document(&doc, &opts);
        let sync = MarkdownScrollSync::default();
        sync.set_version(7);
        let source = doc.blocks[80].source_span().unwrap().start;
        sync.request(SourceAnchor {
            version: 7,
            position: source as f64,
            ..Default::default()
        });
        let scroll = ScrollHandle::new();
        let target = sync.begin(&layout, opts.block_gap, &scroll);
        assert_eq!(target, 80.0 * (22.0 + 16.0));
        assert_eq!(scroll.offset().y, px(-3040.0));
        let near_source = doc.blocks[5].source_span().unwrap().start;
        sync.request(SourceAnchor {
            version: 7,
            position: near_source as f64,
            ..Default::default()
        });
        assert_eq!(sync.begin(&layout, opts.block_gap, &scroll), 190.0);
        assert_eq!(scroll.offset().y, px(-190.0));
        sync.request(SourceAnchor {
            version: 6,
            position: 0.0,
            ..Default::default()
        });
        assert_eq!(sync.anchor().unwrap().position, near_source as f64);
        sync.user_input();
        assert!(!sync.0.borrow().pending);
    }
}
