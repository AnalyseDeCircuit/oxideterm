// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

pub(crate) fn ranges(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut search_from = 0;
    text.split_whitespace().filter_map(move |token| {
        // Searching from the preceding token keeps repeated fields mapped to their actual column.
        let offset = text.get(search_from..)?.find(token)?;
        let start = search_from + offset;
        search_from = start + token.len();
        Some(start..search_from)
    })
}

pub(crate) fn text_at<'a>(text: &'a str, range: &Range<usize>) -> Option<&'a str> {
    text.get(range.clone())
}
