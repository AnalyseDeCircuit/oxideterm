// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_core::{BufferOffset, TextRange};

use crate::*;

#[test]
fn detects_supported_language_extensions_and_shebangs() {
    assert_eq!(LanguageId::from_path("src/main.rs"), Some(LanguageId::Rust));
    assert_eq!(LanguageId::from_path("install.sh"), Some(LanguageId::Bash));
    assert_eq!(
        LanguageId::from_path("PROGRAM.CS"),
        Some(LanguageId::CSharp)
    );
    assert_eq!(LanguageId::from_path("main.c"), Some(LanguageId::C));
    assert_eq!(
        LanguageId::from_path("CMakeLists.txt"),
        Some(LanguageId::CMake)
    );
    assert_eq!(LanguageId::from_path("tool.cmake"), Some(LanguageId::CMake));
    assert_eq!(LanguageId::from_path("main.cpp"), Some(LanguageId::Cpp));
    assert_eq!(LanguageId::from_path("seqlist.h"), Some(LanguageId::Cpp));
    assert_eq!(
        LanguageId::from_path("Dockerfile.prod"),
        Some(LanguageId::Dockerfile)
    );
    assert_eq!(LanguageId::from_path("style.css"), Some(LanguageId::Css));
    assert_eq!(
        LanguageId::from_path("changes.patch"),
        Some(LanguageId::Diff)
    );
    assert_eq!(LanguageId::from_path("mix.exs"), Some(LanguageId::Elixir));
    assert_eq!(LanguageId::from_path("config.fish"), Some(LanguageId::Fish));
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
    assert_eq!(LanguageId::from_path("system.lisp"), Some(LanguageId::Lisp));
    assert_eq!(LanguageId::from_path("init.lua"), Some(LanguageId::Lua));
    assert_eq!(LanguageId::from_path("Makefile"), Some(LanguageId::Make));
    assert_eq!(
        LanguageId::from_path("AppDelegate.m"),
        Some(LanguageId::ObjectiveC)
    );
    assert_eq!(LanguageId::from_path("script.pl"), Some(LanguageId::Perl));
    assert_eq!(LanguageId::from_path("index.php"), Some(LanguageId::Php));
    assert_eq!(
        LanguageId::from_path("profile.ps1"),
        Some(LanguageId::Powershell)
    );
    assert_eq!(
        LanguageId::from_path("README.md"),
        Some(LanguageId::Markdown)
    );
    assert_eq!(
        LanguageId::from_path("guide.markdown"),
        Some(LanguageId::Markdown)
    );
    assert_eq!(
        LanguageId::from_path("page.mdx"),
        Some(LanguageId::Markdown)
    );
    assert_eq!(LanguageId::from_path("main.py"), Some(LanguageId::Python));
    assert_eq!(LanguageId::from_path("analysis.R"), Some(LanguageId::R));
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
    assert_eq!(LanguageId::from_path(".zshrc"), Some(LanguageId::Zsh));
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
        (
            LanguageId::CMake,
            "cmake_minimum_required(VERSION 3.20)\nproject(Demo)\n",
        ),
        (
            LanguageId::C,
            "#include <stdio.h>\nint main(void) { return 0; }\n",
        ),
        (
            LanguageId::Cpp,
            "#include <iostream>\ntemplate <typename T> class Box { T value; };\n",
        ),
        (LanguageId::Css, ".root { color: #fff; display: flex; }\n"),
        (
            LanguageId::Diff,
            "diff --git a/a b/a\n@@ -1 +1 @@\n-old\n+new\n",
        ),
        (LanguageId::Dockerfile, "FROM alpine:3.20\nRUN echo ok\n"),
        (
            LanguageId::Elixir,
            "defmodule Demo do\n  def hello(name), do: \"hi #{name}\"\nend\n",
        ),
        (LanguageId::Fish, "function greet\n    echo hi\nend\n"),
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
        (
            LanguageId::Lisp,
            "(defun hello (name) (format t \"hi ~a\" name))\n",
        ),
        (LanguageId::Lua, "local value = 1\nprint(value)\n"),
        (LanguageId::Make, "build:\n\tcargo build\n"),
        (
            LanguageId::Markdown,
            "# Title\n\nSome `code` and [link](https://example.com).\n\n```rust\nfn main() {}\n```\n",
        ),
        (
            LanguageId::ObjectiveC,
            "#import <Foundation/Foundation.h>\n@interface Demo : NSObject\n@end\n",
        ),
        (LanguageId::Perl, "my $name = \"Ada\";\nprint $name;\n"),
        (LanguageId::Php, "<?php\nfunction demo() { return 1; }\n"),
        (
            LanguageId::Powershell,
            "param($Name)\nWrite-Host \"Hi $Name\"\n",
        ),
        (
            LanguageId::Python,
            "def hello(name):\n    return f\"hi {name}\"\n",
        ),
        (LanguageId::R, "value <- c(1, 2, 3)\nprint(value)\n"),
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
        (LanguageId::Zsh, "autoload -Uz compinit\ncompinit\n"),
        (
            LanguageId::Zig,
            "pub fn main() void { const x: i32 = 1; }\n",
        ),
    ];

    for (language, source) in samples {
        let session = SyntaxSession::parse(language, source)
            .unwrap_or_else(|error| panic!("{language:?} query failed: {error}"));
        let spans = session.highlight_spans(source);
        let mut structure = StructureCache::default();
        structure.update(&session, source, 4, None);
        let expected_folds: std::collections::BTreeMap<_, _> =
            session.fold_ranges().into_iter().fold(
                std::collections::BTreeMap::<usize, usize>::new(),
                |mut folds, range| {
                    folds
                        .entry(range.start_line)
                        .and_modify(|end| *end = (*end).max(range.end_line))
                        .or_insert(range.end_line);
                    folds
                },
            );
        assert_eq!(
            structure.fold_lines().collect::<Vec<_>>(),
            expected_folds.into_iter().collect::<Vec<_>>(),
            "{language:?} folds"
        );
        let guides = session.indent_guides(source, 4);
        for line in 0..=source.lines().count() {
            let mut expected: Vec<_> = guides
                .iter()
                .filter(|guide| guide.start_line < line && guide.end_line >= line)
                .map(|guide| guide.column)
                .collect();
            expected.sort_unstable();
            expected.dedup();
            assert_eq!(
                structure.columns_for_line(line),
                expected,
                "{language:?} line {line}"
            );
        }
        let mut cache = HighlightCache::default();
        cache.update(&session, source, None);
        assert_eq!(
            cache.spans_in_range(0..source.len()).collect::<Vec<_>>(),
            spans,
            "{language:?} initial cache"
        );

        for (start, ch) in source.char_indices() {
            let end = start + ch.len_utf8();
            let expected: Vec<_> = spans
                .iter()
                .filter(|span| span.range.start.0 < end && span.range.end.0 > start)
                .cloned()
                .collect();
            assert_eq!(
                session.highlight_spans_in_range(
                    source,
                    TextRange::new(BufferOffset(start), BufferOffset(end))
                ),
                expected,
                "{language:?}, range {start}..{end}"
            );
        }
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

    let markdown = "# Title\n\nSee [docs](https://example.com).\n\n```sh\necho ok\n```\n";
    let markdown_session = SyntaxSession::parse(LanguageId::Markdown, markdown).unwrap();
    let markdown_spans = markdown_session.highlight_spans(markdown);
    assert!(
        markdown_spans
            .iter()
            .any(|span| span.scope == SyntaxScope::Keyword)
    );
    assert!(
        markdown_spans
            .iter()
            .any(|span| span.scope == SyntaxScope::Function)
    );
    assert!(
        markdown_spans
            .iter()
            .any(|span| span.scope == SyntaxScope::String)
    );
}

#[test]
fn indent_guides_come_from_syntax_blocks() {
    let source = "fn main() {\n    if true {\n        println!(\"ok\");\n    }\n}\n";
    let session = SyntaxSession::parse(LanguageId::Rust, source).unwrap();
    let guides = session.indent_guides(source, 4);

    assert!(guides.iter().any(|guide| guide.column == 0));
    assert!(guides.iter().any(|guide| guide.column == 4));
    assert!(!guides.iter().any(|guide| guide.column == 8));
}

#[test]
fn indent_guides_ignore_shell_alignment_continuations() {
    let source = "CHOICE=$(whiptail --title \"Power\" \\\n                --menu \"Current\" 12 40 3 \\\n                \"1\" \"Show\")\n";
    let session = SyntaxSession::parse(LanguageId::Bash, source).unwrap();

    assert!(session.indent_guides(source, 4).is_empty());
}

#[test]
fn indent_guides_cover_c_blocks_without_macro_alignment() {
    let source = r#"#define GPIOB_CRH (*(uint32_t *) 0x11111111)
void BEEP_Init()
{
    RCC_APB2ENR |= 1<<3;
    GPIOB_CRH &= 0xFFFFFFF0;
}
"#;
    let session = SyntaxSession::parse(LanguageId::C, source).unwrap();
    let guides = session.indent_guides(source, 4);

    assert!(guides.iter().any(|guide| guide.column == 0));
    assert!(!guides.iter().any(|guide| guide.column == 4));
    assert!(!guides.iter().any(|guide| guide.start_line == 0));
}

#[test]
fn indent_guides_keep_body_indentation_for_delimiter_free_languages() {
    let source = "def main():\n    if ready:\n        print(\"ok\")\n";
    let session = SyntaxSession::parse(LanguageId::Python, source).unwrap();
    let guides = session.indent_guides(source, 4);

    assert!(guides.iter().any(|guide| guide.column == 4));
    assert!(guides.iter().any(|guide| guide.column == 8));
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

#[test]
fn fold_and_indent_traversal_preserves_nested_and_sibling_ranges() {
    let source = "fn main() {\n    if true {\n        a();\n    }\n}\nfn other() {\n    b();\n}\n";
    let session = SyntaxSession::parse(LanguageId::Rust, source).unwrap();
    assert_eq!(
        session
            .fold_ranges()
            .iter()
            .map(|r| (r.start_line, r.end_line))
            .collect::<Vec<_>>(),
        [(0, 4), (0, 4), (1, 3), (5, 7), (5, 7)]
    );
    assert_eq!(
        session
            .indent_guides(source, 4)
            .iter()
            .map(|g| (g.start_line, g.end_line, g.column))
            .collect::<Vec<_>>(),
        [(0, 4, 0), (1, 3, 4), (5, 7, 0)]
    );
}

#[test]
#[ignore = "manual release-profile syntax stage benchmark"]
fn syntax_stage_performance() {
    use std::{hint::black_box, time::Instant};
    let pattern = "fn example() { let value = 42; }\n";
    for target in [100usize * 1024, 1024 * 1024] {
        for position in ["start", "middle", "end"] {
            let mut source = pattern.repeat(target.div_ceil(pattern.len()));
            let offset = match position {
                "start" => 0,
                "middle" => source.len() / pattern.len() / 2 * pattern.len(),
                _ => source.len() - pattern.len(),
            } + pattern.find("42").unwrap();
            let mut session = SyntaxSession::parse(LanguageId::Rust, &source).unwrap();
            for run in 0..12 {
                let replacement = if run % 2 == 0 { "43" } else { "42" };
                let edit = SyntaxEdit::replace(
                    &source,
                    TextRange::new(BufferOffset(offset), BufferOffset(offset + 2)),
                    replacement,
                );
                source.replace_range(offset..offset + 2, replacement);
                let started = Instant::now();
                let change = session.apply_edit(&source, edit).unwrap();
                let parsed = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                black_box(change.structural_ranges().collect::<Vec<_>>());
                let compared = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                black_box(session.highlight_spans(&source));
                let highlighted = started.elapsed().as_secs_f64() * 1000.0;
                let range_start = (offset / pattern.len() * pattern.len())
                    .min(source.len().saturating_sub(40 * pattern.len()));
                let started = Instant::now();
                black_box(session.highlight_spans_in_range(
                    &source,
                    TextRange::new(
                        BufferOffset(range_start),
                        BufferOffset(range_start + 40 * pattern.len()),
                    ),
                ));
                let ranged = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                black_box(session.bracket_pairs(&source));
                let brackets = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                black_box(session.fold_ranges());
                let folds = started.elapsed().as_secs_f64() * 1000.0;
                let started = Instant::now();
                black_box(session.indent_guides(&source, 4));
                let indent = started.elapsed().as_secs_f64() * 1000.0;
                eprintln!(
                    "SYNTAX_STAGE bytes={} position={position} run={run} parse_ms={parsed:.3} compare_ms={compared:.3} highlight_ms={highlighted:.3} range_ms={ranged:.3} brackets_ms={brackets:.3} folds_ms={folds:.3} indent_ms={indent:.3}",
                    source.len()
                );
            }
        }
    }
}

#[test]
fn range_highlights_keep_crossing_captures_and_byte_coordinates() {
    let comment = "/* first\n中文🙂 e\u{301} */";
    let source = format!("{comment}\nfn main() {{}}\n");
    let session = SyntaxSession::parse(LanguageId::Rust, &source).unwrap();
    let start = source.find('🙂').unwrap();
    assert_eq!(
        session.highlight_spans_in_range(
            &source,
            TextRange::new(BufferOffset(start), BufferOffset(start + 4))
        ),
        vec![HighlightSpan {
            range: TextRange::new(BufferOffset(0), BufferOffset(comment.len())),
            scope: SyntaxScope::Comment,
        }]
    );
    for offset in [0, start, source.len(), source.len() + 10] {
        assert_eq!(
            session.highlight_spans_in_range(
                &source,
                TextRange::new(BufferOffset(offset), BufferOffset(offset))
            ),
            vec![]
        );
    }
    assert_eq!(
        session.highlight_spans_in_range(
            &source,
            TextRange::new(BufferOffset(source.len()), BufferOffset(source.len() + 10))
        ),
        vec![]
    );
}

#[test]
fn text_only_changes_are_reported_when_query_predicates_change() {
    let before = "fn main() { let value = foo; }\n";
    let after = "fn main() { let value = Foo; }\n";
    let start = before.find("foo").unwrap();
    let range = TextRange::new(BufferOffset(start), BufferOffset(start + 3));
    let mut session = SyntaxSession::parse(LanguageId::Rust, before).unwrap();
    let previous = session.highlight_spans_in_range(before, range);
    assert!(!previous.iter().any(|span| span.scope == SyntaxScope::Type));
    let edit = SyntaxEdit::replace(before, range, "Foo");
    let change = session.apply_edit(after, edit).unwrap();
    assert_eq!(change.edit, edit);
    assert!(
        change.structural_ranges().len() == 0,
        "equal-size identifier edit changed structure"
    );
    assert!(
        session
            .highlight_spans_in_range(after, range)
            .contains(&HighlightSpan {
                range,
                scope: SyntaxScope::Type
            })
    );
}

#[test]
fn range_queries_follow_boundary_edits_in_sequence() {
    for (language, initial, edits) in [
        (
            LanguageId::Rust,
            "fn first() {}\nfn second() {}\n",
            vec![(0, 0, "/*"), (2, 2, "*/"), (0, 4, "")],
        ),
        (
            LanguageId::Rust,
            "fn main() { let x = \"中文🙂\"; }\n",
            vec![(0, 0, "//"), (0, 2, ""), (0, 0, "\n")],
        ),
        (
            LanguageId::Markdown,
            "**中文🙂** and [link](https://example.com)\n\n`code`\n",
            vec![(0, 2, ""), (0, 0, "**"), (0, 0, "# ")],
        ),
    ] {
        let mut source = initial.to_string();
        let mut session = SyntaxSession::parse(language, &source).unwrap();
        let mut comment_change: Option<SyntaxChange> = None;
        for (start, end, replacement) in edits {
            let edit = SyntaxEdit::replace(
                &source,
                TextRange::new(BufferOffset(start), BufferOffset(end)),
                replacement,
            );
            source.replace_range(start..end, replacement);
            let change = session.apply_edit(&source, edit).unwrap();
            assert_eq!(change.edit.start_byte, start);
            assert_eq!(change.edit.old_end_byte, end);
            assert_eq!(change.edit.new_end_byte, start + replacement.len());
            if replacement == "/*" {
                let second = source.find("second").unwrap();
                assert!(
                    change
                        .structural_ranges()
                        .any(|range| range.start.0 <= second && range.end.0 > second),
                    "comment boundary did not report the distant structural change"
                );
            }
            if replacement == "/*" {
                comment_change = Some(change);
            }
            if let Some(change) = &comment_change {
                // Deferred changes must keep their own trees after later edits.
                let second = initial.find("second").unwrap() + 2;
                assert!(
                    change
                        .structural_ranges()
                        .any(|range| range.start.0 <= second && range.end.0 > second)
                );
            }
            let fresh = SyntaxSession::parse(language, &source)
                .unwrap()
                .highlight_spans(&source);
            for (offset, ch) in source.char_indices() {
                let end = offset + ch.len_utf8();
                let expected: Vec<_> = fresh
                    .iter()
                    .filter(|span| span.range.start.0 < end && span.range.end.0 > offset)
                    .cloned()
                    .collect();
                assert_eq!(
                    session.highlight_spans_in_range(
                        &source,
                        TextRange::new(BufferOffset(offset), BufferOffset(end))
                    ),
                    expected,
                    "{language:?} after {edit:?}, range {offset}..{end}"
                );
            }
        }
    }
}

#[test]
#[ignore = "manual multiline fold and indent benchmark"]
fn multiline_structure_performance() {
    use std::{hint::black_box, time::Instant};
    let pattern = "fn example() {\n    if ready {\n        run();\n    }\n}\n";
    for bytes in [200usize * 1024, 1024 * 1024] {
        let source = pattern.repeat(bytes.div_ceil(pattern.len()));
        let session = SyntaxSession::parse(LanguageId::Rust, &source).unwrap();
        let started = Instant::now();
        let folds = session.fold_ranges();
        let fold_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let guides = session.indent_guides(&source, 4);
        let indent_ms = started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "MULTILINE_STRUCTURE bytes={} fold_ms={fold_ms:.3} indent_ms={indent_ms:.3} folds={} guides={}",
            source.len(),
            folds.len(),
            guides.len()
        );
        black_box((folds, guides));
    }
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "manual same-language session allocation benchmark"]
fn syntax_sessions_memory() {
    #[repr(C)]
    #[derive(Default)]
    struct Statistics {
        blocks: u32,
        live: usize,
        peak: usize,
        allocated: usize,
    }
    unsafe extern "C" {
        fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Statistics);
    }
    let allocated = || {
        let mut stats = Statistics::default();
        // Null includes every malloc zone; this excludes native stack and GPU memory.
        unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
        stats.live
    };
    for (language, source) in [
        (LanguageId::Rust, "fn main() { let n = 1; }"),
        (LanguageId::Markdown, "# Heading\n\n**bold** text"),
    ] {
        let baseline = allocated();
        let mut sessions = Vec::new();
        for _ in 0..16 {
            sessions.push(SyntaxSession::parse(language, source).unwrap());
        }
        let opened = allocated();
        let expected = sessions[0].highlight_spans(source);
        for session in &sessions {
            assert_eq!(session.highlight_spans(source), expected);
        }
        drop(sessions);
        eprintln!(
            "SYNTAX_MEMORY language={language:?} baseline={baseline} opened={opened} closed={}",
            allocated()
        );
    }
}

#[test]
fn simultaneous_documents_share_queries_but_keep_independent_parse_state() {
    use std::sync::{Arc, Barrier};
    let barrier = Arc::new(Barrier::new(2));
    let sessions = std::thread::scope(|scope| {
        let tasks: Vec<_> = ["fn alpha() {}", "fn beta() {}"]
            .into_iter()
            .map(|source| {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    (
                        SyntaxSession::parse(LanguageId::Rust, source).unwrap(),
                        source,
                    )
                })
            })
            .collect();
        tasks
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(Arc::ptr_eq(&sessions[0].0.queries, &sessions[1].0.queries));
    let mut sessions = sessions.into_iter();
    let (mut first, _) = sessions.next().unwrap();
    let (second, source) = sessions.next().unwrap();
    first.reparse("fn changed() {}").unwrap();
    for (session, source, name) in [
        (&first, "fn changed() {}", "changed"),
        (&second, source, "beta"),
    ] {
        let names: Vec<_> = session
            .highlight_spans(source)
            .into_iter()
            .filter(|span| span.scope == SyntaxScope::Function)
            .map(|span| source[span.range.start.0..span.range.end.0].to_owned())
            .collect();
        assert_eq!(names, [name]);
    }
}
