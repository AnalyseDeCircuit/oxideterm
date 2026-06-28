use oxideterm_gpui_ui::context_menu::{
    ContextMenuItemKind, context_menu_content, context_menu_event_boundary, context_menu_item,
    context_menu_separator,
};
use oxideterm_gpui_ui::modal::overlay_content_boundary;

const TAB_CONTEXT_MENU_WIDTH: f32 = 228.0;
const TAB_CONTEXT_MENU_HEIGHT: f32 = 136.0;
const TAB_CONTEXT_MENU_MARGIN: f32 = 8.0;

impl WorkspaceApp {
    pub(super) fn update_main_window_tabbar_drop_bounds(
        &mut self,
        window: &Window,
        titlebar_visible: bool,
        zen_mode: bool,
    ) {
        if zen_mode {
            self.main_window_tabbar_drop_bounds = None;
            return;
        }

        let window_bounds = window.bounds();
        let titlebar_height = if titlebar_visible {
            self.tokens.metrics.titlebar_height
        } else {
            0.0
        };
        let left_offset = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };
        let right_offset = if self.context_sidebar_visible() {
            self.ai_sidebar_width
        } else {
            0.0
        };
        let width = (f32::from(window_bounds.size.width) - left_offset - right_offset).max(0.0);
        self.main_window_tabbar_drop_bounds = Some(Bounds::new(
            gpui::point(
                window_bounds.origin.x + px(left_offset),
                window_bounds.origin.y + px(titlebar_height),
            ),
            gpui::size(px(width), px(self.tokens.metrics.tabbar_height)),
        ));
    }

    pub(super) fn open_tab_context_menu(
        &mut self,
        tab_id: TabId,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.main_window_tabs.context_menu = Some(TabContextMenu {
            tab_id,
            x: f32::from(event.position.x),
            y: f32::from(event.position.y),
        });
        cx.notify();
    }

    pub(super) fn close_tab_context_menu(&mut self) -> bool {
        self.main_window_tabs.context_menu.take().is_some()
    }

    pub(super) fn detach_tab_to_window(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.detached_tabs.contains(&tab_id) || self.tab_by_id(tab_id).is_none() {
            return;
        }

        self.detached_tabs.insert(tab_id);
        self.activate_nearest_visible_main_tab(tab_id, window, cx);

        let workspace = cx.weak_entity();
        let bounds = window.bounds();
        let open_result = cx.open_window(
            oxideterm_gpui_platform::window_options(bounds),
            move |detached_window, cx| {
                cx.new(|cx| {
                    super::detached_tab_window::DetachedTabWindow::new(
                        workspace,
                        tab_id,
                        detached_window,
                        cx,
                    )
                })
            },
        );

        if open_result.is_err() {
            self.detached_tabs.remove(&tab_id);
            self.main_window_tabs.active_tab_id = Some(tab_id);
        }
        self.sync_active_tab_surface();
        cx.notify();
    }

    pub(super) fn return_detached_tab_to_main(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        if self.detached_tabs.remove(&tab_id) {
            self.main_window_tabs.active_tab_id = Some(tab_id);
            self.detached_tab_return_drag = None;
            self.sync_active_tab_surface();
            cx.notify();
        }
    }

    fn detached_window_screen_point(
        window: &Window,
        window_point: Point<Pixels>,
    ) -> Point<Pixels> {
        let window_bounds = window.bounds();
        gpui::point(
            window_bounds.origin.x + window_point.x,
            window_bounds.origin.y + window_point.y,
        )
    }

    pub(super) fn start_detached_tab_return_drag(
        &mut self,
        tab_id: TabId,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let screen_point = Self::detached_window_screen_point(window, event.position);
        self.detached_tab_return_drag = Some(DetachedTabReturnDrag {
            tab_id,
            start_screen_x: f32::from(screen_point.x),
            start_screen_y: f32::from(screen_point.y),
            current_screen_x: f32::from(screen_point.x),
            current_screen_y: f32::from(screen_point.y),
            active: false,
        });
        cx.notify();
    }

    pub(super) fn update_detached_tab_return_drag(
        &mut self,
        tab_id: TabId,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let screen_point = Self::detached_window_screen_point(window, event.position);
        let Some(mut drag) = self.detached_tab_return_drag else {
            return;
        };
        if drag.tab_id != tab_id {
            return;
        }

        let was_active = drag.active;
        drag.current_screen_x = f32::from(screen_point.x);
        drag.current_screen_y = f32::from(screen_point.y);
        let delta_x = drag.current_screen_x - drag.start_screen_x;
        let delta_y = drag.current_screen_y - drag.start_screen_y;
        // Treat this as a tab-return gesture only after a real window drag,
        // so ordinary titlebar clicks do not accidentally dock the tab.
        drag.active = delta_x.hypot(delta_y) > TAB_DRAG_THRESHOLD_PX;
        self.detached_tab_return_drag = Some(drag);
        if drag.active != was_active {
            cx.notify();
        }
    }

    pub(super) fn finish_detached_tab_return_drag(
        &mut self,
        tab_id: TabId,
        event: &MouseUpEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let screen_point = Self::detached_window_screen_point(window, event.position);
        let Some(drag) = self.detached_tab_return_drag.take() else {
            return false;
        };
        if drag.tab_id != tab_id || !drag.active {
            cx.notify();
            return false;
        }
        let should_return = self
            .main_window_tabbar_drop_bounds
            .as_ref()
            .is_some_and(|bounds| bounds.contains(&screen_point));
        if should_return {
            self.return_detached_tab_to_main(tab_id, cx);
            true
        } else {
            cx.notify();
            false
        }
    }

    fn detached_tab_return_drag_screen_point(&self) -> Option<Point<Pixels>> {
        let drag = self.detached_tab_return_drag?;
        drag.active.then(|| {
            gpui::point(
                px(drag.current_screen_x),
                px(drag.current_screen_y),
            )
        })
    }

    pub(super) fn render_detached_tab_return_drop_hint(
        &self,
        window: &Window,
    ) -> Option<AnyElement> {
        let drag = self.detached_tab_return_drag?;
        let screen_point = self.detached_tab_return_drag_screen_point()?;
        let drop_bounds = self.main_window_tabbar_drop_bounds.as_ref()?;
        if !drop_bounds.contains(&screen_point) {
            return None;
        }

        let tab_title = self
            .tab_by_id(drag.tab_id)
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        let window_bounds = window.bounds();
        let local_left = f32::from(drop_bounds.origin.x - window_bounds.origin.x);
        let local_top = f32::from(drop_bounds.origin.y - window_bounds.origin.y);
        let theme = self.tokens.ui;
        let accent = theme.accent;

        let hint = div()
            .absolute()
            .left(px(local_left))
            .top(px(local_top))
            .w(drop_bounds.size.width)
            .h(drop_bounds.size.height)
            .flex()
            .items_center()
            .px(px(12.0))
            .bg(rgba((accent << 8) | 0x22))
            .border_1()
            .border_color(rgba((accent << 8) | 0x99))
            .child(
                div()
                    .h(px((self.tokens.metrics.tabbar_height - 8.0).max(24.0)))
                    .max_w(px(self.tokens.metrics.tab_max_width + 96.0))
                    .px(px(12.0))
                    .rounded(px(999.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .bg(rgb(theme.bg_panel))
                    .border_1()
                    .border_color(rgb(accent))
                    .shadow_lg()
                    .child(Self::render_lucide_icon(
                        LucideIcon::PanelLeft,
                        15.0,
                        rgb(accent),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .text_color(rgb(theme.text))
                            .child(tab_title),
                    )
                    .child(
                        div()
                            .text_size(px((self.tokens.metrics.tab_font_size - 1.0).max(11.0)))
                            .text_color(rgb(accent))
                            .child(self.i18n.t("tabbar.return_to_main_window")),
                    ),
            )
            .with_animation(
                ("detached-tab-return-drop-hint", drag.tab_id.0),
                Animation::new(Duration::from_millis(840)).repeat(),
                |hint, delta| {
                    let pulse = if delta < 0.5 {
                        delta * 2.0
                    } else {
                        (1.0 - delta) * 2.0
                    };
                    hint.opacity(0.74 + pulse * 0.2)
                },
            );

        Some(hint.into_any_element())
    }

    pub(super) fn render_tab_detach_drag_preview(&self, window: &Window) -> Option<AnyElement> {
        let drag = self.main_window_tabs.drag.as_ref()?;
        if !drag.active || drag.mode != TabDragMode::Detach {
            return None;
        }

        let tab_title = self
            .tab_by_id(drag.tab_id)
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        let theme = self.tokens.ui;
        let accent = theme.accent;
        let preview_width = (drag
            .tab_widths
            .get(drag.from_index)
            .copied()
            .unwrap_or(self.tokens.metrics.tab_max_width)
            + 96.0)
            .clamp(220.0, 360.0);
        let viewport_width = f32::from(window.viewport_size().width);
        let left = (drag.current_x - preview_width * 0.5)
            .clamp(8.0, (viewport_width - preview_width - 8.0).max(8.0));
        let top = (drag.current_y + 14.0)
            .max(self.tokens.metrics.titlebar_height + self.tokens.metrics.tabbar_height + 8.0);

        // The preview appears only after the drag is classified as a detach,
        // leaving ordinary horizontal tab reordering visually unchanged.
        let preview = div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(preview_width))
            .min_h(px(48.0))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(16.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(rgb(theme.bg_panel))
            .border_1()
            .border_color(rgba((accent << 8) | 0xaa))
            .shadow_lg()
            .child(Self::render_lucide_icon(
                LucideIcon::ExternalLink,
                16.0,
                rgb(accent),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text))
                            .child(tab_title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px((self.tokens.metrics.tab_font_size - 1.0).max(11.0)))
                            .line_height(px(16.0))
                            .text_color(rgb(accent))
                            .child(self.i18n.t("tabbar.detach_to_window")),
                    ),
            )
            .with_animation(
                ("tab-detach-drag-preview", drag.tab_id.0),
                Animation::new(Duration::from_millis(760)).repeat(),
                |preview, delta| {
                    let pulse = if delta < 0.5 {
                        delta * 2.0
                    } else {
                        (1.0 - delta) * 2.0
                    };
                    preview.opacity(0.82 + pulse * 0.16)
                },
            );

        Some(preview.into_any_element())
    }

    fn render_detached_tab_return_drag_preview(
        &self,
        tab_id: TabId,
        window: &Window,
    ) -> Option<AnyElement> {
        let drag = self.detached_tab_return_drag?;
        if drag.tab_id != tab_id || !drag.active {
            return None;
        }

        let tab_title = self
            .tab_by_id(drag.tab_id)
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        let theme = self.tokens.ui;
        let accent = theme.accent;
        let viewport = window.viewport_size();
        let window_bounds = window.bounds();
        let local_x = drag.current_screen_x - f32::from(window_bounds.origin.x);
        let local_y = drag.current_screen_y - f32::from(window_bounds.origin.y);
        let preview_width = (self.tokens.metrics.tab_max_width + 96.0).clamp(220.0, 360.0);
        let left = (local_x - preview_width * 0.5)
            .clamp(8.0, (f32::from(viewport.width) - preview_width - 8.0).max(8.0));
        let top = (local_y + 14.0)
            .clamp(8.0, (f32::from(viewport.height) - 64.0).max(8.0));

        // Return drags originate in the detached window, so this preview is
        // rendered there while the main window separately renders the drop zone.
        let preview = div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(preview_width))
            .min_h(px(48.0))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(16.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(rgb(theme.bg_panel))
            .border_1()
            .border_color(rgba((accent << 8) | 0xaa))
            .shadow_lg()
            .child(Self::render_lucide_icon(
                LucideIcon::PanelLeft,
                16.0,
                rgb(accent),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.tab_font_size))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text))
                            .child(tab_title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px((self.tokens.metrics.tab_font_size - 1.0).max(11.0)))
                            .line_height(px(16.0))
                            .text_color(rgb(accent))
                            .child(self.i18n.t("tabbar.return_to_main_window")),
                    ),
            )
            .with_animation(
                ("detached-tab-return-drag-preview", drag.tab_id.0),
                Animation::new(Duration::from_millis(760)).repeat(),
                |preview, delta| {
                    let pulse = if delta < 0.5 {
                        delta * 2.0
                    } else {
                        (1.0 - delta) * 2.0
                    };
                    preview.opacity(0.82 + pulse * 0.16)
                },
            );

        Some(preview.into_any_element())
    }

    pub(super) fn render_tab_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.main_window_tabs.context_menu?;
        if self.tab_by_id(menu.tab_id).is_none() {
            return None;
        }
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            TAB_CONTEXT_MENU_WIDTH,
            TAB_CONTEXT_MENU_HEIGHT,
            TAB_CONTEXT_MENU_MARGIN,
        );
        let detached = self.detached_tabs.contains(&menu.tab_id);
        let menu_body = context_menu_event_boundary(
            context_menu_content(&self.tokens)
                .w(px(TAB_CONTEXT_MENU_WIDTH))
                .child(
                    context_menu_item(
                        &self.tokens,
                        if detached {
                            self.i18n.t("tabbar.return_to_main_window")
                        } else {
                            self.i18n.t("tabbar.detach_to_window")
                        },
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.close_tab_context_menu();
                            if detached {
                                this.return_detached_tab_to_main(menu.tab_id, cx);
                            } else {
                                this.detach_tab_to_window(menu.tab_id, window, cx);
                            }
                            cx.stop_propagation();
                        }),
                    ),
                )
                .child(context_menu_separator(&self.tokens))
                .child(
                    context_menu_item(
                        &self.tokens,
                        self.i18n.t("tabbar.close_tab"),
                        ContextMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.close_tab_context_menu();
                            this.close_tab_by_id(menu.tab_id, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
        );

        Some(
            self.workspace_context_menu_backdrop(
                deferred(
                    anchored()
                        .anchor(Corner::TopLeft)
                        .position(gpui::point(px(placement.x), px(placement.y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(overlay_content_boundary(menu_body)),
                )
                .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
                cx,
            )
            .into_any_element(),
        )
    }

    pub(super) fn render_detached_tab_window(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(tab) = self.tab_by_id(tab_id).cloned() else {
            return self.render_detached_tab_message("OxideTerm", "tabbar.detached_tab_closed", cx);
        };
        let title = self.tab_display_title(&tab);
        window.set_window_title(&SharedString::from(title.clone()));

        let content = self.render_detached_tab_content(tab_id, &tab.kind, tab.root_pane.as_ref(), window, cx);
        let content = self.wrap_content_background(content, Some(tab_background_key(&tab.kind)), cx);

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(self.tokens.ui.bg))
            .child(self.render_detached_tab_title_bar(tab_id, title, window, cx))
            .child(div().flex_1().min_h(px(0.0)).child(content))
            .when_some(
                self.render_detached_tab_return_drag_preview(tab_id, window),
                |root, preview| root.child(preview),
            )
            .into_any_element()
    }

    fn render_detached_tab_content(
        &mut self,
        tab_id: TabId,
        kind: &TabKind,
        root_pane: Option<&PaneNode>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match (kind, root_pane) {
            (TabKind::Settings, _) => self.render_settings_surface(cx),
            (TabKind::FileManager, _) => self.render_file_manager_surface(window, cx),
            (TabKind::Launcher, _) => self.render_launcher_surface(window, cx),
            (TabKind::Graphics, _) => self.render_graphics_surface(window, cx),
            (TabKind::Runtime, _) => self.render_connection_runtime_surface(cx),
            (TabKind::ConnectionPool, _) => self.render_connection_pool_surface(cx),
            (TabKind::ConnectionMonitor, _) => self.render_connection_monitor_surface(cx),
            (TabKind::Topology, _) => self.render_topology_surface(cx),
            (TabKind::NotificationCenter, _) => self.render_notification_center_surface(cx),
            (TabKind::Sftp, _) => self.render_sftp_surface_for_tab(tab_id, window, cx),
            (TabKind::Ide, _) => self.render_ide_surface_for_tab(tab_id),
            (TabKind::Forwards, _) => self.render_forwards_surface_for_tab(tab_id, window, cx),
            (TabKind::SessionManager, _) => self.render_session_manager_surface(window, cx),
            (TabKind::PluginManager, _) => self.render_plugin_manager_surface(cx),
            (TabKind::Plugin { plugin_id, tab_id }, _) => {
                self.render_native_plugin_tab_surface(plugin_id, tab_id, cx)
            }
            (TabKind::CloudSync, _) => self.render_cloud_sync_surface(cx),
            (_, Some(root_pane)) => self.render_detached_terminal_surface(tab_id, root_pane, cx),
            _ => self.render_empty_workspace(cx),
        }
    }

    fn render_detached_tab_title_bar(
        &self,
        tab_id: TabId,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(self.tokens.metrics.titlebar_height))
            .w_full()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .pl(px(72.0))
            .text_size(px(self.tokens.metrics.titlebar_label_font_size))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .id(("detached-tab-title-drag", tab_id.0))
                    .h_full()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.start_detached_tab_return_drag(tab_id, event, window, cx);
                            window.start_window_move();
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                        this.update_detached_tab_return_drag(tab_id, event, window, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                            if this.finish_detached_tab_return_drag(tab_id, event, window, cx) {
                                window.remove_window();
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .child(div().min_w(px(0.0)).truncate().child(title)),
            )
            .child(
                div()
                    .h_full()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .text_color(rgb(theme.text_muted))
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .child(Self::render_lucide_icon(LucideIcon::PanelLeft, 15.0, rgb(theme.text_muted)))
                    .child(self.i18n.t("tabbar.return_to_main_window"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.return_detached_tab_to_main(tab_id, cx);
                            window.remove_window();
                            cx.stop_propagation();
                        }),
                    ),
            )
            .when(cfg!(any(target_os = "windows", target_os = "linux")), |bar| {
                bar.child(self.render_detached_client_titlebar_controls(window, cx))
            })
            .into_any_element()
    }

    fn render_detached_client_titlebar_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let maximize_glyph = if window.is_maximized() { "❐" } else { "□" };
        div()
            .h_full()
            .flex()
            .child(self.detached_client_titlebar_button("−", gpui::WindowControlArea::Min, cx))
            .child(self.detached_client_titlebar_button(maximize_glyph, gpui::WindowControlArea::Max, cx))
            .child(self.detached_client_titlebar_button("×", gpui::WindowControlArea::Close, cx))
            .into_any_element()
    }

    fn detached_client_titlebar_button(
        &self,
        glyph: &'static str,
        control_area: gpui::WindowControlArea,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w(px(46.0))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .hover({
                let hover_bg = self.tokens.ui.bg_hover;
                move |button| button.bg(rgb(hover_bg))
            })
            .when(cfg!(target_os = "windows"), |button| {
                button.window_control_area(control_area)
            })
            .when(!cfg!(target_os = "windows"), |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |_this, _event, window, cx| {
                        match control_area {
                            gpui::WindowControlArea::Min => window.minimize_window(),
                            gpui::WindowControlArea::Max => window.zoom_window(),
                            gpui::WindowControlArea::Close => window.remove_window(),
                            gpui::WindowControlArea::Drag => {}
                        }
                        cx.stop_propagation();
                    }),
                )
            })
            .child(glyph)
            .into_any_element()
    }

    fn render_detached_tab_message(
        &self,
        title: &'static str,
        message_key: &'static str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(self.tokens.ui.bg))
            .child(
                div()
                    .h(px(self.tokens.metrics.titlebar_height))
                    .flex()
                    .items_center()
                    .px(px(16.0))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(message_key)),
            )
            .into_any_element()
    }

    fn activate_nearest_visible_main_tab(
        &mut self,
        detached_tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.main_window_tabs.active_tab_id != Some(detached_tab_id) {
            return;
        }
        let Some(detached_index) = self.tab_index_by_id(detached_tab_id) else {
            self.main_window_tabs.active_tab_id = None;
            return;
        };
        self.main_window_tabs.active_tab_id = self
            .tabs
            .iter()
            .enumerate()
            .skip(detached_index + 1)
            .find(|(_, tab)| !self.detached_tabs.contains(&tab.id))
            .or_else(|| {
                self.tabs
                    .iter()
                    .enumerate()
                    .take(detached_index)
                    .rev()
                    .find(|(_, tab)| !self.detached_tabs.contains(&tab.id))
            })
            .map(|(_, tab)| tab.id);
        self.sync_active_tab_surface();
        self.focus_active_pane(window, cx);
    }
}
