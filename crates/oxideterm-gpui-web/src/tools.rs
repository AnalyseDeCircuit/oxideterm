mod monitor;
mod forwards;
pub(crate) use forwards::DraftForwards;
use crate::host::{Action, Workspace};
use gpui::{prelude::*, *};
use oxideterm_gpui_onboarding::*;
use oxideterm_gpui_quick_commands::*;
use oxideterm_gpui_settings_view::{
    SettingsTab, animation_label, animation_options, select_menu::language_label,
};
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::{
    QuickCommandTargetContext, QuickCommandTargetProtocol, prepare_quick_command,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabKind;
use std::{path::Path, rc::Rc};

pub struct Tools {
    pub monitor: Option<Entity<monitor::Monitor>>,
    pub onboarding: Entity<OnboardingView>,
    step: usize,
    edit_dark: Option<bool>,
    accepted: bool,
    ai: bool,
    save_failed: bool,
    legal: Option<oxideterm_gpui_markdown::MarkdownVirtualListScrollHandle>,
    pub(crate) forwards: DraftForwards,
    pub quick: TerminalQuickCommandsState,
}
impl Tools {
    pub fn new(
        demo: bool,
        t: ThemeTokens,
        i: &I18n,
        ui_font: &SharedString,
        focus: &FocusHandle,
        cx: &mut Context<Workspace>,
    ) -> Self {
        let owner = cx.weak_entity();
        let read = owner.clone();
        let dispatch = owner.clone();
        let setting_link = |tab: SettingsTab| {
            let owner = owner.clone();
            move |label: String| {
                let owner = owner.clone();
                div()
                    .child(label)
                    .id(("onboarding-setting", tab as usize))
                    .cursor_pointer()
                    .on_click(move |_, w, cx| {
                        let _ = owner.update(cx, |h, cx| {
                            h.settings.update(cx, |s, cx| s.open_section(tab, cx));
                            h.action(Action::Settings, w, cx);
                        });
                    })
                    .into_any_element()
            }
        };
        let language = setting_link(SettingsTab::General);
        let theme = setting_link(SettingsTab::Appearance);
        let font = setting_link(SettingsTab::Terminal);
        let radius = setting_link(SettingsTab::Appearance);
        let launch_owner = owner.clone();
        let onboarding = cx.new(|_| {
            OnboardingView::new(OnboardingViewBindings {
                frame: Rc::new(move |cx| {
                    read.read_with(cx, |h, cx| {
                        let settings = h.settings.read(cx).settings(cx).clone();
                        let app_icon = settings.appearance.app_icon;
                        let system_dark = matches!(
                            cx.window_appearance(),
                            WindowAppearance::Dark | WindowAppearance::VibrantDark
                        );
                        let preview_dark = h.tools.edit_dark.unwrap_or(system_dark);
                        let app_theme = oxideterm_settings_model::ThemeTarget::Application
                            .scheme_id(&settings, preview_dark);
                        let terminal_theme = oxideterm_settings_model::ThemeTarget::Terminal
                            .scheme_id(&settings, preview_dark);
                        let mut preview_tokens = h.tokens;
                        preview_tokens.ui =
                            oxideterm_settings_model::theme_ui_colors(&settings, app_theme);
                        preview_tokens.terminal =
                            oxideterm_gpui_settings_view::theme_picker::appearance_theme_palette(
                                &settings,
                                terminal_theme,
                            );
                        preview_tokens.refresh_palette_metrics();
                        let preview_theme_name =
                            oxideterm_settings_model::custom_theme_display_name(
                                &settings, app_theme,
                            );
                        let mono_font_family =
                            oxideterm_gpui_settings_view::components::settings_mono_font_family(
                                &settings,
                            );
                        OnboardingFrame {
                            tokens: h.tokens,
                            preview_tokens,
                            preview_theme_name,
                            i18n: h.i18n.clone(),
                            language_label: language_label(settings.general.language),
                            settings,
                            sidebar_occupied_width: h.occupied_rail_width(cx),
                            has_background: false,
                            system_dark,
                            edit_scheme: h.tools.edit_dark,
                            app_icon_source: crate::icon::app_icon_source(app_icon),
                            ui_font_family: h.ui_font.clone(),
                            mono_font_family,
                            animation_options: animation_options()
                                .iter()
                                .map(|&speed| (speed, animation_label(speed, &h.i18n)))
                                .collect(),
                            open_settings_select_active: false,
                            step: h.tools.step,
                            disclaimer_accepted: h.tools.accepted,
                            save_failed: h.tools.save_failed,
                            ai_opt_in: h.tools.ai,
                            tool_use_opt_in: false,
                            import_status: OnboardingImportStatus::Idle,
                            imported_count: 0,
                            host_count: None,
                            cli: Default::default(),
                            shortcuts: [
                                (
                                    "app.commandPalette",
                                    "onboarding.shortcut_command_palette",
                                    Action::Palette,
                                ),
                                (
                                    "app.newConnection",
                                    "onboarding.shortcut_new_connection",
                                    Action::NewConnection,
                                ),
                                (
                                    "app.newTerminal",
                                    "onboarding.shortcut_new_tab",
                                    Action::Terminal,
                                ),
                                (
                                    "palette.aiSidebar",
                                    "onboarding.shortcut_ai_chat",
                                    Action::Assistant,
                                ),
                                (
                                    "split.horizontal",
                                    "onboarding.shortcut_split_right",
                                    Action::Split(oxideterm_workspace::SplitDirection::Horizontal),
                                ),
                                (
                                    "split.vertical",
                                    "onboarding.shortcut_split_down",
                                    Action::Split(oxideterm_workspace::SplitDirection::Vertical),
                                ),
                            ]
                            .into_iter()
                            .filter(|(_, _, action)| h.action_available(*action, cx))
                            .filter_map(|(id, label_key, _)| {
                                h.shortcut(id, cx).map(|key_label| OnboardingShortcutItem {
                                    label_key,
                                    key_label,
                                })
                            })
                            .collect(),
                        }
                    })
                    .ok()
                }),
                dispatch: Rc::new(move |r, w, cx| {
                    let _ = dispatch.update(cx, |h, cx| h.onboarding(r, w, cx));
                }),
                language_select: Rc::new(move |_, label, _| language(label.into())),
                theme_select: Rc::new(move |_, s, _| theme(s.appearance.theme.clone())),
                custom_font_input: Rc::new(move |s, _| font(s.terminal.custom_font_family.clone())),
                radius_slider: Rc::new(move |r, _| radius(r.to_string())),
            })
            .for_host("onboarding.browser_save_failed", move |cx| {
                let host = launch_owner.upgrade();
                let h = host.as_ref().map(|h| h.read(cx));
                OnboardingLaunchActions {
                    terminal_description: h.filter(|h| h.ready).map(|h| {
                        if h.demo {
                            "onboarding.demo_terminal_desc"
                        } else {
                            "onboarding.gateway_terminal_desc"
                        }
                    }),
                    new_connection: h
                        .is_some_and(|h| h.action_available(Action::NewConnection, cx)),
                    ai: h.is_some_and(|h| h.action_available(Action::Assistant, cx)),
                    ai_tools: false,
                    import_apps: false,
                    import_ssh: false,
                    cli: false,
                }
            })
        });
        let mut quick = TerminalQuickCommandsState::load(Path::new("browser/settings"));
        if demo {
            quick
                .store
                .commands
                .retain(|c| matches!(c.command.as_str(), "pwd" | "ls -la" | "whoami"));
        }
        let read = owner.clone();
        let dispatch = owner.clone();
        let frame = QuickCommandsFrame {
            tokens: t,
            i18n: i.clone(),
            ui_font: ui_font.clone(),
            mono_font: "Lilex".into(),
            enabled: demo,
            focus: focus.clone(),
            has_background: false,
            caret_visible: true,
            target: None,
            execution_targets: vec![],
        };
        let initial = frame.clone();
        quick.views = Some(QuickCommandsViews::with_local_inputs(
            QuickCommandsAccess::new(owner, |h| &h.tools.quick, |h| &mut h.tools.quick),
            Rc::new(move |cx| {
                read.read_with(cx, |h, _| {
                    let target = h.target();
                    QuickCommandsFrame {
                        ui_font: h.ui_font.clone(),
                        tokens: h.tokens,
                        i18n: h.i18n.clone(),
                        execution_targets: h
                            .tools
                            .quick
                            .pending_execution
                            .as_ref()
                            .map(|draft| draft.targets.clone())
                            .unwrap_or_default(),
                        target,
                        ..frame.clone()
                    }
                })
                .ok()
            }),
            Rc::new(move |r, w, cx| {
                let _ = dispatch.update(cx, |h, cx| h.quick_request(r, w, cx));
            }),
            Rc::new(|v, _| {
                div()
                    .child(v.text)
                    .text_color(rgb(v.color))
                    .into_any_element()
            }),
            initial,
            cx,
        ));
        let monitor = demo.then(|| cx.new(|cx| monitor::Monitor::new(t, i.clone(), cx)));
        Self {
            monitor,
            onboarding,
            step: 0,
            edit_dark: None,
            accepted: false,
            ai: false,
            save_failed: false,
            legal: None,
            forwards: Default::default(),
            quick,
        }
    }
}

impl Workspace {
    pub(crate) fn onboarding_locked(&self) -> bool {
        !self.tools.accepted
            && self
                .layout
                .active_tab()
                .is_some_and(|t| t.kind == TabKind::Onboarding)
    }

    pub(crate) fn open_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self
            .layout
            .tabs()
            .iter()
            .any(|t| t.kind == TabKind::Onboarding)
        {
            let p = self.settings.read(cx).settings(cx);
            self.tools.step = 0;
            self.tools.accepted = p.onboarding_disclaimer_accepted;
            self.tools.ai = p.ai.enabled && p.ai.enabled_confirmed;
            self.tools.save_failed = false;
        }
        self.open_page(TabKind::Onboarding, "onboarding.welcome", cx);
        App::notify(cx, self.tools.onboarding.entity_id());
    }

    fn finish_onboarding(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.tools.accepted {
            return false;
        }
        let Some(id) = self
            .layout
            .tabs()
            .iter()
            .find(|t| t.kind == TabKind::Onboarding)
            .map(|t| t.id)
        else {
            return false;
        };
        let enable_ai = if self.action_available(Action::Assistant, cx) {
            self.tools.ai
        } else {
            self.settings.read(cx).settings(cx).ai.enabled
                && self.settings.read(cx).settings(cx).ai.enabled_confirmed
        };
        if !self.persist_onboarding(enable_ai, cx) {
            self.tools.save_failed = true;
            return false;
        }
        self.settings.update(cx, |s, cx| {
            s.edit_preferences(cx, |p| {
                p.onboarding_completed = true;
                p.onboarding_disclaimer_accepted = true;
                p.ai.enabled = enable_ai;
                p.ai.enabled_confirmed = enable_ai;
            })
        });
        self.tools.legal = None;
        self.close(id, cx);
        true
    }

    pub(crate) fn onboarding_legal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tools.legal.is_none() {
            return false;
        }
        if event.keystroke.key == "escape" {
            self.tools.legal = None;
            cx.stop_propagation();
            cx.notify();
        }
        true
    }

    pub(crate) fn onboarding_legal_dialog(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let scroll = self.tools.legal.as_ref()?;
        let owner = cx.weak_entity();
        Some(
            oxideterm_gpui_settings_view::help_page::SettingsMarkdownDialog {
                tokens: &self.tokens,
                i18n: &self.i18n,
                phase: oxideterm_gpui_ui::motion::ExitPhase::Visible,
                focus: None,
                options: oxideterm_gpui_markdown::MarkdownOptions::from_theme(&self.tokens),
                actions: Default::default(),
            }
            .legal_notice(false, scroll, move |_, cx| {
                let _ = owner.update(cx, |h, cx| {
                    h.tools.legal = None;
                    cx.notify();
                });
            }),
        )
    }

    fn onboarding(&mut self, r: OnboardingRequest, w: &mut Window, cx: &mut Context<Self>) {
        use OnboardingRequest::*;
        match r {
            ToggleDisclaimer => {
                let accepted = !self.tools.accepted;
                self.tools.accepted = accepted;
                self.tools.save_failed = false;
                self.settings.update(cx, |s, cx| {
                    s.edit_preferences(cx, |p| {
                        p.onboarding_disclaimer_accepted = accepted;
                        if !accepted {
                            p.onboarding_completed = false;
                        }
                    })
                });
            }
            Next if self.tools.step + 1 == ONBOARDING_TOTAL_STEPS => {
                self.finish_onboarding(cx);
            }
            Next if self.tools.step != OnboardingStep::DISCLAIMER_INDEX || self.tools.accepted => {
                self.tools.step += 1
            }
            Back => self.tools.step = self.tools.step.saturating_sub(1),
            GoToStep(i)
                if i < ONBOARDING_TOTAL_STEPS
                    && (self.tools.accepted || i <= OnboardingStep::DISCLAIMER_INDEX) =>
            {
                self.tools.step = i
            }
            SkipToLaunch if self.tools.accepted => self.tools.step = ONBOARDING_TOTAL_STEPS - 1,
            ToggleAiOptIn if self.action_available(Action::Assistant, cx) => {
                self.tools.ai = !self.tools.ai
            }
            Complete => {
                self.finish_onboarding(cx);
            }
            OpenTerminal if self.ready && self.finish_onboarding(cx) => {
                self.action(Action::Terminal, w, cx)
            }
            OpenNewConnection
                if self.action_available(Action::NewConnection, cx)
                    && self.finish_onboarding(cx) =>
            {
                self.action(Action::NewConnection, w, cx)
            }
            SetEditScheme(dark) => self.tools.edit_dark = Some(dark),
            SetAppearanceMode(mode) => self.settings.update(cx, |surface, cx| {
                surface.edit_preferences(cx, |s| {
                    oxideterm_settings_model::apply_appearance_mode(s, mode)
                })
            }),
            AssignCurrentScheme(dark) => self.settings.update(cx, |surface, cx| {
                surface.edit_preferences(cx, |s| {
                    oxideterm_settings_model::assign_current_appearance(
                        s,
                        oxideterm_settings::AppearanceScheme::from_dark(dark),
                        matches!(
                            w.appearance(),
                            WindowAppearance::Dark | WindowAppearance::VibrantDark
                        ),
                    )
                })
            }),
            SetCornerStyle(style) => self.settings.update(cx, |surface, cx| {
                surface.edit_preferences(cx, |s| s.appearance.corner_style = style)
            }),
            OpenLegalNotice => self.tools.legal = Some(Default::default()),
            SetFontFamily(f) => self.settings.clone().update(cx, |surface, cx| {
                surface.edit_preferences(cx, |s| s.terminal.font_family = f)
            }),
            AdjustFontSize(n) => self.settings.clone().update(cx, |surface, cx| {
                surface.edit_preferences(cx, |s| {
                    s.terminal.font_size = (s.terminal.font_size + n).clamp(8, 32)
                })
            }),
            SetAnimationSpeed(a) => self.settings.clone().update(cx, |surface, cx| {
                surface.edit_preferences(cx, |s| s.appearance.animation_speed = a)
            }),
            _ => {}
        }
        App::notify(cx, self.tools.onboarding.entity_id());
        self.changed(cx);
    }
    fn target(&self) -> Option<QuickCommandTargetContext> {
        self.target_context(self.active_session()?)
    }
    fn target_context(&self, id: oxideterm_workspace::TerminalSessionId) -> Option<QuickCommandTargetContext> {
        let s = self.sessions.get(&id)?;
        if !s.writable(self.ready) {
            return None;
        }
        let row = self
            .catalog_data
            .items
            .iter()
            .find(|row| row.id() == s.node);
        let text = |v: &str| (!v.is_empty()).then(|| zeroize::Zeroizing::new(v.to_owned()));
        Some(QuickCommandTargetContext {
            target_id: id.0.to_string(),
            label: s.title.clone(),
            protocol: Some(if s.is_ble() {
                QuickCommandTargetProtocol::Ble
            } else if s.node == "local" {
                QuickCommandTargetProtocol::Local
            } else {
                QuickCommandTargetProtocol::Ssh
            }),
            values: oxideterm_quick_commands::QuickCommandContextValues {
                host: row.and_then(|r| text(&r.host)),
                username: row.and_then(|r| text(&r.username)),
                port: row
                    .and_then(|r| u16::try_from(r.port).ok())
                    .filter(|&p| p != 0),
                connection: row.and_then(|r| text(&r.name)),
                group: row.and_then(|r| r.group.as_deref()).and_then(text),
                ..Default::default()
            },
        })
    }

    fn dispatch_quick_command(
        &mut self,
        prepared: oxideterm_quick_commands::PreparedQuickCommand,
        cx: &mut Context<Self>,
    ) {
        let mut skipped = prepared.unavailable_targets;
        let mut sent = false;
        for mut target in prepared.targets {
            target.command.push('\r');
            if target.target_id.parse().ok().is_some_and(|id| {
                self.try_input(
                    oxideterm_workspace::TerminalSessionId(id),
                    target.command.as_bytes(),
                    cx,
                )
            }) {
                sent = true;
            } else {
                skipped.push(target.label);
            }
        }
        if !skipped.is_empty() {
            self.notifications_notice(
                self.i18n
                    .t("terminal.quick_commands.unavailable_targets")
                    .replace("{{targets}}", &skipped.join(", ")),
                cx,
            );
        } else if sent
            && self
                .settings
                .read(cx)
                .settings(cx)
                .terminal
                .command_bar
                .quick_commands_show_toast
        {
            self.notice("terminal.quick_commands.toast_executed", cx);
        }
        self.tools.quick.finish_execution();
    }
    fn quick_request(
        &mut self,
        r: QuickCommandsRequest,
        w: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        use QuickCommandsRequest::*;
        let q = &mut self.tools.quick;
        match r {
            OpenManager => q.open_manager(),
            FocusInput(i, _) => {
                q.store.focused_input = Some(i);
                if let Some(w) = w {
                    w.focus(&self.focus, cx);
                }
            }
            Run(command) => {
                q.close_manager();
                let targets: Vec<_> = self.quick_target_sessions(cx).into_iter().filter_map(|id| {
                    let mut target = self.target_context(id)?;
                    if command.command.contains("ctx.selection") {
                        target.values.selection = self.sessions.get(&id)?.view.read(cx).selected_text().map(zeroize::Zeroizing::new);
                    }
                    Some(target)
                }).collect();
                let confirm = self
                    .settings
                    .read(cx)
                    .settings(cx)
                    .terminal
                    .command_bar
                    .quick_commands_confirm_before_run;
                if !confirm
                    && command.parameters.is_empty()
                    && !command.command.contains("{{")
                    && targets.len() == 1
                    && let Ok(prepared) =
                        prepare_quick_command(&command, &targets, &Default::default())
                    && !prepared.confirmation_required
                    && !prepared.targets.is_empty()
                    && prepared.unavailable_targets.is_empty()
                {
                    self.dispatch_quick_command(prepared, cx);
                } else {
                    self.tools.quick.request_execution_for(command, targets);
                }
            }
            ConfirmExecution => {
                if let Some(draft) = &self.tools.quick.pending_execution {
                    let values = draft
                        .command
                        .parameters
                        .iter()
                        .zip(&draft.parameter_values)
                        .map(|(p, v)| (p.name.clone(), v.clone()))
                        .collect();
                    if let Ok(prepared) =
                        prepare_quick_command(&draft.command, &draft.targets, &values)
                    {
                        self.dispatch_quick_command(prepared, cx);
                    }
                }
            }
            Insert(text) => {
                if let Some(w) = w {
                    self.insert_sender_draft(text, w, cx);
                }
            }
            FocusPane => {
                if let Some(w) = w
                    && let Some(s) = self.active_session().and_then(|id| self.sessions.get(&id))
                {
                    w.focus(&s.view.read(cx).focus_handle(cx), cx);
                }
            }
            Notice(n) => {
                use oxideterm_gpui_ui::toast::ToastVariant;
                let variant = match n.variant {
                    QuickCommandsNoticeKind::Success => ToastVariant::Success,
                    QuickCommandsNoticeKind::Error => ToastVariant::Error,
                };
                self.record_notice(oxideterm_notification_center::NotificationKind::Transfer, variant, n.title, n.description, None, cx);
            }
            _ => {}
        }
        if let Some(v) = &self.tools.quick.views {
            App::notify(cx, v.manager.entity_id());
            App::notify(cx, v.dock.entity_id());
        }
        self.changed(cx);
    }
    pub fn quick_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let q = &mut self.tools.quick;
        if q.focused_input().is_some() {
            return false;
        }
        if e.keystroke.key == "escape" {
            q.close_manager();
            q.close();
            self.changed(cx);
            return true;
        }
        false
    }
}
