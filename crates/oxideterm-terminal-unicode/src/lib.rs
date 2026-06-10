use std::ops::Range;

use oxideterm_terminal::{TerminalCell, TerminalRow};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellCluster {
    pub logical_col: usize,
    pub visual_col: usize,
    pub text: String,
    pub cells: usize,
    pub rtl: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalVisualRun {
    pub visual_col: usize,
    pub logical_cols: Vec<usize>,
    pub text: String,
    pub cells: usize,
    pub rtl: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalVisualLine {
    pub clusters: Vec<CellCluster>,
    pub visual_runs: Vec<TerminalVisualRun>,
    pub logical_to_visual: Vec<Option<usize>>,
    pub visual_to_logical: Vec<Option<usize>>,
    pub has_bidi: bool,
}

impl TerminalVisualLine {
    pub fn identity(row: &TerminalRow) -> Self {
        let logical_cols = row.cells.len();
        let mut clusters = Vec::with_capacity(logical_cols);
        let mut visual_runs = Vec::new();
        let mut logical_to_visual = vec![None; logical_cols];
        let mut visual_to_logical = vec![None; logical_cols];
        let mut run_text = String::new();
        let mut run_cols = Vec::new();
        let mut run_cells = 0;

        for (col, cell) in row.cells.iter().enumerate() {
            let cells = cell_width(cell);
            let text = cell_text(cell);
            logical_to_visual[col] = Some(col);
            for offset in 0..cells {
                if col + offset < visual_to_logical.len() {
                    visual_to_logical[col + offset] = Some(col);
                }
            }
            clusters.push(CellCluster {
                logical_col: col,
                visual_col: col,
                text: text.clone(),
                cells,
                rtl: false,
            });
            run_text.push_str(&text);
            run_cols.push(col);
            run_cells += cells;
        }

        if !run_text.is_empty() {
            visual_runs.push(TerminalVisualRun {
                visual_col: 0,
                logical_cols: run_cols,
                text: run_text,
                cells: run_cells,
                rtl: false,
            });
        }

        Self {
            clusters,
            visual_runs,
            logical_to_visual,
            visual_to_logical,
            has_bidi: false,
        }
    }

    pub fn visual_col_for_logical_col(&self, col: usize) -> usize {
        self.logical_to_visual
            .get(col)
            .copied()
            .flatten()
            .unwrap_or(col)
    }

    pub fn logical_col_for_visual_col(&self, col: usize) -> usize {
        self.visual_to_logical
            .get(col)
            .copied()
            .flatten()
            .unwrap_or(col)
    }

    pub fn visual_rects_for_logical_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = Range<usize>> + '_ {
        let mut cols = range
            .flat_map(|logical_col| {
                let cell = self
                    .clusters
                    .iter()
                    .find(|cluster| cluster.logical_col == logical_col)?;
                Some(cell.visual_col..cell.visual_col + cell.cells)
            })
            .flatten()
            .collect::<Vec<_>>();
        cols.sort_unstable();
        cols.dedup();

        let mut ranges = Vec::new();
        let mut start = None;
        let mut previous = None;
        for col in cols {
            match (start, previous) {
                (Some(active), Some(last)) if col == last + 1 => {
                    previous = Some(col);
                    start = Some(active);
                }
                (Some(active), Some(last)) => {
                    ranges.push(active..last + 1);
                    start = Some(col);
                    previous = Some(col);
                }
                _ => {
                    start = Some(col);
                    previous = Some(col);
                }
            }
        }
        if let (Some(active), Some(last)) = (start, previous) {
            ranges.push(active..last + 1);
        }

        ranges.into_iter()
    }
}

pub fn visual_line_for_row(row: &TerminalRow) -> TerminalVisualLine {
    let Some(content_span) = content_span(row) else {
        return TerminalVisualLine::identity(row);
    };
    let clusters = row_clusters(row, content_span.clone());
    if clusters.is_empty() || !contains_bidi_text(&clusters) {
        return TerminalVisualLine::identity(row);
    }

    let text = clusters
        .iter()
        .map(|cluster| cluster.text.as_str())
        .collect::<String>();
    let bidi = BidiInfo::new(&text, None);
    let Some(paragraph) = bidi.paragraphs.first() else {
        return TerminalVisualLine::identity(row);
    };

    let mut levels = Vec::with_capacity(clusters.len());
    for cluster in &clusters {
        let level = bidi
            .levels
            .get(cluster.byte_range.start)
            .copied()
            .unwrap_or(paragraph.level);
        levels.push(level.number());
    }

    if levels.iter().all(|level| *level == 0) {
        return TerminalVisualLine::identity(row);
    }

    let visual_order = reordered_cluster_indices(&levels);
    let mut logical_to_visual = identity_map(row.cells.len());
    let mut visual_to_logical = identity_map(row.cells.len());
    let mut visual_clusters = Vec::with_capacity(clusters.len());
    let mut visual_runs: Vec<TerminalVisualRun> = Vec::new();
    let mut visual_col = content_span.start;

    for logical_cluster_index in visual_order {
        let source = &clusters[logical_cluster_index];
        let rtl = levels[logical_cluster_index] % 2 == 1;
        let logical_col = source.logical_col;
        logical_to_visual[logical_col] = Some(visual_col);
        for offset in 0..source.cells {
            if visual_col + offset < visual_to_logical.len() {
                visual_to_logical[visual_col + offset] = Some(logical_col);
            }
        }
        visual_clusters.push(CellCluster {
            logical_col,
            visual_col,
            text: source.text.clone(),
            cells: source.cells,
            rtl,
        });
        visual_col += source.cells;
    }

    for cluster in &visual_clusters {
        if let Some(run) = visual_runs.last_mut()
            && run.rtl == cluster.rtl
            && run.visual_col + run.cells == cluster.visual_col
        {
            run.logical_cols.push(cluster.logical_col);
            run.text.push_str(&cluster.text);
            run.cells += cluster.cells;
            continue;
        }
        visual_runs.push(TerminalVisualRun {
            visual_col: cluster.visual_col,
            logical_cols: vec![cluster.logical_col],
            text: cluster.text.clone(),
            cells: cluster.cells,
            rtl: cluster.rtl,
        });
    }

    TerminalVisualLine {
        clusters: visual_clusters,
        visual_runs,
        logical_to_visual,
        visual_to_logical,
        has_bidi: true,
    }
}

fn identity_map(cols: usize) -> Vec<Option<usize>> {
    (0..cols).map(Some).collect()
}

fn content_span(row: &TerminalRow) -> Option<Range<usize>> {
    let start = row.cells.iter().position(is_content_cell)?;
    let end = row.cells.iter().rposition(is_content_cell)? + 1;
    Some(start..end)
}

fn is_content_cell(cell: &TerminalCell) -> bool {
    cell.ch != ' ' || !cell.zerowidth.is_empty() || cell.cursor || cell.hyperlink.is_some()
}

fn row_clusters(row: &TerminalRow, span: Range<usize>) -> Vec<SourceCluster> {
    let mut byte_offset = 0;
    row.cells
        .iter()
        .enumerate()
        .skip(span.start)
        .take(span.end.saturating_sub(span.start))
        .map(|(logical_col, cell)| {
            let text = normalized_cell_text(cell);
            let byte_range = byte_offset..byte_offset + text.len();
            byte_offset += text.len();
            SourceCluster {
                logical_col,
                text,
                cells: cell_width(cell),
                byte_range,
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
struct SourceCluster {
    logical_col: usize,
    text: String,
    cells: usize,
    byte_range: Range<usize>,
}

fn normalized_cell_text(cell: &TerminalCell) -> String {
    let text = cell_text(cell);
    if text.graphemes(true).count() <= 1 {
        text
    } else {
        text.graphemes(true).collect::<String>()
    }
}

fn cell_text(cell: &TerminalCell) -> String {
    if cell.zerowidth.is_empty() {
        cell.ch.to_string()
    } else {
        let mut text = String::with_capacity(cell.ch.len_utf8() + cell.zerowidth.len());
        text.push(cell.ch);
        text.push_str(&cell.zerowidth);
        text
    }
}

fn cell_width(cell: &TerminalCell) -> usize {
    if cell.ch.is_whitespace() {
        1
    } else if cell.wide {
        2
    } else {
        1
    }
}

fn contains_bidi_text(clusters: &[SourceCluster]) -> bool {
    clusters.iter().any(|cluster| {
        cluster.text.chars().any(|ch| {
            matches!(
                unicode_bidi::bidi_class(ch),
                unicode_bidi::BidiClass::R
                    | unicode_bidi::BidiClass::AL
                    | unicode_bidi::BidiClass::AN
            )
        })
    })
}

fn reordered_cluster_indices(levels: &[u8]) -> Vec<usize> {
    let mut indices = (0..levels.len()).collect::<Vec<_>>();
    let Some(max_level) = levels.iter().copied().max() else {
        return indices;
    };
    let lowest_odd_level = levels
        .iter()
        .copied()
        .filter(|level| level % 2 == 1)
        .min()
        .unwrap_or(max_level + 1);
    if lowest_odd_level > max_level {
        return indices;
    }

    for level in (lowest_odd_level..=max_level).rev() {
        let mut start = 0;
        while start < indices.len() {
            while start < indices.len() && levels[indices[start]] < level {
                start += 1;
            }
            let mut end = start;
            while end < indices.len() && levels[indices[end]] >= level {
                end += 1;
            }
            indices[start..end].reverse();
            start = end;
        }
    }

    indices
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oxideterm_terminal::{TerminalAttrs, TerminalCell, TerminalColor, TerminalRow};

    use super::*;

    fn row(text: &str) -> TerminalRow {
        let mut row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(
                text.chars()
                    .map(|ch| TerminalCell {
                        ch,
                        zerowidth: String::new(),
                        wide: false,
                        fg: TerminalColor::rgb(0xff, 0xff, 0xff),
                        bg: TerminalColor::rgb(0, 0, 0),
                        attrs: TerminalAttrs::default(),
                        hyperlink: None,
                        cursor: false,
                    })
                    .collect(),
            ),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        row.refresh_signature();
        row
    }

    #[test]
    fn ltr_rows_use_identity_mapping() {
        let line = visual_line_for_row(&row("abc"));
        assert!(!line.has_bidi);
        assert_eq!(line.logical_to_visual, vec![Some(0), Some(1), Some(2)]);
        assert_eq!(line.visual_to_logical, vec![Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn arabic_row_is_reordered_without_changing_logical_text() {
        let source = "السلام";
        let line = visual_line_for_row(&row(source));
        assert!(line.has_bidi);
        assert_eq!(row(source).text(), source);
        assert_eq!(
            line.clusters.first().unwrap().logical_col,
            source.chars().count() - 1
        );
    }

    #[test]
    fn mixed_hebrew_text_has_visual_mapping() {
        let line = visual_line_for_row(&row("abc שלום 123 def"));
        assert!(line.has_bidi);
        assert_eq!(line.visual_col_for_logical_col(0), 0);
        assert_ne!(line.visual_col_for_logical_col(4), 4);
        let visual_col = line.visual_col_for_logical_col(4);
        assert_eq!(line.logical_col_for_visual_col(visual_col), 4);
    }

    #[test]
    fn logical_ranges_split_into_visual_rects() {
        let line = visual_line_for_row(&row("abc שלום def"));
        let rects = line
            .visual_rects_for_logical_range(4..8)
            .collect::<Vec<_>>();
        assert!(!rects.is_empty());
        assert_eq!(rects.iter().map(|range| range.len()).sum::<usize>(), 4);
    }

    #[test]
    fn rtl_content_ignores_terminal_trailing_blanks() {
        let mut row = row("שלום");
        while row.cells.len() < 16 {
            row.cells_mut().push(TerminalCell {
                ch: ' ',
                zerowidth: String::new(),
                wide: false,
                fg: TerminalColor::rgb(0xff, 0xff, 0xff),
                bg: TerminalColor::rgb(0, 0, 0),
                attrs: TerminalAttrs::default(),
                hyperlink: None,
                cursor: false,
            });
        }
        row.refresh_signature();

        let line = visual_line_for_row(&row);

        assert!(line.has_bidi);
        assert!(line.clusters.iter().all(|cluster| cluster.visual_col < 4));
        assert_eq!(line.visual_col_for_logical_col(15), 15);
        assert_eq!(line.logical_col_for_visual_col(15), 15);
    }

    #[test]
    fn rtl_content_preserves_terminal_leading_blanks() {
        let line = visual_line_for_row(&row("  שלום   "));

        assert!(line.has_bidi);
        assert_eq!(line.visual_col_for_logical_col(0), 0);
        assert_eq!(line.visual_col_for_logical_col(1), 1);
        assert!(line.clusters.iter().all(|cluster| cluster.visual_col >= 2));
    }
}
