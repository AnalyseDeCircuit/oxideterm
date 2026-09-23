use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, Context, CursorStyle, MouseButton, MouseMoveEvent, Pixels, Window, div, prelude::*,
    px, rgb,
};

use super::super::{WorkspaceApp, settings_ui_font_family};

const DEFAULT_HEIGHT: f32 = 260.0;
const MIN_HEIGHT: f32 = 160.0;
const MIN_TERMINAL_HEIGHT: f32 = 180.0;
const RESIZE_HOTZONE: f32 = 6.0;

pub(in crate::workspace) struct QuickCommandsPanelState {
    // Layout measurements exclude the toolbar, quick bar, and sender input.
    pub(in crate::workspace) available_height: Rc<Cell<f32>>,
    height: f32,
    drag: Option<(Pixels, f32)>,
}

impl Default for QuickCommandsPanelState {
    fn default() -> Self {
        Self {
            available_height: Rc::new(Cell::new(0.0)),
            height: DEFAULT_HEIGHT,
            drag: None,
        }
    }
}

impl QuickCommandsPanelState {
    fn clamp_height(&self, height: f32) -> f32 {
        let available = self.available_height.get();
        let maximum = (available - MIN_TERMINAL_HEIGHT)
            .max(0.0)
            .min(available * 0.5);
        height.clamp(MIN_HEIGHT.min(maximum), maximum)
    }

    fn height(&self) -> f32 {
        self.clamp_height(self.height)
    }

    pub(in crate::workspace) fn is_resizing(&self) -> bool {
        self.drag.is_some()
    }

    pub(in crate::workspace) fn finish_resize(&mut self) -> bool {
        self.drag.take().is_some()
    }
}

fn panel_frame(
    tokens: &oxideterm_theme::ThemeTokens,
    height: f32,
    sidebar: AnyElement,
    body: AnyElement,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id("terminal-quick-commands-panel")
        .relative()
        .w_full()
        .h(px(height))
        .flex_none()
        .min_h_0()
        .flex()
        .overflow_hidden()
        .border_t_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgb(tokens.ui.bg))
        .child(sidebar)
        .child(body)
}

impl WorkspaceApp {
    pub(in crate::workspace) fn toggle_terminal_quick_commands_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .terminal
            .update(cx, |terminal, _| terminal.quick_commands.toggle_open());
        if open {
            self.prepare_terminal_quick_commands_panel(window, cx);
        } else {
            self.ime_marked_text = None;
            self.clear_ime_selection();
            self.focus_active_pane(window, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn prepare_terminal_quick_commands_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The sender retains its drafts and running jobs while its expanded surface closes.
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.set_expanded(false, cx);
            sender.set_compact_focused(false, cx);
            sender.dismiss_compact_suggestions();
        });
        self.close_terminal_command_overlays(cx);
        self.ime_marked_text = None;
        self.clear_ime_selection();
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn render_terminal_quick_commands_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let commands = &self.terminal.read(cx).quick_commands;
        let open = commands.is_open()
            && self
                .settings_store
                .settings()
                .terminal
                .command_bar
                .quick_commands_enabled;
        let height = commands.panel.height();
        let resizing = commands.panel.is_resizing();
        let mut tokens = self.tokens;
        if resizing {
            tokens.motion.spatial_enabled = false;
        }
        let snapshot = open.then(|| self.quick_commands_render_snapshot(cx));
        let content = snapshot.map(|snapshot| {
            panel_frame(
                &tokens,
                height,
                self.render_quick_command_category_sidebar(&snapshot, cx),
                self.render_quick_command_body(&snapshot, cx),
            )
            .text_size(px(tokens.metrics.ui_text_sm))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("quick-commands-resize")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(RESIZE_HOTZONE))
                    .cursor(CursorStyle::ResizeRow)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            this.terminal.update(cx, |terminal, _| {
                                let panel = &mut terminal.quick_commands.panel;
                                if event.click_count >= 2 {
                                    panel.height = DEFAULT_HEIGHT;
                                    panel.finish_resize();
                                } else {
                                    panel.drag = Some((event.position.y, panel.height()));
                                }
                            });
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
        });
        oxideterm_gpui_ui::motion::auto_height(&tokens, "quick-commands-panel-height", content)
            .target_height(if open { height } else { 0.0 })
            .into_any_element()
    }

    pub(in crate::workspace) fn update_terminal_quick_commands_resize(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let changed = self.terminal.update(cx, |terminal, _| {
            let panel = &mut terminal.quick_commands.panel;
            let Some((start_y, start_height)) = panel.drag else {
                return false;
            };
            if !event.dragging() {
                return panel.finish_resize();
            }
            let height = panel.clamp_height(start_height - f32::from(event.position.y - start_y));
            let changed = (height - panel.height).abs() > 0.1;
            panel.height = height;
            changed
        });
        if changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn finish_terminal_quick_commands_resize(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.update(cx, |terminal, _| {
            terminal.quick_commands.panel.finish_resize()
        }) {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Render, ScrollHandle, TestAppContext};

    struct DockFixture {
        panel: QuickCommandsPanelState,
        scroll: ScrollHandle,
        open: bool,
    }

    impl Render for DockFixture {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let mut tokens = oxideterm_theme::default_tokens();
            tokens.motion.enabled = false;
            self.panel
                .available_height
                .set(f32::from(window.viewport_size().height) - 64.0);
            let height = self.panel.height();
            let sidebar = div().w(px(160.0)).flex_none().h_full().into_any_element();
            let body = div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(49.0))
                        .flex_none()
                        .debug_selector(|| "search".into()),
                )
                .child(
                    div()
                        .id("rows")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .children((0..20).map(|index| {
                            div()
                                .h(px(48.0))
                                .flex_none()
                                .debug_selector(move || format!("row-{index}"))
                        })),
                )
                .into_any_element();
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .debug_selector(|| "terminal".into()),
                )
                .child(
                    div()
                        .h(px(32.0))
                        .flex_none()
                        .debug_selector(|| "toolbar".into()),
                )
                .child(
                    oxideterm_gpui_ui::motion::auto_height(
                        &tokens,
                        "dock",
                        Some(
                            panel_frame(&tokens, height, sidebar, body)
                                .debug_selector(|| "dock".into())
                                .into_any_element(),
                        ),
                    )
                    .target_height(if self.open { height } else { 0.0 }),
                )
                .child(
                    div()
                        .h(px(32.0))
                        .flex_none()
                        .debug_selector(|| "input".into()),
                )
        }
    }

    #[gpui::test]
    fn dock_occupies_space_and_scrolls_within_small_window_bounds(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| DockFixture {
            panel: QuickCommandsPanelState::default(),
            scroll: ScrollHandle::new(),
            open: true,
        });
        for (width, height, expected_dock_height) in [(900.0, 700.0, 260.0), (480.0, 400.0, 156.0)]
        {
            cx.simulate_resize(gpui::size(px(width), px(height)));
            cx.update(|window, app| window.draw(app).clear(app));
            let terminal = cx.debug_bounds("terminal").unwrap();
            let toolbar = cx.debug_bounds("toolbar").unwrap();
            let dock = cx.debug_bounds("dock").unwrap();
            let input = cx.debug_bounds("input").unwrap();
            assert_eq!(dock.size, gpui::size(px(width), px(expected_dock_height)));
            assert_eq!(terminal.bottom(), toolbar.top());
            assert_eq!(toolbar.bottom(), dock.top());
            assert_eq!(dock.bottom(), input.top());
            assert_eq!(input.bottom(), px(height));
            assert!(terminal.size.height >= px(MIN_TERMINAL_HEIGHT));
            let scroll = view.read_with(cx, |view, _| view.scroll.clone());
            let search = cx.debug_bounds("search").unwrap();
            scroll.set_offset(gpui::point(px(0.0), -scroll.max_offset().y));
            cx.update(|window, app| {
                window.refresh();
                window.draw(app).clear(app);
            });
            assert_eq!(cx.debug_bounds("search").unwrap(), search);
            let last = cx.debug_bounds("row-19").unwrap();
            assert!(last.top() >= scroll.bounds().top());
            assert!((last.bottom() - scroll.bounds().bottom()).abs() < px(1.0));
        }
        view.update(cx, |view, cx| {
            view.open = false;
            cx.notify();
        });
        cx.update(|window, app| window.draw(app).clear(app));
        assert_eq!(cx.debug_bounds("terminal").unwrap().size.height, px(336.0));
        assert_eq!(
            cx.debug_bounds("toolbar").unwrap().bottom(),
            cx.debug_bounds("input").unwrap().top()
        );
    }
}
