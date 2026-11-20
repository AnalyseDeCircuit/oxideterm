// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Syntax highlighting for fenced code blocks using `syntect`.
//!
//! Converts highlighted tokens into GPUI `TextRun` sequences that slot
//! directly into the existing `StyledText` rendering pipeline.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use gpui::{Font, FontStyle, FontWeight, Hsla, Rgba, SharedString, TextRun};
use oxideterm_theme::ThemeTokens;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle as SyntectFontStyle, Style as SyntectStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::options::MarkdownOptions;
use crate::style;

// ─── global lazy-loaded syntect resources & cache ───────────────────────

const HIGHLIGHT_CACHE_MAX_ENTRIES: usize = 256;
const HIGHLIGHT_CACHE_MAX_CODE_BYTES: usize = 128 * 1024;
const SYNTECT_DARK_THEME: &str = "base16-ocean.dark";
const SYNTECT_LIGHT_THEME: &str = "InspiredGitHub";
const SYNTECT_LIGHT_FALLBACK: &str = "base16-ocean.light";

fn syntax_set() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    TS.get_or_init(ThemeSet::load_defaults)
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct HighlightCacheKey {
    language: String,
    light_surface: bool,
    code_font_family: String,
    code_len: usize,
    code_hash: u64,
}

#[derive(Default)]
struct HighlightCache {
    entries: HashMap<HighlightCacheKey, (SharedString, Vec<TextRun>)>,
    insertion_order: VecDeque<HighlightCacheKey>,
}

fn highlight_cache() -> &'static Mutex<HighlightCache> {
    static CACHE: OnceLock<Mutex<HighlightCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HighlightCache::default()))
}

fn cache_key(
    language: &str,
    code: &str,
    light_surface: bool,
    opts: &MarkdownOptions,
) -> HighlightCacheKey {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hasher);
    HighlightCacheKey {
        language: language.trim().to_ascii_lowercase(),
        light_surface,
        code_font_family: opts.code_font_family.clone(),
        code_len: code.len(),
        code_hash: hasher.finish(),
    }
}

fn highlight_cache_get(key: &HighlightCacheKey, code: &str) -> Option<(SharedString, Vec<TextRun>)> {
    highlight_cache()
        .lock()
        .ok()?
        .entries
        .get(key)
        .filter(|(text, _)| text.as_ref() == code)
        .cloned()
}

fn highlight_cache_insert(key: HighlightCacheKey, value: (SharedString, Vec<TextRun>)) {
    let Ok(mut cache) = highlight_cache().lock() else {
        return;
    };
    if !cache.entries.contains_key(&key) {
        cache.insertion_order.push_back(key.clone());
    }
    cache.entries.insert(key, value);
    while cache.entries.len() > HIGHLIGHT_CACHE_MAX_ENTRIES {
        let Some(oldest) = cache.insertion_order.pop_front() else {
            break;
        };
        cache.entries.remove(&oldest);
    }
}

// ─── public API ─────────────────────────────────────────────────────────

/// A single highlighted run of text with font and colour information,
/// ready to be converted to a GPUI `TextRun`.
pub struct HighlightedRun {
    pub text: String,
    pub font: Font,
    pub color: Hsla,
}

/// Highlight `code` using the syntax for `language` and the dark default theme.
///
/// Returns `None` if the language is not recognised by syntect, in which
/// case the caller should fall back to plain monospace rendering.
pub fn highlight_code(
    language: &str,
    code: &str,
    opts: &MarkdownOptions,
) -> Option<Vec<HighlightedRun>> {
    highlight_code_with_surface(language, code, false, opts)
}

/// Highlight `code` with theme-appropriate light/dark syntax colours and a
/// bounded process cache so repaints and streaming updates avoid re-running
/// Oniguruma regex highlighting on unchanged code blocks.
pub fn highlight_code_for_theme(
    language: &str,
    code: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> Option<(SharedString, Vec<TextRun>)> {
    let light_surface = style::is_light_surface(tokens.ui.bg_panel);
    let cacheable = code.len() <= HIGHLIGHT_CACHE_MAX_CODE_BYTES;
    let key = cacheable.then(|| cache_key(language, code, light_surface, opts));
    if let Some(ref key) = key
        && let Some(cached) = highlight_cache_get(key, code)
    {
        return Some(cached);
    }

    let runs = highlight_code_with_surface(language, code, light_surface, opts)?;
    let rendered = highlighted_runs_to_text_runs(&runs);
    if let Some(key) = key {
        highlight_cache_insert(key, rendered.clone());
    }
    Some(rendered)
}

fn highlight_code_with_surface(
    language: &str,
    code: &str,
    light_surface: bool,
    opts: &MarkdownOptions,
) -> Option<Vec<HighlightedRun>> {
    let ss = syntax_set();
    let syntax = syntax_for_tauri_language(ss, language)?;

    let ts = theme_set();
    let theme = if light_surface {
        ts.themes
            .get(SYNTECT_LIGHT_THEME)
            .or_else(|| ts.themes.get(SYNTECT_LIGHT_FALLBACK))
    } else {
        ts.themes.get(SYNTECT_DARK_THEME)
    }?;

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
            letter_spacing: None,
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
        "javascript" | "mjs" | "cjs" => &["js", "JavaScript"],
        // syntect's bundled defaults do not always include TypeScript/TSX;
        // JavaScript still gives useful Prism-like keyword/string colouring.
        "typescript" | "tsx" | "jsx" | "mts" | "cts" => &["ts", "TypeScript", "js", "JavaScript"],
        "python" | "py3" => &["py", "Python"],
        "rust" => &["rs", "Rust"],
        "golang" | "go" => &["go", "Go"],
        "cpp" | "cxx" | "hpp" => &["cc", "c++", "C++"],
        "csharp" | "cs" | "c#" => &["cs", "C#"],
        "docker" | "dockerfile" => &["Dockerfile", "sh", "Shell-Unix-Generic"],
        "makefile" | "make" | "mk" => &["Makefile", "sh", "Shell-Unix-Generic"],
        "markdown" | "md" | "mdx" => &["md", "Markdown"],
        "latex" | "tex" => &["tex", "LaTeX"],
        "gitignore" => &["gitignore", "Git Ignore"],
        "bash" | "zsh" | "sh" | "shell" | "shellscript" | "console" | "terminal" | "fish" => {
            &["sh", "Shell-Unix-Generic", "Bourne Again Shell (bash)"]
        }
        "powershell" | "ps1" | "pwsh" => &["ps1", "PowerShell", "sh", "Shell-Unix-Generic"],
        "yaml" | "yml" => &["yaml", "yml", "YAML"],
        "json" | "jsonc" | "jsonl" => &["json", "JSON"],
        "toml" | "ini" | "conf" | "cfg" | "nginx" | "dotenv" | "env" | "properties" => {
            &["toml", "TOML", "ini", "INI", "Makefile"]
        }
        "sql" | "postgres" | "postgresql" | "mysql" | "sqlite" => &["sql", "SQL"],
        "diff" | "patch" => &["diff", "Diff"],
        "html" | "htm" | "svg" => &["html", "HTML"],
        "xml" | "plist" => &["xml", "XML"],
        "css" | "scss" | "less" => &["css", "CSS"],
        "ruby" | "rb" => &["rb", "Ruby"],
        "lua" => &["lua", "Lua"],
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
    fn highlighting_preserves_source_and_style_through_text_run_conversion() {
        let opts = MarkdownOptions::default();
        for (language, code) in [
            ("rust", "fn main() {\n    println!(\"你好🚀\");\n}\n"),
            ("python", "# greeting\r\nprint('你好🚀')\r\n"),
        ] {
            let runs = highlight_code(language, code, &opts).unwrap();
            let (text, text_runs) = highlighted_runs_to_text_runs(&runs);
            assert_eq!(text.as_ref(), code, "{language}");
            assert!(runs.windows(2).any(|pair| pair[0].color != pair[1].color));
            let mut offset = 0;
            assert_eq!(text_runs.len(), runs.len());
            for (rendered, highlighted) in text_runs.iter().zip(&runs) {
                assert_eq!(&text[offset..offset + rendered.len], highlighted.text);
                assert_eq!(rendered.color, highlighted.color);
                assert_eq!(rendered.font, highlighted.font);
                offset += rendered.len;
            }
            assert_eq!(offset, code.len());
        }
    }

    #[test]
    fn returns_none_for_unknown_language() {
        let opts = MarkdownOptions::default();
        let runs = highlight_code("not_a_real_language_xyz", "hello", &opts);
        assert!(runs.is_none());
    }

    #[test]
    fn theme_aware_highlighting_adapts_palette_caches_and_resolves_ops_aliases() {
        let opts = MarkdownOptions::default();
        let mut dark = oxideterm_theme::default_tokens();
        dark.ui.bg_panel = 0x181a1f;
        let mut light = oxideterm_theme::default_tokens();
        light.ui.bg_panel = 0xf5f7fa;

        let sample = "if [ -f /etc/hosts ]; then echo \"ok\"; fi\n";
        let (dark_text, dark_runs) =
            highlight_code_for_theme("zsh", sample, &dark, &opts).expect("zsh alias highlighted");
        let (light_text, light_runs) =
            highlight_code_for_theme("zsh", sample, &light, &opts).expect("light theme highlighted");
        let (cached_text, cached_runs) =
            highlight_code_for_theme("zsh", sample, &light, &opts).expect("cached light highlight");

        assert_eq!(dark_text.as_ref(), sample);
        assert_eq!(light_text.as_ref(), sample);
        assert_eq!(cached_text, light_text);
        assert_eq!(cached_runs.len(), light_runs.len());
        // Light surface palette must produce visibly darker foregrounds than the dark surface palette.
        let avg_l = |runs: &[TextRun]| {
            runs.iter().map(|r| r.color.l).sum::<f32>() / runs.len().max(1) as f32
        };
        assert!(
            avg_l(&light_runs) < avg_l(&dark_runs),
            "expected light theme syntax colors to be darker than dark theme colors"
        );

        for (lang, snippet) in [
            ("toml", "[server]\nport = 8080\n"),
            ("yaml", "services:\n  web:\n    ports: [80]\n"),
            ("jsonc", "{\"enabled\": true}\n"),
            ("sql", "SELECT id FROM users WHERE active = 1;\n"),
            ("diff", "+added line\n-removed line\n"),
        ] {
            let (text, runs) = highlight_code_for_theme(lang, snippet, &dark, &opts)
                .unwrap_or_else(|| panic!("expected syntax highlighting for alias {lang}"));
            assert_eq!(text.as_ref(), snippet);
            assert!(!runs.is_empty());
        }
    }
}
