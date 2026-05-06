use super::session_manager::SessionManagerInput;
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidebarSection {
    Sessions,
    Connections,
    Terminal,
    Activity,
    Network,
    Extensions,
    Assistant,
    Automation,
    Workspace,
    Files,
    Monitor,
    Notifications,
    Settings,
}

impl SidebarSection {
    pub(super) fn from_settings_key(key: &str) -> Self {
        match key {
            "connections" | "saved" => Self::Connections,
            "sftp" | "terminal" => Self::Terminal,
            "forwards" | "activity" => Self::Activity,
            "network" => Self::Network,
            "extensions" => Self::Extensions,
            "ai" | "assistant" => Self::Assistant,
            "automation" => Self::Automation,
            "workspace" => Self::Workspace,
            "files" => Self::Files,
            "monitor" => Self::Monitor,
            "notifications" => Self::Notifications,
            "settings" => Self::Settings,
            _ => Self::Sessions,
        }
    }

    pub(super) fn as_settings_key(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Connections => "connections",
            Self::Terminal => "terminal",
            Self::Activity => "activity",
            Self::Network => "network",
            Self::Extensions => "extensions",
            Self::Assistant => "ai",
            Self::Automation => "automation",
            Self::Workspace => "workspace",
            Self::Files => "files",
            Self::Monitor => "monitor",
            Self::Notifications => "notifications",
            Self::Settings => "settings",
        }
    }
}

impl WorkspaceApp {
    pub(super) fn persist_sidebar_settings(&mut self) {
        self.settings_store.settings_mut().sidebar_ui.collapsed = self.sidebar_collapsed;
        self.settings_store.settings_mut().sidebar_ui.width = self.sidebar_width.round() as i64;
        self.settings_store.settings_mut().sidebar_ui.active_section =
            self.active_sidebar_section.as_settings_key().to_string();
        let _ = self.settings_store.save();
    }

    pub(super) fn set_sidebar_section(&mut self, section: SidebarSection, cx: &mut Context<Self>) {
        self.active_sidebar_section = section;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.sidebar_resizing = false;
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn sidebar_panel_width(&self) -> f32 {
        (self.sidebar_width - self.tokens.metrics.activity_bar_width).max(0.0)
    }

    pub(super) fn set_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.sidebar_width = width.clamp(
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        cx.notify();
    }

    pub(super) fn start_sidebar_resize(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.sidebar_resizing = true;
        self.set_sidebar_width(f32::from(event.position.x), cx);
    }

    pub(super) fn update_sidebar_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.sidebar_resizing {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        self.set_sidebar_width(f32::from(event.position.x), cx);
    }

    pub(super) fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resizing {
            self.sidebar_resizing = false;
            self.persist_sidebar_settings();
            cx.notify();
        }
    }

    pub(super) fn render_title_bar(&self) -> AnyElement {
        let theme = self.tokens.ui;
        let titlebar_bg = titlebar_background(theme.bg_panel, theme.bg_active, theme.accent);
        let titlebar_border = mix_rgb(titlebar_bg, theme.border, 0.65);
        let label_color = readable_color(titlebar_bg, theme.accent, theme.text_heading);
        let text_color = readable_color(titlebar_bg, theme.text_muted, theme.text);
        let label_padding_left = if cfg!(target_os = "macos") {
            self.tokens.metrics.titlebar_label_x()
        } else {
            12.0
        };

        div()
            .h(px(self.tokens.metrics.titlebar_height))
            .flex()
            .flex_row()
            .items_center()
            .window_control_area(gpui::WindowControlArea::Drag)
            .bg(rgb(titlebar_bg))
            .border_b_1()
            .border_color(rgb(titlebar_border))
            .text_size(px(self.tokens.metrics.titlebar_label_font_size))
            .text_color(rgb(text_color))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .pl(px(label_padding_left))
                    .text_color(rgb(label_color))
                    .child(self.i18n.t("titlebar.open_recent_project")),
            )
            .when(cfg!(target_os = "windows"), |bar| {
                bar.child(self.render_windows_titlebar_controls(titlebar_bg, text_color))
            })
            .into_any_element()
    }

    fn render_windows_titlebar_controls(&self, titlebar_bg: u32, text_color: u32) -> AnyElement {
        div()
            .h_full()
            .flex()
            .flex_row()
            .window_control_area(gpui::WindowControlArea::Drag)
            .child(self.windows_titlebar_button(
                "−",
                gpui::WindowControlArea::Min,
                titlebar_button_hover(titlebar_bg),
                text_color,
            ))
            .child(self.windows_titlebar_button(
                "□",
                gpui::WindowControlArea::Max,
                titlebar_button_hover(titlebar_bg),
                text_color,
            ))
            .child(self.windows_titlebar_button(
                "×",
                gpui::WindowControlArea::Close,
                0xc42b1c,
                0xffffff,
            ))
            .into_any_element()
    }

    fn windows_titlebar_button(
        &self,
        glyph: &'static str,
        control_area: gpui::WindowControlArea,
        hover_bg: u32,
        text_color: u32,
    ) -> AnyElement {
        div()
            .w(px(46.0))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.0))
            .text_color(rgb(text_color))
            .window_control_area(control_area)
            .hover(move |button| button.bg(rgb(hover_bg)))
            .child(glyph)
            .into_any_element()
    }

    pub(super) fn render_activity_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let top_items = [
            (SidebarSection::Sessions, LucideIcon::Link2),
            (SidebarSection::Connections, LucideIcon::LayoutList),
            (SidebarSection::Terminal, LucideIcon::Terminal),
            (SidebarSection::Activity, LucideIcon::Activity),
            (SidebarSection::Network, LucideIcon::Network),
            (SidebarSection::Extensions, LucideIcon::Puzzle),
            (SidebarSection::Assistant, LucideIcon::Sparkles),
            (SidebarSection::Automation, LucideIcon::Bot),
        ];
        let bottom_items = [
            (SidebarSection::Workspace, LucideIcon::Square),
            (SidebarSection::Files, LucideIcon::FolderOpen),
            (SidebarSection::Monitor, LucideIcon::Monitor),
            (SidebarSection::Notifications, LucideIcon::Bell),
            (SidebarSection::Settings, LucideIcon::Settings),
        ];

        let mut bar = div()
            .w(px(self.tokens.metrics.activity_bar_width))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .py_2()
            .bg(rgb(theme.bg))
            .border_r_1()
            .border_color(rgb(theme.border));

        bar = bar
            .child(
                div()
                    .size(px(self.tokens.metrics.activity_icon_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        if self.sidebar_collapsed {
                            LucideIcon::PanelLeft
                        } else {
                            LucideIcon::PanelLeftClose
                        },
                        self.tokens.metrics.activity_icon_glyph_size,
                        rgb(theme.text_heading),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_sidebar(cx);
                        }),
                    ),
            )
            .child(
                div()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .my_1()
                    .bg(rgb(theme.divider)),
            );

        for (section, icon) in top_items {
            bar = bar.child(self.render_activity_icon(section, icon, cx));
        }

        bar.child(div().flex_1())
            .child(
                div()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .bg(rgb(theme.divider)),
            )
            .children(
                bottom_items
                    .into_iter()
                    .map(|(section, icon)| self.render_activity_icon(section, icon, cx)),
            )
            .into_any_element()
    }

    pub(super) fn render_activity_icon(
        &self,
        section: SidebarSection,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_sidebar_section == section;
        div()
            .id(("activity-icon", section as u64))
            .relative()
            .size(px(self.tokens.metrics.activity_icon_size))
            .mb(px(self.tokens.spacing.icon_gap))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .bg(if active {
                rgb(theme.bg_active)
            } else {
                rgb(theme.bg)
            })
            .border_1()
            .border_color(if active {
                rgb(theme.border)
            } else {
                rgb(theme.bg)
            })
            .cursor_pointer()
            .when(active, |icon_el| {
                icon_el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(self.tokens.metrics.activity_indicator_inset))
                        .bottom(px(self.tokens.metrics.activity_indicator_inset))
                        .w(px(self.tokens.metrics.activity_indicator_width))
                        .rounded(px(self.tokens.radii.active_indicator))
                        .bg(rgb(theme.accent)),
                )
            })
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.activity_icon_glyph_size,
                if active {
                    rgb(theme.text_heading)
                } else {
                    rgb(theme.text)
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if section == SidebarSection::Settings {
                        this.open_settings(window, cx);
                    } else if section == SidebarSection::Connections {
                        this.open_session_manager_tab(window, cx);
                    } else if section == SidebarSection::Workspace {
                        this.active_sidebar_section = section;
                        this.persist_sidebar_settings();
                        let _ = this.create_local_terminal_tab(window, cx);
                    } else {
                        this.active_surface = ActiveSurface::Terminal;
                        this.set_sidebar_section(section, cx);
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    pub(super) fn render_sidebar_region(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .relative()
            .w(px(self.sidebar_panel_width()))
            .h_full()
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(self.tokens.metrics.sidebar_resize_handle_width))
                    .cursor(CursorStyle::ResizeColumn)
                    .bg(if self.sidebar_resizing {
                        rgb(theme.accent)
                    } else {
                        rgba(theme.bg << 8)
                    })
                    .hover(|handle| handle.bg(rgba((theme.accent << 8) | 0x80)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event, _window, cx| {
                            this.start_sidebar_resize(event, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border))
            .child(self.render_sidebar_header(cx))
            .child(self.render_sidebar_content(cx))
            .into_any_element()
    }

    pub(super) fn render_sidebar_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let title_key = match self.active_sidebar_section {
            SidebarSection::Connections => "sidebar.panels.saved_connections",
            _ => "sidebar.panels.sessions",
        };
        let mut header = div()
            .h(px(self.tokens.metrics.sidebar_header_height))
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(title_key).to_uppercase()),
            );
        if self.active_sidebar_section != SidebarSection::Connections {
            header = header
                .child(self.render_sidebar_action(LucideIcon::Folder, false, cx))
                .child(self.render_sidebar_action(LucideIcon::Network, false, cx))
                .child(self.render_sidebar_action(LucideIcon::Plus, true, cx));
        }
        header.into_any_element()
    }

    pub(super) fn render_sidebar_action(
        &self,
        icon: LucideIcon,
        opens_connection_form: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut button = div()
            .size(px(self.tokens.metrics.sidebar_action_size))
            .ml_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.sidebar_action_icon_size,
                rgb(theme.text),
            ));

        if opens_connection_form {
            button = button.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.open_new_connection_form(window, cx);
                }),
            );
        }

        button.into_any_element()
    }

    pub(super) fn render_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.active_sidebar_section == SidebarSection::Connections {
            return self.render_saved_connections_sidebar_content(cx);
        }
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .px(px(self.tokens.metrics.empty_sidebar_padding_x))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .w_full()
                    .h(px(self.tokens.metrics.empty_sidebar_height))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(div().mb_3().child(Self::render_lucide_icon(
                        LucideIcon::Server,
                        self.tokens.metrics.empty_sidebar_icon_size,
                        rgba((theme.text_muted << 8) | 0x4d),
                    )))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sessions.tree.no_sessions")),
                    )
                    .child(
                        div()
                            .mt_1()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_subtitle_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sessions.tree.click_to_add")),
                    ),
            )
            .into_any_element()
    }

    fn render_saved_connections_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let query = self.session_manager.search_query.trim().to_lowercase();
        let mut connections = self.connection_store.connection_infos();
        connections.sort_by(|left, right| {
            right
                .last_used_at
                .cmp(&left.last_used_at)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        if !query.is_empty() {
            connections.retain(|conn| {
                conn.name.to_lowercase().contains(&query)
                    || conn.host.to_lowercase().contains(&query)
                    || conn.username.to_lowercase().contains(&query)
            });
        }

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .px(px(12.0))
            .pb(px(10.0))
            .gap(px(10.0))
            .child(self.render_session_text_input(
                SessionManagerInput::Search,
                &self.session_manager.search_query,
                self.i18n.t("sessionManager.toolbar.search_placeholder"),
                cx,
            ))
            .child(
                div()
                    .id("saved-connections-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .children(
                        connections
                            .into_iter()
                            .map(|conn| self.render_saved_connection_sidebar_row(conn, cx)),
                    ),
            )
            .when(self.connection_store.connections().is_empty(), |content| {
                content.child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Server,
                            self.tokens.metrics.empty_sidebar_icon_size,
                            rgba((theme.text_muted << 8) | 0x4d),
                        ))
                        .child(
                            div()
                                .text_center()
                                .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                                .child(self.i18n.t("sessionManager.table.no_connections")),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_saved_connection_sidebar_row(
        &self,
        conn: oxideterm_connections::ConnectionInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let id = conn.id.clone();
        let detail = format!("{}@{}:{}", conn.username, conn.host, conn.port);
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(8.0))
            .py(px(7.0))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                15.0,
                rgb(theme.accent),
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
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(conn.name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.accent))
                            .child(detail),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.open_saved_connection(&id, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}

fn titlebar_background(panel: u32, active: u32, accent: u32) -> u32 {
    let base = mix_rgb(panel, active, 0.42);
    mix_rgb(base, accent, 0.08)
}

fn titlebar_button_hover(background: u32) -> u32 {
    if relative_luminance(background) > 0.45 {
        mix_rgb(background, 0x000000, 0.10)
    } else {
        mix_rgb(background, 0xffffff, 0.12)
    }
}

fn readable_color(background: u32, preferred: u32, fallback: u32) -> u32 {
    if contrast_ratio(background, preferred) >= 3.0 {
        preferred
    } else {
        fallback
    }
}

fn mix_rgb(a: u32, b: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |shift: u32| {
        let left = ((a >> shift) & 0xffu32) as f32;
        let right = ((b >> shift) & 0xffu32) as f32;
        (left + (right - left) * amount).round().clamp(0.0, 255.0) as u32
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

fn contrast_ratio(a: u32, b: u32) -> f32 {
    let l1 = relative_luminance(a);
    let l2 = relative_luminance(b);
    let (light, dark) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (light + 0.05) / (dark + 0.05)
}

fn relative_luminance(color: u32) -> f32 {
    let channel = |shift: u32| {
        let value = ((color >> shift) & 0xffu32) as f32 / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    channel(16) * 0.2126 + channel(8) * 0.7152 + channel(0) * 0.0722
}
