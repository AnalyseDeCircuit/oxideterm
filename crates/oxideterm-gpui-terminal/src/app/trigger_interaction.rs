use super::*;
use gpui::MouseDownEvent;
use oxideterm_terminal_triggers::{TerminalTriggerAction, TriggerMatched, expand_template};

pub(super) struct TriggerHover {
    pub(super) generation: u64,
    pub(super) position: Point<Pixels>,
    pub(super) tooltip: Option<Zeroizing<String>>,
    pub(super) link: Option<TerminalLinkRange>,
    row: usize,
    start: usize,
    end: usize,
}

#[derive(Clone)]
pub(crate) struct TriggerMenu {
    pub(super) generation: u64,
    pub(super) labels: Vec<Zeroizing<String>>,
    matched: TriggerMatched,
}

impl std::fmt::Debug for TriggerMenu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TriggerMenu")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

struct InteractionHit {
    matched: TriggerMatched,
    action: TerminalTriggerAction,
    link: TerminalLinkRange,
}

impl TerminalPane {
    fn interactive_trigger_at(&self, position: Point<Pixels>) -> Option<InteractionHit> {
        let rules = self.interactive_trigger_rules.as_ref()?;
        if self.snapshot_dirty || !rules.has_interactions() {
            return None;
        }
        let point = self.hovered_text_point(position)?;
        // Explicit hyperlinks and existing file links keep their established behavior.
        if self.link_at_position(position).is_some() {
            return None;
        }
        let snapshot = self.displayed_snapshot();
        let row = snapshot.lines.get(point.row)?;
        let cell = row.cells.get(point.col)?;
        if row.active_input || !is_link_interactive_cell(cell) {
            return None;
        }
        let mut start_row = point.row;
        while start_row > 0 && snapshot.lines[start_row - 1].wrapped {
            start_row -= 1;
        }
        let mut text = Zeroizing::new(String::new());
        let mut cells = Vec::new();
        let mut hit_byte = None;
        for row_index in start_row..snapshot.lines.len() {
            let row = &snapshot.lines[row_index];
            if row.active_input {
                return None;
            }
            let mut spacer = false;
            for (col, cell) in row.cells.iter().enumerate() {
                if spacer {
                    spacer = false;
                    continue;
                }
                let end_col = col + if cell.wide { 2 } else { 1 };
                if row_index == point.row && (col..end_col).contains(&point.col) {
                    hit_byte = Some(text.len());
                }
                let begin = text.len();
                text.push(cell.ch);
                text.push_str(cell.zerowidth());
                cells.push((begin, text.len(), row_index, col, end_col));
                if text.len() > oxideterm_terminal_triggers::MAX_LOGICAL_LINE_BYTES {
                    return None;
                }
                spacer = cell.wide;
            }
            if !row.wrapped {
                break;
            }
        }
        let (range, matched, action) =
            rules.interaction_at(text.trim_end_matches(' '), hit_byte?)?;
        let mut visible = cells.iter().filter(|(start, end, row, _, _)| {
            *row == point.row && *start < range.end && *end > range.start
        });
        let first = visible.next()?;
        let end_col = visible.next_back().unwrap_or(first).4;
        Some(InteractionHit {
            link: TerminalLinkRange {
                row: point.row,
                start_col: first.3,
                end_col,
                target: matched.trigger_id().to_owned().into(),
                kind: TerminalLinkKind::Trigger,
            },
            matched,
            action: action.clone(),
        })
    }

    pub(super) fn update_interactive_trigger_hover(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(point) = self.hovered_text_point(position) else {
            self.clear_terminal_hover(cx);
            return false;
        };
        if self
            .interactive_trigger_hover
            .as_ref()
            .is_some_and(|hover| {
                hover.generation == self.snapshot_generation
                    && hover.row == point.row
                    && (hover.start..hover.end).contains(&point.col)
            })
        {
            return true;
        }
        let hit = self.interactive_trigger_at(position);
        let Some(hit) = hit else {
            if self.interactive_trigger_hover.is_some() {
                self.clear_terminal_hover(cx);
            }
            return false;
        };
        self.clear_terminal_hover(cx);
        let tooltip = match &hit.action {
            TerminalTriggerAction::ShowTooltip { text } => expand_template(text, &hit.matched).ok(),
            _ => None,
        };
        let clickable = !matches!(hit.action, TerminalTriggerAction::ShowTooltip { .. });
        self.interactive_trigger_hover = Some(TriggerHover {
            generation: self.snapshot_generation,
            position,
            tooltip,
            row: hit.link.row,
            start: hit.link.start_col,
            end: hit.link.end_col,
            link: clickable.then_some(hit.link),
        });
        cx.emit(TerminalPaneEvent::HoverTooltipChanged);
        true
    }

    pub(super) fn activate_interactive_trigger(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if mouse_mode(self.terminal.lock().mode(), event.modifiers.shift) {
            return false;
        }
        let Some(hit) = self.interactive_trigger_at(event.position) else {
            return false;
        };
        match hit.action {
            TerminalTriggerAction::CustomLink { .. } => {
                self.trigger_interaction_requested = Some((hit.matched, None));
                cx.emit(TerminalPaneEvent::TriggerInteractionRequested);
            }
            TerminalTriggerAction::CustomMenu { items } => {
                let Ok(labels) = items
                    .iter()
                    .map(|item| expand_template(&item.label, &hit.matched))
                    .collect::<Result<Vec<_>, _>>()
                else {
                    return false;
                };
                self.open_terminal_context_menu(event, cx);
                if let Some(menu) = self.context_menu.as_mut() {
                    menu.trigger = Some(TriggerMenu {
                        generation: self.snapshot_generation,
                        matched: hit.matched,
                        labels,
                    });
                }
            }
            _ => return false,
        }
        true
    }

    pub(super) fn request_interactive_trigger_menu_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let request = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.trigger.as_ref())
            .filter(|menu| {
                menu.generation == self.snapshot_generation
                    && !self.snapshot_dirty
                    && index < menu.labels.len()
            })
            .map(|menu| (menu.matched.clone(), Some(index)));
        self.dismiss_terminal_context_menu(cx);
        if let Some(request) = request {
            self.trigger_interaction_requested = Some(request);
            cx.emit(TerminalPaneEvent::TriggerInteractionRequested);
        }
    }

    pub fn take_trigger_interaction(&mut self) -> Option<(TriggerMatched, Option<usize>)> {
        self.trigger_interaction_requested.take()
    }

    pub fn clear_terminal_interactions(&mut self, cx: &mut Context<Self>) {
        self.clear_terminal_hover(cx);
        self.trigger_interaction_requested = None;
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.trigger.is_some())
        {
            self.context_menu = None;
            self.context_menu_presence.reopen();
            cx.notify();
        }
    }
}
