use crate::*;
use gpui::{AnyElement, App, Entity, MouseDownEvent, Window, prelude::*};
use oxideterm_gpui_ui::{
    modal::overlay_content_boundary,
    select::{
        select_option, select_option_action, select_overlay_popup,
        select_panel_overlay_popup_with_max_height,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    ConnectionPoolSettings, Language, NO_TIMEOUT_SECS, PersistedSettings,
    SettingsApplicationProxyMode, SettingsUpstreamProxyProtocol, SftpPresentationPreference,
};
use oxideterm_theme::{BUILT_IN_THEMES, ThemeTokens};
use std::rc::Rc;

mod network;

#[cfg(not(target_family = "wasm"))]
mod desktop;
#[cfg(not(target_family = "wasm"))]
pub use desktop::*;

pub enum SelectMenuRequest {
    Close,
    Blur,
    Focus(SettingsInput),
    Edit(Box<dyn FnOnce(&mut PersistedSettings)>),
    #[cfg(not(target_family = "wasm"))]
    Background(Box<dyn FnOnce(&mut oxideterm_settings::BackgroundStyle)>),
    #[cfg(not(target_family = "wasm"))]
    RemoteShell(oxideterm_settings::RemoteShellIntegrationMode),
    #[cfg(not(target_family = "wasm"))]
    Trigger(
        bool,
        Box<dyn FnOnce(&mut crate::terminal_triggers::TerminalTriggersSettingsState)>,
    ),
    #[cfg(not(target_family = "wasm"))]
    External(ExternalSelect),
    #[cfg(not(target_family = "wasm"))]
    KnowledgeCollectionScope(Option<String>),
    #[cfg(not(target_family = "wasm"))]
    KnowledgeDocumentFormat(&'static str),
    #[cfg(not(target_family = "wasm"))]
    CloseSemantic,
    #[cfg(not(target_family = "wasm"))]
    CloseHighlight,
    #[cfg(not(target_family = "wasm"))]
    Highlight(
        usize,
        Box<dyn FnOnce(&mut oxideterm_settings::HighlightRule)>,
    ),
    #[cfg(not(target_family = "wasm"))]
    HighlightPreset(Vec<oxideterm_settings::HighlightRule>),
    ProxyAuth(crate::proxy_page::NetworkProxyAuthMode),
    SftpPresentation(SftpPresentationPreference),
}

pub fn settings_select_popover(
    tokens: &ThemeTokens,
    select: SettingsSelect,
    anchor: oxideterm_gpui_ui::select::OverlayAnchor,
    content: AnyElement,
    dismiss: impl Fn(&mut Window, &mut App) + Clone + 'static,
) -> AnyElement {
    use gpui::{
        AnchoredPositionMode, Corner, MouseButton, SharedString, anchored, deferred, div, point, px,
    };
    use oxideterm_gpui_ui::modal::{TAURI_POPOVER_LAYER_PRIORITY, popover_backdrop};
    popover_backdrop()
        .on_mouse_down(MouseButton::Left, {
            let dismiss = dismiss.clone();
            move |_, window, cx| {
                dismiss(window, cx);
                cx.stop_propagation();
            }
        })
        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
            dismiss(window, cx);
            cx.stop_propagation();
        })
        .child(
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position(anchor.bounds.bottom_left())
                    .offset(point(px(0.0), px(tokens.metrics.settings_select_popup_gap)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                        tokens,
                        SharedString::from(format!("settings-select-popup-{select:?}")),
                        div().child(content),
                        -6.0,
                        point(0.5, 0.0),
                    )),
            )
            .with_priority(TAURI_POPOVER_LAYER_PRIORITY),
        )
        .into_any_element()
}

pub type SelectMenuDispatch = Rc<dyn Fn(SelectMenuRequest, &mut Window, &mut App)>;

pub trait CoreSelectOwner: Sized + 'static {
    fn custom_cjk(&self, ide: bool) -> bool;
    fn set_custom_cjk(&mut self, ide: bool, custom: bool);
    fn duplicate_id(&self) -> Option<&str>;
    fn duplicate(&mut self, id: &str, cx: &mut gpui::Context<Self>);
}

pub struct CoreSelectMenu<'a, S> {
    pub settings: &'a PersistedSettings,
    pub tokens: &'a ThemeTokens,
    pub i18n: &'a I18n,
    pub owner: &'a Entity<S>,
    pub dispatch: SelectMenuDispatch,
}

#[cfg(target_family = "wasm")]
pub use CoreSelectMenu as SettingsSelectMenu;

struct CoreMenuActions<S> {
    owner: Entity<S>,
    dispatch: SelectMenuDispatch,
}

impl<S> CoreMenuActions<S> {
    fn send(&self, request: SelectMenuRequest, window: &mut Window, cx: &mut App) {
        (self.dispatch)(request, window, cx);
    }
}

impl<S: CoreSelectOwner> CoreSelectMenu<'_, S> {
    fn action<F>(&self, action: F) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F, S>
    where
        F: Fn(&CoreMenuActions<S>, &MouseDownEvent, &mut Window, &mut App) + 'static,
    {
        let actions = CoreMenuActions {
            dispatch: self.dispatch.clone(),
            owner: self.owner.clone(),
        };
        move |event, window, cx| action(&actions, event, window, cx)
    }

    pub fn render(
        &self,
        active_tab: SettingsTab,
        open_select: SettingsSelect,
        width: f32,
        cx: &App,
    ) -> Option<AnyElement> {
        let settings = self.settings;
        let popup = match (active_tab, open_select) {
            (SettingsTab::Sftp, SettingsSelect::SftpPresentation) => {
                let mut popup = select_overlay_popup(self.tokens, width);
                for preference in [
                    SftpPresentationPreference::Ask,
                    SftpPresentationPreference::Tab,
                    SftpPresentationPreference::Sidebar,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            self.tokens,
                            crate::sftp_page::sftp_presentation_label(preference, self.i18n),
                            preference == settings.sftp.presentation,
                        ),
                        false,
                        false,
                        self.action(move |this, _, window, cx| {
                            this.send(SelectMenuRequest::Close, window, cx);
                            this.send(SelectMenuRequest::SftpPresentation(preference), window, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (_, SettingsSelect::Language) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for language in language_options() {
                    let label = language_label(language);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, language == settings.general.language),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.general.language = language
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::CustomThemeDuplicate) => {
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width,
                    self.tokens.metrics.settings_theme_select_popup_max_height,
                );
                let mut themes: Vec<_> = BUILT_IN_THEMES.iter().collect();
                themes.sort_by_key(|theme| theme.id);
                for theme in themes {
                    let theme_id = theme.id.to_string();
                    let selected = self.owner.read(cx).duplicate_id() == Some(theme_id.as_str());
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, theme_display_name(theme.id), selected),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.owner.update(cx, |settings, cx| {
                                settings.duplicate(&theme_id, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceDensity) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &density in density_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            density_label(density, &self.i18n),
                            density == settings.appearance.ui_density,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.appearance.ui_density = density
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceAnimation) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &speed in animation_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            animation_label(speed, &self.i18n),
                            speed == settings.appearance.animation_speed,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            // Apply the selected profile before starting the
                            // exit so choosing Off closes in this same event.
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.appearance.animation_speed = speed
                                })),
                                _window,
                                cx,
                            );
                            this.send(SelectMenuRequest::Close, _window, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Appearance, SettingsSelect::AppearanceRenderProfile) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &profile in render_profile_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            render_profile_label(profile, &self.i18n),
                            profile == settings.appearance.render_profile,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.appearance.render_profile = profile
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ide, SettingsSelect::IdeFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for family in
                    std::iter::once(None).chain(font_family_options().iter().copied().map(Some))
                {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            family.map(font_family_label).unwrap_or_else(|| {
                                self.i18n.t("settings_view.ide.follow_terminal")
                            }),
                            family == settings.ide.font_family,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.ide.font_family = family
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ide, SettingsSelect::IdeCjkFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current_family = settings.ide.cjk_font_family.as_deref().map(str::trim);
                let custom = self.owner.read(cx).custom_cjk(true)
                    || current_family.is_some_and(cjk_font_is_custom);
                for family in std::iter::once(None)
                    .chain(terminal_cjk_font_options().iter().copied().map(Some))
                {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            family
                                .map(|family| terminal_cjk_font_label(family, &self.i18n))
                                .unwrap_or_else(|| {
                                    self.i18n.t("settings_view.ide.follow_terminal")
                                }),
                            !custom && family == current_family,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.owner.update(cx, |state, _| {
                                state.set_custom_cjk(true, false);
                            });
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.ide.cjk_font_family = family.map(str::to_string)
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.cjk_font_custom"),
                        custom,
                    ),
                    false,
                    false,
                    self.action(|this, _, window, cx| {
                        this.owner.update(cx, |state, _| {
                            state.set_custom_cjk(true, true);
                        });
                        this.send(
                            SelectMenuRequest::Focus(SettingsInput::IdeCjkFontFamily),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ));
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &family in font_family_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            font_family_label(family),
                            family == settings.terminal.font_family,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.font_family = family
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalCjkFontFamily) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current_family = settings.terminal.cjk_font_family.trim();
                let custom =
                    self.owner.read(cx).custom_cjk(false) || cjk_font_is_custom(current_family);
                for &family in terminal_cjk_font_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_cjk_font_label(family, &self.i18n),
                            !custom && family == current_family,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.owner.update(cx, |state, _| {
                                state.set_custom_cjk(false, false);
                            });
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.cjk_font_family = family.to_string()
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.cjk_font_custom"),
                        custom,
                    ),
                    false,
                    false,
                    self.action(|this, _, window, cx| {
                        this.owner.update(cx, |state, _| {
                            state.set_custom_cjk(false, true);
                        });
                        this.send(
                            SelectMenuRequest::Focus(SettingsInput::TerminalCjkFontFamily),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ));
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalEncoding) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &encoding in terminal_encoding_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_encoding_label(encoding),
                            encoding == settings.terminal.terminal_encoding,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.terminal_encoding = encoding
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalRightClickAction) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for action in TERMINAL_RIGHT_CLICK_ACTIONS {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_right_click_action_label(action, &self.i18n),
                            action == settings.terminal.right_click_action(),
                        ),
                        false,
                        false,
                        self.action(move |this, _event, window, cx| {
                            this.send(SelectMenuRequest::Close, window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.set_right_click_action(action);
                                })),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalBackspaceSequence) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &sequence in terminal_backspace_sequence_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_backspace_sequence_label(sequence),
                            sequence == settings.terminal.backspace_sequence,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.backspace_sequence = sequence
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalDeleteSequence) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &sequence in terminal_delete_sequence_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_delete_sequence_label(sequence),
                            sequence == settings.terminal.delete_sequence,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.delete_sequence = sequence
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalCursorStyle) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &style in cursor_style_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            cursor_style_label(style, &self.i18n),
                            style == settings.terminal.cursor_style,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.cursor_style = style
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalBellStyle) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &style in terminal_bell_style_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_bell_style_label(style, &self.i18n),
                            style == settings.terminal.bell_style,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.bell_style = style
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for protocol in [
                    oxideterm_settings::FileTransferProtocolPreference::Auto,
                    oxideterm_settings::FileTransferProtocolPreference::Sftp,
                    oxideterm_settings::FileTransferProtocolPreference::Scp,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            file_transfer_protocol_label(protocol, &self.i18n),
                            protocol == settings.sftp.transfer_protocol,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.sftp.transfer_protocol = protocol
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpConcurrent) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &count in sftp_concurrent_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            sftp_transfer_count_label(&self.i18n, count),
                            count == settings.sftp.max_concurrent_transfers,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.sftp.max_concurrent_transfers = count
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpDirectoryParallelism) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &count in sftp_directory_parallelism_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            sftp_transfer_count_label(&self.i18n, count),
                            count == settings.sftp.directory_parallelism,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.sftp.directory_parallelism = count
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpConflict) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for action in [
                    oxideterm_settings::ConflictAction::Ask,
                    oxideterm_settings::ConflictAction::Overwrite,
                    oxideterm_settings::ConflictAction::Skip,
                    oxideterm_settings::ConflictAction::Rename,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            conflict_label(action, &self.i18n),
                            action == settings.sftp.conflict_action,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.sftp.conflict_action = action
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            _ => self.network_or_update_menu(active_tab, open_select, width),
        }?;
        Some(overlay_content_boundary(popup).into_any_element())
    }
}
pub fn language_label(language: Language) -> String {
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
pub fn connection_duration_label(select: SettingsSelect, seconds: i64, i18n: &I18n) -> String {
    let key = match (select, seconds) {
        (SettingsSelect::ClosedSessionRetention, NO_TIMEOUT_SECS) => {
            "settings_view.connections.retention.never"
        }
        (SettingsSelect::ClosedSessionRetention, 0) => {
            "settings_view.connections.retention.immediate"
        }
        (
            SettingsSelect::ConnectionIdleTimeout,
            ConnectionPoolSettings::DISCONNECT_IMMEDIATELY_SECS,
        ) => "settings_view.connections.idle_timeout.immediate",
        (SettingsSelect::ConnectionIdleTimeout, 0) => {
            "settings_view.connections.idle_timeout.never"
        }
        (_, 300) => "settings_view.connections.idle_timeout.5min",
        (_, 900) => "settings_view.connections.idle_timeout.15min",
        (_, 1800) => "settings_view.connections.idle_timeout.30min",
        (_, 3600) => "settings_view.connections.idle_timeout.1hr",
        _ => return seconds.to_string(),
    };
    i18n.t(key)
}

pub fn network_proxy_protocol_label(
    protocol: SettingsUpstreamProxyProtocol,
    i18n: &I18n,
) -> String {
    match protocol {
        SettingsUpstreamProxyProtocol::Socks5 => i18n.t("settings_view.network.protocol_socks5"),
        SettingsUpstreamProxyProtocol::HttpConnect => {
            i18n.t("settings_view.network.protocol_http_connect")
        }
    }
}

pub fn network_application_proxy_mode_label(
    mode: SettingsApplicationProxyMode,
    i18n: &oxideterm_i18n::I18n,
) -> String {
    match mode {
        SettingsApplicationProxyMode::System => {
            i18n.t("settings_view.network.application_mode_system")
        }
        SettingsApplicationProxyMode::Direct => {
            i18n.t("settings_view.network.application_mode_direct")
        }
        SettingsApplicationProxyMode::Shared => {
            i18n.t("settings_view.network.application_mode_shared")
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn application_semantic_scheme_label(settings: &PersistedSettings, i18n: &I18n) -> String {
    settings
        .terminal
        .active_custom_semantic_scheme()
        .map(|scheme| scheme.name.clone())
        .unwrap_or_else(|| terminal_semantic_scheme_label(settings.terminal.semantic_scheme, i18n))
}
