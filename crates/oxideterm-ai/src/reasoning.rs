/// Stable reasoning levels shown by the chat reasoning menu.
///
/// These values are provider protocol identifiers. They must stay in English
/// even when the surrounding explanatory copy is localized.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AiReasoningLevel {
    Auto,
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl AiReasoningLevel {
    pub const ALL_EXPLICIT: [Self; 7] = [
        Self::None,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
        Self::Max,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::None => "None",
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Xhigh => "XHigh",
            Self::Max => "Max",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Self::None,
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" => Self::Xhigh,
            "max" => Self::Max,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiReasoningRequestFormat {
    OpenAi,
    AnthropicEffort,
    GeminiThinkingLevel,
    GeminiThinkingBudget,
    DeepSeek,
    KimiThinking,
    GlmEffort,
    GlmThinking,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiModelReasoningCapability {
    pub levels: Vec<AiReasoningLevel>,
    pub request_format: AiReasoningRequestFormat,
    pub known_model: bool,
}

#[derive(Clone, Copy)]
enum ModelMatch {
    Exact(&'static str),
    Prefix(&'static str),
    Contains(&'static str),
}

impl ModelMatch {
    fn matches(self, model: &str) -> bool {
        let model = model.to_ascii_lowercase();
        match self {
            Self::Exact(value) => model == value,
            Self::Prefix(value) => model.starts_with(value),
            Self::Contains(value) => model.contains(value),
        }
    }
}

struct CapabilityRow {
    provider_type: &'static str,
    model_match: ModelMatch,
    levels: &'static [AiReasoningLevel],
    request_format: AiReasoningRequestFormat,
}

use AiReasoningLevel::{High, Low, Max, Medium, Minimal, None, Xhigh};

const NONE: &[AiReasoningLevel] = &[];
const NONE_LOW_HIGH_XHIGH_MAX: &[AiReasoningLevel] = &[None, Low, High, Xhigh, Max];
const NONE_LOW_MEDIUM_HIGH: &[AiReasoningLevel] = &[None, Low, Medium, High];
const NONE_LOW_MEDIUM_HIGH_XHIGH: &[AiReasoningLevel] = &[None, Low, Medium, High, Xhigh];
const NONE_LOW_MEDIUM_HIGH_XHIGH_MAX: &[AiReasoningLevel] = &[None, Low, Medium, High, Xhigh, Max];
const ALL_REASONING_LEVELS: &[AiReasoningLevel] = &[None, Minimal, Low, Medium, High, Xhigh, Max];
const MINIMAL_LOW_MEDIUM_HIGH: &[AiReasoningLevel] = &[Minimal, Low, Medium, High];
const LOW_MEDIUM_HIGH: &[AiReasoningLevel] = &[Low, Medium, High];
const LOW_MEDIUM_HIGH_XHIGH_MAX: &[AiReasoningLevel] = &[Low, Medium, High, Xhigh, Max];
const LOW_MEDIUM_HIGH_MAX: &[AiReasoningLevel] = &[Low, Medium, High, Max];
const MEDIUM_HIGH_XHIGH: &[AiReasoningLevel] = &[Medium, High, Xhigh];
const HIGH_ONLY: &[AiReasoningLevel] = &[High];
const LOW_HIGH: &[AiReasoningLevel] = &[Low, High];
const LOW_HIGH_MAX: &[AiReasoningLevel] = &[Low, High, Max];
const MINIMAL_HIGH: &[AiReasoningLevel] = &[Minimal, High];

// The table is intentionally ordered from the most specific rule to the
// broadest prefix. Sources were verified against provider documentation on
// 2026-08-12; unknown future models fall back to user-selectable raw levels.
const CAPABILITY_ROWS: &[CapabilityRow] = &[
    // https://developers.openai.com/api/docs/models/gpt-5.4-pro
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-5.4-pro"),
        levels: MEDIUM_HIGH_XHIGH,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    // https://developers.openai.com/api/docs/guides/latest-model
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-5.6"),
        levels: NONE_LOW_MEDIUM_HIGH_XHIGH_MAX,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    // https://developers.openai.com/api/docs/models
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-5.5"),
        levels: NONE_LOW_MEDIUM_HIGH_XHIGH,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    // https://developers.openai.com/api/docs/models/gpt-5.4
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-5.4"),
        levels: NONE_LOW_MEDIUM_HIGH_XHIGH,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    // https://platform.openai.com/docs/api-reference/graders
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-5.1"),
        levels: NONE_LOW_MEDIUM_HIGH,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-5-pro"),
        levels: HIGH_ONLY,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    // https://developers.openai.com/api/docs/models/gpt-5
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-5"),
        levels: MINIMAL_LOW_MEDIUM_HIGH,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    CapabilityRow {
        provider_type: "openai",
        model_match: ModelMatch::Prefix("gpt-4"),
        levels: NONE,
        request_format: AiReasoningRequestFormat::Unsupported,
    },
    // https://platform.claude.com/docs/en/build-with-claude/effort
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("sonnet-5"),
        levels: LOW_MEDIUM_HIGH_XHIGH_MAX,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("opus-4-8"),
        levels: LOW_MEDIUM_HIGH_XHIGH_MAX,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("opus-4-7"),
        levels: LOW_MEDIUM_HIGH_XHIGH_MAX,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("fable-5"),
        levels: LOW_MEDIUM_HIGH_XHIGH_MAX,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("mythos-5"),
        levels: LOW_MEDIUM_HIGH_XHIGH_MAX,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("opus-4-6"),
        levels: LOW_MEDIUM_HIGH_MAX,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("sonnet-4-6"),
        levels: LOW_MEDIUM_HIGH_MAX,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Contains("opus-4-5"),
        levels: LOW_MEDIUM_HIGH,
        request_format: AiReasoningRequestFormat::AnthropicEffort,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Exact("claude-sonnet-4-20250514"),
        levels: NONE,
        request_format: AiReasoningRequestFormat::Unsupported,
    },
    CapabilityRow {
        provider_type: "anthropic",
        model_match: ModelMatch::Prefix("claude-3"),
        levels: NONE,
        request_format: AiReasoningRequestFormat::Unsupported,
    },
    // https://ai.google.dev/gemini-api/docs/thinking
    CapabilityRow {
        provider_type: "gemini",
        model_match: ModelMatch::Prefix("gemini-3.1-flash-lite-image"),
        levels: MINIMAL_HIGH,
        request_format: AiReasoningRequestFormat::GeminiThinkingLevel,
    },
    CapabilityRow {
        provider_type: "gemini",
        model_match: ModelMatch::Prefix("gemini-3-pro"),
        levels: LOW_HIGH,
        request_format: AiReasoningRequestFormat::GeminiThinkingLevel,
    },
    CapabilityRow {
        provider_type: "gemini",
        model_match: ModelMatch::Prefix("gemini-3.1-pro"),
        levels: LOW_MEDIUM_HIGH,
        request_format: AiReasoningRequestFormat::GeminiThinkingLevel,
    },
    CapabilityRow {
        provider_type: "gemini",
        model_match: ModelMatch::Prefix("gemini-3"),
        levels: MINIMAL_LOW_MEDIUM_HIGH,
        request_format: AiReasoningRequestFormat::GeminiThinkingLevel,
    },
    CapabilityRow {
        provider_type: "gemini",
        model_match: ModelMatch::Prefix("gemini-2.5"),
        levels: NONE_LOW_MEDIUM_HIGH,
        request_format: AiReasoningRequestFormat::GeminiThinkingBudget,
    },
    CapabilityRow {
        provider_type: "gemini",
        model_match: ModelMatch::Prefix("gemini-2.0"),
        levels: NONE,
        request_format: AiReasoningRequestFormat::Unsupported,
    },
    // https://api-docs.deepseek.com/guides/thinking_mode
    CapabilityRow {
        provider_type: "deepseek",
        model_match: ModelMatch::Prefix("deepseek-v4"),
        levels: NONE_LOW_HIGH_XHIGH_MAX,
        request_format: AiReasoningRequestFormat::DeepSeek,
    },
    // https://platform.kimi.com/docs/guide/use-reasoning-effort
    CapabilityRow {
        provider_type: "kimi",
        model_match: ModelMatch::Prefix("kimi-k3"),
        levels: LOW_HIGH_MAX,
        request_format: AiReasoningRequestFormat::OpenAi,
    },
    // K2.7 Code always reasons and rejects the `thinking` parameter.
    // https://platform.kimi.com/docs/guide/use-thinking-models
    CapabilityRow {
        provider_type: "kimi",
        model_match: ModelMatch::Prefix("kimi-k2.7-code"),
        levels: NONE,
        request_format: AiReasoningRequestFormat::Unsupported,
    },
    // K2.6 and K2.5 default to thinking and only expose an explicit disable.
    // Auto therefore represents their documented enabled default.
    CapabilityRow {
        provider_type: "kimi",
        model_match: ModelMatch::Prefix("kimi-k2.6"),
        levels: &[None],
        request_format: AiReasoningRequestFormat::KimiThinking,
    },
    CapabilityRow {
        provider_type: "kimi",
        model_match: ModelMatch::Prefix("kimi-k2.5"),
        levels: &[None],
        request_format: AiReasoningRequestFormat::KimiThinking,
    },
    // https://docs.bigmodel.cn/cn/guide/capabilities/thinking
    CapabilityRow {
        provider_type: "glm",
        model_match: ModelMatch::Prefix("glm-5.2"),
        levels: ALL_REASONING_LEVELS,
        request_format: AiReasoningRequestFormat::GlmEffort,
    },
    // GLM-4.5 through GLM-5.1 expose an enabled/disabled thinking switch,
    // while Auto represents each model's documented enabled default.
    CapabilityRow {
        provider_type: "glm",
        model_match: ModelMatch::Prefix("glm-5.1"),
        levels: &[None],
        request_format: AiReasoningRequestFormat::GlmThinking,
    },
    CapabilityRow {
        provider_type: "glm",
        model_match: ModelMatch::Prefix("glm-5"),
        levels: &[None],
        request_format: AiReasoningRequestFormat::GlmThinking,
    },
    CapabilityRow {
        provider_type: "glm",
        model_match: ModelMatch::Prefix("glm-4.7"),
        levels: &[None],
        request_format: AiReasoningRequestFormat::GlmThinking,
    },
    CapabilityRow {
        provider_type: "glm",
        model_match: ModelMatch::Prefix("glm-4.6"),
        levels: &[None],
        request_format: AiReasoningRequestFormat::GlmThinking,
    },
    CapabilityRow {
        provider_type: "glm",
        model_match: ModelMatch::Prefix("glm-4.5"),
        levels: &[None],
        request_format: AiReasoningRequestFormat::GlmThinking,
    },
    CapabilityRow {
        provider_type: "glm",
        model_match: ModelMatch::Prefix("glm-4"),
        levels: NONE,
        request_format: AiReasoningRequestFormat::Unsupported,
    },
];

pub fn model_reasoning_capability(provider_type: &str, model: &str) -> AiModelReasoningCapability {
    if let Some(row) = CAPABILITY_ROWS
        .iter()
        .find(|row| row.provider_type == provider_type && row.model_match.matches(model))
    {
        return AiModelReasoningCapability {
            levels: row.levels.to_vec(),
            request_format: row.request_format,
            known_model: true,
        };
    }

    let request_format = match provider_type {
        "openai" | "openai_compatible" => AiReasoningRequestFormat::OpenAi,
        "anthropic" => AiReasoningRequestFormat::AnthropicEffort,
        "gemini" => AiReasoningRequestFormat::GeminiThinkingLevel,
        "deepseek" => AiReasoningRequestFormat::DeepSeek,
        "kimi" => AiReasoningRequestFormat::OpenAi,
        "glm" => AiReasoningRequestFormat::GlmEffort,
        _ => AiReasoningRequestFormat::Unsupported,
    };
    let levels = if request_format == AiReasoningRequestFormat::Unsupported {
        // Ollama documents `think` for its native `/api/chat` endpoint. OxideTerm
        // currently uses Ollama's OpenAI-compatible endpoint, so no control is
        // exposed until that transport can send the documented parameter.
        // https://docs.ollama.com/capabilities/thinking
        Vec::new()
    } else {
        AiReasoningLevel::ALL_EXPLICIT.to_vec()
    };
    AiModelReasoningCapability {
        levels,
        request_format,
        known_model: false,
    }
}

pub fn normalize_reasoning_level_for_model(
    provider_type: &str,
    model: &str,
    value: &str,
) -> AiReasoningLevel {
    let requested = AiReasoningLevel::parse(value);
    if requested == AiReasoningLevel::Auto {
        return requested;
    }
    let capability = model_reasoning_capability(provider_type, model);
    if capability.known_model && !capability.levels.contains(&requested) {
        AiReasoningLevel::Auto
    } else {
        requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models_reject_unsupported_levels_without_guessing() {
        assert_eq!(
            normalize_reasoning_level_for_model("deepseek", "deepseek-v4-pro", "medium"),
            AiReasoningLevel::Auto
        );
        assert_eq!(
            normalize_reasoning_level_for_model("deepseek", "deepseek-v4-pro", "xhigh"),
            AiReasoningLevel::Xhigh
        );
        assert_eq!(
            normalize_reasoning_level_for_model(
                "openai_compatible",
                "vendor-future-reasoner",
                "medium"
            ),
            AiReasoningLevel::Medium
        );
    }
}
