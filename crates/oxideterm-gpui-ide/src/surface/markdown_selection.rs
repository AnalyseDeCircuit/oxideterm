use oxideterm_gpui_markdown::render::{MarkdownTextFragmentId, MarkdownTextLink};
use oxideterm_gpui_ui::text_selection::{selectable_text_hit_target, selected_text_runs};
use std::cell::RefCell;

#[derive(Default)]
struct MarkdownPreviewSelection {
    text: String,
    fragments: HashMap<String, Range<usize>>,
    layouts: RefCell<Vec<(Range<usize>, gpui::TextLayout)>>,
    anchor: usize,
    head: usize,
    dragging: bool,
}

impl MarkdownPreviewSelection {
    fn new(document: &MarkdownDocument, tokens: &ThemeTokens, options: &MarkdownOptions) -> Self {
        let mut selection = Self::default();
        // Index the renderer's text once per document, including virtualized
        // blocks. Clipboard contents must not depend on which rows were painted.
        let _ = oxideterm_gpui_markdown::render::render_document_selectable(
            document,
            tokens,
            options,
            &mut |key, text, _, _| {
                if !selection.text.is_empty() && !key.join_previous {
                    selection.text.push('\n');
                }
                let start = selection.text.len();
                selection.text.push_str(&text);
                selection
                    .fragments
                    .insert(key.key, start..selection.text.len());
                div().into_any_element()
            },
        );
        selection
    }

    fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    fn index_at(&self, position: Point<Pixels>) -> Option<usize> {
        self.layouts
            .borrow()
            .iter()
            .min_by(|(_, a), (_, b)| {
                let distance = |layout: &gpui::TextLayout| {
                    let bounds = layout.bounds();
                    let x = f32::from(position.x - position.x.clamp(bounds.left(), bounds.right()));
                    let y = f32::from(position.y - position.y.clamp(bounds.top(), bounds.bottom()));
                    x * x + y * y
                };
                distance(a).total_cmp(&distance(b))
            })
            .map(|(range, layout)| {
                range.start
                    + layout
                        .index_for_position(position)
                        .unwrap_or_else(|index| index)
            })
    }

    fn begin(&mut self, index: usize, event: &MouseDownEvent) {
        if event.click_count >= 3 {
            self.anchor = self.text[..index].rfind('\n').map_or(0, |index| index + 1);
            self.head = self.text[index..]
                .find('\n')
                .map_or(self.text.len(), |end| index + end);
        } else if event.click_count == 2 {
            if let Some((start, word)) = self
                .text
                .split_word_bound_indices()
                .find(|(start, word)| *start <= index && index < start + word.len())
            {
                self.anchor = start;
                self.head = start + word.len();
            }
        } else {
            if !event.modifiers.shift {
                self.anchor = index;
            }
            self.head = index;
        }
        self.dragging = event.click_count == 1;
    }
}

impl IdeSurface {
    fn render_markdown_fragment(
        &self,
        tab: EditorTabId,
        key: MarkdownTextFragmentId,
        text: SharedString,
        runs: Vec<TextRun>,
        links: Vec<MarkdownTextLink>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selection = self.markdown_previews[&tab].selection.as_ref().unwrap();
        let fragment = selection.fragments[&key.key].clone();
        let range = selection.range();
        let start = range.start.max(fragment.start).min(fragment.end) - fragment.start;
        let end = range.end.max(fragment.start).min(fragment.end) - fragment.start;
        let runs = if start < end {
            selected_text_runs(
                &runs,
                start..end,
                rgba((self.tokens.ui.accent << 8) | 0x40).into(),
            )
        } else {
            runs
        };
        let styled = gpui::StyledText::new(text).with_runs(runs);
        let layout = styled.layout().clone();
        selection
            .layouts
            .borrow_mut()
            .push((fragment.clone(), layout.clone()));
        let hit_layout = layout.clone();
        let offset = fragment.start;
        let hit = selectable_text_hit_target(
            layout.clone(),
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if !this.is_markdown_preview() || this.workspace.active_tab() != Some(tab) {
                    return;
                }
                let index = offset
                    + hit_layout
                        .index_for_position(event.position)
                        .unwrap_or_else(|index| index);
                let selection = this
                    .markdown_previews
                    .get_mut(&tab)
                    .unwrap()
                    .selection
                    .as_mut()
                    .unwrap();
                selection.begin(index, event);
                window.focus(&this.focus_handle, cx);
                // Let the workspace retire any prior sidebar/input selection;
                // the IDE root preserves this surface's keyboard focus.
                cx.notify();
            }),
        );
        div()
            .relative()
            .min_w_0()
            .debug_selector(move || format!("ide-markdown-fragment-{}", key.key))
            .child(styled)
            .child(hit)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    if event.click_count != 1 || this.workspace.active_tab() != Some(tab) {
                        return;
                    }
                    let Some(selection) = this
                        .markdown_previews
                        .get(&tab)
                        .and_then(|state| state.selection.as_ref())
                    else {
                        return;
                    };
                    if !selection.range().is_empty() {
                        return;
                    }
                    if let Ok(index) = layout.index_for_position(event.position)
                        && let Some(link) = links.iter().find(|link| link.range.contains(&index))
                    {
                        (link.open)(window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn drag_markdown_selection(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.is_markdown_preview() {
            return;
        }
        let Some(tab) = self.active_markdown_tab() else {
            return;
        };
        let Some(selection) = self
            .markdown_previews
            .get_mut(&tab)
            .and_then(|state| state.selection.as_mut())
        else {
            return;
        };
        if !selection.dragging {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            selection.dragging = false;
            return;
        }
        if let Some(index) = selection.index_at(event.position)
            && selection.head != index
        {
            selection.head = index;
            cx.notify();
        }
    }

    fn finish_markdown_selection(&mut self) {
        if let Some(tab) = self.active_markdown_tab()
            && let Some(selection) = self
                .markdown_previews
                .get_mut(&tab)
                .and_then(|state| state.selection.as_mut())
        {
            selection.dragging = false;
        }
    }

    fn copy_markdown_selection(&self, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_markdown_tab()
            && let Some(selection) = self
                .markdown_previews
                .get(&tab)
                .and_then(|state| state.selection.as_ref())
            && !selection.range().is_empty()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(
                selection.text[selection.range()].to_string(),
            ));
        }
    }

    fn select_all_markdown(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_markdown_tab()
            && let Some(selection) = self
                .markdown_previews
                .get_mut(&tab)
                .and_then(|state| state.selection.as_mut())
        {
            selection.anchor = 0;
            selection.head = selection.text.len();
            selection.dragging = false;
            cx.notify();
        }
    }
}
