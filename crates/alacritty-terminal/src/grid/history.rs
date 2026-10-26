//! Storage contracts for lossless history reads without materializing writable cells.
use std::fmt::Debug;

use super::GridCell;
use crate::term::cell::Flags;

pub trait CellView<T>: Copy + PartialEq {
    fn flags(&self) -> &Flags;
    fn is_empty(&self) -> bool;
    fn into_owned(self) -> T;
}

impl<T: GridCell> CellView<T> for &T {
    fn flags(&self) -> &Flags {
        T::flags(self)
    }
    fn is_empty(&self) -> bool {
        T::is_empty(self)
    }
    fn into_owned(self) -> T {
        self.clone()
    }
}

/// Position within a history row. Readers own this state; history remains immutable.
#[derive(Default, Clone, Copy, Debug)]
pub struct HistoryCursor {
    pub(crate) column: usize,
    pub(crate) byte: usize,
    pub(crate) run: usize,
}

pub trait HistoryStorage<T: GridCell>: Clone + Debug {
    type Iter<'a>: DoubleEndedIterator<Item = T::View<'a>> + ExactSizeIterator
    where
        Self: 'a,
        T: 'a;
    fn encode(cells: &[T], reuse: Option<Self>) -> Option<Self>;
    fn iter_cells(&self) -> Self::Iter<'_>;
    fn len(&self) -> usize;
    fn bytes(&self) -> usize;
    fn cell(&self, index: usize) -> T::View<'_>;
    fn cell_with_cursor(&self, index: usize, _: &mut HistoryCursor) -> T::View<'_> {
        self.cell(index)
    }
    /// Materialize a stored span for editing or reflow, with one destination allocation.
    fn decode(&self, range: std::ops::Range<usize>) -> Vec<T> {
        let mut cursor = HistoryCursor::default();
        range
            .map(|i| self.cell_with_cursor(i, &mut cursor).into_owned())
            .collect()
    }
    /// One byte per stored column, without NULs, wide cells or combining characters.
    fn ascii_prefix(&self) -> Option<&str> {
        None
    }
    fn truncate(&mut self, len: usize);
    fn extend_tail(&mut self, len: usize);
    fn push(&mut self, value: T) -> bool;
}

pub struct CellIter<'a, T: GridCell>(pub(super) std::slice::Iter<'a, T>);

impl<'a, T: GridCell> Iterator for CellIter<'a, T> {
    type Item = T::View<'a>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(T::view)
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(T::view)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}
impl<T: GridCell> DoubleEndedIterator for CellIter<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(T::view)
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(T::view)
    }
}
impl<T: GridCell> ExactSizeIterator for CellIter<'_, T> {}

// Generic grid users can retain their original cell representation; packing is cell-specific.
impl<T: GridCell> HistoryStorage<T> for Vec<T> {
    type Iter<'a>
        = CellIter<'a, T>
    where
        T: 'a;
    fn iter_cells(&self) -> Self::Iter<'_> {
        CellIter(self.as_slice().iter())
    }
    fn encode(_: &[T], _: Option<Self>) -> Option<Self> {
        None
    }
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn bytes(&self) -> usize {
        self.capacity() * std::mem::size_of::<T>() + std::mem::size_of::<Self>()
    }
    fn cell(&self, index: usize) -> T::View<'_> {
        self[index].view()
    }
    fn truncate(&mut self, len: usize) {
        Vec::truncate(self, len);
    }
    fn extend_tail(&mut self, len: usize) {
        self.resize(len, self.last().unwrap().clone());
    }
    fn push(&mut self, value: T) -> bool {
        Vec::push(self, value);
        true
    }
}
