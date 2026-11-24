#[path = "external.rs"]
mod external;
pub use external::{ExternalSelect, ExternalSelectInputs, plugin_marketplace_sort_label};
#[path = "knowledge.rs"]
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
    ConnectionPoolSettings, FrostedGlassMode, NO_TIMEOUT_SECS, PersistedSettings,
    RemoteShellIntegrationMode, TerminalSemanticScheme,
};
use oxideterm_settings_model::{
    SEMANTIC_CLASSES, SemanticRuleContext, edit_custom_semantic_scheme,
    reconnect_base_delay_options, reconnect_max_attempt_options, reconnect_max_delay_options,
};
#[path = "triggers.rs"]
mod triggers;
use oxideterm_theme::ThemeTokens;

use super::{
    CoreSelectMenu, CoreSelectOwner, SelectMenuDispatch, SelectMenuRequest,
    application_semantic_scheme_label, connection_duration_label,
};
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
    *,
};

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
            (
                SettingsTab::Terminal,
                SettingsSelect::TerminalTriggerMatchMode
                | SettingsSelect::TerminalTriggerAction
                | SettingsSelect::TerminalTriggerClickAction
                | SettingsSelect::TerminalTriggerProcessMode
                | SettingsSelect::TerminalTriggerQuickCommand
                | SettingsSelect::TerminalTriggerTiming
                | SettingsSelect::TerminalTriggerScope,
            ) => self.trigger_popup(open_select, width, cx),
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
                let (immediate, never) = if retention {
                    (0, NO_TIMEOUT_SECS)
                } else {
                    (ConnectionPoolSettings::DISCONNECT_IMMEDIATELY_SECS, 0)
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for seconds in [immediate, 300, 900, 1800, 3600, never] {
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
            _ => {
                return CoreSelectMenu {
                    settings,
                    tokens: self.tokens,
                    i18n: self.i18n,
                    owner: self.settings_workspace,
                    dispatch: self.dispatch.clone(),
                }
                .render(active_tab, open_select, width, cx);
            }
        }?;
        Some(overlay_content_boundary(popup).into_any_element())
    }
}

impl CoreSelectOwner for SettingsWorkspaceEntity {
    fn custom_cjk(&self, ide: bool) -> bool {
        if ide {
            self.ide_cjk_font_custom
        } else {
            self.terminal_cjk_font_custom
        }
    }

    fn set_custom_cjk(&mut self, ide: bool, custom: bool) {
        if ide {
            self.ide_cjk_font_custom = custom;
        } else {
            self.terminal_cjk_font_custom = custom;
        }
    }

    fn duplicate_id(&self) -> Option<&str> {
        self.theme_editor()
            .map(|editor| editor.duplicate_theme.as_str())
    }

    fn duplicate(&mut self, id: &str, cx: &mut gpui::Context<Self>) {
        self.duplicate_theme_editor_from(id, cx);
    }
}
