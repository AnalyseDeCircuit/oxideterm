// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    TerminalLogicalContext, TerminalLogicalContextRow, TerminalLogicalMember, TerminalSnapshot,
};
use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::Line,
    term::{Term, cell::Flags},
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub(crate) fn capture<T: EventListener>(term: &Term<T>, snapshot: &mut TerminalSnapshot) {
    let history = term.grid().history_size() as i32;
    let screen_end = term.screen_lines() as i32;
    let visible_start = -(snapshot.display_offset as i32);
    let visible_end = (visible_start + snapshot.lines.len() as i32).min(screen_end);
    let wraps = |line: i32| {
        term.grid()[Line(line)]
            .last()
            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE))
    };
    let previous = std::mem::take(&mut snapshot.logical_context);
    let mut contexts = Vec::new();
    let mut current = visible_start;
    while current < visible_end {
        if !wraps(current) && (current == -history || !wraps(current - 1)) {
            current += 1;
            continue;
        }
        let mut start = current;
        while start > -history && wraps(start - 1) {
            start -= 1;
        }
        let mut end = current + 1;
        while end < screen_end && wraps(end - 1) {
            end += 1;
        }
        let start_at_history_boundary = start == -history;
        // Validate the current boundaries and every member, including off-screen rows.
        // Display coordinates and cursor roles are intentionally not reused.
        let reusable = previous.iter().find(|context| {
            context.cols == snapshot.cols
                && context.start_at_history_boundary == start_at_history_boundary
                && context.members.len() == (end - start) as usize
                && context
                    .members
                    .iter()
                    .zip(start..end)
                    .all(|(member, line)| {
                        let row = &term.grid()[Line(line)];
                        member.source_id == row.identity() && member.revision == row.revision()
                    })
        });
        let (text, members, signature) = if let Some(context) = reusable {
            (
                context.text.clone(),
                context.members.clone(),
                context.signature,
            )
        } else {
            // Snapshot-owned text is zeroized when the last reader releases it.
            let mut text = zeroize::Zeroizing::new(String::new());
            let mut members = Vec::with_capacity((end - start) as usize);
            for line in start..end {
                let row = &term.grid()[Line(line)];
                members.push(TerminalLogicalMember {
                    source_id: row.identity(),
                    revision: row.revision(),
                    byte_start: text.len(),
                });
                for cell in row.iter().take(snapshot.cols) {
                    if cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                    {
                        continue;
                    }
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                    if let Some(extra) = cell.zerowidth() {
                        text.extend(extra.iter().copied());
                    }
                }
            }
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            start_at_history_boundary.hash(&mut hasher);
            (Arc::new(text), Arc::new(members), hasher.finish())
        };
        let rows = (current..end.min(visible_end))
            .map(|line| {
                let row = &term.grid()[Line(line)];
                let columns = reusable
                    .and_then(|context| {
                        context
                            .rows
                            .iter()
                            .find(|mapped| mapped.source_id == row.identity())
                    })
                    .map(|mapped| mapped.columns.clone())
                    .unwrap_or_else(|| {
                        let mut byte = members[(line - start) as usize].byte_start;
                        let mut columns = Vec::new();
                        for (col, cell) in row.iter().enumerate().take(snapshot.cols) {
                            if cell.flags.intersects(
                                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
                            ) {
                                continue;
                            }
                            columns.push((byte, col));
                            byte += if cell.c == '\0' { 1 } else { cell.c.len_utf8() };
                            if let Some(extra) = cell.zerowidth() {
                                for &ch in extra {
                                    columns.push((byte, col));
                                    byte += ch.len_utf8();
                                }
                            }
                        }
                        Arc::new(columns)
                    });
                TerminalLogicalContextRow {
                    source_id: row.identity(),
                    columns,
                }
            })
            .collect();
        contexts.push(Arc::new(TerminalLogicalContext {
            text,
            members,
            signature,
            cols: snapshot.cols,
            start_line: (history + start) as usize,
            end_line: (history + end - 1) as usize,
            active_input: (start..end).contains(&term.grid().cursor.point.line.0),
            start_at_history_boundary,
            rows,
        }));
        current = end;
    }
    snapshot.logical_context = contexts;
}
