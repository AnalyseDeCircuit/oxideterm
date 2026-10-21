use super::*;
use std::collections::{HashMap, HashSet};

use alacritty_terminal::{grid::Grid, index::Column, term::cell::Cell};

#[derive(Clone, Copy)]
struct TextAnchor {
    head: usize,
    offset: usize,
    after_line: bool,
}

struct CommandAnchors {
    start: Option<TextAnchor>,
    command: Option<TextAnchor>,
    output_start: Option<TextAnchor>,
    output_end: Option<TextAnchor>,
}

#[derive(Default)]
pub(super) struct ShellReflow {
    generation: Option<u64>,
    commands: HashMap<String, CommandAnchors>,
    prompt: Option<TextAnchor>,
    input: Option<TextAnchor>,
}

impl ShellReflow {
    pub(super) fn clear_anchors(&mut self) {
        self.commands.clear();
        self.prompt = None;
        self.input = None;
    }
}

fn row_width(grid: &Grid<Cell>, row: Line) -> usize {
    let columns = grid.columns();
    columns
        - usize::from(
            grid[row][Column(columns - 1)]
                .flags
                .contains(Flags::LEADING_WIDE_CHAR_SPACER),
        )
}

fn wraps(grid: &Grid<Cell>, row: Line) -> bool {
    grid[row][Column(grid.columns() - 1)]
        .flags
        .contains(Flags::WRAPLINE)
}

fn anchor(
    grid: &Grid<Cell>,
    position: TerminalCommandPosition,
    boundary: bool,
) -> Option<TextAnchor> {
    if position.line > grid.total_lines() {
        return None;
    }
    let mut line = position.line;
    let mut after_line = false;
    // A newline boundary belongs to the completed text, not to the next prompt,
    // whose row can be erased and redrawn by the shell's line editor.
    if boundary
        && position.column == 0
        && line > 0
        && !wraps(grid, Line(line as i32 - grid.history_size() as i32 - 1))
    {
        line -= 1;
        after_line = true;
    }
    if line >= grid.total_lines() {
        return None;
    }
    let mut row = Line(line as i32 - grid.history_size() as i32);
    let mut offset = position.column.min(row_width(grid, row));
    while row > grid.topmost_line() && wraps(grid, row - 1) {
        row -= 1;
        offset += row_width(grid, row);
    }
    Some(TextAnchor {
        head: grid[row].identity(),
        offset,
        after_line,
    })
}

// Row identities survive movement; the first row of a logical line survives
// both merging and splitting. Count cell columns, excluding wide-glyph padding.
type LogicalLines = HashMap<usize, Vec<(usize, usize)>>;

fn index_lines(grid: &Grid<Cell>, wanted: &HashSet<usize>) -> LogicalLines {
    let mut lines = HashMap::new();
    if wanted.is_empty() {
        return lines;
    }
    let mut head = 0;
    let mut continuation = false;
    for line in 0..grid.total_lines() {
        let row = Line(line as i32 - grid.history_size() as i32);
        if !continuation {
            head = grid[row].identity();
        }
        if wanted.contains(&head) {
            lines
                .entry(head)
                .or_insert_with(Vec::new)
                .push((line, row_width(grid, row)));
        }
        continuation = wraps(grid, row);
    }
    lines
}

fn resolve(lines: &LogicalLines, anchor: Option<TextAnchor>) -> Option<TerminalCommandPosition> {
    let anchor = anchor?;
    let rows = lines.get(&anchor.head)?;
    if anchor.after_line {
        return Some(TerminalCommandPosition {
            line: rows.last()?.0 + 1,
            column: 0,
        });
    }
    let mut offset = anchor.offset;
    for (index, &(line, width)) in rows.iter().enumerate() {
        if offset < width || (offset == width && index + 1 == rows.len()) {
            return Some(TerminalCommandPosition {
                line,
                column: offset,
            });
        }
        offset = offset.checked_sub(width)?;
    }
    None
}

impl TerminalShellIntegration {
    pub(super) fn capture_prompt_anchors<T: EventListener>(&mut self, term: &Term<T>) {
        let grid = term.primary_grid();
        self.reflow.prompt = self
            .state
            .prompt_start
            .and_then(|point| anchor(grid, point, false));
        self.reflow.input = self
            .state
            .command_start
            .and_then(|point| anchor(grid, point, false));
    }

    pub(super) fn capture_command_anchors<T: EventListener>(
        &mut self,
        term: &Term<T>,
        events: &[TerminalCommandMarkEvent],
    ) {
        let grid = term.primary_grid();
        for event in events {
            let mark = match event {
                TerminalCommandMarkEvent::Created(mark)
                | TerminalCommandMarkEvent::Closed(mark) => mark,
                _ => continue,
            };
            let anchors = self
                .reflow
                .commands
                .entry(mark.command_id.clone())
                .or_insert_with(|| CommandAnchors {
                    start: anchor(
                        grid,
                        TerminalCommandPosition {
                            line: mark.start_line,
                            column: 0,
                        },
                        false,
                    ),
                    command: anchor(
                        grid,
                        TerminalCommandPosition {
                            line: mark.command_line,
                            column: 0,
                        },
                        false,
                    ),
                    output_start: mark
                        .output_start
                        .and_then(|point| anchor(grid, point, true)),
                    output_end: None,
                });
            anchors.output_end = mark.output_end.and_then(|point| anchor(grid, point, true));
        }
        if self.reflow.commands.len() > self.marks.len() {
            let retained: HashSet<&str> = self
                .marks
                .iter()
                .map(|mark| mark.command_id.as_str())
                .collect();
            self.reflow
                .commands
                .retain(|id, _| retained.contains(id.as_str()));
        }
    }

    pub(crate) fn reflow_command_marks<T: EventListener>(
        &mut self,
        term: &Term<T>,
        mut emit: impl FnMut(crate::TerminalEvent),
    ) {
        let generation = term.resize_generation();
        if self.reflow.generation == Some(generation) {
            return;
        }
        let previous = self.reflow.generation.replace(generation);
        if previous.is_none() && generation == 0 {
            return;
        }
        let mut wanted = HashSet::new();
        for anchors in self.reflow.commands.values() {
            wanted.extend(
                [
                    anchors.start,
                    anchors.command,
                    anchors.output_start,
                    anchors.output_end,
                ]
                .into_iter()
                .flatten()
                .map(|anchor| anchor.head),
            );
        }
        wanted.extend(
            [self.reflow.prompt, self.reflow.input]
                .into_iter()
                .flatten()
                .map(|anchor| anchor.head),
        );
        let grid = term.primary_grid();
        let lines = index_lines(grid, &wanted);
        for mark in &mut self.marks {
            let Some(anchors) = self.reflow.commands.get(&mark.command_id) else {
                continue;
            };
            let start = resolve(&lines, anchors.start);
            let command = resolve(&lines, anchors.command);
            mark.output_start = resolve(&lines, anchors.output_start);
            mark.output_end = resolve(&lines, anchors.output_end);
            if mark.output_start.is_none() {
                mark.stale = true;
                mark.output_confidence = TerminalCommandMarkConfidence::Unknown;
            }
            if let (Some(start), Some(command)) = (start, command) {
                mark.start_line = start.line;
                mark.command_line = command.line;
            } else {
                mark.stale = true;
                mark.command_line_clipped = true;
            }
            if let Some(end) = mark.output_end {
                mark.end_line = Some(
                    end.line
                        .saturating_sub(usize::from(end.column == 0))
                        .max(mark.start_line),
                );
            } else if mark.is_closed {
                mark.stale = true;
                mark.output_confidence = TerminalCommandMarkConfidence::Unknown;
            }
        }
        self.state.prompt_start = resolve(&lines, self.reflow.prompt);
        self.state.command_start = resolve(&lines, self.reflow.input);
        self.state.active_start_line = self.state.active_command_id.as_ref().and_then(|id| {
            self.marks
                .iter()
                .find(|mark| &mark.command_id == id && !mark.stale)
                .map(|mark| mark.start_line)
        });
        emit(crate::TerminalEvent::CommandMark(
            TerminalCommandMarkEvent::Reflowed {
                cols: grid.columns(),
                rows: grid.screen_lines(),
                marks: self.marks.clone(),
            },
        ));
    }
}
