// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Syntax data layer for OxideTerm's native editor.
//!
//! This crate owns tree-sitter parsers and returns byte-range metadata. It does
//! not paint GPUI elements and does not mutate editor buffers.

use std::path::Path;

use oxideterm_editor_core::{BufferOffset, LineCol, TextRange};
use thiserror::Error;
use tree_sitter::{
    InputEdit, Language, Node, Parser, Point, Query, QueryCursor, StreamingIterator, Tree,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum LanguageId {
    Bash,
    CSharp,
    Css,
    Diff,
    Elixir,
    Go,
    Html,
    Java,
    Javascript,
    Json,
    Make,
    Python,
    Ruby,
    Rust,
    Scala,
    Sql,
    Swift,
    Toml,
    Tsx,
    TypeScript,
    Yaml,
    Zig,
}

/// Keep the IDE language surface explicit so adding or removing grammars is a
/// conscious product decision instead of an accidental dependency side effect.
pub const SUPPORTED_LANGUAGES: &[LanguageId] = &[
    LanguageId::Bash,
    LanguageId::CSharp,
    LanguageId::Css,
    LanguageId::Diff,
    LanguageId::Elixir,
    LanguageId::Go,
    LanguageId::Html,
    LanguageId::Java,
    LanguageId::Javascript,
    LanguageId::Json,
    LanguageId::Make,
    LanguageId::Python,
    LanguageId::Ruby,
    LanguageId::Rust,
    LanguageId::Scala,
    LanguageId::Sql,
    LanguageId::Swift,
    LanguageId::Toml,
    LanguageId::Tsx,
    LanguageId::TypeScript,
    LanguageId::Yaml,
    LanguageId::Zig,
];

impl LanguageId {
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase());
        if matches!(
            file_name.as_deref(),
            Some("makefile" | "gnumakefile" | "bsdmakefile")
        ) {
            return Some(Self::Make);
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());
        match extension.as_deref() {
            Some("bash" | "sh" | "zsh") => Some(Self::Bash),
            Some("cs") => Some(Self::CSharp),
            Some("css") => Some(Self::Css),
            Some("diff" | "patch") => Some(Self::Diff),
            Some("ex" | "exs") => Some(Self::Elixir),
            Some("go") => Some(Self::Go),
            Some("html" | "htm") => Some(Self::Html),
            Some("java") => Some(Self::Java),
            Some("js" | "mjs" | "cjs" | "jsx") => Some(Self::Javascript),
            Some("json" | "jsonc") => Some(Self::Json),
            Some("mk") => Some(Self::Make),
            Some("py" | "pyw") => Some(Self::Python),
            Some("rb" | "rake") => Some(Self::Ruby),
            Some("rs") => Some(Self::Rust),
            Some("scala" | "sc") => Some(Self::Scala),
            Some("sql") => Some(Self::Sql),
            Some("swift") => Some(Self::Swift),
            Some("toml") => Some(Self::Toml),
            Some("ts" | "mts" | "cts") => Some(Self::TypeScript),
            Some("tsx") => Some(Self::Tsx),
            Some("yaml" | "yml") => Some(Self::Yaml),
            Some("zig") => Some(Self::Zig),
            _ => None,
        }
    }

    pub fn detect(path: Option<&Path>, source: &str) -> Option<Self> {
        path.and_then(Self::from_path)
            .or_else(|| language_from_shebang(source))
    }

    fn tree_sitter_language(self) -> Language {
        match self {
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Diff => tree_sitter_diff::LANGUAGE.into(),
            Self::Elixir => tree_sitter_elixir::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Html => tree_sitter_html::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Make => tree_sitter_make::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Scala => tree_sitter_scala::LANGUAGE.into(),
            Self::Sql => tree_sitter_sequel::LANGUAGE.into(),
            Self::Swift => tree_sitter_swift::LANGUAGE.into(),
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
        }
    }

    fn highlight_query(self) -> &'static str {
        match self {
            Self::Bash => BASH_HIGHLIGHTS_QUERY,
            Self::CSharp => tree_sitter_c_sharp::HIGHLIGHTS_QUERY,
            Self::Css => tree_sitter_css::HIGHLIGHTS_QUERY,
            Self::Diff => tree_sitter_diff::HIGHLIGHTS_QUERY,
            Self::Elixir => tree_sitter_elixir::HIGHLIGHTS_QUERY,
            Self::Go => tree_sitter_go::HIGHLIGHTS_QUERY,
            Self::Html => tree_sitter_html::HIGHLIGHTS_QUERY,
            Self::Java => tree_sitter_java::HIGHLIGHTS_QUERY,
            Self::Javascript => JAVASCRIPT_HIGHLIGHTS_QUERY,
            Self::Json => tree_sitter_json::HIGHLIGHTS_QUERY,
            Self::Make => tree_sitter_make::HIGHLIGHTS_QUERY,
            Self::Python => tree_sitter_python::HIGHLIGHTS_QUERY,
            Self::Ruby => tree_sitter_ruby::HIGHLIGHTS_QUERY,
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
            Self::Scala => tree_sitter_scala::HIGHLIGHTS_QUERY,
            Self::Sql => tree_sitter_sequel::HIGHLIGHTS_QUERY,
            Self::Swift => tree_sitter_swift::HIGHLIGHTS_QUERY,
            Self::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            Self::Tsx | Self::TypeScript => tree_sitter_typescript::HIGHLIGHTS_QUERY,
            Self::Yaml => tree_sitter_yaml::HIGHLIGHTS_QUERY,
            Self::Zig => tree_sitter_zig::HIGHLIGHTS_QUERY,
        }
    }
}

// `tree-sitter-bash` ships a query file but does not export it from the Rust
// crate. Keep a deliberately small OxideTerm-local query so common remote
// shell files get real tree-sitter spans instead of falling back to plain text.
const BASH_HIGHLIGHTS_QUERY: &str = r#"
[
  "if"
  "then"
  "else"
  "elif"
  "fi"
  "for"
  "while"
  "do"
  "done"
  "case"
  "esac"
  "function"
  "in"
] @keyword
(comment) @comment
(string) @string
(raw_string) @string
(command_name) @function
(variable_name) @variable
"#;

// `tree-sitter-javascript` also ships query files without exporting them from
// the crate. This local query intentionally covers the common scopes used by
// the editor color mapper while staying small enough to keep compile failures
// obvious when the grammar changes.
const JAVASCRIPT_HIGHLIGHTS_QUERY: &str = r#"
[
  "async"
  "await"
  "break"
  "case"
  "catch"
  "class"
  "const"
  "continue"
  "debugger"
  "default"
  "delete"
  "do"
  "else"
  "export"
  "extends"
  "finally"
  "for"
  "from"
  "function"
  "if"
  "import"
  "in"
  "instanceof"
  "let"
  "new"
  "of"
  "return"
  "switch"
  "throw"
  "try"
  "typeof"
  "var"
  "void"
  "while"
  "with"
  "yield"
] @keyword
(comment) @comment
(string) @string
(template_string) @string
(number) @number
(identifier) @variable
(property_identifier) @property
(function_declaration name: (identifier) @function)
(method_definition name: (property_identifier) @function)
(pair key: (property_identifier) @property)
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SyntaxScope {
    Attribute,
    Comment,
    Constant,
    Function,
    Keyword,
    Namespace,
    Number,
    Operator,
    Property,
    Punctuation,
    String,
    Type,
    Variable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HighlightSpan {
    pub range: TextRange,
    pub scope: SyntaxScope,
    pub capture: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BracketPair {
    pub open: BufferOffset,
    pub close: BufferOffset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoldRange {
    pub range: TextRange,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntaxEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: LineCol,
    pub old_end_position: LineCol,
    pub new_end_position: LineCol,
}

impl SyntaxEdit {
    pub fn replace(source_before: &str, range: TextRange, replacement: &str) -> Self {
        let start_position = point_for_byte(source_before, range.start.0);
        let old_end_position = point_for_byte(source_before, range.end.0);
        let new_end_position = advance_position(start_position, replacement);
        Self {
            start_byte: range.start.0,
            old_end_byte: range.end.0,
            new_end_byte: range.start.0 + replacement.len(),
            start_position,
            old_end_position,
            new_end_position,
        }
    }

    fn as_input_edit(self) -> InputEdit {
        InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte: self.new_end_byte,
            start_position: Point {
                row: self.start_position.line,
                column: self.start_position.column,
            },
            old_end_position: Point {
                row: self.old_end_position.line,
                column: self.old_end_position.column,
            },
            new_end_position: Point {
                row: self.new_end_position.line,
                column: self.new_end_position.column,
            },
        }
    }
}

pub struct SyntaxSession {
    language_id: LanguageId,
    language: Language,
    parser: Parser,
    highlight_query: Query,
    tree: Tree,
}

impl SyntaxSession {
    pub fn parse(language_id: LanguageId, source: &str) -> Result<Self, SyntaxError> {
        let language = language_id.tree_sitter_language();
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        let highlight_query = Query::new(&language, language_id.highlight_query())?;
        let tree = parser
            .parse(source, None)
            .ok_or(SyntaxError::ParseCancelled)?;

        Ok(Self {
            language_id,
            language,
            parser,
            highlight_query,
            tree,
        })
    }

    pub fn language_id(&self) -> LanguageId {
        self.language_id
    }

    pub fn root_has_error(&self) -> bool {
        self.tree.root_node().has_error()
    }

    pub fn apply_edit(&mut self, source_after: &str, edit: SyntaxEdit) -> Result<(), SyntaxError> {
        // tree-sitter incremental parsing requires the old tree to be edited
        // with the same byte/point delta before it is passed back as a hint.
        self.tree.edit(&edit.as_input_edit());
        self.tree = self
            .parser
            .parse(source_after, Some(&self.tree))
            .ok_or(SyntaxError::ParseCancelled)?;
        Ok(())
    }

    pub fn reparse(&mut self, source: &str) -> Result<(), SyntaxError> {
        self.parser.set_language(&self.language)?;
        self.tree = self
            .parser
            .parse(source, None)
            .ok_or(SyntaxError::ParseCancelled)?;
        Ok(())
    }

    pub fn highlight_spans(&self, source: &str) -> Vec<HighlightSpan> {
        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(
            &self.highlight_query,
            self.tree.root_node(),
            source.as_bytes(),
        );
        let names = self.highlight_query.capture_names();
        let mut spans = Vec::new();

        while {
            captures.advance();
            captures.get().is_some()
        } {
            let Some((query_match, capture_index)) = captures.get() else {
                continue;
            };
            let Some(capture) = query_match.captures.get(*capture_index).copied() else {
                continue;
            };
            let Some(capture_name) = names.get(capture.index as usize).copied() else {
                continue;
            };
            let Some(scope) = scope_for_capture(capture_name, capture.node.kind()) else {
                continue;
            };
            let range = capture.node.byte_range();
            if range.start < range.end && range.end <= source.len() {
                spans.push(HighlightSpan {
                    range: TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
                    scope,
                    capture: capture_name.to_string(),
                });
            }
        }

        normalize_highlight_spans(spans)
    }

    pub fn bracket_pairs(&self, source: &str) -> Vec<BracketPair> {
        bracket_pairs(source)
    }

    pub fn fold_ranges(&self) -> Vec<FoldRange> {
        let mut ranges = Vec::new();
        collect_fold_ranges(self.tree.root_node(), &mut ranges);
        ranges
    }
}

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("tree-sitter language error: {0}")]
    Language(#[from] tree_sitter::LanguageError),
    #[error("tree-sitter query error: {0}")]
    Query(#[from] tree_sitter::QueryError),
    #[error("tree-sitter parse was cancelled")]
    ParseCancelled,
}

fn language_from_shebang(source: &str) -> Option<LanguageId> {
    let first = source.lines().next()?;
    if !first.starts_with("#!") {
        return None;
    }
    let lower = first.to_ascii_lowercase();
    if lower.contains("rust-script") {
        return Some(LanguageId::Rust);
    }
    if lower.contains("bash") || lower.contains("/sh") || lower.contains("zsh") {
        return Some(LanguageId::Bash);
    }
    if lower.contains("python") {
        return Some(LanguageId::Python);
    }
    if lower.contains("ruby") {
        return Some(LanguageId::Ruby);
    }
    if lower.contains("node") || lower.contains("deno") {
        return Some(LanguageId::Javascript);
    }
    None
}

fn scope_for_capture(capture: &str, node_kind: &str) -> Option<SyntaxScope> {
    if matches!(node_kind, "integer_literal" | "float_literal") {
        return Some(SyntaxScope::Number);
    }

    let root = capture.split('.').next().unwrap_or(capture);
    match root {
        "attribute" => Some(SyntaxScope::Attribute),
        "comment" => Some(SyntaxScope::Comment),
        "constant" => Some(SyntaxScope::Constant),
        "function" => Some(SyntaxScope::Function),
        "keyword" => Some(SyntaxScope::Keyword),
        "module" | "namespace" => Some(SyntaxScope::Namespace),
        "number" => Some(SyntaxScope::Number),
        "operator" => Some(SyntaxScope::Operator),
        "property" | "field" => Some(SyntaxScope::Property),
        "punctuation" => Some(SyntaxScope::Punctuation),
        "string" | "character" => Some(SyntaxScope::String),
        "type" | "constructor" => Some(SyntaxScope::Type),
        "variable" | "parameter" => Some(SyntaxScope::Variable),
        _ => None,
    }
}

fn normalize_highlight_spans(mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    spans.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then_with(|| left.range.end.cmp(&right.range.end))
    });
    spans.dedup_by(|left, right| left.range == right.range && left.scope == right.scope);
    spans
}

fn point_for_byte(source: &str, byte: usize) -> LineCol {
    let mut line = 0;
    let mut line_start = 0;
    for (index, ch) in source.char_indices() {
        if index >= byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    LineCol::new(line, byte.saturating_sub(line_start))
}

fn advance_position(start: LineCol, text: &str) -> LineCol {
    let mut line = start.line;
    let mut column = start.column;
    for ch in text.chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += ch.len_utf8();
        }
    }
    LineCol::new(line, column)
}

fn bracket_pairs(source: &str) -> Vec<BracketPair> {
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut pairs = Vec::new();

    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => stack.push((byte, index)),
            b')' | b']' | b'}' => {
                let Some(position) = stack
                    .iter()
                    .rposition(|(open, _)| brackets_match(*open, byte))
                else {
                    continue;
                };
                let (_, open_index) = stack.remove(position);
                pairs.push(BracketPair {
                    open: BufferOffset(open_index),
                    close: BufferOffset(index),
                });
            }
            _ => {}
        }
    }

    pairs.sort_by_key(|pair| pair.open);
    pairs
}

fn brackets_match(open: u8, close: u8) -> bool {
    matches!((open, close), (b'(', b')') | (b'[', b']') | (b'{', b'}'))
}

fn collect_fold_ranges(node: Node<'_>, ranges: &mut Vec<FoldRange>) {
    if is_foldable_node(node) {
        let start = node.start_position();
        let end = node.end_position();
        if end.row > start.row {
            ranges.push(FoldRange {
                range: TextRange::new(
                    BufferOffset(node.start_byte()),
                    BufferOffset(node.end_byte()),
                ),
                start_line: start.row,
                end_line: end.row,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_fold_ranges(child, ranges);
    }
}

fn is_foldable_node(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "block"
            | "declaration_list"
            | "enum_item"
            | "function_item"
            | "impl_item"
            | "match_block"
            | "mod_item"
            | "struct_item"
            | "trait_item"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_at_least_twenty_supported_languages() {
        assert!(SUPPORTED_LANGUAGES.len() >= 20);
    }

    #[test]
    fn detects_supported_language_extensions_and_shebangs() {
        assert_eq!(LanguageId::from_path("src/main.rs"), Some(LanguageId::Rust));
        assert_eq!(LanguageId::from_path("install.sh"), Some(LanguageId::Bash));
        assert_eq!(
            LanguageId::from_path("PROGRAM.CS"),
            Some(LanguageId::CSharp)
        );
        assert_eq!(LanguageId::from_path("style.css"), Some(LanguageId::Css));
        assert_eq!(
            LanguageId::from_path("changes.patch"),
            Some(LanguageId::Diff)
        );
        assert_eq!(LanguageId::from_path("mix.exs"), Some(LanguageId::Elixir));
        assert_eq!(LanguageId::from_path("main.go"), Some(LanguageId::Go));
        assert_eq!(LanguageId::from_path("index.html"), Some(LanguageId::Html));
        assert_eq!(LanguageId::from_path("Main.java"), Some(LanguageId::Java));
        assert_eq!(
            LanguageId::from_path("app.jsx"),
            Some(LanguageId::Javascript)
        );
        assert_eq!(
            LanguageId::from_path("package.json"),
            Some(LanguageId::Json)
        );
        assert_eq!(LanguageId::from_path("Makefile"), Some(LanguageId::Make));
        assert_eq!(LanguageId::from_path("main.py"), Some(LanguageId::Python));
        assert_eq!(LanguageId::from_path("task.rake"), Some(LanguageId::Ruby));
        assert_eq!(LanguageId::from_path("Main.scala"), Some(LanguageId::Scala));
        assert_eq!(LanguageId::from_path("schema.sql"), Some(LanguageId::Sql));
        assert_eq!(LanguageId::from_path("App.swift"), Some(LanguageId::Swift));
        assert_eq!(LanguageId::from_path("Cargo.toml"), Some(LanguageId::Toml));
        assert_eq!(
            LanguageId::from_path("app.ts"),
            Some(LanguageId::TypeScript)
        );
        assert_eq!(LanguageId::from_path("app.tsx"), Some(LanguageId::Tsx));
        assert_eq!(LanguageId::from_path("compose.yml"), Some(LanguageId::Yaml));
        assert_eq!(LanguageId::from_path("main.zig"), Some(LanguageId::Zig));
        assert_eq!(
            LanguageId::detect(None, "#!/usr/bin/env rust-script\nfn main() {}"),
            Some(LanguageId::Rust)
        );
        assert_eq!(
            LanguageId::detect(None, "#!/usr/bin/env python3\nprint('hi')"),
            Some(LanguageId::Python)
        );
        assert_eq!(
            LanguageId::detect(None, "#!/usr/bin/env node\nconsole.log('hi')"),
            Some(LanguageId::Javascript)
        );
    }

    #[test]
    fn parses_and_highlights_all_supported_languages() {
        let samples = [
            (
                LanguageId::Bash,
                "if command -v cargo; then\n  echo \"ok\"\nfi\n",
            ),
            (
                LanguageId::CSharp,
                "class Demo { static void Main() { var x = 1; } }\n",
            ),
            (LanguageId::Css, ".root { color: #fff; display: flex; }\n"),
            (
                LanguageId::Diff,
                "diff --git a/a b/a\n@@ -1 +1 @@\n-old\n+new\n",
            ),
            (
                LanguageId::Elixir,
                "defmodule Demo do\n  def hello(name), do: \"hi #{name}\"\nend\n",
            ),
            (
                LanguageId::Go,
                "package main\nfunc main() { println(\"hi\") }\n",
            ),
            (
                LanguageId::Html,
                "<main class=\"root\"><h1>Hello</h1></main>\n",
            ),
            (
                LanguageId::Java,
                "class Demo { int add(int a, int b) { return a + b; } }\n",
            ),
            (
                LanguageId::Javascript,
                "function demo(value) { const x = value + 1; return x; }\n",
            ),
            (
                LanguageId::Json,
                "{\"scripts\": {\"build\": \"cargo build\"}}\n",
            ),
            (LanguageId::Make, "build:\n\tcargo build\n"),
            (
                LanguageId::Python,
                "def hello(name):\n    return f\"hi {name}\"\n",
            ),
            (
                LanguageId::Ruby,
                "class Demo\n  def hello\n    puts \"hi\"\n  end\nend\n",
            ),
            (
                LanguageId::Rust,
                "fn main() {\n    let message = \"hi\";\n}\n",
            ),
            (
                LanguageId::Scala,
                "object Demo { def main(args: Array[String]): Unit = println(\"hi\") }\n",
            ),
            (
                LanguageId::Sql,
                "select id, name from users where active = 1;\n",
            ),
            (LanguageId::Swift, "struct Demo { let value: Int }\n"),
            (LanguageId::Toml, "[package]\nname = \"demo\"\n"),
            (
                LanguageId::Tsx,
                "export function App() { return <div className=\"x\">Hi</div>; }\n",
            ),
            (
                LanguageId::TypeScript,
                "type User = { name: string };\nconst user: User = { name: \"Ada\" };\n",
            ),
            (LanguageId::Yaml, "name: demo\nitems:\n  - one\n"),
            (
                LanguageId::Zig,
                "pub fn main() void { const x: i32 = 1; }\n",
            ),
        ];

        for (language, source) in samples {
            let session = SyntaxSession::parse(language, source)
                .unwrap_or_else(|error| panic!("{language:?} query failed: {error}"));
            let spans = session.highlight_spans(source);

            assert!(
                !spans.is_empty(),
                "{language:?} should produce highlight spans"
            );
            assert!(
                spans.iter().all(|span| span.range.end.0 <= source.len()),
                "{language:?} produced an out-of-bounds span"
            );
        }
    }

    #[test]
    fn parses_and_highlights_rust() {
        let source = "fn main() {\n    let message = \"hi\";\n}\n";
        let session = SyntaxSession::parse(LanguageId::Rust, source).unwrap();

        assert!(!session.root_has_error());
        let spans = session.highlight_spans(source);

        assert!(spans.iter().any(|span| span.scope == SyntaxScope::Keyword));
        assert!(spans.iter().any(|span| span.scope == SyntaxScope::Function));
        assert!(spans.iter().any(|span| span.scope == SyntaxScope::String));
        assert!(spans.iter().all(|span| span.range.end.0 <= source.len()));
    }

    #[test]
    fn parses_and_highlights_common_remote_files() {
        let json = "{\"scripts\": {\"build\": \"cargo build\"}}";
        let json_session = SyntaxSession::parse(LanguageId::Json, json).unwrap();
        assert!(
            json_session
                .highlight_spans(json)
                .iter()
                .any(|span| span.scope == SyntaxScope::String)
        );

        let bash = "if command -v cargo; then\n  echo \"ok\"\nfi\n";
        let bash_session = SyntaxSession::parse(LanguageId::Bash, bash).unwrap();
        let spans = bash_session.highlight_spans(bash);
        assert!(spans.iter().any(|span| span.scope == SyntaxScope::Keyword));
        assert!(spans.iter().any(|span| span.scope == SyntaxScope::String));
    }

    #[test]
    fn reparses_incrementally_after_edit() {
        let before = "fn main() {\n    let x = 1;\n}\n";
        let edit_range = TextRange::new(BufferOffset(22), BufferOffset(23));
        let edit = SyntaxEdit::replace(before, edit_range, "10");
        let after = "fn main() {\n    let x = 10;\n}\n";
        let mut session = SyntaxSession::parse(LanguageId::Rust, before).unwrap();

        session.apply_edit(after, edit).unwrap();

        assert!(!session.root_has_error());
        assert!(
            session
                .highlight_spans(after)
                .iter()
                .any(|span| span.scope == SyntaxScope::Number)
        );
    }

    #[test]
    fn computes_bracket_and_fold_ranges() {
        let source = "fn main() {\n    if true {\n        println!(\"x\");\n    }\n}\n";
        let session = SyntaxSession::parse(LanguageId::Rust, source).unwrap();

        assert!(session.bracket_pairs(source).len() >= 3);
        assert!(
            session
                .fold_ranges()
                .iter()
                .any(|range| range.start_line == 0 && range.end_line >= 3)
        );
    }
}
