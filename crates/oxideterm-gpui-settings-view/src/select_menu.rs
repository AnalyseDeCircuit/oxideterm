use std::rc::Rc;

mod external;
pub use external::{ExternalSelect, ExternalSelectInputs, plugin_marketplace_sort_label};
mod knowledge;
pub use knowledge::KnowledgeSelectInputs;

use gpui::{AnyElement, App, Entity, MouseDownEvent, Window, prelude::*};
use oxideterm_connections::{
    ConnectionImportDuplicateStrategy, PrivilegePromptType, PrivilegeResponseMode,
    totp::TotpCredential,
};
use oxideterm_gpui_platform::vibrancy::available_modes;
use oxideterm_gpui_ui::{
    modal::overlay_content_boundary,
    select::{
        select_label, select_option, select_option_action, select_overlay_popup,
        select_panel_overlay_popup_with_max_height, select_separator,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    BackgroundStyle, FrostedGlassMode, HighlightRule, Language, NO_TIMEOUT_SECS, PersistedSettings,
    RemoteShellIntegrationMode, SettingsApplicationProxyMode, SettingsUpstreamProxyAuth,
    SettingsUpstreamProxyProtocol, SftpPresentationPreference, TerminalSemanticScheme,
    UpdateChannel, UpdateProxyMode, UpdateProxyProtocol,
};
use oxideterm_settings_model::{
    SEMANTIC_CLASSES, SemanticRuleContext, edit_custom_semantic_scheme,
    reconnect_base_delay_options, reconnect_max_attempt_options, reconnect_max_delay_options,
};
use oxideterm_terminal_triggers::{
    LocalProcessSpec, TerminalTriggerAction, TerminalTriggerDispatch, TerminalTriggerMatchMode,
    TerminalTriggerScope,
};
use oxideterm_theme::{BUILT_IN_THEMES, ThemeTokens};

use crate::{
    connections_page::{
        connection_import_duplicate_strategy_label, connection_import_source_label,
        connection_import_source_options,
    },
    credentials_page::{
        settings_privilege_prompt_type_label, settings_privilege_response_mode_label,
    },
    entity::SettingsWorkspaceEntity,
    highlight_page::{semantic_class_label, semantic_context_label},
    proxy_page::{NetworkProxyAuthMode, network_proxy_auth_label},
    terminal_triggers::TerminalTriggersSettingsState,
    *,
};

pub enum SelectMenuRequest {
    Close,
    Blur,
    Focus(SettingsInput),
    Edit(Box<dyn FnOnce(&mut PersistedSettings)>),
    Background(Box<dyn FnOnce(&mut BackgroundStyle)>),
    RemoteShell(RemoteShellIntegrationMode),
    Trigger(bool, Box<dyn FnOnce(&mut TerminalTriggersSettingsState)>),
    External(ExternalSelect),
    KnowledgeCollectionScope(Option<String>),
    KnowledgeDocumentFormat(&'static str),
    CloseSemantic,
    CloseHighlight,
    Highlight(usize, Box<dyn FnOnce(&mut HighlightRule)>),
    HighlightPreset(Vec<HighlightRule>),
    ProxyAuth(NetworkProxyAuthMode),
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

struct SelectMenuActions {
    dispatch: SelectMenuDispatch,
    settings_workspace: Entity<SettingsWorkspaceEntity>,
}

impl SelectMenuActions {
    fn send(&self, request: SelectMenuRequest, window: &mut Window, cx: &mut App) {
        (self.dispatch)(request, window, cx);
    }
}

pub struct SettingsSelectMenu<'a> {
    pub settings: &'a PersistedSettings,
    pub tokens: &'a ThemeTokens,
    pub i18n: &'a I18n,
    pub settings_workspace: &'a Entity<SettingsWorkspaceEntity>,
    pub external: ExternalSelectInputs<'a>,
    pub knowledge: KnowledgeSelectInputs<'a>,
    pub totp_credentials: &'a [TotpCredential],
    pub shells: Vec<LocalShellOption>,
    pub trigger_commands: Vec<(String, String)>,
    pub first_command: String,
    pub dispatch: SelectMenuDispatch,
}

#[derive(Clone)]
pub struct LocalShellOption {
    pub id: String,
    pub label: String,
}

impl SettingsSelectMenu<'_> {
    fn action<F>(&self, action: F) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F>
    where
        F: Fn(&SelectMenuActions, &MouseDownEvent, &mut Window, &mut App) + 'static,
    {
        let actions = SelectMenuActions {
            dispatch: self.dispatch.clone(),
            settings_workspace: self.settings_workspace.clone(),
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
            (_, SettingsSelect::PluginMarketplaceSort)
            | (
                SettingsTab::Ai,
                SettingsSelect::IdeAgentMode
                | SettingsSelect::AiProviderTemplate
                | SettingsSelect::AiWebSearchProvider
                | SettingsSelect::AiMcpTransport
                | SettingsSelect::AiMcpAuthMode,
            ) => self.external_popup(open_select, width),
            (
                SettingsTab::Knowledge,
                SettingsSelect::AiEmbeddingProvider
                | SettingsSelect::KnowledgeCollectionScope
                | SettingsSelect::KnowledgeDocumentFormat,
            ) => self.knowledge_popup(open_select, width),
            (SettingsTab::Privilege, SettingsSelect::LocalPrivilegePromptType) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = self.settings_workspace.read(cx).privilege_prompt_type();
                for prompt_type in [
                    PrivilegePromptType::Sudo,
                    PrivilegePromptType::Su,
                    PrivilegePromptType::Custom,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            settings_privilege_prompt_type_label(self.i18n, prompt_type),
                            current == prompt_type,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_privilege_prompt_type(prompt_type, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Privilege, SettingsSelect::LocalPrivilegeResponseMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = self.settings_workspace.read(cx).privilege_response_mode();
                for response_mode in [
                    PrivilegeResponseMode::PasswordOnly,
                    PrivilegeResponseMode::PasswordThenTotp,
                    PrivilegeResponseMode::PasswordWithTotp,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            settings_privilege_response_mode_label(self.i18n, response_mode),
                            current == response_mode,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_privilege_response_mode(response_mode, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Privilege, SettingsSelect::LocalPrivilegeTotpCredential) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = self
                    .settings_workspace
                    .read(cx)
                    .privilege_snapshot()
                    .totp_credential_id;
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n
                            .t("sessionManager.privilege_credentials.totp_none"),
                        selected.is_none(),
                    ),
                    false,
                    false,
                    self.action(|this, _event, _window, cx| {
                        this.send(SelectMenuRequest::Close, _window, cx);
                        this.settings_workspace.update(cx, |settings, cx| {
                            settings.set_privilege_totp_credential(None, cx);
                        });
                        cx.stop_propagation();
                    }),
                ));
                for credential in self.totp_credentials {
                    let credential_id = credential.id.clone();
                    let label = if credential.enabled {
                        credential.name.clone()
                    } else {
                        format!(
                            "{} ({})",
                            credential.name,
                            self.i18n.t("settings_view.privilege_credentials.disabled")
                        )
                    };
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            selected.as_deref() == Some(credential_id.as_str()),
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            let credential_id = credential_id.clone();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_privilege_totp_credential(Some(credential_id), cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionImportSource) => {
                let selected_source = self.settings_workspace.read(cx).connection_import_source();
                let mut popup = select_overlay_popup(&self.tokens, width);
                for source in connection_import_source_options().iter().copied() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_import_source_label(source, &self.i18n),
                            source == selected_source,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_connection_import_source(source, cx)
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionImportDuplicateStrategy) => {
                let selected_strategy = self
                    .settings_workspace
                    .read(cx)
                    .connection_import_duplicate_strategy();
                let mut popup = select_overlay_popup(&self.tokens, width);
                for strategy in [
                    ConnectionImportDuplicateStrategy::Skip,
                    ConnectionImportDuplicateStrategy::Rename,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_import_duplicate_strategy_label(strategy, &self.i18n),
                            strategy == selected_strategy,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_connection_import_duplicate_strategy(strategy, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::NetworkProxyAuth) => {
                let mut popup = select_overlay_popup(self.tokens, width);
                let current = settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| match proxy.auth {
                        SettingsUpstreamProxyAuth::None => NetworkProxyAuthMode::None,
                        SettingsUpstreamProxyAuth::Password { .. } => {
                            NetworkProxyAuthMode::Password
                        }
                    })
                    .unwrap_or(NetworkProxyAuthMode::None);
                for mode in [NetworkProxyAuthMode::None, NetworkProxyAuthMode::Password] {
                    popup = popup.child(select_option_action(
                        select_option(
                            self.tokens,
                            network_proxy_auth_label(mode, self.i18n),
                            mode == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _, window, cx| {
                            this.send(SelectMenuRequest::Close, window, cx);
                            this.send(SelectMenuRequest::ProxyAuth(mode), window, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
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
            (SettingsTab::Help, SettingsSelect::UpdateChannel) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
                    let label = update_channel_label(channel, &self.i18n);
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            channel == settings.general.update_channel,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.general.update_channel = channel
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
            (SettingsTab::Network, SettingsSelect::UpdateProxyMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.general.update_proxy.mode;
                for mode in [
                    UpdateProxyMode::Direct,
                    UpdateProxyMode::Application,
                    UpdateProxyMode::System,
                    UpdateProxyMode::Custom,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            update_proxy_mode_label(mode, &self.i18n),
                            mode == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.general.update_proxy.mode = mode
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
            (SettingsTab::Network, SettingsSelect::UpdateProxyProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.general.update_proxy.protocol;
                for protocol in [
                    UpdateProxyProtocol::Http,
                    UpdateProxyProtocol::Https,
                    UpdateProxyProtocol::Socks5,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            update_proxy_protocol_label(protocol, &self.i18n),
                            protocol == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.general.update_proxy.protocol = protocol
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
            (SettingsTab::Network, SettingsSelect::NetworkApplicationProxyMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings.network.application_proxy_mode;
                for mode in [
                    SettingsApplicationProxyMode::System,
                    SettingsApplicationProxyMode::Direct,
                    SettingsApplicationProxyMode::Shared,
                ] {
                    // A missing shared profile is shown but cannot become an
                    // active route, keeping the runtime policy fail-closed.
                    let shared_proxy_missing = mode == SettingsApplicationProxyMode::Shared
                        && settings.network.upstream_proxy.is_none();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_application_proxy_mode_label(mode, &self.i18n),
                            mode == current,
                        ),
                        shared_proxy_missing,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            if shared_proxy_missing {
                                return;
                            }
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.network.application_proxy_mode = mode
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
                    let selected = self
                        .settings_workspace
                        .read(cx)
                        .theme_editor()
                        .is_some_and(|editor| editor.duplicate_theme == theme_id);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, theme_display_name(theme.id), selected),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.duplicate_theme_editor_from(&theme_id, cx);
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
            (SettingsTab::Appearance, SettingsSelect::AppearanceFrostedGlass) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                // The selector lists platform window materials only. Legacy
                // WebView CSS/native values are normalized to the system entry.
                let selected_mode = match settings.appearance.frosted_glass {
                    FrostedGlassMode::Css | FrostedGlassMode::Native => FrostedGlassMode::System,
                    mode => mode,
                };
                for mode in available_modes()
                    .iter()
                    .copied()
                    .map(frosted_glass_mode_from_native)
                {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            frosted_glass_label(mode, &self.i18n),
                            selected_mode == mode,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.appearance.frosted_glass = mode
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
            (SettingsTab::Appearance, SettingsSelect::AppearanceBackgroundFit) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &fit in background_fit_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            background_fit_label(fit, &self.i18n),
                            fit == settings.terminal.background_fit,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Background(Box::new(move |style| {
                                    style.fit = fit
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
                let custom = self.settings_workspace.read(cx).ide_cjk_font_custom
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
                            this.settings_workspace.update(cx, |state, _| {
                                state.ide_cjk_font_custom = false;
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
                        this.settings_workspace.update(cx, |state, _| {
                            state.ide_cjk_font_custom = true;
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
                let custom = self.settings_workspace.read(cx).terminal_cjk_font_custom
                    || cjk_font_is_custom(current_family);
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
                            this.settings_workspace.update(cx, |state, _| {
                                state.terminal_cjk_font_custom = false;
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
                        this.settings_workspace.update(cx, |state, _| {
                            state.terminal_cjk_font_custom = true;
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
            (SettingsTab::Terminal, SettingsSelect::SshLatencyInterval) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for seconds in [5, 10, 15, 30, 60] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            seconds.to_string(),
                            seconds == settings.terminal.command_bar.ssh_latency_interval_secs,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.command_bar.ssh_latency_interval_secs =
                                        seconds
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
            (SettingsTab::Terminal, SettingsSelect::TerminalSessionLogFileMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &mode in terminal_session_log_file_mode_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_session_log_file_mode_label(mode, &self.i18n),
                            mode == settings.terminal.session_log.file_mode,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.session_log.file_mode = mode
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
            (SettingsTab::Terminal, SettingsSelect::XmodemBlockSize) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for size in [
                    oxideterm_settings::XmodemBlockSize::Bytes128,
                    oxideterm_settings::XmodemBlockSize::Bytes1024,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            xmodem_block_size_label(size, &self.i18n),
                            size == settings.terminal.xmodem_block_size,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.xmodem_block_size = size
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
            (SettingsTab::Terminal, SettingsSelect::RemoteShellIntegrationMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for mode in [
                    RemoteShellIntegrationMode::Ask,
                    RemoteShellIntegrationMode::Enabled,
                    RemoteShellIntegrationMode::Disabled,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            remote_shell_integration_mode_label(mode, &self.i18n),
                            mode == settings.terminal.remote_shell_integration_mode,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.remote_shell_integration_mode = mode
                                })),
                                _window,
                                cx,
                            );
                            this.send(SelectMenuRequest::RemoteShell(mode), _window, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerMatchMode) => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let current = draft.matcher.mode;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for mode in [
                    TerminalTriggerMatchMode::Literal,
                    TerminalTriggerMatchMode::Regex,
                ] {
                    let label = self.i18n.t(match mode {
                        TerminalTriggerMatchMode::Literal => {
                            "settings_view.terminal.triggers.literal"
                        }
                        TerminalTriggerMatchMode::Regex => "settings_view.terminal.triggers.regex",
                    });
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, mode == current),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_match_mode(mode)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (
                SettingsTab::Terminal,
                SettingsSelect::TerminalTriggerAction | SettingsSelect::TerminalTriggerClickAction,
            ) => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let nested = open_select == SettingsSelect::TerminalTriggerClickAction;
                let action = if nested {
                    self.settings_workspace
                        .read(cx)
                        .terminal_triggers
                        .terminal_trigger_draft_action()?
                } else {
                    &draft.action
                };
                let current = match action {
                    TerminalTriggerAction::SendText { .. } => 0,
                    TerminalTriggerAction::RunQuickCommand { .. } => 1,
                    TerminalTriggerAction::LaunchLocalProcess { .. } => 2,
                    TerminalTriggerAction::OpenUrl { .. } => 3,
                    TerminalTriggerAction::CustomLink { .. } => 4,
                    TerminalTriggerAction::CustomMenu { .. } => 5,
                    TerminalTriggerAction::ShowTooltip { .. } => 6,
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for action_index in if nested {
                    vec![0, 1, 2, 3]
                } else {
                    vec![0, 1, 2, 4, 5, 6]
                } {
                    let label_key = match action_index {
                        0 => "settings_view.terminal.triggers.action_send_text",
                        1 => "settings_view.terminal.triggers.action_quick_command",
                        2 => "settings_view.terminal.triggers.action_local_process",
                        3 => "settings_view.terminal.triggers.open_url",
                        4 => "settings_view.terminal.triggers.custom_link",
                        5 => "settings_view.terminal.triggers.custom_menu",
                        _ => "settings_view.terminal.triggers.show_tooltip",
                    };
                    let first_command = self.first_command.clone();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(label_key),
                            action_index == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            let action = match action_index {
                                0 => TerminalTriggerAction::SendText {
                                    text: String::new(),
                                    append_enter: false,
                                },
                                1 => {
                                    let quick_command_id = first_command.clone();
                                    TerminalTriggerAction::RunQuickCommand { quick_command_id }
                                }
                                2 => TerminalTriggerAction::LaunchLocalProcess {
                                    process: LocalProcessSpec::DirectProgram {
                                        executable: String::new(),
                                        arguments: Vec::new(),
                                        working_directory: None,
                                    },
                                },
                                3 => TerminalTriggerAction::OpenUrl { url: String::new() },
                                4 => TerminalTriggerAction::CustomLink {
                                    action: Box::new(TerminalTriggerAction::OpenUrl {
                                        url: String::new(),
                                    }),
                                },
                                5 => TerminalTriggerAction::CustomMenu {
                                    items: vec![
                                        oxideterm_terminal_triggers::TerminalTriggerMenuItem {
                                            label: String::new(),
                                            action: TerminalTriggerAction::OpenUrl {
                                                url: String::new(),
                                            },
                                        },
                                    ],
                                },
                                _ => TerminalTriggerAction::ShowTooltip {
                                    text: String::new(),
                                },
                            };
                            if nested {
                                this.send(
                                    SelectMenuRequest::Trigger(
                                        true,
                                        Box::new(move |state| {
                                            state.select_terminal_trigger_click_action(action)
                                        }),
                                    ),
                                    _window,
                                    cx,
                                );
                            } else {
                                this.send(
                                    SelectMenuRequest::Trigger(
                                        true,
                                        Box::new(move |state| {
                                            state.select_terminal_trigger_action(action)
                                        }),
                                    ),
                                    _window,
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerProcessMode) => {
                let action = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft_action()?;
                let current_shell = matches!(
                    action,
                    TerminalTriggerAction::LaunchLocalProcess {
                        process: LocalProcessSpec::ExplicitShell { .. }
                    }
                );
                let mut popup = select_overlay_popup(&self.tokens, width);
                for shell in [false, true] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(if shell {
                                "settings_view.terminal.triggers.explicit_shell"
                            } else {
                                "settings_view.terminal.triggers.direct_program"
                            }),
                            shell == current_shell,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_process_mode(shell)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerQuickCommand) => {
                let current = match self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft_action()?
                {
                    TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                        quick_command_id.clone()
                    }
                    _ => String::new(),
                };
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width,
                    self.tokens.metrics.settings_theme_select_popup_max_height,
                );
                for (id, name) in &self.trigger_commands {
                    let id = id.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, name.clone(), id == current),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            let id = id.clone();
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_quick_command(id)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerTiming) => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let current = draft.timing.dispatch;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for dispatch in [
                    TerminalTriggerDispatch::Immediate,
                    TerminalTriggerDispatch::AfterNextLineBreak,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(match dispatch {
                                TerminalTriggerDispatch::Immediate => {
                                    "settings_view.terminal.triggers.immediate"
                                }
                                TerminalTriggerDispatch::AfterNextLineBreak => {
                                    "settings_view.terminal.triggers.after_next_line_break"
                                }
                            }),
                            dispatch == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_timing(dispatch)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalTriggerScope) => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let current = match &draft.scope {
                    TerminalTriggerScope::AllTerminals => 0,
                    TerminalTriggerScope::LocalTerminals => 1,
                    TerminalTriggerScope::SavedConnections { .. } => 2,
                };
                let saved_connections = match &draft.scope {
                    TerminalTriggerScope::SavedConnections { connections } => connections.clone(),
                    _ => Vec::new(),
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for scope_index in 0..3 {
                    let label_key = match scope_index {
                        0 => "settings_view.terminal.triggers.all_terminals",
                        1 => "settings_view.terminal.triggers.local_terminals",
                        _ => "settings_view.terminal.triggers.saved_connections",
                    };
                    let existing_connections = saved_connections.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), scope_index == current),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            let scope = match scope_index {
                                0 => TerminalTriggerScope::AllTerminals,
                                1 => TerminalTriggerScope::LocalTerminals,
                                _ => TerminalTriggerScope::SavedConnections {
                                    connections: existing_connections.clone(),
                                },
                            };
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_scope(scope)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::TerminalSemanticScheme) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &scheme in terminal_semantic_scheme_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_semantic_scheme_label(scheme, &self.i18n),
                            scheme == settings.terminal.semantic_scheme,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(SelectMenuRequest::CloseSemantic, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.semantic_scheme = scheme;
                                    settings.terminal.semantic_custom_scheme = None;
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                if !settings.terminal.custom_semantic_schemes.is_empty() {
                    popup = popup
                        .child(select_separator(&self.tokens))
                        .child(select_label(
                            &self.tokens,
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.semantic_scheme_custom"),
                        ));
                    for custom in &settings.terminal.custom_semantic_schemes {
                        let id = custom.id.clone();
                        let selected = settings.terminal.semantic_custom_scheme.as_deref()
                            == Some(id.as_str());
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, custom.name.clone(), selected),
                            false,
                            false,
                            self.action(move |this, _event, _window, cx| {
                                this.send(SelectMenuRequest::Close, _window, cx);
                                let id = id.clone();
                                this.send(
                                    SelectMenuRequest::Edit(Box::new(move |settings| {
                                        settings.terminal.semantic_custom_scheme = Some(id.clone());
                                    })),
                                    _window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightRuleSet) => {
                let mut popup =
                    select_overlay_popup(&self.tokens, width).child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.rule_set_global_base"),
                            settings.terminal.default_highlight_rule_set.is_none(),
                        ),
                        false,
                        false,
                        self.action(|this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(SelectMenuRequest::CloseHighlight, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.terminal.default_highlight_rule_set = None
                                })),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                if !settings.terminal.highlight_rule_sets.is_empty() {
                    popup = popup.child(select_separator(&self.tokens));
                    for rule_set in &settings.terminal.highlight_rule_sets {
                        let id = rule_set.id.clone();
                        let selected = settings.terminal.default_highlight_rule_set.as_deref()
                            == Some(id.as_str());
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, rule_set.name.clone(), selected),
                            false,
                            false,
                            self.action(move |this, _event, _window, cx| {
                                this.send(SelectMenuRequest::Close, _window, cx);
                                this.send(SelectMenuRequest::CloseHighlight, _window, cx);
                                let id = id.clone();
                                this.send(
                                    SelectMenuRequest::Edit(Box::new(move |settings| {
                                        settings.terminal.default_highlight_rule_set =
                                            Some(id.clone());
                                    })),
                                    _window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::SemanticSchemeRuleClass(index)) => {
                let selected = settings
                    .terminal
                    .active_custom_semantic_scheme()
                    .and_then(|scheme| scheme.rules.get(index))
                    .map(|rule| rule.class)?;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for &class in SEMANTIC_CLASSES {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            semantic_class_label(class, &self.i18n),
                            class == selected,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    let _ = edit_custom_semantic_scheme(settings, |scheme| {
                                        if let Some(rule) = scheme.rules.get_mut(index) {
                                            rule.class = class;
                                        }
                                    });
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
            (SettingsTab::Terminal, SettingsSelect::SemanticSchemeRuleContext(index)) => {
                let selected = settings
                    .terminal
                    .active_custom_semantic_scheme()
                    .and_then(|scheme| scheme.rules.get(index))
                    .map(|rule| rule.context)?;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for context in [
                    SemanticRuleContext::Any,
                    SemanticRuleContext::Command,
                    SemanticRuleContext::Output,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            semantic_context_label(context, &self.i18n),
                            context == selected,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    let _ = edit_custom_semantic_scheme(settings, |scheme| {
                                        if let Some(rule) = scheme.rules.get_mut(index) {
                                            rule.context = context;
                                        }
                                    });
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
            (SettingsTab::Terminal, SettingsSelect::HighlightPreset) => {
                let mut popup = select_overlay_popup(&self.tokens, width.max(288.0));
                for (group_index, group) in
                    highlight_preset_groups(&self.i18n).into_iter().enumerate()
                {
                    if group_index > 0 {
                        popup = popup.child(select_separator(&self.tokens));
                    }
                    popup = popup.child(select_label(&self.tokens, group.label));
                    for preset in group.items {
                        popup = popup.child(select_option_action(
                            select_option(&self.tokens, preset.label.clone(), false),
                            false,
                            false,
                            self.action(move |this, _event, _window, cx| {
                                this.send(SelectMenuRequest::Close, _window, cx);
                                this.send(
                                    SelectMenuRequest::HighlightPreset(preset.rules.clone()),
                                    _window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightRenderMode(index)) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings
                    .terminal
                    .highlight_rules
                    .get(index)
                    .map(|rule| rule.render_mode)
                    .unwrap_or_default();
                for &mode in highlight_render_mode_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            highlight_render_mode_label(mode, &self.i18n),
                            mode == selected,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Highlight(
                                    index,
                                    Box::new(move |rule| rule.render_mode = mode),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::HighlightMatchScope(index)) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings
                    .terminal
                    .highlight_rules
                    .get(index)
                    .map(|rule| rule.match_scope)
                    .unwrap_or_default();
                for &scope in highlight_match_scope_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            highlight_match_scope_label(scope, &self.i18n),
                            scope == selected,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Highlight(
                                    index,
                                    Box::new(move |rule| rule.match_scope = scope),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Terminal, SettingsSelect::LocalShell) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = settings.local_terminal.default_shell_id.as_deref();
                for shell in self.shells.iter().cloned().collect::<Vec<_>>() {
                    let shell_id = shell.id.clone();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            shell.label,
                            selected == Some(shell_id.as_str()),
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(SelectMenuRequest::Blur, _window, cx);
                            let shell_id = shell_id.clone();
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    settings.local_terminal.default_shell_id =
                                        Some(shell_id.clone())
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
            (SettingsTab::Terminal, SettingsSelect::LocalShellSemanticScheme(index)) => {
                let shell = self
                    .shells
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .nth(index)?;
                let selected = settings.local_terminal.semantic_scheme_for_shell(&shell.id);
                let shell_id = shell.id.clone();
                let inherited_label = self
                    .i18n
                    .t("ssh.form.terminal_use_application_default")
                    .replace(
                        "{{value}}",
                        &application_semantic_scheme_label(settings, &self.i18n),
                    );
                let mut popup =
                    select_overlay_popup(&self.tokens, width).child(select_option_action(
                        select_option(&self.tokens, inherited_label, selected.is_none()),
                        false,
                        false,
                        self.action({
                            let shell_id = shell_id.clone();
                            move |this, _event, _window, cx| {
                                this.send(SelectMenuRequest::Close, _window, cx);
                                let shell_id = shell_id.clone();
                                this.send(
                                    SelectMenuRequest::Edit(Box::new(move |settings| {
                                        settings
                                            .local_terminal
                                            .semantic_scheme_by_shell
                                            .remove(&shell_id);
                                    })),
                                    _window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ));
                for (scheme_id, scheme) in [
                    ("balanced", TerminalSemanticScheme::Balanced),
                    ("conservative", TerminalSemanticScheme::Conservative),
                ] {
                    let scheme_id = scheme_id.to_string();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            terminal_semantic_scheme_label(scheme, &self.i18n),
                            selected == Some(scheme_id.as_str()),
                        ),
                        false,
                        false,
                        self.action({
                            let shell_id = shell_id.clone();
                            move |this, _event, _window, cx| {
                                this.send(SelectMenuRequest::Close, _window, cx);
                                let shell_id = shell_id.clone();
                                let scheme_id = scheme_id.clone();
                                this.send(
                                    SelectMenuRequest::Edit(Box::new(move |settings| {
                                        settings
                                            .local_terminal
                                            .semantic_scheme_by_shell
                                            .insert(shell_id.clone(), scheme_id.clone());
                                    })),
                                    _window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ));
                }
                if !settings.terminal.custom_semantic_schemes.is_empty() {
                    popup = popup
                        .child(select_separator(&self.tokens))
                        .child(select_label(
                            &self.tokens,
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.semantic_scheme_custom"),
                        ));
                    for custom in &settings.terminal.custom_semantic_schemes {
                        let scheme_id = custom.id.clone();
                        popup = popup.child(select_option_action(
                            select_option(
                                &self.tokens,
                                custom.name.clone(),
                                selected == Some(scheme_id.as_str()),
                            ),
                            false,
                            false,
                            self.action({
                                let shell_id = shell_id.clone();
                                move |this, _event, _window, cx| {
                                    this.send(SelectMenuRequest::Close, _window, cx);
                                    let shell_id = shell_id.clone();
                                    let scheme_id = scheme_id.clone();
                                    this.send(
                                        SelectMenuRequest::Edit(Box::new(move |settings| {
                                            settings
                                                .local_terminal
                                                .semantic_scheme_by_shell
                                                .insert(shell_id.clone(), scheme_id.clone());
                                        })),
                                        _window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            }),
                        ));
                    }
                }
                Some(popup)
            }
            (
                SettingsTab::Connections,
                SettingsSelect::ConnectionIdleTimeout | SettingsSelect::ClosedSessionRetention,
            ) => {
                let retention = open_select == SettingsSelect::ClosedSessionRetention;
                let selected = if retention {
                    settings.sidebar_ui.closed_session_retention_secs
                } else {
                    settings.connection_pool.idle_timeout_secs
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for seconds in [0, 300, 900, 1800, 3600, NO_TIMEOUT_SECS] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_duration_label(open_select, seconds, &self.i18n),
                            seconds == selected,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    if retention {
                                        settings.sidebar_ui.closed_session_retention_secs = seconds;
                                    } else {
                                        settings.connection_pool.idle_timeout_secs = seconds;
                                    }
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
            (SettingsTab::Connections, SettingsSelect::ReconnectMaxAttempts) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for attempts in reconnect_max_attempt_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            attempts.to_string(),
                            attempts == settings.reconnect.max_attempts,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    set_reconnect_max_attempts(settings, attempts)
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
            (SettingsTab::Connections, SettingsSelect::ReconnectBaseDelay) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for (delay_ms, label) in reconnect_base_delay_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            delay_ms == settings.reconnect.base_delay_ms,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    set_reconnect_base_delay(settings, delay_ms)
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
            (SettingsTab::Connections, SettingsSelect::ReconnectMaxDelay) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for (delay_ms, label) in reconnect_max_delay_options() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            delay_ms == settings.reconnect.max_delay_ms,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    set_reconnect_max_delay(settings, delay_ms)
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
            (SettingsTab::Network, SettingsSelect::NetworkProxyProtocol) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| proxy.protocol)
                    .unwrap_or(SettingsUpstreamProxyProtocol::Socks5);
                for protocol in [
                    SettingsUpstreamProxyProtocol::Socks5,
                    SettingsUpstreamProxyProtocol::HttpConnect,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_proxy_protocol_label(protocol, &self.i18n),
                            protocol == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Edit(Box::new(move |settings| {
                                    if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                                        proxy.protocol = protocol;
                                    }
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
            _ => None,
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
        (_, NO_TIMEOUT_SECS) => "settings_view.connections.idle_timeout.never",
        (SettingsSelect::ClosedSessionRetention, 0) => {
            "settings_view.connections.retention.immediate"
        }
        (_, 0) => "settings_view.connections.idle_timeout.immediate",
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

pub fn application_semantic_scheme_label(settings: &PersistedSettings, i18n: &I18n) -> String {
    settings
        .terminal
        .active_custom_semantic_scheme()
        .map(|scheme| scheme.name.clone())
        .unwrap_or_else(|| terminal_semantic_scheme_label(settings.terminal.semantic_scheme, i18n))
}
