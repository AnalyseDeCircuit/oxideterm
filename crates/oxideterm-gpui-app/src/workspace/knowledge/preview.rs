use super::*;
use oxideterm_gpui_editor::{EditorScrollAnchor, EditorScrollOrigin, EditorViewportChanged};
use oxideterm_gpui_markdown::scroll_sync::SourceAnchor;

impl KnowledgeDocumentEditor {
    pub(super) fn render_source_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .h(px(self.tokens.metrics.ui_button_lg_height))
            .flex_none()
            .min_w_0()
            .overflow_x_scrollbar()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(
                div()
                    .h_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(16.0))
                    .child(self.render_format_button(
                        "knowledge-format-undo",
                        KnowledgeFormatGlyph::Icon(LucideIcon::RotateCcw),
                        self.labels.format_undo.clone(),
                        KnowledgeFormatAction::Undo,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-redo",
                        KnowledgeFormatGlyph::Icon(LucideIcon::RefreshCw),
                        self.labels.format_redo.clone(),
                        KnowledgeFormatAction::Redo,
                        cx,
                    ))
                    .child(self.render_format_separator())
                    .child(self.render_format_button(
                        "knowledge-format-heading-1",
                        KnowledgeFormatGlyph::Text("H1"),
                        format!("{} 1", self.labels.format_heading),
                        KnowledgeFormatAction::Heading(1),
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-heading-2",
                        KnowledgeFormatGlyph::Text("H2"),
                        format!("{} 2", self.labels.format_heading),
                        KnowledgeFormatAction::Heading(2),
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-heading-3",
                        KnowledgeFormatGlyph::Text("H3"),
                        format!("{} 3", self.labels.format_heading),
                        KnowledgeFormatAction::Heading(3),
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-heading-4",
                        KnowledgeFormatGlyph::Text("H4"),
                        format!("{} 4", self.labels.format_heading),
                        KnowledgeFormatAction::Heading(4),
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-heading-5",
                        KnowledgeFormatGlyph::Text("H5"),
                        format!("{} 5", self.labels.format_heading),
                        KnowledgeFormatAction::Heading(5),
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-heading-6",
                        KnowledgeFormatGlyph::Text("H6"),
                        format!("{} 6", self.labels.format_heading),
                        KnowledgeFormatAction::Heading(6),
                        cx,
                    ))
                    .child(self.render_format_separator())
                    .child(self.render_format_button(
                        "knowledge-format-bold",
                        KnowledgeFormatGlyph::Text("B"),
                        self.labels.format_bold.clone(),
                        KnowledgeFormatAction::Bold,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-italic",
                        KnowledgeFormatGlyph::Text("I"),
                        self.labels.format_italic.clone(),
                        KnowledgeFormatAction::Italic,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-strike",
                        KnowledgeFormatGlyph::Text("S"),
                        self.labels.format_strike.clone(),
                        KnowledgeFormatAction::Strike,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-code",
                        KnowledgeFormatGlyph::Text("<>"),
                        self.labels.format_inline_code.clone(),
                        KnowledgeFormatAction::InlineCode,
                        cx,
                    ))
                    .child(self.render_format_separator())
                    .child(self.render_format_button(
                        "knowledge-format-quote",
                        KnowledgeFormatGlyph::Text("”"),
                        self.labels.format_quote.clone(),
                        KnowledgeFormatAction::Quote,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-bullets",
                        KnowledgeFormatGlyph::Text("•"),
                        self.labels.format_bullet_list.clone(),
                        KnowledgeFormatAction::BulletList,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-ordered",
                        KnowledgeFormatGlyph::Text("1."),
                        self.labels.format_ordered_list.clone(),
                        KnowledgeFormatAction::OrderedList,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-task",
                        KnowledgeFormatGlyph::Icon(LucideIcon::ListChecks),
                        self.labels.format_task_list.clone(),
                        KnowledgeFormatAction::TaskList,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-code-block",
                        KnowledgeFormatGlyph::Icon(LucideIcon::Code2),
                        self.labels.format_code_block.clone(),
                        KnowledgeFormatAction::CodeBlock,
                        cx,
                    ))
                    .child(self.render_format_separator())
                    .child(self.render_format_button(
                        "knowledge-format-link",
                        KnowledgeFormatGlyph::Icon(LucideIcon::Link2),
                        self.labels.format_link.clone(),
                        KnowledgeFormatAction::Link,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-image",
                        KnowledgeFormatGlyph::Icon(LucideIcon::Image),
                        self.labels.format_image.clone(),
                        KnowledgeFormatAction::Image,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-table",
                        KnowledgeFormatGlyph::Icon(LucideIcon::FileSpreadsheet),
                        self.labels.format_table.clone(),
                        KnowledgeFormatAction::Table,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-horizontal-rule",
                        KnowledgeFormatGlyph::Text("—"),
                        self.labels.format_horizontal_rule.clone(),
                        KnowledgeFormatAction::HorizontalRule,
                        cx,
                    ))
                    .child(self.render_format_separator())
                    .child(self.render_format_button(
                        "knowledge-format-inline-math",
                        KnowledgeFormatGlyph::Text("$"),
                        self.labels.format_inline_math.clone(),
                        KnowledgeFormatAction::InlineMath,
                        cx,
                    ))
                    .child(self.render_format_button(
                        "knowledge-format-display-math",
                        KnowledgeFormatGlyph::Text("$$"),
                        self.labels.format_display_math.clone(),
                        KnowledgeFormatAction::DisplayMath,
                        cx,
                    )),
            )
            .into_any_element()
    }
    pub(super) fn initialize_preview(
        &mut self,
        preferences: oxideterm_settings::KnowledgeEditorUiState,
        cx: &mut Context<Self>,
    ) {
        if self.is_markdown {
            self.mode = preferences.mode;
            self.previous_mode = preferences.mode;
            self.source_ratio = if preferences.source_ratio.is_finite() {
                preferences.source_ratio.clamp(0.2, 0.8)
            } else {
                0.5
            };
        }
        self.editor.update(cx, |editor, _| {
            editor.set_read_only(self.mode == KnowledgeEditorMode::Preview)
        });
        self._viewport_subscription = Some(cx.subscribe(
            &self.editor,
            |this, _, event: &EditorViewportChanged, cx| {
                if this.mode != KnowledgeEditorMode::Split
                    || event.origin == EditorScrollOrigin::Synchronization
                {
                    return;
                }
                if event.origin == EditorScrollOrigin::User {
                    this.preview_leads = false;
                }
                if !this.preview_leads {
                    this.sync_from_editor(cx);
                }
            },
        ));
        let weak = cx.entity().downgrade();
        self.preview_scroll
            .scroll_sync
            .set_callback(move |anchor, _, app| {
                let _ = weak.update(app, |this, cx| {
                    if this.mode != KnowledgeEditorMode::Split
                        || anchor.version != this.observed_buffer_version
                    {
                        return;
                    }
                    this.preview_leads = true;
                    this.editor.update(cx, |editor, cx| {
                        editor.scroll_to_source(
                            EditorScrollAnchor {
                                version: anchor.version,
                                source_position: anchor.position,
                                viewport_y: anchor.viewport_y,
                                at_start: anchor.at_start,
                                at_end: anchor.at_end,
                            },
                            cx,
                        )
                    });
                });
            });
        if self.mode != KnowledgeEditorMode::Source && self.is_markdown {
            self.load_preview(cx);
        }
    }

    pub(super) fn emit_preferences(&self, cx: &mut Context<Self>) {
        if self.is_markdown {
            cx.emit(KnowledgeDocumentEditorEvent::PreferencesChanged(
                oxideterm_settings::KnowledgeEditorUiState {
                    mode: self.mode,
                    source_ratio: self.source_ratio,
                },
            ));
        }
    }

    pub(super) fn remap_preview_anchor(&mut self, changes: &[oxideterm_editor_core::BufferChange]) {
        let mut anchor = self
            .pending_preview_anchor
            .or_else(|| self.preview_scroll.scroll_sync.anchor());
        if let Some(anchor) = &mut anchor {
            for change in changes {
                if anchor.version == change.before_version {
                    anchor.position = change.map_position(anchor.position);
                    anchor.version = change.after_version;
                }
            }
        }
        self.pending_preview_anchor = anchor;
        self.preview_leads = false;
    }

    pub(super) fn sync_from_editor(&mut self, cx: &mut Context<Self>) {
        let anchor = self.editor.read(cx).scroll_anchor();
        let anchor = SourceAnchor {
            version: anchor.version,
            position: anchor.source_position,
            viewport_y: anchor.viewport_y,
            at_start: anchor.at_start,
            at_end: anchor.at_end,
        };
        self.pending_preview_anchor = Some(anchor);
        self.preview_scroll.scroll_sync.request(anchor);
        cx.notify();
    }

    pub(super) fn schedule_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_timer = Some(cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(120)).await;
            let _ = weak.update(cx, |this, cx| {
                this.preview_timer = None;
                this.load_preview(cx);
            });
        }));
    }

    pub(super) fn load_preview(&mut self, cx: &mut Context<Self>) {
        if self.preview_running {
            return;
        }
        self.preview_running = true;
        let source = self.draft.clone();
        let version = self.observed_buffer_version;
        self.preview_task = Some(cx.spawn(async move |weak, cx| {
            let document =
                cx.background_executor()
                    .spawn(async move {
                        oxideterm_gpui_markdown::parser::parse_with_source_ranges(&source)
                    })
                    .await;
            let _ = weak.update(cx, |this, cx| {
                this.preview_running = false;
                if this.observed_buffer_version == version {
                    this.preview_document = Some(document);
                    this.preview_version = version;
                    this.preview_scroll.scroll_sync.set_version(version);
                    if this.mode == KnowledgeEditorMode::Split && !this.preview_leads {
                        this.sync_from_editor(cx);
                    } else if let Some(anchor) = this.pending_preview_anchor.take() {
                        this.preview_scroll.scroll_sync.request(anchor);
                    }
                    cx.notify();
                } else if this.preview_timer.is_none() && this.mode != KnowledgeEditorMode::Source {
                    this.load_preview(cx);
                }
            });
        }));
    }

    pub(super) fn resize_split(&mut self, x: f32, cx: &mut Context<Self>) {
        if !self.split_dragging {
            return;
        }
        if let Some(bounds) = self.split_bounds {
            self.source_ratio = ((x - f32::from(bounds.origin.x))
                / f32::from(bounds.size.width).max(1.0))
            .clamp(0.2, 0.8);
            cx.notify();
        }
    }

    pub(super) fn finish_split_resize(&mut self, cx: &mut Context<Self>) {
        if self.split_dragging {
            self.split_dragging = false;
            self.emit_preferences(cx);
        }
    }
}
