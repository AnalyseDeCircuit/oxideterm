use gpui::{
    AnchoredPositionMode, Corner, Div, ObjectFit, StatefulInteractiveElement, StyledImage,
    anchored, deferred, point,
};
use oxideterm_settings::{
    AdaptiveRendererMode, AiReasoningEffort, AiThinkingStyle, AnimationSpeed, BackgroundFit,
    ConflictAction, CursorStyle as SettingsCursorStyle, FontFamily, IdeAgentMode, Language,
    PersistedSettings, TerminalEncoding, UiDensity, UpdateChannel,
};
use oxideterm_theme::BUILT_IN_THEMES;

use super::ime::WorkspaceImeTarget;
use super::*;
use crate::ui::{
    button,
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with},
    checkbox::checkbox,
    select::{
        OverlayAnchor, SelectAnchorId, select_anchor_probe, select_option, select_overlay_popup,
        select_trigger,
    },
    separator::{SeparatorOrientation, separator},
    slider::{SliderView, slider},
    text_input::{TextInputView, text_input, text_input_anchor_probe},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActiveSurface {
    Terminal,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsTab {
    General,
    Portable,
    Terminal,
    Appearance,
    Local,
    Connections,
    Ssh,
    Reconnect,
    Sftp,
    Ide,
    Ai,
    Knowledge,
    Keybindings,
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalSettingsPage {
    Display,
    Input,
    CommandBar,
    History,
    Transfer,
    Highlight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsSelect {
    Language,
    AppearanceTheme,
    AppearanceDensity,
    AppearanceAnimation,
    AppearanceBackgroundFit,
    TerminalFontFamily,
    TerminalEncoding,
    TerminalAdaptiveRenderer,
    TerminalCursorStyle,
}

impl SettingsSelect {
    fn anchor_id(self) -> SelectAnchorId {
        match self {
            Self::Language => SelectAnchorId::SettingsLanguage,
            Self::AppearanceTheme => SelectAnchorId::SettingsAppearanceTheme,
            Self::AppearanceDensity => SelectAnchorId::SettingsAppearanceDensity,
            Self::AppearanceAnimation => SelectAnchorId::SettingsAppearanceAnimation,
            Self::AppearanceBackgroundFit => SelectAnchorId::SettingsAppearanceBackgroundFit,
            Self::TerminalFontFamily => SelectAnchorId::SettingsTerminalFontFamily,
            Self::TerminalEncoding => SelectAnchorId::SettingsTerminalEncoding,
            Self::TerminalAdaptiveRenderer => SelectAnchorId::SettingsTerminalAdaptiveRenderer,
            Self::TerminalCursorStyle => SelectAnchorId::SettingsTerminalCursorStyle,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum SettingsInput {
    TerminalFontSize,
    TerminalLineHeight,
    AppearanceUiFont,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsSlider {
    TerminalFontSize,
    AppearanceBorderRadius,
    AppearanceBackgroundOpacity,
    AppearanceBackgroundBlur,
}

impl TerminalSettingsPage {
    fn all() -> &'static [Self] {
        &[
            Self::Display,
            Self::Input,
            Self::CommandBar,
            Self::History,
            Self::Transfer,
            Self::Highlight,
        ]
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Display => "settings_view.terminal.page_display",
            Self::Input => "settings_view.terminal.page_input",
            Self::CommandBar => "settings_view.terminal.page_commandBar",
            Self::History => "settings_view.terminal.page_history",
            Self::Transfer => "settings_view.terminal.page_transfer",
            Self::Highlight => "settings_view.terminal.page_highlight",
        }
    }
}

impl SettingsTab {
    fn groups() -> &'static [&'static [Self]] {
        &[
            &[Self::General, Self::Portable],
            &[Self::Terminal, Self::Appearance, Self::Local],
            &[Self::Connections, Self::Ssh, Self::Reconnect],
            &[
                Self::Sftp,
                Self::Ide,
                Self::Ai,
                Self::Knowledge,
                Self::Keybindings,
            ],
            &[Self::Help],
        ]
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::General => "settings.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings.terminal.title",
            Self::Appearance => "settings_view.tabs.appearance",
            Self::Local => "settings_view.tabs.local",
            Self::Connections => "settings_view.tabs.connections",
            Self::Ssh => "settings_view.tabs.ssh",
            Self::Reconnect => "settings_view.tabs.reconnect",
            Self::Sftp => "settings_view.tabs.sftp",
            Self::Ide => "settings_view.tabs.ide",
            Self::Ai => "settings_view.tabs.ai",
            Self::Knowledge => "settings_view.tabs.knowledge",
            Self::Keybindings => "settings_view.tabs.keybindings",
            Self::Help => "settings_view.tabs.help",
        }
    }

    fn title_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.title",
            Self::Portable => "settings_view.general.portable_runtime",
            Self::Terminal => "settings_view.terminal.title",
            Self::Appearance => "settings_view.appearance.title",
            Self::Local => "settings_view.local_terminal.title",
            Self::Connections => "settings_view.connections.title",
            Self::Ssh => "settings_view.tabs.ssh",
            Self::Reconnect => "settings_view.reconnect.title",
            Self::Sftp => "settings_view.sftp.title",
            Self::Ide => "settings_view.ide.title",
            Self::Ai => "settings_view.ai.title",
            Self::Knowledge => "settings_view.knowledge.title",
            Self::Keybindings => "settings_view.keybindings.title",
            Self::Help => "settings_view.help.title",
        }
    }

    fn description_key(self) -> &'static str {
        match self {
            Self::General => "settings_view.general.description",
            Self::Portable => "settings_view.general.portable_runtime_disabled_hint",
            Self::Terminal => "settings_view.terminal.description",
            Self::Appearance => "settings_view.appearance.description",
            Self::Local => "settings_view.local_terminal.description",
            Self::Connections => "settings_view.connections.description",
            Self::Ssh => "ssh.form.subtitle",
            Self::Reconnect => "settings_view.reconnect.description",
            Self::Sftp => "settings_view.sftp.description",
            Self::Ide => "settings_view.ide.description",
            Self::Ai => "settings_view.ai.description",
            Self::Knowledge => "settings_view.knowledge.description",
            Self::Keybindings => "settings_view.keybindings.description",
            Self::Help => "settings_view.help.description",
        }
    }

    fn icon(self) -> LucideIcon {
        match self {
            Self::General | Self::Appearance => LucideIcon::Monitor,
            Self::Portable | Self::Sftp => LucideIcon::HardDrive,
            Self::Local => LucideIcon::Square,
            Self::Terminal => LucideIcon::Terminal,
            Self::Connections => LucideIcon::Shield,
            Self::Ssh => LucideIcon::Key,
            Self::Reconnect => LucideIcon::WifiOff,
            Self::Ide => LucideIcon::Code2,
            Self::Ai => LucideIcon::Sparkles,
            Self::Knowledge => LucideIcon::BookOpen,
            Self::Keybindings => LucideIcon::Keyboard,
            Self::Help => LucideIcon::HelpCircle,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_tab(window, cx);
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let close_active_settings_tab = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Settings);
        self.active_surface = ActiveSurface::Terminal;
        self.open_settings_select = None;
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        if close_active_settings_tab {
            self.close_active_tab(window, cx);
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn render_settings_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(self.render_settings_nav(cx))
            .child(
                div()
                    .id("settings-content-scroll")
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .p(px(self.tokens.metrics.settings_content_padding))
                            .child(self.render_settings_tab_content(cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut nav = div()
            .w(px(224.0))
            .h_full()
            .flex()
            .flex_col()
            .pt(px(24.0))
            .pb_4()
            .bg(rgb(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border));

        nav = nav.child(
            div()
                .px(px(20.0))
                .mb(px(24.0))
                .text_size(px(20.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text_heading))
                .child(self.i18n.t("settings_view.title")),
        );

        let mut list = div()
            .id("settings-nav-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_3()
            .flex()
            .flex_col();

        for (group_index, group) in SettingsTab::groups().iter().enumerate() {
            if group_index > 0 {
                list = list.child(
                    div()
                        .py_2()
                        .child(separator(&self.tokens, SeparatorOrientation::Horizontal)),
                );
            }
            for tab in *group {
                list = list.child(self.render_settings_nav_item(*tab, cx));
            }
        }

        nav.child(list).into_any_element()
    }

    fn render_settings_nav_item(&self, tab: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_settings_tab == tab;
        div()
            .h(px(40.0))
            .w_full()
            .mb(px(4.0))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(self.tokens.radii.md))
            .bg(if active {
                rgb(theme.bg_active)
            } else {
                rgb(theme.bg_panel)
            })
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(rgb(if active {
                theme.text_heading
            } else {
                theme.text
            }))
            .cursor_pointer()
            .hover(move |item| {
                item.bg(if active {
                    rgb(theme.bg_active)
                } else {
                    rgb(theme.bg_hover)
                })
            })
            .child(Self::render_lucide_icon(tab.icon(), 16.0, rgb(theme.text)))
            .child(self.i18n.t(tab.label_key()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.active_settings_tab = tab;
                    this.active_surface = ActiveSurface::Settings;
                    this.open_settings_select = None;
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_settings_tab_content(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(self.render_settings_page_header(self.active_settings_tab))
            .child(separator(&self.tokens, SeparatorOrientation::Horizontal))
            .children(match self.active_settings_tab {
                SettingsTab::General => self.settings_general(cx),
                SettingsTab::Portable => self.settings_portable(),
                SettingsTab::Terminal => self.settings_terminal(cx),
                SettingsTab::Appearance => self.settings_appearance(cx),
                SettingsTab::Local => self.settings_local(cx),
                SettingsTab::Connections => self.settings_connections(cx),
                SettingsTab::Ssh => self.settings_ssh(),
                SettingsTab::Reconnect => self.settings_reconnect(cx),
                SettingsTab::Sftp => self.settings_sftp(cx),
                SettingsTab::Ide => self.settings_ide(cx),
                SettingsTab::Ai => self.settings_ai(cx),
                SettingsTab::Knowledge => self.settings_knowledge(),
                SettingsTab::Keybindings => self.settings_keybindings(),
                SettingsTab::Help => self.settings_help(cx),
            })
            .when_some(
                self.render_settings_select_overlay(cx),
                |content, overlay| content.child(overlay),
            )
            .into_any_element()
    }

    fn render_settings_page_header(&self, tab: SettingsTab) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(24.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    .child(self.i18n.t(tab.title_key())),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(tab.description_key())),
            )
            .into_any_element()
    }

    fn edit_settings(&mut self, edit: impl FnOnce(&mut PersistedSettings), cx: &mut Context<Self>) {
        edit(self.settings_store.settings_mut());
        let settings = self.settings_store.settings().clone();
        self.i18n
            .set_locale(locale_from_settings(settings.general.language));
        self.tokens = tokens_from_settings(&settings);
        self.sidebar_collapsed = settings.sidebar_ui.collapsed;
        self.sidebar_width = settings.sidebar_ui.width as f32;
        let terminal_preferences = self.terminal_preferences();
        for pane in self.panes.values() {
            let preferences = terminal_preferences.clone();
            let _ = pane.update(cx, |pane, cx| {
                pane.set_preferences(preferences, cx);
            });
        }
        let _ = self.settings_store.save();
        self.sync_tab_titles(cx);
        cx.notify();
    }

    fn settings_card(
        &self,
        title_key: &str,
        _description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(
                div()
                    .mb(px(self.tokens.metrics.settings_card_title_nudge_y))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(title_key).to_uppercase()),
            )
            .children(rows)
            .into_any_element()
    }

    fn plain_settings_card(&self, rows: Vec<AnyElement>) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .children(rows)
            .into_any_element()
    }

    fn card_title(&self, title_key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(title_key).to_uppercase())
            .into_any_element()
    }

    fn card_separator(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .w_full()
            .bg(rgba((self.tokens.ui.border << 8) | 0x80))
            .into_any_element()
    }

    fn text_badge(&self, label: String, color: u32) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | 0x1a))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    fn outline_button(&self, label: String, size: ButtonSize) -> AnyElement {
        button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant: ButtonVariant::Outline,
                size,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .into_any_element()
    }

    fn terminal_page_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut tabs = div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_card))
            .p(px(8.0));

        for page in TerminalSettingsPage::all() {
            let page_id = *page;
            let active = self.terminal_settings_page == page_id;
            let item = div()
                .rounded(px(self.tokens.radii.md))
                .px(px(12.0))
                .py(px(6.0))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                })
                .bg(if active {
                    rgba((theme.accent << 8) | 0x26)
                } else {
                    rgba(0x00000000)
                })
                .cursor_pointer()
                .hover(move |style| {
                    if active {
                        style
                    } else {
                        style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
                    }
                })
                .child(self.i18n.t(page_id.label_key()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.terminal_settings_page = page_id;
                        cx.notify();
                    }),
                );
            tabs = tabs.child(item);
        }

        tabs.into_any_element()
    }

    fn value_row(&self, label_key: &str, hint_key: &str, value: String) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            select_trigger(&self.tokens, value, false, false)
                .w(px(self.tokens.metrics.settings_select_width))
                .into_any_element(),
        )
    }

    pub(super) fn update_select_anchor(&mut self, anchor: OverlayAnchor, cx: &mut Context<Self>) {
        if self.select_anchors.get(&anchor.id) != Some(&anchor) {
            self.select_anchors.insert(anchor.id, anchor);
            cx.notify();
        }
    }

    pub(super) fn handle_settings_input_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.focused_settings_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        match key {
            "escape" | "enter" => {
                self.focused_settings_input = None;
                self.settings_input_draft.clear();
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" if !modifiers.platform && !modifiers.control => {
                self.settings_input_draft.pop();
                self.apply_settings_input_draft(input, cx);
                true
            }
            _ => true,
        }
    }

    pub(super) fn blur_text_inputs(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        if self.focused_settings_input.take().is_some() {
            self.settings_input_draft.clear();
            self.ime_marked_text = None;
            changed = true;
        }
        if self.open_settings_select.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if let Some(form) = self.new_connection_form.as_mut()
            && form.field_focused
        {
            form.field_focused = false;
            form.selected_field = None;
            self.ime_marked_text = None;
            changed = true;
        }
        if changed {
            self.new_connection_caret_visible = true;
            cx.notify();
        }
    }

    pub(super) fn update_settings_slider_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(slider) = self.settings_slider_drag {
            self.apply_settings_slider_from_position(slider, f32::from(event.position.x), cx);
        }
    }

    fn apply_settings_slider_from_position(
        &mut self,
        slider: SettingsSlider,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        match slider {
            SettingsSlider::TerminalFontSize => {
                self.set_font_size_from_position(x, cx);
            }
            SettingsSlider::AppearanceBorderRadius => {
                self.set_settings_slider_from_position(
                    SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                    x,
                    0.0,
                    24.0,
                    |settings, value| settings.appearance.border_radius = value.round() as i64,
                    cx,
                );
            }
            SettingsSlider::AppearanceBackgroundOpacity => {
                self.set_settings_slider_from_position(
                    SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                    x,
                    3.0,
                    50.0,
                    |settings, value| {
                        settings.terminal.background_opacity = value.round() as f64 / 100.0
                    },
                    cx,
                );
            }
            SettingsSlider::AppearanceBackgroundBlur => {
                self.set_settings_slider_from_position(
                    SelectAnchorId::SettingsAppearanceBackgroundBlurSlider,
                    x,
                    0.0,
                    20.0,
                    |settings, value| settings.terminal.background_blur = value.round() as i64,
                    cx,
                );
            }
        }
    }

    pub(super) fn finish_settings_slider_drag(&mut self, cx: &mut Context<Self>) {
        if self.settings_slider_drag.take().is_some() {
            cx.notify();
        }
    }

    fn focus_settings_input(
        &mut self,
        input: SettingsInput,
        current_value: String,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_select = None;
        self.focused_settings_input = Some(input);
        self.settings_input_draft = current_value;
        self.new_connection_caret_visible = true;
        cx.notify();
    }

    pub(super) fn apply_settings_input_draft(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) {
        match input {
            SettingsInput::TerminalFontSize => {
                if let Ok(value) = self.settings_input_draft.parse::<i64>() {
                    self.edit_settings(
                        |settings| settings.terminal.font_size = value.clamp(8, 32),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::TerminalLineHeight => {
                if let Ok(value) = self.settings_input_draft.parse::<f64>() {
                    self.edit_settings(
                        |settings| settings.terminal.line_height = value.clamp(0.8, 2.0),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::AppearanceUiFont => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(|settings| settings.appearance.ui_font_family = value, cx);
            }
        }
    }

    fn set_font_size_from_position(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SettingsTerminalFontSizeSlider)
            .copied()
        else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        let value = (8.0 + percent * (32.0 - 8.0)).round() as i64;
        if self.settings_store.settings().terminal.font_size != value {
            self.edit_settings(|settings| settings.terminal.font_size = value, cx);
        }
    }

    fn set_settings_slider_from_position(
        &mut self,
        anchor_id: SelectAnchorId,
        x: f32,
        min: f32,
        max: f32,
        apply: fn(&mut PersistedSettings, f32),
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.select_anchors.get(&anchor_id).copied() else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        let value = min + percent * (max - min);
        self.edit_settings(|settings| apply(settings, value), cx);
    }

    fn render_settings_select_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open_select = self.open_settings_select?;
        let anchor = *self.select_anchors.get(&open_select.anchor_id())?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let settings = self.settings_store.settings();

        let popup = match (self.active_settings_tab, open_select) {
            (SettingsTab::General, SettingsSelect::Language) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for language in language_options() {
                    let label = self.language_label(language);
                    popup = popup.child(
                        select_option(&self.tokens, label, language == settings.general.language)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.open_settings_select = None;
                                    this.edit_settings(
                                        |settings| settings.general.language = language,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceTheme) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for theme in BUILT_IN_THEMES {
                    let theme_id = theme.id.to_string();
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            theme_display_name(theme.id),
                            theme.id == settings.terminal.theme.as_str(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.theme = theme_id.clone(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceDensity) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &density in density_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            density_label(density, &self.i18n),
                            density == settings.appearance.ui_density,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.appearance.ui_density = density,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceAnimation) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &speed in animation_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            animation_label(speed, &self.i18n),
                            speed == settings.appearance.animation_speed,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.appearance.animation_speed = speed,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceBackgroundFit) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &fit in background_fit_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            background_fit_label(fit, &self.i18n),
                            fit == settings.terminal.background_fit,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.background_fit = fit,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &family in font_family_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            font_family_label(family),
                            family == settings.terminal.font_family,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.font_family = family,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalEncoding) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &encoding in terminal_encoding_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            terminal_encoding_label(encoding),
                            encoding == settings.terminal.terminal_encoding,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.terminal_encoding = encoding,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalAdaptiveRenderer) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &mode in adaptive_renderer_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            adaptive_renderer_label(mode, &self.i18n),
                            mode == settings.terminal.adaptive_renderer,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.adaptive_renderer = mode,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalCursorStyle) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &style in cursor_style_options() {
                    popup = popup.child(
                        select_option(
                            &self.tokens,
                            cursor_style_label(style, &self.i18n),
                            style == settings.terminal.cursor_style,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_settings_select = None;
                                this.edit_settings(
                                    |settings| settings.terminal.cursor_style = style,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
                Some(popup)
            }
            _ => None,
        }?;

        Some(
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position(anchor.bounds.bottom_left())
                    .offset(point(
                        px(0.0),
                        px(self.tokens.metrics.settings_select_popup_gap),
                    ))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(popup),
            )
            .with_priority(100)
            .into_any_element(),
        )
    }

    fn language_select_row(&self, selected: Language, cx: &mut Context<Self>) -> AnyElement {
        let control_width = self.tokens.metrics.settings_select_width;
        let anchor_id = SettingsSelect::Language.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, self.language_label(selected), false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select =
                        if this.open_settings_select == Some(SettingsSelect::Language) {
                            None
                        } else {
                            Some(SettingsSelect::Language)
                        };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let control = div()
            .relative()
            .w(px(control_width))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ));

        self.setting_row(
            "settings_view.general.language",
            "settings_view.general.language_hint",
            control.into_any_element(),
        )
    }

    fn select_setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let control = div().relative().w(px(width)).child(select_anchor_probe(
            anchor_id,
            trigger,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            },
        ));

        self.setting_row(label_key, hint_key, control.into_any_element())
    }

    fn count_row(&self, label_key: &str, hint_key: &str, count: usize) -> AnyElement {
        self.value_row(label_key, hint_key, count.to_string())
    }

    fn bool_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        self.setting_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, label, checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
        )
    }

    fn number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value: i64,
        step: i64,
        min: i64,
        max: i64,
        setter: fn(&mut PersistedSettings, i64),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = div()
            .h(px(self.tokens.metrics.ui_control_height))
            .w(px(112.0))
            .px(px(self.tokens.metrics.ui_control_padding_x))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .child(value.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let next = if value >= max { min } else { value + step };
                    this.edit_settings(|settings| setter(settings, next.clamp(min, max)), cx);
                }),
            )
            .into_any_element();
        self.setting_row(label_key, hint_key, control)
    }

    fn setting_row(&self, label_key: &str, hint_key: &str, control: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    fn terminal_preview(&self, settings: &PersistedSettings) -> AnyElement {
        let family = settings
            .terminal
            .font_family
            .terminal_family_name(&settings.terminal.custom_font_family);
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .p(px(self.tokens.metrics.settings_font_preview_padding))
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(self
                        .tokens
                        .metrics
                        .settings_font_preview_label_margin_bottom))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.terminal.font_preview")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family(&family)
                    .text_size(px(settings.terminal.font_size as f32))
                    .line_height(px(
                        settings.terminal.font_size as f32 * settings.terminal.line_height as f32
                    ))
                    .text_color(rgb(self.tokens.ui.text))
                    .child("ABCDEFG abcdefg 0123456789")
                    .child("Thực thi lệnh chậm - lưu, tổ chức, chạy")
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("-> => == != <= >= {}"),
                    )
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.success))
                            .child("天地玄黄 The quick brown fox"),
                    )
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.warning))
                            .child("       󰊤  "),
                    ),
            )
            .into_any_element()
    }

    fn number_input(
        &self,
        input: SettingsInput,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder: value.clone(),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(width))
            .justify_center()
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = match input {
                        SettingsInput::TerminalFontSize => this
                            .settings_store
                            .settings()
                            .terminal
                            .font_size
                            .to_string(),
                        SettingsInput::TerminalLineHeight => {
                            compact_decimal(this.settings_store.settings().terminal.line_height)
                        }
                        SettingsInput::AppearanceUiFont => this
                            .settings_store
                            .settings()
                            .appearance
                            .ui_font_family
                            .clone(),
                    };
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn font_size_row(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        let slider_view = SliderView {
            min: 8.0,
            max: 32.0,
            value: settings.terminal.font_size as f32,
            disabled: false,
        };
        let workspace = cx.entity();
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(self.tokens.metrics.settings_slider_width))
                    .child(select_anchor_probe(
                        SelectAnchorId::SettingsTerminalFontSizeSlider,
                        slider(&self.tokens, slider_view)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                    this.open_settings_select = None;
                                    this.focused_settings_input = None;
                                    this.settings_slider_drag =
                                        Some(SettingsSlider::TerminalFontSize);
                                    this.set_font_size_from_position(
                                        f32::from(event.position.x),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                                    this.finish_settings_slider_drag(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                |this, event: &MouseMoveEvent, _window, cx| {
                                    this.update_settings_slider_drag(event, cx);
                                },
                            )),
                        move |anchor, _window, cx| {
                            let _ = workspace.update(cx, |this, cx| {
                                this.update_select_anchor(anchor, cx);
                            });
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.number_input(
                        SettingsInput::TerminalFontSize,
                        settings.terminal.font_size.to_string(),
                        self.tokens.metrics.settings_font_size_input_width,
                        cx,
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("px"),
                    ),
            )
            .into_any_element();

        self.setting_row(
            "settings_view.terminal.font_size",
            "settings_view.terminal.font_size_hint",
            control,
        )
    }

    fn decimal_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.number_input(
                input,
                value,
                self.tokens.metrics.settings_number_input_width,
                cx,
            ),
        )
    }

    fn checkbox_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
        )
    }

    fn settings_general(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let data_dir = self
            .settings_store
            .path()
            .parent()
            .unwrap_or_else(|| self.settings_store.path())
            .display()
            .to_string();
        let cli_path = std::env::var_os("HOME")
            .map(|home| {
                std::path::PathBuf::from(home)
                    .join(".local")
                    .join("bin")
                    .join("oxt")
                    .display()
                    .to_string()
            })
            .unwrap_or_else(|| "~/.local/bin/oxt".to_string());

        vec![
            self.settings_card(
                "settings_view.general.language",
                "settings_view.general.language_hint",
                vec![self.language_select_row(settings.general.language, cx)],
            ),
            self.plain_settings_card(vec![
                self.card_title("settings_view.general.data_directory"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.general.data_directory")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.general.data_directory_hint")),
                    )
                    .into_any_element(),
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .text_color(rgb(self.tokens.ui.text))
                            .font_family("monospace")
                            .truncate()
                            .child(data_dir),
                    )
                    .child(self.outline_button(
                        self.i18n.t("settings_view.general.change"),
                        ButtonSize::Sm,
                    ))
                    .into_any_element(),
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.warning))
                    .child(
                        self.i18n
                            .t("settings_view.general.data_directory_restart_notice"),
                    )
                    .into_any_element(),
            ]),
            self.plain_settings_card(vec![
                self.card_title("settings_view.general.cli_companion"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.general.cli_tool")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.general.cli_tool_hint")),
                    )
                    .into_any_element(),
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_end()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Terminal,
                                        16.0,
                                        rgb(self.tokens.ui.text_muted),
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_family("monospace")
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child("oxide"),
                                    )
                                    .child(self.text_badge(
                                        self.i18n.t("settings_view.general.cli_installed"),
                                        self.tokens.ui.success,
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_family("monospace")
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(cli_path),
                            ),
                    )
                    .child(self.outline_button(
                        self.i18n.t("settings_view.general.cli_uninstall"),
                        ButtonSize::Sm,
                    ))
                    .into_any_element(),
            ]),
        ]
    }

    fn settings_portable(&self) -> Vec<AnyElement> {
        vec![self.settings_card(
            "settings_view.general.portable_runtime",
            "settings_view.general.portable_runtime_disabled_hint",
            vec![
                self.value_row(
                    "settings_view.general.portable_root_dir",
                    "settings_view.general.portable_runtime_hint",
                    self.i18n
                        .t("settings_view.general.portable_instance_lock_unavailable"),
                ),
                self.value_row(
                    "settings_view.general.portable_activation",
                    "settings_view.general.portable_runtime_hint",
                    self.i18n.t("settings_view.general.portable_activation_disabled"),
                ),
                self.value_row(
                    "settings_view.general.portable_config_path",
                    "settings_view.general.portable_runtime_hint",
                    self.i18n
                        .t("settings_view.general.portable_instance_lock_unavailable"),
                ),
                self.value_row(
                    "settings_view.general.portable_biometric",
                    "settings_view.general.portable_runtime_hint",
                    self.i18n
                        .t("settings_view.general.portable_biometric_unsupported"),
                ),
                self.value_row(
                    "settings_view.general.portable_change_password",
                    "settings_view.general.portable_runtime_hint",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.general.cli_tool",
                    "settings_view.general.cli_tool_hint",
                    self.i18n.t("settings_view.general.cli_not_installed"),
                ),
                self.value_row(
                    "settings_view.general.cli_install",
                    "settings_view.general.cli_reinstall_hint",
                    self.i18n.t("settings_view.general.cli_not_bundled"),
                ),
            ],
        )]
    }

    fn settings_terminal(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let mut rows = vec![self.terminal_page_switcher(cx)];

        match self.terminal_settings_page {
            TerminalSettingsPage::Display => {
                rows.push(self.settings_card(
                    "settings_view.terminal.font",
                    "settings_view.terminal.font_family_hint",
                    vec![
                        self.select_setting_row(
                            "settings_view.terminal.font_family",
                            "settings_view.terminal.font_family_hint",
                            SettingsSelect::TerminalFontFamily,
                            font_family_label(settings.terminal.font_family),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        self.terminal_preview(settings),
                        self.card_separator(),
                        self.font_size_row(settings, cx),
                        self.card_separator(),
                        self.decimal_row(
                            "settings_view.terminal.line_height",
                            "settings_view.terminal.line_height_hint",
                            SettingsInput::TerminalLineHeight,
                            compact_decimal(settings.terminal.line_height),
                            cx,
                        ),
                        self.card_separator(),
                        self.select_setting_row(
                            "settings_view.terminal.encoding",
                            "settings_view.terminal.encoding_hint",
                            SettingsSelect::TerminalEncoding,
                            terminal_encoding_label(settings.terminal.terminal_encoding),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        self.card_separator(),
                        self.select_setting_row(
                            "settings_view.terminal.adaptive_renderer",
                            "settings_view.terminal.adaptive_renderer_hint",
                            SettingsSelect::TerminalAdaptiveRenderer,
                            adaptive_renderer_label(
                                settings.terminal.adaptive_renderer,
                                &self.i18n,
                            ),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        self.checkbox_row(
                            "settings_view.terminal.show_fps_overlay",
                            "settings_view.terminal.show_fps_overlay_hint",
                            settings.terminal.show_fps_overlay,
                            set_show_fps_overlay,
                            cx,
                        ),
                    ],
                ));
                rows.push(self.settings_card(
                    "settings_view.terminal.cursor",
                    "settings_view.terminal.cursor_style_hint",
                    vec![
                        self.select_setting_row(
                            "settings_view.terminal.cursor_style",
                            "settings_view.terminal.cursor_style_hint",
                            SettingsSelect::TerminalCursorStyle,
                            cursor_style_label(settings.terminal.cursor_style, &self.i18n),
                            self.tokens.metrics.settings_select_narrow_width,
                            cx,
                        ),
                        self.card_separator(),
                        self.checkbox_row(
                            "settings_view.terminal.cursor_blink",
                            "settings_view.terminal.cursor_blink_hint",
                            settings.terminal.cursor_blink,
                            set_terminal_cursor_blink,
                            cx,
                        ),
                    ],
                ));
            }
            TerminalSettingsPage::Input => {
                rows.push(self.settings_card(
                    "settings_view.terminal.input_safety",
                    "settings_view.terminal.paste_protection_hint",
                    vec![
                        self.bool_row(
                            "settings_view.terminal.paste_protection",
                            "settings_view.terminal.paste_protection_hint",
                            settings.terminal.paste_protection,
                            set_paste_protection,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.osc52_clipboard",
                            "settings_view.terminal.osc52_clipboard_hint",
                            settings.terminal.osc52_clipboard,
                            set_osc52_clipboard,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.smart_copy",
                            "settings_view.terminal.smart_copy_hint",
                            settings.terminal.smart_copy,
                            set_smart_copy,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.copy_on_select",
                            "settings_view.terminal.copy_on_select_hint",
                            settings.terminal.copy_on_select,
                            set_copy_on_select,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.middle_click_paste",
                            "settings_view.terminal.middle_click_paste_hint",
                            settings.terminal.middle_click_paste,
                            set_middle_click_paste,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.selection_requires_shift",
                            "settings_view.terminal.selection_requires_shift_hint",
                            settings.terminal.selection_requires_shift,
                            set_selection_requires_shift,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.autosuggest_local_history",
                            "settings_view.terminal.autosuggest_local_history_hint",
                            settings.terminal.autosuggest.local_shell_history,
                            set_autosuggest_local_history,
                            cx,
                        ),
                    ],
                ));
            }
            TerminalSettingsPage::CommandBar => {
                rows.push(self.settings_card(
                    "settings_view.terminal.command_bar",
                    "settings_view.terminal.command_bar_hint",
                    vec![
                    self.bool_row(
                        "settings_view.terminal.command_bar",
                        "settings_view.terminal.command_bar_hint",
                        settings.terminal.command_bar.enabled,
                        set_command_bar_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_bar_legacy_toolbar",
                        "settings_view.terminal.command_bar_legacy_toolbar_hint",
                        settings.terminal.command_bar.show_legacy_toolbar,
                        set_command_bar_legacy_toolbar,
                        cx,
                    ),
                    self.card_separator(),
                    self.value_row(
                        "settings_view.terminal.command_bar_focus_handoff",
                        "settings_view.terminal.command_bar_focus_handoff_hint",
                        settings
                            .terminal
                            .command_bar
                            .focus_handoff_commands
                            .join("\n"),
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands",
                        "settings_view.terminal.quick_commands_hint",
                        settings.terminal.command_bar.quick_commands_enabled,
                        set_quick_commands_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_confirm",
                        "settings_view.terminal.quick_commands_confirm_hint",
                        settings
                            .terminal
                            .command_bar
                            .quick_commands_confirm_before_run,
                        set_quick_commands_confirm,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_toast",
                        "settings_view.terminal.quick_commands_toast_hint",
                        settings.terminal.command_bar.quick_commands_show_toast,
                        set_quick_commands_toast,
                        cx,
                    ),
                ],
                ));
            }
            TerminalSettingsPage::History => {
                rows.push(self.settings_card(
                    "settings_view.terminal.command_marks",
                    "settings_view.terminal.command_marks_hint",
                    vec![
                        self.bool_row(
                            "settings_view.terminal.command_marks",
                            "settings_view.terminal.command_marks_hint",
                            settings.terminal.command_marks.enabled,
                            set_command_marks_enabled,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.command_marks_hover_actions",
                            "settings_view.terminal.command_marks_hover_actions_hint",
                            settings.terminal.command_marks.show_hover_actions,
                            set_command_marks_hover_actions,
                            cx,
                        ),
                    ],
                ));
                rows.push(self.settings_card(
                    "settings_view.terminal.buffer",
                    "settings_view.terminal.scrollback_hint",
                    vec![
                        self.number_row(
                            "settings_view.terminal.scrollback",
                            "settings_view.terminal.scrollback_hint",
                            settings.terminal.scrollback,
                            500,
                            500,
                            20000,
                            set_terminal_scrollback,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.backend_buffer_lines",
                            "settings_view.terminal.backend_buffer_lines_hint",
                            settings.buffer.max_lines,
                            500,
                            5000,
                            12000,
                            set_buffer_max_lines,
                            cx,
                        ),
                    ],
                ));
            }
            TerminalSettingsPage::Transfer => {
                rows.push(self.settings_card(
                    "settings_view.terminal.in_band_transfer.title",
                    "settings_view.terminal.in_band_transfer.runtime_note",
                    vec![
                        self.bool_row(
                            "settings_view.terminal.in_band_transfer.enabled",
                            "settings_view.terminal.in_band_transfer.enabled_hint",
                            settings.terminal.in_band_transfer.enabled,
                            set_in_band_transfer_enabled,
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.in_band_transfer.allow_directory",
                            "settings_view.terminal.in_band_transfer.allow_directory_hint",
                            settings.terminal.in_band_transfer.allow_directory,
                            set_in_band_transfer_allow_directory,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.in_band_transfer.max_chunk_bytes",
                            "settings_view.terminal.in_band_transfer.max_chunk_bytes_hint",
                            settings.terminal.in_band_transfer.max_chunk_bytes,
                            262144,
                            1024,
                            16 * 1024 * 1024,
                            set_in_band_transfer_max_chunk_bytes,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.in_band_transfer.max_file_count",
                            "settings_view.terminal.in_band_transfer.max_file_count_hint",
                            settings.terminal.in_band_transfer.max_file_count,
                            1,
                            1,
                            10000,
                            set_in_band_transfer_max_file_count,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.in_band_transfer.max_total_bytes",
                            "settings_view.terminal.in_band_transfer.max_total_bytes_hint",
                            settings.terminal.in_band_transfer.max_total_bytes / (1024 * 1024),
                            512,
                            1,
                            1024 * 1024,
                            set_in_band_transfer_max_total_mb,
                            cx,
                        ),
                        self.card_separator(),
                        self.value_row(
                            "settings_view.terminal.in_band_transfer.title",
                            "settings_view.terminal.in_band_transfer.runtime_note",
                            self.i18n
                                .t("settings_view.terminal.in_band_transfer.runtime_note"),
                        ),
                    ],
                ));
            }
            TerminalSettingsPage::Highlight => {
                rows.push(self.settings_card(
                    "settings_view.terminal.highlight_rules.title",
                    "settings_view.terminal.highlight_rules.description",
                    vec![
                        self.count_row(
                            "settings_view.terminal.highlight_rules.add_rule",
                            "settings_view.terminal.highlight_rules.limit",
                            settings.terminal.highlight_rules.len(),
                        ),
                        self.card_separator(),
                        self.value_row(
                            "settings_view.terminal.highlight_rules.add_preset",
                            "settings_view.terminal.highlight_rules.priority_hint",
                            self.i18n.t("settings_view.terminal.highlight_rules.empty"),
                        ),
                    ],
                ));
            }
        }

        rows
    }

    fn settings_appearance(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.appearance_theme_card(settings, cx),
            self.appearance_layout_card(settings, cx),
            self.appearance_background_card(settings, cx),
        ]
    }

    fn appearance_theme_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.theme"),
            Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.appearance_action_button(
                        LucideIcon::Upload,
                        self.i18n.t("settings_view.appearance.theme_import"),
                    ))
                    .child(self.appearance_action_button(
                        LucideIcon::Plus,
                        self.i18n.t("settings_view.custom_theme.create"),
                    ))
                    .into_any_element(),
            ),
            vec![
                self.appearance_row(
                    "settings_view.appearance.color_theme",
                    "settings_view.appearance.color_theme_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceTheme,
                        theme_display_name(&settings.terminal.theme),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                ),
                self.appearance_theme_preview(settings),
            ],
        )
    }

    fn appearance_layout_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.layout"),
            None,
            vec![
                self.appearance_row(
                    "settings_view.appearance.density",
                    "settings_view.appearance.density_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceDensity,
                        density_label(settings.appearance.ui_density, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.border_radius",
                    "settings_view.appearance.border_radius_hint",
                    self.appearance_radius_control(settings, cx),
                ),
                self.appearance_row(
                    "settings_view.appearance.ui_font",
                    "settings_view.appearance.ui_font_hint",
                    self.appearance_text_input_control(
                        SettingsInput::AppearanceUiFont,
                        settings.appearance.ui_font_family.clone(),
                        self.i18n.t("settings_view.appearance.ui_font_placeholder"),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.animation",
                    "settings_view.appearance.animation_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceAnimation,
                        animation_label(settings.appearance.animation_speed, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
            ],
        )
    }

    fn appearance_background_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card_with_icon(
            LucideIcon::Image,
            self.i18n.t("settings_view.terminal.bg_title"),
            vec![
                self.appearance_checkbox_row(
                    "settings_view.terminal.bg_enabled",
                    "settings_view.terminal.bg_enabled_hint",
                    settings.terminal.background_enabled,
                    set_terminal_background_enabled,
                    cx,
                ),
                self.appearance_background_gallery(settings, cx),
                self.appearance_row(
                    "settings_view.terminal.bg_opacity",
                    "settings_view.terminal.bg_opacity_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundOpacity,
                        SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                        3.0,
                        50.0,
                        (settings.terminal.background_opacity * 100.0).round() as f32,
                        "%",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_blur",
                    "settings_view.terminal.bg_blur_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundBlur,
                        SelectAnchorId::SettingsAppearanceBackgroundBlurSlider,
                        0.0,
                        20.0,
                        settings.terminal.background_blur as f32,
                        "px",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_fit",
                    "settings_view.terminal.bg_fit_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceBackgroundFit,
                        background_fit_label(settings.terminal.background_fit, &self.i18n),
                        self.tokens.metrics.settings_appearance_fit_select_width,
                        cx,
                    ),
                ),
                self.appearance_background_tabs(settings, cx),
            ],
        )
    }

    fn appearance_card(
        &self,
        title: String,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(self.appearance_card_title(title, None))
                .when_some(actions, |header, actions| header.child(actions))
                .into_any_element(),
            rows,
        )
    }

    fn appearance_card_with_icon(
        &self,
        icon: LucideIcon,
        title: String,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(self.appearance_card_title(title, Some(icon)), rows)
    }

    fn appearance_card_shell(&self, header: AnyElement, rows: Vec<AnyElement>) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(header)
            .children(rows)
            .into_any_element()
    }

    fn appearance_card_title(&self, title: String, icon: Option<LucideIcon>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(match icon {
                Some(icon) => Self::render_lucide_icon(icon, 16.0, rgb(self.tokens.ui.text))
                    .into_any_element(),
                None => div().into_any_element(),
            })
            .child(title.to_uppercase())
            .into_any_element()
    }

    fn appearance_action_button(&self, icon: LucideIcon, label: String) -> AnyElement {
        div()
            .h(px(self.tokens.metrics.settings_appearance_action_height))
            .px(px(10.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgba(0x00000000))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            ))
            .child(label)
            .into_any_element()
    }

    fn appearance_row(&self, label_key: &str, hint_key: &str, control: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    fn appearance_checkbox_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
        )
    }

    fn appearance_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        div()
            .relative()
            .w(px(width))
            .min_w(px(0.0))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn appearance_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(width))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = this
                        .settings_store
                        .settings()
                        .appearance
                        .ui_font_family
                        .clone();
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn appearance_radius_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .size(px(28.0))
                    .rounded(px(settings.appearance.border_radius as f32))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_secondary)),
            )
            .child(self.appearance_slider_control(
                SettingsSlider::AppearanceBorderRadius,
                SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                0.0,
                24.0,
                settings.appearance.border_radius as f32,
                cx,
            ))
            .child(
                div()
                    .w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{}px", settings.appearance.border_radius)),
            )
            .into_any_element()
    }

    fn appearance_slider_value_control(
        &self,
        slider: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        unit: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(self.appearance_slider_control(slider, anchor_id, min, max, value, cx))
            .child(
                div()
                    .w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{}{}", value.round() as i64, unit)),
            )
            .into_any_element()
    }

    fn appearance_slider_control(
        &self,
        slider_id: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        div()
            .w(px(self.tokens.metrics.settings_slider_width))
            .child(select_anchor_probe(
                anchor_id,
                slider(
                    &self.tokens,
                    SliderView {
                        min,
                        max,
                        value,
                        disabled: false,
                    },
                )
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        this.open_settings_select = None;
                        this.focused_settings_input = None;
                        this.settings_slider_drag = Some(slider_id);
                        this.apply_settings_slider_from_position(
                            slider_id,
                            f32::from(event.position.x),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        this.finish_settings_slider_drag(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &MouseMoveEvent, _window, cx| {
                        this.update_settings_slider_drag(event, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn appearance_theme_preview(&self, settings: &PersistedSettings) -> AnyElement {
        let terminal = self.tokens.terminal;
        div()
            .w_full()
            .mt(px(self.tokens.metrics.settings_font_preview_margin_top))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(terminal.background))
            .p(px(self.tokens.metrics.settings_theme_preview_padding))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.settings_theme_preview_dot_gap))
                    .child(self.preview_dot(terminal.red))
                    .child(self.preview_dot(terminal.yellow))
                    .child(self.preview_dot(terminal.green)),
            )
            .child(
                div()
                    .font_family(
                        settings
                            .terminal
                            .font_family
                            .terminal_family_name(&settings.terminal.custom_font_family),
                    )
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(self.tokens.metrics.settings_theme_preview_line_height))
                    .text_color(rgb(terminal.foreground))
                    .flex()
                    .flex_col()
                    .child("$ echo \"Hello World\"")
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(6.0))
                            .child(div().text_color(rgb(terminal.blue)).child("~"))
                            .child(div().text_color(rgb(terminal.magenta)).child("git"))
                            .child(div().text_color(rgb(terminal.blue)).child("status")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .child(">")
                            .child(div().w(px(9.0)).h(px(18.0)).bg(rgb(terminal.cursor))),
                    ),
            )
            .into_any_element()
    }

    fn preview_dot(&self, color: u32) -> AnyElement {
        div()
            .size(px(self.tokens.metrics.settings_theme_preview_dot_size))
            .rounded_full()
            .bg(rgb(color))
            .into_any_element()
    }

    fn appearance_background_gallery(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.terminal.bg_gallery")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(self.appearance_action_button(
                                LucideIcon::Plus,
                                self.i18n.t("settings_view.terminal.bg_add"),
                            ))
                            .when(settings.terminal.background_image.is_some(), |actions| {
                                actions.child(
                                    div()
                                        .h(px(self
                                            .tokens
                                            .metrics
                                            .settings_appearance_action_height))
                                        .px(px(10.0))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(6.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.error))
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(rgba((self.tokens.ui.error << 8) | 0x14))
                                        })
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Trash2,
                                            14.0,
                                            rgb(self.tokens.ui.error),
                                        ))
                                        .child(self.i18n.t("settings_view.terminal.bg_clear_all"))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.edit_settings(
                                                    |settings| {
                                                        settings.terminal.background_image = None;
                                                    },
                                                    cx,
                                                );
                                            }),
                                        ),
                                )
                            }),
                    ),
            )
            .child(self.background_thumbnails(settings, cx))
            .into_any_element()
    }

    fn background_thumbnails(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(current) = settings.terminal.background_image.as_deref() else {
            return div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.terminal.bg_hint"))
                .into_any_element();
        };

        div()
            .w_full()
            .grid()
            .grid_cols(4)
            .gap(px(8.0))
            .child(self.background_thumbnail(current, true, cx))
            .into_any_element()
    }

    fn background_thumbnail(
        &self,
        image_path: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image_path = image_path.to_string();
        let image_source = std::path::PathBuf::from(&image_path);
        let fallback_label = std::path::Path::new(&image_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&image_path)
            .to_string();
        let fallback_text_size = self.tokens.metrics.ui_text_xs;
        let fallback_text_color = self.tokens.ui.text_muted;
        let fallback_icon_color = self.tokens.ui.text_muted;
        let fallback_bg = self.tokens.ui.bg_sunken;
        let image = gpui::img(image_source)
            .w_full()
            .h_full()
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || {
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .bg(rgb(fallback_bg))
                    .child(WorkspaceApp::render_lucide_icon(
                        LucideIcon::Image,
                        20.0,
                        rgb(fallback_icon_color),
                    ))
                    .child(
                        div()
                            .max_w_full()
                            .px(px(8.0))
                            .text_size(px(fallback_text_size))
                            .text_color(rgb(fallback_text_color))
                            .truncate()
                            .child(fallback_label.clone()),
                    )
                    .into_any_element()
            });

        div()
            .relative()
            .h(px(self.tokens.metrics.settings_background_thumb_height))
            .rounded(px(self.tokens.radii.md))
            .overflow_hidden()
            .border_2()
            .border_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .cursor_pointer()
            .child(image)
            .when(active, |thumb| {
                thumb.child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .left(px(8.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgb(self.tokens.ui.accent))
                        .px(px(self.tokens.metrics.settings_background_badge_padding_x))
                        .py(px(self.tokens.metrics.settings_background_badge_padding_y))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.accent_text))
                        .child(self.i18n.t("settings_view.terminal.bg_active")),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .p(px(3.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba(0x00000099))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::X,
                        12.0,
                        rgb(self.tokens.ui.text),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| {
                                    settings.terminal.background_image = None;
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let selected_path = image_path.clone();
                    this.edit_settings(
                        move |settings| {
                            settings.terminal.background_image = Some(selected_path);
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
    fn appearance_background_tabs(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut grid = div().w_full().grid().grid_cols(3).gap(px(10.0));
        for (key, label_key, icon) in background_tab_options() {
            let enabled = settings
                .terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == key);
            let key = (*key).to_string();
            grid = grid.child(
                self.background_tab_pill(&key, *label_key, *icon, enabled)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.toggle_background_tab(&key, cx);
                        }),
                    ),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.terminal.bg_tabs")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.terminal.bg_tabs_hint")),
                    ),
            )
            .child(grid)
            .into_any_element()
    }

    fn background_tab_pill(
        &self,
        _key: &str,
        label_key: &str,
        icon: LucideIcon,
        enabled: bool,
    ) -> Div {
        div()
            .h(px(40.0))
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(if enabled {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .bg(if enabled {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .px(px(14.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(if enabled {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.settings_background_tab_icon_size,
                rgb(if enabled {
                    self.tokens.ui.accent
                } else {
                    self.tokens.ui.text_muted
                }),
            ))
            .child(div().truncate().child(self.i18n.t(label_key)))
    }

    fn toggle_background_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                if let Some(index) = settings
                    .terminal
                    .background_enabled_tabs
                    .iter()
                    .position(|tab| tab == key)
                {
                    settings.terminal.background_enabled_tabs.remove(index);
                } else {
                    settings
                        .terminal
                        .background_enabled_tabs
                        .push(key.to_string());
                }
            },
            cx,
        );
    }

    fn settings_local(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.settings_card(
                "settings_view.local_terminal.shell",
                "settings_view.local_terminal.default_shell_hint",
                vec![
                    self.value_row(
                        "settings_view.local_terminal.default_shell",
                        "settings_view.local_terminal.default_shell_hint",
                        settings
                            .local_terminal
                            .default_shell_id
                            .clone()
                            .unwrap_or_else(|| self.i18n.t("settings_view.local_terminal.default")),
                    ),
                    self.value_row(
                        "settings_view.local_terminal.default_cwd",
                        "settings_view.local_terminal.default_cwd_hint",
                        settings
                            .local_terminal
                            .default_cwd
                            .clone()
                            .unwrap_or_else(|| "~".to_string()),
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.local_terminal.shell_profile",
                "settings_view.local_terminal.load_shell_profile_hint",
                vec![self.bool_row(
                    "settings_view.local_terminal.load_shell_profile",
                    "settings_view.local_terminal.load_shell_profile_hint",
                    settings.local_terminal.load_shell_profile,
                    set_load_shell_profile,
                    cx,
                )],
            ),
            self.settings_card(
                "settings_view.local_terminal.oh_my_posh",
                "settings_view.local_terminal.oh_my_posh_note",
                vec![
                    self.bool_row(
                        "settings_view.local_terminal.oh_my_posh_enable",
                        "settings_view.local_terminal.oh_my_posh_enable_hint",
                        settings.local_terminal.oh_my_posh_enabled,
                        set_oh_my_posh,
                        cx,
                    ),
                    self.value_row(
                        "settings_view.local_terminal.oh_my_posh_theme",
                        "settings_view.local_terminal.oh_my_posh_theme_hint",
                        settings
                            .local_terminal
                            .oh_my_posh_theme
                            .clone()
                            .unwrap_or_else(|| {
                                self.i18n
                                    .t("settings_view.local_terminal.oh_my_posh_theme_placeholder")
                            }),
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.local_terminal.shortcuts",
                "settings_view.local_terminal.custom_env_hint",
                vec![self.count_row(
                    "settings_view.local_terminal.custom_env",
                    "settings_view.local_terminal.custom_env_hint",
                    settings.local_terminal.custom_env_vars.len(),
                )],
            ),
            self.settings_card(
                "settings_view.local_terminal.available_shells",
                "settings_view.local_terminal.select_shell",
                vec![self.count_row(
                    "settings_view.local_terminal.available_shells",
                    "settings_view.local_terminal.select_shell",
                    settings.local_terminal.recent_shell_ids.len(),
                )],
            ),
        ]
    }

    fn settings_connections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.connections.title",
            "settings_view.connections.description",
            vec![
                self.value_row(
                    "settings_view.connections.default_username",
                    "settings_view.connections.description",
                    settings.connection_defaults.username.clone(),
                ),
                self.number_row(
                    "settings_view.connections.default_port",
                    "settings_view.connections.description",
                    settings.connection_defaults.port,
                    1,
                    1,
                    65535,
                    set_connection_default_port,
                    cx,
                ),
                self.number_row(
                    "settings_view.connections.idle_timeout.label",
                    "settings_view.connections.idle_timeout.hint",
                    settings.connection_pool.idle_timeout_secs,
                    300,
                    0,
                    3600,
                    set_connection_idle_timeout,
                    cx,
                ),
                self.value_row(
                    "settings_view.connections.groups.title",
                    "settings_view.connections.groups.description",
                    self.i18n.t("settings_view.connections.groups.new_placeholder"),
                ),
                self.value_row(
                    "settings_view.connections.ssh_config.title",
                    "settings_view.connections.ssh_config.description",
                    self.i18n.t("settings_view.connections.ssh_config.no_hosts"),
                ),
            ],
        )]
    }

    fn settings_ssh(&self) -> Vec<AnyElement> {
        vec![self.settings_card(
            "settings_view.ssh_keys.title",
            "settings_view.ssh_keys.description",
            vec![
                self.value_row(
                    "settings_view.ssh_keys.title",
                    "settings_view.ssh_keys.description",
                    self.i18n.t("settings_view.ssh_keys.no_keys"),
                ),
                self.value_row(
                    "ssh.auth.password",
                    "ssh.auth.password",
                    self.i18n.t("common.enabled"),
                ),
                self.value_row(
                    "ssh.auth.ssh_key",
                    "ssh.auth.default_key",
                    self.i18n.t("common.enabled"),
                ),
                self.value_row(
                    "ssh.auth.agent",
                    "ssh.auth.agent",
                    self.i18n.t("common.enabled"),
                ),
                self.value_row(
                    "ssh.auth.two_factor",
                    "ssh.auth.two_factor",
                    self.i18n.t("common.enabled"),
                ),
                self.value_row(
                    "ssh.form.agent_forwarding",
                    "ssh.form.agent_forwarding",
                    self.i18n.t("common.enabled"),
                ),
            ],
        )]
    }

    fn settings_reconnect(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.bool_row(
                "settings_view.reconnect.enabled",
                "settings_view.reconnect.enabled_hint",
                settings.reconnect.enabled,
                set_reconnect_enabled,
                cx,
            ),
            separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap(px(24.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text_heading))
                        .child(self.i18n.t("settings_view.reconnect.strategy")),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(672.0))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.number_row(
                            "settings_view.reconnect.max_attempts",
                            "settings_view.reconnect.max_attempts_hint",
                            settings.reconnect.max_attempts,
                            1,
                            1,
                            20,
                            set_reconnect_max_attempts,
                            cx,
                        ))
                        .child(self.number_row(
                            "settings_view.reconnect.base_delay",
                            "settings_view.reconnect.base_delay_hint",
                            settings.reconnect.base_delay_ms,
                            500,
                            500,
                            10000,
                            set_reconnect_base_delay,
                            cx,
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(672.0))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.number_row(
                            "settings_view.reconnect.max_delay",
                            "settings_view.reconnect.max_delay_hint",
                            settings.reconnect.max_delay_ms,
                            5000,
                            5000,
                            60000,
                            set_reconnect_max_delay,
                            cx,
                        )),
                )
                .child(
                    div()
                        .max_w(px(672.0))
                        .p(px(16.0))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                        .bg(rgb(self.tokens.ui.bg_card))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.reconnect.formula_hint")),
                )
                .into_any_element(),
        ]
    }

    fn settings_sftp(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let mut rows = vec![
            self.plain_settings_card(vec![
                self.number_row(
                    "settings_view.sftp.concurrent",
                    "settings_view.sftp.concurrent_hint",
                    settings.sftp.max_concurrent_transfers,
                    1,
                    1,
                    16,
                    set_sftp_concurrent,
                    cx,
                ),
                self.card_separator(),
                self.number_row(
                    "settings_view.sftp.directory_parallelism",
                    "settings_view.sftp.directory_parallelism_hint",
                    settings.sftp.directory_parallelism,
                    1,
                    1,
                    16,
                    set_sftp_directory_parallelism,
                    cx,
                ),
            ]),
            self.plain_settings_card(vec![self.bool_row(
                "settings_view.sftp.bandwidth",
                "settings_view.sftp.bandwidth_hint",
                settings.sftp.speed_limit_enabled,
                set_sftp_speed_limit_enabled,
                cx,
            )]),
        ];

        if settings.sftp.speed_limit_enabled {
            rows.push(self.plain_settings_card(vec![self.number_row(
                "settings_view.sftp.speed_limit",
                "settings_view.sftp.bandwidth_hint",
                settings.sftp.speed_limit_kbps,
                100,
                0,
                1024 * 1024,
                set_sftp_speed_limit_kbps,
                cx,
            )]));
        }

        rows.push(self.plain_settings_card(vec![self.cycle_row(
            "settings_view.sftp.conflict",
            "settings_view.sftp.conflict_hint",
            conflict_label(settings.sftp.conflict_action, &self.i18n),
            cycle_sftp_conflict,
            cx,
        )]));

        rows
    }

    fn settings_ide(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.ide.title",
            "settings_view.ide.description",
            vec![
                self.bool_row(
                    "settings_view.ide.auto_save",
                    "settings_view.ide.auto_save_hint",
                    settings.ide.auto_save,
                    set_ide_auto_save,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ide.word_wrap",
                    "settings_view.ide.word_wrap_hint",
                    settings.ide.word_wrap,
                    set_ide_word_wrap,
                    cx,
                ),
                self.number_row(
                    "settings_view.ide.font_size",
                    "settings_view.ide.font_size_hint",
                    settings.ide.font_size.unwrap_or(14),
                    1,
                    8,
                    40,
                    set_ide_font_size,
                    cx,
                ),
                self.number_row(
                    "settings_view.ide.line_height",
                    "settings_view.ide.line_height_hint",
                    (settings.ide.line_height.unwrap_or(1.5) * 100.0).round() as i64,
                    5,
                    80,
                    300,
                    set_ide_line_height_percent,
                    cx,
                ),
                self.cycle_row(
                    "settings_view.ide.agent_mode_label",
                    "settings_view.ide.agent_mode_hint",
                    ide_agent_label(settings.ide.agent_mode, &self.i18n),
                    cycle_ide_agent_mode,
                    cx,
                ),
                self.value_row(
                    "settings_view.ide.agent_title",
                    "settings_view.ide.agent_description",
                    self.i18n.t("settings_view.ide.agent_supported"),
                ),
            ],
        )]
    }

    fn settings_ai(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.ai.title",
            "settings_view.ai.description",
            vec![
                self.bool_row(
                    "settings_view.ai.enable",
                    "settings_view.ai.enable_hint",
                    settings.ai.enabled,
                    set_ai_enabled,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.privacy_notice",
                    "settings_view.ai.privacy_text",
                    settings.ai.enabled_confirmed,
                    set_ai_enabled_confirmed,
                    cx,
                ),
                self.value_row(
                    "settings_view.ai.base_url",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.base_url.clone(),
                ),
                self.value_row(
                    "settings_view.ai.model",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.model.clone(),
                ),
                self.count_row(
                    "settings_view.ai.provider_settings",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.providers.len(),
                ),
                self.value_row(
                    "settings_view.ai.default_model",
                    "settings_view.ai.provider_settings_summary",
                    settings
                        .ai
                        .active_model
                        .clone()
                        .unwrap_or_else(|| settings.ai.model.clone()),
                ),
                self.number_row(
                    "settings_view.ai.max_context",
                    "settings_view.ai.max_context_hint",
                    settings.ai.context_max_chars,
                    2000,
                    2000,
                    32000,
                    set_ai_context_max_chars,
                    cx,
                ),
                self.number_row(
                    "settings_view.ai.buffer_history",
                    "settings_view.ai.buffer_history_hint",
                    settings.ai.context_visible_lines,
                    20,
                    20,
                    1000,
                        set_ai_context_lines,
                        cx,
                    ),
                self.bool_row(
                    "settings_view.ai.context_source_ide",
                    "settings_view.ai.context_source_ide_hint",
                    settings.ai.context_sources.ide,
                    set_ai_context_source_ide,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.context_source_sftp",
                    "settings_view.ai.context_source_sftp_hint",
                    settings.ai.context_sources.sftp,
                    set_ai_context_source_sftp,
                    cx,
                ),
                self.cycle_row(
                    "settings_view.ai.reasoning_title",
                    "settings_view.ai.reasoning_hint",
                    ai_thinking_label(settings.ai.thinking_style),
                    cycle_ai_thinking,
                    cx,
                ),
                self.cycle_row(
                    "settings_view.ai.reasoning_title",
                    "settings_view.ai.reasoning_hint",
                    ai_reasoning_label(settings.ai.reasoning_effort),
                        cycle_ai_reasoning,
                        cx,
                    ),
                self.bool_row(
                    "settings_view.ai.memory_enabled",
                    "settings_view.ai.memory_enabled_hint",
                    settings.ai.memory.enabled,
                    set_ai_memory_enabled,
                    cx,
                ),
                self.value_row(
                    "settings_view.ai.custom_system_prompt",
                    "settings_view.ai.system_prompt_hint",
                    if settings.ai.custom_system_prompt.trim().is_empty() {
                        self.i18n.t("settings_view.ai.system_prompt_placeholder")
                    } else {
                        settings.ai.custom_system_prompt.clone()
                    },
                ),
                self.value_row(
                    "settings_view.ai.memory_title",
                    "settings_view.ai.memory_hint",
                    if settings.ai.memory.content.trim().is_empty() {
                        self.i18n.t("settings_view.ai.memory_placeholder")
                    } else {
                        settings.ai.memory.content.clone()
                    },
                ),
                self.bool_row(
                    "settings_view.ai.tool_use_enabled",
                    "settings_view.ai.tool_use_enabled_hint",
                    settings.ai.tool_use.enabled,
                    set_ai_tool_use_enabled,
                    cx,
                ),
                self.number_row(
                    "settings_view.ai.tool_use_max_rounds",
                    "settings_view.ai.tool_use_max_rounds_hint",
                    settings.ai.tool_use.max_rounds.unwrap_or(10),
                    1,
                    1,
                    30,
                    set_ai_tool_use_max_rounds,
                    cx,
                ),
                self.count_row(
                    "settings_view.ai.tool_use_policy_summary",
                    "settings_view.ai.tool_use_approve_hint",
                    settings.ai.tool_use.auto_approve_tools.len(),
                ),
                self.count_row(
                    "settings_view.mcp.title",
                    "settings_view.mcp.description",
                    settings.ai.mcp_servers.len(),
                ),
                self.value_row(
                    "settings_view.ai.embedding_title",
                    "settings_view.ai.embedding_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
                self.count_row(
                    "settings_view.ai.execution_profiles",
                    "settings_view.ai.execution_profiles_hint",
                    settings
                        .ai
                        .execution_profiles
                        .get("profiles")
                        .and_then(|profiles| profiles.as_array())
                        .map(Vec::len)
                        .unwrap_or(0),
                ),
            ],
        )]
    }

    fn settings_knowledge(&self) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.knowledge.title",
            "settings_view.knowledge.description",
            vec![
                self.value_row(
                    "settings_view.knowledge.semantic_search",
                    "settings_view.knowledge.semantic_search_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
                self.value_row(
                    "settings_view.knowledge.keyword_search_ready",
                    "settings_view.knowledge.description",
                    self.i18n.t("common.enabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.collections",
                    "settings_view.knowledge.create_description",
                    self.i18n.t("settings_view.knowledge.no_collections"),
                ),
                self.value_row(
                    "settings_view.knowledge.import_files",
                    "settings_view.knowledge.file_filter_documents",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.generate_embeddings",
                    "settings_view.knowledge.semantic_search_description",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.configure_embeddings",
                    "settings_view.ai.embedding_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
            ],
        )]
    }

    fn settings_keybindings(&self) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.keybindings.title",
            "settings_view.keybindings.description",
            vec![
                self.value_row(
                    "settings_view.keybindings.modified",
                    "settings_view.keybindings.intl_keyboard_note",
                    settings.keybindings.overrides.len().to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.import",
                    "settings_view.keybindings.import_invalid",
                    self.i18n.t("settings_view.keybindings.default_value"),
                ),
                self.value_row(
                    "settings_view.keybindings.export",
                    "settings_view.keybindings.export_error",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.keybindings.reset_all",
                    "settings_view.keybindings.reset_all_confirm",
                    self.i18n.t("settings_view.keybindings.default_value"),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.newTerminal",
                    "settings_view.keybindings.scope_global",
                    "Cmd+T".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.closeTab",
                    "settings_view.keybindings.scope_global",
                    "Cmd+W".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.settings",
                    "settings_view.keybindings.scope_global",
                    "Cmd+,".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.split.horizontal",
                    "settings_view.keybindings.scope_split",
                    "Cmd+Shift+E".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.split.vertical",
                    "settings_view.keybindings.scope_split",
                    "Cmd+Shift+D".to_string(),
                ),
            ],
        )]
    }

    fn settings_help(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.settings_card(
                "settings_view.help.version_info",
                "settings_view.help.description",
                vec![
                    self.value_row(
                        "settings_view.help.app_name",
                        "settings_view.help.version_info",
                        "OxideTerm Native".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.version",
                        "settings_view.help.version_info",
                        env!("CARGO_PKG_VERSION").to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.license",
                        "settings_view.help.resources",
                        "GPL-3.0-only".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.portable_mode",
                        "settings_view.help.portable_mode_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                    self.cycle_row(
                        "settings_view.help.update_channel",
                        "settings_view.help.update_channel_hint",
                        update_channel_label(settings.general.update_channel, &self.i18n),
                        cycle_update_channel,
                        cx,
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.shortcuts",
                "settings_view.help.resources",
                vec![
                    self.value_row(
                        "settings_view.help.shortcut_new_tab",
                        "settings_view.help.category_app",
                        "Cmd+T".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_close_tab",
                        "settings_view.help.category_app",
                        "Cmd+W".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_find",
                        "settings_view.help.category_terminal",
                        "Cmd+F".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_h",
                        "settings_view.help.category_split",
                        "Cmd+Shift+E".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_v",
                        "settings_view.help.category_split",
                        "Cmd+Shift+D".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_settings",
                        "settings_view.help.category_app",
                        "Cmd+,".to_string(),
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.diagnostics",
                "settings_view.help.open_logs_hint",
                vec![
                    self.value_row(
                        "settings_view.help.open_logs",
                        "settings_view.help.open_logs_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.memory_diagnostics_title",
                        "settings_view.help.memory_diagnostics_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.check_update",
                        "settings_view.help.updates_manual_only_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                ],
            ),
        ]
    }

    fn cycle_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value: String,
        cycle: fn(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = button(&self.tokens, value, crate::ui::ButtonTone::Secondary)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(cycle, cx);
                }),
            )
            .into_any_element();
        self.setting_row(label_key, hint_key, control)
    }

    fn language_label(&self, language: Language) -> String {
        match language {
            Language::De => "Deutsch",
            Language::En => "English",
            Language::EsEs => "Español (España)",
            Language::FrFr => "Français (France)",
            Language::It => "Italiano",
            Language::Ko => "한국어",
            Language::PtBr => "Português (Brasil)",
            Language::Vi => "Tiếng Việt",
            Language::Ja => "日本語",
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁體中文",
        }
        .to_string()
    }
}

fn set_terminal_cursor_blink(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.cursor_blink = value;
}

fn set_show_fps_overlay(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.show_fps_overlay = value;
}

fn set_paste_protection(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.paste_protection = value;
}

fn set_smart_copy(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.smart_copy = value;
}

fn set_copy_on_select(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.copy_on_select = value;
}

fn set_osc52_clipboard(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.osc52_clipboard = value;
}

fn set_middle_click_paste(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.middle_click_paste = value;
}

fn set_selection_requires_shift(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.selection_requires_shift = value;
}

fn compact_decimal(value: f64) -> String {
    let text = format!("{value:.1}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn font_family_options() -> &'static [FontFamily] {
    &[
        FontFamily::Jetbrains,
        FontFamily::Meslo,
        FontFamily::Maple,
        FontFamily::Cascadia,
        FontFamily::Consolas,
        FontFamily::Menlo,
        FontFamily::Custom,
    ]
}

fn terminal_encoding_options() -> &'static [TerminalEncoding] {
    &[
        TerminalEncoding::Utf8,
        TerminalEncoding::Gbk,
        TerminalEncoding::Gb18030,
        TerminalEncoding::Big5,
        TerminalEncoding::ShiftJis,
        TerminalEncoding::EucJp,
        TerminalEncoding::EucKr,
        TerminalEncoding::Windows1252,
    ]
}

fn adaptive_renderer_options() -> &'static [AdaptiveRendererMode] {
    &[
        AdaptiveRendererMode::Auto,
        AdaptiveRendererMode::Always60,
        AdaptiveRendererMode::Off,
    ]
}

fn cursor_style_options() -> &'static [SettingsCursorStyle] {
    &[
        SettingsCursorStyle::Block,
        SettingsCursorStyle::Underline,
        SettingsCursorStyle::Bar,
    ]
}

fn density_options() -> &'static [UiDensity] {
    &[
        UiDensity::Compact,
        UiDensity::Comfortable,
        UiDensity::Spacious,
    ]
}

fn animation_options() -> &'static [AnimationSpeed] {
    &[
        AnimationSpeed::Off,
        AnimationSpeed::Reduced,
        AnimationSpeed::Normal,
        AnimationSpeed::Fast,
    ]
}

fn background_fit_options() -> &'static [BackgroundFit] {
    &[
        BackgroundFit::Cover,
        BackgroundFit::Contain,
        BackgroundFit::Fill,
        BackgroundFit::Tile,
    ]
}

fn background_tab_options() -> &'static [(&'static str, &'static str, LucideIcon)] {
    &[
        (
            "terminal",
            "settings_view.terminal.bg_tab_terminal",
            LucideIcon::Terminal,
        ),
        (
            "local_terminal",
            "settings_view.terminal.bg_tab_local",
            LucideIcon::Monitor,
        ),
        (
            "sftp",
            "settings_view.terminal.bg_tab_sftp",
            LucideIcon::FolderInput,
        ),
        (
            "forwards",
            "settings_view.terminal.bg_tab_forwards",
            LucideIcon::ArrowLeftRight,
        ),
        (
            "settings",
            "settings_view.terminal.bg_tab_settings",
            LucideIcon::Settings,
        ),
        (
            "ide",
            "settings_view.terminal.bg_tab_ide",
            LucideIcon::Code2,
        ),
        (
            "connection_monitor",
            "settings_view.terminal.bg_tab_monitor",
            LucideIcon::Activity,
        ),
        (
            "connection_pool",
            "settings_view.terminal.bg_tab_connections",
            LucideIcon::Network,
        ),
        (
            "topology",
            "settings_view.terminal.bg_tab_topology",
            LucideIcon::Network,
        ),
        (
            "file_manager",
            "settings_view.terminal.bg_tab_files",
            LucideIcon::Folder,
        ),
        (
            "session_manager",
            "settings_view.terminal.bg_tab_sessions",
            LucideIcon::ListTree,
        ),
        (
            "launcher",
            "settings_view.terminal.bg_tab_launcher",
            LucideIcon::Rocket,
        ),
        (
            "plugin_manager",
            "settings_view.terminal.bg_tab_plugins",
            LucideIcon::Puzzle,
        ),
    ]
}

fn theme_display_name(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn set_terminal_scrollback(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.scrollback = value;
}

fn set_buffer_max_lines(settings: &mut PersistedSettings, value: i64) {
    settings.buffer.max_lines = value;
}

fn set_load_shell_profile(settings: &mut PersistedSettings, value: bool) {
    settings.local_terminal.load_shell_profile = value;
}

fn set_oh_my_posh(settings: &mut PersistedSettings, value: bool) {
    settings.local_terminal.oh_my_posh_enabled = value;
}

fn set_connection_default_port(settings: &mut PersistedSettings, value: i64) {
    settings.connection_defaults.port = value;
}

fn set_connection_idle_timeout(settings: &mut PersistedSettings, value: i64) {
    settings.connection_pool.idle_timeout_secs = value;
}

fn set_reconnect_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.reconnect.enabled = value;
}

fn set_reconnect_max_attempts(settings: &mut PersistedSettings, value: i64) {
    settings.reconnect.max_attempts = value;
}

fn set_reconnect_base_delay(settings: &mut PersistedSettings, value: i64) {
    settings.reconnect.base_delay_ms = value;
}

fn set_reconnect_max_delay(settings: &mut PersistedSettings, value: i64) {
    settings.reconnect.max_delay_ms = value;
}

fn set_sftp_concurrent(settings: &mut PersistedSettings, value: i64) {
    settings.sftp.max_concurrent_transfers = value;
}

fn set_sftp_directory_parallelism(settings: &mut PersistedSettings, value: i64) {
    settings.sftp.directory_parallelism = value;
}

fn set_sftp_speed_limit_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.sftp.speed_limit_enabled = value;
}

fn set_sftp_speed_limit_kbps(settings: &mut PersistedSettings, value: i64) {
    settings.sftp.speed_limit_kbps = value;
}

fn set_ide_auto_save(settings: &mut PersistedSettings, value: bool) {
    settings.ide.auto_save = value;
}

fn set_ide_word_wrap(settings: &mut PersistedSettings, value: bool) {
    settings.ide.word_wrap = value;
}

fn set_ide_font_size(settings: &mut PersistedSettings, value: i64) {
    settings.ide.font_size = Some(value);
}

fn set_ide_line_height_percent(settings: &mut PersistedSettings, value: i64) {
    settings.ide.line_height = Some(value as f64 / 100.0);
}

fn set_ai_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.ai.enabled = value;
}

fn set_ai_enabled_confirmed(settings: &mut PersistedSettings, value: bool) {
    settings.ai.enabled_confirmed = value;
}

fn set_ai_context_max_chars(settings: &mut PersistedSettings, value: i64) {
    settings.ai.context_max_chars = value;
}

fn set_ai_context_lines(settings: &mut PersistedSettings, value: i64) {
    settings.ai.context_visible_lines = value;
}

fn set_ai_context_source_ide(settings: &mut PersistedSettings, value: bool) {
    settings.ai.context_sources.ide = value;
}

fn set_ai_context_source_sftp(settings: &mut PersistedSettings, value: bool) {
    settings.ai.context_sources.sftp = value;
}

fn set_ai_memory_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.ai.memory.enabled = value;
}

fn set_ai_tool_use_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.ai.tool_use.enabled = value;
}

fn set_ai_tool_use_max_rounds(settings: &mut PersistedSettings, value: i64) {
    settings.ai.tool_use.max_rounds = Some(value);
}

fn set_command_bar_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.enabled = value;
}

fn set_command_bar_legacy_toolbar(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.show_legacy_toolbar = value;
}

fn set_quick_commands_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.quick_commands_enabled = value;
}

fn set_quick_commands_confirm(settings: &mut PersistedSettings, value: bool) {
    settings
        .terminal
        .command_bar
        .quick_commands_confirm_before_run = value;
}

fn set_quick_commands_toast(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.quick_commands_show_toast = value;
}

fn set_autosuggest_local_history(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.autosuggest.local_shell_history = value;
}

fn set_command_marks_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_marks.enabled = value;
}

fn set_command_marks_hover_actions(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_marks.show_hover_actions = value;
}

fn set_in_band_transfer_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.in_band_transfer.enabled = value;
}

fn set_in_band_transfer_allow_directory(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.in_band_transfer.allow_directory = value;
}

fn set_in_band_transfer_max_chunk_bytes(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.in_band_transfer.max_chunk_bytes = value;
}

fn set_in_band_transfer_max_file_count(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.in_band_transfer.max_file_count = value;
}

fn set_in_band_transfer_max_total_mb(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.in_band_transfer.max_total_bytes = value * 1024 * 1024;
}

fn set_terminal_background_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.background_enabled = value;
}

fn language_options() -> [Language; 11] {
    [
        Language::De,
        Language::En,
        Language::EsEs,
        Language::FrFr,
        Language::It,
        Language::Ko,
        Language::PtBr,
        Language::Vi,
        Language::Ja,
        Language::ZhCn,
        Language::ZhTw,
    ]
}

fn cycle_update_channel(settings: &mut PersistedSettings) {
    settings.general.update_channel = match settings.general.update_channel {
        UpdateChannel::Stable => UpdateChannel::Beta,
        UpdateChannel::Beta => UpdateChannel::Stable,
    };
}

fn cycle_sftp_conflict(settings: &mut PersistedSettings) {
    settings.sftp.conflict_action = match settings.sftp.conflict_action {
        ConflictAction::Ask => ConflictAction::Overwrite,
        ConflictAction::Overwrite => ConflictAction::Skip,
        ConflictAction::Skip => ConflictAction::Rename,
        ConflictAction::Rename => ConflictAction::Ask,
    };
}

fn cycle_ide_agent_mode(settings: &mut PersistedSettings) {
    settings.ide.agent_mode = match settings.ide.agent_mode {
        IdeAgentMode::Ask => IdeAgentMode::Enabled,
        IdeAgentMode::Enabled => IdeAgentMode::Disabled,
        IdeAgentMode::Disabled => IdeAgentMode::Ask,
    };
}

fn cycle_ai_thinking(settings: &mut PersistedSettings) {
    settings.ai.thinking_style = match settings.ai.thinking_style {
        AiThinkingStyle::Detailed => AiThinkingStyle::Compact,
        AiThinkingStyle::Compact => AiThinkingStyle::Detailed,
    };
}

fn cycle_ai_reasoning(settings: &mut PersistedSettings) {
    settings.ai.reasoning_effort = match settings.ai.reasoning_effort {
        AiReasoningEffort::None => AiReasoningEffort::Minimal,
        AiReasoningEffort::Minimal => AiReasoningEffort::Low,
        AiReasoningEffort::Low => AiReasoningEffort::Medium,
        AiReasoningEffort::Medium => AiReasoningEffort::High,
        AiReasoningEffort::High => AiReasoningEffort::Xhigh,
        AiReasoningEffort::Xhigh => AiReasoningEffort::Auto,
        AiReasoningEffort::Auto => AiReasoningEffort::None,
    };
}

fn update_channel_label(channel: UpdateChannel, i18n: &I18n) -> String {
    match channel {
        UpdateChannel::Stable => i18n.t("settings_view.help.channel_stable"),
        UpdateChannel::Beta => i18n.t("settings_view.help.channel_beta"),
    }
}

fn terminal_encoding_label(encoding: TerminalEncoding) -> String {
    match encoding {
        TerminalEncoding::Utf8 => "UTF-8",
        TerminalEncoding::Gbk => "GBK",
        TerminalEncoding::Gb18030 => "GB18030",
        TerminalEncoding::Big5 => "Big5",
        TerminalEncoding::ShiftJis => "Shift_JIS",
        TerminalEncoding::EucJp => "EUC-JP",
        TerminalEncoding::EucKr => "EUC-KR",
        TerminalEncoding::Windows1252 => "Windows-1252",
    }
    .to_string()
}

fn adaptive_renderer_label(mode: AdaptiveRendererMode, i18n: &I18n) -> String {
    match mode {
        AdaptiveRendererMode::Auto => i18n.t("settings_view.terminal.adaptive_renderer_auto"),
        AdaptiveRendererMode::Always60 => {
            i18n.t("settings_view.terminal.adaptive_renderer_always60")
        }
        AdaptiveRendererMode::Off => i18n.t("settings_view.terminal.adaptive_renderer_off"),
    }
}

fn cursor_style_label(style: SettingsCursorStyle, i18n: &I18n) -> String {
    match style {
        SettingsCursorStyle::Block => i18n.t("settings_view.terminal.cursor_block"),
        SettingsCursorStyle::Underline => i18n.t("settings_view.terminal.cursor_underline"),
        SettingsCursorStyle::Bar => i18n.t("settings_view.terminal.cursor_bar"),
    }
}

fn background_fit_label(fit: BackgroundFit, i18n: &I18n) -> String {
    match fit {
        BackgroundFit::Cover => i18n.t("settings_view.terminal.bg_fit_cover"),
        BackgroundFit::Contain => i18n.t("settings_view.terminal.bg_fit_contain"),
        BackgroundFit::Fill => i18n.t("settings_view.terminal.bg_fit_fill"),
        BackgroundFit::Tile => i18n.t("settings_view.terminal.bg_fit_tile"),
    }
}

fn conflict_label(action: ConflictAction, i18n: &I18n) -> String {
    match action {
        ConflictAction::Ask => i18n.t("settings_view.sftp.conflict_ask"),
        ConflictAction::Overwrite => i18n.t("settings_view.sftp.conflict_overwrite"),
        ConflictAction::Skip => i18n.t("settings_view.sftp.conflict_skip"),
        ConflictAction::Rename => i18n.t("settings_view.sftp.conflict_rename"),
    }
}

fn ide_agent_label(mode: IdeAgentMode, i18n: &I18n) -> String {
    match mode {
        IdeAgentMode::Ask => i18n.t("settings_view.ide.agent_mode_ask"),
        IdeAgentMode::Enabled => i18n.t("settings_view.ide.agent_mode_enabled"),
        IdeAgentMode::Disabled => i18n.t("settings_view.ide.agent_mode_disabled"),
    }
}

fn font_family_label(family: FontFamily) -> String {
    match family {
        FontFamily::Jetbrains => "JetBrains Mono NF (Subset) ✓".to_string(),
        FontFamily::Meslo => "MesloLGS NF (Subset) ✓".to_string(),
        FontFamily::Maple => "Maple Mono NF CN (Subset) ✓".to_string(),
        FontFamily::Cascadia => "Cascadia Code".to_string(),
        FontFamily::Consolas => "Consolas".to_string(),
        FontFamily::Menlo => "Menlo".to_string(),
        FontFamily::Custom => "Custom...".to_string(),
    }
}

fn density_label(density: UiDensity, i18n: &I18n) -> String {
    match density {
        UiDensity::Compact => i18n.t("settings_view.appearance.density_compact"),
        UiDensity::Comfortable => i18n.t("settings_view.appearance.density_comfortable"),
        UiDensity::Spacious => i18n.t("settings_view.appearance.density_spacious"),
    }
}

fn animation_label(speed: AnimationSpeed, i18n: &I18n) -> String {
    match speed {
        AnimationSpeed::Off => i18n.t("settings_view.appearance.animation_off"),
        AnimationSpeed::Reduced => i18n.t("settings_view.appearance.animation_reduced"),
        AnimationSpeed::Normal => i18n.t("settings_view.appearance.animation_normal"),
        AnimationSpeed::Fast => i18n.t("settings_view.appearance.animation_fast"),
    }
}

fn ai_thinking_label(style: AiThinkingStyle) -> String {
    format!("{style:?}")
}

fn ai_reasoning_label(effort: AiReasoningEffort) -> String {
    format!("{effort:?}")
}
