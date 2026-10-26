//! Lossless text runs, with eight-byte indexed cells for frequently alternating attributes.
use super::*;
use crate::grid::{CellView, HistoryCursor, HistoryStorage};

mod text;
use text::TextHistory;

// Bound table construction even when every terminal cell carries distinct metadata.
const MAX_ROW_ATTRIBUTES: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Attributes {
    fg: Color,
    bg: Color,
    extra: Option<Arc<CellExtra>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
struct PackedCell {
    c: char,
    flags: Flags,
    attributes: u16,
}

const _: () = assert!(std::mem::size_of::<PackedCell>() == 8);

#[derive(Clone, Debug)]
struct IndexedHistory {
    cells: Vec<PackedCell>,
    attributes: Vec<Attributes>,
}

#[derive(Clone, Debug)]
pub struct PackedHistory(History);

#[derive(Clone, Debug)]
enum History {
    Text(TextHistory),
    Indexed(IndexedHistory),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRef<'a> {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
    pub extra: Option<&'a Arc<CellExtra>>,
}

impl<'a> CellRef<'a> {
    #[inline]
    pub fn zerowidth(&self) -> Option<&'a [char]> {
        self.extra.map(|extra| extra.zerowidth.as_slice())
    }
    #[inline]
    pub fn hyperlink(&self) -> Option<Hyperlink> {
        self.extra.and_then(|extra| extra.hyperlink.clone())
    }
    #[inline]
    pub fn underline_color(&self) -> Option<Color> {
        self.extra.and_then(|extra| extra.underline_color)
    }
    #[inline]
    pub fn flags(&self) -> &Flags {
        &self.flags
    }
}

impl PartialEq<Cell> for CellRef<'_> {
    fn eq(&self, cell: &Cell) -> bool {
        *self == cell.view()
    }
}
impl PartialEq<&Cell> for CellRef<'_> {
    fn eq(&self, cell: &&Cell) -> bool {
        *self == cell.view()
    }
}

fn attribute_run_len<const INCLUDE_FLAGS: bool>(source: &[Cell]) -> usize {
    let first = &source[0];
    // Choose the color representation outside the scan. Comparing two Color enums inside every
    // iteration otherwise repeats both discriminant dispatches for ordinary terminal output.
    if first.extra.is_none() {
        match (first.fg, first.bg) {
            (Color::Named(fg), Color::Named(bg)) => return source[1..].iter().position(|cell| {
                !matches!((cell.fg, cell.bg), (Color::Named(f), Color::Named(b)) if f == fg && b == bg)
                    || cell.extra.is_some()
                    || (INCLUDE_FLAGS && cell.flags != first.flags)
            }).map_or(source.len(), |i| i + 1),
            (Color::Indexed(fg), Color::Named(bg)) => return source[1..].iter().position(|cell| {
                !matches!((cell.fg, cell.bg), (Color::Indexed(f), Color::Named(b)) if f == fg && b == bg)
                    || cell.extra.is_some()
                    || (INCLUDE_FLAGS && cell.flags != first.flags)
            }).map_or(source.len(), |i| i + 1),
            _ => {},
        }
    }
    source[1..]
        .iter()
        .position(|cell| {
            cell.fg != first.fg
                || cell.bg != first.bg
                || cell.extra != first.extra
                || (INCLUDE_FLAGS && cell.flags != first.flags)
        })
        .map_or(source.len(), |i| i + 1)
}

impl CellView<Cell> for CellRef<'_> {
    #[inline]
    fn flags(&self) -> &Flags {
        &self.flags
    }
    #[inline]
    fn is_empty(&self) -> bool {
        (self.c == ' ' || self.c == '\t')
            && self.bg == Color::Named(NamedColor::Background)
            && self.fg == Color::Named(NamedColor::Foreground)
            && !self.flags.intersects(
                Flags::INVERSE
                    | Flags::ALL_UNDERLINES
                    | Flags::STRIKEOUT
                    | Flags::WRAPLINE
                    | Flags::WIDE_CHAR_SPACER
                    | Flags::LEADING_WIDE_CHAR_SPACER,
            )
            && self.extra.map(|extra| extra.zerowidth.is_empty()) != Some(false)
    }
    #[inline]
    fn into_owned(self) -> Cell {
        Cell {
            c: self.c,
            fg: self.fg,
            bg: self.bg,
            flags: self.flags,
            extra: self.extra.cloned(),
        }
    }
}

impl HistoryStorage<Cell> for IndexedHistory {
    type Iter<'a> = IndexedIter<'a>;
    fn iter_cells(&self) -> Self::Iter<'_> {
        IndexedIter {
            stored: self,
            range: 0..self.len(),
        }
    }
    fn encode(source: &[Cell], reuse: Option<Self>) -> Option<Self> {
        let budget = std::mem::size_of_val(source) * 3 / 4;
        let payload = source.len() * std::mem::size_of::<PackedCell>();
        if payload + std::mem::size_of::<Attributes>() + std::mem::size_of::<Self>() > budget {
            return None;
        }
        let mut storage = reuse.unwrap_or_else(|| Self {
            cells: Vec::with_capacity(source.len()),
            attributes: Vec::with_capacity(1),
        });
        storage.cells.clear();
        storage.attributes.clear();
        storage.cells.reserve_exact(source.len());
        let Self {
            mut cells,
            mut attributes,
        } = storage;
        let mut start = 0;
        while start < source.len() {
            let first = &source[start];
            let end = start + attribute_run_len::<false>(&source[start..]);
            let matches =
                |a: &Attributes| a.fg == first.fg && a.bg == first.bg && a.extra == first.extra;
            let attribute = if let Some(found) = attributes.iter().position(matches) {
                found as u16
            } else {
                // Bound conversion work on high-entropy streams; original cells remain lossless.
                if attributes.len() == MAX_ROW_ATTRIBUTES {
                    return None;
                }
                let next = attributes.len() as u16;
                attributes.push(Attributes {
                    fg: first.fg,
                    bg: first.bg,
                    extra: first.extra.clone(),
                });
                if payload
                    + attributes.capacity() * std::mem::size_of::<Attributes>()
                    + std::mem::size_of::<Self>()
                    > budget
                {
                    return None;
                }
                next
            };
            // Copy only character and flags through this loop, with no per-cell table lookup
            // or reference-count change. Attribute ownership is per distinct row attribute.
            cells.extend(source[start..end].iter().map(|cell| PackedCell {
                c: cell.c,
                flags: cell.flags,
                attributes: attribute,
            }));
            start = end;
        }
        attributes.shrink_to_fit();
        Some(Self { cells, attributes })
    }
    fn len(&self) -> usize {
        self.cells.len()
    }
    fn bytes(&self) -> usize {
        self.cells.capacity() * std::mem::size_of::<PackedCell>()
            + self.attributes.capacity() * std::mem::size_of::<Attributes>()
            + std::mem::size_of::<Self>()
    }
    #[inline]
    fn cell(&self, index: usize) -> CellRef<'_> {
        let cell = self.cells[index];
        let a = &self.attributes[usize::from(cell.attributes)];
        CellRef {
            c: cell.c,
            flags: cell.flags,
            fg: a.fg,
            bg: a.bg,
            extra: a.extra.as_ref(),
        }
    }
    fn truncate(&mut self, len: usize) {
        self.cells.truncate(len);
        if let Some(tail) = self.cells.last() {
            let keep = self
                .cells
                .iter()
                .rposition(|cell| cell != tail)
                .map_or(1, |i| i + 2);
            if self.cells.len() - keep >= (self.cells.len() / 4).max(4) {
                self.cells.truncate(keep);
            }
        }
        if self.cells.len() <= self.cells.capacity() * 3 / 4 {
            self.cells.shrink_to_fit();
        }
    }
    fn extend_tail(&mut self, len: usize) {
        self.cells.resize(len, *self.cells.last().unwrap());
    }
    fn push(&mut self, value: Cell) -> bool {
        let key = Attributes {
            fg: value.fg,
            bg: value.bg,
            extra: value.extra,
        };
        let index = match self.attributes.iter().position(|entry| entry == &key) {
            Some(index) => index,
            None => {
                if self.attributes.len() > usize::from(u16::MAX) {
                    return false;
                }
                self.attributes.reserve_exact(1);
                self.attributes.push(key);
                self.attributes.len() - 1
            }
        };
        self.cells.reserve_exact(1);
        self.cells.push(PackedCell {
            c: value.c,
            flags: value.flags,
            attributes: index as u16,
        });
        true
    }
}

impl HistoryStorage<Cell> for PackedHistory {
    type Iter<'a> = HistoryIter<'a>;
    fn iter_cells(&self) -> Self::Iter<'_> {
        HistoryIter(match &self.0 {
            History::Text(text) => HistoryIterInner::Text(text.iter()),
            History::Indexed(cells) => HistoryIterInner::Indexed(cells.iter_cells()),
        })
    }
    fn encode(source: &[Cell], reuse: Option<Self>) -> Option<Self> {
        let (text_reuse, indexed_reuse) = match reuse {
            Some(Self(History::Text(text))) => (Some(text), None),
            Some(Self(History::Indexed(cells))) => (None, Some(cells)),
            None => (None, None),
        };
        if let Some(text) = TextHistory::encode(source, text_reuse) {
            return Some(Self(History::Text(text)));
        }
        IndexedHistory::encode(source, indexed_reuse).map(|cells| Self(History::Indexed(cells)))
    }
    fn len(&self) -> usize {
        match &self.0 {
            History::Text(text) => text.len(),
            History::Indexed(cells) => cells.len(),
        }
    }
    fn bytes(&self) -> usize {
        (match &self.0 {
            History::Text(text) => text.bytes() - std::mem::size_of::<TextHistory>(),
            History::Indexed(cells) => cells.bytes() - std::mem::size_of::<IndexedHistory>(),
        }) + std::mem::size_of::<Self>()
    }
    #[inline]
    fn cell(&self, index: usize) -> CellRef<'_> {
        self.cell_with_cursor(index, &mut HistoryCursor::default())
    }
    #[inline(always)]
    fn cell_with_cursor(&self, index: usize, cursor: &mut HistoryCursor) -> CellRef<'_> {
        match &self.0 {
            History::Text(text) => text.cell(index, cursor),
            History::Indexed(cells) => cells.cell(index),
        }
    }
    fn ascii_prefix(&self) -> Option<&str> {
        match &self.0 {
            History::Text(text) => text.ascii_prefix(),
            _ => None,
        }
    }
    fn decode(&self, range: std::ops::Range<usize>) -> Vec<Cell> {
        match &self.0 {
            History::Text(text) => text.decode(range),
            History::Indexed(cells) => cells.decode(range),
        }
    }
    fn truncate(&mut self, len: usize) {
        match &mut self.0 {
            History::Text(text) => text.truncate(len),
            History::Indexed(cells) => cells.truncate(len),
        }
    }
    fn extend_tail(&mut self, len: usize) {
        match &mut self.0 {
            History::Text(text) => text.extend_tail(len),
            History::Indexed(cells) => cells.extend_tail(len),
        }
    }
    fn push(&mut self, value: Cell) -> bool {
        match &mut self.0 {
            History::Text(text) => {
                text.push(value);
                true
            }
            History::Indexed(cells) => cells.push(value),
        }
    }
}

struct IndexedIter<'a> {
    stored: &'a IndexedHistory,
    range: std::ops::Range<usize>,
}
impl<'a> Iterator for IndexedIter<'a> {
    type Item = CellRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.range.next().map(|i| self.stored.cell(i))
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.range.nth(n).map(|i| self.stored.cell(i))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
    fn fold<B, F>(self, value: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        self.range
            .fold(value, |value, i| f(value, self.stored.cell(i)))
    }
}
impl DoubleEndedIterator for IndexedIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.range.next_back().map(|i| self.stored.cell(i))
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.range.nth_back(n).map(|i| self.stored.cell(i))
    }
}
impl ExactSizeIterator for IndexedIter<'_> {}

pub struct HistoryIter<'a>(HistoryIterInner<'a>);
enum HistoryIterInner<'a> {
    Text(text::TextIter<'a>),
    Indexed(IndexedIter<'a>),
}
impl<'a> Iterator for HistoryIter<'a> {
    type Item = CellRef<'a>;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            HistoryIterInner::Text(iter) => iter.next(),
            HistoryIterInner::Indexed(iter) => iter.next(),
        }
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match &mut self.0 {
            HistoryIterInner::Text(iter) => iter.nth(n),
            HistoryIterInner::Indexed(iter) => iter.nth(n),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            HistoryIterInner::Text(iter) => iter.size_hint(),
            HistoryIterInner::Indexed(iter) => iter.size_hint(),
        }
    }
    fn fold<B, F>(self, value: B, f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        match self.0 {
            HistoryIterInner::Text(iter) => iter.fold(value, f),
            HistoryIterInner::Indexed(iter) => iter.fold(value, f),
        }
    }
}
impl DoubleEndedIterator for HistoryIter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            HistoryIterInner::Text(iter) => iter.next_back(),
            HistoryIterInner::Indexed(iter) => iter.next_back(),
        }
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        match &mut self.0 {
            HistoryIterInner::Text(iter) => iter.nth_back(n),
            HistoryIterInner::Indexed(iter) => iter.nth_back(n),
        }
    }
}
impl ExactSizeIterator for HistoryIter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vte::ansi::Rgb;

    #[test]
    fn packed_full_line_preserves_character_flags_and_sparse_metadata() {
        let mut source: Vec<Cell> = (0..400)
            .map(|i| Cell {
                c: char::from(b'a' + (i % 23) as u8),
                flags: Flags::from_bits_retain(i as u16),
                fg: if i < 200 {
                    Color::Named(NamedColor::Red)
                } else {
                    Color::Spec(Rgb { r: 1, g: 2, b: 3 })
                },
                ..Cell::default()
            })
            .collect();
        for cell in &mut source[20..40] {
            cell.set_hyperlink(Some(Hyperlink::new(
                Some("one"),
                "https://example.test/a".into(),
            )));
        }
        source[45].push_zerowidth('\u{301}');
        source[46].set_underline_color(Some(Color::Indexed(9)));
        let mut packed = PackedHistory::encode(&source, None).unwrap();
        assert!(packed.bytes() < 400 * 12);
        for i in 0..400 {
            let cell = packed.cell(i);
            assert_eq!(cell.c, char::from(b'a' + (i % 23) as u8));
            assert_eq!(cell.flags.bits(), i as u16);
            assert_eq!(
                cell.fg,
                if i < 200 {
                    Color::Named(NamedColor::Red)
                } else {
                    Color::Spec(Rgb { r: 1, g: 2, b: 3 })
                }
            );
        }
        assert_eq!(
            packed.cell(20).hyperlink().unwrap().uri(),
            "https://example.test/a"
        );
        assert!(packed.cell(40).hyperlink().is_none());
        assert_eq!(packed.cell(45).zerowidth(), Some(&['\u{301}'][..]));
        assert_eq!(packed.cell(46).underline_color(), Some(Color::Indexed(9)));
        let mut edited = packed.cell(45).into_owned();
        edited.push_zerowidth('\u{302}');
        assert_eq!(packed.cell(45).zerowidth(), Some(&['\u{301}'][..]));
        packed.truncate(200);
        assert!(
            packed.bytes() < 200 * 12,
            "narrowing releases obsolete cell capacity"
        );
        assert_eq!(packed.cell(199).c, char::from(b'a' + (199 % 23) as u8));
    }

    #[test]
    fn reused_history_switches_between_text_and_eight_byte_cells() {
        let mut reuse = None;
        for kind in [0, 1, 2, 0] {
            let source: Vec<Cell> = (0..400)
                .map(|i| Cell {
                    c: if kind == 2 {
                        ['é', 'λ'][i % 2]
                    } else {
                        char::from(b'a' + (i % 23) as u8)
                    },
                    fg: Color::Indexed(if kind == 1 { (i % 2) as u8 } else { 7 }),
                    ..Cell::default()
                })
                .collect();
            let packed = PackedHistory::encode(&source, reuse).unwrap();
            assert!(packed.bytes() < 400 * 9);
            assert_eq!(matches!(packed.0, History::Indexed(_)), kind == 1);
            assert_eq!(packed.ascii_prefix().is_some(), kind == 0);
            let mut cursor = HistoryCursor::default();
            for i in (0..400).rev() {
                let cell = packed.cell_with_cursor(i, &mut cursor);
                assert_eq!(
                    cell.c,
                    if kind == 2 {
                        ['é', 'λ'][i % 2]
                    } else {
                        char::from(b'a' + (i % 23) as u8)
                    }
                );
                assert_eq!(
                    cell.fg,
                    Color::Indexed(if kind == 1 { (i % 2) as u8 } else { 7 })
                );
            }
            reuse = Some(packed);
        }
    }

    #[test]
    fn high_entropy_attributes_skip_shared_tables() {
        let source: Vec<Cell> = (0..400)
            .map(|i| Cell {
                c: char::from(b'a' + (i % 23) as u8),
                fg: Color::Spec(Rgb {
                    r: (i / 256) as u8,
                    g: i as u8,
                    b: 5,
                }),
                ..Cell::default()
            })
            .collect();
        assert!(PackedHistory::encode(&source, None).is_none());
    }
}
