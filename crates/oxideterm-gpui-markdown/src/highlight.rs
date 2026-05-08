// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Syntax highlighting for fenced code blocks using `syntect`.
//!
//! Converts highlighted tokens into GPUI `TextRun` sequences that slot
//! directly into the existing `StyledText` rendering pipeline.

use gpui::{Font, FontStyle, FontWeight, Hsla, Rgba, SharedString, TextRun};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle as SyntectFontStyle, Style as SyntectStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::options::MarkdownOptions;
use crate::style;

use std::sync::OnceLock;

// ─── global lazy-loaded syntect resources ───────────────────────────────

fn syntax_set() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    TS.get_or_init(ThemeSet::load_defaults)
}

/// The syntect theme name used for highlighting.
/// `base16-ocean.dark` is a sensible dark-mode default that pairs well
/// with OxideTerm's dark terminal themes.
const SYNTECT_THEME: &str = "base16-ocean.dark";

// ─── public API ─────────────────────────────────────────────────────────

/// A single highlighted run of text with font and colour information,
/// ready to be converted to a GPUI `TextRun`.
pub struct HighlightedRun {
    pub text: String,
    pub font: Font,
    pub color: Hsla,
}

/// Highlight `code` using the syntax for `language`.
///
/// Returns `None` if the language is not recognised by syntect, in which
/// case the caller should fall back to plain monospace rendering.
pub fn highlight_code(
    language: &str,
    code: &str,
    opts: &MarkdownOptions,
) -> Option<Vec<HighlightedRun>> {
    let ss = syntax_set();
    let syntax = syntax_for_tauri_language(ss, language)?;

    let ts = theme_set();
    let theme = ts.themes.get(SYNTECT_THEME)?;

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut runs = Vec::new();

    for line in syntect::util::LinesWithEndings::from(code) {
        let highlighted = highlighter.highlight_line(line, ss).ok()?;

        for (syn_style, text) in highlighted {
            if text.is_empty() {
                continue;
            }

            runs.push(HighlightedRun {
                text: text.to_string(),
                font: syntect_font(syn_style, opts),
                color: syntect_color_to_hsla(syn_style),
            });
        }
    }

    Some(runs)
}

/// Convert a `Vec<HighlightedRun>` into `(SharedString, Vec<TextRun>)` suitable
/// for `StyledText::new(text).with_runs(runs)`.
pub fn highlighted_runs_to_text_runs(runs: &[HighlightedRun]) -> (SharedString, Vec<TextRun>) {
    let mut text = String::new();
    let mut text_runs = Vec::with_capacity(runs.len());

    for run in runs {
        let len = run.text.len();
        if len == 0 {
            continue;
        }
        text.push_str(&run.text);

        text_runs.push(TextRun {
            len,
            font: run.font.clone(),
            color: run.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    (SharedString::from(text), text_runs)
}

// ─── helpers ────────────────────────────────────────────────────────────

fn syntect_color_to_hsla(syn_style: SyntectStyle) -> Hsla {
    let c = syn_style.foreground;
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: c.a as f32 / 255.0,
    }
    .into()
}

fn syntect_font(syn_style: SyntectStyle, opts: &MarkdownOptions) -> Font {
    let base = style::code_font(opts);

    let weight = if syn_style.font_style.contains(SyntectFontStyle::BOLD) {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    };

    let font_style = if syn_style.font_style.contains(SyntectFontStyle::ITALIC) {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };

    Font {
        weight,
        style: font_style,
        ..base
    }
}

fn syntax_for_tauri_language<'a>(
    syntax_set: &'a SyntaxSet,
    language: &str,
) -> Option<&'a SyntaxReference> {
    let language = language.trim();
    if language.is_empty() {
        return None;
    }
    if let Some(syntax) = syntax_set.find_syntax_by_token(language) {
        return Some(syntax);
    }

    // SFTP preview languages come from the Tauri table, where Prism accepts
    // canonical names like `typescript` while syntect often ships extension or
    // syntax-name aliases. Keep that translation here so highlighting parity
    // stays shared by markdown and raw text previews.
    let aliases: &[&str] = match language.to_ascii_lowercase().as_str() {
        "javascript" => &["js", "JavaScript"],
        // syntect's bundled defaults do not always include TypeScript/TSX;
        // JavaScript still gives useful Prism-like keyword/string colouring.
        "typescript" | "tsx" | "jsx" => &["ts", "TypeScript", "js", "JavaScript"],
        "python" => &["py", "Python"],
        "rust" => &["rs", "Rust"],
        "cpp" => &["cc", "c++", "C++"],
        "docker" => &["Dockerfile", "sh", "Shell-Unix-Generic"],
        "makefile" => &["Makefile", "sh", "Shell-Unix-Generic"],
        "markdown" => &["md", "Markdown"],
        "latex" => &["tex", "LaTeX"],
        "gitignore" => &["gitignore", "Git Ignore"],
        "bash" => &["sh", "Shell-Unix-Generic", "Bourne Again Shell (bash)"],
        _ => &[],
    };

    aliases.iter().find_map(|alias| {
        syntax_set
            .find_syntax_by_token(alias)
            .or_else(|| syntax_set.find_syntax_by_name(alias))
    })
}

// ─── tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_code() {
        let opts = MarkdownOptions::default();
        let runs = highlight_code("rust", "fn main() {}\n", &opts);
        assert!(runs.is_some());
        let runs = runs.unwrap();
        assert!(!runs.is_empty());

        // Concatenated text should contain the original code
        let text: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert!(text.contains("fn"));
        assert!(text.contains("main"));
    }

    #[test]
    fn returns_none_for_unknown_language() {
        let opts = MarkdownOptions::default();
        let runs = highlight_code("not_a_real_language_xyz", "hello", &opts);
        assert!(runs.is_none());
    }

    #[test]
    fn highlights_tauri_sftp_language_aliases() {
        let opts = MarkdownOptions::default();
        assert!(highlight_code("typescript", "const value: number = 1;\n", &opts).is_some());
        assert!(highlight_code("docker", "FROM alpine\n", &opts).is_some());
        assert!(highlight_code("makefile", "all:\n\techo ok\n", &opts).is_some());
    }

    #[test]
    fn text_runs_preserve_length() {
        let opts = MarkdownOptions::default();
        let runs = highlight_code("python", "print('hello')\n", &opts).unwrap();
        let (text, text_runs) = highlighted_runs_to_text_runs(&runs);

        let total_len: usize = text_runs.iter().map(|r| r.len).sum();
        assert_eq!(total_len, text.len());
    }
}
