// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm web preview boundary.
//!
//! This crate intentionally hides the concrete component crate from upper
//! layers. Native app code should depend on OxideTerm-owned request/config
//! types and keep all visual decisions mapped from `oxideterm-theme`.

use oxideterm_theme::ThemeTokens;

/// Default feature gate used by upper layers when checking web preview support.
pub const WEBVIEW_BACKEND: &str = "gpui-component-webview";

/// Web content source understood by OxideTerm preview surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebviewSource {
    /// Navigate to a remote or local URL.
    Url(String),
    /// Render a complete HTML document string.
    Html(String),
}

/// Theme-derived webview appearance hints.
#[derive(Clone, Debug, PartialEq)]
pub struct WebviewAppearance {
    pub background_hex: u32,
    pub text_hex: u32,
    pub font_family: String,
}

impl WebviewAppearance {
    pub fn from_theme(tokens: &ThemeTokens) -> Self {
        Self {
            background_hex: tokens.ui.bg,
            text_hex: tokens.ui.text,
            font_family: tokens.metrics.font_family.to_string(),
        }
    }
}

/// A typed request for an embedded web preview.
#[derive(Clone, Debug, PartialEq)]
pub struct WebviewRequest {
    pub source: WebviewSource,
    pub appearance: WebviewAppearance,
}

impl WebviewRequest {
    pub fn url(url: impl Into<String>, tokens: &ThemeTokens) -> Self {
        Self {
            source: WebviewSource::Url(url.into()),
            appearance: WebviewAppearance::from_theme(tokens),
        }
    }

    pub fn html(html: impl Into<String>, tokens: &ThemeTokens) -> Self {
        Self {
            source: WebviewSource::Html(html.into()),
            appearance: WebviewAppearance::from_theme(tokens),
        }
    }
}

/// Low-level escape hatch for the implementation crate only.
///
/// Keep usage contained to this crate or purpose-built preview adapters.
pub mod backend {
    pub use gpui_component::webview;
}

#[cfg(test)]
mod tests {
    use oxideterm_theme::default_tokens;

    use super::*;

    #[test]
    fn derives_appearance_from_theme_tokens() {
        let tokens = default_tokens();
        let appearance = WebviewAppearance::from_theme(&tokens);

        assert_eq!(appearance.background_hex, tokens.ui.bg);
        assert_eq!(appearance.text_hex, tokens.ui.text);
        assert_eq!(appearance.font_family, tokens.metrics.font_family);
    }
}
