// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::LazyLock,
};

use regex::{Regex, RegexSet};

use crate::{
    SEMANTIC_SCHEME_FORMAT_VERSION, SemanticClass, SemanticLineRole, SemanticRuleContext,
    SemanticRuleDefinition, SemanticScheme, SemanticSchemeDocument, SemanticSpan,
    validate_scheme_document,
};

// Match the standard hexadecimal IPv6 forms, including compressed addresses,
// while leaving IPv4-mapped forms for a later dedicated rule.
const IPV6_ADDRESS_PATTERN: &str = r"(?ix)(?:^|[^0-9a-f:])(
    (?:(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}
      |(?:[0-9a-f]{1,4}:){1,6}:[0-9a-f]{1,4}
      |(?:[0-9a-f]{1,4}:){1,5}(?::[0-9a-f]{1,4}){1,2}
      |(?:[0-9a-f]{1,4}:){1,4}(?::[0-9a-f]{1,4}){1,3}
      |(?:[0-9a-f]{1,4}:){1,3}(?::[0-9a-f]{1,4}){1,4}
      |(?:[0-9a-f]{1,4}:){1,2}(?::[0-9a-f]{1,4}){1,5}
      |[0-9a-f]{1,4}:(?:(?::[0-9a-f]{1,4}){1,6})
      |:(?:(?::[0-9a-f]{1,4}){1,7}))\b
    |(?:[0-9a-f]{1,4}:){1,7}:
)";

const WINDOWS_PATH_PATTERN: &str = r#"(?ix)(?:^|[\s(])((?:
    [a-z]:[\\/][^\s<>"|?*]+
    |\\\\[^\\/\s:*?"<>|]+[\\/][^\\/\s:*?"<>|]+(?:[\\/][^\s<>"|?*]+)*
))"#;

const OPTION_ASSIGNMENT_PATTERN: &str = r"(?:^|\s)(--?[A-Za-z][A-Za-z0-9_-]*)(=)([^\s]+)";
const VARIABLE_ASSIGNMENT_PATTERN: &str = r"(?:^|[\s,])([A-Za-z_][A-Za-z0-9_-]*)(=)([^,\s]+)";
// UUIDs and complete hex digests share a scan and the same identifier styling.
const IDENTIFIER_PATTERN: &str = r"(?x)\b[0-9a-fA-F]{8}(?:
    -[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}
    |[0-9a-fA-F]{24}(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{24}|[0-9a-fA-F]{32}|[0-9a-fA-F]{64}|[0-9a-fA-F]{96})?
)\b";
const ENGLISH_WEEKDAY_PATTERN: &str = r"(?i)\b(?:Mon(?:day)?|Tue(?:sday)?|Wed(?:nesday)?|Thu(?:rsday)?|Fri(?:day)?|Sat(?:urday)?|Sun(?:day)?)\b";
const ENGLISH_MONTH_PATTERN: &str = r"(?i)\b(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\b";

struct BuiltInRule {
    id: &'static str,
    pattern: &'static str,
    capture: usize,
    class: SemanticClass,
    priority: u8,
    context: SemanticRuleContext,
}

impl BuiltInRule {
    const fn new(
        id: &'static str,
        pattern: &'static str,
        capture: usize,
        class: SemanticClass,
        priority: u8,
        context: SemanticRuleContext,
    ) -> Self {
        Self {
            id,
            pattern,
            capture,
            class,
            priority,
            context,
        }
    }
}

#[derive(Clone, Copy)]
struct RuleTarget {
    capture: usize,
    class: SemanticClass,
    priority: u8,
}

#[derive(Clone)]
struct PatternGroup {
    matcher: Regex,
    targets: Vec<RuleTarget>,
}

impl PatternGroup {
    fn emit_candidates(&self, text: &str, candidates: &mut Vec<Candidate>) {
        if let [target] = self.targets.as_slice()
            && target.capture == 0
        {
            // Whole-token rules need only match ranges, not capture-group storage.
            candidates.extend(
                self.matcher
                    .find_iter(text)
                    .filter(|matched| !matched.is_empty())
                    .map(|matched| Candidate {
                        span: SemanticSpan::new(matched.range(), target.class),
                        priority: target.priority,
                    }),
            );
            return;
        }
        for captures in self.matcher.captures_iter(text) {
            for target in &self.targets {
                let Some(matched) = captures.get(target.capture) else {
                    continue;
                };
                if matched.is_empty() {
                    continue;
                }
                candidates.push(Candidate {
                    span: SemanticSpan::new(matched.start()..matched.end(), target.class),
                    priority: target.priority,
                });
            }
        }
    }
}

#[derive(Clone)]
struct ContextMatcher {
    prefilter: Option<RegexSet>,
    groups: Vec<PatternGroup>,
}

impl ContextMatcher {
    fn build(rules: &[&SemanticRuleDefinition], compiled_patterns: &HashMap<&str, Regex>) -> Self {
        let mut groups: Vec<PatternGroup> = Vec::new();
        let mut group_by_pattern: HashMap<&str, usize> = HashMap::new();
        let mut patterns: Vec<&str> = Vec::new();

        for rule in rules {
            let target = RuleTarget {
                capture: rule.capture,
                class: rule.class,
                priority: rule.priority,
            };
            if let Some(&index) = group_by_pattern.get(rule.pattern.as_str()) {
                groups[index].targets.push(target);
            } else {
                let index = groups.len();
                group_by_pattern.insert(rule.pattern.as_str(), index);
                patterns.push(rule.pattern.as_str());
                groups.push(PatternGroup {
                    matcher: compiled_patterns[rule.pattern.as_str()].clone(),
                    targets: vec![target],
                });
            }
        }

        // Fall back to linear group scan only if a large custom scheme exceeds RegexSet size limits.
        let prefilter = if patterns.is_empty() {
            None
        } else {
            RegexSet::new(&patterns).ok()
        };

        Self { prefilter, groups }
    }

    fn emit_candidates(&self, text: &str, candidates: &mut Vec<Candidate>) {
        if self.groups.is_empty() || text.is_empty() {
            return;
        }
        if let Some(prefilter) = &self.prefilter {
            for index in prefilter.matches(text).iter() {
                self.groups[index].emit_candidates(text, candidates);
            }
            return;
        }
        for group in &self.groups {
            group.emit_candidates(text, candidates);
        }
    }
}

#[derive(Clone)]
pub struct CompiledSemanticScheme {
    id: String,
    name: String,
    signature: u64,
    command_matcher: ContextMatcher,
    output_matcher: ContextMatcher,
    unknown_matcher: ContextMatcher,
    rule_classes_mask: u32,
    colors: BTreeMap<SemanticClass, String>,
}

impl CompiledSemanticScheme {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn signature(&self) -> u64 {
        self.signature
    }

    pub fn color(&self, class: SemanticClass) -> Option<&str> {
        self.colors.get(&class).map(String::as_str)
    }

    pub(crate) fn contains_rule_class(&self, class: SemanticClass) -> bool {
        (self.rule_classes_mask & (1u32 << (class as u8))) != 0
    }
}

pub(crate) struct Candidate {
    pub span: SemanticSpan,
    pub priority: u8,
}

impl Candidate {
    pub(crate) fn new(range: std::ops::Range<usize>, class: SemanticClass, priority: u8) -> Self {
        Self {
            span: SemanticSpan::new(range, class),
            priority,
        }
    }

    pub(crate) fn new_with_style_variant(
        range: std::ops::Range<usize>,
        class: SemanticClass,
        priority: u8,
        style_variant: u8,
    ) -> Self {
        let mut candidate = Self::new(range, class, priority);
        candidate.span.style_variant = Some(style_variant);
        candidate
    }
}

const BUILT_IN_RULES: &[BuiltInRule] = &[
    BuiltInRule::new(
        "quoted-string",
        r#"(?s)(?:\"(?:\\.|[^\"\r\n])*\"|'(?:\\.|[^'\r\n])*')"#,
        0,
        SemanticClass::String,
        100,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "url",
        r#"https?://[^\s<>()\[\]{}\"']+"#,
        0,
        SemanticClass::Link,
        95,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "path",
        r#"(?:^|[\s(])((?:~?/|\./|\.\./)[^\s\"']+)"#,
        1,
        SemanticClass::Path,
        90,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "windows-path",
        WINDOWS_PATH_PATTERN,
        1,
        SemanticClass::Path,
        90,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "identifier",
        IDENTIFIER_PATTERN,
        0,
        SemanticClass::Variable,
        89,
        SemanticRuleContext::Any,
    ),
    // Capacity values with a unit (190G / 7.9G / 4.0M / 256K). Without this
    // rule the generic one only matches bare numbers, so "number + unit" output
    // from df -h or ls -lh ends up half tinted and half default. Longer
    // suffixes come first so KiB is not matched by K.
    BuiltInRule::new(
        "quantity",
        r"\b\d+(?:[.,]\d+)?(?:KiB|MiB|GiB|TiB|PiB|EiB|KB|MB|GB|TB|PB|EB|Ki|Mi|Gi|Ti|Pi|Ei|[kKMGTPE]|B)\b",
        0,
        SemanticClass::Number,
        86,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "ipv6-address",
        IPV6_ADDRESS_PATTERN,
        1,
        SemanticClass::Address,
        89,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "mac-address",
        r"(?i)\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b",
        0,
        SemanticClass::Address,
        88,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "ipv4-address",
        r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d?\d)\b",
        0,
        SemanticClass::Address,
        87,
        SemanticRuleContext::Any,
    ),
    // macOS ps switches STIME between a clock, weekday-clock, and
    // day-month-year form as a process gets older.
    BuiltInRule::new(
        "twelve-hour-time",
        r"(?i)\b(?:(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun)\s?(?:0?[1-9]|1[0-2])|(?:0?[1-9]|1[0-2]):[0-5]\d(?::[0-5]\d)?(?:\.\d+)?)\s?(?:AM|PM)\b",
        0,
        SemanticClass::Timestamp,
        86,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "ps-start-date",
        r"(?i)\b(?:0?[1-9]|[12]\d|3[01])(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)(?:\d{2}|\d{4})\b",
        0,
        SemanticClass::Timestamp,
        86,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "weekday-name",
        ENGLISH_WEEKDAY_PATTERN,
        0,
        SemanticClass::Weekday,
        86,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "month-name",
        ENGLISH_MONTH_PATTERN,
        0,
        SemanticClass::Month,
        86,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "localized-month-day",
        r"(?:^|[^\p{L}\p{N}])((?:0?[1-9]|1[0-2])(?:月|월)(?:[12]\d|3[01]|0?[1-9])(?:日|일)?)",
        1,
        SemanticClass::Timestamp,
        86,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "date-time",
        r"\b\d{4}[-/]\d{1,2}[-/]\d{1,2}(?:[ T]\d{1,2}(?::\d{2}){1,2}(?:\.\d+)?)?\b",
        0,
        SemanticClass::Timestamp,
        85,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "elapsed-time",
        r"\b\d{3,}:[0-5]\d(?::[0-5]\d)?(?:\.\d+)?\b",
        0,
        SemanticClass::Timestamp,
        85,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "time",
        r"\b\d{1,2}(?::\d{2}){1,2}(?:\.\d+)?\b",
        0,
        SemanticClass::Timestamp,
        84,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "option",
        r"(?:^|\s)(--?[A-Za-z][A-Za-z0-9_-]*)",
        1,
        SemanticClass::Option,
        82,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "option-assignment-operator",
        OPTION_ASSIGNMENT_PATTERN,
        2,
        SemanticClass::Operator,
        81,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "option-assignment-value",
        OPTION_ASSIGNMENT_PATTERN,
        3,
        SemanticClass::String,
        80,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "variable-assignment-name",
        VARIABLE_ASSIGNMENT_PATTERN,
        1,
        SemanticClass::Variable,
        82,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "variable-assignment-operator",
        VARIABLE_ASSIGNMENT_PATTERN,
        2,
        SemanticClass::Operator,
        81,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "variable-assignment-value",
        VARIABLE_ASSIGNMENT_PATTERN,
        3,
        SemanticClass::String,
        80,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "error-status",
        r"(?i)\b(?:bad|cannot(?:\s+\w+){0,2}|critical|denied|deprecated|disabled|errors?|failed?|failure|fatal|false|incorrect|invalid|no\s+(?:access|permission)|none|not\s+(?:available|configured|connected|enabled|found|installed|running|supported|valid)|panic|(?:ca|could)n't|refused|unknown|unsupported|wrong)\b",
        0,
        SemanticClass::Error,
        78,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "success-status",
        r"(?i)\b(?:can\s+be\s+(?:applied|enabled|installed|updated|upgraded)|correct(?:ly)?|known|ok|passed?|success(?:ful(?:ly)?)?|supported|true|yes|valid)\b",
        0,
        SemanticClass::Success,
        77,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "warning-status",
        r"(?i)\b(?:closed|debug|disconnected|exited|skipped|stopped|sudo|terminated|warn|warnings?)\b",
        0,
        SemanticClass::Warning,
        76,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "info-status",
        r"(?i)\b(?:access|authentication|connection|disconnection|info|login|operation|password|permission)\b",
        0,
        SemanticClass::Info,
        75,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "multilingual-error-status",
        r"(?i)(?:错误|錯誤|失败|失敗|拒绝|拒絕|无效|無效|不支持|不支援|未知|无法|無法|未找到|不存在|エラー|失敗|拒否|無効|未対応|不明|見つかりません|오류|실패|거부|잘못됨|지원되지 않음|알 수 없음|\b(?:error|fehler|échec|erreur|fallo|fracaso|errore|falha|erro|lỗi|thất bại|invalide?|ungültig|inválido|non supportato|não suportado|không được hỗ trợ)\b)",
        0,
        SemanticClass::Error,
        79,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "multilingual-success-status",
        r"(?i)(?:成功|已完成|完成|有效|有効|成功しました|완료|성공|유효|\b(?:succès|réussi|erfolgreich|erfolg|éxito|correcto|riuscito|sucesso|concluído|thành công|hoàn tất|valide?|válido|gültig)\b)",
        0,
        SemanticClass::Success,
        78,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "multilingual-warning-status",
        r"(?i)(?:警告|注意|警告あり|경고|주의|\b(?:avertissement|warnung|advertencia|avviso|aviso|cảnh báo|atenção|achtung)\b)",
        0,
        SemanticClass::Warning,
        77,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "multilingual-info-status",
        r"(?i)(?:信息|資訊|提示|情報|案内|정보|안내|\b(?:information|informations|información|informazione|informação|hinweis|thông tin)\b)",
        0,
        SemanticClass::Info,
        76,
        SemanticRuleContext::Any,
    ),
    BuiltInRule::new(
        "number",
        r"(?i)\b(?:0x[0-9a-f]+|\d+(?:\.\d+)*(?:e[+-]?\d+)?)(?:%|\b)",
        0,
        SemanticClass::Number,
        60,
        SemanticRuleContext::Any,
    ),
    // Commands are classified only when shell integration identifies the line.
    BuiltInRule::new(
        "command",
        r"(?:^\s*|[$#>%❯]\s+)([A-Za-z_./][A-Za-z0-9_./-]*)",
        1,
        SemanticClass::Command,
        110,
        SemanticRuleContext::Command,
    ),
];

static BALANCED_SCHEME: LazyLock<CompiledSemanticScheme> = LazyLock::new(|| {
    compile_scheme_document(&built_in_scheme_document(SemanticScheme::Balanced))
        .expect("balanced semantic scheme must compile")
});

static CONSERVATIVE_SCHEME: LazyLock<CompiledSemanticScheme> = LazyLock::new(|| {
    compile_scheme_document(&built_in_scheme_document(SemanticScheme::Conservative))
        .expect("conservative semantic scheme must compile")
});

pub fn built_in_scheme_document(scheme: SemanticScheme) -> SemanticSchemeDocument {
    let (id, name) = match scheme {
        SemanticScheme::Balanced => ("balanced", "Balanced"),
        SemanticScheme::Conservative => ("conservative", "Conservative"),
    };
    SemanticSchemeDocument {
        version: SEMANTIC_SCHEME_FORMAT_VERSION,
        id: id.to_string(),
        name: name.to_string(),
        rules: BUILT_IN_RULES
            .iter()
            .filter(|rule| scheme.includes(rule.class))
            .map(|rule| SemanticRuleDefinition {
                id: rule.id.to_string(),
                enabled: true,
                pattern: rule.pattern.to_string(),
                capture: rule.capture,
                class: rule.class,
                priority: rule.priority,
                context: rule.context,
            })
            .collect(),
        colors: BTreeMap::new(),
    }
}

pub fn compile_scheme_document(
    document: &SemanticSchemeDocument,
) -> Result<CompiledSemanticScheme, String> {
    validate_scheme_document(document)?;
    let enabled_rules: Vec<&SemanticRuleDefinition> =
        document.rules.iter().filter(|rule| rule.enabled).collect();
    let mut compiled_patterns: HashMap<&str, Regex> = HashMap::new();
    let mut rule_classes_mask = 0u32;
    for rule in &enabled_rules {
        rule_classes_mask |= 1u32 << (rule.class as u8);
        if !compiled_patterns.contains_key(rule.pattern.as_str()) {
            let compiled = Regex::new(&rule.pattern)
                .map_err(|error| format!("Invalid regex for rule '{}': {error}", rule.id))?;
            compiled_patterns.insert(rule.pattern.as_str(), compiled);
        }
    }

    let command_rules: Vec<&SemanticRuleDefinition> = enabled_rules
        .iter()
        .copied()
        .filter(|rule| {
            matches!(
                rule.context,
                SemanticRuleContext::Any | SemanticRuleContext::Command
            )
        })
        .collect();
    let output_rules: Vec<&SemanticRuleDefinition> = enabled_rules
        .iter()
        .copied()
        .filter(|rule| {
            matches!(
                rule.context,
                SemanticRuleContext::Any | SemanticRuleContext::Output
            )
        })
        .collect();
    let unknown_rules: Vec<&SemanticRuleDefinition> = enabled_rules
        .iter()
        .copied()
        .filter(|rule| rule.context == SemanticRuleContext::Any)
        .collect();

    let command_matcher = ContextMatcher::build(&command_rules, &compiled_patterns);
    let output_matcher = ContextMatcher::build(&output_rules, &compiled_patterns);
    let unknown_matcher = ContextMatcher::build(&unknown_rules, &compiled_patterns);

    let mut hasher = DefaultHasher::new();
    serde_json::to_string(document)
        .map_err(|error| format!("Failed to sign semantic scheme: {error}"))?
        .hash(&mut hasher);
    Ok(CompiledSemanticScheme {
        id: document.id.clone(),
        name: document.name.trim().to_string(),
        signature: hasher.finish(),
        command_matcher,
        output_matcher,
        unknown_matcher,
        rule_classes_mask,
        colors: document.colors.clone(),
    })
}

pub fn compiled_builtin_scheme(scheme: SemanticScheme) -> &'static CompiledSemanticScheme {
    match scheme {
        SemanticScheme::Balanced => &BALANCED_SCHEME,
        SemanticScheme::Conservative => &CONSERVATIVE_SCHEME,
    }
}

pub(crate) fn candidates(
    text: &str,
    role: SemanticLineRole,
    semantic_scheme: SemanticScheme,
) -> Vec<Candidate> {
    candidates_for_compiled(text, role, compiled_builtin_scheme(semantic_scheme))
}

pub(crate) fn candidates_for_compiled(
    text: &str,
    role: SemanticLineRole,
    semantic_scheme: &CompiledSemanticScheme,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let matcher = if role == SemanticLineRole::Command {
        &semantic_scheme.command_matcher
    } else if role.is_output() {
        &semantic_scheme.output_matcher
    } else {
        &semantic_scheme.unknown_matcher
    };
    matcher.emit_candidates(text, &mut candidates);
    candidates
}
