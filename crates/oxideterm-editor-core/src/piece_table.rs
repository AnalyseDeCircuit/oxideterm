use crate::{
    BufferOffset, TextEdit, TextRange,
    edit::{SharedText, TextAllocation, TextSlice},
};
use std::{collections::HashMap, ops::Range, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PieceSource {
    Original,
    Add(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Piece {
    pub(crate) source: PieceSource,
    start: usize,
    len: usize,
}

impl Piece {
    fn new(source: PieceSource, start: usize, len: usize) -> Option<Self> {
        (len > 0).then_some(Self { source, start, len })
    }

    fn slice(self, offset: usize, len: usize) -> Option<Self> {
        debug_assert!(offset <= self.len);
        debug_assert!(offset + len <= self.len);
        Self::new(self.source, self.start + offset, len)
    }

    fn end(self) -> usize {
        self.start + self.len
    }
}

/// Piece-table storage inspired by Monaco/VS Code's buffer model.
///
/// `original` is immutable and shared until dead ranges are compacted. Inserted text is appended to
/// bounded `add` chunks, and `pieces` describes the visible document as byte spans.
/// `TextBuffer` materializes the full document only for
/// consumers requiring contiguous text, such as regex search; edits themselves keep
/// the piece table as the source of truth.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PieceTableTextBuffer {
    pub(crate) original: Arc<str>,
    pub(crate) add: Arc<Vec<TextAllocation>>,
    pub(crate) pieces: Arc<Vec<Piece>>,
    len: usize,
}

impl PieceTableTextBuffer {
    pub(crate) fn new(original: Arc<str>) -> Self {
        let len = original.len();
        let pieces = Piece::new(PieceSource::Original, 0, len)
            .into_iter()
            .collect();
        Self {
            original,
            add: Arc::new(Vec::new()),
            pieces: Arc::new(pieces),
            len,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn shared_text(&self) -> SharedText {
        SharedText::from_slices(
            self.pieces
                .iter()
                .map(|piece| TextSlice {
                    allocation: match piece.source {
                        PieceSource::Original => TextAllocation::Original(self.original.clone()),
                        PieceSource::Add(index) => self.add[index].clone(),
                    },
                    range: piece.start..piece.end(),
                })
                .collect(),
        )
    }

    pub(crate) fn from_shared_text(text: SharedText) -> Self {
        if let [slice] = text.slices()
            && let TextAllocation::Original(original) = &slice.allocation
            && slice.range == (0..original.len())
        {
            return Self::new(original.clone());
        }
        let mut storage = Self::new(Arc::from(""));
        let len = text.len();
        storage.replace_batch(
            &[TextEdit::with_shared_text(
                TextRange::caret(BufferOffset::ZERO),
                text,
            )],
            len,
        );
        storage
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn source_text(&self, source: PieceSource) -> &str {
        match source {
            PieceSource::Original => &self.original,
            PieceSource::Add(index) => self.add[index].text(),
        }
    }

    pub(crate) fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.len);
        for piece in self.pieces.iter().copied() {
            let source = self.source_text(piece.source);
            text.push_str(&source[piece.start..piece.end()]);
        }
        text
    }

    pub(crate) fn chunks(&self) -> impl Iterator<Item = &str> {
        self.pieces
            .iter()
            .map(|piece| &self.source_text(piece.source)[piece.start..piece.end()])
    }

    pub(crate) fn slice_to_string(&self, range: Range<usize>) -> String {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len);
        if range.is_empty() {
            return String::new();
        }

        let mut text = String::with_capacity(range.end - range.start);
        let mut position = 0;
        for piece in self.pieces.iter().copied() {
            let piece_start = position;
            let piece_end = position + piece.len;
            position = piece_end;
            if piece_end <= range.start {
                continue;
            }
            if piece_start >= range.end {
                break;
            }
            let local_start = range.start.max(piece_start) - piece_start;
            let local_end = range.end.min(piece_end) - piece_start;
            let source = self.source_text(piece.source);
            text.push_str(&source[piece.start + local_start..piece.start + local_end]);
        }
        text
    }

    pub(crate) fn borrow_range(&self, range: Range<usize>) -> Option<&str> {
        if range.is_empty() {
            return Some("");
        }
        let mut base = 0;
        for piece in self.pieces.iter() {
            if range.start >= base && range.end <= base + piece.len {
                return self
                    .source_text(piece.source)
                    .get(piece.start + range.start - base..piece.start + range.end - base);
            }
            base += piece.len;
            if base > range.start {
                break;
            }
        }
        None
    }

    pub(crate) fn is_char_boundary(&self, offset: usize) -> bool {
        if offset > self.len {
            return false;
        }
        if offset == self.len {
            return true;
        }

        let mut position = 0;
        for piece in self.pieces.iter().copied() {
            let piece_start = position;
            let piece_end = position + piece.len;
            position = piece_end;
            if offset < piece_start {
                break;
            }
            if offset == piece_start || offset == piece_end {
                return true;
            }
            if offset < piece_end {
                let source_offset = piece.start + (offset - piece_start);
                return self
                    .source_text(piece.source)
                    .is_char_boundary(source_offset);
            }
        }
        false
    }

    /// Edits are validated, sorted and expressed in the original document's coordinates.
    pub(crate) fn replace_batch(&mut self, edits: &[TextEdit], new_len: usize) -> Vec<TextEdit> {
        let original_pieces = std::mem::take(&mut self.pieces);
        let mut cursor = PieceCursor::new(&original_pieces);
        let mut next = Vec::with_capacity(original_pieces.len() + edits.len());
        let mut inverse = Vec::with_capacity(edits.len());
        let mut written = 0;
        let mut sources = HashMap::with_capacity(self.add.len() + 1);
        sources.insert(
            TextAllocation::Original(self.original.clone()).allocation_key(),
            PieceSource::Original,
        );
        for (index, allocation) in self.add.iter().enumerate() {
            sources.insert(allocation.allocation_key(), PieceSource::Add(index));
        }

        for edit in edits {
            written += edit.range.start.0 - cursor.position;
            cursor.take_until(edit.range.start.0, |piece| push_piece(&mut next, piece));
            let mut removed: Vec<TextSlice> = Vec::new();
            cursor.take_until(edit.range.end.0, |piece| {
                let allocation = match piece.source {
                    PieceSource::Original => TextAllocation::Original(self.original.clone()),
                    PieceSource::Add(index) => self.add[index].clone(),
                };
                if let Some(previous) = removed.last_mut()
                    && previous.allocation.same_allocation(&allocation)
                    && previous.range.end == piece.start
                {
                    previous.range.end = piece.end();
                } else {
                    removed.push(TextSlice {
                        allocation,
                        range: piece.start..piece.end(),
                    });
                }
            });
            for slice in edit.replacement.slices() {
                let piece = self.append_slice(slice, &mut sources);
                push_piece(&mut next, piece);
            }
            let start = written;
            written += edit.replacement.len();
            inverse.push(TextEdit::with_shared_text(
                TextRange::new(BufferOffset(start), BufferOffset(written)),
                SharedText::from_slices(removed),
            ));
        }
        written += self.len - cursor.position;
        cursor.take_until(self.len, |piece| push_piece(&mut next, piece));
        debug_assert_eq!(written, new_len);
        // The next transaction builds another vector, so retaining spare capacity has no benefit.
        next.shrink_to_fit();
        self.pieces = Arc::new(next);
        self.len = new_len;
        inverse
    }

    pub(crate) fn reclaim_unused_text(&mut self) {
        let original_live = self
            .pieces
            .iter()
            .filter(|piece| piece.source == PieceSource::Original)
            .map(|piece| piece.len)
            .sum::<usize>();
        if original_live == 0 {
            if !self.original.is_empty() {
                self.original = Arc::from("");
            }
        } else if Arc::strong_count(&self.original) == 1
            && self.original.len().saturating_sub(original_live) > original_live
        {
            // Undo and worker snapshots can retain slices of the original.
            // Compact only when dropping this reference would release the old allocation.
            let mut compact = String::with_capacity(original_live);
            for piece in Arc::make_mut(&mut self.pieces) {
                if piece.source == PieceSource::Original {
                    let start = compact.len();
                    compact.push_str(&self.original[piece.start..piece.end()]);
                    piece.start = start;
                }
            }
            self.original = compact.into();
        }
        let mut usage = vec![(0, 0, 0); self.add.len()];
        for piece in self.pieces.iter() {
            if let PieceSource::Add(index) = piece.source {
                usage[index].0 += piece.len;
                usage[index].1 = usage[index].1.max(piece.end());
            }
        }
        let mut retained = 0;
        for (index, (live, end, new_index)) in usage.iter_mut().enumerate() {
            if *live == 0 {
                continue;
            }
            // A shared allocation table is also a reader, even if its text Arc has one owner.
            let table_unique = Arc::strong_count(&self.add) == 1;
            let chunk = &self.add[index];
            if table_unique && chunk.is_unique() && chunk.text().len().saturating_sub(*live) > *live
            {
                let mut compact = String::with_capacity(*live);
                for piece in Arc::make_mut(&mut self.pieces) {
                    if piece.source == PieceSource::Add(index) {
                        let start = compact.len();
                        compact.push_str(&chunk.text()[piece.start..piece.end()]);
                        piece.start = start;
                    }
                }
                Arc::make_mut(&mut self.add)[index] = TextAllocation::Added(Arc::new(compact));
            } else if table_unique
                && *end < chunk.text().len()
                && let TextAllocation::Added(chunk) = &mut Arc::make_mut(&mut self.add)[index]
                && let Some(chunk) = Arc::get_mut(chunk)
            {
                // A worker still reading this chunk keeps its suffix; do not clone it to trim.
                chunk.truncate(*end);
            }
            *new_index = retained;
            retained += 1;
        }
        if retained != self.add.len() {
            for piece in Arc::make_mut(&mut self.pieces) {
                if let PieceSource::Add(index) = &mut piece.source {
                    *index = usage[*index].2;
                }
            }
            let mut index = 0;
            Arc::make_mut(&mut self.add).retain(|_| {
                let keep = usage[index].0 != 0;
                index += 1;
                keep
            });
        }
        if self.add.is_empty() {
            self.add = Arc::new(Vec::new());
        }
    }

    fn append_slice(
        &mut self,
        slice: &TextSlice,
        sources: &mut HashMap<(u8, usize), PieceSource>,
    ) -> Piece {
        let key = slice.allocation.allocation_key();
        if let Some(&source) = sources.get(&key) {
            return Piece::new(source, slice.range.start, slice.range.len()).unwrap();
        }
        // Shared snapshots can keep old chunks indefinitely. Append elsewhere
        // rather than copying their accumulated text on each keystroke.
        const CHUNK_BYTES: usize = 64 * 1024;
        if let Some(last) = Arc::make_mut(&mut self.add).last_mut()
            && slice.allocation.is_unique()
            && let TextAllocation::Added(last) = last
            && last.len().saturating_add(slice.range.len()) <= CHUNK_BYTES
            && let Some(chunk) = Arc::get_mut(last)
        {
            let start = chunk.len();
            chunk.push_str(slice.text());
            return Piece::new(
                PieceSource::Add(self.add.len() - 1),
                start,
                slice.range.len(),
            )
            .unwrap();
        }
        let index = self.add.len();
        Arc::make_mut(&mut self.add).push(slice.allocation.clone());
        let source = PieceSource::Add(index);
        sources.insert(key, source);
        Piece::new(source, slice.range.start, slice.range.len()).unwrap()
    }
}

struct PieceCursor<'a> {
    pieces: &'a [Piece],
    index: usize,
    offset: usize,
    position: usize,
}

impl<'a> PieceCursor<'a> {
    fn new(pieces: &'a [Piece]) -> Self {
        Self {
            pieces,
            index: 0,
            offset: 0,
            position: 0,
        }
    }

    fn take_until(&mut self, end: usize, mut emit: impl FnMut(Piece)) {
        while self.position < end {
            let piece = self.pieces[self.index];
            let len = (piece.len - self.offset).min(end - self.position);
            emit(piece.slice(self.offset, len).unwrap());
            self.position += len;
            self.offset += len;
            if self.offset == piece.len {
                self.index += 1;
                self.offset = 0;
            }
        }
    }
}

fn push_piece(pieces: &mut Vec<Piece>, piece: Piece) {
    if let Some(previous) = pieces.last_mut()
        && previous.source == piece.source
        && previous.end() == piece.start
    {
        previous.len += piece.len;
        return;
    }
    pieces.push(piece);
}
