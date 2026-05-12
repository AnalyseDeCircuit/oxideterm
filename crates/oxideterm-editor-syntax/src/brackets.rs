// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_core::BufferOffset;

use crate::BracketPair;

pub(crate) fn bracket_pairs(source: &str) -> Vec<BracketPair> {
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut pairs = Vec::new();

    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => stack.push((byte, index)),
            b')' | b']' | b'}' => {
                let Some(position) = stack
                    .iter()
                    .rposition(|(open, _)| brackets_match(*open, byte))
                else {
                    continue;
                };
                let (_, open_index) = stack.remove(position);
                pairs.push(BracketPair {
                    open: BufferOffset(open_index),
                    close: BufferOffset(index),
                });
            }
            _ => {}
        }
    }

    pairs.sort_by_key(|pair| pair.open);
    pairs
}

fn brackets_match(open: u8, close: u8) -> bool {
    matches!((open, close), (b'(', b')') | (b'[', b']') | (b'{', b'}'))
}
