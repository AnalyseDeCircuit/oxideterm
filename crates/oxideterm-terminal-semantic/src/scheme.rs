// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::LazyLock;

use regex::Regex;

use crate::{SemanticClass, SemanticLineRole, SemanticScheme, SemanticSpan};

#[derive(Clone, Copy)]
enum RuleContext {
    Any,
    Command,
}

struct Rule {
    matcher: Regex,
    capture: usize,
    class: SemanticClass,
    priority: u8,
    context: RuleContext,
}

impl Rule {
    fn new(
        pattern: &str,
        capture: usize,
        class: SemanticClass,
        priority: u8,
        context: RuleContext,
    ) -> Self {
        Self {
            matcher: Regex::new(pattern).expect("built-in semantic pattern must compile"),
            capture,
            class,
            priority,
            context,
        }
    }

    fn applies_to(&self, role: SemanticLineRole) -> bool {
        matches!(self.context, RuleContext::Any)
            || matches!(
                (self.context, role),
                (RuleContext::Command, SemanticLineRole::Command)
            )
    }
}

pub(crate) struct Candidate {
    pub span: SemanticSpan,
    pub priority: u8,
}

static BUILT_IN_RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        Rule::new(
            r#"(?s)(?:\"(?:\\.|[^\"\r\n])*\"|'(?:\\.|[^'\r\n])*')"#,
            0,
            SemanticClass::String,
            100,
            RuleContext::Any,
        ),
        Rule::new(
            r#"https?://[^\s<>()\[\]{}\"']+"#,
            0,
            SemanticClass::Link,
            95,
            RuleContext::Any,
        ),
        Rule::new(
            r#"(?:^|[\s(])((?:~?/|\./|\.\./)[^\s\"']+)"#,
            1,
            SemanticClass::Path,
            90,
            RuleContext::Any,
        ),
        Rule::new(
            r"(?i)\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b",
            0,
            SemanticClass::Address,
            88,
            RuleContext::Any,
        ),
        Rule::new(
            r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d?\d)\b",
            0,
            SemanticClass::Address,
            87,
            RuleContext::Any,
        ),
        Rule::new(
            r"\b\d{4}[-/]\d{1,2}[-/]\d{1,2}(?:[ T]\d{1,2}(?::\d{2}){1,2}(?:\.\d+)?)?\b",
            0,
            SemanticClass::Timestamp,
            85,
            RuleContext::Any,
        ),
        Rule::new(
            r"\b\d{1,2}(?::\d{2}){1,2}(?:\.\d+)?\b",
            0,
            SemanticClass::Timestamp,
            84,
            RuleContext::Any,
        ),
        Rule::new(
            r"(?:^|\s)(--?[A-Za-z0-9][A-Za-z0-9_-]*(?:=[^\s]+)?)",
            1,
            SemanticClass::Option,
            82,
            RuleContext::Any,
        ),
        Rule::new(
            r"(?i)\b(?:bad|cannot(?:\s+\w+){0,2}|denied|deprecated|disabled|errors?|failed?|failure|false|incorrect|invalid|no(?:\s+\w+)?|none|not(?:\s+\w+){0,2}|(?:do|does|ca|wo|could|should|would)n't(?:\s+\w+){0,2}|refused|unknown|unsupported|wrong)\b",
            0,
            SemanticClass::Error,
            78,
            RuleContext::Any,
        ),
        Rule::new(
            r"(?i)\b(?:can(?:\s+be)?(?:\s+\w+)?|correct(?:ly)?|known|ok|passed?|success(?:ful(?:ly)?)?|supported|true|yes|valid)\b",
            0,
            SemanticClass::Success,
            77,
            RuleContext::Any,
        ),
        Rule::new(
            r"(?i)\b(?:closed|debug|disconnected|exited|skipped|stopped|sudo|terminated|warnings?)\b",
            0,
            SemanticClass::Warning,
            76,
            RuleContext::Any,
        ),
        Rule::new(
            r"(?i)\b(?:access|authentication|connection|disconnection|info|login|operation|password|permission)\b",
            0,
            SemanticClass::Info,
            75,
            RuleContext::Any,
        ),
        Rule::new(
            r"(?i)\b(?:0x[0-9a-f]+|\d+(?:\.\d+)*(?:e[+-]?\d+)?)(?:%|\b)",
            0,
            SemanticClass::Number,
            60,
            RuleContext::Any,
        ),
        // Commands are classified only when shell integration identifies the line.
        Rule::new(
            r"(?:^\s*|[$#>%❯]\s+)([A-Za-z_./][A-Za-z0-9_./-]*)",
            1,
            SemanticClass::Command,
            110,
            RuleContext::Command,
        ),
    ]
});

pub(crate) fn candidates(
    text: &str,
    role: SemanticLineRole,
    semantic_scheme: SemanticScheme,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for rule in BUILT_IN_RULES
        .iter()
        .filter(|rule| rule.applies_to(role) && semantic_scheme.includes(rule.class))
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
