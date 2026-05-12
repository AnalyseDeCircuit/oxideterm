// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::LanguageId;

pub(crate) fn highlight_query_for(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Bash => BASH_HIGHLIGHTS_QUERY,
        LanguageId::CSharp => tree_sitter_c_sharp::HIGHLIGHTS_QUERY,
        LanguageId::Css => tree_sitter_css::HIGHLIGHTS_QUERY,
        LanguageId::Diff => tree_sitter_diff::HIGHLIGHTS_QUERY,
        LanguageId::Elixir => tree_sitter_elixir::HIGHLIGHTS_QUERY,
        LanguageId::Go => tree_sitter_go::HIGHLIGHTS_QUERY,
        LanguageId::Html => tree_sitter_html::HIGHLIGHTS_QUERY,
        LanguageId::Java => tree_sitter_java::HIGHLIGHTS_QUERY,
        LanguageId::Javascript => JAVASCRIPT_HIGHLIGHTS_QUERY,
        LanguageId::Json => tree_sitter_json::HIGHLIGHTS_QUERY,
        LanguageId::Make => tree_sitter_make::HIGHLIGHTS_QUERY,
        LanguageId::Markdown => tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        LanguageId::Python => tree_sitter_python::HIGHLIGHTS_QUERY,
        LanguageId::Ruby => tree_sitter_ruby::HIGHLIGHTS_QUERY,
        LanguageId::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
        LanguageId::Scala => tree_sitter_scala::HIGHLIGHTS_QUERY,
        LanguageId::Sql => tree_sitter_sequel::HIGHLIGHTS_QUERY,
        LanguageId::Swift => tree_sitter_swift::HIGHLIGHTS_QUERY,
        LanguageId::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
        LanguageId::Tsx | LanguageId::TypeScript => tree_sitter_typescript::HIGHLIGHTS_QUERY,
        LanguageId::Yaml => tree_sitter_yaml::HIGHLIGHTS_QUERY,
        LanguageId::Zig => tree_sitter_zig::HIGHLIGHTS_QUERY,
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
