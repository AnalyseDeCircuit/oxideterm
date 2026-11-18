use super::*;

impl FileManagerView {
    pub(super) fn render_file_manager_preview_markdown(
        &self,
        content: &str,
        source_path: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let opts = self
            .localized_markdown_options()
            .with_source_path(source_path);
        let code_actions = self.markdown_mermaid_actions(cx);
        let (document, markdown_scroll) = self.file_manager.update(cx, |state, _| {
            let owner = state
                .preview
                .as_ref()
                .expect("Markdown preview payload")
                .clone();
            let document = state.preview_markdown_cache.document(
                &owner,
                content,
                opts.enable_smart_punctuation,
                &mut state.preview_markdown_scroll,
            );
            (document, state.preview_markdown_scroll.clone())
        });
        div()
            .size_full()
            .p(px(16.0))
            .child(
                oxideterm_gpui_markdown::render::render_document_virtual_with_code_actions(
                    "file-manager-preview-markdown-virtual",
                    &document,
                    &self.tokens,
                    &opts,
                    &markdown_scroll,
                    Some(&code_actions),
                ),
            )
            .into_any_element()
    }

    pub(super) fn render_file_manager_preview_code(
        &self,
        content: &str,
        language: Option<&str>,
        filename: &str,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if oxideterm_gpui_editor::detect_language(Some(std::path::Path::new(filename)), content, cx)
            .is_some()
        {
            let existing = self.file_manager.read(cx).preview_editor.clone();
            let editor = existing.unwrap_or_else(|| {
                let tokens = self.tokens;
                let settings = &self.editor;
                let labels = EditorContextMenuLabels {
                    copy: self.i18n.t("menu.copy"),
                    cut: self.i18n.t("fileManager.cut"),
                    paste: self.i18n.t("menu.paste"),
                    select_all: self.i18n.t("fileManager.selectAll"),
                };
                let editor = cx.new(|cx| {
                    let mut editor = TextEditorView::new(content, &tokens, cx);
                    editor.set_read_only(true);
                    editor.set_context_menu_labels(labels);
                    editor.apply_ide_runtime_settings(
                        &tokens,
                        settings.editor_font_family.clone(),
                        settings.editor_font_weight,
                        settings.editor_font_fallback.clone(),
                        settings.editor_font_size,
                        settings.editor_line_height,
                        settings.word_wrap,
                        settings.background_active,
                        cx,
                    );
                    editor.set_language_from_path(filename, None, cx);
                    editor
                });
                self.file_manager.update(cx, |state, _| {
                    // Retain the editor so grammar updates preserve selection and scroll position.
                    state.preview_editor = Some(editor.clone());
                });
                editor
            });
            return div().size_full().child(editor).into_any_element();
        }
        if content.is_empty() {
            return self.render_file_manager_preview_text_status(
                &self.i18n.t("fileManager.emptyFile"),
                cx,
            );
        }
        let theme = self.tokens.ui;
        let opts = MarkdownOptions::from_theme(&self.tokens);
        let language = language
            .filter(|language| !language.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| file_manager_preview_language_for_name(filename))
            .to_ascii_lowercase();
        let lines = Arc::new(file_manager_preview_visual_lines(content));
        let row_count = lines.len();
        let list_lines = lines;
        let font_family = self.mono_font_family.clone();
        let font_size = self.mono_font_size;
        let row_height = font_size * 1.5;
        let scroll = self.file_manager.read(cx).preview_code_scroll.clone();
        div()
            .size_full()
            .bg(file_manager_bg(theme.bg_sunken, has_background))
            .child(
                div().size_full().p(px(16.0)).child(
                    tauri_virtual_uniform_list(
                        "file-manager-preview-code-virtual",
                        row_count,
                        scroll,
                        TauriVirtualListSpec::new(
                            px(row_height),
                            FILE_MANAGER_PREVIEW_CODE_OVERSCAN,
                        ),
                        move |range, _window, _cx| {
                            let opts = opts.clone();
                            let language = language.clone();
                            let font_family = font_family.clone();
                            range
                                .map(|index| {
                                    let line = &list_lines[index];
                                    let content: AnyElement = if language != "text"
                                        && language != "plain"
                                        && let Some(runs) = highlight::highlight_code(
                                            &language,
                                            &line.content,
                                            &opts,
                                        ) {
                                        let (text, text_runs) =
                                            highlight::highlighted_runs_to_text_runs(&runs);
                                        StyledText::new(text)
                                            .with_runs(text_runs)
                                            .into_any_element()
                                    } else {
                                        SharedString::from(line.content.clone()).into_any_element()
                                    };
                                    div()
                                        .h(px(row_height))
                                        .w_full()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .font_family(font_family.clone())
                                        .text_size(px(font_size))
                                        .line_height(px(row_height))
                                        .text_color(rgb(theme.text))
                                        .child(
                                            div()
                                                .w(px(48.0))
                                                .flex_none()
                                                .pr(px(12.0))
                                                .text_align(gpui::TextAlign::Right)
                                                .text_color(rgba(
                                                    (theme.text_muted << 8)
                                                        | FILE_MANAGER_PREVIEW_CODE_GUTTER_ALPHA,
                                                ))
                                                .child(
                                                    line.line_number
                                                        .map(|line_number| line_number.to_string())
                                                        .unwrap_or_default(),
                                                ),
                                        )
                                        .child(div().flex_1().min_w(px(0.0)).child(content))
                                        .into_any_element()
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .on_scroll(|_, _, cx| cx.stop_propagation()),
                ),
            )
            .into_any_element()
    }

    pub(super) fn render_file_manager_preview_stream_code(
        &self,
        language: Option<&str>,
        filename: &str,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (stream_language, lines, error, eof, scroll) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.preview_stream.language.clone(),
                Arc::new(file_manager.preview_stream.lines.clone()),
                file_manager.preview_stream.error.clone(),
                file_manager.preview_stream.eof,
                file_manager.preview_code_scroll.clone(),
            )
        };
        if lines.is_empty() && eof && error.is_none() {
            return self.render_file_manager_preview_text_status(
                &self.i18n.t("fileManager.emptyFile"),
                cx,
            );
        }

        let theme = self.tokens.ui;
        let opts = MarkdownOptions::from_theme(&self.tokens);
        let language = stream_language
            .as_deref()
            .or(language)
            .filter(|language| !language.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| file_manager_preview_language_for_name(filename))
            .to_ascii_lowercase();
        let row_count = lines.len() + usize::from(!eof || error.is_some());
        let notice = oxideterm_gpui_editor::detect_language(
            Some(std::path::Path::new(filename)),
            lines.first().map(String::as_str).unwrap_or_default(),
            cx,
        )
        .and_then(|language| {
            oxideterm_gpui_editor::render_language_plugin_notice(language, &self.tokens, cx)
        });
        let font_family = self.mono_font_family.clone();
        let font_size = self.mono_font_size;
        let row_height = font_size * 1.5;
        let body = div()
            .size_full()
            .bg(file_manager_bg(theme.bg_sunken, has_background))
            .child(
                div().size_full().p(px(16.0)).overflow_x_scrollbar().child(
                    tauri_virtual_uniform_list(
                        "file-manager-preview-stream-code-virtual",
                        row_count.max(1),
                        scroll,
                        TauriVirtualListSpec::new(
                            px(row_height),
                            FILE_MANAGER_PREVIEW_CODE_OVERSCAN,
                        ),
                        move |range, _window, _cx| {
                            let opts = opts.clone();
                            let language = language.clone();
                            let font_family = font_family.clone();
                            let lines = lines.clone();
                            let error = error.clone();
                            range
                                .map(|index| {
                                    if index >= lines.len() {
                                        let message = error
                                            .clone()
                                            .unwrap_or_else(|| "Loading...".to_string());
                                        return div()
                                            .h(px(row_height))
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .font_family(font_family.clone())
                                            .text_size(px(font_size))
                                            .line_height(px(row_height))
                                            .text_color(rgb(theme.text_muted))
                                            .child(message)
                                            .into_any_element();
                                    }

                                    let line = &lines[index];
                                    let content: AnyElement = if language != "text"
                                        && language != "plain"
                                        && let Some(runs) =
                                            highlight::highlight_code(&language, line, &opts)
                                    {
                                        let (text, text_runs) =
                                            highlight::highlighted_runs_to_text_runs(&runs);
                                        StyledText::new(text)
                                            .with_runs(text_runs)
                                            .into_any_element()
                                    } else {
                                        SharedString::from(line.clone()).into_any_element()
                                    };
                                    div()
                                        .h(px(row_height))
                                        .w_full()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .font_family(font_family.clone())
                                        .text_size(px(font_size))
                                        .line_height(px(row_height))
                                        .text_color(rgb(theme.text))
                                        .child(
                                            div()
                                                .w(px(48.0))
                                                .flex_none()
                                                .pr(px(12.0))
                                                .text_align(gpui::TextAlign::Right)
                                                .text_color(rgba(
                                                    (theme.text_muted << 8)
                                                        | FILE_MANAGER_PREVIEW_CODE_GUTTER_ALPHA,
                                                ))
                                                .child((index + 1).to_string()),
                                        )
                                        .child(div().min_w(px(0.0)).child(content))
                                        .into_any_element()
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .on_scroll(cx.listener(|this, _event, _window, cx| {
                        this.load_more_file_manager_stream_preview(cx);
                        cx.stop_propagation();
                    })),
                ),
            )
            .into_any_element();
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(notice)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}
