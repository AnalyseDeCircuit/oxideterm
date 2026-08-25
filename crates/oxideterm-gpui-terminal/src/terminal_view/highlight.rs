use std::{
    borrow::Cow,
    collections::HashMap,
    ops::Range,
    sync::{Arc, Mutex, OnceLock},
};

use gpui::{Hsla, IntoColor, rgba};
use regex::RegexBuilder;

use crate::command_facts::TransientCommandHighlight;
use crate::terminal_ui::{
    MAX_HIGHLIGHT_PATTERN_LENGTH, MAX_HIGHLIGHT_RULES, TerminalHighlightMatchScope,
    TerminalHighlightRenderMode, TerminalHighlightRule,
};
use crate::terminal_view::element::{TerminalRect, to_hsla};
use oxideterm_terminal::{TerminalColor, TerminalRow, TerminalSnapshot};

#[derive(Clone)]
pub(crate) struct TerminalHighlightLayout {
    pub(crate) backgrounds: Vec<TerminalRect>,
    pub(crate) underlines: Vec<TerminalRect>,
    pub(crate) outlines: Vec<TerminalRect>,
    pub(crate) foregrounds: HashMap<(usize, usize), Hsla>,
}

impl TerminalHighlightLayout {
    pub(crate) fn empty() -> Self {
        Self {
            backgrounds: Vec::new(),
            underlines: Vec::new(),
            outlines: Vec::new(),
            foregrounds: HashMap::new(),
        }
    }

    pub(crate) fn foreground_for_cell(&self, row: usize, col: usize) -> Option<Hsla> {
        self.foregrounds.get(&(row, col)).copied()
    }
}

#[derive(Clone)]
pub(super) struct LogicalLine {
    pub(super) text: String,
    pub(super) map: Vec<TextCell>,
    // The line paint range excludes fixed-width terminal padding.
    content_len: usize,
}

#[derive(Clone)]
pub(super) struct TextCell {
    pub(super) row: usize,
    pub(super) col: usize,
    pub(super) cells: usize,
}

#[derive(Clone)]
struct MatchCandidate<'a> {
    rule: &'a RuntimeHighlightRule,
    start: usize,
    len: usize,
}

#[derive(Clone)]
struct RuntimeHighlightRule {
    source: TerminalHighlightRule,
    matcher: RuntimeHighlightMatcher,
}

#[derive(Clone)]
enum RuntimeHighlightMatcher {
    Literal {
        needle: String,
        case_sensitive: bool,
    },
    Regex(regex::Regex),
}

pub(crate) fn terminal_highlights_for_rows(
    snapshot: &TerminalSnapshot,
    rules: &[TerminalHighlightRule],
    transient: Option<(&TransientCommandHighlight, Hsla)>,
    rows: Range<usize>,
) -> TerminalHighlightLayout {
    if rows.is_empty() || (!rules.iter().any(|rule| rule.enabled) && transient.is_none()) {
        return TerminalHighlightLayout::empty();
    }
    let rules = compiled_runtime_rules(rules);
    if rules.is_empty() && transient.is_none() {
        return TerminalHighlightLayout::empty();
    }

    let mut layout = TerminalHighlightLayout::empty();
    let mut seen_lines = std::collections::HashSet::new();

    for row in rows {
        let Some(line_range) = logical_line_range(snapshot, row) else {
            continue;
        };
        if !seen_lines.insert(line_range.clone()) {
            continue;
        }
        // Check the command range before assembling logical-line text so
        // off-screen history and unrelated output add no transient work.
        let transient_for_line = transient.filter(|(highlight, _)| {
            logical_line_overlaps_transient_output(snapshot, line_range.clone(), highlight)
        });
        if rules.is_empty() && transient_for_line.is_none() {
            continue;
        }
        let line = build_logical_line(snapshot, line_range);
        if !rules.is_empty() {
            let matches = accepted_matches(&line.text, line.content_len, &rules);
            apply_matches(&line, matches, &mut layout);
        }
        if let Some((highlight, color)) = transient_for_line {
            for_each_literal_match(
                &line.text,
                &highlight.query,
                highlight.case_sensitive,
                |start, len| {
                    let end = start.saturating_add(len);
                    if start < end
                        && let Some(cells) = line.map.get(start..end)
                    {
                        layout.backgrounds.extend(rects_for_match(cells, color));
                    }
                },
            );
        }
    }

    layout
}

fn logical_line_overlaps_transient_output(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
    highlight: &TransientCommandHighlight,
) -> bool {
    snapshot
        .lines
        .get(rows)
        .unwrap_or(&[])
        .iter()
        .filter_map(|row| usize::try_from(row.absolute_line).ok())
        .any(|absolute_line| {
            absolute_line >= highlight.output_start_global_line
                && highlight
                    .output_end_global_line
                    .is_none_or(|end_line| absolute_line <= end_line)
        })
}

fn compiled_runtime_rules(rules: &[TerminalHighlightRule]) -> Arc<Vec<RuntimeHighlightRule>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Vec<RuntimeHighlightRule>>>>> =
        OnceLock::new();
    let signature = highlight_rules_signature(rules);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(compiled) = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&signature).cloned())
    {
        return compiled;
    }

    let compiled = Arc::new(build_runtime_rules(rules));
    if let Ok(mut cache) = cache.lock() {
        if cache.len() > 16
            && let Some(first_key) = cache.keys().next().cloned()
        {
            cache.remove(&first_key);
        }
        cache.insert(signature, compiled.clone());
    }
    compiled
}

fn highlight_rules_signature(rules: &[TerminalHighlightRule]) -> String {
    rules
        .iter()
        .take(MAX_HIGHLIGHT_RULES)
        .map(|rule| {
            format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{:?}\u{1f}{:?}\u{1f}{}\u{1f}{}",
                rule.id,
                rule.enabled,
                rule.pattern,
                rule.is_regex,
                rule.case_sensitive,
                rule.foreground.as_deref().unwrap_or_default(),
                rule.background.as_deref().unwrap_or_default(),
                rule.render_mode,
                rule.match_scope,
                rule.preserve_background,
                rule.priority,
            )
        })
        .collect::<Vec<_>>()
        .join("\u{1e}")
}

fn build_runtime_rules(rules: &[TerminalHighlightRule]) -> Vec<RuntimeHighlightRule> {
    let mut rules = rules
        .iter()
        .take(MAX_HIGHLIGHT_RULES)
        .filter(|rule| rule.enabled && !rule.pattern.trim().is_empty())
        .filter_map(|rule| {
            let mut rule = rule.clone();
            rule.pattern = rule.pattern.trim().to_string();
            if rule.pattern.chars().count() > MAX_HIGHLIGHT_PATTERN_LENGTH {
                return None;
            }
            runtime_matcher(&rule).map(|matcher| RuntimeHighlightRule {
                source: rule,
                matcher,
            })
        })
        .collect::<Vec<_>>();
    rules.sort_by(|left, right| right.source.priority.cmp(&left.source.priority));
    rules
}

fn runtime_matcher(rule: &TerminalHighlightRule) -> Option<RuntimeHighlightMatcher> {
    if !rule.is_regex {
        return Some(RuntimeHighlightMatcher::Literal {
            needle: if rule.case_sensitive {
                rule.pattern.clone()
            } else {
                rule.pattern.to_lowercase()
            },
            case_sensitive: rule.case_sensitive,
        });
    }
    let Ok(regex) = RegexBuilder::new(&rule.pattern)
        .case_insensitive(!rule.case_sensitive)
        .unicode(true)
        .build()
    else {
        return None;
    };
    (!regex.is_match("")).then_some(RuntimeHighlightMatcher::Regex(regex))
}

pub(super) fn logical_line_range(snapshot: &TerminalSnapshot, row: usize) -> Option<Range<usize>> {
    if row >= snapshot.lines.len() {
        return None;
    }
    let mut start = row;
    while start > 0 && snapshot.lines.get(start).is_some_and(|line| line.wrapped) {
        start -= 1;
    }
    let mut end = row + 1;
    while end < snapshot.lines.len() && snapshot.lines.get(end).is_some_and(|line| line.wrapped) {
        end += 1;
    }
    Some(start..end)
}

pub(super) fn build_logical_line(snapshot: &TerminalSnapshot, rows: Range<usize>) -> LogicalLine {
    let mut text = String::new();
    let mut map = Vec::new();
    for row_index in rows {
        let Some(row) = snapshot.lines.get(row_index) else {
            continue;
        };
        append_row_text(row, row_index, snapshot.cols, &mut text, &mut map);
    }
    let content_len = text.trim_end_matches(' ').chars().count();
    LogicalLine {
        text,
        map,
        content_len,
    }
}

fn append_row_text(
    row: &TerminalRow,
    row_index: usize,
    max_cols: usize,
    text: &mut String,
    map: &mut Vec<TextCell>,
) {
    let mut skip_wide_spacer = false;
    for (col, cell) in row.cells.iter().take(max_cols).enumerate() {
        if skip_wide_spacer {
            skip_wide_spacer = false;
            continue;
        }
        let cells = if cell.wide { 2 } else { 1 };
        text.push(cell.ch);
        map.push(TextCell {
            row: row_index,
            col,
            cells,
        });
        for ch in cell.zerowidth().chars() {
            text.push(ch);
            map.push(TextCell {
                row: row_index,
                col,
                cells,
            });
        }
        // A wide glyph owns the following terminal spacer cell. Keeping that
        // spacer in logical text breaks matches that cross CJK characters.
        skip_wide_spacer = cell.wide;
    }
}

fn accepted_matches<'a>(
    text: &str,
    content_len: usize,
    rules: &'a [RuntimeHighlightRule],
) -> Vec<MatchCandidate<'a>> {
    let mut matches = Vec::new();
    for rule in rules {
        let first_rule_match = matches.len();
        collect_rule_matches(text, rule, &mut matches);
        if rule.source.match_scope == TerminalHighlightMatchScope::LogicalLine
            && matches.len() > first_rule_match
        {
            // One match is enough to promote this rule to the complete logical line.
            matches.truncate(first_rule_match + 1);
            matches[first_rule_match].start = 0;
            matches[first_rule_match].len = content_len;
        }
    }

    matches.sort_by(|left, right| {
        right
            .rule
            .source
            .priority
            .cmp(&left.rule.source.priority)
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| right.len.cmp(&left.len))
    });

    let mut accepted: Vec<MatchCandidate<'a>> = Vec::new();
    for candidate in matches {
        let candidate_end = candidate.start + candidate.len;
        if accepted.iter().any(|existing| {
            candidate.start < existing.start + existing.len && candidate_end > existing.start
        }) {
            continue;
        }
        accepted.push(candidate);
    }
    accepted.sort_by_key(|candidate| candidate.start);
    accepted
}

fn collect_rule_matches<'a>(
    text: &str,
    rule: &'a RuntimeHighlightRule,
    matches: &mut Vec<MatchCandidate<'a>>,
) {
    match &rule.matcher {
        RuntimeHighlightMatcher::Regex(regex) => {
            for matched in regex.find_iter(text) {
                let len = text[matched.start()..matched.end()].chars().count();
                if len == 0 {
                    continue;
                }
                matches.push(MatchCandidate {
                    rule,
                    start: text[..matched.start()].chars().count(),
                    len,
                });
            }
        }
        RuntimeHighlightMatcher::Literal {
            needle,
            case_sensitive,
        } => {
            for_each_literal_match(text, needle, *case_sensitive, |start, len| {
                matches.push(MatchCandidate { rule, start, len });
            });
        }
    }
}

fn for_each_literal_match(
    text: &str,
    needle: &str,
    case_sensitive: bool,
    mut visit: impl FnMut(usize, usize),
) {
    if needle.is_empty() {
        return;
    }
    let haystack = if case_sensitive {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.to_lowercase())
    };
    let needle_len = needle.chars().count();
    let mut search_from = 0;
    while search_from < haystack.len() {
        let Some(byte_index) = haystack[search_from..].find(needle) else {
            break;
        };
        let start_byte = search_from + byte_index;
        visit(haystack[..start_byte].chars().count(), needle_len);
        search_from = start_byte + needle.len().max(1);
    }
}

fn apply_matches(
    line: &LogicalLine,
    matches: Vec<MatchCandidate<'_>>,
    layout: &mut TerminalHighlightLayout,
) {
    for matched in matches {
        let end = matched.start + matched.len;
        if matched.start >= end || end > line.map.len() {
            continue;
        }
        let foreground = matched
            .rule
            .source
            .foreground
            .as_deref()
            .and_then(parse_hex_color);
        let decoration_color = matched
            .rule
            .source
            .background
            .as_deref()
            .and_then(parse_hex_color)
            .or(foreground)
            .unwrap_or_else(|| rgba(0xf59e0bff).into_color());
        let paint_cells = &line.map[matched.start..end];
        for cell in paint_cells {
            if let Some(foreground) = foreground {
                layout.foregrounds.insert((cell.row, cell.col), foreground);
            }
        }
        match matched.rule.source.render_mode {
            TerminalHighlightRenderMode::Background => {
                // Omitting the overlay preserves both theme and ANSI cell backgrounds.
                if !matched.rule.source.preserve_background {
                    layout
                        .backgrounds
                        .extend(rects_for_match(paint_cells, decoration_color));
                }
            }
            TerminalHighlightRenderMode::Underline => layout
                .underlines
                .extend(rects_for_match(paint_cells, decoration_color)),
            TerminalHighlightRenderMode::Outline => layout
                .outlines
                .extend(rects_for_match(paint_cells, decoration_color)),
        }
    }
}

fn rects_for_match(cells: &[TextCell], color: Hsla) -> Vec<TerminalRect> {
    let mut rects: Vec<TerminalRect> = Vec::new();
    for cell in cells {
        if let Some(rect) = rects.last_mut()
            && rect.row == cell.row
            && rect.col + rect.cells == cell.col
            && rect.color == color
        {
            rect.cells += cell.cells;
            continue;
        }
        rects.push(TerminalRect {
            row: cell.row,
            col: cell.col,
            cells: cell.cells,
            color,
        });
    }
    rects
}

fn parse_hex_color(value: &str) -> Option<Hsla> {
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
    let rgb = u32::from_str_radix(&hex[..6], 16).ok()?;
    Some(to_hsla(TerminalColor::rgb(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    )))
}
