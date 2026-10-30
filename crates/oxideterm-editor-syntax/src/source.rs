// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

/// Borrowed input for parsers and queries. Reads return the remainder of one
/// contiguous region; offsets are bytes and may be inside a UTF-8 character.
/// Regions must end at character boundaries in valid UTF-8 text. Reads before
/// `len()` must make progress; reads at or beyond it return an empty slice.
pub trait TextSource {
    fn len(&self) -> usize;
    fn bytes_at(&self, offset: usize) -> &[u8];
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: AsRef<str> + ?Sized> TextSource for T {
    fn len(&self) -> usize {
        self.as_ref().len()
    }
    fn bytes_at(&self, offset: usize) -> &[u8] {
        self.as_ref().as_bytes().get(offset..).unwrap_or_default()
    }
}

/// A worker-local index over borrowed snapshot pieces. Only offsets and slice
/// references are allocated; the snapshot continues to own the text.
pub struct ChunkedText<'a> {
    pieces: Vec<(usize, &'a [u8])>,
    len: usize,
}

impl<'a> ChunkedText<'a> {
    pub fn new(chunks: impl Iterator<Item = &'a str>) -> Self {
        let mut pieces = Vec::with_capacity(chunks.size_hint().0);
        let mut len = 0;
        for text in chunks {
            if !text.is_empty() {
                pieces.push((len, text.as_bytes()));
                len += text.len();
            }
        }
        Self { pieces, len }
    }
}

impl TextSource for ChunkedText<'_> {
    fn len(&self) -> usize {
        self.len
    }
    fn bytes_at(&self, offset: usize) -> &[u8] {
        if offset >= self.len {
            return &[];
        }
        let index = self.pieces.partition_point(|(start, _)| *start <= offset) - 1;
        let (start, bytes) = self.pieces[index];
        &bytes[offset - start..]
    }
}

pub(crate) struct SourceSlice<'a, S: ?Sized> {
    source: &'a S,
    range: Range<usize>,
}

impl<'a, S: TextSource + ?Sized> SourceSlice<'a, S> {
    pub(crate) fn new(source: &'a S, range: Range<usize>) -> Self {
        debug_assert!(range.start <= range.end && range.end <= source.len());
        Self { source, range }
    }
}

impl<S: TextSource + ?Sized> TextSource for SourceSlice<'_, S> {
    fn len(&self) -> usize {
        self.range.len()
    }
    fn bytes_at(&self, offset: usize) -> &[u8] {
        if offset >= self.len() {
            return &[];
        }
        let bytes = self.source.bytes_at(self.range.start + offset);
        &bytes[..bytes.len().min(self.len() - offset)]
    }
}

pub(crate) struct SourceChunks<'a, S: ?Sized> {
    source: &'a S,
    range: Range<usize>,
}

impl<'a, S: TextSource + ?Sized> SourceChunks<'a, S> {
    pub(crate) fn new(source: &'a S, range: Range<usize>) -> Self {
        Self { source, range }
    }
}

impl<'a, S: TextSource + ?Sized> Iterator for SourceChunks<'a, S> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.range.start >= self.range.end {
            return None;
        }
        let bytes = self.source.bytes_at(self.range.start);
        let len = bytes.len().min(self.range.len());
        if len == 0 {
            return None;
        }
        self.range.start += len;
        Some(&bytes[..len])
    }
}

pub(crate) struct QuerySource<'a, S: ?Sized>(pub(crate) &'a S);

impl<'a, S: TextSource + ?Sized> tree_sitter::TextProvider<&'a [u8]> for QuerySource<'a, S> {
    type I = SourceChunks<'a, S>;
    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        SourceChunks::new(self.0, node.byte_range())
    }
}

pub(crate) fn chars<S: TextSource + ?Sized>(
    source: &S,
    range: Range<usize>,
) -> impl Iterator<Item = char> + '_ {
    SourceChunks::new(source, range).flat_map(|bytes| {
        std::str::from_utf8(bytes)
            .expect("text range must end on character boundaries")
            .chars()
    })
}

pub(crate) struct SourceLines<'a, S: ?Sized> {
    source: &'a S,
    remaining: Range<usize>,
}

pub(crate) fn lines<S: TextSource + ?Sized>(source: &S, range: Range<usize>) -> SourceLines<'_, S> {
    SourceLines {
        source,
        remaining: range,
    }
}

impl<S: TextSource + ?Sized> Iterator for SourceLines<'_, S> {
    type Item = Range<usize>;
    fn next(&mut self) -> Option<Self::Item> {
        let start = self.remaining.start;
        if start >= self.remaining.end {
            return None;
        }
        while self.remaining.start < self.remaining.end {
            let bytes = self.source.bytes_at(self.remaining.start);
            let bytes = &bytes[..bytes.len().min(self.remaining.len())];
            if let Some(index) = memchr::memchr(b'\n', bytes) {
                let mut end = self.remaining.start + index;
                self.remaining.start = end + 1;
                if end > start && self.source.bytes_at(end - 1).first() == Some(&b'\r') {
                    end -= 1;
                }
                return Some(start..end);
            }
            self.remaining.start += bytes.len();
        }
        Some(start..self.remaining.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_reads_and_lines_keep_piece_boundaries_utf8_and_crlf() {
        let source = ChunkedText::new(["", "aé", "🙂\r", "\n中", "z", ""].into_iter());
        assert_eq!(source.len(), 13);
        assert_eq!(source.bytes_at(2), &[0xa9]);
        assert_eq!(source.bytes_at(4), &"🙂\r".as_bytes()[1..]);
        assert_eq!(source.bytes_at(13), b"");
        assert_eq!(lines(&source, 0..13).collect::<Vec<_>>(), [0..7, 9..13]);
        assert_eq!(chars(&source, 1..7).collect::<String>(), "é🙂");
        let slice = SourceSlice::new(&source, 1..12);
        assert_eq!(
            SourceChunks::new(&slice, 0..slice.len())
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
            "é🙂\r\n中".as_bytes()
        );
        assert_eq!(slice.bytes_at(1), &[0xa9]);
    }

    #[test]
    fn query_predicates_read_complete_tokens_across_chunks() {
        use tree_sitter::StreamingIterator;
        let source = ChunkedText::new(
            [
                "{\"ta",
                "rget\"",
                ": \"wrong\", \"other\": ",
                "\"ignored\"",
                "}",
            ]
            .into_iter(),
        );
        let language = crate::LanguageId::Json.tree_sitter_language().unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        let tree = crate::work::parse(&mut parser, &source, None, None).unwrap();
        assert!(!tree.root_node().has_error());
        let query = tree_sitter::Query::new(
            &language,
            r#"((string) @selected (#eq? @selected "\"target\""))"#,
        )
        .unwrap();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut captures = cursor.captures(&query, tree.root_node(), QuerySource(&source));
        let mut ranges = Vec::new();
        loop {
            captures.advance();
            let Some((matched, index)) = captures.get() else {
                break;
            };
            ranges.push(matched.captures()[*index].node.byte_range());
        }
        assert_eq!(ranges, [1..9]);
    }
}
