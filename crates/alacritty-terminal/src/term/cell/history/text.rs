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

const WIDE_PAIR_FLAGS_MASK: u16 =
    !(Flags::WIDE_CHAR.bits() | Flags::WIDE_CHAR_SPACER.bits());

#[derive(Clone, Debug, Default)]
pub(super) struct TextHistory {
    text: String,
    runs: Vec<Run>,
    len: usize,
    ascii: bool,
    plain: bool,
    // When true, every NUL byte in `text` encodes a WIDE_CHAR_SPACER (' ')
    // whose preceding character carries WIDE_CHAR, keeping wide runs intact.
    wide_spacers: bool,
}

#[inline(always)]
fn decode_wide_char_flags(
    text: &str,
    wide_spacers: bool,
    byte_after_char: usize,
    c: char,
    run_flags: Flags,
) -> (char, Flags) {
    if wide_spacers {
        if c == '\0' {
            return (' ', run_flags | Flags::WIDE_CHAR_SPACER);
        }
        if text.as_bytes().get(byte_after_char) == Some(&0) {
            return (c, run_flags | Flags::WIDE_CHAR);
        }
    }
    (c, run_flags)
}

fn can_use_wide_spacers(source: &[Cell]) -> bool {
    let mut i = 0;
    let mut has_wide = false;
    while i < source.len() {
        let cell = &source[i];
        if cell.c == '\0' {
            return false;
        }
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            return false;
        }
        if cell.flags.contains(Flags::WIDE_CHAR) {
            let Some(next) = source.get(i + 1) else {
                return false;
            };
            let base_flags = cell.flags - Flags::WIDE_CHAR;
            if next.c != ' '
                || next.flags != (base_flags | Flags::WIDE_CHAR_SPACER)
                || next.fg != cell.fg
                || next.bg != cell.bg
                || next.extra != cell.extra
            {
                return false;
            }
            has_wide = true;
            i += 2;
        } else {
            i += 1;
        }
    }
    has_wide
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
    pub(super) fn set_ascii(&mut self, text: &str, style: &Cell, tail: Option<&Cell>) {
        self.text.clear();
        self.text.reserve_exact(text.len() + usize::from(tail.is_some()));
        self.text.push_str(text);
        self.runs.clear();
        let last = tail.unwrap_or(style);
        let separate_tail = style.fg != last.fg || style.flags != last.flags || style.extra != last.extra;
        self.runs.reserve_exact(1 + usize::from(separate_tail));
        if tail.is_some() {
            self.text.push(' ');
        }
        let run = |end, cell: &Cell| Run {
            end,
            attributes: Attributes { fg: cell.fg, bg: cell.bg, extra: cell.extra.clone() },
            flags: cell.flags,
        };
        if separate_tail {
            self.runs.push(run(text.len(), style));
        }
        self.len = self.text.len();
        self.runs.push(run(self.len, last));
        self.ascii = true;
        self.plain = style.extra.as_ref().is_none_or(|extra| extra.zerowidth.is_empty());
        self.wide_spacers = false;
        self.reclaim();
    }

    pub(super) fn set_utf8(
        &mut self,
        text: &str,
        cols: usize,
        wide_count: usize,
        style: &Cell,
        tail: Option<&Cell>,
    ) {
        self.text.clear();
        let exact_bytes = text.len() + wide_count + usize::from(tail.is_some());
        self.text.reserve_exact(exact_bytes);
        if wide_count == 0 {
            self.text.push_str(text);
        } else {
            // Copy contiguous narrow UTF-8 slices directly and insert NUL only after wide chars.
            // No Unicode codepoint below U+1100 (UTF-8 lead < 0xE1) has display width 2.
            let bytes = text.as_bytes();
            let mut run_start = 0;
            let mut i = 0;
            while i < bytes.len() {
                let lead = bytes[i];
                if lead < 0x80 {
                    i += 1;
                    continue;
                }
                if lead < 0xe0 {
                    i += 2;
                    continue;
                }
                if lead == 0xe0 {
                    i += 3;
                    continue;
                }
                if (0xe5..=0xe9).contains(&lead) || (0xeb..=0xec).contains(&lead) {
                    let next = i + 3;
                    self.text.push_str(&text[run_start..next]);
                    self.text.push('\0');
                    run_start = next;
                    i = next;
                    continue;
                }
                let c = text[i..].chars().next().unwrap();
                let next = i + c.len_utf8();
                if crate::term::char_width_fast(c) == Some(2) {
                    self.text.push_str(&text[run_start..next]);
                    self.text.push('\0');
                    run_start = next;
                }
                i = next;
            }
            if run_start < bytes.len() {
                self.text.push_str(&text[run_start..]);
            }
        }
        if tail.is_some() {
            self.text.push(' ');
        }
        self.runs.clear();
        let last = tail.unwrap_or(style);
        let separate_tail =
            style.fg != last.fg || style.flags != last.flags || style.extra != last.extra;
        self.runs.reserve_exact(1 + usize::from(separate_tail));
        let run = |end, cell: &Cell| Run {
            end,
            attributes: Attributes {
                fg: cell.fg,
                bg: cell.bg,
                extra: cell.extra.clone(),
            },
            flags: cell.flags,
        };
        if separate_tail {
            self.runs.push(run(cols, style));
        }
        self.len = cols + usize::from(tail.is_some());
        self.runs.push(run(self.len, last));
        self.wide_spacers = wide_count > 0;
        self.ascii = !self.wide_spacers && self.text.is_ascii();
        self.plain =
            self.ascii && style.extra.as_ref().is_none_or(|extra| extra.zerowidth.is_empty());
        self.reclaim();
    }

    pub(super) fn iter(&self) -> TextIter<'_> {
        TextIter {
            text: &self.text,
            ascii: self.ascii,
            wide_spacers: self.wide_spacers,
            front_byte: 0,
            back_byte: self.text.len(),
            runs: &self.runs,
            columns: 0..self.len,
            front_run: 0,
            back_run: self.runs.len() - 1,
        }
    }
    fn plan_runs<const FLAGS_MASK: u16>(source: &[Cell], budget: usize) -> Option<RunLayout> {
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
            let end = start + attribute_run_len::<FLAGS_MASK>(&source[start..]);
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
        // If the recycled row already held wide spacers, try coalesced wide runs first
        // to avoid a wasted normal pass on steady CJK/Unicode streams; otherwise try
        // the normal path first so ASCII/ANSI rows pay zero wide-char pre-scan.
        if reuse.as_ref().is_some_and(|row| row.wide_spacers) && can_use_wide_spacers(source) {
            return Self::encode_inner(source, reuse, budget, true).ok();
        }
        match Self::encode_inner(source, reuse, budget, false) {
            Ok(stored) => Some(stored),
            Err(retry_reuse) => can_use_wide_spacers(source)
                .then(|| Self::encode_inner(source, retry_reuse, budget, true).ok())
                .flatten(),
        }
    }

    fn encode_inner(
        source: &[Cell],
        reuse: Option<Self>,
        budget: usize,
        wide_spacers: bool,
    ) -> Result<Self, Option<Self>> {
        // Recycled text rows already have run storage. Build it in one scan; without reusable
        // storage, reject expensive candidates before allocating or cloning their attributes.
        let layout = if reuse.is_none() {
            let planned = if wide_spacers {
                Self::plan_runs::<WIDE_PAIR_FLAGS_MASK>(source, budget)
            } else {
                Self::plan_runs::<{ u16::MAX }>(source, budget)
            };
            match planned {
                Some(layout) => Some(layout),
                None => return Err(None),
            }
        } else {
            None
        };
        let mut stored = reuse.unwrap_or_default();
        stored.text.clear();
        stored.runs.clear();
        if let Some(layout) = &layout {
            stored.runs.reserve_exact(layout.count);
        }
        let mut plain = !wide_spacers;
        let mut start = 0;
        while start < source.len() {
            let cell = &source[start];
            if stored.runs.len() == MAX_TEXT_RUNS
                || source.len()
                    + (stored.runs.len() + 1) * std::mem::size_of::<Run>()
                    + std::mem::size_of::<PackedHistory>()
                    >= budget
            {
                return Err(Some(stored));
            }
            let end = if let Some(layout) = &layout {
                layout.ends[stored.runs.len()]
            } else if wide_spacers {
                start + attribute_run_len::<WIDE_PAIR_FLAGS_MASK>(&source[start..])
            } else {
                start + attribute_run_len::<{ u16::MAX }>(&source[start..])
            };
            plain &= !cell.flags.intersects(
                Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
            ) && cell
                .extra
                .as_ref()
                .is_none_or(|extra| extra.zerowidth.is_empty());
            let run_flags = if wide_spacers {
                cell.flags - (Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER)
            } else {
                cell.flags
            };
            stored.runs.push(Run {
                end,
                attributes: Attributes {
                    fg: cell.fg,
                    bg: cell.bg,
                    extra: cell.extra.clone(),
                },
                flags: run_flags,
            });
            start = end;
        }
        stored.len = source.len();
        // Copy the ASCII prefix directly as bytes, and switch to UTF-8 encoding
        // in-place on the first non-ASCII or wide-spacer cell without re-scanning.
        let mut nonzero = true;
        let mut bytes = stored.text.into_bytes();
        bytes.reserve_exact(source.len());
        let mut utf8_start = None;
        for (index, cell) in source.iter().enumerate() {
            if !cell.c.is_ascii()
                || (wide_spacers && cell.flags.contains(Flags::WIDE_CHAR_SPACER))
            {
                utf8_start = Some(index);
                break;
            }
            nonzero &= cell.c != '\0';
            bytes.push(cell.c as u8);
        }
        // SAFETY: Every byte pushed into `bytes` was checked with `cell.c.is_ascii()`.
        stored.text = unsafe { String::from_utf8_unchecked(bytes) };
        let ascii = utf8_start.is_none();
        if let Some(first_utf8) = utf8_start {
            for cell in &source[first_utf8..] {
                if wide_spacers && cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    stored.text.push('\0');
                } else {
                    stored.text.push(cell.c);
                }
            }
        }
        stored.ascii = ascii;
        stored.plain = plain && ascii && nonzero;
        stored.wide_spacers = wide_spacers;
        stored.reclaim();
        if stored.bytes() + std::mem::size_of::<PackedHistory>() - std::mem::size_of::<Self>()
            < budget
        {
            Ok(stored)
        } else {
            Err(Some(stored))
        }
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
                let wide_spacers = self.wide_spacers;
                cells.extend(self.text[from..to].char_indices().map(|(rel, c)| {
                    let (c, flags) = decode_wide_char_flags(
                        &self.text,
                        wide_spacers,
                        from + rel + c.len_utf8(),
                        c,
                        run.flags,
                    );
                    CellRef {
                        c,
                        fg: run.attributes.fg,
                        bg: run.attributes.bg,
                        flags,
                        extra: run.attributes.extra.as_ref(),
                    }
                    .into_owned()
                }));
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
        let (c, byte_after) = if self.ascii {
            (char::from(self.text.as_bytes()[index]), 0)
        } else {
            let byte = self.byte_at(index, cursor);
            let c = self.text[byte..].chars().next().unwrap();
            (c, byte + c.len_utf8())
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
        let (c, flags) =
            decode_wide_char_flags(&self.text, self.wide_spacers, byte_after, c, run.flags);
        CellRef {
            c,
            fg: run.attributes.fg,
            bg: run.attributes.bg,
            flags,
            extra: run.attributes.extra.as_ref(),
        }
    }

    pub(super) fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        let byte = self.byte_at(len, &mut HistoryCursor::default());
        // Preserve WIDE_CHAR on the final surviving column if truncation splits a wide pair.
        let split_wide = self.wide_spacers && self.text.as_bytes().get(byte) == Some(&0);
        self.text.truncate(byte);
        self.runs
            .truncate(self.runs.partition_point(|run| run.end < len) + 1);
        self.runs.last_mut().unwrap().end = len;
        self.len = len;
        if split_wide {
            let run_start = self.runs.iter().rev().nth(1).map_or(0, |run| run.end);
            if run_start == len - 1 {
                self.runs.last_mut().unwrap().flags.insert(Flags::WIDE_CHAR);
            } else {
                let mut wide_run = self.runs.last().unwrap().clone();
                self.runs.last_mut().unwrap().end = len - 1;
                wide_run.end = len;
                wide_run.flags.insert(Flags::WIDE_CHAR);
                self.runs.push(wide_run);
            }
            self.reclaim();
            return;
        }
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
        debug_assert!(!self.wide_spacers || self.text.as_bytes().last() != Some(&0));
        let c = self.text.chars().next_back().unwrap();
        self.text.reserve_exact((len - self.len) * c.len_utf8());
        self.text.extend(std::iter::repeat_n(c, len - self.len));
        self.runs.last_mut().unwrap().end = len;
        self.len = len;
    }

    pub(super) fn push(&mut self, cell: Cell) -> bool {
        if self.wide_spacers
            && (cell.c == '\0'
                || cell
                    .flags
                    .intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER))
        {
            return false;
        }
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
        true
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
    wide_spacers: bool,
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
        let run = &self.runs[self.front_run];
        let (c, flags) = decode_wide_char_flags(
            self.text,
            self.wide_spacers,
            self.front_byte,
            c,
            run.flags,
        );
        Some(CellRef {
            c,
            fg: run.attributes.fg,
            bg: run.attributes.bg,
            flags,
            extra: run.attributes.extra.as_ref(),
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.columns.size_hint()
    }
    fn fold<B, F>(self, mut value: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let mut start = self.columns.start;
        let mut byte_pos = self.front_byte;
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
            } else if !self.wide_spacers {
                value = chars
                    .by_ref()
                    .take(end - start)
                    .fold(value, |value, c| f(value, run.cell(c)));
            } else {
                for c in chars.by_ref().take(end - start) {
                    byte_pos += c.len_utf8();
                    let (c, flags) =
                        decode_wide_char_flags(self.text, true, byte_pos, c, run.flags);
                    value = f(
                        value,
                        CellRef {
                            c,
                            fg: run.attributes.fg,
                            bg: run.attributes.bg,
                            flags,
                            extra: run.attributes.extra.as_ref(),
                        },
                    );
                }
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
        let (c, byte_after) = if self.ascii {
            (char::from(self.text.as_bytes()[column]), 0)
        } else {
            let (offset, c) = self.text[..self.back_byte]
                .char_indices()
                .rev()
                .nth(n)
                .unwrap();
            self.back_byte = offset;
            (c, offset + c.len_utf8())
        };
        while self.back_run > 0 && column < self.runs[self.back_run - 1].end {
            self.back_run -= 1;
        }
        let run = &self.runs[self.back_run];
        let (c, flags) =
            decode_wide_char_flags(self.text, self.wide_spacers, byte_after, c, run.flags);
        Some(CellRef {
            c,
            fg: run.attributes.fg,
            bg: run.attributes.bg,
            flags,
            extra: run.attributes.extra.as_ref(),
        })
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
