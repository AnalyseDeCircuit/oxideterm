use std::{
    env,
    ops::Range,
    path::{Path, PathBuf},
};

use oxideterm_terminal::{TerminalCell, TerminalColor, TerminalSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TerminalLinkRange {
    pub(crate) row: usize,
    pub(crate) start_col: usize,
    pub(crate) end_col: usize,
    pub(crate) target: String,
    pub(crate) kind: TerminalLinkKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalLinkKind {
    Url,
    Path,
}

pub(crate) fn link_ranges_contain(ranges: &[TerminalLinkRange], row: usize, col: usize) -> bool {
    ranges
        .iter()
        .any(|range| range.row == row && col >= range.start_col && col < range.end_col)
}

pub(crate) fn is_link_stylable_cell(cell: &TerminalCell) -> bool {
    cell.bg == TerminalColor::rgb(0x0d, 0x0f, 0x12)
}

pub(crate) fn detect_link_ranges(snapshot: &TerminalSnapshot) -> Vec<TerminalLinkRange> {
    detect_link_ranges_for_rows(snapshot, 0..snapshot.lines.len())
}

pub(crate) fn display_link_ranges(snapshot: &TerminalSnapshot) -> Vec<TerminalLinkRange> {
    filter_display_link_ranges(snapshot, detect_link_ranges(snapshot))
}

pub(crate) fn display_link_ranges_for_rows(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
) -> Vec<TerminalLinkRange> {
    filter_display_link_ranges(snapshot, detect_link_ranges_for_rows(snapshot, rows))
}

fn filter_display_link_ranges(
    snapshot: &TerminalSnapshot,
    links: Vec<TerminalLinkRange>,
) -> Vec<TerminalLinkRange> {
    links
        .into_iter()
        .filter(|link| should_display_link(snapshot, link))
        .collect()
}

fn should_display_link(snapshot: &TerminalSnapshot, link: &TerminalLinkRange) -> bool {
    link.kind != TerminalLinkKind::Path
        || !snapshot
            .lines
            .get(link.row)
            .is_some_and(|row| row.active_input)
}

pub(crate) fn detect_link_ranges_for_rows(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
) -> Vec<TerminalLinkRange> {
    let mut links = Vec::new();
    for row_index in rows {
        let Some(row) = snapshot.lines.get(row_index) else {
            continue;
        };
        let text = row.text();
        links.extend(detect_osc8_ranges(row_index, row));
        links.extend(detect_url_ranges(row_index, &text, &links));
        links.extend(detect_path_ranges(row_index, &text, &links));
    }
    links
}

pub(crate) fn detect_osc8_ranges(
    row: usize,
    terminal_row: &oxideterm_terminal::TerminalRow,
) -> Vec<TerminalLinkRange> {
    let mut ranges = Vec::new();
    let mut col = 0;
    while col < terminal_row.cells.len() {
        let Some(uri) = terminal_row.cells[col].hyperlink.as_deref() else {
            col += 1;
            continue;
        };

        let start_col = col;
        col += 1;
        while col < terminal_row.cells.len()
            && terminal_row.cells[col].hyperlink.as_deref() == Some(uri)
        {
            col += 1;
        }

        ranges.push(TerminalLinkRange {
            row,
            start_col,
            end_col: col,
            target: uri.to_string(),
            kind: terminal_link_kind_for_target(uri),
        });
    }
    ranges
}

pub(crate) fn terminal_link_kind_for_target(target: &str) -> TerminalLinkKind {
    if target.contains("://") || target.starts_with("mailto:") {
        TerminalLinkKind::Url
    } else {
        TerminalLinkKind::Path
    }
}

pub(crate) fn detect_url_ranges(
    row: usize,
    text: &str,
    existing_links: &[TerminalLinkRange],
) -> Vec<TerminalLinkRange> {
    let chars: Vec<char> = text.chars().collect();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let rest = chars[index..].iter().collect::<String>();
        let Some(prefix_len) = ["https://", "http://"]
            .iter()
            .find_map(|prefix| rest.starts_with(prefix).then_some(prefix.chars().count()))
        else {
            index += 1;
            continue;
        };

        let start = index;
        index += prefix_len;
        while index < chars.len() && !is_link_terminator(chars[index]) {
            index += 1;
        }
        let end = trim_link_end(&chars, start, index);
        if end > start + prefix_len {
            if existing_links.iter().any(|link| {
                link.row == row && ranges_overlap(start, end, link.start_col, link.end_col)
            }) {
                continue;
            }
            ranges.push(TerminalLinkRange {
                row,
                start_col: start,
                end_col: end,
                target: chars[start..end].iter().collect(),
                kind: TerminalLinkKind::Url,
            });
        }
    }
    ranges
}

pub(crate) fn detect_path_ranges(
    row: usize,
    text: &str,
    existing_links: &[TerminalLinkRange],
) -> Vec<TerminalLinkRange> {
    let chars: Vec<char> = text.chars().collect();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        let start = index;
        while index < chars.len() && !chars[index].is_whitespace() {
            index += 1;
        }
        let end = trim_link_end(&chars, start, index);
        if end <= start {
            continue;
        }
        if existing_links
            .iter()
            .any(|link| link.row == row && ranges_overlap(start, end, link.start_col, link.end_col))
        {
            continue;
        }
        let token: String = chars[start..end].iter().collect();
        if is_path_like(&token) {
            ranges.push(TerminalLinkRange {
                row,
                start_col: start,
                end_col: end,
                target: token,
                kind: TerminalLinkKind::Path,
            });
        }
    }
    ranges
}

pub(crate) fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
}

pub(crate) fn is_path_like(token: &str) -> bool {
    let token = token.trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`'));
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with("~/")
        || (token.contains('/') && token.contains('.'))
}

pub(crate) fn path_link_to_file_url(target: &str, base_dir: &Path) -> Option<String> {
    let target = target.trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`'));
    let path = if let Some(rest) = target.strip_prefix("~/") {
        home_dir()?.join(rest)
    } else {
        let path = PathBuf::from(target);
        if path.is_absolute() {
            path
        } else {
            base_dir.join(path)
        }
    };

    Some(format!("file://{}", percent_encode_path(&path)))
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub(crate) fn percent_encode_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    let mut encoded = String::with_capacity(path.len());
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub(crate) fn is_link_terminator(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '<' | '>' | '[' | ']' | '{' | '}')
}

pub(crate) fn trim_link_end(chars: &[char], start: usize, mut end: usize) -> usize {
    while end > start
        && matches!(
            chars[end - 1],
            '.' | ',' | ':' | ';' | '!' | '?' | ')' | ']'
        )
    {
        end -= 1;
    }
    end
}
