#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HighlightRule {
    pub id: String,
    pub label: String,
    pub pattern: String,
    pub is_regex: bool,
    pub case_sensitive: bool,
    pub foreground: Option<String>,
    pub background: Option<String>,
    #[serde(default)]
    pub render_mode: HighlightRuleRenderMode,
    pub enabled: bool,
    pub priority: i64,
}

impl Default for HighlightRule {
    fn default() -> Self {
        Self {
            id: "highlight-rule-1".to_string(),
            label: String::new(),
            pattern: String::new(),
            is_regex: false,
            case_sensitive: false,
            foreground: Some("#f8fafc".to_string()),
            background: Some("#991b1b".to_string()),
            render_mode: HighlightRuleRenderMode::Background,
            enabled: true,
            priority: 1,
        }
    }
}

#[derive(Clone)]
struct HighlightRuleCandidate {
    rule: HighlightRule,
    sort_priority: i64,
    index: usize,
}

pub fn create_default_highlight_rule(overrides: impl FnOnce(&mut HighlightRule)) -> HighlightRule {
    let mut rule = HighlightRule::default();
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    rule.id = format!("highlight-rule-1-{millis:x}");
    overrides(&mut rule);
    sanitize_highlight_rules(vec![rule])
        .into_iter()
        .next()
        .unwrap_or_default()
}

pub fn sanitize_highlight_rules(input: Vec<HighlightRule>) -> Vec<HighlightRule> {
    let mut seen_ids = std::collections::HashSet::new();
    let candidates = input
        .into_iter()
        .take(MAX_HIGHLIGHT_RULES)
        .enumerate()
        .map(|(index, mut rule)| {
            rule.id = rule.id.trim().to_string();
            if rule.id.is_empty() || seen_ids.contains(&rule.id) {
                rule.id = format!("highlight-rule-{}", index + 1);
            }
            seen_ids.insert(rule.id.clone());
            rule.label = rule.label.trim().to_string();
            rule.pattern = rule.pattern.trim().to_string();
            rule.foreground = sanitize_foreground_color(rule.foreground.as_deref());
            rule.background = sanitize_background_color(rule.background.as_deref());
            rule.priority = rule.priority.clamp(1, MAX_HIGHLIGHT_RULES as i64);
            HighlightRuleCandidate {
                sort_priority: rule.priority,
                rule,
                index,
            }
        })
        .collect::<Vec<_>>();
    normalize_highlight_priorities(candidates)
}

pub fn reindex_highlight_rules(input: Vec<HighlightRule>) -> Vec<HighlightRule> {
    let mut rules = sanitize_highlight_rules(input);
    let total = rules.len() as i64;
    for (index, rule) in rules.iter_mut().enumerate() {
        rule.priority = total - index as i64;
    }
    rules
}

pub fn sanitize_highlight_rules_value(input: &Value) -> Value {
    let Ok(rules) = serde_json::from_value::<Vec<HighlightRule>>(input.clone()) else {
        return json!([]);
    };
    json!(sanitize_highlight_rules(rules))
}

fn normalize_highlight_priorities(candidates: Vec<HighlightRuleCandidate>) -> Vec<HighlightRule> {
    let mut sorted = candidates.clone();
    sorted.sort_by(|left, right| {
        right
            .sort_priority
            .cmp(&left.sort_priority)
            .then_with(|| left.index.cmp(&right.index))
    });

    let highest = sorted.len() as i64;
    let priority_by_id = sorted
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.rule.id.clone(), highest - index as i64))
        .collect::<std::collections::HashMap<_, _>>();

    candidates
        .into_iter()
        .map(|mut candidate| {
            candidate.rule.priority = *priority_by_id.get(&candidate.rule.id).unwrap_or(&1);
            candidate.rule
        })
        .collect()
}

fn sanitize_foreground_color(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty() && is_hex_color(value, false)).then(|| value.to_string())
}

fn sanitize_background_color(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    (is_hex_color(value, true)
        || color_function_like(value, "rgb")
        || color_function_like(value, "rgba")
        || color_function_like(value, "hsl")
        || color_function_like(value, "hsla")
        || value.starts_with("var(--") && value.ends_with(')'))
    .then(|| value.to_string())
}

fn is_hex_color(value: &str, allow_short: bool) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    let valid_len = matches!(hex.len(), 6 | 8) || (allow_short && matches!(hex.len(), 3 | 4));
    valid_len && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn color_function_like(value: &str, name: &str) -> bool {
    value
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('('))
        .is_some_and(|rest| rest.ends_with(')'))
}

