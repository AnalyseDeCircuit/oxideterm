use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PieceSource {
    Original,
    Add,
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
/// `original` never changes after construction. Inserted text is appended to
/// `add`, and `pieces` describes the visible document as byte spans into those
/// two buffers. `TextBuffer` now materializes the full document only for
/// boundary APIs such as save, syntax, search, and IME; edits themselves keep
/// the piece table as the source of truth.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PieceTableTextBuffer {
    pub(crate) original: String,
    pub(crate) add: String,
    pub(crate) pieces: Vec<Piece>,
    len: usize,
}

impl PieceTableTextBuffer {
    pub(crate) fn new(original: String) -> Self {
        let len = original.len();
        let pieces = Piece::new(PieceSource::Original, 0, len)
            .into_iter()
            .collect();
        Self {
            original,
            add: String::new(),
            pieces,
            len,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn source_text(&self, source: PieceSource) -> &str {
        match source {
            PieceSource::Original => &self.original,
            PieceSource::Add => &self.add,
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

    pub(crate) fn replace(&mut self, range: Range<usize>, replacement: &str) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len);

        let replacement_piece = self.append_add_piece(replacement);
        let mut next =
            Vec::with_capacity(self.pieces.len() + usize::from(replacement_piece.is_some()));
        let mut position = 0;
        let mut inserted = false;

        for piece in self.pieces.iter().copied() {
            let piece_start = position;
            let piece_end = position + piece.len;
            position = piece_end;

            if piece_end <= range.start {
                push_piece(&mut next, piece);
                continue;
            }

            if piece_start >= range.end {
                if !inserted {
                    if let Some(piece) = replacement_piece {
                        push_piece(&mut next, piece);
                    }
                    inserted = true;
                }
                push_piece(&mut next, piece);
                continue;
            }

            if !inserted {
                if range.start > piece_start {
                    let left_len = range.start - piece_start;
                    if let Some(left) = piece.slice(0, left_len) {
                        push_piece(&mut next, left);
                    }
                }
                if let Some(piece) = replacement_piece {
                    push_piece(&mut next, piece);
                }
                inserted = true;
            }

            if range.end < piece_end {
                let right_offset = range.end - piece_start;
                let right_len = piece_end - range.end;
                if let Some(right) = piece.slice(right_offset, right_len) {
                    push_piece(&mut next, right);
                }
            }
        }

        if !inserted {
            if let Some(piece) = replacement_piece {
                push_piece(&mut next, piece);
            }
        }

        self.pieces = next;
        self.len = self.len - (range.end - range.start) + replacement.len();
    }

    fn append_add_piece(&mut self, text: &str) -> Option<Piece> {
        let start = self.add.len();
        self.add.push_str(text);
        Piece::new(PieceSource::Add, start, text.len())
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
