mod actions;
mod new_connection;
mod pane_tree;
mod settings;
mod sidebar;
mod tabs;

use std::{collections::HashMap, time::Duration};

use anyhow::Result;
use gpui::{
    AnyElement, App, Context, CursorStyle, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render, Rgba,
    SharedString, Styled, Timer, Window, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_gpui_terminal::{TerminalPane, TerminalUiPreferences, TerminalUiTheme};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_settings::{Language, PersistedSettings, SettingsStore};
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionPoolConfig, NodeId, NodeRouter, SshConfig, SshConnectionRegistry,
};
use oxideterm_terminal::SshSessionConfig;
use oxideterm_theme::{ThemeTokens, UiRadii, theme_by_id};
use oxideterm_workspace::{
    MAX_PANES_PER_TAB, PaneId, PaneNode, SplitDirection, Tab, TabId, TabKind, TerminalSessionId,
    adjusted_split_sizes, balanced_sizes,
};

use self::actions::SearchBarState;
use self::new_connection::{
    HostKeyChallenge, KeyboardInteractiveChallenge, NativeSshPromptHandler, NewConnectionForm,
    SshConnectionWorkerResult,
};
use self::pane_tree::SplitDrag;
use self::settings::{ActiveSurface, SettingsSelect, SettingsTab, TerminalSettingsPage};
use self::sidebar::SidebarSection;
use crate::assets::LucideIcon;
use crate::ui::select::{OverlayAnchor, SelectAnchorId};
use crate::{
    ClosePane, CloseSearch, CloseTab, Copy, Find, FindNext, FindPrev, GoToTab1, GoToTab2, GoToTab3,
    GoToTab4, GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewTerminal, NextTab, OpenSettings,
    Paste, PrevTab, SplitHorizontal, SplitVertical, SwitchLocaleChinese, SwitchLocaleEnglish,
    SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian, SwitchLocaleJapanese,
    SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese,
};

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
    panes: HashMap<PaneId, gpui::Entity<TerminalPane>>,
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
    new_connection_form: Option<NewConnectionForm>,
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
    settings_store: SettingsStore,
}

impl WorkspaceApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        let focus_handle = cx.focus_handle();
        let settings_store = SettingsStore::load_default()?;
        let settings = settings_store.settings().clone();
        let tokens = tokens_from_settings(&settings);
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig {
            idle_timeout: Some(Duration::from_secs(
                settings.connection_pool.idle_timeout_secs as u64,
            )),
            ..ConnectionPoolConfig::default()
        });
        let node_router = NodeRouter::new(ssh_registry.clone());
        let (ssh_worker_tx, ssh_worker_rx) = std::sync::mpsc::channel();
        let mut workspace = Self {
            focus_handle,
            tabs: Vec::new(),
            active_tab_id: None,
            panes: HashMap::new(),
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
            new_connection_form: None,
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
            settings_store,
        };
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
        workspace.create_local_terminal_tab(window, cx)?;
        Ok(workspace)
    }

    pub(crate) fn terminal_preferences(&self) -> TerminalUiPreferences {
        let settings = self.settings_store.settings();
        let terminal = &settings.terminal;
        TerminalUiPreferences {
            font_family: terminal
                .font_family
                .terminal_family_name(&terminal.custom_font_family),
            font_size: terminal.font_size as f32,
            line_height: terminal.line_height as f32,
            cursor_blink: terminal.cursor_blink,
            copy_on_select: terminal.copy_on_select,
            theme: TerminalUiTheme::new(
                self.tokens.terminal.background,
                self.tokens.terminal.foreground,
                self.tokens.terminal.cursor,
            ),
        }
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
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| "OxideTerm".to_string());
        window.set_window_title(&SharedString::from(title));
        if self.needs_active_pane_focus
            && self.active_surface == ActiveSurface::Terminal
            && !self.search.visible
            && self.new_connection_form.is_none()
            && let Some(pane) = self.active_pane()
        {
            self.needs_active_pane_focus = false;
            window.on_next_frame(move |window, cx| {
                pane.read(cx).focus(window);
            });
        }

        let content = if self.active_surface == ActiveSurface::Settings {
            self.render_settings_surface(cx)
        } else if let Some(tab) = self.active_tab() {
            self.render_pane_tree(&tab.root_pane, cx)
        } else {
            self.render_empty_workspace(cx)
        };

        div()
            .id("workspace-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(self.tokens.ui.bg))
            .text_color(rgb(self.tokens.ui.text))
            .font_family(SharedString::from(self.tokens.metrics.font_family))
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.keyboard_interactive_challenge.is_some() {
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
                    let _ = this.handle_new_connection_key(event, window, cx);
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
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_sidebar_resize(cx);
                    this.finish_split_drag(cx);
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
            .on_action(cx.listener(|this, _: &OpenSettings, _window, cx| {
                this.open_settings(cx);
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
                root.child(self.render_new_connection_modal(cx))
            })
            .when(self.host_key_challenge.is_some(), |root| {
                root.child(self.render_host_key_dialog(cx))
            })
            .when(self.keyboard_interactive_challenge.is_some(), |root| {
                root.child(self.render_keyboard_interactive_dialog(cx))
            })
    }
}
