use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{App, ScrollHandle, Window, point, px};

use crate::{
    layout::MarkdownBlockLayout,
    model::{Block, Inline, MarkdownDocument},
    options::MarkdownOptions,
};

#[derive(Clone, Debug)]
pub struct MarkdownNavigation {
    scroll: ScrollHandle,
    state: Rc<RefCell<NavigationState>>,
}

#[derive(Default, Debug)]
struct NavigationState {
    offsets: HashMap<String, f32>,
    pending: Option<String>,
    ancestors: HashMap<String, Vec<String>>,
    returns: HashMap<String, String>,
    disclosures: crate::disclosure::DisclosureState,
}

impl MarkdownNavigation {
    pub fn new(scroll: ScrollHandle) -> Self {
        Self {
            scroll,
            state: Default::default(),
        }
    }

    pub(crate) fn prepare(&self, document: &MarkdownDocument, opts: &MarkdownOptions) {
        let layout = MarkdownBlockLayout::from_document(document, opts);
        let mut state = self.state.borrow_mut();
        state.disclosures = opts.disclosures.clone();
        state.ancestors.clear();
        let mut top = 0.0;
        for (block, size) in document.blocks.iter().zip(layout.item_sizes().iter()) {
            register_targets(block, top, &[], &mut state);
            top += f32::from(size.height) + opts.block_gap;
        }
        for (index, footnote) in document.footnotes.iter().enumerate() {
            register_target(format!("fn:{}", index + 1), top, &[], &mut state);
            for block in &footnote.blocks {
                register_targets(block, top, &[], &mut state);
            }
        }
    }

    pub(crate) fn open(&self, fragment: &str, window: &mut Window) {
        let Some(encoded) = fragment.strip_prefix('#') else {
            return;
        };
        let Some(id) = decode_fragment(encoded) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        if let Some(top) = state.select_target(&id) {
            self.scroll.set_offset(point(px(0.0), px(-top)));
            window.refresh();
        }
    }

    pub(crate) fn measure(&self, id: &str, y: f32, window: &mut Window, cx: &mut App) {
        let top = y - f32::from(self.scroll.bounds().origin.y) - f32::from(self.scroll.offset().y);
        let mut state = self.state.borrow_mut();
        state.offsets.insert(id.to_string(), top);
        if state.pending.as_deref() == Some(id) {
            state.pending = None;
            let scroll = self.scroll.clone();
            window.defer(cx, move |window, _| {
                scroll.set_offset(point(px(0.0), px(-top)));
                window.refresh();
            });
        }
    }
}

impl NavigationState {
    fn select_target(&mut self, id: &str) -> Option<f32> {
        let id = self.destination(id);
        let parents = self.ancestors.get(&id)?;
        let top = *self.offsets.get(&id)?;
        self.disclosures.open(parents);
        self.pending = Some(id);
        Some(top)
    }
    fn destination(&mut self, id: &str) -> String {
        if let Some((target, occurrence)) = id.split_once(":from:")
            && target.starts_with("fn:")
        {
            self.returns.insert(
                target.to_owned(),
                format!("fnref:{}:{occurrence}", &target[3..]),
            );
            target.to_owned()
        } else if let Some(index) = id.strip_prefix("fnback:") {
            self.returns
                .get(&format!("fn:{index}"))
                .cloned()
                .unwrap_or_else(|| format!("fnref:{index}:1"))
        } else {
            id.to_owned()
        }
    }
}

fn register_target(id: String, top: f32, parents: &[String], state: &mut NavigationState) {
    state.offsets.entry(id.clone()).or_insert(top);
    state.ancestors.insert(id, parents.to_vec());
}

fn register_inlines(inlines: &[Inline], top: f32, parents: &[String], state: &mut NavigationState) {
    for inline in inlines {
        match inline {
            Inline::FootnoteReference {
                index, occurrence, ..
            } => register_target(format!("fnref:{index}:{occurrence}"), top, parents, state),
            Inline::Bold(children)
            | Inline::Italic(children)
            | Inline::Strikethrough(children)
            | Inline::Kbd(children)
            | Inline::Subscript(children)
            | Inline::Superscript(children)
            | Inline::Underline(children)
            | Inline::Highlight(children)
            | Inline::Link { text: children, .. } => {
                register_inlines(children, top, parents, state)
            }
            _ => {}
        }
    }
}

fn register_targets(block: &Block, top: f32, parents: &[String], state: &mut NavigationState) {
    match block {
        Block::Located { block, .. } => register_targets(block, top, parents, state),
        Block::Heading { id, inlines, .. } => {
            register_target(id.clone(), top, parents, state);
            register_inlines(inlines, top, parents, state);
        }
        Block::Paragraph { inlines } => register_inlines(inlines, top, parents, state),
        Block::Blockquote { blocks, .. } | Block::HtmlContainer { blocks, .. } => {
            for block in blocks {
                register_targets(block, top, parents, state);
            }
        }
        Block::Details {
            id,
            blocks,
            summary,
            ..
        } => {
            register_inlines(summary, top, parents, state);
            let mut nested = parents.to_vec();
            nested.push(id.clone());
            for block in blocks {
                register_targets(block, top, &nested, state);
            }
        }
        Block::UnorderedList { items } | Block::OrderedList { items, .. } => {
            for item in items {
                register_inlines(&item.inlines, top, parents, state);
                for block in &item.children {
                    register_targets(block, top, parents, state);
                }
            }
        }
        Block::Table { headers, rows, .. } => {
            for cell in headers.iter().chain(rows.iter().flatten()) {
                for block in cell {
                    register_targets(block, top, parents, state);
                }
            }
        }
        _ => {}
    }
}

fn decode_fragment(value: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut source = value.bytes();
    while let Some(byte) = source.next() {
        bytes.push(if byte == b'%' {
            let high = (source.next()? as char).to_digit(16)?;
            let low = (source.next()? as char).to_digit(16)?;
            (high * 16 + low) as u8
        } else {
            byte
        });
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_opens_all_disclosures_and_returns_to_the_clicked_reference() {
        let doc = crate::parser::parse(
            "First[^n], second[^n].\n\n<details><summary>A</summary><details><summary>B</summary><h2 id='hidden'>Hidden</h2></details></details>\n\n[^n]: Note",
        );
        let opts = MarkdownOptions::default();
        let navigation = MarkdownNavigation::new(ScrollHandle::new());
        navigation.prepare(&doc, &opts);
        let mut state = navigation.state.borrow_mut();
        assert_eq!(
            state.ancestors["hidden"],
            vec!["html-details", "html-details-2"]
        );
        state.select_target("hidden");
        assert!(opts.disclosures.is_open("html-details", false));
        assert!(opts.disclosures.is_open("html-details-2", false));
        state.select_target("fn:1:from:2");
        assert_eq!(state.pending.as_deref(), Some("fn:1"));
        state.select_target("fnback:1");
        assert_eq!(state.pending.as_deref(), Some("fnref:1:2"));
    }

    #[test]
    fn heading_targets_include_nested_sections_and_decode_chinese_fragments() {
        let document = crate::parser::parse("# 开始\n\n> ## 嵌套\n\n# 开始");
        let navigation = MarkdownNavigation::new(ScrollHandle::new());
        navigation.prepare(&document, &MarkdownOptions::default());
        let state = navigation.state.borrow();
        let mut targets = state.offsets.keys().map(String::as_str).collect::<Vec<_>>();
        targets.sort_unstable();
        assert_eq!(targets, ["嵌套", "开始", "开始-2"]);
        assert_eq!(
            decode_fragment("%E5%BC%80%E5%A7%8B").as_deref(),
            Some("开始")
        );
        assert_eq!(decode_fragment("broken%2"), None);
    }
}
