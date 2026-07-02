// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings input draft conversion for persisted settings.
//!
//! GPUI owns focus, caret, and IME state. This module owns the settings-domain
//! mapping between an input identity, its displayed value, and the validated
//! mutation applied to `PersistedSettings`.

use oxideterm_ai::{
    model_context_window_info, provider_id as ai_provider_id, provider_string as ai_provider_string,
};
use oxideterm_settings::{
    DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND, DEFAULT_AI_TOOL_MAX_ROUNDS,
    MAX_AI_TOOL_MAX_CALLS_PER_ROUND, MAX_AI_TOOL_MAX_ROUNDS, MIN_AI_TOOL_MAX_CALLS_PER_ROUND,
    MIN_AI_TOOL_MAX_ROUNDS, PersistedSettings, SettingsUpstreamProxyAuth, UpdateProxyMode,
    reindex_highlight_rules,
};

use crate::{
    SettingsInput, ai_update_provider, parse_focus_handoff_command_list,
    set_ai_model_max_response_tokens, set_ai_user_context_window,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsInputDraftApply {
    Applied,
    Invalid,
    Unhandled,
}

pub fn persisted_settings_input_value(
    settings: &PersistedSettings,
    input: SettingsInput,
) -> Option<String> {
    let value = match input {
        SettingsInput::TerminalCustomFontFamily => settings.terminal.custom_font_family.clone(),
        SettingsInput::TerminalFontSize => settings.terminal.font_size.to_string(),
        SettingsInput::TerminalLineHeight => compact_decimal(settings.terminal.line_height),
        SettingsInput::IdeFontSize => settings
            .ide
            .font_size
            .map(|value| value.to_string())
            .unwrap_or_default(),
        SettingsInput::IdeLineHeight => settings
            .ide
            .line_height
            .map(compact_decimal)
            .unwrap_or_default(),
        SettingsInput::AppearanceUiFont => settings.appearance.ui_font_family.clone(),
        SettingsInput::LocalDefaultCwd => settings
            .local_terminal
            .default_cwd
            .clone()
            .unwrap_or_default(),
        SettingsInput::LocalGitBashPath => settings
            .local_terminal
            .git_bash_path
            .clone()
            .unwrap_or_default(),
        SettingsInput::LocalOhMyPoshTheme => settings
            .local_terminal
            .oh_my_posh_theme
            .clone()
            .unwrap_or_default(),
        SettingsInput::ConnectionDefaultUsername => settings.connection_defaults.username.clone(),
        SettingsInput::ConnectionDefaultPort => settings.connection_defaults.port.to_string(),
        SettingsInput::ConnectionImportTargetGroup => return None,
        SettingsInput::NetworkProxyHost => settings
            .network
            .upstream_proxy
            .as_ref()
            .map(|proxy| proxy.host.clone())
            .unwrap_or_default(),
        SettingsInput::NetworkProxyPort => settings
            .network
            .upstream_proxy
            .as_ref()
            .map(|proxy| proxy.port.to_string())
            .unwrap_or_else(|| "1080".to_string()),
        SettingsInput::NetworkProxyNoProxy => settings
            .network
            .upstream_proxy
            .as_ref()
            .map(|proxy| proxy.no_proxy.clone())
            .unwrap_or_default(),
        SettingsInput::NetworkProxyUsername => settings
            .network
            .upstream_proxy
            .as_ref()
            .and_then(|proxy| match &proxy.auth {
                SettingsUpstreamProxyAuth::Password { username, .. } => Some(username.clone()),
                SettingsUpstreamProxyAuth::None => None,
            })
            .unwrap_or_default(),
        SettingsInput::NetworkProxyPassword => String::new(),
        SettingsInput::NetworkProxyTestHost | SettingsInput::NetworkProxyTestPort => return None,
        SettingsInput::UpdateProxyHost => settings.general.update_proxy.host.clone(),
        SettingsInput::UpdateProxyPort => settings.general.update_proxy.port.to_string(),
        SettingsInput::UpdateProxyNoProxy => settings.general.update_proxy.no_proxy.clone(),
        SettingsInput::SftpSpeedLimitKbps => settings.sftp.speed_limit_kbps.to_string(),
        SettingsInput::InBandTransferMaxChunkBytes => settings
            .terminal
            .in_band_transfer
            .max_chunk_bytes
            .to_string(),
        SettingsInput::InBandTransferMaxFileCount => settings
            .terminal
            .in_band_transfer
            .max_file_count
            .to_string(),
        SettingsInput::InBandTransferMaxTotalBytes => settings
            .terminal
            .in_band_transfer
            .max_total_bytes
            .to_string(),
        SettingsInput::TerminalCommandBarFocusHandoff => settings
            .terminal
            .command_bar
            .focus_handoff_commands
            .join("\n"),
        SettingsInput::HighlightLabel(index) => settings
            .terminal
            .highlight_rules
            .get(index)
            .map(|rule| rule.label.clone())
            .unwrap_or_default(),
        SettingsInput::HighlightPattern(index) => settings
            .terminal
            .highlight_rules
            .get(index)
            .map(|rule| rule.pattern.clone())
            .unwrap_or_default(),
        SettingsInput::HighlightForeground(index) => settings
            .terminal
            .highlight_rules
            .get(index)
            .and_then(|rule| rule.foreground.clone())
            .unwrap_or_default(),
        SettingsInput::HighlightBackground(index) => settings
            .terminal
            .highlight_rules
            .get(index)
            .and_then(|rule| rule.background.clone())
            .unwrap_or_default(),
        SettingsInput::AiProviderName(index) => settings
            .ai
            .providers
            .get(index)
            .and_then(|provider| ai_provider_string(provider, "name"))
            .unwrap_or_default(),
        SettingsInput::AiProviderBaseUrl(index) => settings
            .ai
            .providers
            .get(index)
            .and_then(|provider| ai_provider_string(provider, "baseUrl"))
            .unwrap_or_default(),
        SettingsInput::AiProviderDefaultModel(index) => settings
            .ai
            .providers
            .get(index)
            .and_then(|provider| ai_provider_string(provider, "defaultModel"))
            .unwrap_or_default(),
        SettingsInput::AiProviderApiKey(_) => String::new(),
        SettingsInput::AiAcpAgentDisplayName(index) => settings
            .ai
            .acp_agents
            .get(index)
            .map(|agent| agent.display_name.clone())
            .unwrap_or_default(),
        SettingsInput::AiAcpAgentCommand(index) => settings
            .ai
            .acp_agents
            .get(index)
            .map(|agent| agent.command.clone())
            .unwrap_or_default(),
        SettingsInput::AiAcpAgentCwd(index) => settings
            .ai
            .acp_agents
            .get(index)
            .and_then(|agent| agent.cwd.clone())
            .unwrap_or_default(),
        SettingsInput::AiAcpAgentArgs(index) => settings
            .ai
            .acp_agents
            .get(index)
            .map(|agent| agent.args.join("\n"))
            .unwrap_or_default(),
        SettingsInput::AiAcpAgentEnv(index) => settings
            .ai
            .acp_agents
            .get(index)
            .map(|agent| {
                agent
                    .env
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        SettingsInput::AiAcpAgentAuthToken(_) => String::new(),
        SettingsInput::AiSystemPrompt => settings.ai.custom_system_prompt.clone(),
        SettingsInput::AiMemoryContent => settings.ai.memory.content.clone(),
        SettingsInput::AiToolUseMaxRounds => settings
            .ai
            .tool_use
            .max_rounds
            .unwrap_or(DEFAULT_AI_TOOL_MAX_ROUNDS)
            .to_string(),
        SettingsInput::AiToolUseMaxCallsPerRound => settings
            .ai
            .tool_use
            .max_calls_per_round
            .unwrap_or(DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND)
            .to_string(),
        SettingsInput::AiModelContextWindow(provider_index, model_index) => settings
            .ai
            .providers
            .get(provider_index)
            .and_then(ai_provider_id)
            .and_then(|provider_id| {
                let model = provider_model(settings, provider_index, model_index)?;
                settings
                    .ai
                    .user_context_windows
                    .get(&provider_id)
                    .and_then(|windows| windows.get(&model))
                    .and_then(serde_json::Value::as_i64)
                    .or_else(|| {
                        Some(
                            model_context_window_info(
                                &model,
                                &settings.ai.model_context_windows,
                                Some(&provider_id),
                                &settings.ai.user_context_windows,
                            )
                            .value,
                        )
                    })
                    .map(|value| value.to_string())
            })
            .unwrap_or_default(),
        SettingsInput::AiActiveModelMaxResponseTokens => settings
            .ai
            .active_provider_id
            .as_ref()
            .zip(settings.ai.active_model.as_ref())
            .and_then(|(provider_id, model)| {
                settings
                    .ai
                    .model_max_response_tokens
                    .get(provider_id)
                    .and_then(|models| models.get(model))
                    .and_then(serde_json::Value::as_i64)
            })
            .map(|value| value.to_string())
            .unwrap_or_default(),
        SettingsInput::AiEmbeddingModel => settings
            .ai
            .embedding_config
            .as_ref()
            .and_then(|config| config.get("model"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => return None,
    };
    Some(value)
}

pub fn apply_persisted_settings_input_draft(
    settings: &mut PersistedSettings,
    input: SettingsInput,
    draft: &str,
) -> SettingsInputDraftApply {
    match input {
        SettingsInput::TerminalCustomFontFamily => {
            settings.terminal.custom_font_family = draft.trim().to_string();
            SettingsInputDraftApply::Applied
        }
        SettingsInput::TerminalFontSize => parse_i64(draft)
            .map(|value| settings.terminal.font_size = value.clamp(8, 32))
            .into(),
        SettingsInput::TerminalLineHeight => parse_f64(draft)
            .map(|value| settings.terminal.line_height = value.clamp(0.8, 2.0))
            .into(),
        SettingsInput::IdeFontSize => {
            let value = draft.trim();
            if value.is_empty() {
                settings.ide.font_size = None;
                SettingsInputDraftApply::Applied
            } else {
                parse_i64(value)
                    .map(|value| settings.ide.font_size = Some(value.clamp(8, 32)))
                    .into()
            }
        }
        SettingsInput::IdeLineHeight => {
            let value = draft.trim();
            if value.is_empty() {
                settings.ide.line_height = None;
                SettingsInputDraftApply::Applied
            } else {
                parse_f64(value)
                    .map(|value| settings.ide.line_height = Some(value.clamp(0.8, 3.0)))
                    .into()
            }
        }
        SettingsInput::AppearanceUiFont => {
            settings.appearance.ui_font_family = draft.trim().to_string();
            SettingsInputDraftApply::Applied
        }
        SettingsInput::LocalDefaultCwd => {
            settings.local_terminal.default_cwd = non_empty_trimmed(draft);
            SettingsInputDraftApply::Applied
        }
        SettingsInput::LocalGitBashPath => {
            settings.local_terminal.git_bash_path = non_empty_trimmed(draft);
            SettingsInputDraftApply::Applied
        }
        SettingsInput::LocalOhMyPoshTheme => {
            settings.local_terminal.oh_my_posh_theme = non_empty_trimmed(draft);
            SettingsInputDraftApply::Applied
        }
        SettingsInput::ConnectionDefaultUsername => {
            settings.connection_defaults.username = draft.trim().to_string();
            SettingsInputDraftApply::Applied
        }
        SettingsInput::ConnectionDefaultPort => parse_i64(draft)
            .map(|value| settings.connection_defaults.port = value.clamp(1, 65_535))
            .into(),
        SettingsInput::ConnectionImportTargetGroup => SettingsInputDraftApply::Unhandled,
        SettingsInput::NetworkProxyHost => {
            edit_upstream_proxy(settings, |proxy| proxy.host = draft.trim().to_string())
        }
        SettingsInput::NetworkProxyPort => parse_i64(draft)
            .map(|value| {
                edit_upstream_proxy(settings, |proxy| proxy.port = value.clamp(1, 65_535) as u16);
            })
            .into(),
        SettingsInput::NetworkProxyNoProxy => {
            edit_upstream_proxy(settings, |proxy| proxy.no_proxy = draft.trim().to_string())
        }
        SettingsInput::NetworkProxyUsername => edit_upstream_proxy(settings, |proxy| {
            proxy.auth = SettingsUpstreamProxyAuth::Password {
                username: draft.trim().to_string(),
                keychain_id: match &proxy.auth {
                    SettingsUpstreamProxyAuth::Password { keychain_id, .. } => keychain_id.clone(),
                    SettingsUpstreamProxyAuth::None => None,
                },
            };
        }),
        SettingsInput::NetworkProxyPassword => SettingsInputDraftApply::Unhandled,
        SettingsInput::NetworkProxyTestHost | SettingsInput::NetworkProxyTestPort => {
            SettingsInputDraftApply::Unhandled
        }
        SettingsInput::UpdateProxyHost => {
            settings.general.update_proxy.host = draft.trim().to_string();
            SettingsInputDraftApply::Applied
        }
        SettingsInput::UpdateProxyPort => parse_i64(draft)
            .map(|value| {
                settings.general.update_proxy.port = value.clamp(1, 65_535) as u16;
                settings.general.update_proxy.mode = UpdateProxyMode::Custom;
            })
            .into(),
        SettingsInput::UpdateProxyNoProxy => {
            settings.general.update_proxy.no_proxy = draft.trim().to_string();
            SettingsInputDraftApply::Applied
        }
        SettingsInput::SftpSpeedLimitKbps => parse_i64(draft)
            .map(|value| settings.sftp.speed_limit_kbps = value.max(0))
            .into(),
        SettingsInput::InBandTransferMaxChunkBytes => parse_i64(draft)
            .map(|value| settings.terminal.in_band_transfer.max_chunk_bytes = value.max(1024))
            .into(),
        SettingsInput::InBandTransferMaxFileCount => parse_i64(draft)
            .map(|value| settings.terminal.in_band_transfer.max_file_count = value.max(1))
            .into(),
        SettingsInput::InBandTransferMaxTotalBytes => parse_i64(draft)
            .map(|value| settings.terminal.in_band_transfer.max_total_bytes = value.max(1024))
            .into(),
        SettingsInput::TerminalCommandBarFocusHandoff => {
            settings.terminal.command_bar.focus_handoff_commands =
                parse_focus_handoff_command_list(draft);
            SettingsInputDraftApply::Applied
        }
        SettingsInput::HighlightLabel(index) => edit_highlight_rule(settings, index, |rule| {
            rule.label = draft.trim().to_string()
        }),
        SettingsInput::HighlightPattern(index) => edit_highlight_rule(settings, index, |rule| {
            rule.pattern = draft.trim().to_string()
        }),
        SettingsInput::HighlightForeground(index) => edit_highlight_rule(settings, index, |rule| {
            rule.foreground = non_empty_trimmed(draft);
        }),
        SettingsInput::HighlightBackground(index) => edit_highlight_rule(settings, index, |rule| {
            rule.background = non_empty_trimmed(draft);
        }),
        SettingsInput::AiProviderName(index) => {
            set_ai_provider_string(settings, index, "name", draft.trim())
        }
        SettingsInput::AiProviderBaseUrl(index) => {
            set_ai_provider_string(settings, index, "baseUrl", draft.trim())
        }
        SettingsInput::AiProviderDefaultModel(index) => {
            set_ai_provider_string(settings, index, "defaultModel", draft.trim())
        }
        SettingsInput::AiAcpAgentDisplayName(index) => {
            if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                agent.display_name = draft.trim().to_string();
            }
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiAcpAgentCommand(index) => {
            if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                agent.command = draft.trim().to_string();
            }
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiAcpAgentCwd(index) => {
            if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                let cwd = draft.trim();
                agent.cwd = if cwd.is_empty() {
                    None
                } else {
                    Some(cwd.to_string())
                };
            }
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiAcpAgentArgs(index) => {
            if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                agent.args = draft
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiAcpAgentEnv(index) => {
            if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                agent.env = draft
                    .lines()
                    .filter_map(|line| {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            return None;
                        }
                        let (key, value) = trimmed.split_once('=').unwrap_or((trimmed, ""));
                        let key = key.trim();
                        if key.is_empty() {
                            None
                        } else {
                            Some((key.to_string(), value.to_string()))
                        }
                    })
                    .collect();
            }
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiAcpAgentAuthToken(_) => SettingsInputDraftApply::Unhandled,
        SettingsInput::AiSystemPrompt => {
            settings.ai.custom_system_prompt = draft.to_string();
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiMemoryContent => {
            settings.ai.memory.content = draft.to_string();
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiToolUseMaxRounds => parse_i64(draft.trim())
            .map(|value| {
                settings.ai.tool_use.max_rounds =
                    Some(value.clamp(MIN_AI_TOOL_MAX_ROUNDS, MAX_AI_TOOL_MAX_ROUNDS));
            })
            .into(),
        SettingsInput::AiToolUseMaxCallsPerRound => parse_i64(draft.trim())
            .map(|value| {
                settings.ai.tool_use.max_calls_per_round = Some(value.clamp(
                    MIN_AI_TOOL_MAX_CALLS_PER_ROUND,
                    MAX_AI_TOOL_MAX_CALLS_PER_ROUND,
                ));
            })
            .into(),
        SettingsInput::AiModelContextWindow(provider_index, model_index) => {
            let Some(provider_id) = settings
                .ai
                .providers
                .get(provider_index)
                .and_then(ai_provider_id)
            else {
                return SettingsInputDraftApply::Applied;
            };
            let Some(model) = provider_model(settings, provider_index, model_index) else {
                return SettingsInputDraftApply::Applied;
            };
            set_ai_user_context_window(settings, &provider_id, &model, draft.trim().parse().ok());
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiActiveModelMaxResponseTokens => {
            let Some(provider_id) = settings.ai.active_provider_id.clone() else {
                return SettingsInputDraftApply::Applied;
            };
            let Some(model) = settings.ai.active_model.clone() else {
                return SettingsInputDraftApply::Applied;
            };
            set_ai_model_max_response_tokens(
                settings,
                &provider_id,
                &model,
                draft.trim().parse().ok(),
            );
            SettingsInputDraftApply::Applied
        }
        SettingsInput::AiEmbeddingModel => {
            let value = draft.trim().to_string();
            let mut config = settings
                .ai
                .embedding_config
                .take()
                .unwrap_or_else(|| serde_json::json!({ "providerId": null, "model": "" }));
            if let Some(object) = config.as_object_mut() {
                object.insert("model".to_string(), serde_json::json!(value));
            }
            settings.ai.embedding_config = Some(config);
            SettingsInputDraftApply::Applied
        }
        _ => SettingsInputDraftApply::Unhandled,
    }
}

fn provider_model(
    settings: &PersistedSettings,
    provider_index: usize,
    model_index: usize,
) -> Option<String> {
    settings
        .ai
        .providers
        .get(provider_index)
        .and_then(|provider| provider.get("models"))
        .and_then(serde_json::Value::as_array)
        .and_then(|models| models.get(model_index))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn set_ai_provider_string(
    settings: &mut PersistedSettings,
    index: usize,
    key: &'static str,
    value: &str,
) -> SettingsInputDraftApply {
    ai_update_provider(settings, index, |provider| {
        provider.insert(key.to_string(), serde_json::json!(value));
    });
    SettingsInputDraftApply::Applied
}

fn edit_highlight_rule(
    settings: &mut PersistedSettings,
    index: usize,
    edit: impl FnOnce(&mut oxideterm_settings::HighlightRule),
) -> SettingsInputDraftApply {
    let Some(rule) = settings.terminal.highlight_rules.get_mut(index) else {
        return SettingsInputDraftApply::Applied;
    };
    edit(rule);
    settings.terminal.highlight_rules =
        reindex_highlight_rules(settings.terminal.highlight_rules.clone());
    SettingsInputDraftApply::Applied
}

fn edit_upstream_proxy(
    settings: &mut PersistedSettings,
    edit: impl FnOnce(&mut oxideterm_settings::SettingsUpstreamProxyConfig),
) -> SettingsInputDraftApply {
    if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
        edit(proxy);
    }
    SettingsInputDraftApply::Applied
}

fn non_empty_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_i64(value: &str) -> Option<i64> {
    value.parse::<i64>().ok()
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn compact_decimal(value: f64) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

impl From<Option<()>> for SettingsInputDraftApply {
    fn from(value: Option<()>) -> Self {
        if value.is_some() {
            Self::Applied
        } else {
            Self::Invalid
        }
    }
}

/// Splits a settings multiline input into visual lines with UTF-16 ranges.
///
/// GPUI IME selections are tracked in UTF-16 code units to match browser input
/// semantics, so the model layer owns this conversion instead of each view.
pub fn settings_multiline_line_ranges(value: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut ranges = Vec::new();
    let mut utf16_start = 0usize;
    let mut utf16_offset = 0usize;
    let mut byte_start = 0usize;

    for (byte_index, ch) in value.char_indices() {
        if ch == '\n' {
            ranges.push((
                utf16_start..utf16_offset,
                value[byte_start..byte_index].to_string(),
            ));
            utf16_offset += ch.len_utf16();
            utf16_start = utf16_offset;
            byte_start = byte_index + ch.len_utf8();
        } else {
            utf16_offset += ch.len_utf16();
        }
    }

    ranges.push((utf16_start..utf16_offset, value[byte_start..].to_string()));
    ranges
}

/// Maps a global UTF-16 selection into a single rendered settings text line.
///
/// The returned caret offset is separated from the selection range because an
/// empty browser selection renders as a caret, while a non-empty range renders
/// highlighted segments.
pub fn settings_multiline_line_selection(
    selection: Option<&std::ops::Range<usize>>,
    line_range: &std::ops::Range<usize>,
) -> (Option<std::ops::Range<usize>>, Option<usize>) {
    let Some(selection) = selection else {
        return (None, None);
    };

    if selection.start == selection.end {
        let caret = selection.start;
        if caret >= line_range.start && caret <= line_range.end {
            return (None, Some(caret.saturating_sub(line_range.start)));
        }
        return (None, None);
    }

    let start = selection.start.max(line_range.start);
    let end = selection.end.min(line_range.end);
    if start < end {
        (
            Some(start.saturating_sub(line_range.start)..end.saturating_sub(line_range.start)),
            None,
        )
    } else {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_number_drafts_clamp_in_model_layer() {
        let mut settings = PersistedSettings::default();

        assert_eq!(
            apply_persisted_settings_input_draft(
                &mut settings,
                SettingsInput::TerminalFontSize,
                "200",
            ),
            SettingsInputDraftApply::Applied
        );

        assert_eq!(settings.terminal.font_size, 32);
    }

    #[test]
    fn terminal_custom_font_draft_updates_custom_font_family() {
        let mut settings = PersistedSettings::default();

        assert_eq!(
            apply_persisted_settings_input_draft(
                &mut settings,
                SettingsInput::TerminalCustomFontFamily,
                "  'Sarasa Fixed SC', monospace  ",
            ),
            SettingsInputDraftApply::Applied
        );

        assert_eq!(
            settings.terminal.custom_font_family,
            "'Sarasa Fixed SC', monospace"
        );
    }

    #[test]
    fn invalid_persisted_number_draft_is_reported_without_mutation() {
        let mut settings = PersistedSettings::default();
        let original = settings.connection_defaults.port;

        assert_eq!(
            apply_persisted_settings_input_draft(
                &mut settings,
                SettingsInput::ConnectionDefaultPort,
                "not-a-port",
            ),
            SettingsInputDraftApply::Invalid
        );

        assert_eq!(settings.connection_defaults.port, original);
    }

    #[test]
    fn persisted_input_value_formats_optional_decimals() {
        let mut settings = PersistedSettings::default();
        settings.ide.line_height = Some(1.5);

        assert_eq!(
            persisted_settings_input_value(&settings, SettingsInput::IdeLineHeight).as_deref(),
            Some("1.5")
        );
    }

    #[test]
    fn multiline_textarea_ranges_keep_trailing_empty_line() {
        let ranges = settings_multiline_line_ranges("vim\n");

        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], (0..3, "vim".to_string()));
        assert_eq!(ranges[1], (4..4, String::new()));
    }

    #[test]
    fn multiline_textarea_selection_maps_global_caret_to_line_offset() {
        let caret = 5..5;
        let (_selection, caret_offset) = settings_multiline_line_selection(Some(&caret), &(4..8));

        assert_eq!(caret_offset, Some(1));
    }
}
