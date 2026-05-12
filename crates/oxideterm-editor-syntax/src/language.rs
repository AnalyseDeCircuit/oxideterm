// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use tree_sitter::Language;

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
    Markdown,
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
    LanguageId::Markdown,
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
            Some("md" | "mdx" | "markdown") => Some(Self::Markdown),
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

    pub(crate) fn tree_sitter_language(self) -> Language {
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
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
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

    pub(crate) fn highlight_query(self) -> &'static str {
        crate::queries::highlight_query_for(self)
    }
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
