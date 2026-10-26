//! Terminal rows with dense writable cells and lossless repeated-tail history storage.
// OxideTerm modification: read-only access never materializes compact history.

use std::cmp::{max, min};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::history::CellIter;
use crate::grid::{CellView, GridCell, HistoryCursor, HistoryStorage};
use crate::index::Column;
use crate::term::cell::ResetDiscriminant;

#[derive(Debug)]
pub struct Row<T: GridCell> {
    identity: usize,
    // In compact rows the final stored cell represents every remaining logical column.
    inner: Vec<T>,
    packed: Option<T::History>,
    packing_rejected: bool,
    len: usize,
    /// Upper bound of cells modified since reset, independent of their physical representation.
    pub(crate) occ: usize,
}

fn next_row_identity() -> usize {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl<T: GridCell> Default for Row<T> {
    fn default() -> Self {
        Self {
            identity: next_row_identity(),
            inner: Vec::new(),
            packed: None,
            packing_rejected: false,
            len: 0,
            occ: 0,
        }
    }
}

impl<T: GridCell> Clone for Row<T> {
    fn clone(&self) -> Self {
        Self {
            identity: next_row_identity(),
            inner: self.inner.clone(),
            packed: self.packed.clone(),
            packing_rejected: self.packing_rejected,
            len: self.len,
            occ: self.occ,
        }
    }
}

impl<T: GridCell> PartialEq for Row<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

impl<T: GridCell + Default> Row<T> {
    pub fn new(columns: usize) -> Self {
        assert!(columns > 0);
        let inner = std::iter::repeat_with(T::default).take(columns).collect();
        Self {
            identity: next_row_identity(),
            inner,
            packed: None,
            packing_rejected: false,
            len: columns,
            occ: 0,
        }
    }

    /// Unused ring slots need a logical blank row, not a full cell allocation.
    pub(super) fn spare(columns: usize) -> Self {
        assert!(columns > 0);
        Self {
            identity: next_row_identity(),
            inner: vec![T::default()],
            packed: None,
            packing_rejected: false,
            len: columns,
            occ: 0,
        }
    }
}

#[allow(clippy::len_without_is_empty)]
impl<T: GridCell> Row<T> {
    #[inline]
    pub fn identity(&self) -> usize {
        self.identity
    }
    #[inline]
    pub fn occupied_len(&self) -> usize {
        self.occ
    }
    #[inline]
    pub fn from_vec(inner: Vec<T>, occ: usize) -> Self {
        Self {
            identity: next_row_identity(),
            len: inner.len(),
            inner,
            packed: None,
            packing_rejected: false,
            occ,
        }
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }
    #[inline]
    pub fn last(&self) -> Option<T::View<'_>> {
        self.len
            .checked_sub(1)
            .map(|column| self.cell(Column(column)))
    }
    #[inline]
    pub fn iter(&self) -> RowIter<'_, T> {
        let stored = if let Some(packed) = &self.packed {
            StoredIter::Packed(packed.iter_cells())
        } else {
            StoredIter::Dense(CellIter(self.inner.iter()))
        };
        let tail_len = self.len - stored.len();
        RowIter {
            stored,
            tail: if tail_len > 0 { self.last() } else { None },
            tail_len,
        }
    }
    /// Exact ASCII columns when available; the last byte repeats through the logical tail.
    pub fn ascii_prefix(&self) -> Option<&str> {
        self.packed.as_ref()?.ascii_prefix()
    }
    pub fn is_clear(&self) -> bool
    where
        T: GridCell,
    {
        // An implicit tail repeats the final stored cell, so inspecting it again adds no data.
        match &self.packed {
            Some(packed) => packed.iter_cells().all(|cell| cell.is_empty()),
            None => self.inner.iter().all(GridCell::is_empty),
        }
    }
}

impl<T: GridCell> Row<T> {
    /// Mutation may cross the visible/history boundary (for example wide-cell repair).
    #[inline]
    pub(super) fn make_dense(&mut self) {
        if self.packing_rejected {
            self.packing_rejected = false;
        }
        if self.packed.is_some() || self.inner.len() < self.len {
            self.expand();
        }
    }

    // History expansion must stay outside the hot, already-dense screen-write path.
    #[cold]
    #[inline(never)]
    fn expand(&mut self) {
        if self.packed.is_some() {
            let packed = self.packed.take().unwrap();
            self.inner = packed.decode(0..packed.len());
        }
        if self.inner.len() < self.len {
            self.inner
                .resize(self.len, self.inner.last().unwrap().clone());
        }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.make_dense();
        self.occ = self.len;
        &mut self.inner
    }

    /// Bounded writes preserve the dirty-cell bound used by reset and compaction.
    #[inline]
    pub fn cells_mut(&mut self, range: std::ops::Range<usize>) -> &mut [T] {
        self.make_dense();
        self.occ = max(self.occ, range.end);
        &mut self.inner[range]
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.make_dense();
        self.occ = self.len;
        self.inner.last_mut()
    }

    pub fn append(&mut self, vec: &mut Vec<T>)
    where
        T: GridCell,
    {
        self.make_dense();
        self.occ += vec.len();
        self.inner.append(vec);
        self.len = self.inner.len();
    }

    pub fn append_front(&mut self, mut vec: Vec<T>) {
        self.make_dense();
        self.occ += vec.len();
        vec.append(&mut self.inner);
        self.inner = vec;
        self.len = self.inner.len();
    }

    pub fn front_split_off(&mut self, at: usize) -> Vec<T> {
        self.make_dense();
        self.occ = self.occ.saturating_sub(at);
        let mut split = self.inner.split_off(at);
        std::mem::swap(&mut split, &mut self.inner);
        self.len = self.inner.len();
        split
    }

    /// Returns the old dense allocation for a newly exposed screen row to reuse.
    pub(super) fn compact(&mut self) -> Option<(Vec<T>, usize)>
    where
        T: PartialEq,
    {
        self.compact_reusing(None)
    }

    pub(super) fn compact_reusing(&mut self, reuse: Option<T::History>) -> Option<(Vec<T>, usize)> {
        // Reflow can leave only a tiny prefix; shared-attribute headers then cost more than cells.
        if self
            .packed
            .as_ref()
            .is_some_and(|packed| packed.bytes() >= packed.len() * std::mem::size_of::<T>())
        {
            let packed = self.packed.take().unwrap();
            self.inner = packed.decode(0..packed.len());
        }
        if self.packed.is_some() || self.inner.len() < self.len || self.len == 0 {
            return None;
        }
        let tail = self.inner.last().unwrap();
        // All cells after occ are equal, but written spaces before it may have distinct styles.
        let end = self.occ.min(self.len - 1);
        let keep = self.inner[..end]
            .iter()
            .rposition(|cell| cell != tail)
            .map_or(1, |i| i + 2);
        if !self.packing_rejected {
            if let Some(packed) = T::History::encode(&self.inner[..keep], reuse) {
                self.packed = Some(packed);
                return Some((std::mem::take(&mut self.inner), self.occ));
            }
            self.packing_rejected = true;
        }
        // Avoid allocation/copy churn for rows whose tail saves less than a quarter of the row.
        if self.len - keep < (self.len / 4).max(4) {
            return None;
        }
        let prefix = self.inner[..keep].to_vec();
        Some((std::mem::replace(&mut self.inner, prefix), self.occ))
    }

    pub fn shrink(&mut self, columns: usize) -> Option<Vec<T>>
    where
        T: GridCell,
    {
        if self.len <= columns {
            return None;
        }
        // Removed columns may contain all the attributes that made packing unprofitable.
        self.packing_rejected = false;
        if let Some(packed) = &mut self.packed {
            let stored = packed.len();
            let end = if packed.cell(stored - 1).is_empty() {
                stored
            } else {
                self.len
            };
            let tail = packed.cell(stored - 1);
            let mut removed: Vec<T> = if columns >= stored - 1 && tail.is_empty() {
                Vec::new()
            } else {
                let mut removed = packed.decode(columns.min(stored)..stored);
                if end > stored {
                    removed.resize(end - columns, tail.into_owned());
                }
                removed
            };
            let kept = removed
                .iter()
                .rposition(|cell| !cell.is_empty())
                .map_or(0, |i| i + 1);
            removed.truncate(kept);
            packed.truncate(columns);
            self.len = columns;
            self.occ = min(self.occ, columns);
            return (!removed.is_empty()).then_some(removed);
        }
        let old_len = self.len;
        let compact = self.inner.len() < old_len;
        let mut new_row = if compact && columns >= self.inner.len() {
            let tail = self.inner.last().unwrap();
            if tail.is_empty() {
                Vec::new()
            } else {
                vec![tail.clone(); old_len - columns]
            }
        } else {
            let mut removed = self.inner.split_off(columns);
            // Default implicit columns disappear without ever becoming allocated cells.
            // Nonempty tails still participate in upstream's reflow semantics in full.
            if compact
                && let Some(tail) = removed.last()
                && !tail.is_empty()
            {
                removed.resize(old_len - columns, tail.clone());
            }
            removed
        };
        let index = new_row
            .iter()
            .rposition(|c| !c.is_empty())
            .map_or(0, |i| i + 1);
        new_row.truncate(index);
        self.len = columns;
        self.occ = min(self.occ, columns);
        if new_row.is_empty() {
            None
        } else {
            Some(new_row)
        }
    }
}

impl<T: GridCell + Default> Row<T> {
    pub fn grow(&mut self, columns: usize) {
        if self.len >= columns {
            return;
        }
        if let Some(packed) = &mut self.packed {
            if packed.cell(packed.len() - 1).into_owned() == T::default() {
                self.len = columns;
                return;
            }
            packed.extend_tail(self.len);
            if packed.push(T::default()) {
                self.len = columns;
                return;
            }
        }
        // New columns are default cells, not necessarily the existing tail's background.
        if self.inner.len() == self.len || self.inner.last() != Some(&T::default()) {
            self.make_dense();
            self.inner.resize_with(columns, T::default);
        }
        self.len = columns;
    }
}

impl<T: GridCell> Row<T> {
    pub fn reset<D>(&mut self, template: &T)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // An evicted row may have donated its packed allocation to the new history row.
        // Its old content is discarded, so initialize directly from the reset template.
        if self.inner.is_empty() && self.packed.is_none() && self.len != 0 {
            let mut blank = template.clone();
            blank.reset(template);
            self.inner.resize(self.len, blank);
            self.occ = 0;
        }
        self.make_dense();
        self.identity = next_row_identity();
        if self.inner[self.len - 1].discriminant() != template.discriminant() {
            self.occ = self.len;
        }
        for item in &mut self.inner[..self.occ] {
            item.reset(template);
        }
        self.occ = 0;
    }

    pub(super) fn recycle<D>(&mut self, buffer: Vec<T>, occ: usize, template: &T)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        self.len = buffer.len();
        self.packed = None;
        self.inner = buffer;
        self.occ = occ;
        self.reset(template);
    }

    pub(super) fn take_history(&mut self) -> Option<T::History> {
        self.packed.take()
    }
}

impl<T: GridCell> Row<T> {
    #[inline(always)]
    pub(super) fn cell_with_cursor(
        &self,
        index: Column,
        cursor: &mut HistoryCursor,
    ) -> T::View<'_> {
        assert!(index.0 < self.len);
        if let Some(packed) = &self.packed {
            packed.cell_with_cursor(index.0.min(packed.len() - 1), cursor)
        } else {
            self.inner[index.0.min(self.inner.len() - 1)].view()
        }
    }
    #[inline]
    pub fn cell(&self, index: Column) -> T::View<'_> {
        assert!(index.0 < self.len);
        if let Some(packed) = &self.packed {
            packed.cell(index.0.min(packed.len() - 1))
        } else {
            self.inner[index.0.min(self.inner.len() - 1)].view()
        }
    }
    #[inline]
    pub fn cell_mut(&mut self, index: Column) -> &mut T {
        self.make_dense();
        self.occ = max(self.occ, index.0 + 1);
        &mut self.inner[index.0]
    }
}

pub struct RowIter<'a, T: GridCell> {
    stored: StoredIter<'a, T>,
    tail: Option<T::View<'a>>,
    tail_len: usize,
}

enum StoredIter<'a, T: GridCell> {
    Dense(CellIter<'a, T>),
    Packed(<T::History as HistoryStorage<T>>::Iter<'a>),
}

impl<'a, T: GridCell> Iterator for StoredIter<'a, T> {
    type Item = T::View<'a>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Dense(iter) => iter.next(),
            Self::Packed(iter) => iter.next(),
        }
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self {
            Self::Dense(iter) => iter.nth(n),
            Self::Packed(iter) => iter.nth(n),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Dense(iter) => iter.size_hint(),
            Self::Packed(iter) => iter.size_hint(),
        }
    }
}
impl<T: GridCell> DoubleEndedIterator for StoredIter<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Dense(iter) => iter.next_back(),
            Self::Packed(iter) => iter.next_back(),
        }
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        match self {
            Self::Dense(iter) => iter.nth_back(n),
            Self::Packed(iter) => iter.nth_back(n),
        }
    }
}
impl<T: GridCell> ExactSizeIterator for StoredIter<'_, T> {}

impl<'a, T: GridCell> Iterator for RowIter<'a, T> {
    type Item = T::View<'a>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(cell) = self.stored.next() {
            return Some(cell);
        }
        self.tail_len = self.tail_len.checked_sub(1)?;
        self.tail
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let stored = self.stored.len();
        let cell = self.stored.nth(n);
        if n < stored {
            return cell;
        }
        let skip = n - stored;
        if skip < self.tail_len {
            self.tail_len -= skip + 1;
            self.tail
        } else {
            self.tail_len = 0;
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.stored.len() + self.tail_len;
        (len, Some(len))
    }
    fn fold<B, F>(self, value: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let value = match self.stored {
            StoredIter::Dense(iter) => iter.fold(value, &mut f),
            StoredIter::Packed(iter) => iter.fold(value, &mut f),
        };
        match self.tail {
            Some(tail) => std::iter::repeat_n(tail, self.tail_len).fold(value, f),
            None => value,
        }
    }
}
impl<T: GridCell> DoubleEndedIterator for RowIter<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.tail_len != 0 {
            self.tail_len -= 1;
            self.tail
        } else {
            self.stored.next_back()
        }
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n < self.tail_len {
            self.tail_len -= n + 1;
            self.tail
        } else {
            let skip = n - self.tail_len;
            self.tail_len = 0;
            self.stored.nth_back(skip)
        }
    }
}
impl<T: GridCell> ExactSizeIterator for RowIter<'_, T> {}

impl<'a, T: GridCell> IntoIterator for &'a Row<T> {
    type IntoIter = RowIter<'a, T>;
    type Item = T::View<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T: GridCell> IntoIterator for &'a mut Row<T> {
    type IntoIter = std::slice::IterMut<'a, T>;
    type Item = &'a mut T;
    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
    }
}

// Reference recordings retain the existing logical-cell format, irrespective of storage shape.
#[cfg(feature = "serde")]
impl<T: GridCell + serde::Serialize> serde::Serialize for Row<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        struct Cells<'a, T: GridCell>(&'a Row<T>);
        impl<T: GridCell + serde::Serialize> serde::Serialize for Cells<'_, T> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_seq(self.0.iter().map(CellView::into_owned))
            }
        }
        let mut state = serializer.serialize_struct("Row", 2)?;
        state.serialize_field("inner", &Cells(self))?;
        state.serialize_field("occ", &self.occ)?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: GridCell + serde::Deserialize<'de>> serde::Deserialize<'de> for Row<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Cells<T> {
            inner: Vec<T>,
            occ: usize,
        }
        let row = Cells::deserialize(d)?;
        Ok(Self::from_vec(row.inner, row.occ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Dimensions, Grid};
    use crate::index::Line;
    use crate::term::cell::{Cell, Flags, Hyperlink};
    use crate::vte::ansi::{Color, NamedColor};

    #[test]
    fn compact_reads_and_mutation_preserve_styles_width_and_identity() {
        let mut tail = Cell {
            bg: Color::Named(NamedColor::Red),
            ..Cell::default()
        };
        tail.set_hyperlink(Some(Hyperlink::new(
            Some("tail"),
            "https://example.test".into(),
        )));
        let mut row = Row::from_vec(vec![tail.clone(); 120], 120);
        row.cell_mut(Column(0)).c = '界';
        row.cell_mut(Column(0)).flags = Flags::WIDE_CHAR;
        row.cell_mut(Column(1)).flags = Flags::WIDE_CHAR_SPACER;
        row.cell_mut(Column(2)).c = 'e';
        row.cell_mut(Column(2)).push_zerowidth('\u{301}');
        let identity = row.identity();
        let buffer = row.compact().unwrap().0;
        assert_eq!(row.inner.len(), 4);
        assert_eq!(row.inner.capacity(), 4);
        assert_eq!(buffer.len(), 120);
        assert_eq!(row.len(), 120);
        assert_eq!(row.cell(Column(0)).c, '界');
        assert_eq!(row.cell(Column(1)).flags, Flags::WIDE_CHAR_SPACER);
        assert_eq!(row.cell(Column(2)).zerowidth(), Some(&['\u{301}'][..]));
        assert!(row.iter().skip(3).all(|cell| cell == &tail));
        assert!(row.iter().rev().take(117).all(|cell| cell == &tail));
        assert_eq!(row.inner.len(), 4, "reading must not expand history");
        row.cell_mut(Column(118)).flags.insert(Flags::UNDERLINE);
        assert_eq!(row.cell(Column(117)), tail);
        assert!(row.cell(Column(118)).flags.contains(Flags::UNDERLINE));
        assert_eq!(row.cell(Column(119)), tail);
        assert_eq!(row.identity(), identity);
    }

    #[test]
    fn resize_keeps_old_colored_tail_but_adds_default_columns() {
        for colored in [false, true] {
            let tail = Cell {
                bg: if colored {
                    Color::Named(NamedColor::Blue)
                } else {
                    Color::Named(NamedColor::Background)
                },
                ..Cell::default()
            };
            let mut row = Row::from_vec(vec![tail.clone(); 120], 120);
            row.cell_mut(Column(0)).c = 'x';
            drop(row.compact());
            assert_eq!(row.shrink(80), colored.then(|| vec![tail.clone(); 40]));
            row.grow(240);
            assert_eq!(row.cell(Column(0)).c, 'x');
            assert!(row.iter().skip(1).take(79).all(|cell| cell == &tail));
            assert!(row.iter().skip(80).all(|cell| cell == &Cell::default()));
            drop(row.compact());
            assert_eq!(row.shrink(80), None);
            drop(row.compact());
            assert_eq!(row.inner.len(), 2);
            assert!(row.iter().skip(1).all(|cell| cell == &tail));
        }
    }

    #[test]
    fn scrolling_recycles_dense_screen_allocation_and_bounds_history_memory() {
        let mut grid = Grid::<Cell>::new(3, 400, 20);
        for line in 0..80 {
            grid[Line(2)].cell_mut(Column(0)).c = char::from(b'A' + (line % 26) as u8);
            let leaving_identity = grid[Line(0)].identity();
            let buffer = grid[Line(0)].inner.as_ptr();
            grid.scroll_up::<Color>(&(Line(0)..Line(3)), 1);
            assert_eq!(grid[Line(-1)].identity(), leaving_identity);
            assert_eq!(grid[Line(2)].inner.as_ptr(), buffer);
            assert_eq!(grid[Line(2)].cell(Column(0)), Cell::default());
        }
        assert_eq!(grid.history_size(), 20);
        assert_eq!(grid[Line(-1)].cell(Column(0)).c, 'Z');
        let stored: usize = (-20..0).map(|line| grid[Line(line)].inner.capacity()).sum();
        assert!(
            stored <= 40,
            "short history must not retain full-width allocations: {stored}"
        );
        grid.resize(true, 5, 400);
        assert_eq!(grid[Line(0)].cell(Column(0)).c, 'Y');
        assert_eq!(grid[Line(1)].cell(Column(0)).c, 'Z');
        assert_eq!(grid[Line(0)].inner.len(), 400);
        grid[Line(0)].cell_mut(Column(399)).c = '!';
        assert_eq!(grid[Line(0)].cell(Column(399)).c, '!');
    }

    #[cfg(feature = "serde")]
    #[test]
    fn compact_row_serializes_logical_cells_without_expanding_storage() {
        let mut row = Row::from_vec(vec![1_usize, 0, 0, 0, 0, 0, 0, 0], 1);
        drop(row.compact());
        assert_eq!(
            serde_json::to_string(&row).unwrap(),
            "{\"inner\":[1,0,0,0,0,0,0,0],\"occ\":1}"
        );
        assert_eq!(row.inner.len(), 2);
        let loaded: Row<usize> =
            serde_json::from_str("{\"inner\":[1,0,0,0,0,0,0,0],\"occ\":1}").unwrap();
        assert_eq!(loaded, row);
    }

    #[test]
    fn text_history_cursors_preserve_columns_in_both_directions_and_on_mutation() {
        let mut cells: Vec<Cell> = (0..400)
            .map(|i| Cell {
                c: ['a', 'é', 'λ', 'z'][i % 4],
                fg: if i < 200 {
                    Color::Indexed(2)
                } else {
                    Color::Indexed(3)
                },
                ..Cell::default()
            })
            .collect();
        cells[10].c = '界';
        cells[10].flags = Flags::WIDE_CHAR;
        cells[11].c = ' ';
        cells[11].flags = Flags::WIDE_CHAR_SPACER;
        cells[12].push_zerowidth('\u{301}');
        cells[20].set_hyperlink(Some(Hyperlink::new(
            Some("text"),
            "https://example.test/text".into(),
        )));
        cells[21].set_underline_color(Some(Color::Indexed(9)));
        cells[399].flags = Flags::WRAPLINE;
        let mut row = Row::from_vec(cells, 400);
        drop(row.compact());
        assert!(row.packed.as_ref().unwrap().bytes() < 2000);
        assert_eq!(
            row.iter().map(|cell| cell.c).collect::<String>(),
            format!("aéλzaéλzaé界 {}", "aéλz".repeat(97))
        );
        assert_eq!(
            row.cell(Column(20)).hyperlink().unwrap().uri(),
            "https://example.test/text"
        );
        assert_eq!(
            row.cell(Column(21)).underline_color(),
            Some(Color::Indexed(9))
        );
        let mut grid = Grid::<Cell>::new(2, 400, 0);
        grid[Line(0)] = row.clone();
        grid[Line(1)] = row.clone();
        let mut walk = grid.iter_from(crate::index::Point::new(Line(0), Column(399)));
        assert_eq!(walk.next().unwrap().c, 'a');
        use crate::grid::BidirectionalIterator;
        assert_eq!(walk.prev().unwrap().flags, Flags::WRAPLINE);
        assert_eq!(walk.prev().unwrap().c, 'λ');
        assert_eq!(walk.next().unwrap().c, 'z');
        assert_eq!(walk.next().unwrap().fg, Color::Indexed(2));
        let mut iter = row.iter();
        assert_eq!(iter.next().unwrap().c, 'a');
        assert_eq!(iter.next_back().unwrap().flags, Flags::WRAPLINE);
        assert_eq!(iter.nth(9).unwrap().c, '界');
        assert_eq!(iter.next().unwrap().flags, Flags::WIDE_CHAR_SPACER);
        assert_eq!(iter.next().unwrap().zerowidth(), Some(&['\u{301}'][..]));
        assert_eq!(iter.nth_back(198).unwrap().fg, Color::Indexed(3));
        assert_eq!(iter.next_back().unwrap().fg, Color::Indexed(2));
        for (i, cell) in row.iter().enumerate().skip(13).take(386) {
            assert_eq!(cell.c, ['a', 'é', 'λ', 'z'][i % 4]);
        }
        for (i, cell) in row.iter().enumerate().rev().skip(1).take(386) {
            assert_eq!(cell.c, ['a', 'é', 'λ', 'z'][i % 4]);
        }
        row.cell_mut(Column(200)).c = '!';
        row.cell_mut(Column(12)).push_zerowidth('\u{302}');
        assert_eq!(
            row.cell(Column(12)).zerowidth(),
            Some(&['\u{301}', '\u{302}'][..])
        );
        assert_eq!(
            grid[Line(0)].cell(Column(12)).zerowidth(),
            Some(&['\u{301}'][..])
        );
        assert_eq!(row.cell(Column(199)).c, 'z');
        assert_eq!(row.cell(Column(200)).c, '!');
        assert_eq!(row.cell(Column(201)).c, 'é');
        drop(row.compact());
        let removed = row.shrink(200).unwrap();
        assert_eq!(removed[0].c, '!');
        assert_eq!(removed[199].flags, Flags::WRAPLINE);
        row.grow(400);
        assert_eq!(row.cell(Column(199)).fg, Color::Indexed(2));
        assert!(row.iter().skip(200).all(|cell| cell == Cell::default()));
    }

    #[test]
    fn narrowing_reconsiders_a_previously_high_entropy_row() {
        let cells = (0..400)
            .map(|i| Cell {
                c: char::from(b'a' + (i % 23) as u8),
                fg: if i < 200 {
                    Color::Named(NamedColor::Foreground)
                } else {
                    Color::Indexed((i - 200) as u8)
                },
                ..Cell::default()
            })
            .collect();
        let mut row = Row::from_vec(cells, 400);
        drop(row.compact());
        row.shrink(200);
        drop(row.compact());
        assert_eq!(row.len(), 200);
        let bytes = row.packed.as_ref().map_or(
            row.inner.capacity() * std::mem::size_of::<Cell>(),
            HistoryStorage::bytes,
        );
        assert!(
            bytes < 200 * 12,
            "the retained uniform prefix should be packed: {bytes}"
        );
        for (i, cell) in row.iter().enumerate() {
            assert_eq!(cell.c, char::from(b'a' + (i % 23) as u8));
            assert_eq!(cell.fg, Color::Named(NamedColor::Foreground));
        }
    }
}
