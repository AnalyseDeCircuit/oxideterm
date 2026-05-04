// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm code editor boundary for future IDE mode.
//!
//! Upper layers should use OxideTerm-owned configuration types and theme
//! mapping. The concrete editor backend remains behind this crate so GPUI
//! component API changes do not leak into workspace/session code.

use oxideterm_theme::ThemeTokens;

/// Default feature gate used by upper layers when checking editor support.
pub const CODE_EDITOR_BACKEND: &str = "gpui-component-input";

/// OxideTerm-owned editor mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeEditorMode {
    PlainText,
    Code,
    Markdown,
}

/// Theme-derived editor appearance hints.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeEditorAppearance {
    pub background_hex: u32,
    pub text_hex: u32,
    pub muted_text_hex: u32,
    pub accent_hex: u32,
    pub font_family: String,
    pub font_size: f32,
}

impl CodeEditorAppearance {
    pub fn from_theme(tokens: &ThemeTokens) -> Self {
        Self {
            background_hex: tokens.ui.bg,
            text_hex: tokens.ui.text,
            muted_text_hex: tokens.ui.text_muted,
            accent_hex: tokens.ui.accent,
            font_family: tokens.metrics.markdown_code_font_family.to_string(),
            font_size: tokens.metrics.ui_text_sm,
        }
    }
}

/// A typed request for an embedded code editor surface.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeEditorRequest {
    pub mode: CodeEditorMode,
    pub language: Option<String>,
    pub text: String,
    pub read_only: bool,
    pub appearance: CodeEditorAppearance,
}

impl CodeEditorRequest {
    pub fn new(mode: CodeEditorMode, text: impl Into<String>, tokens: &ThemeTokens) -> Self {
        Self {
            mode,
            language: None,
            text: text.into(),
            read_only: false,
            appearance: CodeEditorAppearance::from_theme(tokens),
        }
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

/// Low-level escape hatch for the implementation crate only.
///
/// Keep usage contained to this crate or purpose-built editor adapters.
pub mod backend {
    pub use gpui_component::input;
}

#[cfg(test)]
mod tests {
    use oxideterm_theme::default_tokens;

    use super::*;

    #[test]
    fn derives_appearance_from_theme_tokens() {
        let tokens = default_tokens();
        let appearance = CodeEditorAppearance::from_theme(&tokens);

        assert_eq!(appearance.background_hex, tokens.ui.bg);
        assert_eq!(appearance.text_hex, tokens.ui.text);
        assert_eq!(appearance.muted_text_hex, tokens.ui.text_muted);
        assert_eq!(appearance.accent_hex, tokens.ui.accent);
    }
}
