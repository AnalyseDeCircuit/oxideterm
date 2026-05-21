// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_core::{BufferOffset, TextRange};

use crate::*;

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
