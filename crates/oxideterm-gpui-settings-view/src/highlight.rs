use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb};
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    HighlightRule, HighlightRuleRenderMode, MAX_HIGHLIGHT_PATTERN_LENGTH,
    create_default_highlight_rule,
};

#[derive(Clone)]
pub struct HighlightPreset {
    pub label: String,
    pub rules: Vec<HighlightRule>,
}

#[derive(Clone)]
pub struct HighlightPresetGroup {
    pub label: String,
    pub items: Vec<HighlightPreset>,
}

#[derive(Clone, Copy)]
pub struct HighlightPreviewMatch<'a> {
    pub rule: &'a HighlightRule,
    pub start: usize,
    pub end: usize,
}

pub fn accepted_highlight_preview_matches<'a>(
    line: &str,
    rules: &'a [HighlightRule],
) -> Vec<HighlightPreviewMatch<'a>> {
    let mut candidates = Vec::new();
    for rule in rules.iter().filter(|rule| rule.enabled) {
        if highlight_rule_validation_error(rule).is_some() {
            continue;
        }
        collect_preview_matches(line, rule, &mut candidates);
    }
    candidates.sort_by(|left, right| {
        right
            .rule
            .priority
            .cmp(&left.rule.priority)
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| (right.end - right.start).cmp(&(left.end - left.start)))
    });
    let mut accepted: Vec<HighlightPreviewMatch<'a>> = Vec::new();
    for candidate in candidates {
        if accepted
            .iter()
            .any(|existing| candidate.start < existing.end && candidate.end > existing.start)
        {
            continue;
        }
        accepted.push(candidate);
    }
    accepted.sort_by_key(|matched| matched.start);
    accepted
}

fn collect_preview_matches<'a>(
    line: &str,
    rule: &'a HighlightRule,
    matches: &mut Vec<HighlightPreviewMatch<'a>>,
) {
    if rule.is_regex {
        let Ok(regex) = regex::RegexBuilder::new(&rule.pattern)
            .case_insensitive(!rule.case_sensitive)
            .unicode(true)
            .build()
        else {
            return;
        };
        for matched in regex.find_iter(line) {
            if matched.start() < matched.end() {
                matches.push(HighlightPreviewMatch {
                    rule,
                    start: matched.start(),
                    end: matched.end(),
                });
            }
        }
        return;
    }

    let needle = if rule.case_sensitive {
        rule.pattern.trim().to_string()
    } else {
        rule.pattern.trim().to_lowercase()
    };
    if needle.is_empty() {
        return;
    }
    let haystack = if rule.case_sensitive {
        line.to_string()
    } else {
        line.to_lowercase()
    };
    let mut search_from = 0;
    while search_from < haystack.len() {
        let Some(offset) = haystack[search_from..].find(&needle) else {
            break;
        };
        let start = search_from + offset;
        let end = start + needle.len();
        if line.is_char_boundary(start) && line.is_char_boundary(end) {
            matches.push(HighlightPreviewMatch { rule, start, end });
        }
        search_from = end.max(start + 1);
    }
}

pub fn highlight_preview_segment(text: &str, rule: &HighlightRule) -> AnyElement {
    let fallback = 0xf59e0b;
    let fg = rule
        .foreground
        .as_deref()
        .and_then(parse_hex_u32)
        .unwrap_or(0xf8fafc);
    let bg = rule
        .background
        .as_deref()
        .and_then(parse_hex_u32)
        .unwrap_or(fallback);
    div()
        .px(px(2.0))
        .rounded(px(2.0))
        .text_color(rgb(fg))
        .when(
            rule.render_mode == HighlightRuleRenderMode::Background,
            |item| item.bg(rgb(bg)),
        )
        .when(
            rule.render_mode == HighlightRuleRenderMode::Underline,
            |item| item.border_b_2().border_color(rgb(bg)),
        )
        .when(
            rule.render_mode == HighlightRuleRenderMode::Outline,
            |item| item.border_1().border_color(rgb(bg)),
        )
        .child(text.to_string())
        .into_any_element()
}

pub fn highlight_rule_validation_error(rule: &HighlightRule) -> Option<&'static str> {
    let pattern = rule.pattern.trim();
    if pattern.is_empty() {
        return Some("empty-pattern");
    }
    if pattern.chars().count() > MAX_HIGHLIGHT_PATTERN_LENGTH {
        return Some("pattern-too-long");
    }
    if !rule.is_regex {
        return None;
    }
    let Ok(regex) = regex::RegexBuilder::new(pattern)
        .case_insensitive(!rule.case_sensitive)
        .unicode(true)
        .build()
    else {
        return Some("invalid-regex");
    };
    if regex.is_match("") {
        return Some("empty-match");
    }
    None
}

pub fn parse_hex_u32(value: &str) -> Option<u32> {
    let hex = value.trim().strip_prefix('#')?;
    let expanded;
    let hex = match hex.len() {
        3 => {
            expanded = hex.chars().flat_map(|ch| [ch, ch]).collect::<String>();
            expanded.as_str()
        }
        6 | 8 => hex,
        _ => return None,
    };
    u32::from_str_radix(&hex[..6], 16).ok()
}

pub fn summarize_highlight_pattern(pattern: &str) -> String {
    if pattern.trim().is_empty() {
        return "-".to_string();
    }
    if pattern.chars().count() > 72 {
        format!("{}...", pattern.chars().take(72).collect::<String>())
    } else {
        pattern.to_string()
    }
}

pub fn highlight_render_mode_options() -> &'static [HighlightRuleRenderMode] {
    &[
        HighlightRuleRenderMode::Background,
        HighlightRuleRenderMode::Underline,
        HighlightRuleRenderMode::Outline,
    ]
}

pub fn highlight_render_mode_label(mode: HighlightRuleRenderMode, i18n: &I18n) -> String {
    match mode {
        HighlightRuleRenderMode::Background => {
            i18n.t("settings_view.terminal.highlight_rules.render_mode_background")
        }
        HighlightRuleRenderMode::Underline => {
            i18n.t("settings_view.terminal.highlight_rules.render_mode_underline")
        }
        HighlightRuleRenderMode::Outline => {
            i18n.t("settings_view.terminal.highlight_rules.render_mode_outline")
        }
    }
}

pub fn highlight_preset_groups(i18n: &I18n) -> Vec<HighlightPresetGroup> {
    vec![
        HighlightPresetGroup {
            label: i18n.t("settings_view.terminal.highlight_rules.preset_group_logs"),
            items: vec![
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_status"),
                    rules: vec![
                        highlight_rule(
                            i18n.t("settings_view.terminal.highlight_rules.preset_label_error"),
                            "error",
                            false,
                            "#ffffff",
                            "#b91c1c",
                        ),
                        highlight_rule(
                            i18n.t("settings_view.terminal.highlight_rules.preset_label_warning"),
                            "warning",
                            false,
                            "#111827",
                            "#f59e0b",
                        ),
                        highlight_rule(
                            i18n.t("settings_view.terminal.highlight_rules.preset_label_ok"),
                            "OK",
                            false,
                            "#052e16",
                            "#4ade80",
                        ),
                    ],
                },
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_timestamp"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_timestamp"),
                        r"\b\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}\b",
                        true,
                        "#f8fafc",
                        "#334155",
                    )],
                },
            ],
        },
        HighlightPresetGroup {
            label: i18n.t("settings_view.terminal.highlight_rules.preset_group_network"),
            items: vec![
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_ip"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_ip"),
                        r"\b(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}\b",
                        true,
                        "#eff6ff",
                        "#1d4ed8",
                    )],
                },
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_mac"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_mac"),
                        r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b",
                        true,
                        "#ecfeff",
                        "#0f766e",
                    )],
                },
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_url"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_url"),
                        r"https?:\/\/[^\s)\],;]+[^\s)\],.;:]",
                        true,
                        "#f5f3ff",
                        "#6d28d9",
                    )],
                },
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_port"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_port"),
                        r"\b(?:(?:localhost|(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}|[A-Za-z][A-Za-z0-9-]*|[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+):(?:6553[0-5]|655[0-2]\d|65[0-4]\d{2}|6[0-4]\d{3}|[1-5]?\d{1,4})|port\s+(?:6553[0-5]|655[0-2]\d|65[0-4]\d{2}|6[0-4]\d{3}|[1-5]?\d{1,4}))\b",
                        true,
                        "#fff1f2",
                        "#be185d",
                    )],
                },
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_email"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_email"),
                        r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
                        true,
                        "#ecfeff",
                        "#0f766e",
                    )],
                },
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_domain"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_domain"),
                        r"\b(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[A-Za-z]{2,}\b",
                        true,
                        "#dbeafe",
                        "#1e3a8a",
                    )],
                },
            ],
        },
        HighlightPresetGroup {
            label: i18n.t("settings_view.terminal.highlight_rules.preset_group_system"),
            items: vec![HighlightPreset {
                label: i18n.t("settings_view.terminal.highlight_rules.preset_path"),
                rules: vec![highlight_rule(
                    i18n.t("settings_view.terminal.highlight_rules.preset_label_path"),
                    r#"(?:\b[A-Za-z]:\\(?:[^\\/:*?"<>|\r\n]+\\)*[^\\/:*?"<>|\r\n\s]+|\/(?:[\w-]+|\.[\w-]+)(?:\/[\w.-]+)*)"#,
                    true,
                    "#f7fee7",
                    "#365314",
                )],
            }],
        },
        HighlightPresetGroup {
            label: i18n.t("settings_view.terminal.highlight_rules.preset_group_identity"),
            items: vec![
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_uuid"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_uuid"),
                        r"\b[0-9A-Fa-f]{8}(?:-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}\b",
                        true,
                        "#fff7ed",
                        "#7c2d12",
                    )],
                },
                HighlightPreset {
                    label: i18n.t("settings_view.terminal.highlight_rules.preset_sha256"),
                    rules: vec![highlight_rule(
                        i18n.t("settings_view.terminal.highlight_rules.preset_label_sha256"),
                        r"\b[A-Fa-f0-9]{64}\b",
                        true,
                        "#fef3c7",
                        "#78350f",
                    )],
                },
            ],
        },
    ]
}

fn highlight_rule(
    label: String,
    pattern: &str,
    is_regex: bool,
    foreground: &str,
    background: &str,
) -> HighlightRule {
    create_default_highlight_rule(|rule| {
        rule.label = label;
        rule.pattern = pattern.to_string();
        rule.is_regex = is_regex;
        rule.foreground = Some(foreground.to_string());
        rule.background = Some(background.to_string());
    })
}
