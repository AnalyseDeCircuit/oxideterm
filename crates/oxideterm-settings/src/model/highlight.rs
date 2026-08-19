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
    #[serde(default)]
    pub match_scope: HighlightRuleMatchScope,
    #[serde(default)]
    pub preserve_background: bool,
    pub enabled: bool,
    pub priority: i64,
}

pub const MAX_HIGHLIGHT_RULE_SETS: usize = 32;
pub const GLOBAL_HIGHLIGHT_RULE_SET_ID: &str = "global";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HighlightRuleSet {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub rules: Vec<HighlightRule>,
}

pub fn create_highlight_rule_set(
    name: impl Into<String>,
    rules: Vec<HighlightRule>,
) -> HighlightRuleSet {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    HighlightRuleSet {
        id: format!("highlight-set-{nanos:x}"),
        name: name.into(),
        rules: sanitize_highlight_rules(rules),
    }
}

pub fn sanitize_highlight_rule_sets(input: Vec<HighlightRuleSet>) -> Vec<HighlightRuleSet> {
    let mut seen_ids = std::collections::HashSet::new();
    input
        .into_iter()
        .take(MAX_HIGHLIGHT_RULE_SETS)
        .enumerate()
        .map(|(index, mut rule_set)| {
            rule_set.id = rule_set.id.trim().to_string();
            if rule_set.id.is_empty()
                || rule_set.id == GLOBAL_HIGHLIGHT_RULE_SET_ID
                || seen_ids.contains(&rule_set.id)
            {
                rule_set.id = format!("highlight-set-{}", index + 1);
            }
            seen_ids.insert(rule_set.id.clone());
            rule_set.name = rule_set.name.trim().to_string();
            if rule_set.name.is_empty() {
                rule_set.name = format!("Rule Set {}", index + 1);
            }
            rule_set.rules = sanitize_highlight_rules(rule_set.rules);
            rule_set
        })
        .collect()
}

pub fn sanitize_highlight_rule_sets_value(input: &Value) -> Value {
    let Ok(rule_sets) = serde_json::from_value::<Vec<HighlightRuleSet>>(input.clone()) else {
        return json!([]);
    };
    json!(sanitize_highlight_rule_sets(rule_sets))
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
            match_scope: HighlightRuleMatchScope::Match,
            // New rules should preserve theme, image, and ANSI backgrounds by default.
            preserve_background: true,
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

#[cfg(test)]
mod highlight_rule_tests {
    use super::*;

    #[test]
    fn legacy_rule_keeps_original_background_behavior() {
        let legacy = serde_json::json!({
            "id": "legacy",
            "label": "Legacy",
            "pattern": "ERROR",
            "is_regex": false,
            "case_sensitive": false,
            "foreground": "#ffffff",
            "background": "#991b1b",
            "render_mode": "background",
            "enabled": true,
            "priority": 1
        });

        let rule: HighlightRule = serde_json::from_value(legacy).expect("legacy highlight rule");

        assert_eq!(rule.match_scope, HighlightRuleMatchScope::Match);
        assert!(!rule.preserve_background);
    }

    #[test]
    fn rule_sets_normalize_names_ids_and_rule_priorities() {
        let rule_sets = sanitize_highlight_rule_sets(vec![
            HighlightRuleSet {
                id: " duplicate ".to_string(),
                name: " Operations ".to_string(),
                rules: vec![HighlightRule::default()],
            },
            HighlightRuleSet {
                id: "duplicate".to_string(),
                name: String::new(),
                rules: vec![HighlightRule::default()],
            },
        ]);

        assert_eq!(rule_sets[0].id, "duplicate");
        assert_eq!(rule_sets[0].name, "Operations");
        assert_eq!(rule_sets[1].id, "highlight-set-2");
        assert_eq!(rule_sets[1].name, "Rule Set 2");
        assert_eq!(rule_sets[0].rules[0].priority, 1);
    }
}
