mod monitor;
use crate::host::{Action, Workspace};
use gpui::{prelude::*, *};
use oxideterm_gpui_forwards::{data::*, *};
use oxideterm_gpui_onboarding::*;
use oxideterm_gpui_quick_commands::*;
use oxideterm_gpui_settings_view::{
    SettingsTab, animation_label, animation_options, select_menu::language_label,
};
use oxideterm_gpui_ui::text_input::TextInputAnchorId;
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::{
    QuickCommandTargetContext, QuickCommandTargetProtocol, prepare_quick_command,
};
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabKind;
use std::{path::Path, rc::Rc, time::Duration};

pub struct Tools {
    pub monitor: Option<Entity<monitor::Monitor>>,
    pub onboarding: Entity<OnboardingView>,
    step: usize,
    edit_dark: Option<bool>,
    accepted: bool,
    ai: bool,
    tool_use: bool,
    pub forwards: ForwardsViews,
    rules: ForwardingRuntimeSnapshot,
    pub quick: TerminalQuickCommandsState,
}
impl Tools {
    pub fn forwards_sidebar(&self, t: &ThemeTokens, i: &I18n) -> AnyElement {
        self.forwards.sidebar(&self.rules.rules, t, i)
    }

    pub fn new(
        demo: bool,
        t: ThemeTokens,
        i: &I18n,
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
        let onboarding = cx.new(|_| {
            OnboardingView::new(OnboardingViewBindings {
                frame: Rc::new(move |cx| {
                    read.read_with(cx, |h, cx| {
                        let settings = h.settings.read(cx).settings(cx).clone();
                        OnboardingFrame {
                            tokens: h.tokens,
                            preview_tokens: h.tokens,
                            preview_theme_name: settings.appearance.theme.clone(),
                            i18n: h.i18n.clone(),
                            language_label: language_label(settings.general.language),
                            settings,
                            sidebar_occupied_width: 0.,
                            has_background: false,
                            system_dark: true,
                            edit_scheme: h.tools.edit_dark,
                            app_icon_source: "icons/oxide-mark.svg".into(),
                            ui_font_family: "IBM Plex Sans".into(),
                            mono_font_family: "Lilex".into(),
                            animation_options: animation_options()
                                .iter()
                                .map(|&speed| (speed, animation_label(speed, &h.i18n)))
                                .collect(),
                            open_settings_select_active: false,
                            step: h.tools.step,
                            disclaimer_accepted: h.tools.accepted,
                            save_failed: false,
                            ai_opt_in: h.tools.ai,
                            tool_use_opt_in: h.tools.tool_use,
                            import_status: OnboardingImportStatus::Idle,
                            imported_count: 0,
                            host_count: None,
                            cli: Default::default(),
                            shortcuts: vec![],
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
        });
        let read = owner.clone();
        let dispatch = owner.clone();
        let state = cx.new(|_| ForwardsPageState::new());
        let forwards = ForwardsViews::new(
            state,
            ForwardsViewBindings {
                frame: Rc::new(move |cx| {
                    read.read_with(cx, |h, _| {
                        Some(ForwardsFrame {
                            tab_id: h.layout.active_tab_id()?,
                            node_id: NodeId::new("demo"),
                            node_ready: h.demo,
                            can_save: h.demo,
                            runtime: h.tools.rules.clone(),
                            ports: Default::default(),
                            tokens: h.tokens,
                            i18n: h.i18n.clone(),
                            has_background: false,
                            ui_font: "IBM Plex Sans".into(),
                            cjk_font: "IBM Plex Sans".into(),
                            mono_font: "Lilex".into(),
                        })
                    })
                    .ok()
                    .flatten()
                }),
                ime: Rc::new(|input, _| {
                    Some(ForwardsImeFrame {
                        anchor: TextInputAnchorId(input.anchor_key()),
                        viewport: Default::default(),
                        active_offset: None,
                        selection: None,
                        composition: None,
                        caret_visible: true,
                        place_anchor: Rc::new(|_| {}),
                    })
                }),
                dispatch: Rc::new(move |r, w, cx| {
                    let _ = dispatch.update(cx, |h, cx| h.forward(r, w, cx));
                }),
            },
            cx,
        );
        let mut rules = ForwardingRuntimeSnapshot::default();
        if demo {
            let mut r = ForwardRule::local("127.0.0.1", 8080, "localhost", 80);
            r.status = ForwardStatus::Active;
            rules.rules.push(r);
        }
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
            ui_font: "IBM Plex Sans".into(),
            mono_font: "Lilex".into(),
            enabled: demo,
            focus: focus.clone(),
            has_background: false,
            caret_visible: true,
            target: None,
            execution_targets: vec![],
        };
        let initial = frame.clone();
        quick.views = Some(QuickCommandsViews::new(
            QuickCommandsAccess::new(owner, |h| &h.tools.quick, |h| &mut h.tools.quick),
            QuickCommandsBindings {
                frame: Rc::new(move |cx| {
                    read.read_with(cx, |h, _| {
                        let target = h.target();
                        QuickCommandsFrame {
                            tokens: h.tokens,
                            i18n: h.i18n.clone(),
                            execution_targets: target.iter().cloned().collect(),
                            target,
                            ..frame.clone()
                        }
                    })
                    .ok()
                }),
                dispatch: Rc::new(move |r, w, cx| {
                    let _ = dispatch.update(cx, |h, cx| h.quick_request(r, w, cx));
                }),
                ime: Rc::new(|input, _| QuickCommandsIme {
                    anchor_id: TextInputAnchorId(input.anchor_key()),
                    active_offset: None,
                    selection: None,
                    marked: None,
                }),
                anchor: Rc::new(|_| {}),
                selectable: Rc::new(|v, _| {
                    div()
                        .child(v.text)
                        .text_color(rgb(v.color))
                        .into_any_element()
                }),
            },
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
            tool_use: false,
            forwards,
            rules,
            quick,
        }
    }
}

impl Workspace {
    fn onboarding(&mut self, r: OnboardingRequest, w: &mut Window, cx: &mut Context<Self>) {
        use OnboardingRequest::*;
        match r {
            ToggleDisclaimer => self.tools.accepted = !self.tools.accepted,
            Next if self.tools.step != 1 || self.tools.accepted => {
                self.tools.step = (self.tools.step + 1).min(3)
            }
            Back => self.tools.step = self.tools.step.saturating_sub(1),
            GoToStep(i) => self.tools.step = i.min(if self.tools.accepted { 3 } else { 1 }),
            SkipToLaunch => self.tools.step = if self.tools.accepted { 3 } else { 1 },
            ToggleAiOptIn => self.tools.ai = !self.tools.ai,
            ToggleToolUseOptIn => self.tools.tool_use = !self.tools.tool_use,
            Complete if self.tools.accepted => {
                if let Some(id) = self.layout.active_tab_id() {
                    self.close(id, cx);
                }
            }
            OpenTerminal => self.action(Action::Terminal, w, cx),
            OpenNewConnection => self.action(Action::NewConnection, w, cx),
            OpenCliSettings => self.action(Action::Settings, w, cx),
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
            OpenLegalNotice => {
                self.settings
                    .update(cx, |s, cx| s.open_section(SettingsTab::Help, cx));
                self.action(Action::Settings, w, cx);
            }
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
        let id = self.active_session()?;
        let s = self.sessions.get(&id)?;
        Some(QuickCommandTargetContext {
            target_id: id.0.to_string(),
            label: s.node.clone(),
            protocol: if s.node == "local" {
                QuickCommandTargetProtocol::Local
            } else {
                QuickCommandTargetProtocol::Ssh
            },
            values: Default::default(),
        })
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
                let target = self.target();
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
                    && let Some(target) = target
                    && let Ok(prepared) =
                        prepare_quick_command(&command, &[target], &Default::default())
                    && !prepared.confirmation_required
                    && let Some(mut cmd) = prepared.targets.into_iter().next()
                    && let Some(id) = self.active_session()
                {
                    cmd.command.push('\r');
                    self.input(id, cmd.command.as_bytes(), cx);
                    self.tools.quick.finish_execution();
                } else {
                    self.tools.quick.request_execution(command);
                }
            }
            ConfirmExecution => {
                let target = self.target();
                if let Some(d) = &self.tools.quick.pending_execution
                    && let Some(t) = target
                {
                    let values = d
                        .command
                        .parameters
                        .iter()
                        .zip(&d.parameter_values)
                        .map(|(p, v)| (p.name.clone(), v.clone()))
                        .collect();
                    if let Ok(prepared) = prepare_quick_command(&d.command, &[t], &values)
                        && let Some(mut cmd) = prepared.targets.into_iter().next()
                        && let Some(id) = self.active_session()
                    {
                        cmd.command.push('\r');
                        self.input(id, cmd.command.as_bytes(), cx);
                        self.tools.quick.finish_execution();
                    }
                }
            }
            Insert(text) => {
                if let Some(id) = self.active_session() {
                    self.input(id, text.as_bytes(), cx);
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
                self.notifications_notice(n.title, cx);
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
        if e.keystroke.key == "escape" {
            q.close_manager();
            q.close();
            self.changed(cx);
            return true;
        }
        let Some(i) = q.focused_input() else {
            return false;
        };
        let Some(v) = q.input_value(i) else {
            return false;
        };
        let end = v.encode_utf16().count();
        if e.keystroke.key == "backspace" {
            let size = v.chars().last().map_or(0, char::len_utf16);
            q.replace_input(i, Some(end - size..end), "");
        } else if let Some(s) = &e.keystroke.key_char
            && !e.keystroke.modifiers.control
            && !e.keystroke.modifiers.platform
        {
            q.replace_input(i, Some(end..end), s);
        } else {
            return false;
        }
        if let Some(v) = &q.views {
            App::notify(cx, v.manager.entity_id());
            App::notify(cx, v.dock.entity_id());
        }
        true
    }
    pub fn forwards_key(&self, e: &KeyDownEvent, cx: &mut App) -> bool {
        self.layout
            .active_tab()
            .is_some_and(|t| t.kind == TabKind::Forwards)
            && self.tools.forwards.state.update(cx, |s, cx| {
                if !matches!(s.handle_input_key(e, cx), ForwardsInputKey::Unhandled) {
                    return true;
                }
                if let Some(input) = s.focused_input()
                    && !e.keystroke.modifiers.platform
                    && !e.keystroke.modifiers.control
                    && let Some(text) = e.keystroke.key_char.as_deref()
                {
                    s.replace_input_text(input, None, text);
                    cx.notify();
                    return true;
                }
                false
            })
    }
    fn forward(&mut self, r: ForwardsRequest, w: Option<&mut Window>, cx: &mut Context<Self>) {
        let rules = &mut self.tools.rules.rules;
        match r {
            ForwardsRequest::FocusInput(i, _) => {
                self.tools.forwards.state.update(cx, |s, cx| {
                    s.focus_input(i);
                    cx.notify();
                });
                if let Some(w) = w {
                    w.focus(&self.focus, cx);
                }
                return;
            }
            ForwardsRequest::Create {
                mut rule, start, ..
            } => {
                rule.status = if start {
                    ForwardStatus::Active
                } else {
                    ForwardStatus::Stopped
                };
                rules.push(rule);
            }
            ForwardsRequest::Operation { operation, .. } => {
                use ForwardingRuntimeOperation::*;
                match operation {
                    Save { mut rule } | Create { mut rule, .. } => {
                        rule.status = ForwardStatus::Active;
                        rules.push(rule);
                    }
                    Stop { forward_id } => {
                        if let Some(r) = rules.iter_mut().find(|r| r.id == forward_id) {
                            r.status = ForwardStatus::Stopped;
                        }
                    }
                    Restart { forward_id } => {
                        if let Some(r) = rules.iter_mut().find(|r| r.id == forward_id) {
                            r.status = ForwardStatus::Active;
                        }
                    }
                    Delete { forward_id } => rules.retain(|r| r.id != forward_id),
                    Update { forward_id, update } => {
                        if let Some(r) = rules.iter_mut().find(|r| r.id == forward_id) {
                            if let Some(v) = update.forward_type {
                                r.forward_type = v;
                            }
                            if let Some(v) = update.bind_address {
                                r.bind_address = v;
                            }
                            if let Some(v) = update.bind_port {
                                r.bind_port = v;
                            }
                            if let Some(v) = update.target_host {
                                r.target_host = v;
                            }
                            if let Some(v) = update.target_port {
                                r.target_port = v;
                            }
                            if let Some(v) = update.description {
                                r.description = v;
                            }
                        }
                    }
                    Quick { port, .. } => {
                        let mut r = ForwardRule::local("localhost", port, "localhost", port);
                        r.status = ForwardStatus::Active;
                        rules.push(r);
                    }
                }
            }
            _ => return,
        }
        self.tools.forwards.state.update(cx, |s, cx| {
            s.complete_operation(Ok(()), true, Duration::ZERO, cx)
        });
        cx.notify();
    }
}
