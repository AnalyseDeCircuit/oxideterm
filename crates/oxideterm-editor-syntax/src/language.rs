// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use tree_sitter::Language;

unsafe extern "C" {
    fn tree_sitter_fish() -> *const tree_sitter::ffi::TSLanguage;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum LanguageId {
    Bash,
    C,
    CSharp,
    CMake,
    Cpp,
    Css,
    Diff,
    Dockerfile,
    Elixir,
    Fish,
    Go,
    Html,
    Java,
    Javascript,
    Json,
    Lisp,
    Lua,
    Make,
    Markdown,
    ObjectiveC,
    Perl,
    Php,
    Powershell,
    Python,
    R,
    Ruby,
    Rust,
    Scala,
    Sql,
    Swift,
    Toml,
    Tsx,
    TypeScript,
    Yaml,
    Zsh,
    Zig,
}

/// Keep the IDE language surface explicit so adding or removing grammars is a
/// conscious product decision instead of an accidental dependency side effect.
pub const SUPPORTED_LANGUAGES: &[LanguageId] = &[
    LanguageId::Bash,
    LanguageId::C,
    LanguageId::CSharp,
    LanguageId::CMake,
    LanguageId::Cpp,
    LanguageId::Css,
    LanguageId::Diff,
    LanguageId::Dockerfile,
    LanguageId::Elixir,
    LanguageId::Fish,
    LanguageId::Go,
    LanguageId::Html,
    LanguageId::Java,
    LanguageId::Javascript,
    LanguageId::Json,
    LanguageId::Lisp,
    LanguageId::Lua,
    LanguageId::Make,
    LanguageId::Markdown,
    LanguageId::ObjectiveC,
    LanguageId::Perl,
    LanguageId::Php,
    LanguageId::Powershell,
    LanguageId::Python,
    LanguageId::R,
    LanguageId::Ruby,
    LanguageId::Rust,
    LanguageId::Scala,
    LanguageId::Sql,
    LanguageId::Swift,
    LanguageId::Toml,
    LanguageId::Tsx,
    LanguageId::TypeScript,
    LanguageId::Yaml,
    LanguageId::Zsh,
    LanguageId::Zig,
];

impl LanguageId {
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase());
        if let Some(language) = file_name.as_deref().and_then(language_from_known_file_name) {
            return Some(language);
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());
        match extension.as_deref() {
            Some("bash" | "sh") => Some(Self::Bash),
            Some("c") => Some(Self::C),
            Some("cmake") => Some(Self::CMake),
            Some("cs") => Some(Self::CSharp),
            // Tauri's CodeMirror loader routes `.h` through the C++ language
            // package, which keeps template-heavy remote header files colored.
            Some("cc" | "cpp" | "cxx" | "c++" | "h" | "hpp" | "hxx" | "hh") => Some(Self::Cpp),
            Some("css") => Some(Self::Css),
            Some("diff" | "patch") => Some(Self::Diff),
            Some("ex" | "exs") => Some(Self::Elixir),
            Some("fish") => Some(Self::Fish),
            Some("go") => Some(Self::Go),
            Some("html" | "htm") => Some(Self::Html),
            Some("java") => Some(Self::Java),
            Some("js" | "mjs" | "cjs" | "jsx") => Some(Self::Javascript),
            Some("json" | "jsonc") => Some(Self::Json),
            Some("lisp" | "lsp" | "cl" | "asd") => Some(Self::Lisp),
            Some("lua") => Some(Self::Lua),
            Some("m" | "mm") => Some(Self::ObjectiveC),
            Some("mk") => Some(Self::Make),
            Some("md" | "mdx" | "markdown") => Some(Self::Markdown),
            Some("php" | "phtml" | "php3" | "php4" | "php5" | "php7" | "php8") => Some(Self::Php),
            Some("pl" | "pm" | "pod" | "psgi") => Some(Self::Perl),
            Some("ps1" | "psm1" | "psd1") => Some(Self::Powershell),
            Some("py" | "pyw") => Some(Self::Python),
            Some("r") => Some(Self::R),
            Some("rb" | "rake") => Some(Self::Ruby),
            Some("rs") => Some(Self::Rust),
            Some("scala" | "sc") => Some(Self::Scala),
            Some("sql") => Some(Self::Sql),
            Some("swift") => Some(Self::Swift),
            Some("toml") => Some(Self::Toml),
            Some("ts" | "mts" | "cts") => Some(Self::TypeScript),
            Some("tsx") => Some(Self::Tsx),
            Some("yaml" | "yml") => Some(Self::Yaml),
            Some("zsh" | "zsh-theme") => Some(Self::Zsh),
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
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::CMake => tree_sitter_cmake::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Diff => tree_sitter_diff::LANGUAGE.into(),
            Self::Dockerfile => tree_sitter_containerfile::LANGUAGE.into(),
            Self::Elixir => tree_sitter_elixir::LANGUAGE.into(),
            Self::Fish => fish_language(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Html => tree_sitter_html::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Lisp => tree_sitter_commonlisp::LANGUAGE_COMMONLISP.into(),
            Self::Lua => tree_sitter_lua::LANGUAGE.into(),
            Self::Make => tree_sitter_make::LANGUAGE.into(),
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
            Self::ObjectiveC => tree_sitter_objc::LANGUAGE.into(),
            Self::Perl => ts_parser_perl::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Self::Powershell => tree_sitter_powershell::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::R => tree_sitter_r::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Scala => tree_sitter_scala::LANGUAGE.into(),
            Self::Sql => tree_sitter_sequel::LANGUAGE.into(),
            Self::Swift => tree_sitter_swift::LANGUAGE.into(),
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            Self::Zsh => tree_sitter_zsh::LANGUAGE.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
        }
    }

    pub(crate) fn highlight_query(self) -> &'static str {
        crate::queries::highlight_query_for(self)
    }
}

fn language_from_known_file_name(file_name: &str) -> Option<LanguageId> {
    if matches!(file_name, "makefile" | "gnumakefile" | "bsdmakefile") {
        return Some(LanguageId::Make);
    }
    if matches!(file_name, "cmakelists.txt") {
        return Some(LanguageId::CMake);
    }
    if is_dockerfile_name(file_name) {
        return Some(LanguageId::Dockerfile);
    }
    if matches!(
        file_name,
        ".bashrc" | ".bash_profile" | ".bash_login" | ".profile"
    ) {
        return Some(LanguageId::Bash);
    }
    if matches!(
        file_name,
        ".zshrc" | ".zprofile" | ".zshenv" | ".zlogin" | ".zlogout"
    ) {
        return Some(LanguageId::Zsh);
    }
    None
}

fn is_dockerfile_name(file_name: &str) -> bool {
    // Docker users often keep environment-specific files as `Dockerfile.dev`;
    // classify those by role instead of only matching the extensionless base.
    matches!(file_name, "dockerfile" | "containerfile")
        || file_name.starts_with("dockerfile.")
        || file_name.starts_with("containerfile.")
}

fn fish_language() -> Language {
    // `tree-sitter-fish` still exposes the pre-LanguageFn Rust helper, so use
    // the generated C symbol directly to stay on OxideTerm's tree-sitter ABI.
    unsafe { Language::from_raw(tree_sitter_fish()) }
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
    if lower.contains("zsh") {
        return Some(LanguageId::Zsh);
    }
    if lower.contains("bash") || lower.contains("/sh") {
        return Some(LanguageId::Bash);
    }
    if lower.contains("fish") {
        return Some(LanguageId::Fish);
    }
    if lower.contains("pwsh") || lower.contains("powershell") {
        return Some(LanguageId::Powershell);
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
