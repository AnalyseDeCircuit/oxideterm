//! UTF-8 primary characters and exact attribute runs, indexed by terminal columns.
use super::*;

// Bound conversion attempts during scrolling and repeated reflow. More fragmented rows use
// indexed attributes after this run-discovery budget, even when text could save more memory.
const MAX_TEXT_RUNS: usize = 16;

struct RunLayout {
    ends: [usize; MAX_TEXT_RUNS],
    count: usize,
}

#[derive(Clone, Debug)]
struct Run {
    end: usize,
    attributes: Attributes,
    flags: Flags,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TextHistory {
    text: String,
    runs: Vec<Run>,
    len: usize,
    ascii: bool,
    plain: bool,
}

fn plain_cell(cell: &Cell) -> bool {
    cell.c.is_ascii()
        && cell.c != '\0'
        && !cell.flags.intersects(
            Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
        )
        && cell
            .extra
            .as_ref()
            .is_none_or(|extra| extra.zerowidth.is_empty())
}

impl TextHistory {
    pub(super) fn iter(&self) -> TextIter<'_> {
        TextIter {
            text: &self.text,
            ascii: self.ascii,
            front_byte: 0,
            back_byte: self.text.len(),
            runs: &self.runs,
            columns: 0..self.len,
            front_run: 0,
            back_run: self.runs.len() - 1,
        }
    }
    fn plan_runs(source: &[Cell], budget: usize) -> Option<RunLayout> {
        // Compare against even the smallest indexed table before allocating a text candidate.
        // Many alternating styles therefore keep their compact random-access representation.
        let mut layout = RunLayout {
            ends: [0; MAX_TEXT_RUNS],
            count: 0,
        };
        let mut start = 0;
        while start < source.len() {
            layout.count += 1;
            if layout.count > MAX_TEXT_RUNS
                || source.len()
                    + layout.count * std::mem::size_of::<Run>()
                    + std::mem::size_of::<PackedHistory>()
                    >= budget
            {
                return None;
            }
            let end = start + attribute_run_len::<true>(&source[start..]);
            layout.ends[layout.count - 1] = end;
            start = end;
        }
        Some(layout)
    }

    pub(super) fn encode(source: &[Cell], reuse: Option<Self>) -> Option<Self> {
        let budget = (source.len() * 8
            + std::mem::size_of::<Attributes>()
            + std::mem::size_of::<IndexedHistory>())
        .min(std::mem::size_of_val(source) * 3 / 4);
        // Recycled text rows already have run storage. Build it in one scan; without reusable
        // storage, reject expensive candidates before allocating or cloning their attributes.
        let layout = if reuse.is_none() {
            Some(Self::plan_runs(source, budget)?)
        } else {
            None
        };
        let mut stored = reuse.unwrap_or_default();
        stored.text.clear();
        stored.runs.clear();
        if let Some(layout) = &layout {
            stored.runs.reserve_exact(layout.count);
        }
        let mut plain = true;
        let mut start = 0;
        while start < source.len() {
            let cell = &source[start];
            if stored.runs.len() == MAX_TEXT_RUNS
                || source.len()
                    + (stored.runs.len() + 1) * std::mem::size_of::<Run>()
                    + std::mem::size_of::<PackedHistory>()
                    >= budget
            {
                return None;
            }
            let end = if let Some(layout) = &layout {
                layout.ends[stored.runs.len()]
            } else {
                start + attribute_run_len::<true>(&source[start..])
            };
            plain &= !cell.flags.intersects(
                Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
            ) && cell
                .extra
                .as_ref()
                .is_none_or(|extra| extra.zerowidth.is_empty());
            stored.runs.push(Run {
                end,
                attributes: Attributes {
                    fg: cell.fg,
                    bg: cell.bg,
                    extra: cell.extra.clone(),
                },
                flags: cell.flags,
            });
            start = end;
        }
        stored.len = source.len();
        // Validate ASCII while copying, rather than scanning primary characters twice.
        // No provisional bytes become text until the entire row has been checked.
        let mut ascii = true;
        let mut nonzero = true;
        let mut bytes = stored.text.into_bytes();
        bytes.reserve_exact(source.len());
        bytes.extend(source.iter().map(|cell| {
            ascii &= cell.c.is_ascii();
            nonzero &= cell.c != '\0';
            cell.c as u8
        }));
        if !ascii {
            bytes.clear();
        }
        stored.text = String::from_utf8(bytes).unwrap();
        if !ascii {
            stored.text.extend(source.iter().map(|cell| cell.c));
        }
        stored.ascii = ascii;
        stored.plain = plain && ascii && nonzero;
        stored.reclaim();
        (stored.bytes() + std::mem::size_of::<PackedHistory>() - std::mem::size_of::<Self>()
            < budget)
            .then_some(stored)
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }
    pub(super) fn bytes(&self) -> usize {
        self.text.capacity()
            + self.runs.capacity() * std::mem::size_of::<Run>()
            + std::mem::size_of::<Self>()
    }
    pub(super) fn ascii_prefix(&self) -> Option<&str> {
        self.plain.then_some(&self.text)
    }

    pub(super) fn decode(&self, range: std::ops::Range<usize>) -> Vec<Cell> {
        let mut cells = Vec::with_capacity(range.len());
        let mut start = range.start;
        let mut run_index = self.runs.partition_point(|run| run.end <= start);
        let mut cursor = HistoryCursor::default();
        while start < range.end {
            let run = &self.runs[run_index];
            let end = run.end.min(range.end);
            // Resolve attributes once per run, not once per destination cell.
            if self.ascii {
                cells.extend(
                    self.text.as_bytes()[start..end]
                        .iter()
                        .map(|byte| run.cell(char::from(*byte)).into_owned()),
                );
            } else {
                let from = self.byte_at(start, &mut cursor);
                let to = self.byte_at(end, &mut cursor);
                let mut chars = self.text[from..to].chars();
                cells.extend((start..end).map(|_| run.cell(chars.next().unwrap()).into_owned()));
            }
            start = end;
            run_index += 1;
        }
        cells
    }

    fn byte_at(&self, index: usize, cursor: &mut HistoryCursor) -> usize {
        if self.ascii {
            return index;
        }
        // Seek from the closest known column. Forward/reverse scans remain linear, including
        // direction changes; random access never requires a per-column offset array.
        if index.abs_diff(cursor.column) > index {
            cursor.column = 0;
            cursor.byte = 0;
        }
        if index.abs_diff(cursor.column) > self.len - index {
            cursor.column = self.len;
            cursor.byte = self.text.len();
        }
        if index > cursor.column {
            for ch in self.text[cursor.byte..].chars().take(index - cursor.column) {
                cursor.byte += ch.len_utf8();
            }
        } else if index < cursor.column {
            for ch in self.text[..cursor.byte]
                .chars()
                .rev()
                .take(cursor.column - index)
            {
                cursor.byte -= ch.len_utf8();
            }
        }
        cursor.column = index;
        cursor.byte
    }

    #[inline(always)]
    pub(super) fn cell(&self, index: usize, cursor: &mut HistoryCursor) -> CellRef<'_> {
        let c = if self.ascii {
            char::from(self.text.as_bytes()[index])
        } else {
            let byte = self.byte_at(index, cursor);
            self.text[byte..].chars().next().unwrap()
        };
        if self.runs.len() != 1 {
            while index >= self.runs[cursor.run].end {
                cursor.run += 1;
            }
            while cursor.run > 0 && index < self.runs[cursor.run - 1].end {
                cursor.run -= 1;
            }
        }
        let run = &self.runs[cursor.run];
        CellRef {
            c,
            fg: run.attributes.fg,
            bg: run.attributes.bg,
            flags: run.flags,
            extra: run.attributes.extra.as_ref(),
        }
    }

    pub(super) fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        let byte = self.byte_at(len, &mut HistoryCursor::default());
        self.text.truncate(byte);
        self.runs
            .truncate(self.runs.partition_point(|run| run.end < len) + 1);
        self.runs.last_mut().unwrap().end = len;
        self.len = len;
        // Resizing can expose a long repeated suffix. Preserve the row's implicit-tail
        // contract instead of retaining the materialized tail from an earlier growth.
        let run_start = self.runs.iter().rev().nth(1).map_or(0, |run| run.end);
        let tail = self.text.chars().next_back().unwrap();
        let repeated = self
            .text
            .chars()
            .rev()
            .take(len - run_start)
            .take_while(|c| *c == tail)
            .count();
        if repeated - 1 >= (len / 4).max(4) {
            self.text
                .truncate(self.text.len() - (repeated - 1) * tail.len_utf8());
            self.len -= repeated - 1;
            self.runs.last_mut().unwrap().end = self.len;
        }
        self.reclaim();
    }

    pub(super) fn extend_tail(&mut self, len: usize) {
        if len <= self.len {
            return;
        }
        let c = self.text.chars().next_back().unwrap();
        self.text.reserve_exact((len - self.len) * c.len_utf8());
        self.text.extend(std::iter::repeat_n(c, len - self.len));
        self.runs.last_mut().unwrap().end = len;
        self.len = len;
    }

    pub(super) fn push(&mut self, cell: Cell) {
        self.plain &= plain_cell(&cell);
        self.ascii &= cell.c.is_ascii();
        self.len += 1;
        self.text.reserve_exact(cell.c.len_utf8());
        self.text.push(cell.c);
        let attributes = Attributes {
            fg: cell.fg,
            bg: cell.bg,
            extra: cell.extra,
        };
        let last = self.runs.last_mut().unwrap();
        if last.attributes == attributes && last.flags == cell.flags {
            last.end = self.len;
        } else {
            self.runs.reserve_exact(1);
            self.runs.push(Run {
                end: self.len,
                attributes,
                flags: cell.flags,
            });
        }
    }

    fn reclaim(&mut self) {
        if self.text.len() <= self.text.capacity() * 3 / 4 {
            self.text.shrink_to_fit();
        }
        if self.runs.len() <= self.runs.capacity() * 3 / 4 {
            self.runs.shrink_to_fit();
        }
    }
}

pub(super) struct TextIter<'a> {
    text: &'a str,
    ascii: bool,
    front_byte: usize,
    back_byte: usize,
    runs: &'a [Run],
    columns: std::ops::Range<usize>,
    front_run: usize,
    back_run: usize,
}
impl<'a> Iterator for TextIter<'a> {
    type Item = CellRef<'a>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }
    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let column = self.columns.nth(n)?;
        let c = if self.ascii {
            char::from(self.text.as_bytes()[column])
        } else {
            let (offset, c) = self.text[self.front_byte..].char_indices().nth(n).unwrap();
            self.front_byte += offset + c.len_utf8();
            c
        };
        while column >= self.runs[self.front_run].end {
            self.front_run += 1;
        }
        Some(self.runs[self.front_run].cell(c))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.columns.size_hint()
    }
    fn fold<B, F>(self, mut value: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let mut start = self.columns.start;
        let mut chars = self.text[self.front_byte..self.back_byte].chars();
        for run in &self.runs[self.front_run..] {
            if start >= self.columns.end {
                break;
            }
            let end = run.end.min(self.columns.end);
            if self.ascii {
                value = self.text.as_bytes()[start..end]
                    .iter()
                    .fold(value, |value, byte| f(value, run.cell(char::from(*byte))));
            } else {
                value = chars
                    .by_ref()
                    .take(end - start)
                    .fold(value, |value, c| f(value, run.cell(c)));
            }
            start = end;
        }
        value
    }
}
impl DoubleEndedIterator for TextIter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.nth_back(0)
    }
    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let column = self.columns.nth_back(n)?;
        let c = if self.ascii {
            char::from(self.text.as_bytes()[column])
        } else {
            let (offset, c) = self.text[..self.back_byte]
                .char_indices()
                .rev()
                .nth(n)
                .unwrap();
            self.back_byte = offset;
            c
        };
        while self.back_run > 0 && column < self.runs[self.back_run - 1].end {
            self.back_run -= 1;
        }
        Some(self.runs[self.back_run].cell(c))
    }
}
impl ExactSizeIterator for TextIter<'_> {}

impl Run {
    #[inline]
    fn cell(&self, c: char) -> CellRef<'_> {
        CellRef {
            c,
            fg: self.attributes.fg,
            bg: self.attributes.bg,
            flags: self.flags,
            extra: self.attributes.extra.as_ref(),
        }
    }
}
