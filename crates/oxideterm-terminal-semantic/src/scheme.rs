// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::LazyLock,
};

use regex::Regex;

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

#[derive(Clone)]
struct Rule {
    id: String,
    pattern: String,
    matcher: Regex,
    capture: usize,
    class: SemanticClass,
    priority: u8,
    context: SemanticRuleContext,
}

impl Rule {
    fn new(
        id: impl Into<String>,
        pattern: &str,
        capture: usize,
        class: SemanticClass,
        priority: u8,
        context: SemanticRuleContext,
    ) -> Self {
        Self {
            id: id.into(),
            pattern: pattern.to_string(),
            matcher: Regex::new(pattern).expect("built-in semantic pattern must compile"),
            capture,
            class,
            priority,
            context,
        }
    }

    fn applies_to(&self, role: SemanticLineRole) -> bool {
        match self.context {
            SemanticRuleContext::Any => true,
            SemanticRuleContext::Command => role == SemanticLineRole::Command,
            SemanticRuleContext::Output => role.is_output(),
        }
    }
}

#[derive(Clone)]
pub struct CompiledSemanticScheme {
    id: String,
    name: String,
    signature: u64,
    rules: Vec<Rule>,
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
        self.rules.iter().any(|rule| rule.class == class)
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

static BUILT_IN_RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        Rule::new(
            "quoted-string",
            r#"(?s)(?:\"(?:\\.|[^\"\r\n])*\"|'(?:\\.|[^'\r\n])*')"#,
            0,
            SemanticClass::String,
            100,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "url",
            r#"https?://[^\s<>()\[\]{}\"']+"#,
            0,
            SemanticClass::Link,
            95,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "path",
            r#"(?:^|[\s(])((?:~?/|\./|\.\./)[^\s\"']+)"#,
            1,
            SemanticClass::Path,
            90,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "windows-path",
            WINDOWS_PATH_PATTERN,
            1,
            SemanticClass::Path,
            90,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "ipv6-address",
            IPV6_ADDRESS_PATTERN,
            1,
            SemanticClass::Address,
            89,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "mac-address",
            r"(?i)\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b",
            0,
            SemanticClass::Address,
            88,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "ipv4-address",
            r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d?\d)\b",
            0,
            SemanticClass::Address,
            87,
            SemanticRuleContext::Any,
        ),
        // macOS ps switches STIME between a clock, weekday-clock, and
        // day-month-year form as a process gets older.
        Rule::new(
            "twelve-hour-time",
            r"(?i)\b(?:(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun)\s?(?:0?[1-9]|1[0-2])|(?:0?[1-9]|1[0-2]):[0-5]\d(?::[0-5]\d)?(?:\.\d+)?)\s?(?:AM|PM)\b",
            0,
            SemanticClass::Timestamp,
            86,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "ps-start-date",
            r"(?i)\b(?:0?[1-9]|[12]\d|3[01])(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)(?:\d{2}|\d{4})\b",
            0,
            SemanticClass::Timestamp,
            86,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "localized-month-day",
            r"(?:^|[^\p{L}\p{N}])((?:0?[1-9]|1[0-2])(?:月|월)(?:[12]\d|3[01]|0?[1-9])(?:日|일)?)",
            1,
            SemanticClass::Timestamp,
            86,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "date-time",
            r"\b\d{4}[-/]\d{1,2}[-/]\d{1,2}(?:[ T]\d{1,2}(?::\d{2}){1,2}(?:\.\d+)?)?\b",
            0,
            SemanticClass::Timestamp,
            85,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "elapsed-time",
            r"\b\d{3,}:[0-5]\d(?::[0-5]\d)?(?:\.\d+)?\b",
            0,
            SemanticClass::Timestamp,
            85,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "time",
            r"\b\d{1,2}(?::\d{2}){1,2}(?:\.\d+)?\b",
            0,
            SemanticClass::Timestamp,
            84,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "option",
            r"(?:^|\s)(--?[A-Za-z][A-Za-z0-9_-]*)",
            1,
            SemanticClass::Option,
            82,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "option-assignment-operator",
            OPTION_ASSIGNMENT_PATTERN,
            2,
            SemanticClass::Operator,
            81,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "option-assignment-value",
            OPTION_ASSIGNMENT_PATTERN,
            3,
            SemanticClass::String,
            80,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "variable-assignment-name",
            VARIABLE_ASSIGNMENT_PATTERN,
            1,
            SemanticClass::Variable,
            82,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "variable-assignment-operator",
            VARIABLE_ASSIGNMENT_PATTERN,
            2,
            SemanticClass::Operator,
            81,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "variable-assignment-value",
            VARIABLE_ASSIGNMENT_PATTERN,
            3,
            SemanticClass::String,
            80,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "error-status",
            r"(?i)\b(?:bad|cannot(?:\s+\w+){0,2}|denied|deprecated|disabled|errors?|failed?|failure|false|incorrect|invalid|no\s+(?:access|permission)|none|not\s+(?:available|configured|connected|enabled|found|installed|running|supported|valid)|(?:ca|could)n't|refused|unknown|unsupported|wrong)\b",
            0,
            SemanticClass::Error,
            78,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "success-status",
            r"(?i)\b(?:can\s+be\s+(?:applied|enabled|installed|updated|upgraded)|correct(?:ly)?|known|ok|passed?|success(?:ful(?:ly)?)?|supported|true|yes|valid)\b",
            0,
            SemanticClass::Success,
            77,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "warning-status",
            r"(?i)\b(?:closed|debug|disconnected|exited|skipped|stopped|sudo|terminated|warnings?)\b",
            0,
            SemanticClass::Warning,
            76,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "info-status",
            r"(?i)\b(?:access|authentication|connection|disconnection|info|login|operation|password|permission)\b",
            0,
            SemanticClass::Info,
            75,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "multilingual-error-status",
            r"(?i)(?:错误|錯誤|失败|失敗|拒绝|拒絕|无效|無效|不支持|不支援|未知|无法|無法|未找到|不存在|エラー|失敗|拒否|無効|未対応|不明|見つかりません|오류|실패|거부|잘못됨|지원되지 않음|알 수 없음|\b(?:error|fehler|échec|erreur|fallo|fracaso|errore|falha|erro|lỗi|thất bại|invalide?|ungültig|inválido|non supportato|não suportado|không được hỗ trợ)\b)",
            0,
            SemanticClass::Error,
            79,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "multilingual-success-status",
            r"(?i)(?:成功|已完成|完成|有效|有効|成功しました|완료|성공|유효|\b(?:succès|réussi|erfolgreich|erfolg|éxito|correcto|riuscito|sucesso|concluído|thành công|hoàn tất|valide?|válido|gültig)\b)",
            0,
            SemanticClass::Success,
            78,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "multilingual-warning-status",
            r"(?i)(?:警告|注意|警告あり|경고|주의|\b(?:avertissement|warnung|advertencia|avviso|aviso|cảnh báo|atenção|achtung)\b)",
            0,
            SemanticClass::Warning,
            77,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "multilingual-info-status",
            r"(?i)(?:信息|資訊|提示|情報|案内|정보|안내|\b(?:information|informations|información|informazione|informação|hinweis|thông tin)\b)",
            0,
            SemanticClass::Info,
            76,
            SemanticRuleContext::Any,
        ),
        Rule::new(
            "number",
            r"(?i)\b(?:0x[0-9a-f]+|\d+(?:\.\d+)*(?:e[+-]?\d+)?)(?:%|\b)",
            0,
            SemanticClass::Number,
            60,
            SemanticRuleContext::Any,
        ),
        // Commands are classified only when shell integration identifies the line.
        Rule::new(
            "command",
            r"(?:^\s*|[$#>%❯]\s+)([A-Za-z_./][A-Za-z0-9_./-]*)",
            1,
            SemanticClass::Command,
            110,
            SemanticRuleContext::Command,
        ),
    ]
});

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
                id: rule.id.clone(),
                enabled: true,
                pattern: rule.pattern.clone(),
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
    let rules = document
        .rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| {
            Rule::new(
                rule.id.clone(),
                &rule.pattern,
                rule.capture,
                rule.class,
                rule.priority,
                rule.context,
            )
        })
        .collect();
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(document)
        .map_err(|error| format!("Failed to sign semantic scheme: {error}"))?
        .hash(&mut hasher);
    Ok(CompiledSemanticScheme {
        id: document.id.clone(),
        name: document.name.trim().to_string(),
        signature: hasher.finish(),
        rules,
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
    for rule in semantic_scheme
        .rules
        .iter()
        .filter(|rule| rule.applies_to(role))
    {
        for captures in rule.matcher.captures_iter(text) {
            let Some(matched) = captures.get(rule.capture) else {
                continue;
            };
            if matched.is_empty() {
                continue;
            }
            candidates.push(Candidate {
                span: SemanticSpan::new(matched.start()..matched.end(), rule.class),
                priority: rule.priority,
            });
        }
    }
    candidates
}
