// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Convert browser-compatible HTML5 fragments into OxideTerm-owned markdown nodes.
//!
//! Parsing and rendering are intentionally separate security boundaries. `scraper`
//! provides HTML5 error recovery, while this module accepts only explicit native
//! semantics and never forwards a DOM, attributes, scripts, or CSS to GPUI.

use ego_tree::{NodeRef, iter::Edge};
use scraper::{ElementRef, Html, Node};

use crate::model::{
    Block, BlockAlignment, ImageDimensions, ImageLength, Inline, ListItem, TableAlignment,
};

pub(crate) const MAX_HTML_NESTING_DEPTH: usize = 128;

pub(crate) enum DisclosureBoundary {
    Open(bool),
    Close,
    SummaryOpen,
    SummaryClose,
}

pub(crate) fn has_enclosing_html_container(source: &str) -> bool {
    Html::parse_fragment(source)
        .root_element()
        .child_elements()
        .next()
        .is_some_and(|element| !matches!(element.value().name(), "details" | "summary"))
}

pub(crate) fn disclosure_boundaries(
    source: &str,
) -> Vec<(std::ops::Range<usize>, DisclosureBoundary)> {
    let mut emitter = html5gum::DefaultEmitter::<usize>::new_with_span();
    emitter.naively_switch_states(true);
    html5gum::Tokenizer::new_with_emitter(source, emitter)
        .filter_map(|token| match token.ok()? {
            html5gum::Token::StartTag(tag) if tag.name.as_slice() == b"details" => Some((
                tag.span.start..tag.span.end,
                DisclosureBoundary::Open(
                    tag.attributes.keys().any(|key| key.as_slice() == b"open"),
                ),
            )),
            html5gum::Token::EndTag(tag) if tag.name.as_slice() == b"details" => {
                Some((tag.span.start..tag.span.end, DisclosureBoundary::Close))
            }
            html5gum::Token::StartTag(tag) if tag.name.as_slice() == b"summary" => Some((
                tag.span.start..tag.span.end,
                DisclosureBoundary::SummaryOpen,
            )),
            html5gum::Token::EndTag(tag) if tag.name.as_slice() == b"summary" => Some((
                tag.span.start..tag.span.end,
                DisclosureBoundary::SummaryClose,
            )),
            _ => None,
        })
        .collect()
}

pub(crate) fn summary_inlines(source: &str) -> Vec<Inline> {
    element_children_to_inlines(Html::parse_fragment(source).root_element())
}

/// Supported container kinds for inline HTML events emitted around Markdown text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InlineHtmlKind {
    Bold,
    Italic,
    Strikethrough,
    Underline,
    Highlight,
    Code,
    Kbd,
    Subscript,
    Superscript,
    Link,
    Transparent,
}

/// A parsed inline start tag and the only attribute that affects native output.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InlineHtmlOpen {
    pub kind: InlineHtmlKind,
    pub link_url: Option<String>,
}

/// Safe interpretation of one `pulldown-cmark` inline HTML event.
#[derive(Debug, PartialEq)]
pub(crate) enum InlineHtmlEvent {
    Open(InlineHtmlOpen),
    Close(InlineHtmlKind),
    Node(Inline),
    Unsupported,
}

/// Parse one inline HTML event without interpreting arbitrary attributes or CSS.
pub(crate) fn parse_inline_event(source: &str) -> InlineHtmlEvent {
    if let Some(tag_name) = closing_tag_name(source) {
        return inline_kind(tag_name)
            .map(InlineHtmlEvent::Close)
            .unwrap_or(InlineHtmlEvent::Unsupported);
    }

    let fragment = Html::parse_fragment(source);
    let mut elements = fragment.root_element().child_elements();
    let Some(element) = elements.next() else {
        return InlineHtmlEvent::Unsupported;
    };
    if elements.next().is_some() {
        return InlineHtmlEvent::Unsupported;
    }

    match element.value().name() {
        "br" => InlineHtmlEvent::Node(Inline::LineBreak),
        "img" => element
            .attr("src")
            .filter(|url| !url.trim().is_empty())
            .map(|url| {
                InlineHtmlEvent::Node(Inline::Image {
                    alt: element.attr("alt").unwrap_or_default().to_string(),
                    url: url.to_string(),
                    dimensions: image_dimensions(element),
                })
            })
            .unwrap_or(InlineHtmlEvent::Unsupported),
        tag_name => inline_kind(tag_name)
            .map(|kind| {
                let link_url = (kind == InlineHtmlKind::Link)
                    .then(|| element.attr("href"))
                    .flatten()
                    .map(str::to_string);
                InlineHtmlEvent::Open(InlineHtmlOpen {
                    kind: if kind == InlineHtmlKind::Link && link_url.is_none() {
                        InlineHtmlKind::Transparent
                    } else {
                        kind
                    },
                    link_url,
                })
            })
            .unwrap_or(InlineHtmlEvent::Unsupported),
    }
}

/// Parse a complete block HTML fragment and convert its visible safe subset.
pub(crate) fn parse_block_fragment(
    source: &str,
    heading_id_for: &mut dyn FnMut(&[Inline], Option<&str>) -> String,
) -> Vec<Block> {
    let fragment = Html::parse_fragment(source);
    if html_nesting_exceeds_limit(&fragment) {
        // Preserve pathological input as inert source instead of recursively
        // converting a tree deep enough to exhaust the native stack.
        return vec![Block::Html(source.to_string())];
    }
    blocks_from_nodes(fragment.root_element().children().collect(), heading_id_for)
}

fn html_nesting_exceeds_limit(fragment: &Html) -> bool {
    let mut depth = 0usize;
    for edge in fragment.root_element().traverse() {
        match edge {
            Edge::Open(_) => {
                depth = depth.saturating_add(1);
                if depth > MAX_HTML_NESTING_DEPTH {
                    return true;
                }
            }
            Edge::Close(_) => depth = depth.saturating_sub(1),
        }
    }
    false
}

fn blocks_from_nodes(
    nodes: Vec<NodeRef<'_, Node>>,
    heading_id_for: &mut dyn FnMut(&[Inline], Option<&str>) -> String,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut pending_inlines = Vec::new();

    for node in nodes {
        match node.value() {
            Node::Text(text) => push_collapsed_text(&mut pending_inlines, text),
            Node::Element(_) => {
                let element = ElementRef::wrap(node).expect("element node must be wrappable");
                if is_dropped_element(element.value().name()) {
                    continue;
                }
                if is_block_element(element.value().name()) {
                    flush_paragraph(&mut pending_inlines, &mut blocks);
                    blocks.extend(element_to_blocks(element, heading_id_for));
                } else {
                    pending_inlines.extend(element_to_inlines(element));
                }
            }
            Node::Document
            | Node::Fragment
            | Node::Doctype(_)
            | Node::Comment(_)
            | Node::ProcessingInstruction(_) => {}
        }
    }

    flush_paragraph(&mut pending_inlines, &mut blocks);
    blocks
}

fn element_to_blocks(
    element: ElementRef<'_>,
    heading_id_for: &mut dyn FnMut(&[Inline], Option<&str>) -> String,
) -> Vec<Block> {
    let tag_name = element.value().name();
    match tag_name {
        "p" => wrap_alignment(
            element,
            paragraph_from_inlines(element_children_to_inlines(element)),
        ),
        "div" | "section" | "article" | "main" | "header" | "footer" | "nav" | "aside"
        | "figure" => wrap_alignment(
            element,
            blocks_from_nodes(element.children().collect(), heading_id_for),
        ),
        "center" => wrap_blocks(
            BlockAlignment::Center,
            blocks_from_nodes(element.children().collect(), heading_id_for),
        ),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let mut inlines = element_children_to_inlines(element);
            trim_inline_boundaries(&mut inlines);
            if inlines.is_empty() {
                Vec::new()
            } else {
                let level = tag_name[1..].parse::<u8>().unwrap_or(1);
                let explicit_id = element.attr("id").filter(|id| !id.trim().is_empty());
                let id = heading_id_for(&inlines, explicit_id);
                wrap_alignment(element, vec![Block::Heading { level, id, inlines }])
            }
        }
        "blockquote" => {
            let blocks = blocks_from_nodes(element.children().collect(), heading_id_for);
            if blocks.is_empty() {
                Vec::new()
            } else {
                vec![Block::Blockquote { kind: None, blocks }]
            }
        }
        "pre" => {
            let code = element.text().collect::<String>();
            let language = element
                .child_elements()
                .find(|child| child.value().name() == "code")
                .and_then(code_language);
            vec![Block::CodeBlock { language, code }]
        }
        "ul" => list_from_element(element, false, heading_id_for),
        "ol" => list_from_element(element, true, heading_id_for),
        "table" => table_from_element(element, heading_id_for),
        "details" => details_from_element(element, heading_id_for),
        "summary" | "figcaption" | "dt" => {
            let mut inlines = element_children_to_inlines(element);
            trim_inline_boundaries(&mut inlines);
            paragraph_from_inlines(if inlines.is_empty() {
                inlines
            } else {
                vec![Inline::Bold(inlines)]
            })
        }
        "dd" | "address" => paragraph_from_inlines(element_children_to_inlines(element)),
        "hr" => vec![Block::HorizontalRule],
        _ => blocks_from_nodes(element.children().collect(), heading_id_for),
    }
}

fn details_from_element(
    element: ElementRef<'_>,
    heading_id_for: &mut dyn FnMut(&[Inline], Option<&str>) -> String,
) -> Vec<Block> {
    let mut summary = None;
    let mut body_nodes = Vec::new();

    for child in element.children() {
        let is_first_summary = summary.is_none()
            && ElementRef::wrap(child).is_some_and(|child| child.value().name() == "summary");
        if is_first_summary {
            let summary_element = ElementRef::wrap(child).expect("summary node must be an element");
            let mut inlines = element_children_to_inlines(summary_element);
            trim_inline_boundaries(&mut inlines);
            summary = Some(inlines);
        } else {
            body_nodes.push(child);
        }
    }

    let summary = summary.unwrap_or_default();
    vec![Block::Details {
        id: heading_id_for(&summary, Some("html-details")),
        summary,
        blocks: blocks_from_nodes(body_nodes, heading_id_for),
        open: element.attr("open").is_some(),
    }]
}

fn list_from_element(
    element: ElementRef<'_>,
    ordered: bool,
    heading_id_for: &mut dyn FnMut(&[Inline], Option<&str>) -> String,
) -> Vec<Block> {
    let items = element
        .child_elements()
        .filter(|child| child.value().name() == "li")
        .map(|item| list_item_from_element(item, heading_id_for))
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Vec::new();
    }

    if ordered {
        let start = element
            .attr("start")
            .and_then(|start| start.parse::<u64>().ok())
            .unwrap_or(1);
        vec![Block::OrderedList { start, items }]
    } else {
        vec![Block::UnorderedList { items }]
    }
}

fn list_item_from_element(
    item: ElementRef<'_>,
    heading_id_for: &mut dyn FnMut(&[Inline], Option<&str>) -> String,
) -> ListItem {
    let mut children = blocks_from_nodes(item.children().collect(), heading_id_for);
    let inlines = if matches!(children.first(), Some(Block::Paragraph { .. })) {
        let Block::Paragraph { inlines } = children.remove(0) else {
            unreachable!()
        };
        inlines
    } else {
        Vec::new()
    };
    ListItem {
        source: None,
        inlines,
        children,
        checked: None,
    }
}

fn table_from_element(
    table: ElementRef<'_>,
    heading_id_for: &mut dyn FnMut(&[Inline], Option<&str>) -> String,
) -> Vec<Block> {
    let mut row_elements = Vec::new();
    for child in table.child_elements() {
        match child.value().name() {
            "tr" => row_elements.push(child),
            "thead" | "tbody" | "tfoot" => row_elements.extend(
                child
                    .child_elements()
                    .filter(|row| row.value().name() == "tr"),
            ),
            _ => {}
        }
    }

    let mut headers = Vec::new();
    let mut rows = Vec::new();
    let mut alignments = Vec::new();
    for row in row_elements {
        let cells = row
            .child_elements()
            .filter(|cell| matches!(cell.value().name(), "th" | "td"))
            .collect::<Vec<_>>();
        if cells.is_empty() {
            continue;
        }

        if alignments.len() < cells.len() {
            alignments.resize(cells.len(), TableAlignment::None);
        }
        for (index, cell) in cells.iter().enumerate() {
            if alignments[index] == TableAlignment::None {
                alignments[index] = match element_alignment(*cell) {
                    Some(BlockAlignment::Left) => TableAlignment::Left,
                    Some(BlockAlignment::Center) => TableAlignment::Center,
                    Some(BlockAlignment::Right) => TableAlignment::Right,
                    None => TableAlignment::None,
                };
            }
        }

        let is_header = headers.is_empty() && cells.iter().any(|cell| cell.value().name() == "th");
        let converted = cells
            .into_iter()
            .map(|cell| blocks_from_nodes(cell.children().collect(), heading_id_for))
            .collect::<Vec<_>>();
        if is_header {
            headers = converted;
        } else {
            rows.push(converted);
        }
    }

    if headers.is_empty() && rows.is_empty() {
        Vec::new()
    } else {
        vec![Block::Table {
            headers,
            alignments,
            rows,
        }]
    }
}

fn element_children_to_inlines(element: ElementRef<'_>) -> Vec<Inline> {
    let mut inlines = Vec::new();
    for child in element.children() {
        match child.value() {
            Node::Text(text) => push_collapsed_text(&mut inlines, text),
            Node::Element(_) => {
                let child = ElementRef::wrap(child).expect("element node must be wrappable");
                inlines.extend(element_to_inlines(child));
            }
            _ => {}
        }
    }
    inlines
}

fn element_to_inlines(element: ElementRef<'_>) -> Vec<Inline> {
    let tag_name = element.value().name();
    if is_dropped_element(tag_name) {
        return Vec::new();
    }

    match tag_name {
        "br" => vec![Inline::LineBreak],
        "img" => element
            .attr("src")
            .filter(|url| !url.trim().is_empty())
            .map(|url| {
                vec![Inline::Image {
                    alt: element.attr("alt").unwrap_or_default().to_string(),
                    url: url.to_string(),
                    dimensions: image_dimensions(element),
                }]
            })
            .unwrap_or_default(),
        "strong" | "b" => wrap_inline(Inline::Bold, element_children_to_inlines(element)),
        "em" | "i" => wrap_inline(Inline::Italic, element_children_to_inlines(element)),
        "del" | "s" | "strike" => {
            wrap_inline(Inline::Strikethrough, element_children_to_inlines(element))
        }
        "u" | "ins" => wrap_inline(Inline::Underline, element_children_to_inlines(element)),
        "mark" => wrap_inline(Inline::Highlight, element_children_to_inlines(element)),
        "kbd" => wrap_inline(Inline::Kbd, element_children_to_inlines(element)),
        "sub" => wrap_inline(Inline::Subscript, element_children_to_inlines(element)),
        "sup" => wrap_inline(Inline::Superscript, element_children_to_inlines(element)),
        "code" => {
            let text = element.text().collect::<String>();
            if text.is_empty() {
                Vec::new()
            } else {
                vec![Inline::Code(text)]
            }
        }
        "a" => {
            let children = element_children_to_inlines(element);
            if children.is_empty() {
                Vec::new()
            } else if let Some(url) = element.attr("href") {
                vec![Inline::Link {
                    text: children,
                    url: url.to_string(),
                }]
            } else {
                children
            }
        }
        // Neutral phrasing elements preserve visible content while all CSS,
        // classes, IDs, and event attributes are intentionally ignored.
        "span" | "small" | "abbr" | "cite" | "q" | "time" | "var" | "samp" | "dfn" | "label" => {
            element_children_to_inlines(element)
        }
        _ => element_children_to_inlines(element),
    }
}

fn image_dimensions(element: ElementRef<'_>) -> ImageDimensions {
    ImageDimensions {
        width: style_property(element, "width", image_width)
            .or_else(|| element.attr("width").and_then(image_width)),
        height: style_property(element, "height", positive_length)
            .or_else(|| element.attr("height").and_then(positive_length)),
    }
}

fn image_width(value: &str) -> Option<ImageLength> {
    if let Some(percent) = value.trim().strip_suffix('%') {
        positive_length(percent).map(ImageLength::Percent)
    } else {
        positive_length(value).map(ImageLength::Pixels)
    }
}

fn style_property<T>(
    element: ElementRef<'_>,
    property: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    element
        .attr("style")?
        .split(';')
        .rev()
        .find_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case(property)
                .then(|| parse(value.trim()))
                .flatten()
        })
}

fn element_alignment(element: ElementRef<'_>) -> Option<BlockAlignment> {
    style_property(element, "text-align", |value| {
        html_block_alignment(Some(value))
    })
    .or_else(|| html_block_alignment(element.attr("align")))
}

fn positive_length(value: &str) -> Option<f32> {
    let value = value.trim().strip_suffix("px").unwrap_or(value.trim());
    value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn wrap_inline(
    constructor: impl FnOnce(Vec<Inline>) -> Inline,
    children: Vec<Inline>,
) -> Vec<Inline> {
    if children.is_empty() {
        Vec::new()
    } else {
        vec![constructor(children)]
    }
}

fn paragraph_from_inlines(mut inlines: Vec<Inline>) -> Vec<Block> {
    trim_inline_boundaries(&mut inlines);
    if inlines.is_empty() {
        Vec::new()
    } else {
        vec![Block::Paragraph { inlines }]
    }
}

fn flush_paragraph(inlines: &mut Vec<Inline>, blocks: &mut Vec<Block>) {
    let pending = std::mem::take(inlines);
    blocks.extend(paragraph_from_inlines(pending));
}

fn push_collapsed_text(inlines: &mut Vec<Inline>, text: &str) {
    let mut collapsed = String::new();
    let mut previous_was_whitespace = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !previous_was_whitespace {
                collapsed.push(' ');
            }
            previous_was_whitespace = true;
        } else {
            collapsed.push(character);
            previous_was_whitespace = false;
        }
    }
    if !collapsed.is_empty() {
        inlines.push(Inline::Text(collapsed));
    }
}

fn trim_inline_boundaries(inlines: &mut Vec<Inline>) {
    if let Some(first) = inlines.first_mut() {
        trim_inline_start(first);
    }
    if let Some(last) = inlines.last_mut() {
        trim_inline_end(last);
    }
    inlines.retain(|inline| !matches!(inline, Inline::Text(text) if text.is_empty()));
}

fn trim_inline_start(inline: &mut Inline) {
    match inline {
        Inline::Text(text) => *text = text.trim_start().to_string(),
        Inline::Bold(children)
        | Inline::Italic(children)
        | Inline::Strikethrough(children)
        | Inline::Kbd(children)
        | Inline::Subscript(children)
        | Inline::Superscript(children)
        | Inline::Underline(children)
        | Inline::Highlight(children)
        | Inline::Link { text: children, .. } => {
            if let Some(first) = children.first_mut() {
                trim_inline_start(first);
            }
        }
        _ => {}
    }
}

fn trim_inline_end(inline: &mut Inline) {
    match inline {
        Inline::Text(text) => *text = text.trim_end().to_string(),
        Inline::Bold(children)
        | Inline::Italic(children)
        | Inline::Strikethrough(children)
        | Inline::Kbd(children)
        | Inline::Subscript(children)
        | Inline::Superscript(children)
        | Inline::Underline(children)
        | Inline::Highlight(children)
        | Inline::Link { text: children, .. } => {
            if let Some(last) = children.last_mut() {
                trim_inline_end(last);
            }
        }
        _ => {}
    }
}

fn wrap_alignment(element: ElementRef<'_>, blocks: Vec<Block>) -> Vec<Block> {
    let alignment = if element.value().name() == "center" {
        Some(BlockAlignment::Center)
    } else {
        element_alignment(element)
    };
    match alignment {
        Some(alignment) => wrap_blocks(alignment, blocks),
        None => blocks,
    }
}

fn wrap_blocks(alignment: BlockAlignment, blocks: Vec<Block>) -> Vec<Block> {
    if blocks.is_empty() {
        Vec::new()
    } else {
        vec![Block::HtmlContainer { alignment, blocks }]
    }
}

fn html_block_alignment(value: Option<&str>) -> Option<BlockAlignment> {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("left") => Some(BlockAlignment::Left),
        Some("center") | Some("middle") => Some(BlockAlignment::Center),
        Some("right") => Some(BlockAlignment::Right),
        _ => None,
    }
}

fn code_language(element: ElementRef<'_>) -> Option<String> {
    element
        .value()
        .classes()
        .find_map(|class_name| class_name.strip_prefix("language-"))
        .filter(|language| !language.is_empty())
        .map(str::to_string)
}

fn closing_tag_name(source: &str) -> Option<&str> {
    let source = source.trim();
    let inner = source.strip_prefix("</")?.strip_suffix('>')?.trim();
    (!inner.is_empty()
        && inner
            .chars()
            .all(|character| character.is_ascii_alphanumeric()))
    .then_some(inner)
}

fn inline_kind(tag_name: &str) -> Option<InlineHtmlKind> {
    match tag_name.to_ascii_lowercase().as_str() {
        "strong" | "b" => Some(InlineHtmlKind::Bold),
        "em" | "i" => Some(InlineHtmlKind::Italic),
        "del" | "s" | "strike" => Some(InlineHtmlKind::Strikethrough),
        "u" | "ins" => Some(InlineHtmlKind::Underline),
        "mark" => Some(InlineHtmlKind::Highlight),
        "code" => Some(InlineHtmlKind::Code),
        "kbd" => Some(InlineHtmlKind::Kbd),
        "sub" => Some(InlineHtmlKind::Subscript),
        "sup" => Some(InlineHtmlKind::Superscript),
        "a" => Some(InlineHtmlKind::Link),
        "span" | "small" | "abbr" | "cite" | "q" | "time" | "var" | "samp" | "dfn" | "label" => {
            Some(InlineHtmlKind::Transparent)
        }
        _ => None,
    }
}

fn is_block_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "center"
            | "dd"
            | "details"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "summary"
            | "table"
            | "ul"
    )
}

fn is_dropped_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "base"
            | "embed"
            | "frame"
            | "frameset"
            | "iframe"
            | "link"
            | "meta"
            | "noscript"
            | "object"
            | "param"
            | "script"
            | "style"
            | "template"
            | "title"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading_id(inlines: &[Inline], explicit_id: Option<&str>) -> String {
        explicit_id.map(str::to_string).unwrap_or_else(|| {
            inlines
                .iter()
                .filter_map(|inline| match inline {
                    Inline::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("-")
                .to_ascii_lowercase()
        })
    }

    #[test]
    fn parses_html5_fragments_with_nested_native_semantics() {
        let blocks = parse_block_fragment(
            "<div><h2>Title</h2><p>Hello <mark><b>world</b></mark>.</p></div>",
            &mut heading_id,
        );

        assert!(matches!(
            &blocks[0],
            Block::Heading { level: 2, id, .. } if id == "title"
        ));
        assert!(matches!(
            &blocks[1],
            Block::Paragraph { inlines }
                if inlines.iter().any(|inline| matches!(inline, Inline::Highlight(_)))
        ));
    }

    #[test]
    fn ignores_active_content_and_event_attributes() {
        let blocks = parse_block_fragment(
            "<div onclick='alert(1)'>safe<script>alert(2)</script><iframe>hidden</iframe></div>",
            &mut heading_id,
        );

        assert_eq!(
            blocks,
            vec![Block::Paragraph {
                inlines: vec![Inline::Text("safe".to_string())],
            }]
        );
    }

    #[test]
    fn accepts_supported_image_dimensions_and_alignment_styles() {
        let blocks = parse_block_fragment(
            "<p align='left' style='text-align:center;position:fixed'><img src='a.png' width='320' height='160' style='width:50%;height:80px;background-image:url(bad)' /></p>",
            &mut heading_id,
        );
        assert_eq!(
            blocks,
            vec![Block::HtmlContainer {
                alignment: BlockAlignment::Center,
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Image {
                        alt: String::new(),
                        url: "a.png".into(),
                        dimensions: ImageDimensions {
                            width: Some(ImageLength::Percent(50.0)),
                            height: Some(80.0)
                        },
                    }]
                }],
            }]
        );
        for invalid in ["-1", "NaN", "inf", "calc(100% - 1px)", "url(x)"] {
            let parsed = parse_inline_event(&format!(
                "<img src='a.png' width='{invalid}' height='{invalid}'>"
            ));
            assert_eq!(
                parsed,
                InlineHtmlEvent::Node(Inline::Image {
                    alt: String::new(),
                    url: "a.png".into(),
                    dimensions: ImageDimensions::default(),
                }),
                "{invalid}"
            );
        }
    }

    #[test]
    fn html5_parser_recovers_misnested_formatting() {
        let blocks = parse_block_fragment("<p><b>bold <i>both</b> italic</i></p>", &mut heading_id);

        assert!(matches!(&blocks[0], Block::Paragraph { inlines } if !inlines.is_empty()));
    }

    #[test]
    fn preserves_collapsed_spacing_across_inline_elements() {
        let blocks =
            parse_block_fragment("<p>before <span> middle </span> after</p>", &mut heading_id);

        assert_eq!(
            blocks,
            vec![Block::Paragraph {
                inlines: vec![
                    Inline::Text("before ".to_string()),
                    Inline::Text(" middle ".to_string()),
                    Inline::Text(" after".to_string()),
                ],
            }]
        );
    }

    #[test]
    fn preserves_details_summary_body_and_initial_state() {
        for (attribute, open) in [("", false), (" open", true)] {
            let blocks = parse_block_fragment(
                &format!("<details{attribute}><summary>More</summary><p>Body</p></details>"),
                &mut heading_id,
            );
            assert_eq!(
                blocks,
                vec![Block::Details {
                    id: "html-details".into(),
                    summary: vec![Inline::Text("More".into())],
                    blocks: vec![Block::Paragraph {
                        inlines: vec![Inline::Text("Body".into())]
                    }],
                    open,
                }]
            );
        }
    }

    #[test]
    fn deeply_nested_html_falls_back_to_inert_source() {
        let source = format!(
            "{}content{}",
            "<span>".repeat(MAX_HTML_NESTING_DEPTH + 1),
            "</span>".repeat(MAX_HTML_NESTING_DEPTH + 1),
        );
        let blocks = parse_block_fragment(&source, &mut heading_id);

        assert_eq!(blocks, vec![Block::Html(source)]);
    }

    #[test]
    fn parses_safe_block_alignment_without_css() {
        let blocks = parse_block_fragment(
            "<div align='center' style='position:fixed'><p>Centered</p></div>",
            &mut heading_id,
        );

        assert!(matches!(
            &blocks[0],
            Block::HtmlContainer { alignment: BlockAlignment::Center, blocks }
                if matches!(&blocks[0], Block::Paragraph { .. })
        ));
    }

    #[test]
    fn parses_inline_attributes_without_accepting_css() {
        assert_eq!(
            parse_inline_event("<a class='button' onclick='bad()' href='https://example.com'>"),
            InlineHtmlEvent::Open(InlineHtmlOpen {
                kind: InlineHtmlKind::Link,
                link_url: Some("https://example.com".to_string()),
            })
        );
        assert_eq!(
            parse_inline_event("</A>"),
            InlineHtmlEvent::Close(InlineHtmlKind::Link)
        );
    }
}
