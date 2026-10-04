// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::LanguageId;

pub(crate) fn highlight_query_for(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Bash => BASH_HIGHLIGHTS_QUERY,
        LanguageId::C => "",
        LanguageId::CSharp => "",
        LanguageId::CMake => CMAKE_HIGHLIGHTS_QUERY,
        LanguageId::Cpp => "",
        LanguageId::Css => "",
        LanguageId::Diff => tree_sitter_diff::HIGHLIGHTS_QUERY,
        LanguageId::Dockerfile => tree_sitter_containerfile::HIGHLIGHTS_QUERY,
        LanguageId::Elixir => "",
        LanguageId::Fish => tree_sitter_fish::HIGHLIGHTS_QUERY,
        LanguageId::Go => "",
        LanguageId::Html => "",
        LanguageId::Java => "",
        LanguageId::Javascript => "",
        LanguageId::Json => tree_sitter_json::HIGHLIGHTS_QUERY,
        LanguageId::Lisp => "",
        LanguageId::Lua => tree_sitter_lua::HIGHLIGHTS_QUERY,
        LanguageId::Make => tree_sitter_make::HIGHLIGHTS_QUERY,
        LanguageId::Markdown => tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        LanguageId::ObjectiveC => "",
        LanguageId::Perl => "",
        LanguageId::Php => "",
        LanguageId::Powershell => tree_sitter_powershell::HIGHLIGHTS_QUERY,
        LanguageId::Python => tree_sitter_python::HIGHLIGHTS_QUERY,
        LanguageId::R => "",
        LanguageId::Ruby => "",
        LanguageId::Rust => "",
        LanguageId::Scala => "",
        LanguageId::Sql => tree_sitter_sequel::HIGHLIGHTS_QUERY,
        LanguageId::Swift => "",
        LanguageId::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
        LanguageId::Tsx | LanguageId::TypeScript => "",
        LanguageId::Yaml => tree_sitter_yaml::HIGHLIGHTS_QUERY,
        LanguageId::Zsh => tree_sitter_zsh::HIGHLIGHT_QUERY,
        LanguageId::Zig => "",
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

; Keep structural shell punctuation visible without treating dashes in words as operators.
[
  "$"
  "&&"
  "||"
  "&"
  "|"
  ";"
  ";;"
  ">"
  ">>"
  "<"
  "<<"
  "<<<"
  "="
  "=="
  "=~"
  "+"
  "-"
  "*"
  "/"
  "%"
] @operator
"#;

// `tree-sitter-cmake` ships a highlight query file but does not export it from
// the Rust crate. Keep a compact local query for the scopes our editor theme
// already maps instead of reaching into Cargo's private registry layout.
const CMAKE_HIGHLIGHTS_QUERY: &str = r#"
[
  (function)
  (endfunction)
  (macro)
  (endmacro)
  (if)
  (elseif)
  (else)
  (endif)
  (foreach)
  (endforeach)
  (while)
  (endwhile)
] @keyword

(normal_command (identifier) @function)
(quoted_argument) @string
(bracket_argument) @string
(line_comment) @comment
(bracket_comment) @comment
(variable) @variable
"#;
