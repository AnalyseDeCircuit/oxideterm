// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::IndentGuide;

#[derive(Debug, Default)]
pub(super) struct IndentGuideIndex {
    root: Option<Box<IndentGuideNode>>,
}

#[derive(Debug)]
struct IndentGuideNode {
    center_line: usize,
    guides_by_start: Vec<IndentGuide>,
    guides_by_end: Vec<usize>,
    before: Option<Box<IndentGuideNode>>,
    after: Option<Box<IndentGuideNode>>,
}

impl IndentGuideIndex {
    pub(super) fn new(guides: Vec<IndentGuide>) -> Self {
        Self {
            root: IndentGuideNode::build(guides),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub(super) fn columns_for_line(&self, line: usize) -> Vec<usize> {
        let mut columns = Vec::new();
        if let Some(root) = self.root.as_deref() {
            root.collect_columns(line, &mut columns);
        }
        columns.sort_unstable();
        columns.dedup();
        columns
    }
}

impl IndentGuideNode {
    fn build(mut guides: Vec<IndentGuide>) -> Option<Box<Self>> {
        guides.retain(|guide| guide.end_line > guide.start_line);
        if guides.is_empty() {
            return None;
        }

        let pivot = &guides[guides.len() / 2];
        // Choose a physical line owned by the pivot's half-open interval
        // `(start_line, end_line]` so every recursive partition makes progress.
        let center_line = pivot.start_line + (pivot.end_line - pivot.start_line).div_ceil(2);
        let mut before = Vec::new();
        let mut overlapping = Vec::new();
        let mut after = Vec::new();
        for guide in guides {
            if guide.end_line < center_line {
                before.push(guide);
            } else if guide.start_line >= center_line {
                after.push(guide);
            } else {
                overlapping.push(guide);
            }
        }

        let mut guides_by_start = overlapping;
        guides_by_start.sort_by_key(|guide| (guide.start_line, guide.column, guide.end_line));
        let mut guides_by_end: Vec<_> = (0..guides_by_start.len()).collect();
        guides_by_end.sort_by_key(|&index| {
            let guide = &guides_by_start[index];
            (
                std::cmp::Reverse(guide.end_line),
                guide.column,
                guide.start_line,
            )
        });
        Some(Box::new(Self {
            center_line,
            guides_by_start,
            guides_by_end,
            before: Self::build(before),
            after: Self::build(after),
        }))
    }

    fn collect_columns(&self, line: usize, columns: &mut Vec<usize>) {
        match line.cmp(&self.center_line) {
            std::cmp::Ordering::Less => {
                columns.extend(
                    self.guides_by_start
                        .iter()
                        .take_while(|guide| guide.start_line < line)
                        .map(|guide| guide.column),
                );
                if let Some(before) = self.before.as_deref() {
                    before.collect_columns(line, columns);
                }
            }
            std::cmp::Ordering::Equal => {
                columns.extend(self.guides_by_start.iter().map(|guide| guide.column));
            }
            std::cmp::Ordering::Greater => {
                columns.extend(
                    self.guides_by_end
                        .iter()
                        .map(|&index| &self.guides_by_start[index])
                        .take_while(|guide| guide.end_line >= line)
                        .map(|guide| guide.column),
                );
                if let Some(after) = self.after.as_deref() {
                    after.collect_columns(line, columns);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(start_line: usize, end_line: usize, column: usize) -> IndentGuide {
        IndentGuide {
            start_line,
            end_line,
            column,
        }
    }

    #[test]
    fn queries_nested_guides_without_expanding_every_line() {
        let index = IndentGuideIndex::new(vec![guide(0, 8, 4), guide(1, 5, 8), guide(3, 4, 12)]);

        assert_eq!(index.columns_for_line(0), Vec::<usize>::new());
        assert_eq!(index.columns_for_line(2), vec![4, 8]);
        assert_eq!(index.columns_for_line(4), vec![4, 8, 12]);
        assert_eq!(index.columns_for_line(8), vec![4]);
        assert_eq!(index.columns_for_line(9), Vec::<usize>::new());
    }

    #[test]
    fn ignores_invalid_and_duplicate_guides() {
        let index = IndentGuideIndex::new(vec![guide(2, 2, 4), guide(0, 3, 4), guide(0, 3, 4)]);

        assert_eq!(index.columns_for_line(2), vec![4]);
    }

    #[test]
    fn indexed_queries_match_linear_interval_semantics() {
        let guides = vec![
            guide(0, 20, 4),
            guide(1, 3, 8),
            guide(2, 14, 12),
            guide(7, 8, 16),
            guide(9, 18, 20),
            guide(15, 22, 24),
        ];
        let index = IndentGuideIndex::new(guides.clone());

        for line in 0..25 {
            let mut expected = guides
                .iter()
                .filter(|guide| line > guide.start_line && line <= guide.end_line)
                .map(|guide| guide.column)
                .collect::<Vec<_>>();
            expected.sort_unstable();
            expected.dedup();
            assert_eq!(index.columns_for_line(line), expected, "line {line}");
        }
    }
}
