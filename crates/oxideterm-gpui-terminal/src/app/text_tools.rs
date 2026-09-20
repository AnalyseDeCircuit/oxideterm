use super::*;
use crate::reading_tools::{ExtractionShell, NumberInspection, extraction_command, inspect_number};
use gpui::{
    Anchor, AnchoredPositionMode, AnyElement, SharedString, anchored, deferred, div, prelude::*,
    rgb,
};
use oxideterm_gpui_ui::modal::overlay_content_boundary;

pub(super) struct HoverInspection {
    text: Zeroizing<String>,
    position: gpui::Point<gpui::Pixels>,
    values: Vec<(String, String)>,
}

impl TerminalPane {
    pub(super) fn text_token_at(
        &self,
        position: gpui::Point<gpui::Pixels>,
    ) -> Option<Zeroizing<String>> {
        if !self.bounds.is_some_and(|bounds| bounds.contains(&position)) {
            return None;
        }
        let point = self.terminal_point_for_position(position);
        let range = crate::terminal_view::highlight::logical_line_range(&self.snapshot, point.row)?;
        let mut text = Zeroizing::new(String::new());
        let mut offset = 0;
        for row in range {
            for (col, cell) in self.snapshot.lines[row].cells.iter().enumerate() {
                if row == point.row && col == point.col {
                    offset = text.len();
                }
                if col > 0 && self.snapshot.lines[row].cells[col - 1].wide {
                    continue;
                }
                text.push(cell.ch);
                text.push_str(cell.zerowidth());
            }
        }
        token_at(&text, offset).map(|token| Zeroizing::new(token.to_string()))
    }

    pub(super) fn update_text_inspection(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let text = enabled.then(|| self.text_token_at(position)).flatten();
        if self
            .hover_inspection
            .as_ref()
            .map(|hover| hover.text.as_str())
            == text.as_deref().map(|text| text.as_str())
        {
            return;
        }
        let previously_open = self.hover_inspection.is_some();
        self.hover_inspection = text.and_then(|text| {
            let labels = &self.preferences.reading_labels;
            let values = match inspect_number(&text)? {
                NumberInspection::Timestamp { local, utc } => {
                    vec![(labels.local.clone(), local), (labels.utc.clone(), utc)]
                }
                NumberInspection::Hex { decimal, binary } => vec![
                    (labels.decimal.clone(), decimal),
                    (labels.binary.clone(), binary),
                ],
            };
            Some(HoverInspection {
                text,
                position,
                values,
            })
        });
        if previously_open || self.hover_inspection.is_some() {
            cx.notify();
        }
    }

    pub fn inspect_selected_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.selected_text_snapshot().map(Zeroizing::new) else {
            return;
        };
        self.dismiss_terminal_context_menu(cx);
        if extraction_command(&text, ExtractionShell::Posix).is_some() {
            self.open_archive_editor(text.to_string(), window, cx);
        } else if let Some(result) = inspect_number(&text) {
            let labels = &self.preferences.reading_labels;
            let values = match result {
                NumberInspection::Timestamp { local, utc } => {
                    vec![(labels.local.clone(), local), (labels.utc.clone(), utc)]
                }
                NumberInspection::Hex { decimal, binary } => vec![
                    (labels.decimal.clone(), decimal),
                    (labels.binary.clone(), binary),
                ],
            };
            self.hover_inspection = Some(HoverInspection {
                text,
                position: self.content_origin(),
                values,
            });
            cx.notify();
        }
    }

    pub(super) fn archive_context_path(&self) -> Option<Zeroizing<String>> {
        let text = self
            .selected_text_snapshot()
            .map(Zeroizing::new)
            .or_else(|| {
                self.context_menu
                    .as_ref()
                    .and_then(|menu| self.text_token_at(gpui::point(px(menu.x), px(menu.y))))
            })?;
        extraction_command(&text, ExtractionShell::Posix).map(|_| text)
    }

    pub(super) fn render_text_inspection(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let hover = self.hover_inspection.as_ref()?;
        let tokens = self.theme.tokens;
        let panel_width = (340.0 * (tokens.metrics.ui_text_sm / 12.0).max(1.0))
            .min((f32::from(window.viewport_size().width) - 16.0).max(0.0));
        let mut body = oxideterm_gpui_ui::command_panel::command_panel(
            &tokens,
            oxideterm_gpui_ui::command_panel::CommandPanelOptions::new()
                .terminal_owned()
                .width(panel_width)
                .padding(oxideterm_gpui_ui::SurfacePadding::Compact)
                .has_background_image(self.reading_background_active()),
        )
        .bg(oxideterm_gpui_ui::color_for_background(
            tokens.ui.bg_elevated,
            self.reading_background_active(),
            (tokens.metrics.panel_vibrancy_alpha.clamp(0.0, 1.0) * 255.0).round() as u32,
        ))
        .gap(px(tokens.spacing.one))
        .font_family(tokens.metrics.font_family)
        .text_size(px(tokens.metrics.ui_text_sm))
        .text_color(rgb(tokens.ui.text))
        .child(oxideterm_gpui_ui::section::section_header(
            &tokens,
            hover.text.to_string(),
            oxideterm_gpui_ui::section::SectionHeaderOptions::new().compact(),
            Some(self.reading_icon("hash", tokens.metrics.sidebar_action_icon_size)),
            Some(self.reading_close_button(
                "inspection-close",
                |pane, _, _, cx| {
                    pane.hover_inspection = None;
                    cx.notify();
                },
                cx,
            )),
        ));
        for (index, (label, value)) in hover.values.iter().enumerate() {
            let copied = value.clone();
            let copy_label = self.preferences.reading_labels.copy.clone();
            let copy = oxideterm_gpui_ui::button::icon_button(
                &tokens,
                self.reading_icon("copy", tokens.metrics.sidebar_action_icon_size),
                oxideterm_gpui_ui::button::IconButtonOptions::compact(
                    tokens.metrics.ui_button_sm_height,
                ),
            )
            .id(SharedString::from(format!("inspection-copy-{index}")))
            .tooltip(move |_, cx| {
                oxideterm_gpui_ui::tooltip::tooltip_view(tokens, copy_label.clone(), None, cx)
            })
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
            }))
            .into_any_element();
            body = body.child(
                oxideterm_gpui_ui::entity_row::entity_list_row(
                    &tokens,
                    oxideterm_gpui_ui::entity_row::EntityListRowOptions::new()
                        .compact()
                        .has_background_image(self.reading_background_active()),
                    Some(
                        div()
                            .w(px(tokens.metrics.ui_text_sm * 5.0))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(label.clone())
                            .into_any_element(),
                    ),
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(self.preferences.font_family.clone())
                        .child(value.clone())
                        .into_any_element(),
                    None,
                    Vec::new(),
                    vec![copy],
                )
                .px_0()
                .py(px(tokens.spacing.one / 2.0)),
            );
        }
        let position = gpui::point(
            hover
                .position
                .x
                .min((window.viewport_size().width - px(panel_width + 8.0)).max(px(0.0))),
            (hover.position.y + px(8.0))
                .min((window.viewport_size().height - px(150.0)).max(px(0.0))),
        );
        Some(
            deferred(
                anchored()
                    .anchor(Anchor::TopLeft)
                    .position_mode(AnchoredPositionMode::Window)
                    .position(position)
                    .child(
                        overlay_content_boundary(body)
                            .on_mouse_move(|_, _, cx| cx.stop_propagation()),
                    ),
            )
            .into_any_element(),
        )
    }
}

fn token_at(text: &str, offset: usize) -> Option<&str> {
    // Quotes delimit paths with spaces; unquoted punctuation delimits numeric log fields.
    let mut quote = None;
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if let Some(delimiter) = quote {
            if ch == delimiter {
                if offset >= start && offset < index {
                    return Some(&text[start..index]);
                }
                quote = None;
                start = index + ch.len_utf8();
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            start = index + ch.len_utf8();
        } else if ch.is_whitespace() || matches!(ch, '[' | ']' | '(' | ')' | ',' | ';' | '=' | ':')
        {
            if offset >= start && offset < index {
                return Some(&text[start..index]);
            }
            start = index + ch.len_utf8();
        }
    }
    (quote.is_none() && offset >= start && offset < text.len()).then(|| &text[start..])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokens_respect_identifier_boundaries_and_quoted_paths() {
        assert_eq!(token_at("ts=[1700000000]", 7), Some("1700000000"));
        assert_eq!(token_at("id1700000000", 5), Some("id1700000000"));
        assert_eq!(token_at("'my file.zip' next", 5), Some("my file.zip"));
        assert_eq!(token_at("0xFFbadG", 3), Some("0xFFbadG"));
    }
}
