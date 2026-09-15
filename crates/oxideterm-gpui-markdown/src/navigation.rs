use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{App, ScrollHandle, Window, point, px};

use crate::{
    layout::MarkdownBlockLayout,
    model::{Block, MarkdownDocument},
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
        let mut top = 0.0;
        for (block, size) in document.blocks.iter().zip(layout.item_sizes().iter()) {
            register_headings(block, top, &mut state.offsets);
            top += f32::from(size.height) + opts.block_gap;
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
        if let Some(&top) = state.offsets.get(&id) {
            state.pending = Some(id);
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

fn register_headings(block: &Block, top: f32, offsets: &mut HashMap<String, f32>) {
    match block {
        Block::Heading { id, .. } => {
            offsets.entry(id.clone()).or_insert(top);
        }
        Block::Blockquote { blocks, .. } | Block::HtmlContainer { blocks, .. } => {
            for block in blocks {
                register_headings(block, top, offsets);
            }
        }
        Block::UnorderedList { items } | Block::OrderedList { items, .. } => {
            for item in items {
                for block in &item.children {
                    register_headings(block, top, offsets);
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
