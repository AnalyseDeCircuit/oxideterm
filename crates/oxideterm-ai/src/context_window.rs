use std::sync::LazyLock;

use regex::Regex;

pub const DEFAULT_CONTEXT_WINDOW: i64 = 8_192;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextWindowSource {
    User,
    Api,
    Pattern,
    Name,
    Default,
}

impl ContextWindowSource {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::User => "settings_view.ai.ctx_source_user",
            Self::Api => "settings_view.ai.ctx_source_api",
            Self::Pattern => "settings_view.ai.ctx_source_pattern",
            Self::Name => "settings_view.ai.ctx_source_name",
            Self::Default => "settings_view.ai.ctx_source_default",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelContextWindowInfo {
    pub value: i64,
    pub source: ContextWindowSource,
}

static MODEL_CONTEXT_WINDOWS: LazyLock<Vec<(Regex, i64)>> = LazyLock::new(|| {
    [
        (r"gpt-4\.1", 1_048_576),
        (r"o[3-9][-.]|o[3-9]$", 200_000),
        (r"o[1-2][-.]|o[1-2]$", 200_000),
        (r"gpt-4o-mini", 128_000),
        (r"gpt-4-turbo|gpt-4o", 128_000),
        (r"gpt-4-32k", 32_768),
        (r"gpt-4($|[^o-])", 8_192),
        (r"gpt-3\.5-turbo-16k", 16_384),
        (r"gpt-3\.5", 4_096),
        (r"claude-4|claude-3\.7|claude-3\.6", 200_000),
        (r"claude-3|claude-sonnet|claude-opus|claude-haiku", 200_000),
        (r"claude-2", 100_000),
        (r"claude", 200_000),
        (r"gemini-2\.5|gemini-2|gemini-1\.5", 1_048_576),
        (r"gemini", 128_000),
        (r"llama-?4", 1_048_576),
        (r"llama-?3\.1|llama-?3\.2|llama-?3\.3", 128_000),
        (r"llama-?3", 8_192),
        (r"llama", 4_096),
        (r"mistral-large|mistral-medium", 128_000),
        (r"mixtral", 32_000),
        (r"mistral", 32_000),
        (r"qwen-?3|qwen3|qwen-?2\.5|qwen2\.5|qwen-max", 128_000),
        (r"qwen", 32_000),
        (r"deepseek-v4", 1_048_576),
        (r"deepseek-v3|deepseek-r1", 128_000),
        (r"deepseek", 128_000),
        (r"moonshot", 128_000),
        (r"glm-4", 128_000),
        (r"glm", 32_000),
        (r"ernie", 8_192),
        (r"doubao", 128_000),
        (r"minimax|abab", 245_760),
        (r"command-r", 128_000),
        (r"command", 4_096),
        (r"yi-large|yi-lightning", 32_000),
        (r"yi", 4_000),
    ]
    .into_iter()
    .map(|(pattern, tokens)| {
        (
            Regex::new(pattern).expect("Tauri context-window pattern should compile"),
            tokens,
        )
    })
    .collect()
});

static CONTEXT_WINDOW_IN_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[-_./:])(\d+)k(?:$|[-_./:@])").unwrap());

pub fn model_context_window_info(
    model_id: &str,
    cached_context_windows: &serde_json::Map<String, serde_json::Value>,
    provider_id: Option<&str>,
    user_context_windows: &serde_json::Map<String, serde_json::Value>,
) -> ModelContextWindowInfo {
    if let Some(value) = provider_context_window(user_context_windows, provider_id, model_id)
        .filter(|value| *value > 0)
    {
        return ModelContextWindowInfo {
            value,
            source: ContextWindowSource::User,
        };
    }

    if let Some(value) = provider_context_window(cached_context_windows, provider_id, model_id)
        .filter(|value| *value > 0)
    {
        return ModelContextWindowInfo {
            value,
            source: ContextWindowSource::Api,
        };
    }

    let lower = model_id.to_lowercase();
    for (pattern, value) in MODEL_CONTEXT_WINDOWS.iter() {
        if pattern.is_match(&lower) {
            return ModelContextWindowInfo {
                value: *value,
                source: ContextWindowSource::Pattern,
            };
        }
    }

    if let Some(value) = extract_context_window_from_model_name(&lower) {
        return ModelContextWindowInfo {
            value,
            source: ContextWindowSource::Name,
        };
    }

    ModelContextWindowInfo {
        value: DEFAULT_CONTEXT_WINDOW,
        source: ContextWindowSource::Default,
    }
}

pub fn model_context_window(
    model_id: &str,
    cached_context_windows: &serde_json::Map<String, serde_json::Value>,
    provider_id: Option<&str>,
    user_context_windows: &serde_json::Map<String, serde_json::Value>,
) -> i64 {
    model_context_window_info(
        model_id,
        cached_context_windows,
        provider_id,
        user_context_windows,
    )
    .value
}

pub fn extract_context_window_from_model_name(model_id: &str) -> Option<i64> {
    let mut best = None;
    for captures in CONTEXT_WINDOW_IN_NAME.captures_iter(model_id) {
        let Some(number) = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<i64>().ok())
        else {
            continue;
        };
        let tokens = number.saturating_mul(1024);
        if (1024..=4 * 1024 * 1024).contains(&tokens) && best.is_none_or(|best| tokens > best) {
            best = Some(tokens);
        }
    }
    best
}

fn provider_context_window(
    windows: &serde_json::Map<String, serde_json::Value>,
    provider_id: Option<&str>,
    model_id: &str,
) -> Option<i64> {
    windows
        .get(provider_id?)
        .and_then(serde_json::Value::as_object)
        .and_then(|provider| provider.get(model_id))
        .and_then(serde_json::Value::as_i64)
}
