use super::*;

/// Visit the selectable text and fragment identities without constructing UI elements.
pub fn visit_selectable_text(
    document: &MarkdownDocument,
    opts: &MarkdownOptions,
    visit: &mut impl FnMut(MarkdownTextFragmentId, &str),
) {
    visit_blocks(&document.blocks, "b", visit);
    if opts.enable_footnotes {
        for (index, footnote) in document.footnotes.iter().enumerate() {
            visit(
                format!("footnote:{index}:label").into(),
                &format!("[{}] ↩", index + 1),
            );
            visit_blocks(&footnote.blocks, &format!("footnote:{index}:body"), visit);
        }
    }
}

fn visit_blocks(
    blocks: &[Block],
    path: &str,
    visit: &mut impl FnMut(MarkdownTextFragmentId, &str),
) {
    for (index, block) in blocks.iter().enumerate() {
        visit_block(block, &format!("{path}:{index}"), visit);
    }
}

fn visit_block(block: &Block, path: &str, visit: &mut impl FnMut(MarkdownTextFragmentId, &str)) {
    match block.unlocated() {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines } => {
            visit_inlines(inlines, path, visit);
        }
        Block::Html(html) => visit_inlines(&[Inline::Html(html.clone())], path, visit),
        Block::HtmlContainer { blocks, .. } => visit_blocks(blocks, path, visit),
        Block::Details { blocks, .. } => visit_blocks(blocks, &format!("{path}:details"), visit),
        Block::Blockquote { blocks, .. } => visit_blocks(blocks, &format!("{path}:quote"), visit),
        Block::CodeBlock { language, code } => {
            if !should_render_mermaid_block(language.as_deref(), code) {
                visit(format!("{path}:code").into(), code_display_text(code));
            }
        }
        Block::UnorderedList { items } | Block::OrderedList { items, .. } => {
            for (index, item) in items.iter().enumerate() {
                let path = format!("{path}:li:{index}");
                visit_inlines(&item.inlines, &path, visit);
                visit_blocks(&item.children, &format!("{path}:child"), visit);
            }
        }
        Block::Table { headers, rows, .. } => {
            for (column, blocks) in headers.iter().enumerate() {
                visit_blocks(blocks, &format!("{path}:th:{column}"), visit);
            }
            for (row, cells) in rows.iter().enumerate() {
                for (column, blocks) in cells.iter().enumerate() {
                    visit_blocks(blocks, &format!("{path}:td:{row}:{column}"), visit);
                }
            }
        }
        Block::HorizontalRule => {}
        Block::Located { block, .. } => visit_block(block, path, visit),
    }
}

fn visit_inlines(
    inlines: &[Inline],
    path: &str,
    visit: &mut impl FnMut(MarkdownTextFragmentId, &str),
) {
    let mut flat = Vec::new();
    collect_runs(inlines, FlatRunStyle::default(), &mut flat);
    if !flat
        .iter()
        .any(|run| run.image_url.is_some() || run.math_latex.is_some())
    {
        let text = flat.iter().map(|run| run.text.as_str()).collect::<String>();
        visit(path.to_owned().into(), &text);
        return;
    }
    let breaks = inline_break_offsets(&flat);
    let mut cursor = 0;
    let mut part = 0;
    let mut previous_hard_break = false;
    let mut runs = flat.as_slice();
    while !runs.is_empty() {
        let count = runs
            .iter()
            .take_while(|run| run.image_url.is_none() && run.math_latex.is_none())
            .count();
        if count > 0 {
            let text = runs[..count]
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>();
            for range in inline_text_segments(&text, cursor, &breaks) {
                let hard_break = text[range.clone()].ends_with('\n');
                let end = range.end - usize::from(hard_break);
                visit(
                    MarkdownTextFragmentId {
                        key: format!("{path}:text:{part}"),
                        join_previous: part > 0 && !previous_hard_break,
                    },
                    &text[range.start..end],
                );
                part += 1;
                previous_hard_break = hard_break;
            }
            cursor += text.len();
            runs = &runs[count..];
        } else {
            if runs[0].math_latex.is_some() && runs[0].math_display {
                previous_hard_break = true;
            }
            cursor += '\u{fffc}'.len_utf8();
            runs = &runs[1..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn index_matches_rendered_fragments_without_building_an_element_tree(
        cx: &mut gpui::TestAppContext,
    ) {
        let source = "# Title\n\nAlpha **bold** 与🙂. ![alt](https://example.com/x.png) after $x$ end.  \nNext.\n\n- first\n  - nested\n\n> quote\n\n| Head |\n| --- |\n| cell |\n\n```text\ncode\n```\n\n<details><summary>Summary</summary><p>Body</p></details>\n\nFootnote[^x].\n\n[^x]: note";
        let document = crate::parser::parse(source);
        let opts = MarkdownOptions::default();
        let mut indexed = Vec::new();
        visit_selectable_text(&document, &opts, &mut |key, text| {
            indexed.push((key, text.to_string()))
        });
        assert_eq!(indexed[0].1, "Title");
        assert!(indexed.iter().any(|(_, text)| text == "code"));
        assert!(indexed.iter().any(|(_, text)| text == "Body"));
        assert!(indexed.iter().any(|(_, text)| text == "note"));
        // Render inside a real frame so its temporary elements are released.
        struct RenderFixture {
            document: MarkdownDocument,
            options: MarkdownOptions,
            fragments: Rc<std::cell::RefCell<Vec<(MarkdownTextFragmentId, String)>>>,
        }
        impl gpui::Render for RenderFixture {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                self.fragments.borrow_mut().clear();
                render_document_selectable(
                    &self.document,
                    &oxideterm_theme::default_tokens(),
                    &self.options,
                    &mut |key, text, _, _| {
                        self.fragments.borrow_mut().push((key, text.to_string()));
                        div().into_any_element()
                    },
                )
            }
        }
        let rendered = Rc::new(std::cell::RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view(|_, _| RenderFixture {
            document,
            options: opts,
            fragments: rendered.clone(),
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(indexed, *rendered.borrow());
    }
}
