use crate::{LocalEventListener, TerminalSearchMatch};
use alacritty_terminal::{sync::FairMutex, term::Term};
use oxideterm_terminal_emulator::search::scrollback::ScrollbackScan;
pub use oxideterm_terminal_emulator::search::scrollback::{
    TerminalSearchOptions, TerminalSearchQuery, TerminalSearchResult,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct TerminalSearchSource {
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    cols: usize,
}

impl TerminalSearchSource {
    pub(crate) fn new(term: Arc<FairMutex<Term<LocalEventListener>>>, cols: usize) -> Self {
        Self { term, cols }
    }

    pub fn search_matches(
        &self,
        query: &str,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Vec<TerminalSearchMatch> {
        self.search_matches_with_options(query, TerminalSearchOptions::default(), is_cancelled)
    }

    pub fn search_matches_with_options(
        &self,
        query: &str,
        options: TerminalSearchOptions,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Vec<TerminalSearchMatch> {
        let Ok(query) = TerminalSearchQuery::new(query, options) else {
            return Vec::new();
        };
        let result = self.search(&query, is_cancelled);
        if is_cancelled() {
            return Vec::new();
        }
        self.resolve_search_result(result)
    }

    pub fn search(
        &self,
        query: &TerminalSearchQuery,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> TerminalSearchResult {
        let mut scan = ScrollbackScan::new(&self.term.lock(), self.cols, query);
        while !scan.advance(|| self.term.lock(), is_cancelled) {}
        scan.finish(is_cancelled)
    }

    pub fn resolve_search_result(&self, result: TerminalSearchResult) -> Vec<TerminalSearchMatch> {
        result.resolve(|| self.term.lock())
    }
}

#[cfg(test)]
use alacritty_terminal::term::cell::{Cell, Flags};
#[cfg(test)]
use oxideterm_terminal_emulator::search::{append_grid_line_text, search_logical_line_matches};

#[cfg(test)]
fn search_pattern_matches(
    text: &str,
    cell_map: &[(i32, usize, usize)],
    pattern: &regex::Regex,
    max_cols: usize,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Vec<TerminalSearchMatch> {
    let mut previous_byte = 0;
    let mut previous_char = 0;
    pattern
        .find_iter(text)
        .take_while(|_| !is_cancelled())
        .filter_map(|matched| {
            let start = previous_char + text[previous_byte..matched.start()].chars().count();
            let end = start + matched.as_str().chars().count();
            previous_byte = matched.end();
            previous_char = end;
            TerminalSearchMatch::from_cell_map(cell_map, start, end, max_cols)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_search_preserves_text_history_tails_and_chunk_boundaries() {
        let mut session =
            crate::TerminalSession::recording_playback(400, 3, Default::default(), 100);
        for line in 0..70 {
            let row = format!("row{line:02} {}", "abcdefghij".repeat(39));
            session.feed_recording_output(format!("\x1b[31m{row}\x1b[0m\r\n").as_bytes());
        }
        let source = session.search_source().unwrap();
        for (query, regex, expected) in [
            ("row32", false, vec![(-36, 0, 5)]),
            (
                "ij    ",
                false,
                (0..70).map(|i| (i - 68, 394, 400)).collect(),
            ),
            ("ghij\nrow33", false, vec![(-36, 392, 396), (-35, 0, 5)]),
            (r"ghij\nrow33", true, vec![(-36, 392, 396), (-35, 0, 5)]),
        ] {
            let matches = source.search_matches_with_options(
                query,
                TerminalSearchOptions {
                    regex,
                    case_sensitive: true,
                },
                &|| false,
            );
            let actual: Vec<_> = matches
                .iter()
                .flat_map(|hit| &hit.ranges)
                .map(|range| (range.line, range.start_col, range.end_col))
                .collect();
            assert_eq!(actual, expected, "{query}");
        }
        let mut wrapped =
            crate::TerminalSession::recording_playback(80, 3, Default::default(), 100);
        wrapped.feed_recording_output(
            format!(
                "{}needle{}\r\none\r\ntwo\r\nthree",
                "a".repeat(76),
                "b".repeat(78)
            )
            .as_bytes(),
        );
        let matches = wrapped
            .search_source()
            .unwrap()
            .search_matches("needle", &|| false);
        let ranges: Vec<_> = matches
            .iter()
            .flat_map(|hit| &hit.ranges)
            .map(|range| (range.line, range.start_col, range.end_col))
            .collect();
        assert_eq!(ranges, vec![(-2, 76, 80), (-1, 0, 2)]);
    }

    #[test]
    fn search_ranges_cover_complete_wide_cells() {
        let mut cells = Vec::new();
        for ch in "a中文b".chars() {
            let mut cell = Cell::default();
            cell.c = ch;
            if matches!(ch, '中' | '文') {
                cell.flags.insert(Flags::WIDE_CHAR);
            }
            cells.push(cell);
            if matches!(ch, '中' | '文') {
                let mut spacer = Cell::default();
                spacer.flags.insert(Flags::WIDE_CHAR_SPACER);
                cells.push(spacer);
            }
        }
        let mut text = String::new();
        let mut map = Vec::new();
        append_grid_line_text(
            cells.iter().map(alacritty_terminal::grid::GridCell::view),
            0,
            6,
            &mut text,
            &mut map,
        );
        for (query, expected) in [("中", (1, 3)), ("中文", (1, 5)), ("文b", (3, 6))] {
            for options in [
                TerminalSearchOptions::default(),
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                TerminalSearchOptions {
                    case_sensitive: false,
                    ..Default::default()
                },
            ] {
                let hits = if options == TerminalSearchOptions::default() {
                    search_logical_line_matches(&text, &map, query, 6)
                } else {
                    search_pattern_matches(
                        &text,
                        &map,
                        &options.compile(query).unwrap(),
                        6,
                        &|| false,
                    )
                };
                assert_eq!(
                    hits.iter()
                        .flat_map(|hit| hit.ranges.iter())
                        .map(|range| (range.line, range.start_col, range.end_col))
                        .collect::<Vec<_>>(),
                    vec![(0, expected.0, expected.1)],
                    "{query}, {options:?}"
                );
            }
        }
    }

    #[test]
    fn search_modes_preserve_unicode_coordinates_and_cross_line_ranges() {
        let text = "猫 Foo foo\nbar";
        let map = vec![
            (0, 0, 2),
            (0, 2, 3),
            (0, 3, 4),
            (0, 4, 5),
            (0, 5, 6),
            (0, 6, 7),
            (0, 7, 8),
            (0, 8, 9),
            (0, 9, 10),
            (0, 80, 81),
            (1, 0, 1),
            (1, 1, 2),
            (1, 2, 3),
        ];
        for (query, options, expected) in [
            (
                "foo",
                TerminalSearchOptions::default(),
                vec![vec![(0, 7, 10)]],
            ),
            (
                "foo",
                TerminalSearchOptions {
                    case_sensitive: false,
                    ..Default::default()
                },
                vec![vec![(0, 3, 6)], vec![(0, 7, 10)]],
            ),
            (
                "F.o",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 3, 6)]],
            ),
            ("F.o", TerminalSearchOptions::default(), vec![]),
            (
                "foo.bar",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![],
            ),
            (
                ".*",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 0, 10)], vec![(1, 0, 3)]],
            ),
            (
                "(?s).*",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 0, 10), (1, 0, 3)]],
            ),
            (
                r"foo\nbar",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 7, 10), (1, 0, 3)]],
            ),
            (
                "foo\nbar",
                TerminalSearchOptions::default(),
                vec![vec![(0, 7, 10), (1, 0, 3)]],
            ),
        ] {
            let pattern = options.compile(query).unwrap();
            let actual = search_pattern_matches(text, &map, &pattern, 80, &|| false);
            let ranges: Vec<Vec<_>> = actual
                .iter()
                .map(|hit| {
                    hit.ranges
                        .iter()
                        .map(|range| (range.line, range.start_col, range.end_col))
                        .collect()
                })
                .collect();
            assert_eq!(ranges, expected, "query={query:?}, options={options:?}");
        }
    }

    #[test]
    fn invalid_and_zero_width_patterns_do_not_produce_highlights() {
        let options = TerminalSearchOptions {
            regex: true,
            ..Default::default()
        };
        assert!(options.compile("[").is_err());
        assert_eq!(
            search_pattern_matches(
                "abc",
                &[(0, 0, 1), (0, 1, 2), (0, 2, 3)],
                &options.compile("^").unwrap(),
                80,
                &|| false
            ),
            vec![]
        );
    }
}
