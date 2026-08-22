use std::{
    env,
    ops::Range,
    path::{Path, PathBuf},
};

use gpui::SharedString;
use oxideterm_terminal::{TerminalCell, TerminalColor, TerminalSnapshot};

#[derive(Clone, Debug)]
struct LinkText {
    text: String,
    boundaries: Vec<usize>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TerminalLinkRange {
    pub(crate) row: usize,
    pub(crate) start_col: usize,
    pub(crate) end_col: usize,
    // Cached visible link ranges are rebuilt every frame, so targets must remain cheap to clone.
    pub(crate) target: SharedString,
    pub(crate) kind: TerminalLinkKind,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum TerminalLinkKind {
    Url,
    Path,
}

pub(crate) fn link_should_be_styled(
    ranges: &[TerminalLinkRange],
    hovered: Option<&TerminalLinkRange>,
    row: usize,
    col: usize,
) -> bool {
    ranges
        .iter()
        .find(|range| range.row == row && col >= range.start_col && col < range.end_col)
        .is_some_and(|range| range.kind == TerminalLinkKind::Url || hovered == Some(range))
}

pub(crate) fn is_link_stylable_cell(cell: &TerminalCell) -> bool {
    cell.bg == TerminalColor::rgb(0x0d, 0x0f, 0x12)
}

#[cfg(test)]
pub(crate) fn display_link_ranges_with_path_detection(
    snapshot: &TerminalSnapshot,
    detect_file_paths: bool,
) -> Vec<TerminalLinkRange> {
    filter_display_link_ranges(
        snapshot,
        detect_link_ranges_for_rows_with_path_detection(
            snapshot,
            0..snapshot.lines.len(),
            detect_file_paths,
        ),
    )
}

pub(crate) fn display_link_ranges_for_rows_with_path_detection(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
    detect_file_paths: bool,
) -> Vec<TerminalLinkRange> {
    filter_display_link_ranges(
        snapshot,
        detect_link_ranges_for_rows_with_path_detection(snapshot, rows, detect_file_paths),
    )
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

pub(super) fn detect_link_ranges_for_rows_with_path_detection(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
    detect_file_paths: bool,
) -> Vec<TerminalLinkRange> {
    let mut links = Vec::new();
    for row_index in rows {
        let Some(row) = snapshot.lines.get(row_index) else {
            continue;
        };
        let link_text = link_text_for_row(row);
        let row_links_start = links.len();
        links.extend(detect_osc8_ranges(row_index, row));
        let url_ranges = detect_url_ranges(row_index, &link_text, &links[row_links_start..]);
        links.extend(url_ranges);
        if detect_file_paths {
            let path_ranges = detect_path_ranges(row_index, &link_text, &links[row_links_start..]);
            links.extend(path_ranges);
        }
    }
    links
}

fn link_text_for_row(row: &oxideterm_terminal::TerminalRow) -> LinkText {
    let mut text = String::new();
    let mut boundaries = Vec::new();
    let mut skip_wide_spacer = false;
    let mut last_end_col = 0;

    for (col, cell) in row.cells.iter().enumerate() {
        if skip_wide_spacer {
            skip_wide_spacer = false;
            continue;
        }

        boundaries.push(col);
        text.push(cell.ch);
        last_end_col = col + if cell.wide { 2 } else { 1 };

        for ch in cell.zerowidth().chars() {
            boundaries.push(col);
            text.push(ch);
        }

        skip_wide_spacer = cell.wide;
    }

    boundaries.push(last_end_col);
    LinkText { text, boundaries }
}

fn char_range_to_cell_range(
    link_text: &LinkText,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    let start_col = *link_text.boundaries.get(start)?;
    let end_col = *link_text.boundaries.get(end)?;
    (end_col > start_col).then_some((start_col, end_col))
}

pub(crate) fn detect_osc8_ranges(
    row: usize,
    terminal_row: &oxideterm_terminal::TerminalRow,
) -> Vec<TerminalLinkRange> {
    let mut ranges = Vec::new();
    let mut col = 0;
    while col < terminal_row.cells.len() {
        let Some(uri) = terminal_row.cells[col].hyperlink() else {
            col += 1;
            continue;
        };

        let start_col = col;
        col += 1;
        while col < terminal_row.cells.len() && terminal_row.cells[col].hyperlink() == Some(uri) {
            col += 1;
        }

        ranges.push(TerminalLinkRange {
            row,
            start_col,
            end_col: col,
            target: uri.into(),
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

fn detect_url_ranges(
    row: usize,
    link_text: &LinkText,
    existing_links: &[TerminalLinkRange],
) -> Vec<TerminalLinkRange> {
    const HTTPS_PREFIX: [char; 8] = ['h', 't', 't', 'p', 's', ':', '/', '/'];
    const HTTP_PREFIX: [char; 7] = ['h', 't', 't', 'p', ':', '/', '/'];

    let chars: Vec<char> = link_text.text.chars().collect();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        // Match directly on the character slice so a non-link prefix never allocates or copies
        // the remainder of a long terminal row.
        let prefix_len = if chars[index..].starts_with(&HTTPS_PREFIX) {
            HTTPS_PREFIX.len()
        } else if chars[index..].starts_with(&HTTP_PREFIX) {
            HTTP_PREFIX.len()
        } else {
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
            let Some((start_col, end_col)) = char_range_to_cell_range(link_text, start, end) else {
                continue;
            };
            if existing_links.iter().any(|link| {
                link.row == row && ranges_overlap(start_col, end_col, link.start_col, link.end_col)
            }) {
                continue;
            }
            ranges.push(TerminalLinkRange {
                row,
                start_col,
                end_col,
                target: chars[start..end].iter().collect::<String>().into(),
                kind: TerminalLinkKind::Url,
            });
        }
    }
    ranges
}

fn detect_path_ranges(
    row: usize,
    link_text: &LinkText,
    existing_links: &[TerminalLinkRange],
) -> Vec<TerminalLinkRange> {
    let chars: Vec<char> = link_text.text.chars().collect();
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
        let Some((start_col, end_col)) = char_range_to_cell_range(link_text, start, end) else {
            continue;
        };
        if existing_links.iter().any(|link| {
            link.row == row && ranges_overlap(start_col, end_col, link.start_col, link.end_col)
        }) {
            continue;
        }
        let token: String = chars[start..end].iter().collect();
        if is_path_like(&token) {
            ranges.push(TerminalLinkRange {
                row,
                start_col,
                end_col,
                target: token.into(),
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
