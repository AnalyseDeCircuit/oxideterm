use super::*;
use crate::{
    appearance_page::CoreAppearancePage,
    components::{SettingsWindowDispatch, TerminalPreviewFont, settings_mono_font_family},
    help_page::{HelpPresentation, HelpRequest, SettingsMarkdownDialog},
    keybinding_page::{KeybindingPageView, KeybindingRequest, KeybindingTextRole},
    proxy_page::ProxyPresentation,
    select_menu::{CoreSelectMenu, settings_select_popover},
    terminal_page::TerminalSettingsSection,
    theme_editor::{ThemeEditorBody, ThemeEditorFrame},
};
use oxideterm_gpui_ui::motion::ExitPhase;

pub(super) fn tokens(settings: &PersistedSettings, dark: bool) -> ThemeTokens {
    use oxideterm_theme::{UiDensityProfile, UiRadii};
    // Only RGB swatches are needed here. The runtime terminal parser interns selection colors.
    let app = oxideterm_settings_model::ThemeTarget::Application.resolved_id(settings, dark);
    let terminal = oxideterm_settings_model::ThemeTarget::Terminal.resolved_id(settings, dark);
    let mut tokens = ThemeTokens::from_builtin(oxideterm_theme::theme_by_id(app));
    tokens.ui = oxideterm_settings_model::theme_ui_colors(settings, app);
    tokens.terminal = crate::theme_picker::appearance_theme_palette(settings, terminal);
    tokens.refresh_palette_metrics();
    let radius = settings.appearance.border_radius as f32;
    tokens.radii = UiRadii {
        xs: (radius - 4.0).max(0.0),
        sm: (radius - 2.0).max(0.0),
        md: radius,
        lg: radius + 4.0,
        active_indicator: radius.clamp(1.0, 2.0),
    };
    tokens.apply_density(match settings.appearance.ui_density {
        oxideterm_settings::UiDensity::Compact => UiDensityProfile::Compact,
        oxideterm_settings::UiDensity::Comfortable => UiDensityProfile::Comfortable,
        oxideterm_settings::UiDensity::Spacious => UiDensityProfile::Spacious,
    });
    tokens.apply_ui_font_scale(
        settings.appearance.ui_font_size as f32 / oxideterm_settings::DEFAULT_UI_FONT_SIZE as f32,
    );
    tokens.apply_motion(match settings.appearance.animation_speed {
        oxideterm_settings::AnimationSpeed::Off => oxideterm_theme::UiMotionProfile::Off,
        oxideterm_settings::AnimationSpeed::Reduced => oxideterm_theme::UiMotionProfile::Reduced,
        oxideterm_settings::AnimationSpeed::Normal => oxideterm_theme::UiMotionProfile::Normal,
        oxideterm_settings::AnimationSpeed::Fast => oxideterm_theme::UiMotionProfile::Fast,
    });
    tokens
}

fn font_preview(settings: &PersistedSettings, cx: &App) -> TerminalPreviewFont {
    let term = &settings.terminal;
    let configured = term
        .font_family
        .terminal_family_name(&term.custom_font_family);
    let families = oxideterm_gpui_ui::typography::css_font_family_stack(&configured);
    let mut font = gpui::font(settings_mono_font_family(settings));
    font.weight = gpui::FontWeight(term.font_weight as f32);
    if !term.font_ligatures {
        font.features = gpui::FontFeatures::disable_ligatures();
    }
    let (available, missing): (Vec<_>, Vec<_>) = families.into_iter().partition(|family| {
        cx.text_system()
            .font_id(&gpui::Font {
                family: family.clone(),
                ..font.clone()
            })
            .is_ok()
    });
    if let Some(primary) = available.first() {
        font.family = primary.clone();
    }
    let fallbacks = available
        .iter()
        .skip(1)
        .map(ToString::to_string)
        .chain(
            oxideterm_gpui_ui::typography::css_font_family_stack(&term.cjk_font_family)
                .into_iter()
                .map(|f| f.to_string()),
        )
        .collect();
    font.fallbacks = Some(gpui::FontFallbacks::from_fonts(fallbacks));
    let size = px(term.font_size as f32);
    let font_id = cx.text_system().resolve_font(&font);
    let width = cx.text_system().layout_width(font_id, size, 'm');
    let spacing = (term.character_spacing
        * oxideterm_settings::TERMINAL_CHARACTER_SPACING_BASELINE_SCALE) as f32;
    let height = f32::from(cx.text_system().ascent(font_id, size)).max(0.0)
        + f32::from(cx.text_system().descent(font_id, size)).abs();
    TerminalPreviewFont {
        font,
        font_size: size,
        line_height: px((if height > 0.0 {
            height
        } else {
            f32::from(size)
        } * term.line_height as f32)
            .max(1.0)),
        letter_spacing: Some(width * (spacing - 1.0)),
        unavailable_families: missing.into_iter().map(|f| f.to_string()).collect(),
    }
}

pub(super) fn render_section(
    owner: &Entity<PageState>,
    tokens: &ThemeTokens,
    dark: bool,
    section: SettingsSectionId,
    dispatch: SettingsWindowDispatch,
    cx: &App,
) -> AnyElement {
    use SettingsSectionId::*;
    let state = owner.read(cx);
    let c = state.controls(tokens, dispatch);
    match section {
        GeneralLanguage | GeneralWindowBehavior | GeneralExternalConnectionUris => {
            c.general_preference_section(section, cx)
        }
        ConnectionsKnownHosts => {
            let hosts = &state.known_hosts;
            let owner = owner.downgrade();
            c.known_hosts_list(
                &hosts.snapshot,
                &hosts.query,
                hosts.selected.as_ref(),
                &hosts.scroll,
                hosts.handler.is_some(),
                None,
                Rc::new(move |request, cx| {
                    let _ = owner.update(cx, |s, cx| s.known_hosts_action(request, cx));
                }),
                cx,
            )
        }
        NetworkSharedProxy | NetworkRouting => {
            let owner = owner.downgrade();
            ProxyPresentation {
                controls: c,
                password: None,
                probe: state.probe.form.clone(),
                can_probe: state.probe.handler.is_some(),
                dispatch: Rc::new(move |request, cx| {
                    let _ = owner.update(cx, |s, cx| s.proxy_action(request, cx));
                }),
            }
            .section(section, cx)
        }
        HelpVersion | HelpDiagnostics | HelpTechStack | HelpResources | HelpSafety | HelpLegal => {
            let owner = owner.downgrade();
            HelpPresentation {
                controls: c,
                year: &chrono::Utc::now().format("%Y").to_string(),
                dispatch: Rc::new(move |request, cx| match request {
                    HelpRequest::Url(url) => cx.open_url(url),
                    HelpRequest::Legal | HelpRequest::ThirdParty => {
                        let _ = owner.update(cx, |s, cx| {
                            s.fields.blur();
                            s.document = Some((
                                matches!(request, HelpRequest::ThirdParty),
                                Default::default(),
                            ));
                            cx.notify();
                        });
                    }
                    _ => unreachable!("desktop help operations are not mounted"),
                }),
            }
            .section(section, cx)
        }
        SftpOpening | SftpPerformance | SftpConflict => c.settings_sftp_section(section, cx),
        IdeEditing | IdeTypography | IdeAgent | IdeAgentPrivacy => {
            c.settings_ide_section(section, state.ide_custom, cx)
        }
        AppearanceTheme | AppearanceLayout => {
            let owner = owner.downgrade();
            let page = CoreAppearancePage {
                controls: c,
                system_dark: dark,
                edit_scheme: state.edit_scheme(dark),
                onboarding: false,
                desktop: false,
                dispatch: Rc::new(move |action, cx| {
                    let _ = owner.update(cx, |s, cx| s.appearance(action, dark, cx));
                }),
            };
            if section == AppearanceTheme {
                let rows = page.appearance_theme_controls_with_hints(&state.settings, true, cx);
                page.appearance_card(state.i18n.t("settings_view.appearance.theme"), None, rows)
            } else {
                page.appearance_layout_card(&state.settings, cx)
            }
        }
        TerminalPageSwitcher => {
            let current = state.route.route_snapshot().terminal_page;
            oxideterm_gpui_ui::tabs::tabs_list(tokens)
                .children(
                    [TerminalSettingsPage::Display, TerminalSettingsPage::Input].map(|page| {
                        let owner = owner.clone();
                        oxideterm_gpui_ui::tabs::tabs_trigger(
                            tokens,
                            state.i18n.t(page.label_key()),
                            page == current,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            move |_, _, cx| {
                                owner.update(cx, |s, cx| {
                                    s.fields.blur();
                                    s.select = None;
                                    s.route.set_terminal_page(page);
                                    cx.notify();
                                })
                            },
                        )
                    }),
                )
                .into_any_element()
        }
        KeybindingsToolbar | KeybindingsScope(_) => keybindings(owner, &c, false, section, cx),
        _ => c.terminal_settings_section(
            match section {
                TerminalDisplayFont => TerminalSettingsSection::Font {
                    custom_cjk: state.terminal_custom,
                    preview: font_preview(&state.settings, cx),
                },
                TerminalDisplayBehavior => TerminalSettingsSection::DisplayBehavior,
                TerminalDisplayCursor => TerminalSettingsSection::Cursor,
                TerminalDisplayCommandMarks => TerminalSettingsSection::CommandMarks,
                TerminalDisplayBuffer => TerminalSettingsSection::Buffer,
                TerminalInputClipboard => TerminalSettingsSection::Clipboard,
                TerminalInputMouseLinks => TerminalSettingsSection::MouseLinks,
                TerminalInputKeyboard => TerminalSettingsSection::Keyboard,
                _ => unreachable!("only supported sections are mounted"),
            },
            cx,
        ),
    }
}

fn keybindings(
    owner: &Entity<PageState>,
    c: &SettingsControls<'_>,
    reset: bool,
    section: SettingsSectionId,
    cx: &App,
) -> AnyElement {
    let state = owner.read(cx);
    let search = |cx: &App| {
        c.keybinding_search_field(
            c.ime.frame(
                SettingsInput::KeybindingSearch,
                state.shortcuts.keybinding_search_query(),
                cx,
            ),
            c.dispatch.clone(),
        )
    };
    let label = |_: KeybindingTextRole, _, value: String, color: u32| {
        div().text_color(rgb(color)).child(value).into_any_element()
    };
    let weak = owner.downgrade();
    let page = KeybindingPageView {
        components: c.components,
        state: owner,
        catalog: &state.catalog,
        search: &search,
        label: &label,
        dispatch: Rc::new(move |req: KeybindingRequest, cx| {
            let _ = weak.update(cx, |s, cx| s.shortcut(req, cx));
        }),
    };
    if reset {
        page.render_keybinding_reset_all_confirm_dialog(cx)
    } else {
        page.settings_keybindings_section(section, cx)
    }
}

impl PortableSettingsSurface {
    pub(super) fn overlays(
        &self,
        tokens: &ThemeTokens,
        dispatch: SettingsWindowDispatch,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let mut layers = Vec::new();
        let state = self.state.read(cx);
        if let Some((third_party, scroll)) = &state.document {
            let owner = self.state.downgrade();
            layers.push(
                SettingsMarkdownDialog {
                    tokens,
                    i18n: &state.i18n,
                    phase: ExitPhase::Visible,
                    focus: None,
                    options: oxideterm_gpui_markdown::MarkdownOptions::from_theme(tokens),
                    actions: Default::default(),
                }
                .legal_notice(*third_party, scroll, move |_, cx| {
                    let _ = owner.update(cx, |s, cx| {
                        s.document = None;
                        cx.notify();
                    });
                }),
            );
        }
        if state.reset_open {
            layers.push(keybindings(
                &self.state,
                &state.controls(tokens, dispatch.clone()),
                true,
                SettingsSectionId::KeybindingsToolbar,
                cx,
            ));
        }
        if let Some(editor) = state.editor.clone() {
            let frame = ThemeEditorFrame {
                tokens: *tokens,
                i18n: state.i18n.clone(),
                mono_font: settings_mono_font_family(&state.settings),
                scroll: state.editor_scroll.clone(),
                open_select: state.select,
                select_origin: Some(oxideterm_gpui_ui::select::BrowserFocusOrigin::Pointer),
            };
            let owner = self.state.downgrade();
            let ime = self.state.downgrade();
            let body = ThemeEditorBody {
                settings: &self.state,
                frame,
                background: None,
                window: dispatch,
                ime: &move |id, value, _, cx| {
                    Some(SettingsImeSource::frame(
                        ime.upgrade()?.read(cx),
                        id,
                        value,
                        cx,
                    ))
                },
                dispatch: Rc::new(move |r, _, cx| {
                    let _ = owner.update(cx, |s, cx| s.editor_action(r, cx));
                }),
                preview: &|editor, frame, _| {
                    let terminal =
                        crate::theme_picker::preview_palette(frame.tokens.terminal, |key| {
                            let index = oxideterm_settings_model::TERMINAL_THEME_COLOR_FIELDS
                                .iter()
                                .position(|f| f.json_key == key)?;
                            oxideterm_settings_model::parse_color_hex(
                                editor.terminal_colors.get(index)?,
                            )
                        });
                    Some(crate::settings_theme_editor_preview(
                        &frame.tokens,
                        &editor.name,
                        terminal,
                        oxideterm_settings_model::editor_ui_colors(&editor.ui_colors),
                        frame.mono_font.clone(),
                        None,
                        editor.background.scope,
                    ))
                },
            };
            layers.extend(body.render_modal(&editor, ExitPhase::Visible, cx));
        }
        let state = self.state.read(cx);
        if let Some(select) = state.select
            && let Some(&anchor) = state.anchors.get(&select.anchor_id())
        {
            let owner = self.state.downgrade();
            let width = f32::from(anchor.bounds.size.width);
            let popup = if select.theme_target().is_some() {
                Some(self.picker.clone().into_any_element())
            } else {
                CoreSelectMenu {
                    settings: &state.settings,
                    tokens,
                    i18n: &state.i18n,
                    owner: &self.state,
                    dispatch: Rc::new(move |req, _, cx| {
                        let _ = owner.update(cx, |s, cx| s.menu_action(req, cx));
                    }),
                }
                .render(state.route.route_snapshot().active_tab, select, width, cx)
            };
            if let Some(popup) = popup {
                let owner = self.state.clone();
                layers.push(settings_select_popover(
                    tokens,
                    select,
                    anchor,
                    popup,
                    move |_, cx| {
                        owner.update(cx, |s, cx| {
                            s.select = None;
                            cx.notify();
                        })
                    },
                ));
            }
        }
        layers
    }
}
