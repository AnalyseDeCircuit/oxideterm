mod actions;
mod ime;
mod new_connection;
mod pane_tree;
mod session_manager;
mod settings;
mod sidebar;
mod tabs;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use gpui::{
    AnyElement, App, Context, CursorStyle, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement, Pixels,
    Render, RenderImage, Rgba, ScrollWheelEvent, SharedString, Styled, StyledImage, Timer, Window,
    div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_connections::ConnectionStore;
use oxideterm_gpui_platform::{
    rendering::detect_graphics,
    vibrancy::{NativeVibrancyMode, apply_window_vibrancy},
};
use oxideterm_gpui_terminal::{
    BackgroundImageRenderCache, TerminalBackgroundFit, TerminalBackgroundPreferences,
    TerminalHighlightRenderMode, TerminalHighlightRule as UiHighlightRule, TerminalPane,
    TerminalPasteLabels, TerminalUiPreferences, TerminalUiTheme,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_render_policy::{
    DetectedGraphics, EffectiveRenderPolicy, RenderProfile, compute_render_policy,
};
use oxideterm_settings::{
    BackgroundFit, CursorStyle as SettingsCursorStyle, FrostedGlassMode, HighlightRuleRenderMode,
    Language, PersistedSettings, SettingsStore, TerminalEncoding as SettingsTerminalEncoding,
    default_settings_path,
};
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionPoolConfig, NodeId, NodeRouter, SshConfig, SshConnectionRegistry,
};
use oxideterm_terminal::{
    LocalPtyConfig, ShellInfo, SshSessionConfig, TerminalCursorShape,
    TerminalEncoding as SessionTerminalEncoding, scan_shells,
};
use oxideterm_theme::{ThemeTokens, UiRadii, theme_by_id};
use oxideterm_workspace::{
    MAX_PANES_PER_TAB, PaneId, PaneNode, SplitDirection, Tab, TabId, TabKind, TabTitleSource,
    TerminalSessionId, adjusted_split_sizes, balanced_sizes,
};

use self::actions::SearchBarState;
use self::ime::{WorkspaceImeElement, keystroke_commits_platform_text};
use self::new_connection::{
    HostKeyChallenge, KeyboardInteractiveChallenge, NativeSshPromptHandler, NewConnectionForm,
    NewConnectionSelect, SavedConnectionPromptAction, SshAuthTab, SshConnectionWorkerResult,
};
use self::pane_tree::SplitDrag;
use self::session_manager::SessionManagerState;
use self::sidebar::SidebarSection;
use crate::assets::LucideIcon;
use crate::{
    ClosePane, CloseSearch, CloseTab, Copy, Find, FindNext, FindPrev, GoToTab1, GoToTab2, GoToTab3,
    GoToTab4, GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewTerminal, NextTab, OpenSettings,
    Paste, PrevTab, SplitHorizontal, SplitVertical, SwitchLocaleChinese, SwitchLocaleEnglish,
    SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian, SwitchLocaleJapanese,
    SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese,
};
use oxideterm_gpui_settings_view::{
    ActiveSurface, SettingsInput, SettingsSelect, SettingsSlider, SettingsTab, TerminalSettingsPage,
};
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId};

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
    panes: HashMap<PaneId, gpui::Entity<TerminalPane>>,
    tab_scroll_x: f32,
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    search: SearchBarState,
    split_drag: Option<SplitDrag>,
    sidebar_resizing: bool,
    sidebar_collapsed: bool,
    sidebar_width: f32,
    needs_active_pane_focus: bool,
    active_sidebar_section: SidebarSection,
    active_surface: ActiveSurface,
    active_settings_tab: SettingsTab,
    terminal_settings_page: TerminalSettingsPage,
    open_settings_select: Option<SettingsSelect>,
    select_anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    text_input_anchors: HashMap<TextInputAnchorId, TextInputAnchor>,
    ime_marked_text: Option<String>,
    focused_settings_input: Option<SettingsInput>,
    settings_input_draft: String,
    settings_slider_drag: Option<SettingsSlider>,
    background_blur_preview: Option<i64>,
    background_blur_commit_generation: u64,
    background_cache_poll_scheduled: bool,
    new_connection_form: Option<NewConnectionForm>,
    editing_saved_connection_id: Option<String>,
    saved_connection_prompt_action: Option<SavedConnectionPromptAction>,
    open_new_connection_select: Option<NewConnectionSelect>,
    new_connection_caret_visible: bool,
    host_key_challenge: Option<HostKeyChallenge>,
    keyboard_interactive_challenge: Option<KeyboardInteractiveChallenge>,
    ssh_worker_tx: std::sync::mpsc::Sender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    ssh_registry: SshConnectionRegistry,
    node_router: NodeRouter,
    next_ssh_node_id: u64,
    i18n: I18n,
    tokens: ThemeTokens,
    detected_graphics: DetectedGraphics,
    render_profile_override: Option<RenderProfile>,
    render_policy: EffectiveRenderPolicy,
    applied_vibrancy_mode: NativeVibrancyMode,
    background_image_cache: BackgroundImageRenderCache,
    settings_store: SettingsStore,
    connection_store: ConnectionStore,
    session_manager: SessionManagerState,
    settings_connection_new_group: String,
    settings_selected_ssh_hosts: HashSet<String>,
    settings_connection_status: Option<String>,
    local_shells: Vec<ShellInfo>,
}

impl WorkspaceApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let settings_store = SettingsStore::load_default()?;
        let connection_store = ConnectionStore::load(default_connections_path())?;
        let settings = settings_store.settings().clone();
        let local_shells = scan_shells();
        let tokens = tokens_from_settings(&settings);
        let detected_graphics = detect_graphics(window);
        let render_profile_override = render_profile_from_env();
        let render_policy = compute_render_policy(
            render_profile_override.unwrap_or(settings.appearance.render_profile),
            &detected_graphics,
        );
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig {
            idle_timeout: Some(Duration::from_secs(
                settings.connection_pool.idle_timeout_secs as u64,
            )),
            ..ConnectionPoolConfig::default()
        });
        let node_router = NodeRouter::new(ssh_registry.clone());
        let (ssh_worker_tx, ssh_worker_rx) = std::sync::mpsc::channel();
        let initial_vibrancy_mode = effective_vibrancy_mode(&settings, &render_policy);
        let mut background_image_cache = BackgroundImageRenderCache::default();
        background_image_cache.set_byte_limit(render_policy.image_cache_bytes);
        let workspace = Self {
            focus_handle,
            tabs: Vec::new(),
            active_tab_id: None,
            panes: HashMap::new(),
            tab_scroll_x: 0.0,
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            search: SearchBarState::default(),
            split_drag: None,
            sidebar_resizing: false,
            sidebar_collapsed: settings.sidebar_ui.collapsed,
            sidebar_width: settings.sidebar_ui.width as f32,
            needs_active_pane_focus: false,
            active_sidebar_section: SidebarSection::from_settings_key(
                &settings.sidebar_ui.active_section,
            ),
            active_surface: ActiveSurface::Terminal,
            active_settings_tab: SettingsTab::General,
            terminal_settings_page: TerminalSettingsPage::Display,
            open_settings_select: None,
            select_anchors: HashMap::new(),
            text_input_anchors: HashMap::new(),
            ime_marked_text: None,
            focused_settings_input: None,
            settings_input_draft: String::new(),
            settings_slider_drag: None,
            background_blur_preview: None,
            background_blur_commit_generation: 0,
            background_cache_poll_scheduled: false,
            new_connection_form: None,
            editing_saved_connection_id: None,
            saved_connection_prompt_action: None,
            open_new_connection_select: None,
            new_connection_caret_visible: true,
            host_key_challenge: None,
            keyboard_interactive_challenge: None,
            ssh_worker_tx,
            ssh_worker_rx,
            ssh_registry,
            node_router,
            next_ssh_node_id: 1,
            i18n: I18n::new(locale_from_settings(settings.general.language)),
            tokens,
            detected_graphics,
            render_profile_override,
            render_policy,
            applied_vibrancy_mode: initial_vibrancy_mode,
            background_image_cache,
            settings_store,
            connection_store,
            session_manager: SessionManagerState::default(),
            settings_connection_new_group: String::new(),
            settings_selected_ssh_hosts: HashSet::new(),
            settings_connection_status: None,
            local_shells,
        };
        let _ = apply_window_vibrancy(window, initial_vibrancy_mode);
        let window_handle = window
            .window_handle()
            .downcast::<Self>()
            .expect("workspace root window handle");
        cx.spawn(async move |_weak, cx| {
            loop {
                Timer::after(Duration::from_millis(530)).await;
                if window_handle
                    .update(cx, |workspace, window, cx| {
                        workspace.poll_ssh_worker_results(window, cx);
                        if workspace.new_connection_form.is_some()
                            || workspace.keyboard_interactive_challenge.is_some()
                            || workspace.focused_settings_input.is_some()
                            || workspace.session_manager.focused_input.is_some()
                        {
                            workspace.new_connection_caret_visible =
                                !workspace.new_connection_caret_visible;
                            cx.notify();
                        } else if !workspace.new_connection_caret_visible {
                            workspace.new_connection_caret_visible = true;
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Ok(workspace)
    }

    pub(crate) fn terminal_preferences_for_tab_kind(
        &self,
        kind: &TabKind,
    ) -> TerminalUiPreferences {
        self.terminal_preferences_for_background_key(tab_background_key(kind))
    }

    pub(crate) fn terminal_preferences_for_pane(&self, pane_id: PaneId) -> TerminalUiPreferences {
        let key = self
            .tabs
            .iter()
            .find_map(|tab| {
                tab.root_pane
                    .as_ref()
                    .is_some_and(|root| root.contains_pane(pane_id))
                    .then_some(tab_background_key(&tab.kind))
            })
            .unwrap_or("local_terminal");
        self.terminal_preferences_for_background_key(key)
    }

    fn terminal_preferences_for_background_key(
        &self,
        background_key: &str,
    ) -> TerminalUiPreferences {
        let settings = self.settings_store.settings();
        let terminal = &settings.terminal;
        TerminalUiPreferences {
            font_family: terminal
                .font_family
                .terminal_family_name(&terminal.custom_font_family),
            font_size: terminal.font_size as f32,
            line_height: terminal.line_height as f32,
            cursor_shape: match terminal.cursor_style {
                SettingsCursorStyle::Block => TerminalCursorShape::Block,
                SettingsCursorStyle::Underline => TerminalCursorShape::Underline,
                SettingsCursorStyle::Bar => TerminalCursorShape::Bar,
            },
            cursor_blink: terminal.cursor_blink,
            paste_protection: terminal.paste_protection,
            smart_copy: terminal.smart_copy,
            osc52_clipboard: terminal.osc52_clipboard,
            copy_on_select: terminal.copy_on_select,
            middle_click_paste: terminal.middle_click_paste,
            selection_requires_shift: terminal.selection_requires_shift,
            bidi_enabled: terminal.unicode.bidi_enabled,
            terminal_encoding: session_terminal_encoding(terminal.terminal_encoding),
            render_policy: self.render_policy.clone(),
            background: self.terminal_background_preferences(background_key),
            paste_labels: TerminalPasteLabels {
                title_template: self.i18n.t("terminal.paste.title"),
                more_lines_template: self.i18n.t("terminal.paste.more_lines"),
                confirm: self.i18n.t("terminal.paste.confirm"),
                cancel: self.i18n.t("terminal.paste.cancel"),
                paste: self.i18n.t("terminal.paste.paste"),
            },
            highlight_rules: terminal
                .highlight_rules
                .iter()
                .map(|rule| UiHighlightRule {
                    id: rule.id.clone(),
                    pattern: rule.pattern.clone(),
                    is_regex: rule.is_regex,
                    case_sensitive: rule.case_sensitive,
                    foreground: rule.foreground.clone(),
                    background: rule.background.clone(),
                    render_mode: match rule.render_mode {
                        HighlightRuleRenderMode::Background => {
                            TerminalHighlightRenderMode::Background
                        }
                        HighlightRuleRenderMode::Underline => {
                            TerminalHighlightRenderMode::Underline
                        }
                        HighlightRuleRenderMode::Outline => TerminalHighlightRenderMode::Outline,
                    },
                    enabled: rule.enabled,
                    priority: rule.priority,
                })
                .collect(),
            theme: TerminalUiTheme::new(
                self.tokens.terminal.background,
                self.tokens.terminal.foreground,
                self.tokens.terminal.cursor,
            ),
        }
    }

    fn terminal_background_preferences(
        &self,
        background_key: &str,
    ) -> Option<TerminalBackgroundPreferences> {
        if !self.render_policy.allow_background_images {
            return None;
        }
        let terminal = &self.settings_store.settings().terminal;
        if !terminal.background_enabled
            || !terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == background_key)
        {
            return None;
        }
        let path = PathBuf::from(terminal.background_image.as_deref()?);
        if !path.exists() {
            return None;
        }
        Some(TerminalBackgroundPreferences {
            path,
            opacity: terminal.background_opacity.clamp(0.0, 1.0) as f32,
            blur: terminal.background_blur.clamp(0, 20) as f32,
            fit: terminal_background_fit(terminal.background_fit),
        })
    }
}

fn tab_background_key(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::LocalTerminal => "local_terminal",
        TabKind::SshTerminal => "terminal",
        TabKind::SessionManager => "session_manager",
        TabKind::Settings => "settings",
    }
}

fn terminal_background_fit(fit: BackgroundFit) -> TerminalBackgroundFit {
    match fit {
        BackgroundFit::Cover => TerminalBackgroundFit::Cover,
        BackgroundFit::Contain => TerminalBackgroundFit::Contain,
        BackgroundFit::Fill => TerminalBackgroundFit::Fill,
        BackgroundFit::Tile => TerminalBackgroundFit::Tile,
    }
}

fn session_terminal_encoding(encoding: SettingsTerminalEncoding) -> SessionTerminalEncoding {
    match encoding {
        SettingsTerminalEncoding::Utf8 => SessionTerminalEncoding::Utf8,
        SettingsTerminalEncoding::Gbk => SessionTerminalEncoding::Gbk,
        SettingsTerminalEncoding::Gb18030 => SessionTerminalEncoding::Gb18030,
        SettingsTerminalEncoding::Big5 => SessionTerminalEncoding::Big5,
        SettingsTerminalEncoding::ShiftJis => SessionTerminalEncoding::ShiftJis,
        SettingsTerminalEncoding::EucJp => SessionTerminalEncoding::EucJp,
        SettingsTerminalEncoding::EucKr => SessionTerminalEncoding::EucKr,
        SettingsTerminalEncoding::Windows1252 => SessionTerminalEncoding::Windows1252,
    }
}

fn locale_from_settings(language: Language) -> Locale {
    match language {
        Language::De => Locale::De,
        Language::En => Locale::En,
        Language::EsEs => Locale::EsEs,
        Language::FrFr => Locale::FrFr,
        Language::It => Locale::It,
        Language::Ja => Locale::Ja,
        Language::Ko => Locale::Ko,
        Language::PtBr => Locale::PtBr,
        Language::Vi => Locale::Vi,
        Language::ZhCn => Locale::ZhCn,
        Language::ZhTw => Locale::ZhTw,
    }
}

fn settings_language_from_locale(locale: Locale) -> Language {
    match locale {
        Locale::De => Language::De,
        Locale::En => Language::En,
        Locale::EsEs => Language::EsEs,
        Locale::FrFr => Language::FrFr,
        Locale::It => Language::It,
        Locale::Ja => Language::Ja,
        Locale::Ko => Language::Ko,
        Locale::PtBr => Language::PtBr,
        Locale::Vi => Language::Vi,
        Locale::ZhCn => Language::ZhCn,
        Locale::ZhTw => Language::ZhTw,
    }
}

fn tokens_from_settings(settings: &PersistedSettings) -> ThemeTokens {
    let mut tokens = ThemeTokens::from_builtin(theme_by_id(&settings.terminal.theme));
    let radius = settings.appearance.border_radius as f32;
    tokens.radii = UiRadii {
        xs: (radius - 4.0).max(0.0),
        sm: (radius - 2.0).max(0.0),
        md: radius,
        lg: radius + 4.0,
        active_indicator: 2.0_f32.min(radius.max(1.0)),
    };
    tokens
}

fn native_vibrancy_mode(mode: FrostedGlassMode) -> NativeVibrancyMode {
    match mode {
        FrostedGlassMode::Off | FrostedGlassMode::Css => NativeVibrancyMode::Off,
        FrostedGlassMode::Native | FrostedGlassMode::System => NativeVibrancyMode::System,
        FrostedGlassMode::Mica => NativeVibrancyMode::Mica,
        FrostedGlassMode::Acrylic => NativeVibrancyMode::Acrylic,
    }
}

fn effective_vibrancy_mode(
    settings: &PersistedSettings,
    policy: &EffectiveRenderPolicy,
) -> NativeVibrancyMode {
    if policy.allow_vibrancy {
        native_vibrancy_mode(settings.appearance.frosted_glass)
    } else {
        NativeVibrancyMode::Off
    }
}

fn render_profile_from_env() -> Option<RenderProfile> {
    let value = std::env::var("OXIDETERM_RENDER_PROFILE").ok()?;
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "auto" => Some(RenderProfile::Auto),
        "quality" | "high-quality" | "high" => Some(RenderProfile::Quality),
        "low-power" | "lowpower" | "low" => Some(RenderProfile::LowPower),
        "compatibility" | "compat" | "safe" | "safe-mode" => Some(RenderProfile::Compatibility),
        _ => None,
    }
}

fn workspace_background(tokens: &ThemeTokens, mode: NativeVibrancyMode) -> Rgba {
    match mode {
        NativeVibrancyMode::Off => rgb(tokens.ui.bg),
        NativeVibrancyMode::System | NativeVibrancyMode::Mica | NativeVibrancyMode::Acrylic => {
            rgba((tokens.ui.bg << 8) | alpha_byte(tokens.metrics.window_vibrancy_tint_alpha))
        }
    }
}

fn alpha_byte(alpha: f32) -> u32 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

fn settings_ui_font_family(configured_family: &str) -> SharedString {
    css_font_family_head(configured_family).unwrap_or_else(|| SharedString::from("Inter"))
}

fn settings_mono_font_family(settings: &PersistedSettings) -> SharedString {
    SharedString::from(
        settings
            .terminal
            .font_family
            .terminal_family_name(&settings.terminal.custom_font_family),
    )
}

fn css_font_family_head(configured_family: &str) -> Option<SharedString> {
    configured_family
        .split(',')
        .map(|family| family.trim().trim_matches(['"', '\'']))
        .find(|family| !family.is_empty())
        .map(|family| SharedString::from(family.to_string()))
}

impl Focusable for WorkspaceApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WorkspaceApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_tab_titles(cx);
        let title = self
            .active_tab()
            .map(|tab| self.tab_display_title(tab))
            .unwrap_or_else(|| "OxideTerm".to_string());
        window.set_window_title(&SharedString::from(title));
        let vibrancy_mode =
            effective_vibrancy_mode(self.settings_store.settings(), &self.render_policy);
        if self.applied_vibrancy_mode != vibrancy_mode {
            let _ = apply_window_vibrancy(window, vibrancy_mode);
            self.applied_vibrancy_mode = vibrancy_mode;
        }
        if self.needs_active_pane_focus
            && self
                .active_tab()
                .is_some_and(|tab| !matches!(tab.kind, TabKind::Settings | TabKind::SessionManager))
            && !self.search.visible
            && self.new_connection_form.is_none()
            && let Some(pane) = self.active_pane()
        {
            self.needs_active_pane_focus = false;
            window.on_next_frame(move |window, cx| {
                pane.read(cx).focus(window);
            });
        }

        let content = if let Some(tab) = self.active_tab() {
            match (&tab.kind, &tab.root_pane) {
                (TabKind::Settings, _) => self.render_settings_surface(cx),
                (TabKind::SessionManager, _) => self.render_session_manager_surface(window, cx),
                (_, Some(root_pane)) => self.render_pane_tree(root_pane, cx),
                _ => self.render_empty_workspace(cx),
            }
        } else {
            self.render_empty_workspace(cx)
        };
        let content = self.wrap_content_background(
            content,
            self.active_tab().map(|tab| tab_background_key(&tab.kind)),
            cx,
        );

        div()
            .id("workspace-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(workspace_background(&self.tokens, vibrancy_mode))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.keyboard_interactive_challenge.is_some() {
                    if keystroke_commits_platform_text(&event.keystroke) {
                        return;
                    }
                    let _ = this.handle_keyboard_interactive_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.host_key_challenge.is_some() {
                    if event.keystroke.key.as_str() == "escape" {
                        this.cancel_host_key_challenge(cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.new_connection_form.is_some() {
                    if this.active_ime_target().is_some()
                        && keystroke_commits_platform_text(&event.keystroke)
                    {
                        return;
                    }
                    let _ = this.handle_new_connection_key(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this
                    .active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::SessionManager)
                    && this.session_manager.focused_input.is_some()
                {
                    if keystroke_commits_platform_text(&event.keystroke) {
                        return;
                    }
                    let _ = this.handle_session_manager_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                } else if this.active_surface == ActiveSurface::Settings
                    && this.focused_settings_input.is_some()
                {
                    if keystroke_commits_platform_text(&event.keystroke) {
                        return;
                    }
                    let _ = this.handle_settings_input_key(event, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_workspace_key(event, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_sidebar_resize(event, cx);
                this.update_split_drag(event, window, cx);
                this.update_settings_slider_drag(event, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    this.blur_text_inputs(cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_sidebar_resize(cx);
                    this.finish_split_drag(cx);
                    this.finish_settings_slider_drag(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| {
                let _ = this.create_local_terminal_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.close_active_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.next_tab(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| {
                this.next_tab(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitHorizontal, window, cx| {
                this.split_active_pane(SplitDirection::Horizontal, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitVertical, window, cx| {
                this.split_active_pane(SplitDirection::Vertical, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| {
                this.close_active_pane(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Copy, _window, cx| {
                if this.new_connection_form.is_none() {
                    this.copy(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, _window, cx| {
                if this.new_connection_form.is_some() {
                    this.paste_into_new_connection_field(cx);
                } else {
                    this.paste(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Find, window, cx| {
                this.open_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FindNext, _window, cx| {
                this.search_next(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FindPrev, _window, cx| {
                this.search_next(false, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseSearch, window, cx| {
                this.close_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_settings(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleEnglish, window, cx| {
                this.switch_locale(Locale::En, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleChinese, window, cx| {
                this.switch_locale(Locale::ZhCn, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SwitchLocaleTraditionalChinese, window, cx| {
                    this.switch_locale(Locale::ZhTw, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SwitchLocaleGerman, window, cx| {
                this.switch_locale(Locale::De, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleSpanish, window, cx| {
                this.switch_locale(Locale::EsEs, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleFrench, window, cx| {
                this.switch_locale(Locale::FrFr, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleItalian, window, cx| {
                this.switch_locale(Locale::It, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleJapanese, window, cx| {
                this.switch_locale(Locale::Ja, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchLocaleKorean, window, cx| {
                this.switch_locale(Locale::Ko, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SwitchLocalePortugueseBrazil, window, cx| {
                    this.switch_locale(Locale::PtBr, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SwitchLocaleVietnamese, window, cx| {
                this.switch_locale(Locale::Vi, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab1, window, cx| {
                this.go_to_tab(0, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab2, window, cx| {
                this.go_to_tab(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab3, window, cx| {
                this.go_to_tab(2, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab4, window, cx| {
                this.go_to_tab(3, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab5, window, cx| {
                this.go_to_tab(4, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab6, window, cx| {
                this.go_to_tab(5, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab7, window, cx| {
                this.go_to_tab(6, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab8, window, cx| {
                this.go_to_tab(7, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoToTab9, window, cx| {
                this.go_to_tab(8, window, cx);
            }))
            .child(self.render_title_bar())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(self.render_activity_bar(cx))
                    .when(!self.sidebar_collapsed, |layout| {
                        layout.child(self.render_sidebar_region(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_w(px(self.tokens.metrics.min_main_width))
                            .overflow_hidden()
                            .child(self.render_tab_bar(cx))
                            .when(self.search.visible, |main| {
                                main.child(self.render_search_bar(cx))
                            })
                            .child(
                                div().flex_1().relative().overflow_hidden().child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .right_0()
                                        .bottom_0()
                                        .child(content),
                                ),
                            ),
                    ),
            )
            .when(self.new_connection_form.is_some(), |root| {
                root.child(self.render_new_connection_modal(window, cx))
            })
            .when_some(
                self.render_new_connection_select_overlay(window, cx),
                |root, overlay| root.child(overlay),
            )
            .when(self.host_key_challenge.is_some(), |root| {
                root.child(self.render_host_key_dialog(cx))
            })
            .when(self.keyboard_interactive_challenge.is_some(), |root| {
                root.child(self.render_keyboard_interactive_dialog(cx))
            })
            .child(WorkspaceImeElement::new(
                cx.entity(),
                self.focus_handle.clone(),
            ))
    }
}

impl WorkspaceApp {
    fn wrap_content_background(
        &mut self,
        content: AnyElement,
        background_key: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(background_key) = background_key else {
            return content;
        };
        if matches!(background_key, "terminal" | "local_terminal") {
            return content;
        }
        let Some(background) = self.terminal_background_preferences(background_key) else {
            return content;
        };
        let blurred_image = self
            .background_image_cache
            .render_blurred_image(&background);
        if self.background_image_cache.has_pending() {
            self.schedule_background_cache_poll(cx);
        }

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(workspace_background_image_layer(background, blurred_image))
            .child(div().relative().size_full().child(content))
            .into_any_element()
    }

    fn schedule_background_cache_poll(&mut self, cx: &mut Context<Self>) {
        if self.background_cache_poll_scheduled {
            return;
        }
        self.background_cache_poll_scheduled = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, |this, cx| {
                this.background_cache_poll_scheduled = false;
                if this.background_image_cache.drain_completed() {
                    cx.notify();
                }
                if this.background_image_cache.has_pending() {
                    this.schedule_background_cache_poll(cx);
                }
            });
        })
        .detach();
    }
}

fn workspace_background_image_layer(
    background: TerminalBackgroundPreferences,
    blurred_image: Option<Arc<RenderImage>>,
) -> AnyElement {
    let image = if let Some(blurred_image) = blurred_image {
        gpui::img(blurred_image)
            .size_full()
            .object_fit(workspace_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .into_any_element()
    } else {
        gpui::img(background.path)
            .size_full()
            .object_fit(workspace_background_object_fit(background.fit))
            .opacity(background.opacity.clamp(0.0, 1.0))
            .with_fallback(|| div().size_full().into_any_element())
            .into_any_element()
    };

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .overflow_hidden()
        .child(image)
        .into_any_element()
}

fn workspace_background_object_fit(fit: TerminalBackgroundFit) -> ObjectFit {
    match fit {
        TerminalBackgroundFit::Cover => ObjectFit::Cover,
        TerminalBackgroundFit::Contain => ObjectFit::Contain,
        TerminalBackgroundFit::Fill => ObjectFit::Fill,
        TerminalBackgroundFit::Tile => ObjectFit::None,
    }
}

fn default_connections_path() -> PathBuf {
    default_settings_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("connections.json")
}
