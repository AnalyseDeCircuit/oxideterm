// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Self-owned Mermaid subset renderer.

pub mod cache;
pub mod layout;
pub mod model;
pub mod parser;
pub mod svg;

pub use cache::{
    RenderedMermaidImage, render_mermaid_svg, render_mermaid_svg_image, render_mermaid_svg_scaled,
};

/// Return true when a fenced code block should be treated as Mermaid.
pub fn is_mermaid_language(language: Option<&str>) -> bool {
    matches!(
        language
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "mermaid" | "mmd"
    )
}

/// Return true when an unlabeled/text code fence is likely a Mermaid diagram.
pub fn is_mermaid_source_candidate(source: &str) -> bool {
    let Some(first_line) = source.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return false;
    };

    first_line == "sequenceDiagram"
        || first_line
            .split_whitespace()
            .next()
            .is_some_and(|kind| matches!(kind, "graph" | "flowchart"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mermaid_languages() {
        assert!(is_mermaid_language(Some("mermaid")));
        assert!(is_mermaid_language(Some(" MMD ")));
        assert!(!is_mermaid_language(Some("rust")));
        assert!(!is_mermaid_language(None));
    }

    #[test]
    fn detects_mermaid_like_source() {
        assert!(is_mermaid_source_candidate("graph TD\nA --> B"));
        assert!(is_mermaid_source_candidate("flowchart LR\nA --> B"));
        assert!(is_mermaid_source_candidate("sequenceDiagram\nA->B: hi"));
        assert!(!is_mermaid_source_candidate("graphical output"));
        assert!(!is_mermaid_source_candidate("echo graph TD"));
    }
}
